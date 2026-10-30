*[English](ARCHITECTURE.md) · Italiano*

# Kryptos: architettura

Password manager offline e zero-knowledge. Nessun componente apre socket di rete.

```
┌──────────── Browser ────────────┐        ┌──────────── App desktop (Tauri) ─────────────┐
│ extension/popup + background.js │ stdio  │                                               │
│  connectNative ─────────────────┼──────▶ │ kryptos-native-host ──Unix socket 0600──▶ bridge.rs
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
| Codici 2FA | TOTP (RFC 6238, SHA-1/256/512, 6–8 cifre) calcolato in locale in `totp.rs` dal segreto salvato nella voce (chiave base32 o link `otpauth://`), verificato con i vettori della RFC. Importato dai CSV di Bitwarden/1Password/Dashlane e dalle esportazioni delle app di autenticazione (`otp_import.rs`). |

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

1. L'utente apre il popup (o preme ⌘⇧L). L'unico script sulle pagine è `content.js`, che propone i login sotto i campi di accesso (vedi sotto).
2. Il popup invia `{type:"logins", url}` e l'app risponde con le voci che corrispondono al sito.
3. Al click, il popup passa la compilazione a `background.js` e la richiesta di sistema (o la finestra dell'app, in modalità master password) chiede di
   **confermare questa compilazione** (`confirm.rs`): Touch ID o la password del Mac (LocalAuthentication,
   `DeviceOwnerAuthentication`), oppure la master password, verificata sul file del vault con Argon2id.
   Impostazioni → Browser → *Conferma ogni compilazione* sceglie quale; fuori da macOS è sempre la master
   password. Nessuna delle due arriva mai al browser. Il popup si chiude appena Kryptos prende il focus, per
   questo l'attesa avviene nello script in background, su una porta `connectNative` che tiene anche in vita
   il service worker.
4. Solo allora `{type:"credentials", id, url}` restituisce le credenziali, e **solo se** l'URL corrisponde
   alla voce. Il vault non resta occupato mentre l'utente risponde: viene riletto dopo, quindi un blocco nel
   frattempo vince comunque. Aspetta una richiesta alla volta (una seconda riceve `busy`); dopo 170 s arriva
   `timeout` (il native host aspetta 180 s per questa richiesta, 10 s per le altre).
5. Una funzione iniettata una sola volta riempie il form e controlla che `location.origin` non sia cambiato
   nel frattempo. Se Kryptos era venuto in primo piano solo per chiedere, si nasconde di nuovo e il focus
   torna al browser.

Anche **il salvataggio di un login** parte dall'utente: *Salva il login di questa pagina* nel popup inietta una
sola volta una funzione che legge il modulo di accesso (solo stessa origine), mostra username e password
mascherata e, con *Salva*, invia `{type:"save", url, username, password}`. L'app applica la stessa regola
di Android (`UnlockedVault::save_login`): nuova password per la voce di quel sito e username, altrimenti una
voce nuova per l'origine della pagina. I salvataggi rientrano nello stesso limite di 20 al minuto e
compaiono nell'app.

**Funzioni del popup.** Tutte su azione dell'utente, con gli stessi controlli di origine:
- *Questo sito*: i login della pagina, prima quelli usati di recente, con le etichette *riusata* / *debole* / *2FA*.
- *Avviso sito imitato* (`smart::lookalike`): se la pagina non ha un login suo ma imita un sito salvato
  (una o due lettere di differenza, caratteri ingannevoli come `paypa1`, o il dominio salvato usato come
  esca davanti, come `paypal.com.secure-login.io`), il popup mostra un avviso rosso e la scorciatoia di
  compilazione si rifiuta di agire.
- *Cerca*: tutto il vault per nome, utente o sito (solo nomi e utenti, al massimo 20, da due caratteri);
  un risultato si apre nell'app. La compilazione resta limitata al sito di ogni login.
- *Genera*: una password da Web Crypto, scritta nei campi "nuova password" della pagina (mai in quello
  della password attuale), e se vuoi salvata come login del sito.
- *Codice 2FA da QR*: cattura la scheda visibile (`tabs.captureVisibleTab`, permessa da `activeTab` solo
  dopo quel clic), decodifica nel popup il QR `otpauth://` con jsQR e lo passa all'app, che viene in primo
  piano e fa scegliere la voce (`otp_add.rs`). Senza quella scelta non si salva nulla; lo screenshot non
  esce mai dalla memoria del popup.
- *Scorciatoia* ⌥⇧L: compila il login del sito usato più di recente. Il menu del tasto destro offre
  *Compila con Kryptos* e *Genera una password sicura qui* (`contextMenus`, l'unico permesso aggiunto).

Corrispondenza URL (`matching.rs`): stesso dominio registrabile (eTLD+1, Public Suffix List inclusa
nel binario, quindi senza rete). `github.io`, `co.uk` e simili sono trattati come suffissi pubblici. Una voce salvata
per https non viene mai offerta a una pagina http.

**Difese del bridge**, dall'esterno verso l'interno:

| Attacco | Difesa | Verificato |
|---|---|---|
| Un programma si collega direttamente al socket dell'app | L'app identifica il processo collegato (`LOCAL_PEERPID`) e accetta solo il `kryptos-native-host` del proprio bundle, con firma di codice intatta e, se l'app è firmata Developer ID, dello stesso Team ID | `untrusted_peer` |
| Un programma avvia il native host legittimo e gli scrive richieste | Il native host risponde solo se il processo che l'ha avviato è un browser con firma Apple valida di un produttore fidato (Google, Mozilla, Microsoft, Brave) e se il browser dichiara come chiamante l'ID della nostra estensione | `untrusted_caller` |
| Un'altra estensione chiama il native host | `allowed_origins` nel manifest: il browser lo avvia solo per il nostro ID | — |
| Una pagina ostile, un iframe di un altro sito o una pagina cambiata nel frattempo | La compilazione controlla `location.origin` in ogni frame. Sulle pagine `content.js` reagisce solo a un clic reale su un campo di accesso: non legge valori, e la tendina è una pagina dell'estensione in una closed shadow root | — |
| Siti simili (`example.com.evil.io`, `alice.github.io` rispetto a `mallory.github.io`) | Corrispondenza per eTLD+1 con la Public Suffix List. Https non viene mai offerto a http | test unitari |
| Estrazione di massa | Ogni compilazione richiede l'OK dell'utente nell'app (Touch ID, password del Mac o master password); al massimo 20 password al minuto, e ogni invio appare come notifica nell'app | — |

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
  processo** tramite JNI (`src-tauri/src/android.rs`). Se il vault è bloccato, propone "Sblocca Kryptos".
- Il dominio web viene considerato solo se l'app che chiede le credenziali è un browser noto (Chrome,
  Firefox, Brave, Edge, Samsung…). Un'app qualsiasi potrebbe dichiarare `webDomain = banca.it` per rubare
  la password. Le app native ricevono solo le voci collegate esplicitamente a `androidapp://<package>`.
- **Nessun permesso INTERNET** nella build di produzione (c'è solo in debug, per il dev server).
- Backup cloud e trasferimento tra dispositivi disattivati (`data_extraction_rules.xml`).
- `FLAG_SECURE` in produzione: niente screenshot, registrazioni o anteprime nelle app recenti.
- Argon2id a 64 MiB: circa 1,4 s per creare il vault e circa 0,4 s per sbloccarlo sull'emulatore Pixel 7 Pro.
- Librerie native allineate a 16 KB (`.cargo/config.toml`), come richiesto da Android 15 e Google Play.
- I suggerimenti compaiono dentro la tastiera (inline, Android 11+), con il menu a tendina come riserva.
- **Sblocco con impronta**: la vault key è cifrata con una chiave AES dell'Android Keystore che richiede
  una biometria forte per ogni singolo uso e viene distrutta quando si registra una nuova impronta (chi
  conosce il PIN non può aggiungere il proprio dito). Si salva solo la vault key cifrata. La master password
  resta obbligatoria dopo ogni riavvio e ogni 7 giorni. Lo sblocco con il volto funziona solo sui telefoni
  dove è certificato come biometria forte (es. Pixel 8 e successivi, non Pixel 7).
- **Un'impronta per ogni compilazione.** Con lo sblocco a impronta attivo, i suggerimenti non contengono
  né username né password: ognuno è un intent di autenticazione (`AutofillUnlockActivity`). Toccandolo
  compare l'impronta sopra il browser e solo dopo arriva il dataset compilato, quindi Android non tiene
  mai una password che l'utente non ha scelto. Il prompt usa la stessa chiave del Keystore dello sblocco (la
  vault key rilasciata viene cancellata senza usarla), quindi il controllo lo impone l'hardware sicuro, non
  solo una callback. Con il vault bloccato, lo stesso tocco lo sblocca: un solo prompt, non due. L'intent
  porta solo l'id della voce; la corrispondenza viene rifatta sul modulo stesso, quindi non può tirare fuori
  una voce di un altro sito. Senza sblocco a impronta, a vault sbloccato i suggerimenti compilano subito,
  come prima.
- **Salvataggio dei nuovi login.** Ogni modulo di login riceve un `SaveInfo` (password obbligatoria,
  username facoltativo, anche quando i campi spariscono senza chiudere la schermata), così dopo aver
  scritto una password nuova Android chiede "Salvare in Kryptos?". Se sì, `onSaveRequest` la passa a Rust
  (`UnlockedVault::save_login`): la voce dello stesso sito con lo stesso username riceve la nuova password,
  altrimenti nasce una voce nuova, legata al dominio della pagina (browser fidati) o ad
  `androidapp://<pacchetto>` (app), con le stesse regole della compilazione. Con il vault bloccato il login
  aspetta solo in memoria, per al massimo 5 minuti: `SaveLoginActivity` chiede l'impronta oppure, senza,
  si apre Kryptos e lo salva subito dopo la master password. I login divisi su due schermate (prima
  username, poi password) vengono salvati con la sola password.

**iOS** (da fare, serve Xcode): `tauri ios init` più una *Credential Provider Extension* in Swift che
chiama il core Rust tramite FFI e condivide il vault con l'app tramite App Group. Il limite di memoria
delle estensioni (~120 MiB) è il motivo dei 64 MiB di Argon2 su mobile.

## Blocco automatico

- Inattività (1, 5, 15, 60 o 240 minuti). Il tempo trascorso si misura sia con l'orologio monotono sia con quello
  di sistema, e vale il maggiore dei due: il blocco scatta anche se il processo è stato sospeso.
- Sospensione del computer: ce ne accorgiamo perché l'orologio monotono si ferma durante lo sleep, quello
  di sistema no.
- Schermo bloccato su macOS (`CGSessionCopyCurrentDictionary`).
- Android: appena si spegne lo schermo (`ACTION_SCREEN_OFF`) e 1 minuto dopo che Kryptos esce dallo schermo
  (`ProcessLifecycleOwner`), abbastanza per un login in due passaggi. Per riaprirlo basta l'impronta.
- Appunti: la password copiata viene cancellata dopo 15–120 s, ma solo se nel frattempo non hai copiato altro.

## Sincronizzazione senza rete

I dispositivi non si collegano mai tra loro: si porta una copia del **file del vault cifrato** e la si
unisce (`crates/core/src/vault.rs`, `apps/desktop/src-tauri/src/sync.rs`).

- **Trasporto.** Un file di backup (Android lo passa al menu di condivisione di sistema tramite il suo
  `FileProvider`), oppure una sequenza di QR mostrata su uno schermo e letta con la fotocamera dell'altro
  dispositivo (`getUserMedia` + [jsQR](https://github.com/cozmo/jsQR), incluso nell'app). Testo di ogni
  fotogramma: `KRY1|<sessione>|<indice>|<totale>|<base64 di 200 byte>`, correzione d'errore M, versione 13,
  5 fotogrammi al secondo. I 200 byte sono stati scelti decodificando fotogrammi veri con sfocatura e
  contrasto basso simulati; con 300 byte (versione 16) la lettura falliva. Un vault con 100 login sono
  circa 150 fotogrammi, una trentina di secondi per giro.
- **Viaggia il file del vault così com'è su disco**, quindi il canale non impara più di quanto imparerebbe
  da un backup: un file intercettato o una sequenza di QR filmata richiedono comunque la master password,
  e Argon2id rende costoso indovinarla. Per mostrare i QR il vault deve essere sbloccato.
- **Unione.** La copia ricevuta si apre con la *sua* master password (può essere un vault diverso) e si
  unisce voce per voce per id: vince l'`updated_at` più recente; una cancellazione lascia un **tombstone**
  (id + data, nessun dato) che vince su qualsiasi modifica precedente, mentre una modifica fatta dopo la
  cancellazione vince sul tombstone. Una voce con un altro id ma stesso sito, username e password non viene
  duplicata (lo stesso CSV importato su due dispositivi). Se gli orologi dei dispositivi non sono allineati
  un conflitto può risolversi dalla parte sbagliata; per un singolo utente è accettabile.
- **Permessi.** Android chiede la fotocamera solo quando parte la scansione; resta senza permesso INTERNET.
  macOS dichiara `NSCameraUsageDescription` e l'entitlement per la fotocamera.

**Sincronizzazione con una cartella cloud** (`apps/desktop/src-tauri/src/cloud.rs`, macOS e Android).
Kryptos continua a non collegarsi mai: tiene `<cartella cloud>/Kryptos/vault.kryptos` e il file lo sposta
l'app del fornitore (Google Drive, iCloud Drive, Dropbox, OneDrive). Le cartelle vengono trovate in
`~/Library/CloudStorage` e `~/Library/Mobile Documents`, quindi per attivarla basta una scelta, senza
selezionare file.

- **Un solo vault condiviso.** Tutti i dispositivi usano lo stesso vault (id, chiave, master password),
  quindi la copia si apre con la chiave già in memoria e si unisce in silenzio (`UnlockedVault::sync_from`).
  Un dispositivo che parte con un vault suo si unisce una volta (`UnlockedVault::join`): apre la copia
  condivisa con la password di quel vault, ci porta le proprie voci e da lì usa quel vault e quella password.
- **Quando.** Un ciclo in background (5 s) unisce la copia quando il fornitore ne porta una più nuova e la
  riscrive dopo ogni modifica locale (prima legge, poi scrive, così la copia scritta ha entrambe le parti).
  Le copie in conflitto che il fornitore crea quando due dispositivi scrivono insieme (`vault (1).kryptos`…)
  vengono unite e rimosse. Anche il cambio di master password viaggia: il corpo registra quando è cambiata e
  vince l'intestazione più recente (si apre solo con la chiave condivisa, quindi chi non la ha non può
  piazzarne una).
- **Requisito.** Per attivarla serve la master password, verificata sul file del vault, e l'interfaccia
  accetta solo una password "fortissima": la copia ora sta presso terzi, che possono provare password offline.
- **Android** non ha cartelle cloud su disco né il permesso INTERNET, quindi l'utente sceglie la cartella una
  volta con il selettore di sistema (`ACTION_OPEN_DOCUMENT_TREE`; Google Drive, Dropbox… compaiono lì) e
  Kryptos conserva un permesso di lettura e scrittura solo su quella cartella (`CloudFolder.kt`, chiamato da
  Rust via JNI). La rete la fa l'app del fornitore. Unirsi a un vault cambia la chiave del vault, quindi lo
  sblocco a impronta si spegne e va riattivato. Il fornitore può mostrare una modifica remota con un po' di
  ritardo; non si perde niente, si unisce al giro successivo.

## Funzioni smart

- **Cestino** (`vault.rs`): eliminare sposta la voce nel cestino per 30 giorni (sempre cifrata nel vault);
  ripristinarla conta come una modifica fatta ora, quindi vince sull'eliminazione anche sugli altri
  dispositivi. Il cestino si unisce tra dispositivi come le voci.
- **Cronologia delle password**: ogni cambio conserva la password precedente (le ultime 10) con la data,
  visibile e copiabile nella voce. Unendo i doppioni ci finiscono le password delle altre copie.
- **Recenti**: l'ora di ogni copia o compilazione, tenuta a parte dalle voci (`usage`) così usare una voce
  non conta come modifica nei conflitti di sincronizzazione; ordina il filtro *Recenti* e l'elenco
  dell'estensione.
- **Pulizia** (`smart.rs`): titoli leggibili per le voci che hanno come nome un indirizzo
  (`account.live.com` → *Microsoft*, con una piccola tabella inclusa, altrimenti il nome del dominio) e
  gruppi di doppioni (stesso dominio registrabile e stesso utente); unendo resta la più recente e le altre
  vanno nel cestino. Sono solo proposte: le applica l'utente.
- **2FA da attivare** (`twofa.rs`): un elenco incluso dei servizi che offrono i codici delle app di
  autenticazione; le voci di quei siti senza codice compaiono nell'analisi di sicurezza (non pesano sul
  punteggio).
- **Cambia la password sul sito**: apre `https://<sito>/.well-known/change-password` (l'indirizzo standard
  che i siti reindirizzano alla loro pagina), costruito in Rust dal sito della voce; l'interfaccia non può
  aprire indirizzi arbitrari (il plugin opener non è esposto a JS).
- **Passphrase**: parole dalla lista EFF inclusa (7776 parole, circa 12,9 bit l'una), per le master password.
- **QR 2FA con la fotocamera** nell'editor (telefono), **riquadro nelle impostazioni rapide** di Android
  (`KryptosTileService`, apre solo l'app), scorciatoie da tastiera sul computer (⌘K cerca e copia, ⌘C
  utente, ⇧⌘C password, ⌥⌘C codice 2FA).

## Assistenti IA (MCP)

`crates/mcp-server` produce `kryptos-mcp`, un server Model Context Protocol (stdio) incluso accanto al
native host. Un'app IA (Claude Desktop, Claude Code…) lo avvia dalla sua configurazione MCP. Non contiene
segreti e non legge mai il vault: ogni chiamata (`kryptos_list_credentials`, `kryptos_get_credential`,
`kryptos_get_totp`) passa all'app in esecuzione tramite il suo socket Unix. Livelli, tutti necessari:

1. **Spento di default**: Impostazioni → Assistenti IA.
2. **Controllo del programma**: l'app accetta solo il proprio `kryptos-mcp` incluso (stessi controlli di
   percorso e firma del native host), e ognuno dei due programmi può fare solo le sue richieste: il
   server MCP non può chiedere compilazioni, salvataggi o ricerche, il browser non può fare richieste IA.
3. **Codice di accesso**: 256 bit casuali, mostrato una volta in una configurazione pronta da incollare;
   le impostazioni tengono solo il suo SHA-256 (confronto a tempo costante), scade (da 1 ora a 30 giorni)
   e si può revocare.
4. **Condivisione per voce**: compaiono solo le voci con *Disponibile per gli assistenti IA* attivo (solo
   nomi e utenti).
5. **Conferma di ogni segreto**: ogni password o codice 2FA richiede Touch ID / la password del Mac o la
   master password nella finestra dell'app, che dice quale assistente chiede (il nome che dichiara lui,
   solo indicativo). La voce viene riletta dopo la risposta.
6. **Limiti e tracce**: 10 richieste al minuto; ogni richiesta è registrata in memoria (mai su disco, così
   i nomi delle voci non finiscono in chiaro) e annunciata nell'app.

Un segreto dato a un assistente esce da Kryptos (entra nel contesto del modello e sui server di chi lo
gestisce): la condivisione è pensata per credenziali di lavoro o di test, non per la banca o l'email principale.

## Roadmap

- [x] Core: vault, KDF, generatore, matching URL, storage atomico, import CSV, analisi sicurezza, unione per la sincronizzazione (30 test)
- [x] Desktop: blocco automatico (inattività, sospensione, schermo bloccato), appunti auto-puliti, impostazioni persistenti
- [x] Import CSV (Google/Chrome, Bitwarden, 1Password, Firefox) e backup cifrato
- [x] Estensione MV3 con ID fisso, collegamento con un clic dall'app, native host incluso nel bundle
- [x] Android: stessa app via Tauri, AutofillService di sistema, nessun permesso di rete
- [x] Android: autofill end-to-end in Chrome, con i suggerimenti dentro la tastiera
- [x] Android: sblocco con impronta (chiave legata al Keystore), blocco a schermo spento e all'uscita dall'app
- [x] Android: import CSV e backup cifrato (selettore file di sistema, URI `content://` tramite il plugin fs)
- [ ] macOS: sblocco con Touch ID, vault key nel Portachiavi (serve l'app firmata)
- [ ] iOS: Credential Provider Extension (richiede Xcode)
- [x] Android: salvataggio dei nuovi login da app e browser (`onSaveRequest`)
- [x] Salvataggio dei nuovi login dall'estensione del browser su desktop (pulsante nel popup)
- [x] TOTP: codici 2FA nelle voci (si incolla la chiave o il link `otpauth://`, si copia il codice attuale),
      più i codici di backup da segnare come usati (il segno si sincronizza come ogni altra modifica)
- [x] TOTP: il codice viene compilato nei campi del codice monouso su Android e dall'estensione (insieme al
      login, dopo la stessa conferma); importazione da Aegis/2FAS (non cifrati), link di trasferimento di
      Google Authenticator ed elenchi otpauth://; codici di backup letti dal file che il sito fa scaricare
- [x] TOTP dai QR: l'estensione legge il QR della pagina, il telefono lo inquadra con la fotocamera
- [ ] TOTP: esportazioni cifrate di Aegis/2FAS
- [x] Cestino, cronologia password, recenti, pulizia intelligente, 2FA da attivare, passphrase, cambio password sul sito
- [x] Estensione: ricerca, generatore, avviso sito imitato, scorciatoia di compilazione, menu del tasto destro
- [x] Server MCP per gli assistenti IA (spento di default, limitato alle voci scelte, conferma a ogni richiesta)
- [ ] Windows: named pipe per il bridge e registrazione del native host nel registro
- [x] Sincronizzazione senza rete: unione di un backup o trasferimento con i QR, unione per voce con tombstone
- [x] Sincronizzazione con cartella cloud, automatica: macOS trova Google Drive, iCloud Drive, Dropbox,
      OneDrive; Android sceglie la cartella una volta con il selettore di sistema
- [ ] Sync LAN opzionale (abbinamento con QR e chiave effimera X25519, canale Noise). Su Android servirebbe il permesso INTERNET, quindi solo su richiesta esplicita

## Progetti (file .env)

I `.env` stanno nel vault, uno per progetto e ambiente, come testo (commenti e ordine compresi), con le
ultime 10 versioni, un cestino di 30 giorni e la stessa sincronizzazione delle voci
(`crates/core/src/envs.rs`). Lo stesso ambiente creato su due dispositivi si unisce in uno: il contenuto più
vecchio passa nello storico.

`kryptos` (`crates/cli`, incluso nella app) li raggiunge dal socket della app, come `kryptos-mcp`. Non contiene
segreti e la app accetta solo quel binario incluso e con firma verificata, che può fare solo richieste `env`.
Ogni lettura e scrittura si conferma nella finestra della app; una lettura confermata vale 5 minuti nello
stesso sblocco, mai per la produzione. `kryptos env run` passa le variabili al comando e non scrive nulla su
disco; `pull` scrive un file 0600, non sovrascrive un file diverso e avvisa se git non lo ignora.

Gli ambienti si condividono con gli assistenti IA uno per uno. La produzione (nomi che iniziano con `prod`,
`live`, `main`) non si può mai condividere.

## Profili

Vault separati su un dispositivo (`apps/desktop/src-tauri/src/profiles.rs`): ognuno ha file, master password,
impostazioni e cartella cloud propri. Il primo resta nel percorso originale; gli altri stanno in
`<data>/profiles/<id>/`. `profiles.json` contiene solo i nomi e l'id attivo.

È aperto solo il profilo attivo: il cambio scarta il vault decifrato e annulla le conferme precedenti. Così
estensione, assistenti IA, riga di comando e compilazione automatica Android raggiungono solo quel vault. Due
profili non possono usare la stessa cartella cloud. Su Android ogni profilo ha la sua chiave legata all'impronta.

Un team condivide un vault sincronizzando un profilo su una cartella condivisa e condividendo la password di
quel vault. Chi ha la password e una copia del file può leggerlo, quindi quando qualcuno esce vanno cambiati i
segreti contenuti. I team (sotto) evitano di condividere una password.

## Team

Un team condivide un vault tramite una cartella cloud, e le persone entrano con una chiave personale invece di
una password (`crates/core/src/team.rs`). Ognuno mantiene il proprio vault locale e la propria master password.
La copia condivisa è cifrata con una chiave team casuale e separata e non ha alcun accesso via password;
contiene i dati e l'elenco dei membri, mai le chiavi di qualcuno.

Ogni persona ha un'identità X25519 nel vault locale. Manda all'amministratore un codice pubblico, ed entrambi
confrontano un codice di sicurezza a voce o di persona. L'amministratore sigilla la chiave team per quel membro
(X25519 effimero e dell'amministratore, HKDF-SHA256, XChaCha20-Poly1305) in `member-<hash>.key` accanto alla
copia condivisa, e il membro registra l'amministratore.

La rimozione crea una nuova chiave team, sigillata solo per chi resta. Gli aggiornamenti valgono solo se
sigillati dall'amministratore registrato, quindi un membro rimosso non può imporre una chiave. Password e impronta
di un membro non dipendono dalla chiave team. Una copia con la vecchia chiave trovata nella cartella non viene mai
unita: l'amministratore la sovrascrive e gli altri aspettano quella nuova. Quindi le modifiche fatte da un altro
membro durante un cambio chiave possono andare perse, e l'elenco dei membri può essere alterato da qualsiasi
membro attuale (serve solo a mostrarlo: gli aggiornamenti di chiave dipendono solo dall'amministratore).

C'è un solo amministratore, che non può uscire né essere sostituito. Quello che qualcuno ha già letto non si può
riprendere: i segreti visti vanno cambiati.

## Note di sicurezza

Cambiare la master password crea una nuova chiave del vault: la vecchia password con una vecchia copia non legge
più le copie nuove. Gli altri dispositivi smettono di sincronizzare finché non si ricollegano con la nuova
password; una password o un'intestazione non viene mai adottata da un file sincronizzato. Le fusioni ignorano le
date oltre un giorno avanti e ogni modifica ha una data successiva a ciò che sostituisce, quindi una data
futura inserita ad arte non annulla le modifiche successive. Creare un team o unirsi svuota il cestino. Un invito
viene accettato solo se è esattamente il file di cui è stato mostrato il codice di sicurezza.

Una lettura confermata dalla riga di comando vale 5 minuti per qualsiasi processo dello stesso utente del
sistema, non solo per quello che l'ha chiesta. La produzione si riconosce dal nome (`prod…`, `prd`, `live`, `main`,
anche dentro un nome come `eu-prod`), quindi usa nomi di questo tipo.

## Login sotto il campo

`content.js` gira nel frame principale di ogni pagina http(s), ed è per questo che l'estensione ha i permessi sui
siti. Vede solo la pagina. A un clic reale su un campo di accesso interroga `background.js`, che prende il sito dal
browser (`sender`), mai dallo script, e non mostra nulla su una imitazione di un sito salvato. La lista è
`inline.html`, una pagina dell'estensione in un iframe dentro una shadow root chiusa, raggiunta con un token
casuale, quindi il sito non può leggere i nomi utente. I clic valgono solo 400 ms dopo la comparsa, e una
compilazione richiede comunque la solita conferma, che nomina il sito. Il sito può accorgersi che una lista è
comparsa, quindi può capire che lì hai un login. Si può disattivare dal popup: lo script allora non fa nulla.
