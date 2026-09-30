//! PDF Decryption implementation.
//!
//! This module implements PDF decryption according to the PDF 1.7 specification
//! (Section 7.6 "Encryption"). It supports:
//!
//! - Standard security handler (password-based encryption)
//! - RC4 encryption (V1, V2)
//! - AES-128 encryption (V4)
//! - AES-256 encryption (V5, revisions 5 and 6)
//!
//! # PDF Encryption Overview
//!
//! PDF encryption works as follows:
//! 1. The encryption dictionary specifies the algorithm and parameters
//! 2. A file encryption key is derived from the password + document ID
//! 3. For V=1 through V=4, each object has a unique key derived from the file key
//! 4. Streams and strings are encrypted/decrypted with the object key
//!
//! # Algorithm Selection
//!
//! - V=1, R=2: RC4 with 40-bit key (Algorithm 1)
//! - V=2, R=3: RC4 with variable length key up to 128-bit (Algorithm 1)
//! - V=4, R=4: AES-128 in CBC mode (Algorithm 1 for key, AES for encryption)
//! - V=5, R=5/6: AES-256 in CBC mode with a password-wrapped file key

use thiserror::Error;

use crate::{
    cipher::{aes_128_cbc_decrypt, aes_256_cbc_decrypt, rc4_crypt},
    encryption::{CryptFilterMethod, EncryptDictionary, EncryptionFilter, EncryptionVersion},
    md5_key_derivation::{Md5KeyDerivation, compute_object_key},
    sha2_key_derivation::Sha2KeyDerivation,
};
use pdf_object_reader::{
    dictionary::Dictionary, object_id::ObjectId, object_variant::ObjectVariant,
    stream::StreamObject,
};

/// Errors that can occur during PDF decryption.
#[derive(Debug, Error)]
pub enum DecryptionError {
    #[error("incorrect password")]
    IncorrectPassword,
    #[error("unsupported encryption algorithm: V={version} ")]
    UnsupportedAlgorithm { version: EncryptionVersion },
    #[error("unsupported security handler: {0}")]
    UnsupportedSecurityHandler(String),
    #[error("AES decryption failed: {0}")]
    AesDecryptionFailed(String),
    #[error("invalid encrypted data: {0}")]
    InvalidData(String),
}

/// A decryptor for PDF documents.
///
/// This struct holds the encryption key and provides methods to decrypt
/// individual objects within the PDF document.
#[derive(Debug, Clone)]
pub struct DocumentDecryptor {
    /// The file encryption key derived from the password.
    file_key: Vec<u8>,
    /// The key length in bytes (used for validation, may be useful for future extensions).
    #[allow(dead_code)]
    key_length_bytes: usize,
    /// Whether metadata streams should be encrypted.
    encrypt_metadata: bool,
    /// Default crypt filter used for streams.
    stream_method: CryptFilterMethod,
    /// Default crypt filter used for strings.
    string_method: CryptFilterMethod,
}

impl DocumentDecryptor {
    /// Creates a new document decryptor by authenticating with a password.
    ///
    /// This function attempts to authenticate using the provided password
    /// (trying it as both user and owner password). If authentication succeeds,
    /// it derives the file encryption key.
    ///
    /// # Arguments
    ///
    /// - `encrypt`: The encryption dictionary from the PDF trailer.
    /// - `document_id`: The first element of the /ID array from the trailer.
    /// - `password`: The password to try (empty string for no password).
    ///
    /// # Returns
    ///
    /// A `DocumentDecryptor` on success, or a `DecryptionError` if the password
    /// is incorrect or the encryption is unsupported.
    pub(crate) fn new(
        encrypt: &EncryptDictionary,
        document_id: &[u8],
        password: &[u8],
    ) -> Result<Self, DecryptionError> {
        if let EncryptionFilter::Other(filter) = &encrypt.filter {
            return Err(DecryptionError::UnsupportedSecurityHandler(
                String::from_utf8_lossy(filter).into_owned(),
            ));
        }
        match encrypt.version {
            EncryptionVersion::V1 | EncryptionVersion::V2 | EncryptionVersion::V4 => {
                Self::new_md5(encrypt, document_id, password)
            }
            EncryptionVersion::V5 => Self::new_v5(encrypt, password),
            version @ EncryptionVersion::V3 => {
                Err(DecryptionError::UnsupportedAlgorithm { version })
            }
        }
    }

    /// Creates a V=1, V=2 or V=4 decryptor whose file key is derived with MD5 (Algorithm 2).
    fn new_md5(
        encrypt: &EncryptDictionary,
        document_id: &[u8],
        password: &[u8],
    ) -> Result<Self, DecryptionError> {
        let key_derivation = Md5KeyDerivation::new(encrypt, document_id)?;
        let file_key = key_derivation.authenticate(password)?;
        Ok(Self::from_file_key(
            encrypt,
            key_derivation.object_key_base(file_key),
            key_derivation.key_length_bytes,
        ))
    }

    /// Creates a V=5 decryptor by retrieving the AES-256 file key.
    fn new_v5(encrypt: &EncryptDictionary, password: &[u8]) -> Result<Self, DecryptionError> {
        let file_key = Sha2KeyDerivation::new(encrypt)?.authenticate(password)?;
        Ok(Self::from_file_key(
            encrypt,
            file_key,
            Sha2KeyDerivation::FILE_KEY_BYTES,
        ))
    }

    /// Builds a decryptor from an authenticated file key and the dictionary's crypt filters.
    fn from_file_key(
        encrypt: &EncryptDictionary,
        file_key: Vec<u8>,
        key_length_bytes: usize,
    ) -> Self {
        Self {
            file_key,
            key_length_bytes,
            encrypt_metadata: encrypt.encrypt_metadata,
            stream_method: encrypt.stream_method,
            string_method: encrypt.string_method,
        }
    }

    /// Decrypts stream data for a specific object.
    ///
    /// # Arguments
    ///
    /// - `object_number`: The object number of the stream.
    /// - `generation_number`: The generation number of the stream.
    /// - `encrypted_data`: The encrypted stream bytes.
    ///
    /// # Returns
    ///
    /// The decrypted stream data.
    pub fn decrypt_stream(
        &self,
        object_number: usize,
        generation_number: usize,
        encrypted_data: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        self.decrypt_data(
            self.stream_method,
            object_number,
            generation_number,
            encrypted_data,
        )
    }

    /// Decrypts a stream object when PDF encryption rules require it.
    ///
    /// Cross-reference streams are not encrypted. Metadata streams are also left
    /// unchanged when the encryption dictionary sets `/EncryptMetadata false`.
    pub(crate) fn decrypt_stream_object(
        &self,
        stream: StreamObject,
    ) -> Result<StreamObject, DecryptionError> {
        if should_skip_stream_decryption(&stream.dictionary, self.encrypt_metadata) {
            return Ok(stream);
        }

        let decrypted_data = self.decrypt_stream(
            stream.object_number,
            stream.generation_number,
            stream.raw_data(),
        )?;

        Ok(StreamObject::new_encoded(
            stream.object_number,
            stream.generation_number,
            stream.dictionary,
            decrypted_data,
        ))
    }

    /// Decrypts a string for a specific object.
    ///
    /// # Arguments
    ///
    /// - `object_number`: The object number containing the string.
    /// - `generation_number`: The generation number of the object.
    /// - `encrypted_string`: The encrypted string bytes.
    ///
    /// # Returns
    ///
    /// The decrypted string bytes.
    pub fn decrypt_string(
        &self,
        object_number: usize,
        generation_number: usize,
        encrypted_string: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        self.decrypt_data(
            self.string_method,
            object_number,
            generation_number,
            encrypted_string,
        )
    }

    /// Decrypts data using the selected document-default crypt filter.
    fn decrypt_data(
        &self,
        method: CryptFilterMethod,
        object_number: usize,
        generation_number: usize,
        encrypted_data: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        match method {
            CryptFilterMethod::Identity => Ok(encrypted_data.to_vec()),
            CryptFilterMethod::Aes256 => aes_256_cbc_decrypt(&self.file_key, encrypted_data),
            CryptFilterMethod::Rc4 => rc4_crypt(
                &compute_object_key(&self.file_key, object_number, generation_number, method)?,
                encrypted_data,
            ),
            CryptFilterMethod::Aes128 => aes_128_cbc_decrypt(
                &compute_object_key(&self.file_key, object_number, generation_number, method)?,
                encrypted_data,
            ),
        }
    }

    /// Decrypts every encrypted string and stream contained in an indirect PDF object.
    ///
    /// PDF encryption derives one key per indirect object, so nested strings use the object
    /// number and generation of the indirect object that contains them.
    pub(crate) fn decrypt_object(
        &self,
        identifier: ObjectId,
        object: ObjectVariant,
    ) -> Result<ObjectVariant, DecryptionError> {
        match object {
            ObjectVariant::Stream(mut stream) => {
                stream.object_number = identifier.number;
                stream.generation_number = identifier.generation;
                self.decrypt_stream_value(stream)
            }
            other => self.decrypt_object_value(other, identifier.number, identifier.generation),
        }
    }

    /// Decrypts a nested value using the key derived for its containing indirect object.
    fn decrypt_object_value(
        &self,
        object: ObjectVariant,
        object_number: usize,
        generation_number: usize,
    ) -> Result<ObjectVariant, DecryptionError> {
        match object {
            ObjectVariant::String(value)
                if value.kind() != pdf_object_reader::string_kind::StringKind::Name =>
            {
                Ok(pdf_object_reader::pdf_string::PdfString::from(
                    self.decrypt_string(object_number, generation_number, value.as_bytes())?,
                    value.kind(),
                ))
            }
            ObjectVariant::Array(values) => values
                .into_iter()
                .map(|value| self.decrypt_object_value(value, object_number, generation_number))
                .collect::<Result<pdf_object_reader::pdf_array::PdfArray, _>>()
                .map(ObjectVariant::Array),
            ObjectVariant::Dictionary(dictionary) => Ok(ObjectVariant::Dictionary(
                self.decrypt_dictionary(dictionary, object_number, generation_number)?,
            )),
            ObjectVariant::Stream(stream) => self.decrypt_stream_value(stream),
            other => Ok(other),
        }
    }

    /// Decrypts dictionary values while preserving signature contents required by PDF rules.
    fn decrypt_dictionary(
        &self,
        dictionary: Dictionary,
        object_number: usize,
        generation_number: usize,
    ) -> Result<Dictionary, DecryptionError> {
        let is_signature = is_signature_dictionary(&dictionary);
        let entries = dictionary
            .dictionary
            .into_iter()
            .map(|(key, value)| {
                if is_signature && key == b"Contents" {
                    Ok((key, value))
                } else {
                    self.decrypt_object_value(value, object_number, generation_number)
                        .map(|value| (key, value))
                }
            })
            .collect::<Result<_, _>>()?;
        Ok(Dictionary {
            dictionary: entries,
            object_number: dictionary.object_number,
        })
    }

    /// Decrypts a stream's data and recursively transforms its dictionary values.
    fn decrypt_stream_value(&self, stream: StreamObject) -> Result<ObjectVariant, DecryptionError> {
        let object_number = stream.object_number;
        let generation_number = stream.generation_number;
        let stream = self.decrypt_stream_object(stream)?;
        let dictionary =
            self.decrypt_dictionary(stream.dictionary, object_number, generation_number)?;
        Ok(ObjectVariant::Stream(StreamObject::new_encoded(
            object_number,
            generation_number,
            dictionary,
            stream.data,
        )))
    }
}

/// Returns whether a dictionary represents a signature whose `/Contents` is not encrypted.
fn is_signature_dictionary(dictionary: &Dictionary) -> bool {
    const SIGNATURE_KEYS: [&[u8]; 2] = [b"Type", b"FT"];
    SIGNATURE_KEYS.into_iter().any(|key| {
        matches!(
            dictionary.get(key),
            Some(ObjectVariant::String(value)) if value.as_bytes() == b"Sig"
        )
    })
}

/// Returns whether PDF encryption rules exempt this stream from decryption.
fn should_skip_stream_decryption(dictionary: &Dictionary, encrypt_metadata: bool) -> bool {
    match dictionary.get(b"Type") {
        Some(ObjectVariant::String(name)) if name.as_bytes() == b"XRef" => true,
        Some(ObjectVariant::String(name)) if name.as_bytes() == b"Metadata" => !encrypt_metadata,
        _ => false,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_decryptor(encrypt_metadata: bool) -> DocumentDecryptor {
        DocumentDecryptor {
            file_key: vec![0; 16],
            key_length_bytes: 16,
            encrypt_metadata,
            stream_method: CryptFilterMethod::Aes128,
            string_method: CryptFilterMethod::Aes128,
        }
    }

    fn make_stream(type_name: Option<&[u8]>, data: Vec<u8>) -> StreamObject {
        let mut entries = BTreeMap::new();
        if let Some(type_name) = type_name {
            entries.insert(
                Vec::from(b"Type"),
                pdf_object_reader::pdf_string::PdfString::from(
                    type_name.to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            );
        }

        StreamObject::new(7, 0, Dictionary::new(entries), data)
    }

    fn encrypt_for_object(
        decryptor: &DocumentDecryptor,
        object_number: usize,
        generation_number: usize,
        plaintext: &[u8],
    ) -> Vec<u8> {
        use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};

        type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

        let object_key = compute_object_key(
            &decryptor.file_key,
            object_number,
            generation_number,
            CryptFilterMethod::Aes128,
        )
        .expect("test object key is valid");
        let iv = [0u8; 16];
        let encryptor = Aes128CbcEnc::new_from_slices(&object_key, &iv)
            .expect("AES object key and IV should be valid");
        let mut buffer = vec![0; plaintext.len().saturating_add(16)];
        buffer[..plaintext.len()].copy_from_slice(plaintext);
        let encrypted = encryptor
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext.len())
            .expect("encryption buffer includes a padding block");
        let mut result = iv.to_vec();
        result.extend_from_slice(encrypted);
        result
    }

    #[test]
    fn test_xref_stream_decryption_is_skipped() {
        let decryptor = make_decryptor(true);
        let data = vec![0x42; 422];
        let stream = make_stream(Some(b"XRef"), data.clone());
        let data_ptr = stream.raw_data().as_ptr();

        let decrypted = decryptor.decrypt_stream_object(stream).unwrap();

        assert_eq!(decrypted.raw_data(), data.as_slice());
        assert_eq!(decrypted.raw_data().as_ptr(), data_ptr);
    }

    #[test]
    fn test_malformed_aes_ordinary_stream_still_errors() {
        let decryptor = make_decryptor(true);
        let stream = make_stream(None, vec![0x42; 422]);

        let error = decryptor.decrypt_stream_object(stream).unwrap_err();

        assert!(
            matches!(error, DecryptionError::InvalidData(message) if message == "AES ciphertext length must be a multiple of 16")
        );
    }

    #[test]
    fn test_metadata_stream_decryption_is_skipped_only_when_encrypt_metadata_is_false() {
        let data = vec![0x42; 422];
        let skipped_stream = make_stream(Some(b"Metadata"), data.clone());

        let skipped = make_decryptor(false)
            .decrypt_stream_object(skipped_stream)
            .unwrap();
        assert_eq!(skipped.raw_data(), data.as_slice());

        let encrypted_stream = make_stream(Some(b"Metadata"), data);
        let error = make_decryptor(true)
            .decrypt_stream_object(encrypted_stream)
            .unwrap_err();
        assert!(
            matches!(error, DecryptionError::InvalidData(message) if message == "AES ciphertext length must be a multiple of 16")
        );
    }

    #[test]
    fn decrypt_object_recursively_decrypts_annotation_strings() {
        let decryptor = make_decryptor(true);
        let object_number = 7;
        let generation_number = 0;
        let contents = b"H\xF6ll\xF6";
        let rich_contents = b"<p>H\xF6ll\xF6</p>";
        let identifier = ObjectId {
            number: object_number,
            generation: generation_number,
        };
        let object = ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Contents"),
                pdf_object_reader::pdf_string::PdfString::from(
                    encrypt_for_object(&decryptor, object_number, generation_number, contents),
                    pdf_object_reader::string_kind::StringKind::Literal,
                ),
            ),
            (
                Vec::from(b"RC"),
                ObjectVariant::Array(
                    vec![pdf_object_reader::pdf_string::PdfString::from(
                        encrypt_for_object(
                            &decryptor,
                            object_number,
                            generation_number,
                            rich_contents,
                        ),
                        pdf_object_reader::string_kind::StringKind::Hexadecimal,
                    )]
                    .into(),
                ),
            ),
            (
                Vec::from(b"Subtype"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"FreeText".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Parent"),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(4, 0)),
            ),
        ])));

        let decrypted = decryptor
            .decrypt_object(identifier, object)
            .expect("object decrypts");
        let ObjectVariant::Dictionary(dictionary) = decrypted else {
            panic!("decrypted object should remain a dictionary");
        };
        assert_eq!(
            dictionary.get(b"Contents"),
            Some(&pdf_object_reader::pdf_string::PdfString::from(
                contents.to_vec(),
                pdf_object_reader::string_kind::StringKind::Literal
            ))
        );
        assert_eq!(
            dictionary.get(b"RC"),
            Some(&ObjectVariant::Array(
                vec![pdf_object_reader::pdf_string::PdfString::from(
                    rich_contents.to_vec(),
                    pdf_object_reader::string_kind::StringKind::Hexadecimal.into()
                )]
                .into()
            ))
        );
        assert_eq!(
            dictionary.get(b"Subtype"),
            Some(&pdf_object_reader::pdf_string::PdfString::from(
                b"FreeText".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name
            ))
        );
        assert_eq!(
            dictionary.get(b"Parent"),
            Some(&ObjectVariant::Reference(
                pdf_object_reader::object_id::ObjectId::new(4, 0)
            ))
        );
    }

    #[test]
    fn decrypt_object_decrypts_stream_dictionary_strings() {
        let decryptor = make_decryptor(true);
        let object_number = 8;
        let generation_number = 0;
        let contents = b"appearance";
        let stream = StreamObject::new(
            99,
            1,
            Dictionary::new(BTreeMap::from([(
                Vec::from(b"Label"),
                pdf_object_reader::pdf_string::PdfString::from(
                    encrypt_for_object(
                        &decryptor,
                        object_number,
                        generation_number,
                        b"annotation appearance",
                    ),
                    pdf_object_reader::string_kind::StringKind::Literal,
                ),
            )])),
            encrypt_for_object(&decryptor, object_number, generation_number, contents),
        );

        let decrypted = decryptor
            .decrypt_object(
                ObjectId {
                    number: object_number,
                    generation: generation_number,
                },
                ObjectVariant::Stream(stream),
            )
            .expect("stream decrypts");
        let ObjectVariant::Stream(stream) = decrypted else {
            panic!("decrypted object should remain a stream");
        };
        assert_eq!(stream.object_number, object_number);
        assert_eq!(stream.generation_number, generation_number);
        assert_eq!(stream.raw_data(), contents);
        assert_eq!(
            stream.dictionary.get(b"Label"),
            Some(&pdf_object_reader::pdf_string::PdfString::from(
                b"annotation appearance".to_vec(),
                pdf_object_reader::string_kind::StringKind::Literal
            ))
        );
    }

    #[test]
    fn decrypt_object_preserves_unencrypted_signature_contents() {
        let decryptor = make_decryptor(true);
        let identifier = ObjectId {
            number: 9,
            generation: 0,
        };
        let object = ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Type"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Sig".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Contents"),
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0, 0, 0, 0],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
        ])));

        let decrypted = decryptor
            .decrypt_object(identifier, object)
            .expect("signature decrypts");
        let ObjectVariant::Dictionary(dictionary) = decrypted else {
            panic!("decrypted object should remain a dictionary");
        };
        assert_eq!(
            dictionary.get(b"Contents"),
            Some(&pdf_object_reader::pdf_string::PdfString::from(
                vec![0, 0, 0, 0],
                pdf_object_reader::string_kind::StringKind::Hexadecimal
            ))
        );
    }
}
