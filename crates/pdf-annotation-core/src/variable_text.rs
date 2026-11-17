//! The live value of a text-entry widget, which the host paints on the page canvas in
//! the widget's `/DA` font in place of the value its source appearance shows.
use crate::{
    fields::{ComboValue, Field, FieldKind, TextMode},
    models::Annotation,
    pdf_data::BorderStyleName,
    style::{Alignment, DefaultAppearance},
};
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
}

impl Annotation {
    /// The live value of this widget, bound to `field`, when the host paints it: an
    /// editable text field or editable combo box with a normal appearance and a `/DA`
    /// font. `None` for other widgets and annotations.
    pub(crate) fn variable_text(&self, field: &Field) -> Option<VariableText> {
        let definition = &field.definition;
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
                    ComboValue::Option(id) => combo
                        .options
                        .iter()
                        .find(|option| option.id == *id)
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
        })
    }

    /// Inset, in page units, of the box a text entry with a `border_width` border lays its
    /// value out in, as viewers lay form text out: the border width, doubled for a beveled
    /// or inset border.
    pub(crate) fn text_inset(&self, border_width: f32) -> f32 {
        let doubled = self
            .source_widget()
            .and_then(|widget| widget.border_style.as_ref())
            .and_then(|border| border.style.as_ref())
            .is_some_and(|style| {
                matches!(style, BorderStyleName::Beveled | BorderStyleName::Inset)
            });
        if doubled {
            border_width * 2.0
        } else {
            border_width
        }
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
}
