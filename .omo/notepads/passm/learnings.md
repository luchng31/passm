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
