use crate::{
    Jbig2Error, Result,
    arithmetic::IntegerValue,
    cursor::{BitCursor, ByteCursor},
    document::DecodeBudget,
    error::{LimitKind, SourceLocation},
    segment::{Parsed, SegmentNumber},
};

/// Identifies one of the fifteen standard tables in T.88 Annex B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardTable {
    /// Table B.1.
    B1,
    /// Table B.2.
    B2,
    /// Table B.3.
    B3,
    /// Table B.4.
    B4,
    /// Table B.5.
    B5,
    /// Table B.6.
    B6,
    /// Table B.7.
    B7,
    /// Table B.8.
    B8,
    /// Table B.9.
    B9,
    /// Table B.10.
    B10,
    /// Table B.11.
    B11,
    /// Table B.12.
    B12,
    /// Table B.13.
    B13,
    /// Table B.14.
    B14,
    /// Table B.15.
    B15,
}

/// A table selection in a region or dictionary header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSelection {
    /// A built-in Annex B table.
    Standard(StandardTable),
    /// A custom code-table segment resolved through the resource resolver.
    Custom(SegmentNumber),
}

/// One canonical Huffman code's range semantics, including lower ranges and OOB.
#[derive(Debug, Clone, Copy)]
pub enum HuffmanRange {
    /// Nonnegative offset added to a signed base.
    Upper {
        /// Starting value for the range.
        base: i64,
        /// Number of following offset bits.
        bits: u8,
    },
    /// Nonnegative offset subtracted from a signed base.
    Lower {
        /// Highest value in the range.
        base: i64,
        /// Number of following offset bits.
        bits: u8,
    },
    /// Explicit out-of-band integer result.
    OutOfBand,
}

/// Canonical code description before table validation/building.
#[derive(Debug, Clone, Copy)]
pub struct HuffmanLine {
    /// Length of the canonical prefix in bits.
    pub prefix_bits: u8,
    /// Interpretation of the range bits following that prefix.
    pub range: HuffmanRange,
}

/// One assigned canonical code and the range it selects.
#[derive(Debug, Clone, Copy)]
struct AssignedCode {
    prefix_bits: u8,
    code: u32,
    range: HuffmanRange,
}

/// Owned validated code table; independent of the encoded input lifetime.
///
/// Codes are stored sorted by prefix width and then by code value, so decoding
/// looks each candidate up directly instead of rescanning every line.
#[derive(Debug)]
pub struct HuffmanTable {
    codes: Vec<AssignedCode>,
}

impl HuffmanTable {
    /// Build a standard table while charging any required storage.
    pub fn standard(table: StandardTable, budget: &mut DecodeBudget) -> Result<Self> {
        let (has_oob, rows) = standard_rows(table);
        let mut lines = Vec::new();
        lines.try_reserve_exact(rows.len())?;
        let last = rows.len().saturating_sub(1);
        for (index, &row) in rows.iter().enumerate() {
            lines.push(HuffmanLine {
                prefix_bits: row.0,
                range: standard_range(row, index == last, has_oob),
            });
        }
        Self::from_lines(&lines, budget)
    }

    /// Validate and own custom lines, rejecting conflicting or invalid prefixes.
    pub fn from_lines(lines: &[HuffmanLine], budget: &mut DecodeBudget) -> Result<Self> {
        let counts = count_prefix_widths(lines)?;
        let mut first = canonical_first_codes(&counts)?;
        budget.charge(LimitKind::AllocationBytes, table_allocation(lines.len())?)?;
        let mut codes = Vec::new();
        codes.try_reserve_exact(lines.len())?;
        for line in lines {
            // A zero prefix width marks a line the table does not use; it is
            // assigned no code and can never be decoded.
            if line.prefix_bits == 0 {
                continue;
            }
            let next = first
                .get_mut(usize::from(line.prefix_bits))
                .ok_or(Jbig2Error::Overflow("Huffman prefix"))?;
            let code = u32::try_from(*next).map_err(|_| Jbig2Error::Overflow("Huffman code"))?;
            *next = next
                .checked_add(1)
                .ok_or(Jbig2Error::Overflow("Huffman code"))?;
            codes.push(AssignedCode {
                prefix_bits: line.prefix_bits,
                code,
                range: line.range,
            });
        }
        codes.sort_unstable_by_key(|assigned| (assigned.prefix_bits, assigned.code));
        Ok(Self { codes })
    }

    /// Look up the range a completed prefix code selects.
    fn lookup(&self, prefix_bits: u8, code: u32) -> Option<HuffmanRange> {
        self.codes
            .binary_search_by_key(&(prefix_bits, code), |assigned| {
                (assigned.prefix_bits, assigned.code)
            })
            .ok()
            .and_then(|index| self.codes.get(index))
            .map(|assigned| assigned.range)
    }

    /// Parse a code-table segment payload into an owned, validated table.
    pub fn parse<'a>(
        input: &'a [u8],
        location: SourceLocation,
        budget: &mut DecodeBudget,
    ) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(input, location);
        let header = CustomTableHeader::parse(&mut cursor)?;
        let body = cursor.remaining();
        let mut decoder = HuffmanDecoder::new(body, cursor.location());
        let lines = header.read_lines(&mut decoder, budget)?;
        let consumed = cursor
            .position()
            .checked_add(decoder.finish()?.div_ceil(8))
            .ok_or(Jbig2Error::Overflow("Huffman table length"))?;
        Ok(Parsed {
            value: Self::from_lines(&lines, budget)?,
            remaining: input
                .get(consumed..)
                .ok_or(Jbig2Error::InvalidLength(location))?,
            consumed,
        })
    }
}

/// Fixed fields of a code table segment (T.88 Annex B.2).
#[derive(Debug, Clone, Copy)]
struct CustomTableHeader {
    /// Bit width of each line's prefix length field.
    prefix_width: u8,
    /// Bit width of each line's range length field.
    range_width: u8,
    /// Lowest value the table's explicit ranges cover.
    low: i32,
    /// One past the highest value the table's explicit ranges cover.
    high: i32,
    /// Whether the table defines an out-of-band code.
    has_out_of_band: bool,
}

impl CustomTableHeader {
    /// Read and validate the nine fixed header bytes.
    fn parse(cursor: &mut ByteCursor<'_>) -> Result<Self> {
        let flags_location = cursor.location();
        let flags = cursor.u8()?;
        if flags & 0x80 != 0 {
            return Err(Jbig2Error::ReservedField {
                location: flags_location,
                field: "Huffman table flags",
            });
        }
        let low = cursor.i32()?;
        let high = cursor.i32()?;
        if low > high {
            return Err(Jbig2Error::InvalidCodingData {
                location: flags_location,
                reason: "custom Huffman range",
            });
        }
        Ok(Self {
            prefix_width: ((flags >> 1) & 7).saturating_add(1),
            range_width: ((flags >> 4) & 7).saturating_add(1),
            low,
            high,
            has_out_of_band: flags & 1 != 0,
        })
    }

    /// Read every coded line: the explicit ranges covering `low..high`, the two
    /// open ranges below and above them, and the optional out-of-band code.
    fn read_lines(
        self,
        decoder: &mut HuffmanDecoder<'_>,
        budget: &mut DecodeBudget,
    ) -> Result<Vec<HuffmanLine>> {
        let mut lines = Vec::new();
        let mut current = i64::from(self.low);
        while current < i64::from(self.high) {
            budget.charge(LimitKind::Work, 1)?;
            let prefix_bits = self.read_width(decoder, self.prefix_width, budget)?;
            let bits = self.read_width(decoder, self.range_width, budget)?;
            if bits >= 31 {
                return Err(Jbig2Error::InvalidResource("custom Huffman range width"));
            }
            lines.try_reserve(1)?;
            lines.push(HuffmanLine {
                prefix_bits,
                range: HuffmanRange::Upper {
                    base: current,
                    bits,
                },
            });
            current = current
                .checked_add(1_i64 << bits)
                .ok_or(Jbig2Error::Overflow("custom Huffman range"))?;
        }
        for range in [
            HuffmanRange::Lower {
                base: i64::from(self.low).saturating_sub(1),
                bits: 32,
            },
            HuffmanRange::Upper {
                base: i64::from(self.high),
                bits: 32,
            },
        ] {
            let prefix_bits = self.read_width(decoder, self.prefix_width, budget)?;
            lines.try_reserve(1)?;
            lines.push(HuffmanLine { prefix_bits, range });
        }
        if self.has_out_of_band {
            let prefix_bits = self.read_width(decoder, self.prefix_width, budget)?;
            lines.try_reserve(1)?;
            lines.push(HuffmanLine {
                prefix_bits,
                range: HuffmanRange::OutOfBand,
            });
        }
        Ok(lines)
    }

    /// Read one width field, which never exceeds a code or range width.
    fn read_width(
        self,
        decoder: &mut HuffmanDecoder<'_>,
        width: u8,
        budget: &mut DecodeBudget,
    ) -> Result<u8> {
        u8::try_from(decoder.read_bits(width, budget)?)
            .map_err(|_| Jbig2Error::Overflow("Huffman width field"))
    }
}

/// Bit reader over bounded Huffman-coded bytes; tables are borrowed per call.
/// Input storage must remain valid across any foreign adapter's decoder handle.
#[derive(Debug)]
pub struct HuffmanDecoder<'a> {
    cursor: BitCursor<'a>,
}

impl<'a> HuffmanDecoder<'a> {
    /// Create a decoder at the start of a bounded coded payload.
    pub fn new(input: &'a [u8], location: SourceLocation) -> Self {
        Self {
            cursor: BitCursor::new(input, location),
        }
    }

    /// Decode a signed integer or OOB result; check truncation and work limits.
    ///
    /// Canonical codes are prefix free, so reading one bit at a time and
    /// testing the accumulated value against this width's codes terminates at
    /// the first complete code (T.88 Annex B.4).
    pub fn decode_integer(
        &mut self,
        table: &HuffmanTable,
        budget: &mut DecodeBudget,
    ) -> Result<IntegerValue> {
        let mut code = 0_u32;
        for width in 1..=32_u8 {
            code = (code << 1) | self.read_bits(1, budget)?;
            let Some(range) = table.lookup(width, code) else {
                continue;
            };
            return match range {
                HuffmanRange::OutOfBand => Ok(IntegerValue::OutOfBand),
                HuffmanRange::Upper { base, bits } => {
                    let offset = i64::from(self.read_bits(bits, budget)?);
                    Ok(IntegerValue::Value(
                        base.checked_add(offset)
                            .ok_or(Jbig2Error::Overflow("Huffman value"))?,
                    ))
                }
                HuffmanRange::Lower { base, bits } => {
                    let offset = i64::from(self.read_bits(bits, budget)?);
                    Ok(IntegerValue::Value(
                        base.checked_sub(offset)
                            .ok_or(Jbig2Error::Overflow("Huffman value"))?,
                    ))
                }
            };
        }
        Err(Jbig2Error::InvalidCodingData {
            location: self.cursor.location(),
            reason: "Huffman code",
        })
    }

    /// Read a fixed-width field such as a symbol identifier.
    pub fn read_bits(&mut self, count: u8, budget: &mut DecodeBudget) -> Result<u32> {
        budget.charge(LimitKind::Work, u64::from(count))?;
        self.cursor.bits(count)
    }

    /// Discard bits up to the next byte boundary.
    pub fn align_to_byte(&mut self) -> Result<()> {
        self.cursor.align_to_byte()
    }

    /// Align, then borrow and skip a nested body of `count` bytes.
    pub(crate) fn take_aligned_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        self.cursor.take_aligned_bytes(count)
    }

    /// Borrow the whole bytes spanning an earlier bit range of this payload.
    pub(crate) fn slice_bits(&self, start_bit: usize, end_bit: usize) -> Result<&'a [u8]> {
        self.cursor.slice_bits(start_bit, end_bit)
    }

    /// Resume reading at an absolute bit position within this payload.
    pub(crate) fn seek_to_bit(&mut self, bit_position: usize) {
        self.cursor.seek_to_bit(bit_position);
    }

    /// Locate the next unread bit, for diagnostics and nested decoders.
    pub(crate) fn location(&self) -> SourceLocation {
        self.cursor.location()
    }

    /// Return the number of bits consumed so far.
    pub fn bit_position(&self) -> usize {
        self.cursor.bit_position()
    }

    /// Validate completion and return consumed bits, retaining sub-byte precision.
    pub fn finish(self) -> Result<usize> {
        Ok(self.cursor.bit_position())
    }
}

/// Count how many lines use each prefix width, validating every width.
fn count_prefix_widths(lines: &[HuffmanLine]) -> Result<[u64; PREFIX_WIDTHS]> {
    if lines.is_empty() {
        return Err(Jbig2Error::InvalidResource("empty Huffman table"));
    }
    let mut counts = [0_u64; PREFIX_WIDTHS];
    for line in lines {
        if line.prefix_bits > 32 {
            return Err(Jbig2Error::InvalidResource("Huffman prefix width"));
        }
        let range_bits = match line.range {
            HuffmanRange::Upper { bits, .. } | HuffmanRange::Lower { bits, .. } => bits,
            HuffmanRange::OutOfBand => 0,
        };
        if range_bits > 32 {
            return Err(Jbig2Error::InvalidResource("Huffman range width"));
        }
        if line.prefix_bits != 0
            && let Some(count) = counts.get_mut(usize::from(line.prefix_bits))
        {
            *count = count.saturating_add(1);
        }
    }
    if counts.iter().skip(1).all(|count| *count == 0) {
        return Err(Jbig2Error::InvalidResource("Huffman table has no codes"));
    }
    Ok(counts)
}

/// Compute the first canonical code of each prefix width, rejecting a code set
/// that claims more codes than its width can represent (T.88 Annex B.3).
fn canonical_first_codes(counts: &[u64; PREFIX_WIDTHS]) -> Result<[u64; PREFIX_WIDTHS]> {
    let mut first = [0_u64; PREFIX_WIDTHS];
    let mut code = 0_u64;
    for width in 1..=32_usize {
        code = code
            .checked_add(
                *counts
                    .get(width.saturating_sub(1))
                    .ok_or(Jbig2Error::Overflow("Huffman count"))?,
            )
            .and_then(|value| value.checked_mul(2))
            .ok_or(Jbig2Error::Overflow("Huffman code"))?;
        let used = counts
            .get(width)
            .ok_or(Jbig2Error::Overflow("Huffman count"))?;
        if code
            .checked_add(*used)
            .is_none_or(|end| end > (1_u64 << width))
        {
            return Err(Jbig2Error::InvalidResource("oversubscribed Huffman codes"));
        }
        if let Some(slot) = first.get_mut(width) {
            *slot = code;
        }
    }
    Ok(first)
}

/// Bytes a table of `lines` lines reserves for its owned code storage.
fn table_allocation(lines: usize) -> Result<u64> {
    let bytes = lines
        .checked_mul(size_of::<AssignedCode>())
        .ok_or(Jbig2Error::Overflow("Huffman table allocation"))?;
    u64::try_from(bytes).map_err(|_| Jbig2Error::Overflow("Huffman table allocation"))
}

/// Prefix widths a canonical table can use, plus the unused zero width.
const PREFIX_WIDTHS: usize = 33;

/// A table selection resolved to something the decoder can read from.
///
/// Standard tables are built on demand and owned here; a custom table is
/// borrowed from the Code Table segment that defined it, so referring to one
/// never copies it.
#[derive(Debug)]
pub enum ResolvedTable<'a> {
    /// A built-in Annex B table, built for this use.
    Standard(HuffmanTable),
    /// A custom table owned by a referred-to Code Table segment.
    Custom(&'a HuffmanTable),
}

impl ResolvedTable<'_> {
    /// Borrow the table itself, whichever kind it is.
    pub fn table(&self) -> &HuffmanTable {
        match self {
            Self::Standard(table) => table,
            Self::Custom(table) => table,
        }
    }
}

/// Build or borrow the table a selection names.
pub fn resolve_table<'a>(
    selection: TableSelection,
    resources: &'a dyn crate::dictionary::ResourceResolver,
    budget: &mut DecodeBudget,
) -> Result<ResolvedTable<'a>> {
    match selection {
        TableSelection::Standard(table) => {
            HuffmanTable::standard(table, budget).map(ResolvedTable::Standard)
        }
        TableSelection::Custom(number) => match resources.resolve(number)? {
            crate::dictionary::DecodedResourceRef::Huffman(table) => {
                Ok(ResolvedTable::Custom(table))
            }
            _ => Err(Jbig2Error::WrongResourceType(number)),
        },
    }
}

/// One standard table row: prefix width, range width, and range base.
///
/// A zero prefix width marks a row the table does not use. A thirty-two bit
/// range with a negative base is the table's lower open range, which counts
/// down from its base; every other row counts up (T.88 Annex B.5).
type StandardRow = (u8, u8, i64);

/// Classify a standard table row into the range it selects.
fn standard_range(row: StandardRow, is_last: bool, has_oob: bool) -> HuffmanRange {
    let (_, bits, base) = row;
    if has_oob && is_last {
        HuffmanRange::OutOfBand
    } else if bits == 32 && base < 0 {
        HuffmanRange::Lower { base, bits }
    } else {
        HuffmanRange::Upper { base, bits }
    }
}

/// Return a standard table's rows and whether its final row is out of band.
fn standard_rows(table: StandardTable) -> (bool, &'static [StandardRow]) {
    match table {
        StandardTable::B1 => (
            false,
            &[
                (1, 4, 0),
                (2, 8, 16),
                (3, 16, 272),
                (0, 32, -1),
                (3, 32, 65_808),
            ],
        ),
        StandardTable::B2 => (
            true,
            &[
                (1, 0, 0),
                (2, 0, 1),
                (3, 0, 2),
                (4, 3, 3),
                (5, 6, 11),
                (0, 32, -1),
                (6, 32, 75),
                (6, 0, 0),
            ],
        ),
        StandardTable::B3 => (
            true,
            &[
                (8, 8, -256),
                (1, 0, 0),
                (2, 0, 1),
                (3, 0, 2),
                (4, 3, 3),
                (5, 6, 11),
                (8, 32, -257),
                (7, 32, 75),
                (6, 0, 0),
            ],
        ),
        StandardTable::B4 => (
            false,
            &[
                (1, 0, 1),
                (2, 0, 2),
                (3, 0, 3),
                (4, 3, 4),
                (5, 6, 12),
                (0, 32, -1),
                (5, 32, 76),
            ],
        ),
        StandardTable::B5 => (
            false,
            &[
                (7, 8, -255),
                (1, 0, 1),
                (2, 0, 2),
                (3, 0, 3),
                (4, 3, 4),
                (5, 6, 12),
                (7, 32, -256),
                (6, 32, 76),
            ],
        ),
        StandardTable::B6 => (
            false,
            &[
                (5, 10, -2048),
                (4, 9, -1024),
                (4, 8, -512),
                (4, 7, -256),
                (5, 6, -128),
                (5, 5, -64),
                (4, 5, -32),
                (2, 7, 0),
                (3, 7, 128),
                (3, 8, 256),
                (4, 9, 512),
                (4, 10, 1024),
                (6, 32, -2049),
                (6, 32, 2048),
            ],
        ),
        StandardTable::B7 => (
            false,
            &[
                (4, 9, -1024),
                (3, 8, -512),
                (4, 7, -256),
                (5, 6, -128),
                (5, 5, -64),
                (4, 5, -32),
                (4, 5, 0),
                (5, 5, 32),
                (5, 6, 64),
                (4, 7, 128),
                (3, 8, 256),
                (3, 9, 512),
                (3, 10, 1024),
                (5, 32, -1025),
                (5, 32, 2048),
            ],
        ),
        StandardTable::B8 => (
            true,
            &[
                (8, 3, -15),
                (9, 1, -7),
                (8, 1, -5),
                (9, 0, -3),
                (7, 0, -2),
                (4, 0, -1),
                (2, 1, 0),
                (5, 0, 2),
                (6, 0, 3),
                (3, 4, 4),
                (6, 1, 20),
                (4, 4, 22),
                (4, 5, 38),
                (5, 6, 70),
                (5, 7, 134),
                (6, 7, 262),
                (7, 8, 390),
                (6, 10, 646),
                (9, 32, -16),
                (9, 32, 1670),
                (2, 0, 0),
            ],
        ),
        StandardTable::B9 => (
            true,
            &[
                (8, 4, -31),
                (9, 2, -15),
                (8, 2, -11),
                (9, 1, -7),
                (7, 1, -5),
                (4, 1, -3),
                (3, 1, -1),
                (3, 1, 1),
                (5, 1, 3),
                (6, 1, 5),
                (3, 5, 7),
                (6, 2, 39),
                (4, 5, 43),
                (4, 6, 75),
                (5, 7, 139),
                (5, 8, 267),
                (6, 8, 523),
                (7, 9, 779),
                (6, 11, 1291),
                (9, 32, -32),
                (9, 32, 3339),
                (2, 0, 0),
            ],
        ),
        StandardTable::B10 => (
            true,
            &[
                (7, 4, -21),
                (8, 0, -5),
                (7, 0, -4),
                (5, 0, -3),
                (2, 2, -2),
                (5, 0, 2),
                (6, 0, 3),
                (7, 0, 4),
                (8, 0, 5),
                (2, 6, 6),
                (5, 5, 70),
                (6, 5, 102),
                (6, 6, 134),
                (6, 7, 198),
                (6, 8, 326),
                (6, 9, 582),
                (6, 10, 1094),
                (7, 11, 2118),
                (8, 32, -22),
                (8, 32, 4166),
                (2, 0, 0),
            ],
        ),
        StandardTable::B11 => (
            false,
            &[
                (1, 0, 1),
                (2, 1, 2),
                (4, 0, 4),
                (4, 1, 5),
                (5, 1, 7),
                (5, 2, 9),
                (6, 2, 13),
                (7, 2, 17),
                (7, 3, 21),
                (7, 4, 29),
                (7, 5, 45),
                (7, 6, 77),
                (0, 32, 0),
                (7, 32, 141),
            ],
        ),
        StandardTable::B12 => (
            false,
            &[
                (1, 0, 1),
                (2, 0, 2),
                (3, 1, 3),
                (5, 0, 5),
                (5, 1, 6),
                (6, 1, 8),
                (7, 0, 10),
                (7, 1, 11),
                (7, 2, 13),
                (7, 3, 17),
                (7, 4, 25),
                (8, 5, 41),
                (0, 32, 0),
                (8, 32, 73),
            ],
        ),
        StandardTable::B13 => (
            false,
            &[
                (1, 0, 1),
                (3, 0, 2),
                (4, 0, 3),
                (5, 0, 4),
                (4, 1, 5),
                (3, 3, 7),
                (6, 1, 15),
                (6, 2, 17),
                (6, 3, 21),
                (6, 4, 29),
                (6, 5, 45),
                (7, 6, 77),
                (0, 32, 0),
                (7, 32, 141),
            ],
        ),
        StandardTable::B14 => (
            false,
            &[
                (3, 0, -2),
                (3, 0, -1),
                (1, 0, 0),
                (3, 0, 1),
                (3, 0, 2),
                (0, 32, -3),
                (0, 32, 3),
            ],
        ),
        StandardTable::B15 => (
            false,
            &[
                (7, 4, -24),
                (6, 2, -8),
                (5, 1, -4),
                (4, 0, -2),
                (3, 0, -1),
                (1, 0, 0),
                (3, 0, 1),
                (4, 0, 2),
                (5, 1, 3),
                (6, 2, 5),
                (7, 4, 9),
                (7, 32, -25),
                (7, 32, 25),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{HuffmanDecoder, HuffmanLine, HuffmanRange, HuffmanTable, StandardTable};
    use crate::{
        Result,
        arithmetic::IntegerValue,
        document::{DecodeBudget, DecodeLimits},
        error::{InputOrigin, SourceLocation},
    };

    fn budget() -> DecodeBudget {
        DecodeBudget::new(DecodeLimits {
            input_bytes: 1000,
            segments: 100,
            references: 100,
            pixels: 100,
            allocation_bytes: 4000,
            symbols: 100,
            work: 1000,
        })
    }

    fn location() -> SourceLocation {
        SourceLocation {
            origin: InputOrigin::Embedded,
            byte_offset: 0,
            bit_offset: None,
        }
    }

    #[test]
    fn standard_table_b1_decodes_small_positive_value() -> Result<()> {
        let mut budget = budget();
        let table = HuffmanTable::standard(StandardTable::B1, &mut budget)?;
        let mut decoder = HuffmanDecoder::new(&[0b0001_1000], location());
        assert_eq!(
            decoder.decode_integer(&table, &mut budget)?,
            IntegerValue::Value(3)
        );
        assert_eq!(decoder.finish()?, 5);
        Ok(())
    }

    #[test]
    fn rejects_oversubscribed_and_truncated_codes() -> Result<()> {
        let mut budget = budget();
        let lines = [HuffmanLine {
            prefix_bits: 1,
            range: HuffmanRange::OutOfBand,
        }; 3];
        assert!(HuffmanTable::from_lines(&lines, &mut budget).is_err());
        let table = HuffmanTable::standard(StandardTable::B1, &mut budget)?;
        assert!(
            HuffmanDecoder::new(&[], location())
                .decode_integer(&table, &mut budget)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn custom_table_header_rejects_invalid_range() {
        let bytes = [0, 0, 0, 0, 2, 0, 0, 0, 1];
        assert!(HuffmanTable::parse(&bytes, location(), &mut budget()).is_err());
    }
}
