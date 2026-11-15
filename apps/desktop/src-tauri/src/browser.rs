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
#[cfg_attr(not(desktop), allow(dead_code))]
pub fn host_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "kryptos-native-host.exe" } else { "kryptos-native-host" };
    let p = exe.parent().ok_or("no exe dir")?.join(name);
    if p.exists() { Ok(p) } else { Err(format!("native host not found at {}", p.display())) }
}

/// The MCP server for AI assistants, bundled the same way (see crates/mcp-server).
#[cfg_attr(not(desktop), allow(dead_code))]
pub fn mcp_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "kryptos-mcp.exe" } else { "kryptos-mcp" };
    let p = exe.parent().ok_or("no exe dir")?.join(name);
    if p.exists() { Ok(p) } else { Err(format!("MCP server not found at {}", p.display())) }
}

/// The command line, bundled the same way (see crates/cli).
#[cfg_attr(not(desktop), allow(dead_code))]
pub fn cli_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "kryptos.exe" } else { "kryptos" };
    let p = exe.parent().ok_or("no exe dir")?.join(name);
    if p.exists() { Ok(p) } else { Err(format!("command line not found at {}", p.display())) }
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

#[cfg_attr(not(desktop), allow(dead_code))]
fn manifest(host: &Path, firefox: bool) -> serde_json::Value {
    let host_str = host.to_string_lossy();
    let host_clean = if cfg!(windows) { host_str.strip_prefix(r"\\?\").unwrap_or(&host_str) } else { &host_str };
    let mut m = serde_json::json!({
        "name": HOST_NAME,
        "description": "Kryptos local vault bridge",
        "path": host_clean,
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

#[cfg(all(windows, desktop))]
mod win_reg {
    use core::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    pub const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    pub const HKEY_LOCAL_MACHINE: isize = 0x8000_0002_u32 as i32 as isize;
    const KEY_READ: u32 = 0x0002_0019;
    const KEY_WRITE: u32 = 0x0002_0006;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const REG_OPTION_NON_VOLATILE: u32 = 0;
    const REG_SZ: u32 = 1;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(hKey: isize, lpSubKey: *const u16, ulOptions: u32, samDesired: u32, phkResult: *mut isize) -> i32;
        fn RegCreateKeyExW(
            hKey: isize,
            lpSubKey: *const u16,
            Reserved: u32,
            lpClass: *const u16,
            dwOptions: u32,
            samDesired: u32,
            lpSecurityAttributes: *const c_void,
            phkResult: *mut isize,
            lpdwDisposition: *mut u32,
        ) -> i32;
        fn RegSetValueExW(hKey: isize, lpValueName: *const u16, Reserved: u32, dwType: u32, lpData: *const u8, cbData: u32) -> i32;
        fn RegDeleteKeyW(hKey: isize, lpSubKey: *const u16) -> i32;
        fn RegCloseKey(hKey: isize) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    pub fn key_exists(root: isize, subkey: &str) -> bool {
        let subkey_w = wide(subkey);
        let mut hkey: isize = 0;
        let rc = unsafe { RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut hkey) };
        if rc == 0 {
            unsafe { RegCloseKey(hkey) };
            return true;
        }
        false
    }

    pub fn set_default_sz(subkey: &str, value: &Path) -> Result<(), String> {
        let subkey_w = wide(subkey);
        let value_str = value.to_string_lossy();
        let value_clean = value_str.strip_prefix(r"\\?\").unwrap_or(&value_str);
        let value_w = wide(value_clean);
        let bytes_len = u32::try_from(value_w.len() * 2).map_err(|_| "manifest path too long".to_string())?;
        let mut hkey: isize = 0;
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey_w.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            return Err(format!("registry create failed ({rc})"));
        }
        let set_rc = unsafe { RegSetValueExW(hkey, std::ptr::null(), 0, REG_SZ, value_w.as_ptr().cast(), bytes_len) };
        unsafe { RegCloseKey(hkey) };
        if set_rc != 0 {
            return Err(format!("registry write failed ({set_rc})"));
        }
        Ok(())
    }

    pub fn delete_key(subkey: &str) {
        let subkey_w = wide(subkey);
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, subkey_w.as_ptr()) };
    }
}

#[cfg(all(windows, desktop))]
struct WinTarget {
    label: &'static str,
    vendor_key: &'static str,
    local_dir: &'static str,
    firefox: bool,
}

#[cfg(all(windows, desktop))]
const WIN_TARGETS: &[WinTarget] = &[
    WinTarget { label: "Chrome", vendor_key: r"Software\Google\Chrome", local_dir: r"Google\Chrome\User Data", firefox: false },
    WinTarget { label: "Chromium", vendor_key: r"Software\Chromium", local_dir: r"Chromium\User Data", firefox: false },
    WinTarget { label: "Brave", vendor_key: r"Software\BraveSoftware\Brave-Browser", local_dir: r"BraveSoftware\Brave-Browser\User Data", firefox: false },
    WinTarget { label: "Edge", vendor_key: r"Software\Microsoft\Edge", local_dir: r"Microsoft\Edge\User Data", firefox: false },
    WinTarget { label: "Vivaldi", vendor_key: r"Software\Vivaldi", local_dir: r"Vivaldi\User Data", firefox: false },
    WinTarget { label: "Arc", vendor_key: r"Software\Arc", local_dir: r"Arc\User Data", firefox: false },
    WinTarget { label: "Firefox", vendor_key: r"Software\Mozilla", local_dir: r"Mozilla\Firefox", firefox: true },
];

#[cfg(all(windows, desktop))]
fn manifests_dir() -> PathBuf {
    kryptos_core::paths::data_dir().join("manifests")
}

#[cfg(all(windows, desktop))]
pub fn register() -> Result<Registered, String> {
    let host = host_binary()?;
    let dir = manifests_dir();
    let chrome_manifest = dir.join(format!("{HOST_NAME}.chrome.json"));
    let firefox_manifest = dir.join(format!("{HOST_NAME}.firefox.json"));

    let chrome_body = serde_json::to_vec_pretty(&manifest(&host, false)).map_err(|e| e.to_string())?;
    let firefox_body = serde_json::to_vec_pretty(&manifest(&host, true)).map_err(|e| e.to_string())?;
    kryptos_core::storage::write_atomic(&chrome_manifest, &chrome_body).map_err(|e| e.to_string())?;
    kryptos_core::storage::write_atomic(&firefox_manifest, &firefox_body).map_err(|e| e.to_string())?;

    let local = dirs::data_local_dir();
    let roaming = dirs::data_dir();
    let mut browsers = Vec::new();
    for target in WIN_TARGETS {
        let base_dir = if target.firefox { roaming.as_ref() } else { local.as_ref() };
        let installed = win_reg::key_exists(win_reg::HKEY_CURRENT_USER, target.vendor_key)
            || win_reg::key_exists(win_reg::HKEY_LOCAL_MACHINE, target.vendor_key)
            || base_dir.is_some_and(|d| d.join(target.local_dir).exists())
            || (target.label == "Arc"
                && local.as_ref().and_then(|d| std::fs::read_dir(d.join("Packages")).ok()).is_some_and(|rd| {
                    rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("TheBrowserCompany.Arc"))
                }));
        if !installed {
            continue;
        }
        let host_key = format!(r"{}\NativeMessagingHosts\{HOST_NAME}", target.vendor_key);
        let manifest_path = if target.firefox { &firefox_manifest } else { &chrome_manifest };
        win_reg::set_default_sz(&host_key, manifest_path)?;
        browsers.push(target.label.to_owned());
    }
    Ok(Registered { browsers })
}

#[cfg(all(windows, desktop))]
pub fn unregister() {
    for target in WIN_TARGETS {
        let host_key = format!(r"{}\NativeMessagingHosts\{HOST_NAME}", target.vendor_key);
        win_reg::delete_key(&host_key);
    }
    let dir = manifests_dir();
    let _ = std::fs::remove_file(dir.join(format!("{HOST_NAME}.chrome.json")));
    let _ = std::fs::remove_file(dir.join(format!("{HOST_NAME}.firefox.json")));
}

#[cfg(not(all(desktop, any(unix, windows))))]
pub fn register() -> Result<Registered, String> {
    Err("unsupported_platform".into())
}

#[cfg(not(all(desktop, any(unix, windows))))]
pub fn unregister() {}
