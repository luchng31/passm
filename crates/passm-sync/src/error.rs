//! Typed error surface for passm-sync.

use passm_crypto::envelope::EnvelopeError;
use std::fmt;
use std::io;

/// Errors surfaced by the sync crate.
#[derive(Debug)]
pub enum SyncError {
    /// The PAT store is empty; a PAT is required for this operation.
    PatMissing,
    /// Filesystem error (device_id persistence).
    Io(io::Error),
    /// Platform keyring failure.
    KeyringError(String),
    /// The existing device_id file does not contain a valid UUIDv4.
    InvalidDeviceId,
    /// git2/libgit2 failure (clone/fetch/push/commit/read errors).
    Git(git2::Error),
    /// The remote rejected a push because it was not a fast-forward.
    /// T10 trigger: re-fetch + merge + retry.
    NonFastForward,
    /// A repository operation was attempted before `git_repo::ensure_clone`.
    RepoNotInitialized,
    /// A PASSM1 envelope could not be parsed or decrypted (wrong key or
    /// tampered blob). A remote decrypt failure never clobbers the local vault.
    Envelope(EnvelopeError),
    /// The decrypted vault plaintext is not valid vault JSON.
    Json(serde_json::Error),
    /// The local working tree has no `vault.enc` to merge.
    VaultMissing,
    /// The sync loop retried [`crate::sync_engine::MAX_ATTEMPTS`] times and
    /// the remote kept advancing (someone else keeps pushing concurrently).
    SyncRetryExhausted,
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::PatMissing => write!(f, "PAT is not configured"),
            SyncError::Io(e) => write!(f, "I/O error: {e}"),
            SyncError::KeyringError(msg) => write!(f, "keyring error: {msg}"),
            SyncError::InvalidDeviceId => {
                write!(f, "device_id file does not contain a valid UUIDv4")
            }
            SyncError::Git(e) => write!(f, "git error: {e}"),
            SyncError::NonFastForward => write!(f, "push rejected: not a fast-forward"),
            SyncError::RepoNotInitialized => {
                write!(
                    f,
                    "repository not initialized; call git_repo::ensure_clone first"
                )
            }
            SyncError::Envelope(e) => write!(f, "envelope error: {e}"),
            SyncError::Json(e) => write!(f, "vault JSON error: {e}"),
            SyncError::VaultMissing => {
                write!(f, "vault.enc is missing from the local working tree")
            }
            SyncError::SyncRetryExhausted => {
                write!(f, "sync retried 3 times and the remote kept advancing")
            }
        }
    }
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SyncError::Io(e) => Some(e),
            SyncError::Git(e) => Some(e),
            SyncError::Envelope(e) => Some(e),
            SyncError::Json(e) => Some(e),
            _ => None,
        }
    }
}
