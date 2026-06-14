use std::collections::{BTreeSet, HashMap};

use crate::{
    WritingMode,
    cmap::{parser::CMapParser, token::CMapToken},
    error::CMapError,
};

use super::{
    EmbeddedCMap,
    embedded::{CidRange, CodeSpaceRange},
};

/// Mutable parser state for building an embedded Type0 encoding CMap.
pub(crate) struct EmbeddedCMapBuilder<'a> {
    pub code_space_ranges: Vec<CodeSpaceRange>,
    pub code_lengths: BTreeSet<usize>,
    pub cid_chars: HashMap<u32, u16>,
    pub cid_ranges: Vec<CidRange>,
    pub writing_mode: WritingMode,
    pub parser: CMapParser<'a>,
}

/// The embedded CMap section currently being parsed.
#[derive(Debug, Clone, Copy)]
enum EmbeddedCMapSection {
    CodeSpaceRange,
    BfChar,
    BfRange,
    CidChar,
    CidRange,
}

/// Incremental parser state for the current embedded CMap section.
#[derive(Debug)]
enum EmbeddedCMapSectionState {
    Start,
    CodeSpaceEnd { start: u32, len: usize },
    BfCharDestination { code: u32 },
    BfRangeEnd { start: u32 },
    BfRangeDestination { start: u32, end: u32 },
    CidCharDestination { code: u32 },
    CidRangeEnd { start: u32 },
    CidRangeDestination { start: u32, end: u32 },
}

impl<'a> From<&'a [u8]> for EmbeddedCMapBuilder<'a> {
    /// Create a new embedded CMap parser from raw CMap bytes.
    ///
    /// # Parameters
    ///
    /// - `data`: Raw embedded CMap stream bytes.
    ///
    /// # Returns
    ///
    /// A builder initialized with empty ranges and mappings and a parser over
    /// `data`.
    fn from(data: &'a [u8]) -> Self {
        Self {
            code_space_ranges: Vec::new(),
            code_lengths: BTreeSet::new(),
            cid_chars: HashMap::new(),
            cid_ranges: Vec::new(),
            writing_mode: WritingMode::Horizontal,
            parser: CMapParser::from(data),
        }
    }
}

impl EmbeddedCMapBuilder<'_> {
    /// Finalize the builder into an `EmbeddedCMap`.
    ///
    /// # Parameters
    ///
    /// None.
    ///
    /// # Returns
    ///
    /// The parsed embedded CMap, or an error if no code space ranges were
    /// parsed.
    pub(crate) fn finish(self) -> Result<EmbeddedCMap, CMapError> {
        if self.code_space_ranges.is_empty() {
            return Err(CMapError::InvalidType0EncodingCMap(
                "missing codespace ranges".to_string(),
            ));
        }

        Ok(EmbeddedCMap {
            code_space_ranges: self.code_space_ranges,
            allowed_code_lengths: self.code_lengths.into_iter().collect(),
            cid_chars: self.cid_chars,
            cid_ranges: self.cid_ranges,
            writing_mode: self.writing_mode,
        })
    }

    /// Parse a `begincodespacerange` section.
    ///
    /// # Parameters
    ///
    /// - `None`: The section is parsed from the builder's internal token
    ///   stream; `begincodespacerange` must already be consumed.
    ///
    /// # Returns
    ///
    /// `Ok(())` after consuming `endcodespacerange`, or an error if a range is
    /// malformed or the section is not closed.
    pub(crate) fn parse_codespace_ranges(&mut self) -> Result<(), CMapError> {
        self.parse_section(EmbeddedCMapSection::CodeSpaceRange)
    }

    /// Parse a `beginbfchar` section.
    ///
    /// # Returns
    ///
    /// `Ok(())` after consuming `endbfchar`, or an error if a mapping is
    /// malformed or the section is not closed.
    pub(crate) fn parse_bf_chars(&mut self) -> Result<(), CMapError> {
        self.parse_section(EmbeddedCMapSection::BfChar)
    }

    /// Parse a `beginbfrange` section.
    ///
    /// # Parameters
    ///
    /// - `None`: Range data is appended to the builder's internal CID range
    ///   list. `beginbfrange` must already be consumed.
    ///
    /// # Returns
    ///
    /// `Ok(())` after consuming `endbfrange`, or an error if a range is
    /// malformed or the section is not closed.
    pub(crate) fn parse_bf_ranges(&mut self) -> Result<(), CMapError> {
        self.parse_section(EmbeddedCMapSection::BfRange)
    }

    /// Parse a `begincidchar` section.
    ///
    /// # Parameters
    ///
    /// - `None`: Mappings are inserted into the builder's internal CID map.
    ///   `begincidchar` must already be consumed.
    ///
    /// # Returns
    ///
    /// `Ok(())` after consuming `endcidchar`, or an error if a mapping is
    /// malformed or the section is not closed.
    pub(crate) fn parse_cid_chars(&mut self) -> Result<(), CMapError> {
        self.parse_section(EmbeddedCMapSection::CidChar)
    }

    /// Parse a `begincidrange` section.
    ///
    /// # Parameters
    ///
    /// - `None`: Range data is appended to the builder's internal CID range
    ///   list. `begincidrange` must already be consumed.
    ///
    /// # Returns
    ///
    /// `Ok(())` after consuming `endcidrange`, or an error if a range is
    /// malformed or the section is not closed.
    pub(crate) fn parse_cid_ranges(&mut self) -> Result<(), CMapError> {
        self.parse_section(EmbeddedCMapSection::CidRange)
    }

    /// Parse one complete embedded CMap section.
    ///
    /// # Parameters
    ///
    /// - `section`: The section type currently being parsed.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the matching end token is reached in the start state, or
    /// an error if the section ends prematurely or contains malformed tokens.
    fn parse_section(&mut self, section: EmbeddedCMapSection) -> Result<(), CMapError> {
        let mut state = EmbeddedCMapSectionState::Start;

        loop {
            match self.parser.next_token()? {
                Some(token)
                    if token == section.end_token()
                        && matches!(state, EmbeddedCMapSectionState::Start) =>
                {
                    return Ok(());
                }
                Some(token) => {
                    state = self.parse_section_token(section, state, token)?;
                }
                None => {
                    return Err(CMapError::InvalidType0EncodingCMap(
                        section.missing_end_message().to_string(),
                    ));
                }
            }
        }
    }

    /// Consume one token within an embedded CMap section.
    ///
    /// # Parameters
    ///
    /// - `section`: The active section.
    /// - `state`: The current parser state for that section.
    /// - `token`: The next parsed CMap token.
    ///
    /// # Returns
    ///
    /// The next parser state, or an error if the token does not match the
    /// section grammar.
    fn parse_section_token(
        &mut self,
        section: EmbeddedCMapSection,
        state: EmbeddedCMapSectionState,
        token: CMapToken,
    ) -> Result<EmbeddedCMapSectionState, CMapError> {
        match state {
            EmbeddedCMapSectionState::Start => self.parse_section_start(section, token),
            EmbeddedCMapSectionState::CodeSpaceEnd { start, len } => {
                let end = token.u32_from_bytes()?;
                self.code_lengths.insert(len);
                self.code_space_ranges
                    .push(CodeSpaceRange { start, end, len });
                Ok(EmbeddedCMapSectionState::Start)
            }
            EmbeddedCMapSectionState::BfCharDestination { code } => {
                let cid = token.u16_from_bytes()?;
                self.cid_chars.insert(code, cid);
                Ok(EmbeddedCMapSectionState::Start)
            }
            EmbeddedCMapSectionState::BfRangeEnd { start } => {
                let end = token.u32_from_bytes()?;
                Ok(EmbeddedCMapSectionState::BfRangeDestination { start, end })
            }
            EmbeddedCMapSectionState::BfRangeDestination { start, end } => {
                let cid_start = token.u16_from_bytes()?;
                self.cid_ranges.push(CidRange {
                    start,
                    end,
                    cid_start,
                });
                Ok(EmbeddedCMapSectionState::Start)
            }
            EmbeddedCMapSectionState::CidCharDestination { code } => {
                let cid = token.u16_from_integer()?;
                self.cid_chars.insert(code, cid);
                Ok(EmbeddedCMapSectionState::Start)
            }
            EmbeddedCMapSectionState::CidRangeEnd { start } => {
                let end = token.u32_from_bytes()?;
                Ok(EmbeddedCMapSectionState::CidRangeDestination { start, end })
            }
            EmbeddedCMapSectionState::CidRangeDestination { start, end } => {
                let cid_start = token.u16_from_integer()?;
                self.cid_ranges.push(CidRange {
                    start,
                    end,
                    cid_start,
                });
                Ok(EmbeddedCMapSectionState::Start)
            }
        }
    }

    /// Begin parsing a new section entry from its first token.
    ///
    /// # Parameters
    ///
    /// - `section`: The active section.
    /// - `token`: The token that begins the entry.
    ///
    /// # Returns
    ///
    /// The next parser state for the section, or an error if the token is not
    /// valid for the section.
    fn parse_section_start(
        &self,
        section: EmbeddedCMapSection,
        token: CMapToken,
    ) -> Result<EmbeddedCMapSectionState, CMapError> {
        let value = token.u32_from_bytes()?;

        match section {
            EmbeddedCMapSection::CodeSpaceRange => {
                let CMapToken::HexString(bytes) = token else {
                    return Err(CMapError::InvalidCMapU32Bytes);
                };

                let len = bytes.len();
                Ok(EmbeddedCMapSectionState::CodeSpaceEnd { start: value, len })
            }
            EmbeddedCMapSection::BfChar => {
                Ok(EmbeddedCMapSectionState::BfCharDestination { code: value })
            }
            EmbeddedCMapSection::BfRange => {
                Ok(EmbeddedCMapSectionState::BfRangeEnd { start: value })
            }
            EmbeddedCMapSection::CidChar => {
                Ok(EmbeddedCMapSectionState::CidCharDestination { code: value })
            }
            EmbeddedCMapSection::CidRange => {
                Ok(EmbeddedCMapSectionState::CidRangeEnd { start: value })
            }
        }
    }
}

impl EmbeddedCMapSection {
    /// Return the token that terminates this section.
    ///
    /// # Parameters
    ///
    /// - `self`: The section whose end token is requested.
    ///
    /// # Returns
    ///
    /// The matching end token for the active section.
    fn end_token(self) -> CMapToken {
        match self {
            Self::CodeSpaceRange => CMapToken::EndCodeSpaceRange,
            Self::BfChar => CMapToken::EndBfChar,
            Self::BfRange => CMapToken::EndBfRange,
            Self::CidChar => CMapToken::EndCidChar,
            Self::CidRange => CMapToken::EndCidRange,
        }
    }

    /// Return the error message used when the section is not closed.
    ///
    /// # Parameters
    ///
    /// - `self`: The section whose missing-end message is requested.
    ///
    /// # Returns
    ///
    /// The error message for an unterminated section.
    fn missing_end_message(self) -> &'static str {
        match self {
            Self::CodeSpaceRange => "missing endcodespacerange",
            Self::BfChar => "missing endbfchar",
            Self::BfRange => "missing endbfrange",
            Self::CidChar => "missing endcidchar",
            Self::CidRange => "missing endcidrange",
        }
    }
}
