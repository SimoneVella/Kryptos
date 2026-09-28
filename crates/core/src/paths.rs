//! Well-known locations shared by the desktop app and the native-messaging host.

use std::path::PathBuf;
use std::sync::OnceLock;

static OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Mobile apps call this at startup with their sandboxed app-data directory.
pub fn set_data_dir(dir: PathBuf) {
    let _ = OVERRIDE.set(dir);
}

pub const APP_ID: &str = "com.kryptos.desktop";

/// `~/Library/Application Support/com.kryptos.desktop` on macOS,
/// `~/.local/share/com.kryptos.desktop` on Linux, `%APPDATA%\com.kryptos.desktop` on Windows.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = OVERRIDE.get() {
        return dir.clone();
    }
    // Debug builds only: lets dev/e2e runs use a throwaway vault.
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("KRYPTOS_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_ID)
}

pub fn vault_path() -> PathBuf {
    data_dir().join("vault.kryptos")
}

/// Unix domain socket the desktop app listens on for the browser bridge.
/// Not a TCP port: unreachable from the network, guarded by filesystem permissions.
pub fn ipc_socket_path() -> PathBuf {
    data_dir().join("bridge.sock")
}
