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

- La **seconda strada di D4** nel banco di prova, esercitata contro il
  processo. Il controllo `pipeline.indicizzazione.solo_i_ratificati_sono_citabili`
  confrontava l'indice della pipeline con una tabella Rust che dichiarava una
  ratifica: un input che non esiste in nessun file e in nessun database,
  perché una pipeline che indicizza una cartella non firma niente. Il
  controllo è stato **sostituito, non allentato**, da quattro controlli che
  eseguono gli atti che una scuola percorre: `kbs verify` su un database
  vuoto (l'indice non cita niente), `kbs promote` su ciò che la tabella
  dichiara ratificato di fresco (l'atto del docente, e l'unico modo in cui una
  ratifica entra), `kbs verify` sullo stesso database (l'indice cita
  esattamente il gruppo promosso), e la stessa strada su una copia con il
  contratto di un item riscritto sotto la ratifica già firmata (l'item esce
  per `stale-ratification`, e non esce nessun altro). **La ratifica superata
  è così esercitata end-to-end per la prima volta**: prima esisteva solo come
  riga di tabella e come unit test di `kbs-core`.
- `kbs_fixtures::adapter::Pipeline::sequenza`, e con essa `Atto`, `Session` ed
  `Eseguito`: il banco esegue più atti **sullo stesso database**, ciascuno con
  la sua radice. Il confine di processo non è cambiato — il banco non chiama
  funzioni interne, chiama il binario — ed è la stessa interfaccia che D10.2
  dichiara come «la CLI come protocollo».
- **D11 è implementato, e prima non lo era.** `kbs-verify` dichiarava il trait
  `InstanceGenerator` e scriveva che «`kbs-exercise` implementa»: non era vero.
  Nessuna famiglia reale lo implementava, e la determinazione del replay era
  provata solo contro uno stub costruito a mano dentro `kbs-verify` — quindi non
  diceva niente sui generatori veri, che è la terza parte mancante di D16. Ora
  ogni famiglia lo implementa e il catalogo è un enum con due `match` **chiuse**:
  aggiungere una famiglia senza implementare il trait non compila. La
  dipendenza va in un solo verso (`kbs-exercise → kbs-verify`, che non conosce
  `kbs-exercise`) e il grafo resta aciclico. Il nuovo
  `kbs-exercise/tests/il_replay_sulle_famiglie_reali.rs` passa da
  `kbs_verify::replay` — il consumatore vero, non una ricerca di stringhe — su
  istanze vere delle cinque famiglie, e prova anche i due preconditi (corpus e
  versione del generatore) come **verdetti** e non come errori, che è la
  distinzione su cui si regge un ricorso.
- La copertura di «**qualunque stringa dello studente riceve un verdetto**» era
  più stretta della promessa: i test passavano risposte non numeriche e indici
  fuori intervallo, mai una stringa arbitraria. Il nuovo
  `kbs-exercise/tests/qualunque_stringa.rs` passa dieci kB di rumore, caratteri
  di controllo, `usize::MAX` come indice, la stringa vuota, parentesi non
  bilanciate e altre forme, sulle **quattro** forme di risposta e su istanze
  vere dei generatori. Nessuno di quei casi restituiva un `Err`: la promessa di
  `check` era vera, era la misura che mancava. Lo stesso file fissa il confine
  chiuso della tolleranza, il quasi-miss che una tolleranza relativa accetterebbe
  e la precedenza del testo sull'indice nella scelta multipla — regole dichiarate
  in `check` che nessun test prendeva.

- **Il binario del server esiste.** `kbs-serve` è il primo modo di avviare
  `kbs-server`: fino ad ora il crate era una libreria e nessuno poteva prendere
  una porta. Tre cose che la libreria non poteva decidere e il processo decide,
  e che adesso sono scritte in codice: il **rifiuto di partire** su un database
  all'epoch di un binario più nuovo (la guardia di `kbs-store` era spesa dentro
  `kbs-store` e non saliva), il **rifiuto di creare il corpus** (una radice
  vuota fa rispondere «non c'è» a ogni artifact, che è la stessa risposta di
  «non lo vedi», e il motivo diventerebbe invisibile) e il **rifiuto di mettere
  il database dentro il corpus** (D12: l'uscita è `rm -rf`, e un registro che ci
  sta dentro sparisce con i file che dovrebbe descrivere). Tutti e tre escono
  con il codice `2`, che è «rifiutato da una regola» e non «errore di sistema»:
  un supervisor che legge `4` riavvia, e riavviare un binario vecchio sopra uno
  schema nuovo è il danno che la guardia evita. Ascolta su **loopback** per
  default e `--listen 0.0.0.0:…` lo dice ad alta voce, perché qui l'identità è
  una dichiarazione e una porta aperta su tutta la rete è un piedistallo.
  `SIGINT` e `SIGTERM` chiudono in grazia: le richieste in volto finiscono e il
  processo esce con `0`. Il nome è `kbs-serve` e non `kbs` perché `kbs-intake`
  dichiara già un binario `kbs` e `kbs-fixtures` lo cerca in `target/debug` — due
  binari omonimi non falliscono, è l'ultimo che compila che vince, in silenzio.
  `kbs-server/tests/daemon.rs` prova il processo vero su una porta `:0` presa
  dal sistema operativo: salute, radice, `404` identici byte per byte a «non lo
  vedi», e il `SIGTERM`.
- **`observations.unaided` e `observations.n_hints` (migrazione `V6`).** La
  colonna che distingue il sistema da una bottiglia con i fantasmi: senza di
  essa il registro delle dimostrazioni misura **interazione**, e «ha lavorato»
  non è «sa». La claim del progetto — *la quota di argomenti che passano da non
  dimostrato a dimostrato* — con questa colonna ha un numeratore che non
  contiene la coda di practice; senza, non lo ha.
  Le due colonne sono **senza `NOT NULL` e senza `DEFAULT`**, ed è una scelta:
  SQLite non ammette `ADD COLUMN … NOT NULL` senza un `DEFAULT` diverso da
  `NULL`, quindi la colonna sarebbe *obbligata* a dichiarare in anticipo un
  giudizio su righe già scritte. `DEFAULT 1` dichiarerebbe una padronanza che
  il sistema non ha mai misurato, e `DEFAULT 0` dichiarerebbe che ogni
  osservazione passata era assistita: un'altra dichiarazione, non meno
  infondata. `NULL` vuol dire **non registrato**: le righe precedenti alla colonna
  escono fuori dal predicato `unaided = 1` per la logica a tre valori, e non
  entrano in nessuna quota. Il costo è dichiarato nella migrazione.
  `n_hints` è nullable per la stessa ragione — `n_hints = 0` è la misura
  «nessuna pista disponibile», e un conteggio che non è stato fatto non è uno
  zero — e un trigger vieta la combinazione ambigua: non si dichiara quante
  piste c'erano per una riga di cui si ignora se ce n'era qualcuna.
- **La regola «lo studente vede solo le osservazioni non assistite» sta nello
  schema, non in una rotta.** È la vista `unaided_observations`, definita in
  `V6__unaided.sql` con il suo `WHERE unaided = 1`. `kbs-store` sceglie la
  relazione — vista per lo studente, tabella per chi insegna — con la stessa
  relazione di D5 che autorizza la lettura, e non duplica la frase: un filtro
  in una rotta è un filtro che marcisce, una vista è una parte del file che il
  database porta con sé.
- **Il segnale di coorte di D9 conta le osservazioni non assistite**, dalla
  stessa vista. «Dove cade la classe» è una domanda su che cosa gli studenti
  fanno senza aiuto: un tentativo assistito che è andato storto dice che
  l'aiuto non è bastato, e metterlo fra «questa classe non sa il terzo
  teorema» fa dire al docente una cosa che il registro non dice. Il numeratore
  di D9 e la superficie dello studente prendono le righe dallo stesso posto e
  non possono discordare.
- **`kbs insegna`, e con essa la relazione che decide chi vede cosa.** Fino a
  oggi `relations` era una tabella che **nessun codice di produzione
  scriveva**: `Store::add_relation` esisteva, era testata, e i suoi soli
  chiamanti erano test. Il daemon girava, il corpus era ratificato, e nessuno
  poteva diventare `teaches` — quindi `Store::exercise` (D8), `register_scope`
  (lo scrutinio) e `export_fixed_columns` (D12, `NotACourseTeacher`) erano
  chiusi a ogni persona, e `kbs_server::capability::require` rispondeva
  `Absent` su ogni rotta di corso. Il verbo scrive **`teaches` e nient'altro**:
  il rifiuto di `author_of`, `ratified` e `speculative_for` in `add_relation` non
  è stato allargato, perché sono fatti su un oggetto e il predicato li deriva
  dagli argomenti.
  **Non è una strada di rete.** Il verbo esiste solo nella CLI locale, e la
  ragione è dichiarata dove va dichiarata: `kbs_server::identity` dice che
  l'identità è *dichiarata* (`x-kbs-person`, `?person=`) e che «il confine di
  sicurezza è il deployment». Su HTTP o su MCP un verbo che scrive relazioni
  sarebbe la definizione letterale di `teaches` in `kbs_core::may_read`: chiunque
  potrebbe dichiararsi docente di un corso e leggerne tutto. In CLI locale il
  costo è una riga e il rischio è zero, perché chi esegue il comando ha già il
  file del database in tasca. `kbs-intake/tests/la_strada_mcp.rs` lo prova per
  nome, quindi il giorno in cui qualcuno lo espone l'MCP il banco diventa
  rosso invece che la cosa diventa una discussione.
  **Che cosa non c'è, detto qui perché un progetto che enumera le cose fatte e
  tace sulle altre mente per omissione:** non c'è `kbs iscrivi` (l'iscrizione è
  un'altra relazione e un altro percorso), non c'è un verbo che chiuda un
  incarico (`Store::end_relation` esiste e la CLI non lo espone), non c'è una
  **lettura** della provenienza — `relations_of` restituisce le relazioni e non
  chi le ha registrate, e la colonna è raggiungibile solo da SQL finché non
  servirà a un audit, cosa che è un'altra decisione —, e la provenienza non
  entra nell'export D12, che esporta gli argomenti e non le relazioni.
- **La provenienza delle relazioni** (`V8__provenienza_delle_relazioni.sql`):
  `relations.recorded_by TEXT REFERENCES people (id)`, nullable. `relations` era
  l'unica tabella che decide la visibilità e l'unica **senza** chi l'ha
  scritta, mentre `claims`, `observations` e `gradings` portano l'emittente. Il
  `NULL` vuol dire *non registrato*, non *nessuno*, ed è la semantica che `V6`
  dà a `unaided IS NULL`; dichiarare retroattivamente un emittente sarebbe una
  falsificazione firmata da una migrazione. **Nessuna foglia della catena di
  hash cambia** — `relations` non è un registro — e l'export D12 resta a 20
  colonne: è la differenza rispetto a `V6`, che lì lo dichiarava perché lì era
  vero.

### Cambiato

- **`Store::add_relation` chiede chi registra.** La firma è ora
  `add_relation(&mut self, r: &CourseRelation, registrato_da: &PersonId)`. Il
  parametro non è un campo di `CourseRelation` perché `CourseRelation` è **il
  fatto** e chi lo ha premuto è **la dichiarazione del fatto**: dentro il fatto,
  due persone che dichiarano la stessa relazione avrebbero descritto due fatti
  diversi. Tutti i chiamanti sono stati migrati, nessuno è stato lasciato con un
  valore inventato. È un'API interna che pre-1.0 non è stabile, quindi la
  rottura è dichiarata e non scusata.

- **La foglia di ogni riga di `observations` cambia.** `leaf_of` impegna il JSON
  canonico della riga intera, quindi aggiungere due campi a `Observation` cambia
  l'hash delle righe già scritte. Una voce del testimone registrata **prima** di
  `V6` non torna più con la testa ricalcolata, e `kbs-verify` lo dice invece di
  nasconderlo. Non c'è modo di evitarlo senza escludere i campi nuovi dalla
  foglia, e un campo escluso dalla foglia è un campo che un riscrittore può
  cambiare senza pagarne.
- **`record_cohort_signal` rifiuta i numeri dichiarati con la coda di practice
  dentro**, con `CohortCountMismatch` che porta **entrambi** i numeri. Un
  docente che aveva dichiarato `total` contando anche i tentativi assistiti
  riceve un errore, non un silenzio: su un campo compilato il silenzio è il modo
  più veloce per insegnare al chiamante che quel campo non esiste.
- **L'export a colonne fisse non ha una colonna `unaided`, e non è una
  dimenticanza.** La decima colonna, `row_json`, **è** la riga: ne porta il
  JSON canonico, e la foglia ne è l'hash. Una colonna `unaided` metterebbe lo
  stesso fatto in due posti del file, che è la ragione per cui anche
  `at_millis` non viene duplicato. `unaided` è **sempre presente** nel payload,
  dichiarato esplicitamente anche quando non è registrato: `null` e «assente»
  sono due cose diverse nella forma canonica, e quindi una riga con aiuto ignoto
  è falsificabile invece che ambigua.

### Corretto

- `kbs-store` — `observations_for` **onora il predicato che dichiarava**: uno
  `SoloMio` (chi ha emesso un giudizio su quello studente) era accettato e il
  risultato buttato via, quindi un pari che aveva valutato leggeva anche le
  righe che non lo riguardavano. Ora è `NotReadable`, come dice il doc del
  modulo: lo scope del registro delle dimostrazioni è «lo studente o chi
  insegna», e nient'altro.

- `kbs-doc` — `ContractReport::require` non fa più panic su uno stato che il
  tipo consente. «Un contratto troncato non è eseguibile» è una disgiunzione, e
  il membro «troncato» vale senza il membro «errori»: un rapporto che si
  dichiara troncato e non registra l'errore che lo rende non eseguibile —
  stato che `inspect` non produce, ma che è raggiungibile da fuori perché tutti
  i campi sono pubblici e il rapporto deriva `Deserialize` — faceva scattare un
  `expect` la cui giustificazione era, parola per parola, quell'affermazione
  che lo rende possibile. Ora l'errore è per valore e, in quello stato, nomina
  il troncamento (`OverCap`), che è la ragione che `executable` nega.
- `kbs-doc` — due commenti che citavano test **inesistenti**: in `contract`, il
  riferimento alla regola di D7, e in `parser`, quello al caso in cui il
  contratto non è testo del documento. Entrambi ora citano il test che esiste
  e dimostra la frase. In più, `slug` non confronta più `kebab(t)` con sé
  stesso: la riga era decorazione e il test è rimasto quello che pinna il
  valore dello slug.

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
- **Nessun registro accessi nel daemon.** `kbs-serve` stampa l'annuncio
  all'avvio e gli errori di avvio su stderr, ma non installa un
  `tracing-subscriber`: gli eventi `tracing` emessi dagli handler arrivano a un
  sottoscrittore che non c'è e non vengono stampati. Il registro di questa
  istanza è quello del processo che la avvia. È un limite dichiarato e non
  nascosto — aggiungere `tracing-subscriber` porterebbe dentro una dipendenza
  che il workspace oggi non ha, e il daemon è utile senza. Ma è un buco: un
  `tracing::error!` dentro un handler è silenzioso, e `db.rs` scrive che «il
  panico si vede nei log» — il panico si vede, perché va su stderr, ma gli
  eventi no.
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
