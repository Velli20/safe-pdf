use std::sync::Arc;

use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};

use crate::{color_space::ColorSpace, error::ColorSpaceError};

/// Reads a color space given as a name (e.g., `/DeviceRGB`) or as an array
/// (e.g., `[/Indexed /DeviceRGB 255 <lookup data>]`).
///
/// Nested color spaces are read through the same traversal, so the reader's
/// decode-depth limit and cycle detection bound maliciously nested definitions.
impl FromPdfObject for ColorSpace {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let object = context.object();
        let ObjectVariant::Array(array) = object else {
            return match object.try_bytes(context.source())? {
                b"DeviceRGB" => Ok(Self::DeviceRGB),
                b"DeviceCMYK" => Ok(Self::DeviceCMYK),
                b"DeviceGray" => Ok(Self::DeviceGray),
                // Default color spaces substitute for the device spaces when a resource-level
                // override is not available. Without access to the resource dictionary here,
                // fall back to the corresponding device space.
                b"DefaultGray" => Ok(Self::DeviceGray),
                b"DefaultRGB" => Ok(Self::DeviceRGB),
                b"DefaultCMYK" => Ok(Self::DeviceCMYK),
                b"Pattern" => Ok(Self::Pattern(None)),
                unknown => Err(ColorSpaceError::InvalidColorSpace {
                    description: format!("unsupported color space name: /{unknown:?}"),
                }
                .into()),
            };
        };
        let Some(first) = array.first() else {
            return Err(ColorSpaceError::InvalidColorSpace {
                description: "empty color space array".into(),
            }
            .into());
        };
        // A one-element array names its family alone, as in `[/DeviceRGB]`.
        if array.len() == 1 {
            return context.read(first);
        }

        let family = context.read::<Arc<[u8]>>(first)?;
        match &*family {
            b"Indexed" => context.read(object).map(Self::Indexed),
            b"Separation" => context.read(object).map(Self::Separation),
            b"DeviceN" => context.read(object).map(Self::DeviceN),
            // The second element is the underlying color space used with uncolored
            // tiling patterns.
            b"Pattern" => Ok(Self::Pattern(Some(Box::new(context.array()?.at(1)?)))),
            family @ (b"ICCBased" | b"Lab" | b"CalGray" | b"CalRGB") => {
                let mut array = context.array()?;
                let found = array.array().len();
                if found != 2 {
                    return Err(ColorSpaceError::InvalidColorSpace {
                        description: format!(
                            "/{} requires 2 elements, found {found}",
                            String::from_utf8_lossy(family)
                        ),
                    }
                    .into());
                }
                match family {
                    b"ICCBased" => array.at(1).map(Self::ICCBased),
                    b"Lab" => array.at(1).map(Self::Lab),
                    b"CalGray" => array.at(1).map(Self::CalGray),
                    _ => array.at(1).map(Self::CalRGB),
                }
            }
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
