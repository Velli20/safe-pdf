use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};

use crate::{
    color_space::ColorSpace, device_n_color_space::DeviceNColorSpace, error::ColorSpaceError,
    indexed_color_space::IndexedColorSpace, separation_color_space::SeparationColorSpace,
};

/// Reads a color space given as a name (e.g., `/DeviceRGB`) or as an array
/// (e.g., `[/Indexed /DeviceRGB 255 <lookup data>]`).
///
/// Nested color spaces are read through the same traversal, so the reader's
/// decode-depth limit and cycle detection bound maliciously nested definitions.
impl FromPdfObject for ColorSpace {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object().value();
        let ObjectVariant::Array(array) = value else {
            return Ok(Self::try_from(value.try_bytes(context.source())?)?);
        };
        let family = array
            .first()
            .ok_or_else(|| ColorSpaceError::InvalidColorSpace {
                description: "empty color space array".into(),
            })?
            .try_bytes(context.source())?;
        if array.len() == 1 {
            return Ok(Self::try_from(family)?);
        }

        match family {
            b"Indexed" => IndexedColorSpace::from_pdf_object(context).map(Self::Indexed),
            b"Separation" => SeparationColorSpace::from_pdf_object(context).map(Self::Separation),
            b"DeviceN" => DeviceNColorSpace::from_pdf_object(context).map(Self::DeviceN),
            b"ICCBased" => sole_operand(context, "ICCBased").map(Self::ICCBased),
            b"Lab" => sole_operand(context, "Lab").map(Self::Lab),
            b"CalGray" => sole_operand(context, "CalGray").map(Self::CalGray),
            b"CalRGB" => sole_operand(context, "CalRGB").map(Self::CalRGB),
            // The second element is the underlying color space used with uncolored
            // tiling patterns.
            b"Pattern" => Ok(Self::Pattern(Some(Box::new(context.array()?.at(1)?)))),
            unknown => Err(ColorSpaceError::InvalidColorSpace {
                description: format!(
                    "unsupported color space type: /{} (array with {} elements)",
                    String::from_utf8_lossy(unknown),
                    array.len()
                ),
            }
            .into()),
        }
    }
}

/// Reads the single operand of a `[/Family operand]` color space array.
fn sole_operand<T: FromPdfObject>(
    context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    family: &str,
) -> ReadResult<T> {
    let mut array = context.array()?;
    let found = array.array().len();
    if found != 2 {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/{family} requires 2 elements, found {found}"),
        }
        .into());
    }
    array.at(1)
}

#[cfg(test)]
mod tests {
    use pdf_object_reader::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use crate::{color_space::ColorSpace, error::ColorSpaceError};

    fn name(value: &str) -> ObjectVariant {
        pdf_object_reader::pdf_string::PdfString::from(
            value.as_bytes().to_vec(),
            pdf_object_reader::string_kind::StringKind::Name,
        )
    }

    #[test]
    fn parses_single_name_device_color_space_arrays() {
        let cases = [
            (
                ObjectVariant::Array(vec![name("DeviceGray".into())].into()),
                ColorSpace::DeviceGray,
            ),
            (
                ObjectVariant::Array(vec![name("DeviceRGB".into())].into()),
                ColorSpace::DeviceRGB,
            ),
            (
                ObjectVariant::Array(vec![name("DeviceCMYK".into())].into()),
                ColorSpace::DeviceCMYK,
            ),
        ];

        for (object, expected) in cases {
            let parsed = pdf_object_reader::ObjectReader::new(&PassthroughResolver)
                .read::<ColorSpace>(&object)
                .unwrap();

            match (parsed, expected) {
                (ColorSpace::DeviceGray, ColorSpace::DeviceGray)
                | (ColorSpace::DeviceRGB, ColorSpace::DeviceRGB)
                | (ColorSpace::DeviceCMYK, ColorSpace::DeviceCMYK) => {}
                (parsed, expected) => {
                    panic!("expected {expected:?} for single-name array, got {parsed:?}");
                }
            }
        }
    }

    #[test]
    fn parses_single_name_pattern_array() {
        let parsed = pdf_object_reader::ObjectReader::new(&PassthroughResolver)
            .read::<ColorSpace>(&ObjectVariant::Array(vec![name("Pattern".into())].into()))
            .unwrap();

        assert!(matches!(parsed, ColorSpace::Pattern(None)));
    }

    #[test]
    fn rejects_empty_color_space_array() {
        let error = pdf_object_reader::ObjectReader::new(&PassthroughResolver)
            .read::<ColorSpace>(&ObjectVariant::Array(
                pdf_object_reader::pdf_array::PdfArray::new(Vec::new()),
            ))
            .unwrap_err();

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<ColorSpaceError>(), Some(ColorSpaceError::InvalidColorSpace { description }) if description == "empty color space array")
        ));
    }

    #[test]
    fn rejects_unsupported_multi_element_color_space_array() {
        let error = pdf_object_reader::ObjectReader::new(&PassthroughResolver)
            .read::<ColorSpace>(&ObjectVariant::Array(
                vec![name("DeviceGray".into()), ObjectVariant::Integer(1)].into(),
            ))
            .unwrap_err();

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<ColorSpaceError>(), Some(ColorSpaceError::InvalidColorSpace { description }) if description == "unsupported color space type: /DeviceGray (array with 2 elements)")
        ));
    }
}
