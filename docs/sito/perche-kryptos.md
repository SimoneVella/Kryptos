# Perché Kryptos

**Il password manager che non si collega a internet.**
Nessun server, nessun account, nessun abbonamento. Le tue password restano sui tuoi dispositivi, cifrate,
e solo tu hai la chiave.

---

## In breve

- **Zero server.** Non esiste un "cloud di Kryptos" da violare. Niente database con milioni di vault da
  rubare in un colpo solo.
- **Zero account.** Non devi registrarti, non c'è un'email da confermare, non c'è un account da farsi
  rubare con il phishing.
- **Zero rete.** Su Android l'app non ha nemmeno il permesso di accedere a internet: non *può* inviare
  niente, neanche volendo.
- **Gratis e open source.** Licenza GPL-3.0: il codice è pubblico, chiunque può leggerlo e verificarlo.
- **Una conferma per ogni password.** Ogni compilazione chiede la tua impronta o Touch ID. Una password
  non esce mai senza che tu lo sappia.

---

## Sicurezza

### Crittografia di livello professionale, scelta per resistere agli attacchi offline
- **Argon2id** per trasformare la master password in una chiave: il vincitore della Password Hashing
  Competition, volutamente lento e affamato di memoria (256 MiB su computer, 64 MiB su telefono). Ogni
  tentativo di indovinare la tua password costa tempo e RAM, anche a chi usa schede grafiche.
- **XChaCha20-Poly1305** per cifrare il vault: cifratura autenticata moderna, la stessa famiglia usata nei
  protocolli più recenti. Se qualcuno modifica anche un solo byte del file, il vault non si apre.
- **Chiave del vault casuale** a 256 bit, separata dalla master password: cambiare la password non
  ricifra tutto, e la chiave non dipende da quanto è "indovinabile" la password.
- **Algoritmi pubblici e standard.** La sicurezza non si basa su segreti nascosti nel codice, ma solo sulla
  tua master password.

### Una conferma per ogni compilazione
- **Sul computer:** quando l'estensione del browser chiede una password, Kryptos ti chiede **Touch ID** o
  la password del Mac (oppure la master password, se preferisci). Ogni volta, non solo all'avvio.
- **Su Android:** con lo sblocco a impronta attivo, **ogni suggerimento chiede l'impronta** prima di
  compilare. Il controllo lo impone l'hardware sicuro del telefono (Android Keystore), non un semplice
  pulsante "OK".
- Risultato: la sensazione che "la password di tutto sia la master password" sparisce. Ogni singolo accesso
  passa da te.

### Un'estensione del browser costruita per non dare nulla ai siti
- **Un solo piccolo script sulle pagine.** Serve a notare il clic su un campo di accesso e aprire la tendina
  dei login. Non legge quello che scrivi; la tendina è una pagina dell'estensione isolata dal sito, e puoi
  spegnerla dal popup. Per il resto l'estensione entra in una pagina solo quando scegli tu un login.
- **Protezione dal phishing.** Le password vengono offerte solo sul dominio giusto, con la Public Suffix
  List inclusa nell'app: `paypal.com.sito-falso.io` o `alice.github.io` al posto di `mallory.github.io` non
  ricevono niente. Una password salvata per `https` non viene mai offerta a una pagina `http`.
- **Controllo di ogni frame.** Prima di scrivere, l'estensione verifica di nuovo l'origine: un iframe di un
  altro sito o una pagina cambiata nel frattempo non ricevono nulla.
- **Canale verificato con l'app.** L'estensione parla con Kryptos solo tramite il browser, che verifica
  l'identità dell'estensione; Kryptos verifica a sua volta che dall'altra parte ci sia il suo programma
  ponte e un browser firmato. Niente porte di rete aperte sul tuo computer.
- **Ogni compilazione è visibile.** Ogni password consegnata al browser compare come notifica nell'app, e
  c'è un limite di richieste al minuto contro l'estrazione in massa.

### Protezioni di tutti i giorni
- **Blocco automatico** dopo un periodo di inattività, quando il computer va in stop, quando blocchi lo
  schermo e, sul telefono, poco dopo che esci dall'app.
- **Appunti che si puliscono da soli:** una password copiata sparisce dagli appunti dopo il tempo che
  scegli.
- **Niente screenshot delle password** sul telefono, né nell'anteprima delle app recenti.
- **Memoria ripulita:** il motore dell'app cancella chiavi e password dalla RAM quando il vault si blocca.
- **Backup automatico** del vault a ogni salvataggio, con scrittura atomica: un'interruzione di corrente
  non può corrompere le tue password.

---

## Privacy

- **Nessuna telemetria, nessuna statistica, nessun tracciamento.** Kryptos non sa chi sei né come lo usi.
- **I loghi dei siti sono inclusi nell'app.** Molti password manager scaricano l'icona di ogni sito da un
  server, rivelando così quali siti hai salvato. Kryptos no: i loghi sono già dentro l'app.
- **Nessun dato personale richiesto.** Niente email, niente numero di telefono, niente carta di credito.
- **Nessuno può essere costretto a consegnare i tuoi dati,** perché nessuno li ha.

---

## Sincronizzazione senza consegnare le chiavi a nessuno

Kryptos tiene allineati i tuoi dispositivi **senza un server suo**:

- **Con il tuo cloud.** Scegli Google Drive, iCloud Drive, Dropbox o OneDrive: Kryptos ci mette una copia
  **cifrata** del vault e da quel momento allinea da solo Mac e telefono. Il fornitore del cloud conserva
  solo dati illeggibili; Kryptos continua a non collegarsi a internet, il file lo sposta l'app del cloud.
  Per attivarla Kryptos richiede una master password fortissima.
- **Senza nessun cloud.** Trasferisci il vault con un file di backup, oppure con una sequenza di **codici
  QR** mostrati su uno schermo e letti con la fotocamera dell'altro dispositivo. Niente rete, niente
  cavi.
- **Unione intelligente.** Le modifiche vengono unite voce per voce: vince la più recente, e le
  cancellazioni vengono riportate sugli altri dispositivi senza far "resuscitare" voci eliminate.
- **Anche il cambio della master password** arriva da solo sugli altri dispositivi.

---

## Tutto quello che serve, già incluso

- **Compilazione automatica** nel browser (estensione per Chrome, Arc, Brave, Edge, Firefox, con la tendina sotto i campi di accesso) e in tutte le
  app e i browser di Android, con i suggerimenti direttamente nella tastiera.
- **Salvataggio dei nuovi login:** quando ti registri o cambi password, Kryptos propone di salvarla o di
  aggiornare quella vecchia, su Android e dal browser.
- **Codici 2FA (TOTP) integrati:** il codice a 6 cifre viene compilato insieme a username e password. Niente
  più app separata da aprire per ogni accesso.
- **Codici di backup della 2FA:** salvali nella voce, copiali con un tocco e segna quelli già usati. Puoi
  anche caricarli direttamente dal file che il sito ti fa scaricare.
- **Importazione facile:**
  - password da Google/Chrome, Bitwarden, 1Password, Firefox, Dashlane e altri (CSV);
  - codici 2FA da Aegis, 2FAS, Google Authenticator e qualsiasi elenco `otpauth://`.
- **Analisi della sicurezza:** trova le password deboli, quelle riutilizzate su più siti e quelle vecchie,
  con un punteggio complessivo.
- **Generatore di password** configurabile, con stima della robustezza.
- **Backup cifrato** in un file, da conservare dove vuoi.
- **Progetti e `.env`:** i file `.env` stanno nel vault, divisi per progetto e ambiente, con versioni e
  confronto. Con `kryptos env run` le variabili arrivano al comando senza un file su disco.
- **Profili:** vault separati sullo stesso dispositivo (personale, lavoro, gruppo), uno aperto alla volta.
- **Team con inviti (Beta):** ogni persona entra con la propria chiave, senza passarsi una password.
- **Assistenti IA (MCP):** un assistente vede solo i login che condividi, e ogni segreto chiede il tuo OK.
- **Lettura del QR 2FA:** l'estensione legge il QR sulla pagina e lo aggiunge alla voce che scegli.
- **9 lingue:** italiano, inglese, spagnolo, francese, tedesco, portoghese, russo, ucraino, cinese.

---

## Kryptos e gli altri

| | **Kryptos** | Password manager con cloud proprio | Password manager del browser / del sistema |
|---|---|---|---|
| Server dell'azienda con i tuoi dati | **No** | Sì | Sì |
| Account da creare | **No** | Sì | Sì (l'account Google/Apple) |
| Chi ruba l'account vede le password | **Non c'è un account** | Dipende dal servizio | Sì, salvo cifratura aggiuntiva da attivare |
| L'app può collegarsi a internet | **No (su Android neanche il permesso)** | Sì | Sì |
| Conferma con impronta / Touch ID a ogni compilazione | **Sì** | Di solito solo allo sblocco | Di solito solo allo sblocco |
| Codici 2FA integrati | **Sì, inclusi** | Spesso solo a pagamento | In genere no |
| Codici di backup della 2FA | **Sì** | Raramente | No |
| Sincronizzazione | **Sul tuo cloud, cifrata, o senza rete** | Sul loro server | Sul loro server |
| Funziona tra Mac e Android | **Sì** | Sì | Non sempre |
| Loghi dei siti senza contattare server | **Sì** | Spesso no | — |
| Codice sorgente pubblico | **Sì (GPL-3.0)** | Alcuni sì, altri no | No |
| Prezzo | **Gratis** | Spesso in abbonamento | Gratis |

---

## Per chi è Kryptos

- Per chi **non vuole affidare le proprie password a un'azienda**.
- Per chi vuole **sapere esattamente dove sono** i propri dati.
- Per chi usa **Mac e Android insieme** e non vuole restare legato a un solo ecosistema.
- Per chi vuole **password, codici 2FA e codici di backup in un unico posto**, protetti da una sola
  master password e da una conferma a ogni uso.

---

**Kryptos: le tue password, solo sui tuoi dispositivi.**
