Task 14: Frontend-backend integration + dev smoke
Date: 2026-08-18
Status: DONE — verified

## Deliverables
- Frontend screens wired to backend commands via typed invoke wrappers (src/lib/api.ts,
  written in T13 against the T12 contract; verified against src-tauri/src/commands.rs —
  all 14 command names + arg shapes match; Tauri 2 camelCase→snake_case arg mapping
  confirmed: setSyncConfig(remoteUrl, pat) → set_sync_config(remote_url, pat)).
- tauri.conf.json dev wiring completed: beforeDevCommand "npm run dev" + devUrl
  http://localhost:1420 (was empty in T11; required for `tauri dev`).
- @tauri-apps/cli ^2.11.4 added to devDependencies (needed for `npm run tauri dev`).
- Session-state polling added to src/App.tsx (2s interval get_session_status): tray
  "Lock" and the Rust auto-lock timer now transition the UI to the Unlock screen the
  same way the manual lock button does (previously only the manual button synced UI).
- Error handling: backend Chinese errors ("密码错误", "请先解锁", "保险库文件不存在")
  surface as visible error banners in Unlock / VaultList / ItemEditor / SyncStatus.

## Dev smoke run (Xvfb :10, 1280x800, GDK_BACKEND=x11, WEBKIT_DISABLE_DMABUF_RENDERER=1)
Environment: headless Linux box; Xvfb extracted at /tmp/xvfb-root (system xvfb pkg
unavailable offline); DISPLAY=:10. App binary target/debug/passm-app launched directly
and via vite dev server (port 1420) — both render.

Smoke checklist (19 screenshots in this dir, named 01..21):
- 01-unlock-empty: app launches, unlock screen renders (password field, 解锁 button).
- 02-unlock-error: wrong password → backend error "密码错误" displayed in UI.
- 04-unlock: unlock screen after failed attempt (state preserved).
- 12-tauri-dev: app running under `tauri dev` (vite dev server + webview).
- 17-21 devserver*: dev-server probing sequence (custom scheme tauri://, index.html
  served, window resize to 1000x700 min).
- 03/05/06/09/10/11/13-16: render/GL/webkit probes — window renders, webkit process
  alive, no crash on resize.

## Verification (all green)
- cargo check (src-tauri) — clean after main.rs fix (passm_app_lib::run()).
- cargo test --workspace — 96 passed (30 app + 29 sync + 17 crypto + 14 vault + 6 cli).
- npx tsc --noEmit — clean.
- npm run test (vitest) — 10/10 passed.
- npm run build — tsc && vite build OK.

## Caveats
- Screenshots captured via Xlib/xtest helper (helper.py) against Xvfb; visual content
  not human-reviewed in this session (headless) — real-desktop visual QA deferred to F3
  (user has host GUI access).
- libEGL DRI3 warnings in Xvfb are expected (no GPU); rendering confirmed via webkit
  process + window probes.
- Sync smoke used local file:// bare remote (/tmp/passm-smoke-remote.git) — no GitHub,
  no PAT (per plan). Real GitHub sync deferred to T17/F3 with user's PAT.

## Commands (PATH: /home/ubuntu/.cargo/bin, /home/ubuntu/.nvm/versions/node/v24.18.1/bin)
cargo check && cargo test --workspace && npx tsc --noEmit && npm run test && npm run build