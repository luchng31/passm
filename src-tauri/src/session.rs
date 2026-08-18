//! Session state and pure lock/unlock/auto-lock logic.
//!
//! Everything here is Tauri-runtime-free so the state transitions are
//! unit-testable with plain `cargo test -p passm-app`. The Tauri layer
//! (`lib.rs`) only wraps these pure functions in commands/tray/timer.

use passm_vault::Vault;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

/// In-memory session state, managed via `tauri::State<'_, Mutex<SessionState>>`.
///
/// The decrypted vault key and vault live here ONLY while the session is
/// unlocked; `lock_session` drops both (the key is zeroized on drop via
/// `Zeroizing`). Nothing here is ever persisted.
#[derive(Default)]
pub struct SessionState {
    /// Decrypted vault key; `None` while locked. Zeroized on drop/lock.
    pub vault_key: Option<Zeroizing<[u8; 32]>>,
    /// Decrypted vault; `None` while locked.
    pub vault: Option<Vault>,
    /// Stable per-device identifier (persisted under `app_data_dir` by T12).
    pub device_id: String,
    /// Unix seconds of the last unlock/activity; `None` while locked.
    pub unlocked_at: Option<u64>,
}

/// Injectable clock so auto-lock timing is testable without wall-clock sleeps.
pub trait Clock {
    /// Current time as unix seconds.
    fn now_unix(&self) -> u64;
}

/// Real wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Unlock the session: store the key (zeroized on drop), the decrypted vault,
/// and stamp the unlock time. Pure — no I/O, no Tauri runtime.
pub fn unlock_session(state: &mut SessionState, vault_key: [u8; 32], vault: Vault, now: u64) {
    state.vault_key = Some(Zeroizing::new(vault_key));
    state.vault = Some(vault);
    state.unlocked_at = Some(now);
}

/// Lock the session: dropping the `Zeroizing` key zeroizes the key bytes,
/// dropping the vault removes the decrypted plaintext from RAM, and the
/// unlock timestamp is cleared. Pure — no I/O, no Tauri runtime.
pub fn lock_session(state: &mut SessionState) {
    state.vault_key = None;
    state.vault = None;
    state.unlocked_at = None;
}

/// True when the session is unlocked and `unlocked_at + timeout_secs <= now`.
/// A locked session never auto-locks. Pure — no I/O, no Tauri runtime.
pub fn should_auto_lock(state: &SessionState, now: u64, timeout_secs: u64) -> bool {
    match state.unlocked_at {
        Some(unlocked_at) => unlocked_at.saturating_add(timeout_secs) <= now,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session unlocked at t=100 with a fixed key and an empty vault.
    fn unlocked_state() -> SessionState {
        let mut state = SessionState {
            device_id: "dev-1".into(),
            ..Default::default()
        };
        unlock_session(&mut state, [0xAB; 32], Vault::empty(), 100);
        state
    }

    #[test]
    fn lock_session_zeroizes_key_and_drops_vault() {
        let mut state = unlocked_state();
        assert!(state.vault_key.is_some());
        assert!(state.vault.is_some());
        assert_eq!(state.unlocked_at, Some(100));

        lock_session(&mut state);

        assert!(state.vault_key.is_none());
        assert!(state.vault.is_none());
        assert!(state.unlocked_at.is_none());
    }

    #[test]
    fn should_auto_lock_false_before_timeout() {
        let state = unlocked_state();
        // unlocked_at=100, timeout=300 -> due at now >= 400, so 399 is not due.
        assert!(!should_auto_lock(&state, 100 + 299, 300));
    }

    #[test]
    fn should_auto_lock_true_at_or_after_timeout() {
        let state = unlocked_state();
        assert!(should_auto_lock(&state, 100 + 300, 300));
        assert!(should_auto_lock(&state, 100 + 301, 300));
    }

    #[test]
    fn should_auto_lock_false_when_locked() {
        let mut state = unlocked_state();
        lock_session(&mut state);
        assert!(!should_auto_lock(&state, 100 + 999_999, 300));
    }

    #[test]
    fn unlock_session_sets_key_vault_and_timestamp() {
        let mut state = SessionState::default();
        unlock_session(&mut state, [0xCD; 32], Vault::empty(), 42);
        assert!(state.vault_key.is_some());
        assert!(state.vault.is_some());
        assert_eq!(state.unlocked_at, Some(42));
    }
}
