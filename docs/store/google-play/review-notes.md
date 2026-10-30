# Google Play: moduli e note per la revisione

## Sicurezza dei dati (Data safety)

- Dati raccolti: **nessuno**. Dati condivisi: **nessuno**. L'app non ha il permesso `INTERNET` nella versione di release, quindi non può trasmettere nulla.
- Cifratura in transito: non applicabile, l'app non trasmette dati.
- Eliminazione: i dati stanno solo sul dispositivo e si cancellano dall'app o disinstallandola.
- Sincronizzazione cloud: la copia cifrata la sposta l'app del cloud scelta dall'utente tramite il selettore di file di Android. Rileggi le definizioni del modulo su questo punto.

## Permessi e servizi

| Voce | Perché |
|---|---|
| `CAMERA` | Leggere i QR della 2FA e ricevere il vault con i codici QR. Le immagini restano sul dispositivo. |
| `AutofillService` | Compilare i login nelle app e nei browser. L'utente lo attiva dalle impostazioni di Android. |

La build di debug aggiunge `INTERNET` (`src/debug/AndroidManifest.xml`): pubblica solo la release.

## Accesso per la revisione (App access)

L'app non ha account. Al primo avvio chiunque crea un vault con una master password a scelta: nessuna credenziale di prova necessaria.

## Classificazione dei contenuti

Strumento di utilità senza contenuti generati da altri utenti, senza pubblicità.
