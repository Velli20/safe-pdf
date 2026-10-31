use std::num::NonZeroU32;

use crate::{
    Jbig2Error, Result,
    cursor::ByteCursor,
    document::DecodeBudget,
    error::{LimitKind, SourceLocation},
};

/// T.88 segment identifier, independent of page association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegmentNumber(pub u32);

/// Nonzero page identifier; zero is reserved for global segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageNumber(pub NonZeroU32);

/// Scope selected by a segment header's page association field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageAssociation {
    /// Resource applicable across pages, including PDF globals.
    Global,
    /// Segment associated with one nonzero page number.
    Page(PageNumber),
}

/// Region family encoded by a region segment type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Symbol placement and optional refinement.
    Text,
    /// Pattern-grid halftone reconstruction.
    Halftone,
    /// Arithmetic or MMR pixel reconstruction.
    Generic,
    /// Refinement relative to another bitmap.
    GenericRefinement,
}

/// Whether a region is retained as a resource or composed immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionMode {
    /// Retain the decoded region for later references.
    Intermediate,
    /// Composite a region without a lossless guarantee.
    Immediate,
    /// Composite a region marked lossless by its segment type.
    ImmediateLossless,
}

/// Segment classifications from T.88 section 7.2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentType {
    /// Imported, newly decoded, and exported symbols.
    SymbolDictionary,
    /// A region family and its page-composition behavior.
    Region {
        /// Reconstruction procedure used by the region.
        kind: RegionKind,
        /// Retention or immediate composition behavior.
        mode: RegionMode,
    },
    /// Patterns referenced by halftone regions.
    PatternDictionary,
    /// Page dimensions, defaults, resolution, and striping.
    PageInformation,
    /// Page-completion marker.
    EndOfPage,
    /// Stripe-completion marker carrying a row position.
    EndOfStripe,
    /// Standalone file-completion marker.
    EndOfFile,
    /// Decoder capability profile declaration.
    Profile,
    /// Custom Huffman table definition.
    CodeTable,
    /// Extension metadata, interpreted according to its extension identifier.
    Extension,
    /// Unrecognized six-bit type code preserved for diagnostics.
    Unknown(u8),
}

/// Declared payload extent; unknown length is valid only where T.88 permits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentDataLength {
    /// Explicit payload byte count, excluding the segment header.
    Known(u32),
    /// A generic-region terminator and row count determine the actual extent.
    Unknown,
}

/// Result of parsing a prefix without allocating or copying the remainder.
#[derive(Debug)]
pub struct Parsed<'a, T> {
    /// Validated value parsed from the input prefix.
    pub value: T,
    /// Unconsumed suffix with the original input lifetime.
    pub remaining: &'a [u8],
    /// Number of bytes consumed from the supplied slice.
    pub consumed: usize,
}

/// Validated encoded reference numbers, borrowing their variable-width bytes.
///
/// No alignment is assumed and no typed slice casts are performed. Foreign
/// callers must retain the backing segment buffer for this view's lifetime.
#[derive(Debug, Clone, Copy)]
pub struct ReferredSegments<'a> {
    encoded: &'a [u8],
    width: u8,
    count: u32,
}

impl<'a> ReferredSegments<'a> {
    /// Iterate validated references without allocating a decoded list.
    pub fn iter(self) -> ReferredSegmentIter<'a> {
        ReferredSegmentIter {
            references: self,
            position: 0,
        }
    }

    /// Return the count validated against the encoded byte extent.
    pub fn len(self) -> usize {
        usize::try_from(self.count).unwrap_or(usize::MAX)
    }

    /// Report whether this segment has no dependencies.
    pub fn is_empty(self) -> bool {
        self.count == 0
    }
}

/// Iterator decoding big-endian reference numbers from validated bytes.
#[derive(Debug)]
pub struct ReferredSegmentIter<'a> {
    references: ReferredSegments<'a>,
    position: u32,
}

impl Iterator for ReferredSegmentIter<'_> {
    type Item = SegmentNumber;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.references.count {
            return None;
        }
        let width = usize::from(self.references.width);
        let offset = usize::try_from(self.position).ok()?.checked_mul(width)?;
        let bytes = self
            .references
            .encoded
            .get(offset..offset.checked_add(width)?)?;
        self.position = self.position.checked_add(1)?;
        Some(SegmentNumber(
            bytes
                .iter()
                .fold(0_u32, |value, byte| (value << 8) | u32::from(*byte)),
        ))
    }
}

/// Borrowed short- or long-form retention bits, including the current segment.
#[derive(Debug, Clone, Copy)]
pub struct RetentionFlags<'a> {
    encoded: &'a [u8],
    count: u32,
}

impl RetentionFlags<'_> {
    /// Return a retention flag, or `None` for an index outside the validated set.
    /// Index zero denotes the current segment; later indices follow references.
    pub fn get(self, index: usize) -> Option<bool> {
        if index >= usize::try_from(self.count).ok()? {
            return None;
        }
        self.encoded
            .get(index / 8)
            .map(|byte| byte & (1_u8 << (index % 8)) != 0)
    }
}

/// Parsed header borrowing only encoded reference and retention storage.
///
/// Parsing validates reserved encodings, reference widths, and field extents.
/// It does not resolve dependencies or infer an unknown payload length.
///
/// Header reference views cannot outlive their input:
/// ```compile_fail
/// use pdf_jbig2::{document::DecodeBudget, error::SourceLocation,
///     segment::SegmentHeader};
/// fn escaping(budget: &mut DecodeBudget, location: SourceLocation)
///     -> SegmentHeader<'static> {
///     let bytes = vec![0_u8; 32];
///     SegmentHeader::parse(&bytes, location, budget).unwrap().value
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SegmentHeader<'a> {
    /// Unique segment identifier within this decoding namespace.
    pub number: SegmentNumber,
    /// Payload syntax and processing behavior.
    pub segment_type: SegmentType,
    /// Owning page or global association.
    pub association: PageAssociation,
    /// Encoded dependency numbers.
    pub references: ReferredSegments<'a>,
    /// Retention decisions for this segment and its dependencies.
    pub retention: RetentionFlags<'a>,
    /// Defer non-retention until the applicable end-of-page boundary.
    pub deferred_non_retain: bool,
    /// Declared payload extent, separate from resolved framing.
    pub data_length: SegmentDataLength,
    /// Original header location for diagnostics.
    pub location: SourceLocation,
}

impl<'a> SegmentHeader<'a> {
    /// Parse a header prefix, charging reference/segment limits without copying.
    /// Returns truncation, reserved-field, syntax, overflow, or budget errors.
    pub fn parse(
        input: &'a [u8],
        location: SourceLocation,
        budget: &mut DecodeBudget,
    ) -> Result<Parsed<'a, Self>> {
        let mut cursor = ByteCursor::new(input, location);
        let number = cursor.u32()?;
        let flags = cursor.u8()?;
        let (count, retention) = parse_referred_count(&mut cursor)?;
        let references = parse_references(&mut cursor, number, count)?;
        if references.iter().any(|reference| reference.0 >= number) {
            return Err(Jbig2Error::MalformedHeader {
                location: cursor.location(),
                field: "referred-to segment number",
            });
        }
        let association_width = if flags & 0x40 != 0 { 4 } else { 1 };
        let association = match NonZeroU32::new(cursor.uint(association_width)?) {
            Some(value) => PageAssociation::Page(PageNumber(value)),
            None => PageAssociation::Global,
        };
        let raw_length = cursor.u32()?;
        budget.charge(LimitKind::Segments, 1)?;
        budget.charge(LimitKind::References, u64::from(count))?;
        Ok(Parsed {
            value: Self {
                number: SegmentNumber(number),
                segment_type: segment_type(flags & 0x3f),
                association,
                references,
                retention: RetentionFlags {
                    encoded: retention,
                    count: count
                        .checked_add(1)
                        .ok_or(Jbig2Error::Overflow("retention count"))?,
                },
                deferred_non_retain: flags & 0x80 != 0,
                data_length: if raw_length == u32::MAX {
                    SegmentDataLength::Unknown
                } else {
                    SegmentDataLength::Known(raw_length)
                },
                location,
            },
            remaining: cursor.remaining(),
            consumed: cursor.position(),
        })
    }
}

/// Parse the referred-to count and its retention bits (T.88 section 7.2.4).
///
/// The short form packs a three-bit count and five retention bits into one
/// byte; the long form spends four bytes on the count and then as many whole
/// bytes as the count plus this segment requires.
fn parse_referred_count<'a>(cursor: &mut ByteCursor<'a>) -> Result<(u32, &'a [u8])> {
    let count_location = cursor.location();
    if cursor.peek_u8()? >> 5 != 7 {
        let encoded = cursor.take(1)?;
        let count = u32::from(
            encoded
                .first()
                .ok_or(Jbig2Error::InvalidLength(count_location))?
                >> 5,
        );
        if count > 4 {
            return Err(Jbig2Error::ReservedField {
                location: count_location,
                field: "referred-to count",
            });
        }
        return Ok((count, encoded));
    }
    let count = cursor.u32()? & 0x1fff_ffff;
    let retained = count
        .checked_add(1)
        .ok_or(Jbig2Error::Overflow("retention count"))?;
    let retention_len = usize::try_from(retained.div_ceil(8))
        .map_err(|_| Jbig2Error::Overflow("retention bytes"))?;
    Ok((count, cursor.take(retention_len)?))
}

/// Borrow the referred-to segment numbers, whose width follows this segment's
/// own number (T.88 section 7.2.5).
fn parse_references<'a>(
    cursor: &mut ByteCursor<'a>,
    number: u32,
    count: u32,
) -> Result<ReferredSegments<'a>> {
    let width: u8 = if number <= 256 {
        1
    } else if number <= 65_536 {
        2
    } else {
        4
    };
    let extent = usize::try_from(count)
        .map_err(|_| Jbig2Error::Overflow("reference count"))?
        .checked_mul(usize::from(width))
        .ok_or(Jbig2Error::Overflow("reference extent"))?;
    Ok(ReferredSegments {
        encoded: cursor.take(extent)?,
        width,
        count,
    })
}

/// A framed segment with a bounded payload slice, never an unresolved suffix.
#[derive(Debug, Clone, Copy)]
pub struct Segment<'a> {
    header: SegmentHeader<'a>,
    payload: &'a [u8],
    payload_location: SourceLocation,
    actual_rows: Option<u32>,
}

impl<'a> Segment<'a> {
    pub(crate) fn new(
        header: SegmentHeader<'a>,
        payload: &'a [u8],
        payload_location: SourceLocation,
        actual_rows: Option<u32>,
    ) -> Self {
        Self {
            header,
            payload,
            payload_location,
            actual_rows,
        }
    }

    /// Inspect the header without tying its input borrows to this temporary borrow.
    pub fn header(&self) -> &SegmentHeader<'a> {
        &self.header
    }

    /// Borrow the payload for the original input lifetime.
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }

    /// Locate the first payload byte in its original buffer.
    pub fn payload_location(&self) -> SourceLocation {
        self.payload_location
    }

    /// Return the actual row count recovered from an unknown-length terminator.
    pub fn actual_rows(&self) -> Option<u32> {
        self.actual_rows
    }
}

fn segment_type(code: u8) -> SegmentType {
    use RegionKind::{Generic, GenericRefinement, Halftone, Text};
    use RegionMode::{Immediate, ImmediateLossless, Intermediate};
    let region = match code {
        4 => Some((Text, Intermediate)),
        6 => Some((Text, Immediate)),
        7 => Some((Text, ImmediateLossless)),
        20 => Some((Halftone, Intermediate)),
        22 => Some((Halftone, Immediate)),
        23 => Some((Halftone, ImmediateLossless)),
        36 => Some((Generic, Intermediate)),
        38 => Some((Generic, Immediate)),
        39 => Some((Generic, ImmediateLossless)),
        40 => Some((GenericRefinement, Intermediate)),
        42 => Some((GenericRefinement, Immediate)),
        43 => Some((GenericRefinement, ImmediateLossless)),
        _ => None,
    };
    if let Some((kind, mode)) = region {
        return SegmentType::Region { kind, mode };
    }
    match code {
        0 => SegmentType::SymbolDictionary,
        16 => SegmentType::PatternDictionary,
        48 => SegmentType::PageInformation,
        49 => SegmentType::EndOfPage,
        50 => SegmentType::EndOfStripe,
        51 => SegmentType::EndOfFile,
        52 => SegmentType::Profile,
        53 => SegmentType::CodeTable,
        62 => SegmentType::Extension,
        _ => SegmentType::Unknown(code),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PageAssociation, RegionKind, RegionMode, SegmentHeader, SegmentNumber, SegmentType,
    };
    use crate::{
        Result,
        document::{DecodeBudget, DecodeLimits},
        error::{InputOrigin, SourceLocation},
    };

    fn budget() -> DecodeBudget {
        DecodeBudget::new(DecodeLimits {
            input_bytes: 1000,
            segments: 100,
            references: 100,
            pixels: 100,
            allocation_bytes: 1000,
            symbols: 100,
            work: 100,
        })
    }

    fn location() -> SourceLocation {
        SourceLocation {
            origin: InputOrigin::Embedded,
            byte_offset: 7,
            bit_offset: None,
        }
    }

    #[test]
    fn short_header_borrows_references_and_retention() -> Result<()> {
        let data = [0, 0, 0, 3, 38, 0b0100_0101, 1, 2, 1, 0, 0, 0, 4, 99];
        let parsed = SegmentHeader::parse(&data, location(), &mut budget())?;
        assert_eq!(parsed.consumed, 13);
        assert_eq!(parsed.remaining, &[99]);
        assert_eq!(
            parsed.value.references.iter().collect::<Vec<_>>(),
            [SegmentNumber(1), SegmentNumber(2)]
        );
        assert_eq!(parsed.value.retention.get(0), Some(true));
        assert_eq!(parsed.value.retention.get(1), Some(false));
        assert_eq!(parsed.value.retention.get(2), Some(true));
        assert_eq!(parsed.value.retention.get(3), None);
        assert!(matches!(parsed.value.association, PageAssociation::Page(_)));
        assert!(matches!(
            parsed.value.segment_type,
            SegmentType::Region {
                kind: RegionKind::Generic,
                mode: RegionMode::Immediate
            }
        ));
        Ok(())
    }

    #[test]
    fn rejects_truncated_and_forward_reference() {
        let data = [0, 0, 0, 3, 38, 0b0010_0011, 3, 1, 0, 0, 0, 0];
        assert!(SegmentHeader::parse(&data, location(), &mut budget()).is_err());
        for len in 0..data.len() {
            if let Some(prefix) = data.get(..len) {
                assert!(SegmentHeader::parse(prefix, location(), &mut budget()).is_err());
            }
        }
    }
}
