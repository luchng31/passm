//! PASSM1 envelope: XChaCha20-Poly1305 AEAD with an AAD-bound 75-byte header.
//!
//! Byte layout (all integers big-endian):
//!   bytes 0..5    magic `b"PASSM1"`
//!   byte  6       version `0x01`
//!   bytes 7..10   mem_kib u32 BE
//!   bytes 11..14  iterations u32 BE
//!   bytes 15..18  parallelism u32 BE
//!   bytes 19..50  salt (32B)
//!   bytes 51..74  nonce (24B, fresh from OsRng per encrypt)
//!   bytes 75..    ciphertext || 16B Poly1305 tag
//!
//! The entire 75-byte header is bound as AAD to XChaCha20-Poly1305, so any
//! tampering with the header (including the KDF params and salt) fails tag
//! verification. The header stores the KDF params + salt so `decrypt` can
//! re-derive the vault key from a password on another device.

use crate::KdfParams;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use std::fmt;

/// Magic bytes identifying a PASSM1 envelope.
pub const MAGIC: &[u8; 6] = b"PASSM1";
/// Envelope format version.
pub const VERSION: u8 = 0x01;
/// Header length: magic(6) + version(1) + 3x u32(12) + salt(32) + nonce(24).
pub const HEADER_LEN: usize = 75;
/// Poly1305 authentication tag length.
pub const TAG_LEN: usize = 16;

/// Errors produced by envelope parsing and decryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeError {
    /// Blob is shorter than the 75-byte header.
    TooShort,
    /// Magic bytes do not match `b"PASSM1"`.
    BadMagic,
    /// Version byte is not `0x01`.
    UnsupportedVersion(u8),
    /// AEAD authentication failed (wrong key, tampered data, or wrong AAD).
    AuthenticationFailed,
    /// AEAD encryption failed (impossible for in-memory payloads).
    EncryptFailed,
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "blob shorter than the 75-byte PASSM1 header"),
            Self::BadMagic => write!(f, "blob does not start with PASSM1 magic"),
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported PASSM1 envelope version {v:#04x}")
            }
            Self::AuthenticationFailed => write!(
                f,
                "PASSM1 authentication failed (wrong key or tampered data)"
            ),
            Self::EncryptFailed => write!(f, "PASSM1 encryption failed"),
        }
    }
}

impl std::error::Error for EnvelopeError {}

/// Result alias for envelope operations.
pub type Result<T> = std::result::Result<T, EnvelopeError>;

/// KDF params and salt stored in a PASSM1 envelope header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvelopeHeader {
    pub params: KdfParams,
    pub salt: [u8; 32],
}

/// Parses and validates the 75-byte header of a PASSM1 envelope.
///
/// Returns the KDF params and salt stored in the header, which `decrypt`
/// needs to re-derive the vault key from a password.
pub fn parse_header(blob: &[u8]) -> Result<EnvelopeHeader> {
    if blob.len() < HEADER_LEN {
        return Err(EnvelopeError::TooShort);
    }
    let h = &blob[..HEADER_LEN];
    if &h[0..6] != MAGIC {
        return Err(EnvelopeError::BadMagic);
    }
    if h[6] != VERSION {
        return Err(EnvelopeError::UnsupportedVersion(h[6]));
    }
    let mem_kib = u32::from_be_bytes(h[7..11].try_into().map_err(|_| EnvelopeError::TooShort)?);
    let iterations = u32::from_be_bytes(h[11..15].try_into().map_err(|_| EnvelopeError::TooShort)?);
    let parallelism =
        u32::from_be_bytes(h[15..19].try_into().map_err(|_| EnvelopeError::TooShort)?);
    let salt: [u8; 32] = h[19..51].try_into().map_err(|_| EnvelopeError::TooShort)?;
    Ok(EnvelopeHeader {
        params: KdfParams {
            mem_kib,
            iterations,
            parallelism,
        },
        salt,
    })
}

/// Encrypts `plaintext` into a PASSM1 envelope.
///
/// The returned blob is `[75-byte header || ciphertext || 16-byte tag]`. The
/// header (magic, version, KDF params, salt, and a fresh 24-byte nonce from
/// `OsRng`) is bound as AAD to XChaCha20-Poly1305.
///
/// # Errors
/// Returns [`EnvelopeError::EncryptFailed`] if the AEAD rejects the payload;
/// only possible for messages of 256 GiB or more, or AAD lengths that do not
/// fit in `u64`, so callers can treat it as an internal error.
pub fn encrypt(
    vault_key: &[u8; 32],
    params: &KdfParams,
    salt: [u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(VERSION);
    header.extend_from_slice(&params.mem_kib.to_be_bytes());
    header.extend_from_slice(&params.iterations.to_be_bytes());
    header.extend_from_slice(&params.parallelism.to_be_bytes());
    header.extend_from_slice(&salt);
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    header.extend_from_slice(&nonce);
    debug_assert_eq!(header.len(), HEADER_LEN);

    let cipher = XChaCha20Poly1305::new(&Key::from(*vault_key));
    let payload = Payload {
        msg: plaintext,
        aad: &header,
    };
    let ciphertext = cipher
        .encrypt(&XNonce::from(nonce), payload)
        .map_err(|_| EnvelopeError::EncryptFailed)?;
    header.extend_from_slice(&ciphertext);
    Ok(header)
}

/// Decrypts a PASSM1 envelope, reading the KDF params, salt, and nonce from
/// the header.
///
/// # Errors
/// Returns [`EnvelopeError::TooShort`] if the blob is shorter than the
/// 75-byte header, [`EnvelopeError::BadMagic`] if the magic is wrong,
/// [`EnvelopeError::UnsupportedVersion`] if the version is not `0x01`, and
/// [`EnvelopeError::AuthenticationFailed`] if the AEAD tag does not verify
/// (wrong key, tampered header, or tampered ciphertext).
pub fn decrypt(vault_key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>> {
    parse_header(blob)?;
    let nonce: [u8; 24] = blob[51..75]
        .try_into()
        .map_err(|_| EnvelopeError::TooShort)?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*vault_key));
    let payload = Payload {
        msg: &blob[HEADER_LEN..],
        aad: &blob[..HEADER_LEN],
    };
    cipher
        .decrypt(&XNonce::from(nonce), payload)
        .map_err(|_| EnvelopeError::AuthenticationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KdfParams;

    const KEY: [u8; 32] = [0x11; 32];
    const WRONG_KEY: [u8; 32] = [0x22; 32];
    const SALT: [u8; 32] = [0x42; 32];
    const PLAINTEXT: &[u8] = b"correct horse battery staple";
    // Non-default params so the header roundtrip test proves exact storage.
    const PARAMS: KdfParams = KdfParams {
        mem_kib: 131072,
        iterations: 5,
        parallelism: 2,
    };

    #[test]
    fn roundtrip_returns_original_plaintext() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        assert_eq!(decrypt(&KEY, &blob).unwrap(), PLAINTEXT);
    }

    #[test]
    fn wrong_key_fails_tag_verification() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        assert_eq!(
            decrypt(&WRONG_KEY, &blob),
            Err(EnvelopeError::AuthenticationFailed)
        );
    }

    #[test]
    fn tampering_any_header_byte_is_rejected() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        for i in 0..HEADER_LEN {
            let mut tampered = blob.clone();
            tampered[i] ^= 0x01;
            let result = decrypt(&KEY, &tampered);
            assert!(result.is_err(), "header byte {i} tamper must be rejected");
            // Bytes 7..74 (KDF params, salt, nonce) are AEAD-bound; magic and
            // version are validated structurally before the AEAD check.
            if (7..HEADER_LEN).contains(&i) {
                assert_eq!(
                    result,
                    Err(EnvelopeError::AuthenticationFailed),
                    "header byte {i} tamper must fail tag verification"
                );
            }
        }
    }

    #[test]
    fn tampering_ciphertext_fails_tag_verification() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        let indices = [HEADER_LEN, HEADER_LEN + PLAINTEXT.len() / 2, blob.len() - 1];
        for i in indices {
            let mut tampered = blob.clone();
            tampered[i] ^= 0x01;
            assert_eq!(
                decrypt(&KEY, &tampered),
                Err(EnvelopeError::AuthenticationFailed),
                "ciphertext byte {i} tamper must fail tag verification"
            );
        }
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        for version in [0x00, 0x02, 0xff] {
            let mut forged = blob.clone();
            forged[6] = version;
            assert_eq!(
                decrypt(&KEY, &forged),
                Err(EnvelopeError::UnsupportedVersion(version)),
                "version {version:#04x} must be rejected"
            );
        }
    }

    #[test]
    fn bad_magic_is_rejected() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        let mut forged = blob.clone();
        forged[0] = b'X';
        assert_eq!(decrypt(&KEY, &forged), Err(EnvelopeError::BadMagic));
    }

    #[test]
    fn two_encrypts_with_same_inputs_produce_different_ciphertext() {
        let a = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        let b = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        assert_ne!(a, b, "fresh nonce must produce distinct ciphertexts");
        assert_eq!(decrypt(&KEY, &a).unwrap(), PLAINTEXT);
        assert_eq!(decrypt(&KEY, &b).unwrap(), PLAINTEXT);
    }

    #[test]
    fn empty_plaintext_roundtrips() {
        let blob = encrypt(&KEY, &PARAMS, SALT, b"").unwrap();
        assert_eq!(blob.len(), HEADER_LEN + TAG_LEN);
        assert_eq!(decrypt(&KEY, &blob).unwrap(), b"");
    }

    #[test]
    fn blob_shorter_than_header_is_rejected() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        for len in 0..HEADER_LEN {
            assert_eq!(
                decrypt(&KEY, &blob[..len]),
                Err(EnvelopeError::TooShort),
                "blob of {len} bytes must be rejected as too short"
            );
        }
    }

    #[test]
    fn header_roundtrips_params_and_salt() {
        let blob = encrypt(&KEY, &PARAMS, SALT, PLAINTEXT).unwrap();
        let hdr = parse_header(&blob).unwrap();
        assert_eq!(hdr.params, PARAMS);
        assert_eq!(hdr.salt, SALT);
    }

    #[test]
    fn header_layout_is_byte_exact() {
        let blob = encrypt(&KEY, &PARAMS, SALT, b"").unwrap();
        assert_eq!(&blob[0..6], b"PASSM1");
        assert_eq!(blob[6], VERSION);
        assert_eq!(
            u32::from_be_bytes(blob[7..11].try_into().unwrap()),
            PARAMS.mem_kib
        );
        assert_eq!(
            u32::from_be_bytes(blob[11..15].try_into().unwrap()),
            PARAMS.iterations
        );
        assert_eq!(
            u32::from_be_bytes(blob[15..19].try_into().unwrap()),
            PARAMS.parallelism
        );
        assert_eq!(&blob[19..51], &SALT);
        assert_ne!(
            &blob[51..75],
            &[0u8; 24],
            "nonce must be fresh random bytes"
        );
    }
}
