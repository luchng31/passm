//! passm-sync: git-backed sync (git2), PAT storage (keyring), device identity.

pub mod device_id;
pub mod error;
pub mod git_repo;
pub mod pat_store;

pub use device_id::{load, load_or_create};
pub use error::SyncError;
pub use git_repo::{
    checkout_vault_file, commit_vault_file, current_head, ensure_clone, fetch, is_fast_forward,
    push, remote_head, write_vault_file,
};
pub use pat_store::{KeyringPatStore, MockPatStore, PatStore};