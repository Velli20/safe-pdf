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
    /// of a native presentation: source ink without strokes, and link, caret or unknown
    /// subtypes that carry a normal appearance.
    pub fn is_drawn_from_appearance(&self) -> bool {
        match &self.content {
            AnnotationKind::Ink(ink) => ink.strokes.is_empty(),
            AnnotationKind::Link(_) | AnnotationKind::Caret(_) | AnnotationKind::Unknown(_) => {
                self.has_normal_appearance()
            }
            _ => false,
        }
    }

    /// Whether this shape annotation (square, circle, line, polygon or polyline) still holds
    /// the payload adopted from its source, whose normal appearance then shows it as
    /// authored, including cloudy borders, line endings and blend modes an overlay above the
    /// page cannot reproduce. Moving or restyling it ends this.
    pub(crate) fn shows_source_shape(&self) -> bool {
        matches!(
            self.content,
            AnnotationKind::Square(_)
                | AnnotationKind::Circle(_)
                | AnnotationKind::Line(_)
                | AnnotationKind::Polygon(_)
                | AnnotationKind::PolyLine(_)
        ) && self.has_normal_appearance()
            && self
                .source()
                .and_then(|source| AnnotationKind::from_source(source, None, None).ok())
                .is_some_and(|source| source == self.content)
    }

    /// Whether the page canvas paints this annotation's source appearance at its source
    /// rectangle: always for [`Self::is_drawn_from_appearance`] (including links with a
    /// normal appearance), and for shape annotations
    /// while they show their source shape.
    pub(crate) fn paints_source_appearance(&self) -> bool {
        self.is_drawn_from_appearance() || self.shows_source_shape()
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
