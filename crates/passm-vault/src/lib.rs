//! passm-vault: vault data model (entries, metadata), serialization (serde), ids (uuid).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single password entry; `deleted` acts as a tombstone for sync/merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: Uuid,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub version: u64,
    pub device_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
}

impl Entry {
    /// New entry: version 1, timestamps set to now, not deleted.
    pub fn new(
        title: String,
        username: String,
        password: String,
        url: String,
        notes: String,
        device_id: String,
    ) -> Self {
        let now = now_unix_secs();
        Self {
            id: Uuid::new_v4(),
            title,
            username,
            password,
            url,
            notes,
            version: 1,
            device_id,
            created_at: now,
            updated_at: now,
            deleted: false,
        }
    }

    /// Bump the version and refresh `updated_at` (edit operation).
    pub fn bump(&mut self) {
        self.version += 1;
        self.updated_at = now_unix_secs();
    }

    /// Mark this entry as deleted (tombstone).
    pub fn mark_deleted(&mut self) {
        self.deleted = true;
    }
}

/// A vault is a set of entries; canonical serialization sorts them by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    pub entries: Vec<Entry>,
}

impl Vault {
    /// Empty vault for first-run bootstrap: `{"entries":[]}`.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Canonical JSON: entries sorted by id for byte-stable output.
    pub fn canonical_json(&self) -> Vec<u8> {
        let mut sorted = self.entries.clone();
        sorted.sort_by_key(|e| e.id);
        let vault = Vault { entries: sorted };
        serde_json::to_vec(&vault).expect("Vault serialization is infallible")
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_with_id(id: Uuid) -> Entry {
        Entry {
            id,
            title: "title".into(),
            username: "user".into(),
            password: "pass".into(),
            url: "https://example.com".into(),
            notes: "notes".into(),
            version: 1,
            device_id: "dev-1".into(),
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
            deleted: false,
        }
    }

    fn ids(entries: &[Entry]) -> Vec<Uuid> {
        entries.iter().map(|e| e.id).collect()
    }

    #[test]
    fn serde_roundtrip_preserves_entry() {
        let entry = Entry::new(
            "title".into(),
            "user".into(),
            "pass".into(),
            "https://example.com".into(),
            "notes".into(),
            "dev-1".into(),
        );
        let bytes = serde_json::to_vec(&entry).unwrap();
        let back: Entry = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(entry, back);
    }

    #[test]
    fn serde_roundtrip_preserves_vault() {
        let vault = Vault {
            entries: vec![entry_with_id(Uuid::new_v4()), entry_with_id(Uuid::new_v4())],
        };
        let bytes = serde_json::to_vec(&vault).unwrap();
        let back: Vault = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(vault, back);
    }

    #[test]
    fn canonical_json_is_byte_stable_across_entry_order() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let vault_abc = Vault {
            entries: vec![entry_with_id(a), entry_with_id(b), entry_with_id(c)],
        };
        let vault_cba = Vault {
            entries: vec![entry_with_id(c), entry_with_id(b), entry_with_id(a)],
        };
        assert_eq!(vault_abc.canonical_json(), vault_cba.canonical_json());
    }

    #[test]
    fn canonical_json_sorts_entries_by_id() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let vault = Vault {
            entries: vec![entry_with_id(c), entry_with_id(a), entry_with_id(b)],
        };
        let bytes = vault.canonical_json();
        let parsed: Vault = serde_json::from_slice(&bytes).unwrap();
        let mut sorted = vec![a, b, c];
        sorted.sort();
        assert_eq!(ids(&parsed.entries), sorted);
    }

    #[test]
    fn new_sets_defaults() {
        let entry = Entry::new(
            "title".into(),
            "user".into(),
            "pass".into(),
            "https://example.com".into(),
            "notes".into(),
            "dev-1".into(),
        );
        assert_eq!(entry.version, 1);
        assert!(!entry.deleted);
        assert!(entry.created_at > 0);
        assert!(entry.updated_at > 0);
        assert_eq!(entry.created_at, entry.updated_at);
    }

    #[test]
    fn bump_increments_version_and_refreshes_updated_at() {
        let mut entry = entry_with_id(Uuid::new_v4());
        entry.updated_at = 1_000_000;
        entry.bump();
        assert_eq!(entry.version, 2);
        assert!(entry.updated_at > 1_000_000);
    }

    #[test]
    fn mark_deleted_sets_tombstone() {
        let mut entry = entry_with_id(Uuid::new_v4());
        assert!(!entry.deleted);
        entry.mark_deleted();
        assert!(entry.deleted);
    }

    #[test]
    fn empty_vault_serializes_to_empty_entries() {
        let vault = Vault::empty();
        assert_eq!(vault.canonical_json(), br#"{"entries":[]}"#);
    }
}