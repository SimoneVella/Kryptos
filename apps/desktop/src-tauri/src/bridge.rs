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
//!    after the user confirms that fill in the app window (Touch ID, the Mac
//!    password or the master password, see confirm.rs), at most
//!    `MAX_FILLS_PER_MINUTE` times a minute, and every release is shown in the app.
//!
//! Protocol: one JSON request line -> one JSON response line.
//!   {"type":"status"}                          -> {"ok":true,"unlocked":bool}
//!   {"type":"logins","url":U}                  -> {"ok":true,"logins":[{id,title,username}]}
//!   {"type":"credentials","id":I,"url":U}      -> {"ok":true,"username":..,"password":..,"totp":..}
//!                                                 (waits for the user; errors "denied", "timeout", "busy")
//!   {"type":"save","url":U,"username":..,"password":..}
//!                                              -> {"ok":true,"outcome":"added"|"updated"|"unchanged"}
//!   {"type":"add_totp","url":U,"uri":otpauth}  -> {"ok":true,"title":..} once the user picks the entry
//!                                                 in the app (errors "denied", "timeout", "busy")
//!   {"type":"search","query":Q}               -> {"ok":true,"results":[{id,title,username,url}]} (max 20, no secrets)
//!   {"type":"open","id":I}                     -> shows that entry in the app window
//!   {"type":"lookalike","url":U}               -> {"ok":true,"similar":{domain,title}|null} (phishing check)
//!   {"type":"focus"}                           -> brings the app window forward (to unlock)
//!
//! Saving is started by the user in the extension popup ("Save login from this
//! page"), which reads the form only then; no script watches pages for logins.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use kryptos_core::matching::entry_matches;
use kryptos_core::SaveOutcome;
use zeroize::Zeroizing;

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Status,
    Logins { url: String },
    Credentials { id: String, url: String },
    Save { url: String, username: String, password: String },
    AddTotp { url: String, uri: String },
    /// From kryptos-mcp only: an AI assistant's request (see ai.rs).
    Ai {
        token: String,
        #[serde(default)]
        client: String,
        op: String,
        #[serde(default)]
        id: String,
    },
    /// From the `kryptos` command line only (see projects.rs).
    Env {
        op: String,
        #[serde(default)]
        project: String,
        #[serde(default)]
        env: String,
        #[serde(default)]
        content: String,
    },
    Search { query: String },
    Open { id: String },
    Lookalike { url: String },
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

/// Who is on the other end of the socket: each of our bundled binaries may only
/// make its own kind of request.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Peer {
    /// kryptos-native-host, i.e. the browser extension.
    Browser,
    /// kryptos-mcp, i.e. an AI assistant (only `ai` requests, see ai.rs).
    Ai,
    /// kryptos, the command line (only `env` requests, see projects.rs).
    Cli,
}

/// Is the process on the other end of `stream` one of our own bundled binaries?
#[cfg(unix)]
fn peer_of(stream: &std::os::unix::net::UnixStream) -> Option<Peer> {
    use std::os::fd::AsRawFd;
    use kryptos_core::peer;

    let pid = peer::socket_peer_pid(stream.as_raw_fd())?;
    let exe = peer::exe_path(pid)?.canonicalize().ok()?;
    let is = |p: Result<std::path::PathBuf, String>| p.ok().and_then(|p| p.canonicalize().ok()).is_some_and(|p| p == exe);
    let who = if is(crate::browser::host_binary()) {
        Peer::Browser
    } else if is(crate::browser::mcp_binary()) {
        Peer::Ai
    } else if is(crate::browser::cli_binary()) {
        Peer::Cli
    } else {
        return None;
    };
    verified(pid).then_some(who)
}

/// The binary's code signature is intact (and, for Developer ID builds, from our team).
#[cfg(unix)]
fn verified(pid: i32) -> bool {
    #[cfg(target_os = "macos")]
    {
        use kryptos_core::peer;
        if !peer::valid_signature(pid) {
            return false;
        }
        // Release builds signed with a Developer ID: require the same team, so a
        // swapped binary at the same path cannot pass.
        if let Some(team) = peer::own_team_id() {
            return peer::satisfies(pid, &format!("anchor apple generic and certificate leaf[subject.OU] = \"{team}\""));
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = pid;
    true
}

fn handle(app: &AppHandle, peer: Peer, line: &str) -> Value {
    let state = app.state::<AppState>();
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(_) => return json!({ "ok": false, "error": "bad_request" }),
    };
    // The MCP server may only ask for AI-shared logins; the browser never may.
    let allowed = match req {
        Request::Ai { .. } => peer == Peer::Ai,
        Request::Env { .. } => peer == Peer::Cli,
        _ => peer == Peer::Browser,
    };
    if !allowed {
        return json!({ "ok": false, "error": "untrusted_peer" });
    }
    let result = match req {
        Request::Ai { token, client, op, id } => crate::ai::handle(app, &state, &Zeroizing::new(token), &client, &op, &id),
        Request::Env { op, project, env, content } => {
            let content = Zeroizing::new(content);
            if fill_allowed() { crate::projects::handle(app, &state, &op, &project, &env, &content) } else { Err("rate_limited".into()) }
        }
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
            // With the health flags (no secrets), most recently used first.
            let report = kryptos_core::health::analyze(v.entries());
            let used: std::collections::HashMap<_, _> = v.list().into_iter().map(|e| (e.id, e.last_used)).collect();
            let mut logins: Vec<(u64, Value)> = v
                .entries()
                .iter()
                .filter(|e| entry_matches(e, &url))
                .map(|e| {
                    let last_used = used.get(&e.id).copied().unwrap_or(0);
                    let item = json!({
                        "id": e.id, "title": e.title, "username": e.username,
                        "weak": report.weak.contains(&e.id), "reused": report.reused.contains(&e.id),
                        "has_totp": !e.totp.trim().is_empty(), "last_used": last_used,
                    });
                    (last_used, item)
                })
                .collect();
            logins.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
            Ok(json!({ "ok": true, "logins": logins.into_iter().map(|(_, l)| l).collect::<Vec<_>>() }))
        }),
        // Whole-vault search from the popup: names and usernames only (never a
        // secret), at most 20, and only for queries of two characters or more.
        Request::Search { query } => state.read(|v| {
            let q = query.trim().to_lowercase();
            if q.chars().count() < 2 {
                return Ok(json!({ "ok": true, "results": [] }));
            }
            let results: Vec<Value> = v
                .entries()
                .iter()
                .filter(|e| e.title.to_lowercase().contains(&q) || e.username.to_lowercase().contains(&q) || e.urls.iter().any(|u| u.to_lowercase().contains(&q)))
                .take(20)
                .map(|e| json!({ "id": e.id, "title": e.title, "username": e.username, "url": e.urls.first() }))
                .collect();
            Ok(json!({ "ok": true, "results": results }))
        }),
        // Shows an entry in the app (from a search result in the popup).
        Request::Open { id } => {
            let id = uuid::Uuid::parse_str(&id).map_err(|_| "invalid_id".to_string());
            match id.and_then(|id| state.read(|v| v.get(id).map(|e| e.id).map_err(|_| "not_found".to_string()))) {
                Ok(id) => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = app.emit("open-entry", id.to_string());
                    Ok(json!({ "ok": true }))
                }
                Err(e) => Err(e),
            }
        }
        // Does this page imitate a saved site? Only the saved domain and title come back.
        Request::Lookalike { url } => state.read(|v| Ok(json!({ "ok": true, "similar": kryptos_core::smart::lookalike(v.entries(), &url) }))),
        Request::Credentials { id, url } => credentials(app, &state, &id, &url),
        Request::AddTotp { url, uri } => {
            if !fill_allowed() {
                Err("rate_limited".into())
            } else {
                crate::otp_add::add(app, &state, &url, &Zeroizing::new(uri)).map(|title| json!({ "ok": true, "title": title }))
            }
        }
        Request::Save { url, username, password } => save(app, &state, &url, Zeroizing::new(username), Zeroizing::new(password)),
    };
    result.unwrap_or_else(|e| json!({ "ok": false, "error": e }))
}

/// Releases one entry's credentials for `url`, after the user confirms it in
/// the app window. The vault is not held while the user answers: it is read
/// again afterwards, so a lock in the meantime still wins.
fn credentials(app: &AppHandle, state: &AppState, id: &str, url: &str) -> Result<Value, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "invalid_id")?;
    let entry = |v: &kryptos_core::UnlockedVault| -> Result<kryptos_core::Entry, String> {
        let e = v.get(id).map_err(|_| "not_found")?;
        if !entry_matches(e, url) {
            return Err("url_mismatch".into());
        }
        Ok(e.clone())
    };
    let (title, username) = state.read(|v| entry(v).map(|e| (e.title.clone(), e.username.clone())))?;
    let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();

    crate::confirm::ask(app, title, username, host.clone(), &state.settings().fill_confirm, "browser", String::new())?;

    let e = state.read(entry)?; // Entry wipes itself on drop
    let _ = state.mutate(|v| {
        v.record_use(id);
        Ok(())
    });
    if !fill_allowed() {
        return Err("rate_limited".into());
    }
    let _ = app.emit("browser-fill", json!({ "title": e.title, "host": host }));
    // The current 2FA code too, for pages that ask for it ("" when the entry has none).
    let totp = kryptos_core::totp::Totp::parse(&e.totp).map(|t| t.now().0).unwrap_or_default();
    Ok(json!({ "ok": true, "username": e.username, "password": e.password, "totp": totp }))
}

/// Saves the login the user typed on `url` (same rules as Android's save sheet:
/// the entry for that site and username gets the new password, else a new one).
fn save(app: &AppHandle, state: &AppState, url: &str, username: Zeroizing<String>, password: Zeroizing<String>) -> Result<Value, String> {
    let page = url::Url::parse(url).map_err(|_| "bad_url")?;
    if !matches!(page.scheme(), "https" | "http") {
        return Err("bad_url".into());
    }
    let host = page.host_str().ok_or("bad_url")?;
    let title = host.strip_prefix("www.").unwrap_or(host).to_owned();
    if !fill_allowed() {
        return Err("rate_limited".into());
    }
    let target = page.origin().ascii_serialization();
    let outcome = state.mutate(|v| v.save_login(&target, &title, &username, &password).map_err(crate::commands::code))?;
    let word = match outcome {
        SaveOutcome::Added(_) => "added",
        SaveOutcome::Updated(_) => "updated",
        SaveOutcome::Unchanged(_) => "unchanged",
    };
    let _ = app.emit("login-saved", json!({ "title": title, "outcome": word }));
    Ok(json!({ "ok": true, "outcome": word }))
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
                let Some(peer) = peer_of(&stream) else {
                    let mut s = &stream;
                    let _ = s.write_all(b"{\"ok\":false,\"error\":\"untrusted_peer\"}\n");
                    return;
                };
                let mut line = String::new();
                let reader = stream.try_clone().map(|s| BufReader::new(s.take(1024 * 1024)));
                if let Ok(mut r) = reader {
                    if r.read_line(&mut line).is_ok() {
                        let mut out = serde_json::to_vec(&handle(&app, peer, line.trim_end())).unwrap_or_default();
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
