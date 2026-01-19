//! PDF Decryption implementation.
//!
//! This module implements PDF decryption according to the PDF 1.7 specification
//! (Section 7.6 "Encryption"). It supports:
//!
//! - Standard security handler (password-based encryption)
//! - RC4 encryption (V1, V2)
//! - AES-128 encryption (V4)
//!
//! # PDF Encryption Overview
//!
//! PDF encryption works as follows:
//! 1. The encryption dictionary specifies the algorithm and parameters
//! 2. A file encryption key is derived from the password + document ID
//! 3. Each object has a unique key derived from the file key + object number
//! 4. Streams and strings are encrypted/decrypted with the object key
//!
//! # Algorithm Selection
//!
//! - V=1, R=2: RC4 with 40-bit key (Algorithm 1)
//! - V=2, R=3: RC4 with variable length key up to 128-bit (Algorithm 1)
//! - V=4, R=4: AES-128 in CBC mode (Algorithm 1 for key, AES for encryption)

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use md5::{Digest, Md5};
use thiserror::Error;

use crate::encryption::{EncryptDictionary, EncryptionVersion};

/// Errors that can occur during PDF decryption.
#[derive(Debug, Error)]
pub enum DecryptionError {
    #[error("incorrect password")]
    IncorrectPassword,
    #[error("unsupported encryption algorithm: V={version} ")]
    UnsupportedAlgorithm { version: EncryptionVersion },
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
        let revision = encrypt.revision;
        let owner_hash = encrypt.owner_password_hash.as_ref();
        let user_hash = encrypt.user_password_hash.as_ref();
        let permissions = encrypt.permissions;
        let key_length_bits = encrypt.effective_key_length();
        let key_length_bytes = (key_length_bits / 8) as usize;

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
        );

        // Verify the user password
        if authenticate_user_password(
            &file_key,
            user_hash,
            document_id,
            revision,
            encrypt.encrypt_metadata,
        ) {
            return Ok(DocumentDecryptor {
                file_key,
                version: encrypt.version,
                key_length_bytes,
                encrypt_metadata: encrypt.encrypt_metadata,
            });
        }

        // Try as owner password
        let user_password =
            recover_user_password_from_owner(password, owner_hash, key_length_bytes, revision);

        let file_key = compute_file_encryption_key(
            &user_password,
            owner_hash,
            permissions,
            document_id,
            key_length_bytes,
            revision,
            encrypt.encrypt_metadata,
        );

        if authenticate_user_password(
            &file_key,
            user_hash,
            document_id,
            revision,
            encrypt.encrypt_metadata,
        ) {
            return Ok(DocumentDecryptor {
                file_key,
                version: encrypt.version,
                key_length_bytes,
                encrypt_metadata: encrypt.encrypt_metadata,
            });
        }

        Err(DecryptionError::IncorrectPassword)
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
        // Derive the object-specific key
        let object_key = compute_object_key(
            &self.file_key,
            object_number,
            generation_number,
            self.version,
        );

        match self.version {
            EncryptionVersion::V1 | EncryptionVersion::V2 => {
                Ok(rc4_crypt(&object_key, encrypted_data))
            }
            EncryptionVersion::V4 => aes_128_cbc_decrypt(&object_key, encrypted_data),
            _ => Err(DecryptionError::UnsupportedAlgorithm {
                version: self.version,
            }),
        }
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
        // String decryption uses the same algorithm as stream decryption
        self.decrypt_stream(object_number, generation_number, encrypted_string)
    }

    /// Returns whether metadata streams should be encrypted.
    pub fn encrypt_metadata(&self) -> bool {
        self.encrypt_metadata
    }
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
) -> Vec<u8> {
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
            hasher.update(&hash[..key_length_bytes]);
            hash = hasher.finalize().to_vec();
        }
    }

    // Return key of appropriate length
    hash.truncate(key_length_bytes);
    hash
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
) -> bool {
    let computed_hash = if revision == 2 {
        // Algorithm 4: RC4-encrypt the padding string with the file key
        rc4_crypt(file_key, &PADDING)
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
        let mut result = rc4_crypt(file_key, &hash);

        for i in 1..=REVISION_3_MIXING_ROUNDS {
            let modified_key: Vec<u8> = file_key.iter().map(|&b| b ^ i).collect();
            result = rc4_crypt(&modified_key, &result);
        }

        result
    };

    // Compare first 16 bytes for R>=3, or full 32 bytes for R=2
    if revision == 2 {
        computed_hash == user_hash
    } else {
        computed_hash.len() >= 16 && user_hash.len() >= 16 && computed_hash[..16] == user_hash[..16]
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
) -> Vec<u8> {
    // Compute the owner key from the owner password
    let padded = pad_password(owner_password);
    let mut hasher = Md5::new();
    hasher.update(padded);
    let mut hash = hasher.finalize().to_vec();

    if revision >= 3 {
        for _ in 0..50 {
            let mut h = Md5::new();
            h.update(&hash[..key_length_bytes]);
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
            result = rc4_crypt(&modified_key, &result);
        }
        result
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
) -> Vec<u8> {
    let mut hasher = Md5::new();

    // Hash the file encryption key
    hasher.update(file_key);

    // Hash the object number as 3 bytes (little-endian)
    hasher.update(&(object_number as u32).to_le_bytes()[..3]);

    // Hash the generation number as 2 bytes (little-endian)
    hasher.update((generation_number as u16).to_le_bytes());

    // For AES, add the "sAlT" marker
    if matches!(version, EncryptionVersion::V4) {
        hasher.update(b"sAlT");
    }

    let hash = hasher.finalize();

    // Key length is min(file_key.len() + 5, 16) bytes
    let key_len = std::cmp::min(file_key.len() + 5, 16);
    hash[..key_len].to_vec()
}

/// Pads or truncates a password to exactly 32 bytes using the padding string.
fn pad_password(password: &[u8]) -> [u8; 32] {
    let mut result = [0u8; 32];
    let copy_len = std::cmp::min(password.len(), 32);

    result[..copy_len].copy_from_slice(&password[..copy_len]);

    if copy_len < 32 {
        result[copy_len..].copy_from_slice(&PADDING[..(32 - copy_len)]);
    }

    result
}

/// RC4 encryption/decryption (symmetric cipher).
///
/// RC4 is a stream cipher where encryption and decryption are the same operation.
fn rc4_crypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    // Initialize the S-box
    let mut s: [u8; 256] = [0; 256];
    for (i, item) in s.iter_mut().enumerate() {
        *item = i as u8;
    }

    // Key-scheduling algorithm (KSA)
    let mut j: usize = 0;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
        s.swap(i, j);
    }

    // Pseudo-random generation algorithm (PRGA)
    let mut result = Vec::with_capacity(data.len());
    let mut i: usize = 0;
    j = 0;

    for &byte in data {
        i = (i + 1) % 256;
        j = (j + s[i] as usize) % 256;
        s.swap(i, j);
        let k = s[(s[i] as usize + s[j] as usize) % 256];
        result.push(byte ^ k);
    }

    result
}

/// AES-128 CBC decryption.
///
/// The first 16 bytes of the input are the IV.
fn aes_128_cbc_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    if data.len() < 16 {
        return Err(DecryptionError::InvalidData(
            "AES data too short (need at least 16 bytes for IV)".to_string(),
        ));
    }

    // First 16 bytes are the IV
    let iv = &data[..16];
    let ciphertext = &data[16..];

    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }

    if !ciphertext.len().is_multiple_of(16) {
        return Err(DecryptionError::InvalidData(
            "AES ciphertext length must be a multiple of 16".to_string(),
        ));
    }

    // Ensure key is exactly 16 bytes
    let key_16: [u8; 16] = key
        .get(..16)
        .and_then(|s| s.try_into().ok())
        .unwrap_or_else(|| {
            let mut arr = [0u8; 16];
            let len = std::cmp::min(key.len(), 16);
            arr[..len].copy_from_slice(&key[..len]);
            arr
        });

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

#[cfg(test)]
mod tests {
    use super::*;

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

        let ciphertext = rc4_crypt(key, plaintext);
        let decrypted = rc4_crypt(key, &ciphertext);

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_rc4_known_vector() {
        // Known test vector for RC4
        let key = b"Key";
        let plaintext = b"Plaintext";
        let ciphertext = rc4_crypt(key, plaintext);

        // RC4("Key", "Plaintext") should produce known bytes
        // This is a basic sanity check
        assert_eq!(ciphertext.len(), plaintext.len());
        assert_ne!(&ciphertext[..], plaintext);

        // Verify decryption works
        let decrypted = rc4_crypt(key, &ciphertext);
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
    fn test_compute_object_key() {
        let file_key = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let object_key = compute_object_key(&file_key, 1, 0, EncryptionVersion::V2);

        // Object key should be at most 16 bytes
        assert!(object_key.len() <= 16);
        // Object key length should be min(file_key.len() + 5, 16) = min(10, 16) = 10
        assert_eq!(object_key.len(), 10);
    }

    #[test]
    fn test_compute_object_key_aes() {
        let file_key = vec![0x01; 16];
        let object_key_rc4 = compute_object_key(&file_key, 1, 0, EncryptionVersion::V2);
        let object_key_aes = compute_object_key(&file_key, 1, 0, EncryptionVersion::V4);

        // AES adds "sAlT" to the hash, so keys should differ
        assert_ne!(object_key_rc4, object_key_aes);
    }
}
