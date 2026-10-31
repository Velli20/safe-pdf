use crate::{
    Jbig2Error, Result,
    document::DecodeBudget,
    error::{InputOrigin, LimitKind, SourceLocation},
    segment::{
        Parsed, RegionKind, RegionMode, Segment, SegmentDataLength, SegmentHeader, SegmentType,
    },
};

/// Standalone segment layout from T.88 Annex D; unrelated to seekable I/O traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Organization {
    /// Each segment header immediately precedes its payload.
    Sequential,
    /// All segment headers precede all payloads, in matching order.
    RandomAccess,
}

/// Page count declared in a standalone file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageCount {
    /// The file explicitly declares this number of pages.
    Known(u32),
    /// The decoder discovers the count through page completion markers.
    Unknown,
}

/// Validated standalone file metadata; embedded PDF streams omit this header.
#[derive(Debug, Clone, Copy)]
pub struct FileHeader {
    /// Physical ordering of headers and payloads.
    pub organization: Organization,
    /// Declared or unspecified number of pages.
    pub pages: PageCount,
}

impl FileHeader {
    /// Validate magic bytes, flags, and optional page count without copying.
    pub fn parse(input: &[u8]) -> Result<Parsed<'_, Self>> {
        let location = SourceLocation {
            origin: InputOrigin::Standalone,
            byte_offset: 0,
            bit_offset: None,
        };
        let magic = [0x97, 0x4a, 0x42, 0x32, 0x0d, 0x0a, 0x1a, 0x0a];
        if input.len() < 9 {
            return Err(Jbig2Error::Truncated {
                location,
                needed: 9_usize.saturating_sub(input.len()),
            });
        }
        if input.get(..8) != Some(magic.as_slice()) {
            return Err(Jbig2Error::MalformedHeader {
                location,
                field: "JBIG2 file signature",
            });
        }
        let flags = *input.get(8).ok_or(Jbig2Error::InvalidLength(location))?;
        if flags & 0xfc != 0 {
            return Err(Jbig2Error::ReservedField {
                location: SourceLocation {
                    byte_offset: 8,
                    ..location
                },
                field: "file flags",
            });
        }
        let organization = if flags & 1 == 0 {
            Organization::RandomAccess
        } else {
            Organization::Sequential
        };
        let (pages, consumed) = if flags & 2 != 0 {
            (PageCount::Unknown, 9)
        } else {
            let bytes = input.get(9..13).ok_or_else(|| Jbig2Error::Truncated {
                location: SourceLocation {
                    byte_offset: 9,
                    ..location
                },
                needed: 13_usize.saturating_sub(input.len()),
            })?;
            (
                PageCount::Known(
                    bytes
                        .iter()
                        .fold(0_u32, |value, byte| (value << 8) | u32::from(*byte)),
                ),
                13,
            )
        };
        Ok(Parsed {
            value: Self {
                organization,
                pages,
            },
            remaining: input
                .get(consumed..)
                .ok_or(Jbig2Error::InvalidLength(location))?,
            consumed,
        })
    }
}

/// Complete caller-owned input; no method takes ownership of the encoded bytes.
#[derive(Debug, Clone, Copy)]
pub enum Input<'a> {
    /// Standalone file including its signature and file header.
    Standalone(&'a [u8]),
    /// Header/payload pairs from an embedding format, without a file header.
    Embedded(&'a [u8]),
}

/// Framing cursor over borrowed input.
///
/// Unknown lengths are resolved by coding-aware termination and row counts,
/// never by assuming the remainder belongs to a segment. Iteration emits one
/// error and then terminates. Buffers must outlive every yielded segment, even
/// across WASM adapter calls; no `Read`/`Seek` or incremental input is required.
#[derive(Debug)]
pub struct SegmentStream<'a> {
    input: &'a [u8],
    location: SourceLocation,
    position: usize,
    failed: bool,
}

impl<'a> SegmentStream<'a> {
    /// Parse the container and prepare framing, charging all index allocations.
    /// Returns framing, unsupported-feature, allocation, or resource-limit errors.
    pub fn new(input: Input<'a>, budget: &mut DecodeBudget) -> Result<Self> {
        match input {
            Input::Embedded(bytes) => Self::embedded(bytes, InputOrigin::Embedded, budget),
            Input::Standalone(bytes) => {
                let _ = FileHeader::parse(bytes)?;
                Err(Jbig2Error::UnsupportedFeature(
                    "standalone JBIG2 containers",
                ))
            }
        }
    }

    /// Prepare a PDF globals buffer, assigning globals-relative diagnostics.
    pub fn globals(input: &'a [u8], budget: &mut DecodeBudget) -> Result<Self> {
        Self::embedded(input, InputOrigin::Globals, budget)
    }

    /// Frame the next segment, returning `None` at a valid container boundary.
    /// The same cumulative budget must be supplied throughout this stream's life.
    pub fn next_segment(&mut self, budget: &mut DecodeBudget) -> Result<Option<Segment<'a>>> {
        if self.failed {
            return Ok(None);
        }
        if self.position == self.input.len() {
            return Ok(None);
        }
        let result = self.read_segment(budget);
        if result.is_err() {
            self.failed = true;
        }
        result.map(Some)
    }

    fn embedded(input: &'a [u8], origin: InputOrigin, budget: &mut DecodeBudget) -> Result<Self> {
        budget.charge(
            LimitKind::InputBytes,
            u64::try_from(input.len()).map_err(|_| Jbig2Error::Overflow("input length"))?,
        )?;
        Ok(Self {
            input,
            location: SourceLocation {
                origin,
                byte_offset: 0,
                bit_offset: None,
            },
            position: 0,
            failed: false,
        })
    }

    fn read_segment(&mut self, budget: &mut DecodeBudget) -> Result<Segment<'a>> {
        let location = SourceLocation {
            byte_offset: self.position,
            ..self.location
        };
        let suffix = self
            .input
            .get(self.position..)
            .ok_or(Jbig2Error::InvalidLength(location))?;
        let parsed = SegmentHeader::parse(suffix, location, budget)?;
        let payload_start = self
            .position
            .checked_add(parsed.consumed)
            .ok_or(Jbig2Error::Overflow("segment start"))?;
        let (length, actual_rows) = match parsed.value.data_length {
            SegmentDataLength::Known(length) => (
                usize::try_from(length).map_err(|_| Jbig2Error::Overflow("segment length"))?,
                None,
            ),
            SegmentDataLength::Unknown => {
                let (length, rows) = self.unknown_length(&parsed.value, payload_start)?;
                (length, Some(rows))
            }
        };
        let payload_end = payload_start
            .checked_add(length)
            .ok_or(Jbig2Error::Overflow("segment end"))?;
        let payload_location = SourceLocation {
            byte_offset: payload_start,
            ..self.location
        };
        let payload =
            self.input
                .get(payload_start..payload_end)
                .ok_or_else(|| Jbig2Error::Truncated {
                    location: payload_location,
                    needed: payload_end.saturating_sub(self.input.len()),
                })?;
        self.position = payload_end;
        Ok(Segment::new(
            parsed.value,
            payload,
            payload_location,
            actual_rows,
        ))
    }

    /// Find the extent of an unknown-length payload (T.88 section 7.2.7).
    ///
    /// Only an immediate generic region may omit its length. Its coded data
    /// ends with an end-of-data marker, `00 00` for MMR or `FF AC` for
    /// arithmetic coding, followed by the four-byte count of rows actually
    /// coded. Returns the payload length through that count, and the count.
    fn unknown_length(&self, header: &SegmentHeader<'a>, start: usize) -> Result<(usize, u32)> {
        if !matches!(
            header.segment_type,
            SegmentType::Region {
                kind: RegionKind::Generic,
                mode: RegionMode::Immediate | RegionMode::ImmediateLossless,
            }
        ) {
            return Err(Jbig2Error::InvalidLength(SourceLocation {
                byte_offset: start,
                ..self.location
            }));
        }
        let payload = self.input.get(start..).unwrap_or_default();
        let truncated = || Jbig2Error::Truncated {
            location: SourceLocation {
                byte_offset: start,
                ..self.location
            },
            needed: 1,
        };
        // Region information is 17 bytes and the flags byte follows it.
        let flags = *payload.get(17).ok_or_else(truncated)?;
        let (marker, header_len) = if flags & 1 != 0 {
            ([0x00, 0x00], 18)
        } else if (flags >> 1) & 3 == 0 {
            ([0xff, 0xac], 26)
        } else {
            ([0xff, 0xac], 20)
        };
        let data = payload.get(header_len..).ok_or_else(truncated)?;
        let marker_end = data
            .windows(2)
            .position(|pair| pair == marker)
            .and_then(|index| index.checked_add(header_len)?.checked_add(marker.len()))
            .ok_or_else(truncated)?;
        let count_end = marker_end
            .checked_add(4)
            .ok_or(Jbig2Error::Overflow("segment end"))?;
        let count = payload.get(marker_end..count_end).ok_or_else(truncated)?;
        let rows = u32::from_be_bytes(
            count
                .try_into()
                .map_err(|_| Jbig2Error::Overflow("row count"))?,
        );
        Ok((count_end, rows))
    }
}

#[cfg(test)]
mod tests {
    use super::{FileHeader, Input, Organization, PageCount, SegmentStream};
    use crate::{
        Result,
        document::{DecodeBudget, DecodeLimits},
        error::InputOrigin,
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

    #[test]
    fn file_header_parses_known_and_unknown_page_counts() -> Result<()> {
        let magic = [0x97, 0x4a, 0x42, 0x32, 0x0d, 0x0a, 0x1a, 0x0a];
        let mut known = magic.to_vec();
        known.extend_from_slice(&[1, 0, 0, 0, 2]);
        let parsed = FileHeader::parse(&known)?;
        assert_eq!(parsed.consumed, 13);
        assert_eq!(parsed.value.organization, Organization::Sequential);
        assert_eq!(parsed.value.pages, PageCount::Known(2));
        let mut unknown = magic.to_vec();
        unknown.push(2);
        assert_eq!(FileHeader::parse(&unknown)?.value.pages, PageCount::Unknown);
        assert!(
            known
                .get(..12)
                .is_some_and(|bytes| FileHeader::parse(bytes).is_err())
        );
        Ok(())
    }

    #[test]
    fn embedded_payload_is_bounded_and_error_terminates_stream() -> Result<()> {
        let input = [0, 0, 0, 1, 49, 0, 1, 0, 0, 0, 1, 0xaa, 0xff];
        let mut budget = budget();
        let mut stream = SegmentStream::new(Input::Embedded(&input), &mut budget)?;
        let first = stream
            .next_segment(&mut budget)?
            .ok_or(crate::Jbig2Error::InvalidLength(
                crate::error::SourceLocation {
                    origin: InputOrigin::Embedded,
                    byte_offset: 0,
                    bit_offset: None,
                },
            ))?;
        assert_eq!(first.payload(), &[0xaa]);
        assert_eq!(first.payload_location().byte_offset, 11);
        assert!(stream.next_segment(&mut budget).is_err());
        assert!(stream.next_segment(&mut budget)?.is_none());
        Ok(())
    }
}
