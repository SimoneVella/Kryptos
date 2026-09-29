*English · [Italiano](ARCHITECTURE.it.md)*

# Kryptos: architecture

Offline, zero-knowledge password manager. No component opens a network socket.

```
┌──────────── Browser ────────────┐        ┌──────────── Desktop app (Tauri) ─────────────┐
│ extension/popup.js              │ stdio  │                                               │
│  sendNativeMessage ─────────────┼──────▶ │ kryptos-native-host ──Unix socket 0600──▶ bridge.rs
│  scripting.executeScript (fill) │        │ (relay, no secrets)                │          │
└─────────────────────────────────┘        │                                    ▼          │
                                           │ React UI ──invoke──▶ commands.rs ▶ AppState   │
                                           │                                    │ (vault in RAM)
                                           │                     kryptos-core ◀─┘          │
                                           └───────────────────────┬───────────────────────┘
                                                                   ▼
                                    ~/Library/Application Support/com.kryptos.desktop/vault.kryptos
```

## Encryption (`crates/core`)

| What | Choice |
|---|---|
| Password → key | Argon2id: 256 MiB, t=3, p=4 on desktop; 64 MiB on mobile. Parameters are stored in the header, with validated min/max limits. |
| Key hierarchy | KEK = Argon2id(master, salt) wraps a random 256-bit **vault key**. The body is encrypted with the vault key. |
| Encryption | XChaCha20-Poly1305 (AEAD, random 192-bit nonce, fresh on every save). |
| Integrity | Magic + version + JSON header are the body's AAD: any change makes decryption fail. |
| Memory | Keys and entries are `Zeroize`d and cleared on lock/drop. |
| Disk | Atomic write (tmp → fsync → rename), 0600 permissions, one `.bak` backup. |

Changing the master password only requires re-wrapping the vault key. Biometric unlock will store the
vault key (not the password) in the Keychain / Secure Enclave / Android Keystore: the API is already in
place (`raw_key()` / `unlock_with_key()`).

### Differences from the original plan

- **No SQLCipher.** A password vault is small (a few hundred KB even with thousands of entries), so a
  single AEAD file is simpler to audit, doesn't drag in OpenSSL, and syncing only needs to transfer one
  encrypted blob (the same model KeePass/KDBX uses). Note: SQLCipher uses AES-256-**CBC** + HMAC-SHA512,
  not GCM as the original plan said.
- **The native host doesn't just "talk" to the browser over a pipe**: the browser spawns a new host
  process for every message, and that process has no access to the unlocked vault. That's why the host
  relays to the app over a **Unix domain socket** (not TCP, not reachable from the network).
- **Tauri can't "turn off the network" at the OS level.** What we do instead: no http/shell/fs plugins,
  the CSP only allows `ipc:` in `connect-src`, and JS only has `core:default`. The clipboard is handled by
  Rust, so copied passwords never pass through JS.

## Browser autofill

1. The user opens the popup (or presses ⌘⇧L). No content script runs on pages.
2. The popup sends `{type:"logins", url}` and the app replies with entries matching the site.
3. On click, `{type:"credentials", id, url}` returns credentials **only if** the URL matches the entry.
4. A function injected once fills the form and checks that `location.origin` hasn't changed in the
   meantime.

URL matching (`matching.rs`): same registrable domain (eTLD+1, Public Suffix List bundled in the binary,
so no network needed). `github.io`, `co.uk` and similar are treated as public suffixes. An entry saved
for https is never offered to an http page.

**Bridge defenses**, from the outside in:

| Attack | Defense | Verified |
|---|---|---|
| A program connects directly to the app's socket | The app identifies the connecting process (`LOCAL_PEERPID`) and only accepts `kryptos-native-host` from its own bundle, with an intact code signature and, if the app is Developer ID signed, the same Team ID | `untrusted_peer` |
| A program launches the legitimate native host and writes requests to it | The native host only replies if the process that launched it is a browser with a valid Apple signature from a trusted vendor (Google, Mozilla, Microsoft, Brave) and the browser declares our extension's ID as the caller | `untrusted_caller` |
| Another extension calls the native host | `allowed_origins` in the manifest: the browser only launches it for our ID | — |
| A malicious page, an iframe from another site, or a page that changed in the meantime | Fill checks `location.origin` in every frame. No script runs on pages | — |
| Lookalike sites (`example.com.evil.io`, `alice.github.io` vs. `mallory.github.io`) | eTLD+1 matching with the Public Suffix List. Https is never offered to http | unit tests |
| Bulk extraction | At most 20 passwords per minute, and every fill shows up as a notification in the app | — |

**Known limits, to be disclosed:**
- Malware running as your user can still record the master password as you type it (keylogger). This is
  true of any password manager.
- Local builds only have an ad-hoc signature: native host verification relies on the path inside the
  bundle and macOS's "App Management" protection. With a **Developer ID** signature it becomes
  cryptographic (same Team ID), and hardened runtime kicks in, which stops other processes from reading
  the app's memory.
- With eTLD+1, an entry saved for `google.com` is also offered on `sites.google.com`: user-published
  content on subdomains of the same domain is a residual risk, mitigated by the domain always being shown
  in the popup before filling.
- On Linux, browser verification relies on the executable name, since there's no code signing there.
- Additional browsers (Arc, Vivaldi…) need to be added to `TRUSTED_BROWSER_TEAMS` in
  `crates/core/src/peer.rs` after reading their Team ID with `codesign -dv`.

## Mobile (Tauri 2)

Instead of React Native, Android and iOS use **Tauri 2 mobile**: the same React UI, the same Rust
commands, the same `kryptos-core` as desktop. Only what the OS requires stays native.

**Android** (`apps/desktop/src-tauri/gen/android`)
- `KryptosAutofillService.kt` is the system autofill service. It reads the unlocked vault **in the same
  process** via JNI (`src-tauri/src/android.rs`). If the vault is locked, it offers "Unlock Kryptos",
  which opens the app.
- The web domain is only trusted if the app requesting credentials is a known browser (Chrome, Firefox,
  Brave, Edge, Samsung…). Any app could otherwise declare `webDomain = mybank.com` to steal the password.
  Native apps only receive entries explicitly linked to `androidapp://<package>`.
- **No INTERNET permission** in the production build (it's only present in debug, for the dev server).
- Cloud backup and cross-device transfer are disabled (`data_extraction_rules.xml`).
- `FLAG_SECURE` in production: no screenshots, recordings or previews in the recent-apps view.
- Argon2id at 64 MiB: roughly 1.4 s to create the vault and about 0.4 s to unlock it on a Pixel 7 Pro
  emulator.
- Native libraries 16 KB-aligned (`.cargo/config.toml`), as required by Android 15 and Google Play.
- Suggestions appear inside the keyboard (inline, Android 11+) with a dropdown fallback.
- **Fingerprint unlock**: the vault key is encrypted with an Android Keystore AES key that requires a
  strong biometric for every single use and is destroyed when a new fingerprint is enrolled (someone who
  knows the PIN cannot add their own finger). Only the encrypted vault key is stored. The master password
  is still required after every reboot and every 7 days. Face unlock only works on phones where it is
  certified as strong biometrics (e.g. Pixel 8 and later, not Pixel 7).
- With the vault locked, the "Unlock Kryptos" suggestion shows the fingerprint prompt over the browser
  and fills the form directly, without opening the app.

**iOS** (still to do, needs Xcode): `tauri ios init` plus a *Credential Provider Extension* in Swift that
calls into the Rust core via FFI and shares the vault with the app through an App Group. The extension
memory limit (~120 MiB) is why mobile uses 64 MiB of Argon2.

## Auto-lock

- Inactivity (1, 5, 15, 60 or 240 minutes). Elapsed time is measured with both the monotonic clock and the
  system clock, and the larger of the two wins: the lock still triggers even if the process was
  suspended.
- Computer sleep: detected because the monotonic clock stops during sleep while the system clock doesn't.
- Locked screen on macOS (`CGSessionCopyCurrentDictionary`).
- Android: as soon as the screen turns off (`ACTION_SCREEN_OFF`) and 1 minute after Kryptos leaves the
  screen (`ProcessLifecycleOwner`), long enough for a two-step login. Unlocking again takes a fingerprint.
- Clipboard: a copied password is cleared after 15–120 s, but only if you haven't copied something else
  in the meantime.

## Roadmap

- [x] Core: vault, KDF, generator, URL matching, atomic storage, CSV import, security analysis (23 tests)
- [x] Desktop: auto-lock (inactivity, sleep, locked screen), self-clearing clipboard, persistent settings
- [x] CSV import (Google/Chrome, Bitwarden, 1Password, Firefox) and encrypted backup
- [x] MV3 extension with a fixed ID, one-click connection from the app, native host bundled with the app
- [x] Android: same app via Tauri, system AutofillService, no network permission
- [x] Android: end-to-end autofill in Chrome, with suggestions inside the keyboard
- [x] Android: fingerprint unlock (Keystore-bound key), lock on screen-off and when leaving the app
- [x] Android: CSV import and encrypted backup (system file picker, `content://` URIs via the fs plugin)
- [ ] macOS: Touch ID unlock with the vault key in the Keychain (needs a signed app)
- [ ] iOS: Credential Provider Extension (needs Xcode)
- [ ] Saving new logins from the browser and from apps (Android `onSaveRequest`)
- [ ] TOTP
- [ ] Windows: named pipe for the bridge and native host registration in the registry
- [ ] LAN sync: vault encrypted over local TCP with QR pairing (ephemeral X25519 key in the QR, Noise
  channel), per-entry merge using `updated_at`
