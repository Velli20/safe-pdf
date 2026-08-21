//! External graphics-state soft mask parsing.

use std::rc::Rc;

use pdf_content_stream::ContentStreamIdAllocator;
use pdf_graphics::MaskMode;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{
    error::PdfPagesError, form::FormXObject, object_reader::ReadCycleTracker, resource::Resource,
    resource_cache::ResourceCache, xobject::read_xobject,
};

/// Soft mask extracted from an ExtGState `SMask` entry.
pub struct SoftMask {
    /// How the mask is derived from the transparency group output: from color
    /// luminance (`Luminosity`) or from alpha/shape (`Alpha`).
    pub mask_type: MaskMode,
    /// The transparency group XObject (`G`) whose rendered result provides the
    /// input used to compute the soft mask.
    pub shape: Rc<FormXObject>,
}

impl SoftMask {
    /// Parses a soft mask dictionary.
    ///
    /// Returns `None` when the mask's transparency group is skipped because it
    /// would introduce a cycle in the XObject graph.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<Self>, PdfPagesError> {
        let mask_type = MaskMode::from(dictionary.required_name(b"S", objects)?);

        let content = dictionary.get_or_err(b"G")?;
        let stream = content.try_stream(objects)?;
        let subtype = stream.dictionary.required_name(b"Subtype", objects)?;
        if subtype != b"Form" {
            return Err(PdfPagesError::InvalidExtGStateEntryValue {
                entry: "SMask".to_string(),
                reason: format!("group XObject must have /Subtype /Form, found /{subtype:?}"),
            });
        }

        let Some(shape) = read_xobject(
            content,
            &stream.dictionary,
            stream,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        )?
        else {
            return Ok(None);
        };

        let Resource::Form(shape) = shape else {
            return Err(PdfPagesError::InvalidExtGStateEntryValue {
                entry: "SMask".to_string(),
                reason: "group XObject did not produce a Form resource".to_string(),
            });
        };

        Ok(Some(Self { mask_type, shape }))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_graphics::MaskMode;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use crate::{
        error::PdfPagesError, object_reader::ReadCycleTracker, resource_cache::DefaultResourceCache,
    };

    use super::SoftMask;

    fn soft_mask_dictionary(stream_object_number: usize, subtype: &str) -> Dictionary {
        let form_dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"BBox"),
                ObjectVariant::Array(vec![
                    ObjectVariant::Integer(0),
                    ObjectVariant::Integer(0),
                    ObjectVariant::Integer(10),
                    ObjectVariant::Integer(10),
                ]),
            ),
            (
                Vec::from(b"Subtype"),
                ObjectVariant::Name(subtype.as_bytes().to_vec()),
            ),
        ]));
        let stream = StreamObject::new(
            stream_object_number,
            0,
            Box::new(form_dictionary),
            Vec::new(),
        );

        Dictionary::new(BTreeMap::from([
            (Vec::from(b"G"), ObjectVariant::Stream(stream)),
            (Vec::from(b"S"), ObjectVariant::Name(b"Alpha".to_vec())),
        ]))
    }

    #[test]
    fn parses_soft_mask_dictionary() {
        let dictionary = soft_mask_dictionary(7, "Form");
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let soft_mask = SoftMask::from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("soft mask should parse")
        .expect("soft mask should be present");

        assert_eq!(soft_mask.mask_type, MaskMode::Alpha);
        assert_eq!(soft_mask.shape.content_stream.id, 0);
    }

    #[test]
    fn cycle_suppressed_shape_returns_none() {
        let dictionary = soft_mask_dictionary(7, "Form");
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();
        assert!(cycle_tracker.begin_read(7));

        let soft_mask = SoftMask::from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("cycle-suppressed soft mask should not fail");

        assert!(soft_mask.is_none());
    }

    #[test]
    fn non_form_shape_is_rejected() {
        let dictionary = soft_mask_dictionary(7, "Image");
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let error = match SoftMask::from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        ) {
            Err(error) => error,
            Ok(_) => panic!("an image cannot be used as an ExtGState soft-mask group"),
        };

        assert!(matches!(
            error,
            PdfPagesError::InvalidExtGStateEntryValue { entry, .. } if entry == "SMask"
        ));
    }
}
