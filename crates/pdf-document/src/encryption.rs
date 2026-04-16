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

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::error::PdfReaderError;

/// Standard security handler filter names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncryptionFilter {
    /// Standard security handler (password-based encryption).
    Standard,
    /// Other or unsupported filter.
    Other(String),
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
        write!(f, "{}", version_num)
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
    #[allow(dead_code)]
    filter: EncryptionFilter,
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
            .get_or_err("Filter")?
            .try_str(objects)
            .map(|obj| EncryptionFilter::from(obj.as_ref()))?;

        let version_obj = dict.get_or_err("V")?;

        let version_num = version_obj.try_number::<i32>(objects)?;
        let version = EncryptionVersion::try_from(version_num)?;

        let revision = dict.get_or_err("R")?.try_number::<i32>(objects)?;

        let owner_password_hash = dict.get_or_err("O")?.try_bytes(objects)?.to_vec();

        let user_password_hash = dict.get_or_err("U")?.try_bytes(objects)?.to_vec();

        let permissions = dict.get_or_err("P")?.try_number::<i32>(objects)?;

        let key_length = dict
            .get("Length")
            .map(|l| l.try_number::<i32>(objects))
            .transpose()?;

        let encrypt_metadata = dict
            .get("EncryptMetadata")
            .map(|em| em.try_boolean(objects))
            .transpose()?
            .unwrap_or(Self::ENCRYPT_METADATA_DEFAULT);

        Ok(EncryptDictionary {
            filter,
            version,
            revision,
            owner_password_hash,
            user_password_hash,
            permissions,
            key_length,
            encrypt_metadata,
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
    }
}
