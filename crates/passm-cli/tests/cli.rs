//! Integration tests for the passm-cli verification harness.
//!
//! These drive the real compiled binary (`CARGO_BIN_EXE_passm-cli`) through
//! its CLI surface. The fixed password/salt below are the SAME inputs T2 froze
//! in `crates/passm-crypto/src/lib.rs` (`PASSWORD`/`SALT`), so the `derive`
//! test pins the CLI's output to T2's golden vectors.
//!
//! Argon2id at 64 MiB costs ~1-2 s per derive; each test derives only as many
//! times as it must (never in a loop).

use passm_vault::Vault;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// T2's frozen fixture password (crates/passm-crypto/src/lib.rs `PASSWORD`).
const PASSWORD: &str = "correct horse battery staple";
/// T2's frozen fixture salt `[0x42; 32]` as hex.
const SALT_HEX: &str = "4242424242424242424242424242424242424242424242424242424242424242";
/// T2's frozen `GOLDEN_MASTER_KEY` as hex.
const GOLDEN_MASTER_KEY_HEX: &str =
    "eae979a72a22bbe97f27910e712144453cdbf7d8d9abecad6a90cc72730f4cb1";
/// T2's frozen `GOLDEN_VAULT_KEY` as hex.
const GOLDEN_VAULT_KEY_HEX: &str =
    "b7f0cc5b680771019ec1575c402653824a643fffe733125fc38bb3fa12831eeb";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_passm-cli")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn passm-cli")
}

fn assert_ok(out: &Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Fresh per-test temp dir (pid-scoped so parallel runs cannot collide).
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("passm-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn derive_matches_t2_golden_vectors() {
    let out = run(&["derive", "--password", PASSWORD, "--salt", SALT_HEX]);
    assert_ok(&out, "derive");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains(&format!("master_key={GOLDEN_MASTER_KEY_HEX}")),
        "master key must match T2 GOLDEN_MASTER_KEY"
    );
    assert!(
        stdout.contains(&format!("vault_key={GOLDEN_VAULT_KEY_HEX}")),
        "vault key must match T2 GOLDEN_VAULT_KEY"
    );
}

#[test]
fn encrypt_then_decrypt_roundtrips_fixture() {
    let dir = temp_dir("roundtrip");
    let plain = dir.join("vault.plain.json");
    std::fs::copy(fixture("vault.plain.json"), &plain).expect("copy fixture");
    let blob = dir.join("vault.passm1");
    let out = dir.join("vault.out.json");

    let enc = run(&[
        "encrypt",
        "--in",
        plain.to_str().unwrap(),
        "--out",
        blob.to_str().unwrap(),
        "--password",
        PASSWORD,
    ]);
    assert_ok(&enc, "encrypt");

    let dec = run(&[
        "decrypt",
        "--in",
        blob.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--password",
        PASSWORD,
    ]);
    assert_ok(&dec, "decrypt");

    let original = std::fs::read(&plain).expect("read plaintext");
    let decrypted = std::fs::read(&out).expect("read decrypted");
    assert_eq!(decrypted, original, "decrypt must reproduce the exact plaintext bytes");
}

#[test]
fn decrypt_golden_blob_matches_fixture() {
    let dir = temp_dir("golden");
    let out = dir.join("vault.out.json");

    let dec = run(&[
        "decrypt",
        "--in",
        fixture("vault.golden.passm1").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--password",
        PASSWORD,
    ]);
    assert_ok(&dec, "decrypt golden blob");

    let original = std::fs::read(fixture("vault.plain.json")).expect("read fixture");
    let decrypted = std::fs::read(&out).expect("read decrypted");
    assert_eq!(
        decrypted, original,
        "committed golden blob must decrypt to the committed plaintext fixture"
    );
}

#[test]
fn decrypt_with_wrong_password_exits_nonzero() {
    let dir = temp_dir("wrongpw");
    let out = dir.join("vault.out.json");

    let dec = run(&[
        "decrypt",
        "--in",
        fixture("vault.golden.passm1").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--password",
        "wrong password",
    ]);
    assert!(
        !dec.status.success(),
        "wrong password must exit nonzero (got {:?})",
        dec.status.code()
    );
    assert!(
        !dec.stderr.is_empty(),
        "wrong password must print an error message to stderr"
    );
    assert!(!out.exists(), "no output file may be written on failure");
}

#[test]
fn vault_add_then_list_shows_new_entry() {
    let dir = temp_dir("addlist");
    let blob = dir.join("vault.passm1");
    std::fs::copy(fixture("vault.golden.passm1"), &blob).expect("copy golden blob");

    let add = run(&[
        "vault-add",
        "--vault",
        blob.to_str().unwrap(),
        "--password",
        PASSWORD,
        "--title",
        "New Service",
        "--username",
        "newuser",
        "--password-value",
        "s3cret",
        "--url",
        "https://new.example.com",
        "--notes",
        "added by cli",
    ]);
    assert_ok(&add, "vault-add");

    let list = run(&["vault-list", "--vault", blob.to_str().unwrap(), "--password", PASSWORD]);
    assert_ok(&list, "vault-list");
    let stdout = String::from_utf8(list.stdout).expect("utf8 stdout");
    assert!(stdout.contains("New Service"), "vault-list must show the added title");
    assert!(stdout.contains("newuser"), "vault-list must show the added username");
}

#[test]
fn vault_add_preserves_existing_entries() {
    let dir = temp_dir("preserve");
    let blob = dir.join("vault.passm1");
    std::fs::copy(fixture("vault.golden.passm1"), &blob).expect("copy golden blob");

    let add = run(&[
        "vault-add",
        "--vault",
        blob.to_str().unwrap(),
        "--password",
        PASSWORD,
        "--title",
        "Another",
        "--username",
        "u2",
        "--password-value",
        "p2",
        "--url",
        "https://x.example.com",
        "--notes",
        "",
    ]);
    assert_ok(&add, "vault-add");

    let out = dir.join("vault.out.json");
    let dec = run(&[
        "decrypt",
        "--in",
        blob.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--password",
        PASSWORD,
    ]);
    assert_ok(&dec, "decrypt after vault-add");

    let after: Vault =
        serde_json::from_slice(&std::fs::read(&out).expect("read decrypted")).expect("parse vault");
    let before: Vault = serde_json::from_slice(
        &std::fs::read(fixture("vault.plain.json")).expect("read fixture"),
    )
    .expect("parse fixture");

    for entry in &before.entries {
        assert!(
            after.entries.iter().any(|e| e.id == entry.id),
            "entry {} ({}) lost after vault-add",
            entry.id,
            entry.title
        );
    }
    assert!(
        after.entries.iter().any(|e| e.title == "Another"),
        "added entry must be present after vault-add"
    );
}