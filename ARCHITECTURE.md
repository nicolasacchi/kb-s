# kb-s — decisioni di architettura

Questo file è il contratto. Ogni agente che lavora in questo repository lo legge prima di scrivere codice.
Nessuna decisione qui può essere annullata da un singolo file: cambiarla significa cambiare questo documento
per primo, e dire perché.

Data: 2026-09-26.

---

## D1 · Linguaggio: Rust, non C

`kb` è Rust (axum 0.8, rusqlite, LanceDB, scraper/html5ever). Il requisito «gli stessi database di kb»
e il riuso del modello ad artifact sono entrambi legati all'ecosistema Rust. In C si riscriverebbero i
binding di storage per un guadagno che è zero. `kbs` è un workspace Cargo con più crate.

## D2 · Storage: SQLite ovunque, niente LanceDB — e perché è una scelta, non una semplificazione

`kb` usa **due** motori: LanceDB per il corpus e SQLite per lo stato. In `kb` la parte di LanceDB che
giustifica il peso di Arrow/lance è **l'indice vettoriale**.

Ma la tesi di `kb-s` è che **nessun modello gira dentro il prodotto**. Non c'è embedder, quindi non c'è
vettore, quindi metà della ragione di LanceDB sparisce e resta un database columnare con dentro una
singola tabella di documenti.

Quindi: **SQLite (`rusqlite`) per tutto, con FTS5 per la ricerca lessicale.** Non è una scorciatoia:
è la coerenza fra D2 e la tesi. Il costo reale si paga in tre modi, e sono dichiarati qui:

1. La ricerca è lessicale, non ibrida. Va bene alle scale che `kb-s` considera (una scuola, una
   facoltà, ordine di 10⁴–10⁵ documenti) e va bene *peggio* alle altre. **Se un domani serve la
   ricerca semantica, va aggiunto un embedder**, e allora si aggiunge — non si rimpiange.
2. Lo schema lascia volutamente `chunks.embedding REAL NULL` e un tratto `kbs-store::Provider`, così
   l'aggiunta di un embedder è una migrazione, non un rifacimento.
3. FTS5 ha la sua grammatica di tokenizzazione. Va conosciuta, non aggirata: i termini tecnici
   italiani vanno indicizzati con una pipeline che non li spezzi.

`kb` conserva FTS/BM25 su `title, body, headings, code, prompt`. `kb-s` indicizza gli stessi campi
più `contract`, perché il contratto didattico è parte del contenuto e non un allegato.

## D3 · Nessun modello dentro il prodotto, mai

Non un LLM generativo, non un embedder, non un correttore semantico. L'LLM è **uno strumento del
docente**, esterno, che produce materiale che il docente poi ratifica.

Conseguenze che sono vincoli, non suggestioni:

- Nessun costo di inferenza, a nessuna scala. Il costo del prodotto è archiviazione e ricerca.
- Nessun lock-in di fornitore, nessuna rottura a metà anno scolastico per un deprezzamento.
- **Nessun dato dello studente esce dal prodotto**, perché nel prodotto non entra niente che lo
  tocchi. DPIA, FRIA, profilazione, verifica d'età, alto rischio AI Act: non si applicano *per
  posizione*, non perché sono state risolte.
- Il correttore di esercizi è **deterministico**. L'ordine di grading è: deterministico, poi pari,
  poi umano. Un LLM non valuta nulla (D3), quindi la catena di grading è chiusa e ispezionabile.

## D4 · Due strade di scrittura, non una

- **Percorso di pubblicazione (condiviso)**: generato dal docente con l'aiuto dell'LLM → verificato →
  **ratificato** → `published`. Solo dopo entra nell'indice condiviso e diventa **citabile**.
- **Percorso speculativo (per-studente)**: niente gate, usa-e-getta, non entra nell'indice
  condiviso, **non è citabile**.

Regola che ne deriva, e che va scritta nel codice non solo nei commenti:
> **Un item non verificato è leggibile, ma non è citabile.**

Se i due percorsi scrivono nello stesso modo, il gate è decorazione — e un gate decorativo è
peggiore di nessun gate, perché dà a tutti l'illusione che il controllo esista.

## D5 · Visibilità per relazione, non RBAC

La visibilità è una funzione della **relazione** fra chi chiede e l'oggetto, non di un ruolo con
permessi. Le relazioni sono: `enrolled_in`, `teaches`, `author_of`, `ratified`, `visible_to`, `state == published`.

`kb` oggi ha un solo livello di fiducia, nessun tenant, nessun ruolo, e il mount del corpus *è* l'ACL
(`CLAUDE.md:406-411`, `router.rs:112`, `routes/comments.rs:236-238`). `kb-s` eredita il modello a
documento e corregge la sicurezza. Non è unClone di kb: è l'inverso.

## D6 · Tre registri, append-only, con catena di hash

Tre domande diverse, tre durate diverse, **mai confondibili**:

| registro | domanda | riga |
|---|---|---|
| `claims` | questa frase è vera? | fatto atomico + span che lo sostiene + stato |
| `observations` | chi ha dimostrato che cosa? | dimostrazione su un argomento + prova + chi ha giudicato |
| `gradings` | chi ha deciso, con che cosa, e qualcuno ha contestato? | giudizio + rubric + checker + contestazione/ricorso |

La catena di hash copre `observations`:
`leaf = SHA256(0x00 ‖ canonical_json(row))`, `node = SHA256(0x01 ‖ left ‖ right)`, testa per sessione,
più una consistency proof fra segmenti.

**I tre limiti vanno dichiarati nel codice, non solo nella documentazione**, perché sono le cose che
un revisore cerca per primo e che un sistema onesto dichiara da sé:
1. chi riscrive l'intera catena da capo produce una catena coerente e indistinguibile senza una copia
   indipendente;
2. un rollback da backup è indistinguibile da una riscrittura;
3. l'hash garantisce **integrità, non verità**: uno span che non sostiene la claim è una riga
   impeccabilmente conforme.

## D7 · Il contratto didattico: otto sezioni, 8192 byte, GUARDIAN per primo

Lo slot `<template id="kb-kbprompt">` (nome esatto deciso in `kbs-doc`) contiene un contratto con
**otto sezioni obbligatorie, in quest'ordine**:

| # | sezione | budget |
|---|---|---|
| 1 | `GUARDIAN` | 640 B |
| 2 | `PREREQUISITI` | 256 B |
| 3 | `OBIETTIVI` | 512 B |
| 4 | `SCALA` | 1 536 B |
| 5 | `EQUIVOCI` | 1 024 B |
| 6 | `ESEMPIO-LAVORATO` | 1 536 B |
| 7 | `VERIFICA` | 512 B |
| 8 | `LIMITE` | 256 B |

Somma **6 272 B**, hard cap **8 192 B**, margine **1 920 B**.

Regole che il validatore deve applicare, in fase di build:
- conta i byte e verifica le otto intestazioni;
- `GUARDIAN` deve stare **entro i primi 640 byte**, perché il troncamento taglia la coda;
- `contracts.truncated = 1` ⇒ **non eseguibile**.

> Un contratto troncato è leggibile ma non eseguibile. L'agente può citare l'argomento e mostrare il
> materiale, non può farlo lavorare.

## D8 · Esercizi parametrizzati con verificatore deterministico

Ogni esercizio ha un generatore con seed, e il check è un programma, non un giudizio. Se due studenti
hanno lo stesso item con parametri diversi, la risposta è diversa ma il ragionamento è lo stesso, e
copiare non funziona.

La griglia del grading: `deterministic | peer | human`, in quest'ordine. Un esercizio senza
verificatore deterministico **non entra** nel percorso di pubblicazione: o gli si dà un checker, o
resta speculativo. È la risposta all'integrità accademica per costruzione, non per sorveglianza.

Test obbligatorio su ogni item pubblicato: `no-solution-leak` — il contratto non contiene la
soluzione. Fallisce su 1 000 esempi campionati ⇒ l'unità non pubblica.

## D9 · Il cohorte invisibile, con soglia

Il sistema vede che cosa lo studente produce **attraverso il sistema** e aggrega **anonimamente** quale
argomento cade a livello di classe. Soglia **k ≥ 5**, e la soglia è un accesso, non una cancellazione:
il dato individuale resta.

Zero minuti aggiuntivi per il docente: deriva da compiti che raccoglie già.

## D10 · L'interfaccia con l'IA è del docente, non del prodotto

Nessun modello dentro (D3). L'accoppiamento è a quattro livelli, in ordine di costo crescente, e
tutti e quattro devono esistere:

1. **endpoint di cattura** — il docente incolla, il sistema registra. Il più semplice, il primo.
2. **la CLI come protocollo** — l'agente shella su `kbs`. Già vero in `kb` (`README.md:232-238`).
3. **il file è l'interfaccia** — l'agente scrive un file, `kbs` lo indicizza, il contratto sta nello slot.
4. **MCP in ingresso** — il docente in un agente legge e scrive il corpus. (`kb` oggi non ha un
   server MCP: è un rifiuto registrato, e qui diventa il prodotto.)

Ogni generazione registra un **model lock**: id del modello, hash del prompt, hash del corpus al
momento della generazione, timestamp. Senza questo non c'è un registro, c'è un diario.

## D11 · Il replay deterministico

Data la tupla (hash del corpus, id dei chunk, versione del modello) il sistema deve poter
**riprodurre** la generazione. Serve al ricorso e al diritto di spiegazione. È la condizione perché un
registro append-only sia difendibile davanti a un contesto.

## D12 · L'uscita è `rm -rf`

Il corpus è una cartella di file, versionata, su git. Il server dell'istituto è un mirror, non il
padrone. Esportazione a **colonne fisse** (il protocollo d'intesa MIUR è l'unico contratto di
portabilità normativo che esista in Italia e `kb-s` lo implementa).

## D13 · Interoperabilità in uscita

LTI 1.3 in uscita verso l'LMS che la scuola ha già. Non è opzionale: una scuola non può adottare un
sistema che le impedisce di entrare nel registro elettronico.

---

## Cosa `kb-s` non è

Non è un LMS (Moodle ha 147 283 siti, 116 970 sotto mille utenti). Non è un gradebook. Non è una
libreria di contenuti. Non è un chatbot con un programma. Non è monitoraggio. Non è un rilevatore di
copia: l'integrità è per costruzione (D8), non per sorveglianza. Non è un sistema di formazione
docente. Non è un'agenzia di procurement.

Ognuno di questi confini nomina chi lo fa meglio. Sono decisioni, non mancanze.

## La claim, che resta falsificabile

> Vale la pena costruire `kb-s` **se e solo se**, in una classe che lo usa per un intero quadrimestre,
> la quota di argomenti che passano da «non dimostrato» a «dimostrato» è **almeno il doppio** della
> stessa quota misurata in una classe di controllo che segue il protocollo identico senza il sistema —
> a parità di tempi di pratica assistita, e misurata sul **compito a risorse chiuse**, non sulla
> pratica.

Sotto ×1,5 il progetto è fallito anche se è positivo. La misura va fatta su un **compito ritardato**
(4–8 settimane) e su almeno un **compito di trasferimento**, perché il risultato immediato non
distingue un tutor da un ripetitore di esempi.

**`kb-s` oggi non sa se funziona in una classe italiana, e non lo saprà finché nessuno lo misura.**
