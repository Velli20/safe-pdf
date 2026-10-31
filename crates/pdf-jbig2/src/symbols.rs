//! Symbol dictionary header syntax and the shared symbol decoding procedure.
//!
//! The height class loop of T.88 section 6.5.5 is written once here. Arithmetic
//! and Huffman dictionaries drive it through [`SymbolSource`], supplying only
//! their entropy-specific integer reads and bitmap reconstruction.

use crate::{
    Jbig2Error, Result,
    bitmap::{Bitmap, BitmapRef},
    cursor::ByteCursor,
    dictionary::{RetainedContexts, SymbolCoding, SymbolDictionaryDescriptor},
    document::DecodeBudget,
    error::LimitKind,
    huffman::TableSelection,
    segment::{Parsed, Segment},
    template::{GenericTemplate, GenericTemplateId, RefinementTemplate, RefinementTemplateId},
};

/// Symbol dictionary flags (T.88 section 7.4.2.1.1).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DictionaryFlags(u16);

impl DictionaryFlags {
    /// Bits above the defined fields, which an encoder must leave clear.
    const RESERVED: u16 = 0xe000;

    /// Whether integer fields use Huffman rather than arithmetic coding.
    fn huffman(self) -> bool {
        self.0 & 1 != 0
    }

    /// Whether symbols may be refined from, or aggregated out of, other symbols.
    fn refine_aggregate(self) -> bool {
        self.0 & 2 != 0
    }

    /// Whether this dictionary reuses contexts a referred dictionary retained.
    fn reuse_contexts(self) -> bool {
        self.0 & 0x100 != 0
    }

    /// Whether this dictionary retains its contexts for a later dictionary.
    fn retain_contexts(self) -> bool {
        self.0 & 0x200 != 0
    }

    /// Generic template used for directly coded symbol bitmaps.
    fn generic_template(self) -> Result<GenericTemplateId> {
        GenericTemplateId::try_from(field(self.0, 10, 3))
    }

    /// Refinement template used for refined and aggregated symbols.
    fn refinement_template(self) -> Result<RefinementTemplateId> {
        RefinementTemplateId::try_from(field(self.0, 12, 1))
    }

    /// Resolve the four Huffman table selections this dictionary declares.
    ///
    /// Selector order follows the header's field order, which is also the order
    /// custom code table references are consumed (T.88 section 7.4.2.1.6).
    fn tables(self, custom: &mut CustomTableCursor<'_>) -> Result<SymbolCoding> {
        use crate::huffman::StandardTable::{B1, B2, B3, B4, B5};
        Ok(SymbolCoding::Huffman {
            height: custom.select(field(self.0, 2, 3), &[B4, B5], 3)?,
            width: custom.select(field(self.0, 4, 3), &[B2, B3], 3)?,
            bitmap_size: custom.select(field(self.0, 6, 1), &[B1], 1)?,
            aggregate_count: custom.select(field(self.0, 7, 1), &[B1], 1)?,
        })
    }
}

/// Extract a `width`-bit field starting at `shift`.
fn field(flags: u16, shift: u16, mask: u16) -> u8 {
    u8::try_from((flags >> shift) & mask).unwrap_or(0)
}

/// Assigns referred-to code table segments to selectors in header-field order.
///
/// A selector naming a custom table consumes the next unused Code Table
/// reference; standard selectors consume none (T.88 section 7.4.3.1.6).
pub(crate) struct CustomTableCursor<'a> {
    tables: &'a [crate::segment::SegmentNumber],
    next: usize,
}

impl<'a> CustomTableCursor<'a> {
    /// Begin assigning from the segment's Code Table references, in order.
    pub(crate) fn new(tables: &'a [crate::segment::SegmentNumber]) -> Self {
        Self { tables, next: 0 }
    }

    /// Resolve one selector against the standard tables it may name.
    ///
    /// `custom` is the selector value that names a custom table, which is
    /// always the field's largest value. That is not simply one past the last
    /// standard choice: a two-bit field offering two standard tables reserves
    /// value two and uses three for custom, while a one-bit field offering one
    /// standard table uses one.
    pub(crate) fn select(
        &mut self,
        selector: u8,
        standard: &[crate::huffman::StandardTable],
        custom: u8,
    ) -> Result<TableSelection> {
        if selector != custom {
            return standard
                .get(usize::from(selector))
                .copied()
                .map(TableSelection::Standard)
                .ok_or(Jbig2Error::InvalidResource("Huffman table selector"));
        }
        let number = self
            .tables
            .get(self.next)
            .copied()
            .ok_or(Jbig2Error::InvalidResource(
                "custom Huffman table reference",
            ))?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("custom table cursor"))?;
        Ok(TableSelection::Custom(number))
    }
}

/// Parse a symbol dictionary segment header (T.88 section 7.4.2.1).
pub(crate) fn parse_descriptor<'a>(
    segment: Segment<'a>,
    tables: &[crate::segment::SegmentNumber],
) -> Result<Parsed<'a, SymbolDictionaryDescriptor<'a>>> {
    let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
    let flags_location = cursor.location();
    let flags = DictionaryFlags(cursor.u16()?);
    if flags.0 & DictionaryFlags::RESERVED != 0 {
        return Err(Jbig2Error::ReservedField {
            location: flags_location,
            field: "symbol dictionary flags",
        });
    }
    // Adaptive coordinates appear only where the coding mode reads them: a
    // Huffman dictionary codes no generic bitmaps, and refinement template one
    // has no adaptive pixels.
    let generic_template = if flags.huffman() {
        None
    } else {
        Some(GenericTemplate::parse(
            flags.generic_template()?,
            &mut cursor,
        )?)
    };
    let refinement_template = if flags.refine_aggregate() {
        Some(RefinementTemplate::parse(
            flags.refinement_template()?,
            &mut cursor,
        )?)
    } else {
        None
    };
    let exported_symbols = cursor.u32()?;
    let new_symbols = cursor.u32()?;
    let mut custom = CustomTableCursor::new(tables);
    let coding = if flags.huffman() {
        flags.tables(&mut custom)?
    } else {
        SymbolCoding::Arithmetic
    };
    let data = cursor.remaining();
    Ok(Parsed {
        value: SymbolDictionaryDescriptor {
            coding,
            refinement_aggregation: flags.refine_aggregate(),
            generic_template,
            refinement_template,
            reuse_contexts: flags.reuse_contexts(),
            retain_contexts: flags.retain_contexts(),
            new_symbols,
            exported_symbols,
            data,
        },
        remaining: data,
        consumed: cursor.position(),
    })
}

/// Symbols an in-progress dictionary may refer to: the ones it imported,
/// followed by the ones it has already decoded (T.88 section 6.5.8.2.3).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DictionarySymbols<'a> {
    imported: &'a [BitmapRef<'a>],
    decoded: &'a [Option<Bitmap>],
}

impl<'a> DictionarySymbols<'a> {
    /// View the imported symbols followed by the new symbol slots.
    ///
    /// A slot is empty while its symbol is still deferred, which makes a
    /// reference to it fail rather than silently read the wrong pixels.
    pub(crate) fn new(imported: &'a [BitmapRef<'a>], decoded: &'a [Option<Bitmap>]) -> Self {
        Self { imported, decoded }
    }

    /// Index the symbol being decoded right now will occupy.
    pub(crate) fn next_index(self) -> usize {
        self.decoded.len()
    }

    /// Borrow the symbol an identifier selects.
    pub(crate) fn get(self, id: u32) -> Result<BitmapRef<'a>> {
        let index = usize::try_from(id).map_err(|_| Jbig2Error::Overflow("symbol index"))?;
        if let Some(symbol) = self.imported.get(index) {
            return Ok(*symbol);
        }
        let offset = index.saturating_sub(self.imported.len());
        let slot = self
            .decoded
            .get(offset)
            .ok_or(Jbig2Error::InvalidResource("symbol identifier"))?;
        slot.as_ref()
            .map(Bitmap::as_ref)
            .ok_or(Jbig2Error::UnresolvedSymbol(id))
    }
}

/// What a coding mode produced for one symbol of a height class.
pub(crate) enum SymbolOutcome {
    /// Pixels decoded in place.
    Ready(Bitmap),
    /// Pixels that this height class's collective bitmap supplies.
    Collective,
    /// Pixels that can only be decoded once every symbol exists, because this
    /// symbol aggregates one the dictionary has not reached yet.
    Deferred,
}

/// Dimensions of one symbol within its height class.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SymbolSize {
    /// Width in pixels, accumulated from this class's width deltas.
    pub(crate) width: u32,
    /// Height in pixels, shared by every symbol of this class.
    pub(crate) height: u32,
}

/// Entropy-specific reads the symbol decoding procedure needs.
pub(crate) trait SymbolSource {
    /// Decode `DH`, this height class's height delta (T.88 section 6.5.6).
    fn delta_height(&mut self, budget: &mut DecodeBudget) -> Result<i32>;

    /// Decode `DW`, or `None` at the height class's out-of-band terminator
    /// (T.88 section 6.5.7).
    fn delta_width(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>>;

    /// Decode one symbol's bitmap, or `None` to defer it to this height class's
    /// collective bitmap (T.88 section 6.5.8).
    fn decode_symbol(
        &mut self,
        size: SymbolSize,
        symbols: DictionarySymbols<'_>,
        budget: &mut DecodeBudget,
    ) -> Result<SymbolOutcome>;

    /// Split a height class's collective bitmap into its deferred symbols
    /// (T.88 section 6.5.9).
    fn collective_symbols(
        &mut self,
        widths: &[u32],
        height: u32,
        budget: &mut DecodeBudget,
    ) -> Result<Vec<Bitmap>> {
        let _ = (widths, height, budget);
        Err(Jbig2Error::InvalidResource(
            "collective bitmap in a directly coded dictionary",
        ))
    }

    /// Fill any slots this source deferred, now that every symbol exists.
    ///
    /// Only coding modes that can defer implement this; the default reports
    /// that nothing was deferred.
    fn resolve_deferred(
        &mut self,
        slots: &mut [Option<Bitmap>],
        imported: &[BitmapRef<'_>],
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        let _ = (slots, imported, budget);
        Ok(())
    }

    /// Decode `EX`, the next export run length (T.88 section 6.5.10).
    fn export_run(&mut self, budget: &mut DecodeBudget) -> Result<u32>;

    /// Release the generic and refinement statistics this source used, for
    /// a dictionary that retains them.
    fn into_contexts(self: Box<Self>) -> RetainedContexts;
}

/// Decode every new symbol this dictionary defines.
///
/// Symbols arrive in height classes of increasing height; within a class widths
/// accumulate from successive deltas until an out-of-band code ends the class
/// (T.88 section 6.5.5).
pub(crate) fn decode_new_symbols(
    source: &mut dyn SymbolSource,
    imported: &[BitmapRef<'_>],
    count: u32,
    budget: &mut DecodeBudget,
) -> Result<Vec<Bitmap>> {
    budget.charge(LimitKind::Symbols, u64::from(count))?;
    let capacity = usize::try_from(count).map_err(|_| Jbig2Error::Overflow("symbol count"))?;
    let mut slots: Vec<Option<Bitmap>> = Vec::new();
    slots.try_reserve_exact(capacity)?;
    let mut height = 0_u32;
    while slots.len() < capacity {
        height = advance_extent(height, source.delta_height(budget)?, "symbol height")?;
        decode_height_class(source, imported, &mut slots, height, capacity, budget)?;
    }
    source.resolve_deferred(&mut slots, imported, budget)?;
    let mut decoded = Vec::new();
    decoded.try_reserve_exact(slots.len())?;
    for slot in slots {
        decoded.push(slot.ok_or(Jbig2Error::InvalidResource("unresolved deferred symbol"))?);
    }
    Ok(decoded)
}

/// Decode one height class, appending its symbols in width order.
fn decode_height_class(
    source: &mut dyn SymbolSource,
    imported: &[BitmapRef<'_>],
    slots: &mut Vec<Option<Bitmap>>,
    height: u32,
    capacity: usize,
    budget: &mut DecodeBudget,
) -> Result<()> {
    let mut width = 0_u32;
    let mut collective: Vec<(usize, u32)> = Vec::new();
    while let Some(delta) = source.delta_width(budget)? {
        width = advance_extent(width, delta, "symbol width")?;
        if slots.len() >= capacity {
            return Err(Jbig2Error::InvalidResource(
                "symbol dictionary exceeds its declared new symbol count",
            ));
        }
        let size = SymbolSize { width, height };
        let outcome = {
            let symbols = DictionarySymbols::new(imported, slots);
            source.decode_symbol(size, symbols, budget)?
        };
        let index = slots.len();
        slots.try_reserve(1)?;
        match outcome {
            SymbolOutcome::Ready(bitmap) => slots.push(Some(bitmap)),
            SymbolOutcome::Deferred => slots.push(None),
            SymbolOutcome::Collective => {
                slots.push(None);
                collective.try_reserve(1)?;
                collective.push((index, width));
            }
        }
    }
    if collective.is_empty() {
        return Ok(());
    }
    let widths: Vec<u32> = collective.iter().map(|(_, width)| *width).collect();
    let symbols = source.collective_symbols(&widths, height, budget)?;
    if symbols.len() != collective.len() {
        return Err(Jbig2Error::InvalidResource(
            "collective bitmap symbol count",
        ));
    }
    for ((index, _), bitmap) in collective.into_iter().zip(symbols) {
        let slot = slots
            .get_mut(index)
            .ok_or(Jbig2Error::InvalidResource("collective bitmap slot"))?;
        *slot = Some(bitmap);
    }
    Ok(())
}

/// Apply a signed delta to a running symbol extent.
fn advance_extent(current: u32, delta: i32, what: &'static str) -> Result<u32> {
    let next = i64::from(current)
        .checked_add(i64::from(delta))
        .ok_or(Jbig2Error::Overflow(what))?;
    u32::try_from(next).map_err(|_| Jbig2Error::InvalidResource(what))
}

/// Select the exported subset of the imported and newly decoded symbols.
///
/// Export flags arrive as alternating run lengths, beginning with a run of
/// symbols that are not exported (T.88 section 6.5.10). An exported symbol that
/// came from another dictionary is copied, so this dictionary's pixels stay
/// valid after its inputs are released.
pub(crate) fn apply_export_runs(
    source: &mut dyn SymbolSource,
    imported: &[BitmapRef<'_>],
    decoded: Vec<Bitmap>,
    exported: u32,
    budget: &mut DecodeBudget,
) -> Result<Vec<Bitmap>> {
    let total = imported.len().saturating_add(decoded.len());
    let wanted = usize::try_from(exported).map_err(|_| Jbig2Error::Overflow("export count"))?;
    if wanted > total {
        return Err(Jbig2Error::InvalidResource(
            "dictionary exports more symbols than it holds",
        ));
    }
    let mut flags = Vec::new();
    flags.try_reserve_exact(total)?;
    flags.resize(total, false);
    let mut index = 0_usize;
    let mut exporting = false;
    while index < total {
        let run = usize::try_from(source.export_run(budget)?)
            .map_err(|_| Jbig2Error::Overflow("export run"))?;
        let end = index.checked_add(run).filter(|end| *end <= total).ok_or(
            Jbig2Error::InvalidResource("export run exceeds symbol count"),
        )?;
        if exporting {
            for flag in flags.get_mut(index..end).unwrap_or_default() {
                *flag = true;
            }
        }
        index = end;
        exporting = !exporting;
    }
    collect_exported(imported, decoded, &flags, wanted, budget)
}

/// Gather the flagged symbols, moving new ones and copying imported ones.
fn collect_exported(
    imported: &[BitmapRef<'_>],
    decoded: Vec<Bitmap>,
    flags: &[bool],
    wanted: usize,
    budget: &mut DecodeBudget,
) -> Result<Vec<Bitmap>> {
    let mut exported = Vec::new();
    exported.try_reserve_exact(wanted)?;
    for (index, symbol) in imported.iter().enumerate() {
        if flags.get(index).copied().unwrap_or(false) {
            exported.try_reserve(1)?;
            exported.push(Bitmap::copy_of(*symbol, budget)?);
        }
    }
    for (offset, symbol) in decoded.into_iter().enumerate() {
        let index = imported.len().saturating_add(offset);
        if flags.get(index).copied().unwrap_or(false) {
            exported.try_reserve(1)?;
            exported.push(symbol);
        }
    }
    if exported.len() != wanted {
        return Err(Jbig2Error::InvalidResource(
            "export runs disagree with the declared export count",
        ));
    }
    Ok(exported)
}
