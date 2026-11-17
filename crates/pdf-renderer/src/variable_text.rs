//! Draws a text-entry widget's live value into its appearance: the text objects inside the
//! appearance's `/Tx` marked content, which show the value the document was saved with,
//! are replaced by operators showing the live value in the widget's `/DA` font.
use crate::{PdfRenderer, PdfRendererError};
use pdf_annotation_core::{PaintedAppearance, VariableText, style::Alignment};
use pdf_canvas::{
    canvas_backend::CanvasBackend, font_selection::FontSelection, pdf_canvas::PdfCanvas,
};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::{PdfTextItem, variants::PdfOperatorVariant as Op};
use pdf_font::{
    host_font_program::HostFontProgram,
    pdf_font_handle::{LineMetrics, PdfFontHandle},
};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::pdf_name::PdfName;
use pdf_resources::resources::Resources;
use pdf_text_engine::{FontSystem, PdfTextEncoder, text::PdfTextRun, text_style::TextStyle};
use std::ops::Range;

/// The marked-content tag enclosing a widget's variable text.
const VARIABLE_TEXT_TAG: &[u8] = b"Tx";

impl PdfRenderer {
    /// `content`, an appearance stream drawn in `bbox` with `resources` on `canvas`, with
    /// its `/Tx` marked content showing `text` instead of the saved value. An appearance
    /// without `/Tx` marked content keeps all of its operators and draws the value after
    /// them.
    ///
    /// `None` when the `/DA` font cannot be loaded or has no code for a character of the
    /// value, so the appearance is drawn as it is.
    pub(crate) fn variable_text_stream<B: CanvasBackend>(
        &self,
        canvas: &mut PdfCanvas<'_, B>,
        content: &ContentStream,
        bbox: Rect,
        resources: Option<&Resources>,
        text: &VariableText,
    ) -> Option<ContentStream> {
        let (font, _) = canvas.load_named_font(&text.font, resources).ok()?;
        let layout = ValueLayout::new(&self.font_system, font, bbox, text).ok()?;
        Some(ContentStream {
            operators: AppearanceOperators(&content.operators).with_value(layout.operators()?),
            id: content.id,
        })
    }

    /// The program of `appearance`'s `/DA` font on page `page_index`, mapped by Unicode
    /// onto the glyphs the value is painted with, for a host that shows the value being
    /// edited in that font. `None` for an appearance without a live value and for a font
    /// a host program cannot be built from.
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
        Ok(self
            .font_system
            .host_font_program(selection.spec(), &family)?)
    }
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
}

impl<'a> ValueLayout<'a> {
    /// Lays `text` out in `font`, its loaded `/DA` font.
    fn new(
        font_system: &'a FontSystem,
        font: PdfFontHandle,
        bbox: Rect,
        text: &'a VariableText,
    ) -> Result<Self, PdfRendererError> {
        Ok(Self {
            text,
            encoder: PdfTextEncoder::new(font.spec())?,
            font,
            font_system,
            bbox: bbox.normalized(),
        })
    }

    /// The box the value is laid out in and clipped to: the appearance box less the
    /// text inset on each side.
    fn plate(&self) -> Rect {
        let inset = self.text.inset;
        Rect {
            left: self.bbox.left + inset,
            top: self.bbox.top + inset,
            right: self.bbox.right - inset,
            bottom: self.bbox.bottom - inset,
        }
    }

    /// The advance of `codes` at the value's font size.
    fn width(&self, codes: &[u8]) -> f32 {
        let items = [PdfTextItem::Text(codes.into())];
        let run = PdfTextRun {
            font: &self.font,
            items: &items,
            style: TextStyle {
                font_size: self.text.font_size,
                ..TextStyle::default()
            },
        };
        self.font_system
            .layout_pdf(&run)
            .map(|layout| layout.advance.x)
            .unwrap_or_default()
    }

    /// The encoded lines of the value with their widths: the whole value on one line, or
    /// each paragraph of a multiline value wrapped to the plate width. `None` when the font
    /// has no code for a character of the value.
    fn lines(&self) -> Option<Vec<(Vec<u8>, f32)>> {
        if !self.text.multiline {
            let codes = self.encoder.encode(&self.text.value)?;
            let width = self.width(&codes);
            return Some(vec![(codes, width)]);
        }
        let mut lines = Vec::new();
        for paragraph in self.text.value.split(['\r', '\n']) {
            self.wrap(paragraph, &mut lines)?;
        }
        Some(lines)
    }

    /// Appends the lines of `paragraph` to `lines`, as viewers wrap form text: a word moves
    /// to the next line with the spaces after it when they overflow the plate width, and
    /// those spaces count in the width of the line they end. A word wider than the plate
    /// is broken between characters. Each word is encoded and measured once.
    fn wrap(&self, paragraph: &str, lines: &mut Vec<(Vec<u8>, f32)>) -> Option<()> {
        let max_width = self.plate().width();
        let mut line = Vec::new();
        let mut width = 0.0;
        for word in Words(paragraph) {
            let codes = self.encoder.encode(word)?;
            let word_width = self.width(&codes);
            if width + word_width <= max_width {
                line.extend(codes);
                width += word_width;
            } else if !line.is_empty() && word_width <= max_width {
                lines.push((std::mem::replace(&mut line, codes), width));
                width = word_width;
            } else {
                self.break_word(word, max_width, lines, &mut line, &mut width)?;
            }
        }
        lines.push((line, width));
        Some(())
    }

    /// Appends `word`, wider than the space left on `line`, character by character,
    /// starting a new line before each character that overflows `max_width`; a line's
    /// first character always stays on it.
    fn break_word(
        &self,
        word: &str,
        max_width: f32,
        lines: &mut Vec<(Vec<u8>, f32)>,
        line: &mut Vec<u8>,
        width: &mut f32,
    ) -> Option<()> {
        let mut buffer = [0; 4];
        for character in word.chars() {
            let codes = self.encoder.encode(character.encode_utf8(&mut buffer))?;
            let character_width = self.width(&codes);
            if !line.is_empty() && *width + character_width > max_width {
                lines.push((std::mem::take(line), *width));
                *width = 0.0;
            }
            line.extend(codes);
            *width += character_width;
        }
        Some(())
    }

    /// The left end of a line `width` wide under the value's alignment.
    fn line_start(&self, width: f32) -> f32 {
        let plate = self.plate();
        match self.text.alignment {
            Alignment::Left => plate.left,
            Alignment::Center => (plate.left + plate.right - width) / 2.0,
            Alignment::Right => plate.right - width,
        }
    }

    /// The baselines of `count` lines: one line centered vertically in the plate, or lines
    /// stepping down from its top by the font's line height.
    fn baselines(&self, count: usize) -> impl Iterator<Item = f32> {
        let LineMetrics { ascent, descent } = self.font.line_metrics();
        let size = self.text.font_size;
        let plate = self.plate();
        let (first, step) = if self.text.multiline {
            (plate.bottom - ascent * size, (ascent - descent) * size)
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
        let lines = self.lines()?;
        let plate = self.plate();
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
        for ((codes, width), baseline) in lines.iter().zip(self.baselines(lines.len())) {
            if codes.is_empty() {
                continue;
            }
            operators.push(Op::SetTextMatrix {
                matrix: Transform::from_translate(self.line_start(*width), baseline),
            });
            operators.push(Op::ShowText {
                text: PdfTextItem::Text(codes.as_slice().into()),
            });
        }
        operators.extend([Op::EndText, Op::RestoreGraphicsState]);
        Some(operators)
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

/// The operators of a widget's appearance stream.
struct AppearanceOperators<'a>(&'a [Op]);

impl AppearanceOperators<'_> {
    /// The operators with the text objects of the first `/Tx` marked content removed and
    /// `value` drawn at its end, so the backgrounds and borders some appearances draw
    /// inside it remain. Without `/Tx` marked content, every operator is kept and `value`
    /// is appended in marked content of its own.
    fn with_value(&self, value: Vec<Op>) -> Vec<Op> {
        let Some(span) = self.variable_text_span() else {
            return self
                .0
                .iter()
                .cloned()
                .chain([Op::BeginMarkedContent {
                    tag: PdfName::from(VARIABLE_TEXT_TAG),
                }])
                .chain(value)
                .chain([Op::EndMarkedContent])
                .collect();
        };
        let before = self.0.get(..span.start).unwrap_or_default();
        let inside = self.0.get(span.clone()).unwrap_or_default();
        let after = self.0.get(span.end..).unwrap_or_default();
        before
            .iter()
            .chain(AppearanceOperators(inside).without_text_objects())
            .cloned()
            .chain(value)
            .chain(after.iter().cloned())
            .collect()
    }

    /// The range from the first `/Tx` marked-content begin up to, not including, the end
    /// that closes it; up to the last operator when it is not closed.
    fn variable_text_span(&self) -> Option<Range<usize>> {
        let start = self.0.iter().position(|operator| match operator {
            Op::BeginMarkedContent { tag } => tag.as_bytes() == VARIABLE_TEXT_TAG,
            Op::BeginMarkedContentWithProps(begin) => begin.tag() == VARIABLE_TEXT_TAG,
            _ => false,
        })?;
        let mut depth = 0_usize;
        let len = self
            .0
            .get(start..)
            .unwrap_or_default()
            .iter()
            .position(|operator| {
                match operator {
                    Op::BeginMarkedContent { .. } | Op::BeginMarkedContentWithProps(_) => {
                        depth = depth.saturating_add(1);
                    }
                    Op::EndMarkedContent => depth = depth.saturating_sub(1),
                    _ => {}
                }
                depth == 0
            })
            .unwrap_or(self.0.len().saturating_sub(start));
        Some(start..start.saturating_add(len))
    }

    /// The operators without those from each `BT` through its `ET`.
    fn without_text_objects(&self) -> impl Iterator<Item = &Op> {
        let mut in_text = false;
        self.0.iter().filter(move |operator| {
            let inside = in_text || matches!(operator, Op::BeginText);
            in_text = inside && !matches!(operator, Op::EndText);
            !inside
        })
    }
}
