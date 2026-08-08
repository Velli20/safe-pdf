use std::collections::BTreeMap;

use pdf_graphics::{rect::Rect, transform::Transform};

use crate::{
    error::ObjectError, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<String, ObjectVariant>,
    pub object_number: Option<usize>,
}

impl Dictionary {
    const HEIGHT_KEY: &'static str = "Height";
    const WIDTH_KEY: &'static str = "Width";

    pub fn new(dictionary: BTreeMap<String, ObjectVariant>) -> Self {
        Dictionary {
            dictionary,
            object_number: None,
        }
    }

    /// Returns a reference to the value associated with the given key, if present.
    ///
    /// # Parameters:
    ///
    ///  - `key`: The dictionary entry name to look up.
    ///
    /// # Returns
    ///
    /// Returns an optional reference to [`ObjectVariant`] when the key exists, or `None` if it does not.
    pub fn get(&self, key: &str) -> Option<&ObjectVariant> {
        self.dictionary.get(key)
    }

    /// Converts the value associated with the given key when it is present.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary entry name to look up.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(T))` when the key exists and conversion succeeds, `Ok(None)` when the
    /// key is absent, or `Err(T::Error)` when conversion fails.
    pub fn try_get_as<T>(
        &self,
        key: &str,
    ) -> Result<Option<T>, <T as TryFrom<&ObjectVariant>>::Error>
    where
        for<'a> T: TryFrom<&'a ObjectVariant>,
    {
        self.get(key).map(T::try_from).transpose()
    }

    /// Removes and returns the value associated with the given key, if present.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary entry name to remove and return.
    ///
    /// # Returns
    ///
    /// Returns an `Option` containing the [`ObjectVariant`] if the key exists, or `None` if it does not.
    pub fn take(&mut self, key: &str) -> Option<ObjectVariant> {
        self.dictionary.remove(key)
    }

    /// Returns a reference to the value associated with the given key, or an error if the key is missing.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary entry name to look up.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&ObjectVariant)` if the key exists, or an [`ObjectError::MissingRequiredKey`] if it does not.
    pub fn get_or_err(&self, key: &str) -> Result<&ObjectVariant, ObjectError> {
        self.get(key)
            .ok_or_else(|| ObjectError::MissingRequiredKey {
                key: key.to_owned(),
            })
    }

    /// Reads the optional `/BBox` entry as a rectangle.
    ///
    /// Missing entries and explicit PDF `null` values are treated as absent.
    /// The coordinates are returned in their original order.
    pub fn optional_bbox(&self, objects: &dyn ObjectResolver) -> Result<Option<Rect>, ObjectError> {
        Ok(self
            .optional_array_of::<f32, 4>("BBox", objects)?
            .map(Rect::from))
    }

    /// Reads the required `/BBox` entry as a rectangle.
    ///
    /// The coordinates are returned in their original order.
    pub fn required_bbox(&self, objects: &dyn ObjectResolver) -> Result<Rect, ObjectError> {
        self.required_array_of::<f32, 4>("BBox", objects)
            .map(Rect::from)
    }

    /// Reads optional `/Width` and `/Height` entries as an origin-based integer rectangle.
    ///
    /// Returns `None` when both entries are missing or `null`. If only one dimension is
    /// available, the missing counterpart is reported as a required-key error.
    pub fn optional_size(
        &self,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Rect<usize>>, ObjectError> {
        let width = self.optional_number::<usize>(Self::WIDTH_KEY, objects)?;
        let height = self.optional_number::<usize>(Self::HEIGHT_KEY, objects)?;

        if width.is_none() && height.is_none() {
            return Ok(None);
        }

        let width = width.ok_or_else(|| ObjectError::MissingRequiredKey {
            key: Self::WIDTH_KEY.to_owned(),
        })?;
        let height = height.ok_or_else(|| ObjectError::MissingRequiredKey {
            key: Self::HEIGHT_KEY.to_owned(),
        })?;

        Ok(Some(Rect::<usize>::from_size(width, height)))
    }

    /// Reads required `/Width` and `/Height` entries as an origin-based integer rectangle.
    pub fn required_size(&self, objects: &dyn ObjectResolver) -> Result<Rect<usize>, ObjectError> {
        let width = self.required_number::<usize>(Self::WIDTH_KEY, objects)?;
        let height = self.required_number::<usize>(Self::HEIGHT_KEY, objects)?;

        Ok(Rect::<usize>::from_size(width, height))
    }

    /// Reads the optional `/MediaBox` entry as a rectangle.
    ///
    /// Missing entries and explicit PDF `null` values are treated as absent.
    /// The coordinates are returned in their original order.
    pub fn optional_media_box(
        &self,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Rect>, ObjectError> {
        Ok(self
            .optional_array_of::<f32, 4>("MediaBox", objects)?
            .map(Rect::from))
    }

    /// Reads the optional `/Matrix` entry as an affine transform.
    ///
    /// Missing entries and explicit PDF `null` values are treated as absent.
    pub fn optional_matrix(
        &self,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Transform>, ObjectError> {
        Ok(self
            .optional_array_of::<f32, 6>("Matrix", objects)?
            .map(|[sx, ky, kx, sy, tx, ty]| Transform::from_row(sx, ky, kx, sy, tx, ty)))
    }

    /// Reads the required `/Matrix` entry as an affine transform.
    pub fn required_matrix(&self, objects: &dyn ObjectResolver) -> Result<Transform, ObjectError> {
        let [sx, ky, kx, sy, tx, ty] = self.required_array_of::<f32, 6>("Matrix", objects)?;

        Ok(Transform::from_row(sx, ky, kx, sy, tx, ty))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::object_resolver::PassthroughResolver;

    use super::*;

    #[derive(Debug, PartialEq)]
    struct ParsedInteger(i64);

    impl TryFrom<&ObjectVariant> for ParsedInteger {
        type Error = &'static str;

        fn try_from(value: &ObjectVariant) -> Result<Self, Self::Error> {
            match value {
                ObjectVariant::Integer(value) => Ok(Self(*value)),
                _ => Err("expected integer"),
            }
        }
    }

    fn dictionary_with(key: &str, value: ObjectVariant) -> Dictionary {
        let mut values = BTreeMap::new();
        values.insert(key.to_owned(), value);
        Dictionary::new(values)
    }

    fn size_dictionary(width: ObjectVariant, height: ObjectVariant) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            ("Height".to_owned(), height),
            ("Width".to_owned(), width),
        ]))
    }

    #[test]
    fn try_get_as_converts_existing_value() {
        let dictionary = dictionary_with("Count", ObjectVariant::Integer(42));

        let value = dictionary
            .try_get_as::<ParsedInteger>("Count")
            .expect("integer parses");

        assert_eq!(value, Some(ParsedInteger(42)));
    }

    #[test]
    fn try_get_as_returns_none_for_missing_key() {
        let dictionary = Dictionary::new(BTreeMap::new());

        let value = dictionary
            .try_get_as::<ParsedInteger>("Count")
            .expect("missing key is not an error");

        assert_eq!(value, None);
    }

    #[test]
    fn try_get_as_propagates_conversion_error() {
        let dictionary = dictionary_with("Count", ObjectVariant::Name(b"Count".to_vec()));

        let error = dictionary
            .try_get_as::<ParsedInteger>("Count")
            .expect_err("name is not an integer");

        assert_eq!(error, "expected integer");
    }

    #[test]
    fn optional_bbox_parses_rectangle() {
        let dictionary = dictionary_with(
            "BBox",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(1),
                ObjectVariant::Real(2.5),
                ObjectVariant::Integer(3),
                ObjectVariant::Real(4.5),
            ]),
        );

        let bbox = dictionary
            .optional_bbox(&PassthroughResolver)
            .expect("bounding box parses");

        assert_eq!(
            bbox,
            Some(Rect {
                left: 1.0,
                top: 2.5,
                right: 3.0,
                bottom: 4.5,
            })
        );
    }

    #[test]
    fn optional_bbox_returns_none_for_missing_or_null_entry() {
        let missing = Dictionary::new(BTreeMap::new());
        let null = dictionary_with("BBox", ObjectVariant::Null);

        assert_eq!(
            missing
                .optional_bbox(&PassthroughResolver)
                .expect("missing bounding box is optional"),
            None
        );
        assert_eq!(
            null.optional_bbox(&PassthroughResolver)
                .expect("null bounding box is optional"),
            None
        );
    }

    #[test]
    fn required_bbox_parses_rectangle() {
        let dictionary = dictionary_with(
            "BBox",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(2),
                ObjectVariant::Integer(3),
                ObjectVariant::Integer(4),
            ]),
        );

        let bbox = dictionary
            .required_bbox(&PassthroughResolver)
            .expect("bounding box parses");

        assert_eq!(
            bbox,
            Rect {
                left: 1.0,
                top: 2.0,
                right: 3.0,
                bottom: 4.0,
            }
        );
    }

    #[test]
    fn required_bbox_propagates_invalid_array_length() {
        let dictionary = dictionary_with(
            "BBox",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(2),
                ObjectVariant::Integer(3),
            ]),
        );

        let error = dictionary
            .required_bbox(&PassthroughResolver)
            .expect_err("bounding box must contain four numbers");

        assert_eq!(
            error,
            ObjectError::InvalidArrayLength {
                expected: 4,
                found: 3,
            }
        );
    }

    #[test]
    fn optional_size_parses_width_and_height() {
        let dictionary = size_dictionary(ObjectVariant::Integer(40), ObjectVariant::Integer(15));

        let size = dictionary
            .optional_size(&PassthroughResolver)
            .expect("size parses");

        assert_eq!(size, Some(Rect::<usize>::from_size(40, 15)));
    }

    #[test]
    fn optional_size_returns_none_when_both_dimensions_are_absent() {
        let missing = Dictionary::new(BTreeMap::new());
        let null = size_dictionary(ObjectVariant::Null, ObjectVariant::Null);

        assert_eq!(
            missing
                .optional_size(&PassthroughResolver)
                .expect("missing dimensions are optional"),
            None
        );
        assert_eq!(
            null.optional_size(&PassthroughResolver)
                .expect("null dimensions are optional"),
            None
        );
    }

    #[test]
    fn optional_size_rejects_partial_dimensions() {
        let width_only = dictionary_with("Width", ObjectVariant::Integer(40));
        let height_only = dictionary_with("Height", ObjectVariant::Integer(15));

        assert_eq!(
            width_only
                .optional_size(&PassthroughResolver)
                .expect_err("height is required when width is present"),
            ObjectError::MissingRequiredKey {
                key: "Height".to_owned(),
            }
        );
        assert_eq!(
            height_only
                .optional_size(&PassthroughResolver)
                .expect_err("width is required when height is present"),
            ObjectError::MissingRequiredKey {
                key: "Width".to_owned(),
            }
        );
    }

    #[test]
    fn required_size_parses_width_and_height() {
        let dictionary = size_dictionary(ObjectVariant::Integer(40), ObjectVariant::Integer(15));

        let size = dictionary
            .required_size(&PassthroughResolver)
            .expect("required size parses");

        assert_eq!(size, Rect::<usize>::from_size(40, 15));
    }

    #[test]
    fn required_size_rejects_missing_or_invalid_dimensions() {
        let missing_height = dictionary_with("Width", ObjectVariant::Integer(40));
        let negative_width =
            size_dictionary(ObjectVariant::Integer(-1), ObjectVariant::Integer(15));

        assert_eq!(
            missing_height
                .required_size(&PassthroughResolver)
                .expect_err("height is required"),
            ObjectError::MissingRequiredKey {
                key: "Height".to_owned(),
            }
        );
        assert_eq!(
            negative_width
                .required_size(&PassthroughResolver)
                .expect_err("negative width cannot convert to usize"),
            ObjectError::NumberConversionError
        );
    }

    #[test]
    fn optional_media_box_parses_rectangle() {
        let dictionary = dictionary_with(
            "MediaBox",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(10),
                ObjectVariant::Integer(20),
                ObjectVariant::Integer(210),
                ObjectVariant::Integer(320),
            ]),
        );

        let media_box = dictionary
            .optional_media_box(&PassthroughResolver)
            .expect("media box parses");

        assert_eq!(
            media_box,
            Some(Rect {
                left: 10.0,
                top: 20.0,
                right: 210.0,
                bottom: 320.0,
            })
        );
    }

    #[test]
    fn optional_media_box_returns_none_for_missing_or_null_entry() {
        let missing = Dictionary::new(BTreeMap::new());
        let null = dictionary_with("MediaBox", ObjectVariant::Null);

        assert_eq!(
            missing
                .optional_media_box(&PassthroughResolver)
                .expect("missing media box is optional"),
            None
        );
        assert_eq!(
            null.optional_media_box(&PassthroughResolver)
                .expect("null media box is optional"),
            None
        );
    }

    #[test]
    fn optional_media_box_propagates_invalid_array_length() {
        let dictionary = dictionary_with(
            "MediaBox",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(10),
                ObjectVariant::Integer(20),
                ObjectVariant::Integer(210),
            ]),
        );

        let error = dictionary
            .optional_media_box(&PassthroughResolver)
            .expect_err("media box must contain four numbers");

        assert_eq!(
            error,
            ObjectError::InvalidArrayLength {
                expected: 4,
                found: 3,
            }
        );
    }

    #[test]
    fn optional_matrix_parses_transform() {
        let dictionary = dictionary_with(
            "Matrix",
            ObjectVariant::Array(vec![
                ObjectVariant::Real(1.0),
                ObjectVariant::Real(2.0),
                ObjectVariant::Real(3.0),
                ObjectVariant::Real(4.0),
                ObjectVariant::Real(5.0),
                ObjectVariant::Real(6.0),
            ]),
        );

        let matrix = dictionary
            .optional_matrix(&PassthroughResolver)
            .expect("matrix parses");

        assert_eq!(
            matrix,
            Some(Transform::from_row(1.0, 2.0, 3.0, 4.0, 5.0, 6.0))
        );
    }

    #[test]
    fn optional_matrix_returns_none_for_missing_or_null_entry() {
        let missing = Dictionary::new(BTreeMap::new());
        let null = dictionary_with("Matrix", ObjectVariant::Null);

        assert_eq!(
            missing
                .optional_matrix(&PassthroughResolver)
                .expect("missing matrix is optional"),
            None
        );
        assert_eq!(
            null.optional_matrix(&PassthroughResolver)
                .expect("null matrix is optional"),
            None
        );
    }

    #[test]
    fn required_matrix_parses_transform() {
        let dictionary = dictionary_with(
            "Matrix",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(2),
                ObjectVariant::Integer(3),
                ObjectVariant::Integer(4),
                ObjectVariant::Integer(5),
                ObjectVariant::Integer(6),
            ]),
        );

        let matrix = dictionary
            .required_matrix(&PassthroughResolver)
            .expect("matrix parses");

        assert_eq!(matrix, Transform::from_row(1.0, 2.0, 3.0, 4.0, 5.0, 6.0));
    }

    #[test]
    fn required_matrix_propagates_invalid_array_length() {
        let dictionary = dictionary_with(
            "Matrix",
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(2),
                ObjectVariant::Integer(3),
                ObjectVariant::Integer(4),
                ObjectVariant::Integer(5),
            ]),
        );

        let error = dictionary
            .required_matrix(&PassthroughResolver)
            .expect_err("matrix must contain six numbers");

        assert_eq!(
            error,
            ObjectError::InvalidArrayLength {
                expected: 6,
                found: 5,
            }
        );
    }
}
