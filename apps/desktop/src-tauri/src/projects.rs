//! `kryptos env`: the command line's requests for a project's `.env`
//! (crates/cli, bundled as `kryptos`). Like the browser and AI bridges:
//! 1. only that bundled, signature-checked binary can make these requests;
//! 2. reading or writing asks the user in the app window (Touch ID, the Mac
//!    password or the master password, see confirm.rs), naming the environment;
//! 3. a confirmed read stays valid for `GRACE`, so restarting a dev server doesn't
//!    prompt each time. Only reads, never for production, and not past a lock.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use kryptos_core::envs;

use crate::state::AppState;

const GRACE: Duration = Duration::from_secs(5 * 60);

/// Environment -> when its read was confirmed and in which unlock session.
static CONFIRMED: Mutex<Option<HashMap<Uuid, (Instant, u64)>>> = Mutex::new(None);

fn recently_confirmed(state: &AppState, id: Uuid) -> bool {
    let session = state.session();
    CONFIRMED.lock().unwrap().as_ref().and_then(|m| m.get(&id)).is_some_and(|(at, s)| *s == session && at.elapsed() < GRACE)
}

fn remember(state: &AppState, id: Uuid) {
    CONFIRMED.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, (Instant::now(), state.session()));
}

fn confirm(app: &AppHandle, state: &AppState, title: &str, what: &str) -> Result<(), String> {
    let mode = state.settings().fill_confirm;
    crate::confirm::ask(app, title.to_owned(), String::new(), String::new(), &mode, "env", what.to_owned())
}

pub fn handle(app: &AppHandle, state: &AppState, op: &str, project: &str, env: &str, content: &str) -> Result<Value, String> {
    match op {
        "list" => state.read(|v| {
            let list: Vec<Value> = v
                .envs()
                .iter()
                .filter(|e| e.deleted_at == 0)
                .map(|e| json!({ "project": e.project, "env": e.name, "vars": e.vars, "production": e.production }))
                .collect();
            Ok(json!({ "ok": true, "environments": list }))
        }),
        "get" => {
            let (id, title, production) = state.read(|v| {
                let e = v.env_named(project, env).ok_or("env_not_found")?;
                Ok((e.id, format!("{} · {}", e.project, e.name), envs::is_production(&e.name)))
            })?;
            if production || !recently_confirmed(state, id) {
                confirm(app, state, &title, "read")?;
                if !production {
                    remember(state, id);
                }
            }
            // Read after the wait: a lock or a deletion meanwhile still wins.
            let content = state.read(|v| {
                let e = v.env(id).map_err(crate::commands::code)?;
                if e.deleted_at != 0 {
                    return Err("env_not_found".to_owned());
                }
                Ok(e.content.clone())
            })?;
            let _ = app.emit("env-used", json!({ "title": title, "op": "read" }));
            Ok(json!({ "ok": true, "content": content }))
        }
        "put" => {
            let project = envs::clean_name(project).ok_or("invalid_name")?;
            let env = envs::clean_name(env).ok_or("invalid_name")?;
            let title = format!("{project} · {env}");
            state.read(|_| Ok(()))?;
            confirm(app, state, &title, "write")?;
            state.mutate(|v| v.env_put(&project, &env, content).map_err(crate::commands::code))?;
            let _ = app.emit("env-used", json!({ "title": title, "op": "write" }));
            Ok(json!({ "ok": true }))
        }
        _ => Err("bad_request".into()),
    }
}
