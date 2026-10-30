//! `kryptos-mcp`: a Model Context Protocol server (stdio) that lets an AI
//! assistant (Claude Desktop, Claude Code, …) use the logins the user chose to
//! share with AI, and nothing else.
//!
//! Started by the AI app from its MCP config, with the access code made in
//! Kryptos → Settings → AI access in `KRYPTOS_MCP_TOKEN`. It holds no secrets
//! and never reads the vault: each tool call is relayed to the running Kryptos
//! app over its Unix socket (the app checks that this exact bundled binary is
//! on the other end). The app then checks the code, that AI access is on, that
//! the entry is shared with AI, and asks the user to confirm every password or
//! 2FA code in its window. No network sockets are opened.
//!
//! Transport: newline-delimited JSON-RPC 2.0 on stdin/stdout (MCP stdio).

use std::io::{self, BufRead, BufReader, Read, Write};

use serde_json::{json, Value};

const PROTOCOL: &str = "2025-06-18";
const MAX_LINE: u64 = 1024 * 1024;

fn main() {
    let token = std::env::var("KRYPTOS_MCP_TOKEN").unwrap_or_default();
    let mut client = String::from("AI assistant");
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            let _ = writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } }));
            continue;
        };
        // Notifications (no id) get no answer.
        let Some(id) = msg.get("id").cloned() else { continue };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => {
                if let Some(name) = params.pointer("/clientInfo/name").and_then(Value::as_str) {
                    client = name.chars().filter(|c| !c.is_control()).take(60).collect();
                }
                let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "kryptos", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "Kryptos is the user's password manager. Only logins the user explicitly shared with AI are listed. Every password or 2FA code you request needs the user's approval in the Kryptos app, so ask only when the task really needs it, and never repeat a password back in your reply."
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => Ok(call(&token, &client, &params)),
            _ => Err(json!({ "code": -32601, "message": "method not found" })),
        };
        let reply = match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

fn tools() -> Value {
    let id = json!({ "type": "object", "properties": { "id": { "type": "string", "description": "The entry id from kryptos_list_credentials" } }, "required": ["id"] });
    json!([
        {
            "name": "kryptos_list_credentials",
            "description": "Lists the logins and project environments the user shared with AI assistants: ids, names, usernames and sites. No passwords or values.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "kryptos_get_credential",
            "description": "Gets the username and password of one shared login. The user must approve it in the Kryptos app (Touch ID or master password); it may be denied.",
            "inputSchema": id
        },
        {
            "name": "kryptos_get_totp",
            "description": "Gets the current 6-digit 2FA code of one shared login (valid for up to 30 seconds). Needs the user's approval in the Kryptos app.",
            "inputSchema": id
        },
        {
            "name": "kryptos_get_env",
            "description": "Gets the .env text of one shared project environment. The user must approve it in the Kryptos app; production environments are never available. Never repeat its values in your reply or write them to a file the user didn't ask for.",
            "inputSchema": id
        }
    ])
}

/// One tool call, relayed to the app. Errors become tool results with
/// `isError`, so the assistant can tell the user what happened.
fn call(token: &str, client: &str, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let id = params.pointer("/arguments/id").and_then(Value::as_str).unwrap_or_default();
    let op = match name {
        "kryptos_list_credentials" => "list",
        "kryptos_get_credential" => "credential",
        "kryptos_get_totp" => "totp",
        "kryptos_get_env" => "env",
        _ => return text(true, &format!("unknown tool {name}")),
    };
    if token.is_empty() {
        return text(true, "KRYPTOS_MCP_TOKEN is not set: create an access code in Kryptos → Settings → AI access and put it in this server's env.");
    }
    match relay(&json!({ "type": "ai", "token": token, "client": client, "op": op, "id": id })) {
        Ok(r) if r.get("ok").and_then(Value::as_bool) == Some(true) => {
            let mut r = r;
            if let Some(o) = r.as_object_mut() {
                o.remove("ok");
            }
            text(false, &r.to_string())
        }
        Ok(r) => text(true, &explain(r.get("error").and_then(Value::as_str).unwrap_or("error"))),
        Err(e) => text(true, &explain(&e)),
    }
}

fn explain(code: &str) -> String {
    match code {
        "app_not_running" => "Kryptos isn't running. Ask the user to open it.".into(),
        "locked" => "The Kryptos vault is locked. Ask the user to unlock it, then try again.".into(),
        "ai_disabled" => "AI access is turned off in Kryptos (Settings → AI access).".into(),
        "ai_token_invalid" => "The access code is wrong, revoked or expired. The user can make a new one in Kryptos → Settings → AI access.".into(),
        "not_shared" => "That login isn't shared with AI assistants.".into(),
        "denied" => "The user declined this request in Kryptos.".into(),
        "timeout" => "The user didn't answer in Kryptos in time.".into(),
        "busy" => "Kryptos is already waiting for the user on another request.".into(),
        "rate_limited" => "Too many requests in a short time. Wait a minute.".into(),
        other => format!("Kryptos error: {other}"),
    }
}

fn text(is_error: bool, s: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": s }], "isError": is_error })
}

#[cfg(unix)]
fn relay(msg: &Value) -> Result<Value, String> {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = kryptos_core::paths::ipc_socket_path();
    let mut stream = UnixStream::connect(&path).map_err(|_| "app_not_running".to_string())?;
    // Requests for a secret wait for the user's approval in the app (170 s there).
    stream.set_read_timeout(Some(Duration::from_secs(180))).map_err(|e| e.to_string())?;
    let mut line = msg.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream.take(MAX_LINE)).read_line(&mut reply).map_err(|e| e.to_string())?;
    serde_json::from_str(&reply).map_err(|_| "bad_reply".to_string())
}

#[cfg(not(unix))]
fn relay(_msg: &Value) -> Result<Value, String> {
    Err("unsupported_on_this_platform".into())
}
