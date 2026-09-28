//! Browser bridge: a Unix domain socket served to `kryptos-native-host`.
//!
//! Defences, from the outside in:
//! 1. The socket lives in a 0700 directory and is itself 0600: only processes
//!    of the same OS user can connect.
//! 2. Every connection is checked: the peer must be the `kryptos-native-host`
//!    shipped inside this app bundle (by path, with an intact code signature,
//!    and - when the app is Developer ID signed - the same Team ID).
//! 3. The native host itself only relays when its parent is a browser signed
//!    by a trusted vendor and the caller is the Kryptos extension.
//! 4. Credentials are only released for a URL that matches the entry (eTLD+1),
//!    at most `MAX_FILLS_PER_MINUTE` times a minute, and every release is
//!    shown in the app.
//!
//! Protocol: one JSON request line -> one JSON response line.
//!   {"type":"status"}                          -> {"ok":true,"unlocked":bool}
//!   {"type":"logins","url":U}                  -> {"ok":true,"logins":[{id,title,username}]}
//!   {"type":"credentials","id":I,"url":U}      -> {"ok":true,"username":..,"password":..}
//!   {"type":"focus"}                           -> brings the app window forward (to unlock)

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use kryptos_core::matching::entry_matches;

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Status,
    Logins { url: String },
    Credentials { id: String, url: String },
    Focus,
}

/// Caps bulk extraction even through a legitimate channel.
const MAX_FILLS_PER_MINUTE: usize = 20;
static FILLS: Mutex<VecDeque<Instant>> = Mutex::new(VecDeque::new());

fn fill_allowed() -> bool {
    let mut q = FILLS.lock().unwrap();
    while q.front().is_some_and(|t| t.elapsed() > Duration::from_secs(60)) {
        q.pop_front();
    }
    if q.len() >= MAX_FILLS_PER_MINUTE {
        return false;
    }
    q.push_back(Instant::now());
    true
}

/// Is the process on the other end of `stream` our own bundled native host?
#[cfg(unix)]
fn peer_is_our_host(stream: &std::os::unix::net::UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    use kryptos_core::peer;

    let Some(pid) = peer::socket_peer_pid(stream.as_raw_fd()) else { return false };
    let (Some(exe), Ok(expected)) = (peer::exe_path(pid), crate::browser::host_binary()) else { return false };
    let same_file = match (exe.canonicalize(), expected.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if !same_file {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        if !peer::valid_signature(pid) {
            return false;
        }
        // Release builds signed with a Developer ID: require the same team, so a
        // swapped binary at the same path cannot pass.
        if let Some(team) = peer::own_team_id() {
            return peer::satisfies(pid, &format!("anchor apple generic and certificate leaf[subject.OU] = \"{team}\""));
        }
    }
    true
}

fn handle(app: &AppHandle, line: &str) -> Value {
    let state = app.state::<AppState>();
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(_) => return json!({ "ok": false, "error": "bad_request" }),
    };
    let result = match req {
        Request::Status => Ok(json!({ "ok": true, "unlocked": state.is_unlocked() })),
        Request::Focus => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            Ok(json!({ "ok": true }))
        }
        Request::Logins { url } => state.read(|v| {
            let logins: Vec<Value> = v
                .entries()
                .iter()
                .filter(|e| entry_matches(e, &url))
                .map(|e| json!({ "id": e.id, "title": e.title, "username": e.username }))
                .collect();
            Ok(json!({ "ok": true, "logins": logins }))
        }),
        Request::Credentials { id, url } => {
            let res = state.read(|v| {
                let id = uuid::Uuid::parse_str(&id).map_err(|_| "invalid_id")?;
                let e = v.get(id).map_err(|_| "not_found")?;
                if !entry_matches(e, &url) {
                    return Err("url_mismatch".into());
                }
                if !fill_allowed() {
                    return Err("rate_limited".into());
                }
                Ok((e.title.clone(), json!({ "ok": true, "username": e.username, "password": e.password })))
            });
            res.map(|(title, reply)| {
                let host = url::Url::parse(&url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
                let _ = app.emit("browser-fill", json!({ "title": title, "host": host }));
                reply
            })
        }
    };
    result.unwrap_or_else(|e| json!({ "ok": false, "error": e }))
}

#[cfg(unix)]
pub fn start(app: AppHandle) {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let path = kryptos_core::paths::ipc_socket_path();
    // sun_path is 104 bytes on macOS, 108 on Linux.
    if path.as_os_str().len() >= 104 {
        eprintln!("bridge: socket path too long ({} bytes): {}", path.as_os_str().len(), path.display());
        return;
    }
    if let Some(dir) = path.parent() {
        if kryptos_core::storage::ensure_private_dir(dir).is_err() {
            eprintln!("bridge: cannot create {}", dir.display());
            return;
        }
    }
    let _ = std::fs::remove_file(&path); // stale socket from a previous run
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bridge: bind failed: {e}");
            return;
        }
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || {
                if !peer_is_our_host(&stream) {
                    let mut s = &stream;
                    let _ = s.write_all(b"{\"ok\":false,\"error\":\"untrusted_peer\"}\n");
                    return;
                }
                let mut line = String::new();
                let reader = stream.try_clone().map(|s| BufReader::new(s.take(1024 * 1024)));
                if let Ok(mut r) = reader {
                    if r.read_line(&mut line).is_ok() {
                        let mut out = serde_json::to_vec(&handle(&app, line.trim_end())).unwrap_or_default();
                        out.push(b'\n');
                        let mut s = stream;
                        let _ = s.write_all(&out);
                    }
                }
            });
        }
    });
}

#[cfg(not(unix))]
pub fn start(_app: AppHandle) {
    // TODO: Windows named pipe with an ACL restricted to the current user SID.
}
