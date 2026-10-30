# Mac App Store: note per la revisione

## Da verificare prima di inviare

L'app oggi è pensata per la distribuzione diretta, e il Mac App Store impone la App Sandbox. Queste funzioni
vanno provate in sandbox, perché potrebbero non funzionare o richiedere modifiche al codice:

| Funzione | Rischio in sandbox |
|---|---|
| Collegamento all'estensione | Kryptos scrive il manifest Native Messaging nelle cartelle dei browser, fuori dal suo contenitore. |
| Riga di comando | L'installazione del comando e il socket Unix usati da `kryptos` devono stare fuori dal contenitore. |
| Assistenti IA (MCP) | Il binario `kryptos-mcp` viene avviato da altre app e parla con Kryptos. |
| Sincronizzazione cloud | L'elenco legge `~/Library/CloudStorage`: in sandbox serve il selettore di cartelle con segnalibri di sicurezza. |
| Ricerca dei `.env` | Stesso problema: serve un accesso concesso dall'utente. |

Se non si risolve, le alternative sono togliere queste funzioni dalla versione App Store o restare alla
distribuzione diretta con firma Developer ID e notarizzazione.

## Testo per la revisione (English)

Kryptos needs no account or server. On first launch, create a vault with any master password. The app makes no network connections. Sample flow: add an entry, then open Security and Generator. [DA ADATTARE alle funzioni che resteranno nella versione App Store.]

## Crittografia (export compliance)

L'app usa cifratura (XChaCha20-Poly1305, Argon2id) per proteggere i dati dell'utente. Rispondi alle domande di App Store Connect su questa base e verifica se serve la documentazione per la Francia o per l'esportazione.

## Privacy (App Privacy)

Dichiarazione proposta: **Data Not Collected**. L'URL dell'informativa è obbligatorio.
