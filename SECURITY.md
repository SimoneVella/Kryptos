# Sicurezza / Security

Kryptos è un password manager: le vulnerabilità vanno segnalate **in privato**, mai in una issue pubblica.

## Come segnalare

Usa la segnalazione privata di GitHub: scheda **Security → Report a vulnerability** di questo repository.

Includi, se possibile:
- versione o commit e piattaforma (macOS, Android, estensione browser…);
- i passi per riprodurre il problema;
- l'impatto che ti aspetti (ad esempio "un'altra app locale può leggere le password").

Riceverai una risposta entro 7 giorni. Una volta pubblicata la correzione, sarai citato nelle note di rilascio, se lo desideri.

## Ambito

Rientrano nell'ambito, per esempio:
- decifrare il vault o ricavarne dati senza la master password;
- ottenere credenziali tramite il bridge del browser, l'estensione o il servizio di autofill di Android
  senza l'azione dell'utente, o per un sito diverso da quello salvato;
- qualsiasi traffico di rete generato dall'app;
- fughe di dati sensibili in log, file temporanei, appunti o backup.

Fuori ambito: attacchi che richiedono il controllo completo del sistema (root o jailbreak) o un malware già in
esecuzione che registra la tastiera. Il modello di minaccia completo è in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

**English:** please report vulnerabilities privately via **Security → Report a vulnerability** on this
repository, not in public issues. We aim to respond within 7 days.
