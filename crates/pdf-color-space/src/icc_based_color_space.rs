use bytes::Bytes;
use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{color_space::ColorSpace, error::ColorSpaceError};

/// ICC profile-based color space.
///
/// Uses an embedded ICC color profile for device-independent color.
#[derive(Debug, Clone)]
pub struct ICCBasedColorSpace {
    /// Number of color components (must be 1, 3, or 4 per spec).
    pub num_components: usize,
    /// Alternate color space used as a rendering fallback.
    ///
    /// If absent, the device-equivalent for `num_components` is used instead.
    pub alternate_space: Option<Box<ColorSpace>>,
    /// Shared raw ICC profile bytes.
    ///
    /// ICC colour management is not yet implemented. The alternate space (or a
    /// device-equivalent) is used for rendering until ICC support is added.
    pub profile_data: Bytes,
}

/// Reads the stream operand of an ICCBased color space, `[/ICCBased stream]`.
///
/// The stream dictionary must contain an `/N` entry (1, 3, or 4).
/// The optional `/Alternate` entry names the fallback color space.
impl FromPdfObject for ICCBasedColorSpace {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut stream = context.stream()?;
        let profile_data = stream.stream().shared_data();
        let mut dictionary = stream.dictionary();
        let num_components = dictionary.required::<usize>(b"N")?;

        // N shall be 1, 3, or 4.
        if !matches!(num_components, 1 | 3 | 4) {
            return Err(ColorSpaceError::InvalidColorSpace {
                description: format!("/ICCBased /N must be 1, 3, or 4; found {num_components}"),
            }
            .into());
        }

        let alternate_space = dictionary
            .optional::<ColorSpace>(b"Alternate")?
            .map(Box::new);

        Ok(Self {
            num_components,
            alternate_space,
            profile_data,
        })
    }
}

impl ICCBasedColorSpace {
    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        if components.len() != self.num_components {
            return Err(ColorSpaceError::InsufficientComponents(
                self.num_components,
                components.len(),
            ));
        }
        // ICC colour management is not yet implemented.
        // Use the alternate space if present; otherwise use the device-equivalent.
        if let Some(alt) = &self.alternate_space {
            return alt.apply(components);
        }
        Color::from_device_components(components).ok_or_else(|| {
            ColorSpaceError::Unsupported(format!(
                "ICCBased with {} components has no alternate space",
                self.num_components
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use pdf_graphics::color::Color;
    use pdf_object_reader::{
        ObjectReader, dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::ICCBasedColorSpace;
    use crate::color_space::ColorSpace;

    fn icc_based(num_components: usize) -> ICCBasedColorSpace {
        ICCBasedColorSpace {
            num_components,
            alternate_space: None,
            profile_data: Bytes::new(),
        }
    }

    #[test]
    fn parsed_profile_shares_stream_data() {
        let stream = StreamObject::new(
            1,
            0,
            Dictionary::from_entries([(b"N", ObjectVariant::Integer(3))]),
            vec![1, 2, 3, 4],
        );
        let stream_data = stream.shared_data();
        let array = ObjectVariant::Array(
            vec![
                pdf_object_reader::pdf_string::PdfString::from(
                    b"ICCBased".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
                ObjectVariant::Stream(stream),
            ]
            .into(),
        );

        let parsed = ObjectReader::new(&PassthroughResolver)
            .read::<ColorSpace>(&array)
            .expect("ICCBased color space should parse");
        let ColorSpace::ICCBased(icc_based) = parsed else {
            panic!("expected ICCBased color space");
        };

        assert_eq!(icc_based.profile_data.as_ptr(), stream_data.as_ptr());
    }

    #[test]
    fn uses_device_equivalent_without_an_alternate_space() {
        assert_eq!(
            icc_based(1).apply(&[0.5]).expect("valid gray fallback"),
            Color::from_gray(0.5)
        );
        assert_eq!(
            icc_based(3)
                .apply(&[0.1, 0.2, 0.3])
                .expect("valid RGB fallback"),
            Color::from_rgb(0.1, 0.2, 0.3)
        );
        assert_eq!(
            icc_based(4)
                .apply(&[0.1, 0.2, 0.3, 0.4])
                .expect("valid CMYK fallback"),
            Color::from_cmyk(0.1, 0.2, 0.3, 0.4)
        );
    }
}
