//! Adding a 2FA code read by the browser extension from a site's QR code.
//!
//! The extension only reads the QR when the user asks (a screenshot of the
//! visible tab, decoded in the popup). The app then comes forward and the user
//! picks which entry gets the code, or a new one: nothing is stored without that
//! choice. Same window handling as confirm.rs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use kryptos_core::matching::entry_matches;
use kryptos_core::otp_import::parse_export;
use kryptos_core::EntryInput;

use crate::state::AppState;

const TIMEOUT: Duration = Duration::from_secs(170);

#[derive(Clone, Serialize)]
pub struct Candidate {
    id: String,
    title: String,
    username: String,
}

#[derive(Clone, Serialize)]
pub struct OtpAddRequest {
    id: u64,
    host: String,
    issuer: String,
    account: String,
    /// Entries for this site, best first.
    candidates: Vec<Candidate>,
}

struct Pending {
    id: u64,
    tx: mpsc::Sender<Option<String>>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The user's answer from the app window: an entry id, "new", or None (cancel).
pub fn answer(id: u64, choice: Option<String>) {
    if let Some(p) = PENDING.lock().unwrap().as_ref().filter(|p| p.id == id) {
        let _ = p.tx.send(choice);
    }
}

/// Asks in the app window where the 2FA code `uri` read on page `url` goes, and
/// stores it there. Returns the entry's title.
pub fn add(app: &AppHandle, state: &AppState, url: &str, uri: &str) -> Result<String, String> {
    let accounts = parse_export(uri.as_bytes()).map_err(|_| "invalid_totp")?;
    let acc = accounts.into_iter().next().ok_or("invalid_totp")?;
    let page = url::Url::parse(url).map_err(|_| "bad_url")?;
    if !matches!(page.scheme(), "https" | "http") {
        return Err("bad_url".into());
    }
    let host = page.host_str().unwrap_or_default().trim_start_matches("www.").to_owned();
    let candidates = state.read(|v| {
        Ok(v.entries()
            .iter()
            .filter(|e| entry_matches(e, url))
            .map(|e| Candidate { id: e.id.to_string(), title: e.title.clone(), username: e.username.clone() })
            .collect::<Vec<_>>())
    })?;

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    {
        let mut pending = PENDING.lock().unwrap();
        if pending.is_some() {
            return Err("busy".into());
        }
        *pending = Some(Pending { id, tx });
    }
    let window = app.get_webview_window("main");
    let was_focused = window.as_ref().is_some_and(|w| w.is_focused().unwrap_or(false));
    if let Some(w) = &window {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let request = OtpAddRequest { id, host: host.clone(), issuer: acc.issuer.clone(), account: acc.account.clone(), candidates };
    let _ = app.emit("otp-add", request);
    let choice = rx.recv_timeout(TIMEOUT);
    PENDING.lock().unwrap().take_if(|p| p.id == id);
    let _ = app.emit("otp-add-done", id);
    if !was_focused {
        #[cfg(target_os = "macos")]
        let _ = app.hide();
    }

    let choice = match choice {
        Ok(Some(c)) => c,
        Ok(None) => return Err("denied".into()),
        Err(_) => return Err("timeout".into()),
    };
    let title = state.mutate(|v| {
        if choice == "new" {
            let title = [acc.issuer.trim(), host.as_str()].into_iter().find(|t| !t.is_empty()).unwrap_or("2FA").to_owned();
            v.add(EntryInput {
                title: title.clone(),
                username: acc.account.clone(),
                password: String::new(),
                urls: vec![page.origin().ascii_serialization()],
                notes: String::new(),
                favorite: false,
                totp: acc.uri.clone(),
                backup_codes: Vec::new(),
                ai_access: false,
            })
            .map_err(crate::commands::code)?;
            Ok(title)
        } else {
            let id = uuid::Uuid::parse_str(&choice).map_err(|_| "invalid_id")?;
            // Only an entry offered for this site can receive the code.
            let e = v.get(id).map_err(crate::commands::code)?;
            if !entry_matches(e, url) {
                return Err("url_mismatch".to_string());
            }
            let title = e.title.clone();
            v.set_totp(id, &acc.uri).map_err(crate::commands::code)?;
            Ok(title)
        }
    })?;
    let _ = app.emit("otp-added", json!({ "title": title }));
    Ok(title)
}
