//! OS-level "system proxy" detection for the git transport.
//!
//! libgit2 never consults the system-wide proxy that desktop apps and
//! browsers use (Windows WinINET settings); it only knows git config
//! `http.proxy` and the standard environment variables. A user routing
//! traffic through a LAN/system proxy tool therefore sees sync failures
//! ("failed to send request") even though their browser reaches GitHub —
//! so we read the same settings the browser does, as a fallback after the
//! explicit env vars. Re-read on every call: a changed proxy address is
//! picked up on the next sync without any caching.
//!
//! Resolution order lives in `git_repo::proxy_from_env`:
//! Android-injected proxy > HTTPS_PROXY/ALL_PROXY/... env > [`system_proxy`].
//!
//! PAC scripts (`AutoConfigURL`) are intentionally not evaluated; if only a
//! PAC is configured there is no static proxy address to return.

/// Returns the OS-configured proxy as an `http://host:port` URL, or None.
///
/// Only Windows has a meaningful GUI-level system proxy today; on other
/// desktops libgit2's environment-variable support covers the common cases,
/// and this returns `None`.
pub(crate) fn system_proxy() -> Option<String> {
    #[cfg(windows)]
    {
        windows_system_proxy()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }
    let server: String = settings.get_value("ProxyServer").ok()?;
    parse_proxy_server(&server)
}

/// Parses a WinINET `ProxyServer` value into an `http://host:port` URL.
///
/// Accepted shapes:
///   - `"host:port"` — one server for all protocols
///   - `"http=h:p;https=h2:p2;ftp=..."` — per-protocol entries; an https
///     entry wins over http; ftp/gopher/socks entries are ignored because
///     the git transport speaks HTTP CONNECT
///
/// Returns None when nothing usable remains. Compiled on every platform so
/// the parsing rules stay unit-tested off-Windows too.
#[cfg(any(windows, test))]
fn parse_proxy_server(server: &str) -> Option<String> {
    let from_entry = |entry: &str| -> Option<String> {
        let (proto, addr) = entry.split_once('=')?;
        if !matches!(proto, "https" | "http") {
            return None;
        }
        // WinINET always writes host:port here; tolerate nothing looser.
        if addr.is_empty() || !addr.contains(':') {
            return None;
        }
        Some(format!("http://{addr}"))
    };

    if server.contains(';') || server.contains('=') {
        let mut http_fallback = None;
        for entry in server.split(';').filter(|e| !e.is_empty()) {
            match entry.split('=').next().unwrap_or("") {
                "https" => {
                    if let Some(url) = from_entry(entry) {
                        return Some(url);
                    }
                }
                "http" => http_fallback = http_fallback.or_else(|| from_entry(entry)),
                _ => {}
            }
        }
        http_fallback
    } else if server.contains(':') {
        // Bare host:port applies to every protocol.
        Some(format!("http://{server}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_proxy_server;

    #[test]
    fn bare_host_port_applies_to_all_protocols() {
        assert_eq!(
            parse_proxy_server("192.168.1.2:10808"),
            Some("http://192.168.1.2:10808".to_string())
        );
        assert_eq!(
            parse_proxy_server("[::1]:7890"),
            Some("http://[::1]:7890".to_string())
        );
    }

    #[test]
    fn per_protocol_prefers_https_entry() {
        assert_eq!(
            parse_proxy_server("http=10.0.0.1:8080;https=10.0.0.2:8443;ftp=10.0.0.3:21"),
            Some("http://10.0.0.2:8443".to_string())
        );
    }

    #[test]
    fn per_protocol_falls_back_to_http_entry() {
        assert_eq!(
            parse_proxy_server("ftp=h:21;http=10.0.0.1:8080"),
            Some("http://10.0.0.1:8080".to_string())
        );
    }

    #[test]
    fn ignores_socks_only_and_garbage() {
        assert_eq!(parse_proxy_server("socks=127.0.0.1:10808"), None);
        assert_eq!(parse_proxy_server("localhost"), None);
        assert_eq!(parse_proxy_server(""), None);
        assert_eq!(parse_proxy_server("https=novalue"), None);
    }
}
