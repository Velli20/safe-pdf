use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum MatrixReadError {
    #[error("Invalid type for /Matrix entry: expected Array, found {found_type}")]
    InvalidEntryType { found_type: &'static str },
    #[error("/Matrix array must have 6 elements, but it has {count}")]
    InvalidElementCount { count: usize },
    #[error("Failed to convert element in /Matrix array to a number: {source}")]
    NumericConversionError {
        #[source]
        source: ObjectError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix(pub [f32; 6]);

impl FromDictionary for Matrix {
    const KEY: &'static str = "Matrix";
    type ResultType = Option<Matrix>;
    type ErrorType = MatrixReadError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, MatrixReadError> {
        if let Some(matrix_obj) = dictionary.get("Matrix") {
            let arr = matrix_obj
                .as_array()
                .ok_or(MatrixReadError::InvalidEntryType {
                    found_type: matrix_obj.name(),
                })?;
            // `/Matrix` must have exactly 6 elements if present.
            if arr.len() != 6 {
                return Err(MatrixReadError::InvalidElementCount { count: arr.len() });
            }
            let mut vals = [0.0f32; 6];
            for (i, obj) in arr.iter().enumerate() {
                vals[i] = obj
                    .as_number::<f32>()
                    .map_err(|source| MatrixReadError::NumericConversionError { source })?;
            }
            Ok(Some(Matrix(vals)))
        } else {
            Ok(None)
        }
    }
}
