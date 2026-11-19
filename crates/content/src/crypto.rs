//! Depot keys and the symmetric encryption Steam uses for depot chunks and
//! encrypted manifest file names.
//!
//! Format (SteamKit2 `Steam/CDN/DepotChunk.cs`, steamroom `src/depot/chunk.rs`):
//! the first 16 bytes are the IV, encrypted with AES-256-ECB (no padding);
//! the rest is AES-256-CBC with that IV and PKCS#7 padding.

use std::fmt;

use aes::Aes256;
use aes::cipher::block_padding::{NoPadding, Pkcs7};
use aes::cipher::{BlockDecryptMut, KeyInit, KeyIvInit};

const IV_LEN: usize = 16;

/// The AES-256 key of a depot. Required to decrypt its chunks and, for
/// some depots, the file names in its manifests.
///
/// `Debug` does not print the key.
#[derive(Clone, PartialEq, Eq)]
pub struct DepotKey([u8; 32]);

impl DepotKey {
    /// Wraps a 32-byte key. Returns `None` for any other length.
    pub fn from_slice(key: &[u8]) -> Option<Self> {
        <[u8; 32]>::try_from(key).ok().map(Self)
    }

    /// The raw key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for DepotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DepotKey(<redacted>)")
    }
}

/// Decryption failed: data too short, or wrong key / corrupted data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// Less than one IV plus one AES block.
    #[error("encrypted data is too short")]
    TooShort,
    /// The padding is invalid, which means the key is wrong or the data is corrupted.
    #[error("decryption failed (wrong depot key or corrupted data)")]
    BadPadding,
}

/// Decrypts `ECB(IV) || CBC(data)`.
pub(crate) fn decrypt(data: &[u8], key: &DepotKey) -> Result<Vec<u8>, CryptoError> {
    if data.len() < IV_LEN * 2 {
        return Err(CryptoError::TooShort);
    }
    let (encrypted_iv, body) = data.split_at(IV_LEN);
    let iv = ecb::Decryptor::<Aes256>::new(key.0.as_slice().into())
        .decrypt_padded_vec_mut::<NoPadding>(encrypted_iv)
        .map_err(|_| CryptoError::BadPadding)?;
    cbc::Decryptor::<Aes256>::new(key.0.as_slice().into(), iv.as_slice().into())
        .decrypt_padded_vec_mut::<Pkcs7>(body)
        .map_err(|_| CryptoError::BadPadding)
}

/// The inverse of [`decrypt`], for tests.
#[cfg(test)]
pub(crate) fn encrypt(plain: &[u8], key: &DepotKey, iv: [u8; IV_LEN]) -> Vec<u8> {
    use aes::cipher::BlockEncryptMut;

    let mut out = ecb::Encryptor::<Aes256>::new(key.0.as_slice().into())
        .encrypt_padded_vec_mut::<NoPadding>(&iv);
    out.extend(
        cbc::Encryptor::<Aes256>::new(key.0.as_slice().into(), iv.as_slice().into())
            .encrypt_padded_vec_mut::<Pkcs7>(plain),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> DepotKey {
        DepotKey([byte; 32])
    }

    #[test]
    fn round_trips() {
        let plain = b"some chunk data that spans more than one AES block".to_vec();
        let encrypted = encrypt(&plain, &key(7), [3; 16]);
        assert_ne!(&encrypted[16..], plain.as_slice());
        assert_eq!(decrypt(&encrypted, &key(7)).unwrap(), plain);
    }

    #[test]
    fn wrong_key_or_short_data_fails() {
        let encrypted = encrypt(b"secret", &key(1), [9; 16]);
        assert_ne!(decrypt(&encrypted, &key(2)), Ok(b"secret".to_vec()));
        assert_eq!(decrypt(&[0; 20], &key(1)), Err(CryptoError::TooShort));
    }

    #[test]
    fn debug_hides_the_key() {
        assert_eq!(format!("{:?}", key(0xAB)), "DepotKey(<redacted>)");
        assert!(DepotKey::from_slice(&[0; 31]).is_none());
        assert!(DepotKey::from_slice(&[0; 32]).is_some());
    }
}
