//! Un solo tipo di errore per tutto il crate.
//!
//! I messaggi sono in italiano e dicono **quale regola** è stata rotta, non
//! soltanto che qualcosa è andato storto: chi ha sbagliato è una persona che
//! sta costruendo un corso, e la frase che la ferma è la parte utile del crash.

use kbs_core::{ArgumentId, CourseId, Invariant, PersonId, PublicationState};
use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migrazioni: {0}")]
    Migration(#[from] refinery::Error),

    /// La regola è nel dominio, non qui: `kbs-store` la applica e la restituisce.
    #[error("regola di dominio: {0}")]
    Invariant(#[from] Invariant),

    #[error("{kind} `{id}` non esiste")]
    NotFound { kind: &'static str, id: String },

    #[error("{kind} `{id}` esiste già: gli id sono unici e non si riciclano")]
    DuplicateId { kind: &'static str, id: String },

    /// La risposta alla lettura per una persona. Unica **anche** quando
    /// l'argomento non esiste: distinguerlo sarebbe un canale per imparare
    /// quale materiale c'è, e la forma dell'id (`arg_<hash del percorso>`) è
    /// enumerabile da chiunque abbia il corpus.
    #[error("{person} non può leggere {id}: nessuna relazione lo mette in condizione di vederlo in stato `{state:?}`")]
    NotReadable {
        person: PersonId,
        id: ArgumentId,
        /// Lo stato della riga. Per un argomento inesistente è `Bozza`, cioè lo
        /// stato più restrittivo: il messaggio non deve dire più di quanto il
        /// chiamante abbia diritto di sapere.
        state: PublicationState,
    },

    #[error("il database è all'epoch {db} e questo binario conosce l'epoch {binary}: un binario vecchio non scrive sopra uno schema nuovo")]
    SchemaFromTheFuture { db: i32, binary: i32 },

    #[error("dopo le migrazioni il database è all'epoch {db} ma le migrazioni dicono {binary}: lo schema non è quello che kbs-store crede")]
    SchemaEpochMismatch { db: i32, binary: i32 },

    #[error("sqlite {found} non serve: `kbs-s` richiede almeno {required} (tabelle STRICT, `contentless_delete`)")]
    SqliteTooOld {
        found: String,
        required: &'static str,
    },

    #[error("la sessione di registro `{id}` è chiusa: un registro append-only non si riapre")]
    SessionSealed { id: String },

    #[error("la sessione `{id}` è di registro `{expected}` e non `{got}`: `SeqInSession` sta dentro il suo registro, non dentro quello")]
    SessionRegister {
        id: String,
        expected: String,
        got: String,
    },

    #[error("la sessione di registro `{id}` non esiste")]
    UnknownSession { id: String },

    /// Una riga che non si decodifica. Non è un panico: è un database
    /// corrotto o scritto da mano, e la risposta è dire che cosa non torna.
    #[error("riga `{table}`: il campo `{field}` non si decodifica ({reason})")]
    Corrupt {
        table: &'static str,
        field: &'static str,
        reason: String,
    },

    #[error("campo `{field}` inaccettabile: {reason}")]
    InvalidField {
        field: &'static str,
        reason: String,
    },

    /// Lo stato non si scrive da `upsert_argument`: le transizioni passano da
    /// `publish` e da `archive`, e da nessun'altra parte. Scrivere `in-corso` da
    /// soli renderebbe decorativa la ratifica, che è la cosa che D4 vieta.
    #[error("lo stato di {id} è {from:?} e non si può portare a {to:?} scrivendo: le transizioni passano da `publish` e da `archive` (D4)")]
    StateTransition {
        id: ArgumentId,
        from: Option<PublicationState>,
        to: PublicationState,
    },

    /// Una ratifica installata da `upsert_argument` renderebbe la porta
    /// decorativa: la precondizione di `publish` deve poter essere scritta solo
    /// dal metodo che la registra.
    #[error("l'argomento {id} porta una ratifica e la scrittura non è il posto giusto: una ratifica si registra con `ratify` e si ritira con `withdraw_ratification`")]
    RatificationThroughUpsert { id: ArgumentId },

    #[error("il giudizio `{grading}` è già contestato: una contestazione si apre una volta, poi si risolve")]
    AlreadyContested { grading: String },

    #[error("{person} non insegna il corso {course}: l'esportazione a colonne fisse è del docente")]
    NotACourseTeacher { person: PersonId, course: CourseId },

    #[error("la ricerca non contiene nessun termine cercabile")]
    EmptyQuery,
}

impl Error {
    /// Il nome della regola rotta, per chi registra l'errore senza leggerne il
    /// messaggio. Le invarianti di dominio hanno già il nome del tipo.
    pub fn rule(&self) -> &'static str {
        match self {
            Error::Invariant(_) => "invariant.kbs-core",
            Error::SchemaFromTheFuture { .. } | Error::SchemaEpochMismatch { .. } => {
                "schema.epoch"
            }
            Error::SqliteTooOld { .. } => "schema.sqlite",
            Error::NotReadable { .. } | Error::NotACourseTeacher { .. } => "visibility",
            Error::StateTransition { .. } | Error::RatificationThroughUpsert { .. } => {
                "publication.gate"
            }
            Error::SessionSealed { .. } => "session.sealed",
            Error::SessionRegister { .. } => "session.register",
            Error::AlreadyContested { .. } => "grading.contestation",
            Error::EmptyQuery => "search.empty",
            Error::NotFound { .. } => "lookup.missing",
            Error::DuplicateId { .. } => "lookup.duplicate",
            Error::Corrupt { .. } => "storage.corrupt",
            Error::InvalidField { .. } => "validation",
            Error::Sqlite(_) | Error::Migration(_) | Error::UnknownSession { .. } => "storage",
        }
    }
}
