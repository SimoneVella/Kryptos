//! AI assistants, through the MCP server (crates/mcp-server, bundled as
//! `kryptos-mcp`). Layers, all of which must agree:
//! 1. AI access is on in Settings (off by default);
//! 2. the request carries the current access code (only its SHA-256 is stored;
//!    codes expire and can be revoked);
//! 3. the entry is one the user shared with AI (per-entry switch);
//! 4. every password or 2FA code is confirmed by the user in the app window,
//!    with Touch ID / the Mac password or the master password (confirm.rs),
//!    showing which assistant asks;
//! 5. at most 10 requests a minute; each is logged (in memory only, so entry
//!    names never land on disk in clear) and announced in the app.
//!
//! Listing returns names and usernames only.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use kryptos_core::totp::Totp;

use crate::state::AppState;

const PER_MINUTE: usize = 10;
const LOG_LEN: usize = 50;

#[derive(Clone, Serialize)]
pub struct LogItem {
    at: u64,
    client: String,
    title: String,
    op: &'static str,
    /// "given", "denied" or an error code.
    outcome: String,
}

static LOG: Mutex<VecDeque<LogItem>> = Mutex::new(VecDeque::new());
static RECENT: Mutex<VecDeque<Instant>> = Mutex::new(VecDeque::new());

pub fn log() -> Vec<LogItem> {
    LOG.lock().unwrap().iter().cloned().collect()
}

fn record(client: &str, title: &str, op: &'static str, outcome: &str) {
    let mut log = LOG.lock().unwrap();
    log.push_front(LogItem { at: now(), client: client.into(), title: title.into(), op, outcome: outcome.into() });
    log.truncate(LOG_LEN);
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn allowed() -> bool {
    let mut q = RECENT.lock().unwrap();
    while q.front().is_some_and(|t| t.elapsed() > Duration::from_secs(60)) {
        q.pop_front();
    }
    if q.len() >= PER_MINUTE {
        return false;
    }
    q.push_back(Instant::now());
    true
}

/// Whether `code` is the current, unexpired access code and AI access is on.
fn authorized(state: &AppState, code: &str) -> Result<(), String> {
    let s = state.settings();
    if !s.ai_enabled {
        return Err("ai_disabled".into());
    }
    // Codes always expire: a missing expiry counts as expired (as in ai_status).
    let expired = s.ai_code_expires == 0 || now() > s.ai_code_expires;
    if s.ai_code_hash.is_empty() || expired || !kryptos_core::ai_token::verify(code, &s.ai_code_hash) {
        return Err("ai_token_invalid".into());
    }
    Ok(())
}

/// A project's `.env`: same confirmation as a password. Production never leaves.
fn environment(app: &AppHandle, state: &AppState, client: &str, id: uuid::Uuid) -> Result<Value, String> {
    let shared = |v: &kryptos_core::UnlockedVault| -> Result<(String, String), String> {
        let e = v.env(id).map_err(|_| "not_shared")?;
        if !e.ai_access || e.deleted_at != 0 || kryptos_core::envs::is_production(&e.name) {
            return Err("not_shared".into());
        }
        Ok((format!("{} · {}", e.project, e.name), e.content.clone()))
    };
    let (title, _) = state.read(shared)?;
    let mode = state.settings().fill_confirm;
    if let Err(e) = crate::confirm::ask(app, title.clone(), String::new(), String::new(), &mode, "ai", client.to_owned()) {
        record(client, &title, "env", &e);
        return Err(e);
    }
    let (_, content) = state.read(shared)?;
    record(client, &title, "env", "given");
    let _ = app.emit("ai-used", json!({ "client": client, "title": title, "op": "env" }));
    Ok(json!({ "ok": true, "title": title, "content": content }))
}

pub fn handle(app: &AppHandle, state: &AppState, code: &str, client: &str, op: &str, id: &str) -> Result<Value, String> {
    authorized(state, code)?;
    if !allowed() {
        return Err("rate_limited".into());
    }
    // Self-reported by the assistant: shown as a hint, never trusted for anything.
    let client: String = client.chars().filter(|c| !c.is_control()).take(60).collect();
    let client = if client.trim().is_empty() { "AI assistant" } else { client.trim() };
    if op == "list" {
        return state.read(|v| {
            let list: Vec<Value> = v
                .entries()
                .iter()
                .filter(|e| e.ai_access)
                .map(|e| json!({ "id": e.id, "title": e.title, "username": e.username, "site": e.urls.first().and_then(|u| kryptos_core::import::host_of(u)) }))
                .collect();
            let environments: Vec<Value> = v
                .envs()
                .iter()
                .filter(|e| e.ai_access && !e.production && e.deleted_at == 0)
                .map(|e| json!({ "id": e.id, "project": e.project, "environment": e.name, "variables": e.vars }))
                .collect();
            Ok(json!({ "ok": true, "credentials": list, "environments": environments }))
        });
    }
    let op: &'static str = match op {
        "credential" => "credential",
        "totp" => "totp",
        "env" => "env",
        _ => return Err("bad_request".into()),
    };
    let id = uuid::Uuid::parse_str(id).map_err(|_| "invalid_id")?;
    if op == "env" {
        return environment(app, state, client, id);
    }
    let shared = |v: &kryptos_core::UnlockedVault| -> Result<(String, String, String), String> {
        let e = v.get(id).map_err(|_| "not_shared")?;
        if !e.ai_access {
            return Err("not_shared".into());
        }
        if op == "totp" && e.totp.trim().is_empty() {
            return Err("no_totp".into());
        }
        let site = e.urls.first().and_then(|u| kryptos_core::import::host_of(u)).unwrap_or_default();
        Ok((e.title.clone(), e.username.clone(), site))
    };
    let (title, username, site) = state.read(shared)?;

    let mode = state.settings().fill_confirm;
    if let Err(e) = crate::confirm::ask(app, title.clone(), username, site, &mode, "ai", client.to_owned()) {
        record(client, &title, op, &e);
        return Err(e);
    }
    // Read again after the wait: a lock, or the entry unshared meanwhile, still wins.
    let out = state.read(|v| {
        shared(v)?;
        let e = v.get(id).map_err(|_| "not_shared")?;
        Ok(if op == "totp" {
            json!({ "ok": true, "title": e.title, "code": Totp::parse(&e.totp).map_err(crate::commands::code)?.now().0 })
        } else {
            json!({ "ok": true, "title": e.title, "username": e.username, "password": e.password })
        })
    })?;
    let _ = state.mutate(|v| {
        v.record_use(id);
        Ok(())
    });
    record(client, &title, op, "given");
    let _ = app.emit("ai-used", json!({ "client": client, "title": title, "op": op }));
    Ok(out)
}
