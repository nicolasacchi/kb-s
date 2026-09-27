# Changelog

Tutte le modifiche rilevanti di `kb-s` sono qui. Il formato segue
[Keep a Changelog](https://keepachangelog.com/it/1.1.0/) e il versioning segue
[Semantic Versioning](https://semver.org/lang/it/).

Il progetto è a **0.1.0**: pre-1.0, l'API interna non è stabile e le voci
«Cambiato» descrivono cambiamenti che rompevano il codice. Una cosa che non
compare qui **non è stata fatta**.

La sezione **Non costruito** non è una lista di desideri: è l'elenco di ciò che
`kb-s` dichiara di fare in `ARCHITECTURE.md` e che non esiste ancora nel
codice. Un progetto che non elenca i propri buchi li ripete dentro se stesso.

## [Non rilasciato]

### Aggiunto

- Nessuna modifica non pubblicata. La prima release è `0.1.0`.

## [0.1.0] — 2026-09-26

La prima versione: un banco di prova e la faccia pubblica del repository. Non
è un prodotto utilizzabile in classe, e le sezioni sotto dicono perché.

### Aggiunto

- `kbs-core` — i tipi di dominio. `PublicationState` è una macchina a stati e
  non una stringa; `GraderKind` non ha una variante «modello», che è il modo in
  cui la regola di D3 è resa impossibile invece che documentata; la visibilità è
  la funzione `may_read` e non un ruolo; `check_citable` è l'unico posto in cui
  la citabilità viene decisa, e distingue la ratifica assente dalla ratifica
  superata.
- `kbs-fixtures` — il banco di prova: quaranta item che coprono le dodici
  famiglie di media del corpus di studio, con cinque fixture rotte per
  costruzione (sezione mancante, `GUARDIAN` oltre i 640 byte, contratto oltre
  l'hard cap, riferimento a una CDN, ciclo nei prerequisiti), una ratifica
  superata, una claim contraddetta e una non citabile, una scena 3D con una
  claim per nodo e per arco, e un esercizio parametrizzato con due istanze. Il
  runner dice in ogni esecuzione che cosa ha verificato, che cosa ha fallito e
  **che cosa ha saltato e perché**.
- `kbs-bench` — il comando del banco. `run` gira e stampa il referto,
  `emit` riscrive il corpus a partire dalla tabella. Il corpus è quindi
  riproducibile e non scritto a mano.
- `ARCHITECTURE.md` — quindici decisioni (D1–D15) che valgono come
  contratto, e la claim falsificabile del progetto con la sua soglia.
- `vendor/three/` — three.js r170 vendorizzato nel repository, minificato e
  gzippato, perché una classe non ha garanzia di rete.
- La CI che costruisce, prova, esegue i doc-test e gira il banco, con la
  toolchain inchiodata e `-D warnings` su tutto il workspace.

### Non costruito

Elencato per ordine di gravità, non di importanza percepita.

- **Nessun modello dentro il prodotto — e questo è l'unico modo in cui la
  privacy è garantita.** È l'unica riga di questa lista che è una
  *decisione* e non una lacuna. Quello che non esiste è l'acoppiamento col
  docente: nessun endpoint di cattura, nessuna CLI come protocollo, nessun
  file come interfaccia, nessun server MCP in ingresso (D10). L'unico di questi
  quattro che il repository dimostra è la CLI, e la dimostra come **banco**,
  non come prodotto.
- **Nessuna pipeline sotto il banco.** Il banco chiama `kbs verify --json` e,
  in assenza del binario, salta i nove controlli che ne dipendono dicendo
  perché. In locale il banco è verde con nove saltati; in CI, dove gira con
  `--require-pipeline`, quegli stessi saltati sono un fallimento. Finché i
  crate non sono a posto, la metà `pipeline.*` del banco **non ha mai girato** e
  nessuna capacità di D7, D8, D11 e D15 è dimostrata.
- **Nessuna validazione del contratto didattico** (D7): il banco scrive
  contratti di otto sezioni e ne misura i byte, ma non li valida. La
  validazione è di `kbs-doc`, che non è ancora scritto.
- **Nessuno store, nessuna catena di hash, nessun registro** (D6): le tre
  tabelle, la catena `leaf = SHA256(0x00 ‖ riga)` e la consistency proof fra
  segmenti non esistono. I limiti che D6 dichiara — riscrittura indistinguibile
  senza copia indipendente, rollback indistinguibile da riscrittura, integrità
  e non verità — sono quindi **dichiarati e non ancora applicati**.
- **Nessuna coorte** (D9): la soglia `k ≥ 5` esiste come costante in
  `kbs-core` e come predicato, e non esiste come aggregazione.
- **Nessuna visibilità applicata** (D5): il predicato esiste ed è testato, e
  nessun accesso passa attraverso di esso.
- **Nessuna scena 3D renderizzata** (D15.1): i dati ci sono, in un manifest
  separato dal codice che li disegna, e la scena non viene disegnata da nessuna
  parte. Le quattro regole di D15.1 sono esercitate come proprietà dei dati e
  non come interazione.
- **Nessun'uscita `rm -rf`, nessuna colonna a larghezza fissa, nessun
  LTI 1.3** (D12, D13).
- **Nessuna conversione da markdown** (D14): l'HTML è il formato nativo e il
  markdown in ingresso non esiste come percorso.
- **Nessuna misura.** La claim del progetto è formulata e non valutata. Il
  criterio di fallimento — sotto ×1,5 il progetto è fallito anche se è
  positivo — non è stato applicato a nessuna classe, perché nessuna classe usa
  ancora il sistema.

### Limiti dichiarati, non risolti

- La catena di hash coprirà `observations` e non i voti. Un sistema che
  registra le prove e non i giudizi rende la difesa del ricorso più difficile
  di quanto sembri a prima vista, e va detto prima che venga costruita.
- Il banco verifica le **fixture** e il **comportamento dichiarato**. Non
  verifica l'ergonomia, l'accessibilità, la compatibilità con i browser delle
  scuole italiane, e non può: un banco che finge di coprirle è peggio di un
  banco che le dichiara scoperte.

[Non rilasciato]: https://github.com/nicolasacchi/kb-s/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nicolasacchi/kb-s/releases/tag/v0.1.0
