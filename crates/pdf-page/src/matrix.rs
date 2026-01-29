use pdf_graphics::transform::Transform;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    traits::FromDictionary,
};

pub struct Matrix;

impl FromDictionary for Matrix {
    const KEY: &'static str = "Matrix";
    type ResultType = Option<Transform>;
    type ErrorType = ObjectError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(matrix_obj) = dictionary.get("Matrix") else {
            return Ok(None);
        };

        let [sx, ky, kx, sy, tx, ty] = matrix_obj.try_array_of::<f32, 6>(objects)?;

        Ok(Some(Transform::from_row(sx, ky, kx, sy, tx, ty)))
    }
}
