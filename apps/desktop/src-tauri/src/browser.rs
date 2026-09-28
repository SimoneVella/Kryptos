//! Registers the bundled `kryptos-native-host` with installed browsers, so the
//! extension can reach the app. Only writes to browsers that exist on disk.

use std::path::{Path, PathBuf};

use serde::Serialize;

pub const HOST_NAME: &str = "com.kryptos.bridge";
/// Fixed by the public key in extension/manifest.json.
pub const CHROME_EXTENSION_ID: &str = "jclbckdbmecjgoopnpchbijihdfeajkb";
pub const FIREFOX_EXTENSION_ID: &str = "kryptos@kryptos.local";

#[derive(Serialize)]
pub struct Registered {
    pub browsers: Vec<String>,
}

/// The sidecar sits next to the main executable (Contents/MacOS on macOS).
#[cfg_attr(not(all(unix, desktop)), allow(dead_code))]
pub fn host_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "kryptos-native-host.exe" } else { "kryptos-native-host" };
    let p = exe.parent().ok_or("no exe dir")?.join(name);
    if p.exists() { Ok(p) } else { Err(format!("native host not found at {}", p.display())) }
}

#[cfg(all(unix, desktop))]
fn targets() -> Vec<(&'static str, PathBuf, bool)> {
    let home = dirs::home_dir().unwrap_or_default();
    // (label, NativeMessagingHosts dir, is_firefox). The parent must exist = browser installed.
    #[cfg(target_os = "macos")]
    let list = {
        let s = home.join("Library/Application Support");
        vec![
            ("Chrome", s.join("Google/Chrome/NativeMessagingHosts"), false),
            ("Chromium", s.join("Chromium/NativeMessagingHosts"), false),
            ("Brave", s.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"), false),
            ("Edge", s.join("Microsoft Edge/NativeMessagingHosts"), false),
            ("Arc", s.join("Arc/User Data/NativeMessagingHosts"), false),
            ("Vivaldi", s.join("Vivaldi/NativeMessagingHosts"), false),
            ("Firefox", s.join("Mozilla/NativeMessagingHosts"), true),
        ]
    };
    #[cfg(not(target_os = "macos"))]
    let list = {
        let c = home.join(".config");
        vec![
            ("Chrome", c.join("google-chrome/NativeMessagingHosts"), false),
            ("Chromium", c.join("chromium/NativeMessagingHosts"), false),
            ("Brave", c.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"), false),
            ("Edge", c.join("microsoft-edge/NativeMessagingHosts"), false),
            ("Vivaldi", c.join("vivaldi/NativeMessagingHosts"), false),
            ("Firefox", home.join(".mozilla/native-messaging-hosts"), true),
        ]
    };
    list
}

#[cfg_attr(not(all(unix, desktop)), allow(dead_code))]
fn manifest(host: &Path, firefox: bool) -> serde_json::Value {
    let mut m = serde_json::json!({
        "name": HOST_NAME,
        "description": "Kryptos local vault bridge",
        "path": host,
        "type": "stdio",
    });
    if firefox {
        m["allowed_extensions"] = serde_json::json!([FIREFOX_EXTENSION_ID]);
    } else {
        m["allowed_origins"] = serde_json::json!([format!("chrome-extension://{CHROME_EXTENSION_ID}/")]);
    }
    m
}

#[cfg(all(unix, desktop))]
pub fn register() -> Result<Registered, String> {
    let host = host_binary()?;
    let mut browsers = Vec::new();
    for (label, dir, firefox) in targets() {
        if !dir.parent().is_some_and(Path::exists) {
            continue;
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let body = serde_json::to_vec_pretty(&manifest(&host, firefox)).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{HOST_NAME}.json")), body).map_err(|e| e.to_string())?;
        browsers.push(label.to_owned());
    }
    Ok(Registered { browsers })
}

#[cfg(all(unix, desktop))]
pub fn unregister() {
    for (_, dir, _) in targets() {
        let _ = std::fs::remove_file(dir.join(format!("{HOST_NAME}.json")));
    }
}

#[cfg(not(all(unix, desktop)))]
pub fn register() -> Result<Registered, String> {
    // TODO: HKCU\Software\Google\Chrome\NativeMessagingHosts\com.kryptos.bridge
    Err("unsupported_platform".into())
}

#[cfg(not(all(unix, desktop)))]
pub fn unregister() {}
