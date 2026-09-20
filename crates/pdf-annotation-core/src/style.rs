//! Device-independent annotation styling; appearance streams are never executed.
use crate::error::ValidationError;
use crate::pdf_data::{NativeAnnotation, SourceAnnotation};
use num_traits::ToPrimitive;
use pdf_graphics::color::Color;
use serde::{Deserialize, Serialize};

/// Stroke width used when a resolved style has no positive border width.
const DEFAULT_STROKE_WIDTH: f64 = 1.0;

/// Fill and stroke of one shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ShapePaint {
    /// Outline color whose alpha is the outline opacity; `None` draws no outline.
    pub stroke: Option<Color>,
    /// Interior color whose alpha is the interior opacity; `None` leaves the interior unpainted.
    pub fill: Option<Color>,
    /// Outline width in local units.
    pub width: f64,
    /// Dash lengths; empty means solid.
    pub dash: Vec<f64>,
    /// Round caps and joins instead of the host default.
    pub round: bool,
}

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
    /// Opaque text or stroke sRGB color with channels in the range zero through one.
    pub color: Color,
    /// Optional opaque interior fill.
    pub background: Option<Color>,
    /// Optional opaque border color.
    pub border_color: Option<Color>,
    /// Nonnegative border width in page units.
    pub border_width: f64,
    /// Nonnegative dash lengths; empty means solid.
    pub dash: Vec<f64>,
    /// Opacity in the range zero through one.
    pub opacity: f64,
    /// Positive font size in page units.
    pub font_size: f64,
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
            dash: Vec::new(),
            opacity: 1.0,
            font_size: 12.0,
            font_family: "sans-serif".into(),
            alignment: Alignment::Left,
        }
    }
}

impl ResolvedStyle {
    /// Style from a compact payload color, whose alpha is the opacity, and stroke width.
    /// The color is made opaque; other fields keep their defaults.
    pub fn from_compact(color: Color, width: f64) -> Self {
        Self {
            color: Color::from_rgb(color.r, color.g, color.b),
            opacity: f64::from(color.a),
            border_width: width,
            ..Self::default()
        }
    }

    /// Merges the style color and opacity into one RGBA color.
    ///
    /// Returns `InvalidStyle` when the opacity has no finite `f32` representation.
    pub fn rgba_color(&self) -> Result<Color, ValidationError> {
        let opacity = self
            .opacity
            .to_f32()
            .filter(|v| v.is_finite())
            .ok_or(ValidationError::InvalidStyle { field: "opacity" })?;
        Ok(Color::from_rgba(
            self.color.r,
            self.color.g,
            self.color.b,
            opacity,
        ))
    }

    /// Returns the positive border width, or one page unit when no positive width is set.
    /// This preserves the outline fallback; it does not validate the style.
    pub fn stroke_width(&self) -> f64 {
        if self.border_width > 0.0 {
            self.border_width
        } else {
            DEFAULT_STROKE_WIDTH
        }
    }

    /// Standard 14 font resource name for the generic font family, inverting the
    /// aliasing done by [`apply_default_style`]; unknown families use Helvetica.
    pub fn font_resource_name(&self) -> &'static [u8] {
        match self.font_family.as_str() {
            "monospace" => b"Cour",
            "serif" => b"TiRo",
            _ => b"Helv",
        }
    }

    /// Builds an outline using the style color and, when `fill` is true, its background.
    ///
    /// Paint colors are made opaque because the host applies annotation opacity
    /// separately. The paint uses [`Self::stroke_width`], copies the dash pattern,
    /// and leaves caps and joins at the host default. This does not validate the style.
    pub fn outline(&self, fill: bool) -> ShapePaint {
        ShapePaint {
            stroke: Some(Color {
                a: 1.0,
                ..self.color
            }),
            fill: self
                .background
                .filter(|_| fill)
                .map(|color| Color { a: 1.0, ..color }),
            width: self.stroke_width(),
            dash: self.dash.clone(),
            round: false,
        }
    }

    /// Validate colors, widths, opacity, and text size before an edit commits.
    ///
    /// Returns `InvalidStyle` for invalid values; does not panic.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let colors = [Some(self.color), self.background, self.border_color]
            .into_iter()
            .flatten()
            .flat_map(|color| [color.r, color.g, color.b]);
        if !colors
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || !self.border_width.is_finite()
            || self.border_width < 0.0
            || self.dash.iter().any(|v| !v.is_finite() || *v < 0.0)
            || !self.font_size.is_finite()
            || self.font_size <= 0.0
        {
            return Err(ValidationError::InvalidStyle {
                field: "resolved style",
            });
        }
        Ok(())
    }
}

/// Normalize a retained source color into an opaque style color.
///
/// A fully transparent color (alpha zero, as retained for an empty PDF color
/// array) yields `None`. Returns `InvalidStyle` for nonfinite channels. Channels
/// are clamped to the range zero through one; this does not quantize to
/// eight-bit channels and does not panic.
fn opaque(color: &Color) -> Result<Option<Color>, ValidationError> {
    let channels = [color.r, color.g, color.b, color.a];
    if channels.iter().any(|v| !v.is_finite()) {
        return Err(ValidationError::InvalidStyle {
            field: "device color",
        });
    }
    if color.a == 0.0 {
        return Ok(None);
    }
    let [r, g, b] = [color.r, color.g, color.b].map(|v| v.clamp(0.0, 1.0));
    Ok(Some(Color::from_rgb(r, g, b)))
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
pub fn apply_default_style(bytes: &[u8], style: &mut ResolvedStyle) {
    let text = String::from_utf8_lossy(bytes);
    let mut operands: Vec<&str> = Vec::new();
    for token in text.lines().flat_map(|line| {
        line.split('%')
            .next()
            .unwrap_or_default()
            .split_ascii_whitespace()
    }) {
        match token {
            "Tf" => {
                if let [name, size] = operands.as_slice() {
                    if let Ok(size) = size.parse::<f64>()
                        && size.is_finite()
                        && size > 0.0
                    {
                        style.font_size = size;
                    }
                    style.font_family =
                        generic_font_family(name.strip_prefix('/').unwrap_or(name).as_bytes())
                            .into();
                }
                operands.clear();
            }
            "g" | "rg" | "k" => {
                if let Ok(components) = operands
                    .iter()
                    .map(|v| v.parse::<f32>())
                    .collect::<Result<Vec<_>, _>>()
                    && let Some(color) = Color::from_device_components(&components)
                    && let Ok(Some(color)) = opaque(&color)
                {
                    style.color = color;
                }
                operands.clear();
            }
            value if value.starts_with('/') || value.parse::<f64>().is_ok() => {
                if operands.len() < 4 {
                    operands.push(value);
                } else {
                    operands.clear();
                }
            }
            _ => operands.clear(),
        }
    }
}

/// Resolve legacy style precedence from retained `annotation` metadata.
///
/// Common color/border precede subtype border/interior values, then widget `/MK`
/// and text `/DA` and `/Q`. Missing highlight color is yellow. This is an explicit
/// source-inspection helper: callers must not overwrite edited Core styles with it.
/// Returns typed validation errors for unusable styles; does not panic.
pub fn resolve_source_style(
    annotation: &SourceAnnotation,
) -> Result<ResolvedStyle, ValidationError> {
    crate::validation::validate(annotation)?;
    let mut style = ResolvedStyle {
        opacity: annotation.opacity,
        ..ResolvedStyle::default()
    };
    if let Some(color) = &annotation.color {
        if let Some(color) = opaque(color)? {
            style.color = color;
            style.border_color = Some(color);
        }
    } else if matches!(annotation.kind, NativeAnnotation::Highlight(_)) {
        style.color = Color::from_rgb(1.0, 1.0, 0.0);
    }
    if let Some(border) = &annotation.border {
        style.border_width = border.width.max(0.0);
        style.dash = border
            .dash_pattern
            .as_ref()
            .map(|pattern| pattern.intervals.iter().copied().map(f64::from).collect())
            .unwrap_or_default();
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
        style.border_width = border.resolved_width()?;
        style.dash = border.dash_pattern.clone().unwrap_or_default();
    }
    style.background = interior.map(opaque).transpose()?.flatten();
    let (appearance, quadding) = match &annotation.kind {
        NativeAnnotation::Widget(v) => {
            if let Some(mk) = &v.appearance_characteristics {
                style.background = mk
                    .background_color
                    .as_ref()
                    .map(opaque)
                    .transpose()?
                    .flatten();
                style.border_color = mk.border_color.as_ref().map(opaque).transpose()?.flatten();
            }
            (v.default_appearance.as_deref(), v.quadding)
        }
        NativeAnnotation::FreeText(v) => (v.default_appearance.as_deref(), v.quadding),
        _ => (None, None),
    };
    if let Some(bytes) = appearance {
        apply_default_style(bytes, &mut style);
    }
    style.alignment = match quadding {
        Some(1) => Alignment::Center,
        Some(2) => Alignment::Right,
        _ => Alignment::Left,
    };
    style.validate()?;
    Ok(style)
}
