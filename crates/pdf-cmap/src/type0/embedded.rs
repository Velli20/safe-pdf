use std::collections::HashMap;

use crate::WritingMode;

use crate::{
    cmap::token::CMapToken, cmap_support::Type0CodeMap, error::CMapError,
    type0::parser::EmbeddedCMapBuilder,
};

/// Parsed data for an embedded Type0 encoding CMap stream.
#[derive(Debug, Clone)]
pub struct EmbeddedCMap {
    /// Inclusive source-code ranges accepted by the CMap.
    pub(super) code_space_ranges: Vec<CodeSpaceRange>,
    /// Sorted, deduplicated source-code widths cached by the parser.
    pub(super) allowed_code_lengths: Vec<usize>,
    /// Explicit source-code-to-CID entries.
    pub(super) cid_chars: HashMap<u32, u16>,
    /// Sequential source-code-to-CID ranges.
    pub(super) cid_ranges: Vec<CidRange>,
    /// Direction declared by `/WMode`.
    pub(super) writing_mode: WritingMode,
}

/// One inclusive code-space interval for a fixed source width.
#[derive(Debug, Clone, Copy)]
pub(super) struct CodeSpaceRange {
    /// Packed big-endian lower bound.
    pub(super) start: u32,
    /// Packed big-endian upper bound.
    pub(super) end: u32,
    /// Source width in bytes.
    pub(super) len: usize,
}

/// One inclusive source range mapped to sequential CIDs.
#[derive(Debug, Clone, Copy)]
pub(super) struct CidRange {
    /// Packed big-endian lower source bound.
    pub(super) start: u32,
    /// Packed big-endian upper source bound.
    pub(super) end: u32,
    /// CID corresponding to `start`.
    pub(super) cid_start: u16,
}

impl EmbeddedCMap {
    /// Returns the writing direction parsed from `/WMode`.
    pub(crate) fn writing_mode(&self) -> WritingMode {
        self.writing_mode
    }
}

impl Type0CodeMap for EmbeddedCMap {
    /// Borrows the cached source widths without allocating per decoded code.
    fn allowed_code_lengths(&self) -> &[usize] {
        &self.allowed_code_lengths
    }

    /// Tests the packed code against ranges of the same byte width.
    fn has_code_space(&self, code: u32, len: usize) -> bool {
        self.code_space_ranges
            .iter()
            .any(|range| range.len == len && range.start <= code && code <= range.end)
    }

    /// Map one parsed character code to its CID.
    fn map_code_to_cid(&self, code: u32) -> Option<u16> {
        if let Some(cid) = self.cid_chars.get(&code) {
            return Some(*cid);
        }

        self.cid_ranges.iter().find_map(|range| {
            if range.start <= code && code <= range.end {
                let offset = code.saturating_sub(range.start);
                let offset = u16::try_from(offset).ok()?;
                range.cid_start.checked_add(offset)
            } else {
                None
            }
        })
    }
}

impl TryFrom<&[u8]> for EmbeddedCMap {
    type Error = CMapError;

    /// Parse an embedded Type0 encoding CMap stream from raw bytes.
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let mut state = EmbeddedCMapBuilder::from(data);

        while let Some(token) = state.parser.next_token_lenient()? {
            match token {
                CMapToken::BeginCodeSpaceRange => {
                    state.parse_codespace_ranges()?;
                }
                CMapToken::BeginBfChar => {
                    state.parse_bf_chars()?;
                }
                CMapToken::BeginBfRange => {
                    state.parse_bf_ranges()?;
                }
                CMapToken::BeginCidChar => {
                    state.parse_cid_chars()?;
                }
                CMapToken::BeginCidRange => {
                    state.parse_cid_ranges()?;
                }
                CMapToken::Name(name) if name.as_slice() == b"WMode" => {
                    let mode = state.parser.expect_integer_token("invalid /WMode value")?;
                    state.writing_mode = WritingMode::from(mode);
                }
                _ => {}
            }
        }

        state.finish()
    }
}
