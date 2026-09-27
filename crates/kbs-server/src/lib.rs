//! `kbs-server` — la superficie HTTP di `kb-s`.
//!
//! Qui D5 smette di essere un tipo e diventa un percorso di richiesta.
//! `kbs-core::may_read` è la funzione, `kbs-store` è il posto in cui viene
//! applicata, e **questo crate non la riapplica**: ogni lettura di un
//! argomento passa per un metodo già gated di `kbs-store`. Il motivo è
//! semantico, non di pigrizia: una seconda copia del predicato in un crate
//! diverso è una seconda risposta alla stessa domanda, e le due divergono.
//!
//! # Le quattro cose che questo crate è
//!
//! 1. **Un predicato, applicato ovunque.** Nessuna tabella `roles`, nessun
//!    controllo per ruolo, nessun `if person.role == "teacher"`. Le relazioni
//!    di D5 sono l'unica cosa che autorizza, e sono relazioni *di corso* o
//!    *di oggetto*, mai attributi di una persona.
//! 2. **Un bus SSE partizionato per corso.** In `kb` il bus è del daemon e non
//!    è partizionato (`kb/crates/kb-server/src/routes/events.rs:82-98`); un
//!    iscritto a un corso riceve l'attività degli altri. Qui ogni corso ha il
//!    suo canale, e un sottoscrittore del corso A non è *strutturalmente*
//!    collegato a quello di B: la partizione non è un filtro applicato
//!    all'evento, è l'assenza del collegamento.
//! 3. **L'assenza e l'invisibilità sono la stessa risposta.** Lo stesso
//!    status, gli stessi header, gli stessi byte. `kbs-store` restituisce già
//!    `NotReadable` per entrambe; qui la traduzione in HTTP non le distingue
//!    e non distingue per durata (vedi [`error`]).
//! 4. **Un'identità che è una dichiarazione, dichiarata tale.** Vedi
//!    [`identity`]: qui non c'è autenticazione e non ce ne sarà in questo
//!    crate, e il testo che lo dice è nel codice perché il primo a leggerlo
//!    deve saperlo prima di fidarsi.
//!
//! # L'identità è una dichiarazione, non un'autenticazione
//!
//! `kb` ha un solo livello di fiducia: non autentica nessuno e ogni identità è
//! un co-operatore a pieni diressi (`kb/CLAUDE.md:406-410`). `kbs-s` non
//! eredita quel modello, ma **non può ancora sostituirlo**: qui non esiste una
//! sessione. Quindi [`Identity`] è un id di persona che *arriva da qualche
//! parte* e viene trattato come un'affermazione del cliente.
//!
//! Le conseguenze sono scritte nei posti in cui contano:
//!
//! * chiunque può dichiarare chi è, quindi [`Identity`] non è una
//!   credenziale e non va trattata come tale;
//! * il predicato D5 resta il controllo di accesso *dell'applicazione*, e
//!   diventa un controllo di sicurezza *del deployment* solo quando qualcosa
//!   davanti a questo server autentica e rende credibile la dichiarazione;
//! * nessun endpoint scrive sulla fiducia della dichiarazione per rafforzarla:
//!   un link con `?person=…` è condivisibile come un header, e la sua
//!   condivisione non è un incidente perché non è un segreto.
//!
//! # Le route
//!
//! Convenzione: `404` significa **una sola cosa** — «non c'è, o non lo vedi» —
//! e la risposta è identica byte per byte nelle due ipotesi. Le risposte che
//! riguardano una persona non sono cacheabili: `Cache-Control: no-store` su
//! tutto ciò che sta sotto `/api/`.
//!
//! ### Argomenti, claim, ricerca
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/api/v1/courses/{course}/arguments` | `200` `{arguments:[…], course:…}` — elenco di ciò che la persona vede, **anche vuoto** | `404` se non ha relazioni col corso |
//! | `GET` | `/api/v1/arguments/{id}` | `200` `{argument:{…}}` | `404` |
//! | `GET` | `/api/v1/arguments/{id}/claims` | `200` `{claims:[…]}` | `404` |
//! | `GET` | `/api/v1/search?q=…&limit=…` | `200` `{hits:[…]}` | `400` ricerca vuota o `limit` fuori scala |
//!
//! **Ogni rotta di corso dà la stessa risposta per «corso inesistente» e
//! «corso di cui non hai relazioni»**: `404`, con gli stessi byte. Un elenco
//! vuoto sarebbe coerente solo se lo fosse ovunque, e non lo è: la coda di
//! ratifica, la coorte e l'esportazione non possono rispondere `[]` a chi non
//! c'entra, perché `[]` è una risposta vera e lì non lo sarebbe. Quindi la
//! regola è una sola e vale per tutte: senza relazione, «assente».
//!
//! ### La coda di ratifica (D4) — superficie di prodotto, non pagina di amministrazione
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/api/v1/courses/{course}/queue` | `200` `{queue:[…]}` | `404` se non `teaches` il corso |
//! | `POST` | `/api/v1/courses/{course}/queue/{id}/ratify` corpo `{note}` | `200` `{ratification:{…}}` | `400` nota vuota · `404` · `409` |
//! | `POST` | `/api/v1/courses/{course}/queue/{id}/publish` | `200` `{state:"in-corso"}` | `404` · **`409` senza ratifica valida** |
//! | `POST` | `/api/v1/courses/{course}/queue/{id}/withdraw` | `204` | `404` |
//!
//! Ogni voce della coda è `{argument, needs_ratification, needs_publication}`.
//! La coda contiene **solo** argomenti in stato mutabile: nulla che sia già in
//! corso o archiviato la attraversa, perché un oggetto già pubblicato non
//! «aspetta» nessuno.
//!
//! ### I registri (D6)
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/api/v1/arguments/{id}/observations?person=…` | `200` `{observations:[…]}` | `404` |
//! | `GET` | `/api/v1/courses/{course}/gradings?person=…` | `200` `{gradings:[…]}` | `404` |
//!
//! `person` è opzionale e vale l'identità dichiarata: uno studente vede il
//! proprio registro senza chiedere nulla, un docente vede quello di chi
//! chiede. Le contestazioni viaggiano **dentro** la riga del giudizio, non in
//! una rotta separata: chi legge un ricorso deve vedere il voto e la
//! contestazione nello stesso atto.
//!
//! ### Coorte (D9)
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/api/v1/arguments/{id}/cohort` | `200` `{argument:…, signals:[…]}` | `404` se non `teaches` il corso |
//!
//! Sotto `COHORT_MIN_K` non c'è un segnale: c'è un elenco **vuoto**, senza
//! conteggi, senza zero, senza segnaposto. `{"signals":[]}` è l'unica risposta
//! possibile, e il filtro qui è doppio di proposito ([`routes::cohort`]).
//!
//! ### Esportazione a colonne fisse (D12)
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/api/v1/courses/{course}/export` | `200` `text/tab-separated-values` con `Content-Disposition` | `404` se non `teaches` il corso |
//!
//! ### Eventi (SSE)
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/api/v1/courses/{course}/events` | `200` `text/event-stream` | `401` senza identità dichiarata · `404` senza relazione col corso |
//!
//! ### three.js vendorizzato (D15)
//!
//! | metodo | percorso | successo | errori |
//! |---|---|---|---|
//! | `GET` | `/three/three.module.min.js` | `200` `application/javascript` con `ETag` e `Cache-Control` lungo | `304` · `500` se il runtime non è vendorizzato |
//!
//! Serve su **qualsiasi** origine, artifact compresa: è la rotta su cui
//! l'artifact carica il runtime, e l'artifact sta su un'altra origine.
//!
//! ### Gli artifact, su un'altra origine
//!
//! La rotta è il *fallback* del router, e la decisione è nel header `Host`:
//! se l'host ha la forma `<argomento><suffix>` si serve l'artifact da quel
//! label, altrimenti si serve l'interfaccia dalla radice web — che è
//! `crates/kbs-server/web/`, il punto di mount dell'agente delle rotte. Se la
//! radice non c'è, la risposta è `404` `interfaccia-non-installata`, e lo dice.
//!
//! Vedi [`sandbox`] per la grammatica dell'host e per la ragione — l'unica cosa
//! che rende sicuro `allow-same-origin` — di cui il `sandbox` **non** è
//! responsabile.
//!
//! # Che cosa deve sapere un client, senza leggere un handler
//!
//! * **Identità**: `X-Kbs-Person: person_0001` sulle chiamate che il client
//!   controlla, `?person=person_0001` su quelle che il browser non può
//!   intestare — la navigazione di un `<iframe>` e `EventSource`. Sono la stessa
//!   dichiarazione, e sono entrambe dichiarazioni ([`identity`]).
//! * **`404`**: una sola risposta per «non c'è» e «non lo vedi», corpo
//!   `{"error":"non-trovato"}`. Un client che vuole distinguere i due casi non
//!   può, e non deve.
//! * **`401`**: nessuna dichiarazione. **`400`**: dichiarazione malformata o
//!   richiesta non valida. **`409`**: la regola di dominio ha detto no, e
//!   `regola` dice quale (`D4`, `D9`).
//! * **`no-store`**: tutto ciò che sta sotto `/api/` è di una persona e non
//!   entra in nessuna cache. `/three/` fa eccezione ed è l'unico con cache
//!   lunga.
//! * **Artifact**: `<iframe src="https://<argomento><suffisso>/?person=…">`, con
//!   `sandbox` = [`sandbox::SANDBOX_FLAGS`]. Il suffisso non è decorazione:
//!   [`sandbox`] spiega perché, e il test `l_isolamento_viene_dall_origine_e_non_dal_flag`
//!   lo tiene vivo.
//! * **SSE**: il primo evento è `ciao` con `da_evento`, che è **dove comincia**
//!   la vista e non da dove riprendere. Poi `ratificato`, `pubblicato`,
//!   `ratifica-ritirata`, `lag`. `Last-Event-ID` non è onorato e non lo
//!   sarà: il bus non ha un log, e la verità è la coda.
//! * **Formati**: le risposte sono `{ "<chiave>": … }`, mai liste nude, perché
//!   possono aggiungere campi senza rompere nessuno.
//!
//! # Quello che questo crate non costruisce
//!
//! * **nessuna autenticazione e nessuna sessione**, per scelta e non per
//!   dimenticanza: [`identity`] dice perché;
//! * **nessun ruolo**, in nessun punto, nemmeno in un commento come scorciatoia;
//! * **nessuna scrittura dei registri**: appendere un giudizio o
//!   un'osservazione è un atto del docente che passa dalla CLI e dal file
//!   (D10), e una rotta HTTP che lo accettasse trasformerebbe il registro da
//!   atto a modulo da compilare;
//! * **nessun frontend**: l'interfaccia è dell'agente dopo, e può contare su
//!   questa tabella senza leggere un handler.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod bus;
pub mod capability;
pub mod config;
pub mod db;
pub mod daemon;
pub mod error;
pub mod identity;
pub mod ids;
pub mod router;
pub mod routes;
pub mod sandbox;
#[cfg(test)]
mod tests;
pub mod vendor;

pub use bus::{Bus, CourseEvent, EventKind};
pub use config::ServerConfig;
pub use db::Db;
pub use daemon::{Errore as DaemonError, Esito, Opzioni, Richiesta};
pub use error::{ApiError, ErrorBody};
pub use identity::{Identity, IDENTITY_HEADER, IDENTITY_QUERY};
pub use router::app;
pub use sandbox::{
    artifact_host, origin_of, parse_artifact_id, ArtifactResolution, SANDBOX_FLAGS,
    DEFAULT_HOST_SUFFIX,
};
pub use vendor::Vendor;
