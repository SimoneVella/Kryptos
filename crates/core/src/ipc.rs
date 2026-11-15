//! Local IPC client shared by `kryptos-native-host`, `kryptos-cli`, and
//! `kryptos-mcp` to talk to the running desktop app.
//!
//! Uses a private Unix domain socket on macOS/Linux and a local-only Named
//! Pipe on Windows, checking the server binary's location (and code signature
//! where available) before sending a request.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::time::Duration;

fn same_bundle_dir(server_exe: &Path) -> bool {
    let Ok(self_exe) = std::env::current_exe().and_then(|p| p.canonicalize()) else {
        return false;
    };
    let Some(self_dir) = self_exe.parent() else {
        return false;
    };
    server_exe.canonicalize().ok().is_some_and(|p| p.parent() == Some(self_dir))
}

#[cfg(unix)]
fn verify_unix_server(stream: &std::os::unix::net::UnixStream) -> Result<(), &'static str> {
    use std::os::fd::AsRawFd;

    let Some(pid) = crate::peer::socket_peer_pid(stream.as_raw_fd()) else {
        return Err("untrusted_peer");
    };
    let Some(server_exe) = crate::peer::exe_path(pid) else {
        return Err("untrusted_peer");
    };
    if !same_bundle_dir(&server_exe) {
        return Err("untrusted_peer");
    }
    #[cfg(target_os = "macos")]
    {
        if !crate::peer::valid_signature(pid) {
            return Err("untrusted_peer");
        }
        if let Some(team) = crate::peer::own_team_id() {
            if !crate::peer::satisfies(pid, &format!("anchor apple generic and certificate leaf[subject.OU] = \"{team}\"")) {
                return Err("untrusted_peer");
            }
        }
    }
    Ok(())
}

/// Sends a single compact JSON line to the running desktop app and reads one
/// reply line (up to `max_reply` bytes), timing out after `timeout`.
#[cfg(unix)]
pub fn send(line: &str, timeout: Duration, max_reply: u64) -> std::result::Result<String, String> {
    use std::os::unix::net::UnixStream;

    let path = crate::paths::ipc_socket_path();
    let mut stream = UnixStream::connect(&path).map_err(|_| "app_not_running".to_string())?;
    verify_unix_server(&stream).map_err(str::to_string)?;
    stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;

    let mut reply = String::new();
    BufReader::new(stream.take(max_reply)).read_line(&mut reply).map_err(|e| e.to_string())?;
    Ok(reply)
}

#[cfg(windows)]
pub fn send(line: &str, _timeout: Duration, max_reply: u64) -> std::result::Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        fn WaitNamedPipeW(lpNamedPipeName: *const u16, nTimeOut: u32) -> i32;
    }

    const ERROR_PIPE_BUSY: i32 = 231;

    let pipe = crate::paths::ipc_pipe_name();
    let pipe_w: Vec<u16> = std::ffi::OsStr::new(&pipe).encode_wide().chain(Some(0)).collect();

    let mut file = None;
    for attempt in 0..15 {
        match std::fs::OpenOptions::new().read(true).write(true).open(&pipe) {
            Ok(f) => {
                file = Some(f);
                break;
            }
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && attempt < 14 => {
                unsafe { WaitNamedPipeW(pipe_w.as_ptr(), 200) };
            }
            Err(_) => return Err("app_not_running".to_string()),
        }
    }
    let mut file = file.ok_or_else(|| "app_not_running".to_string())?;

    let Some(pid) = crate::peer::pipe_server_pid(file.as_raw_handle()) else {
        return Err("untrusted_peer".to_string());
    };
    let Some(server_exe) = crate::peer::exe_path(pid) else {
        return Err("untrusted_peer".to_string());
    };
    if !same_bundle_dir(&server_exe) {
        return Err("untrusted_peer".to_string());
    }
    if let Ok(self_exe) = std::env::current_exe() {
        if crate::peer::valid_authenticode(&self_exe) && !crate::peer::valid_authenticode(&server_exe) {
            return Err("untrusted_peer".to_string());
        }
    }

    file.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.flush().ok();

    let mut reply = String::new();
    BufReader::new(file.take(max_reply)).read_line(&mut reply).map_err(|e| e.to_string())?;
    Ok(reply)
}

#[cfg(not(any(unix, windows)))]
pub fn send(_line: &str, _timeout: Duration, _max_reply: u64) -> std::result::Result<String, String> {
    Err("unsupported_platform".to_string())
}
