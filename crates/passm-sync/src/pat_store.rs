//! PAT storage abstraction over the platform keyring.

use crate::error::SyncError;
use std::sync::Mutex;

/// Service/user pair identifying the PAT credential in the platform keyring.
const KEYRING_SERVICE: &str = "passm";
const KEYRING_USER: &str = "github-pat";

/// Abstraction over PAT storage so sync logic is testable without a keyring.
pub trait PatStore: Send + Sync {
    /// Returns the stored PAT, or `None` if none is configured.
    fn get(&self) -> Result<Option<String>, SyncError>;
    /// Stores the PAT.
    fn set(&self, pat: &str) -> Result<(), SyncError>;
    /// Removes the stored PAT.
    fn delete(&self) -> Result<(), SyncError>;
}

/// PAT store backed by the platform keyring.
///
/// Desktop (Windows/macOS/Linux): the keyring 4.x `v1` facade
/// (`keyring::Entry`), which registers the platform-native store in
/// `set_credential_store`.
///
/// Android: keyring 4.x's `v1` facade has no Android branch and returns
/// `NoDefaultStore`, so we register `android-native-keyring-store` directly
/// with `keyring_core` and use `keyring_core::Entry`. The store requires the
/// NDK context initialized via `io.crates.keyring.Keyring.initializeNdkContext`
/// in MainActivity.onCreate (wired in T16).
///
/// Windows uses `windows-native-keyring-store` (Credential Manager).
pub struct KeyringPatStore {
    entry: KeyringEntry,
}

/// Entry type: the keyring 4.x `v1` facade is desktop-only (its
/// `set_credential_store` has no Android branch and returns
/// `NoDefaultStore`), so Android registers the native store with
/// keyring-core directly and uses `keyring_core::Entry`.
#[cfg(target_os = "android")]
type KeyringEntry = keyring_core::Entry;
#[cfg(not(target_os = "android"))]
type KeyringEntry = keyring::Entry;

impl KeyringPatStore {
    pub fn new() -> Result<Self, SyncError> {
        #[cfg(target_os = "android")]
        {
            // SAFETY: `Store::new()` requires the NDK context initialized by
            // `Keyring.initializeNdkContext` in MainActivity.onCreate; it is
            // idempotent (vault lookup reuses an existing vault) so re-registering
            // on every call is safe and heals stale state. `set_default_store`
            // swaps an Arc in keyring-core's global RwLock.
            let store = android_native_keyring_store::Store::new()
                .map_err(|e| SyncError::KeyringError(e.to_string()))?;
            keyring_core::set_default_store(store);
        }
        let entry = KeyringEntry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| SyncError::KeyringError(e.to_string()))?;
        Ok(Self { entry })
    }
}

impl PatStore for KeyringPatStore {
    fn get(&self) -> Result<Option<String>, SyncError> {
        match self.entry.get_password() {
            Ok(pat) => Ok(Some(pat)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(SyncError::KeyringError(e.to_string())),
        }
    }

    fn set(&self, pat: &str) -> Result<(), SyncError> {
        self.entry
            .set_password(pat)
            .map_err(|e| SyncError::KeyringError(e.to_string()))
    }

    fn delete(&self) -> Result<(), SyncError> {
        self.entry
            .delete_credential()
            .map_err(|e| SyncError::KeyringError(e.to_string()))
    }
}

/// In-memory PAT store for tests.
#[derive(Default)]
pub struct MockPatStore {
    pat: Mutex<Option<String>>,
}

impl PatStore for MockPatStore {
    fn get(&self) -> Result<Option<String>, SyncError> {
        let guard = self
            .pat
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(guard.clone())
    }

    fn set(&self, pat: &str) -> Result<(), SyncError> {
        let mut guard = self
            .pat
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(pat.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), SyncError> {
        let mut guard = self
            .pat
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_empty_returns_none() {
        let store = MockPatStore::default();
        assert_eq!(store.get().unwrap(), None);
    }

    #[test]
    fn set_then_get_returns_pat() {
        let store = MockPatStore::default();
        store.set("ghp_example123").unwrap();
        assert_eq!(store.get().unwrap(), Some("ghp_example123".to_string()));
    }

    #[test]
    fn delete_clears_pat() {
        let store = MockPatStore::default();
        store.set("ghp_example123").unwrap();
        store.delete().unwrap();
        assert_eq!(store.get().unwrap(), None);
    }

    #[test]
    fn set_overwrites_previous_pat() {
        let store = MockPatStore::default();
        store.set("ghp_first").unwrap();
        store.set("ghp_second").unwrap();
        assert_eq!(store.get().unwrap(), Some("ghp_second".to_string()));
    }

    #[test]
    fn delete_when_empty_is_ok() {
        let store = MockPatStore::default();
        store.delete().unwrap();
        assert_eq!(store.get().unwrap(), None);
    }

    #[test]
    #[ignore = "requires a real desktop keyring backend (CI is headless)"]
    fn keyring_roundtrip_on_real_backend() {
        let store = KeyringPatStore::new().unwrap();
        store.set("ghp_ci_probe").unwrap();
        assert_eq!(store.get().unwrap(), Some("ghp_ci_probe".to_string()));
        store.delete().unwrap();
        assert_eq!(store.get().unwrap(), None);
    }
}
