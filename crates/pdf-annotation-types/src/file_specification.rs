use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
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
        key: &'static str,
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

        let file_system = dictionary
            .get("FS")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let file_name = dictionary
            .get("F")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let unicode_file_name = dictionary
            .get("UF")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let mac_file_name = dictionary
            .get("Mac")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let dos_file_name = dictionary
            .get("DOS")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let unix_file_name = dictionary
            .get("Unix")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let volatile = dictionary
            .get("V")
            .map(|value| value.try_boolean(objects))
            .transpose()?;

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
