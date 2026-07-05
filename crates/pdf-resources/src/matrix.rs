use pdf_graphics::transform::Transform;
use pdf_object::{dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver};

pub struct Matrix;

impl Matrix {
    const KEY: &'static str = "Matrix";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Transform>, ObjectError> {
        let Some(matrix_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let [sx, ky, kx, sy, tx, ty] = matrix_obj.try_array_of::<f32, 6>(objects)?;

        Ok(Some(Transform::from_row(sx, ky, kx, sy, tx, ty)))
    }
}
