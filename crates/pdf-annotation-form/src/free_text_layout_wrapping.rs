//! Width-aware text wrapping with source cursor tracking.

use std::ops::RangeInclusive;

use pdf_font::PdfFontSpec;

/// An encoded visual line and the source cursor positions represented by it.
pub(crate) struct WrappedLine {
    /// The normalized encoded bytes rendered for the line.
    bytes: Vec<u8>,
    /// The inclusive source cursor range associated with the line.
    cursor_range: RangeInclusive<usize>,
}

impl WrappedLine {
    /// Creates a visual line from encoded bytes and source cursor bounds.
    fn new(bytes: Vec<u8>, source_start: usize, source_end: usize) -> Self {
        Self {
            bytes,
            cursor_range: source_start..=source_end,
        }
    }

    /// Returns the encoded bytes rendered for the line.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the first source cursor represented by the line.
    pub(super) fn source_start(&self) -> usize {
        *self.cursor_range.start()
    }

    /// Reports whether the line represents a source cursor.
    pub(super) fn contains_cursor(&self, cursor: usize) -> bool {
        self.cursor_range.contains(&cursor)
    }
}

/// Wraps encoded text using font-derived visual widths.
pub(super) struct TextWrapper<'a> {
    /// The font used to measure candidate lines.
    font: &'a PdfFontSpec,
    /// The font size used for measurement.
    font_size: f32,
    /// The maximum permitted line width.
    maximum_width: f32,
}

impl<'a> TextWrapper<'a> {
    /// Creates a text wrapper with fixed measurement parameters.
    pub(super) fn new(font: &'a PdfFontSpec, font_size: f32, maximum_width: f32) -> Self {
        Self {
            font,
            font_size,
            maximum_width,
        }
    }

    /// Wraps encoded text while retaining source cursor positions.
    pub(super) fn wrap(&self, text: &[u8]) -> Vec<WrappedLine> {
        SourceParagraphs::new(text)
            .flat_map(|paragraph| self.wrap_paragraph(paragraph))
            .collect()
    }

    /// Wraps one explicit-newline-delimited paragraph.
    fn wrap_paragraph(&self, paragraph: SourceParagraph<'_>) -> Vec<WrappedLine> {
        if paragraph.bytes().is_empty() {
            return vec![WrappedLine::new(
                Vec::new(),
                paragraph.source_start(),
                paragraph.source_start(),
            )];
        }

        let mut lines = Vec::new();
        let mut line = LineBuilder::empty(paragraph.source_start());
        for word in SourceWords::new(paragraph) {
            if line.try_push_word(&word, self) {
                continue;
            }

            if !line.is_empty() {
                lines.push(line.take(word.source_start()));
            }
            for (index, byte) in word.bytes().iter().copied().enumerate() {
                let source_index = word.source_start().saturating_add(index);
                if let Some(completed) = line.push_byte(byte, source_index, self) {
                    lines.push(completed);
                }
            }
        }

        lines.push(line.finish());
        lines
    }

    /// Reports whether encoded bytes fit within the configured width.
    fn fits(&self, bytes: &[u8]) -> bool {
        pdf_text_engine::measure_encoded_text_width(self.font, bytes, self.font_size)
            <= self.maximum_width
    }
}

/// A paragraph slice paired with its source offset.
#[derive(Clone, Copy)]
struct SourceParagraph<'a> {
    /// The encoded paragraph bytes without the newline separator.
    bytes: &'a [u8],
    /// The paragraph's starting offset in the full source text.
    source_start: usize,
}

impl<'a> SourceParagraph<'a> {
    /// Creates a source paragraph.
    fn new(bytes: &'a [u8], source_start: usize) -> Self {
        Self {
            bytes,
            source_start,
        }
    }

    /// Returns the encoded paragraph bytes.
    fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the paragraph's source offset.
    fn source_start(self) -> usize {
        self.source_start
    }
}

/// Iterates explicit-newline-delimited paragraphs and their source offsets.
struct SourceParagraphs<'a> {
    /// The bytes not yet returned, including a possible empty trailing paragraph.
    remaining: Option<&'a [u8]>,
    /// The source offset of the next paragraph.
    next_source_start: usize,
}

impl<'a> SourceParagraphs<'a> {
    /// Creates a paragraph iterator over encoded source text.
    fn new(text: &'a [u8]) -> Self {
        Self {
            remaining: Some(text),
            next_source_start: 0,
        }
    }
}

impl<'a> Iterator for SourceParagraphs<'a> {
    type Item = SourceParagraph<'a>;

    /// Returns the next paragraph while preserving empty and trailing lines.
    fn next(&mut self) -> Option<Self::Item> {
        let remaining = self.remaining.take()?;
        let source_start = self.next_source_start;
        let Some(separator_index) = remaining.iter().position(|byte| *byte == b'\n') else {
            return Some(SourceParagraph::new(remaining, source_start));
        };
        let paragraph = remaining.get(..separator_index).unwrap_or_default();
        let next_index = separator_index.saturating_add(1);
        self.remaining = remaining.get(next_index..);
        self.next_source_start = source_start.saturating_add(next_index);
        Some(SourceParagraph::new(paragraph, source_start))
    }
}

/// A non-whitespace word paired with its source offset.
struct SourceWord<'a> {
    /// The encoded bytes in the word.
    bytes: &'a [u8],
    /// The word's starting offset in the full source text.
    source_start: usize,
}

impl<'a> SourceWord<'a> {
    /// Creates a source word.
    fn new(bytes: &'a [u8], source_start: usize) -> Self {
        Self {
            bytes,
            source_start,
        }
    }

    /// Returns the encoded word bytes.
    fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the word's source offset.
    fn source_start(&self) -> usize {
        self.source_start
    }
}

/// Iterates non-whitespace words within one source paragraph.
struct SourceWords<'a> {
    /// The paragraph being scanned.
    paragraph: SourceParagraph<'a>,
    /// The next paragraph-relative byte index to inspect.
    next_index: usize,
}

impl<'a> SourceWords<'a> {
    /// Creates a word iterator for a source paragraph.
    fn new(paragraph: SourceParagraph<'a>) -> Self {
        Self {
            paragraph,
            next_index: 0,
        }
    }
}

impl<'a> Iterator for SourceWords<'a> {
    type Item = SourceWord<'a>;

    /// Returns the next non-whitespace word and its exact source offset.
    fn next(&mut self) -> Option<Self::Item> {
        let remaining = self.paragraph.bytes().get(self.next_index..)?;
        let leading_whitespace = remaining
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())?;
        let word_start = self.next_index.saturating_add(leading_whitespace);
        let word_tail = self.paragraph.bytes().get(word_start..)?;
        let word_length = word_tail
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(word_tail.len());
        let word_end = word_start.saturating_add(word_length);
        self.next_index = word_end;
        let bytes = self
            .paragraph
            .bytes()
            .get(word_start..word_end)
            .unwrap_or_default();
        let source_start = self.paragraph.source_start().saturating_add(word_start);
        Some(SourceWord::new(bytes, source_start))
    }
}

/// Incrementally assembles one wrapped visual line.
struct LineBuilder {
    /// The normalized encoded bytes currently assigned to the line.
    bytes: Vec<u8>,
    /// The first source cursor represented by the line.
    source_start: usize,
    /// The final source cursor represented by the line.
    source_end: usize,
}

impl LineBuilder {
    /// Creates an empty line beginning at a source cursor.
    fn empty(source_start: usize) -> Self {
        Self {
            bytes: Vec::new(),
            source_start,
            source_end: source_start,
        }
    }

    /// Reports whether the line contains no encoded bytes.
    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Appends a whole word when the resulting line remains within its limit.
    fn try_push_word(&mut self, word: &SourceWord<'_>, wrapper: &TextWrapper<'_>) -> bool {
        let original_length = self.bytes.len();
        if original_length != 0 {
            self.bytes.push(b' ');
        }
        self.bytes.extend_from_slice(word.bytes());

        if wrapper.fits(&self.bytes) {
            if original_length == 0 {
                self.source_start = word.source_start();
            }
            self.source_end = word.source_start().saturating_add(word.bytes().len());
            true
        } else {
            self.bytes.truncate(original_length);
            false
        }
    }

    /// Appends one byte, returning a completed line when the byte causes a wrap.
    fn push_byte(
        &mut self,
        byte: u8,
        source_index: usize,
        wrapper: &TextWrapper<'_>,
    ) -> Option<WrappedLine> {
        if self.bytes.is_empty() {
            self.source_start = source_index;
        }
        self.bytes.push(byte);
        let completed = if self.bytes.len() > 1 && !wrapper.fits(&self.bytes) {
            self.bytes.pop();
            let completed = self.take(source_index);
            self.bytes.push(byte);
            Some(completed)
        } else {
            None
        };

        self.source_end = source_index.saturating_add(1);
        completed
    }

    /// Completes the current line and starts an empty replacement line.
    fn take(&mut self, next_start: usize) -> WrappedLine {
        std::mem::replace(self, Self::empty(next_start)).finish()
    }

    /// Converts the builder into a completed wrapped line.
    fn finish(self) -> WrappedLine {
        WrappedLine::new(self.bytes, self.source_start, self.source_end)
    }
}

#[cfg(test)]
mod tests {
    use pdf_font::standard14::Standard14Font;

    use super::*;

    #[test]
    fn wrapping_normalizes_ascii_whitespace_and_tracks_source_cursors() {
        let font = PdfFontSpec::from(Standard14Font::Courier);
        let lines =
            TextWrapper::new(&font, 12.0, f32::INFINITY).wrap(b" first\t\tsecond  \n\nthird");
        let line_bytes: Vec<&[u8]> = lines.iter().map(WrappedLine::bytes).collect();

        assert_eq!(line_bytes, vec![b"first second".as_slice(), b"", b"third"]);
        assert_eq!(
            lines.first().map(|line| line.cursor_range.clone()),
            Some(1..=14)
        );
        assert_eq!(
            lines.get(1).map(|line| line.cursor_range.clone()),
            Some(17..=17)
        );
        assert_eq!(
            lines.get(2).map(|line| line.cursor_range.clone()),
            Some(18..=23)
        );
    }

    #[test]
    fn wrapping_fills_lines_before_splitting_oversized_words() {
        let font = PdfFontSpec::from(Standard14Font::Courier);
        let four_glyphs = pdf_text_engine::measure_encoded_text_width(&font, b"aaaa", 12.0);
        let lines = TextWrapper::new(&font, 12.0, four_glyphs).wrap(b"  aa b cccccc");
        let line_bytes: Vec<&[u8]> = lines.iter().map(WrappedLine::bytes).collect();

        assert_eq!(
            line_bytes,
            vec![b"aa b".as_slice(), b"cccc".as_slice(), b"cc".as_slice()]
        );
        assert_eq!(
            lines.first().map(|line| line.cursor_range.clone()),
            Some(2..=6)
        );
        assert!(lines.get(1).is_some_and(|line| line.contains_cursor(11)));
        assert!(lines.get(2).is_some_and(|line| line.contains_cursor(13)));
    }
}
