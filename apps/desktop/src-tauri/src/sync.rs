//! Moving the vault between devices without any network: merging another copy
//! (a backup file, or one received as animated QR codes) and sending this one
//! as QR codes.
//!
//! What travels is always the encrypted vault file itself, so the channel (a
//! file-sharing app, a screen someone could film) never sees more than a backup
//! would, and merging it needs that copy's master password.
//!
//! Flow: `sync_stage_file` / `sync_stage_bytes` hold the incoming copy, the UI
//! asks for its master password, `sync_merge` merges it (or `sync_cancel`).

use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use kryptos_core::{format, storage, MergeReport, UnlockedVault};
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::FsExt;
use zeroize::Zeroizing;

use crate::commands::{code, file_name, CmdResult};
use crate::state::AppState;

/// The copy waiting for its master password.
#[derive(Default)]
pub struct Staged(Mutex<Option<Vec<u8>>>);

#[derive(Serialize)]
pub struct StagedInfo {
    /// File name, or empty when it came from QR codes.
    file: String,
    /// Same vault as this one (another device or a backup of it): usually the same master password.
    same_vault: bool,
}

/// Raw bytes per QR frame. 200 bytes are ~286 characters once framed, a
/// version-13 code at error-correction level M: in tests it still decodes
/// from a blurred, low-contrast camera-like image, where 300 bytes (version 16)
/// did not. A vault with 100 logins is ~150 frames, about 30 s at 5 fps.
const QR_CHUNK: usize = 200;
/// Frame text: `KRY1|<session>|<index>|<count>|<base64 chunk>`.
const QR_PREFIX: &str = "KRY1";

fn stage(state: &AppState, staged: &Staged, bytes: Vec<u8>, file: String) -> CmdResult<StagedInfo> {
    // Cheap structural check now, so a wrong file fails before the password prompt.
    let parsed = format::decode(&bytes).map_err(|_| "not_a_vault")?;
    let same_vault = state.read(|v| Ok(v.vault_id() == parsed.header.vault_id)).unwrap_or(false);
    *staged.0.lock().unwrap() = Some(bytes);
    Ok(StagedInfo { file, same_vault })
}

/// Lets the user pick a backup file (any `.kryptos` copy of a vault).
#[tauri::command]
pub async fn sync_stage_file(app: AppHandle, state: State<'_, AppState>, staged: State<'_, Staged>) -> CmdResult<Option<StagedInfo>> {
    let dialog = app.dialog().file().set_title("Kryptos backup");
    // Android maps extensions to MIME types and knows nothing of ".kryptos".
    #[cfg(not(target_os = "android"))]
    let dialog = dialog.add_filter("Kryptos", &["kryptos"]);
    let Some(picked) = dialog.blocking_pick_file() else { return Ok(None) };
    let file = file_name(&picked);
    let bytes = app.fs().read(picked).map_err(|e| e.to_string())?;
    stage(&state, &staged, bytes, file).map(Some)
}

/// A copy received as QR codes and reassembled by the UI (base64).
#[tauri::command]
pub fn sync_stage_bytes(state: State<'_, AppState>, staged: State<'_, Staged>, data: String) -> CmdResult<StagedInfo> {
    let bytes = B64.decode(data.trim()).map_err(|_| "not_a_vault")?;
    stage(&state, &staged, bytes, String::new())
}

/// Opens the staged copy with its master password and merges it into this vault.
/// On a wrong password the copy stays staged so the user can try again.
#[tauri::command]
pub async fn sync_merge(state: State<'_, AppState>, staged: State<'_, Staged>, password: String) -> CmdResult<MergeReport> {
    let bytes = staged.0.lock().unwrap().clone().ok_or("not_found")?;
    let password = Zeroizing::new(password);
    // Argon2 takes about a second: keep it off the main thread and outside the vault lock.
    let other = tauri::async_runtime::spawn_blocking(move || UnlockedVault::unlock(&bytes, &password))
        .await
        .map_err(|e| e.to_string())?
        .map_err(code)?;
    let report = state.mutate(|v| Ok(v.merge_from(&other)))?;
    staged.0.lock().unwrap().take();
    Ok(report)
}

#[tauri::command]
pub fn sync_cancel(staged: State<'_, Staged>) {
    staged.0.lock().unwrap().take();
}

#[derive(Serialize)]
pub struct QrFrames {
    /// One SVG per frame, to be shown in a loop.
    frames: Vec<String>,
    bytes: usize,
}

/// This vault, encrypted as on disk, split into QR codes for another device to scan.
#[tauri::command]
pub fn sync_qr_frames(state: State<'_, AppState>) -> CmdResult<QrFrames> {
    state.touch();
    // Only while unlocked: showing the vault is a deliberate act of the user.
    state.read(|_| Ok(()))?;
    let bytes = storage::read(&state.vault_path).map_err(code)?;
    let session = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let frames = frame_texts(&bytes, session).iter().map(|t| frame_svg(t)).collect::<CmdResult<Vec<_>>>()?;
    Ok(QrFrames { frames, bytes: bytes.len() })
}

/// The text of each frame (the UI's ScanQrSheet parses it back).
fn frame_texts(bytes: &[u8], session: &str) -> Vec<String> {
    let chunks: Vec<_> = bytes.chunks(QR_CHUNK).collect();
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| format!("{QR_PREFIX}|{session}|{i}|{}|{}", chunks.len(), B64.encode(chunk)))
        .collect()
}

fn frame_svg(text: &str) -> CmdResult<String> {
    let qr = QrCode::with_error_correction_level(text, EcLevel::M).map_err(|e| e.to_string())?;
    Ok(qr.render::<svg::Color>().min_dimensions(480, 480).quiet_zone(true).build())
}

/// Android: hands an encrypted backup to the system share sheet (Quick Share,
/// Drive, a chat with yourself...).
#[tauri::command]
pub async fn share_backup(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state.touch();
    #[cfg(target_os = "android")]
    {
        let src = state.vault_path.to_string_lossy().into_owned();
        let name = format!("Kryptos-backup-{}.kryptos", crate::commands::today());
        tauri::async_runtime::spawn_blocking(move || crate::android::share_file(&app, src, name))
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err("unsupported_on_this_device".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_reassemble_and_fit_in_a_qr() {
        let bytes: Vec<u8> = (0..2000u32).map(|i| (i * 7 % 251) as u8).collect();
        let texts = frame_texts(&bytes, "abcd1234");
        assert_eq!(texts.len(), 10);
        // Same parsing as the UI: KRY1|session|index|count|base64.
        let mut out = Vec::new();
        for (i, t) in texts.iter().enumerate() {
            let p: Vec<_> = t.split('|').collect();
            assert_eq!((p[0], p[1], p[2], p[3]), (QR_PREFIX, "abcd1234", i.to_string().as_str(), "10"));
            out.extend(B64.decode(p[4]).unwrap());
        }
        assert_eq!(out, bytes);
        for t in &texts {
            let qr = QrCode::with_error_correction_level(t, EcLevel::M).unwrap();
            assert!(qr.width() <= 69, "frame too dense: {} modules", qr.width());
        }
        // Lets the scanner be checked against a real frame (see the sync docs).
        if let Ok(path) = std::env::var("KRYPTOS_QR_DUMP") {
            std::fs::write(path, frame_svg(&texts[0]).unwrap()).unwrap();
        }
    }
}
