//! File key retrieval for the SHA-2-based standard security handler (V=5, revisions 5 and 6).
//!
//! Implements ISO 32000-2 Algorithms 2.A and 2.B: the password is validated against the
//! hashes in `/U` or `/O`, and the AES-256 file key is unwrapped from `/UE` or `/OE`.

use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::{
    cipher::{
        aes_128_cbc_encrypt_without_padding, aes_256_cbc_decrypt_without_padding,
        aes_256_ecb_decrypt,
    },
    decryption::DecryptionError,
    encryption::{CryptFilterMethod, EncryptDictionary},
};

/// Password authentication and file key retrieval for the V=5 standard security handler.
pub(crate) struct Sha2KeyDerivation<'a> {
    /// The encryption dictionary from the PDF trailer.
    encrypt: &'a EncryptDictionary,
    /// The owner-wrapped file key (`/OE`).
    owner_encrypted_key: &'a [u8],
    /// The user-wrapped file key (`/UE`).
    user_encrypted_key: &'a [u8],
    /// The encrypted permissions block (`/Perms`).
    encrypted_permissions: &'a [u8],
}

impl<'a> Sha2KeyDerivation<'a> {
    /// Length of the AES-256 file key in bytes.
    pub(crate) const FILE_KEY_BYTES: usize = 32;

    /// Validates the V=5 encryption dictionary entries required for key retrieval.
    pub(crate) fn new(encrypt: &'a EncryptDictionary) -> Result<Self, DecryptionError> {
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
        validate_v5_entry(&encrypt.owner_password_hash, "O", 48)?;
        validate_v5_entry(&encrypt.user_password_hash, "U", 48)?;

        Ok(Self {
            encrypt,
            owner_encrypted_key: required_v5_entry(&encrypt.owner_encrypted_key, "OE", 32)?,
            user_encrypted_key: required_v5_entry(&encrypt.user_encrypted_key, "UE", 32)?,
            encrypted_permissions: required_v5_entry(&encrypt.encrypted_permissions, "Perms", 16)?,
        })
    }

    /// Returns the file key for `password`, accepted as either the user or the owner password.
    pub(crate) fn authenticate(&self, password: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        let password = prepare_v5_password(password)?;
        let file_key = self
            .unwrap_file_key(&password)?
            .ok_or(DecryptionError::IncorrectPassword)?;
        self.validate_permissions(&file_key)?;
        Ok(file_key)
    }

    /// Unwraps the file key with whichever of the user or owner entries `password` opens.
    fn unwrap_file_key(&self, password: &[u8]) -> Result<Option<Vec<u8>>, DecryptionError> {
        let revision = self.encrypt.revision;
        let user_hash = self.encrypt.user_password_hash.as_slice();
        let owner_hash = self.encrypt.owner_password_hash.as_slice();

        let (password_entry, owner_input, encrypted_key) =
            if validate_v5_password(password, revision, user_hash, None)? {
                (user_hash, None, self.user_encrypted_key)
            } else if validate_v5_password(password, revision, owner_hash, Some(user_hash))? {
                (owner_hash, Some(user_hash), self.owner_encrypted_key)
            } else {
                return Ok(None);
            };

        let key = derive_v5_password_key(password, revision, password_entry, owner_input)?;
        aes_256_cbc_decrypt_without_padding(&key, &[0; 16], encrypted_key).map(Some)
    }

    /// Validates the encrypted `/Perms` block against `/P` and `/EncryptMetadata`.
    fn validate_permissions(&self, file_key: &[u8]) -> Result<(), DecryptionError> {
        let decrypted = aes_256_ecb_decrypt(file_key, self.encrypted_permissions)?;
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
        let metadata_marker = if self.encrypt.encrypt_metadata {
            b'T'
        } else {
            b'F'
        };
        let valid = i32::from_le_bytes(permissions_bytes) == self.encrypt.permissions
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
}
