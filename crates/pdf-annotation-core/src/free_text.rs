//! Plain free-text editing without PDF appearance-stream generation.
use crate::{
    error::ValidationError,
    models::Point,
    pdf_data::BorderEffect,
    style::{Alignment, ResolvedStyle},
};
use pdf_graphics::{color::Color, polyline::Polyline, rect::Rect};
use serde::{Deserialize, Serialize};

/// Left, top, right, and bottom text insets used when the source supplies none.
const DEFAULT_INSETS: [f64; 4] = [2.0; 4];

/// Solid free-text border in page units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FreeTextBorder {
    /// Unquantized opaque sRGB color.
    pub color: Color,
    /// Nonnegative border width.
    pub width: f64,
}

/// Explicit native free-text style, independent of retained PDF source metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FreeTextStyle {
    /// Plain PDF font resource name; no slash or PDF delimiters.
    pub font_name: Vec<u8>,
    /// Strictly positive font size in page units.
    pub font_size: f64,
    /// Unquantized opaque text sRGB color.
    pub text_color: Color,
    /// Optional solid border.
    pub border: Option<FreeTextBorder>,
    /// Nonnegative left, top, right, bottom text insets, in that order.
    pub insets: [f64; 4],
    /// Horizontal text alignment.
    pub alignment: Alignment,
}

impl Default for FreeTextStyle {
    fn default() -> Self {
        Self {
            font_name: b"Helv".to_vec(),
            font_size: 12.0,
            text_color: Color::from_rgb(0.0, 0.0, 0.0),
            border: None,
            insets: DEFAULT_INSETS,
            alignment: Alignment::Left,
        }
    }
}

impl FreeTextStyle {
    /// Plain-text style carrying `style`'s font, color, border, and alignment.
    ///
    /// The border is present only when the style has a border color; `insets`
    /// default to two page units per side. Does not validate the result.
    pub fn from_resolved(style: &ResolvedStyle, insets: Option<[f64; 4]>) -> Self {
        Self {
            font_name: style.font_resource_name().to_vec(),
            font_size: style.font_size,
            text_color: style.color,
            border: style.border_color.map(|color| FreeTextBorder {
                color,
                width: style.border_width,
            }),
            insets: insets.unwrap_or(DEFAULT_INSETS),
            alignment: style.alignment,
        }
    }

    /// Overlays this style's font, size, color, alignment, and border onto `style`,
    /// inverting [`Self::from_resolved`]. Font interpretation uses the live font
    /// name, never a retained source `/DA`. Other fields of `style` are kept.
    pub fn apply_to(&self, style: &mut ResolvedStyle) {
        style.font_family = crate::style::generic_font_family(&self.font_name).into();
        style.font_size = self.font_size;
        style.color = self.text_color;
        style.alignment = self.alignment;
        style.border_color = self.border.as_ref().map(|b| b.color);
        style.border_width = self.border.as_ref().map_or(0.0, |b| b.width);
    }

    /// Validate the legacy plain-text style constraints.
    ///
    /// Font names must be nonempty printable ASCII without PDF delimiters, font
    /// sizes positive, text colors and insets finite/nonnegative, and border widths
    /// finite/nonnegative with finite border colors. Returns `InvalidStyle` for
    /// invalid input and never panics. Retained values are not quantized or clamped.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !self.font_size.is_finite()
            || self.font_size <= 0.0
            || self.font_name.is_empty()
            || self
                .font_name
                .iter()
                .any(|b| !b.is_ascii_graphic() || b"()<>[]{}/%#".contains(b))
            || [self.text_color.r, self.text_color.g, self.text_color.b]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0)
            || self.insets.iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.border.as_ref().is_some_and(|b| {
                !b.width.is_finite()
                    || b.width < 0.0
                    || [b.color.r, b.color.g, b.color.b]
                        .iter()
                        .any(|v| !v.is_finite())
            })
        {
            return Err(ValidationError::InvalidStyle { field: "free text" });
        }
        Ok(())
    }
}

/// Styled page text, distinct from an anchored comment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FreeText {
    /// Page-space placement.
    pub bounds: Rect<f64>,
    /// Current Unicode text.
    pub text: String,
    /// Current text and border style.
    pub style: FreeTextStyle,
    /// Rich text retained without claiming a rich-text editor or renderer.
    pub rich_contents: Option<Vec<u8>>,
    /// Optional open two- or three-point callout, translated with the annotation.
    pub callout: Option<Polyline<f64>>,
    /// Retained border effect, unsupported by the plain-text editor.
    pub border_effect: Option<BorderEffect>,
}

impl FreeText {
    /// Whether the legacy plain-text edit can preserve this annotation's semantics.
    /// This query does not mutate state or panic; annotation flags are checked separately.
    pub fn is_plain(&self) -> bool {
        self.rich_contents.is_none() && self.callout.is_none() && self.border_effect.is_none()
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if !self.bounds.is_valid() {
            return Err(ValidationError::InvalidBounds);
        }

        self.style.validate()?;
        if let Some(callout) = &self.callout {
            if callout.closed || !matches!(callout.points.len(), 2 | 3) {
                return Err(ValidationError::InvalidPath {
                    field: "free-text callout",
                });
            }
            for point in &callout.points {
                crate::models::validate_point(*point)?;
            }
        }
        crate::validation::validate(&self.border_effect)
    }

    pub(crate) fn translate(&mut self, delta: Point) {
        self.bounds.translate(delta.x, delta.y);
        for point in self
            .callout
            .iter_mut()
            .flat_map(|callout| &mut callout.points)
        {
            point.translate(delta.x, delta.y);
        }
    }
}
