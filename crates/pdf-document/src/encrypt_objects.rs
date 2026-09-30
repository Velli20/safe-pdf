//! Loads the encryption dictionary before the document's objects are loaded.
//!
//! Decrypting objects requires the encryption dictionary, so it cannot come from the
//! regular object loader. Its entries, such as `/CF` and the crypt filters inside it,
//! may still be indirect objects. This module parses the dictionary and the uncompressed
//! objects it references directly from their cross-reference offsets.

use std::collections::BTreeMap;

use crate::encryption::EncryptDictionary;
use crate::error::PdfReaderError;
use crate::reader::object_id;
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    cross_reference_table::CrossReferenceEntryType, object_error::ObjectError,
    object_resolver::PassthroughResolver, object_variant::ObjectVariant,
};
use pdf_parser::{error::ParserError, parser::PdfParser};

/// Upper bound on indirect objects loaded for one encryption dictionary.
///
/// A well-formed dictionary references a handful of crypt filters; the bound keeps a
/// malformed reference graph from pulling the whole file in before decryption.
const MAX_ENCRYPT_OBJECTS: usize = 32;

/// Parses the encryption dictionary and the indirect objects it references.
pub(crate) struct EncryptObjectLoader<'a, 'input> {
    entries: &'a BTreeMap<usize, CrossReferenceEntryType>,
    parser: &'a PdfParser<'input>,
}

impl<'a, 'input> EncryptObjectLoader<'a, 'input> {
    /// Creates a loader over the document's cross-reference entries.
    pub(crate) fn new(
        entries: &'a BTreeMap<usize, CrossReferenceEntryType>,
        parser: &'a PdfParser<'input>,
    ) -> Self {
        Self { entries, parser }
    }

    /// Resolves the trailer's `/Encrypt` value and parses it without decrypting it.
    pub(crate) fn load_dictionary(
        &self,
        encrypt: ObjectVariant,
    ) -> Result<EncryptDictionary, PdfReaderError> {
        let dictionary = match encrypt {
            ObjectVariant::Reference(identifier) => self.load_object(identifier.number)?,
            object => object,
        };
        let objects = self.load_referenced_objects(&dictionary)?;
        EncryptDictionary::from_dictionary(dictionary.try_dictionary(&objects)?, &objects)
    }

    /// Loads the objects referenced from `root` into a collection used for resolution.
    ///
    /// Nested references that cannot be loaded are left unresolved; parsing the
    /// dictionary reports them only if the entry holding them is actually needed.
    fn load_referenced_objects(
        &self,
        root: &ObjectVariant,
    ) -> Result<ObjectCollection, PdfReaderError> {
        let mut objects = ObjectCollection::default();
        let mut pending = Vec::new();
        collect_references(root, &mut pending);
        let mut loaded = 0usize;
        while let Some(number) = pending.pop() {
            if loaded == MAX_ENCRYPT_OBJECTS {
                break;
            }
            if objects.get(number).is_some() {
                continue;
            }
            let Ok(object) = self.load_object(number) else {
                continue;
            };
            collect_references(&object, &mut pending);
            objects.insert(object_id(number), object)?;
            loaded = loaded.saturating_add(1);
        }
        Ok(objects)
    }

    /// Parses the uncompressed indirect object `number` at its cross-reference offset.
    fn load_object(&self, number: usize) -> Result<ObjectVariant, PdfReaderError> {
        let byte_offset = self
            .entries
            .get(&number)
            .and_then(CrossReferenceEntryType::byte_offset)
            .ok_or(ObjectError::FailedResolveObjectReference { obj_num: number })?;
        let mut parser = self.parser.at_offset(byte_offset)?;
        let identifier = parser.parse_indirect_object_id().ok_or(
            ParserError::ExpectedIndirectObjectDeclaration {
                position: byte_offset,
            },
        )?;
        Ok(parser.parse_indirect_object_value(identifier, &PassthroughResolver)?)
    }
}

/// Appends the object numbers of every reference nested in `value` to `references`.
fn collect_references(value: &ObjectVariant, references: &mut Vec<usize>) {
    match value {
        ObjectVariant::Reference(identifier) => references.push(identifier.number),
        ObjectVariant::Dictionary(dictionary) => dictionary
            .iter()
            .for_each(|(_, value)| collect_references(value, references)),
        ObjectVariant::Stream(stream) => stream
            .dictionary
            .iter()
            .for_each(|(_, value)| collect_references(value, references)),
        ObjectVariant::Array(array) => array
            .iter()
            .for_each(|value| collect_references(value, references)),
        _ => {}
    }
}
