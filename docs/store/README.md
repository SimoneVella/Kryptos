# Materiale per gli store

Testi e immagini per Chrome Web Store, Firefox Add-ons, Google Play e Mac App Store, in italiano e inglese. Nulla
è stato inviato. Misure e limiti controllati sulle pagine ufficiali il 30 ottobre 2026.

| Cartella | Contenuto |
|---|---|
| `chrome-web-store/` | Scheda, note per la revisione, icona, 5 screenshot 1280×800, riquadri 440×280 e 1400×560 |
| `firefox-addons/` | Scheda, note per la revisione, icona, 5 screenshot 1280×800 |
| `google-play/` | Scheda, moduli, icona 512, immagine in evidenza 1024×500, 8 screenshot 1080×1920 |
| `mac-app-store/` | Scheda, note per la revisione, icona, 6 screenshot 2560×1600 |
| `privacy-policy.it.md`, `privacy-policy.en.md` | Bozza dell'informativa sulla privacy |
| `github-repository.md` | Descrizione e topic del repository, da applicare a mano |

## Cosa manca per pubblicare

### Bloccante per tutti gli store

- [ ] **Una release scaricabile.** Il repository non ha release, quindi i pulsanti "Scarica" del sito portano a una pagina vuota e chi fa la revisione dell'estensione non può installare l'app.
- [ ] **Informativa online.** Compila contatto e data in `privacy-policy.*.md` e pubblicala a un URL pubblico. Falla leggere a chi conosce la materia.
- [ ] **Account sviluppatore.** Chrome Web Store, Google Play, Apple Developer e Firefox: verifica i costi attuali al momento dell'iscrizione.
- [ ] **Decidere sui marchi negli screenshot.** I dati di esempio mostrano loghi e nomi reali (GitHub, Google, Netflix, Intesa Sanpaolo, Amazon, Spotify, Instagram). Sono i loghi che l'app mostra davvero, ma uno store può contestarli.

- [ ] **Nome.** Verifica che "Kryptos" sia libero su ogni store (Apple richiede nomi unici) e che non ci siano marchi in conflitto.
- [ ] **Dichiarazione di operatore commerciale (DSA, UE).** Gli store possono chiederla: se ti dichiari operatore, indirizzo e telefono possono diventare pubblici. Verifica prima di compilare.

### Chrome Web Store

- [ ] **Campo `key` nel manifest.** Secondo diverse segnalazioni il primo caricamento viene rifiutato se il manifest contiene `key` (fonti discordi sugli aggiornamenti successivi): rimuovilo solo dallo zip per lo store.
- [ ] **ID dell'estensione.** Senza `key` lo store assegna un ID che può essere diverso da `CHROME_EXTENSION_ID` in `apps/desktop/src-tauri/src/browser.rs`. L'app autorizza solo quell'ID, quindi serve una modifica al codice o l'estensione pubblicata non si collega.
- [ ] **Riepilogo.** Controlla se è modificabile: altrimenti arriva da `extDescription` in `extension/_locales`, che va aggiornato (`chrome-web-store/listing.md`).
- [ ] Preparare lo zip di `extension/`, incluso il `brand-icons.js` generato.

### Firefox Add-ons

- [ ] **`data_collection_permissions`.** Obbligatorio per le nuove estensioni dal 3 novembre 2025: aggiungi `"data_collection_permissions": { "required": ["none"] }` in `browser_specific_settings.gecko`. Senza, la firma viene bloccata.
- [ ] Indicare versione e origine di `jsQR.js`, e i sorgenti del codice generato (`brand-icons.js`).
- [ ] Provare il pacchetto su Firefox e il collegamento con l'app.

### Google Play

- [ ] **12 tester per 14 giorni.** Per gli account personali creati dopo il 13 novembre 2023 serve un test chiuso con almeno 12 tester iscritti in modo continuo per 14 giorni, prima di chiedere la produzione.
- [ ] Build AAB di release firmata. La build di debug contiene il permesso `INTERNET`: non caricarla.
- [ ] Compilare Data safety, classificazione dei contenuti e accesso all'app (`google-play/review-notes.md`).
- [ ] Controllare il `targetSdk` richiesto al momento dell'invio (oggi 36).

### Mac App Store

- [ ] **App Sandbox.** È il punto più rischioso: estensione, riga di comando, MCP e cloud potrebbero non funzionare in sandbox (`mac-app-store/review-notes.md`). Va deciso se adattare il codice, togliere funzioni o usare solo Developer ID e notarizzazione.
- [ ] Certificati, provisioning e build firmata per il Mac App Store.
- [ ] Risposte su crittografia ed esportazione, e dichiarazione App Privacy.
- [ ] Aggiornare `mac-app-store/listing.md` in base alle funzioni che restano.

### Prima dell'invio

- [ ] **Screenshot da dispositivo.** Quelli attuali vengono dall'anteprima di design nel browser, con dati finti: stessa interfaccia, ma non sono catture di una build vera. Conviene rifarli da build reali almeno per Google Play e Mac App Store.
- [ ] **Revisione di sicurezza indipendente.** Non è obbligatoria per gli store, ma i testi dicono apertamente che non è stata fatta: aggiornali solo quando esiste.

### Pulizie facoltative

- [ ] `docs/screenshots/` (vecchi) e `docs/assets/social-preview.png` non sono più usati dai README e dal sito: non li ho cancellati, decidi tu.
- [ ] La riga di comando non ha uno screenshot da telefono perché esiste solo sul computer.
- [ ] Nell'app inglese si leggono "1 versions" e "1 titles": plurali da correggere prima di rifare gli screenshot.

## Fonti

- [Chrome: immagini](https://developer.chrome.com/docs/webstore/images), [scheda](https://developer.chrome.com/docs/webstore/cws-dashboard-listing)
- [Firefox: scheda](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/Listing), [consenso sui dati](https://blog.mozilla.org/addons/2025/10/23/data-collection-consent-changes-for-new-firefox-extensions/), [sorgenti](https://extensionworkshop.com/documentation/publish/source-code-submission/)
- [Google Play: risorse grafiche](https://support.google.com/googleplay/android-developer/answer/9866151?hl=en), [limiti di testo](https://support.google.com/googleplay/android-developer/answer/9859152?hl=en), [requisito dei tester](https://support.google.com/googleplay/android-developer/answer/14151465?hl=en)
- [Apple: screenshot](https://developer.apple.com/help/app-store-connect/reference/screenshot-specifications/), [informazioni app](https://developer.apple.com/help/app-store-connect/reference/app-information/app-information), [versione](https://developer.apple.com/help/app-store-connect/reference/app-information/platform-version-information)
- [Chromium: `key` nel manifest](https://groups.google.com/a/chromium.org/g/chromium-extensions/c/x_NBS6_-NKs)
