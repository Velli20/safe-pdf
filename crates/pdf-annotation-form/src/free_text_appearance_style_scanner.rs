//! Interprets typography and fill state from a FreeText appearance stream.

use pdf_content_stream_operators::{
    color_operators::SetRGBFill, text_state_operators::SetFont, variants::PdfOperatorVariant,
};
use pdf_font::encoding::FontEncoding;
use pdf_graphics::color::Color;
use pdf_resources::form::FormXObject;

use crate::{FreeTextFont, FreeTextStyle};

/// Tracks the editable appearance state encountered while scanning PDF operators.
pub(super) struct FreeTextAppearanceStyleScanner<'a> {
    /// Form whose operators and font resources are being inspected.
    form: &'a FormXObject,
    /// Portable style defaults and annotation metadata awaiting refinement.
    style: FreeTextStyle,
    /// Whether subsequent text-state operators occur inside a text object.
    inside_text: bool,
    /// Most recent valid nonstroking RGB color in inherited graphics state.
    active_fill: Option<Color>,
    /// Most recent valid fill color selected for visible text.
    text_color: Option<Color>,
    /// Most recent valid Standard 14 font selected inside a text object.
    font: Option<FreeTextFont>,
    /// Size associated with the most recent valid font selection.
    font_size: Option<f32>,
    /// Most recent valid leading selected inside a text object.
    line_height: Option<f32>,
}

impl<'a> FreeTextAppearanceStyleScanner<'a> {
    /// Creates a scanner over one normal appearance form.
    pub(super) fn new(form: &'a FormXObject, style: FreeTextStyle) -> Self {
        Self {
            form,
            style,
            inside_text: false,
            active_fill: None,
            text_color: None,
            font: None,
            font_size: None,
            line_height: None,
        }
    }

    /// Scans all operators and returns the recovered editable style.
    pub(super) fn scan(mut self) -> FreeTextStyle {
        let form = self.form;
        for operator in &form.content_stream.operators {
            self.apply_operator(operator);
        }
        self.finish()
    }

    /// Updates scanner state for one appearance-stream operator.
    fn apply_operator(&mut self, operator: &PdfOperatorVariant) {
        match operator {
            PdfOperatorVariant::BeginText(_) => self.begin_text(),
            PdfOperatorVariant::EndText(_) => self.inside_text = false,
            PdfOperatorVariant::SetFont(set_font) if self.inside_text => {
                self.apply_font(set_font);
            }
            PdfOperatorVariant::SetRGBFill(fill) => self.apply_fill(fill),
            PdfOperatorVariant::SetLeading(leading) if self.inside_text => {
                self.apply_leading(leading.leading());
            }
            _ => {}
        }
    }

    /// Enters a text object and inherits the active graphics-state fill color.
    fn begin_text(&mut self) {
        self.inside_text = true;
        self.text_color = self.active_fill;
    }

    /// Records a valid RGB fill and applies it to text when appropriate.
    fn apply_fill(&mut self, fill: &SetRGBFill) {
        let channels = fill.components();
        if !Self::valid_rgb(channels) {
            return;
        }
        let [red, green, blue] = channels;
        let color = Color::from_rgb(red, green, blue);
        self.active_fill = Some(color);
        if self.inside_text {
            self.text_color = Some(color);
        }
    }

    /// Replaces the current font selection, clearing it when the operator is invalid.
    fn apply_font(&mut self, set_font: &SetFont) {
        self.font = None;
        self.font_size = None;
        let size = set_font.size();
        if !size.is_finite() || size <= 0.0 {
            return;
        }
        let Some(font) = self.standard14_font(set_font) else {
            return;
        };
        self.font = Some(font);
        self.font_size = Some(size);
    }

    /// Resolves a font operator through the form resources as a Standard 14 font.
    fn standard14_font(&self, set_font: &SetFont) -> Option<FreeTextFont> {
        let resources = self.form.resources.as_ref()?;
        let font_resource = resources.fonts.get(set_font.name())?.as_font()?;
        let standard14 = font_resource.0.as_standard14()?;
        Some(FreeTextFont {
            standard14,
            resource_name: set_font.name().to_owned(),
            encoding: FontEncoding::WinAnsi,
        })
    }

    /// Records finite, positive text leading while inside a text object.
    fn apply_leading(&mut self, leading: f32) {
        if leading.is_finite() && leading > 0.0 {
            self.line_height = Some(leading);
        }
    }

    /// Returns whether every RGB channel is finite and within the PDF range.
    fn valid_rgb(channels: [f32; 3]) -> bool {
        channels
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }

    /// Applies recovered state and computes portable leading when none was explicit.
    fn finish(mut self) -> FreeTextStyle {
        if let Some(font) = self.font
            && let Some(font_size) = self.font_size
        {
            self.style.font = font;
            self.style.font_size = font_size;
        }
        if let Some(color) = self.text_color {
            self.style.text_color = color;
        }
        self.style.line_height = self.line_height.unwrap_or(self.style.font_size * 1.2);
        self.style
    }
}
