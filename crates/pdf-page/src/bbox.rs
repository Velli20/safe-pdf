use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum BBoxReadError {
    #[error("Error parsing Dictionary: {0}")]
    ObjectError(#[from] ObjectError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BBox(pub [f32; 4]);

impl BBox {
    fn from(obj: &ObjectVariant) -> Result<Self, BBoxReadError> {
        let vals = obj.as_array_of::<f32, 4>()?;
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
