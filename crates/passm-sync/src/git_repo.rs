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
use libgit2_sys::{
    git_libgit2_init, git_libgit2_opts, GIT_OPT_ADD_SSL_X509_CERT, GIT_OPT_SET_SERVER_CONNECT_TIMEOUT,
    GIT_OPT_SET_SERVER_TIMEOUT,
};
use openssl_sys::{BIO_free_all, BIO_new_mem_buf, PEM_read_bio_X509, X509_free};
use std::cell::RefCell;
use std::fs;
use std::os::raw::{c_int, c_void};
use std::path::{Component, Path, PathBuf};
use std::ptr::null_mut;
use std::sync::Mutex;

/// Dummy username accepted by GitHub when the PAT is supplied as the password.
const CRED_USERNAME: &str = "x-access-token";
/// Deterministic commit identity: the vault repo is a private sync channel,
/// not user-facing git.
const COMMIT_NAME: &str = "passm";
const COMMIT_EMAIL: &str = "passm@local";
const REMOTE_NAME: &str = "origin";
const FETCH_REFSPEC: &str = "+refs/heads/*:refs/remotes/origin/*";

/// Mozilla CA bundle shipped with the binary. Android has no system trust
/// store, so libgit2's OpenSSL backend cannot verify GitHub's certificate
/// (git2-rs #920). `ensure_cert_store` parses this in memory and injects each
/// certificate into libgit2's global OpenSSL store — no file I/O, which is
/// unreliable on Android (`BIO_new_file` fails with `ERR_R_BIO_LIB`).
const CACERT_BYTES: &[u8] = include_bytes!("../assets/cacert.pem");

/// Connect timeout (ms) for git network ops. libgit2 has no default (relies on
/// the OS TCP timeout, typically ~75s and not always enforced), so a silent
/// network failure on Android would hang clone/fetch/push forever — the UI
/// spinner never ends. 15s bounds the handshake.
const CONNECT_TIMEOUT_MS: c_int = 15_000;
/// Per-read/write server timeout (ms); bounds a stalled transfer after the
/// handshake (e.g. half-open TLS). 60s is generous for a small vault repo.
const SERVER_TIMEOUT_MS: c_int = 60_000;

/// Injects the embedded Mozilla CA bundle into libgit2's global OpenSSL
/// certificate store, so HTTPS clone/fetch/push can verify GitHub on Android
/// (which has no system trust store). The bundle is parsed in memory and each
/// certificate is added via `GIT_OPT_ADD_SSL_X509_CERT` — no certificate file
/// is ever written. Also sets libgit2's global connect + server timeouts so a
/// silent network failure cannot hang a git op forever. The injection is
/// re-run on every call: the store add is idempotent (duplicates are no-ops),
/// matching the old rewrite-on-every-call behavior that healed stale state.
/// Chain validation and the hostname SAN check remain fully enforced by
/// libgit2/OpenSSL.
pub fn ensure_cert_store() -> Result<(), SyncError> {
    // SAFETY: `git_libgit2_init` is refcounted and thread-safe. It must run
    // before the first `GIT_OPT_ADD_SSL_X509_CERT` call so libgit2's global
    // SSL ctx (`git__ssl_ctx`, created by `git_openssl_stream_global_init`)
    // exists. The `git2` crate also inits; the extra refcount is released on
    // the final matching shutdown, so this is always sound.
    unsafe {
        git_libgit2_init();
    }
    // SAFETY: `git_libgit2_opts` is a variadic FFI call; both timeout options
    // take a single `c_int` in milliseconds, which is the natural vararg
    // promotion of our typed constants. Setting a global option is idempotent
    // (re-set to the same value on every call), and the values are only ever
    // read by libgit2's own transport layer while a connection is active.
    unsafe {
        git_libgit2_opts(GIT_OPT_SET_SERVER_CONNECT_TIMEOUT as c_int, CONNECT_TIMEOUT_MS);
        git_libgit2_opts(GIT_OPT_SET_SERVER_TIMEOUT as c_int, SERVER_TIMEOUT_MS);
    }
    // SAFETY: `CACERT_BYTES` is a `'static` slice; `as_ptr` is valid for
    // `len` bytes and `BIO_new_mem_buf` copies the buffer into a new BIO, so
    // no borrow of the slice outlives this call. A null return means the BIO
    // could not be allocated and is handled before use.
    let bio = unsafe {
        BIO_new_mem_buf(
            CACERT_BYTES.as_ptr().cast::<c_void>(),
            CACERT_BYTES.len() as c_int,
        )
    };
    if bio.is_null() {
        return Err(SyncError::Git(git2::Error::from_str(
            "failed to create memory BIO for CA bundle",
        )));
    }
    let mut added = 0usize;
    loop {
        // SAFETY: `bio` is a live `BIO*` (null-checked above, freed only by
        // `BIO_free_all` below). `PEM_read_bio_X509` advances the BIO and
        // returns a null-owned `X509*` that must be released with
        // `X509_free`; `null_mut()` for the out-param and `None` callback are
        // valid per the OpenSSL API contract.
        let cert = unsafe { PEM_read_bio_X509(bio, null_mut(), None, null_mut()) };
        if cert.is_null() {
            break;
        }
        // SAFETY: `cert` is a valid `X509*` returned by `PEM_read_bio_X509`
        // above. `GIT_OPT_ADD_SSL_X509_CERT` adds it to libgit2's global SSL
        // store, which up-refs the certificate (`X509_STORE_add_cert`), so
        // the caller's reference remains ours to free afterwards. The variadic
        // call expects the `X509*`; `*mut X509` is FFI-safe as a vararg.
        let rc = unsafe { git_libgit2_opts(GIT_OPT_ADD_SSL_X509_CERT as c_int, cert) };
        // SAFETY: we own the reference returned by `PEM_read_bio_X509`; the
        // store up-ref'd the certificate, so `X509_free` releases exactly our
        // reference (no use-after-free, no double-free).
        unsafe {
            X509_free(cert);
        }
        if rc < 0 {
            // SAFETY: `bio` is the memory BIO created above; `BIO_free_all`
            // releases it (and any chained BIOs) exactly once, after which no
            // pointer into it is used again.
            unsafe {
                BIO_free_all(bio);
            }
            return Err(SyncError::Git(git2::Error::from_str(
                "failed to add CA certificate to libgit2 SSL store",
            )));
        }
        added += 1;
    }
    // SAFETY: releases the memory BIO created by `BIO_new_mem_buf` above; the
    // parse loop has ended, so no further reads occur through `bio`.
    unsafe {
        BIO_free_all(bio);
    }
    if added == 0 {
        return Err(SyncError::Git(git2::Error::from_str(
            "no certificates parsed from embedded CA bundle",
        )));
    }
    Ok(())
}

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
    let guard = REPO_DIR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = guard.as_deref().ok_or(SyncError::RepoNotInitialized)?;
    Repository::open(dir).map_err(SyncError::Git)
}

/// Returns the local repository directory set by `ensure_clone`.
pub fn repo_dir() -> Result<PathBuf, SyncError> {
    let guard = REPO_DIR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    current_branch_name(&repo)
        .ok_or_else(|| SyncError::Git(git2::Error::from_str("cannot determine current branch")))
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
    ensure_cert_store()?;
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
        builder
            .clone(remote_url, local_dir)
            .map_err(SyncError::Git)?;
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
    let mut guard = REPO_DIR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(local_dir.to_path_buf());
    Ok(())
}

/// Fetches all refs from `origin`, updating the remote-tracking refs.
pub fn fetch(pat: &str) -> Result<(), SyncError> {
    // libgit2's SSL store is process-global and initialized once per process;
    // re-inject the bundled CA bundle into it before any network op (Android
    // has no system trust store). Injection is idempotent, so re-running on
    // every call is safe.
    ensure_cert_store()?;
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
    // libgit2's SSL store is process-global and initialized once per process;
    // re-inject the bundled CA bundle into it before any network op (Android
    // has no system trust store). Injection is idempotent, so re-running on
    // every call is safe.
    ensure_cert_store()?;
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
    let oid = reference
        .target()
        .ok_or_else(|| SyncError::Git(git2::Error::from_str("ref has no commit target")))?;
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
        SyncError::Git(git2::Error::from_str(
            "merge requires an existing local commit",
        ))
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
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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

    #[test]
    fn ensure_cert_store_loads_bundle_certs() {
        let _guard = test_guard();
        ensure_cert_store().unwrap();
        // Second call must not fail: the store add is idempotent.
        ensure_cert_store().unwrap();
    }

    #[test]
    fn embedded_bundle_contains_121_certs() {
        let marker = b"-----BEGIN CERTIFICATE-----";
        let count = CACERT_BYTES
            .windows(marker.len())
            .filter(|window| *window == marker)
            .count();
        assert_eq!(count, 121);
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
        assert_eq!(repo.find_remote("origin").unwrap().url().unwrap(), url);
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
        assert_eq!(cred.credtype(), CredentialType::USER_PASS_PLAINTEXT.bits());
    }
}
