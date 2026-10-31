//! Text region header syntax and decoding (T.88 section 7.4.3).
//!
//! Placement itself lives in [`crate::text`], which a symbol dictionary's
//! aggregation path shares. This module parses the region's header, resolves
//! the symbols and tables it refers to, and runs that shared procedure.

use crate::{
    Jbig2Error, Result,
    arithmetic_source::ArithmeticSource,
    bitmap::{Bitmap, CombinationOperator},
    cursor::ByteCursor,
    dictionary::imported_symbols,
    huffman::{
        HuffmanDecoder,
        StandardTable::{B6, B7, B8, B9, B10, B11, B12, B13, B14, B15},
        resolve_table,
    },
    huffman_source::{HuffmanTextSource, TextTables},
    region::{DecodeContext, RegionInfo, TextCoding, TextHuffmanTables, TextRegion},
    segment::{Parsed, Segment, SegmentNumber},
    symbols::CustomTableCursor,
    template::{RefinementTemplate, RefinementTemplateId},
    text::{SymbolSet, TextLayout, decode_text},
};

/// Decode a Huffman-coded text region's strips.
///
/// The symbol identifier table comes first in the payload, so the same decoder
/// that read it continues into the strip data on the byte boundary after it.
fn decode_huffman(
    region: &TextRegion<'_>,
    selections: TextHuffmanTables,
    layout: TextLayout,
    symbols: &SymbolSet<'_>,
    bitmap: &mut Bitmap,
    context: &mut DecodeContext<'_>,
) -> Result<()> {
    let resources = context.resources;
    let mut decoder = HuffmanDecoder::new(region.data, context.location);
    let identifiers = crate::symbol_id_table::parse(&mut decoder, symbols.len(), context.budget)?;
    let tables = TextTables {
        first_s: resolve_table(selections.first_s, resources, context.budget)?,
        delta_s: resolve_table(selections.delta_s, resources, context.budget)?,
        delta_t: resolve_table(selections.delta_t, resources, context.budget)?,
        refine_width: resolve_table(selections.refinement_width, resources, context.budget)?,
        refine_height: resolve_table(selections.refinement_height, resources, context.budget)?,
        refine_x: resolve_table(selections.refinement_x, resources, context.budget)?,
        refine_y: resolve_table(selections.refinement_y, resources, context.budget)?,
        refine_size: resolve_table(selections.refinement_size, resources, context.budget)?,
        identifiers,
    };
    let mut source = HuffmanTextSource::new(
        decoder,
        &tables,
        region.refinement,
        region.log_strip_size,
        context.budget,
    )?;
    decode_text(
        layout,
        symbols,
        &mut source,
        bitmap,
        context.compositor,
        context.budget,
    )
}

/// Text region segment flags (T.88 section 7.4.3.1.1, Table 9).
#[derive(Debug, Clone, Copy)]
struct TextFlags(u16);

impl TextFlags {
    /// Signed values the five-bit `SBDSOFFSET` field encodes.
    const DS_OFFSETS: [i8; 32] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, -16, -15, -14, -13, -12, -11, -10,
        -9, -8, -7, -6, -5, -4, -3, -2, -1,
    ];

    /// Whether integer fields use Huffman rather than arithmetic coding.
    fn huffman(self) -> bool {
        self.0 & 1 != 0
    }

    /// Whether individual instances may refine their symbol.
    fn refine(self) -> bool {
        self.0 & 2 != 0
    }

    /// Encoded reference corner selector.
    fn reference_corner(self) -> u8 {
        field(self.0, 4, 3)
    }

    /// Whether the S and T axes are exchanged.
    fn transposed(self) -> bool {
        self.0 & 0x40 != 0
    }

    /// Operator combining symbols into the region bitmap.
    fn operator(self) -> Result<CombinationOperator> {
        CombinationOperator::try_from(field(self.0, 7, 3))
    }

    /// Initial value of the region's pixels.
    fn default_black(self) -> bool {
        self.0 & 0x200 != 0
    }

    /// Signed offset added to every subsequent-symbol S delta.
    fn delta_s_offset(self) -> i8 {
        Self::DS_OFFSETS
            .get(usize::from(field(self.0, 10, 0x1f)))
            .copied()
            .unwrap_or(0)
    }

    /// Refinement template selector.
    fn refinement_template(self) -> Result<RefinementTemplateId> {
        RefinementTemplateId::try_from(field(self.0, 15, 1))
    }
}

/// Extract a masked field starting at `shift`.
fn field(flags: u16, shift: u16, mask: u16) -> u8 {
    u8::try_from((flags >> shift) & mask).unwrap_or(0)
}

/// Resolve the eight Huffman table selections a text region declares
/// (T.88 section 7.4.3.1.2), in the order custom references are consumed.
fn huffman_tables(encoded: u16, custom: &mut CustomTableCursor<'_>) -> Result<TextHuffmanTables> {
    if encoded & 0x8000 != 0 {
        return Err(Jbig2Error::InvalidResource("text Huffman flags"));
    }
    Ok(TextHuffmanTables {
        first_s: custom.select(field(encoded, 0, 3), &[B6, B7], 3)?,
        delta_s: custom.select(field(encoded, 2, 3), &[B8, B9, B10], 3)?,
        delta_t: custom.select(field(encoded, 4, 3), &[B11, B12, B13], 3)?,
        refinement_width: custom.select(field(encoded, 6, 3), &[B14, B15], 3)?,
        refinement_height: custom.select(field(encoded, 8, 3), &[B14, B15], 3)?,
        refinement_x: custom.select(field(encoded, 10, 3), &[B14, B15], 3)?,
        refinement_y: custom.select(field(encoded, 12, 3), &[B14, B15], 3)?,
        refinement_size: custom.select(
            field(encoded, 14, 1),
            &[crate::huffman::StandardTable::B1],
            1,
        )?,
    })
}

/// Parse a text region segment header.
pub(crate) fn parse_descriptor<'a>(
    segment: Segment<'a>,
    tables: &[SegmentNumber],
) -> Result<Parsed<'a, TextRegion<'a>>> {
    let mut cursor = ByteCursor::new(segment.payload(), segment.payload_location());
    let info = RegionInfo::parse(&mut cursor, segment.header().segment_type)?;
    let flags = TextFlags(cursor.u16()?);
    let mut custom = CustomTableCursor::new(tables);
    let coding = if flags.huffman() {
        TextCoding::Huffman(huffman_tables(cursor.u16()?, &mut custom)?)
    } else {
        TextCoding::Arithmetic
    };
    // Refinement adaptive coordinates appear only for the template that has
    // them, and only when instance refinement is enabled at all.
    let refinement = if flags.refine() {
        Some(RefinementTemplate::parse(
            flags.refinement_template()?,
            &mut cursor,
        )?)
    } else {
        None
    };
    let instances = cursor.u32()?;
    let data = cursor.remaining();
    Ok(Parsed {
        value: TextRegion {
            info,
            coding,
            instances,
            log_strip_size: field(flags.0, 2, 3),
            reference_corner: flags.reference_corner().try_into()?,
            transposed: flags.transposed(),
            default_black: flags.default_black(),
            symbol_operator: flags.operator()?,
            delta_s_offset: flags.delta_s_offset(),
            refinement,
            data,
        },
        remaining: data,
        consumed: cursor.position(),
    })
}

/// Decode a text region into its own bitmap, ready for page composition.
pub(crate) fn decode(region: &TextRegion<'_>, context: &mut DecodeContext<'_>) -> Result<Bitmap> {
    let symbols = SymbolSet::new(imported_symbols(context)?);
    let layout = TextLayout {
        instances: region.instances,
        strip_size: 1_u32 << region.log_strip_size,
        reference_corner: region.reference_corner,
        transposed: region.transposed,
        operator: region.symbol_operator,
        delta_s_offset: region.delta_s_offset,
    };
    let mut bitmap = Bitmap::new(
        region.info.width,
        region.info.height,
        region.default_black,
        context.budget,
    )?;
    // A region that places nothing is its default pixels, even when it
    // refers to no symbols at all.
    if region.instances == 0 {
        return Ok(bitmap);
    }
    if symbols.len() == 0 {
        return Err(Jbig2Error::InvalidCodingData {
            location: context.location,
            reason: "text region without symbols",
        });
    }
    match region.coding {
        TextCoding::Arithmetic => {
            let mut source = ArithmeticSource::for_text(
                region.data,
                region.refinement,
                symbols.len(),
                context.location,
                context.budget,
            )?;
            decode_text(
                layout,
                &symbols,
                &mut source,
                &mut bitmap,
                context.compositor,
                context.budget,
            )?;
        }
        TextCoding::Huffman(selections) => {
            decode_huffman(region, selections, layout, &symbols, &mut bitmap, context)?;
        }
    }
    Ok(bitmap)
}
