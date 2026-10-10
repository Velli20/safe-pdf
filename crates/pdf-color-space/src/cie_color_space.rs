use pdf_object_reader::{DictionaryContext, ObjectAccess, ReadResult};

/// Parameters shared by the CalGray, CalRGB, and Lab color spaces.
#[derive(Debug, PartialEq)]
pub(crate) struct CieColorSpaceParams {
    /// Reference white point in CIE XYZ coordinates `[Xw, Yw, Zw]`.
    pub(crate) white_point: [f32; 3],
    /// Reference black point in CIE XYZ coordinates `[Xb, Yb, Zb]`.
    ///
    /// Defaults to `[0.0, 0.0, 0.0]` when `/BlackPoint` is absent.
    pub(crate) black_point: [f32; 3],
}

impl CieColorSpaceParams {
    /// Reads `/WhitePoint` and `/BlackPoint` from a CIE-based color space dictionary.
    pub(crate) fn read(
        dictionary: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let white_point = dictionary.required(b"WhitePoint")?;
        let black_point = dictionary.optional(b"BlackPoint")?.unwrap_or_default();

        Ok(Self {
            white_point,
            black_point,
        })
    }
}

#[cfg(test)]
mod tests {
    use pdf_object_reader::{
        ObjectReader, dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant,
    };

    use crate::cal_gray_color_space::CalGrayColorSpace;

    fn read_cal_gray(dictionary: Dictionary) -> CalGrayColorSpace {
        ObjectReader::new(&PassthroughResolver)
            .read::<CalGrayColorSpace>(&ObjectVariant::Dictionary(dictionary))
            .unwrap()
    }

    fn array(values: &[f64]) -> ObjectVariant {
        ObjectVariant::Array(values.iter().copied().map(ObjectVariant::Real).collect())
    }

    #[test]
    fn parses_white_and_black_points() {
        let dictionary = Dictionary::from_entries([
            (b"BlackPoint", array(&[0.1, 0.2, 0.3])),
            (b"WhitePoint", array(&[0.9, 1.0, 0.8])),
        ]);

        let cal_gray = read_cal_gray(dictionary);

        assert_eq!(cal_gray.white_point, [0.9, 1.0, 0.8]);
        assert_eq!(cal_gray.black_point, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn defaults_missing_black_point_to_zero() {
        let dictionary = Dictionary::from_entries([(b"WhitePoint", array(&[0.9, 1.0, 0.8]))]);

        let cal_gray = read_cal_gray(dictionary);

        assert_eq!(cal_gray.black_point, [0.0, 0.0, 0.0]);
    }
}
