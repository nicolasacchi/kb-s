//! `kbs-verify` — l'evidenza di manomissione e il replay deterministico.
//!
//! Questo crate non ha opinioni su come si insegna, e non ha un modello. Fa
//! tre cose, e le tre sono le cose che rendono un registro append-only
//! qualcosa di più di un diario.
//!
//! 1. **La catena di hash sulle osservazioni** ([`chain`], D6): foglia
//!    `SHA256(0x00 ‖ canonical_json(riga))`, nodo `SHA256(0x01 ‖ l ‖ r)`, testa
//!    per sessione, e una prova di coerenza per segmento. Il JSON canonico
//!    ([`canonical`]) è scritto qui e testato qui, perché due serializzatori che
//!    disagree renderebbero la catena non falsificabile.
//! 2. **I tre limiti dichiarati** ([`limits`], D6): cosa questo crate **non**
//!    garantisce, e cosa porta con sé ogni verdetto. Non è un paragrafo: è un
//!    array di tre che non si può accorciare senza rompere la compilazione, e
//!    un tipo [`Verified`] che nessuna funzione di questo crate può restituire
//!    nudo.
//! 3. **Il replay deterministico** ([`replay`], D11): la tupla che riproduce
//!    un'istanza e la verifica che la riproduzione torni. Un replay che non
//!    torna è un verdetto con due valori, non un errore.
//!
//! E due oggetti che esistono perché i limiti non restino lettera: il
//! **testimone** ([`witness`]), la copia indipendente che rende visibile la
//! riscrittura e il rollback, e l'**export a colonne fisse** ([`export`], D12),
//! che porta fuori righe, catena, prove, testimone e limiti insieme — e che si
//! rilegge, perché un'uscita che non si può reimportare non è un'uscita.
//!
//! ## I tre limiti, in una riga ciascuno
//!
//! 1. chi riscrive l'intera catena da capo produce una catena coerente,
//!    indistinguibile senza una copia indipendente;
//! 2. un rollback da backup è indistinguibile da una riscrittura, se non c'è
//!    un ancoraggio esterno e monotono;
//! 3. l'hash garantisce integrità, **non verità**: uno span che non sostiene la
//!    claim è una riga impeccabilmente conforme.
//!
//! Il terzo è il più importante e non ha rimedio. Il primo e il secondo hanno
//! un rimedio — il testimone — che è dichiarato come rimedio e non come soluzione:
//! è una copia che qualcuno deve conservare, e se la copia è nel sistema, il
//! limite torna.
//!
//! ## Cosa questo crate non è
//!
//! Non è storage: prende righe come valori e restituisce un verdetto come
//! valore. Non ha bisogno di sapere come sono arrivate né dove stanno. Non è
//! HTTP. Non genera esercizi: il generatore sta in `kbs-exercise` e il confine
//! è il trait [`replay::InstanceGenerator`], dichiarato qui perché sia questa
//! parte a porre la domanda.
//!
//! ## Un tipo definito qui
//!
//! [`SessionId`] non è in `kbs-core`: a `kbs-core` una `Observation` è una
//! riga e non sa di sessioni, e `kbs-core` è il contratto e non si tocca.
//!
//! Sta qui e non in `kbs-store` per una ragione che è un requisito, non una
//! preferenza di collocazione: **questo crate non deve dipendere da nessun
//! database**. Il consumatore più importante dell'export a colonne fisse è
//! l'auditor esterno — colui che riceve il file da una scuola e non si fida
//! del server. Se per ricontrollare una catena dovesse dipendere da
//! `rusqlite`, non lo farebbe mai. Se `kbs-store` ha bisogno del tipo, lo
//! importa da qui: un solo newtype, in un crate che non tira dentro niente.
//!
pub mod canonical;
pub mod chain;
pub mod export;
pub mod hash;
pub mod limits;
pub mod replay;
pub mod session;
pub mod verify;
pub mod witness;
pub use canonical::{
    CanonicalError, canonical_of, canonicalize, canonicalize_str, leaf_of, leaf_of_value,
};
pub use chain::{
    Chain, ConsistencyProof, ProofError, ProofStep, Segment, SegmentPlan, Side, VerifyError,
};
pub use export::{
    CHAIN_COLUMNS, CHAIN_COLUMNS_WITH_ROWS, ChainExport, ExportError, ExportRow, LIMIT_COLUMNS,
    SessionExport,
};
pub use hash::{Hash, HashParseError, leaf, node};
pub use limits::{Limit, LimitId, Limits, Verified};
pub use replay::{
    GeneratorKey, InstanceGenerator, ReplayError, ReplayField, ReplayKey, ReplayOutcome,
    ReplayRecord, replay,
};
pub use session::SessionId;
pub use verify::{ChainVerdict, Coherence, VerifyRequest, verify};
pub use witness::{Confrontation, Witness, WitnessEntry, WitnessError};
