//! passm-crypto: AEAD (ChaCha20-Poly1305), KDF (Argon2id, HKDF), key hygiene (zeroize).

use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

pub mod envelope;

/// Argon2id KDF parameters. Defaults match the PASSM1 spec: 64 MiB / t=3 / p=4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            mem_kib: 65536,
            iterations: 3,
            parallelism: 4,
        }
    }
}

/// Derives the 32-byte master key from the password via Argon2id (v1.3).
///
/// The password is copied into an owned buffer that is zeroized before return.
pub fn derive_master_key(
    password: &[u8],
    salt: &[u8; 32],
    params: &KdfParams,
) -> Result<[u8; 32], argon2::Error> {
    let argon2 = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(params.mem_kib, params.iterations, params.parallelism, Some(32))?,
    );
    let mut password_buf = password.to_vec();
    let mut master = [0u8; 32];
    let result = argon2.hash_password_into(&password_buf, salt, &mut master);
    password_buf.zeroize();
    match result {
        Ok(()) => Ok(master),
        Err(err) => {
            master.zeroize();
            Err(err)
        }
    }
}

/// Derives the 32-byte vault key from the master key via HKDF-SHA256.
///
/// # Panics
/// Never in practice: HKDF-SHA256 expand with L=32 cannot fail (max output is 255*32 bytes).
pub fn derive_vault_key(master: &[u8; 32]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, master);
    let mut vault_key = [0u8; 32];
    match hk.expand(b"passm-v1-vault-key", &mut vault_key) {
        Ok(()) => vault_key,
        Err(_) => unreachable!("HKDF-SHA256 expand with L=32 cannot fail"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hkdf::Hkdf;
    use sha2::Sha256;

    // Fixed test fixture. The golden vector below was generated once with the real
    // implementation (2026-08-14) and frozen; any change to the KDF parameters,
    // algorithm, or info string breaks these constants.
    const PASSWORD: &[u8] = b"correct horse battery staple";
    const SALT: [u8; 32] = [0x42; 32];

    // Golden vector: derive_master_key(PASSWORD, &SALT, &KdfParams::default())
    // then derive_vault_key(&master). Inputs: password = b"correct horse battery staple",
    // salt = [0x42; 32], params = { mem_kib: 65536, iterations: 3, parallelism: 4 }.
    const GOLDEN_MASTER_KEY: [u8; 32] = [
        234, 233, 121, 167, 42, 34, 187, 233, 127, 39, 145, 14, 113, 33, 68, 69, 60, 219, 247, 216,
        217, 171, 236, 173, 106, 144, 204, 114, 115, 15, 76, 177,
    ];
    const GOLDEN_VAULT_KEY: [u8; 32] = [
        183, 240, 204, 91, 104, 7, 113, 1, 158, 193, 87, 92, 64, 38, 83, 130, 74, 100, 63, 255,
        231, 51, 18, 95, 195, 139, 179, 250, 18, 131, 30, 235,
    ];

    #[test]
    fn kdf_params_default_is_64mib_3_4() {
        let params = KdfParams::default();
        assert_eq!(params.mem_kib, 65536);
        assert_eq!(params.iterations, 3);
        assert_eq!(params.parallelism, 4);
    }

    #[test]
    fn golden_vector_master_and_vault_key() {
        let params = KdfParams::default();
        let master = derive_master_key(PASSWORD, &SALT, &params).unwrap();
        assert_eq!(master, GOLDEN_MASTER_KEY);
        let vault = derive_vault_key(&master);
        assert_eq!(vault, GOLDEN_VAULT_KEY);
    }

    #[test]
    fn vault_key_matches_independent_hkdf() {
        let params = KdfParams::default();
        let master = derive_master_key(PASSWORD, &SALT, &params).unwrap();
        let hk = Hkdf::<Sha256>::new(None, &master);
        let mut expected = [0u8; 32];
        hk.expand(b"passm-v1-vault-key", &mut expected).unwrap();
        assert_eq!(derive_vault_key(&master), expected);
    }

    #[test]
    fn wrong_password_yields_different_master_key() {
        let params = KdfParams::default();
        let master = derive_master_key(PASSWORD, &SALT, &params).unwrap();
        let wrong = derive_master_key(b"wrong password", &SALT, &params).unwrap();
        assert_ne!(master, wrong);
    }

    #[test]
    fn wrong_salt_yields_different_master_key() {
        let params = KdfParams::default();
        let master = derive_master_key(PASSWORD, &SALT, &params).unwrap();
        let mut wrong_salt = SALT;
        wrong_salt[0] ^= 0x01;
        let wrong = derive_master_key(PASSWORD, &wrong_salt, &params).unwrap();
        assert_ne!(master, wrong);
    }

    #[test]
    fn wrong_params_yield_different_master_key() {
        let params = KdfParams::default();
        let master = derive_master_key(PASSWORD, &SALT, &params).unwrap();
        let wrong_params = KdfParams {
            iterations: 4,
            ..KdfParams::default()
        };
        let wrong = derive_master_key(PASSWORD, &SALT, &wrong_params).unwrap();
        assert_ne!(master, wrong);
    }
}