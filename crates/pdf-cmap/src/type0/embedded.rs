use std::collections::HashMap;

use crate::{
    WritingMode, cmap::token::CMapToken, cmap_support::Type0CodeMap, error::CMapError,
    type0::parser::EmbeddedCMapBuilder,
};

/// Parsed data for an embedded Type0 encoding CMap stream.
#[derive(Debug, Clone)]
pub struct EmbeddedCMap {
    pub(super) code_space_ranges: Vec<CodeSpaceRange>,
    pub(super) allowed_code_lengths: Vec<usize>,
    pub(super) cid_chars: HashMap<u32, u16>,
    pub(super) cid_ranges: Vec<CidRange>,
    pub(super) writing_mode: WritingMode,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CodeSpaceRange {
    pub(super) start: u32,
    pub(super) end: u32,
    pub(super) len: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CidRange {
    pub(super) start: u32,
    pub(super) end: u32,
    pub(super) cid_start: u16,
}

impl EmbeddedCMap {
    pub(crate) fn writing_mode(&self) -> WritingMode {
        self.writing_mode
    }
}

impl Type0CodeMap for EmbeddedCMap {
    fn allowed_code_lengths(&self) -> Vec<usize> {
        self.allowed_code_lengths.clone()
    }

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
                    state.writing_mode = WritingMode::from_integer(mode);
                }
                _ => {}
            }
        }

        state.finish()
    }
}
