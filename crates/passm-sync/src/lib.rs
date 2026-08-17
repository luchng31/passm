//! passm-sync: git-backed sync (git2), PAT storage (keyring), device identity.

pub mod device_id;
pub mod error;
pub mod pat_store;

pub use device_id::{load, load_or_create};
pub use error::SyncError;
pub use pat_store::{KeyringPatStore, MockPatStore, PatStore};