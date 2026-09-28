# Kryptos

Password manager 100% offline per macOS, Windows, Linux e Android (iOS in arrivo).
Nessun server, nessun account, nessuna connessione di rete: il vault cifrato resta sul tuo dispositivo.
Dettagli tecnici e modello di minaccia in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

> [!WARNING]
> **Versione 0.1: software sperimentale.** Il codice non ha ancora avuto una revisione di sicurezza
> indipendente e alcuni flussi (compilazione nel browser, autofill su Android) sono stati provati solo in
> ambienti di test. Non usarlo come **unica** copia di password importanti: tieni un'altra copia finché il
> progetto non è maturo. Le vulnerabilità vanno segnalate in privato: vedi [SECURITY.md](SECURITY.md).

**Crittografia:** Argon2id (256 MiB su desktop, 64 MiB su mobile) e XChaCha20-Poly1305, con una vault key
casuale cifrata dalla master password. Algoritmi pubblici e standard: la sicurezza dipende dalla tua
master password, non dalla segretezza del codice.

```
crates/core                 Rust: crittografia, vault, generatore, matching URL, import CSV, analisi sicurezza
crates/native-host          Relay Native Messaging (browser ⇄ app), incluso nell'app come sidecar
apps/desktop                App Tauri 2 condivisa desktop + mobile
  src/                      UI React (unica per tutte le piattaforme)
  src-tauri/src/            Comandi Rust, bridge browser (desktop), JNI autofill (Android)
  src-tauri/gen/android/    Progetto Android: AutofillService in Kotlin
extension/                  Estensione browser MV3 (Chrome, Arc, Brave, Edge, Firefox)
apps/desktop/brand/         Logo sorgente + make-icons.py (rigenera tutte le icone)
```

## Sviluppo

Requisiti: Rust (rustup), Node 20+. Per Android: Android Studio (SDK + NDK).

```bash
cd apps/desktop && npm install && npm run tauri dev
```

**Anteprima di design nel browser** (dati finti, niente Rust): `npm run dev`, poi apri
http://127.0.0.1:1420. La master password è `password`.

Test del core:

```bash
cargo test -p kryptos-core
```

### Build

```bash
cd apps/desktop && npx tauri build            # macOS .app / .dmg (include il native host)
```

```bash
cd apps/desktop && npx tauri android build --apk --target aarch64
```

Per Android servono `ANDROID_HOME`, `NDK_HOME` e `JAVA_HOME`. Il JDK incluso in Android Studio va bene:
`/Applications/Android Studio.app/Contents/jbr/Contents/Home`.

## Cambiare il logo

```bash
cd apps/desktop && python3 brand/make-icons.py /percorso/nuovo-logo.png && npx tauri icon icon.png -o src-tauri/icons && python3 brand/make-icons.py --android-only
```

## Estensione browser

1. `chrome://extensions` → Modalità sviluppatore → *Carica estensione non pacchettizzata* → `extension/`
   (l'ID è fisso: `jclbckdbmecjgoopnpchbijihdfeajkb`)
2. Nell'app: **Impostazioni → Estensione browser → Collega**, poi riavvia il browser
3. Su una pagina di login: clic sull'icona Kryptos, oppure ⌘⇧L

La chiave privata dell'estensione (`secrets/extension-key.pem`) serve solo per pacchettizzare un `.crx`.
Non va condivisa ed è esclusa da git.

## Autofill su Android

Impostazioni Android → Password e account → Servizio di compilazione automatica → **Kryptos**.
Oppure, via adb:

```bash
adb shell settings put secure autofill_service com.kryptos.app/.KryptosAutofillService
```

## Licenza

[GPL-3.0-or-later](LICENSE). Puoi usare, studiare, modificare e ridistribuire Kryptos. Le versioni
modificate che distribuisci devono restare open source con la stessa licenza.
