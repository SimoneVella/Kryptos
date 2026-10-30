*English · [Italiano](ARCHITECTURE.it.md)*

# Kryptos: architecture

Offline, zero-knowledge password manager. No component opens a network socket.

```
┌──────────── Browser ────────────┐        ┌──────────── Desktop app (Tauri) ─────────────┐
│ extension/popup + background.js │ stdio  │                                               │
│  connectNative ─────────────────┼──────▶ │ kryptos-native-host ──Unix socket 0600──▶ bridge.rs
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
| 2FA codes | TOTP (RFC 6238, SHA-1/256/512, 6–8 digits) computed locally in `totp.rs` from the secret stored in the entry (base32 key or `otpauth://` link), tested against the RFC vectors. Imported from Bitwarden/1Password/Dashlane CSVs and from authenticator exports (`otp_import.rs`). |

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
3. On click, the popup hands the fill to `background.js` and the app window comes forward to **confirm
   this fill** (`confirm.rs`): Touch ID or the Mac password (LocalAuthentication, `DeviceOwnerAuthentication`),
   or the master password, verified against the vault file with Argon2id. Settings → Browser → *Confirm each
   fill* picks which; outside macOS it is always the master password. Neither ever reaches the browser.
   The popup closes as soon as Kryptos takes the focus, which is why the wait happens in the background
   script, over a `connectNative` port that also keeps the service worker alive.
4. Only then does `{type:"credentials", id, url}` return credentials, and **only if** the URL matches the
   entry. The vault is not held while the user answers: it is read again afterwards, so a lock in the
   meantime still wins. One request waits at a time (a second one gets `busy`); after 170 s it gets
   `timeout` (the native host waits 180 s for this request, 10 s for the others).
5. A function injected once fills the form and checks that `location.origin` hasn't changed in the
   meantime. If Kryptos came forward only to ask, it hides again and the browser gets the focus back.

**Saving a login** is also started by the user: *Save login from this page* in the popup injects a function
once that reads the login form (same origin only), shows the username and a masked password, and on *Save*
sends `{type:"save", url, username, password}`. The app applies the same rule as Android
(`UnlockedVault::save_login`): new password for the entry of that site and username, otherwise a new entry
for the page origin. Saves count towards the same 20-per-minute limit and show up in the app.

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
| Bulk extraction | Every fill needs the user's OK in the app (Touch ID, Mac password or master password); at most 20 passwords per minute, and every fill shows up as a notification in the app | — |

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
  process** via JNI (`src-tauri/src/android.rs`). If the vault is locked, it offers "Unlock Kryptos".
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
- **A fingerprint for every fill.** With fingerprint unlock on, suggestions carry no username or password:
  each one is an authentication intent (`AutofillUnlockActivity`). Tapping it shows the fingerprint prompt
  over the browser and only then returns the filled dataset, so Android never holds a password the user
  hasn't picked. The prompt uses the same Keystore key as unlocking (the released vault key is wiped
  unused), so the check is enforced by the secure hardware, not just by a callback. With the vault locked,
  the same tap unlocks it: one prompt, not two. The intent only carries the entry id; matching is redone
  on the form itself, so it cannot pull an entry for another site. Without fingerprint unlock, suggestions
  fill directly while the vault is unlocked, as before.
- **Saving new logins.** Every login form gets a `SaveInfo` (password required, username optional, also
  when the fields just disappear), so after typing a new password Android asks "Save to Kryptos?". On yes,
  `onSaveRequest` hands it to Rust (`UnlockedVault::save_login`): the entry for the same site and username
  gets the new password, otherwise a new entry is created, linked to the page domain (trusted browsers) or
  to `androidapp://<package>` (apps), with the same rules as filling. With the vault locked the login waits
  in memory only, for at most 5 minutes: `SaveLoginActivity` asks for the fingerprint, or, without it,
  Kryptos opens and saves it right after the master password. Logins spread over two screens (username,
  then password) are saved with the password only.

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

## Sync without network

Devices never connect to each other: a copy of the **encrypted vault file** is carried over and merged
(`crates/core/src/vault.rs`, `apps/desktop/src-tauri/src/sync.rs`).

- **Transport.** Either a backup file (Android can hand it to the system share sheet through its
  `FileProvider`), or a loop of QR codes shown on one screen and read with the other device's camera
  (`getUserMedia` + [jsQR](https://github.com/cozmo/jsQR), bundled). Frame text:
  `KRY1|<session>|<index>|<count>|<base64 of 200 bytes>`, error correction M, version 13, 5 frames/s.
  200 bytes was picked by decoding real frames under simulated camera blur and low contrast; 300 bytes
  (version 16) failed. A vault with 100 logins is about 150 frames, roughly 30 s per loop.
- **What travels is the vault file as on disk**, so the channel learns no more than from a backup: an
  intercepted file or a filmed QR loop still needs the master password, and Argon2id makes guessing it
  expensive. Showing the QR loop requires the vault to be unlocked.
- **Merge.** The incoming copy is opened with *its own* master password (it can be a different vault)
  and merged entry by entry by id: the newer `updated_at` wins; a deletion leaves a **tombstone**
  (id + time, no data) that wins over any older edit, and an edit made after a deletion wins over the
  tombstone. An entry with another id but the same site, username and password is not duplicated (the
  same CSV imported on two devices). Clock skew between devices can pick the wrong side of a conflict;
  acceptable for a single user.
- **Permissions.** Android asks for the camera only when scanning starts; it still has no INTERNET
  permission. macOS declares `NSCameraUsageDescription` and the camera entitlement.

**Cloud folder sync** (`apps/desktop/src-tauri/src/cloud.rs`, macOS and Android). Kryptos still never connects:
it keeps `<cloud folder>/Kryptos/vault.kryptos` and the provider's app (Google Drive, iCloud Drive,
Dropbox, OneDrive) moves it. The folders are found under `~/Library/CloudStorage` and
`~/Library/Mobile Documents`, so turning it on is one choice, no file picking.

- **One shared vault.** Every device uses the same vault (id, key, master password), so the copy opens
  with the key already in memory and merges silently (`UnlockedVault::sync_from`). A device that starts
  with its own vault joins once (`UnlockedVault::join`): it opens the shared copy with that vault's
  password, moves its own entries in, and from then on uses that vault and password.
- **When.** A background loop (5 s) merges the copy when the provider brings a newer one, and writes it
  after every local change (pull first, then push, so the written copy has both sides). Conflict copies
  the provider makes when two devices write at once (`vault (1).kryptos`…) are merged and removed. A
  master password change travels too: the body records when it changed and the newer header wins (it
  only opens with the shared key, so nobody without it can plant one).
- **Gate.** Turning it on requires the master password, verified against the vault file, and the UI
  accepts only a "very strong" one: the copy now sits with a third party, who can try passwords offline.
- **Android** has no cloud folders on disk and no INTERNET permission, so the user picks the folder once
  with the system picker (`ACTION_OPEN_DOCUMENT_TREE`; Google Drive, Dropbox… are listed there) and
  Kryptos keeps a persisted read/write grant on that one folder (`CloudFolder.kt`, called from Rust
  through JNI). The provider's app does the network. Joining a vault replaces the vault key, so
  fingerprint unlock is turned off and must be turned on again. The provider may show a remote change
  a little late; nothing is lost, it merges on the next round.

## Roadmap

- [x] Core: vault, KDF, generator, URL matching, atomic storage, CSV import, security analysis, sync merge (30 tests)
- [x] Desktop: auto-lock (inactivity, sleep, locked screen), self-clearing clipboard, persistent settings
- [x] CSV import (Google/Chrome, Bitwarden, 1Password, Firefox) and encrypted backup
- [x] MV3 extension with a fixed ID, one-click connection from the app, native host bundled with the app
- [x] Android: same app via Tauri, system AutofillService, no network permission
- [x] Android: end-to-end autofill in Chrome, with suggestions inside the keyboard
- [x] Android: fingerprint unlock (Keystore-bound key), lock on screen-off and when leaving the app
- [x] Android: CSV import and encrypted backup (system file picker, `content://` URIs via the fs plugin)
- [ ] macOS: Touch ID unlock with the vault key in the Keychain (needs a signed app)
- [ ] iOS: Credential Provider Extension (needs Xcode)
- [x] Android: saving new logins from apps and browsers (`onSaveRequest`)
- [x] Saving new logins from the desktop browser extension (button in the popup)
- [x] TOTP: 2FA codes in entries (paste the key or the `otpauth://` link, copy the current code), plus
      backup codes that can be marked as used (the mark syncs like any other change)
- [x] TOTP: the code is filled into one-time-code fields on Android and by the extension (with the login,
      after the same confirmation); import from Aegis/2FAS (unencrypted), Google Authenticator transfer
      links and otpauth:// lists; backup codes read from the file the site lets you download
- [ ] TOTP: scan the QR with the camera; encrypted Aegis/2FAS exports
- [ ] Windows: named pipe for the bridge and native host registration in the registry
- [x] Sync without network: merge a backup or a QR-code transfer, per-entry merge with tombstones
- [x] Cloud folder sync, automatic: macOS finds Google Drive, iCloud Drive, Dropbox, OneDrive; Android
      picks the folder once through the system picker
- [ ] Optional LAN sync (QR pairing with an ephemeral X25519 key, Noise channel). Would need the INTERNET
  permission on Android, so only as an opt-in
