//! Coordinates generation and recovery of editable free-text appearances.

use pdf_annotation_types::Annotation;

use crate::{
    FreeText, FreeTextEditError, FreeTextStyle,
    free_text_appearance_annotation_generator::FreeTextAnnotationGenerator,
    free_text_appearance_style_deriver::FreeTextStyleDeriver,
};

/// Provides the crate-internal entry points for free-text appearance handling.
pub(super) struct FreeTextAppearance;

impl FreeTextAppearance {
    /// Generates a complete PDF annotation from editable free-text state.
    pub(super) fn generate(free_text: FreeText) -> Result<Annotation, FreeTextEditError> {
        FreeTextAnnotationGenerator::new(free_text).generate()
    }

    /// Recovers the editable style represented by an existing annotation.
    pub(super) fn derive_style(annotation: &Annotation) -> FreeTextStyle {
        FreeTextStyleDeriver::new(annotation).derive()
    }
}

#[cfg(test)]
mod tests {
    use pdf_annotation_types::{AppearanceField, FreeTextAlignment};
    use pdf_content_stream_operators::{
        color_operators::SetRGBFill,
        text_object_operators::{BeginText, EndText},
        text_state_operators::{SetFont, SetLeading},
        variants::PdfOperatorVariant,
    };
    use pdf_graphics::{color::Color, rect::Rect};
    use std::sync::Arc;

    use super::*;
    use crate::FreeTextBorder;

    #[test]
    fn appearance_and_caret_share_horizontal_alignment() {
        for alignment in [
            FreeTextAlignment::Left,
            FreeTextAlignment::Center,
            FreeTextAlignment::Right,
        ] {
            let style = FreeTextStyle {
                alignment,
                ..FreeTextStyle::default()
            };
            let free_text = FreeText {
                rect: Rect {
                    left: 10.0,
                    top: 20.0,
                    right: 210.0,
                    bottom: 60.0,
                },
                text: "aligned".to_owned(),
                style,
            };
            let caret = free_text
                .caret_rect(0)
                .expect("default style should produce a caret");
            let annotation =
                FreeTextAppearance::generate(free_text).expect("default style should be generated");
            let appearance = annotation
                .appearance
                .as_ref()
                .expect("generated annotation should have an appearance");
            let form = appearance
                .normal
                .as_ref()
                .expect("generated annotation should have a normal appearance")
                .appearance_field_for_state(&annotation.appearance_state)
                .expect("generated normal appearance should be a stream");
            let text_matrix = form
                .content_stream
                .operators
                .iter()
                .find_map(|operator| match operator {
                    PdfOperatorVariant::SetTextMatrix(matrix) => Some(matrix.matrix()),
                    _ => None,
                })
                .expect("appearance should position its first line");

            assert_eq!(caret.left, 10.0 + text_matrix.tx);
        }
    }

    #[test]
    fn generated_layers_follow_pdf_paint_order_and_register_the_font() {
        let style = FreeTextStyle {
            background_color: Some(Color::from_rgb(0.9, 0.8, 0.7)),
            border: Some(FreeTextBorder {
                color: Color::from_rgb(0.1, 0.2, 0.3),
                width: 2.0,
            }),
            ..FreeTextStyle::default()
        };
        let annotation = FreeTextAppearance::generate(FreeText {
            rect: Rect::new(100.0, 40.0),
            text: "layered".to_owned(),
            style,
        })
        .expect("valid layers should generate an appearance");
        let appearance = annotation
            .appearance
            .as_ref()
            .expect("generated annotation should have an appearance");
        let form = appearance
            .normal
            .as_ref()
            .expect("generated annotation should have a normal appearance")
            .appearance_field_for_state(&annotation.appearance_state)
            .expect("generated normal appearance should be a stream");
        let mut operators = form.content_stream.operators.iter();

        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SaveGraphicsState(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SetRGBFill(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::Rectangle(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::FillPathNonZero(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SetRGBStroke(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SetLineWidth(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::Rectangle(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::StrokePath(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SetRGBFill(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::BeginText(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SetFont(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::SetTextMatrix(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::ShowText(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::EndText(_))
        ));
        assert!(matches!(
            operators.next(),
            Some(PdfOperatorVariant::RestoreGraphicsState(_))
        ));
        assert!(operators.next().is_none());
        assert!(form.resources.as_ref().is_some_and(|resources| {
            resources
                .get()
                .is_ok_and(|resources| resources.fonts.contains_key(b"Helv".as_slice()))
        }));
    }

    #[test]
    fn style_scanner_recovers_inherited_fill_font_and_leading() {
        let mut annotation = FreeTextAppearance::generate(FreeText {
            rect: Rect::new(100.0, 40.0),
            text: "styled".to_owned(),
            style: FreeTextStyle::default(),
        })
        .expect("default style should generate an appearance");
        let appearance = annotation
            .appearance
            .as_mut()
            .expect("generated annotation should have an appearance");
        let normal = appearance
            .normal
            .as_mut()
            .expect("generated annotation should have a normal appearance");
        assert!(matches!(normal, AppearanceField::Stream(_)));
        let AppearanceField::Stream(form) = normal else {
            return;
        };
        form.content_stream.operators = vec![
            PdfOperatorVariant::SetRGBFill(SetRGBFill::new(0.2, 0.4, 0.6)),
            PdfOperatorVariant::BeginText(BeginText),
            PdfOperatorVariant::SetFont(SetFont::new(Arc::from(b"Helv".to_vec()), 18.0)),
            PdfOperatorVariant::SetLeading(SetLeading::new(24.0)),
            PdfOperatorVariant::EndText(EndText),
        ];

        let style = FreeTextAppearance::derive_style(&annotation);

        assert_eq!(style.text_color, Color::from_rgb(0.2, 0.4, 0.6));
        assert_eq!(style.font.resource_name, b"Helv");
        assert_eq!(style.font_size, 18.0);
        assert_eq!(style.line_height, 24.0);
    }

    #[test]
    fn style_scanner_uses_defaults_for_malformed_operators() {
        let mut annotation = FreeTextAppearance::generate(FreeText {
            rect: Rect::new(100.0, 40.0),
            text: "defaults".to_owned(),
            style: FreeTextStyle::default(),
        })
        .expect("default style should generate an appearance");
        let appearance = annotation
            .appearance
            .as_mut()
            .expect("generated annotation should have an appearance");
        let normal = appearance
            .normal
            .as_mut()
            .expect("generated annotation should have a normal appearance");
        assert!(matches!(normal, AppearanceField::Stream(_)));
        let AppearanceField::Stream(form) = normal else {
            return;
        };
        form.content_stream.operators = vec![
            PdfOperatorVariant::SetRGBFill(SetRGBFill::new(f32::NAN, 0.0, 0.0)),
            PdfOperatorVariant::BeginText(BeginText),
            PdfOperatorVariant::SetFont(SetFont::new(Arc::from(b"Missing".to_vec()), -1.0)),
            PdfOperatorVariant::SetLeading(SetLeading::new(f32::INFINITY)),
            PdfOperatorVariant::EndText(EndText),
        ];

        let style = FreeTextAppearance::derive_style(&annotation);
        let mut expected = FreeTextStyle::default();
        expected.line_height = expected.font_size * 1.2;

        assert_eq!(style, expected);
    }
}
