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

## [2026-08-17] T3 done: PASSM1 envelope (XChaCha20-Poly1305, AAD-bound 75B header)
- `envelope` module in passm-crypto: `encrypt(vault_key, &KdfParams, salt, plaintext) -> Vec<u8>`, `decrypt(vault_key, blob) -> Result<Vec<u8>>`, `parse_header(blob) -> Result<EnvelopeHeader{params, salt}>`. Header = magic `PASSM1`(6) + version 0x01(1) + mem_kib/iterations/parallelism u32 BE(12) + salt(32) + nonce(24) = 75B, ALL as AAD to XChaCha20-Poly1305; ciphertext+16B tag at offset 75.
- **chacha20poly1305 0.11 API**: `XNonce`/`Key` are `hybrid_array::Array` aliases. `encrypt`/`decrypt` take `&XNonce`/`&Key` BY REFERENCE. `Array::from_slice()` is **deprecated** in hybrid-array 0.4.14 → use `XNonce::from([u8;24])` / `Key::from([u8;32])` (From<[T;N]> impl) — same semantics, clippy-clean.
- `encrypt` returns `Vec<u8>` (spec signature, not Result): aead::Error is provably unreachable for in-memory payloads (only ≥256 GiB or AAD > u64::MAX) → `match` + `unreachable!` + `# Panics` doc, same pattern as T2's `derive_vault_key`.
- **Clippy gotcha**: a doc line starting with `>=` (e.g. ">= 256 GiB") trips `clippy::doc_lazy_continuation` (interpreted as a quote) → reword to "of 256 GiB or more".
- Fresh nonce via `rand::rngs::OsRng` + `RngCore::fill_bytes` (rand 0.8). Enabled `chacha20poly1305` `zeroize` feature at member level (`{ workspace = true, features = ["zeroize"] }`) so the cipher's internal key copy is wiped on drop — same member-level feature pattern as T4's uuid serde.
- Tamper-every-header-byte test: bytes 0..5 (magic) → `BadMagic`, byte 6 (version) → `UnsupportedVersion`, bytes 7..74 (KDF params/salt/nonce) → `AuthenticationFailed`. QA mutation (remove AAD from both encrypt+decrypt) made the test FAIL → AAD binding proven non-vacuous.
- TDD flow: RED (37 compile errors) → GREEN (17/17: 11 envelope + 6 T2 KDF). Clippy `-D warnings` clean.
- Evidence: .omo/evidence/task-3-envelope.txt

## [2026-08-17] T5 done: passm-vault commutative merge
- `pub fn merge(local: &Vault, remote: &Vault) -> Vault` in crates/passm-vault/src/lib.rs, pure (no I/O). Rule per id: higher version wins; equal version + one tombstone → tombstone wins (no-resurrect); equal version + both live → lexicographically higher device_id wins; single-side entry taken as-is.
- Implemented as a **total order** (`entry_cmp`: version → deleted → device_id → remaining fields) and winner = max. Commutativity is free by construction (`max(a,b)==max(b,a)`); idempotence follows because re-merging an input can never beat the already-chosen max. Result entries sorted by id.
- **Bug caught by the rule-specific test that the property test could NOT catch**: first impl used `b.deleted.cmp(&a.deleted)` which inverted the tombstone preference (live won at equal version). The randomized commutativity/idempotence test passed anyway — any deterministic total order is commutative/idempotent. Lesson: property tests prove convergence, but rule-specific deterministic tests are what pin the actual preference direction.
- Property test: `StdRng::seed_from_u64` (rand 0.8 workspace dep, added to passm-vault `[dev-dependencies]`), 50 iterations, asserts `canonical_json(merge(a,b)) == canonical_json(merge(b,a))` + idempotence `merge(merge(a,b), b) == merge(a,b)`. No proptest needed.
- rand 0.8 API: `StdRng::seed_from_u64`, `rng.gen_range(0..=n)`, `rng.gen_bool(0.3)` — trait imports (`Rng`, `SeedableRng`) must be at the test-module level, not inside the test fn, if helper fns also use them.
- TDD: RED (15 compile errors) → GREEN (14/14). Clippy `-D warnings` clean.
- Evidence: .omo/evidence/task-5-merge.txt

## [2026-08-17] T6 done: passm-cli verification harness (golden vectors + unlock seam)
- Bin crate with 5 flat subcommands, manual `--flag value` parsing (no clap). `parse_flags` stores flags WITH leading `--` so `--password` and `--password-value` are distinct keys. Deps: passm-crypto, passm-vault, argon2 (direct, to name `argon2::Error` in the CLI error type — same "make transitive dep direct" pattern), serde_json, rand. Dropped the unused passm-sync dep.
- **The unlock seam** (what the app's unlock will do): `parse_header(blob)` → params+salt → `derive_master_key(password, salt, params)` → `derive_vault_key` → `envelope::decrypt`. Implemented once as `unlock()` in commands.rs, shared by decrypt/vault-add/vault-list.
- **CRITICAL design invariant: the envelope salt is a per-vault constant, NOT per-encryption.** The vault key = HKDF(Argon2id(password, header_salt)); re-encrypting with a fresh salt changes the key and the password can no longer unlock the vault. First vault-add impl used a fresh salt → vault-list/decrypt after add failed with AuthenticationFailed (caught by the vault-add tests). Fix: vault-add reuses the original header salt + params; only the nonce is fresh. This is a real bug the integration tests caught that unit tests could not.
- Golden-vector linkage: reused T2's exact frozen inputs (password `correct horse battery staple`, salt `[0x42;32]` hex `4242...42`, default params) as the CLI fixture password. `derive` prints `master_key=eae979a72a22bbe97f27910e712144453cdbf7d8d9abecad6a90cc72730f4cb1` / `vault_key=b7f0cc5b680771019ec1575c402653824a643fffe733125fc38bb3fa12831eeb` — byte-identical to T2's GOLDEN_MASTER_KEY/GOLDEN_VAULT_KEY. Pinned by an integration test.
- Integration tests drive the real binary via `env!("CARGO_BIN_EXE_passm-cli")` (set for integration tests of bin crates) and `env!("CARGO_MANIFEST_DIR")` for fixture paths. Temp dirs are pid-scoped (`passm-cli-<tag>-<pid>`) so parallel test runs can't collide.
- Golden ciphertext fixture (`vault.golden.passm1`) is generated ONCE by the CLI itself (encrypt with the T2 password) and committed; it is NOT byte-deterministic (fresh salt+nonce) but is stable — decrypt test just needs it to decrypt to the committed plaintext fixture.
- Argon2id 64 MiB ≈ 1-2 s/derive; the 6-test suite does 8 derives total (~14 s). Never loop derives in tests.
- Error mapping: typed `CliError` enum (Usage/Io/Json/Argon2/Envelope) + `From` impls; `main` returns `ExitCode::FAILURE` (exit 1) after printing to stderr — no panics in non-test code.
- Evidence: .omo/evidence/task-6-cli.txt

## [2026-08-17] T7 done: Android cross-compile spike — GO for git2-on-Android
- **git2 0.21 (vendored-libgit2+vendored-openssl+https) cross-compiles for aarch64-linux-android**: clean rebuild 2m 23s (vs 13m56s on Linux — Android NDK clang is faster than the box's gcc). keyring 4.1.6 android-native-keyring-store + chacha20poly1305/argon2/hkdf all compile. Binary is a valid aarch64 ELF (interpreter /system/bin/linker64).
- **cargo-ndk 4.1.2 gotchas**: (1) `ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/r26d` MUST be set — it does NOT auto-detect the NDK under ANDROID_HOME; (2) `-p` goes AFTER `build`: `cargo ndk -t arm64-v8a build -p passm-crypto`, not before.
- **git2-rs #920 (Android SSL certs, OPEN)**: Android has no /etc/ssl/certs; libgit2's OpenSSL backend can't find the trust store → "the SSL certificate is invalid" on HTTPS fetch. `SSL_CERT_DIR`/`set_ssl_cert_dir` don't help. Plan for T16: bundle Mozilla cacert.pem asset + set `SSL_CERT_FILE` or validate in `certificate_check` callback. NEVER accept-all (passm syncs encrypted vaults).
- **git2-rs #1174 (Tauri+SELinux, OPEN)**: `avc: denied { link }` on HEAD.lock in app_data_file on some devices. VERIFIED libgit2 1.9.6 uses `open(O_CREAT|O_EXCL)`+`rename()` for lockfiles, NOT `link()` (link only in local-clone copy opt) → #1174 is device/ROM-specific runtime noise, watch item for T16 real-device testing, not a compile blocker.
- **keyring android runtime**: `android-native-keyring-store` compiles but needs `io.crates.keyring.Keyring.initializeNdkContext(context)` in MainActivity.onCreate (Tauri 2.11+ removed auto init) — already planned in T8/T16.
- **Spike main.rs gotchas** (compile-proof only): `XChaCha20Poly1305::new` needs `use chacha20poly1305::KeyInit`; `git2::Repository::open` is generic → bare fn-pointer ref needs `::<&str>` turbofish.
- **DECISION: GO** — git2 on Android. Fallback (reqwest+rustls Contents API) NOT adopted. T9/T10/T16 proceed with git2 as designed.
- Evidence: .omo/evidence/task-7-android-spike.md; logs /tmp/opencode/t7-crypto.log + t7-spike.log; spike crate /tmp/opencode/t7-spike/ (uncommitted).

## [2026-08-17] T9 done: passm-sync git plumbing (git2 0.21)
- `git_repo.rs`: ensure_clone/fetch/push/is_fast_forward/current_head/remote_head/checkout_vault_file/write_vault_file/commit_vault_file. PAT via `Cred::userpass_plaintext("x-access-token", pat)` in a `RemoteCallbacks::credentials` closure on BOTH fetch and push opts — NEVER in the remote URL (would leak into .git/config). ensure_clone also repairs a drifted origin URL.
- **git2 0.21 API gotchas**: `Reference::shorthand()` returns `Result<&str, Error>` (NOT Option) → `.ok()` first. `Time::now()` does NOT exist → use `Signature::now(name, email)`. `Cred` has no username/password getters → test asserts `has_username()` + `credtype() == CredentialType::USER_PASS_PLAINTEXT.bits()`.
- **Tail-expression temporary E0597**: `match repo.head() { ... }` as a function tail keeps the temporary `Reference` alive past `repo`'s drop → "repo does not live long enough". Fix: bind the match result to a local (`let result = match ...; result`).
- **Non-FF detection**: libgit2 push.c returns GIT_ENONFASTFORWARD client-side when remote ref isn't an ancestor (verified in vendored libgit2-sys push.c:346/357). Detect via `e.code() == ErrorCode::NotFastForward` + message scan + `push_update_reference` rejection reason (3 layers).
- **Global-state test isolation**: module-global `REPO_DIR` must be `Mutex<Option<PathBuf>>` (NOT OnceLock — re-clone must repoint it), and tests sharing it MUST be serialized via a `static TEST_LOCK: Mutex<()>` guard, else parallel tests operate on another test's already-deleted tempdir ("No such file or directory").
- `push_update_reference` closure signature: `FnMut(&str, Option<&str>) -> Result<(), Error>` (Some = rejection reason). `RemoteCallbacks::credentials` bound is `+ 'a` (not 'static) — but capture-by-ref still needs the captured var defined BEFORE the callbacks var (drop order).
- checkout_vault_file reads the WORKING TREE (not HEAD tree) so T10 sees post-merge content before committing.
- TDD: RED (8 tests, missing functions) → GREEN (19/19 + 1 ignored keyring). Clippy `-D warnings` clean. Workspace green.
- Evidence: .omo/evidence/task-9-git.txt
