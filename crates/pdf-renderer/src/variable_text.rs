//! Draws a text-entry widget's live value into its appearance: the operators inside the
//! appearance's `/Tx` marked content, which show the value the document was saved with,
//! are replaced by operators showing the live value in the widget's `/DA` font.
use crate::{PdfRenderer, PdfRendererError};
use pdf_annotation_core::{PaintedAppearance, VariableText, style::Alignment};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::{PdfTextItem, variants::PdfOperatorVariant as Op};
use pdf_font::{
    PdfFontSpec, Standard14Font, host_font_program::HostFontProgram, pdf_font_handle::PdfFontHandle,
};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::pdf_name::PdfName;
use pdf_resources::resources::Resources;
use pdf_text_engine::{FontSystem, PdfTextEncoder, text::PdfTextRun, text_style::TextStyle};

/// The marked-content tag enclosing a widget's variable text.
const VARIABLE_TEXT_TAG: &[u8] = b"Tx";
/// Ascender, as a fraction of the font size, of a font without usable metrics.
const FALLBACK_ASCENT: f32 = 0.8;
/// Descender, as a fraction of the font size, of a font without usable metrics.
const FALLBACK_DESCENT: f32 = -0.2;

impl PdfRenderer {
    /// `content`, an appearance stream drawn in `bbox` with `resources`, with its `/Tx`
    /// marked content showing `text` instead. An appearance without `/Tx` marked content
    /// loses its text objects, and the value is drawn after the rest of the appearance.
    ///
    /// `None` when the `/DA` font cannot be loaded, so the appearance is drawn as it is.
    pub(crate) fn variable_text_stream(
        &self,
        content: &ContentStream,
        bbox: Rect,
        resources: Option<&Resources>,
        text: &VariableText,
    ) -> Option<ContentStream> {
        let spec = self.field_font_spec(resources, &text.font);
        let layout = ValueLayout::new(&self.font_system, &spec, bbox, text)?;
        Some(ContentStream {
            operators: replace_variable_text(&content.operators, layout.operators()),
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
        let spec = self.field_font_spec(resources.as_deref(), &text.font);
        let family = String::from_utf8_lossy(&text.font);
        Ok(self.font_system.host_font_program(&spec, &family)?)
    }

    /// The font resource `name` a widget's value is drawn with: from the appearance's
    /// `resources`, then the interactive form's `/DR`, else the Standard 14 font the
    /// canvas substitutes for a name no resources define.
    fn field_font_spec(&self, resources: Option<&Resources>, name: &[u8]) -> PdfFontSpec {
        resources
            .and_then(|resources| resources.font(name))
            .or_else(|| self.form_resources()?.font(name))
            .map_or_else(
                || PdfFontSpec::from(Standard14Font::for_missing_resource(name)),
                |(spec, _)| spec.as_ref().clone(),
            )
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
    /// Loads `spec`, the `/DA` font of `text`; `None` when it cannot be loaded.
    fn new(
        font_system: &'a FontSystem,
        spec: &PdfFontSpec,
        bbox: Rect,
        text: &'a VariableText,
    ) -> Option<Self> {
        let encoder = PdfTextEncoder::new(spec).ok()?;
        let font = font_system.load_pdf_font(spec.clone()).ok()?;
        Some(Self {
            text,
            font,
            encoder,
            font_system,
            bbox: bbox.normalized(),
        })
    }

    /// The border inset that clips the value: the border width, at least one unit.
    fn clip_inset(&self) -> f32 {
        self.text.border_width.max(1.0)
    }

    /// The box the value is laid out in, inside the border and an equal padding.
    fn text_box(&self) -> Rect {
        let inset = self.clip_inset() * 2.0;
        Rect {
            left: self.bbox.left + inset,
            top: self.bbox.top + inset,
            right: self.bbox.right - inset,
            bottom: self.bbox.bottom - inset,
        }
    }

    /// The font's ascender and descender as fractions of the font size.
    fn vertical_metrics(&self) -> (f32, f32) {
        self.font
            .primary()
            .metrics()
            .and_then(|metrics| {
                let units = f32::from(metrics.units_per_em);
                (units > 0.0 && metrics.ascender > metrics.descender)
                    .then(|| (metrics.ascender / units, metrics.descender / units))
            })
            .unwrap_or((FALLBACK_ASCENT, FALLBACK_DESCENT))
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

    /// The encoded lines of the value: the whole value on one line, or each paragraph
    /// of a multiline value wrapped at spaces to the text box width.
    fn lines(&self) -> Vec<Vec<u8>> {
        if !self.text.multiline {
            return vec![self.encoder.encode(&self.text.value)];
        }
        let width = self.text_box().width();
        self.text
            .value
            .split(['\r', '\n'])
            .flat_map(|paragraph| self.wrap(paragraph, width))
            .collect()
    }

    /// The lines of `paragraph` broken at spaces so each fits `width` where it can; a
    /// word wider than `width` gets a line of its own.
    fn wrap(&self, paragraph: &str, width: f32) -> Vec<Vec<u8>> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && self.width(&self.encoder.encode(&candidate)) > width {
                lines.push(self.encoder.encode(&line));
                line = word.to_owned();
            } else {
                line = candidate;
            }
        }
        lines.push(self.encoder.encode(&line));
        lines
    }

    /// The left end of a line `width` wide under the value's alignment.
    fn line_start(&self, width: f32) -> f32 {
        let text_box = self.text_box();
        match self.text.alignment {
            Alignment::Left => text_box.left,
            Alignment::Center => (text_box.left + text_box.right - width) / 2.0,
            Alignment::Right => text_box.right - width,
        }
    }

    /// The baselines of `count` lines: one line centered vertically in the text box, or
    /// lines stepping down from its top by the font's line height.
    fn baselines(&self, count: usize) -> impl Iterator<Item = f32> {
        let (ascent, descent) = self.vertical_metrics();
        let size = self.text.font_size;
        let text_box = self.text_box();
        let (first, step) = if self.text.multiline {
            (text_box.bottom - ascent * size, (ascent - descent) * size)
        } else {
            (
                (text_box.top + text_box.bottom - (ascent + descent) * size) / 2.0,
                0.0,
            )
        };
        (0..count).scan(first, move |baseline, _| {
            let current = *baseline;
            *baseline -= step;
            Some(current)
        })
    }

    /// The operators showing the value, clipped inside the border.
    fn operators(&self) -> Vec<Op> {
        let lines = self.lines();
        let inset = self.clip_inset();
        let color = self.text.color;
        let mut operators = vec![
            Op::SaveGraphicsState,
            Op::Rectangle {
                x: self.bbox.left + inset,
                y: self.bbox.top + inset,
                width: self.bbox.width() - inset * 2.0,
                height: self.bbox.height() - inset * 2.0,
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
        for (codes, baseline) in lines.iter().zip(self.baselines(lines.len())) {
            if codes.is_empty() {
                continue;
            }
            operators.push(Op::SetTextMatrix {
                matrix: Transform::from_translate(self.line_start(self.width(codes)), baseline),
            });
            operators.push(Op::ShowText {
                text: PdfTextItem::Text(codes.as_slice().into()),
            });
        }
        operators.extend([Op::EndText, Op::RestoreGraphicsState]);
        operators
    }
}

/// `operators` with the text objects of the first `/Tx` marked content removed and
/// `value` drawn at its end, so the backgrounds and borders some appearances draw inside
/// it remain. Without `/Tx` marked content, every text object is removed and `value` is
/// appended in marked content of its own.
fn replace_variable_text(operators: &[Op], value: Vec<Op>) -> Vec<Op> {
    let Some(start) = operators.iter().position(|operator| match operator {
        Op::BeginMarkedContent { tag } => tag.as_bytes() == VARIABLE_TEXT_TAG,
        Op::BeginMarkedContentWithProps(begin) => begin.tag() == VARIABLE_TEXT_TAG,
        _ => false,
    }) else {
        return without_text_objects(operators)
            .chain([&Op::BeginMarkedContent {
                tag: PdfName::from(VARIABLE_TEXT_TAG),
            }])
            .cloned()
            .chain(value)
            .chain([Op::EndMarkedContent])
            .collect();
    };
    let end = start.saturating_add(marked_content_len(
        operators.get(start..).unwrap_or_default(),
    ));
    let before = operators.get(..start).unwrap_or_default();
    let span = operators.get(start..end).unwrap_or_default();
    let after = operators.get(end..).unwrap_or_default();
    before
        .iter()
        .chain(without_text_objects(span))
        .cloned()
        .chain(value)
        .chain(after.iter().cloned())
        .collect()
}

/// The number of operators from the marked-content begin opening `operators` up to,
/// not including, the end that closes it; all of them when it is not closed.
fn marked_content_len(operators: &[Op]) -> usize {
    let mut depth = 0_usize;
    operators
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
        .unwrap_or(operators.len())
}

/// `operators` without the operators from each `BT` through its `ET`.
fn without_text_objects(operators: &[Op]) -> impl Iterator<Item = &Op> {
    let mut in_text = false;
    operators.iter().filter(move |operator| {
        let inside = in_text || matches!(operator, Op::BeginText);
        in_text = inside && !matches!(operator, Op::EndText);
        !inside
    })
}
