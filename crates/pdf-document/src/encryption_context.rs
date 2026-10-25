//! Decryption state applied while a document's objects are loaded.
//!
//! The trailer's `/Encrypt` entry selects the security handler, and its `/ID` array
//! supplies the identifier mixed into password-derived file keys. Both are read before
//! any other object, because every later object may need decrypting.

use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_id::ObjectId,
    object_variant::ObjectVariant, pdf_string::PdfString, trailer::Trailer,
};

use crate::decryption::{DecryptionError, DocumentDecryptor};
use crate::diagnostic::PdfReadDiagnostic;
use crate::encrypt_objects::EncryptObjectLoader;
use crate::error::PdfReaderError;

/// Decrypts the objects of a document whose streams or strings are encrypted.
pub(crate) struct EncryptionContext {
    /// Authenticated decryptor for the document's default crypt filters.
    decryptor: DocumentDecryptor,
    /// The indirect encryption dictionary, which is itself stored unencrypted.
    dictionary: Option<ObjectId>,
}

impl EncryptionContext {
    /// Authenticates against the trailer's `/Encrypt` dictionary.
    ///
    /// Returns `None` when the document stores its streams and strings unencrypted,
    /// including when a malformed encryption dictionary was reported and ignored.
    pub(crate) fn from_trailer(
        trailer: &mut Trailer,
        loader: &EncryptObjectLoader,
        password: Option<&[u8]>,
        diagnostics: &mut Vec<PdfReadDiagnostic>,
    ) -> Result<Option<Self>, PdfReaderError> {
        let Some(encrypt) = trailer.dictionary.take(b"Encrypt") else {
            return Ok(None);
        };
        let dictionary = match &encrypt {
            ObjectVariant::Reference(identifier) => Some(*identifier),
            _ => None,
        };
        let Some(encryption) = loader.load_dictionary(encrypt, dictionary, diagnostics)? else {
            return Ok(None);
        };
        if encryption.leaves_streams_and_strings_unencrypted() {
            return Ok(None);
        }

        let document_id = loader.required::<DocumentId>(&trailer.dictionary, b"ID")?;
        let decryptor = DocumentDecryptor::new(
            &encryption,
            document_id.0.as_bytes(),
            password.unwrap_or_default(),
        )
        .map_err(PdfReaderError::from_decryption_setup)?;
        Ok(Some(Self {
            decryptor,
            dictionary,
        }))
    }

    /// Decrypts an indirect object, leaving the encryption dictionary itself unchanged.
    pub(crate) fn decrypt(
        &self,
        identifier: ObjectId,
        object: ObjectVariant,
    ) -> Result<ObjectVariant, DecryptionError> {
        if self
            .dictionary
            .is_some_and(|dictionary| dictionary.number == identifier.number)
        {
            return Ok(object);
        }
        self.decryptor.decrypt_object(identifier, object)
    }
}

/// The first string of the trailer's `/ID` array, used to derive file keys.
struct DocumentId(PdfString);

impl FromPdfObject for DocumentId {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut identifiers = context.array()?;
        if identifiers.array().is_empty() {
            return Err(PdfReaderError::MissingDocumentId.into());
        }
        identifiers.at::<PdfString>(0).map(Self)
    }
}
