use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, FileSpecification};

/// Annotation-specific file attachment state.
pub struct FileAttachmentAnnotation {
    /// The required file specification.
    pub file_specification: FileSpecification,
    /// The file icon name.
    pub name: Option<Vec<u8>>,
}

impl FileAttachmentAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let file_specification = FileSpecification::from_dictionary(dictionary, "FS", objects)?
            .ok_or(AnnotationError::MissingEntry { entry: "FS" })?;
        let name = dictionary.optional_bytes_vec("Name", objects)?;

        Ok(Self {
            file_specification,
            name,
        })
    }
}
