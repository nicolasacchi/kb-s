//! `kbs-store` — la persistenza di `kb-s`.
//!
//! SQLite ovunque, FTS5 per la ricerca lessicale, e nessun modello dentro il
//! prodotto (D2, D3). Questo crate è il secondo pezzo più importante del
//! repository dopo `kbs-core`: `kbs-core` dice *che cosa* è lecito, qui si
//! verifica *che cosa è successo* e *chi lo può sapere*.
//!
//! # I tre limiti che questo crate dichiara da sé
//!
//! D6 chiede che siano dichiarati nel codice e non solo nella documentazione,
//! perché sono le cose che un revisore cerca per primo e che un sistema onesto
//! dichiara da sé. Qui sono in tre punti precisi, e nessuno dei tre è risolto:
//!
//! 1. **La catena di hash non è calcolata qui.** Questo crate conserva
//!    l'ordine che la catena ordina — `observations_in_session` restituisce
//!    `kbs_core::Observation` nell'ordine del `seq`, così `kbs_verify::leaf_of`
//!    lavora sul tipo di dominio senza conversioni — e i trigger di
//!    `observations` rifiutano `UPDATE` e `DELETE`. *Chi riscrive l'intera catena
//!    da capo produce una catena coerente e indistinguibile senza una copia
//!    indipendente.* I trigger fermano la riscrittura **per iscritto**: non
//!    fermano un ripristino da backup, un `DROP TABLE` ricreato o un file
//!    copiato sopra, e non sono una prova crittografica di niente.
//! 2. **Un rollback da backup è indistinguibile da una riscrittura.** Il
//!    formato del database è lo stesso prima e dopo, e l'epoch è la stessa.
//!    Un ripristino è una scelta legittima e lascia la stessa traccia di una
//!    cancellazione.
//! 3. **L'hash garantisce integrità, non verità.** Una claim può avere uno
//!    `span_anchor` e uno `span_text` perfettamente conformi e sostenere
//!    un'affermazione falsa. Il registro conserva l'affermazione, il suo stato
//!    e la sua traccia; non certifica che l'affermazione sia vera.
//!
//! # La visibilità è decisa in un posto solo
//!
//! `kbs_core::may_read` è la funzione, e questo è il posto in cui la si
//! applica. **Ogni lettura di un argomento per una persona passa di lì**, e sono
//! cinque le strade che lo fanno: [`Store::read_argument`],
//! [`Store::visible_arguments`], [`Store::claims_for`],
//! [`Store::observations_for`], [`Store::gradings_for`] e [`Store::search`].
//! Le query non filtrate sono `pub(crate)`, e l'unico modo pubblico di arrivarci
//! è [`Store::conn`], che è dichiarato come la porta di servizio che è.
//!
//! Le altre tre cose hanno regole diverse, e sono dichiarate per quello che
//! sono invece di essere accodate a `may_read` con la scusa che «è la stessa
//! cosa»:
//!
//! * **i registri di uno studente** ([`Store::student_gradings`]) sono letti da
//!   chi è lo studente, da chi ha emesso un giudizio e da chi insegna: la
//!   regola è in `registers`, dichiarata in tre righe;
//! * **l'esportazione** ([`Store::export_fixed_columns`]) è del docente del corso;
//! * **rubriche, generazioni e manutenzione dell'indice** non hanno una persona
//!   e non hanno un predicato: sono letture di manutenzione, e fingere il
//!   contrario sarebbe unRBAC travestito da sicurezza.
//!
//! I due limiti di quel modello, dichiarati perché sono i prossimi a venire
//! letti: i **ruoli non esistono** (D5 li vieta come oggetto memorizzato: sono un
//! insieme di relazioni e si derivano), e **non c'è autenticazione** — il
//! predicato è il modello di sicurezza, la sessione non è ancora costruita, e
//! chi scrive non è autorizzato come chi legge.
//!
//! # La ricerca in italiano costa, e quanto costa
//!
//! [`italian`] misura il tokenizzatore invece di descriverlo, e le otto cose
//! che misura sono documentate lì con il costo di ognuna. In una riga: FTS5
//! `unicode61` piega gli accenti e basta, e chi scrive in italiano scrive
//! apostrofi, trattini, due punti e virgole decimali — che per `unicode61` non
//! sono testo, sono sintassi. La risposta non è un dizionario delle elisioni,
//! è una pipeline di piegatura in Rust applicata **a entrambi i lati** dell'indice.
//!
//! # Che cosa questo crate NON fa
//!
//! * **non autentica e non autorizza in scrittura**: il predicato governa le
//!   letture; chi può scrivere dipende dalla sessione, che non è qui;
//! * **non calcola la catena di hash** e non verifica l'integrità delle
//!   generazioni: conserva l'ordine, `kbs-verify` verifica;
//! * **non aggrega i segnali di coorte**: li registra, rifiutando quelli sotto
//!   soglia, e sa che «chi sbaglia» è una domanda del docente e non del
//!   database;
//! * **non ha un embedder e non riempirà mai `artifact_chunks.embedding`**: D3.
//!   La colonna c'è perché aggiungerlo sia una migrazione (D2), e resta `NULL`
//!   perché in questo repository non esiste il codice che la scriverebbe.
//! * **non serve HTTP**: quello è `kbs-server`.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
pub mod italian;
mod publish;
mod registers;
mod schema;
mod search;
mod store;
pub mod types;
mod export;

pub use error::{Error, Result};
pub use export::FIXED_COLUMNS;
pub use schema::{binary_epoch, MINIMUM_SQLITE};
pub use store::Store;
pub use types::{
    ArtifactChunk, ChunkId, ChunkKind, CourseRelation, GenerationEvent, GradeLevel, GradingDraft,
    ObservationDraft, Person, Register, Rubric, RubricVersion, SearchHit, SessionId, Source,
    SourceStatus,
};

/// La versione di `kbs-store`. Se un altro crate ha bisogno di saper con quale
/// schema è stato scritto un database, la domanda giusta non è questa: è
/// `Store::schema_epoch`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
