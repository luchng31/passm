//! git-backed sync plumbing (git2): clone/fetch/push, fast-forward checks,
//! and vault-file read/write/commit against the local sync repository.
//!
//! The local repository lives at `<app_data>/repo` (resolved by the Tauri
//! layer in T11). PAT credentials are supplied per-call via a git2
//! credential callback (`Cred::userpass_plaintext`) and are NEVER embedded
//! in the remote URL, so they cannot leak into `.git/config` on disk.

use crate::error::SyncError;
use git2::{
    build::RepoBuilder, Cred, CredentialType, ErrorCode, FetchOptions, Oid, PushOptions,
    RemoteCallbacks, Repository, Signature,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

/// Dummy username accepted by GitHub when the PAT is supplied as the password.
const CRED_USERNAME: &str = "x-access-token";
/// Deterministic commit identity: the vault repo is a private sync channel,
/// not user-facing git.
const COMMIT_NAME: &str = "passm";
const COMMIT_EMAIL: &str = "passm@local";
const REMOTE_NAME: &str = "origin";
const FETCH_REFSPEC: &str = "+refs/heads/*:refs/remotes/origin/*";

/// Local repository directory, set by `ensure_clone`. A `Mutex` (not
/// `OnceLock`) so a re-clone can point it at a fresh directory.
static REPO_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Serializes tests across modules that share the module-global `REPO_DIR`
/// (git_repo and sync_engine test suites must never run concurrently).
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Returns a credential callback that authenticates with the PAT as the
/// password and a dummy username (GitHub accepts PATs this way).
fn pat_credentials(
    pat: String,
) -> impl FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, git2::Error> {
    move |_url, _username, _allowed| Cred::userpass_plaintext(CRED_USERNAME, &pat)
}

/// Opens the repository previously set up by `ensure_clone`.
fn open_repo() -> Result<Repository, SyncError> {
    let guard = REPO_DIR.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = guard.as_deref().ok_or(SyncError::RepoNotInitialized)?;
    Repository::open(dir).map_err(SyncError::Git)
}

/// Returns the local repository directory set by `ensure_clone`.
pub fn repo_dir() -> Result<PathBuf, SyncError> {
    let guard = REPO_DIR.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clone().ok_or(SyncError::RepoNotInitialized)
}

/// Branch name of the current HEAD, handling the unborn (empty clone) case
/// where HEAD points at a branch that does not exist yet.
fn current_branch_name(repo: &Repository) -> Option<String> {
    if let Ok(head) = repo.head() {
        if let Ok(shorthand) = head.shorthand() {
            return Some(shorthand.to_string());
        }
    }
    let head_ref = repo.find_reference("HEAD").ok()?;
    let target = head_ref.symbolic_target().ok()??;
    target.strip_prefix("refs/heads/").map(str::to_string)
}

/// Shorthand name of the current branch, handling the unborn (empty clone)
/// case. Errors only if HEAD is detached.
pub fn current_branch() -> Result<String, SyncError> {
    let repo = open_repo()?;
    current_branch_name(&repo).ok_or_else(|| {
        SyncError::Git(git2::Error::from_str("cannot determine current branch"))
    })
}

/// Rejects absolute paths and paths escaping the working tree.
fn safe_rel_path(relative_path: &str) -> Result<&Path, SyncError> {
    let path = Path::new(relative_path);
    let unsafe_component = path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if relative_path.is_empty() || unsafe_component {
        return Err(SyncError::Git(git2::Error::from_str(&format!(
            "invalid repository-relative path: {relative_path}"
        ))));
    }
    Ok(path)
}

/// Clones `remote_url` into `local_dir` if it is missing or empty, otherwise
/// opens the existing clone. The `origin` remote is (re)configured to point at
/// `remote_url` — the PAT is only ever passed through the credential callback,
/// never stored in the URL.
pub fn ensure_clone(remote_url: &str, local_dir: &Path, pat: &str) -> Result<(), SyncError> {
    if !local_dir.join(".git").exists() {
        if let Some(parent) = local_dir.parent() {
            fs::create_dir_all(parent).map_err(SyncError::Io)?;
        }
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(pat_credentials(pat.to_string()));
        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);
        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_opts);
        builder.clone(remote_url, local_dir).map_err(SyncError::Git)?;
    }
    let repo = Repository::open(local_dir).map_err(SyncError::Git)?;
    let origin_url = match repo.find_remote(REMOTE_NAME) {
        Ok(remote) => remote.url().ok().map(str::to_string),
        Err(_) => None,
    };
    if origin_url.as_deref() != Some(remote_url) {
        repo.remote_set_url(REMOTE_NAME, remote_url)
            .map_err(SyncError::Git)?;
    }
    let mut guard = REPO_DIR.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(local_dir.to_path_buf());
    Ok(())
}

/// Fetches all refs from `origin`, updating the remote-tracking refs.
pub fn fetch(pat: &str) -> Result<(), SyncError> {
    let repo = open_repo()?;
    let mut remote = repo.find_remote(REMOTE_NAME).map_err(SyncError::Git)?;
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(pat_credentials(pat.to_string()));
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    remote
        .fetch(&[FETCH_REFSPEC], Some(&mut fetch_opts), Some("passm fetch"))
        .map_err(SyncError::Git)?;
    Ok(())
}

/// Pushes the current branch to `origin`. A rejected non-fast-forward push
/// surfaces as `SyncError::NonFastForward` (the T10 merge trigger).
pub fn push(pat: &str) -> Result<(), SyncError> {
    let repo = open_repo()?;
    let head = repo.head().map_err(SyncError::Git)?;
    let branch = head
        .shorthand()
        .ok()
        .ok_or_else(|| SyncError::Git(git2::Error::from_str("HEAD is detached")))?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    let mut remote = repo.find_remote(REMOTE_NAME).map_err(SyncError::Git)?;
    let rejected: RefCell<Option<String>> = RefCell::new(None);
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(pat_credentials(pat.to_string()));
    callbacks.push_update_reference(|refname, status| {
        if let Some(reason) = status {
            *rejected.borrow_mut() = Some(format!("{refname}: {reason}"));
        }
        Ok(())
    });
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    match remote.push(&[refspec.as_str()], Some(&mut push_opts)) {
        Err(e) if e.code() == ErrorCode::NotFastForward => Err(SyncError::NonFastForward),
        Err(e) if e.message().to_lowercase().contains("non-fast-forward") => {
            Err(SyncError::NonFastForward)
        }
        Err(e) => Err(SyncError::Git(e)),
        Ok(()) => match rejected.borrow().as_deref() {
            Some(reason) if reason.to_lowercase().contains("non-fast-forward") => {
                Err(SyncError::NonFastForward)
            }
            Some(reason) => Err(SyncError::Git(git2::Error::from_str(&format!(
                "push rejected: {reason}"
            )))),
            None => Ok(()),
        },
    }
}

/// True when `local_head` can be pushed onto `remote_head` as a fast-forward
/// (i.e. `remote_head` is an ancestor of, or equal to, `local_head`).
pub fn is_fast_forward(local_head: &Oid, remote_head: &Oid) -> bool {
    if local_head == remote_head {
        return true;
    }
    match open_repo() {
        Ok(repo) => repo
            .graph_descendant_of(*local_head, *remote_head)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Oid of the current branch tip, or `None` for an empty/unborn repository.
pub fn current_head() -> Result<Option<Oid>, SyncError> {
    let repo = open_repo()?;
    let result = match repo.head() {
        Ok(reference) => Ok(reference.target()),
        Err(_) => Ok(None),
    };
    result
}

/// Oid of the remote-tracking ref for the current branch, or `None` if the
/// remote ref does not exist yet. Works for an unborn HEAD (empty clone).
pub fn remote_head() -> Result<Option<Oid>, SyncError> {
    let repo = open_repo()?;
    let Some(branch) = current_branch_name(&repo) else {
        return Ok(None);
    };
    let tracking = format!("refs/remotes/origin/{branch}");
    let result = match repo.find_reference(&tracking) {
        Ok(reference) => Ok(reference.target()),
        Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
        Err(e) => Err(SyncError::Git(e)),
    };
    result
}

/// Reads the `vault.enc` blob from the tree of the commit a ref points at.
pub fn read_vault_from_ref(refname: &str) -> Result<Vec<u8>, SyncError> {
    let repo = open_repo()?;
    let reference = repo.find_reference(refname).map_err(SyncError::Git)?;
    let oid = reference.target().ok_or_else(|| {
        SyncError::Git(git2::Error::from_str("ref has no commit target"))
    })?;
    let commit = repo.find_commit(oid).map_err(SyncError::Git)?;
    let tree = commit.tree().map_err(SyncError::Git)?;
    let entry = tree
        .get_path(Path::new("vault.enc"))
        .map_err(SyncError::Git)?;
    let blob = repo.find_blob(entry.id()).map_err(SyncError::Git)?;
    Ok(blob.content().to_vec())
}

/// Reads the vault blob from the working tree (post-merge content, for T10).
pub fn checkout_vault_file(relative_path: &str) -> Result<Vec<u8>, SyncError> {
    let repo = open_repo()?;
    let path = safe_rel_path(relative_path)?;
    let workdir = repo.workdir().ok_or_else(|| {
        SyncError::Git(git2::Error::from_str("bare repository has no working tree"))
    })?;
    fs::read(workdir.join(path)).map_err(SyncError::Io)
}

/// Writes the vault blob to the working tree and stages it.
pub fn write_vault_file(relative_path: &str, bytes: &[u8]) -> Result<(), SyncError> {
    let repo = open_repo()?;
    let path = safe_rel_path(relative_path)?;
    let workdir = repo.workdir().ok_or_else(|| {
        SyncError::Git(git2::Error::from_str("bare repository has no working tree"))
    })?;
    let full = workdir.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(SyncError::Io)?;
    }
    fs::write(&full, bytes).map_err(SyncError::Io)?;
    let mut index = repo.index().map_err(SyncError::Git)?;
    index.add_path(path).map_err(SyncError::Git)?;
    index.write().map_err(SyncError::Git)?;
    Ok(())
}

/// Commits the staged vault file with the deterministic `passm <passm@local>`
/// identity, returning the new commit oid.
pub fn commit_vault_file(relative_path: &str, message: &str) -> Result<Oid, SyncError> {
    let repo = open_repo()?;
    let path = safe_rel_path(relative_path)?;
    let mut index = repo.index().map_err(SyncError::Git)?;
    index.add_path(path).map_err(SyncError::Git)?;
    let tree_oid = index.write_tree().map_err(SyncError::Git)?;
    let tree = repo.find_tree(tree_oid).map_err(SyncError::Git)?;
    let signature = Signature::now(COMMIT_NAME, COMMIT_EMAIL).map_err(SyncError::Git)?;
    let parents: Vec<git2::Commit> = match current_head()? {
        Some(oid) => vec![repo.find_commit(oid).map_err(SyncError::Git)?],
        None => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .map_err(SyncError::Git)?;
    index.write().map_err(SyncError::Git)?;
    Ok(oid)
}

/// Commits the staged vault file as a two-parent merge commit with the remote
/// head as the second parent. A push only succeeds when the local branch is a
/// descendant of the remote branch, so a conflict-resolution commit must adopt
/// the remote head as a parent or a rejected push can never converge.
pub fn commit_vault_merge(
    relative_path: &str,
    message: &str,
    remote_oid: Oid,
) -> Result<Oid, SyncError> {
    let repo = open_repo()?;
    let path = safe_rel_path(relative_path)?;
    let mut index = repo.index().map_err(SyncError::Git)?;
    index.add_path(path).map_err(SyncError::Git)?;
    let tree_oid = index.write_tree().map_err(SyncError::Git)?;
    let tree = repo.find_tree(tree_oid).map_err(SyncError::Git)?;
    let signature = Signature::now(COMMIT_NAME, COMMIT_EMAIL).map_err(SyncError::Git)?;
    let local_oid = current_head()?.ok_or_else(|| {
        SyncError::Git(git2::Error::from_str("merge requires an existing local commit"))
    })?;
    let parents = [
        repo.find_commit(local_oid).map_err(SyncError::Git)?,
        repo.find_commit(remote_oid).map_err(SyncError::Git)?,
    ];
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .map_err(SyncError::Git)?;
    index.write().map_err(SyncError::Git)?;
    Ok(oid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Commit, CredentialType, Oid, Repository, Signature};
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    const PAT: &str = "ghp_test_dummy_pat";

    /// Serializes git_repo tests: they share the module-global `REPO_DIR`, so
    /// parallel execution would make one test operate on another's repository.
    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn bare_remote(parent: &Path) -> (PathBuf, String) {
        let dir = parent.join("remote.git");
        Repository::init_bare(&dir).unwrap();
        let url = format!("file://{}", dir.display());
        (dir, url)
    }

    fn local_dir(parent: &Path) -> PathBuf {
        parent.join("local")
    }

    fn branch_name(repo: &Repository) -> String {
        repo.head().unwrap().shorthand().unwrap().to_string()
    }

    fn advance_remote(remote_dir: &Path, branch: &str, parent: Option<Oid>, contents: &[u8]) -> Oid {
        let repo = Repository::open_bare(remote_dir).unwrap();
        let blob = repo.blob(contents).unwrap();
        let mut tb = repo.treebuilder(None).unwrap();
        tb.insert("vault.enc", blob, 0o100644).unwrap();
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

    #[test]
    fn ensure_clone_creates_repo_with_remote() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        assert!(local.join(".git").exists());
        let repo = Repository::open(&local).unwrap();
        let remote = repo.find_remote("origin").unwrap();
        assert_eq!(remote.name().unwrap(), Some("origin"));
        assert_eq!(remote.url().unwrap(), url);
    }

    #[test]
    fn ensure_clone_is_idempotent() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        ensure_clone(&url, &local, PAT).unwrap();
        ensure_clone(&url, &local, PAT).unwrap();
        let repo = Repository::open(&local).unwrap();
        assert_eq!(
            repo.find_remote("origin").unwrap().url().unwrap(),
            url
        );
    }

    #[test]
    fn write_commit_push_reaches_remote() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        write_vault_file("vault.enc", b"first").unwrap();
        let oid = commit_vault_file("vault.enc", "initial commit").unwrap();
        push(PAT).unwrap();

        let branch = branch_name(&Repository::open(&local).unwrap());
        let remote_repo = Repository::open_bare(&remote_dir).unwrap();
        let remote_ref = remote_repo
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap();
        assert_eq!(remote_ref.target(), Some(oid));
    }

    #[test]
    fn fetch_updates_remote_tracking_ref() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        write_vault_file("vault.enc", b"v1").unwrap();
        let v1 = commit_vault_file("vault.enc", "v1").unwrap();
        push(PAT).unwrap();

        let branch = branch_name(&Repository::open(&local).unwrap());
        let v2 = advance_remote(&remote_dir, &branch, Some(v1), b"v2");

        let tracking = format!("refs/remotes/origin/{branch}");
        let local_repo = Repository::open(&local).unwrap();
        let before = local_repo.find_reference(&tracking).unwrap();
        assert_eq!(before.target(), Some(v1));

        fetch(PAT).unwrap();
        let local_repo = Repository::open(&local).unwrap();
        let after = local_repo.find_reference(&tracking).unwrap();
        assert_eq!(after.target(), Some(v2));
    }

    #[test]
    fn is_fast_forward_checks_ancestry() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        write_vault_file("vault.enc", b"a").unwrap();
        let c1 = commit_vault_file("vault.enc", "c1").unwrap();
        push(PAT).unwrap();

        write_vault_file("vault.enc", b"b").unwrap();
        let c2 = commit_vault_file("vault.enc", "c2").unwrap();
        assert!(is_fast_forward(&c2, &c1));
        assert!(is_fast_forward(&c2, &c2));

        let branch = branch_name(&Repository::open(&local).unwrap());
        let r2 = advance_remote(&remote_dir, &branch, Some(c1), b"remote");
        assert!(!is_fast_forward(&c2, &r2));
    }

    #[test]
    fn non_fast_forward_push_is_distinguishable() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        write_vault_file("vault.enc", b"base").unwrap();
        let base = commit_vault_file("vault.enc", "base").unwrap();
        push(PAT).unwrap();

        let branch = branch_name(&Repository::open(&local).unwrap());
        advance_remote(&remote_dir, &branch, Some(base), b"remote-first");

        write_vault_file("vault.enc", b"local-first").unwrap();
        commit_vault_file("vault.enc", "local").unwrap();

        match push(PAT) {
            Err(SyncError::NonFastForward) => {}
            other => panic!("expected NonFastForward, got {other:?}"),
        }
    }

    #[test]
    fn checkout_vault_file_roundtrip() {
        let _guard = test_guard();
        let tmp = tempdir().unwrap();
        let (_remote_dir, url) = bare_remote(tmp.path());
        let local = local_dir(tmp.path());
        ensure_clone(&url, &local, PAT).unwrap();
        let payload: Vec<u8> = (0..=255u8).collect();
        write_vault_file("vault.enc", &payload).unwrap();
        commit_vault_file("vault.enc", "payload").unwrap();
        assert_eq!(checkout_vault_file("vault.enc").unwrap(), payload);
    }

    #[test]
    fn credential_callback_uses_pat_as_password() {
        let mut cb = pat_credentials(PAT.to_string());
        let cred = cb(
            "https://github.com/acme/passm",
            None,
            CredentialType::USER_PASS_PLAINTEXT,
        )
        .unwrap();
        assert!(cred.has_username());
        assert_eq!(
            cred.credtype(),
            CredentialType::USER_PASS_PLAINTEXT.bits()
        );
    }
}
