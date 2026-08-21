use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::AnnotationError;

/// A file specification.
pub enum FileSpecification {
    /// A file specification dictionary.
    Dictionary(FileSpecificationDictionary),
    /// A direct path string.
    Path(Vec<u8>),
}

/// A parsed file specification dictionary.
pub struct FileSpecificationDictionary {
    /// The file system name.
    pub file_system: Option<Vec<u8>>,
    /// The platform-independent file name.
    pub file_name: Option<Vec<u8>>,
    /// The Unicode file name.
    pub unicode_file_name: Option<Vec<u8>>,
    /// The macOS file name.
    pub mac_file_name: Option<Vec<u8>>,
    /// The DOS file name.
    pub dos_file_name: Option<Vec<u8>>,
    /// The Unix file name.
    pub unix_file_name: Option<Vec<u8>>,
    /// Whether the file is volatile.
    pub volatile: Option<bool>,
}

impl FileSpecification {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        dictionary
            .get(key)
            .map(|value| Self::from_object(value, objects))
            .transpose()
    }

    pub(crate) fn from_object(
        value: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let Ok(dictionary) = value.try_dictionary(objects) else {
            return Ok(Self::Path(value.try_bytes(objects)?.to_vec()));
        };

        let file_system = dictionary.optional_bytes(b"FS", objects)?.map(Vec::from);
        let file_name = dictionary.optional_bytes_vec(b"F", objects)?;
        let unicode_file_name = dictionary.optional_bytes_vec(b"UF", objects)?;
        let mac_file_name = dictionary.optional_bytes_vec(b"Mac", objects)?;
        let dos_file_name = dictionary.optional_bytes_vec(b"DOS", objects)?;
        let unix_file_name = dictionary.optional_bytes_vec(b"Unix", objects)?;
        let volatile = dictionary.optional_boolean(b"V", objects)?;

        Ok(Self::Dictionary(FileSpecificationDictionary {
            file_system,
            file_name,
            unicode_file_name,
            mac_file_name,
            dos_file_name,
            unix_file_name,
            volatile,
        }))
    }
}
