use crate::{
    Jbig2Error, Result,
    arithmetic::{ArithmeticContexts, ArithmeticDecoder},
    bitmap::{Bitmap, BitmapCompositor, BitmapRef, CombinationOperator, Placement},
    cursor::ByteCursor,
    dictionary::{DecodedResourceRef, PatternDictionary, ResourceResolver},
    document::DecodeBudget,
    error::{LimitKind, SourceLocation},
    huffman::TableSelection,
    segment::{Parsed, RegionMode, Segment, SegmentHeader, SegmentNumber, SegmentType},
    template::{GenericTemplate, GenericTemplateId, RefinementTemplate, RefinementTemplateId},
};

/// Shared metadata for region segment information fields (T.88 section 7.4.1).
#[derive(Debug, Clone, Copy)]
pub struct RegionInfo {
    /// Decoded pixel width, before any page clipping.
    pub width: u32,
    /// Declared pixel height; an unknown-length generic payload may supply fewer rows.
    pub height: u32,
    /// Page placement and the region's requested combination operator.
    pub placement: Placement,
    /// Intermediate retention or immediate composition behavior.
    pub mode: RegionMode,
}

impl RegionInfo {
    /// Parse the seventeen-byte region information field.
    pub(crate) fn parse(cursor: &mut ByteCursor<'_>, segment_type: SegmentType) -> Result<Self> {
        let SegmentType::Region { mode, .. } = segment_type else {
            return Err(Jbig2Error::MalformedHeader {
                location: cursor.location(),
                field: "region segment type",
            });
        };
        let width = cursor.u32()?;
        let height = cursor.u32()?;
        let x = cursor.i32()?;
        let y = cursor.i32()?;
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        if flags & 0xf8 != 0 {
            return Err(Jbig2Error::ReservedField {
                location: flags_location,
                field: "region combination flags",
            });
        }
        let operator =
            CombinationOperator::try_from(flags & 7).map_err(|_| Jbig2Error::ReservedField {
                location: flags_location,
                field: "region combination operator",
            })?;
        Ok(Self {
            width,
            height,
            placement: Placement {
                x: i64::from(x),
                y: i64::from(y),
                operator,
            },
            mode,
        })
    }
}

/// Explicit short-lived dependencies shared by concrete coding procedures.
///
/// The resolver borrow prevents resource eviction during decoding. Budget and
/// compositor borrows are exclusive. This context must never cross an FFI ABI;
/// adapters construct it inside a Rust call. Nested procedures reborrow it.
pub struct DecodeContext<'a> {
    /// Header supplying allowed segment references and diagnostic identity.
    pub header: &'a SegmentHeader<'a>,
    /// Source location of the bounded coded payload.
    pub location: SourceLocation,
    /// Already decoded resources, including applicable globals.
    pub resources: &'a dyn ResourceResolver,
    /// Shared cumulative resource and work accounting.
    pub budget: &'a mut DecodeBudget,
    /// Pixel composition used by text, halftone, and symbol aggregation.
    pub compositor: &'a mut dyn BitmapCompositor,
}

/// Generic region entropy mode (T.88 section 6.2).
#[derive(Debug, Clone, Copy)]
pub enum GenericCoding {
    /// Modified Modified READ bilevel coding.
    Mmr,
    /// Context-adaptive arithmetic bitmap coding.
    Arithmetic {
        /// Template and resolved adaptive pixel coordinates.
        template: GenericTemplate,
        /// Enable typical prediction for generic decoding.
        typical_prediction: bool,
    },
}

/// Borrowed syntax for a generic region and its bounded entropy payload.
#[derive(Debug)]
pub struct GenericRegion<'a> {
    /// Region geometry and composition behavior.
    pub info: RegionInfo,
    /// MMR or arithmetic coding parameters.
    pub coding: GenericCoding,
    /// Actual height from unknown-length termination, when present.
    pub actual_rows: Option<u32>,
    /// Borrowed coded bytes; the input owner must outlive this descriptor.
    pub data: &'a [u8],
}

impl<'a> GenericRegion<'a> {
    /// Parse region syntax and validate coding-specific flags and dimensions.
    pub fn parse(segment: Segment<'a>, _budget: &mut DecodeBudget) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
        let info = RegionInfo::parse(&mut cursor, segment.header().segment_type)?;
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        if flags & 0xf0 != 0 {
            return Err(Jbig2Error::ReservedField {
                location: flags_location,
                field: "generic region flags",
            });
        }
        let coding = if flags & 1 != 0 {
            GenericCoding::Mmr
        } else {
            let id = GenericTemplateId::try_from((flags >> 1) & 3)?;
            GenericCoding::Arithmetic {
                template: GenericTemplate::parse(id, &mut cursor)?,
                typical_prediction: flags & 8 != 0,
            }
        };
        let data = cursor.remaining();
        Ok(Parsed {
            value: Self {
                info,
                coding,
                actual_rows: segment.actual_rows(),
                data,
            },
            remaining: data,
            consumed: cursor.position(),
        })
    }

    /// Resolve the row count actually present, which an unknown-length payload
    /// may reduce below the declared height.
    fn rows(&self) -> Result<u32> {
        let height = self.actual_rows.unwrap_or(self.info.height);
        if height > self.info.height {
            return Err(Jbig2Error::InvalidBitmap(
                "actual generic rows exceed declared height",
            ));
        }
        Ok(height)
    }
}

/// Concrete arithmetic/MMR generic bitmap reconstruction.
#[derive(Debug, Default)]
pub struct GenericRegionDecoder;

impl GenericRegionDecoder {
    /// Reconstruct pixels with an optional skip bitmap supplied by a caller.
    /// Reports malformed coding, incompatible skip geometry, or budget failures.
    pub fn decode(
        &mut self,
        region: &GenericRegion<'_>,
        skip: Option<BitmapRef<'_>>,
        context: &mut DecodeContext<'_>,
    ) -> Result<Bitmap> {
        if let Some(mask) = skip
            && (mask.width() != region.info.width || mask.height() != region.info.height)
        {
            return Err(Jbig2Error::InvalidBitmap("generic skip mask geometry"));
        }
        let height = region.rows()?;
        match region.coding {
            GenericCoding::Mmr => {
                let (bitmap, _consumed) = crate::mmr::decode(
                    region.info.width,
                    height,
                    region.data,
                    context.location,
                    context.budget,
                )?;
                apply_skip(bitmap, skip)
            }
            GenericCoding::Arithmetic {
                template,
                typical_prediction,
            } => {
                let plane = GenericPlane {
                    width: region.info.width,
                    height,
                    template,
                    typical_prediction,
                    skip,
                };
                let mut states =
                    ArithmeticContexts::new(template.id().context_count(), context.budget)?;
                let mut decoder = ArithmeticDecoder::new(region.data, context.location)?;
                let bitmap = plane.decode(&mut decoder, &mut states, context.budget)?;
                Ok(bitmap)
            }
        }
    }
}

/// Clear every pixel a skip mask marks, for coding paths that decode the whole
/// bitmap before the mask is applied.
fn apply_skip(mut bitmap: Bitmap, skip: Option<BitmapRef<'_>>) -> Result<Bitmap> {
    let Some(mask) = skip else {
        return Ok(bitmap);
    };
    let (width, height) = (bitmap.as_ref().width(), bitmap.as_ref().height());
    for y in 0..height {
        for x in 0..width {
            if mask.pixel(x, y).unwrap_or(false) {
                bitmap.set_pixel(x, y, false)?;
            }
        }
    }
    Ok(bitmap)
}

/// One arithmetic generic bitmap decoded against caller-owned entropy state.
///
/// Symbol dictionaries and halftone bitplanes decode several planes from one
/// arithmetic decoder, so the decoder and contexts are supplied per call rather
/// than created here (T.88 section 6.2.5.7).
pub(crate) struct GenericPlane<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) template: GenericTemplate,
    pub(crate) typical_prediction: bool,
    pub(crate) skip: Option<BitmapRef<'a>>,
}

impl GenericPlane<'_> {
    /// Decode this plane's pixels, advancing the supplied decoder and contexts.
    pub(crate) fn decode(
        &self,
        decoder: &mut ArithmeticDecoder<'_>,
        states: &mut ArithmeticContexts,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let mut bitmap = Bitmap::new(self.width, self.height, false, budget)?;
        budget.charge(
            LimitKind::Work,
            u64::from(self.width)
                .checked_mul(u64::from(self.height))
                .ok_or(Jbig2Error::Overflow("generic region work"))?,
        )?;
        let mut ltp = false;
        for y in 0..self.height {
            if self.typical_prediction {
                let index = self.template.id().typical_prediction_context();
                ltp ^= decoder.decode_bit(states, index, budget)?;
            }
            if ltp {
                copy_previous_row(&mut bitmap, y, self.width)?;
                continue;
            }
            self.decode_row(&mut bitmap, y, decoder, states, budget)?;
        }
        Ok(bitmap)
    }

    /// Decode one row left to right, leaving skipped pixels unset and unread.
    fn decode_row(
        &self,
        bitmap: &mut Bitmap,
        y: u32,
        decoder: &mut ArithmeticDecoder<'_>,
        states: &mut ArithmeticContexts,
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        for x in 0..self.width {
            if self.skip.and_then(|mask| mask.pixel(x, y)).unwrap_or(false) {
                continue;
            }
            let index = generic_context(bitmap.as_ref(), x, y, self.template)?;
            let pixel = decoder.decode_bit(states, index, budget)?;
            bitmap.set_pixel(x, y, pixel)?;
        }
        Ok(())
    }
}

/// Replicate the row above, which typical prediction selects for identical rows.
fn copy_previous_row(bitmap: &mut Bitmap, y: u32, width: u32) -> Result<()> {
    for x in 0..width {
        let above = y
            .checked_sub(1)
            .and_then(|row| bitmap.as_ref().pixel(x, row))
            .unwrap_or(false);
        bitmap.set_pixel(x, y, above)?;
    }
    Ok(())
}

/// A fixed context pixel: column delta, row delta, and its context bit position.
type ContextPixel = (i8, i8, u8);

/// Fixed context pixels and adaptive bit positions for each generic template
/// (T.88 section 6.2.5.3, figures 4 through 7).
const GENERIC_TEMPLATE_0: (&[ContextPixel], &[u8]) = (
    &[
        (-1, 0, 0),
        (-2, 0, 1),
        (-3, 0, 2),
        (-4, 0, 3),
        (2, -1, 5),
        (1, -1, 6),
        (0, -1, 7),
        (-1, -1, 8),
        (-2, -1, 9),
        (1, -2, 12),
        (0, -2, 13),
        (-1, -2, 14),
    ],
    &[4, 10, 11, 15],
);
const GENERIC_TEMPLATE_1: (&[ContextPixel], &[u8]) = (
    &[
        (-1, 0, 0),
        (-2, 0, 1),
        (-3, 0, 2),
        (2, -1, 4),
        (1, -1, 5),
        (0, -1, 6),
        (-1, -1, 7),
        (-2, -1, 8),
        (2, -2, 9),
        (1, -2, 10),
        (0, -2, 11),
        (-1, -2, 12),
    ],
    &[3],
);
const GENERIC_TEMPLATE_2: (&[ContextPixel], &[u8]) = (
    &[
        (-1, 0, 0),
        (-2, 0, 1),
        (1, -1, 3),
        (0, -1, 4),
        (-1, -1, 5),
        (-2, -1, 6),
        (1, -2, 7),
        (0, -2, 8),
        (-1, -2, 9),
    ],
    &[2],
);
const GENERIC_TEMPLATE_3: (&[ContextPixel], &[u8]) = (
    &[
        (-1, 0, 0),
        (-2, 0, 1),
        (-3, 0, 2),
        (-4, 0, 3),
        (1, -1, 5),
        (0, -1, 6),
        (-1, -1, 7),
        (-2, -1, 8),
        (-3, -1, 9),
    ],
    &[4],
);

/// Build the arithmetic context value addressing one generic pixel's neighbors.
fn generic_context(
    bitmap: BitmapRef<'_>,
    x: u32,
    y: u32,
    template: GenericTemplate,
) -> Result<usize> {
    let (fixed, adaptive_bits) = match template.id() {
        GenericTemplateId::Zero => GENERIC_TEMPLATE_0,
        GenericTemplateId::One => GENERIC_TEMPLATE_1,
        GenericTemplateId::Two => GENERIC_TEMPLATE_2,
        GenericTemplateId::Three => GENERIC_TEMPLATE_3,
    };
    let mut value = 0_usize;
    for &(dx, dy, bit) in fixed {
        if sample(bitmap, x, y, dx, dy) {
            value |= 1_usize << bit;
        }
    }
    for (index, &bit) in adaptive_bits.iter().enumerate() {
        let (dx, dy) = template
            .pixel(index)
            .ok_or(Jbig2Error::InvalidResource("generic adaptive pixel"))?;
        if sample(bitmap, x, y, dx, dy) {
            value |= 1_usize << bit;
        }
    }
    Ok(value)
}

/// Read a neighboring pixel, treating anything outside the bitmap as white.
fn sample(bitmap: BitmapRef<'_>, x: u32, y: u32, dx: i8, dy: i8) -> bool {
    let sx = i64::from(x).saturating_add(i64::from(dx));
    let sy = i64::from(y).saturating_add(i64::from(dy));
    u32::try_from(sx)
        .ok()
        .zip(u32::try_from(sy).ok())
        .and_then(|(px, py)| bitmap.pixel(px, py))
        .unwrap_or(false)
}

/// Borrowed arithmetic refinement syntax (T.88 section 6.3).
#[derive(Debug)]
pub struct RefinementRegion<'a> {
    /// Output region geometry and page-composition behavior.
    pub info: RegionInfo,
    /// Refinement template and resolved adaptive pixels.
    pub template: RefinementTemplate,
    /// Enable typical prediction for refinement decoding.
    pub typical_prediction: bool,
    /// Bounded arithmetic-coded bitmap bytes.
    pub data: &'a [u8],
}

impl<'a> RefinementRegion<'a> {
    /// Parse a refinement-region segment, validating its template and flags.
    pub fn parse(segment: Segment<'a>, _budget: &mut DecodeBudget) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
        let info = RegionInfo::parse(&mut cursor, segment.header().segment_type)?;
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        if flags & 0xfc != 0 {
            return Err(Jbig2Error::ReservedField {
                location: flags_location,
                field: "refinement flags",
            });
        }
        let id = RefinementTemplateId::try_from(flags & 1)?;
        let template = RefinementTemplate::parse(id, &mut cursor)?;
        let data = cursor.remaining();
        Ok(Parsed {
            value: Self {
                info,
                template,
                typical_prediction: flags & 2 != 0,
                data,
            },
            remaining: data,
            consumed: cursor.position(),
        })
    }
}

/// One refinement bitmap decoded against a borrowed reference.
///
/// A refinement region segment refines the co-located page or intermediate
/// region, so its displacements are zero (T.88 section 7.4.7.2). Text region
/// instance refinement supplies nonzero displacements (section 6.4.11).
pub(crate) struct RefinementProcedure {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) template: RefinementTemplate,
    pub(crate) typical_prediction: bool,
    pub(crate) reference_dx: i32,
    pub(crate) reference_dy: i32,
}

impl RefinementProcedure {
    /// Decode this bitmap, advancing the supplied decoder and contexts.
    pub(crate) fn decode(
        &self,
        reference: BitmapRef<'_>,
        decoder: &mut ArithmeticDecoder<'_>,
        states: &mut ArithmeticContexts,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let mut bitmap = Bitmap::new(self.width, self.height, false, budget)?;
        budget.charge(
            LimitKind::Work,
            u64::from(self.width)
                .checked_mul(u64::from(self.height))
                .ok_or(Jbig2Error::Overflow("refinement work"))?,
        )?;
        let mut ltp = false;
        for y in 0..self.height {
            if self.typical_prediction {
                let index = self.template.id().typical_prediction_context();
                ltp ^= decoder.decode_bit(states, index, budget)?;
            }
            for x in 0..self.width {
                let implicit = if ltp {
                    self.uniform_reference_pixel(reference, x, y)
                } else {
                    None
                };
                let pixel = match implicit {
                    Some(pixel) => pixel,
                    None => {
                        let label = self.context_label(bitmap.as_ref(), reference, x, y)?;
                        decoder.decode_bit(states, label, budget)?
                    }
                };
                bitmap.set_pixel(x, y, pixel)?;
            }
        }
        Ok(bitmap)
    }

    /// Read a reference pixel at a displaced offset, treating anything outside
    /// the reference bitmap as white.
    ///
    /// T.88 section 6.3.5.3 places the reference pixel at
    /// `GRREFERENCE[y - GRREFERENCEDY + dy][x - GRREFERENCEDX + dx]`, so the
    /// displacement is subtracted. The legacy `pdf-jbig2` decoder adds it,
    /// which only agrees for the zero displacements a refinement region
    /// segment uses.
    fn reference_sample(&self, reference: BitmapRef<'_>, x: u32, y: u32, dx: i8, dy: i8) -> bool {
        let sx = i64::from(x)
            .saturating_sub(i64::from(self.reference_dx))
            .saturating_add(i64::from(dx));
        let sy = i64::from(y)
            .saturating_sub(i64::from(self.reference_dy))
            .saturating_add(i64::from(dy));
        u32::try_from(sx)
            .ok()
            .zip(u32::try_from(sy).ok())
            .and_then(|(px, py)| reference.pixel(px, py))
            .unwrap_or(false)
    }

    /// Return the pixel value when the whole reference neighborhood agrees,
    /// which typical prediction decodes implicitly (T.88 section 6.3.5.6).
    fn uniform_reference_pixel(&self, reference: BitmapRef<'_>, x: u32, y: u32) -> Option<bool> {
        let center = self.reference_sample(reference, x, y, 0, 0);
        for dy in -1..=1_i8 {
            for dx in -1..=1_i8 {
                if self.reference_sample(reference, x, y, dx, dy) != center {
                    return None;
                }
            }
        }
        Some(center)
    }

    /// Build the arithmetic context value for one refined pixel.
    fn context_label(
        &self,
        current: BitmapRef<'_>,
        reference: BitmapRef<'_>,
        x: u32,
        y: u32,
    ) -> Result<usize> {
        let (coding, refs) = match self.template.id() {
            RefinementTemplateId::Zero => (REFINEMENT_CODING_0, REFINEMENT_REFERENCE_0),
            RefinementTemplateId::One => (REFINEMENT_CODING_1, REFINEMENT_REFERENCE_1),
        };
        let mut label = 0_usize;
        for &(dx, dy) in coding {
            label = (label << 1) | usize::from(sample(current, x, y, dx, dy));
        }
        if let Some((dx, dy)) = self.template.pixel(0) {
            label = (label << 1) | usize::from(sample(current, x, y, dx, dy));
        }
        for &(dx, dy) in refs {
            label = (label << 1) | usize::from(self.reference_sample(reference, x, y, dx, dy));
        }
        if let Some((dx, dy)) = self.template.pixel(1) {
            label = (label << 1) | usize::from(self.reference_sample(reference, x, y, dx, dy));
        }
        Ok(label)
    }
}

/// Coding and reference context pixels for each refinement template
/// (T.88 section 6.3.5.3, figures 12 and 13).
const REFINEMENT_CODING_0: &[(i8, i8)] = &[(0, -1), (1, -1), (-1, 0)];
const REFINEMENT_REFERENCE_0: &[(i8, i8)] = &[
    (0, -1),
    (1, -1),
    (-1, 0),
    (0, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];
const REFINEMENT_CODING_1: &[(i8, i8)] = &[(-1, -1), (0, -1), (1, -1), (-1, 0)];
const REFINEMENT_REFERENCE_1: &[(i8, i8)] = &[(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1), (1, 1)];

/// The bitmap a refinement region segment refines (T.88 section 7.4.7.5).
#[derive(Debug, Clone, Copy)]
pub enum RefinementReference<'a> {
    /// A referred-to intermediate region, refined as a whole.
    Region(BitmapRef<'a>),
    /// The page being composed, refined where the region is placed on it.
    Page(BitmapRef<'a>),
}

/// Concrete refinement reconstruction against a supplied immutable reference.
#[derive(Debug, Default)]
pub struct RefinementRegionDecoder;

impl RefinementRegionDecoder {
    /// Produce owned pixels; the reference may be an intermediate region or page.
    pub fn decode(
        &mut self,
        region: &RefinementRegion<'_>,
        reference: RefinementReference<'_>,
        context: &mut DecodeContext<'_>,
    ) -> Result<Bitmap> {
        // Reference pixels are read at `x - dx`, so a page reference is
        // displaced by the negated region position.
        let (reference, reference_dx, reference_dy) = match reference {
            RefinementReference::Region(bitmap) => (bitmap, 0, 0),
            RefinementReference::Page(bitmap) => (
                bitmap,
                negated_offset(region.info.placement.x)?,
                negated_offset(region.info.placement.y)?,
            ),
        };
        let procedure = RefinementProcedure {
            width: region.info.width,
            height: region.info.height,
            template: region.template,
            typical_prediction: region.typical_prediction,
            reference_dx,
            reference_dy,
        };
        let mut states = ArithmeticContexts::new(8192, context.budget)?;
        let mut decoder = ArithmeticDecoder::new(region.data, context.location)?;
        let bitmap = procedure.decode(reference, &mut decoder, &mut states, context.budget)?;
        Ok(bitmap)
    }
}

/// Negate a region coordinate into a refinement displacement.
fn negated_offset(position: i64) -> Result<i32> {
    position
        .checked_neg()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or(Jbig2Error::Overflow("refinement reference offset"))
}

/// Symbol corner used as the text placement reference.
#[derive(Debug, Clone, Copy)]
pub enum ReferenceCorner {
    /// Top-left corner.
    TopLeft,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-right corner.
    BottomRight,
}

/// Text-region Huffman table choices for placement and refinement integers.
#[derive(Debug, Clone, Copy)]
pub struct TextHuffmanTables {
    /// First-symbol S displacement.
    pub first_s: TableSelection,
    /// Subsequent-symbol S displacement.
    pub delta_s: TableSelection,
    /// Strip T displacement.
    pub delta_t: TableSelection,
    /// Refinement width delta.
    pub refinement_width: TableSelection,
    /// Refinement height delta.
    pub refinement_height: TableSelection,
    /// Refinement horizontal displacement.
    pub refinement_x: TableSelection,
    /// Refinement vertical displacement.
    pub refinement_y: TableSelection,
    /// Refinement coded-bitmap size.
    pub refinement_size: TableSelection,
}

/// Integer coding selection for a text region.
#[derive(Debug, Clone, Copy)]
pub enum TextCoding {
    /// Arithmetic integers and symbol identifiers.
    Arithmetic,
    /// Huffman integers and the text payload's symbol-ID code table.
    Huffman(TextHuffmanTables),
}

/// Borrowed text placement and optional symbol-refinement syntax.
#[derive(Debug)]
pub struct TextRegion<'a> {
    /// Output geometry and external composition behavior.
    pub info: RegionInfo,
    /// Integer entropy selection.
    pub coding: TextCoding,
    /// Number of symbol instances to place.
    pub instances: u32,
    /// Base-two strip-height exponent validated from the header.
    pub log_strip_size: u8,
    /// Corner used for symbol positioning.
    pub reference_corner: ReferenceCorner,
    /// Whether S and T axes are transposed.
    pub transposed: bool,
    /// Initial value of pixels inside the text-region bitmap.
    pub default_black: bool,
    /// Operator combining symbols within the text-region bitmap.
    pub symbol_operator: CombinationOperator,
    /// Signed S-offset applied between symbol instances.
    pub delta_s_offset: i8,
    /// Optional template enabling symbol refinement.
    pub refinement: Option<RefinementTemplate>,
    /// Bounded symbol-ID, placement, and refinement coded data.
    pub data: &'a [u8],
}

impl<'a> TextRegion<'a> {
    /// Parse text syntax, preserving borrowed adaptive pixels and coded bytes.
    ///
    /// Custom Huffman selections resolve against `tables`, the segment's
    /// referred-to Code Table segments in header order.
    pub fn parse(
        segment: Segment<'a>,
        tables: &[SegmentNumber],
        _budget: &mut DecodeBudget,
    ) -> Result<Parsed<'a, Self>> {
        crate::text_region::parse_descriptor(segment, tables)
    }
}

/// Concrete text reconstruction using referred-to symbol dictionaries.
#[derive(Debug, Default)]
pub struct TextRegionDecoder;

impl TextRegionDecoder {
    /// Resolve symbols and tables, decode placements, and composite the region.
    pub fn decode(
        &mut self,
        region: &TextRegion<'_>,
        context: &mut DecodeContext<'_>,
    ) -> Result<Bitmap> {
        crate::text_region::decode(region, context)
    }
}

/// Halftone grid geometry; origins and steps retain their encoded fixed precision.
#[derive(Debug, Clone, Copy)]
pub struct HalftoneGrid {
    /// Number of pattern columns.
    pub width: u32,
    /// Number of pattern rows.
    pub height: u32,
    /// Signed horizontal origin in units of 1/256 pixel.
    pub origin_x: i32,
    /// Signed vertical origin in units of 1/256 pixel.
    pub origin_y: i32,
    /// Encoded horizontal grid vector component, in units of 1/256 pixel.
    pub step_x: u16,
    /// Encoded vertical grid vector component, in units of 1/256 pixel.
    pub step_y: u16,
}

impl HalftoneGrid {
    /// Visit every grid cell in decoding order, row by row.
    fn cells(self) -> impl Iterator<Item = (u32, u32)> {
        (0..self.height).flat_map(move |row| (0..self.width).map(move |column| (row, column)))
    }
}

/// Borrowed halftone-region syntax and grayscale bitplane bytes.
#[derive(Debug)]
pub struct HalftoneRegion<'a> {
    /// Region geometry and external composition behavior.
    pub info: RegionInfo,
    /// Pattern placement grid.
    pub grid: HalftoneGrid,
    /// MMR selection for grayscale bitplanes.
    pub mmr: bool,
    /// Arithmetic template identifier when MMR is disabled.
    pub template: GenericTemplateId,
    /// Enable skip bitmap generation for out-of-bounds patterns.
    pub enable_skip: bool,
    /// Initial value of the halftone-region pixels.
    pub default_black: bool,
    /// Operator combining patterns within the region.
    pub pattern_operator: CombinationOperator,
    /// Bounded encoded grayscale bitplanes.
    pub data: &'a [u8],
}

impl<'a> HalftoneRegion<'a> {
    /// Parse halftone syntax and validate grid, coding, and composition flags.
    pub fn parse(segment: Segment<'a>, _budget: &mut DecodeBudget) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
        let info = RegionInfo::parse(&mut cursor, segment.header().segment_type)?;
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        let pattern_operator = CombinationOperator::try_from((flags >> 4) & 7).map_err(|_| {
            Jbig2Error::ReservedField {
                location: flags_location,
                field: "halftone combination operator",
            }
        })?;
        let grid = HalftoneGrid {
            width: cursor.u32()?,
            height: cursor.u32()?,
            origin_x: cursor.i32()?,
            origin_y: cursor.i32()?,
            step_x: cursor.u16()?,
            step_y: cursor.u16()?,
        };
        let data = cursor.remaining();
        Ok(Parsed {
            value: Self {
                info,
                grid,
                mmr: flags & 1 != 0,
                template: GenericTemplateId::try_from((flags >> 1) & 3)?,
                enable_skip: flags & 8 != 0,
                default_black: flags & 0x80 != 0,
                pattern_operator,
                data,
            },
            remaining: data,
            consumed: cursor.position(),
        })
    }
}

/// Concrete grayscale-bitplane and pattern-placement reconstruction.
#[derive(Debug, Default)]
pub struct HalftoneRegionDecoder;

impl HalftoneRegionDecoder {
    /// Resolve the pattern dictionary, reconstruct indices, and compose patterns.
    pub fn decode(
        &mut self,
        region: &HalftoneRegion<'_>,
        context: &mut DecodeContext<'_>,
    ) -> Result<Bitmap> {
        let patterns = resolve_patterns(context)?;
        let bits = bits_per_pattern(patterns.len())?;
        let skip = build_skip_mask(region, patterns.pattern(0)?, context.budget)?;
        let planes = decode_gray_planes(region, bits, skip.as_ref(), context)?;
        let mut bitmap = Bitmap::new(
            region.info.width,
            region.info.height,
            region.default_black,
            context.budget,
        )?;
        compose_patterns(region, patterns, &planes, &mut bitmap, context)?;
        Ok(bitmap)
    }
}

/// Borrow the pattern dictionary this halftone region refers to.
fn resolve_patterns<'a>(context: &DecodeContext<'a>) -> Result<&'a PatternDictionary> {
    let resources = context.resources;
    for id in context.header.references.iter() {
        if let Ok(DecodedResourceRef::Patterns(patterns)) = resources.resolve(id) {
            return Ok(patterns);
        }
    }
    Err(Jbig2Error::InvalidCodingData {
        location: context.location,
        reason: "halftone pattern reference",
    })
}

/// Number of grayscale bitplanes required to address every pattern index.
fn bits_per_pattern(count: usize) -> Result<usize> {
    let mut bits = 0_usize;
    let mut capacity = 1_usize;
    while capacity < count {
        bits = bits
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("halftone bits per pattern"))?;
        capacity = capacity
            .checked_mul(2)
            .ok_or(Jbig2Error::Overflow("halftone pattern count"))?;
    }
    Ok(bits)
}

/// Mark grid cells whose pattern falls entirely outside the region, so their
/// grayscale bits are never coded (T.88 section 6.6.5.1).
fn build_skip_mask(
    region: &HalftoneRegion<'_>,
    pattern: BitmapRef<'_>,
    budget: &mut DecodeBudget,
) -> Result<Option<Bitmap>> {
    if !region.enable_skip || region.mmr {
        return Ok(None);
    }
    let mut mask = Bitmap::new(region.grid.width, region.grid.height, false, budget)?;
    for (row, column) in region.grid.cells() {
        let (x, y) = halftone_position(region.grid, row, column)?;
        let outside = x.saturating_add(i64::from(pattern.width())) <= 0
            || y.saturating_add(i64::from(pattern.height())) <= 0
            || x >= i64::from(region.info.width)
            || y >= i64::from(region.info.height);
        mask.set_pixel(column, row, outside)?;
    }
    Ok(Some(mask))
}

/// Decode the grayscale bitplanes, most significant first, undoing the Gray
/// code as each plane arrives (T.88 Annex C).
fn decode_gray_planes(
    region: &HalftoneRegion<'_>,
    bits: usize,
    skip: Option<&Bitmap>,
    context: &mut DecodeContext<'_>,
) -> Result<Vec<Bitmap>> {
    context.budget.charge(
        LimitKind::AllocationBytes,
        u64::try_from(
            bits.checked_mul(size_of::<Bitmap>())
                .ok_or(Jbig2Error::Overflow("halftone planes"))?,
        )
        .map_err(|_| Jbig2Error::Overflow("halftone planes"))?,
    )?;
    let mut planes = Vec::new();
    planes.try_reserve_exact(bits)?;
    if region.mmr {
        let mut position = 0_usize;
        for _ in 0..bits {
            let (mut plane, next) = crate::mmr::decode_from_bit(
                region.grid.width,
                region.grid.height,
                region.data,
                position,
                context.location,
                context.budget,
            )?;
            xor_plane(&mut plane, planes.last())?;
            planes.push(plane);
            position = next;
        }
        return Ok(planes);
    }
    if bits == 0 {
        return Ok(planes);
    }
    let template = GenericTemplate::with_defaults(region.template);
    let plane = GenericPlane {
        width: region.grid.width,
        height: region.grid.height,
        template,
        typical_prediction: false,
        skip: skip.map(Bitmap::as_ref),
    };
    let mut states = ArithmeticContexts::new(region.template.context_count(), context.budget)?;
    let mut decoder = ArithmeticDecoder::new(region.data, context.location)?;
    for _ in 0..bits {
        let mut decoded = plane.decode(&mut decoder, &mut states, context.budget)?;
        xor_plane(&mut decoded, planes.last())?;
        planes.push(decoded);
    }
    Ok(planes)
}

/// Rebuild each cell's pattern index from the bitplanes and compose its pattern.
fn compose_patterns(
    region: &HalftoneRegion<'_>,
    patterns: &PatternDictionary,
    planes: &[Bitmap],
    bitmap: &mut Bitmap,
    context: &mut DecodeContext<'_>,
) -> Result<()> {
    for (row, column) in region.grid.cells() {
        let index = gray_index(planes, row, column)?;
        let pattern = patterns.pattern(index)?;
        let (x, y) = halftone_position(region.grid, row, column)?;
        context.compositor.compose(
            bitmap,
            pattern,
            Placement {
                x,
                y,
                operator: region.pattern_operator,
            },
            context.budget,
        )?;
    }
    Ok(())
}

/// Read one cell's pattern index out of the decoded bitplanes.
fn gray_index(planes: &[Bitmap], row: u32, column: u32) -> Result<u32> {
    let mut index = 0_usize;
    for plane in planes {
        index = index
            .checked_mul(2)
            .ok_or(Jbig2Error::Overflow("halftone index"))?
            .checked_add(usize::from(
                plane.as_ref().pixel(column, row).unwrap_or(false),
            ))
            .ok_or(Jbig2Error::Overflow("halftone index"))?;
    }
    u32::try_from(index).map_err(|_| Jbig2Error::Overflow("halftone index"))
}

/// Compute a grid cell's pixel origin from the signed 1/256-pixel grid vectors.
fn halftone_position(grid: HalftoneGrid, row: u32, column: u32) -> Result<(i64, i64)> {
    let row = i64::from(row);
    let column = i64::from(column);
    let x = i64::from(grid.origin_x)
        .checked_add(
            row.checked_mul(i64::from(grid.step_y))
                .ok_or(Jbig2Error::Overflow("halftone X"))?,
        )
        .and_then(|value| value.checked_add(column.saturating_mul(i64::from(grid.step_x))))
        .ok_or(Jbig2Error::Overflow("halftone X"))?;
    let y = i64::from(grid.origin_y)
        .checked_add(
            row.checked_mul(i64::from(grid.step_x))
                .ok_or(Jbig2Error::Overflow("halftone Y"))?,
        )
        .and_then(|value| value.checked_sub(column.saturating_mul(i64::from(grid.step_y))))
        .ok_or(Jbig2Error::Overflow("halftone Y"))?;
    Ok((x.div_euclid(256), y.div_euclid(256)))
}

/// Undo one Gray-code step by combining a plane with its more significant
/// neighbor (T.88 section C.5).
fn xor_plane(plane: &mut Bitmap, previous: Option<&Bitmap>) -> Result<()> {
    if let Some(previous) = previous {
        let view = previous.as_ref();
        for y in 0..view.height() {
            for x in 0..view.width() {
                let pixel =
                    plane.as_ref().pixel(x, y).unwrap_or(false) ^ view.pixel(x, y).unwrap_or(false);
                plane.set_pixel(x, y, pixel)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DecodeContext, GenericRegion, GenericRegionDecoder, HalftoneGrid, RefinementReference,
        RefinementRegion, RefinementRegionDecoder, halftone_position, xor_plane,
    };
    use crate::{
        Jbig2Error, Result,
        bitmap::{BitmapRef, DefaultCompositor},
        dictionary::{DecodedResourceRef, ResourceResolver},
        document::{DecodeBudget, DecodeLimits},
        segment::SegmentNumber,
        stream::{Input, SegmentStream},
    };

    struct NoResources;
    impl ResourceResolver for NoResources {
        fn resolve(&self, number: SegmentNumber) -> Result<DecodedResourceRef<'_>> {
            Err(Jbig2Error::MissingReference(number))
        }
    }

    fn budget() -> DecodeBudget {
        DecodeBudget::new(DecodeLimits {
            input_bytes: 1000,
            segments: 100,
            references: 100,
            pixels: 100,
            allocation_bytes: 100_000,
            symbols: 100,
            work: 1000,
        })
    }

    #[test]
    fn halftone_fixed_point_positions_and_gray_plane_xor() -> Result<()> {
        let grid = HalftoneGrid {
            width: 2,
            height: 2,
            origin_x: -128,
            origin_y: 256,
            step_x: 256,
            step_y: 128,
        };
        assert_eq!(halftone_position(grid, 0, 0)?, (-1, 1));
        assert_eq!(halftone_position(grid, 1, 1)?, (1, 1));
        let mut previous = crate::bitmap::Bitmap::new(1, 1, true, &mut budget())?;
        let mut current = crate::bitmap::Bitmap::new(1, 1, false, &mut budget())?;
        xor_plane(&mut current, Some(&previous))?;
        assert_eq!(current.as_ref().pixel(0, 0), Some(true));
        previous.set_pixel(0, 0, false)?;
        xor_plane(&mut current, Some(&previous))?;
        assert_eq!(current.as_ref().pixel(0, 0), Some(true));
        Ok(())
    }

    #[test]
    fn parses_and_decodes_one_pixel_arithmetic_generic_region() -> Result<()> {
        let payload = [
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0xff, 0xfd, 0xff, 2, 0xfe,
            0xfe, 0xfe, 0,
        ];
        let mut input = vec![0, 0, 0, 1, 38, 0, 1, 0, 0, 0, 27];
        input.extend_from_slice(&payload);
        let mut budget = budget();
        let mut stream = SegmentStream::new(Input::Embedded(&input), &mut budget)?;
        let segment = stream
            .next_segment(&mut budget)?
            .ok_or(Jbig2Error::InvalidPageSequence("segment"))?;
        let region = GenericRegion::parse(segment, &mut budget)?.value;
        assert_eq!(region.info.width, 1);
        let mut compositor = DefaultCompositor;
        let mut context = DecodeContext {
            header: segment.header(),
            location: segment.payload_location(),
            resources: &NoResources,
            budget: &mut budget,
            compositor: &mut compositor,
        };
        let bitmap = GenericRegionDecoder.decode(&region, None, &mut context)?;
        assert_eq!(bitmap.as_ref().bytes(), &[0]);
        let skip = BitmapRef::new(2, 1, 1, &[0])?;
        assert!(
            GenericRegionDecoder
                .decode(&region, Some(skip), &mut context)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn refinement_decodes_against_borrowed_reference() -> Result<()> {
        let payload = [
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0,
        ];
        let mut input = vec![0, 0, 0, 1, 42, 0, 1, 0, 0, 0, 23];
        input.extend_from_slice(&payload);
        let mut budget = budget();
        let mut stream = SegmentStream::new(Input::Embedded(&input), &mut budget)?;
        let segment = stream
            .next_segment(&mut budget)?
            .ok_or(Jbig2Error::InvalidPageSequence("segment"))?;
        let region = RefinementRegion::parse(segment, &mut budget)?.value;
        let reference = BitmapRef::new(1, 1, 1, &[0])?;
        let mut compositor = DefaultCompositor;
        let mut context = DecodeContext {
            header: segment.header(),
            location: segment.payload_location(),
            resources: &NoResources,
            budget: &mut budget,
            compositor: &mut compositor,
        };
        let bitmap = RefinementRegionDecoder.decode(
            &region,
            RefinementReference::Region(reference),
            &mut context,
        )?;
        assert_eq!(bitmap.as_ref().bytes(), &[0]);
        Ok(())
    }
}
