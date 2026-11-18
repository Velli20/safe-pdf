//! Draws a text-entry widget's live value into its appearance: the text objects inside the
//! appearance's `/Tx` marked content, which show the value the document was saved with,
//! are replaced by operators showing the live value in the widget's `/DA` font.
use crate::{PdfRenderer, PdfRendererError};
use num_traits::ToPrimitive;
use pdf_annotation_core::{PaintedAppearance, VariableText, style::Alignment};
use pdf_canvas::{
    canvas_backend::CanvasBackend, font_selection::FontSelection, pdf_canvas::PdfCanvas,
};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::{PdfTextItem, variants::PdfOperatorVariant as Op};
use pdf_font::{
    font::LineMetrics, host_font_program::HostFontProgram, pdf_font_handle::PdfFontHandle,
};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::pdf_name::PdfName;
use pdf_resources::{form::FormXObject, resources::Resources};
use pdf_text_engine::{FontSystem, PdfTextEncoder};

/// The marked-content tag enclosing a widget's variable text.
const VARIABLE_TEXT_TAG: &[u8] = b"Tx";

impl PdfRenderer {
    /// The content of `form`, drawn with `resources` on `canvas`, with its `/Tx` marked
    /// content showing the live value of the text entry `appearance` instead of the saved
    /// one. `None` for an appearance without a live value, and when the `/DA` font cannot
    /// be loaded or has no code for a character of the value, so the form is drawn as it is.
    pub(crate) fn live_value_content<B: CanvasBackend>(
        &self,
        canvas: &mut PdfCanvas<'_, B>,
        form: &FormXObject,
        resources: Option<&Resources>,
        appearance: &PaintedAppearance,
    ) -> Option<ContentStream> {
        appearance
            .text
            .as_ref()
            .zip(form.bbox)
            .and_then(|(text, bbox)| {
                let (font, _) = canvas.load_named_font(&text.font, resources).ok()?;
                ValueLayout::new(&self.font_system, font, bbox, text)
                    .ok()?
                    .stream(&form.content_stream)
            })
    }

    /// The program of `appearance`'s `/DA` font on page `page_index`, mapped by Unicode
    /// onto the glyphs the value is painted with, for a host that shows the value being
    /// edited in that font. A comb field's program spaces its glyphs one per cell, as the
    /// value is painted. `None` for an appearance without a live value and for a font a
    /// host program cannot be built from.
    ///
    /// # Errors
    /// Returns an error if the page is missing, the appearance's resources cannot be read,
    /// or the program cannot be built.
    pub fn field_font_program(
        &self,
        page_index: usize,
        appearance: &PaintedAppearance,
    ) -> Result<Option<HostFontProgram>, PdfRendererError> {
        let Some(text) = &appearance.text else {
            return Ok(None);
        };
        let Some(form) = self.appearance_form(appearance) else {
            return Ok(None);
        };
        let resources = self.appearance_resources(&form, self.page(page_index)?)?;
        let selection = FontSelection::new(
            &text.font,
            resources.as_deref(),
            self.form_resources().map(AsRef::as_ref),
        );
        let family = String::from_utf8_lossy(&text.font);
        // The cell in thousandths of an em, the unit of PDF glyph widths.
        let comb_cell = form
            .bbox
            .and_then(|bbox| text.comb_cell_width(bbox.normalized().width()))
            .map(|cell| cell / text.font_size * 1000.0);
        Ok(self
            .font_system
            .host_font_program(selection.spec(), &family, comb_cell)?)
    }
}

/// Encoded text and its advance at the value's font size.
#[derive(Default)]
struct Line {
    /// The `/DA` font's character codes.
    codes: Vec<u8>,
    /// The advance of the codes.
    width: f32,
}

impl Line {
    /// Appends `other` to the end of this line.
    fn append(&mut self, other: Self) {
        self.codes.extend(other.codes);
        self.width += other.width;
    }
}

/// Encoded text placed at the start of its baseline.
struct PlacedRun {
    /// The `/DA` font's character codes.
    codes: Vec<u8>,
    /// The start of the run.
    x: f32,
    /// The baseline the run is drawn on.
    baseline: f32,
}

/// The lines of a widget's value laid out inside its appearance box.
struct ValueLayout<'a> {
    /// The value, its font and its text state.
    text: &'a VariableText,
    /// The loaded `/DA` font, which measures the value.
    font: PdfFontHandle,
    /// The `/DA` font's character codes.
    encoder: PdfTextEncoder,
    /// The font system that loaded `font`.
    font_system: &'a FontSystem,
    /// The appearance box, normalized.
    bbox: Rect,
    /// The box the value is laid out in and clipped to: the appearance box less the
    /// text inset on each side.
    plate: Rect,
}

impl<'a> ValueLayout<'a> {
    /// Lays `text` out in `font`, its loaded `/DA` font.
    fn new(
        font_system: &'a FontSystem,
        font: PdfFontHandle,
        bbox: Rect,
        text: &'a VariableText,
    ) -> Result<Self, PdfRendererError> {
        let bbox = bbox.normalized();
        Ok(Self {
            text,
            encoder: PdfTextEncoder::new(font.spec())?,
            font,
            font_system,
            bbox,
            plate: bbox.inset(text.inset),
        })
    }

    /// `content` with the text objects of its first `/Tx` marked content removed and the
    /// value drawn at its end, so the backgrounds and borders some appearances draw inside
    /// it remain. Without `/Tx` marked content, every operator is kept and the value is
    /// appended in marked content of its own. `None` when the font has no code for a
    /// character of the value.
    fn stream(&self, content: &ContentStream) -> Option<ContentStream> {
        let value = self.operators()?;
        let operators = &content.operators;
        let operators = match content.marked_content_span(VARIABLE_TEXT_TAG) {
            Some(span) => {
                let mut in_text = false;
                let inside = operators.get(span.clone()).unwrap_or_default();
                let without_text = inside.iter().filter(move |operator| {
                    let text = in_text || matches!(operator, Op::BeginText);
                    in_text = text && !matches!(operator, Op::EndText);
                    !text
                });
                operators
                    .get(..span.start)
                    .unwrap_or_default()
                    .iter()
                    .chain(without_text)
                    .cloned()
                    .chain(value)
                    .chain(
                        operators
                            .get(span.end..)
                            .unwrap_or_default()
                            .iter()
                            .cloned(),
                    )
                    .collect()
            }
            None => operators
                .iter()
                .cloned()
                .chain([Op::BeginMarkedContent {
                    tag: PdfName::from(VARIABLE_TEXT_TAG),
                }])
                .chain(value)
                .chain([Op::EndMarkedContent])
                .collect(),
        };
        Some(ContentStream {
            operators,
            id: content.id,
        })
    }

    /// `text` encoded in the `/DA` font, with its advance; `None` when the font has no
    /// code for one of its characters.
    fn line(&self, text: &str) -> Option<Line> {
        let codes = self.encoder.encode(text)?;
        let width = self
            .font_system
            .text_width(&self.font, &codes, self.text.font_size);
        Some(Line { codes, width })
    }

    /// The lines of the value: the whole value on one line, or each paragraph of a
    /// multiline value wrapped to the plate width. `None` when the font has no code for a
    /// character of the value.
    fn lines(&self) -> Option<Vec<Line>> {
        if !self.text.multiline {
            return Some(vec![self.line(&self.text.value)?]);
        }
        let mut breaker = LineBreaker {
            layout: self,
            lines: Vec::new(),
            line: Line::default(),
        };
        for paragraph in self.text.value.split(['\r', '\n']) {
            for word in Words(paragraph) {
                breaker.push_word(word)?;
            }
            breaker.end_paragraph();
        }
        Some(breaker.lines)
    }

    /// The runs of the value: one per line, or one per character of a comb field. `None`
    /// when the font has no code for a character of the value.
    fn runs(&self) -> Option<Vec<PlacedRun>> {
        if let Some(cells) = self.text.comb {
            return self.comb_runs(cells);
        }
        let lines = self.lines()?;
        let baselines = self.baselines(lines.len());
        Some(
            lines
                .into_iter()
                .zip(baselines)
                .map(|(line, baseline)| PlacedRun {
                    x: self.line_start(line.width),
                    codes: line.codes,
                    baseline,
                })
                .collect(),
        )
    }

    /// One run per character of a comb field's value, each centred in its own of `cells`
    /// equal cells across the plate as viewers space comb text: the characters fill the
    /// first cells, the last cells, or the cells nearest the middle under the value's
    /// alignment. Characters beyond the last cell are left out.
    fn comb_runs(&self, cells: u32) -> Option<Vec<PlacedRun>> {
        let characters = self.text.value.chars().take(usize::try_from(cells).ok()?);
        let count = u32::try_from(characters.clone().count()).ok()?;
        let free = cells.checked_sub(count)?;
        let start = match self.text.alignment {
            Alignment::Left => 0,
            Alignment::Center => free / 2,
            Alignment::Right => free,
        };
        let cell = self.text.comb_cell_width(self.bbox.width())?;
        let baseline = self.baselines(1).next()?;
        let mut buffer = [0; 4];
        (start..)
            .zip(characters)
            .map(|(index, character)| {
                let glyph = self.line(character.encode_utf8(&mut buffer))?;
                let center = self.plate.left + cell * (index.to_f32()? + 0.5);
                Some(PlacedRun {
                    x: center - glyph.width / 2.0,
                    codes: glyph.codes,
                    baseline,
                })
            })
            .collect()
    }

    /// The left end of a line `width` wide under the value's alignment.
    fn line_start(&self, width: f32) -> f32 {
        let plate = self.plate;
        match self.text.alignment {
            Alignment::Left => plate.left,
            Alignment::Center => (plate.left + plate.right - width) / 2.0,
            Alignment::Right => plate.right - width,
        }
    }

    /// The baselines of `count` lines: one line centered vertically in the plate, or lines
    /// stepping down from its top by the font's line height.
    fn baselines(&self, count: usize) -> impl Iterator<Item = f32> {
        let line = self.font.line_metrics();
        let LineMetrics { ascent, descent } = line;
        let size = self.text.font_size;
        let plate = self.plate;
        let (first, step) = if self.text.multiline {
            (plate.bottom - ascent * size, line.line_height() * size)
        } else {
            (
                (plate.top + plate.bottom - (ascent + descent) * size) / 2.0,
                0.0,
            )
        };
        (0..count).scan(first, move |baseline, _| {
            let current = *baseline;
            *baseline -= step;
            Some(current)
        })
    }

    /// The operators showing the value, clipped to the plate; `None` when the font has no
    /// code for a character of the value.
    fn operators(&self) -> Option<Vec<Op>> {
        let runs = self.runs()?;
        let plate = self.plate;
        let color = self.text.color;
        let mut operators = vec![
            Op::SaveGraphicsState,
            Op::Rectangle {
                x: plate.left,
                y: plate.top,
                width: plate.width(),
                height: plate.height(),
            },
            Op::ClipNonZero,
            Op::EndPath,
            Op::BeginText,
            Op::SetFont {
                name: PdfName::from(self.text.font.as_ref()),
                size: self.text.font_size,
            },
            Op::SetRGBFill {
                r: color.r,
                g: color.g,
                b: color.b,
            },
        ];
        for run in runs.into_iter().filter(|run| !run.codes.is_empty()) {
            operators.push(Op::SetTextMatrix {
                matrix: Transform::from_translate(run.x, run.baseline),
            });
            operators.push(Op::ShowText {
                text: PdfTextItem::Text(run.codes.as_slice().into()),
            });
        }
        operators.extend([Op::EndText, Op::RestoreGraphicsState]);
        Some(operators)
    }
}

/// Wraps the words of a multiline value into lines no wider than the plate, as viewers
/// wrap form text.
struct LineBreaker<'l, 'a> {
    /// The layout that encodes and measures the words.
    layout: &'l ValueLayout<'a>,
    /// The finished lines.
    lines: Vec<Line>,
    /// The line being filled.
    line: Line,
}

impl LineBreaker<'_, '_> {
    /// Adds `word`, with the spaces after it: it moves to the next line when it overflows
    /// the plate width, and a word wider than the plate is broken between characters.
    /// Each word is encoded and measured once. `None` when the font has no code for one of
    /// its characters.
    fn push_word(&mut self, word: &str) -> Option<()> {
        let max_width = self.layout.plate.width();
        let next = self.layout.line(word)?;
        if self.line.width + next.width <= max_width {
            self.line.append(next);
        } else if !self.line.codes.is_empty() && next.width <= max_width {
            self.lines.push(std::mem::replace(&mut self.line, next));
        } else {
            let mut buffer = [0; 4];
            for character in word.chars() {
                self.push_character(self.layout.line(character.encode_utf8(&mut buffer))?);
            }
        }
        Some(())
    }

    /// Adds one encoded character of a word wider than the space left, starting a new line
    /// when it overflows the plate width; a line's first character always stays on it.
    fn push_character(&mut self, character: Line) {
        if !self.line.codes.is_empty()
            && self.line.width + character.width > self.layout.plate.width()
        {
            self.lines.push(std::mem::take(&mut self.line));
        }
        self.line.append(character);
    }

    /// Ends the line being filled, as each paragraph does.
    fn end_paragraph(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
    }
}

/// The words of a paragraph, each with the spaces that follow it, which wrap together.
struct Words<'a>(&'a str);

impl<'a> Iterator for Words<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.0.is_empty() {
            return None;
        }
        let spaces = self.0.find(' ').unwrap_or(self.0.len());
        let end = self
            .0
            .get(spaces..)
            .and_then(|tail| tail.find(|character| character != ' '))
            .map_or(self.0.len(), |offset| spaces + offset);
        let (word, rest) = self.0.split_at_checked(end)?;
        self.0 = rest;
        Some(word)
    }
}
