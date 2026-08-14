---
slug: passm
status: approved
intent: clear
review_required: false
pending-action: execute .omo/plans/passm.md via /start-work (worker session)
approach: Tauri 2.x (Rust core + web frontend) single codebase for Windows + Android; crypto-core + sync as testable Rust crates (TDD); encrypted single-blob vault (Argon2id + XChaCha20-Poly1305, params-in-AAD); GitHub private repo sync via git2 with entry-level merge at unlock; MSI via GitHub Actions CI, APK via tauri android build; then final verification wave.
---

# Draft: passm

## Components (topology ledger)
| id | outcome (one line) | status | evidence path |
| --- | --- | --- | --- |
| crypto-core | Tamper-evident encrypted vault blob; correct key derivation, AEAD, envelope format | active | crypto research (OWASP/RFC9106/KDBX4/age/USENIX 2026) + RustCrypto Android verified |
| sync | Two devices converge on one vault via private GitHub repo (git2); conflicts merged without data loss | active | Cloudflare research + Android crate verification (git2 0.21 recipe) |
| desktop-app | Windows client: unlock, list/search, edit, copy, tray, sync at unlock + manual sync_now | active | framework research (Tauri 2.x) |
| mobile-app | Android client: unlock, list/search, edit, copy, sync | active | framework research (Tauri 2.x Android) + crate verification |
| build-pipeline | Reproducible installers (MSI + APK), signing, CI, local dev loop | active | framework research (Tauri bundler) |

## Open assumptions (announced defaults)
| assumption | adopted default | rationale | reversible? |
| --- | --- | --- | --- |
| KDF | Argon2id, FIXED m=64 MiB (65536 KiB) / t=3 / p=4 (RFC 9106 option 2). NO runtime benchmark tuner in v1. | OWASP minimums are for auth, not encryption; Bitwarden uses 32/6/4, KeePass 64MB/2/p2 | yes (params live in authenticated header/AAD) |
| AEAD | XChaCha20-Poly1305 (192-bit/24-byte nonce, misuse-resistant, no AES-NI needed on ARM) | eliminates the #1 self-rolled failure (nonce reuse); deployed in libsodium/Tink/Go/WireGuard/age | yes |
| Envelope | PASSM1: magic "PASSM1"(6B) + version(0x01) + memKiB u32BE(4B) + iter u32BE(4B) + p u32BE(4B) + salt(32B) + nonce(24B) = bytes 0..75; AAD = bytes 0..74; ciphertext+16B tag at offset 75. Uncompressed canonical JSON payload. | KDBX4/age model; anti-downgrade (params in AAD) | yes (version byte) |
| Key hierarchy | passphrase -> Argon2id(64/3/4, salt=32B random per save) -> master key(32B) -> HKDF-SHA256(IKM=master, salt=none, info="passm-v1-vault-key", out=32B) -> vault key. Per-item keys OUT v1. | rotation/blast-radius; exact HKDF params stated so devices don't diverge | yes |
| Sync design | Single encrypted blob `vault.enc` in dedicated PRIVATE GitHub repo (never source). git2 crate, local clone at `<app_data>/repo`. Auth: fine-grained PAT (Contents R+W, single repo) in OS keyring (keyring 4.1.x, Windows Credential Manager / Android Keystore). | free + no card; client-side conflict detection via git | yes |
| Merge rule | Per-entry: higher version wins; equal version + both live -> lexicographically higher device_id wins; equal version + one tombstone -> tombstone wins (no-resurrect). Canonical serialize (entries sorted by id). Backup remote blob before merge. | convergence (merge(a,b)==merge(b,a)), no infinite non-FF loop | no (format-level) |
| First-run bootstrap | Empty remote (no README): `git init -b main` locally, first push `-u origin main`; empty vault = `{"entries":[]}`. Distinct states: fetch-failure vs empty-remote. | HEAD==origin/main breaks on empty remote | yes |
| No master-password verifier stored | Correct AEAD tag == password check (KeePass model) | a stored verifier is an offline-guessing oracle | yes |
| Clipboard | Auto-clear after 30s; ALSO cleared on lock and on quit | standard manager behavior | yes |
| git2 HTTPS backend | vendored-openssl everywhere (Windows/Linux/Android); Android has open cert-validation issue (git2-rs #920) -> bundle CA roots / certificate_check callback planned | verified recipe | yes |
| Signing | APK: debug keystore locally; MSI: unsigned (SmartScreen warning documented); no paid certs v1 | personal use | yes |

## Findings (cited - path:lines)

**Crypto core** (librarian research, Aug 2026):
- KDF: OWASP min Argon2id 19MiB/t2/p1 (auth only); RFC 9106: 2GiB/t1/p4 or 64MiB/t3/p4. Bitwarden new accounts: Argon2id 32MiB/t6/p4 (sdk-internal kdf.rs L155-L179); KeePass 2.57.1: Argon2 64MB/t2/p2; KeePassXC: default Argon2d 64MiB. -> adopt 64/3/4.
- AEAD: incumbents use AES-256-CBC+HMAC (Bitwarden), AES-256-CBC/ChaCha20+block HMAC (KeePass KDBX4), AES-256-GCM (Proton Pass). None use XChaCha20-Poly1305. XChaCha20 = draft-irtf-cfrg-xchacha-03 (expired IRTF doc) but deployed in libsodium/Tink/Go/WireGuard/age. chacha20poly1305 0.11.0 confirmed pure Rust with 24-byte XNonce.
- Key commitment: AES-GCM not key-committing (Invisible Salamander, eprint 2019/016; Albertini USENIX 2022). age binds key via HKDF.
- Pitfalls: KDF downgrade via unauthenticated params (Bitwarden BW07, USENIX 2026 / palant.info), cut-and-paste/field-swap without vault-level MAC (zkae.io), legacy unauthenticated fallbacks (BW12), nonce reuse (NIST SP 800-38D 2^32 limit), memory hygiene (KeePass CVE-2023-32784).

**Sync backend** (librarian research, verified against developers.cloudflare.com Aug 14 2026):
- R2 free tier generous but **requires payment method on file (~$5 hold)** -> user chose GitHub private repo.
- GitHub private repo: free, no card, 100MB file cap, no server-side conditional writes -> client-side git merge design.
- Alternatives rejected: B2 (no conditional writes), TeraCLOUD/InfiniCLOUD WebDAV (account expiry), KV/D1 (eventual consistency).

**Framework** (librarian research, Aug 2026):
- Tauri 2.x stable, current 2.11.x (Jul 2026). Windows solid; Android min API 26, system WebView; 1Password uses Tauri. Flutter close second. Electron: no Android.

**Android crate verification** (librarian, Aug 14 2026, against docs.rs + GitHub issues):
- git2 0.21.0: `default = []`; recipe `features = ["vendored-libgit2", "vendored-openssl", "https"]`. Cross-compiles to aarch64-linux-android (issue #754 solved by vendored-openssl). OPEN issues: #920 SSL cert invalid on Android (since Jan 2023, needs custom cert handling/bundled CA), #1174 Tauri+SELinux link denial (Jun 2025). Build needs cmake + perl + make + NDK toolchain.
- gitoxide 0.86.0: pure Rust, cross-tested on Android (#1895), PAT auth works, BUT **push NOT implemented** (gix-protocol 0.64.0 has no push module; Direction::Push unused) -> NOT viable for sync. Re-evaluate when gix lands push.
- keyring 3.6.3: NO android feature. keyring 4.1.6: `android-native-keyring-store` feature (store crate 1.0.0, Jul 2026); needs ndk-context init - Tauri 2.11+ removed auto-init -> Kotlin `io.crates.keyring.Keyring.initializeNdkContext(context)` in onCreate (issue #21).
- RustCrypto chacha20poly1305 0.11.0 (XChaCha20-Poly1305, 24B nonce), argon2 0.5.3 (Argon2id default), hkdf 0.13.0: all pure Rust, no Android issues.
- tauri-plugin-clipboard-manager: Windows + Android (plain text only). tauri-plugin-biometric: Android+iOS only (NOT used in v1).

**Metis gap analysis** (Aug 14 2026) - folded into decisions below; highlights: merge tiebreak "local wins" was WRONG (fixed), biometric contradiction removed, R2 topology removed, backup-before-merge added, envelope/HKDF byte-exactness pinned, first-run bootstrap added, convergence + golden vectors acceptance added.

## Decisions (with rationale)
- Route: CLEAR -> interview surviving forks only.
- review_required: false.
- **USER-ANSWERED FORKS (Aug 14 2026):**
  1. Framework: **Tauri 2.x / Rust**.
  2. Feature scope: **仅登录凭据** (login credentials: title/username/password/url/notes + search + clipboard + tiny password generator). TOTP/import/notes OUT.
  3. Sync backend: **GitHub 私有仓库** (over R2 - avoids card requirement). Consequences: client-side conflict detection via git; entry-level merge at unlock.
  4. Test strategy: **TDD**.
- Sync design (decision-complete):
  - Remote: dedicated PRIVATE GitHub repo (never source code), contains one file `vault.enc`. Create repo WITHOUT README.
  - Local clone: `<app_data>/repo` via Tauri `path().app_data_dir()` on both OSes. Auth: fine-grained PAT (Contents: Read+Write, single repo) in OS keyring via keyring 4.1.x (`android-native-keyring-store` feature on Android + Kotlin ndk-context init; windows-native default on Windows). git2 `Cred::userpass_plaintext(PAT as password)` each op, never stored in .git/config. repo-local user.name/email set.
  - Sync flow (after unlock): ensure_clone (`git init -b main` on first run) -> fetch -> if HEAD==origin/main: push local commits (fast-forward). If remote advanced: copy origin/main vault.enc -> `<app_data>/backups/vault.<unix>.enc` (retain last 20), decrypt both blobs, merge entries (higher version wins; equal -> lexicographically higher device_id wins; tombstone wins equal-version), re-encrypt, commit, push. Push non-fast-forward -> re-fetch+merge+retry.
  - Every local save = write vault.enc + local commit.
  - Entry.version: create -> 1; every modify/delete bumps by 1. Entry.device_id = last-modifying device's id (persisted per install at `<app_data>/device_id`).
  - Canonical serialize: entries sorted by id, stable key order. Merge must be commutative + idempotent (convergence).
- Envelope format (decision-complete): file = "PASSM1"(6B) + 0x01 + memKiB u32BE + iter u32BE + p u32BE + salt(32B) + nonce(24B) -> bytes 0..74 (75 bytes) are AAD to XChaCha20-Poly1305; ciphertext+16B tag at offset 75. Any tampering with params/salt/version breaks the tag (anti-downgrade). Payload: uncompressed canonical JSON. Master key never stored; AEAD tag success == password check (no verifier).
- Key hierarchy (decision-complete): Argon2id(password, salt=32B fresh) -> 32B master key; HKDF-SHA256(IKM=master, salt=none, info="passm-v1-vault-key", 32B) -> vault key.
- Build: dev on this Linux box (`cargo test`/`clippy` + `cargo-ndk`/`tauri android build` produce APK); Windows MSI via GitHub Actions windows-latest runner. CI gates: cargo test + clippy -D warnings + fmt --check + tsc --noEmit + vitest run.
- App UX: unlock/lock (vault key held in memory, zeroize on lock), auto-lock timer default 5 min, Windows tray (show/lock/quit), clipboard 30s auto-clear + clear on lock/quit. Commands: unlock, lock, list, get, create, update, delete, search, copy, sync_now, get/set sync config, generate_password (length param only).
- Autofill service OUT in v1; biometric unlock OUT in v1 (plugin exists but deferred). NO browser extension, NO sharing/multi-user, NO import, NO TOTP, NO strength meter, NO trash UI, NO key rotation.

## Scope IN
- Personal-use password vault: login credentials (title/username/password/URL/notes), search, copy-to-clipboard with auto-clear, edit/add/delete, tiny password generator.
- Master passphrase unlock with Argon2id-derived key; zero-knowledge vs GitHub provider.
- Encrypted single-blob vault; Windows (Tauri 2.x Rust) + Android clients sharing crypto/sync core; GitHub private repo sync with entry-level merge at unlock.
- Installable apps (MSI on Windows via CI, APK on Android); TDD + agent-executed tests/QA; CLI/dev test harness for crypto-core (golden vectors).

## Scope OUT (Must NOT have)
- NO multi-user, sharing, server-side accounts, browser extension, autofill service v1, biometric unlock v1.
- NO key escrow / account recovery / SSO / key rotation / per-item keys.
- NO storing any unencrypted metadata beyond the format header (AAD); NO master-password verifier.
- NO Electron. NO Flutter. NO building both frameworks.
- NO TOTP / import / secure-notes / strength meter / passphrase generator / trash UI / multi-vault / cloud backup.
- NO runtime KDF benchmark tuner (fixed 64/3/4).
- Vault sync repo and source-code repo MUST be separate GitHub repos.

## Open questions
- (none - all forks answered Aug 14 2026: Tauri 2.x / credentials-only / GitHub private repo / TDD)

## Approval gate
status: approved (user approved direct execution Aug 14 2026)
plan: /home/ubuntu/passm/.omo/plans/passm.md (decision-complete, 17 todos, waves + dependency matrix + F1-F4 verification)