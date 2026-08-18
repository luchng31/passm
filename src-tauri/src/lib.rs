//! passm-app: Tauri 2 desktop shell — window, tray, auto-lock timer, session state.
//!
//! All testable logic lives in [`session`] as pure functions; this module only
//! wires them into Tauri commands, the tray menu, and the auto-lock timer.

pub(crate) mod commands;
mod session;

use session::{lock_session, should_auto_lock, Clock, SessionState, SystemClock};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Default auto-lock timeout: 5 minutes after unlock/last activity.
pub const AUTO_LOCK_TIMEOUT_SECS: u64 = 300;
/// Auto-lock timer check interval.
const AUTO_LOCK_CHECK_INTERVAL_SECS: u64 = 30;

/// Managed app paths, resolved once at startup via `app.path().app_data_dir()`.
/// Repo/backups/device_id live under this directory (T12 uses it).
#[derive(Clone)]
pub struct AppPaths {
    /// `app_data_dir` for this platform (never hardcoded).
    pub data_dir: PathBuf,
}

/// Lock the session, zeroizing the vault key and dropping the decrypted vault,
/// and clear the clipboard (a copied secret must not outlive the session).
/// Shared by the `lock` command, the tray Lock item, and the auto-lock timer.
fn lock_session_state(app: &AppHandle) {
    let state = app.state::<Mutex<SessionState>>();
    let mut guard = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    lock_session(&mut guard);
    let _ = app.clipboard().clear();
}

/// Snapshot of the session for the frontend (no secrets).
#[derive(serde::Serialize)]
struct SessionStatus {
    unlocked: bool,
    device_id: String,
}

fn session_status(state: &Mutex<SessionState>) -> SessionStatus {
    let guard = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    SessionStatus {
        unlocked: guard.vault_key.is_some(),
        device_id: guard.device_id.clone(),
    }
}

/// Focus the main window (tray Show / tray icon click).
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Build the tray icon with Show / Lock / Quit menu items.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &lock, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "lock" => lock_session_state(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Background thread that checks the auto-lock timeout every 30s and locks the
/// session when `unlocked_at + AUTO_LOCK_TIMEOUT_SECS <= now`.
fn spawn_auto_lock_timer(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(AUTO_LOCK_CHECK_INTERVAL_SECS));
        let state = app.state::<Mutex<SessionState>>();
        let now = SystemClock.now_unix();
        let due = {
            let guard = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            should_auto_lock(&guard, now, AUTO_LOCK_TIMEOUT_SECS)
        };
        if due {
            lock_session_state(&app);
        }
    });
}

/// Lock the session (same path as the tray Lock item and the auto-lock timer).
#[tauri::command]
async fn lock(app: AppHandle) -> Result<(), String> {
    lock_session_state(&app);
    Ok(())
}

/// Report whether the session is unlocked and the device id (no secrets).
#[tauri::command]
async fn get_session_status(app: AppHandle) -> Result<SessionStatus, String> {
    let state = app.state::<Mutex<SessionState>>();
    Ok(session_status(state.inner()))
}

/// Application entry point (desktop and mobile).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Resolve the app data dir once; repo/backups/device_id live under it.
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppPaths { data_dir });
            app.manage(Mutex::new(SessionState::default()));
            setup_tray(app)?;
            spawn_auto_lock_timer(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lock,
            get_session_status,
            commands::unlock,
            commands::list,
            commands::get,
            commands::create,
            commands::update,
            commands::delete,
            commands::search,
            commands::copy,
            commands::generate_password,
            commands::sync_now,
            commands::get_sync_config,
            commands::set_sync_config
        ])
        .run(tauri::generate_context!())
}