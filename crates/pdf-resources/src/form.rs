use crate::resources::Resources;
use pdf_content_stream::ContentStream;
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult,
    object_variant::ObjectVariant,
};

/// A parsed Form XObject, including deferred resource graph edges.
pub struct FormXObject {
    /// Normalized form bounds.
    pub bbox: Rect,
    /// Optional form transformation.
    pub matrix: Option<Transform>,
    /// Resources needed to paint the form.
    pub resources: Option<ObjectHandle<Resources>>,
    /// Parsed form operators.
    pub content_stream: ContentStream,
}

impl FromPdfObject for FormXObject {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object().object().clone();
        // Both stream-backed and dictionary-only forms expose the same metadata.
        let mut context = context.dictionary()?;
        let content_stream = match object.value() {
            // Dictionary-only forms still receive an ID, but have no drawing operators.
            ObjectVariant::Dictionary(_) => ContentStream {
                operators: Vec::new(),
                id: context.content_stream_ids().next_id()?,
            },
            _ => context.read::<ContentStream>(object.value())?,
        };
        let bbox = context
            .dictionary()
            .required_bbox(context.source())?
            .normalized();
        let matrix = context.dictionary().optional_matrix(context.source())?;
        // Retain deferred handles so recursive resource graphs can finish decoding.
        let resources = context
            .dictionary()
            .get(b"Resources")
            .cloned()
            .map(|value| context.read_shared(&value))
            .transpose()?;
        Ok(Self {
            bbox,
            matrix,
            resources,
            content_stream,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_graphics::transform::Transform;
    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::FormXObject;

    #[test]
    fn read_xobject_normalizes_inverted_bbox() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"BBox"),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(265.077),
                    ObjectVariant::Real(71.8304),
                    ObjectVariant::Real(301.321),
                    ObjectVariant::Real(43.3206),
                ]),
            ),
            (Vec::from(b"Subtype"), ObjectVariant::Name(b"Form".to_vec())),
        ]));
        let stream = StreamObject::new(7, 0, dictionary.clone(), Vec::new());

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let form = reader
            .read::<FormXObject>(&ObjectVariant::Stream(stream))
            .expect("form xobject should parse");

        assert_eq!(form.bbox.left, 265.077);
        assert_eq!(form.bbox.top, 43.3206);
        assert_eq!(form.bbox.right, 301.321);
        assert_eq!(form.bbox.bottom, 71.8304);
    }

    #[test]
    fn empty_from_dictionary_creates_empty_content_stream() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"BBox"),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(10.0),
                    ObjectVariant::Real(10.0),
                ]),
            ),
            (Vec::from(b"Subtype"), ObjectVariant::Name(b"Form".to_vec())),
            (
                Vec::from(b"Matrix"),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(2.0),
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(3.0),
                    ObjectVariant::Real(4.0),
                    ObjectVariant::Real(5.0),
                ]),
            ),
        ]));

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let form = reader
            .read::<FormXObject>(
                &pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                    (&dictionary).clone(),
                ),
            )
            .expect("dictionary-only form should parse");

        assert_eq!(form.bbox.left, 0.0);
        assert_eq!(form.bbox.top, 0.0);
        assert_eq!(form.bbox.right, 10.0);
        assert_eq!(form.bbox.bottom, 10.0);
        assert_eq!(
            form.matrix,
            Some(Transform::from_row(2.0, 0.0, 0.0, 3.0, 4.0, 5.0))
        );
        assert_eq!(form.content_stream.id, 0);
        assert!(form.content_stream.operators.is_empty());
    }
}
