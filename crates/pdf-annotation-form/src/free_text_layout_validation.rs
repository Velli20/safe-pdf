//! Validation and normalization of free-text layout input.

use pdf_font::{font::Font, true_type_font::TrueTypeFont};
use pdf_graphics::{color::Color, rect::Rect};

use crate::{FreeText, FreeTextEditError, FreeTextOverflow, FreeTextStyle};

/// A free-text annotation whose layout inputs are ready for wrapping.
pub(super) struct ValidatedFreeText<'a> {
    /// The normalized and validated annotation rectangle.
    rect: Rect,
    /// The validated appearance style.
    style: &'a FreeTextStyle,
    /// The source text encoded for the selected PDF font.
    encoded_text: Vec<u8>,
    /// The font used for text measurement and rendering.
    font: Font,
    /// The maximum width allowed for a wrapped line.
    maximum_line_width: f32,
    /// The number of characters in the source text.
    character_count: usize,
}

impl<'a> TryFrom<&'a FreeText> for ValidatedFreeText<'a> {
    type Error = FreeTextEditError;

    /// Validates an editable annotation and prepares its font-dependent input.
    fn try_from(free_text: &'a FreeText) -> Result<Self, Self::Error> {
        StyleValidator::new(&free_text.style).validate()?;
        let rectangle = ValidatedRectangle::try_from(free_text.rect)?;
        let encoded_text = free_text.style.font.encoding.encode(&free_text.text)?;
        let font = Font::TrueType(TrueTypeFont::synthetic_standard14_font(
            free_text.style.font.standard14,
        ));
        let maximum_line_width = match free_text.style.overflow {
            FreeTextOverflow::ExpandRight => f32::INFINITY,
            FreeTextOverflow::ExpandHeight | FreeTextOverflow::Reject => {
                rectangle.content_width(free_text.style.insets)?
            }
        };

        Ok(Self {
            rect: rectangle.get(),
            style: &free_text.style,
            encoded_text,
            font,
            maximum_line_width,
            character_count: free_text.text.chars().count(),
        })
    }
}

impl<'a> ValidatedFreeText<'a> {
    /// Returns the normalized annotation rectangle.
    pub(super) fn rect(&self) -> Rect {
        self.rect
    }

    /// Returns the validated appearance style.
    pub(super) fn style(&self) -> &'a FreeTextStyle {
        self.style
    }

    /// Returns the font used to measure the encoded text.
    pub(super) fn font(&self) -> &Font {
        &self.font
    }

    /// Returns the encoded source text.
    pub(super) fn encoded_text(&self) -> &[u8] {
        &self.encoded_text
    }

    /// Returns the maximum width permitted for one wrapped line.
    pub(super) fn maximum_line_width(&self) -> f32 {
        self.maximum_line_width
    }

    /// Returns the number of characters accepted as cursor positions.
    pub(super) fn character_count(&self) -> usize {
        self.character_count
    }

    /// Consumes the validated input and returns its measurement font.
    pub(super) fn into_font(self) -> Font {
        self.font
    }
}

/// A normalized rectangle known to have finite, positive dimensions.
struct ValidatedRectangle {
    /// The normalized PDF rectangle.
    rect: Rect,
}

impl TryFrom<Rect> for ValidatedRectangle {
    type Error = FreeTextEditError;

    /// Normalizes a rectangle and rejects unusable coordinates or dimensions.
    fn try_from(rect: Rect) -> Result<Self, Self::Error> {
        let rect = rect.normalized();
        if rect.is_valid() {
            Ok(Self { rect })
        } else {
            Err(FreeTextEditError::invalid_input(
                "rectangle",
                "coordinates must be finite with positive width and height",
            ))
        }
    }
}

impl ValidatedRectangle {
    /// Returns the normalized rectangle.
    fn get(&self) -> Rect {
        self.rect
    }

    /// Returns the horizontal text area remaining after applying insets.
    fn content_width(&self, insets: Rect) -> Result<f32, FreeTextEditError> {
        let width = self.rect.width() - insets.left - insets.right;
        if width.is_finite() && width > 0.0 {
            Ok(width)
        } else {
            Err(FreeTextEditError::invalid_input(
                "insets",
                "insets leave no horizontal text area",
            ))
        }
    }
}

/// Validates every style value used by layout or appearance generation.
struct StyleValidator<'a> {
    /// The style being validated.
    style: &'a FreeTextStyle,
}

impl<'a> StyleValidator<'a> {
    /// Creates a validator for one style.
    fn new(style: &'a FreeTextStyle) -> Self {
        Self { style }
    }

    /// Validates the complete style while preserving stable error reporting.
    fn validate(&self) -> Result<(), FreeTextEditError> {
        self.validate_resource_name()?;
        self.validate_typography()?;
        self.validate_insets()?;
        self.validate_colors()?;
        self.validate_border()
    }

    /// Validates the PDF font resource name.
    fn validate_resource_name(&self) -> Result<(), FreeTextEditError> {
        let name = &self.style.font.resource_name;
        if name.is_empty()
            || name
                .iter()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        {
            Err(FreeTextEditError::invalid_input(
                "font resource name",
                "name must be a non-empty PDF resource name",
            ))
        } else {
            Ok(())
        }
    }

    /// Validates font size and line spacing.
    fn validate_typography(&self) -> Result<(), FreeTextEditError> {
        Self::require_positive("font size", self.style.font_size)?;
        Self::require_positive("line height", self.style.line_height)
    }

    /// Validates all text-area insets.
    fn validate_insets(&self) -> Result<(), FreeTextEditError> {
        Self::require_non_negative("left inset", self.style.insets.left)?;
        Self::require_non_negative("top inset", self.style.insets.top)?;
        Self::require_non_negative("right inset", self.style.insets.right)?;
        Self::require_non_negative("bottom inset", self.style.insets.bottom)
    }

    /// Validates text and optional background colors.
    fn validate_colors(&self) -> Result<(), FreeTextEditError> {
        Self::require_opaque_color("text color", self.style.text_color)?;
        if let Some(color) = self.style.background_color {
            Self::require_opaque_color("background color", color)?;
        }
        Ok(())
    }

    /// Validates the optional border width and color.
    fn validate_border(&self) -> Result<(), FreeTextEditError> {
        if let Some(border) = self.style.border {
            Self::require_positive("border width", border.width)?;
            Self::require_opaque_color("border color", border.color)?;
        }
        Ok(())
    }

    /// Requires a finite number greater than zero.
    fn require_positive(field: &'static str, value: f32) -> Result<(), FreeTextEditError> {
        if value.is_finite() && value > 0.0 {
            Ok(())
        } else {
            Err(FreeTextEditError::invalid_input(
                field,
                "value must be finite and positive",
            ))
        }
    }

    /// Requires a finite number greater than or equal to zero.
    fn require_non_negative(field: &'static str, value: f32) -> Result<(), FreeTextEditError> {
        if value.is_finite() && value >= 0.0 {
            Ok(())
        } else {
            Err(FreeTextEditError::invalid_input(
                field,
                "value must be finite and non-negative",
            ))
        }
    }

    /// Requires finite, normalized RGB channels and full opacity.
    fn require_opaque_color(field: &'static str, color: Color) -> Result<(), FreeTextEditError> {
        let channels = [color.r, color.g, color.b, color.a];
        if channels
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
            && color.a == 1.0
        {
            Ok(())
        } else {
            Err(FreeTextEditError::invalid_input(
                field,
                "colors must be finite, opaque, and within 0.0..=1.0",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_preserves_specific_style_errors() {
        let mut free_text = FreeText {
            rect: Rect::new(100.0, 30.0),
            text: "text".to_owned(),
            style: FreeTextStyle::default(),
        };
        free_text.style.font.resource_name = Vec::from(b"bad name");

        assert!(matches!(
            ValidatedFreeText::try_from(&free_text),
            Err(FreeTextEditError::InvalidInput {
                field: "font resource name",
                reason: "name must be a non-empty PDF resource name",
            })
        ));

        free_text.style.font.resource_name = Vec::from(b"Helv");
        free_text.style.insets.left = -1.0;
        assert!(matches!(
            ValidatedFreeText::try_from(&free_text),
            Err(FreeTextEditError::InvalidInput {
                field: "left inset",
                reason: "value must be finite and non-negative",
            })
        ));
    }

    #[test]
    fn validation_rejects_rectangles_without_horizontal_text_space() {
        let mut free_text = FreeText {
            rect: Rect::new(4.0, 30.0),
            text: "text".to_owned(),
            style: FreeTextStyle::default(),
        };
        free_text.style.overflow = FreeTextOverflow::ExpandHeight;

        assert!(matches!(
            ValidatedFreeText::try_from(&free_text),
            Err(FreeTextEditError::InvalidInput {
                field: "insets",
                reason: "insets leave no horizontal text area",
            })
        ));
    }
}
