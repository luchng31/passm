//! Typed error surface for passm-sync.

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
                write!(f, "repository not initialized; call git_repo::ensure_clone first")
            }
        }
    }
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SyncError::Io(e) => Some(e),
            SyncError::Git(e) => Some(e),
            _ => None,
        }
    }
}