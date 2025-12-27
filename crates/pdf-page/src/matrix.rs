use pdf_graphics::transform::Transform;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

pub struct Matrix;

impl FromDictionary for Matrix {
    const KEY: &'static str = "Matrix";
    type ResultType = Option<Transform>;
    type ErrorType = ObjectError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(matrix_obj) = dictionary.get("Matrix") else {
            return Ok(None);
        };

        let [sx, ky, kx, sy, tx, ty] = matrix_obj.as_array_of::<f32, 6>()?;

        Ok(Some(Transform::from_row(sx, ky, kx, sy, tx, ty)))
    }
}
