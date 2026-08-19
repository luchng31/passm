//! passm-vault: vault data model (entries, metadata), serialization (serde), ids (uuid).

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
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
    ///
    /// # Errors
    /// Returns a [`serde_json::Error`] if serialization fails; with the plain
    /// `Serialize` derive this is impossible, so callers can treat it as an
    /// internal error.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut sorted = self.entries.clone();
        sorted.sort_by_key(|e| e.id);
        let vault = Vault { entries: sorted };
        serde_json::to_vec(&vault)
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Deterministic total order over two entries with the same id.
/// Higher version wins; equal version + one tombstone -> tombstone wins
/// (no-resurrect); equal version + both live -> lexicographically higher
/// device_id wins. Remaining fields break full ties so the winner is
/// independent of argument order (commutativity).
fn entry_cmp(a: &Entry, b: &Entry) -> Ordering {
    a.version
        .cmp(&b.version)
        .then_with(|| a.deleted.cmp(&b.deleted))
        .then_with(|| a.device_id.cmp(&b.device_id))
        .then_with(|| a.title.cmp(&b.title))
        .then_with(|| a.username.cmp(&b.username))
        .then_with(|| a.password.cmp(&b.password))
        .then_with(|| a.url.cmp(&b.url))
        .then_with(|| a.notes.cmp(&b.notes))
        .then_with(|| a.created_at.cmp(&b.created_at))
        .then_with(|| a.updated_at.cmp(&b.updated_at))
}

/// Commutative + idempotent merge of two vaults (pure, no I/O).
/// Per entry id: higher version wins; equal version + one tombstone ->
/// tombstone wins (no-resurrect); equal version + both live ->
/// lexicographically higher device_id wins. Entries present on only one
/// side are taken as-is. Result entries are sorted by id (canonical).
pub fn merge(local: &Vault, remote: &Vault) -> Vault {
    let mut by_id: HashMap<Uuid, Entry> = HashMap::new();
    for entry in local.entries.iter().chain(remote.entries.iter()) {
        by_id
            .entry(entry.id)
            .and_modify(|existing| {
                if entry_cmp(entry, existing) == Ordering::Greater {
                    *existing = entry.clone();
                }
            })
            .or_insert_with(|| entry.clone());
    }
    let mut entries: Vec<Entry> = by_id.into_values().collect();
    entries.sort_by_key(|e| e.id);
    Vault { entries }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::Rng;
    use rand::SeedableRng;

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
        assert_eq!(
            vault_abc.canonical_json().unwrap(),
            vault_cba.canonical_json().unwrap()
        );
    }

    #[test]
    fn canonical_json_sorts_entries_by_id() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let vault = Vault {
            entries: vec![entry_with_id(c), entry_with_id(a), entry_with_id(b)],
        };
        let bytes = vault.canonical_json().unwrap();
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
        assert_eq!(vault.canonical_json().unwrap(), br#"{"entries":[]}"#);
    }

    fn entry_with_fields(id: Uuid, version: u64, device_id: &str, deleted: bool) -> Entry {
        Entry {
            id,
            title: "title".into(),
            username: "user".into(),
            password: "pass".into(),
            url: "https://example.com".into(),
            notes: "notes".into(),
            version,
            device_id: device_id.into(),
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
            deleted,
        }
    }

    fn random_entry(rng: &mut StdRng, id: Uuid) -> Entry {
        Entry {
            id,
            title: format!("title-{}", rng.gen_range(0..10)),
            username: format!("user-{}", rng.gen_range(0..10)),
            password: format!("pass-{}", rng.gen_range(0..10)),
            url: format!("https://example.com/{}", rng.gen_range(0..10)),
            notes: format!("notes-{}", rng.gen_range(0..10)),
            version: rng.gen_range(1..=5),
            device_id: format!("dev-{}", rng.gen_range(0..3)),
            created_at: rng.gen_range(1_700_000_000..1_700_000_100),
            updated_at: rng.gen_range(1_700_000_000..1_700_000_100),
            deleted: rng.gen_bool(0.3),
        }
    }

    fn random_vault(rng: &mut StdRng, id_pool: &[Uuid]) -> Vault {
        let n = rng.gen_range(0..=id_pool.len());
        let entries = id_pool[..n]
            .iter()
            .map(|id| random_entry(rng, *id))
            .collect();
        Vault { entries }
    }

    #[test]
    fn merge_disjoint_entries_from_both_sides() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();
        let local = Vault {
            entries: vec![
                entry_with_fields(a, 1, "dev-a", false),
                entry_with_fields(b, 1, "dev-a", false),
            ],
        };
        let remote = Vault {
            entries: vec![
                entry_with_fields(c, 1, "dev-b", false),
                entry_with_fields(d, 1, "dev-b", false),
            ],
        };
        let merged = merge(&local, &remote);
        let mut got = ids(&merged.entries);
        got.sort();
        let mut want = vec![a, b, c, d];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn higher_version_wins_both_directions() {
        let id = Uuid::new_v4();
        let local = Vault {
            entries: vec![entry_with_fields(id, 1, "dev-a", false)],
        };
        let remote = Vault {
            entries: vec![entry_with_fields(id, 3, "dev-b", false)],
        };
        for merged in [merge(&local, &remote), merge(&remote, &local)] {
            assert_eq!(merged.entries.len(), 1);
            assert_eq!(merged.entries[0].version, 3);
            assert_eq!(merged.entries[0].device_id, "dev-b");
        }
    }

    #[test]
    fn equal_version_live_tiebreak_by_device_id() {
        let id = Uuid::new_v4();
        let local = Vault {
            entries: vec![entry_with_fields(id, 2, "dev-a", false)],
        };
        let remote = Vault {
            entries: vec![entry_with_fields(id, 2, "dev-b", false)],
        };
        for merged in [merge(&local, &remote), merge(&remote, &local)] {
            assert_eq!(merged.entries.len(), 1);
            assert_eq!(merged.entries[0].device_id, "dev-b");
        }
    }

    #[test]
    fn tombstone_wins_equal_version_no_resurrect() {
        let id = Uuid::new_v4();
        let local = Vault {
            entries: vec![entry_with_fields(id, 2, "dev-a", false)],
        };
        let remote = Vault {
            entries: vec![entry_with_fields(id, 2, "dev-b", true)],
        };
        for merged in [merge(&local, &remote), merge(&remote, &local)] {
            assert_eq!(merged.entries.len(), 1);
            assert!(merged.entries[0].deleted);
        }
    }

    #[test]
    fn deleted_higher_version_does_not_resurrect() {
        let id = Uuid::new_v4();
        let local = Vault {
            entries: vec![entry_with_fields(id, 5, "dev-a", true)],
        };
        let remote = Vault {
            entries: vec![entry_with_fields(id, 3, "dev-b", false)],
        };
        for merged in [merge(&local, &remote), merge(&remote, &local)] {
            assert_eq!(merged.entries.len(), 1);
            assert!(merged.entries[0].deleted);
            assert_eq!(merged.entries[0].version, 5);
        }
    }

    #[test]
    fn merge_is_commutative_and_idempotent_randomized() {
        let mut rng = StdRng::seed_from_u64(0x5EED_CAFE);
        let id_pool: Vec<Uuid> = (0..8).map(|_| Uuid::new_v4()).collect();
        for _ in 0..50 {
            let local = random_vault(&mut rng, &id_pool);
            let remote = random_vault(&mut rng, &id_pool);
            let ab = merge(&local, &remote);
            let ba = merge(&remote, &local);
            assert_eq!(ab.canonical_json().unwrap(), ba.canonical_json().unwrap());
            assert_eq!(
                merge(&ab, &remote).canonical_json().unwrap(),
                ab.canonical_json().unwrap()
            );
            assert_eq!(
                merge(&ab, &local).canonical_json().unwrap(),
                ab.canonical_json().unwrap()
            );
        }
    }
}
