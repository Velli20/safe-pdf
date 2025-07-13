use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum BBoxReadError {
    #[error("Missing required entry '{entry_name}' in Form XObject dictionary")]
    MissingEntry { entry_name: &'static str },
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Error parsing /BBox array: Expected 4 elements, got {count}")]
    InvalidElementCount { count: usize },

    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BBox(pub [f32; 4]);

impl BBox {
    fn from(obj: &ObjectVariant) -> Result<Self, BBoxReadError> {
        let arr = obj.as_array().ok_or(BBoxReadError::InvalidEntryType {
            entry_name: "BBox",
            expected_type: "Array",
            found_type: obj.name(),
        })?;

        // `/BBox` must have exactly 4 elements.
        if arr.len() != 4 {
            return Err(BBoxReadError::InvalidElementCount { count: arr.len() });
        }
        let mut vals = [0.0f32; 4];
        for (i, obj) in arr.iter().enumerate() {
            vals[i] =
                obj.as_number::<f32>()
                    .map_err(|e| BBoxReadError::NumericConversionError {
                        entry_description: "width in [w1...wn] array",
                        source: e,
                    })?;
        }
        Ok(BBox(vals))
    }
}

impl FromDictionary for BBox {
    const KEY: &'static str = "BBox";
    type ResultType = Option<BBox>;
    type ErrorType = BBoxReadError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, BBoxReadError> {
        // Retrieve the `/BBox` entry, which must be an array of 4 numbers.
        if let Some(matrix_obj) = dictionary.get("BBox") {
            Ok(Some(Self::from(matrix_obj)?))
        } else {
            Ok(None)
        }
    }
}
