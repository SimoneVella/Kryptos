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

/// Named pipe path for the local IPC bridge on Windows (`\\.\pipe\kryptos-bridge-<tag>`),
/// derived deterministically from [`data_dir`] so each user (and each test's
/// `KRYPTOS_DATA_DIR`) has its own isolated pipe.
pub fn ipc_pipe_name() -> String {
    use sha2::{Digest, Sha256};

    let dir = data_dir().to_string_lossy().to_lowercase();
    let digest = Sha256::digest(dir.as_bytes());
    let mut hex = String::with_capacity(16);
    for b in &digest[..8] {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    format!(r"\\.\pipe\kryptos-bridge-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_deterministic_and_local() {
        let a = ipc_pipe_name();
        let b = ipc_pipe_name();
        assert_eq!(a, b);
        assert!(a.starts_with(r"\\.\pipe\kryptos-bridge-"));
        assert_eq!(a.len(), r"\\.\pipe\kryptos-bridge-".len() + 16);
    }
}
