//! PDF Encryption support.
//!
//! This module provides types and utilities for handling PDF encryption.
//! The encryption dictionary (specified by the `/Encrypt` entry in the trailer)
//! defines how the document is encrypted and how a reader must decrypt objects
//! and streams.
//!
//! According to PDF 1.7 Specification (Section 7.6 "Encryption"):
//! - The `/Encrypt` entry in the trailer dictionary specifies the encryption dictionary.
//! - The encryption dictionary contains parameters needed to decrypt the document.
//! - Before reading other objects, the encryption dictionary must be resolved first.

use std::sync::Arc;

use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
    dictionary::Dictionary,
};

use crate::decryption::DecryptionError;
use crate::error::PdfReaderError;

/// Encryption method selected by a document-default crypt filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CryptFilterMethod {
    /// The data is not encrypted.
    Identity,
    /// RC4 encryption used by V=1 and V=2.
    Rc4,
    /// AES-128 encryption used by V=4.
    Aes128,
    /// AES-256 encryption used by V=5.
    Aes256,
}

/// Reads a crypt filter dictionary, an entry of `/CF`, as the method its `/CFM` entry selects.
impl FromPdfObject for CryptFilterMethod {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        match dictionary.required::<Arc<[u8]>>(b"CFM")?.as_ref() {
            b"None" => Ok(CryptFilterMethod::Identity),
            b"V2" => Ok(CryptFilterMethod::Rc4),
            b"AESV2" => Ok(CryptFilterMethod::Aes128),
            b"AESV3" => {
                let key_length = dictionary.optional::<i32>(b"Length")?;
                if key_length.is_some_and(|length| length != 32) {
                    return Err(PdfReaderError::DecryptionSetup(
                        "AESV3 crypt filter /Length must be 32 bytes".to_string(),
                    )
                    .into());
                }
                Ok(CryptFilterMethod::Aes256)
            }
            method => Err(PdfReaderError::DecryptionSetup(format!(
                "unsupported crypt filter method: {}",
                String::from_utf8_lossy(method)
            ))
            .into()),
        }
    }
}

/// Encryption algorithm version (V entry).
///
/// According to PDF 1.7 Specification:
/// - V = 1: RC4 or AES encryption with a 40-bit key.
/// - V = 2: RC4 or AES encryption with a key longer than 40 bits.
/// - V = 3: Unpublished algorithm (not used).
/// - V = 4: AES-128 or RC4 with additional crypt filters.
/// - V = 5: AES-256 encryption (PDF 2.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncryptionVersion {
    /// V=1: 40-bit RC4 encryption.
    V1,
    /// V=2: RC4 with key > 40 bits (up to 128 bits).
    V2,
    /// V=3: Unpublished algorithm.
    V3,
    /// V=4: AES-128 or RC4 with crypt filters.
    V4,
    /// V=5: AES-256 encryption (PDF 2.0).
    V5,
}

impl std::fmt::Display for EncryptionVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let version_num = match self {
            EncryptionVersion::V1 => 1,
            EncryptionVersion::V2 => 2,
            EncryptionVersion::V3 => 3,
            EncryptionVersion::V4 => 4,
            EncryptionVersion::V5 => 5,
        };
        write!(f, "{version_num}")
    }
}

/// Reads the encryption dictionary's `/V` entry.
impl FromPdfObject for EncryptionVersion {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.decode::<i32>()? {
            1 => Ok(EncryptionVersion::V1),
            2 => Ok(EncryptionVersion::V2),
            3 => Ok(EncryptionVersion::V3),
            4 => Ok(EncryptionVersion::V4),
            5 => Ok(EncryptionVersion::V5),
            version => Err(PdfReaderError::UnsupportedEncryptionVersion { version }.into()),
        }
    }
}

impl EncryptionVersion {
    /// Returns whether `/StmF` and `/StrF` select this version's default crypt filters.
    fn names_crypt_filters(self) -> bool {
        matches!(self, EncryptionVersion::V4 | EncryptionVersion::V5)
    }

    /// Returns the default crypt filter when none is named.
    ///
    /// A missing V=4 entry keeps the historical AES-128 default; a missing V=5 entry is
    /// `/Identity`.
    fn unnamed_crypt_filter(self) -> CryptFilterMethod {
        match self {
            EncryptionVersion::V1 | EncryptionVersion::V2 => CryptFilterMethod::Rc4,
            EncryptionVersion::V3 | EncryptionVersion::V5 => CryptFilterMethod::Identity,
            EncryptionVersion::V4 => CryptFilterMethod::Aes128,
        }
    }

    /// Accepts a named crypt filter's method when this version supports it.
    fn accept_crypt_filter(self, method: CryptFilterMethod) -> ReadResult<CryptFilterMethod> {
        match (self, method) {
            (EncryptionVersion::V4, CryptFilterMethod::Aes256) => {
                Err(PdfReaderError::DecryptionSetup(
                    "AESV3 crypt filters require V=5 encryption".to_string(),
                )
                .into())
            }
            (EncryptionVersion::V5, method @ CryptFilterMethod::Aes256)
            | (EncryptionVersion::V4, method) => Ok(method),
            (_, method) => Err(PdfReaderError::DecryptionSetup(format!(
                "unsupported V=5 crypt filter method: {method:?}"
            ))
            .into()),
        }
    }
}

/// The document-default crypt filters of an encryption dictionary: the methods its
/// `/StmF` and `/StrF` entries select for streams and strings.
struct DefaultCryptFilters {
    stream: CryptFilterMethod,
    string: CryptFilterMethod,
}

/// Resolves each named filter through `/CF`, so RC4 (`/V2`) documents decrypt correctly.
impl FromPdfObject for DefaultCryptFilters {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        let version: EncryptionVersion = dictionary.required(b"V")?;
        let mut select = |entry: &[u8]| -> ReadResult<CryptFilterMethod> {
            if !version.names_crypt_filters() {
                return Ok(version.unnamed_crypt_filter());
            }
            match dictionary.optional::<Arc<[u8]>>(entry)?.as_deref() {
                None => Ok(version.unnamed_crypt_filter()),
                Some(b"Identity") => Ok(CryptFilterMethod::Identity),
                Some(name) => {
                    let crypt_filters: Dictionary = dictionary.required(b"CF")?;
                    let method = DictionaryContext::new(&crypt_filters, dictionary.access())
                        .required(name)?;
                    version.accept_crypt_filter(method)
                }
            }
        };
        Ok(Self {
            stream: select(b"StmF")?,
            string: select(b"StrF")?,
        })
    }
}

/// Represents the encryption dictionary from the PDF trailer.
///
/// The encryption dictionary specifies the security handler and encryption
/// parameters needed to decrypt the document.
///
/// Only the Standard (password-based) security handler is supported, so `Filter` must be
/// `/Standard`.
///
/// # Required Entries
///
/// - `Filter`: The name of the security handler.
/// - `V`: The algorithm version number.
///
/// # Standard Security Handler Entries
///
/// - `R`: The revision of the standard security handler.
/// - `O`: A 32-byte string used to verify the owner password.
/// - `U`: A 32-byte string used to verify the user password.
/// - `P`: Permission flags (a 32-bit integer).
/// - `Length`: (Optional) The length of the encryption key in bits.
#[derive(Debug, Clone)]
pub(crate) struct EncryptDictionary {
    /// The encryption algorithm version (V entry).
    pub version: EncryptionVersion,
    /// The revision of the standard security handler (R entry).
    pub revision: i32,
    /// Owner password verification string (O entry).
    /// A 32-byte string for Standard handler.
    pub owner_password_hash: Arc<[u8]>,
    /// User password verification string (U entry).
    /// A 32-byte string for Standard handler.
    pub user_password_hash: Arc<[u8]>,
    /// A set of flags specifying which operations are permitted
    /// when the document is opened with the user password.
    pub permissions: i32,
    /// The length of the encryption key in bits (Length entry).
    /// Default is 40 for V=1, may be 40-128 for V=2, V=3.
    /// For V=4, this is determined by the crypt filter.
    pub key_length: Option<i32>,
    /// Encryption metadata flag (EncryptMetadata entry).
    /// If true (default), document metadata is encrypted.
    pub encrypt_metadata: bool,
    /// Encrypted file key for the owner password (OE entry).
    pub owner_encrypted_key: Option<Arc<[u8]>>,
    /// Encrypted file key for the user password (UE entry).
    pub user_encrypted_key: Option<Arc<[u8]>>,
    /// Encrypted permissions block (Perms entry).
    pub encrypted_permissions: Option<Arc<[u8]>>,
    /// Default method used to encrypt streams.
    pub stream_method: CryptFilterMethod,
    /// Default method used to encrypt strings.
    pub string_method: CryptFilterMethod,
}

impl EncryptDictionary {
    /// Default value for EncryptMetadata if not specified.
    const ENCRYPT_METADATA_DEFAULT: bool = true;
}

impl FromPdfObject for EncryptDictionary {
    /// Parses an encryption dictionary.
    ///
    /// Fails if parsing fails or the security handler is not `/Standard`.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        // Other handlers (such as the public-key `/Adobe.PubSec`) lack the Standard entries
        // below and need keys this reader cannot obtain.
        let filter: Arc<[u8]> = dictionary.required(b"Filter")?;
        if filter.as_ref() != b"Standard" {
            return Err(PdfReaderError::from_decryption_setup(
                DecryptionError::UnsupportedSecurityHandler(
                    String::from_utf8_lossy(&filter).into_owned(),
                ),
            )
            .into());
        }

        let version: EncryptionVersion = dictionary.required(b"V")?;
        let crypt_filters: DefaultCryptFilters = dictionary.read(object)?;
        Ok(EncryptDictionary {
            version,
            revision: dictionary.required(b"R")?,
            owner_password_hash: dictionary.required(b"O")?,
            user_password_hash: dictionary.required(b"U")?,
            permissions: dictionary.required(b"P")?,
            key_length: dictionary.optional(b"Length")?,
            encrypt_metadata: dictionary
                .optional(b"EncryptMetadata")?
                .unwrap_or(Self::ENCRYPT_METADATA_DEFAULT),
            owner_encrypted_key: dictionary.optional(b"OE")?,
            user_encrypted_key: dictionary.optional(b"UE")?,
            encrypted_permissions: dictionary.optional(b"Perms")?,
            stream_method: crypt_filters.stream,
            string_method: crypt_filters.string,
        })
    }
}

impl EncryptDictionary {
    /// Returns the effective key length in bits.
    ///
    /// - For V=1, always returns 40.
    /// - For V=2 and V=3, returns the Length entry or 40 if not specified.
    /// - For V=4, the key length is determined by crypt filters.
    /// - For V=5, always returns 256 (AES-256).
    pub(crate) fn effective_key_length(&self) -> i32 {
        match self.version {
            EncryptionVersion::V1 => 40,
            EncryptionVersion::V2 | EncryptionVersion::V3 => self.key_length.unwrap_or(40),
            EncryptionVersion::V4 => self.key_length.unwrap_or(128),
            EncryptionVersion::V5 => 256,
        }
    }

    /// Returns whether both document-default crypt filters are `/Identity`.
    ///
    /// Such documents store every stream and string unencrypted, so reading them needs no
    /// password even when other crypt filters (for example `/EFF` for embedded files) do.
    pub(crate) fn leaves_streams_and_strings_unencrypted(&self) -> bool {
        self.stream_method == CryptFilterMethod::Identity
            && self.string_method == CryptFilterMethod::Identity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object_reader::{EmptySource, dictionary::Dictionary, object_variant::ObjectVariant};
    use std::collections::BTreeMap;

    fn make_dictionary(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            map.insert(Vec::from(key.as_bytes()), value);
        }
        Dictionary::from_iter(map)
    }

    #[test]
    fn test_encryption_version_conversion() {
        let read = |version| {
            pdf_object_reader::ObjectReader::new(EmptySource)
                .read::<EncryptionVersion>(&ObjectVariant::Integer(version))
        };
        assert_eq!(read(1).unwrap(), EncryptionVersion::V1);
        assert_eq!(read(2).unwrap(), EncryptionVersion::V2);
        assert_eq!(read(3).unwrap(), EncryptionVersion::V3);
        assert_eq!(read(4).unwrap(), EncryptionVersion::V4);
        assert_eq!(read(5).unwrap(), EncryptionVersion::V5);
        assert!(read(6).is_err());
        assert!(read(0).is_err());
    }

    #[test]
    fn test_parse_full_encrypt_dictionary() {
        let dict = make_dictionary(vec![
            (
                "Filter",
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Standard",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            ("V", ObjectVariant::Integer(4)),
            ("R", ObjectVariant::Integer(4)),
            (
                "O",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 32],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            (
                "U",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 32],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            ("P", ObjectVariant::Integer(-1)),
            ("Length", ObjectVariant::Integer(128)),
            ("EncryptMetadata", ObjectVariant::Boolean(false)),
        ]);

        let result = pdf_object_reader::ObjectReader::new(EmptySource)
            .read::<EncryptDictionary>(&ObjectVariant::Dictionary(dict));

        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
        let encrypt = result.unwrap();

        assert_eq!(encrypt.version, EncryptionVersion::V4);
        assert_eq!(encrypt.revision, 4);
        assert_eq!(*encrypt.owner_password_hash, [0u8; 32]);
        assert_eq!(*encrypt.user_password_hash, [0u8; 32]);
        assert_eq!(encrypt.permissions, -1);
        assert_eq!(encrypt.key_length, Some(128));
        assert!(!encrypt.encrypt_metadata);
        assert_eq!(encrypt.stream_method, CryptFilterMethod::Aes128);
        assert_eq!(encrypt.string_method, CryptFilterMethod::Aes128);
        assert_eq!(encrypt.owner_encrypted_key, None);
        assert_eq!(encrypt.user_encrypted_key, None);
        assert_eq!(encrypt.encrypted_permissions, None);
    }

    #[test]
    fn test_parse_v5_aes_256_crypt_filters() {
        let std_cf = make_dictionary(vec![
            (
                "CFM",
                pdf_object_reader::pdf_string::PdfString::from(
                    b"AESV3",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            ("Length", ObjectVariant::Integer(32)),
        ]);
        let crypt_filters = make_dictionary(vec![("StdCF", ObjectVariant::Dictionary(std_cf))]);
        let dict = make_dictionary(vec![
            (
                "Filter",
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Standard",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            ("V", ObjectVariant::Integer(5)),
            ("R", ObjectVariant::Integer(6)),
            (
                "O",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 48],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            (
                "U",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 48],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            (
                "OE",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 32],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            (
                "UE",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 32],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            (
                "Perms",
                pdf_object_reader::pdf_string::PdfString::from(
                    vec![0u8; 16],
                    pdf_object_reader::string_kind::StringKind::Hexadecimal,
                ),
            ),
            ("P", ObjectVariant::Integer(-4)),
            ("CF", ObjectVariant::Dictionary(crypt_filters)),
            (
                "StmF",
                pdf_object_reader::pdf_string::PdfString::from(
                    b"StdCF",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                "StrF",
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Identity",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
        ]);

        let encrypt = pdf_object_reader::ObjectReader::new(EmptySource)
            .read::<EncryptDictionary>(&ObjectVariant::Dictionary(dict))
            .unwrap();

        assert_eq!(encrypt.version, EncryptionVersion::V5);
        assert_eq!(encrypt.stream_method, CryptFilterMethod::Aes256);
        assert_eq!(encrypt.string_method, CryptFilterMethod::Identity);
        assert_eq!(encrypt.owner_encrypted_key.as_deref(), Some(&[0u8; 32][..]));
        assert_eq!(encrypt.user_encrypted_key.as_deref(), Some(&[0u8; 32][..]));
        assert_eq!(
            encrypt.encrypted_permissions.as_deref(),
            Some(&[0u8; 16][..])
        );
    }
}
