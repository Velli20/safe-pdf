//! File key derivation for the MD5-based standard security handler (V=1, V=2 and V=4).
//!
//! Implements Algorithms 1 through 5 of the PDF 1.7 specification: the file key is an
//! MD5 digest of the padded password, `/O`, `/P` and the document ID, and each object
//! is encrypted with a key derived from the file key and its object identifier.

use md5::{Digest, Md5};

use crate::{
    cipher::rc4_crypt,
    decryption::DecryptionError,
    encryption::{CryptFilterMethod, EncryptDictionary, EncryptionVersion},
};

/// The padding string used in PDF encryption key derivation.
/// This is a fixed 32-byte sequence defined in the PDF specification.
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// The number of additional hashing rounds used in Revision 3 encryption algorithm 5.
const REVISION_3_MIXING_ROUNDS: u8 = 19;

/// Password authentication and file key derivation for the V=1, V=2 and V=4 standard
/// security handler.
pub(crate) struct Md5KeyDerivation<'a> {
    /// The encryption dictionary from the PDF trailer.
    encrypt: &'a EncryptDictionary,
    /// The first element of the trailer's `/ID` array.
    document_id: &'a [u8],
    /// The declared file key length in bytes.
    pub(crate) key_length_bytes: usize,
}

impl<'a> Md5KeyDerivation<'a> {
    /// Minimum file key length, in bytes, used to derive V=4 object keys.
    const V4_MIN_OBJECT_KEY_BASE_BYTES: usize = 16;

    /// Number of extra MD5 rounds applied by revision 3 and later.
    const REVISION_3_HASH_ROUNDS: usize = 50;

    /// Validates the declared key length and prepares key derivation for a document.
    pub(crate) fn new(
        encrypt: &'a EncryptDictionary,
        document_id: &'a [u8],
    ) -> Result<Self, DecryptionError> {
        Ok(Self {
            encrypt,
            document_id,
            key_length_bytes: key_length_in_bytes(encrypt.effective_key_length())?,
        })
    }

    /// Returns the file key for `password`, accepted as either the user or the owner password.
    pub(crate) fn authenticate(&self, password: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        let user_file_key = self.file_key(password)?;
        if self.is_valid_file_key(&user_file_key)? {
            return Ok(user_file_key);
        }

        let owner_file_key = self.file_key(&self.recover_user_password(password)?)?;
        if self.is_valid_file_key(&owner_file_key)? {
            return Ok(owner_file_key);
        }

        Err(DecryptionError::IncorrectPassword)
    }

    /// Converts an authenticated file key into the key that per-object keys are derived from.
    ///
    /// V=4 producers derive object keys from the file key zero-padded to 16 bytes
    /// when the declared key length is shorter, while `/U` is computed from the
    /// unpadded key.
    pub(crate) fn object_key_base(&self, mut file_key: Vec<u8>) -> Vec<u8> {
        if self.encrypt.version == EncryptionVersion::V4 {
            let padded_length = file_key.len().max(Self::V4_MIN_OBJECT_KEY_BASE_BYTES);
            file_key.resize(padded_length, 0);
        }
        file_key
    }

    /// Derives the file key for a user password (Algorithm 2).
    fn file_key(&self, user_password: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        let mut hasher = Md5::new();
        hasher.update(pad_password(user_password));
        hasher.update(&self.encrypt.owner_password_hash);
        hasher.update(self.encrypt.permissions.to_le_bytes());
        hasher.update(self.document_id);
        if self.encrypt.revision >= 4 && !self.encrypt.encrypt_metadata {
            hasher.update([0xFF; 4]);
        }
        self.stretch_key(hasher.finalize().to_vec())
    }

    /// Returns whether `file_key` reproduces the dictionary's `/U` value.
    ///
    /// Implements Algorithm 4 for revision 2 and Algorithm 5 for revisions 3 and 4.
    fn is_valid_file_key(&self, file_key: &[u8]) -> Result<bool, DecryptionError> {
        let user_hash = self.encrypt.user_password_hash.as_slice();
        if self.encrypt.revision == 2 {
            return Ok(rc4_crypt(file_key, &PADDING)? == user_hash);
        }

        let mut hasher = Md5::new();
        hasher.update(PADDING);
        hasher.update(self.document_id);
        let mut computed_hash = rc4_crypt(file_key, &hasher.finalize())?;
        for round in 1..=REVISION_3_MIXING_ROUNDS {
            computed_hash = rc4_crypt(&xor_key(file_key, round), &computed_hash)?;
        }

        let computed_prefix = computed_hash.get(..16);
        Ok(computed_prefix.is_some() && computed_prefix == user_hash.get(..16))
    }

    /// Recovers the user password encrypted in `/O` by the owner password (Algorithm 3
    /// in reverse).
    fn recover_user_password(&self, owner_password: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        let owner_key = self.stretch_key(Md5::digest(pad_password(owner_password)).to_vec())?;
        let owner_hash = self.encrypt.owner_password_hash.as_slice();
        if self.encrypt.revision == 2 {
            return rc4_crypt(&owner_key, owner_hash);
        }

        (0..=REVISION_3_MIXING_ROUNDS)
            .rev()
            .try_fold(owner_hash.to_vec(), |data, round| {
                rc4_crypt(&xor_key(&owner_key, round), &data)
            })
    }

    /// Applies the revision 3 MD5 rounds to a digest and truncates it to the key length.
    fn stretch_key(&self, mut hash: Vec<u8>) -> Result<Vec<u8>, DecryptionError> {
        if self.encrypt.revision >= 3 {
            for _ in 0..Self::REVISION_3_HASH_ROUNDS {
                hash = Md5::digest(key_prefix(&hash, self.key_length_bytes)?).to_vec();
            }
        }
        hash.truncate(self.key_length_bytes);
        Ok(hash)
    }
}

/// Returns `key` with every byte XORed with `round`, as used by the RC4 mixing rounds.
fn xor_key(key: &[u8], round: u8) -> Vec<u8> {
    key.iter().map(|byte| byte ^ round).collect()
}

/// Computes the object-specific encryption key.
///
/// This implements Algorithm 1 (Encryption of data using the RC4 or AES algorithms).
pub(crate) fn compute_object_key(
    file_key: &[u8],
    object_number: usize,
    generation_number: usize,
    method: CryptFilterMethod,
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
    if method == CryptFilterMethod::Aes128 {
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
    fn test_compute_object_key() {
        let file_key = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let object_key = compute_object_key(&file_key, 1, 0, CryptFilterMethod::Rc4)
            .expect("test object key is valid");

        // Object key should be at most 16 bytes
        assert!(object_key.len() <= 16);
        // Object key length should be min(file_key.len() + 5, 16) = min(10, 16) = 10
        assert_eq!(object_key.len(), 10);
    }

    #[test]
    fn test_compute_object_key_aes() {
        let file_key = vec![0x01; 16];
        let object_key_rc4 = compute_object_key(&file_key, 1, 0, CryptFilterMethod::Rc4)
            .expect("test object key is valid");
        let object_key_aes = compute_object_key(&file_key, 1, 0, CryptFilterMethod::Aes128)
            .expect("test object key is valid");

        // AES adds "sAlT" to the hash, so keys should differ
        assert_ne!(object_key_rc4, object_key_aes);
    }
}
