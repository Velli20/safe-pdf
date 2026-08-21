//! Builds PDF annotations from validated editable free-text input.

use pdf_annotation_types::{Annotation, AnnotationBorder, AnnotationColor, FreeTextAnnotation};
use pdf_font::text_string;
use pdf_graphics::color::Color;

use crate::{
    FreeText, FreeTextEditError, FreeTextStyle,
    free_text_appearance_stream_builder::FreeTextAppearanceStreamBuilder,
    free_text_layout::FreeTextLayout,
};

/// Owns the editable input while assembling its generated annotation.
pub(super) struct FreeTextAnnotationGenerator {
    /// Editable text, geometry, and style supplied by the caller.
    free_text: FreeText,
}

impl FreeTextAnnotationGenerator {
    /// Creates a generator for one editable free-text annotation.
    pub(super) fn new(free_text: FreeText) -> Self {
        Self { free_text }
    }

    /// Validates the input and builds the complete PDF annotation.
    pub(super) fn generate(self) -> Result<Annotation, FreeTextEditError> {
        let layout = FreeTextLayout::new(&self.free_text)?;
        let grown_rect = layout.grown_rect()?;
        let appearance =
            FreeTextAppearanceStreamBuilder::new(layout, grown_rect, &self.free_text.style).build();
        let border = Self::annotation_border(&self.free_text.style);
        let color = Self::annotation_color(&self.free_text.style);
        let annotation_data = Self::annotation_data(&self.free_text.style);

        Ok(Annotation::new_free_text(
            grown_rect,
            text_string::encode(&self.free_text.text),
            appearance,
            border,
            color,
            annotation_data,
        ))
    }

    /// Converts editable border styling to the annotation `/Border` value.
    fn annotation_border(style: &FreeTextStyle) -> Option<AnnotationBorder> {
        style.border.map(|border| AnnotationBorder {
            horizontal_radius: 0.0,
            vertical_radius: 0.0,
            width: border.width,
            dash_pattern: None,
        })
    }

    /// Converts editable border styling to the annotation color entry.
    fn annotation_color(style: &FreeTextStyle) -> Option<AnnotationColor> {
        style
            .border
            .map(|border| Self::color_components(border.color))
    }

    /// Converts an opaque RGB color to PDF annotation color components.
    fn color_components(color: Color) -> AnnotationColor {
        AnnotationColor {
            components: vec![color.r, color.g, color.b],
        }
    }

    /// Builds the FreeText-specific annotation dictionary fields.
    fn annotation_data(style: &FreeTextStyle) -> FreeTextAnnotation {
        FreeTextAnnotation {
            default_appearance: Some(Self::default_appearance(style)),
            quadding: Some(style.alignment.quadding()),
            rich_contents: None,
            default_style: None,
            callout_line: None,
            border_effect: None,
            difference_rect: Some([
                style.insets.left,
                style.insets.top,
                style.insets.right,
                style.insets.bottom,
            ]),
            intent: None,
        }
    }

    /// Serializes the portable font and text color into the `/DA` program.
    fn default_appearance(style: &FreeTextStyle) -> Vec<u8> {
        let suffix = format!(
            " {} Tf {} {} {} rg",
            style.font_size, style.text_color.r, style.text_color.g, style.text_color.b
        );
        let mut appearance = Vec::with_capacity(
            style
                .font
                .resource_name
                .len()
                .saturating_add(suffix.len())
                .saturating_add(1),
        );
        appearance.push(b'/');
        appearance.extend_from_slice(&style.font.resource_name);
        appearance.extend_from_slice(suffix.as_bytes());
        appearance
    }
}
