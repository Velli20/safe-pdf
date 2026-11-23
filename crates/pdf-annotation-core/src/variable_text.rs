//! The live value of a text-entry widget, which the host paints on the page canvas in
//! the widget's `/DA` font in place of the value its source appearance shows.
use crate::{
    fields::{ComboValue, Field, FieldKind, TextMode},
    models::Annotation,
    pdf_data::BorderStyle,
    style::{Alignment, DefaultAppearance},
};
use num_traits::ToPrimitive;
use pdf_graphics::color::Color;
use std::sync::Arc;

/// A text-entry widget's live value and the `/DA` text state it is drawn with.
#[derive(Clone, Debug)]
pub struct VariableText {
    /// The text drawn; empty for a password field, whose value is never painted.
    pub value: String,
    /// The font resource name `/DA` selects, without its leading slash.
    pub font: Arc<[u8]>,
    /// Positive font size in page units, already fitted when `/DA` asks for auto size.
    pub font_size: f32,
    /// Text color.
    pub color: Color,
    /// Horizontal alignment from `/Q`.
    pub alignment: Alignment,
    /// Inset, in page units, of the box the value is laid out in from each side of the
    /// appearance box.
    pub inset: f32,
    /// Whether the value wraps onto several lines.
    pub multiline: bool,
    /// The number of equally wide cells a comb field spaces its characters into, one each;
    /// `None` for other fields.
    pub comb: Option<u32>,
}

impl VariableText {
    /// This text with its value left out, for an entry whose control shows the value.
    #[must_use]
    pub fn without_value(self) -> Self {
        Self {
            value: String::new(),
            ..self
        }
    }

    /// The width of each cell of a comb field whose appearance box is `width` wide: the
    /// box less the text inset on each side, divided evenly between the cells. `None` for
    /// other fields.
    #[must_use]
    pub fn comb_cell_width(&self, width: f32) -> Option<f32> {
        let cells = self.comb?.to_f32()?;
        Some((width - self.inset * 2.0) / cells)
    }
}

impl Annotation {
    /// The live value of this widget, bound to `field`, when the host paints it: an
    /// editable text field or editable combo box with a normal appearance and a `/DA`
    /// font. `None` for other widgets and annotations.
    pub(crate) fn variable_text(&self, field: &Field) -> Option<VariableText> {
        let definition = &field.definition;
        let comb = self.comb_cells(field);
        let (value, multiline) = match &definition.content {
            FieldKind::Text(_) if definition.read_only => return None,
            FieldKind::Text(text) => match text.mode {
                TextMode::Password => (String::new(), false),
                TextMode::Multiline => (text.value.clone(), true),
                TextMode::SingleLine => (text.value.clone(), false),
            },
            FieldKind::ComboBox(combo) if combo.editable => {
                let value = match &combo.value {
                    ComboValue::Text(text) => text.clone(),
                    ComboValue::Option(_) => combo
                        .selected_option()
                        .map(|option| option.label.clone())
                        .unwrap_or_default(),
                    ComboValue::Empty => String::new(),
                };
                (value, false)
            }
            _ => return None,
        };
        if !self.has_normal_appearance() {
            return None;
        }
        let font =
            DefaultAppearance::parse(self.source_widget()?.default_appearance.as_deref()?).font?;
        let style = self.style().ok()?;
        Some(VariableText {
            value,
            font,
            font_size: style.font_size,
            color: style.color,
            alignment: style.alignment,
            inset: self.text_inset(style.border_width),
            multiline,
            comb,
        })
    }

    /// Inset, in page units, of the box a text entry with a `border_width` border lays its
    /// value out in, as viewers lay form text out: the border width, doubled for a beveled
    /// or inset border.
    pub(crate) fn text_inset(&self, border_width: f32) -> f32 {
        let three_dimensional = self
            .source_widget()
            .and_then(|widget| widget.border_style.as_ref())
            .is_some_and(BorderStyle::is_three_dimensional);
        if three_dimensional {
            border_width * 2.0
        } else {
            border_width
        }
    }

    /// The cells of this widget, bound to `field`, when it is a comb field: a single-line
    /// text field with the comb flag and a positive `/MaxLen`, which is its cell count.
    pub(crate) fn comb_cells(&self, field: &Field) -> Option<u32> {
        let FieldKind::Text(text) = &field.definition.content else {
            return None;
        };
        (text.mode == TextMode::SingleLine && self.source_widget()?.is_comb())
            .then_some(text.max_length?)
            .filter(|cells| *cells > 0)
    }

    /// Whether `field`, this widget's bound text field, still holds the source `/V`, which
    /// the widget's source appearance shows.
    pub(crate) fn shows_source_value(&self, field: &Field) -> bool {
        let FieldKind::Text(text) = &field.definition.content else {
            return false;
        };
        self.source_widget()
            .and_then(|widget| widget.text_value().ok())
            .is_some_and(|value| value == text.value)
    }

    /// Whether `field`, this widget's bound list box, still holds the source selection,
    /// which the widget's source appearance shows.
    pub(crate) fn shows_source_selection(&self, field: &Field) -> bool {
        matches!(field.definition.content, FieldKind::ListBox(_))
            && self
                .source_widget()
                .and_then(|widget| widget.choice_field().ok())
                .is_some_and(|source| source == field.definition.content)
    }
}
