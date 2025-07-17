use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum MatrixReadError {
    #[error("Failed to parse /Matrix: {0}")]
    ObjectError(#[from] ObjectError),
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
            let arr = matrix_obj.as_array_of::<f32, 6>()?;
            Ok(Some(Matrix(arr)))
        } else {
            Ok(None)
        }
    }
}
