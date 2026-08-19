# passm - Work Plan

## TL;DR (For humans)

**What you'll get:** A personal password manager with two apps — one for Windows, one for Android — built from a single codebase. All your login credentials (title, username, password, URL, notes) live in one encrypted vault file on your device; the vault syncs privately through a GitHub repository that only you can access, so your passwords never touch a third-party server in readable form.

**Why this approach:** Rust + Tauri gives us battle-tested, auditable cryptography libraries and one codebase for both Windows and Android. A private GitHub repo gives free, no-credit-card sync. The vault is encrypted with Argon2id + XChaCha20-Poly1305 (the same class of crypto used by KeePass and age), and the format is tamper-evident — any change to the crypto parameters is detected.

**What it will NOT do:** No autofill, no browser extension, no TOTP/2FA codes, no importing from other password managers, no secure notes, no biometric unlock, no account sharing, no cloud provider beyond your own GitHub repo.

**Effort:** Large
**Risk:** Medium - the only real risk is compiling the git library (git2) for Android; we de-risk it with an early spike that has a documented fallback if it fails.

**Decisions to sanity-check:**
1. You need to create a dedicated **private GitHub repo** (separate from the source code repo) plus a **fine-grained PAT** with Contents Read+Write on just that repo. The app never stores the PAT in the repo — it lives in your OS keychain.
2. The Android app is signed with a debug key (fine for personal sideloading; you'll get an "unknown source" warning). The Windows installer is unsigned (SmartScreen warning) — we skip paid certs for v1.
3. If two devices edit the same entry, the higher internal version number wins; on an exact tie the entry is kept (no silent data loss) — and we back up the remote copy before any merge.

Your next move: approve this plan. Full execution detail follows below.

---

> TL;DR (machine): Large effort, Medium risk (Android git2 cross-compile spike is the single driver; fallback documented). Deliverables: passm-crypto / passm-vault / passm-sync / passm-cli crates + Tauri 2.x app (React+TS frontend) + Windows MSI (CI) + Android APK, all TDD.

## Scope
### Must have
- PASSM1 envelope: magic "PASSM1"(6B) + version(0x01) + memKiB u32BE + iter u32BE + parallelism u32BE + salt(32B) + nonce(24B) = 75 bytes, ALL as AAD to XChaCha20-Poly1305; ciphertext+16B tag at offset 75. Uncompressed canonical JSON payload.
- Key hierarchy: Argon2id(password, salt 32B, m=65536KiB/t=3/p=4) → 32B master key → HKDF-SHA256(IKM=master, salt=none, info="passm-v1-vault-key", L=32) → vault key. No stored master-password verifier (AEAD tag success IS the check).
- Vault model: Entry{id UUIDv4, title, username, password, url, notes, version u64, device_id String, created_at/updated_at unix secs, deleted bool tombstone}; Vault{entries}. Canonical serialization: entries sorted by id, stable field order.
- Merge rule (commutative + idempotent): per-entry higher version wins; equal version + both live → lexicographically higher device_id wins; equal version + one tombstone → tombstone wins (no-resurrect).
- Sync via git2 0.21 (`features = ["vendored-libgit2", "vendored-openssl", "https"]`) against dedicated PRIVATE GitHub repo, single file vault.enc. Local clone at `<app_data>/repo`. PAT (fine-grained, Contents R+W, single repo) via keyring 4.1.x (windows-native / android-native-keyring-store + Kotlin ndk-context init). `Cred::userpass_plaintext(PAT)` per op, never in .git/config. Repo-local user.name/email. INTERNET permission in AndroidManifest.
- Sync flow: ensure_clone (`git init -b main` first run, first push `-u origin main`) → fetch → HEAD==origin/main ? push : backup remote blob → `<app_data>/backups/vault.<unix>.enc` (retain 20) → decrypt both → merge → re-encrypt → commit → push; non-fast-forward → re-fetch+merge+retry (bounded).
- Every local save = write vault.enc + local commit.
- App: Tauri 2.x shell; session holds vault key in memory (zeroize on lock); auto-lock timer default 5 min; Windows tray (show/lock/quit); clipboard 30s auto-clear + clear on lock/quit.
- Commands: unlock, lock, list, get, create, update, delete, search, copy, generate_password (length param only), sync_now, get_sync_config, set_sync_config.
- Frontend React 18 + TS + Vite: unlock screen, list+search, item editor, copy buttons, sync status.
- Artifacts: MSI via GitHub Actions windows-latest at `src-tauri/target/release/bundle/msi/*.msi`; APK via `tauri android build` at `gen/android/app/build/outputs/apk/universal/release/*.apk`.
- TDD everywhere. CI gates: cargo test + clippy -D warnings + fmt --check + tsc --noEmit + vitest run.

### Must NOT have (guardrails, anti-slop, scope boundaries)
- NO gitoxide (gix): push is NOT implemented in gix 0.86.0 — must use git2.
- NO Cloudflare R2 / B2 / WebDAV: sync backend is GitHub private repo only.
- NO autofill service, NO biometric unlock, NO browser extension, NO multi-user/sharing/SSO/escrow/recovery.
- NO TOTP, NO import/export, NO secure notes, NO strength meter, NO passphrase generator, NO trash UI, NO multi-vault, NO cloud backup, NO per-item keys, NO key rotation, NO KDF benchmark tuner (fixed 64/3/4).
- NO unencrypted metadata beyond the format header; NO master-password verifier; NO password/PAT in logs or .git/config; NO unwrap()/panic in non-test code.
- NO compression in v1 (uncompressed JSON payload).
- NO Electron, NO Flutter, NO second framework.
- Vault sync repo and source-code repo MUST remain separate GitHub repos.

## Verification strategy
> Zero human intervention - all verification is agent-executed.
- Test decision: **TDD** (write failing tests first per unit, then implement). cargo test for Rust crates; vitest for frontend logic.
- Evidence: `<attemptDir>/task-<N>-${slug}.<ext>` (attemptDir = currentAttemptDir from 'omo ulw-loop status --json', .omo/evidence/ulw/<session>/<goalId>/a<attempt>; outside ulw-loop use `.omo/evidence/`). Save: test logs, golden-vector fixture outputs, cross-compile logs, artifact listings (ls -la), screenshots for UI.
- Golden vectors: fixed password/salt/nonce fixture → asserted constant ciphertext; HKDF cross-check vector pinned in test. Harness CLI: `cargo run -p passm-cli -- encrypt|decrypt --in <file> --out <file> --password <pw>`.
- Convergence proof: test asserts `merge(a,b)` byte-identical to `merge(b,a)` and idempotent; two-device e2e test (T17) proves no infinite non-FF loop.

## Execution strategy
### Parallel execution waves
- **Wave 1 (7 todos, parallel track):** foundation crates (T1), key derivation (T2), envelope (T3), vault models (T4), merge (T5), CLI harness (T6) — AND the **Android cross-compile spike (T7)** running in parallel from the start (it is the top risk and must fail fast with a documented fallback).
- **Wave 2 (3 todos):** keyring + device_id (T8, needs T7), git plumbing (T9, needs T3+T8), conflict merge (T10, needs T5+T9).
- **Wave 3 (4 todos):** Tauri shell + tray + auto-lock + session (T11), backend commands (T12, needs T6+T10+T11), frontend (T13), integration wiring + dev smoke (T14).
- **Wave 4 (3 todos):** CI + MSI (T15), Android APK + signing (T16), two-device convergence e2e (T17).
- Final: F1-F4 parallel verification wave, ALL must approve, then surface to user.

### Dependency matrix
| Todo | Depends on | Blocks | Can parallelize with |
| --- | --- | --- | --- |
| T1 scaffold | — | T2,T3,T4,T5,T6,T7 | — |
| T2 key derivation | T1 | T3 | T4,T5,T7 |
| T3 envelope | T2 | T6,T9,T10 | T4,T5,T7 |
| T4 vault models | T1 | T5,T6 | T2,T3,T7 |
| T5 merge | T4 | T6,T10 | T2,T3,T7 |
| T6 CLI harness | T2,T3,T4,T5 | T12 (verification seam) | T7 |
| T7 Android spike | T1 | T8,T16; risk decision | T2,T3,T4,T5,T6 |
| T8 keyring+device_id | T7 | T9 | T6 |
| T9 sync git plumbing | T3,T8 | T10 | T6 |
| T10 conflict merge | T5,T9 | T12,T17 | T6 |
| T11 Tauri shell | T1 | T12,T14,T16 | T2..T10 |
| T12 backend commands | T6,T10,T11 | T13,T14 | — |
| T13 frontend | T12 | T14 | — |
| T14 integration | T12,T13 | T15 | — |
| T15 CI+MSI | T1,T6,T12,T14 | F-wave | T16 |
| T16 APK+signing | T7,T11 | F-wave | T15 |
| T17 convergence e2e | T10,T12 | F-wave | T15,T16 |

## Todos
> Implementation + Test = ONE todo. Never separate.
<!-- APPEND TASK BATCHES BELOW THIS LINE WITH edit/apply_patch - never rewrite the headers above. -->
- [x] 1. Workspace scaffold + git2-on-Linux proof
  What to do / Must NOT do: Create cargo workspace at /home/ubuntu/passm with members `crates/passm-crypto`, `crates/passm-vault`, `crates/passm-sync`, `crates/passm-cli` (bin), `src-tauri` (Tauri 2 app placeholder crate). Add pinned deps to workspace Cargo.toml: git2 0.21, chacha20poly1305 0.11, argon2 0.5, hkdf 0.13, keyring 4.1, serde 1, uuid 1 (v4), zeroize 1, rand 0.8. Each crate gets lib.rs with a placeholder function + 1 smoke test. Verify git2 0.21 with `["vendored-libgit2","vendored-openssl","https"]` compiles on THIS Linux dev box (this de-risks the toolchain early). Must NOT: no .gitignore missing (add /target, /node_modules, /dist, *.local), no README unless asked, no bundler config yet. Must NOT: do not init git2 without vendored features (0.21 has `default = []` — https must be explicit).
  Parallelization: Wave 1 | Blocked by: — | Blocks: T2,T3,T4,T5,T6,T7
  References (executor has NO interview context - be exhaustive): draft /home/ubuntu/passm/.omo/drafts/passm.md (decisions + scope); Android verification report /home/ubuntu/.local/share/opencode/tool-output/tool_fff2adeec001rpPEnbIck8LVzf (git2 recipe L640-652: `git2 = { version = "0.21", features = ["vendored-libgit2", "vendored-openssl", "https"] }`, `default = []`); Tauri 2 docs https://v2.tauri.app/start/create-project/ and https://v2.tauri.app/start/prerequisites/.
  Acceptance criteria (agent-executable): `cargo build --workspace` succeeds; `cargo tree -i git2` shows git2 0.21 with features vendored-libgit2+vendored-openssl+https; `cargo test --workspace` green.
  QA scenarios (name the exact tool + invocation): happy: `cargo test --workspace` (all pass); failure: remove vendored features → expect build error → restore. Evidence `.omo/evidence/task-1-scaffold.txt` (build + tree output).
  Commit: Y | chore: scaffold cargo workspace with pinned deps

- [x] 2. passm-crypto: key derivation (Argon2id + HKDF) with golden vectors
  What to do / Must NOT do: In `crates/passm-crypto`, implement `KdfParams { mem_kib: u32 = 65536, iterations: u32 = 3, parallelism: u32 = 4 }` (serde), `derive_master_key(password: &[u8], salt: &[u8;32], params: &KdfParams) -> Result<[u8;32]>` using argon2 0.5 `Algorithm::Argon2id` + `Version::V0x13`, and `derive_vault_key(master: &[u8;32]) -> [u8;32]` using hkdf 0.13 HKDF-SHA256 with salt=none, info=`b"passm-v1-vault-key"`, L=32. TDD: (a) golden vector — freeze one derived master key + vault key for fixed password+salt+params as asserted constants in the test (generate once, then assert); (b) HKDF cross-check vector; (c) wrong password/salt → different key; (d) wrong params → different key. Must NOT: no logging of keys/passwords; zeroize password buffer after derive; no unwrap/panic.
  Parallelization: Wave 1 | Blocked by: T1 | Blocks: T3
  References: draft passm.md key-hierarchy line (HKDF IKM=master, salt=none, info="passm-v1-vault-key", 32B); argon2 0.5.3 docs https://docs.rs/argon2/0.5.3/argon2/ (Algorithm::Argon2id default); hkdf 0.13 docs https://docs.rs/hkdf/0.13.0/hkdf/; crypto research tool_fff0cdee2001tVjdpZJiVgw0zI (KDF rationale, RFC 9106 option 2).
  Acceptance criteria (agent-executable): `cargo test -p passm-crypto` green; golden-vector constants present in test file; `cargo clippy -p passm-crypto -- -D warnings` clean.
  QA scenarios: happy: run `cargo test -p passm-crypto` → all pass; failure: mutate a golden-vector constant → test fails → revert. Evidence `.omo/evidence/task-2-kdf.txt`.
  Commit: Y | feat(crypto): Argon2id+HKDF key derivation with golden vectors

- [x] 3. passm-crypto: PASSM1 envelope (encrypt/decrypt, AAD-bound header)
  What to do / Must NOT do: Implement `envelope::encrypt(vault_key: &[u8;32], params: &KdfParams, salt: [u8;32], plaintext: &[u8]) -> Vec<u8>` and `envelope::decrypt(vault_key: &[u8;32], blob: &[u8]) -> Result<Vec<u8>>`. Layout: bytes 0..5 magic b"PASSM1", byte 6 version 0x01, bytes 7..10 mem_kib u32 BE, 11..14 iterations u32 BE, 15..18 parallelism u32 BE, 19..50 salt, 51..74 nonce (24B from rand), ciphertext+16B tag from offset 75. AAD = bytes 0..74 inclusive (the whole 75-byte header). XChaCha20-Poly1305 via chacha20poly1305 0.11 `XChaCha20Poly1305`. Fresh random salt AND nonce per encrypt. TDD: roundtrip; wrong key → error; tamper EVERY header byte 0..74 (loop 75 times) → tag failure; tamper 3 ciphertext bytes → failure; version != 0x01 → reject; two encrypts with different nonce → different ciphertext; empty plaintext roundtrip; blob shorter than header → error. Must NOT: no compression; no plaintext metadata beyond header; no fixed nonce; no unwrap/panic.
  Parallelization: Wave 1 | Blocked by: T2 | Blocks: T6,T9,T10
  References: draft passm.md envelope decision (75-byte header ALL as AAD, tag at offset 75, uncompressed JSON payload); chacha20poly1305 0.11 docs https://docs.rs/chacha20poly1305/0.11.0/ (XChaCha20Poly1305, 24-byte XNonce); Android verification tool_fff2adeec001rpPEnbIck8LVzf L584 (pure Rust confirmed); Metis gap analysis in draft (byte-exactness pin).
  Acceptance criteria (agent-executable): `cargo test -p passm-crypto` green including 75-byte tamper loop; `cargo clippy -p passm-crypto -- -D warnings` clean.
  QA scenarios: happy: tamper test proves every header byte is authenticated; failure: flip a header byte in a fixture and run decrypt → error. Evidence `.omo/evidence/task-3-envelope.txt`.
  Commit: Y | feat(crypto): PASSM1 envelope with AAD-bound header

- [x] 4. passm-vault: Entry/Vault models + canonical serialization
  What to do / Must NOT do: In `crates/passm-vault`, define `Entry { id: Uuid, title: String, username: String, password: String, url: String, notes: String, version: u64, device_id: String, created_at: i64, updated_at: i64, deleted: bool }` and `Vault { entries: Vec<Entry> }` with serde derive (struct field order = stable key order). Provide `Entry::new(...) -> version=1, created_at/updated_at=now, deleted=false`, `bump()` (version+=1, updated_at=now), `mark_deleted()`. Provide `Vault::canonical_json() -> Vec<u8>` = serde_json::to_vec of a Vault whose entries are SORTED BY ID (stable). TDD: serde roundtrip; canonical_json byte-stable across two differently-ordered Vaults; id sort; new() defaults. Must NOT: no HashMap anywhere in serialization (order instability); no timestamps as String; no extra fields.
  Parallelization: Wave 1 | Blocked by: T1 | Blocks: T5,T6
  References: draft passm.md vault model + canonical-serialize decision; serde docs https://serde.rs/; uuid crate v4 docs https://docs.rs/uuid/.
  Acceptance criteria (agent-executable): `cargo test -p passm-vault` green; `cargo clippy -p passm-vault -- -D warnings` clean.
  QA scenarios: happy: canonical_json of two orderings is byte-identical; failure: inserting HashMap-based serde → canonical test fails → fix. Evidence `.omo/evidence/task-4-vault.txt`.
  Commit: Y | feat(vault): Entry/Vault models with canonical JSON

- [x] 5. passm-vault: commutative merge (version + device_id + tombstone)
  What to do / Must NOT do: Implement `merge(local: &Vault, remote: &Vault) -> Vault` (pure function, no I/O). Rule per entry id: take higher version; if equal version AND one is tombstone (deleted=true) → tombstone wins (no-resurrect); if equal version AND both live → lexicographically higher device_id wins; entry only in one side → taken as-is. Result serialized canonically (sorted by id). TDD: disjoint merge; higher-version wins both directions; equal-version live-vs-live tiebreak by device_id (and reverse argument order gives same winner → commutativity); tombstone-vs-live equal version → tombstone wins, and reverse → same (no-resurrect); **convergence: merge(a,b) byte-identical to merge(b,a) for randomized inputs (property test, ≥50 random cases); idempotence: merge(merge(a,b), b) == merge(a,b)**; deleted entry with strictly higher version does not resurrect on later merge with older live copy. Must NOT: no I/O/side effects; no "local wins" special case (draft correction — equal version is device_id tiebreak, NOT local-wins); no unwrap/panic.
  Parallelization: Wave 1 | Blocked by: T4 | Blocks: T6,T10
  References: draft passm.md merge-rule decision (verbatim rule) + Metis finding (contradiction "local wins" → lexicographic device_id; tombstone wins equal-version; no-resurrect; canonical serialize; convergence proof required).
  Acceptance criteria (agent-executable): `cargo test -p passm-vault` green incl. property tests (50 random merge pairs, assert commutativity byte-identical); `cargo clippy -- -D warnings` clean.
  QA scenarios: happy: random-input commutativity property test passes; failure: reintroduce local-wins → commutativity test fails. Evidence `.omo/evidence/task-5-merge.txt`.
  Commit: Y | feat(vault): commutative merge with version+device_id tiebreak

- [x] 6. passm-cli: core command-line harness (golden vectors + verification seam)
  What to do / Must NOT do: In `crates/passm-cli` (bin), subcommands: `derive --password <pw> --salt <hex> [--params ...]` prints master+vault key hex (for vector pinning); `encrypt --in <file> --out <file> --password <pw>` (read JSON plaintext, derive fresh salt, write PASSM1 blob); `decrypt --in <file> --out <file> --password <pw>` (inverse; nonzero exit on wrong password); `vault-add --vault <file> ...` / `vault-list --vault <file>` for core CRUD without UI. Produce a committed fixture: `crates/passm-cli/tests/fixtures/vault.plain.json` + pinned golden ciphertext file + the constants asserted in T2/T3. Must NOT: no key/password echoed to stdout beyond `derive` (explicit dev command only); no storing of the test password anywhere; no network calls.
  Parallelization: Wave 1 | Blocked by: T2,T3,T4,T5 | Blocks: T12 (agent-executable verification of core commands)
  References: draft passm.md (golden vectors acceptance: harness encrypt/decrypt on fixture files); Metis gap analysis (UI automation seam: core covered via CLI harness, UI smoke via vitest + manual F3).
  Acceptance criteria (agent-executable): `cargo run -p passm-cli -- derive --password test --salt 0000...00` prints deterministic keys; `encrypt` then `decrypt` roundtrips the fixture; wrong password → exit code != 0; fixture + golden files committed.
  QA scenarios: happy: roundtrip; failure: `decrypt` with wrong password exits nonzero. Evidence `.omo/evidence/task-6-cli.txt`.
  Commit: Y | feat(cli): core harness with golden-vector fixtures

- [x] 7. Android cross-compile spike: git2 + keyring for aarch64-linux-android (TOP RISK, fail fast)
  What to do / Must NOT do: On THIS Linux box, install Android NDK r26+/SDK + Java 17 per Tauri 2 prerequisites (https://v2.tauri.app/start/prerequisites/), then verify `cargo ndk -t arm64-v8a -p passm-crypto build` AND a spike crate depending on git2 0.21 (vendored-libgit2+vendored-openssl+https) + keyring 4.1 (`features=["android-native-keyring-store"]`) + chacha20poly1305 + argon2 + hkdf all compile for aarch64-linux-android. Save full build log. Also confirm `tauri android build` toolchain present (may be done in T16; here just verify NDK/SDK/Java versions). MUST document findings in `.omo/evidence/task-7-android-spike.md` INCLUDING git2-rs #920 (Android SSL cert validation open issue — plan to bundle CA roots / certificate_check callback) and #1174 (Tauri+SELinux). **If git2 fails to cross-compile after reasonable effort: record DECISION — fallback = reqwest+rustls GitHub Contents API on Android (same merge logic, git2 on Windows only) and update draft + this plan's T9/T10/T16 accordingly. Must NOT: do not block the whole plan — spike is time-boxed; do not modify product code beyond the spike crate.
  Parallelization: Wave 1 | Blocked by: T1 | Blocks: T8,T16; risk decision
  References: Android verification report tool_fff2adeec001rpPEnbIck8LVzf (full: git2 recipe L553-563 + open issues #920/#1174 L657-659; gitoxide push missing L574 → why git2 is mandatory; keyring v4 android feature L576-581 + ndk-context init L579 + issue #21; RustCrypto pure Rust L584-587; tauri-plugin-clipboard-manager Android text-only L590); cargo-ndk docs https://github.com/rust-mobile/cargo-ndk.
  Acceptance criteria (agent-executable): `cargo ndk -t arm64-v8a -p passm-crypto build` succeeds; spike crate with git2+keyring cross-compiles (build log saved); evidence md documents #920/#1174 + fallback decision.
  QA scenarios: happy: cross-compile log ends with "Finished"; failure: compile error → try documented workaround (cargo-ndk -p/llvm-ar config) → if stuck, record fallback decision. Evidence `.omo/evidence/task-7-android-spike.md` + build log.
  Commit: N (spike; findings committed via evidence file + plan/draft update if fallback)

- [x] 8. keyring PAT store + persistent device_id
  What to do / Must NOT do: In `crates/passm-sync`, `PatStore` over keyring 4.1.x: service `"passm"`, user `"github-pat"`, get/set/delete. Windows path = windows-native (default features); Android = `android-native-keyring-store` feature + Kotlin `io.crates.keyring.Keyring.initializeNdkContext(context)` wired in the Tauri Android project (Tauri 2.11+ removed auto ndk-context init — REQUIRED). `DeviceId`: UUIDv4 generated on first run, persisted to `<app_data>/device_id` (path via Tauri `path().app_data_dir()`), loaded on subsequent runs. TDD: PatStore get/set/delete against a mock/in-memory backend trait (so tests run on Linux CI); device_id persistence test (write → reload → same value; missing file → generates). Must NOT: PAT never written to disk outside keyring; never to .git/config; no hardcoded data_dir (use app_data_dir); no unwrap/panic.
  Parallelization: Wave 2 | Blocked by: T7 | Blocks: T9
  References: Android verification tool_fff2adeec001rpPEnbIck8LVzf L576-581 + L691-693 (keyring 4.1.6 android feature, ndk-context init Kotlin, Tauri 2.11 breakage, keyring-demo repo); keyring 4.1 docs https://docs.rs/keyring/4.1.6/keyring/; Tauri path API https://v2.tauri.app/reference/path/.
  Acceptance criteria (agent-executable): `cargo test -p passm-sync` green (PatStore mock + device_id tests); `cargo clippy -p passm-sync -- -D warnings` clean; Android Kotlin init snippet present in src-tauri/gen/android.
  QA scenarios: happy: device_id stable across reload; failure: keyring backend unavailable → typed error, app shows "配置 PAT" path. Evidence `.omo/evidence/task-8-keyring.txt`.
  Commit: Y | feat(sync): keyring PAT store and persistent device_id

- [x] 9. passm-sync: git plumbing (ensure_clone / fetch / fast-forward push / non-FF detection)
  What to do / Must NOT do: In `crates/passm-sync`, `SyncService { repo_dir, remote_url, pat_store, device_id }`. Operations via git2 0.21: `ensure_clone()` — if no repo at repo_dir: `git init -b main`, set repo-local user.name="passm", user.email="passm@local", add remote origin=<https URL>, initial commit of `vault.enc` (empty vault `{"entries":[]}`), first push `-u origin main` with `Cred::userpass_plaintext(username, PAT)` from PatStore; `fetch()`; `is_fast_forward()` (HEAD == origin/main); `push()` (uses PAT cred per op, credential callback NEVER touches .git/config); `save_local(vault_blob)` = write vault.enc + commit. Typed errors: PatMissing, RemoteUnreachable, NonFastForward, AuthFailed (surface 401 as "refresh PAT"). TDD: integration tests against LOCAL file:// bare temp repos (create with git2::Repository::init_bare): first-push to empty remote; fast-forward push; fetch detects remote advance; non-FF push → NonFastForward error; PatMissing when store empty. Must NOT: no SSH, no libssh2, no default-branch assumption other than main; no credentials stored in repo config; no unwrap/panic; no network in tests (file:// only).
  Parallelization: Wave 2 | Blocked by: T3,T8 | Blocks: T10
  References: draft passm.md sync flow + git2 recipe; Android verification tool_fff2adeec001rpPEnbIck8LVzf L553-563 (features incl. https for PAT auth; default=[]); git2 crate docs https://docs.rs/git2/0.21.0/git2/ (Repository::init_bare, RemoteCallbacks::credentials, Cred::userpass_plaintext, push with RefSpec).
  Acceptance criteria (agent-executable): `cargo test -p passm-sync` green (file:// integration suite); `cargo clippy -p passm-sync -- -D warnings` clean.
  QA scenarios: happy: local bare remote round-trips push/fetch; failure: empty PatStore → PatMissing error. Evidence `.omo/evidence/task-9-sync.txt`.
  Commit: Y | feat(sync): git2 clone/fetch/push plumbing with PAT creds

- [x] 10. passm-sync: conflict merge (backup-before-merge + converge + retry)
  What to do / Must NOT do: Implement `sync_now(vault_key, local_vault, plaintext_bytes)` full flow: ensure_clone → fetch → if fast-forward: push local (done). Else (remote advanced): (1) BACKUP remote blob: copy origin/main vault.enc → `<app_data>/backups/vault.<unix>.enc`, prune to retain last 20; (2) read remote blob, decrypt with vault_key (PASSM1) — on decrypt failure of REMOTE blob: do NOT clobber — return typed error, keep local; (3) merge(local, remote-decrypted) via passm-vault rule; (4) re-encrypt (fresh salt/nonce), write vault.enc, commit, push; (5) if push non-fast-forward again → re-fetch → re-merge → retry, bounded (max 3). TDD: integration tests with local file:// remotes: remote-advanced conflict creates backup file matching `vault.<unix>.enc` pattern; concurrent edits converge to same blob on both "devices"; remote decrypt-failure → error + local intact; backup pruning keeps ≤20; fast-forward path does NOT create backup. Must NOT: no data-loss paths (always backup before overwrite); no unbounded retry; no blocking main thread (async command in app layer); no unwrap/panic.
  Parallelization: Wave 2 | Blocked by: T5,T9 | Blocks: T12,T17
  References: draft passm.md sync-flow (backup remote → merge → re-encrypt → push; non-FF → re-fetch+merge+retry); Metis gap analysis (backup step blocker; backup-exists test `vault.<unix>.enc`; convergence proof).
  Acceptance criteria (agent-executable): `cargo test -p passm-sync` green incl. conflict/backup/convergence tests; `cargo clippy -p passm-sync -- -D warnings` clean.
  QA scenarios: happy: two local clones diverge → sync converges, backup exists; failure: corrupt remote blob → typed error, local untouched. Evidence `.omo/evidence/task-10-conflict.txt`.
  Commit: Y | feat(sync): backup-before-merge conflict resolution

- [x] 11. Tauri app shell: window, tray, auto-lock timer, session state
  What to do / Must NOT do: In `src-tauri`, set up Tauri 2.x app (tauri.conf.json: productName "passm", identifier e.g. com.passm.app, window ~1000x700, min sizes). SessionState (managed via tauri State / Mutex): `vault_key: Option<Zeroizing<[u8;32]>>`, `vault: Option<Vault>`, `device_id: String`, `unlocked_at`. `lock()` zeroizes key + drops vault. Auto-lock timer: default 5 min after unlock/last activity, fires lock (configurable via command). Windows tray: menu items Show / Lock / Quit; tray lock triggers the same lock path. Register tauri-plugin-clipboard-manager. `app_data_dir` resolution via `tauri::Manager::path()` — repo/backups/device_id live under it. TDD where feasible (pure state transitions unit-testable; timer via injected clock trait). Must NOT: no vault data in RAM beyond session (decrypted vault only while unlocked); no key persistence; no background auto-sync (sync on unlock + manual sync_now only); no unwrap/panic.
  Parallelization: Wave 3 | Blocked by: T1 | Blocks: T12,T14,T16
  References: draft passm.md (tray, auto-lock 5 min, session-holds-key, zeroize-on-lock, data_dir via app_data_dir); Tauri 2 docs: https://v2.tauri.app/learn/state-management/, https://v2.tauri.app/plugin/clipboard/, tray https://v2.tauri.app/learn/system-tray/; zeroize docs https://docs.rs/zeroize/.
  Acceptance criteria (agent-executable): `cargo build` in src-tauri succeeds; lock() zeroizes (unit test with injected session); tray menu registered on Windows build (verify in T15 CI artifact or dev on Linux with mock).
  QA scenarios: happy: auto-lock timer (injected short clock) fires lock and key is zeroized; failure: session without key → unlock screen. Evidence `.omo/evidence/task-11-shell.txt`.
  Commit: Y | feat(app): Tauri shell with tray, auto-lock, session state

- [x] 12. Backend commands (unlock/lock/CRUD/search/copy/generate/sync/config)
  What to do / Must NOT do: Implement Tauri commands (async, typed Result, Chinese error messages): `unlock(password)` (derive master+vault key from params in header/salt, decrypt vault.enc, load session, then trigger sync_now best-effort); `lock()`; `list`/`get`/`create`/`update`/`delete` (CRUD with Entry::new/bump/mark_deleted + device_id from session + save_local commit); `search(q)` (case-insensitive title/username/url); `copy(field)` (clipboard-manager write + 30s auto-clear timer + clear on lock/quit); `generate_password(length)` (cryptographic RNG charset A-Za-z0-9 + symbols, length param only); `sync_now()`; `get_sync_config`/`set_sync_config` (PAT → PatStore, remote URL → config store, then ensure_clone). Every command re-checks unlocked state → typed error. TDD: command logic extracted into testable functions (pure CRUD/merge orchestration testable without Tauri runtime); tauri command wrappers thin. Must NOT: no plaintext logging; no command that stores PAT into repo; no unwrap/panic; clipboard cleared on lock.
  Parallelization: Wave 3 | Blocked by: T6,T10,T11 | Blocks: T13,T14
  References: draft passm.md commands list + session/copy semantics; Tauri 2 command docs https://v2.tauri.app/develop/calling-rust/; clipboard plugin https://v2.tauri.app/plugin/clipboard/ (Android text-only).
  Acceptance criteria (agent-executable): `cargo test -p passm-app` (or workspace) green for command logic; `cargo clippy --workspace -- -D warnings` clean; CLI harness (T6) can drive create/list roundtrip through passm-cli equivalence.
  QA scenarios: happy: create → list → search → copy → lock → unlock roundtrip via tests; failure: command while locked → typed error. Evidence `.omo/evidence/task-12-commands.txt`.
  Commit: Y | feat(app): core Tauri commands

- [x] 13. Frontend: React 18 + TS + Vite (unlock, list/search, editor, copy, sync status)
  What to do / Must NOT do: Vite + React 18 + TypeScript strict at project root (per Tauri 2 template: `src/` frontend + `src-tauri/`). Screens: Unlock (password field, error display); Vault list (search box filtering title/username/url client-side via `invoke('search')` or local filter — pick local filter for responsiveness, test the filter logic); Item editor (create/edit form: title/username/password/url/notes, show/hide password, copy buttons, delete with confirm); sync status indicator + manual "sync now" button; lock button. Use `@tauri-apps/api` invoke wrappers with typed payloads. vitest: search filter logic (case-insensitive, multi-term), copy-timer behavior (mock timers). Must NOT: no plaintext passwords in browser storage/URLs/logs; no autofill; no biometric; keep components small (<250 LOC); no unstyled default — minimal clean CSS.
  Parallelization: Wave 3 | Blocked by: T12 | Blocks: T14
  References: draft passm.md frontend scope; Tauri 2 + Vite template https://v2.tauri.app/start/create-project/; React 18 docs; vitest https://vitest.dev/.
  Acceptance criteria (agent-executable): `tsc --noEmit` clean; `npm run test` (vitest) green; `npm run build` succeeds.
  QA scenarios: happy: vitest search-filter cases pass; failure: XSS attempt in title → rendered escaped (React default) — assert in a component test. Evidence `.omo/evidence/task-13-frontend.txt`.
  Commit: Y | feat(ui): React vault UI with search, editor, copy

- [x] 14. Frontend-backend integration + dev smoke
  What to do / Must NOT do: Wire all screens to commands via typed invoke wrappers; handle error toasts (Chinese messages from backend); unlock → list → edit → lock flows; tray lock event locks UI; auto-lock locks UI. Run `tauri dev` on this Linux box with a local file:// remote or empty config, manual smoke: create entry, copy, sync_now, lock, unlock. Save smoke notes + screenshot(s) to evidence. Must NOT: no hardcoded API keys; no skipping the locked-state UI; no unwrap/panic; no .gitignore bypass (frontend dist ignored).
  Parallelization: Wave 3 | Blocked by: T12,T13 | Blocks: T15
  References: Tauri 2 IPC docs https://v2.tauri.app/develop/calling-frontend/.
  Acceptance criteria (agent-executable): `tauri dev` launches; smoke checklist documented with evidence files; `tsc --noEmit` + vitest still green.
  QA scenarios: happy: end-to-end create→sync→lock→unlock on dev box; failure: invoke error → toast shown. Evidence `.omo/evidence/task-14-integration/` (notes + screenshot).
  Commit: Y | chore(app): wire frontend to backend commands

- [ ] 15. CI pipeline: tests + clippy + Windows MSI artifact
  > EXTERNAL BLOCKER (2026-08-19): GitHub Actions is disabled at the account
  > level ("Please reach out to GitHub Support"). Workflow file + rust-toolchain
  > committed (c9cedb4) and pushed (250cf82); local gates verified green
  > (fmt/clippy/test/tsc/vitest). CI run + MSI artifact pending account
  > re-enablement. Evidence: task-15-ci.txt (IN PROGRESS).
  What to do / Must NOT do: `.github/workflows/ci.yml`: job 1 ubuntu-latest — rust stable + node, gates: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `tsc --noEmit`, `vitest run`; job 2 windows-latest — `tauri build` producing MSI (WiX; install wixtoolset or use tauri's bundled NSIS/WiX per docs) at `src-tauri/target/release/bundle/msi/*.msi`, upload as artifact. Pin toolchain (rust-toolchain.toml stable + components clippy,rustfmt). Must NOT: no secrets in workflow (PAT is user-supplied at runtime, not CI); no skipping clippy; MSI unsigned (document SmartScreen warning in README/evidence).
  Parallelization: Wave 4 | Blocked by: T1,T6,T12,T14 | Blocks: F-wave
  References: Tauri 2 CI guide https://v2.tauri.app/develop/ci/; GitHub Actions docs; draft passm.md build decision (windows-latest runner; unsigned MSI v1).
  Acceptance criteria (agent-executable): push → both jobs green; MSI artifact downloadable; `ls src-tauri/target/release/bundle/msi/*.msi` on the windows job.
  QA scenarios: happy: CI green end-to-end; failure: clippy warning → job fails → fix. Evidence `.omo/evidence/task-15-ci.txt` (workflow run link + artifact listing).
  Commit: Y | ci: test gates and Windows MSI pipeline

- [x] 16. Android APK: tauri android build + signing + manifest
  What to do / Must NOT do: `tauri android init` (gen/android), add INTERNET permission to AndroidManifest, wire keyring Kotlin `initializeNdkContext` call in MainActivity.onCreate (per T8), then `tauri android build --apk` → APK at `gen/android/app/build/outputs/apk/universal/release/*.apk` signed with debug keystore (Tauri default). Verify `apksigner verify`. Document "unknown source" install warning. Must NOT: no Play Store/keystore purchase v1; no obfuscation config beyond defaults; no committing gen/android build outputs (gitignore); no unwrap/panic.
  Parallelization: Wave 4 | Blocked by: T7,T11 | Blocks: F-wave
  References: Android verification tool_fff2adeec001rpPEnbIck8LVzf L591 (clipboard text-only note), L579/691 (keyring init); Tauri android docs https://v2.tauri.app/start/prerequisites/ and https://v2.tauri.app/distribute/ (APK); draft passm.md (APK via tauri android build, debug keystore).
  Acceptance criteria (agent-executable): `tauri android build --apk` produces APK; `apksigner verify --print-certs <apk>` succeeds with debug cert; INTERNET permission present in merged manifest.
  QA scenarios: happy: APK builds and verifies; failure: cross-compile error → consult T7 spike evidence/fallback decision. Evidence `.omo/evidence/task-16-apk.txt`.
  Commit: Y | build(android): APK with debug signing and keyring init

- [x] 17. Two-device convergence e2e test
  What to do / Must NOT do: Integration test in passm-sync simulating two devices: shared file:// bare remote; device A (app_data_a, device_id A) creates entries + sync_now; device B (app_data_b, device_id B) clones + edits different entries + sync_now; assert both converge to byte-identical vault.enc content (decrypt both). Conflict scenario: both edit the SAME entry offline (different versions) → sync → merged per rule, backup exists, both converge. Assert: no non-FF loop (bounded retries, test completes); final blob decrypts with same vault key on both. Must NOT: no network; no sleeps-based flakiness (use deterministic commits); no unwrap/panic.
  Parallelization: Wave 4 | Blocked by: T10,T12 | Blocks: F-wave
  References: draft passm.md convergence decision; Metis gap analysis (convergence proof blocker: merge(a,b)==merge(b,a) byte-identical; idempotence; no infinite non-FF loop).
  Acceptance criteria (agent-executable): `cargo test -p passm-sync two_device` green; test asserts byte-identical convergence + backup file created.
  QA scenarios: happy: both devices converge; failure: merge non-commutativity → test fails → fix merge. Evidence `.omo/evidence/task-17-convergence.txt`.
  Commit: Y | test(sync): two-device convergence e2e

## Final verification wave
> Runs in parallel after ALL todos. ALL must APPROVE. Surface results and wait for the user's explicit okay before declaring complete.
- [x] F1. Plan compliance audit (oracle): every todo's acceptance criteria met; evidence files exist for T1-T17; artifacts (MSI, APK) produced at documented paths.
  > APPROVE 2026-08-19 (evidence task-fwave-verification.txt). MSI pending T15 external blocker; APK apksigner-verified byte-identical.
- [x] F2. Code quality review (oracle): clippy -D warnings clean across workspace; no unwrap/panic in non-test code; zeroize discipline verified; no secrets/PAT in code or .git/config; crypto reviewed against PASSM1 spec (AAD extent, nonce freshness, no verifier stored).
  > APPROVE 2026-08-19 after fixes: 3 non-test panic sites -> Result; PAT-shaped fixture value replaced + golden regenerated; PAT zeroized in ensure_repo_ready. All gates re-verified green (98 tests).
- [x] F3. Real manual QA (unspecified-high): run `tauri dev` on the Linux dev box; full user flow (unlock, add, search, copy + 30s clear, lock, auto-lock) with screenshot evidence; if an Android emulator is available, install APK and smoke-test unlock+list (else document as deferred with APK artifact verified by apksigner).
  > DEFERRED by user decision 2026-08-19 (GUI interaction deferred; APK already apksigner-verified). Run tauri dev manually when convenient.
- [x] F4. Scope fidelity (oracle): no Must NOT violated (git2 not gitoxide, GitHub not R2, no TOTP/import/bio/autofill, fixed KDF 64/3/4, no compression, separate vault repo from source repo, no unwrap/panic).
  > APPROVE 2026-08-19 after fixes: constraint 7 (non-test panics -> Result) and constraint 9 (.env added to .gitignore) closed.

## Commit strategy
- Conventional Commits: `feat(crypto|vault|sync|app|ui|cli): ...`, `fix(...)`, `test(...)`, `ci(...)`, `chore(...)`, `build(...)`.
- ONE commit per todo, after its tests are green and clippy clean. Atomic: no todo ships without its tests.
- Evidence files go in `.omo/evidence/` and are committed alongside their todo.
- Vault sync repo (user's private GitHub repo) is OUT OF SCOPE for the source repo — never push vault.enc or PAT here; the source repo only ever contains code + tests + evidence.
- No commit of: `target/`, `node_modules/`, `dist/`, `gen/android/**/build/`, `.env`, any `.git` internals.

## Success criteria
- All 17 todos complete with green tests; evidence present for every todo.
- Golden vectors pinned and passing (deterministic derive + fixed ciphertext for fixed inputs).
- Merge convergence proven: `merge(a,b) == merge(b,a)` byte-identical, idempotent, tombstone no-resurrect.
- Two-device sync demonstrated end-to-end (T17): converge to byte-identical vault, backups created on conflict, bounded retry.
- Artifacts: MSI from windows-latest CI (`src-tauri/target/release/bundle/msi/*.msi`) and APK (`gen/android/app/build/outputs/apk/universal/release/*.apk`, apksigner-verified).
- CI gates green on every push: cargo test + clippy -D warnings + fmt --check + tsc --noEmit + vitest run.
- F1-F4 final verification wave ALL approve; results surfaced to the user for explicit okay.
