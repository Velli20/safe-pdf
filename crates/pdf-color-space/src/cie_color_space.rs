use pdf_object_reader::{
    dictionary::Dictionary, object_error::ObjectError, object_lookup::ObjectLookupExt,
    object_resolver::ObjectResolver,
};

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
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, ObjectError> {
        let white_point = dictionary.required_array_of::<f32, 3>(b"WhitePoint", objects)?;
        let black_point = dictionary
            .optional_array_of::<f32, 3>(b"BlackPoint", objects)?
            .unwrap_or_default();

        Ok(Self {
            white_point,
            black_point,
        })
    }
}

#[cfg(test)]
mod tests {
    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::CieColorSpaceParams;

    fn array(values: &[f64]) -> ObjectVariant {
        ObjectVariant::Array(values.iter().copied().map(ObjectVariant::Real).collect())
    }

    #[test]
    fn parses_white_and_black_points() {
        let dictionary = Dictionary::from_entries([
            (b"BlackPoint", array(&[0.1, 0.2, 0.3])),
            (b"WhitePoint", array(&[0.9, 1.0, 0.8])),
        ]);

        let params =
            CieColorSpaceParams::from_dictionary(&dictionary, &PassthroughResolver).unwrap();

        assert_eq!(
            params,
            CieColorSpaceParams {
                white_point: [0.9, 1.0, 0.8],
                black_point: [0.1, 0.2, 0.3],
            }
        );
    }

    #[test]
    fn defaults_missing_black_point_to_zero() {
        let dictionary = Dictionary::from_entries([(b"WhitePoint", array(&[0.9, 1.0, 0.8]))]);

        let params =
            CieColorSpaceParams::from_dictionary(&dictionary, &PassthroughResolver).unwrap();

        assert_eq!(params.black_point, [0.0, 0.0, 0.0]);
    }
}
