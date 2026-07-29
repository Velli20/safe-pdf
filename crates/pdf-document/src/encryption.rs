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

use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::PdfReaderError;

/// Standard security handler filter names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncryptionFilter {
    /// Standard security handler (password-based encryption).
    Standard,
    /// Other or unsupported filter.
    Other(String),
}

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

impl From<&str> for EncryptionFilter {
    fn from(s: &str) -> Self {
        match s {
            "Standard" => EncryptionFilter::Standard,
            other => EncryptionFilter::Other(other.to_string()),
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

impl TryFrom<i32> for EncryptionVersion {
    type Error = PdfReaderError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(EncryptionVersion::V1),
            2 => Ok(EncryptionVersion::V2),
            3 => Ok(EncryptionVersion::V3),
            4 => Ok(EncryptionVersion::V4),
            5 => Ok(EncryptionVersion::V5),
            _ => Err(PdfReaderError::UnsupportedEncryptionVersion { version: value }),
        }
    }
}

/// Represents the encryption dictionary from the PDF trailer.
///
/// The encryption dictionary specifies the security handler and encryption
/// parameters needed to decrypt the document.
///
/// # Required Entries
///
/// - `Filter`: The name of the security handler (e.g., `/Standard`).
/// - `V`: The algorithm version number.
///
/// # Standard Security Handler Entries (when Filter = Standard)
///
/// - `R`: The revision of the standard security handler.
/// - `O`: A 32-byte string used to verify the owner password.
/// - `U`: A 32-byte string used to verify the user password.
/// - `P`: Permission flags (a 32-bit integer).
/// - `Length`: (Optional) The length of the encryption key in bits.
#[derive(Debug, Clone)]
pub(crate) struct EncryptDictionary {
    /// The security handler filter (e.g., Standard).
    pub filter: EncryptionFilter,
    /// The encryption algorithm version (V entry).
    pub version: EncryptionVersion,
    /// The revision of the standard security handler (R entry).
    /// Only applicable when filter is Standard.
    pub revision: i32,
    /// Owner password verification string (O entry).
    /// A 32-byte string for Standard handler.
    pub owner_password_hash: Vec<u8>,
    /// User password verification string (U entry).
    /// A 32-byte string for Standard handler.
    pub user_password_hash: Vec<u8>,
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
    pub owner_encrypted_key: Option<Vec<u8>>,
    /// Encrypted file key for the user password (UE entry).
    pub user_encrypted_key: Option<Vec<u8>>,
    /// Encrypted permissions block (Perms entry).
    pub encrypted_permissions: Option<Vec<u8>>,
    /// Default method used to encrypt streams.
    pub stream_method: CryptFilterMethod,
    /// Default method used to encrypt strings.
    pub string_method: CryptFilterMethod,
}

impl EncryptDictionary {
    /// Default value for EncryptMetadata if not specified.
    const ENCRYPT_METADATA_DEFAULT: bool = true;
}

impl EncryptDictionary {
    /// Parses an encryption dictionary from a PDF Dictionary object.
    ///
    /// # Arguments
    ///
    /// - `dict`: The encryption dictionary to parse.
    /// - `objects`: The object collection for resolving indirect references.
    ///
    /// # Returns
    ///
    /// An `EncryptDictionary` on success, or a `PdfReaderError` if parsing fails.
    pub fn from_dictionary(
        dict: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfReaderError> {
        let filter = dict
            .required_str("Filter", objects)
            .map(EncryptionFilter::from)?;

        let version_num = dict.required_number::<i32>("V", objects)?;
        let version = EncryptionVersion::try_from(version_num)?;

        let revision = dict.required_number::<i32>("R", objects)?;
        let owner_password_hash = dict.required_bytes("O", objects)?.to_vec();
        let user_password_hash = dict.required_bytes("U", objects)?.to_vec();
        let permissions = dict.required_number::<i32>("P", objects)?;
        let key_length = dict.optional_number::<i32>("Length", objects)?;
        let encrypt_metadata = dict
            .optional_boolean("EncryptMetadata", objects)?
            .unwrap_or(Self::ENCRYPT_METADATA_DEFAULT);
        let owner_encrypted_key = dict.optional_bytes_vec("OE", objects)?;
        let user_encrypted_key = dict.optional_bytes_vec("UE", objects)?;
        let encrypted_permissions = dict.optional_bytes_vec("Perms", objects)?;
        let (stream_method, string_method) = match version {
            EncryptionVersion::V1 | EncryptionVersion::V2 => {
                (CryptFilterMethod::Rc4, CryptFilterMethod::Rc4)
            }
            EncryptionVersion::V4 => (CryptFilterMethod::Aes128, CryptFilterMethod::Aes128),
            EncryptionVersion::V5 => (
                parse_v5_crypt_filter(dict, "StmF", objects)?,
                parse_v5_crypt_filter(dict, "StrF", objects)?,
            ),
            EncryptionVersion::V3 => (CryptFilterMethod::Identity, CryptFilterMethod::Identity),
        };

        Ok(EncryptDictionary {
            filter,
            version,
            revision,
            owner_password_hash,
            user_password_hash,
            permissions,
            key_length,
            encrypt_metadata,
            owner_encrypted_key,
            user_encrypted_key,
            encrypted_permissions,
            stream_method,
            string_method,
        })
    }

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
}

/// Resolves one of the document-default V=5 crypt filters.
fn parse_v5_crypt_filter(
    dictionary: &Dictionary,
    entry: &str,
    objects: &dyn ObjectResolver,
) -> Result<CryptFilterMethod, PdfReaderError> {
    let filter_name = dictionary
        .optional_str(entry, objects)?
        .unwrap_or("Identity");
    if filter_name == "Identity" {
        return Ok(CryptFilterMethod::Identity);
    }

    let crypt_filters = dictionary.required_dictionary("CF", objects)?;
    let crypt_filter = crypt_filters.required_dictionary(filter_name, objects)?;
    match crypt_filter.required_str("CFM", objects)? {
        "AESV3" => {
            let key_length = crypt_filter.optional_number::<i32>("Length", objects)?;
            if key_length.is_some_and(|length| length != 32) {
                return Err(PdfReaderError::DecryptionSetup(
                    "AESV3 crypt filter /Length must be 32 bytes".to_string(),
                ));
            }
            Ok(CryptFilterMethod::Aes256)
        }
        method => Err(PdfReaderError::DecryptionSetup(format!(
            "unsupported V=5 crypt filter method: {method}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };
    use std::collections::BTreeMap;

    fn make_dictionary(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            map.insert(key.to_string(), value);
        }
        Dictionary::new(map)
    }

    #[test]
    fn test_encryption_version_conversion() {
        assert_eq!(
            EncryptionVersion::try_from(1).unwrap(),
            EncryptionVersion::V1
        );
        assert_eq!(
            EncryptionVersion::try_from(2).unwrap(),
            EncryptionVersion::V2
        );
        assert_eq!(
            EncryptionVersion::try_from(3).unwrap(),
            EncryptionVersion::V3
        );
        assert_eq!(
            EncryptionVersion::try_from(4).unwrap(),
            EncryptionVersion::V4
        );
        assert_eq!(
            EncryptionVersion::try_from(5).unwrap(),
            EncryptionVersion::V5
        );
        assert!(EncryptionVersion::try_from(6).is_err());
        assert!(EncryptionVersion::try_from(0).is_err());
    }

    #[test]
    fn test_encryption_filter_conversion() {
        assert_eq!(
            EncryptionFilter::from("Standard"),
            EncryptionFilter::Standard
        );
        assert_eq!(
            EncryptionFilter::from("Custom"),
            EncryptionFilter::Other("Custom".to_string())
        );
    }

    #[test]
    fn test_parse_full_encrypt_dictionary() {
        let dict = make_dictionary(vec![
            ("Filter", ObjectVariant::Name(b"Standard".to_vec())),
            ("V", ObjectVariant::Integer(4)),
            ("R", ObjectVariant::Integer(4)),
            ("O", ObjectVariant::HexString(vec![0u8; 32])),
            ("U", ObjectVariant::HexString(vec![0u8; 32])),
            ("P", ObjectVariant::Integer(-1)),
            ("Length", ObjectVariant::Integer(128)),
            ("EncryptMetadata", ObjectVariant::Boolean(false)),
        ]);

        let result = EncryptDictionary::from_dictionary(&dict, &PassthroughResolver);

        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
        let encrypt = result.unwrap();

        assert_eq!(encrypt.filter, EncryptionFilter::Standard);
        assert_eq!(encrypt.version, EncryptionVersion::V4);
        assert_eq!(encrypt.revision, 4);
        assert_eq!(encrypt.owner_password_hash, vec![0u8; 32]);
        assert_eq!(encrypt.user_password_hash, vec![0u8; 32]);
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
            ("CFM", ObjectVariant::Name(b"AESV3".to_vec())),
            ("Length", ObjectVariant::Integer(32)),
        ]);
        let crypt_filters =
            make_dictionary(vec![("StdCF", ObjectVariant::Dictionary(Box::new(std_cf)))]);
        let dict = make_dictionary(vec![
            ("Filter", ObjectVariant::Name(b"Standard".to_vec())),
            ("V", ObjectVariant::Integer(5)),
            ("R", ObjectVariant::Integer(6)),
            ("O", ObjectVariant::HexString(vec![0u8; 48])),
            ("U", ObjectVariant::HexString(vec![0u8; 48])),
            ("OE", ObjectVariant::HexString(vec![0u8; 32])),
            ("UE", ObjectVariant::HexString(vec![0u8; 32])),
            ("Perms", ObjectVariant::HexString(vec![0u8; 16])),
            ("P", ObjectVariant::Integer(-4)),
            ("CF", ObjectVariant::Dictionary(Box::new(crypt_filters))),
            ("StmF", ObjectVariant::Name(b"StdCF".to_vec())),
            ("StrF", ObjectVariant::Name(b"Identity".to_vec())),
        ]);

        let encrypt = EncryptDictionary::from_dictionary(&dict, &PassthroughResolver).unwrap();

        assert_eq!(encrypt.version, EncryptionVersion::V5);
        assert_eq!(encrypt.stream_method, CryptFilterMethod::Aes256);
        assert_eq!(encrypt.string_method, CryptFilterMethod::Identity);
        assert_eq!(encrypt.owner_encrypted_key, Some(vec![0u8; 32]));
        assert_eq!(encrypt.user_encrypted_key, Some(vec![0u8; 32]));
        assert_eq!(encrypt.encrypted_permissions, Some(vec![0u8; 16]));
    }
}
