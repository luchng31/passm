# Learnings — passm

Conventions, patterns, and successful approaches discovered during work on this plan.

_Auto-scaffolded by /start-work. Append new entries below - never overwrite._

---

## [2026-08-14] Toolchain resolved (T1 unblocked)
- User installed Rust themselves: rustc 1.97.1 + cargo 1.97.1 + clippy/rustfmt/rust-docs/rust-std via rustup at `~/.cargo/bin` (default stable). Well above Tauri 2's 1.77.2 minimum.
- cmake 4.4.2 at `~/.local/bin/cmake` (installed via `pip3 install --user --break-system-packages cmake` — no sudo available).
- **PATH for all build commands: `export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"`** (both rustup and cmake live outside default PATH).
- Quick `cargo new` + `cargo build` smoke test passed → toolchain fully functional.
- Disk: 8.2G free on / (71% used) — enough for git2 vendored + Tauri builds, but watch it.
- Java 17 + Android SDK/NDK still NOT installed (needed for T7/T16) — install before T7.

## [2026-08-14] T1 done: workspace scaffold + git2 vendored build proven
- Workspace at /home/ubuntu/passm: 5 members (passm-crypto/vault/sync/cli + src-tauri as `passm-app` placeholder, no tauri dep).
- Pinned deps all resolved exactly as planned: git2 0.21.0 (vendored-libgit2+vendored-openssl+https, verified via `cargo tree -i git2 -e features`), chacha20poly1305 0.11.0, argon2 0.5.3, hkdf 0.13.0, serde 1.0.229, uuid 1.24.0, zeroize 1.9.0, rand 0.8.7. keyring 4.1 declared (not yet built — no crate uses it in T1).
- git2 vendored build took **13m 56s** on this box (libgit2-sys 0.18.7+1.9.6 + openssl from source). One-time cost; incremental builds are fast. Do NOT kill it.
- `cargo test --workspace` green: 1 smoke test per crate × 5 crates.
- `cargo build --workspace` stays green with src-tauri as a plain crate (no webkit2gtk needed) — T11 will flip it to real Tauri.
- Recipe confirmed: `git2 = { version = "0.21", features = ["vendored-libgit2", "vendored-openssl", "https"] }` works on Ubuntu 24.04 with cmake 4.4.2 at ~/.local/bin.
- Evidence: .omo/evidence/task-1-scaffold.txt

## [2026-08-14] T4 done: passm-vault Entry/Vault models + canonical JSON
- `Entry`/`Vault` in crates/passm-vault/src/lib.rs with serde derive; struct field order = stable JSON key order. `canonical_json()` sorts entries by id → byte-stable output for T5 merge convergence proof.
- **uuid workspace dep needs `features = ["serde"]`** for `Uuid` to implement Serialize/Deserialize — added at the member level (`uuid = { workspace = true, features = ["serde"] }`); features union with the workspace dep, no workspace manifest change needed.
- serde_json is NOT in the workspace pinned list → added as plain `serde_json = "1"` in passm-vault's own [dependencies] (acceptable per plan).
- Timestamps are i64 unix secs via `SystemTime::now().duration_since(UNIX_EPOCH)` — no `time` crate needed.
- `canonical_json` uses `.expect()` on a provably-infallible serialization (Vault = only Uuid/String/u64/i64/bool, no maps) to satisfy the `-> Vec<u8>` signature; documented in evidence.
- TDD flow: tests first (RED: 25 compile errors), then impl (GREEN: 8/8). Clippy `-D warnings` clean.
- Evidence: .omo/evidence/task-4-vault.txt

## [2026-08-14] T2 done: passm-crypto Argon2id + HKDF key derivation with golden vectors
- `KdfParams { mem_kib: 65536, iterations: 3, parallelism: 4 }` (serde derive + Default), `derive_master_key(password, salt: &[u8;32], params) -> Result<[u8;32]>` via argon2 0.5 `Algorithm::Argon2id` + `Version::V0x13`, `derive_vault_key(master) -> [u8;32]` via hkdf 0.13 HKDF-SHA256 salt=None info=`b"passm-v1-vault-key"` L=32.
- **sha2 was NOT in the workspace pinned list but is REQUIRED**: hkdf 0.13's `Hkdf<H>` needs a concrete hash type, and hkdf only depends on `hmac`. Added `sha2 = "0.11"` to workspace.dependencies — **must be 0.11, NOT 0.10**: hmac 0.13 uses digest 0.11, sha2 0.10 uses digest 0.10 → `Hkdf::<Sha256>` fails to compile with a wall of trait-bound errors. This is the same "make transitive dep direct" pattern as T4's serde_json.
- `derive_vault_key` returns `[u8;32]` (not Result) per spec; HKDF expand with L=32 is statically infallible (max 255*32 bytes) → handled with `match` + `unreachable!` + `# Panics` doc, not unwrap.
- zeroize: password copied to owned `Vec<u8>`, zeroized after derive; master zeroized on error path.
- Golden vector frozen in test: password=`b"correct horse battery staple"`, salt=`[0x42;32]`, default params → master `[234,233,121,...]`, vault `[183,240,204,...]`. QA: mutating a constant byte → test FAILED → reverted.
- TDD flow: RED (23 compile errors, functions missing) → GREEN (5/6, golden placeholder red) → freeze real values (6/6). Clippy `-D warnings` clean.
- Evidence: .omo/evidence/task-2-kdf.txt
