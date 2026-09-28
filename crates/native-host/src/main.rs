//! Launched by the browser (never by the user) when the Kryptos extension calls
//! `chrome.runtime.connectNative`. It holds no secrets and never touches the
//! vault file: it only relays messages between the browser (stdin/stdout,
//! 4-byte length-prefixed JSON) and the running desktop app (Unix socket,
//! newline-delimited JSON). No network sockets are opened.
//!
//! It refuses to relay anything unless (1) its parent process is a browser with a
//! valid signature from a trusted vendor, and (2) the browser says the caller is
//! the Kryptos extension. This stops other local programs from spawning the host
//! and feeding it requests. The app separately verifies that its peer is this binary.

use std::io::{self, BufRead, BufReader, Read, Write};

use serde_json::{json, Value};

/// Chrome caps host->browser messages at 1 MiB; apply the same limit inbound.
const MAX_MSG: usize = 1024 * 1024;

const CHROME_ORIGIN: &str = "chrome-extension://jclbckdbmecjgoopnpchbijihdfeajkb/";
const FIREFOX_ID: &str = "kryptos@kryptos.local";

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let trusted = caller_is_trusted();

    while let Some(msg) = read_message(&mut input) {
        let reply = match msg {
            _ if !trusted => json!({ "ok": false, "error": "untrusted_caller" }),
            Ok(v) => relay(&v).unwrap_or_else(|e| json!({ "ok": false, "error": e })),
            Err(e) => json!({ "ok": false, "error": e }),
        };
        if write_message(&mut output, &reply).is_err() {
            break;
        }
    }
}

/// Chrome passes the calling extension's origin as argv[1]; Firefox passes the
/// manifest path and then the extension ID.
fn caller_is_trusted() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let origin_ok = args.first().is_some_and(|a| a == CHROME_ORIGIN) || args.get(1).is_some_and(|a| a == FIREFOX_ID);
    origin_ok && parent_is_browser()
}

#[cfg(target_os = "macos")]
fn parent_is_browser() -> bool {
    let ppid = unsafe { libc::getppid() };
    kryptos_core::peer::satisfies(ppid, &kryptos_core::peer::trusted_browser_requirement())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn parent_is_browser() -> bool {
    // No kernel-backed code signing on Linux: fall back to the executable name.
    const BROWSERS: &[&str] = &["chrome", "chromium", "brave", "msedge", "firefox", "firefox-bin", "vivaldi-bin"];
    let ppid = unsafe { libc::getppid() };
    kryptos_core::peer::exe_path(ppid)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .is_some_and(|n| BROWSERS.contains(&n.as_str()))
}

#[cfg(not(unix))]
fn parent_is_browser() -> bool {
    false
}

fn read_message(r: &mut impl Read) -> Option<Result<Value, String>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?; // EOF: browser closed the port.
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_MSG {
        return None;
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).ok()?;
    Some(serde_json::from_slice(&buf).map_err(|_| "invalid json".to_string()))
}

fn write_message(w: &mut impl Write, v: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(v)?;
    w.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

#[cfg(unix)]
fn relay(msg: &Value) -> Result<Value, String> {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = kryptos_core::paths::ipc_socket_path();
    let mut stream = UnixStream::connect(&path).map_err(|_| "app_not_running".to_string())?;
    // Unlock prompts are handled in the popup, so the app should answer quickly.
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

    // Re-serialize: compact JSON never contains a raw newline, which frames the socket protocol.
    let mut line = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
    line.push(b'\n');
    stream.write_all(&line).map_err(|e| e.to_string())?;

    let mut resp = String::new();
    BufReader::new(stream.take(MAX_MSG as u64)).read_line(&mut resp).map_err(|e| e.to_string())?;
    serde_json::from_str(&resp).map_err(|_| "bad reply from app".to_string())
}

#[cfg(not(unix))]
fn relay(_msg: &Value) -> Result<Value, String> {
    // TODO: Windows named pipe (\\.\pipe\kryptos-bridge-<user SID>).
    Err("unsupported_platform".into())
}
