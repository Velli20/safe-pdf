//! Huffman entropy source for symbol dictionaries and text regions.
//!
//! One bit cursor walks the whole coded payload. Where a procedure nests an
//! arithmetic body — a refined symbol instance, whose size the surrounding
//! Huffman stream declares — the body is isolated to exactly its declared
//! bytes and the outer cursor resumes on the byte boundary after it
//! (T.88 sections 6.4.11 and 6.5.8.2).

use crate::{
    Jbig2Error, Result,
    arithmetic::{ArithmeticContexts, ArithmeticDecoder},
    bitmap::{Bitmap, BitmapRef},
    dictionary::RetainedContexts,
    document::DecodeBudget,
    huffman::{HuffmanDecoder, HuffmanTable, ResolvedTable},
    region::RefinementProcedure,
    symbols::{DictionarySymbols, SymbolOutcome, SymbolSize, SymbolSource},
    template::RefinementTemplate,
    text::TextSymbolSource,
};

/// The tables a Huffman symbol dictionary reads its integer fields with.
pub(crate) struct DictionaryTables<'a> {
    /// `SDHUFFDH`, height class height delta.
    pub(crate) height: ResolvedTable<'a>,
    /// `SDHUFFDW`, symbol width delta.
    pub(crate) width: ResolvedTable<'a>,
    /// `SDHUFFBMSIZE`, collective bitmap size.
    pub(crate) bitmap_size: ResolvedTable<'a>,
    /// `SDHUFFAGGINST`, aggregate instance count.
    pub(crate) aggregate_count: ResolvedTable<'a>,
    /// Table B.1, which both export runs and a refined symbol's body size
    /// always use, whatever `SDHUFFBMSIZE` selects for collective bitmaps
    /// (T.88 sections 6.5.8.2.2 and 6.5.10).
    pub(crate) standard_b1: ResolvedTable<'a>,
    /// Table B.15, which a refined symbol's displacements always use
    /// (T.88 section 6.5.8.2.2).
    pub(crate) displacement: ResolvedTable<'a>,
}

/// Huffman decoding state for one symbol dictionary segment.
pub(crate) struct HuffmanSymbolSource<'a, 't> {
    decoder: HuffmanDecoder<'a>,
    tables: &'t DictionaryTables<'t>,
    refinement: Option<RefinementState>,
    code_bits: u8,
    /// Fixed tables for the aggregate text run, when aggregation is possible.
    aggregate: Option<AggregateTables>,
    /// Aggregate symbols captured for decoding once every symbol exists.
    pending: Vec<PendingSymbol<'a>>,
    /// Total width of the height class being decoded, for its collective bitmap.
    class_width: u32,
}

/// Refinement template and contexts, shared across every refined symbol.
struct RefinementState {
    template: RefinementTemplate,
    contexts: ArithmeticContexts,
}

/// The tables a refined instance reads its deltas and body size with.
struct RefinementTables<'a> {
    width: &'a HuffmanTable,
    height: &'a HuffmanTable,
    x: &'a HuffmanTable,
    y: &'a HuffmanTable,
    size: &'a HuffmanTable,
}

/// Decode one refined instance from a Huffman-coded stream.
///
/// The four deltas and the body size are Huffman coded; the bitmap itself is
/// arithmetic coded in a body of exactly the declared size, after which the
/// Huffman cursor resumes on the next byte boundary (T.88 section 6.4.11).
fn decode_refined_instance(
    decoder: &mut HuffmanDecoder<'_>,
    state: &mut RefinementState,
    tables: &RefinementTables<'_>,
    reference: BitmapRef<'_>,
    budget: &mut DecodeBudget,
) -> Result<Bitmap> {
    let delta_width = huffman_integer(decoder, tables.width, "refinement width", budget)?;
    let delta_height = huffman_integer(decoder, tables.height, "refinement height", budget)?;
    let delta_x = huffman_integer(decoder, tables.x, "refinement X", budget)?;
    let delta_y = huffman_integer(decoder, tables.y, "refinement Y", budget)?;
    let body_bytes = usize::try_from(
        decoder
            .decode_integer(tables.size, budget)?
            .value("refined instance size")?,
    )
    .map_err(|_| Jbig2Error::Overflow("refined instance size"))?;
    let size = crate::arithmetic_source::refined_size(reference, delta_width, delta_height)?;
    let procedure = RefinementProcedure {
        width: size.width,
        height: size.height,
        template: state.template,
        typical_prediction: false,
        reference_dx: crate::arithmetic_source::centered_offset(delta_width, delta_x)?,
        reference_dy: crate::arithmetic_source::centered_offset(delta_height, delta_y)?,
    };
    let location = decoder.location();
    let body = decoder.take_aligned_bytes(body_bytes)?;
    let mut arithmetic = ArithmeticDecoder::new(body, location)?;
    let bitmap = procedure.decode(reference, &mut arithmetic, &mut state.contexts, budget)?;
    decoder.align_to_byte()?;
    Ok(bitmap)
}

/// Decode one Huffman integer field, rejecting an out-of-band result.
fn huffman_integer(
    decoder: &mut HuffmanDecoder<'_>,
    table: &HuffmanTable,
    what: &'static str,
    budget: &mut DecodeBudget,
) -> Result<i32> {
    i32::try_from(decoder.decode_integer(table, budget)?.value(what)?)
        .map_err(|_| Jbig2Error::Overflow(what))
}

/// One symbol whose coded bytes were captured for a later pass.
///
/// Either refinement shape may name a symbol the dictionary has not reached
/// yet, so every refined and aggregated symbol is captured here and decoded
/// once the whole set exists.
struct PendingSymbol<'a> {
    /// New-symbol index this entry fills.
    slot: usize,
    /// Dimensions of the symbol being assembled.
    size: SymbolSize,
    /// The captured bytes and how to reconstruct them.
    coding: PendingCoding<'a>,
}

/// How a captured symbol is reconstructed.
enum PendingCoding<'a> {
    /// A single refinement of one referenced symbol.
    Refined {
        /// Identifier of the reference symbol.
        id: u32,
        /// Horizontal displacement of the reference.
        reference_dx: i32,
        /// Vertical displacement of the reference.
        reference_dy: i32,
        /// Arithmetic body; empty means the reference is adopted unchanged.
        body: &'a [u8],
    },
    /// A text run placing several instances into one symbol.
    Aggregate {
        /// Number of instances the run places.
        instances: u32,
        /// The run's bytes, from the byte holding its first bit onward.
        body: &'a [u8],
        /// Bit position of the run's start within its first byte.
        start_bit_offset: usize,
    },
}

/// Standard tables the aggregate text run inside a symbol dictionary fixes
/// (T.88 section 6.5.8.2.1).
struct AggregateTables {
    first_s: HuffmanTable,
    delta_s: HuffmanTable,
    delta_t: HuffmanTable,
    displacement: HuffmanTable,
    size: HuffmanTable,
}

impl AggregateTables {
    /// Build the fixed standard tables this procedure always uses.
    fn new(budget: &mut DecodeBudget) -> Result<Self> {
        use crate::huffman::StandardTable::{B1, B6, B8, B11, B15};
        Ok(Self {
            first_s: HuffmanTable::standard(B6, budget)?,
            delta_s: HuffmanTable::standard(B8, budget)?,
            delta_t: HuffmanTable::standard(B11, budget)?,
            displacement: HuffmanTable::standard(B15, budget)?,
            size: HuffmanTable::standard(B1, budget)?,
        })
    }

    /// Borrow the refinement tables an aggregated instance uses.
    fn refinement(&self) -> RefinementTables<'_> {
        RefinementTables {
            width: &self.displacement,
            height: &self.displacement,
            x: &self.displacement,
            y: &self.displacement,
            size: &self.size,
        }
    }
}

impl<'a, 't> HuffmanSymbolSource<'a, 't> {
    /// Prepare decoding for one dictionary's coded payload.
    pub(crate) fn new(
        data: &'a [u8],
        location: crate::error::SourceLocation,
        tables: &'t DictionaryTables<'t>,
        refinement_template: Option<RefinementTemplate>,
        inherited: Option<ArithmeticContexts>,
        code_bits: u8,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        let refinement = match refinement_template {
            Some(template) => Some(RefinementState {
                contexts: ArithmeticContexts::inherit(
                    inherited,
                    template.id().context_count(),
                    budget,
                )?,
                template,
            }),
            None => None,
        };
        let aggregate = match refinement {
            Some(_) => Some(AggregateTables::new(budget)?),
            None => None,
        };
        Ok(Self {
            decoder: HuffmanDecoder::new(data, location),
            tables,
            refinement,
            code_bits,
            aggregate,
            pending: Vec::new(),
            class_width: 0,
        })
    }

    /// Decode one integer field, rejecting an out-of-band result.
    fn integer(
        &mut self,
        table: &HuffmanTable,
        what: &'static str,
        budget: &mut DecodeBudget,
    ) -> Result<i32> {
        i32::try_from(self.decoder.decode_integer(table, budget)?.value(what)?)
            .map_err(|_| Jbig2Error::Overflow(what))
    }

    /// Capture a single refinement's fields and its arithmetic body.
    ///
    /// The reference may be a symbol this dictionary has not decoded yet, so
    /// nothing is reconstructed here (T.88 section 6.5.8.2.2).
    fn capture_refinement(&mut self, budget: &mut DecodeBudget) -> Result<PendingCoding<'a>> {
        let id = self.decoder.read_bits(self.code_bits, budget)?;
        let reference_dx = {
            let table = self.tables.displacement.table();
            huffman_integer(&mut self.decoder, table, "refinement X", budget)?
        };
        let reference_dy = {
            let table = self.tables.displacement.table();
            huffman_integer(&mut self.decoder, table, "refinement Y", budget)?
        };
        let body_bytes = {
            let table = self.tables.standard_b1.table();
            let declared = self.decoder.decode_integer(table, budget)?;
            usize::try_from(declared.value("refined symbol size")?)
                .map_err(|_| Jbig2Error::Overflow("refined symbol size"))?
        };
        Ok(PendingCoding::Refined {
            id,
            reference_dx,
            reference_dy,
            body: self.decoder.take_aligned_bytes(body_bytes)?,
        })
    }

    /// Capture an aggregate run's bytes by walking it without reconstructing.
    ///
    /// The run is a text region decoding procedure, which ends on its last
    /// out-of-band code rather than on a byte boundary; only an explicitly
    /// sized bitmap body aligns the stream (T.88 section 6.5.8.2.1).
    fn capture_aggregate(
        &mut self,
        instances: u32,
        budget: &mut DecodeBudget,
    ) -> Result<PendingCoding<'a>> {
        let tables = aggregate_tables(&self.aggregate)?;
        let start_bit = self.decoder.bit_position();
        scan_aggregate_run(&mut self.decoder, tables, self.code_bits, instances, budget)?;
        let end_bit = self.decoder.bit_position();
        Ok(PendingCoding::Aggregate {
            instances,
            body: self.decoder.slice_bits(start_bit, end_bit)?,
            start_bit_offset: start_bit % 8,
        })
    }

    /// Decode one captured entry against the symbols available so far.
    ///
    /// Reports `UnresolvedSymbol` when the entry names a symbol that is itself
    /// still captured, which the resolution rounds retry later.
    fn decode_pending(
        &mut self,
        entry: &PendingSymbol<'_>,
        slots: &[Option<Bitmap>],
        imported: &[BitmapRef<'_>],
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let symbols = DictionarySymbols::new(imported, slots);
        match entry.coding {
            PendingCoding::Refined {
                id,
                reference_dx,
                reference_dy,
                body,
            } => {
                let reference = symbols.get(id)?;
                if body.is_empty() {
                    // No coded body means the reference is adopted unchanged.
                    return Bitmap::copy_of(reference, budget);
                }
                let location = self.decoder.location();
                let state = self
                    .refinement
                    .as_mut()
                    .ok_or(Jbig2Error::InvalidResource("refinement template"))?;
                let procedure = RefinementProcedure {
                    width: entry.size.width,
                    height: entry.size.height,
                    template: state.template,
                    typical_prediction: false,
                    reference_dx,
                    reference_dy,
                };
                let mut arithmetic = ArithmeticDecoder::new(body, location)?;
                procedure.decode(reference, &mut arithmetic, &mut state.contexts, budget)
            }
            PendingCoding::Aggregate {
                instances,
                body,
                start_bit_offset,
            } => self.replay_aggregate(entry, symbols, instances, body, start_bit_offset, budget),
        }
    }

    /// Replay one captured aggregate run over the completed symbol set.
    ///
    /// The run is decoded from its own captured bytes on a separate cursor, so
    /// the dictionary's main cursor, long past it, is not disturbed.
    fn replay_aggregate(
        &mut self,
        entry: &PendingSymbol<'_>,
        symbols: DictionarySymbols<'_>,
        instances: u32,
        body: &[u8],
        start_bit_offset: usize,
        budget: &mut DecodeBudget,
    ) -> Result<Bitmap> {
        let mut bitmap = Bitmap::new(entry.size.width, entry.size.height, false, budget)?;
        let layout = crate::text::TextLayout {
            instances,
            strip_size: 1,
            reference_corner: crate::region::ReferenceCorner::TopLeft,
            transposed: false,
            operator: crate::bitmap::CombinationOperator::Or,
            delta_s_offset: 0,
        };
        let location = self.decoder.location();
        let tables = aggregate_tables(&self.aggregate)?;
        let state = self
            .refinement
            .as_mut()
            .ok_or(Jbig2Error::InvalidResource("refinement template"))?;
        let mut replay = AggregateReplay {
            decoder: HuffmanDecoder::new(body, location),
            tables,
            refinement: state,
            code_bits: self.code_bits,
        };
        replay.decoder.seek_to_bit(start_bit_offset);
        // Aggregation assembles one symbol's own pixels rather than page
        // output, so it uses the scalar compositor with the OR operator that
        // T.88 section 6.5.8.2.1 fixes.
        let mut compositor = crate::bitmap::DefaultCompositor;
        crate::text::decode_text(
            layout,
            &symbols,
            &mut replay,
            &mut bitmap,
            &mut compositor,
            budget,
        )?;
        Ok(bitmap)
    }
}

impl SymbolSource for HuffmanSymbolSource<'_, '_> {
    fn delta_height(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        self.class_width = 0;
        let table = self.tables.height.table();
        i32::try_from(
            self.decoder
                .decode_integer(table, budget)?
                .value("symbol height delta")?,
        )
        .map_err(|_| Jbig2Error::Overflow("symbol height delta"))
    }

    fn delta_width(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>> {
        let table = self.tables.width.table();
        match self.decoder.decode_integer(table, budget)? {
            crate::arithmetic::IntegerValue::OutOfBand => Ok(None),
            value => i32::try_from(value.value("symbol width delta")?)
                .map(Some)
                .map_err(|_| Jbig2Error::Overflow("symbol width delta")),
        }
    }

    fn decode_symbol(
        &mut self,
        size: SymbolSize,
        symbols: DictionarySymbols<'_>,
        budget: &mut DecodeBudget,
    ) -> Result<SymbolOutcome> {
        self.class_width = self
            .class_width
            .checked_add(size.width)
            .ok_or(Jbig2Error::Overflow("height class width"))?;
        if self.refinement.is_none() {
            // Without refinement every symbol of this class comes from one
            // collective bitmap read after the class ends.
            return Ok(SymbolOutcome::Collective);
        }
        let instances = {
            let table = self.tables.aggregate_count.table();
            self.integer(table, "aggregate instances", budget)?
        };
        if instances < 1 {
            return Err(Jbig2Error::InvalidResource("aggregate instance count"));
        }
        let slot = symbols.next_index();
        let coding = if instances > 1 {
            let instances = u32::try_from(instances)
                .map_err(|_| Jbig2Error::InvalidResource("aggregate instance count"))?;
            self.capture_aggregate(instances, budget)?
        } else {
            self.capture_refinement(budget)?
        };
        self.pending.try_reserve(1)?;
        self.pending.push(PendingSymbol { slot, size, coding });
        Ok(SymbolOutcome::Deferred)
    }

    fn collective_symbols(
        &mut self,
        widths: &[u32],
        height: u32,
        budget: &mut DecodeBudget,
    ) -> Result<Vec<Bitmap>> {
        let total = widths
            .iter()
            .try_fold(0_u32, |sum, width| sum.checked_add(*width))
            .ok_or(Jbig2Error::Overflow("collective bitmap width"))?;
        let declared = {
            let table = self.tables.bitmap_size.table();
            let value = self.decoder.decode_integer(table, budget)?;
            usize::try_from(value.value("collective bitmap size")?)
                .map_err(|_| Jbig2Error::Overflow("collective bitmap size"))?
        };
        let location = self.decoder.location();
        let collective = if declared == 0 {
            decode_uncompressed_collective(&mut self.decoder, total, height, budget)?
        } else {
            let body = self.decoder.take_aligned_bytes(declared)?;
            crate::mmr::decode(total, height, body, location, budget)?.0
        };
        split_collective(collective.as_ref(), widths, height, budget)
    }

    fn resolve_deferred(
        &mut self,
        slots: &mut [Option<Bitmap>],
        imported: &[BitmapRef<'_>],
        budget: &mut DecodeBudget,
    ) -> Result<()> {
        // A captured symbol may name any other, so resolution proceeds in
        // rounds: whatever can be decoded is, and the rest waits. A round that
        // decodes nothing means the references form a cycle.
        let mut pending = std::mem::take(&mut self.pending);
        while !pending.is_empty() {
            let round = pending.len();
            let mut blocked = Vec::new();
            blocked.try_reserve_exact(round)?;
            for entry in pending.drain(..) {
                match self.decode_pending(&entry, slots, imported, budget) {
                    Ok(bitmap) => {
                        let slot = slots
                            .get_mut(entry.slot)
                            .ok_or(Jbig2Error::InvalidResource("deferred symbol slot"))?;
                        *slot = Some(bitmap);
                    }
                    Err(Jbig2Error::UnresolvedSymbol(_)) => blocked.push(entry),
                    Err(error) => return Err(error),
                }
            }
            if blocked.len() == round {
                return Err(Jbig2Error::InvalidResource("symbol dependency cycle"));
            }
            pending = blocked;
        }
        Ok(())
    }

    fn export_run(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        let table = self.tables.standard_b1.table();
        let run = self
            .decoder
            .decode_integer(table, budget)?
            .value("export run")?;
        u32::try_from(run).map_err(|_| Jbig2Error::InvalidResource("negative export run"))
    }

    fn into_contexts(self: Box<Self>) -> RetainedContexts {
        RetainedContexts {
            generic: None,
            refinement: self.refinement.map(|state| state.contexts),
        }
    }
}

/// Replays one captured aggregate run over the completed symbol set.
struct AggregateReplay<'a, 't> {
    decoder: HuffmanDecoder<'a>,
    tables: &'t AggregateTables,
    refinement: &'t mut RefinementState,
    code_bits: u8,
}

impl TextSymbolSource for AggregateReplay<'_, '_> {
    fn strip_delta_t(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        huffman_integer(
            &mut self.decoder,
            &self.tables.delta_t,
            "aggregate strip delta",
            budget,
        )
    }

    fn first_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        huffman_integer(
            &mut self.decoder,
            &self.tables.first_s,
            "aggregate first S",
            budget,
        )
    }

    fn next_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>> {
        match self.decoder.decode_integer(&self.tables.delta_s, budget)? {
            crate::arithmetic::IntegerValue::OutOfBand => Ok(None),
            value => i32::try_from(value.value("aggregate S delta")?)
                .map(Some)
                .map_err(|_| Jbig2Error::Overflow("aggregate S delta")),
        }
    }

    fn current_t(&mut self, _budget: &mut DecodeBudget) -> Result<i32> {
        // Aggregation fixes SBSTRIPS to one, so no strip offset is coded.
        Ok(0)
    }

    fn symbol_id(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        self.decoder.read_bits(self.code_bits, budget)
    }

    fn refine_flag(&mut self, budget: &mut DecodeBudget) -> Result<bool> {
        Ok(self.decoder.read_bits(1, budget)? != 0)
    }

    fn refine(&mut self, reference: BitmapRef<'_>, budget: &mut DecodeBudget) -> Result<Bitmap> {
        let tables = self.tables.refinement();
        decode_refined_instance(
            &mut self.decoder,
            self.refinement,
            &tables,
            reference,
            budget,
        )
    }
}

impl TextSymbolSource for HuffmanSymbolSource<'_, '_> {
    fn strip_delta_t(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        let tables = aggregate_tables(&self.aggregate)?;
        huffman_integer(
            &mut self.decoder,
            &tables.delta_t,
            "aggregate strip delta",
            budget,
        )
    }

    fn first_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        let tables = aggregate_tables(&self.aggregate)?;
        huffman_integer(
            &mut self.decoder,
            &tables.first_s,
            "aggregate first S",
            budget,
        )
    }

    fn next_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>> {
        let tables = aggregate_tables(&self.aggregate)?;
        match self.decoder.decode_integer(&tables.delta_s, budget)? {
            crate::arithmetic::IntegerValue::OutOfBand => Ok(None),
            value => i32::try_from(value.value("aggregate S delta")?)
                .map(Some)
                .map_err(|_| Jbig2Error::Overflow("aggregate S delta")),
        }
    }

    fn current_t(&mut self, _budget: &mut DecodeBudget) -> Result<i32> {
        // Aggregation fixes SBSTRIPS to one, so no strip offset is coded.
        Ok(0)
    }

    fn symbol_id(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        self.decoder.read_bits(self.code_bits, budget)
    }

    fn refine_flag(&mut self, budget: &mut DecodeBudget) -> Result<bool> {
        Ok(self.decoder.read_bits(1, budget)? != 0)
    }

    fn refine(&mut self, reference: BitmapRef<'_>, budget: &mut DecodeBudget) -> Result<Bitmap> {
        let tables = self
            .aggregate
            .as_ref()
            .ok_or(Jbig2Error::InvalidResource("aggregate tables"))?
            .refinement();
        let state = self
            .refinement
            .as_mut()
            .ok_or(Jbig2Error::InvalidResource("refinement template"))?;
        decode_refined_instance(&mut self.decoder, state, &tables, reference, budget)
    }
}

/// Walk an aggregate text run without reconstructing anything, to learn where
/// it ends.
///
/// This consumes exactly the fields the strip procedure reads, so the
/// surrounding Huffman stream continues at the right place even though the
/// symbols the run names may not exist yet. Aggregation fixes `SBSTRIPS` to
/// one, so no within-strip offset is coded (T.88 sections 6.4.5, 6.5.8.2.1).
fn scan_aggregate_run(
    decoder: &mut HuffmanDecoder<'_>,
    tables: &AggregateTables,
    code_bits: u8,
    instances: u32,
    budget: &mut DecodeBudget,
) -> Result<()> {
    let label = "aggregate strip delta";
    huffman_integer(decoder, &tables.delta_t, label, budget)?;
    let mut placed = 0_u32;
    while placed < instances {
        huffman_integer(decoder, &tables.delta_t, label, budget)?;
        huffman_integer(decoder, &tables.first_s, "aggregate first S", budget)?;
        loop {
            if placed >= instances {
                return Err(Jbig2Error::InvalidResource(
                    "aggregate run places more instances than declared",
                ));
            }
            decoder.read_bits(code_bits, budget)?;
            if decoder.read_bits(1, budget)? != 0 {
                scan_refined_instance(decoder, tables, budget)?;
            }
            placed = placed
                .checked_add(1)
                .ok_or(Jbig2Error::Overflow("aggregate instances"))?;
            if matches!(
                decoder.decode_integer(&tables.delta_s, budget)?,
                crate::arithmetic::IntegerValue::OutOfBand
            ) {
                break;
            }
        }
    }
    Ok(())
}

/// Skip one refined instance's deltas and its declared arithmetic body.
fn scan_refined_instance(
    decoder: &mut HuffmanDecoder<'_>,
    tables: &AggregateTables,
    budget: &mut DecodeBudget,
) -> Result<()> {
    for _ in 0..4 {
        huffman_integer(decoder, &tables.displacement, "refinement delta", budget)?;
    }
    let body_bytes = usize::try_from(
        decoder
            .decode_integer(&tables.size, budget)?
            .value("refined instance size")?,
    )
    .map_err(|_| Jbig2Error::Overflow("refined instance size"))?;
    decoder.take_aligned_bytes(body_bytes)?;
    decoder.align_to_byte()
}

/// Borrow the fixed aggregate tables, which exist whenever aggregation is on.
fn aggregate_tables(tables: &Option<AggregateTables>) -> Result<&AggregateTables> {
    tables
        .as_ref()
        .ok_or(Jbig2Error::InvalidResource("aggregate tables"))
}

/// Read a height class's collective bitmap stored without compression
/// (T.88 section 6.5.9), whose rows are byte aligned.
fn decode_uncompressed_collective(
    decoder: &mut HuffmanDecoder<'_>,
    width: u32,
    height: u32,
    budget: &mut DecodeBudget,
) -> Result<Bitmap> {
    let stride = usize::try_from(width)
        .map_err(|_| Jbig2Error::Overflow("collective bitmap width"))?
        .div_ceil(8);
    let rows = usize::try_from(height).map_err(|_| Jbig2Error::Overflow("collective height"))?;
    let extent = stride
        .checked_mul(rows)
        .ok_or(Jbig2Error::Overflow("collective bitmap storage"))?;
    let bytes = decoder.take_aligned_bytes(extent)?;
    let mut bitmap = Bitmap::new(width, height, false, budget)?;
    for y in 0..height {
        let start = usize::try_from(y)
            .map_err(|_| Jbig2Error::Overflow("collective row"))?
            .checked_mul(stride)
            .ok_or(Jbig2Error::Overflow("collective row"))?;
        let end = start
            .checked_add(stride)
            .ok_or(Jbig2Error::Overflow("collective row"))?;
        let row = bytes
            .get(start..end)
            .ok_or(Jbig2Error::InvalidBitmap("collective bitmap row"))?;
        bitmap.write_row(y, row)?;
    }
    Ok(bitmap)
}

/// Cut a height class's collective bitmap into its symbols, left to right.
fn split_collective(
    collective: BitmapRef<'_>,
    widths: &[u32],
    height: u32,
    budget: &mut DecodeBudget,
) -> Result<Vec<Bitmap>> {
    let mut symbols = Vec::new();
    symbols.try_reserve_exact(widths.len())?;
    let mut offset = 0_u32;
    for &width in widths {
        let mut symbol = Bitmap::new(width, height, false, budget)?;
        for y in 0..height {
            for x in 0..width {
                let source = offset
                    .checked_add(x)
                    .ok_or(Jbig2Error::Overflow("collective offset"))?;
                if collective.pixel(source, y).unwrap_or(false) {
                    symbol.set_pixel(x, y, true)?;
                }
            }
        }
        offset = offset
            .checked_add(width)
            .ok_or(Jbig2Error::Overflow("collective offset"))?;
        symbols.push(symbol);
    }
    Ok(symbols)
}

/// The tables a Huffman text region reads its integer fields with.
pub(crate) struct TextTables<'a> {
    /// `SBHUFFFS`, first symbol S delta.
    pub(crate) first_s: ResolvedTable<'a>,
    /// `SBHUFFDS`, subsequent symbol S delta.
    pub(crate) delta_s: ResolvedTable<'a>,
    /// `SBHUFFDT`, strip T delta.
    pub(crate) delta_t: ResolvedTable<'a>,
    /// `SBHUFFRDW`, refinement width delta.
    pub(crate) refine_width: ResolvedTable<'a>,
    /// `SBHUFFRDH`, refinement height delta.
    pub(crate) refine_height: ResolvedTable<'a>,
    /// `SBHUFFRDX`, refinement horizontal displacement.
    pub(crate) refine_x: ResolvedTable<'a>,
    /// `SBHUFFRDY`, refinement vertical displacement.
    pub(crate) refine_y: ResolvedTable<'a>,
    /// `SBHUFFRSIZE`, refined instance body size.
    pub(crate) refine_size: ResolvedTable<'a>,
    /// The run-coded symbol identifier table read from this region's payload.
    pub(crate) identifiers: HuffmanTable,
}

/// Huffman decoding state for one text region segment.
pub(crate) struct HuffmanTextSource<'a, 't> {
    decoder: HuffmanDecoder<'a>,
    tables: &'t TextTables<'t>,
    refinement: Option<RefinementState>,
    /// Bits a within-strip T offset occupies, `LOGSBSTRIPS`.
    strip_bits: u8,
    refine_enabled: bool,
}

impl<'a, 't> HuffmanTextSource<'a, 't> {
    /// Continue from the decoder that read the symbol identifier table.
    pub(crate) fn new(
        decoder: HuffmanDecoder<'a>,
        tables: &'t TextTables<'t>,
        refinement_template: Option<RefinementTemplate>,
        strip_bits: u8,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        let refinement = match refinement_template {
            Some(template) => Some(RefinementState {
                contexts: ArithmeticContexts::new(template.id().context_count(), budget)?,
                template,
            }),
            None => None,
        };
        Ok(Self {
            decoder,
            tables,
            refine_enabled: refinement.is_some(),
            refinement,
            strip_bits,
        })
    }

    /// Decode one integer field, rejecting an out-of-band result.
    fn integer(
        &mut self,
        table: &HuffmanTable,
        what: &'static str,
        budget: &mut DecodeBudget,
    ) -> Result<i32> {
        i32::try_from(self.decoder.decode_integer(table, budget)?.value(what)?)
            .map_err(|_| Jbig2Error::Overflow(what))
    }
}

impl TextSymbolSource for HuffmanTextSource<'_, '_> {
    fn strip_delta_t(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        let table = self.tables.delta_t.table();
        self.integer(table, "strip delta", budget)
    }

    fn first_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        let table = self.tables.first_s.table();
        self.integer(table, "first S delta", budget)
    }

    fn next_delta_s(&mut self, budget: &mut DecodeBudget) -> Result<Option<i32>> {
        let table = self.tables.delta_s.table();
        match self.decoder.decode_integer(table, budget)? {
            crate::arithmetic::IntegerValue::OutOfBand => Ok(None),
            value => i32::try_from(value.value("S delta")?)
                .map(Some)
                .map_err(|_| Jbig2Error::Overflow("S delta")),
        }
    }

    fn current_t(&mut self, budget: &mut DecodeBudget) -> Result<i32> {
        i32::try_from(self.decoder.read_bits(self.strip_bits, budget)?)
            .map_err(|_| Jbig2Error::Overflow("strip T offset"))
    }

    fn symbol_id(&mut self, budget: &mut DecodeBudget) -> Result<u32> {
        let id = self
            .decoder
            .decode_integer(&self.tables.identifiers, budget)?
            .value("symbol identifier")?;
        u32::try_from(id).map_err(|_| Jbig2Error::InvalidResource("symbol identifier"))
    }

    fn refine_flag(&mut self, budget: &mut DecodeBudget) -> Result<bool> {
        if !self.refine_enabled {
            return Ok(false);
        }
        Ok(self.decoder.read_bits(1, budget)? != 0)
    }

    fn refine(&mut self, reference: BitmapRef<'_>, budget: &mut DecodeBudget) -> Result<Bitmap> {
        let tables = RefinementTables {
            width: self.tables.refine_width.table(),
            height: self.tables.refine_height.table(),
            x: self.tables.refine_x.table(),
            y: self.tables.refine_y.table(),
            size: self.tables.refine_size.table(),
        };
        let state = self
            .refinement
            .as_mut()
            .ok_or(Jbig2Error::InvalidResource("refinement template"))?;
        decode_refined_instance(&mut self.decoder, state, &tables, reference, budget)
    }
}
