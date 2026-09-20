//! External graphics-state soft mask decoding.
use crate::{error::PdfPagesError, form::FormXObject};
use num_traits::ToPrimitive;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::MaskMode;
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult};
use std::sync::Arc;

/// A soft mask and its transparency group.
pub struct SoftMask {
    /// Whether the group's alpha or luminosity supplies the mask.
    pub mask_type: MaskMode,
    /// The transparency group. Recursive groups remain deferred until painting.
    pub shape: ObjectHandle<FormXObject>,
    /// Optional `/TR` mapping, sampled for the renderer's 8-bit mask coverage.
    pub transfer: Option<Arc<[u8; 256]>>,
}
impl FromPdfObject for SoftMask {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let mask_type = MaskMode::from(
            context
                .dictionary()
                .required_bytes(b"S", context.source())?,
        );
        let group = context.dictionary().get_or_err(b"G")?;
        let stream = group.try_stream(context.source())?;
        let subtype = stream
            .dictionary
            .required_bytes(b"Subtype", context.source())?;
        if subtype != b"Form" {
            return Err(PdfPagesError::InvalidExtGStateEntryValue {
                entry: "SMask".to_string(),
                reason: format!("group XObject must have /Subtype /Form, found /{subtype:?}"),
            }
            .into());
        }
        let shape = context.required_shared(b"G")?;
        let transfer = match context.dictionary().get(b"TR") {
            None => None,
            Some(value)
                if value
                    .try_bytes(context.source())
                    .is_ok_and(|name| name == b"Identity") =>
            {
                None
            }
            Some(value) => {
                let function =
                    Function::parse(value, context.source()).map_err(PdfPagesError::from)?;
                let mut table = [0; 256];
                for (input, output) in (0_u8..=255).zip(&mut table) {
                    let values = function
                        .apply(&[f32::from(input) / 255.0])
                        .map_err(PdfPagesError::from)?;
                    let [value] = values.as_slice() else {
                        return Err(PdfPagesError::InvalidExtGStateEntryValue {
                            entry: "SMask.TR".into(),
                            reason: "expected one output".into(),
                        }
                        .into());
                    };
                    if !value.is_finite() {
                        return Err(PdfPagesError::InvalidExtGStateEntryValue {
                            entry: "SMask.TR".into(),
                            reason: "nonfinite output".into(),
                        }
                        .into());
                    }
                    *output = (value.clamp(0.0, 1.0) * 255.0)
                        .round()
                        .to_u8()
                        .ok_or_else(|| PdfPagesError::InvalidExtGStateEntryValue {
                            entry: "SMask.TR".into(),
                            reason: "invalid output".into(),
                        })?;
                }
                Some(Arc::new(table))
            }
        };
        Ok(Self {
            mask_type,
            shape,
            transfer,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_graphics::MaskMode;
    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use crate::error::PdfPagesError;

    use super::SoftMask;

    fn soft_mask_dictionary(stream_object_number: usize, subtype: &str) -> Dictionary {
        let form_dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"BBox"),
                ObjectVariant::Array(
                    vec![
                        ObjectVariant::Integer(0.into()),
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(10),
                        ObjectVariant::Integer(10),
                    ]
                    .into(),
                ),
            ),
            (
                Vec::from(b"Subtype"),
                pdf_object_reader::pdf_string::PdfString::from(
                    subtype.as_bytes().to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
        ]));
        let stream = StreamObject::new(stream_object_number, 0, form_dictionary, Vec::new());

        Dictionary::new(BTreeMap::from([
            (Vec::from(b"G"), ObjectVariant::Stream(stream)),
            (
                Vec::from(b"S"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Alpha".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
        ]))
    }

    #[test]
    fn parses_inverting_mask_transfer_function() {
        let mut dictionary = soft_mask_dictionary(7, "Form");
        let function = StreamObject::new(
            8,
            0,
            Dictionary::from_entries([
                (b"FunctionType".as_slice(), ObjectVariant::Integer(4)),
                (
                    b"Domain".as_slice(),
                    ObjectVariant::Array(
                        vec![ObjectVariant::Integer(0), ObjectVariant::Integer(1)].into(),
                    ),
                ),
                (
                    b"Range".as_slice(),
                    ObjectVariant::Array(
                        vec![ObjectVariant::Integer(0), ObjectVariant::Integer(1)].into(),
                    ),
                ),
            ]),
            b"{1 exch sub}".to_vec(),
        );
        dictionary
            .dictionary
            .insert(b"TR".to_vec(), ObjectVariant::Stream(function));
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let mask = reader
            .read::<SoftMask>(&ObjectVariant::Dictionary(dictionary))
            .unwrap();
        let table = mask.transfer.unwrap();
        for (input, output) in (0_u8..=255).zip(table.iter()) {
            assert_eq!(*output, 255 - input);
        }
    }

    #[test]
    fn identity_mask_transfer_needs_no_table() {
        let mut dictionary = soft_mask_dictionary(7, "Form");
        dictionary
            .dictionary
            .insert(b"TR".to_vec(), ObjectVariant::name_from_bytes(b"Identity"));
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let mask = reader
            .read::<SoftMask>(&ObjectVariant::Dictionary(dictionary))
            .unwrap();
        assert!(mask.transfer.is_none());
    }

    #[test]
    fn parses_soft_mask_dictionary() {
        let dictionary = soft_mask_dictionary(7, "Form");

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let soft_mask = reader
            .read::<Option<SoftMask>>(
                &pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                    (&dictionary).clone(),
                ),
            )
            .expect("soft mask should parse")
            .expect("soft mask should be present");

        assert_eq!(soft_mask.mask_type, MaskMode::Alpha);
        assert_eq!(
            soft_mask
                .shape
                .get()
                .expect("published shape")
                .content_stream
                .id,
            0
        );
    }

    #[test]
    fn recursive_soft_mask_retains_a_shared_shape() {
        use pdf_object_collection::object_collection::ObjectCollection;
        use pdf_object_reader::{ObjectReader, object_id::ObjectId};
        let mut dictionary = soft_mask_dictionary(7, "Form");
        let Some(ObjectVariant::Stream(mut stream)) = dictionary.take(b"G") else {
            panic!("group stream");
        };
        stream.dictionary.dictionary.insert(
            b"Resources".to_vec(),
            ObjectVariant::Dictionary(Dictionary::from_entries([(
                b"ExtGState".as_slice(),
                ObjectVariant::Dictionary(Dictionary::from_entries([(
                    b"GS".as_slice(),
                    ObjectVariant::Dictionary(Dictionary::from_entries([(
                        b"SMask".as_slice(),
                        ObjectVariant::Reference(ObjectId::new(8, 0)),
                    )])),
                )])),
            )])),
        );
        dictionary
            .dictionary
            .insert(b"G".to_vec(), ObjectVariant::Reference(ObjectId::new(7, 0)));
        let mut objects = ObjectCollection::default();
        objects
            .insert(ObjectId::new(7, 0), ObjectVariant::Stream(stream))
            .expect("group");
        objects
            .insert(ObjectId::new(8, 0), ObjectVariant::Dictionary(dictionary))
            .expect("mask");
        let reader = ObjectReader::new(objects);
        let mask = reader
            .read_shared_indirect::<SoftMask>(ObjectId::new(8, 0))
            .expect("recursive mask")
            .get()
            .expect("published mask");
        assert_eq!(mask.shape.object_id(), Some(ObjectId::new(7, 0)));
        assert!(mask.shape.get().is_ok());
    }

    #[test]
    fn non_form_shape_is_rejected() {
        let dictionary = soft_mask_dictionary(7, "Image");

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let error = match reader.read::<Option<SoftMask>>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((&dictionary).clone()),
        ) {
            Err(error) => error,
            Ok(_) => panic!("an image cannot be used as an ExtGState soft-mask group"),
        };

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<PdfPagesError>(), Some(PdfPagesError::InvalidExtGStateEntryValue { entry, .. }) if entry == "SMask")
        ));
    }
}
