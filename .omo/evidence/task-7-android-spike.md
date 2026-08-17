# T7: Android cross-compile spike — git2 + keyring for aarch64-linux-android

Date: 2026-08-17
Status: **DONE — GO for git2-on-Android**
Time-box: ~40 min (spent ~35 min)

## Environment (verified on this box)

| Component | Version |
| --- | --- |
| Java | openjdk 21.0.11 (2026-04-21) |
| Android SDK | `/home/ubuntu/Android/Sdk` — build-tools 34.0.0, cmdline-tools latest, platform-tools, platforms;android-34 |
| Android NDK | r26d (`/home/ubuntu/Android/Sdk/ndk/r26d`) |
| cargo / rustc | 1.97.1 |
| cargo-ndk | 4.1.2 |
| rustup target | `aarch64-linux-android` (installed) |
| Env vars required | `ANDROID_HOME=$HOME/Android/Sdk`, `ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/r26d`, `PATH=$HOME/.cargo/bin:$HOME/.local/bin:$PATH` |

## Build 1 — pure-Rust stack: `cargo ndk -t arm64-v8a build -p passm-crypto`

**RESULT: SUCCESS — `Finished` in 18.76s, exit 0.** Full log: `/tmp/opencode/t7-crypto.log`.

Compiled for aarch64-linux-android: chacha20poly1305 0.11.0, argon2 0.5.3, hkdf 0.13.0,
sha2 0.11.0, rand 0.8.7, zeroize 1.9.0, serde 1.0.229 — all pure Rust, no C deps, clean.

Two cargo-ndk gotchas hit and resolved:
1. **`ANDROID_NDK_HOME` must be set explicitly** — cargo-ndk 4.1.2 does NOT auto-detect
   the NDK under `$ANDROID_HOME/ndk/r26d` ("Could not find any NDK").
2. **`-p` must come AFTER `build`** — `cargo ndk -t arm64-v8a -p passm-crypto build`
   fails ("unexpected argument '-p'"); correct form is
   `cargo ndk -t arm64-v8a build -p passm-crypto`.

## Build 2 — spike crate: git2 + keyring + RustCrypto

Spike crate at `/tmp/opencode/t7-spike/` (OUTSIDE the workspace — not committed):

```toml
git2 = { version = "0.21", features = ["vendored-libgit2", "vendored-openssl", "https"] }
keyring = { version = "4.1", features = ["android-native-keyring-store"] }
chacha20poly1305 = "0.11"
argon2 = "0.5"
hkdf = "0.13"
sha2 = "0.11"
```

`main.rs` references every type so the linker pulls the real code paths:
`git2::Repository::open`, `RemoteCallbacks::new`, `FetchOptions::new`, `Cred::default`,
`keyring::Entry::new`, `XChaCha20Poly1305::new`, `Argon2::default`, `Hkdf::<Sha256>::new`.
No unwrap/panic — `main() -> Result<(), Box<dyn Error>>`.

**RESULT: SUCCESS — clean rebuild `Finished` in 2m 23s, exit 0.** Full log:
`/tmp/opencode/t7-spike.log`. Binary verified:

```
ELF 64-bit LSB pie executable, ARM aarch64, version 1 (SYSV), dynamically linked,
interpreter /system/bin/linker64, with debug_info, not stripped
```

Full dependency graph that cross-compiled (from `cargo tree -e features` + build log):

| Crate | Version | Notes |
| --- | --- | --- |
| git2 | 0.21.0 | vendored-libgit2 + vendored-openssl + https |
| libgit2-sys | 0.18.7+1.9.6 | vendored libgit2 1.9.6, C compiled by NDK clang |
| openssl-sys | 0.9.117 | vendored |
| openssl-src | 300.6.1+3.6.3 | OpenSSL 3.6.3 built from source |
| libz-sys | 1.1.29 | zlib for libgit2 |
| keyring | 4.1.6 | android-native-keyring-store |
| android-native-keyring-store | 1.0.0 | JNI store backend |
| ndk-context | 0.1.1 | requires runtime init (see below) |
| jni | 0.21.1 | JNI bindings |
| chacha20poly1305 / argon2 / hkdf | 0.11.0 / 0.5.3 / 0.13.0 | pure Rust |

Spike main.rs compile fixes (spike-only, not product code): `XChaCha20Poly1305::new`
needs `use chacha20poly1305::KeyInit` in scope; `Repository::open` is generic over
`P: AsRef<Path>` so a bare fn-pointer reference needs `::<&str>` turbofish.

## git2-rs #920 — Android SSL cert validation (OPEN, documented)

https://github.com/rust-lang/git2-rs/issues/920 — opened Jan 2023, still OPEN.

- Symptom: HTTPS fetch fails on Android with
  `Git(Error { code: -17, klass: 16, message: "the SSL certificate is invalid" })`.
  `vendored-openssl` does NOT help.
- Root cause: Android has no standard CA bundle path (no `/etc/ssl/certs`). libgit2's
  OpenSSL backend cannot find the system trust store. `set_ssl_cert_dir` / `SSL_CERT_DIR`
  do not help — Android stores system certs in a different format/location.
- Why reqwest works: `native-tls` loads Android system certs by default (converts
  PEM→X509 at runtime). libgit2's OpenSSL backend has no equivalent.
- Workarounds from the thread:
  1. `git2::RemoteCallbacks::certificate_check(|_, _| Ok(CertificateCheckStatus::CertificateOk))`
     — supported API, but accept-all = MITM risk.
  2. Patch libgit2 source to skip the `SSL_get_verify_result` check (hacky, forks the vendored C).
  3. jgit via jni-rs (not applicable — we need git2's merge logic).

**PASSM PLAN (T9/T16):** bundle Mozilla CA roots (`cacert.pem`) as an app asset and
validate properly — set `SSL_CERT_FILE` to the bundled pem before any fetch, OR use
`certificate_check` to verify the presented chain against the bundled roots. **Do NOT
accept-all** — passm syncs encrypted vaults over HTTPS; a MITM on the cert check is
unacceptable. This is a runtime wiring task for T16, not a compile blocker.

## git2-rs #1174 — Tauri + Android SELinux `link` denial (OPEN, documented)

https://github.com/rust-lang/git2-rs/issues/1174 — opened Jun 2025, still OPEN, no comments.

- Symptom (Tauri app): `Repository::init` in the app data dir fails with
  `avc: denied { link } for name="HEAD.lock" ... scontext=u:r:untrusted_app:s0:... tcontext=u:object_r:app_data_file:s0:... tclass=file permissive=0`.
- Context: Android SELinux does not grant the `link` permission to the `untrusted_app`
  domain on `app_data_file` (AOSP `app.te` grants `create_file_perms` only; Termux
  issue #837 confirms `link()` is disallowed for regular apps).
- **Verified against libgit2 1.9.6 source (vendored in git2 0.21):** lockfile creation
  uses `open(O_CREAT|O_EXCL)` (`git_futils_creat_locked`) and commit uses `rename()`
  (`git_filebuf`). `link()` appears ONLY in `git_futils_cp_r` with `GIT_CPDIR_LINK_FILES`
  (the local-clone copy optimization). So the exact `link()` trigger in #1174 is not
  reproducible from libgit2 1.9.6 alone — likely an older libgit2 or another component
  in the reporter's stack.
- PASSM IMPACT: runtime, device/ROM-specific watch item for T16 real-device testing.
  passm sync = network clone/fetch/push (never the local-clone link path). If a real
  device denies repo init/ref-write: (a) test on stock AOSP devices first, (b) pre-create
  the repo structure via plain file writes, (c) last resort — reqwest+rustls Contents
  API on that device only.

## keyring Android runtime requirement (already documented in T8)

keyring 4.1.6 `android-native-keyring-store` cross-compiles, but at runtime the JNI
store needs the NDK context initialized. Tauri 2.11+ removed automatic ndk-context
init, so `io.crates.keyring.Keyring.initializeNdkContext(context)` must be called in
`MainActivity.onCreate` (wired in T16; documented in `pat_store.rs` doc comment).

## DECISION: **GO** — git2 on Android

- git2 0.21 (vendored libgit2 1.9.6 + OpenSSL 3.6.3 + https) cross-compiles cleanly
  for aarch64-linux-android: clean rebuild 2m 23s, exit 0, valid aarch64 ELF.
- keyring 4.1.6 android-native-keyring-store cross-compiles (runtime ndk-context init
  already planned in T8/T16).
- Pure-Rust crypto stack cross-compiles (18.76s).
- Both open git2-rs issues (#920 SSL certs, #1174 SELinux) are RUNTIME concerns with
  documented mitigations; neither blocks the compile path.
- **Fallback NOT needed:** reqwest+rustls GitHub Contents API on Android is NOT adopted.
  T9/T10/T16 proceed with git2 on Android as designed. No plan/draft update required.

## Artifacts

- Build logs: `/tmp/opencode/t7-crypto.log`, `/tmp/opencode/t7-spike.log`
- Spike crate: `/tmp/opencode/t7-spike/` (NOT committed — outside workspace)
- Commit: N (spike; findings committed via this evidence file)