use pdf_graphics::transform::Transform;
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

pub struct Matrix;

impl FromDictionary for Matrix {
    const KEY: &'static str = "Matrix";
    type ResultType = Option<Transform>;
    type ErrorType = MatrixReadError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, MatrixReadError> {
        let Some(matrix_obj) = dictionary.get("Matrix") else {
            return Ok(None);
        };

        let [sx, ky, kx, sy, tx, ty] = matrix_obj.as_array_of::<f32, 6>()?;

        Ok(Some(Transform::from_row(sx, ky, kx, sy, tx, ty)))
    }
}
