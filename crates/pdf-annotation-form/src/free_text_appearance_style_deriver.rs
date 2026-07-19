//! Recovers annotation-dictionary style metadata before stream inspection.

use pdf_annotation_types::{Annotation, AnnotationKind, AppearanceField, FreeTextAlignment};
use pdf_graphics::rect::Rect;
use pdf_resources::form::FormXObject;

use crate::{FreeTextStyle, free_text_appearance_style_scanner::FreeTextAppearanceStyleScanner};

/// Derives the editable style represented by one PDF annotation.
pub(super) struct FreeTextStyleDeriver<'a> {
    /// Annotation whose portable FreeText subset is being inspected.
    annotation: &'a Annotation,
    /// Style initialized with portable defaults and progressively refined.
    style: FreeTextStyle,
}

impl<'a> FreeTextStyleDeriver<'a> {
    /// Creates a derivation context initialized with portable defaults.
    pub(super) fn new(annotation: &'a Annotation) -> Self {
        Self {
            annotation,
            style: FreeTextStyle::default(),
        }
    }

    /// Recovers valid dictionary metadata and normal-appearance typography.
    pub(super) fn derive(mut self) -> FreeTextStyle {
        self.apply_annotation_dictionary();
        let Some(form) = Self::normal_appearance(self.annotation) else {
            return self.style;
        };
        FreeTextAppearanceStyleScanner::new(form, self.style).scan()
    }

    /// Applies valid `/Q` alignment and `/RD` inset metadata.
    fn apply_annotation_dictionary(&mut self) {
        let AnnotationKind::FreeText(free_text) = &self.annotation.kind else {
            return;
        };
        if let Some(alignment) = free_text
            .quadding
            .and_then(FreeTextAlignment::from_quadding)
        {
            self.style.alignment = alignment;
        }
        if let Some(insets) = free_text.difference_rect.and_then(Self::valid_insets) {
            self.style.insets = insets;
        }
    }

    /// Converts a finite, non-negative `/RD` array into editable insets.
    fn valid_insets(values: [f32; 4]) -> Option<Rect> {
        if !values
            .into_iter()
            .all(|inset| inset.is_finite() && inset >= 0.0)
        {
            return None;
        }
        let [left, top, right, bottom] = values;
        Some(Rect {
            left,
            top,
            right,
            bottom,
        })
    }

    /// Resolves a stream-valued normal appearance from the annotation.
    fn normal_appearance(annotation: &Annotation) -> Option<&FormXObject> {
        let appearance = annotation.appearance.as_ref()?;
        let AppearanceField::Stream(form) = appearance.normal.as_ref()? else {
            return None;
        };
        Some(form)
    }
}
