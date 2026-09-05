use pdf_annotation_types::{
    Annotation, AnnotationKind, FreeTextAnnotation, annotation_id::AnnotationId,
};
use pdf_document::page::PdfPage;
use pdf_font::text_string;
use pdf_graphics::rect::Rect;
use thiserror::Error;

use crate::{
    FreeTextStyle, free_text_appearance::FreeTextAppearance, free_text_layout::FreeTextLayout,
};

/// Complete editable contents and appearance of a plain free-text annotation.
#[derive(Clone, Debug, PartialEq)]
pub struct FreeText {
    /// Minimum annotation rectangle.
    pub rect: Rect,
    /// Visible annotation text.
    pub text: String,
    /// Generated appearance styling.
    pub style: FreeTextStyle,
}

/// Errors produced while editing FreeText annotations.
#[derive(Debug, Error, PartialEq)]
pub enum FreeTextEditError {
    /// The requested annotation is not present on the page.
    #[error("annotation {id} was not found on this page")]
    AnnotationNotFound { id: usize },
    /// The requested annotation is not a free text annotation.
    #[error("annotation {id} has subtype /{subtype}, expected /FreeText")]
    WrongSubtype { id: usize, subtype: String },
    /// The page cannot allocate another stable annotation identifier.
    #[error("annotation identifier space is exhausted")]
    AnnotationIdExhausted,
    /// The requested rectangle or style cannot produce a usable appearance.
    #[error("invalid free text {field}: {reason}")]
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    /// A character cannot be represented by the v1 WinAnsi font.
    #[error("character {character:?} is not representable in WinAnsi")]
    UnsupportedCharacter { character: char },
    /// Existing bytes cannot be interpreted as WinAnsi text.
    #[error("byte 0x{byte:02X} is undefined in WinAnsi")]
    InvalidWinAnsi { byte: u8 },
    /// A font encoding operation failed unexpectedly.
    #[error("font encoding error: {message}")]
    FontEncodingError { message: String },
    /// A text cursor is outside the annotation contents.
    #[error("free text cursor {cursor} exceeds the character count {character_count}")]
    InvalidCursor {
        cursor: usize,
        character_count: usize,
    },
    /// The annotation uses a FreeText feature unsupported by plain-text regeneration.
    #[error("free text annotation {id} is not a plain FreeText annotation")]
    UnsupportedFreeTextVariant { id: usize },
    /// Existing annotation bytes are not a supported PDF text string.
    #[error("free text contents are not a supported PDF text string")]
    InvalidTextString,
    #[error("{0}")]
    FontError(#[from] pdf_font::error::FontError),
}

impl FreeTextEditError {
    fn annotation_not_found(id: AnnotationId) -> Self {
        Self::AnnotationNotFound { id: id.get() }
    }

    pub(super) fn invalid_input(field: &'static str, reason: &'static str) -> Self {
        Self::InvalidInput { field, reason }
    }
}

/// Edits annotations attached to one materialized PDF page.
pub struct FreeTextEditor<'a> {
    page: &'a mut PdfPage,
}

impl<'a> FreeTextEditor<'a> {
    /// Creates an editor for a page.
    pub fn new(page: &'a mut PdfPage) -> Self {
        Self { page }
    }

    /// Adds a generated free text annotation and returns its stable page-scoped ID.
    pub fn create(&mut self, free_text: FreeText) -> Result<AnnotationId, FreeTextEditError> {
        let generated = FreeTextAppearance::generate(free_text)?;
        let id = self
            .page
            .reserve_annotation_id()
            .ok_or(FreeTextEditError::AnnotationIdExhausted)?;
        self.page.push_annotation(generated, id);
        Ok(id)
    }

    /// Replaces the editable state and regenerates the annotation appearance atomically.
    pub fn update(
        &mut self,
        id: AnnotationId,
        free_text: FreeText,
    ) -> Result<(), FreeTextEditError> {
        plain_free_text(self.annotation(id)?, id)?;
        let generated = FreeTextAppearance::generate(free_text)?;
        apply_generated_fields(self.annotation_mut(id)?, generated);
        Ok(())
    }

    /// Returns a complete editable snapshot of a plain free-text annotation.
    pub fn get(&self, id: AnnotationId) -> Result<FreeText, FreeTextEditError> {
        let annotation = self.annotation(id)?;
        plain_free_text(annotation, id)?;
        let rect = annotation.rect.ok_or_else(|| {
            FreeTextEditError::invalid_input(
                "rectangle",
                "the existing annotation has no rectangle",
            )
        })?;
        let text = text_string::decode(annotation.contents.as_deref().unwrap_or_default())
            .map_err(|_| FreeTextEditError::InvalidTextString)?;

        Ok(FreeText {
            rect,
            text,
            style: FreeTextAppearance::derive_style(annotation),
        })
    }

    /// Removes a free text annotation from the page.
    pub fn remove(&mut self, id: AnnotationId) -> Result<Annotation, FreeTextEditError> {
        free_text(self.annotation(id)?, id)?;
        self.page
            .take_annotation(id)
            .ok_or_else(|| FreeTextEditError::annotation_not_found(id))
    }

    fn annotation(&self, id: AnnotationId) -> Result<&Annotation, FreeTextEditError> {
        self.page
            .annotation(id)
            .ok_or_else(|| FreeTextEditError::annotation_not_found(id))
    }

    fn annotation_mut(&mut self, id: AnnotationId) -> Result<&mut Annotation, FreeTextEditError> {
        self.page
            .annotation_mut(id)
            .ok_or_else(|| FreeTextEditError::annotation_not_found(id))
    }
}

impl FreeText {
    /// Returns the caret rectangle for a character cursor in PDF user space.
    pub fn caret_rect(&self, cursor: usize) -> Result<Rect, FreeTextEditError> {
        FreeTextLayout::new(self)?.caret_rect(cursor)
    }
}

fn free_text(
    annotation: &Annotation,
    id: AnnotationId,
) -> Result<&FreeTextAnnotation, FreeTextEditError> {
    let AnnotationKind::FreeText(free_text) = &annotation.kind else {
        return Err(FreeTextEditError::WrongSubtype {
            id: id.get(),
            subtype: String::from_utf8_lossy(&annotation.subtype).into_owned(),
        });
    };
    Ok(free_text)
}

fn plain_free_text(
    annotation: &Annotation,
    id: AnnotationId,
) -> Result<&FreeTextAnnotation, FreeTextEditError> {
    let free_text = free_text(annotation, id)?;
    // `/DS` and `/IT /FreeTextTypewriter` are presentation metadata. The
    // regenerated `/AP` remains authoritative, so they do not prevent editing.
    if free_text.rich_contents.is_none()
        && free_text.callout_line.is_none()
        && free_text.border_effect.is_none()
    {
        Ok(free_text)
    } else {
        Err(FreeTextEditError::UnsupportedFreeTextVariant { id: id.get() })
    }
}

/// Replaces only fields owned by the generated plain-FreeText appearance.
fn apply_generated_fields(existing: &mut Annotation, generated: Annotation) {
    existing.rect = generated.rect;
    existing.contents = generated.contents;
    existing.appearance = generated.appearance;
    existing.border = generated.border;
    existing.color = generated.color;
    if let (
        AnnotationKind::FreeText(existing_free_text),
        AnnotationKind::FreeText(generated_free_text),
    ) = (&mut existing.kind, generated.kind)
    {
        existing_free_text.default_appearance = generated_free_text.default_appearance;
        existing_free_text.quadding = generated_free_text.quadding;
        existing_free_text.difference_rect = generated_free_text.difference_rect;
    }
}

#[cfg(test)]
mod tests {
    use pdf_font::{BaseEncoding, standard14::Standard14Font};
    use pdf_graphics::color::Color;

    use crate::{FreeTextFont, FreeTextOverflow};

    use super::*;

    fn document_style() -> FreeTextStyle {
        FreeTextStyle {
            font: FreeTextFont {
                standard14: Standard14Font::CourierBold,
                resource_name: Vec::from(b"Body"),
                encoding: BaseEncoding::WinAnsi,
            },
            font_size: 18.0,
            line_height: 21.6,
            text_color: Color::from_rgb(0.25, 0.5, 0.75),
            background_color: None,
            border: None,
            insets: Rect {
                left: 3.0,
                top: 4.0,
                right: 5.0,
                bottom: 6.0,
            },
            alignment: pdf_annotation_types::FreeTextAlignment::Right,
            overflow: FreeTextOverflow::ExpandRight,
        }
    }

    #[test]
    fn derives_style_from_normal_appearance() {
        let mut page = PdfPage::default();
        let style = document_style();
        let id = FreeTextEditor::new(&mut page)
            .create(FreeText {
                rect: Rect::new(100.0, 30.0),
                text: "editable".to_owned(),
                style: style.clone(),
            })
            .expect("generated FreeText should be valid");

        let derived = FreeTextEditor::new(&mut page)
            .get(id)
            .expect("generated FreeText should expose its editable state");

        assert_eq!(derived.text, "editable");
        assert_eq!(derived.style.font, style.font);
        assert_eq!(derived.style.font_size, style.font_size);
        assert_eq!(derived.style.line_height, style.line_height);
        assert_eq!(derived.style.text_color, style.text_color);
        assert_eq!(derived.style.insets, style.insets);
        assert_eq!(derived.style.alignment, style.alignment);
        assert_eq!(derived.style.overflow, FreeTextOverflow::ExpandRight);
    }

    #[test]
    fn rejects_invalid_pdf_text_string_contents() {
        let mut page = PdfPage::default();
        let id = FreeTextEditor::new(&mut page)
            .create(FreeText {
                rect: Rect::new(100.0, 30.0),
                text: "editable".to_owned(),
                style: document_style(),
            })
            .expect("generated FreeText should be valid");
        let annotation = page
            .annotation_mut(id)
            .expect("generated annotation should remain on the page");
        annotation.contents = Some(vec![0x7F]);

        assert!(matches!(
            FreeTextEditor::new(&mut page).get(id),
            Err(FreeTextEditError::InvalidTextString)
        ));
    }

    #[test]
    fn failed_update_leaves_the_annotation_unchanged() {
        let mut page = PdfPage::default();
        let original = FreeText {
            rect: Rect::new(100.0, 30.0),
            text: "original".to_owned(),
            style: FreeTextStyle::default(),
        };
        let id = FreeTextEditor::new(&mut page)
            .create(original.clone())
            .expect("original FreeText should be valid");
        let before = FreeTextEditor::new(&mut page)
            .get(id)
            .expect("original FreeText should be readable");
        let mut invalid = original.clone();
        invalid.text = "replacement".to_owned();
        invalid.style.font_size = f32::NAN;

        assert!(FreeTextEditor::new(&mut page).update(id, invalid).is_err());
        assert_eq!(
            FreeTextEditor::new(&mut page)
                .get(id)
                .expect("original FreeText should remain editable"),
            before
        );
    }
}
