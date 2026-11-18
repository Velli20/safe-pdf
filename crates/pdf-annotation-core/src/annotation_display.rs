//! Whether and how a live annotation is shown.
use crate::{kind::AnnotationKind, metadata::AnnotationFlags, models::Annotation};
use pdf_optional_content::OptionalContentState;

impl Annotation {
    /// Whether this annotation is shown: live flags permit display, the payload has
    /// geometry to place, and `optional_content` does not hide it. Unknown subtypes
    /// without usable bounds are never shown, nor are invisible-flagged unknown subtypes
    /// without a normal appearance.
    pub fn is_displayed(&self, optional_content: &OptionalContentState) -> bool {
        // Optional content hides an annotation without altering its flags, so the two
        // conditions are independent: either one alone suppresses display.
        self.metadata.is_visible()
            && !matches!(&self.content, AnnotationKind::Unknown(v) if v.bounds.is_none())
            && !self.is_invisible_unknown()
            && self
                .optional_content()
                .is_none_or(|content| optional_content.is_visible(content))
    }

    /// Whether the invisible flag hides this annotation: it applies only to unknown
    /// subtypes that carry no normal appearance to draw.
    fn is_invisible_unknown(&self) -> bool {
        self.metadata.flags.contains(AnnotationFlags::INVISIBLE)
            && matches!(self.content, AnnotationKind::Unknown(_))
            && !self.has_normal_appearance()
    }

    /// Whether this annotation's appearance is painted at its fixed source rectangle instead
    /// of a native presentation: source ink without strokes, and caret or unknown subtypes
    /// that carry a normal appearance.
    pub fn is_drawn_from_appearance(&self) -> bool {
        match &self.content {
            AnnotationKind::Ink(ink) => ink.strokes.is_empty(),
            AnnotationKind::Caret(_) | AnnotationKind::Unknown(_) => self.has_normal_appearance(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{AnnotationOverlay, PresentationState, pdf_data::SourceAnnotation};
    use pdf_object_reader::{
        Dictionary, EmptySource, ObjectReader, StreamObject, object_variant::ObjectVariant,
        pdf_array::PdfArray, pdf_string::PdfString, string_kind::StringKind,
    };

    #[test]
    fn invisible_flag_does_not_hide_signature_widget_appearance() {
        let name = |bytes: &[u8]| PdfString::from(bytes, StringKind::Name);
        let appearance = StreamObject::new(2, 0, Dictionary::from_entries([]), b"q Q".to_vec());
        // `/F 133` is Invisible | Print | Locked.
        let widget = ObjectVariant::Dictionary(Dictionary::from_entries([
            (b"Subtype".as_slice(), name(b"Widget")),
            (b"FT".as_slice(), name(b"Sig")),
            (b"F".as_slice(), ObjectVariant::Integer(133)),
            (
                b"Rect".as_slice(),
                ObjectVariant::Array(PdfArray::new(
                    [460, 645, 500, 685].map(ObjectVariant::Integer),
                )),
            ),
            (
                b"AP".as_slice(),
                ObjectVariant::Dictionary(Dictionary::from_entries([(
                    b"N".as_slice(),
                    ObjectVariant::Stream(appearance),
                )])),
            ),
        ]));
        let source: SourceAnnotation = ObjectReader::new(EmptySource).read(&widget).unwrap();
        let overlay = AnnotationOverlay::new([std::slice::from_ref(&source)], None, None).unwrap();
        let appearances = overlay
            .painted_appearances(0, PresentationState::default())
            .unwrap();
        assert_eq!(appearances.len(), 1);
    }
}
