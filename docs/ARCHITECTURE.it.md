*[English](ARCHITECTURE.md) · Italiano*

# Kryptos: architettura

Password manager offline e zero-knowledge. Nessun componente apre socket di rete.

```
┌──────────── Browser ────────────┐        ┌──────────── App desktop (Tauri) ─────────────┐
│ extension/popup.js              │ stdio  │                                               │
│  sendNativeMessage ─────────────┼──────▶ │ kryptos-native-host ──Unix socket 0600──▶ bridge.rs
│  scripting.executeScript (fill) │        │ (relay, nessun segreto)            │          │
└─────────────────────────────────┘        │                                    ▼          │
                                           │ React UI ──invoke──▶ commands.rs ▶ AppState   │
                                           │                                    │ (vault in RAM)
                                           │                     kryptos-core ◀─┘          │
                                           └───────────────────────┬───────────────────────┘
                                                                   ▼
                                    ~/Library/Application Support/com.kryptos.desktop/vault.kryptos
```

## Crittografia (`crates/core`)

| Cosa | Scelta |
|---|---|
| Password → chiave | Argon2id: 256 MiB, t=3, p=4 su desktop; 64 MiB su mobile. Parametri salvati nell'header, con limiti min/max validati. |
| Gerarchia delle chiavi | KEK = Argon2id(master, salt) cifra (wrap) una **vault key** casuale da 256 bit. Il body è cifrato con la vault key. |
| Cifratura | XChaCha20-Poly1305 (AEAD, nonce casuale da 192 bit, nuovo a ogni salvataggio). |
| Integrità | Magic + versione + header JSON sono l'AAD del body: qualsiasi modifica fa fallire la decifratura. |
| Memoria | Chiavi e voci sono `Zeroize` e vengono azzerate al lock/drop. |
| Disco | Scrittura atomica (tmp → fsync → rename), permessi 0600, un backup `.bak`. |

Cambiare la master password richiede solo un nuovo wrap della vault key. Lo sblocco biometrico
salverà la vault key (non la password) nel Keychain / Secure Enclave / Android Keystore:
l'API è già pronta (`raw_key()` / `unlock_with_key()`).

### Differenze rispetto al piano originale

- **Niente SQLCipher.** Un vault di password è piccolo (qualche centinaio di KB anche con migliaia di
  voci), quindi un singolo file AEAD è più semplice da verificare, non si porta dietro OpenSSL, e per
  la sincronizzazione basta trasferire un solo blob cifrato (è lo stesso modello di KeePass/KDBX).
  Nota: SQLCipher usa AES-256-**CBC** + HMAC-SHA512, non GCM come diceva il piano.
- **Il native host non "parla" col browser via pipe e basta**: il browser avvia un nuovo processo
  host a ogni messaggio, e quel processo non ha accesso al vault sbloccato. Per questo l'host fa da
  relay verso l'app tramite un **Unix domain socket** (non TCP, non raggiungibile dalla rete).
- **Tauri non può "spegnere la rete" a livello di OS.** Quello che facciamo: nessun plugin http/shell/fs,
  la CSP permette solo `ipc:` in `connect-src`, e JS ha solo `core:default`. Gli appunti vengono gestiti
  da Rust, quindi le password copiate non passano da JS.

## Autofill nel browser

1. L'utente apre il popup (o preme ⌘⇧L). Nessun content script gira sulle pagine.
2. Il popup invia `{type:"logins", url}` e l'app risponde con le voci che corrispondono al sito.
3. Al click, `{type:"credentials", id, url}` restituisce le credenziali **solo se** l'URL corrisponde alla voce.
4. Una funzione iniettata una sola volta riempie il form e controlla che `location.origin` non sia cambiato nel frattempo.

Corrispondenza URL (`matching.rs`): stesso dominio registrabile (eTLD+1, Public Suffix List inclusa
nel binario, quindi senza rete). `github.io`, `co.uk` e simili sono trattati come suffissi pubblici. Una voce salvata
per https non viene mai offerta a una pagina http.

**Difese del bridge**, dall'esterno verso l'interno:

| Attacco | Difesa | Verificato |
|---|---|---|
| Un programma si collega direttamente al socket dell'app | L'app identifica il processo collegato (`LOCAL_PEERPID`) e accetta solo il `kryptos-native-host` del proprio bundle, con firma di codice intatta e, se l'app è firmata Developer ID, dello stesso Team ID | `untrusted_peer` |
| Un programma avvia il native host legittimo e gli scrive richieste | Il native host risponde solo se il processo che l'ha avviato è un browser con firma Apple valida di un produttore fidato (Google, Mozilla, Microsoft, Brave) e se il browser dichiara come chiamante l'ID della nostra estensione | `untrusted_caller` |
| Un'altra estensione chiama il native host | `allowed_origins` nel manifest: il browser lo avvia solo per il nostro ID | — |
| Una pagina ostile, un iframe di un altro sito o una pagina cambiata nel frattempo | La compilazione controlla `location.origin` in ogni frame. Nessuno script gira sulle pagine | — |
| Siti simili (`example.com.evil.io`, `alice.github.io` rispetto a `mallory.github.io`) | Corrispondenza per eTLD+1 con la Public Suffix List. Https non viene mai offerto a http | test unitari |
| Estrazione di massa | Al massimo 20 password al minuto, e ogni invio appare come notifica nell'app | — |

**Limiti noti, da dichiarare:**
- Un malware che gira con il tuo utente può comunque registrare la master password mentre la digiti (keylogger). Vale per qualsiasi password manager.
- Le build locali hanno solo una firma ad-hoc: la verifica del native host si basa sul percorso nel bundle e
  sulla protezione "App Management" di macOS. Con la firma **Developer ID** diventa crittografica (stesso Team ID)
  e si attiva l'hardened runtime, che impedisce ad altri processi di leggere la memoria dell'app.
- Con l'eTLD+1, una voce salvata per `google.com` viene offerta anche su `sites.google.com`: i contenuti
  pubblicati da utenti su sottodomini dello stesso dominio sono un rischio residuo, attenuato dal fatto che il
  dominio è sempre mostrato nel popup prima della compilazione.
- Su Linux la verifica del browser si basa sul nome dell'eseguibile, perché lì non c'è la firma di codice.
- Browser aggiuntivi (Arc, Vivaldi…) vanno aggiunti a `TRUSTED_BROWSER_TEAMS` in `crates/core/src/peer.rs`
  dopo averne letto il Team ID con `codesign -dv`.

## Mobile (Tauri 2)

Invece di React Native, Android e iOS usano **Tauri 2 mobile**: stessa UI React, stessi comandi Rust,
stesso `kryptos-core` del desktop. Resta nativo solo ciò che il sistema operativo impone.

**Android** (`apps/desktop/src-tauri/gen/android`)
- `KryptosAutofillService.kt` è il servizio di autofill di sistema. Legge il vault sbloccato **nello stesso
  processo** tramite JNI (`src-tauri/src/android.rs`). Se il vault è bloccato, propone "Sblocca Kryptos",
  che apre l'app.
- Il dominio web viene considerato solo se l'app che chiede le credenziali è un browser noto (Chrome,
  Firefox, Brave, Edge, Samsung…). Un'app qualsiasi potrebbe dichiarare `webDomain = banca.it` per rubare
  la password. Le app native ricevono solo le voci collegate esplicitamente a `androidapp://<package>`.
- **Nessun permesso INTERNET** nella build di produzione (c'è solo in debug, per il dev server).
- Backup cloud e trasferimento tra dispositivi disattivati (`data_extraction_rules.xml`).
- `FLAG_SECURE` in produzione: niente screenshot, registrazioni o anteprime nelle app recenti.
- Argon2id a 64 MiB: circa 1,4 s per creare il vault e circa 0,4 s per sbloccarlo sull'emulatore Pixel 7 Pro.
- Librerie native allineate a 16 KB (`.cargo/config.toml`), come richiesto da Android 15 e Google Play.

**iOS** (da fare, serve Xcode): `tauri ios init` più una *Credential Provider Extension* in Swift che
chiama il core Rust tramite FFI e condivide il vault con l'app tramite App Group. Il limite di memoria
delle estensioni (~120 MiB) è il motivo dei 64 MiB di Argon2 su mobile.

## Blocco automatico

- Inattività (1, 5, 15 o 60 minuti). Il tempo trascorso si misura sia con l'orologio monotono sia con quello
  di sistema, e vale il maggiore dei due: il blocco scatta anche se il processo è stato sospeso.
- Sospensione del computer: ce ne accorgiamo perché l'orologio monotono si ferma durante lo sleep, quello
  di sistema no.
- Schermo bloccato su macOS (`CGSessionCopyCurrentDictionary`).
- Appunti: la password copiata viene cancellata dopo 15–120 s, ma solo se nel frattempo non hai copiato altro.

## Roadmap

- [x] Core: vault, KDF, generatore, matching URL, storage atomico, import CSV, analisi sicurezza (23 test)
- [x] Desktop: blocco automatico (inattività, sospensione, schermo bloccato), appunti auto-puliti, impostazioni persistenti
- [x] Import CSV (Google/Chrome, Bitwarden, 1Password, Firefox) e backup cifrato
- [x] Estensione MV3 con ID fisso, collegamento con un clic dall'app, native host incluso nel bundle
- [x] Android: stessa app via Tauri, AutofillService di sistema, nessun permesso di rete
- [ ] Android: test end-to-end dell'autofill in Chrome (serve completare la configurazione iniziale di Chrome sul dispositivo)
- [ ] Android: import CSV e backup (il selettore file restituisce URI `content://`, serve il plugin fs)
- [ ] Sblocco biometrico (Touch ID / impronta), con la vault key protetta da Keychain / Android Keystore
- [ ] iOS: Credential Provider Extension (richiede Xcode)
- [ ] Salvataggio di nuovi login dal browser e dalle app (Android `onSaveRequest`)
- [ ] TOTP
- [ ] Windows: named pipe per il bridge e registrazione del native host nel registro
- [ ] Sync LAN: vault cifrato via TCP locale con pairing tramite QR (chiave effimera X25519 nel QR, canale Noise), merge per voce usando `updated_at`
