//! Android system-proxy plumbing.
//!
//! The Android system HTTP proxy (Settings.Global.HTTP_PROXY, typically set
//! by per-app proxy apps like v2rayNG's HTTP inbound) is not visible to
//! libgit2 through the standard `HTTPS_PROXY` env var — Android apps cannot
//! read their own env vars from Java, and libgit2 never reads the system
//! settings. `MainActivity.kt` calls `Keyring.setHttpProxy(...)` (JNI) on
//! startup with the proxy string from `Settings.Global`, and we stash it in a
//! process-global that `git_repo::proxy_from_env` consults before falling back
//! to the environment.

use std::sync::OnceLock;

static ANDROID_PROXY: OnceLock<Option<String>> = OnceLock::new();

/// Registers the Android system HTTP proxy (host:port, no scheme) for use by
/// the git transport. Idempotent: only the first call wins.
#[cfg(target_os = "android")]
pub fn set_android_proxy(proxy: Option<String>) {
    let _ = ANDROID_PROXY.get_or_init(|| proxy);
}

/// Returns the proxy string to use for git operations: the Android system
/// proxy takes precedence over the environment, because on Android the env
/// vars are never set by the OS and can only come from this module.
#[cfg(target_os = "android")]
pub fn android_proxy() -> Option<&'static str> {
    ANDROID_PROXY.get().and_then(|p| p.as_deref())
}

#[cfg(not(target_os = "android"))]
pub fn android_proxy() -> Option<&'static str> {
    None
}

/// JNI entry point called by `MainActivity.kt` (`Keyring.setHttpProxy`) with
/// the system HTTP proxy from `Settings.Global.HTTP_PROXY`. The string is
/// host:port (no scheme); the git transport prepends `http://`.
///
/// The function name must match the Kotlin declaration exactly:
/// `io.crates.keyring.Keyring.setHttpProxy(proxy: String?)`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_crates_keyring_Keyring_00024Companion_setHttpProxy(
    mut _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    proxy: jni::objects::JString,
) {
    let proxy: Option<String> = if proxy.is_null() {
        None
    } else {
        let raw = _env.get_string(&proxy);
        raw.ok().map(|s| s.into())
    };
    set_android_proxy(proxy);
}