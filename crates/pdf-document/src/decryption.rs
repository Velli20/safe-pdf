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

use aes::cipher::{
    BlockDecrypt, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit,
    block_padding::{NoPadding, Pkcs7},
    generic_array::GenericArray,
};
use md5::{Digest, Md5};
use rc4::{
    Key, Rc4, StreamCipher,
    consts::{U1, U2, U3, U4, U5, U6, U7, U8, U9, U10, U11, U12, U13, U14, U15, U16},
};
use sha2::{Sha256, Sha384, Sha512};
use thiserror::Error;

use crate::encryption::{
    CryptFilterMethod, EncryptDictionary, EncryptionFilter, EncryptionVersion,
};
use pdf_object::{
    dictionary::Dictionary, object_id::PdfObjectId, object_variant::ObjectVariant,
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

/// The padding string used in PDF encryption key derivation.
/// This is a fixed 32-byte sequence defined in the PDF specification.
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// The number of additional hashing rounds used in Revision 3 encryption algorithm 5.
const REVISION_3_MIXING_ROUNDS: u8 = 19;

/// A decryptor for PDF documents.
///
/// This struct holds the encryption key and provides methods to decrypt
/// individual objects within the PDF document.
#[derive(Debug, Clone)]
pub struct DocumentDecryptor {
    /// The file encryption key derived from the password.
    file_key: Vec<u8>,
    /// The encryption version (determines algorithm).
    version: EncryptionVersion,
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
        if encrypt.version == EncryptionVersion::V5 {
            return Self::new_v5(encrypt, password);
        }

        let revision = encrypt.revision;
        let owner_hash = encrypt.owner_password_hash.as_ref();
        let user_hash = encrypt.user_password_hash.as_ref();
        let permissions = encrypt.permissions;
        let key_length_bytes = key_length_in_bytes(encrypt.effective_key_length())?;

        // Validate supported algorithms
        match (encrypt.version, revision) {
            (EncryptionVersion::V1, _) => {}
            (EncryptionVersion::V2, _) => {}
            (EncryptionVersion::V4, _) => {}
            (v, _) => {
                return Err(DecryptionError::UnsupportedAlgorithm { version: v });
            }
        }

        // Try authenticating with user password first
        let file_key = compute_file_encryption_key(
            password,
            owner_hash,
            permissions,
            document_id,
            key_length_bytes,
            revision,
            encrypt.encrypt_metadata,
        )?;

        // Verify the user password
        if authenticate_user_password(
            &file_key,
            user_hash,
            document_id,
            revision,
            encrypt.encrypt_metadata,
        )? {
            return Ok(DocumentDecryptor {
                file_key,
                version: encrypt.version,
                key_length_bytes,
                encrypt_metadata: encrypt.encrypt_metadata,
                stream_method: encrypt.stream_method,
                string_method: encrypt.string_method,
            });
        }

        // Try as owner password
        let user_password =
            recover_user_password_from_owner(password, owner_hash, key_length_bytes, revision)?;

        let file_key = compute_file_encryption_key(
            &user_password,
            owner_hash,
            permissions,
            document_id,
            key_length_bytes,
            revision,
            encrypt.encrypt_metadata,
        )?;

        if authenticate_user_password(
            &file_key,
            user_hash,
            document_id,
            revision,
            encrypt.encrypt_metadata,
        )? {
            return Ok(DocumentDecryptor {
                file_key,
                version: encrypt.version,
                key_length_bytes,
                encrypt_metadata: encrypt.encrypt_metadata,
                stream_method: encrypt.stream_method,
                string_method: encrypt.string_method,
            });
        }

        Err(DecryptionError::IncorrectPassword)
    }

    /// Creates a V=5 decryptor by retrieving the AES-256 file key.
    fn new_v5(encrypt: &EncryptDictionary, password: &[u8]) -> Result<Self, DecryptionError> {
        if !matches!(encrypt.revision, 5 | 6) {
            return Err(DecryptionError::InvalidData(format!(
                "V=5 requires security handler revision 5 or 6, found {}",
                encrypt.revision
            )));
        }
        if encrypt.key_length.is_some_and(|length| length != 256) {
            return Err(DecryptionError::InvalidData(
                "V=5 encryption dictionary /Length must be 256 bits".to_string(),
            ));
        }
        if !matches!(
            (encrypt.stream_method, encrypt.string_method),
            (
                CryptFilterMethod::Aes256 | CryptFilterMethod::Identity,
                CryptFilterMethod::Aes256 | CryptFilterMethod::Identity
            )
        ) {
            return Err(DecryptionError::InvalidData(
                "V=5 requires AESV3 or Identity crypt filters".to_string(),
            ));
        }

        let owner_encrypted_key = required_v5_entry(&encrypt.owner_encrypted_key, "OE", 32)?;
        let user_encrypted_key = required_v5_entry(&encrypt.user_encrypted_key, "UE", 32)?;
        let encrypted_permissions = required_v5_entry(&encrypt.encrypted_permissions, "Perms", 16)?;
        validate_v5_entry(&encrypt.owner_password_hash, "O", 48)?;
        validate_v5_entry(&encrypt.user_password_hash, "U", 48)?;

        let password = prepare_v5_password(password)?;
        let file_key = authenticate_v5_password(
            &password,
            encrypt.revision,
            &encrypt.owner_password_hash,
            &encrypt.user_password_hash,
            owner_encrypted_key,
            user_encrypted_key,
        )?
        .ok_or(DecryptionError::IncorrectPassword)?;
        validate_permissions(
            &file_key,
            encrypted_permissions,
            encrypt.permissions,
            encrypt.encrypt_metadata,
        )?;

        Ok(Self {
            file_key,
            version: encrypt.version,
            key_length_bytes: 32,
            encrypt_metadata: encrypt.encrypt_metadata,
            stream_method: encrypt.stream_method,
            string_method: encrypt.string_method,
        })
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
            CryptFilterMethod::Rc4 | CryptFilterMethod::Aes128 => {
                let object_key = compute_object_key(
                    &self.file_key,
                    object_number,
                    generation_number,
                    self.version,
                )?;
                match method {
                    CryptFilterMethod::Rc4 => rc4_crypt(&object_key, encrypted_data),
                    CryptFilterMethod::Aes128 => aes_128_cbc_decrypt(&object_key, encrypted_data),
                    CryptFilterMethod::Identity | CryptFilterMethod::Aes256 => {
                        Err(DecryptionError::InvalidData(
                            "invalid legacy crypt filter selection".to_string(),
                        ))
                    }
                }
            }
        }
    }

    /// Decrypts every encrypted string and stream contained in an indirect PDF object.
    ///
    /// PDF encryption derives one key per indirect object, so nested strings use the object
    /// number and generation of the indirect object that contains them.
    pub(crate) fn decrypt_object(
        &self,
        identifier: PdfObjectId,
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
            ObjectVariant::LiteralString(bytes) => Ok(ObjectVariant::LiteralString(
                self.decrypt_string(object_number, generation_number, &bytes)?,
            )),
            ObjectVariant::HexString(bytes) => Ok(ObjectVariant::HexString(self.decrypt_string(
                object_number,
                generation_number,
                &bytes,
            )?)),
            ObjectVariant::Array(values) => values
                .into_iter()
                .map(|value| self.decrypt_object_value(value, object_number, generation_number))
                .collect::<Result<Vec<_>, _>>()
                .map(ObjectVariant::Array),
            ObjectVariant::Dictionary(dictionary) => Ok(ObjectVariant::Dictionary(Box::new(
                self.decrypt_dictionary(*dictionary, object_number, generation_number)?,
            ))),
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
            self.decrypt_dictionary(*stream.dictionary, object_number, generation_number)?;
        Ok(ObjectVariant::Stream(StreamObject::new_encoded(
            object_number,
            generation_number,
            Box::new(dictionary),
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
            Some(ObjectVariant::Name(value)) if value.as_slice() == b"Sig"
        )
    })
}

/// Returns whether PDF encryption rules exempt this stream from decryption.
fn should_skip_stream_decryption(dictionary: &Dictionary, encrypt_metadata: bool) -> bool {
    match dictionary.get(b"Type") {
        Some(ObjectVariant::Name(name)) if name.as_slice() == b"XRef" => true,
        Some(ObjectVariant::Name(name)) if name.as_slice() == b"Metadata" => !encrypt_metadata,
        _ => false,
    }
}

/// Returns a required V=5 encryption dictionary entry with its expected length.
fn required_v5_entry<'a>(
    entry: &'a Option<Vec<u8>>,
    name: &str,
    expected_length: usize,
) -> Result<&'a [u8], DecryptionError> {
    let value = entry.as_deref().ok_or_else(|| {
        DecryptionError::InvalidData(format!("V=5 encryption dictionary is missing /{name}"))
    })?;
    validate_v5_entry(value, name, expected_length)?;
    Ok(value)
}

/// Validates the exact length of a V=5 encryption dictionary byte string.
fn validate_v5_entry(
    value: &[u8],
    name: &str,
    expected_length: usize,
) -> Result<(), DecryptionError> {
    if value.len() != expected_length {
        return Err(DecryptionError::InvalidData(format!(
            "V=5 /{name} entry must contain {expected_length} bytes"
        )));
    }
    Ok(())
}

/// Applies the Unicode password preparation required by revisions 5 and 6.
fn prepare_v5_password(password: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    let password = std::str::from_utf8(password).map_err(|error| {
        DecryptionError::InvalidData(format!("V=5 password is not valid UTF-8: {error}"))
    })?;
    let prepared = stringprep::saslprep(password).map_err(|error| {
        DecryptionError::InvalidData(format!("V=5 password failed SASLprep: {error}"))
    })?;
    Ok(prepared.as_bytes().iter().copied().take(127).collect())
}

/// Authenticates a V=5 password and retrieves the 256-bit file encryption key.
fn authenticate_v5_password(
    password: &[u8],
    revision: i32,
    owner_hash: &[u8],
    user_hash: &[u8],
    owner_encrypted_key: &[u8],
    user_encrypted_key: &[u8],
) -> Result<Option<Vec<u8>>, DecryptionError> {
    if validate_v5_password(password, revision, user_hash, None)? {
        let key = derive_v5_password_key(password, revision, user_hash, None)?;
        return aes_256_cbc_decrypt_without_padding(&key, &[0; 16], user_encrypted_key).map(Some);
    }

    if validate_v5_password(password, revision, owner_hash, Some(user_hash))? {
        let key = derive_v5_password_key(password, revision, owner_hash, Some(user_hash))?;
        return aes_256_cbc_decrypt_without_padding(&key, &[0; 16], owner_encrypted_key).map(Some);
    }

    Ok(None)
}

/// Checks a password against the validation hash stored in an O or U entry.
fn validate_v5_password(
    password: &[u8],
    revision: i32,
    password_entry: &[u8],
    user_hash: Option<&[u8]>,
) -> Result<bool, DecryptionError> {
    let expected_hash = password_entry.get(..32).ok_or_else(|| {
        DecryptionError::InvalidData("V=5 password entry is too short".to_string())
    })?;
    let validation_salt = password_entry.get(32..40).ok_or_else(|| {
        DecryptionError::InvalidData("V=5 validation salt is missing".to_string())
    })?;
    let computed_hash = compute_v5_hash(password, validation_salt, user_hash, revision)?;
    Ok(computed_hash.get(..32) == Some(expected_hash))
}

/// Derives the key used to decrypt an OE or UE entry.
fn derive_v5_password_key(
    password: &[u8],
    revision: i32,
    password_entry: &[u8],
    user_hash: Option<&[u8]>,
) -> Result<Vec<u8>, DecryptionError> {
    let key_salt = password_entry
        .get(40..48)
        .ok_or_else(|| DecryptionError::InvalidData("V=5 key salt is missing".to_string()))?;
    compute_v5_hash(password, key_salt, user_hash, revision)
}

/// Computes the revision-specific V=5 password hash.
fn compute_v5_hash(
    password: &[u8],
    salt: &[u8],
    user_hash: Option<&[u8]>,
    revision: i32,
) -> Result<Vec<u8>, DecryptionError> {
    if revision == 5 {
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        if let Some(user_hash) = user_hash {
            hasher.update(user_hash);
        }
        return Ok(hasher.finalize().to_vec());
    }
    revision_6_hash(password, salt, user_hash)
}

/// Implements ISO 32000-2 Algorithm 2.B for revision 6 passwords.
fn revision_6_hash(
    password: &[u8],
    salt: &[u8],
    user_hash: Option<&[u8]>,
) -> Result<Vec<u8>, DecryptionError> {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    if let Some(user_hash) = user_hash {
        hasher.update(user_hash);
    }
    let mut hash = hasher.finalize().to_vec();
    let mut round = 0usize;

    loop {
        let user_hash_length = user_hash.map_or(0, <[u8]>::len);
        let sequence_length = password
            .len()
            .checked_add(hash.len())
            .and_then(|length| length.checked_add(user_hash_length))
            .ok_or_else(|| {
                DecryptionError::InvalidData(
                    "revision 6 password hash input is too large".to_string(),
                )
            })?;
        let buffer_length = sequence_length.checked_mul(64).ok_or_else(|| {
            DecryptionError::InvalidData("revision 6 password hash buffer is too large".to_string())
        })?;
        let mut buffer = Vec::with_capacity(buffer_length);
        for _ in 0..64 {
            buffer.extend_from_slice(password);
            buffer.extend_from_slice(&hash);
            if let Some(user_hash) = user_hash {
                buffer.extend_from_slice(user_hash);
            }
        }

        let key = hash.get(..16).ok_or_else(|| {
            DecryptionError::InvalidData("revision 6 hash did not contain an AES key".to_string())
        })?;
        let iv = hash.get(16..32).ok_or_else(|| {
            DecryptionError::InvalidData("revision 6 hash did not contain an AES IV".to_string())
        })?;
        let encrypted = aes_128_cbc_encrypt_without_padding(key, iv, &buffer)?;
        let selector = encrypted
            .get(..16)
            .ok_or_else(|| {
                DecryptionError::InvalidData(
                    "revision 6 encrypted hash input is too short".to_string(),
                )
            })?
            .iter()
            .copied()
            .map(usize::from)
            .fold(0usize, usize::saturating_add)
            % 3;
        hash = match selector {
            0 => Sha256::digest(&encrypted).to_vec(),
            1 => Sha384::digest(&encrypted).to_vec(),
            _ => Sha512::digest(&encrypted).to_vec(),
        };

        round = round.saturating_add(1);
        let last_byte = encrypted.last().copied().ok_or_else(|| {
            DecryptionError::InvalidData("revision 6 encrypted hash input is empty".to_string())
        })?;
        if round >= 64 && usize::from(last_byte) <= round.saturating_sub(32) {
            return hash.get(..32).map(<[u8]>::to_vec).ok_or_else(|| {
                DecryptionError::InvalidData("revision 6 hash result is too short".to_string())
            });
        }
    }
}

/// Validates the encrypted V=5 permissions block.
fn validate_permissions(
    file_key: &[u8],
    encrypted_permissions: &[u8],
    permissions: i32,
    encrypt_metadata: bool,
) -> Result<(), DecryptionError> {
    let decrypted = aes_256_ecb_decrypt(file_key, encrypted_permissions)?;
    let permissions_bytes: [u8; 4] = decrypted
        .get(..4)
        .ok_or_else(|| {
            DecryptionError::InvalidData("decrypted permissions block is too short".to_string())
        })?
        .try_into()
        .map_err(|_| {
            DecryptionError::InvalidData(
                "decrypted permissions value must contain four bytes".to_string(),
            )
        })?;
    let metadata_marker = if encrypt_metadata { b'T' } else { b'F' };
    let valid = i32::from_le_bytes(permissions_bytes) == permissions
        && decrypted.get(4..8) == Some(&[0xff; 4])
        && decrypted.get(8) == Some(&metadata_marker)
        && decrypted.get(9..12) == Some(b"adb");
    if !valid {
        return Err(DecryptionError::InvalidData(
            "V=5 permissions validation failed".to_string(),
        ));
    }
    Ok(())
}

/// Computes the file encryption key from the user password.
///
/// This implements Algorithm 2 (Computing an encryption key) from the PDF spec.
fn compute_file_encryption_key(
    password: &[u8],
    owner_hash: &[u8],
    permissions: i32,
    document_id: &[u8],
    key_length_bytes: usize,
    revision: i32,
    encrypt_metadata: bool,
) -> Result<Vec<u8>, DecryptionError> {
    // Step 1: Pad or truncate the password to 32 bytes
    let padded_password = pad_password(password);

    // Step 2: Initialize MD5 hash
    let mut hasher = Md5::new();

    // Step 3: Hash the padded password
    hasher.update(padded_password);

    // Step 4: Hash the O value
    hasher.update(owner_hash);

    // Step 5: Hash the P value (permissions) as a 4-byte little-endian integer
    hasher.update(permissions.to_le_bytes());

    // Step 6: Hash the document ID (first element)
    hasher.update(document_id);

    // Step 7: For revision 4+, if metadata is not encrypted, add 4 bytes of 0xFF
    if revision >= 4 && !encrypt_metadata {
        hasher.update([0xFF, 0xFF, 0xFF, 0xFF]);
    }

    let mut hash = hasher.finalize().to_vec();

    // Step 8: For revision 3+, do 50 additional rounds of MD5
    if revision >= 3 {
        for _ in 0..50 {
            let mut hasher = Md5::new();
            hasher.update(key_prefix(&hash, key_length_bytes)?);
            hash = hasher.finalize().to_vec();
        }
    }

    // Return key of appropriate length
    hash.truncate(key_length_bytes);
    Ok(hash)
}

/// Authenticates the user password by comparing against the U value.
///
/// This implements Algorithm 4 (Authenticating the user password) for R=2
/// and Algorithm 5 for R=3,4.
fn authenticate_user_password(
    file_key: &[u8],
    user_hash: &[u8],
    document_id: &[u8],
    revision: i32,
    _encrypt_metadata: bool,
) -> Result<bool, DecryptionError> {
    let computed_hash = if revision == 2 {
        // Algorithm 4: RC4-encrypt the padding string with the file key
        rc4_crypt(file_key, &PADDING)?
    } else {
        // Algorithm 5: MD5 hash of padding + document ID, then RC4 with key variations
        let mut hasher = Md5::new();
        hasher.update(PADDING);
        hasher.update(document_id);
        let hash = hasher.finalize();

        // RC4 encrypt with key, then 19 more rounds with modified keys.
        // This loop corresponds to step 4 of Algorithm 5 in the PDF 1.7 specification.
        // It performs 19 additional encryption rounds (for a total of 20) using
        // keys derived by XORing the original key with the loop index (1..19).
        // This "key stretching" was intended to make brute-force attacks more computationally expensive.
        let mut result = rc4_crypt(file_key, &hash)?;

        for i in 1..=REVISION_3_MIXING_ROUNDS {
            let modified_key: Vec<u8> = file_key.iter().map(|&b| b ^ i).collect();
            result = rc4_crypt(&modified_key, &result)?;
        }

        result
    };

    // Compare first 16 bytes for R>=3, or full 32 bytes for R=2
    if revision == 2 {
        Ok(computed_hash == user_hash)
    } else {
        Ok(computed_hash.get(..16) == user_hash.get(..16) && computed_hash.get(..16).is_some())
    }
}

/// Recovers the user password from the owner password.
///
/// This implements Algorithm 3 (Computing the O value) in reverse.
fn recover_user_password_from_owner(
    owner_password: &[u8],
    owner_hash: &[u8],
    key_length_bytes: usize,
    revision: i32,
) -> Result<Vec<u8>, DecryptionError> {
    // Compute the owner key from the owner password
    let padded = pad_password(owner_password);
    let mut hasher = Md5::new();
    hasher.update(padded);
    let mut hash = hasher.finalize().to_vec();

    if revision >= 3 {
        for _ in 0..50 {
            let mut h = Md5::new();
            h.update(key_prefix(&hash, key_length_bytes)?);
            hash = h.finalize().to_vec();
        }
    }

    hash.truncate(key_length_bytes);

    // Decrypt the O value to get the user password
    if revision == 2 {
        rc4_crypt(&hash, owner_hash)
    } else {
        let mut result = owner_hash.to_vec();
        for i in (0..=REVISION_3_MIXING_ROUNDS).rev() {
            let modified_key: Vec<u8> = hash.iter().map(|&b| b ^ i).collect();
            result = rc4_crypt(&modified_key, &result)?;
        }
        Ok(result)
    }
}

/// Computes the object-specific encryption key.
///
/// This implements Algorithm 1 (Encryption of data using the RC4 or AES algorithms).
fn compute_object_key(
    file_key: &[u8],
    object_number: usize,
    generation_number: usize,
    version: EncryptionVersion,
) -> Result<Vec<u8>, DecryptionError> {
    let mut hasher = Md5::new();

    // Hash the file encryption key
    hasher.update(file_key);

    // Hash the object number as 3 bytes (little-endian)
    let object_number = u32::try_from(object_number).map_err(|_| {
        DecryptionError::InvalidData("object number exceeds the PDF encryption range".to_string())
    })?;
    hasher.update(object_number.to_le_bytes().get(..3).ok_or_else(|| {
        DecryptionError::InvalidData("object number could not be encoded".to_string())
    })?);

    // Hash the generation number as 2 bytes (little-endian)
    let generation_number = u16::try_from(generation_number).map_err(|_| {
        DecryptionError::InvalidData(
            "generation number exceeds the PDF encryption range".to_string(),
        )
    })?;
    hasher.update(generation_number.to_le_bytes());

    // For AES, add the "sAlT" marker
    if matches!(version, EncryptionVersion::V4) {
        hasher.update(b"sAlT");
    }

    let hash = hasher.finalize();

    // Key length is min(file_key.len() + 5, 16) bytes
    let key_length = file_key.len().saturating_add(5).min(16);
    Ok(key_prefix(hash.as_slice(), key_length)?.to_vec())
}

/// Validates a PDF encryption key length and converts it to bytes.
fn key_length_in_bytes(key_length_bits: i32) -> Result<usize, DecryptionError> {
    if key_length_bits <= 0 || key_length_bits.rem_euclid(8) != 0 {
        return Err(DecryptionError::InvalidData(
            "encryption key length must be a positive multiple of eight".to_string(),
        ));
    }
    usize::try_from(key_length_bits / 8)
        .map_err(|_| DecryptionError::InvalidData("encryption key length is too large".to_string()))
}

/// Returns a validated prefix of key material.
fn key_prefix(bytes: &[u8], length: usize) -> Result<&[u8], DecryptionError> {
    bytes.get(..length).ok_or_else(|| {
        DecryptionError::InvalidData("encryption key material is shorter than declared".to_string())
    })
}

/// Pads or truncates a password to exactly 32 bytes using the padding string.
fn pad_password(password: &[u8]) -> [u8; 32] {
    let mut result = [0u8; 32];
    for (destination, source) in result.iter_mut().zip(password.iter().chain(PADDING.iter())) {
        *destination = *source;
    }
    result
}

/// RC4 encryption/decryption (symmetric cipher).
///
/// RC4 is a stream cipher where encryption and decryption are the same operation.
fn rc4_crypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    let mut result = Vec::with_capacity(data.len());
    result.extend_from_slice(data);
    match key.len() {
        1 => apply_rc4::<U1>(key, &mut result)?,
        2 => apply_rc4::<U2>(key, &mut result)?,
        3 => apply_rc4::<U3>(key, &mut result)?,
        4 => apply_rc4::<U4>(key, &mut result)?,
        5 => apply_rc4::<U5>(key, &mut result)?,
        6 => apply_rc4::<U6>(key, &mut result)?,
        7 => apply_rc4::<U7>(key, &mut result)?,
        8 => apply_rc4::<U8>(key, &mut result)?,
        9 => apply_rc4::<U9>(key, &mut result)?,
        10 => apply_rc4::<U10>(key, &mut result)?,
        11 => apply_rc4::<U11>(key, &mut result)?,
        12 => apply_rc4::<U12>(key, &mut result)?,
        13 => apply_rc4::<U13>(key, &mut result)?,
        14 => apply_rc4::<U14>(key, &mut result)?,
        15 => apply_rc4::<U15>(key, &mut result)?,
        16 => apply_rc4::<U16>(key, &mut result)?,
        _ => {
            return Err(DecryptionError::InvalidData(
                "RC4 keys must contain between 1 and 16 bytes".to_string(),
            ));
        }
    }
    Ok(result)
}

/// Applies RustCrypto RC4 using a compile-time key size selected by the PDF key length.
fn apply_rc4<KeySize>(key: &[u8], data: &mut [u8]) -> Result<(), DecryptionError>
where
    KeySize: rc4::cipher::generic_array::ArrayLength<u8>,
{
    let mut rc4_key = Key::<KeySize>::default();
    if rc4_key.len() != key.len() {
        return Err(DecryptionError::InvalidData(
            "RC4 key length did not match its selected size".to_string(),
        ));
    }
    rc4_key
        .iter_mut()
        .zip(key.iter())
        .for_each(|(destination, source)| *destination = *source);
    let mut cipher = Rc4::<KeySize>::new(&rc4_key);
    cipher.apply_keystream(data);
    Ok(())
}

/// AES-128 CBC decryption.
///
/// The first 16 bytes of the input are the IV.
fn aes_128_cbc_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    let Some((iv, ciphertext)) = data.split_at_checked(16) else {
        return Err(DecryptionError::InvalidData(
            "AES data too short (need at least 16 bytes for IV)".to_string(),
        ));
    };

    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }

    if !ciphertext.len().is_multiple_of(16) {
        return Err(DecryptionError::InvalidData(
            "AES ciphertext length must be a multiple of 16".to_string(),
        ));
    }

    // Ensure key is exactly 16 bytes
    let key_16: [u8; 16] = key.try_into().map_err(|_| {
        DecryptionError::InvalidData("AES-128 requires a 16-byte object key".to_string())
    })?;

    let iv_16: [u8; 16] = iv
        .try_into()
        .map_err(|_| DecryptionError::InvalidData("IV must be exactly 16 bytes".to_string()))?;

    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

    let decryptor = Aes128CbcDec::new(&key_16.into(), &iv_16.into());

    let mut buffer = ciphertext.to_vec();

    let decrypted = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|e| DecryptionError::AesDecryptionFailed(e.to_string()))?;

    Ok(decrypted.to_vec())
}

/// AES-256 CBC decryption for PDF strings and streams.
///
/// The first 16 bytes of the input are the IV and the remaining bytes use
/// PKCS#7 padding.
fn aes_256_cbc_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    let Some((iv, ciphertext)) = data.split_at_checked(16) else {
        return Err(DecryptionError::InvalidData(
            "AES data too short (need at least 16 bytes for IV)".to_string(),
        ));
    };
    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }
    if !ciphertext.len().is_multiple_of(16) {
        return Err(DecryptionError::InvalidData(
            "AES ciphertext length must be a multiple of 16".to_string(),
        ));
    }

    let mut buffer = ciphertext.to_vec();
    let decryptor = cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv).map_err(|_| {
        DecryptionError::InvalidData("AES-256 requires a 32-byte key and a 16-byte IV".to_string())
    })?;
    let decrypted = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|error| DecryptionError::AesDecryptionFailed(error.to_string()))?;
    Ok(decrypted.to_vec())
}

/// AES-128 CBC encryption without padding, used by revision 6 hashing.
fn aes_128_cbc_encrypt_without_padding(
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, DecryptionError> {
    let mut buffer = data.to_vec();
    let encryptor = cbc::Encryptor::<aes::Aes128>::new_from_slices(key, iv).map_err(|_| {
        DecryptionError::InvalidData("AES-128 requires a 16-byte key and a 16-byte IV".to_string())
    })?;
    let data_length = buffer.len();
    let encrypted = encryptor
        .encrypt_padded_mut::<NoPadding>(&mut buffer, data_length)
        .map_err(|error| DecryptionError::AesDecryptionFailed(error.to_string()))?;
    Ok(encrypted.to_vec())
}

/// AES-256 CBC decryption without padding, used for OE and UE entries.
fn aes_256_cbc_decrypt_without_padding(
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, DecryptionError> {
    let mut buffer = data.to_vec();
    let decryptor = cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv).map_err(|_| {
        DecryptionError::InvalidData("AES-256 requires a 32-byte key and a 16-byte IV".to_string())
    })?;
    let decrypted = decryptor
        .decrypt_padded_mut::<NoPadding>(&mut buffer)
        .map_err(|error| DecryptionError::AesDecryptionFailed(error.to_string()))?;
    Ok(decrypted.to_vec())
}

/// Decrypts the single AES-256 ECB permissions block.
fn aes_256_ecb_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    let cipher = aes::Aes256::new_from_slice(key)
        .map_err(|_| DecryptionError::InvalidData("AES-256 requires a 32-byte key".to_string()))?;
    let block: [u8; 16] = data.try_into().map_err(|_| {
        DecryptionError::InvalidData("AES-256 permissions data must contain 16 bytes".to_string())
    })?;
    let mut block = GenericArray::from(block);
    cipher.decrypt_block(&mut block);
    Ok(block.to_vec())
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

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).expect("test hex is ASCII");
                u8::from_str_radix(pair, 16).expect("test hex is valid")
            })
            .collect()
    }

    fn make_decryptor(encrypt_metadata: bool) -> DocumentDecryptor {
        DocumentDecryptor {
            file_key: vec![0; 16],
            version: EncryptionVersion::V4,
            key_length_bytes: 16,
            encrypt_metadata,
            stream_method: CryptFilterMethod::Aes128,
            string_method: CryptFilterMethod::Aes128,
        }
    }

    fn make_stream(type_name: Option<&[u8]>, data: Vec<u8>) -> StreamObject {
        let mut entries = BTreeMap::new();
        if let Some(type_name) = type_name {
            entries.insert(Vec::from(b"Type"), ObjectVariant::Name(type_name.to_vec()));
        }

        StreamObject::new(7, 0, Box::new(Dictionary::new(entries)), data)
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
            decryptor.version,
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
    fn test_pad_password_empty() {
        let padded = pad_password(b"");
        assert_eq!(padded, PADDING);
    }

    #[test]
    fn test_pad_password_short() {
        let padded = pad_password(b"test");
        assert_eq!(&padded[..4], b"test");
        assert_eq!(&padded[4..], &PADDING[..28]);
    }

    #[test]
    fn test_pad_password_exact() {
        let password = [b'x'; 32];
        let padded = pad_password(&password);
        assert_eq!(padded, password);
    }

    #[test]
    fn test_pad_password_long() {
        let password = [b'y'; 40];
        let padded = pad_password(&password);
        assert_eq!(padded, [b'y'; 32]);
    }

    #[test]
    fn test_rc4_basic() {
        // RC4 is symmetric, so encrypt then decrypt should give original
        let key = b"secret";
        let plaintext = b"Hello, World!";

        let ciphertext = rc4_crypt(key, plaintext).expect("test key is valid");
        let decrypted = rc4_crypt(key, &ciphertext).expect("test key is valid");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_rc4_known_vector() {
        // Known test vector for RC4
        let key = b"Key";
        let plaintext = b"Plaintext";
        let ciphertext = rc4_crypt(key, plaintext).expect("test key is valid");

        // RC4("Key", "Plaintext") should produce known bytes
        // This is a basic sanity check
        assert_eq!(ciphertext.len(), plaintext.len());
        assert_ne!(&ciphertext[..], plaintext);

        // Verify decryption works
        let decrypted = rc4_crypt(key, &ciphertext).expect("test key is valid");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_128_cbc_roundtrip() {
        use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
        type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

        let key = [0u8; 16];
        let iv = [0u8; 16];
        let plaintext = b"Hello, AES World!";

        // Encrypt
        let encryptor = Aes128CbcEnc::new(&key.into(), &iv.into());
        let mut buffer = vec![0u8; plaintext.len() + 16]; // Extra space for padding
        buffer[..plaintext.len()].copy_from_slice(plaintext);

        let ciphertext_len = encryptor
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext.len())
            .unwrap()
            .len();

        // Prepend IV for our decrypt function
        let mut encrypted_with_iv = iv.to_vec();
        encrypted_with_iv.extend_from_slice(&buffer[..ciphertext_len]);

        // Decrypt
        let decrypted = aes_128_cbc_decrypt(&key, &encrypted_with_iv).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_128_cbc_short_data() {
        let key = [0u8; 16];
        let short_data = [0u8; 8]; // Too short, needs at least 16 for IV

        let result = aes_128_cbc_decrypt(&key, &short_data);
        assert!(result.is_err());
    }

    #[test]
    fn revision_6_hash_matches_real_world_empty_password_vector() {
        let user_hash = decode_hex(
            "3ceaf18c38452ccc258275458c1e863b552e70ee48e00c1cf\
             b959cc264b945a0f546dd2b31571c100cfd45f9050c8af4",
        );
        let salt = user_hash
            .get(32..40)
            .expect("test U entry contains validation salt");

        let hash = revision_6_hash(b"", salt, None).expect("revision 6 hash succeeds");

        assert_eq!(
            hash.as_slice(),
            user_hash.get(..32).expect("test U entry contains hash")
        );
    }

    #[test]
    fn v5_password_preparation_applies_saslprep_and_truncation() {
        let prepared = prepare_v5_password("pass\u{00ad}word\u{00a0}x".as_bytes())
            .expect("test password passes SASLprep");
        let long_password =
            prepare_v5_password(&[b'a'; 200]).expect("ASCII password passes SASLprep");

        assert_eq!(prepared, b"password x");
        assert_eq!(long_password.len(), 127);
    }

    #[test]
    fn test_compute_object_key() {
        let file_key = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let object_key = compute_object_key(&file_key, 1, 0, EncryptionVersion::V2)
            .expect("test object key is valid");

        // Object key should be at most 16 bytes
        assert!(object_key.len() <= 16);
        // Object key length should be min(file_key.len() + 5, 16) = min(10, 16) = 10
        assert_eq!(object_key.len(), 10);
    }

    #[test]
    fn test_compute_object_key_aes() {
        let file_key = vec![0x01; 16];
        let object_key_rc4 = compute_object_key(&file_key, 1, 0, EncryptionVersion::V2)
            .expect("test object key is valid");
        let object_key_aes = compute_object_key(&file_key, 1, 0, EncryptionVersion::V4)
            .expect("test object key is valid");

        // AES adds "sAlT" to the hash, so keys should differ
        assert_ne!(object_key_rc4, object_key_aes);
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
        let identifier = PdfObjectId {
            number: object_number,
            generation: generation_number,
        };
        let object = ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Contents"),
                ObjectVariant::LiteralString(encrypt_for_object(
                    &decryptor,
                    object_number,
                    generation_number,
                    contents,
                )),
            ),
            (
                Vec::from(b"RC"),
                ObjectVariant::Array(vec![ObjectVariant::HexString(encrypt_for_object(
                    &decryptor,
                    object_number,
                    generation_number,
                    rich_contents,
                ))]),
            ),
            (
                Vec::from(b"Subtype"),
                ObjectVariant::Name(b"FreeText".to_vec()),
            ),
            (Vec::from(b"Parent"), ObjectVariant::Reference(4)),
        ]))));

        let decrypted = decryptor
            .decrypt_object(identifier, object)
            .expect("object decrypts");
        let ObjectVariant::Dictionary(dictionary) = decrypted else {
            panic!("decrypted object should remain a dictionary");
        };
        assert_eq!(
            dictionary.get(b"Contents"),
            Some(&ObjectVariant::LiteralString(contents.to_vec()))
        );
        assert_eq!(
            dictionary.get(b"RC"),
            Some(&ObjectVariant::Array(vec![ObjectVariant::HexString(
                rich_contents.to_vec()
            )]))
        );
        assert_eq!(
            dictionary.get(b"Subtype"),
            Some(&ObjectVariant::Name(b"FreeText".to_vec()))
        );
        assert_eq!(
            dictionary.get(b"Parent"),
            Some(&ObjectVariant::Reference(4))
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
            Box::new(Dictionary::new(BTreeMap::from([(
                Vec::from(b"Label"),
                ObjectVariant::LiteralString(encrypt_for_object(
                    &decryptor,
                    object_number,
                    generation_number,
                    b"annotation appearance",
                )),
            )]))),
            encrypt_for_object(&decryptor, object_number, generation_number, contents),
        );

        let decrypted = decryptor
            .decrypt_object(
                PdfObjectId {
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
            Some(&ObjectVariant::LiteralString(
                b"annotation appearance".to_vec()
            ))
        );
    }

    #[test]
    fn decrypt_object_preserves_unencrypted_signature_contents() {
        let decryptor = make_decryptor(true);
        let identifier = PdfObjectId {
            number: 9,
            generation: 0,
        };
        let object = ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
            (Vec::from(b"Type"), ObjectVariant::Name(b"Sig".to_vec())),
            (
                Vec::from(b"Contents"),
                ObjectVariant::HexString(vec![0, 0, 0, 0]),
            ),
        ]))));

        let decrypted = decryptor
            .decrypt_object(identifier, object)
            .expect("signature decrypts");
        let ObjectVariant::Dictionary(dictionary) = decrypted else {
            panic!("decrypted object should remain a dictionary");
        };
        assert_eq!(
            dictionary.get(b"Contents"),
            Some(&ObjectVariant::HexString(vec![0, 0, 0, 0]))
        );
    }
}
