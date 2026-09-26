//! Gli errori di `kbs-exercise`.
//!
//! Due famiglie, e la distinzione è deliberata:
//!
//! * [`ExerciseError`] è un **errore di costruzione**: il checker, o l'esercizio,
//!   è rotto. Non si corregge con un voto, si corregge e si ricostruisce.
//! * [`Leak`] (in `leak`) è un **rifiuto di pubblicazione**: il testo del
//!   contratto rivela la risposta.
//!
//! La regola che li tiene separati è scritta in `check::grade`: **una risposta
//! non è mai un errore**. Qualunque stringa dello studente riceve un verdetto;
//! un `Err` significa che è l'esercizio a essere sbagliato. Il motivo è
//! pedagogico prima che tecnico — un sistema che distingue «sbagliato» da
//! «non valutabile» costringe chi registra i voti a decidere che cosa fare del
//! secondo caso, e la decisione giusta (chiedere, non mettere zero) non può
//! essere nascosta dietro un'eccezione.

use kbs_core::GraderKind;
use thiserror::Error;

/// Un esercizio che non si può neanche costruire.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ExerciseError {
    #[error("il checker MultipleChoice dichiara l'esatto all'indice {index} ma le opzioni sono {len}: l'indice dell'esatto non esiste, e il confronto sarebbe un voto inventato")]
    CorrectIndexOutOfRange { index: u8, len: usize },

    #[error("il checker MultipleChoice non ha opzioni: una scelta fra nulla non è un esercizio")]
    NoOptions,

    #[error("il checker MultipleChoice ha un'opzione vuota: un'opzione vuota è indistinguibile dal nessuna risposta")]
    EmptyOption,

    #[error("il checker MultipleChoice ripete l'opzione {option:?} due volte: con due opzioni identiche la domanda ha due risposte uguali e una sola risposta giusta")]
    DuplicateOption { option: String },

    #[error("il checker Set dichiara l'elemento {element:?} due volte: un insieme non ha molteplicità, e un elemento ripetuto significa che il confronto accetterebbe una risposta sbagliata")]
    DuplicateSetElement { element: String },

    #[error(
        "il checker Set ha un elemento vuoto: un insieme di stringhe vuote non distingue nulla"
    )]
    EmptySetElement,

    #[error("il checker Set non ha elementi: un insieme vuoto è ogni risposta")]
    EmptySet,

    #[error("la tolleranza {tolerance} non è confrontabile (negativa, NaN o infinita): un checker numerico che non può fallire non è un checker")]
    BadTolerance { tolerance: f64 },

    #[error("il checker Equivalence ha la forma normale vuota: senza dichiarazione il confronto non è un confronto, è un'uguaglianza di stringhe con un nome che fa finta di essere altro")]
    EmptyNormalForm,

    #[error("l'istanza non è legata a questo checker: {reason}")]
    UnboundInstance { reason: String },

    #[error("parametri non generabili: {reason}")]
    DegenerateInstance { reason: String },

    #[error("il range [{lo}, {hi}] non può fornire {want} valori distinti")]
    DegenerateRange { lo: u64, hi: u64, want: usize },

    #[error("{attempted:?} viene dopo {missing:?} nella catena di grading, e {missing:?} non è ancora stato provato: l'ordine è deterministico, poi pari, poi umano")]
    GradingOrder {
        attempted: GraderKind,
        missing: GraderKind,
    },

    #[error("la catena di grading è chiusa da {closed_by:?}: un giudizio conclusivo non si riapre, altrimenti la catena non è una catena")]
    GradingChainClosed { closed_by: GraderKind },

    #[error(
        "{kind:?} non è un anello della catena di grading pubblicata (deterministico, pari, umano)"
    )]
    KindNotInChain { kind: GraderKind },
}
