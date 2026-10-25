//! Loads the encryption dictionary before the document's objects are loaded.
//!
//! Decrypting objects requires the encryption dictionary, so it cannot come from the
//! regular object loader. Its entries, such as `/CF` and the crypt filters inside it,
//! may still be indirect objects. This module parses the dictionary and the uncompressed
//! objects it references directly from their cross-reference offsets.

use std::collections::BTreeMap;

use crate::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
use crate::encryption::EncryptDictionary;
use crate::error::PdfReaderError;
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    FromPdfObject, ObjectReader, cross_reference_table::CrossReferenceEntryType,
    dictionary::Dictionary, object_error::ObjectError, object_id::ObjectId,
    object_variant::ObjectVariant,
};
use pdf_parser::{error::ParserError, indirect_object::IndirectValue, parser::PdfParser};

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
    ///
    /// A dictionary that cannot be parsed is reported as a diagnostic and yields `None`,
    /// so the document is read as unencrypted; other failures are fatal.
    pub(crate) fn load_dictionary(
        &self,
        encrypt: ObjectVariant,
        identifier: Option<ObjectId>,
        diagnostics: &mut Vec<PdfReadDiagnostic>,
    ) -> Result<Option<EncryptDictionary>, PdfReaderError> {
        match self.read::<EncryptDictionary>(encrypt) {
            Ok(dictionary) => Ok(Some(dictionary)),
            Err(error) if error.is_recoverable_optional_object_error() => {
                diagnostics.push(PdfReadDiagnostic::new(
                    PdfReadDiagnosticKind::MalformedEncryption,
                    None,
                    identifier,
                    error,
                ));
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Loads `value` and the objects it references, then decodes it through them.
    ///
    /// Nested references that cannot be loaded are left unresolved; decoding reports
    /// them only if the entry holding them is actually needed.
    fn read<T: FromPdfObject>(&self, value: ObjectVariant) -> Result<T, PdfReaderError> {
        let mut objects = ObjectCollection::default();
        let value = match value {
            ObjectVariant::Reference(identifier) => {
                self.load_object(identifier.number, &objects)?
            }
            object => object,
        };
        self.load_referenced_objects(&value, &mut objects)?;
        Ok(ObjectReader::new(objects).read::<T>(&value)?)
    }

    /// Loads the required `key` entry of `dictionary` and the objects it references, then
    /// decodes it through them.
    ///
    /// Nested references that cannot be loaded are left unresolved; decoding reports
    /// them only if the entry holding them is actually needed.
    pub(crate) fn required<T: FromPdfObject>(
        &self,
        dictionary: &Dictionary,
        key: &[u8],
    ) -> Result<T, PdfReaderError> {
        let mut objects = ObjectCollection::default();
        if let Some(value) = dictionary.get(key) {
            self.load_referenced_objects(value, &mut objects)?;
        }
        Ok(ObjectReader::new(objects).required::<T>(dictionary, key)?)
    }

    /// Loads the objects referenced from `root` into `objects`.
    fn load_referenced_objects(
        &self,
        root: &ObjectVariant,
        objects: &mut ObjectCollection,
    ) -> Result<(), PdfReaderError> {
        let mut pending = PendingReferences::default();
        pending.push_nested(root);
        let mut loaded = 0usize;
        while loaded < MAX_ENCRYPT_OBJECTS
            && let Some(identifier) = pending.pop()
        {
            if objects.get(identifier.number).is_some() {
                continue;
            }
            let Ok(object) = self.load_object(identifier.number, objects) else {
                continue;
            };
            pending.push_nested(&object);
            objects.insert(identifier, object)?;
            loaded = loaded.saturating_add(1);
        }
        Ok(())
    }

    /// Parses the uncompressed indirect object `number` at its cross-reference offset.
    ///
    /// A stream's `/Length` may refer to one of the `objects` already loaded.
    fn load_object(
        &self,
        number: usize,
        objects: &ObjectCollection,
    ) -> Result<ObjectVariant, PdfReaderError> {
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
        Ok(match parser.parse_indirect_object_value(identifier)? {
            IndirectValue::Object(object) => object,
            IndirectValue::Stream(header) => {
                let length = objects
                    .loaded()
                    .required::<usize>(header.dictionary(), b"Length")
                    .map_err(ParserError::from)?;
                parser.parse_stream_body(header, length)?
            }
        })
    }
}

/// Indirect references discovered in the encryption dictionary but not yet loaded.
#[derive(Default)]
struct PendingReferences(Vec<ObjectId>);

impl PendingReferences {
    /// Queues every reference nested in `value`.
    fn push_nested(&mut self, value: &ObjectVariant) {
        match value {
            ObjectVariant::Reference(identifier) => self.0.push(*identifier),
            ObjectVariant::Dictionary(dictionary) => dictionary
                .iter()
                .for_each(|(_, value)| self.push_nested(value)),
            ObjectVariant::Stream(stream) => stream
                .dictionary
                .iter()
                .for_each(|(_, value)| self.push_nested(value)),
            ObjectVariant::Array(array) => array.iter().for_each(|value| self.push_nested(value)),
            _ => {}
        }
    }

    /// Takes the most recently queued reference.
    fn pop(&mut self) -> Option<ObjectId> {
        self.0.pop()
    }
}
