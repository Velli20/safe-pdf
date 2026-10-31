use std::{marker::PhantomData, num::NonZeroU32};

use crate::{
    Jbig2Error, Result,
    bitmap::{Bitmap, BitmapCompositor, BitmapRef, CombinationOperator, Placement},
    cursor::ByteCursor,
    dictionary::{
        DecodedResource, DecodedResourceRef, PatternDictionary, PatternDictionaryDecoder,
        PatternDictionaryDescriptor, ResourceResolver, ResourceStore, SymbolDictionary,
        SymbolDictionaryDecoder, SymbolDictionaryDescriptor,
    },
    error::{LimitKind, SourceLocation},
    huffman::HuffmanTable,
    region::{
        DecodeContext, GenericRegion, GenericRegionDecoder, HalftoneRegion, HalftoneRegionDecoder,
        RefinementReference, RefinementRegion, RefinementRegionDecoder, TextRegion,
        TextRegionDecoder,
    },
    segment::{
        PageAssociation, PageNumber, Parsed, RegionKind, RegionMode, Segment, SegmentHeader,
        SegmentNumber, SegmentType,
    },
    stream::{Input, SegmentStream},
};

/// Explicit cumulative limits; there is deliberately no implicit unlimited default.
///
/// Counts apply to a decoding job, including intermediate resources. Allocation
/// accounting is cumulative requested bytes, not the process's resident memory.
/// Work counts entropy decisions, decoded pixels, and symbol/pattern placements.
#[derive(Debug, Clone, Copy)]
pub struct DecodeLimits {
    /// Maximum total encoded bytes accepted by this job.
    pub input_bytes: u64,
    /// Maximum number of parsed segments.
    pub segments: u64,
    /// Maximum total referred-to segment entries.
    pub references: u64,
    /// Maximum cumulative decoded bitmap pixels.
    pub pixels: u64,
    /// Maximum cumulative requested allocation bytes.
    pub allocation_bytes: u64,
    /// Maximum cumulative decoded symbols.
    pub symbols: u64,
    /// Maximum abstract entropy and composition work units.
    pub work: u64,
}

/// Mutable accounting shared explicitly across parsing and decoding stages.
#[derive(Debug)]
pub struct DecodeBudget {
    limits: DecodeLimits,
    used: DecodeLimits,
}

impl DecodeLimits {
    /// Limits for one PDF embedded image, scaled to its declared size and the
    /// encoded bytes the caller will supply.
    ///
    /// `encoded_bytes` must cover the page stream and any globals together,
    /// since both charge the same input counter.
    ///
    /// These are a safety net against a small stream demanding unbounded work,
    /// not a tight bound on an honest one. Symbols, intermediate regions and
    /// every composed instance charge the same counters as the page itself, so
    /// each allowance is a generous multiple of what one image needs. A stream
    /// that exceeds them is rejected rather than allowed to exhaust memory;
    /// these are the numbers to revisit if a valid image is ever refused.
    pub fn for_pdf_image(width: u32, height: u32, encoded_bytes: usize) -> Self {
        let area = u64::from(width).saturating_mul(u64::from(height));
        let encoded = u64::try_from(encoded_bytes).unwrap_or(u64::MAX);
        Self {
            // A kilobyte of slack, so a caller that rounds its byte count does
            // not sit exactly on the limit.
            input_bytes: encoded.saturating_add(1024),
            // A segment costs at least its eleven-byte header.
            segments: encoded.saturating_div(11).saturating_add(64),
            // A referred-to entry costs at least one header byte.
            references: encoded.saturating_add(64),
            // Cumulative bitmap pixels: the page, its symbols and patterns, and
            // every intermediate region.
            pixels: area.saturating_mul(64).saturating_add(1 << 20),
            // Bitmap storage is an eighth of its pixels; context banks and
            // decoded tables scale with the input instead.
            allocation_bytes: area
                .saturating_mul(8)
                .saturating_add(encoded.saturating_mul(256))
                .saturating_add(1 << 22),
            // Declared symbol counts are read from the stream before anything
            // is allocated, so this is what bounds a fabricated count.
            symbols: encoded.saturating_mul(8).saturating_add(1 << 16),
            // Entropy decisions and composition work, which a text region
            // repeats once per placed instance.
            work: area
                .saturating_mul(256)
                .saturating_add(encoded.saturating_mul(8192))
                .saturating_add(1 << 24),
        }
    }
}

impl DecodeBudget {
    /// Begin accounting with all usage counters zero and caller-selected limits.
    pub fn new(limits: DecodeLimits) -> Self {
        Self {
            limits,
            used: DecodeLimits {
                input_bytes: 0,
                segments: 0,
                references: 0,
                pixels: 0,
                allocation_bytes: 0,
                symbols: 0,
                work: 0,
            },
        }
    }

    /// Charge a resource, returning overflow or limit errors before doing work.
    /// Rejected charges leave accounting unchanged.
    pub fn charge(&mut self, kind: LimitKind, amount: u64) -> Result<()> {
        let (used, limit) = match kind {
            LimitKind::InputBytes => (&mut self.used.input_bytes, self.limits.input_bytes),
            LimitKind::Segments => (&mut self.used.segments, self.limits.segments),
            LimitKind::References => (&mut self.used.references, self.limits.references),
            LimitKind::Pixels => (&mut self.used.pixels, self.limits.pixels),
            LimitKind::AllocationBytes => (
                &mut self.used.allocation_bytes,
                self.limits.allocation_bytes,
            ),
            LimitKind::Symbols => (&mut self.used.symbols, self.limits.symbols),
            LimitKind::Work => (&mut self.used.work, self.limits.work),
        };
        let next = used
            .checked_add(amount)
            .ok_or(crate::Jbig2Error::ResourceLimit(kind))?;
        if next > limit {
            return Err(crate::Jbig2Error::ResourceLimit(kind));
        }
        *used = next;
        Ok(())
    }

    /// Inspect cumulative usage without resetting any counters.
    pub fn usage(&self) -> DecodeLimits {
        self.used
    }
}

/// Page height from the page information segment.
#[derive(Debug, Clone, Copy)]
pub enum PageHeight {
    /// Complete height known before composition begins.
    Known(u32),
    /// Height established by valid end-of-stripe/page processing.
    Unknown,
}

/// Page storage organization described by its striping information.
#[derive(Debug, Clone, Copy)]
pub enum Striping {
    /// Complete page dimensions are known and no stripes are declared.
    None,
    /// Composition progresses through stripes, with checked bitmap growth.
    Striped {
        /// Declared maximum stripe height in rows.
        max_height: u16,
    },
}

/// Page information syntax (T.88 section 7.4.8), before lifecycle validation.
#[derive(Debug, Clone, Copy)]
pub struct PageInfo {
    /// Width of the composed page in pixels.
    pub width: u32,
    /// Declared or initially unknown page height.
    pub height: PageHeight,
    /// Horizontal resolution as encoded by T.88.
    pub resolution_x: u32,
    /// Vertical resolution as encoded by T.88.
    pub resolution_y: u32,
    /// Initial pixel value of the composed page.
    pub default_black: bool,
    /// Default operator applied to immediate regions.
    pub default_operator: CombinationOperator,
    /// Permit a region to override the default operator.
    pub allow_operator_override: bool,
    /// Page striping and growth constraints.
    pub striping: Striping,
    /// Page flags declare that eventual reconstruction is lossless.
    pub lossless: bool,
    /// Page flags declare that refinement may be required.
    pub requires_refinement: bool,
    /// Page flags declare that auxiliary buffers may be used.
    pub auxiliary_buffers: bool,
}

/// Decode the page striping information field (T.88 section 7.4.8.6).
fn parse_striping(encoded: u16) -> Striping {
    if encoded & 0x8000 == 0 {
        Striping::None
    } else {
        Striping::Striped {
            max_height: encoded & 0x7fff,
        }
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Sealed lifecycle marker; downstream crates cannot introduce new page states.
pub trait PageState: sealed::Sealed {}

/// Page information has been parsed and validated, but no pixels are allocated.
#[derive(Debug)]
pub struct HeaderParsed {
    _private: (),
}

/// The page accepts immediate regions and valid stripe-completion markers.
#[derive(Debug)]
pub struct Compositing {
    _private: (),
}

/// The page is finalized and its immutable output may be extracted.
#[derive(Debug)]
pub struct Complete {
    _private: (),
}

impl sealed::Sealed for HeaderParsed {}
impl sealed::Sealed for Compositing {}
impl sealed::Sealed for Complete {}
impl PageState for HeaderParsed {}
impl PageState for Compositing {}
impl PageState for Complete {}

/// Owned page with compile-time lifecycle sequencing and private construction.
///
/// Transitions consume the previous state. Runtime validation still checks page
/// associations, stripe boundaries, and dimensions. These states are Rust-only;
/// foreign adapters should keep the page behind an owned, opaque handle.
///
/// Incomplete pages cannot export output:
/// ```compile_fail
/// use pdf_jbig2::document::{HeaderParsed, Page};
/// fn premature(page: Page<HeaderParsed>) { let _ = page.into_bitmap(); }
/// ```
/// Compositing pages also cannot export final output:
/// ```compile_fail
/// use pdf_jbig2::document::{Compositing, Page};
/// fn premature(page: Page<Compositing>) { let _ = page.into_bitmap(); }
/// ```
/// Completed pages have no composition operation:
/// ```compile_fail
/// use pdf_jbig2::{bitmap::{BitmapCompositor, BitmapRef, Placement},
///     document::{Complete, DecodeBudget, Page}};
/// fn late(page: &mut Page<Complete>, pixels: BitmapRef<'_>, placement: Placement,
///     compositor: &mut dyn BitmapCompositor, budget: &mut DecodeBudget) {
///     page.compose(pixels, placement, compositor, budget);
/// }
/// ```
/// Callers cannot manufacture states:
/// ```compile_fail
/// use pdf_jbig2::document::Complete;
/// let state = Complete { _private: () };
/// ```
/// Nor can they introduce a state that bypasses the lifecycle:
/// ```compile_fail
/// use pdf_jbig2::document::PageState;
/// struct Forged;
/// impl PageState for Forged {}
/// ```
#[derive(Debug)]
pub struct Page<S: PageState> {
    number: PageNumber,
    info: PageInfo,
    bitmap: Bitmap,
    state: PhantomData<S>,
}

impl<S: PageState> Page<S> {
    /// Return the validated nonzero page association.
    pub fn number(&self) -> PageNumber {
        self.number
    }

    /// Inspect the original validated page information without changing state.
    pub fn info(&self) -> &PageInfo {
        &self.info
    }
}

impl Page<HeaderParsed> {
    /// Parse a page-information payload and validate height/striping invariants.
    /// Reports syntax, geometry, reserved-field, and resource-limit errors.
    pub fn parse<'a>(
        number: PageNumber,
        input: &'a [u8],
        location: SourceLocation,
        _budget: &mut DecodeBudget,
    ) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(input, location);
        let width = cursor.u32()?;
        let raw_height = cursor.u32()?;
        if width == 0 || raw_height == 0 {
            return Err(Jbig2Error::InvalidBitmap(
                "page dimensions must be positive",
            ));
        }
        let resolution_x = cursor.u32()?;
        let resolution_y = cursor.u32()?;
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        if flags & 0x80 != 0 {
            return Err(Jbig2Error::ReservedField {
                location: flags_location,
                field: "page segment flags",
            });
        }
        let striping = parse_striping(cursor.u16()?);
        if raw_height == u32::MAX && matches!(striping, Striping::None) {
            return Err(Jbig2Error::InvalidPageSequence(
                "unknown page height requires stripes",
            ));
        }
        let info = PageInfo {
            width,
            height: if raw_height == u32::MAX {
                PageHeight::Unknown
            } else {
                PageHeight::Known(raw_height)
            },
            resolution_x,
            resolution_y,
            default_black: flags & 4 != 0,
            default_operator: CombinationOperator::from_page_default(flags >> 3)?,
            allow_operator_override: flags & 0x40 != 0,
            striping,
            lossless: flags & 1 != 0,
            requires_refinement: flags & 2 != 0,
            auxiliary_buffers: flags & 0x20 != 0,
        };
        Ok(Parsed {
            value: Self {
                number,
                info,
                bitmap: Bitmap::empty(),
                state: PhantomData,
            },
            remaining: cursor.remaining(),
            consumed: cursor.position(),
        })
    }

    /// Consume the parsed page and allocate initialized composition storage.
    ///
    /// A striped page of known height is allocated in full, so its stripes
    /// need no growth. A page whose height the stripes discover is not
    /// supported.
    pub fn begin(self, budget: &mut DecodeBudget) -> Result<Page<Compositing>> {
        let height = match self.info.height {
            PageHeight::Known(height) => height,
            PageHeight::Unknown => {
                return Err(Jbig2Error::UnsupportedFeature("striped JBIG2 pages"));
            }
        };
        let bitmap = Bitmap::new(self.info.width, height, self.info.default_black, budget)?;
        Ok(Page {
            number: self.number,
            info: self.info,
            bitmap,
            state: PhantomData,
        })
    }
}

impl Page<Compositing> {
    /// Borrow current pixels for refinement without permitting final extraction.
    /// The borrow must end before page composition or stripe growth resumes.
    pub fn reference_bitmap(&self) -> BitmapRef<'_> {
        self.bitmap.as_ref()
    }

    /// Apply page override rules, clip the region, and invoke the compositor.
    /// Unknown-height striped pages grow only within the shared budget.
    pub fn compose(
        &mut self,
        region: BitmapRef<'_>,
        mut placement: Placement,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        if !self.info.allow_operator_override {
            placement.operator = self.info.default_operator;
        }
        compositor.compose(&mut self.bitmap, region, placement, budget)
    }

    /// Process an end-of-stripe row.
    ///
    /// `last_row` is the inclusive page-relative row index from the segment.
    /// Every compositing page holds its full height, so a stripe boundary
    /// changes no storage; it only has to belong to a striped page.
    pub fn end_stripe(&mut self, _last_row: u32, _budget: &mut DecodeBudget) -> Result<()> {
        if matches!(self.info.striping, Striping::None) {
            return Err(Jbig2Error::InvalidPageSequence(
                "end of stripe on an unstriped page",
            ));
        }
        Ok(())
    }

    /// Adopt the defaults of a page information segment for a canvas an
    /// embedding application already allocated from its own dimensions.
    ///
    /// The canvas keeps its size. Its pixels are reset to the declared default
    /// value, which is only valid before any region has been composed.
    fn adopt_defaults(&mut self, info: PageInfo, budget: &mut DecodeBudget) -> Result<()> {
        let height = self.bitmap.as_ref().height();
        let height_matches = match info.height {
            PageHeight::Known(declared) => declared == height,
            PageHeight::Unknown => true,
        };
        if info.width != self.info.width || !height_matches {
            return Err(Jbig2Error::InvalidPageSequence(
                "page information differs from PDF dimensions",
            ));
        }
        if info.default_black != self.info.default_black {
            self.bitmap = Bitmap::new(info.width, height, info.default_black, budget)?;
        }
        self.info = PageInfo {
            height: PageHeight::Known(height),
            ..info
        };
        Ok(())
    }

    /// Complete a page after an end-of-page or embedding-defined completion event.
    ///
    /// Unstriped pages allocate their full height up front, so nothing remains
    /// to resolve here. Striped pages are rejected by `begin`, and resolving
    /// their final height is part of adding stripe support.
    pub fn finish(self, _budget: &mut DecodeBudget) -> Result<Page<Complete>> {
        self.finish_page()
    }

    /// Finalize without a budget, for callers that hold no budget borrow.
    fn finish_page(self) -> Result<Page<Complete>> {
        Ok(Page {
            number: self.number,
            info: self.info,
            bitmap: self.bitmap,
            state: PhantomData,
        })
    }
}

impl Page<Complete> {
    /// Borrow immutable final output while keeping the completed page alive.
    pub fn bitmap(&self) -> BitmapRef<'_> {
        self.bitmap.as_ref()
    }

    /// Transfer output ownership, ending the page handle's lifetime.
    pub fn into_bitmap(self) -> Bitmap {
        self.bitmap
    }
}

/// Decoded reusable globals, independent of the globals input-buffer lifetime.
///
/// A document borrows this owner for resource resolution. Decoding globals and
/// pages may share a budget; reusing cached globals avoids recharging decoding
/// work already performed, while each document still charges its own work.
#[derive(Debug)]
pub struct Globals {
    resources: ResourceStore,
}

impl Globals {
    /// Decode global header/payload pairs, rejecting non-global associations.
    /// Validates reference ordering/types and applies segment retention rules.
    pub fn decode(input: &[u8], budget: &mut DecodeBudget) -> Result<Self> {
        let mut stream = SegmentStream::globals(input, budget)?;
        let mut resources = ResourceStore::new();
        let mut compositor = crate::bitmap::DefaultCompositor;
        while let Some(segment) = stream.next_segment(budget)? {
            let header = segment.header();
            if header.association != PageAssociation::Global {
                return Err(Jbig2Error::InvalidPageSequence("page segment in globals"));
            }
            let resource = match header.segment_type {
                SegmentType::CodeTable => DecodedResource::Huffman(
                    HuffmanTable::parse(segment.payload(), segment.payload_location(), budget)?
                        .value,
                ),
                SegmentType::PatternDictionary => DecodedResource::Patterns(
                    decode_pattern_dictionary(&segment, &resources, &mut compositor, budget)?,
                ),
                SegmentType::SymbolDictionary => DecodedResource::Symbols(
                    decode_symbol_dictionary(&segment, &resources, &mut compositor, budget)?,
                ),
                SegmentType::Profile | SegmentType::Extension => continue,
                SegmentType::EndOfFile => break,
                _ => return Err(Jbig2Error::UnsupportedFeature("global segment type")),
            };
            resources.insert(header.number, resource)?;
        }
        Ok(Self { resources })
    }
}

impl ResourceResolver for Globals {
    fn resolve(&self, number: SegmentNumber) -> Result<DecodedResourceRef<'_>> {
        self.resources.resolve(number)
    }
}

/// Document driver with independent input and decoded-globals lifetimes.
///
/// Dispatch resolves dependencies before decoding, caches intermediate resources,
/// and composites immediate regions. Global and local segment numbers share a
/// namespace; duplicates are errors. Local releases never mutate borrowed
/// globals. The driver checks page association, completion, and file page counts.
/// Returned completed pages own their storage and need not retain input buffers.
///
/// Standalone decoding:
/// ```no_run
/// use pdf_jbig2::{Result, bitmap::DefaultCompositor, document::{DecodeBudget,
///     DecodeLimits, DocumentDecoder}, stream::Input};
/// fn decode(bytes: &[u8], limits: DecodeLimits) -> Result<()> {
///     let mut budget = DecodeBudget::new(limits);
///     let mut decoder = DocumentDecoder::new(Input::Standalone(bytes), None, &mut budget)?;
///     let mut compositor = DefaultCompositor;
///     while let Some(page) = decoder.next_page(&mut compositor, &mut budget)? {
///         let pixels = page.into_bitmap();
///         let _bytes = pixels.into_bytes();
///     }
///     Ok(())
/// }
/// ```
/// Embedded input with separately decoded globals:
/// ```no_run
/// use pdf_jbig2::{Result, bitmap::DefaultCompositor, document::{DecodeBudget,
///     DocumentDecoder, Globals}, stream::Input};
/// fn embedded(globals: &[u8], page: &[u8], budget: &mut DecodeBudget) -> Result<()> {
///     let globals = Globals::decode(globals, budget)?;
///     let mut decoder = DocumentDecoder::new(Input::Embedded(page), Some(&globals), budget)?;
///     let _page = decoder.next_page(&mut DefaultCompositor, budget)?;
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct DocumentDecoder<'a, 'g> {
    stream: SegmentStream<'a>,
    globals: Option<&'g Globals>,
    resources: ResourceStore,
    pages: Vec<(PageNumber, Page<Compositing>)>,
    completed_pages: u32,
    failed: bool,
}

impl<'a, 'g> DocumentDecoder<'a, 'g> {
    /// Prepare a bounded stream and optional reusable globals for dispatch.
    /// No lifetime relationship between encoded page bytes and globals is required.
    pub fn new(
        input: Input<'a>,
        globals: Option<&'g Globals>,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        Ok(Self {
            stream: SegmentStream::new(input, budget)?,
            globals,
            resources: ResourceStore::new(),
            pages: Vec::new(),
            completed_pages: 0,
            failed: false,
        })
    }

    /// Decode until a page completes, or validate the end and return `None`.
    /// Returns pages in completion order. After an error the driver is terminal;
    /// subsequent calls return `None`. Supply the same budget throughout the job.
    pub fn next_page(
        &mut self,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<Option<Page<Complete>>> {
        if self.failed {
            return Ok(None);
        }
        let result = self.decode_until_page(compositor, budget);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(crate) fn for_pdf(
        data: &'a [u8],
        globals: Option<&'g Globals>,
        width: u32,
        height: u32,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Jbig2Error::InvalidBitmap(
                "positive PDF dimensions required",
            ));
        }
        let mut decoder = Self::new(Input::Embedded(data), globals, budget)?;
        let number = PageNumber(
            NonZeroU32::new(1).ok_or(Jbig2Error::InvalidPageSequence("PDF page number"))?,
        );
        let info = PageInfo {
            width,
            height: PageHeight::Known(height),
            resolution_x: 0,
            resolution_y: 0,
            default_black: false,
            default_operator: CombinationOperator::Or,
            allow_operator_override: true,
            striping: Striping::None,
            lossless: false,
            requires_refinement: false,
            auxiliary_buffers: false,
        };
        let page = Page::<HeaderParsed> {
            number,
            info,
            bitmap: Bitmap::empty(),
            state: PhantomData,
        }
        .begin(budget)?;
        decoder.pages.try_reserve(1)?;
        decoder.pages.push((number, page));
        Ok(decoder)
    }

    fn decode_until_page(
        &mut self,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<Option<Page<Complete>>> {
        while let Some(segment) = self.stream.next_segment(budget)? {
            if let Some(page) = self.dispatch(segment, compositor, budget)? {
                return Ok(Some(page));
            }
        }
        if self.pages.len() == 1 {
            return self.take_completed_page(0).map(Some);
        }
        if self.pages.is_empty() {
            Ok(None)
        } else {
            Err(Jbig2Error::InvalidPageSequence(
                "ambiguous incomplete pages",
            ))
        }
    }

    /// Decode one segment, returning a page when this segment completes it.
    fn dispatch(
        &mut self,
        segment: Segment<'a>,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<Option<Page<Complete>>> {
        let header = segment.header();
        // Extension and profile segments are metadata about other segments,
        // including page information and immediate regions, which are never
        // retained as resources.
        if !matches!(
            header.segment_type,
            SegmentType::Extension | SegmentType::Profile
        ) {
            for reference in header.references.iter() {
                let _ = self.resolve(reference)?;
            }
        }
        match header.segment_type {
            SegmentType::PageInformation => self.page_information(&segment, budget).map(|()| None),
            SegmentType::Region { kind, mode } => self
                .region(&segment, kind, mode, compositor, budget)
                .map(|()| None),
            SegmentType::CodeTable => {
                let parsed =
                    HuffmanTable::parse(segment.payload(), segment.payload_location(), budget)?;
                self.resources
                    .insert(header.number, DecodedResource::Huffman(parsed.value))
                    .map(|()| None)
            }
            SegmentType::PatternDictionary => {
                let dictionary = decode_pattern_dictionary(&segment, self, compositor, budget)?;
                self.resources
                    .insert(header.number, DecodedResource::Patterns(dictionary))
                    .map(|()| None)
            }
            SegmentType::SymbolDictionary => {
                let dictionary = decode_symbol_dictionary(&segment, self, compositor, budget)?;
                self.resources
                    .insert(header.number, DecodedResource::Symbols(dictionary))
                    .map(|()| None)
            }
            SegmentType::EndOfPage => self.complete_page(header.association).map(Some),
            SegmentType::EndOfStripe => self.end_of_stripe(&segment, budget).map(|()| None),
            SegmentType::Profile | SegmentType::Extension => Ok(None),
            _ => Err(Jbig2Error::UnsupportedFeature("segment decoding procedure")),
        }
    }

    /// Begin a page, or reconcile a page information segment with the canvas
    /// an embedding application already established from its own dimensions.
    fn page_information(&mut self, segment: &Segment<'a>, budget: &mut DecodeBudget) -> Result<()> {
        let header = segment.header();
        let PageAssociation::Page(number) = header.association else {
            return Err(Jbig2Error::InvalidPageSequence("global page information"));
        };
        let parsed = Page::<HeaderParsed>::parse(
            number,
            segment.payload(),
            segment.payload_location(),
            budget,
        )?;
        let Some((_, existing)) = self.pages.iter_mut().find(|(id, _)| *id == number) else {
            self.pages.try_reserve(1)?;
            self.pages.push((number, parsed.value.begin(budget)?));
            return Ok(());
        };
        existing.adopt_defaults(parsed.value.info, budget)
    }

    /// Mark the end of a stripe on the page a segment is associated with.
    fn end_of_stripe(&mut self, segment: &Segment<'a>, budget: &mut DecodeBudget) -> Result<()> {
        let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
        let last_row = cursor.u32()?;
        self.page_mut(segment.header().association, "end of stripe before page")?
            .end_stripe(last_row, budget)
    }

    /// Decode a region segment and apply its retention or composition behavior.
    fn region(
        &mut self,
        segment: &Segment<'a>,
        kind: RegionKind,
        mode: RegionMode,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        let (bitmap, placement) = self.decode_region(segment, kind, compositor, budget)?;
        self.retain_or_compose(
            segment.header(),
            mode,
            placement,
            bitmap,
            compositor,
            budget,
        )
    }

    /// Reconstruct a region's pixels without touching any decoder-owned storage.
    fn decode_region(
        &self,
        segment: &Segment<'a>,
        kind: RegionKind,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<(Bitmap, Placement)> {
        // A refinement region's reference must be resolved before the shared
        // decode context borrows this decoder, because the fallback reference is
        // the partially composed page itself.
        let reference = match kind {
            RegionKind::GenericRefinement => Some(self.refinement_reference(segment.header())?),
            _ => None,
        };
        let tables = match kind {
            RegionKind::Text => code_tables(self, segment.header())?,
            _ => Vec::new(),
        };
        let mut context = DecodeContext {
            header: segment.header(),
            location: segment.payload_location(),
            resources: self,
            budget,
            compositor,
        };
        match kind {
            RegionKind::Generic => {
                let region = GenericRegion::parse(*segment, context.budget)?.value;
                let bitmap = GenericRegionDecoder.decode(&region, None, &mut context)?;
                Ok((bitmap, region.info.placement))
            }
            RegionKind::GenericRefinement => {
                let region = RefinementRegion::parse(*segment, context.budget)?.value;
                let reference = reference.ok_or(Jbig2Error::InvalidPageSequence(
                    "refinement without a reference",
                ))?;
                let bitmap = RefinementRegionDecoder.decode(&region, reference, &mut context)?;
                Ok((bitmap, region.info.placement))
            }
            RegionKind::Halftone => {
                let region = HalftoneRegion::parse(*segment, context.budget)?.value;
                let bitmap = HalftoneRegionDecoder.decode(&region, &mut context)?;
                Ok((bitmap, region.info.placement))
            }
            RegionKind::Text => {
                let region = TextRegion::parse(*segment, &tables, context.budget)?.value;
                let bitmap = TextRegionDecoder.decode(&region, &mut context)?;
                Ok((bitmap, region.info.placement))
            }
        }
    }

    /// Select the bitmap a refinement region refines: a referred-to intermediate
    /// region when one is available, otherwise the page being composed.
    fn refinement_reference(&self, header: &SegmentHeader<'a>) -> Result<RefinementReference<'_>> {
        let referred = header
            .references
            .iter()
            .find_map(|id| match self.resolve(id) {
                Ok(DecodedResourceRef::Region(bitmap)) => Some(bitmap),
                _ => None,
            });
        match referred {
            Some(bitmap) => Ok(RefinementReference::Region(bitmap)),
            None => Ok(RefinementReference::Page(
                self.page(header.association, "refinement before page")?
                    .reference_bitmap(),
            )),
        }
    }

    /// Retain an intermediate region as a resource, or compose an immediate one.
    fn retain_or_compose(
        &mut self,
        header: &SegmentHeader<'a>,
        mode: RegionMode,
        placement: Placement,
        bitmap: Bitmap,
        compositor: &mut dyn BitmapCompositor,
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        match mode {
            RegionMode::Intermediate => self
                .resources
                .insert(header.number, DecodedResource::Region(bitmap)),
            RegionMode::Immediate | RegionMode::ImmediateLossless => {
                let page = self.page_mut(header.association, "region before page")?;
                page.compose(bitmap.as_ref(), placement, compositor, budget)
            }
        }
    }

    /// Borrow the compositing page a segment is associated with.
    fn page(&self, association: PageAssociation, what: &'static str) -> Result<&Page<Compositing>> {
        let PageAssociation::Page(number) = association else {
            return Err(Jbig2Error::InvalidPageSequence(what));
        };
        self.pages
            .iter()
            .find(|(id, _)| *id == number)
            .map(|(_, page)| page)
            .ok_or(Jbig2Error::InvalidPageSequence(what))
    }

    /// Borrow the compositing page a segment is associated with, for mutation.
    fn page_mut(
        &mut self,
        association: PageAssociation,
        what: &'static str,
    ) -> Result<&mut Page<Compositing>> {
        let PageAssociation::Page(number) = association else {
            return Err(Jbig2Error::InvalidPageSequence(what));
        };
        self.pages
            .iter_mut()
            .find(|(id, _)| *id == number)
            .map(|(_, page)| page)
            .ok_or(Jbig2Error::InvalidPageSequence(what))
    }

    /// Remove a completed page from the in-progress set and finalize it.
    fn complete_page(&mut self, association: PageAssociation) -> Result<Page<Complete>> {
        let PageAssociation::Page(number) = association else {
            return Err(Jbig2Error::InvalidPageSequence("global end of page"));
        };
        let index = self
            .pages
            .iter()
            .position(|(id, _)| *id == number)
            .ok_or(Jbig2Error::InvalidPageSequence("unknown page completion"))?;
        self.take_completed_page(index)
    }

    /// Finalize the page at `index`, charging it against the file page count.
    fn take_completed_page(&mut self, index: usize) -> Result<Page<Complete>> {
        if index >= self.pages.len() {
            return Err(Jbig2Error::InvalidPageSequence("unknown page completion"));
        }
        let (_, page) = self.pages.remove(index);
        self.completed_pages = self
            .completed_pages
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("completed pages"))?;
        page.finish_page()
    }
}

/// Resolve a dependency against this document's own resources, falling back to
/// the reusable globals. Local definitions shadow globals of the same number.
impl ResourceResolver for DocumentDecoder<'_, '_> {
    fn resolve(&self, number: SegmentNumber) -> Result<DecodedResourceRef<'_>> {
        match self.resources.resolve(number) {
            Err(Jbig2Error::MissingReference(_)) => self
                .globals
                .ok_or(Jbig2Error::MissingReference(number))?
                .resolve(number),
            other => other,
        }
    }
}

/// Decode a pattern dictionary segment against already decoded resources.
///
/// Both the document driver and globals decoding need this, and neither can
/// lend the other its resource store.
fn decode_pattern_dictionary(
    segment: &Segment<'_>,
    resources: &dyn ResourceResolver,
    compositor: &mut dyn BitmapCompositor,
    budget: &mut DecodeBudget,
) -> Result<PatternDictionary> {
    let descriptor = PatternDictionaryDescriptor::parse(*segment, budget)?.value;
    let mut context = DecodeContext {
        header: segment.header(),
        location: segment.payload_location(),
        resources,
        budget,
        compositor,
    };
    PatternDictionaryDecoder.decode(&descriptor, &mut context)
}

/// Decode a symbol dictionary segment into an owned, exportable dictionary.
///
/// Like pattern dictionaries, symbol dictionaries may be global or local.
fn decode_symbol_dictionary(
    segment: &Segment<'_>,
    resources: &dyn ResourceResolver,
    compositor: &mut dyn BitmapCompositor,
    budget: &mut DecodeBudget,
) -> Result<SymbolDictionary> {
    let tables = code_tables(resources, segment.header())?;
    let descriptor = SymbolDictionaryDescriptor::parse(*segment, &tables, budget)?.value;
    let mut context = DecodeContext {
        header: segment.header(),
        location: segment.payload_location(),
        resources,
        budget,
        compositor,
    };
    SymbolDictionaryDecoder.decode(&descriptor, &mut context)
}

/// Collect the referred-to Code Table segments in header order.
///
/// Custom Huffman selections consume these in the order the segment's own
/// fields name them, so the order here is part of the contract.
fn code_tables(
    resources: &dyn ResourceResolver,
    header: &SegmentHeader<'_>,
) -> Result<Vec<SegmentNumber>> {
    let mut tables = Vec::new();
    for id in header.references.iter() {
        if matches!(resources.resolve(id), Ok(DecodedResourceRef::Huffman(_))) {
            tables.try_reserve(1)?;
            tables.push(id);
        }
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::{Complete, DecodeBudget, HeaderParsed, Page};
    use crate::{Result, error::LimitKind};

    #[test]
    fn page_lifecycle_allocates_composes_and_finishes() -> Result<()> {
        use crate::{
            bitmap::{BitmapRef, CombinationOperator, DefaultCompositor, Placement},
            error::{InputOrigin, SourceLocation},
            segment::PageNumber,
        };
        use std::num::NonZeroU32;
        let mut budget = DecodeBudget::new(super::DecodeLimits {
            input_bytes: 1000,
            segments: 100,
            references: 100,
            pixels: 100,
            allocation_bytes: 1000,
            symbols: 100,
            work: 100,
        });
        let header = [0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let number = PageNumber(
            NonZeroU32::new(1).ok_or(crate::Jbig2Error::InvalidPageSequence("page number"))?,
        );
        let parsed = Page::<HeaderParsed>::parse(
            number,
            &header,
            SourceLocation {
                origin: InputOrigin::Embedded,
                byte_offset: 0,
                bit_offset: None,
            },
            &mut budget,
        )?;
        let mut page = parsed.value.begin(&mut budget)?;
        let source = BitmapRef::new(1, 1, 1, &[0x80])?;
        page.compose(
            source,
            Placement {
                x: 1,
                y: 0,
                operator: CombinationOperator::Replace,
            },
            &mut DefaultCompositor,
            &mut budget,
        )?;
        let result = page.finish(&mut budget)?.into_bitmap();
        assert_eq!(result.as_ref().bytes(), &[0x40]);
        Ok(())
    }

    #[test]
    fn budget_rejects_limit_and_overflow_without_changing_usage() {
        let limits = super::DecodeLimits {
            input_bytes: 3,
            segments: u64::MAX,
            references: 0,
            pixels: 0,
            allocation_bytes: 0,
            symbols: 0,
            work: 0,
        };
        let mut budget = DecodeBudget::new(limits);
        assert_eq!(budget.usage().input_bytes, 0);
        assert!(budget.charge(LimitKind::InputBytes, 3).is_ok());
        assert!(budget.charge(LimitKind::InputBytes, 1).is_err());
        assert_eq!(budget.usage().input_bytes, 3);
        assert!(budget.charge(LimitKind::Segments, u64::MAX).is_ok());
        assert!(budget.charge(LimitKind::Segments, 1).is_err());
        assert_eq!(budget.usage().segments, u64::MAX);
    }

    // A type-checked function pointer proves the valid lifecycle without calling stubs.
    const _: fn(Page<HeaderParsed>, &mut DecodeBudget) -> Result<Page<Complete>> =
        |page, budget| page.begin(budget)?.finish(budget);
}
