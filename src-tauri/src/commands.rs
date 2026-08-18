//! Tauri backend commands: unlock/lock/CRUD/search/copy/generate/sync/config.
//!
//! All testable logic lives in pure functions below (no Tauri runtime); the
//! `#[tauri::command]` wrappers at the bottom are thin adapters that read
//! managed state, call a pure function, and persist the vault.

use passm_crypto::envelope;
use passm_sync::PatStore;
use passm_sync::SyncError;
use passm_vault::{Entry, Vault};
use rand::rngs::OsRng;
use rand::RngCore;
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::session::{unlock_session, Clock, SessionState, SystemClock};
use crate::AppPaths;

/// Seconds after which a copied secret is cleared from the clipboard.
pub const CLIPBOARD_CLEAR_SECS: u64 = 30;
/// Password generator length bounds (clamped, never errors).
pub const PASSWORD_MIN_LEN: u32 = 8;
pub const PASSWORD_MAX_LEN: u32 = 128;

/// Payload for create/update commands (all fields required, empty allowed).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntryInput {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
}

/// Persisted sync configuration (`<data_dir>/sync_config.json`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    pub remote_url: String,
}

/// Serializable projection of `passm_sync::SyncOutcome` for the frontend.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SyncStatus {
    pub pushed: bool,
    pub pulled: bool,
    pub merged: bool,
    pub backup_created: Option<String>,
}

impl From<&passm_sync::SyncOutcome> for SyncStatus {
    fn from(outcome: &passm_sync::SyncOutcome) -> Self {
        Self {
            pushed: outcome.pushed,
            pulled: outcome.pulled,
            merged: outcome.merged,
            backup_created: outcome
                .backup_created
                .as_ref()
                .map(|p| p.display().to_string()),
        }
    }
}

/// Typed command error; serializes to its Chinese display message so the
/// frontend can surface it directly.
#[derive(Debug)]
pub enum CommandError {
    Locked,
    WrongPassword,
    VaultFileMissing,
    EntryNotFound,
    SyncNotConfigured,
    InvalidInput(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Envelope(envelope::EnvelopeError),
    Argon2(argon2::Error),
    Sync(SyncError),
    Clipboard(String),
    Internal(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Locked => write!(f, "请先解锁"),
            CommandError::WrongPassword => write!(f, "密码错误"),
            CommandError::VaultFileMissing => write!(f, "保险库文件不存在"),
            CommandError::EntryNotFound => write!(f, "条目不存在"),
            CommandError::SyncNotConfigured => write!(f, "同步未配置"),
            CommandError::InvalidInput(msg) => write!(f, "输入无效: {msg}"),
            CommandError::Io(e) => write!(f, "文件读写失败: {e}"),
            CommandError::Json(e) => write!(f, "数据解析失败: {e}"),
            CommandError::Envelope(e) => write!(f, "{}", envelope_message(e)),
            CommandError::Argon2(e) => write!(f, "密钥派生失败: {e}"),
            CommandError::Sync(e) => write!(f, "{}", sync_message(e)),
            CommandError::Clipboard(msg) => write!(f, "剪贴板错误: {msg}"),
            CommandError::Internal(msg) => write!(f, "内部错误: {msg}"),
        }
    }
}

fn envelope_message(e: &envelope::EnvelopeError) -> &'static str {
    match e {
        envelope::EnvelopeError::AuthenticationFailed => "密码错误",
        _ => "保险库文件损坏",
    }
}

fn sync_message(e: &SyncError) -> String {
    match e {
        SyncError::PatMissing => "同步未配置".to_string(),
        _ => format!("同步失败: {e}"),
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<envelope::EnvelopeError> for CommandError {
    fn from(e: envelope::EnvelopeError) -> Self {
        CommandError::Envelope(e)
    }
}

impl From<argon2::Error> for CommandError {
    fn from(e: argon2::Error) -> Self {
        CommandError::Argon2(e)
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        CommandError::Json(e)
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e)
    }
}

impl From<SyncError> for CommandError {
    fn from(e: SyncError) -> Self {
        CommandError::Sync(e)
    }
}

/// Password generator charset: A-Z, a-z, 0-9, and common symbols.
pub const PASSWORD_CHARSET: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()-_=+[]{};:,.<>?";

// ---------------------------------------------------------------------------
// Pure functions (no Tauri runtime — unit-tested above)
// ---------------------------------------------------------------------------

/// Derives the vault key from `password` + the blob's header salt/params,
/// decrypts the vault, and returns the plaintext vault + key. Wrong password
/// surfaces as `CommandError::WrongPassword` ("密码错误").
pub fn unlock_vault(password: &str, blob: &[u8]) -> Result<(Vault, [u8; 32]), CommandError> {
    let header = envelope::parse_header(blob)?;
    let master = Zeroizing::new(passm_crypto::derive_master_key(
        password.as_bytes(),
        &header.salt,
        &header.params,
    )?);
    let vault_key = passm_crypto::derive_vault_key(&master);
    let plaintext = envelope::decrypt(&vault_key, blob).map_err(|e| match e {
        envelope::EnvelopeError::AuthenticationFailed => CommandError::WrongPassword,
        other => CommandError::Envelope(other),
    })?;
    let vault: Vault = serde_json::from_slice(&plaintext)?;
    Ok((vault, vault_key))
}

/// Adds a new entry (version 1) to the vault and returns it.
pub fn create_entry(vault: &mut Vault, input: &EntryInput, device_id: &str) -> Entry {
    let entry = Entry::new(
        input.title.clone(),
        input.username.clone(),
        input.password.clone(),
        input.url.clone(),
        input.notes.clone(),
        device_id.to_string(),
    );
    vault.entries.push(entry.clone());
    entry
}

/// Updates a live entry's fields, records the editing device, and bumps it.
pub fn update_entry(
    vault: &mut Vault,
    id: Uuid,
    input: &EntryInput,
    device_id: &str,
) -> Result<Entry, CommandError> {
    let entry = vault
        .entries
        .iter_mut()
        .find(|e| e.id == id && !e.deleted)
        .ok_or(CommandError::EntryNotFound)?;
    entry.title = input.title.clone();
    entry.username = input.username.clone();
    entry.password = input.password.clone();
    entry.url = input.url.clone();
    entry.notes = input.notes.clone();
    entry.device_id = device_id.to_string();
    entry.bump();
    Ok(entry.clone())
}

/// Tombstones a live entry (sync-safe delete) and bumps it.
pub fn delete_entry(vault: &mut Vault, id: Uuid, device_id: &str) -> Result<Entry, CommandError> {
    let entry = vault
        .entries
        .iter_mut()
        .find(|e| e.id == id && !e.deleted)
        .ok_or(CommandError::EntryNotFound)?;
    entry.mark_deleted();
    entry.device_id = device_id.to_string();
    entry.bump();
    Ok(entry.clone())
}

/// Live entries sorted by id (deterministic; tombstones are sync artifacts).
pub fn list_entries(vault: &Vault) -> Vec<Entry> {
    let mut entries: Vec<Entry> = vault
        .entries
        .iter()
        .filter(|e| !e.deleted)
        .cloned()
        .collect();
    entries.sort_by_key(|e| e.id);
    entries
}

/// A live entry by id; `None` for missing or tombstoned entries.
pub fn get_entry(vault: &Vault, id: Uuid) -> Option<Entry> {
    vault
        .entries
        .iter()
        .find(|e| e.id == id && !e.deleted)
        .cloned()
}

/// Case-insensitive multi-term search over title/username/url. Every
/// whitespace-separated term must match at least one field (AND semantics).
pub fn search_entries(vault: &Vault, q: &str) -> Vec<Entry> {
    let terms: Vec<String> = q.split_whitespace().map(|t| t.to_lowercase()).collect();
    let mut results: Vec<Entry> = vault
        .entries
        .iter()
        .filter(|e| !e.deleted)
        .filter(|e| {
            terms.iter().all(|term| {
                e.title.to_lowercase().contains(term)
                    || e.username.to_lowercase().contains(term)
                    || e.url.to_lowercase().contains(term)
            })
        })
        .cloned()
        .collect();
    results.sort_by_key(|e| e.id);
    results
}

/// Cryptographically random password from `PASSWORD_CHARSET`, length clamped
/// to `PASSWORD_MIN_LEN..=PASSWORD_MAX_LEN`. Rejection sampling avoids modulo
/// bias.
pub fn generate_password_pure(length: u32) -> String {
    let len = length.clamp(PASSWORD_MIN_LEN, PASSWORD_MAX_LEN) as usize;
    let charset = PASSWORD_CHARSET.as_bytes();
    let range = 256 - (256 % charset.len());
    let mut rng = OsRng;
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut byte = [0u8; 1];
        rng.fill_bytes(&mut byte);
        if (byte[0] as usize) < range {
            out.push(charset[byte[0] as usize % charset.len()] as char);
        }
    }
    out
}

/// True when the clipboard still holds the copied secret (clear it); false
/// when the user has copied something else since (leave it alone).
pub fn should_clear_clipboard(original: &str, current: &str) -> bool {
    original == current
}

/// Re-encrypts the vault reusing the ORIGINAL header salt + params from the
/// current blob (T6 invariant: the vault key is derived from password +
/// header salt, so a fresh salt would change the key). Only the nonce is
/// fresh.
pub fn reencrypt_vault(
    vault_key: &[u8; 32],
    current_blob: &[u8],
    vault: &Vault,
) -> Result<Vec<u8>, CommandError> {
    let header = envelope::parse_header(current_blob)?;
    Ok(envelope::encrypt(
        vault_key,
        &header.params,
        header.salt,
        &vault.canonical_json(),
    ))
}

/// Reads the persisted sync config, if any.
pub fn load_sync_config(data_dir: &Path) -> Result<Option<SyncConfig>, CommandError> {
    match fs::read(data_dir.join("sync_config.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(CommandError::Json),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CommandError::Io(e)),
    }
}

/// Persists the sync config as JSON at `<data_dir>/sync_config.json`.
pub fn save_sync_config(data_dir: &Path, config: &SyncConfig) -> Result<(), CommandError> {
    fs::create_dir_all(data_dir)?;
    let bytes = serde_json::to_vec(config)?;
    fs::write(data_dir.join("sync_config.json"), bytes)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers (Tauri runtime)
// ---------------------------------------------------------------------------

/// Restores the in-memory repo handle from disk when a repo already exists
/// (offline-safe: `ensure_clone` only opens + repairs origin when `.git`
/// exists, no network). Needed because `passm_sync`'s repo handle is
/// process-lifetime state that is empty after an app restart.
fn ensure_repo_ready(data_dir: &Path) -> Result<(), CommandError> {
    let config = load_sync_config(data_dir)?.ok_or(CommandError::SyncNotConfigured)?;
    let repo_dir = data_dir.join("repo");
    if !repo_dir.join(".git").exists() {
        return Err(CommandError::SyncNotConfigured);
    }
    let pat_store = passm_sync::KeyringPatStore::new()?;
    let pat = pat_store.get()?.unwrap_or_default();
    passm_sync::ensure_clone(&config.remote_url, &repo_dir, &pat)?;
    Ok(())
}

/// Re-encrypts the in-memory vault (reusing the current blob's header
/// salt+params), writes `vault.enc`, and commits it. When sync is not
/// configured the local save still succeeds and the commit is skipped.
fn persist_vault(app: &AppHandle, state: &SessionState, msg: &str) -> Result<(), CommandError> {
    let vault = state.vault.as_ref().ok_or(CommandError::Locked)?;
    let vault_key = state.vault_key.as_ref().ok_or(CommandError::Locked)?;
    let paths = app.state::<AppPaths>();
    let blob_path = paths.data_dir.join("repo").join(passm_sync::VAULT_FILE);
    let current_blob = fs::read(&blob_path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => CommandError::VaultFileMissing,
        _ => CommandError::Io(e),
    })?;
    let new_blob = reencrypt_vault(vault_key, &current_blob, vault)?;
    fs::write(&blob_path, &new_blob)?;
    match passm_sync::commit_vault_file(passm_sync::VAULT_FILE, msg) {
        Ok(_) => Ok(()),
        Err(SyncError::RepoNotInitialized) => {
            if ensure_repo_ready(&paths.data_dir).is_ok() {
                passm_sync::commit_vault_file(passm_sync::VAULT_FILE, msg)?;
            }
            Ok(())
        }
        Err(e) => Err(CommandError::Sync(e)),
    }
}

/// Runs a sync pass with the session's key + device id and the keyring PAT.
/// The blocking git work runs on the blocking thread pool.
async fn sync_now_inner(app: &AppHandle) -> Result<SyncStatus, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let (vault_key, device_id) = {
        let guard = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = guard.vault_key.as_ref().ok_or(CommandError::Locked)?;
        (key.clone(), guard.device_id.clone())
    };
    let pat_store = passm_sync::KeyringPatStore::new()?;
    let pat = Zeroizing::new(pat_store.get()?.ok_or(CommandError::SyncNotConfigured)?);
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        passm_sync::sync(&pat, &vault_key, &device_id)
    })
    .await
    .map_err(|e| CommandError::Internal(e.to_string()))??;
    Ok(SyncStatus::from(&outcome))
}

// ---------------------------------------------------------------------------
// Tauri commands (thin wrappers)
// ---------------------------------------------------------------------------

/// Unlock: derive the vault key from the password + header salt/params,
/// decrypt `vault.enc`, load the session, then sync best-effort (unlock
/// succeeds even offline).
#[tauri::command]
pub(crate) async fn unlock(app: AppHandle, password: String) -> Result<(), CommandError> {
    let password = Zeroizing::new(password);
    let paths = app.state::<AppPaths>();
    let blob_path = paths.data_dir.join("repo").join(passm_sync::VAULT_FILE);
    let blob = fs::read(&blob_path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => CommandError::VaultFileMissing,
        _ => CommandError::Io(e),
    })?;
    let (vault, vault_key) = unlock_vault(&password, &blob)?;
    let device_id =
        passm_sync::device_id::load_or_create(&paths.data_dir).map_err(CommandError::Sync)?;
    let state = app.state::<Mutex<SessionState>>();
    {
        let mut guard = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.device_id = device_id;
        unlock_session(&mut guard, vault_key, vault, SystemClock.now_unix());
    }
    let _ = ensure_repo_ready(&paths.data_dir);
    let _ = sync_now_inner(&app).await;
    Ok(())
}

/// List live entries (sorted by id).
#[tauri::command]
pub(crate) async fn list(app: AppHandle) -> Result<Vec<Entry>, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let vault = guard.vault.as_ref().ok_or(CommandError::Locked)?;
    Ok(list_entries(vault))
}

/// Get one live entry by id.
#[tauri::command]
pub(crate) async fn get(app: AppHandle, id: String) -> Result<Entry, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let vault = guard.vault.as_ref().ok_or(CommandError::Locked)?;
    let id = parse_entry_id(&id)?;
    get_entry(vault, id).ok_or(CommandError::EntryNotFound)
}

/// Create an entry, persist the vault, and commit.
#[tauri::command]
pub(crate) async fn create(app: AppHandle, input: EntryInput) -> Result<Entry, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let device_id = guard.device_id.clone();
    let entry = {
        let vault = guard.vault.as_mut().ok_or(CommandError::Locked)?;
        create_entry(vault, &input, &device_id)
    };
    let msg = format!("create: {}", entry.title);
    persist_vault(&app, &guard, &msg)?;
    Ok(entry)
}

/// Update an entry, persist the vault, and commit.
#[tauri::command]
pub(crate) async fn update(
    app: AppHandle,
    id: String,
    input: EntryInput,
) -> Result<Entry, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let device_id = guard.device_id.clone();
    let id = parse_entry_id(&id)?;
    let entry = {
        let vault = guard.vault.as_mut().ok_or(CommandError::Locked)?;
        update_entry(vault, id, &input, &device_id)?
    };
    let msg = format!("update: {}", entry.title);
    persist_vault(&app, &guard, &msg)?;
    Ok(entry)
}

/// Delete (tombstone) an entry, persist the vault, and commit.
#[tauri::command]
pub(crate) async fn delete(app: AppHandle, id: String) -> Result<Entry, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let device_id = guard.device_id.clone();
    let id = parse_entry_id(&id)?;
    let entry = {
        let vault = guard.vault.as_mut().ok_or(CommandError::Locked)?;
        delete_entry(vault, id, &device_id)?
    };
    let msg = format!("delete: {}", entry.title);
    persist_vault(&app, &guard, &msg)?;
    Ok(entry)
}

/// Case-insensitive multi-term search over title/username/url.
#[tauri::command]
pub(crate) async fn search(app: AppHandle, q: String) -> Result<Vec<Entry>, CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let vault = guard.vault.as_ref().ok_or(CommandError::Locked)?;
    Ok(search_entries(vault, &q))
}

/// Copy a field (password/username/url) to the clipboard and schedule a 30s
/// auto-clear that only fires if the clipboard still holds the same value.
#[tauri::command]
pub(crate) async fn copy(app: AppHandle, field: String, id: String) -> Result<(), CommandError> {
    let state = app.state::<Mutex<SessionState>>();
    let value = {
        let guard = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let vault = guard.vault.as_ref().ok_or(CommandError::Locked)?;
        let id = parse_entry_id(&id)?;
        let entry = get_entry(vault, id).ok_or(CommandError::EntryNotFound)?;
        match field.as_str() {
            "password" => entry.password,
            "username" => entry.username,
            "url" => entry.url,
            _ => return Err(CommandError::InvalidInput("无效的字段".into())),
        }
    };
    app.clipboard()
        .write_text(value.clone())
        .map_err(|e| CommandError::Clipboard(e.to_string()))?;
    let app_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(CLIPBOARD_CLEAR_SECS));
        let clipboard = app_handle.clipboard();
        if let Ok(current) = clipboard.read_text() {
            if should_clear_clipboard(&value, &current) {
                let _ = clipboard.clear();
            }
        }
    });
    Ok(())
}

/// Generate a cryptographically random password (length clamped 8..=128).
#[tauri::command]
pub(crate) async fn generate_password(length: u32) -> Result<String, CommandError> {
    Ok(generate_password_pure(length))
}

/// Sync the vault with the remote (requires unlocked session + configured PAT).
#[tauri::command]
pub(crate) async fn sync_now(app: AppHandle) -> Result<SyncStatus, CommandError> {
    sync_now_inner(&app).await
}

/// Current sync config (remote URL), or `None` if sync is not configured.
#[tauri::command]
pub(crate) async fn get_sync_config(app: AppHandle) -> Result<Option<SyncConfig>, CommandError> {
    let paths = app.state::<AppPaths>();
    load_sync_config(&paths.data_dir)
}

/// Configure sync: clone/repair the repo, store the PAT in the keyring, and
/// persist the remote URL. The PAT is never written to the repo or config.
#[tauri::command]
pub(crate) async fn set_sync_config(
    app: AppHandle,
    remote_url: String,
    pat: String,
) -> Result<(), CommandError> {
    let paths = app.state::<AppPaths>();
    let repo_dir = paths.data_dir.join("repo");
    let pat = Zeroizing::new(pat);
    passm_sync::ensure_clone(&remote_url, &repo_dir, &pat)?;
    let pat_store = passm_sync::KeyringPatStore::new()?;
    pat_store.set(&pat)?;
    save_sync_config(
        &paths.data_dir,
        &SyncConfig {
            remote_url: remote_url.clone(),
        },
    )?;
    Ok(())
}

fn parse_entry_id(id: &str) -> Result<Uuid, CommandError> {
    Uuid::parse_str(id).map_err(|_| CommandError::InvalidInput("无效的条目 ID".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use passm_crypto::envelope;
    use passm_crypto::KdfParams;
    use passm_vault::Vault;
    use tempfile::tempdir;

    fn input(title: &str, username: &str) -> EntryInput {
        EntryInput {
            title: title.to_string(),
            username: username.to_string(),
            password: "pw".to_string(),
            url: "https://example.com".to_string(),
            notes: "notes".to_string(),
        }
    }

    fn vault_with(entries: Vec<Entry>) -> Vault {
        Vault { entries }
    }

    // ---- CRUD ----

    #[test]
    fn create_entry_adds_version_1_entry() {
        let mut vault = Vault::empty();
        let entry = create_entry(&mut vault, &input("GitHub", "alice"), "dev-1");
        assert_eq!(entry.version, 1);
        assert!(!entry.deleted);
        assert_eq!(entry.device_id, "dev-1");
        assert_eq!(entry.title, "GitHub");
        assert_eq!(vault.entries.len(), 1);
    }

    #[test]
    fn update_entry_bumps_version_and_updates_fields() {
        let mut vault = Vault::empty();
        let created = create_entry(&mut vault, &input("GitHub", "alice"), "dev-1");
        let updated =
            update_entry(&mut vault, created.id, &input("GitLab", "bob"), "dev-2").unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(updated.title, "GitLab");
        assert_eq!(updated.username, "bob");
        assert_eq!(updated.device_id, "dev-2");
        assert_eq!(vault.entries.len(), 1);
    }

    #[test]
    fn update_entry_missing_id_errors() {
        let mut vault = Vault::empty();
        let err = update_entry(&mut vault, Uuid::new_v4(), &input("x", "y"), "dev-1").unwrap_err();
        assert_eq!(err.to_string(), "条目不存在");
    }

    #[test]
    fn delete_entry_marks_tombstone_and_bumps() {
        let mut vault = Vault::empty();
        let created = create_entry(&mut vault, &input("GitHub", "alice"), "dev-1");
        let deleted = delete_entry(&mut vault, created.id, "dev-2").unwrap();
        assert!(deleted.deleted);
        assert_eq!(deleted.version, 2);
        assert_eq!(deleted.device_id, "dev-2");
    }

    #[test]
    fn delete_entry_missing_id_errors() {
        let mut vault = Vault::empty();
        let err = delete_entry(&mut vault, Uuid::new_v4(), "dev-1").unwrap_err();
        assert_eq!(err.to_string(), "条目不存在");
    }

    #[test]
    fn list_entries_excludes_deleted_and_sorts_by_id() {
        let mut vault = Vault::empty();
        let a = create_entry(&mut vault, &input("A", "a"), "dev-1");
        let b = create_entry(&mut vault, &input("B", "b"), "dev-1");
        delete_entry(&mut vault, a.id, "dev-1").unwrap();
        let listed = list_entries(&vault);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, b.id);
    }

    #[test]
    fn get_entry_returns_entry_and_none_for_deleted_or_missing() {
        let mut vault = Vault::empty();
        let created = create_entry(&mut vault, &input("GitHub", "alice"), "dev-1");
        assert_eq!(get_entry(&vault, created.id).unwrap().title, "GitHub");
        assert!(get_entry(&vault, Uuid::new_v4()).is_none());
        delete_entry(&mut vault, created.id, "dev-1").unwrap();
        assert!(get_entry(&vault, created.id).is_none());
    }

    // ---- Search ----

    #[test]
    fn search_is_case_insensitive() {
        let mut vault = Vault::empty();
        create_entry(
            &mut vault,
            &EntryInput {
                title: "GitHub Login".into(),
                username: "alice@example.com".into(),
                password: "pw".into(),
                url: "https://github.com".into(),
                notes: "".into(),
            },
            "dev-1",
        );
        assert_eq!(search_entries(&vault, "github").len(), 1);
        assert_eq!(search_entries(&vault, "GITHUB").len(), 1);
        assert_eq!(search_entries(&vault, "ALICE").len(), 1);
        assert_eq!(search_entries(&vault, "example.com").len(), 1);
    }

    #[test]
    fn search_multi_term_requires_all_terms() {
        let mut vault = Vault::empty();
        create_entry(
            &mut vault,
            &input("GitHub Login", "alice@example.com"),
            "dev-1",
        );
        create_entry(
            &mut vault,
            &input("GitLab Login", "bob@example.com"),
            "dev-1",
        );
        assert_eq!(search_entries(&vault, "github alice").len(), 1);
        assert_eq!(search_entries(&vault, "github bob").len(), 0);
        assert_eq!(search_entries(&vault, "login").len(), 2);
    }

    #[test]
    fn search_excludes_deleted_entries() {
        let mut vault = Vault::empty();
        let created = create_entry(&mut vault, &input("GitHub", "alice"), "dev-1");
        delete_entry(&mut vault, created.id, "dev-1").unwrap();
        assert!(search_entries(&vault, "github").is_empty());
    }

    #[test]
    fn search_empty_query_returns_all_live_entries() {
        let mut vault = Vault::empty();
        create_entry(&mut vault, &input("A", "a"), "dev-1");
        create_entry(&mut vault, &input("B", "b"), "dev-1");
        assert_eq!(search_entries(&vault, "").len(), 2);
        assert_eq!(search_entries(&vault, "   ").len(), 2);
    }

    // ---- generate_password ----

    #[test]
    fn generate_password_respects_length() {
        assert_eq!(generate_password_pure(16).len(), 16);
        assert_eq!(generate_password_pure(8).len(), 8);
        assert_eq!(generate_password_pure(128).len(), 128);
    }

    #[test]
    fn generate_password_clamps_length() {
        assert_eq!(generate_password_pure(4).len(), 8);
        assert_eq!(generate_password_pure(0).len(), 8);
        assert_eq!(generate_password_pure(200).len(), 128);
    }

    #[test]
    fn generate_password_uses_only_charset() {
        let charset: Vec<char> = PASSWORD_CHARSET.chars().collect();
        for _ in 0..20 {
            let pw = generate_password_pure(32);
            assert!(pw.chars().all(|c| charset.contains(&c)));
        }
    }

    #[test]
    fn generate_password_different_outputs() {
        let a = generate_password_pure(32);
        let b = generate_password_pure(32);
        assert_ne!(a, b);
    }

    // ---- unlock (via envelope fixture) ----

    fn fixture_blob(password: &str, vault: &Vault) -> Vec<u8> {
        let params = KdfParams {
            mem_kib: 1024,
            iterations: 1,
            parallelism: 1,
        };
        let salt = [0x42; 32];
        let master = passm_crypto::derive_master_key(password.as_bytes(), &salt, &params).unwrap();
        let key = passm_crypto::derive_vault_key(&master);
        envelope::encrypt(&key, &params, salt, &vault.canonical_json())
    }

    #[test]
    fn unlock_vault_correct_password_succeeds() {
        let vault = vault_with(vec![Entry::new(
            "GitHub".into(),
            "alice".into(),
            "pw".into(),
            "https://github.com".into(),
            "".into(),
            "dev-1".into(),
        )]);
        let blob = fixture_blob("correct password", &vault);
        let (decrypted, key) = unlock_vault("correct password", &blob).unwrap();
        assert_eq!(decrypted.canonical_json(), vault.canonical_json());
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn unlock_vault_wrong_password_errors_with_chinese_message() {
        let blob = fixture_blob("correct password", &Vault::empty());
        let err = unlock_vault("wrong password", &blob).unwrap_err();
        assert_eq!(err.to_string(), "密码错误");
    }

    #[test]
    fn unlock_vault_corrupt_blob_errors() {
        let err = unlock_vault("any", b"not an envelope").unwrap_err();
        assert_eq!(err.to_string(), "保险库文件损坏");
    }

    // ---- re-encrypt salt invariant (T6) ----

    #[test]
    fn reencrypt_reuses_header_salt_and_params() {
        let params = KdfParams {
            mem_kib: 1024,
            iterations: 1,
            parallelism: 1,
        };
        let salt = [0x42; 32];
        let key = [0x11; 32];
        let vault = vault_with(vec![Entry::new(
            "GitHub".into(),
            "alice".into(),
            "pw".into(),
            "https://github.com".into(),
            "".into(),
            "dev-1".into(),
        )]);
        let blob = envelope::encrypt(&key, &params, salt, &vault.canonical_json());
        let new_blob = reencrypt_vault(&key, &blob, &vault).unwrap();
        let header = envelope::parse_header(&new_blob).unwrap();
        assert_eq!(header.params, params);
        assert_eq!(header.salt, salt);
        assert_eq!(
            envelope::decrypt(&key, &new_blob).unwrap(),
            vault.canonical_json()
        );
    }

    #[test]
    fn crud_mutation_survives_reencrypt_roundtrip() {
        let key = [0x11; 32];
        let params = KdfParams {
            mem_kib: 1024,
            iterations: 1,
            parallelism: 1,
        };
        let salt = [0x42; 32];
        let mut vault = Vault::empty();
        let blob = envelope::encrypt(&key, &params, salt, &vault.canonical_json());
        create_entry(&mut vault, &input("GitHub", "alice"), "dev-1");
        let new_blob = reencrypt_vault(&key, &blob, &vault).unwrap();
        let plaintext = envelope::decrypt(&key, &new_blob).unwrap();
        let decrypted: Vault = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(decrypted.canonical_json(), vault.canonical_json());
        assert_eq!(list_entries(&decrypted).len(), 1);
    }

    // ---- copy timer decision ----

    #[test]
    fn should_clear_clipboard_true_when_value_unchanged() {
        assert!(should_clear_clipboard("secret", "secret"));
    }

    #[test]
    fn should_clear_clipboard_false_when_value_changed() {
        assert!(!should_clear_clipboard("secret", "something else"));
        assert!(!should_clear_clipboard("secret", ""));
    }

    // ---- sync config store ----

    #[test]
    fn save_then_load_sync_config_roundtrip() {
        let dir = tempdir().unwrap();
        let config = SyncConfig {
            remote_url: "https://github.com/acme/passm.git".into(),
        };
        save_sync_config(dir.path(), &config).unwrap();
        assert_eq!(load_sync_config(dir.path()).unwrap(), Some(config));
    }

    #[test]
    fn load_missing_sync_config_returns_none() {
        let dir = tempdir().unwrap();
        assert_eq!(load_sync_config(dir.path()).unwrap(), None);
    }

    // ---- Chinese error messages ----

    #[test]
    fn chinese_error_messages() {
        assert_eq!(CommandError::Locked.to_string(), "请先解锁");
        assert_eq!(CommandError::WrongPassword.to_string(), "密码错误");
        assert_eq!(CommandError::SyncNotConfigured.to_string(), "同步未配置");
        assert_eq!(CommandError::EntryNotFound.to_string(), "条目不存在");
        assert_eq!(
            CommandError::VaultFileMissing.to_string(),
            "保险库文件不存在"
        );
        assert_eq!(
            CommandError::Sync(SyncError::PatMissing).to_string(),
            "同步未配置"
        );
    }
}
