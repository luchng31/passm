//! Subcommand implementations for the passm-cli verification harness.
//!
//! The `unlock` helper is the seam this crate exists to prove: parse the
//! PASSM1 header, re-derive the vault key from the password, and decrypt —
//! exactly what the app's unlock flow will do.

use crate::error::CliError;
use passm_crypto::envelope;
use passm_crypto::{derive_master_key, derive_vault_key, KdfParams};
use passm_vault::{Entry, Vault};
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use std::fs;

/// Device id stamped on entries created by the CLI (the app's real device id
/// comes from keyring + a persisted uuid in T8).
const DEVICE_ID: &str = "passm-cli";

/// `derive --password <pw> --salt <hex>`: print master + vault key hex.
///
/// Deterministic for a fixed password+salt (default KDF params), so the output
/// can be pinned against the T2 golden vectors.
pub fn derive(flags: &HashMap<String, String>) -> Result<(), CliError> {
    let password = get_flag(flags, "--password")?;
    let salt = parse_hex_salt(get_flag(flags, "--salt")?)?;
    let params = KdfParams::default();
    let master = derive_master_key(password.as_bytes(), &salt, &params)?;
    let vault_key = derive_vault_key(&master)
        .map_err(|e| CliError::Internal(format!("HKDF expand failed: {e}")))?;
    println!("master_key={}", hex(&master));
    println!("vault_key={}", hex(&vault_key));
    Ok(())
}

/// `encrypt --in <file> --out <file> --password <pw>`: plaintext -> PASSM1 blob.
///
/// Derives a fresh salt per invocation; the blob header stores the salt and
/// KDF params so `decrypt` can re-derive the key from the password alone.
pub fn encrypt(flags: &HashMap<String, String>) -> Result<(), CliError> {
    let in_path = get_flag(flags, "--in")?;
    let out_path = get_flag(flags, "--out")?;
    let password = get_flag(flags, "--password")?;
    let plaintext = fs::read(in_path)?;
    let salt = random_salt();
    let params = KdfParams::default();
    let master = derive_master_key(password.as_bytes(), &salt, &params)?;
    let vault_key = derive_vault_key(&master)
        .map_err(|e| CliError::Internal(format!("HKDF expand failed: {e}")))?;
    let blob =
        envelope::encrypt(&vault_key, &params, salt, &plaintext).map_err(CliError::Envelope)?;
    fs::write(out_path, blob)?;
    Ok(())
}

/// `decrypt --in <file> --out <file> --password <pw>`: PASSM1 blob -> plaintext.
///
/// A wrong password fails the AEAD tag check and surfaces as
/// [`CliError::Envelope`], which `main` maps to a nonzero exit code.
pub fn decrypt(flags: &HashMap<String, String>) -> Result<(), CliError> {
    let in_path = get_flag(flags, "--in")?;
    let out_path = get_flag(flags, "--out")?;
    let password = get_flag(flags, "--password")?;
    let blob = fs::read(in_path)?;
    let (_, _, plaintext) = unlock(password, &blob)?;
    fs::write(out_path, plaintext)?;
    Ok(())
}

/// `vault-add --vault <file> --password <pw> --title <t> --username <u>
/// --password-value <p> --url <u> --notes <n>`: add an entry and re-encrypt.
///
/// Decrypts the vault, appends the new entry, re-serializes canonically, and
/// writes a fresh PASSM1 blob back to the same path. The header salt and KDF
/// params are REUSED from the original blob: the vault key is derived from
/// `password + header salt`, so a fresh salt would change the key and the
/// password could no longer unlock the vault. Only the nonce is fresh.
pub fn vault_add(flags: &HashMap<String, String>) -> Result<(), CliError> {
    let vault_path = get_flag(flags, "--vault")?;
    let password = get_flag(flags, "--password")?;
    let title = get_flag(flags, "--title")?;
    let username = get_flag(flags, "--username")?;
    let password_value = get_flag(flags, "--password-value")?;
    let url = get_flag(flags, "--url")?;
    let notes = get_flag(flags, "--notes")?;
    let blob = fs::read(vault_path)?;
    let (header, vault_key, plaintext) = unlock(password, &blob)?;
    let mut vault: Vault = serde_json::from_slice(&plaintext)?;
    vault.entries.push(Entry::new(
        title.to_string(),
        username.to_string(),
        password_value.to_string(),
        url.to_string(),
        notes.to_string(),
        DEVICE_ID.to_string(),
    ));
    let new_plaintext = vault
        .canonical_json()
        .map_err(|e| CliError::Internal(format!("vault serialization failed: {e}")))?;
    let new_blob = envelope::encrypt(&vault_key, &header.params, header.salt, &new_plaintext)
        .map_err(CliError::Envelope)?;
    fs::write(vault_path, new_blob)?;
    Ok(())
}

/// `vault-list --vault <file> --password <pw>`: print `id title username` lines.
///
/// Deleted (tombstone) entries are marked with a trailing `[deleted]`.
pub fn vault_list(flags: &HashMap<String, String>) -> Result<(), CliError> {
    let vault_path = get_flag(flags, "--vault")?;
    let password = get_flag(flags, "--password")?;
    let blob = fs::read(vault_path)?;
    let (_, _, plaintext) = unlock(password, &blob)?;
    let vault: Vault = serde_json::from_slice(&plaintext)?;
    for entry in &vault.entries {
        if entry.deleted {
            println!("{} {} {} [deleted]", entry.id, entry.title, entry.username);
        } else {
            println!("{} {} {}", entry.id, entry.title, entry.username);
        }
    }
    Ok(())
}

/// The unlock seam: header -> params+salt -> master key -> vault key -> decrypt.
///
/// Returns the parsed header alongside the vault key and plaintext so callers
/// that re-encrypt (vault-add) can reuse the original salt and KDF params.
fn unlock(
    password: &str,
    blob: &[u8],
) -> Result<(envelope::EnvelopeHeader, [u8; 32], Vec<u8>), CliError> {
    let header = envelope::parse_header(blob)?;
    let master = derive_master_key(password.as_bytes(), &header.salt, &header.params)?;
    let vault_key = derive_vault_key(&master)
        .map_err(|e| CliError::Internal(format!("HKDF expand failed: {e}")))?;
    let plaintext = envelope::decrypt(&vault_key, blob)?;
    Ok((header, vault_key, plaintext))
}

fn get_flag<'a>(flags: &'a HashMap<String, String>, name: &str) -> Result<&'a str, CliError> {
    flags
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing required flag {name}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parses a 64-character hex string into a 32-byte salt.
fn parse_hex_salt(s: &str) -> Result<[u8; 32], CliError> {
    if s.len() != 64 {
        return Err(CliError::Usage(format!(
            "salt must be 64 hex characters, got {}",
            s.len()
        )));
    }
    let mut salt = [0u8; 32];
    for (i, byte) in salt.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| CliError::Usage(format!("invalid hex salt at byte {i}")))?;
    }
    Ok(salt)
}

fn random_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}
