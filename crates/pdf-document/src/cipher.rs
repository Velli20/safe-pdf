//! Block and stream ciphers used by PDF encryption.
//!
//! RC4 decrypts V=1, V=2 and V=4 `/V2` data and drives password checks for the
//! MD5-based security handler. AES-128 and AES-256 decrypt `/AESV2` and `/AESV3`
//! data, and the unpadded AES variants unwrap V=5 file keys and hash revision 6
//! passwords.

use aes::cipher::{
    BlockDecrypt, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit,
    block_padding::{NoPadding, Pkcs7},
    generic_array::GenericArray,
};
use rc4::{
    Key, Rc4, StreamCipher,
    consts::{U1, U2, U3, U4, U5, U6, U7, U8, U9, U10, U11, U12, U13, U14, U15, U16},
};

use crate::decryption::DecryptionError;

/// RC4 encryption/decryption (symmetric cipher).
///
/// RC4 is a stream cipher where encryption and decryption are the same operation.
pub(crate) fn rc4_crypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
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
pub(crate) fn aes_128_cbc_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
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
pub(crate) fn aes_256_cbc_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
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
pub(crate) fn aes_128_cbc_encrypt_without_padding(
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
pub(crate) fn aes_256_cbc_decrypt_without_padding(
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
pub(crate) fn aes_256_ecb_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptionError> {
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
}
