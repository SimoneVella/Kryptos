# Firefox Add-ons: note per la revisione

## English (paste in "Notes to reviewer")

| Permission | Why Kryptos needs it | Where in the code |
|---|---|---|
| `nativeMessaging` | The extension holds no passwords. It asks the Kryptos desktop app on the same computer for the logins of the current site, and the app asks the user to confirm each fill. This is the only channel: there are no network requests. | `popup.js` `send()`, `background.js` `ask()` |
| `host_permissions` (`http://*/*`, `https://*/*`) | Logins can belong to any site the user saved, and the list of sites is private to the user, so it can't be written in the manifest. Needed for the content script that opens the list under login fields, and to fill from the list, the keyboard shortcut and the context menu, where there may be no fresh `activeTab` grant. The fill also checks the origin of each frame before writing. | `manifest.json`, `content.js`, `background.js` |
| `scripting` | Writes the chosen username, password and 2FA code in the page's fields, reads what the user typed when they press **Save login**, and puts a generated password in the form. Runs only after a user action. | `background.js`, `popup.js` `readCredentials`, `injectNewPassword` |
| `activeTab` | Lets the popup take one screenshot of the visible tab when the user presses **2FA code from QR**. The image is decoded locally by jsQR and is not stored or sent anywhere. | `popup.js` `scanQr`, `captureVisibleTab` |
| `contextMenus` | Adds *Fill with Kryptos* and *Generate a strong password here* to the right-click menu on fields. | `background.js` |
| `storage` | `local`: the user's choice to turn the list under fields off. `session`: short-lived random tokens that tie the list to the tab it opened in. No passwords or usernames are stored. | `popup.js`, `background.js` |

**Content script.** `content.js` runs on http and https pages in the top frame. It reacts only to a trusted click on a login field. It doesn't read field values and never sees a username or a password: the list lives in `inline.html`, an extension page in a frame inside a closed shadow root, so the site can't read it. Users can turn it off from the popup.

**Remote code.** None. All code is in the package, and the extension pages' CSP is `default-src 'self'; object-src 'none'`.

**Data.** Nothing is sent to the developer or to any server. The page's address goes to the local app through Native Messaging, to look up the logins for that site.

**Third-party code.** `jsQR.js` is the jsQR library (Apache-2.0, https://github.com/cozmo/jsQR), used to read 2FA QR codes locally. [DA COMPLETARE: versione e confronto con il file ufficiale.]

**Generated code.** `brand-icons.js` is generated at build time from simple-icons (CC0) by `apps/desktop/scripts/gen-brand-icons.mjs` (`npm run icons` in `apps/desktop`). `material-symbols.woff2` is a subset font made by `gen-material-icons.mjs`. The source is at https://github.com/SimoneVella/Kryptos.

**How to test.** The extension needs the Kryptos desktop app. Install it from [DA COMPLETARE: link alla release], create a vault with a test password, open **Settings → Browser extension → Connect**, restart Firefox and open a login page.

## Italiano

| Permesso | Perché serve a Kryptos | Dove nel codice |
|---|---|---|
| `nativeMessaging` | L'estensione non contiene password. Chiede all'app Kryptos sullo stesso computer i login del sito, e l'app chiede all'utente di confermare ogni compilazione. È l'unico canale: non ci sono richieste di rete. | `popup.js` `send()`, `background.js` `ask()` |
| `host_permissions` (`http://*/*`, `https://*/*`) | I login possono riguardare qualsiasi sito salvato, e l'elenco dei siti è privato: non si può scrivere nel manifest. Serve allo script che apre la tendina sotto i campi e alla compilazione da tendina, scorciatoia e menu contestuale, dove `activeTab` può non essere attivo. Prima di scrivere si controlla l'origine di ogni frame. | `manifest.json`, `content.js`, `background.js` |
| `scripting` | Scrive username, password e codice 2FA scelti nei campi, legge quanto digitato quando l'utente preme **Salva login**, mette nel modulo una password generata. Solo dopo un'azione dell'utente. | `background.js`, `popup.js` `readCredentials`, `injectNewPassword` |
| `activeTab` | Permette al popup di fare una cattura della scheda visibile quando l'utente preme **Codice 2FA da QR**. L'immagine viene decodificata in locale da jsQR e non viene salvata né inviata. | `popup.js` `scanQr`, `captureVisibleTab` |
| `contextMenus` | Aggiunge *Compila con Kryptos* e *Genera una password sicura qui* al menu del tasto destro. | `background.js` |
| `storage` | `local`: la scelta di spegnere la tendina. `session`: token casuali di breve durata che legano la tendina alla sua scheda. Non vengono salvati password né username. | `popup.js`, `background.js` |

**Content script.** `content.js` gira sulle pagine http e https nel frame principale e reagisce solo a un clic reale su un campo di accesso. Non legge i valori e non vede mai username o password: la tendina sta in `inline.html`, una pagina dell'estensione in un frame dentro una closed shadow root, così il sito non può leggerla. Si spegne dal popup.

**Codice remoto.** Nessuno. Tutto il codice è nel pacchetto, e la CSP delle pagine dell'estensione è `default-src 'self'; object-src 'none'`.

**Dati.** Nulla viene inviato allo sviluppatore o a server. L'indirizzo della pagina arriva all'app locale tramite Native Messaging, per cercare i login di quel sito.

**Codice di terze parti e generato.** `jsQR.js` è la libreria jsQR (Apache-2.0). `brand-icons.js` è generato da `gen-brand-icons.mjs`, il font da `gen-material-icons.mjs`. AMO chiede i sorgenti per il codice generato: il repository pubblico va indicato nell'invio, e se serve va caricato l'archivio con le istruzioni di build.
