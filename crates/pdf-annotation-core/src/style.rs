//! Device-independent annotation styling; appearance streams are never executed.
use crate::error::SourceAdoptionError;
use crate::fields::TextMode;
use crate::pdf_data::{NativeAnnotation, SourceAnnotation, WidgetAnnotation};
use num_traits::ToPrimitive;
use pdf_graphics::{color::Color, rect::Rect};
use serde::{Deserialize, Serialize};

/// Stroke width used when a resolved style has no positive border width.
const DEFAULT_STROKE_WIDTH: f32 = 1.0;

/// Line height, in ems, an auto-sized single-line widget must fit within its height.
const AUTO_SIZE_LINE_FACTOR: f32 = 1.35;

/// Estimated average glyph advance, in ems, used to fit a push-button caption's width.
const AVERAGE_GLYPH_EM: f32 = 0.6;

/// Horizontal alignment of annotation text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// Align to the left edge.
    #[default]
    Left,
    /// Center each line.
    Center,
    /// Align to the right edge.
    Right,
}

/// Native style in page units, retaining floating-point color precision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ResolvedStyle {
    /// Text or stroke sRGB color with channels in the range zero through one; transparent
    /// for an empty PDF color array.
    pub color: Color,
    /// Optional interior fill.
    pub background: Option<Color>,
    /// Optional border color.
    pub border_color: Option<Color>,
    /// Nonnegative border width in page units.
    pub border_width: f32,
    /// Nonnegative dash lengths; `None` is solid.
    pub dash: Option<Vec<f32>>,
    /// Opacity in the range zero through one.
    pub opacity: f32,
    /// Positive font size in page units.
    pub font_size: f32,
    /// Host-facing font family or generic fallback.
    pub font_family: String,
    /// Horizontal text alignment.
    pub alignment: Alignment,
}

impl Default for ResolvedStyle {
    fn default() -> Self {
        Self {
            color: Color::from_rgb(0.0, 0.0, 0.0),
            background: None,
            border_color: None,
            border_width: 0.0,
            dash: None,
            opacity: 1.0,
            font_size: 12.0,
            font_family: "sans-serif".into(),
            alignment: Alignment::Left,
        }
    }
}

impl ResolvedStyle {
    /// Returns the positive border width, or one page unit when no positive width is set.
    /// This preserves the outline fallback; it does not validate the style.
    pub fn stroke_width(&self) -> f32 {
        if self.border_width > 0.0 {
            self.border_width
        } else {
            DEFAULT_STROKE_WIDTH
        }
    }
}

/// Generic font family for a Standard 14 resource name without its leading slash;
/// unknown resources use sans-serif.
pub(crate) fn generic_font_family(name: &[u8]) -> &'static str {
    match name {
        b"Courier" | b"Courier-Bold" | b"Courier-Oblique" | b"Courier-BoldOblique" | b"Cour" => {
            "monospace"
        }
        b"Times-Roman" | b"Times-Bold" | b"Times-Italic" | b"Times-BoldItalic" | b"TiRo" => "serif",
        _ => "sans-serif",
    }
}

/// Interpret only `/DA` font and device-color tokens into `style`.
///
/// Unknown operators clear pending operands. Invalid operands leave the previous
/// style value intact. Standard 14 aliases resolve to generic families; unknown
/// resources use sans-serif. This function performs no resource access and does
/// not panic. It does not validate unrelated preexisting style fields.
///
/// Returns whether the last `Tf` requested automatic sizing with a zero font size;
/// the caller decides the size because it depends on the annotation's geometry.
pub fn apply_default_style(bytes: &[u8], style: &mut ResolvedStyle) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let mut operands: Vec<&str> = Vec::new();
    let mut auto_size = false;
    for token in text.lines().flat_map(|line| {
        line.split('%')
            .next()
            .unwrap_or_default()
            .split_ascii_whitespace()
    }) {
        match token {
            "Tf" => {
                if let [name, size] = operands.as_slice() {
                    let size = size.parse::<f32>().ok().filter(|size| size.is_finite());
                    auto_size = size == Some(0.0);
                    if let Some(size) = size.filter(|size| *size > 0.0) {
                        style.font_size = size;
                    }
                    style.font_family =
                        generic_font_family(name.strip_prefix('/').unwrap_or(name).as_bytes())
                            .into();
                }
                operands.clear();
            }
            "g" | "rg" | "k" => {
                if let Some(components) = operands
                    .iter()
                    .map(|v| v.parse::<f32>().ok().filter(|v| v.is_finite()))
                    .collect::<Option<Vec<_>>>()
                    && let Some(color) = Color::from_device_components(&components)
                {
                    style.color = color;
                }
                operands.clear();
            }
            value if value.starts_with('/') || value.parse::<f32>().is_ok() => {
                if operands.len() < 4 {
                    operands.push(value);
                } else {
                    operands.clear();
                }
            }
            _ => operands.clear(),
        }
    }
    auto_size
}

impl WidgetAnnotation {
    /// Font size for a `/DA` auto size (`0 Tf`) that fits the text inside `rect` less
    /// `border_width` on each side, or `None` to keep the default size.
    ///
    /// Only single-line text, combo boxes, and push buttons are auto-sized; a push
    /// button's `/MK /CA` caption must also fit the width. Returns `None` when the
    /// border leaves no room. Does not panic.
    pub fn auto_font_size(&self, rect: &Rect, border_width: f32) -> Option<f32> {
        let single_line = match self.field_type.as_deref() {
            Some(b"Tx") => self.text_mode() != TextMode::Multiline,
            Some(b"Ch") => self.is_combo_box(),
            Some(b"Btn") => self.is_push_button(),
            _ => false,
        };
        if !single_line {
            return None;
        }
        let inset = border_width * 2.0;
        let mut size = (rect.height() - inset) / AUTO_SIZE_LINE_FACTOR;
        let caption_chars = self
            .is_push_button()
            .then_some(self.appearance_characteristics.as_ref())
            .flatten()
            .and_then(|mk| mk.normal_caption.as_deref())
            .map(|caption| String::from_utf8_lossy(caption).chars().count())
            .and_then(|count| count.to_f32())
            .filter(|count| *count > 0.0);
        if let Some(count) = caption_chars {
            size = size.min((rect.width() - inset) / (AVERAGE_GLYPH_EM * count));
        }
        (size.is_finite() && size > 0.0).then_some(size)
    }
}

/// Resolve legacy style precedence from retained `annotation` metadata.
///
/// Common color/border precede subtype border/interior values, then widget `/MK`
/// and text `/DA` and `/Q`. Missing highlight color is yellow. This is an explicit
/// source-inspection helper: callers must not overwrite edited Core styles with it.
/// Returns `Validation` when the resolved style is unusable; does not panic.
pub fn resolve_source_style(
    annotation: &SourceAnnotation,
) -> Result<ResolvedStyle, SourceAdoptionError> {
    let mut style = ResolvedStyle {
        opacity: annotation.opacity,
        ..ResolvedStyle::default()
    };
    if let Some(color) = annotation.color {
        style.color = color;
        style.border_color = Some(color);
    } else if matches!(annotation.kind, NativeAnnotation::Highlight(_)) {
        style.color = Color::from_rgb(1.0, 1.0, 0.0);
    }
    if let Some(border) = &annotation.border {
        style.border_width = border.width.max(0.0);
        style.dash = border
            .dash_pattern
            .as_ref()
            .map(|pattern| pattern.intervals.clone());
    }
    let (border, interior) = match &annotation.kind {
        NativeAnnotation::Square(v) => (v.border_style.as_ref(), v.interior_color.as_ref()),
        NativeAnnotation::Circle(v) => (v.border_style.as_ref(), v.interior_color.as_ref()),
        NativeAnnotation::Line(v) => (v.border_style.as_ref(), v.interior_color.as_ref()),
        NativeAnnotation::Polygon(v) => (v.border_style.as_ref(), v.interior_color.as_ref()),
        NativeAnnotation::PolyLine(v) => (v.border_style.as_ref(), v.interior_color.as_ref()),
        NativeAnnotation::Ink(v) => (v.border_style.as_ref(), v.interior_color.as_ref()),
        NativeAnnotation::Widget(v) => (v.border_style.as_ref(), None),
        _ => (None, None),
    };
    if let Some(border) = border {
        style.border_width = border.resolved_width();
        style.dash = border.dash_pattern.clone();
    }
    style.background = interior.copied();
    let mut widget = None;
    let (appearance, quadding) = match &annotation.kind {
        NativeAnnotation::Widget(v) => {
            widget = Some(v.as_ref());
            if let Some(mk) = &v.appearance_characteristics {
                style.background = mk.background_color;
                style.border_color = mk.border_color;
            }
            (v.default_appearance.as_deref(), v.quadding)
        }
        NativeAnnotation::FreeText(v) => (v.default_appearance.as_deref(), v.quadding),
        _ => (None, None),
    };
    let auto_size = appearance.is_some_and(|bytes| apply_default_style(bytes, &mut style));
    if auto_size
        && let Some(widget) = widget
        && let Ok(rect) = annotation.bounds()
        && let Some(size) = widget.auto_font_size(&rect, style.border_width)
    {
        style.font_size = size;
    }
    // Viewers center push-button captions regardless of `/Q`.
    let push_button = widget.is_some_and(|w| w.is_button() && w.is_push_button());
    style.alignment = match quadding {
        _ if push_button => Alignment::Center,
        Some(1) => Alignment::Center,
        Some(2) => Alignment::Right,
        _ => Alignment::Left,
    };
    style.validate()?;
    Ok(style)
}
