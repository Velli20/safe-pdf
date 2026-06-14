use crate::{
    cmap::{
        parser::CMapParser,
        token::CMapToken,
        unicode::{
            bytes_to_char_code, codepoint_to_chars, sequential_base_code, utf16_bytes_to_chars,
            utf16_bytes_to_chars_non_empty,
        },
    },
    error::CMapError,
};

/// Parser state while reading a `beginbfchar` section.
enum BfCharState {
    NeedSource,
    NeedDestination(u16),
}

/// Parser state while reading a `beginbfrange` section.
enum BfRangeState {
    NeedStart,
    NeedEnd(u16),
    NeedValue { start: u16, end: u16 },
    Array { code: u16, end: u16 },
}

impl CMapParser<'_> {
    /// Advance this parser until `end_operator` is found or the input ends.
    ///
    /// # Parameters
    ///
    /// - `end_operator`: Operator bytes to search for, such as
    ///   `b"endbfrange"`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` after consuming the requested operator, `Ok(false)`
    /// when input ends first, or a [`CMapError`] if tokenization fails while
    /// scanning.
    fn consume_until_operator(&mut self, end_operator: CMapToken) -> Result<bool, CMapError> {
        loop {
            match self.next_token()? {
                Some(token) if token == end_operator => return Ok(true),
                Some(_) => {}
                None => return Ok(false),
            }
        }
    }

    /// Parse a ToUnicode `beginbfchar` section into direct mappings.
    pub fn parse_bfchar_section(&mut self) -> Result<bool, CMapError> {
        let mut state = BfCharState::NeedSource;

        loop {
            let Some(token) = self.next_token()? else {
                return Ok(true);
            };

            let Some(next_state) = (match token {
                CMapToken::EndBfChar => return Ok(true),
                CMapToken::HexString(bytes) => Some(self.handle_bfchar_hex_token(state, &bytes)),
                _ => self.recover_bfchar_state(state)?,
            }) else {
                return Ok(false);
            };

            state = next_state;
        }
    }

    /// Parse a ToUnicode `beginbfrange` section into range mappings.
    pub fn parse_bfrange_section(&mut self) -> Result<bool, CMapError> {
        let mut state = BfRangeState::NeedStart;

        loop {
            let Some(token) = self.next_token()? else {
                return Ok(true);
            };

            let Some(next_state) = (match token {
                CMapToken::EndBfRange => return Ok(true),
                CMapToken::HexString(bytes) => Some(self.handle_bfrange_hex_token(state, &bytes)),
                CMapToken::LeftSquareBracket => self.enter_bfrange_array(state)?,
                CMapToken::RightSquareBracket => Some(Self::close_bfrange_array(state)),
                _ => self.recover_bfrange_state(state)?,
            }) else {
                return Ok(false);
            };

            state = next_state;
        }
    }

    /// Handle one hex-string token inside a `beginbfchar` section.
    ///
    /// # Parameters
    ///
    /// - `state`: The current bfchar parser state.
    /// - `bytes`: The raw hex-string bytes from the token.
    ///
    /// # Returns
    ///
    /// The next bfchar parser state after consuming the token.
    fn handle_bfchar_hex_token(&mut self, state: BfCharState, bytes: &[u8]) -> BfCharState {
        match state {
            BfCharState::NeedSource => BfCharState::NeedDestination(bytes_to_char_code(bytes)),
            BfCharState::NeedDestination(source) => {
                self.insert_bfchar_mapping(source, bytes);
                BfCharState::NeedSource
            }
        }
    }

    /// Insert one direct ToUnicode mapping from a bfchar destination string.
    ///
    /// # Parameters
    ///
    /// - `source`: The source character code to map.
    /// - `bytes`: The destination UTF-16BE byte string.
    ///
    /// # Returns
    ///
    /// Nothing. The mapping is inserted into the parser's output map when the
    /// decoded destination is not empty.
    fn insert_bfchar_mapping(&mut self, source: u16, bytes: &[u8]) {
        let chars = utf16_bytes_to_chars(bytes);
        if !chars.is_empty() {
            self.to_unicode_map.insert(source, chars);
        }
    }

    /// Recover bfchar state after encountering an unexpected token.
    ///
    /// # Parameters
    ///
    /// - `state`: The current bfchar parser state.
    ///
    /// # Returns
    ///
    /// The recovered state, `Ok(None)` if the section ends unexpectedly while
    /// skipping to `endbfchar`, or a [`CMapError`] if tokenization fails.
    fn recover_bfchar_state(
        &mut self,
        state: BfCharState,
    ) -> Result<Option<BfCharState>, CMapError> {
        if matches!(state, BfCharState::NeedDestination(_))
            && !self.consume_until_operator(CMapToken::EndBfChar)?
        {
            return Ok(None);
        }

        Ok(Some(BfCharState::NeedSource))
    }

    /// Handle one hex-string token inside a `beginbfrange` section.
    ///
    /// # Parameters
    ///
    /// - `state`: The current bfrange parser state.
    /// - `bytes`: The raw hex-string bytes from the token.
    ///
    /// # Returns
    ///
    /// The next bfrange parser state after consuming the token.
    fn handle_bfrange_hex_token(&mut self, state: BfRangeState, bytes: &[u8]) -> BfRangeState {
        match state {
            BfRangeState::NeedStart => BfRangeState::NeedEnd(bytes_to_char_code(bytes)),
            BfRangeState::NeedEnd(start) => BfRangeState::NeedValue {
                start,
                end: bytes_to_char_code(bytes),
            },
            BfRangeState::NeedValue { start, end } => {
                self.insert_sequential_range(start, end, bytes);
                BfRangeState::NeedStart
            }
            BfRangeState::Array { code, end } => {
                self.insert_bfrange_array_mapping(code, bytes);
                Self::next_bfrange_array_state(code, end)
            }
        }
    }

    /// Enter a `beginbfrange` array mapping after reading `[` or recover.
    ///
    /// # Parameters
    ///
    /// - `state`: The current bfrange parser state.
    ///
    /// # Returns
    ///
    /// The next bfrange parser state, `Ok(None)` if the section cannot be
    /// recovered, or a [`CMapError`] if tokenization fails.
    fn enter_bfrange_array(
        &mut self,
        state: BfRangeState,
    ) -> Result<Option<BfRangeState>, CMapError> {
        match state {
            BfRangeState::NeedValue { start, end } => {
                Ok(Some(BfRangeState::Array { code: start, end }))
            }
            BfRangeState::Array { .. } => Ok(Some(state)),
            _ if self.consume_until_operator(CMapToken::EndBfRange)? => {
                Ok(Some(BfRangeState::NeedStart))
            }
            _ => Ok(None),
        }
    }

    /// Recover bfrange state after encountering an unexpected token.
    ///
    /// # Parameters
    ///
    /// - `state`: The current bfrange parser state.
    ///
    /// # Returns
    ///
    /// The recovered state, `Ok(None)` if the section ends unexpectedly while
    /// skipping to `endbfrange`, or a [`CMapError`] if tokenization fails.
    fn recover_bfrange_state(
        &mut self,
        state: BfRangeState,
    ) -> Result<Option<BfRangeState>, CMapError> {
        match state {
            BfRangeState::NeedStart | BfRangeState::Array { .. } => Ok(Some(state)),
            _ if self.consume_until_operator(CMapToken::EndBfRange)? => {
                Ok(Some(BfRangeState::NeedStart))
            }
            _ => Ok(None),
        }
    }

    /// Close a bfrange array section when `]` is encountered.
    ///
    /// # Parameters
    ///
    /// - `state`: The current bfrange parser state.
    ///
    /// # Returns
    ///
    /// The next bfrange parser state after the closing bracket.
    fn close_bfrange_array(state: BfRangeState) -> BfRangeState {
        match state {
            BfRangeState::Array { .. } => BfRangeState::NeedStart,
            _ => state,
        }
    }

    /// Insert one array entry from a `beginbfrange` destination list.
    ///
    /// # Parameters
    ///
    /// - `code`: The source code that corresponds to the current array entry.
    /// - `bytes`: The destination UTF-16BE byte string.
    ///
    /// # Returns
    ///
    /// Nothing. The mapping is inserted into the parser's output map when the
    /// decoded destination is not empty.
    fn insert_bfrange_array_mapping(&mut self, code: u16, bytes: &[u8]) {
        if let Some(chars) = utf16_bytes_to_chars_non_empty(bytes) {
            self.to_unicode_map.insert(code, chars);
        }
    }

    /// Advance the array-entry state for a `beginbfrange` mapping.
    ///
    /// # Parameters
    ///
    /// - `code`: The current source code value.
    /// - `end`: The inclusive end of the source code range.
    ///
    /// # Returns
    ///
    /// The next array state, or `NeedStart` once the inclusive end is reached.
    fn next_bfrange_array_state(code: u16, end: u16) -> BfRangeState {
        if code >= end {
            BfRangeState::NeedStart
        } else {
            BfRangeState::Array {
                code: code.saturating_add(1),
                end,
            }
        }
    }

    /// Insert a sequential `beginbfrange` mapping into the output map.
    ///
    /// # Parameters
    ///
    /// - `start`: The first source code in the inclusive range.
    /// - `end`: The last source code in the inclusive range.
    /// - `base_bytes`: The UTF-16BE destination bytes for the first entry in
    ///   the range.
    ///
    /// # Returns
    ///
    /// Nothing. Each valid sequential destination is inserted into the parser's
    /// output map.
    fn insert_sequential_range(&mut self, start: u16, end: u16, base_bytes: &[u8]) {
        let mut code = start;
        let mut base = sequential_base_code(base_bytes);

        loop {
            let chars = codepoint_to_chars(base);
            if !chars.is_empty() {
                self.to_unicode_map.insert(code, chars);
            }
            if code >= end {
                break;
            }
            code = code.saturating_add(1);
            base = base.saturating_add(1);
        }
    }
}
