# Issues — passm

Problems and gotchas encountered during work on this plan.

_Auto-scaffolded by /start-work. Append new entries below - never overwrite._

---

## [2026-08-14] Toolchain blockers on dev box (T1 blocked)
- rustc 1.75.0 (apt, /usr/bin) is TOO OLD for Tauri 2 (needs >= 1.77.2). rustup was absent.
- rustup stable download via static.rust-lang.org is EXTREMELY slow (~3-4 MB/min; timed out twice at 300s/600s). crates.io sparse index works fine (HTTP 200).
- cmake was missing; installed via `pip3 install --user --break-system-packages cmake` → 4.4.2 at `~/.local/bin/cmake` (no sudo available — sudo requires password).
- User took over Rust installation themselves ("我来装rust"); my rustup install was removed (~/.cargo, ~/.rustup gone). User's `curl https://sh.rustup.rs` seen running in pts/1 at 16:26.
- T1 marked `- [~]` BLOCKED in plan until `rustc --version` >= 1.77.2 works. Resume T1 immediately once toolchain is ready.
- Java 17 and Android SDK/NDK also NOT installed (needed for T7/T16) — check after Rust is resolved.
