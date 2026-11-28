//! How the annotations on one projected page relate to the popups among them.
use crate::{
    kind::AnnotationKind,
    layer_error::AnnotationLayerResult,
    models::{Annotation, AnnotationId},
};
use std::collections::BTreeSet;

/// Popup relationships among one page's annotations, gathered once per projection.
pub(crate) struct PagePopups {
    /// Identities named as a parent by a popup.
    parents: BTreeSet<u32>,
    /// Popups with no text of their own and none from their parent, which have nothing
    /// to show.
    blank: BTreeSet<u32>,
}

impl PagePopups {
    /// Gathers popup relationships among `annotations`, the annotations of one page.
    ///
    /// # Errors
    ///
    /// Propagates a failure to decode a popup parent's text.
    pub(crate) fn new(annotations: &[&Annotation]) -> AnnotationLayerResult<Self> {
        let parents = annotations
            .iter()
            .filter_map(|annotation| annotation.content.popup_parent())
            .map(|id| id.0)
            .collect();
        let mut blank = BTreeSet::new();
        for annotation in annotations {
            let AnnotationKind::Popup(popup) = &annotation.content else {
                continue;
            };
            if !popup.text.is_empty() {
                continue;
            }
            let parent = popup
                .parent
                .and_then(|id| annotations.iter().find(|candidate| candidate.id == id));
            let parent_blank = match parent {
                Some(parent) => parent.text()?.is_empty(),
                None => true,
            };
            if parent_blank {
                blank.insert(annotation.id.0);
            }
        }
        Ok(Self { parents, blank })
    }

    /// Whether a popup on the page names `id` as its parent.
    pub(crate) fn has_popup(&self, id: AnnotationId) -> bool {
        self.parents.contains(&id.0)
    }

    /// Whether `id` is a popup with no text to show, neither its own nor its parent's.
    pub(crate) fn is_blank(&self, id: AnnotationId) -> bool {
        self.blank.contains(&id.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AnnotationOverlay, kind::AnnotationKind, pdf_data::SourceAnnotation};
    use pdf_graphics::{rect::Rect, size::Size, viewport::PageViewport};
    use pdf_object_reader::{
        Dictionary, EmptySource, ObjectReader, object_id::ObjectId, object_variant::ObjectVariant,
        pdf_array::PdfArray, pdf_string::PdfString, string_kind::StringKind,
    };

    #[test]
    fn open_popup_without_text_is_not_shown() {
        let name = |bytes: &[u8]| PdfString::from(bytes, StringKind::Name);
        let rect = |edges: [i64; 4]| {
            ObjectVariant::Array(PdfArray::new(edges.map(ObjectVariant::Integer)))
        };
        let reader = ObjectReader::new(EmptySource);
        let note = |object: u32, contents: &[u8]| {
            let mut source: SourceAnnotation = reader
                .read(&ObjectVariant::Dictionary(Dictionary::from_entries([
                    (b"Subtype".as_slice(), name(b"Text")),
                    (b"Rect".as_slice(), rect([170, 776, 190, 796])),
                    (
                        b"Contents".as_slice(),
                        PdfString::from(contents, StringKind::Literal),
                    ),
                ])))
                .unwrap();
            source.object_number = Some(object);
            source
        };
        let popup = |parent: usize| -> SourceAnnotation {
            reader
                .read(&ObjectVariant::Dictionary(Dictionary::from_entries([
                    (b"Subtype".as_slice(), name(b"Popup")),
                    (b"Rect".as_slice(), rect([473, 676, 653, 796])),
                    (b"Open".as_slice(), ObjectVariant::Boolean(true)),
                    (
                        b"Parent".as_slice(),
                        ObjectVariant::Reference(ObjectId::new(parent, 0)),
                    ),
                ])))
                .unwrap()
        };
        let sources = [note(5, b""), popup(5), note(7, b"Comment"), popup(7)];
        let mut overlay = AnnotationOverlay::new([sources.as_slice()], None, None).unwrap();
        let page = Rect {
            left: 0.0,
            top: 0.0,
            right: 595.0,
            bottom: 842.0,
        };
        let viewport = PageViewport::new(page, 0, Size::new(595.0, 842.0)).unwrap();
        let prepared = overlay.prepare(0, &viewport, 0).unwrap();
        let popups = prepared
            .entries()
            .iter()
            .filter(|entry| matches!(entry.content, AnnotationKind::Popup(_)))
            .map(|entry| entry.visible)
            .collect::<Vec<_>>();
        assert_eq!(popups, [false, true]);
    }
}
