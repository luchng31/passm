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
        }
    }
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SyncError::Io(e) => Some(e),
            _ => None,
        }
    }
}