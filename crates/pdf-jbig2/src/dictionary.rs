use crate::{
    Jbig2Error, Result,
    arithmetic::{ArithmeticContexts, ArithmeticDecoder},
    arithmetic_source::ArithmeticSource,
    bitmap::{Bitmap, BitmapRef},
    cursor::ByteCursor,
    document::DecodeBudget,
    error::LimitKind,
    huffman::{HuffmanTable, ResolvedTable, StandardTable, TableSelection, resolve_table},
    huffman_source::{DictionaryTables, HuffmanSymbolSource},
    region::{DecodeContext, GenericPlane},
    segment::{Parsed, Segment, SegmentNumber},
    symbols::SymbolSource,
    template::{GenericTemplate, GenericTemplateId, RefinementTemplate},
};

/// Entropy selection for symbol dictionary integer fields (T.88 section 6.5).
#[derive(Debug, Clone, Copy)]
pub enum SymbolCoding {
    /// Arithmetic integer coding with context retention/reuse.
    Arithmetic,
    /// Huffman integer coding; refinement bitmaps still use arithmetic coding.
    Huffman {
        /// Delta-height table.
        height: TableSelection,
        /// Delta-width table.
        width: TableSelection,
        /// Collective-bitmap size table.
        bitmap_size: TableSelection,
        /// Aggregation symbol-count table.
        aggregate_count: TableSelection,
    },
}

/// Borrowed symbol dictionary syntax and bounded coded bytes.
///
/// Adaptive template bytes remain in the caller's segment buffer. Referred
/// dictionaries and custom tables are resolved separately through the context.
#[derive(Debug)]
pub struct SymbolDictionaryDescriptor<'a> {
    /// Integer coding procedure and table selections.
    pub coding: SymbolCoding,
    /// Whether symbols may be refined or aggregated from other symbols.
    pub refinement_aggregation: bool,
    /// Generic bitmap template, absent when Huffman coding supplies no
    /// directly coded symbol bitmaps.
    pub generic_template: Option<GenericTemplate>,
    /// Refinement template when refinement/aggregation is enabled.
    pub refinement_template: Option<RefinementTemplate>,
    /// Reuse contexts retained by a referred-to symbol dictionary.
    pub reuse_contexts: bool,
    /// Retain resulting contexts for future dictionaries.
    pub retain_contexts: bool,
    /// Number of symbols introduced by this segment.
    pub new_symbols: u32,
    /// Number of symbols exported to subsequent segments.
    pub exported_symbols: u32,
    /// Remaining bounded entropy-coded bytes.
    pub data: &'a [u8],
}

impl<'a> SymbolDictionaryDescriptor<'a> {
    /// Parse dictionary syntax; report malformed flags, templates, or truncation.
    ///
    /// Custom Huffman selections resolve against `tables`, the segment's
    /// referred-to Code Table segments in header order.
    pub fn parse(
        segment: Segment<'a>,
        tables: &[SegmentNumber],
        _budget: &mut DecodeBudget,
    ) -> Result<Parsed<'a, Self>> {
        crate::symbols::parse_descriptor(segment, tables)
    }
}

/// Owned exported symbol bitmaps and optionally retained probability contexts.
/// Resource references borrow this owner, never the original compressed input.
#[derive(Debug)]
pub struct SymbolDictionary {
    symbols: Vec<Bitmap>,
    contexts: Option<RetainedContexts>,
}

/// Generic and refinement statistics a symbol dictionary leaves for a later
/// one to continue from (T.88 section 7.4.2.2).
#[derive(Debug, Default)]
pub(crate) struct RetainedContexts {
    /// Generic region decoding statistics, when the dictionary used them.
    pub(crate) generic: Option<ArithmeticContexts>,
    /// Refinement region decoding statistics, when the dictionary used them.
    pub(crate) refinement: Option<ArithmeticContexts>,
}

impl RetainedContexts {
    /// Copy these statistics for a dictionary that continues from them.
    fn duplicate(&self, budget: &mut DecodeBudget) -> Result<Self> {
        Ok(Self {
            generic: self
                .generic
                .as_ref()
                .map(|bank| bank.duplicate(budget))
                .transpose()?,
            refinement: self
                .refinement
                .as_ref()
                .map(|bank| bank.duplicate(budget))
                .transpose()?,
        })
    }
}

impl SymbolDictionary {
    /// Borrow an exported symbol or report an invalid symbol identifier.
    pub fn symbol(&self, index: u32) -> Result<BitmapRef<'_>> {
        let index = usize::try_from(index).map_err(|_| Jbig2Error::Overflow("symbol index"))?;
        self.symbols
            .get(index)
            .map(Bitmap::as_ref)
            .ok_or(Jbig2Error::InvalidResource("symbol index"))
    }

    /// Borrow every exported symbol in identifier order.
    pub fn views(&self) -> impl Iterator<Item = BitmapRef<'_>> {
        self.symbols.iter().map(Bitmap::as_ref)
    }

    /// Return the number of symbols visible to referring segments.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Report whether this dictionary exports no symbols.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Concrete symbol reconstruction procedure, separate from resource ownership.
#[derive(Debug, Default)]
pub struct SymbolDictionaryDecoder;

impl SymbolDictionaryDecoder {
    /// Import referenced symbols, reconstruct new ones, and apply export runs.
    /// Reports coding, reference, context-reuse, allocation, or budget failures.
    pub fn decode(
        &mut self,
        descriptor: &SymbolDictionaryDescriptor<'_>,
        context: &mut DecodeContext<'_>,
    ) -> Result<SymbolDictionary> {
        let inherited = if descriptor.reuse_contexts {
            inherited_contexts(context)?
        } else {
            RetainedContexts::default()
        };
        let imported = imported_symbols(context)?;
        let total = imported
            .len()
            .checked_add(
                usize::try_from(descriptor.new_symbols)
                    .map_err(|_| Jbig2Error::Overflow("new symbol count"))?,
            )
            .ok_or(Jbig2Error::Overflow("symbol count"))?;
        // Building the Huffman tables borrows the resolver, so it happens
        // before the source takes over the budget for the decoding run.
        let tables = huffman_tables(descriptor.coding, context)?;
        let mut source: Box<dyn SymbolSource> = match tables.as_ref() {
            None => Box::new(ArithmeticSource::for_dictionary(
                descriptor,
                total,
                inherited,
                context.location,
                context.budget,
            )?),
            Some(tables) => Box::new(HuffmanSymbolSource::new(
                descriptor.data,
                context.location,
                tables,
                descriptor.refinement_template,
                inherited.refinement,
                crate::text::symbol_code_length(total)?.max(1),
                context.budget,
            )?),
        };
        let decoded = crate::symbols::decode_new_symbols(
            source.as_mut(),
            &imported,
            descriptor.new_symbols,
            context.budget,
        )?;
        let symbols = crate::symbols::apply_export_runs(
            source.as_mut(),
            &imported,
            decoded,
            descriptor.exported_symbols,
            context.budget,
        )?;
        let contexts = descriptor.retain_contexts.then(|| source.into_contexts());
        Ok(SymbolDictionary { symbols, contexts })
    }
}

/// Copy the statistics retained by the last referred-to symbol dictionary,
/// which a dictionary that uses retained contexts continues from
/// (T.88 section 7.4.2.2).
fn inherited_contexts(context: &mut DecodeContext<'_>) -> Result<RetainedContexts> {
    let resources = context.resources;
    let last = context
        .header
        .references
        .iter()
        .filter_map(|id| match resources.resolve(id) {
            Ok(DecodedResourceRef::Symbols(dictionary)) => Some(dictionary),
            _ => None,
        })
        .last()
        .ok_or(Jbig2Error::InvalidResource(
            "retained contexts without a referred-to dictionary",
        ))?;
    last.contexts
        .as_ref()
        .ok_or(Jbig2Error::InvalidResource(
            "referred-to dictionary retained no contexts",
        ))?
        .duplicate(context.budget)
}

/// Pattern dictionary syntax for halftone reconstruction (T.88 section 6.7).
#[derive(Debug)]
pub struct PatternDictionaryDescriptor<'a> {
    /// Whether the collective bitmap uses MMR instead of arithmetic coding.
    pub mmr: bool,
    /// Generic arithmetic template identifier when MMR is disabled.
    pub template: GenericTemplateId,
    /// Width of each pattern in pixels.
    pub width: u8,
    /// Height of each pattern in pixels.
    pub height: u8,
    /// Highest pattern index; pattern count is this value plus one, checked.
    pub gray_max: u32,
    /// Bounded collective-bitmap coded bytes.
    pub data: &'a [u8],
}

impl<'a> PatternDictionaryDescriptor<'a> {
    /// Parse and validate the pattern header without copying its coded payload.
    pub fn parse(segment: Segment<'a>, _budget: &mut DecodeBudget) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        if flags & 0xf8 != 0 {
            return Err(Jbig2Error::ReservedField {
                location: flags_location,
                field: "pattern dictionary flags",
            });
        }
        let width = cursor.u8()?;
        let height = cursor.u8()?;
        if width == 0 || height == 0 {
            return Err(Jbig2Error::InvalidBitmap("zero-sized pattern"));
        }
        let gray_max = cursor.u32()?;
        let data = cursor.remaining();
        Ok(Parsed {
            value: Self {
                mmr: flags & 1 != 0,
                template: GenericTemplateId::try_from((flags >> 1) & 3)?,
                width,
                height,
                gray_max,
                data,
            },
            remaining: data,
            consumed: cursor.position(),
        })
    }
}

/// Owned halftone patterns independent of encoded-buffer lifetime.
#[derive(Debug)]
pub struct PatternDictionary {
    patterns: Vec<Bitmap>,
}

impl PatternDictionary {
    /// Borrow a pattern selected by a decoded grayscale index.
    pub fn pattern(&self, index: u32) -> Result<BitmapRef<'_>> {
        let index = usize::try_from(index).map_err(|_| Jbig2Error::Overflow("pattern index"))?;
        self.patterns
            .get(index)
            .map(Bitmap::as_ref)
            .ok_or(Jbig2Error::InvalidResource("pattern index"))
    }

    /// Return the number of available pattern indices.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Report whether the dictionary contains no patterns.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

/// Concrete collective-bitmap decoding and pattern extraction procedure.
#[derive(Debug, Default)]
pub struct PatternDictionaryDecoder;

impl PatternDictionaryDecoder {
    /// Decode patterns with checked collective dimensions and shared limits.
    pub fn decode(
        &mut self,
        descriptor: &PatternDictionaryDescriptor<'_>,
        context: &mut DecodeContext<'_>,
    ) -> Result<PatternDictionary> {
        let count = descriptor
            .gray_max
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("pattern count"))?;
        let collective = decode_collective_bitmap(descriptor, count, context)?;
        split_collective_bitmap(descriptor, count, collective.as_ref(), context.budget)
    }
}

/// Decode the single wide bitmap holding every pattern side by side
/// (T.88 section 6.7.5).
fn decode_collective_bitmap(
    descriptor: &PatternDictionaryDescriptor<'_>,
    count: u32,
    context: &mut DecodeContext<'_>,
) -> Result<Bitmap> {
    let width = count
        .checked_mul(u32::from(descriptor.width))
        .ok_or(Jbig2Error::Overflow("collective pattern width"))?;
    let height = u32::from(descriptor.height);
    if descriptor.mmr {
        let (bitmap, _consumed) = crate::mmr::decode(
            width,
            height,
            descriptor.data,
            context.location,
            context.budget,
        )?;
        return Ok(bitmap);
    }
    let template = GenericTemplate::pattern_collective(descriptor.template, descriptor.width)?;
    let plane = GenericPlane {
        width,
        height,
        template,
        typical_prediction: false,
        skip: None,
    };
    let mut states = ArithmeticContexts::new(descriptor.template.context_count(), context.budget)?;
    let mut decoder = ArithmeticDecoder::new(descriptor.data, context.location)?;
    let bitmap = plane.decode(&mut decoder, &mut states, context.budget)?;
    Ok(bitmap)
}

/// Cut the collective bitmap into one owned bitmap per pattern index.
fn split_collective_bitmap(
    descriptor: &PatternDictionaryDescriptor<'_>,
    count: u32,
    collective: BitmapRef<'_>,
    budget: &mut DecodeBudget,
) -> Result<PatternDictionary> {
    let entries = usize::try_from(count).map_err(|_| Jbig2Error::Overflow("pattern count"))?;
    budget.charge(
        LimitKind::AllocationBytes,
        u64::try_from(
            entries
                .checked_mul(size_of::<Bitmap>())
                .ok_or(Jbig2Error::Overflow("pattern storage"))?,
        )
        .map_err(|_| Jbig2Error::Overflow("pattern storage"))?,
    )?;
    let mut patterns = Vec::new();
    patterns.try_reserve_exact(entries)?;
    let (width, height) = (u32::from(descriptor.width), u32::from(descriptor.height));
    for index in 0..count {
        let start = index
            .checked_mul(width)
            .ok_or(Jbig2Error::Overflow("pattern offset"))?;
        let mut pattern = Bitmap::new(width, height, false, budget)?;
        for y in 0..height {
            for x in 0..width {
                let source = start
                    .checked_add(x)
                    .ok_or(Jbig2Error::Overflow("pattern offset"))?;
                if collective.pixel(source, y).unwrap_or(false) {
                    pattern.set_pixel(x, y, true)?;
                }
            }
        }
        patterns.push(pattern);
    }
    Ok(PatternDictionary { patterns })
}

/// Resolve a Huffman dictionary's four selected tables, plus the two the
/// specification fixes, or `None` for an arithmetic dictionary.
fn huffman_tables<'a>(
    coding: SymbolCoding,
    context: &mut DecodeContext<'a>,
) -> Result<Option<DictionaryTables<'a>>> {
    let SymbolCoding::Huffman {
        height,
        width,
        bitmap_size,
        aggregate_count,
    } = coding
    else {
        return Ok(None);
    };
    let resources = context.resources;
    Ok(Some(DictionaryTables {
        height: resolve_table(height, resources, context.budget)?,
        width: resolve_table(width, resources, context.budget)?,
        bitmap_size: resolve_table(bitmap_size, resources, context.budget)?,
        aggregate_count: resolve_table(aggregate_count, resources, context.budget)?,
        standard_b1: ResolvedTable::Standard(HuffmanTable::standard(
            StandardTable::B1,
            context.budget,
        )?),
        displacement: ResolvedTable::Standard(HuffmanTable::standard(
            StandardTable::B15,
            context.budget,
        )?),
    }))
}

/// Borrow every symbol this segment imports, in referred-to segment order.
///
/// Only views are collected, so a dictionary shared through globals is never
/// copied for the segments that refer to it (T.88 section 6.5.5).
pub(crate) fn imported_symbols<'a>(context: &DecodeContext<'a>) -> Result<Vec<BitmapRef<'a>>> {
    let resources = context.resources;
    let mut symbols = Vec::new();
    for id in context.header.references.iter() {
        let Ok(DecodedResourceRef::Symbols(dictionary)) = resources.resolve(id) else {
            continue;
        };
        symbols.try_reserve(dictionary.len())?;
        symbols.extend(dictionary.views());
    }
    Ok(symbols)
}

/// Typed borrowed result of resolving a decoded segment dependency.
/// The resolver owner must outlive the view, including across adapter calls.
#[derive(Debug, Clone, Copy)]
pub enum DecodedResourceRef<'a> {
    /// Exported symbols and retained contexts.
    Symbols(&'a SymbolDictionary),
    /// Halftone patterns.
    Patterns(&'a PatternDictionary),
    /// Custom integer Huffman table.
    Huffman(&'a HuffmanTable),
    /// Intermediate region bitmap available for refinement.
    Region(BitmapRef<'a>),
}

/// Read-only extension boundary for typed, already decoded dependencies.
///
/// Resolution must not mutate or release resources while a returned view is
/// borrowed. Missing references return `Jbig2Error::MissingReference`; callers
/// check expected resource kinds. Foreign code needs an adapter, not a vtable ABI.
///
/// Resource users borrow pixels without cloning dictionaries:
/// ```no_run
/// use pdf_jbig2::{Jbig2Error, Result, bitmap::BitmapRef,
///     dictionary::{DecodedResourceRef, ResourceResolver}, segment::SegmentNumber};
/// fn first_symbol(resources: &dyn ResourceResolver, id: SegmentNumber)
///     -> Result<BitmapRef<'_>> {
///     match resources.resolve(id)? {
///         DecodedResourceRef::Symbols(symbols) => symbols.symbol(0),
///         _ => Err(Jbig2Error::WrongResourceType(id)),
///     }
/// }
/// ```
pub trait ResourceResolver {
    /// Resolve a segment without cloning its decoded resource.
    fn resolve(&self, number: SegmentNumber) -> Result<DecodedResourceRef<'_>>;
}

/// Owned resources behind the default resolver.
#[derive(Debug)]
pub(crate) enum DecodedResource {
    Symbols(SymbolDictionary),
    Patterns(PatternDictionary),
    Huffman(HuffmanTable),
    Region(Bitmap),
}

/// Owned resources keyed by the segment number that defined them.
#[derive(Debug)]
pub(crate) struct ResourceStore {
    resources: Vec<(SegmentNumber, DecodedResource)>,
}

impl ResourceResolver for ResourceStore {
    fn resolve(&self, number: SegmentNumber) -> Result<DecodedResourceRef<'_>> {
        let resource = self
            .resources
            .iter()
            .find(|(id, _)| *id == number)
            .map(|(_, value)| value)
            .ok_or(Jbig2Error::MissingReference(number))?;
        Ok(match resource {
            DecodedResource::Symbols(value) => DecodedResourceRef::Symbols(value),
            DecodedResource::Patterns(value) => DecodedResourceRef::Patterns(value),
            DecodedResource::Huffman(value) => DecodedResourceRef::Huffman(value),
            DecodedResource::Region(value) => DecodedResourceRef::Region(value.as_ref()),
        })
    }
}

impl ResourceStore {
    pub(crate) fn new() -> Self {
        Self {
            resources: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, number: SegmentNumber, value: DecodedResource) -> Result<()> {
        if self.resources.iter().any(|(id, _)| *id == number) {
            return Err(Jbig2Error::DuplicateSegment(number));
        }
        self.resources.try_reserve(1)?;
        self.resources.push((number, value));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedResource, DecodedResourceRef, PatternDictionaryDecoder, PatternDictionaryDescriptor,
        ResourceResolver, ResourceStore,
    };
    use crate::{
        Jbig2Error, Result,
        bitmap::{Bitmap, DefaultCompositor},
        document::{DecodeBudget, DecodeLimits},
        region::DecodeContext,
        segment::SegmentNumber,
        stream::{Input, SegmentStream},
    };

    struct EmptyResources;
    impl ResourceResolver for EmptyResources {
        fn resolve(&self, number: SegmentNumber) -> Result<DecodedResourceRef<'_>> {
            Err(Jbig2Error::MissingReference(number))
        }
    }

    #[test]
    fn store_resolves_typed_region_and_rejects_duplicates() -> Result<()> {
        let mut budget = DecodeBudget::new(DecodeLimits {
            input_bytes: 10,
            segments: 10,
            references: 10,
            pixels: 10,
            allocation_bytes: 10,
            symbols: 10,
            work: 10,
        });
        let mut store = ResourceStore::new();
        let bitmap = Bitmap::new(1, 1, true, &mut budget)?;
        store.insert(SegmentNumber(2), DecodedResource::Region(bitmap))?;
        assert!(matches!(
            store.resolve(SegmentNumber(2))?,
            DecodedResourceRef::Region(_)
        ));
        assert!(store.resolve(SegmentNumber(1)).is_err());
        assert!(
            store
                .insert(SegmentNumber(2), DecodedResource::Region(Bitmap::empty()))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn mmr_pattern_dictionary_extracts_black_pattern() -> Result<()> {
        let payload = [1, 1, 1, 0, 0, 0, 0, 0x26, 0xa8];
        let mut input = vec![0, 0, 0, 1, 16, 0, 0, 0, 0, 0, 9];
        input.extend_from_slice(&payload);
        let mut budget = DecodeBudget::new(DecodeLimits {
            input_bytes: 100,
            segments: 10,
            references: 10,
            pixels: 100,
            allocation_bytes: 1000,
            symbols: 10,
            work: 1000,
        });
        let mut stream = SegmentStream::new(Input::Embedded(&input), &mut budget)?;
        let segment = stream
            .next_segment(&mut budget)?
            .ok_or(Jbig2Error::InvalidPageSequence("segment"))?;
        let descriptor = PatternDictionaryDescriptor::parse(segment, &mut budget)?.value;
        let mut compositor = DefaultCompositor;
        let mut context = DecodeContext {
            header: segment.header(),
            location: segment.payload_location(),
            resources: &EmptyResources,
            budget: &mut budget,
            compositor: &mut compositor,
        };
        let dictionary = PatternDictionaryDecoder.decode(&descriptor, &mut context)?;
        assert_eq!(dictionary.len(), 1);
        assert_eq!(dictionary.pattern(0)?.bytes(), &[0x80]);
        Ok(())
    }
}
