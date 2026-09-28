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
//! applica. **Ogni lettura di un argomento per una persona passa di lì.** Le
//! query non filtrate sono `pub(crate)`, e l'unico modo pubblico di arrivarci è
//! [`Store::conn`], che è dichiarato come la porta di servizio che è.
//!
//! ## L'inventario, che è chiuso
//!
//! Un inventario dichiarato e non controllato è un inventario che invecchia, e
//! quindi **questo elenco è chiuso e c'è un test che lo controlla**: ogni `pub fn`
//! che consegna contenuto di un corso senza prendere una persona deve comparire
//! qui, per nome. `tests::inventory` legge le firme dal sorgente e fallisce
//! quando una strada nuova non è dichiarata da nessuna parte.
//!
//! * **passano da `may_read`, per una persona** — [`Store::read_argument`],
//!   [`Store::visible_arguments`], [`Store::search`], [`Store::claims_for`],
//!   [`Store::observations_for`], [`Store::gradings_for`],
//!   [`Store::student_gradings`], [`Store::observations_in_session`],
//!   [`Store::exercise`], [`Store::instances_of`], [`Store::generations_for`] e
//!   [`Store::rubric_version`];
//! * **non hanno una persona, e sono eccezioni dichiarate** —
//!   [`Store::cohort_signals`] è l'**aggregato anonimo** oltre soglia di D9, e
//!   non contiene persone: il dato individuale da cui viene si legge solo da
//!   [`Store::observations_for`], che è soggetta al predicato;
//! * **non sono letture** — le scritture ([`Store::append_observation`],
//!   [`Store::append_grading`], [`Store::append_claim`], `upsert_*`, `put_*`,
//!   `record_*`, `open_session`, `close_session`, `index_chunk`, …) restano senza
//!   predicato **per dichiarazione**: questo crate non autentica e non autorizza
//!   in scrittura, e mettere un `&PersonId` in una firma di scrittura che
//!   non usa sarebbe una coperta. Le letture sono il modello di sicurezza; le
//!   scritture sono una porta di servizio, e va detto.
//!
//! Quattro eccezioni hanno una regola che **non** è `may_read` e sono dichiarate
//! perché sono state proprio quelle che il modulo non ripeteva:
//!
//! * **il lato generatore di D8** ([`Store::exercise`], [`Store::instances_of`])
//!   chiede **`teaches` sul corso**: il checker e `Instance::expected` sono la
//!   risposta, e D8 dice che l'integrità è per costruzione perché la risposta non
//!   è nel materiale che lo studente vede. «Vedere l'argomento» non basta: uno
//!   studente iscritto vede un argomento in corso.
//! * **il registro dello studente** ([`Store::gradings_for`],
//!   [`Store::student_gradings`]) ha due righe e non una: lo studente e chi insegna
//!   leggono tutto; chi ha emesso un giudizio legge **il proprio**. Vedi il doc
//!   di `registers`, che spiega perché la differenza è una relazione e non un
//!   dettaglio di implementazione.
//! * **il registro delle dimostrazioni** ([`Store::observations_for`]) ha una
//!   regola in più, e non è una variante: lo studente che guarda il proprio
//!   registro legge dalla vista `unaided_observations`, chi insegna dalla
//!   tabella. La vista è definita in `V6__unaided.sql` e la sua definizione
//!   contiene `WHERE unaided = 1`: qui dentro c'è il nome della relazione, non
//!   la frase. «Lo studente vede solo le osservazioni non assistite, mai la coda
//!   di practice» è una riga che, se sta in una rotta, è un filtro che marcisce;
//!   se sta in una vista, è una parte del file che il database porta con sé.
//! * **l'esportazione a colonne fisse di D12** ([`Store::export_fixed_columns`])
//!   prende una persona e chiede **`teaches` sul corso**: l'export è il
//!   materiale del docente per intero, e «vedere l'argomento» non basta, perché
//!   uno studente iscritto vede un argomento in corso e non deve poterne
//!   scaricare la colonna. Ha una persona, quindi non è fra le eccezioni qui
//!   sopra: sta in questo blocco per la regola che chiede, non per la firma.
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
pub mod padronanza;
pub mod calendario;
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
