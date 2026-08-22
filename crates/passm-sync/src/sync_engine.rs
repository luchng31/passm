//! Sync engine: converge two devices' vaults through a shared remote.
//!
//! Drives the git plumbing in [`crate::git_repo`] and the commutative vault
//! merge in `passm_vault`. The vault is a single encrypted blob (`vault.enc`)
//! in a private git repository; sync fast-forwards when possible and merges
//! (with a crash-safe backup first) when the two histories diverge.

use crate::error::SyncError;
use crate::git_repo;
use passm_crypto::envelope;
use passm_vault::Vault;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Vault blob filename inside the sync repository.
pub const VAULT_FILE: &str = "vault.enc";
/// Backup directory inside the sync repository (untracked).
const BACKUP_DIR: &str = "backups";
/// Maximum number of pre-merge backups retained.
const MAX_BACKUPS: usize = 20;
/// Maximum sync attempts before giving up on a non-fast-forward race.
const MAX_ATTEMPTS: usize = 3;

/// Outcome of a sync run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    /// A local commit was pushed to the remote.
    pub pushed: bool,
    /// The remote vault was pulled into the local working tree.
    pub pulled: bool,
    /// A conflict merge was performed (backup + merge + re-encrypt + push).
    pub merged: bool,
    /// Path of the pre-merge backup created during a conflict merge.
    pub backup_created: Option<PathBuf>,
}

impl SyncOutcome {
    fn noop() -> Self {
        Self {
            pushed: false,
            pulled: false,
            merged: false,
            backup_created: None,
        }
    }
}

/// Test seam: runs before every push so tests can simulate the remote
/// advancing mid-merge (the non-fast-forward race). The hook is cloned out of
/// the mutex before being invoked, so it may safely reinstall/clear itself.
#[cfg(test)]
static PRE_PUSH: std::sync::Mutex<Option<std::sync::Arc<dyn Fn() + Send + Sync>>> =
    std::sync::Mutex::new(None);

/// Converges the local vault with the remote, retrying up to [`MAX_ATTEMPTS`]
/// times when the remote advances mid-merge.
///
/// The repository must already be cloned via [`git_repo::ensure_clone`]; the
/// app layer owns the remote URL and repo directory.
pub fn sync(pat: &str, vault_key: &[u8; 32], device_id: &str) -> Result<SyncOutcome, SyncError> {
    for _attempt in 0..MAX_ATTEMPTS {
        match sync_once(pat, vault_key, device_id) {
            Ok(outcome) => return Ok(outcome),
            Err(SyncError::NonFastForward) => {
                // Someone else pushed while we were merging: re-fetch and retry.
                git_repo::fetch(pat)?;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(SyncError::SyncRetryExhausted)
}

/// One fetch + decide + act pass of the sync loop.
fn sync_once(pat: &str, vault_key: &[u8; 32], device_id: &str) -> Result<SyncOutcome, SyncError> {
    git_repo::fetch(pat)?;
    let local = git_repo::current_head()?;
    let remote = git_repo::remote_head()?;

    match (local, remote) {
        (None, None) => Ok(SyncOutcome::noop()),
        (Some(l), Some(r)) if l == r => Ok(SyncOutcome::noop()),
        (None, Some(_)) => pull_first(vault_key),
        (Some(_), None) => {
            push_with_hook(pat)?;
            Ok(SyncOutcome {
                pushed: true,
                ..SyncOutcome::noop()
            })
        }
        (Some(l), Some(r)) if git_repo::is_fast_forward(&l, &r) => {
            push_with_hook(pat)?;
            Ok(SyncOutcome {
                pushed: true,
                ..SyncOutcome::noop()
            })
        }
        (Some(l), Some(r)) if git_repo::is_fast_forward(&r, &l) => {
            pull_fast_forward(vault_key)?;
            Ok(SyncOutcome {
                pulled: true,
                ..SyncOutcome::noop()
            })
        }
        (Some(_), Some(r)) => conflict_merge(pat, vault_key, device_id, r),
    }
}

/// First pull: adopt the remote vault into an empty local repository.
fn pull_first(vault_key: &[u8; 32]) -> Result<SyncOutcome, SyncError> {
    let remote_blob = read_remote_vault()?;
    // Validate the remote vault decrypts with our key before adopting it.
    envelope::decrypt(vault_key, &remote_blob).map_err(SyncError::Envelope)?;
    git_repo::write_vault_file(VAULT_FILE, &remote_blob)?;
    git_repo::commit_vault_file(VAULT_FILE, "pull: initial")?;
    Ok(SyncOutcome {
        pulled: true,
        ..SyncOutcome::noop()
    })
}

/// Fast-forward pull: adopt the remote vault when it is strictly newer.
fn pull_fast_forward(vault_key: &[u8; 32]) -> Result<SyncOutcome, SyncError> {
    let remote_blob = read_remote_vault()?;
    // Validate the remote vault decrypts with our key before adopting it.
    envelope::decrypt(vault_key, &remote_blob).map_err(SyncError::Envelope)?;
    git_repo::write_vault_file(VAULT_FILE, &remote_blob)?;
    git_repo::commit_vault_file(VAULT_FILE, "pull: fast-forward")?;
    Ok(SyncOutcome {
        pulled: true,
        ..SyncOutcome::noop()
    })
}

/// Conflict merge: backup the local vault, merge with the remote, re-encrypt
/// (reusing the original header salt + params), commit, and push.
fn conflict_merge(
    pat: &str,
    vault_key: &[u8; 32],
    device_id: &str,
    remote_oid: git2::Oid,
) -> Result<SyncOutcome, SyncError> {
    // 1. Backup BEFORE merge: the current local vault is the pre-merge state.
    let local_blob = read_local_vault()?;
    let backup_path = backup_vault(&local_blob)?;

    // 2. Read the remote vault from the remote-tracking ref.
    let remote_blob = read_remote_vault()?;

    // 3. Decrypt both. A remote decrypt failure must NOT clobber the local
    //    vault — the backup is already safe and we return a typed error.
    let local_plaintext = envelope::decrypt(vault_key, &local_blob).map_err(SyncError::Envelope)?;
    let remote_plaintext =
        envelope::decrypt(vault_key, &remote_blob).map_err(SyncError::Envelope)?;
    let local_vault: Vault = serde_json::from_slice(&local_plaintext).map_err(SyncError::Json)?;
    let remote_vault: Vault = serde_json::from_slice(&remote_plaintext).map_err(SyncError::Json)?;

    // 4. Commutative + idempotent merge.
    let merged = passm_vault::merge(&local_vault, &remote_vault);

    // 5. Re-encrypt reusing the ORIGINAL header salt + params: the vault key
    //    is derived from password + header salt, so a fresh salt would change
    //    the key and the password could no longer unlock the vault (T6).
    let header = envelope::parse_header(&local_blob).map_err(SyncError::Envelope)?;
    let merged_blob = envelope::encrypt(
        vault_key,
        &header.params,
        header.salt,
        &merged.canonical_json().map_err(SyncError::Json)?,
    )
    .map_err(SyncError::Envelope)?;

    // 6. Write + commit (two-parent merge; see git_repo::commit_vault_merge).
    git_repo::write_vault_file(VAULT_FILE, &merged_blob)?;
    git_repo::commit_vault_merge(
        VAULT_FILE,
        &format!("merge: converge (device {device_id})"),
        remote_oid,
    )?;

    // 7. Push; a non-fast-forward rejection propagates to the retry loop.
    push_with_hook(pat)?;

    Ok(SyncOutcome {
        pushed: true,
        merged: true,
        backup_created: Some(backup_path),
        ..SyncOutcome::noop()
    })
}

/// Reads the local vault blob from the working tree.
fn read_local_vault() -> Result<Vec<u8>, SyncError> {
    match git_repo::checkout_vault_file(VAULT_FILE) {
        Ok(blob) => Ok(blob),
        Err(SyncError::Io(e)) if e.kind() == ErrorKind::NotFound => Err(SyncError::VaultMissing),
        Err(e) => Err(e),
    }
}

/// Reads the remote vault blob from the remote-tracking ref.
fn read_remote_vault() -> Result<Vec<u8>, SyncError> {
    let branch = git_repo::current_branch()?;
    let refname = format!("refs/remotes/origin/{branch}");
    git_repo::read_vault_from_ref(&refname)
}

/// Pushes, running the test-only pre-push hook first.
fn push_with_hook(pat: &str) -> Result<(), SyncError> {
    #[cfg(test)]
    run_pre_push_hook();
    git_repo::push(pat)
}

/// Runs the test-only pre-push hook, if one is installed.
#[cfg(test)]
fn run_pre_push_hook() {
    let hook = PRE_PUSH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook();
    }
}

/// Writes `blob` to `<repo>/backups/vault.<unix_ts>.enc` and prunes the
/// backup directory to the [`MAX_BACKUPS`] newest files.
pub fn backup_vault(blob: &[u8]) -> Result<PathBuf, SyncError> {
    let repo_dir = git_repo::repo_dir()?;
    let backups_dir = repo_dir.join(BACKUP_DIR);
    fs::create_dir_all(&backups_dir).map_err(SyncError::Io)?;
    let mut ts = now_unix_secs();
    let mut path = backups_dir.join(format!("vault.{ts}.enc"));
    // Same-second collisions (two merges within one second) must not
    // overwrite an existing backup: bump the timestamp until the name is free.
    while path.exists() {
        ts += 1;
        path = backups_dir.join(format!("vault.{ts}.enc"));
    }
    fs::write(&path, blob).map_err(SyncError::Io)?;
    prune_backups(&backups_dir)?;
    Ok(path)
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Deletes the oldest backups so at most [`MAX_BACKUPS`] remain.
fn prune_backups(backups_dir: &Path) -> Result<(), SyncError> {
    let mut backups: Vec<(u64, PathBuf)> = Vec::new();
    for entry in fs::read_dir(backups_dir).map_err(SyncError::Io)? {
        let entry = entry.map_err(SyncError::Io)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(ts) = parse_backup_ts(&name) {
            backups.push((ts, entry.path()));
        }
    }
    backups.sort_by_key(|(ts, _)| *ts);
    let excess = backups.len().saturating_sub(MAX_BACKUPS);
    for (_, path) in backups.iter().take(excess) {
        fs::remove_file(path).map_err(SyncError::Io)?;
    }
    Ok(())
}

/// Parses `vault.<ts>.enc` into the numeric timestamp.
fn parse_backup_ts(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("vault.")?;
    let ts = rest.strip_suffix(".enc")?;
    ts.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_repo::TEST_LOCK;
    use crate::{checkout_vault_file, commit_vault_file, ensure_clone, push, write_vault_file};
    use git2::{Commit, Oid, Repository, Signature};
    use passm_crypto::KdfParams;
    use passm_vault::Entry;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::tempdir;

    const PAT: &str = "ghp_test_dummy_pat";
    const KEY: [u8; 32] = [0x42; 32];
    const SALT: [u8; 32] = [0x42; 32];

    /// Cross-platform `file://` URI for a local path (see git_repo::tests).
    fn file_uri(dir: &Path) -> String {
        let normalized = dir.to_string_lossy().replace('\\', "/");
        let leading = if normalized.starts_with('/') { "" } else { "/" };
        format!("file://{leading}{normalized}")
    }

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn bare_remote(parent: &Path) -> (PathBuf, String) {
        let dir = parent.join("remote.git");
        Repository::init_bare(&dir).unwrap();
        let url = file_uri(&dir);
        (dir, url)
    }

    fn local_dir(parent: &Path) -> PathBuf {
        parent.join("local")
    }

    fn advance_remote(
        remote_dir: &Path,
        branch: &str,
        parent: Option<Oid>,
        contents: &[u8],
    ) -> Oid {
        let repo = Repository::open_bare(remote_dir).unwrap();
        let blob = repo.blob(contents).unwrap();
        let mut tb = repo.treebuilder(None).unwrap();
        tb.insert(VAULT_FILE, blob, 0o100644).unwrap();
        let tree_oid = tb.write().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = Signature::now("passm", "passm@local").unwrap();
        let parents: Vec<Commit> = parent
            .map(|o| repo.find_commit(o).unwrap())
            .into_iter()
            .collect();
        let parent_refs: Vec<&Commit> = parents.iter().collect();
        repo.commit(
            Some(&format!("refs/heads/{branch}")),
            &sig,
            &sig,
            "remote commit",
            &tree,
            &parent_refs,
        )
        .unwrap()
    }

    fn encrypt_vault(vault: &Vault) -> Vec<u8> {
        envelope::encrypt(
            &KEY,
            &KdfParams::default(),
            SALT,
            &vault.canonical_json().unwrap(),
        )
        .unwrap()
    }

    fn decrypt_vault(blob: &[u8]) -> Vault {
        let plaintext = envelope::decrypt(&KEY, blob).unwrap();
        serde_json::from_slice(&plaintext).unwrap()
    }

    fn entry(title: &str, device_id: &str) -> Entry {
        Entry::new(
            title.to_string(),
            "user".to_string(),
            "pass".to_string(),
            "https://example.com".to_string(),
            "notes".to_string(),
            device_id.to_string(),
        )
    }

    fn remote_vault(remote_dir: &Path) -> Vault {
        let repo = Repository::open_bare(remote_dir).unwrap();
        let oid = repo
            .find_reference("refs/heads/master")
            .unwrap()
            .target()
            .unwrap();
        let commit = repo.find_commit(oid).unwrap();
        let tree = commit.tree().unwrap();
        let entry = tree.get_path(Path::new(VAULT_FILE)).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        decrypt_vault(blob.content())
    }

    fn titles(vault: &Vault) -> Vec<String> {
        let mut t: Vec<String> = vault.entries.iter().map(|e| e.title.clone()).collect();
        t.sort_unstable();
        t
    }

    #[test]
    fn first_push_succeeds_on_empty_remote() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        let vault = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&vault)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();

        let outcome = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(outcome.pushed);
        assert!(!outcome.pulled);
        assert!(!outcome.merged);
        assert!(outcome.backup_created.is_none());
        assert_eq!(remote_vault(&remote_dir), vault);
    }

    #[test]
    fn first_pull_succeeds_on_empty_local() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());

        // Device B clones the empty remote first, so its local repo is empty.
        let local_b = tmp.path().join("device-b");
        ensure_clone(&url, &local_b, PAT).unwrap();

        // Device A pushes a vault.
        let local_a = tmp.path().join("device-a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let vault = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&vault)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        // Device B syncs: local empty, remote has a vault -> first pull.
        ensure_clone(&url, &local_b, PAT).unwrap();
        let outcome = sync(PAT, &KEY, "dev-b").unwrap();
        assert!(outcome.pulled);
        assert!(!outcome.pushed);
        assert!(!outcome.merged);
        assert!(outcome.backup_created.is_none());

        let local_blob = checkout_vault_file(VAULT_FILE).unwrap();
        assert_eq!(decrypt_vault(&local_blob), vault);
    }

    #[test]
    fn converged_sync_is_noop() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        let vault = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&vault)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        let outcome = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(!outcome.pushed);
        assert!(!outcome.pulled);
        assert!(!outcome.merged);
        assert!(outcome.backup_created.is_none());
    }

    #[test]
    fn fast_forward_push_advances_remote() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        let v1 = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v1)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        let mut v2 = v1.clone();
        v2.entries.push(entry("e2", "dev-a"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v2)).unwrap();
        commit_vault_file(VAULT_FILE, "v2").unwrap();

        let outcome = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(outcome.pushed);
        assert!(!outcome.merged);
        assert!(outcome.backup_created.is_none());
        assert_eq!(
            remote_vault(&remote_dir).canonical_json().unwrap(),
            v2.canonical_json().unwrap()
        );
    }

    #[test]
    fn fast_forward_pull_adopts_remote() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());

        // Device A pushes v1.
        let local_a = tmp.path().join("device-a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let v1 = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v1)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        // Device B clones while the remote has v1.
        let local_b = tmp.path().join("device-b");
        ensure_clone(&url, &local_b, PAT).unwrap();

        // Device A pushes v2.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let mut v2 = v1.clone();
        v2.entries.push(entry("e2", "dev-a"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v2)).unwrap();
        commit_vault_file(VAULT_FILE, "v2").unwrap();
        push(PAT).unwrap();

        // Device B syncs: local v1, remote v2 -> fast-forward pull.
        ensure_clone(&url, &local_b, PAT).unwrap();
        let outcome = sync(PAT, &KEY, "dev-b").unwrap();
        assert!(outcome.pulled);
        assert!(!outcome.pushed);
        assert!(!outcome.merged);
        assert!(outcome.backup_created.is_none());

        let local_blob = checkout_vault_file(VAULT_FILE).unwrap();
        assert_eq!(
            decrypt_vault(&local_blob).canonical_json().unwrap(),
            v2.canonical_json().unwrap()
        );
    }

    #[test]
    fn conflict_merge_converges_and_creates_backup() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());

        // Device A: push v1.
        let local_a = tmp.path().join("device-a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let e1 = entry("e1", "dev-a");
        let v1 = Vault {
            entries: vec![e1.clone()],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v1)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        // Device B: clone (has v1), add e2 locally -> v2.
        let local_b = tmp.path().join("device-b");
        ensure_clone(&url, &local_b, PAT).unwrap();
        let e2 = entry("e2", "dev-b");
        let mut v2 = v1.clone();
        v2.entries.push(e2.clone());
        write_vault_file(VAULT_FILE, &encrypt_vault(&v2)).unwrap();
        commit_vault_file(VAULT_FILE, "v2").unwrap();

        // Device A: add e3 and push -> remote advances to v3.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let e3 = entry("e3", "dev-a");
        let mut v3 = v1.clone();
        v3.entries.push(e3.clone());
        write_vault_file(VAULT_FILE, &encrypt_vault(&v3)).unwrap();
        commit_vault_file(VAULT_FILE, "v3").unwrap();
        push(PAT).unwrap();

        // Device B syncs: local v2, remote v3 -> conflict merge.
        ensure_clone(&url, &local_b, PAT).unwrap();
        let outcome = sync(PAT, &KEY, "dev-b").unwrap();
        assert!(outcome.merged);
        assert!(outcome.pushed);
        assert!(!outcome.pulled);
        let backup = outcome.backup_created.expect("backup must be created");
        assert!(backup.exists());

        // Merged vault contains entries from both sides.
        let merged = decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap());
        assert_eq!(titles(&merged), vec!["e1", "e2", "e3"]);

        // Backup is the pre-merge local vault (v2) and decrypts with the key.
        let backup_blob = fs::read(&backup).unwrap();
        let backup_vault = decrypt_vault(&backup_blob);
        assert_eq!(titles(&backup_vault), vec!["e1", "e2"]);

        // Commutativity: device A syncs and converges to the same vault
        // (as a fast-forward pull, since its v3 is an ancestor of the merge).
        ensure_clone(&url, &local_a, PAT).unwrap();
        let outcome_a = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(outcome_a.pulled || outcome_a.merged);
        let merged_a = decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap());
        assert_eq!(
            merged_a.canonical_json().unwrap(),
            merged.canonical_json().unwrap()
        );
    }

    #[test]
    fn backup_retention_prunes_to_twenty() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();

        for i in 0..25 {
            backup_vault(format!("backup-{i}").as_bytes()).unwrap();
        }

        let backups_dir = local.join("backups");
        let mut files: Vec<String> = fs::read_dir(&backups_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files.len(), 20);

        // The 20 remaining backups are the newest: consecutive timestamps.
        files.sort();
        let first_ts: u64 = files
            .first()
            .unwrap()
            .strip_prefix("vault.")
            .unwrap()
            .strip_suffix(".enc")
            .unwrap()
            .parse()
            .unwrap();
        let last_ts: u64 = files
            .last()
            .unwrap()
            .strip_prefix("vault.")
            .unwrap()
            .strip_suffix(".enc")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(last_ts - first_ts, 19);
    }

    #[test]
    fn backup_file_is_valid_passm1_blob() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();

        let vault = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        let blob = encrypt_vault(&vault);
        let path = backup_vault(&blob).unwrap();
        assert!(path.exists());

        let read_back = fs::read(&path).unwrap();
        assert_eq!(read_back, blob);
        let plaintext = envelope::decrypt(&KEY, &read_back).unwrap();
        let parsed: Vault = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(parsed, vault);
    }

    #[test]
    fn sync_exhausts_retries_when_remote_keeps_advancing() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());

        // Device A: push v1.
        let local_a = tmp.path().join("device-a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let v1 = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v1)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        // Device B: clone, diverge locally (v2).
        let local_b = tmp.path().join("device-b");
        ensure_clone(&url, &local_b, PAT).unwrap();
        let mut v2 = v1.clone();
        v2.entries.push(entry("e2", "dev-b"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v2)).unwrap();
        commit_vault_file(VAULT_FILE, "v2").unwrap();

        // Remote advances to v3.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let mut v3 = v1.clone();
        v3.entries.push(entry("e3", "dev-a"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v3)).unwrap();
        commit_vault_file(VAULT_FILE, "v3").unwrap();
        push(PAT).unwrap();

        // Every push is rejected: the hook advances the remote before each
        // push, so the sync can never fast-forward.
        let remote_dir_hook = remote_dir.clone();
        let v1_hook = v1.clone();
        *PRE_PUSH.lock().unwrap() = Some(Arc::new(move || {
            let repo = Repository::open_bare(&remote_dir_hook).unwrap();
            let current = repo
                .find_reference("refs/heads/master")
                .unwrap()
                .target()
                .unwrap();
            advance_remote(
                &remote_dir_hook,
                "master",
                Some(current),
                &encrypt_vault(&v1_hook),
            );
        }));

        ensure_clone(&url, &local_b, PAT).unwrap();
        let result = sync(PAT, &KEY, "dev-b");
        assert!(matches!(result, Err(SyncError::SyncRetryExhausted)));

        *PRE_PUSH.lock().unwrap() = None;
    }

    #[test]
    fn sync_retries_and_converges_when_remote_advances_once() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());

        // Device A: push v1.
        let local_a = tmp.path().join("device-a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let v1 = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v1)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        push(PAT).unwrap();

        // Device B: clone, diverge locally (v2).
        let local_b = tmp.path().join("device-b");
        ensure_clone(&url, &local_b, PAT).unwrap();
        let mut v2 = v1.clone();
        v2.entries.push(entry("e2", "dev-b"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v2)).unwrap();
        commit_vault_file(VAULT_FILE, "v2").unwrap();

        // Remote advances to v3.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let mut v3 = v1.clone();
        v3.entries.push(entry("e3", "dev-a"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v3)).unwrap();
        commit_vault_file(VAULT_FILE, "v3").unwrap();
        push(PAT).unwrap();

        // The remote advances once (before the first push), then stops.
        let remote_dir_hook = remote_dir.clone();
        let v1_hook = v1.clone();
        *PRE_PUSH.lock().unwrap() = Some(Arc::new(move || {
            let repo = Repository::open_bare(&remote_dir_hook).unwrap();
            let current = repo
                .find_reference("refs/heads/master")
                .unwrap()
                .target()
                .unwrap();
            advance_remote(
                &remote_dir_hook,
                "master",
                Some(current),
                &encrypt_vault(&v1_hook),
            );
            *PRE_PUSH.lock().unwrap() = None;
        }));

        ensure_clone(&url, &local_b, PAT).unwrap();
        let outcome = sync(PAT, &KEY, "dev-b").unwrap();
        assert!(outcome.merged);
        assert!(outcome.pushed);

        // The remote now holds the converged vault.
        assert_eq!(titles(&remote_vault(&remote_dir)), vec!["e1", "e2", "e3"]);

        *PRE_PUSH.lock().unwrap() = None;
    }

    #[test]
    fn two_device_distinct_edits_converge_byte_identical() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());

        // Device A creates the vault (e1) and syncs it to the shared remote.
        let local_a = tmp.path().join("app_data_a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let v_a1 = Vault {
            entries: vec![entry("e1", "dev-a")],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v_a1)).unwrap();
        commit_vault_file(VAULT_FILE, "a: v1").unwrap();
        let outcome = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(outcome.pushed);
        assert!(!outcome.merged);

        // Device B clones the populated remote (v1 checked out locally) and
        // edits a DIFFERENT entry (adds e2), then syncs it up.
        let local_b = tmp.path().join("app_data_b");
        ensure_clone(&url, &local_b, PAT).unwrap();
        let mut v_b = decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap());
        assert_eq!(titles(&v_b), vec!["e1"]);
        v_b.entries.push(entry("e2", "dev-b"));
        write_vault_file(VAULT_FILE, &encrypt_vault(&v_b)).unwrap();
        commit_vault_file(VAULT_FILE, "b: add e2").unwrap();
        let outcome = sync(PAT, &KEY, "dev-b").unwrap();
        assert!(outcome.pushed);

        // Device A syncs: fast-forward pull to B's commit.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let outcome = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(outcome.pulled);

        // Both devices hold byte-identical vault.enc, decrypting to the same
        // vault with the same key.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let blob_a = checkout_vault_file(VAULT_FILE).unwrap();
        ensure_clone(&url, &local_b, PAT).unwrap();
        let blob_b = checkout_vault_file(VAULT_FILE).unwrap();
        assert_eq!(
            blob_a, blob_b,
            "both devices must converge to byte-identical vault.enc"
        );
        let v_a_final = decrypt_vault(&blob_a);
        let v_b_final = decrypt_vault(&blob_b);
        assert_eq!(v_a_final, v_b_final);
        assert_eq!(titles(&v_a_final), vec!["e1", "e2"]);
    }

    #[test]
    fn two_device_same_entry_conflict_merges_and_converges() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());

        // Device A pushes e1 at v1.
        let local_a = tmp.path().join("app_data_a");
        ensure_clone(&url, &local_a, PAT).unwrap();
        let e1 = entry("e1", "dev-a");
        let v1 = Vault {
            entries: vec![e1.clone()],
        };
        write_vault_file(VAULT_FILE, &encrypt_vault(&v1)).unwrap();
        commit_vault_file(VAULT_FILE, "v1").unwrap();
        sync(PAT, &KEY, "dev-a").unwrap();

        // Device B clones the populated remote (e1 v1 checked out locally).
        let local_b = tmp.path().join("app_data_b");
        ensure_clone(&url, &local_b, PAT).unwrap();
        assert_eq!(decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap()), v1);

        // Device A edits e1 (bump to v2) and pushes.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let mut v_a = decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap());
        let e_a = v_a.entries.iter_mut().find(|e| e.title == "e1").unwrap();
        e_a.bump();
        write_vault_file(VAULT_FILE, &encrypt_vault(&v_a)).unwrap();
        commit_vault_file(VAULT_FILE, "a: e1 v2").unwrap();
        let outcome = sync(PAT, &KEY, "dev-a").unwrap();
        assert!(outcome.pushed);

        // Device B offline-edits the SAME entry to a HIGHER version (v3).
        ensure_clone(&url, &local_b, PAT).unwrap();
        let mut v_b = decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap());
        let e_b = v_b.entries.iter_mut().find(|e| e.title == "e1").unwrap();
        e_b.bump();
        e_b.bump();
        write_vault_file(VAULT_FILE, &encrypt_vault(&v_b)).unwrap();
        commit_vault_file(VAULT_FILE, "b: e1 v3").unwrap();

        // Device B syncs: divergence -> conflict merge with backup; the higher
        // version (v3, B's edit) wins per the merge rule.
        ensure_clone(&url, &local_b, PAT).unwrap();
        let outcome = sync(PAT, &KEY, "dev-b").unwrap();
        assert!(outcome.merged);
        assert!(outcome.pushed);
        let backup = outcome
            .backup_created
            .expect("conflict merge must create a backup");
        assert!(backup.exists());
        let merged_b = decrypt_vault(&checkout_vault_file(VAULT_FILE).unwrap());
        let e1_b = merged_b.entries.iter().find(|e| e.title == "e1").unwrap();
        assert_eq!(e1_b.version, 3);

        // Device A syncs: converges to the same blob (fast-forward or merge).
        ensure_clone(&url, &local_a, PAT).unwrap();
        let outcome_a = sync(PAT, &KEY, "dev-a").unwrap();

        // Both devices converge to byte-identical vault.enc decrypting with the
        // same key; the merged entry is the higher version.
        ensure_clone(&url, &local_a, PAT).unwrap();
        let blob_a = checkout_vault_file(VAULT_FILE).unwrap();
        ensure_clone(&url, &local_b, PAT).unwrap();
        let blob_b = checkout_vault_file(VAULT_FILE).unwrap();
        assert_eq!(
            blob_a, blob_b,
            "both devices must converge to byte-identical vault.enc after conflict merge"
        );
        let v_a_final = decrypt_vault(&blob_a);
        let v_b_final = decrypt_vault(&blob_b);
        assert_eq!(v_a_final, v_b_final);
        let e1_final = v_a_final.entries.iter().find(|e| e.title == "e1").unwrap();
        assert_eq!(e1_final.version, 3);
        assert!(outcome_a.pulled || outcome_a.merged);
    }
}
