//! L'unico posto in cui questo crate chiede delle relazioni.
//!
//! # Qui non ci sono ruoli, e non ci saranno
//!
//! Una persona in `kb-s` non è «docente» o «studente»: è una persona che ha
//! delle relazioni con dei corsi e con degli oggetti. Un ruolo è un *insieme*
//! di relazioni, e D5 vieta di memorizzarlo — non perché sia scomodo, ma
//! perché un ruolo memorizzato è un secondo vocabolario che a un certo punto
//! dice una cosa diversa dalle relazioni che lo avevano prodotto, e quando i
//! due discordano nessuno dei due è autorevole.
//!
//! Quindi in questo crate non esiste un tipo ruolo, non esiste una tabella dei
//! ruoli, e [`require`] chiede una [`Relation`] — non un nome. Il predicato di
//! lettura, che è un'altra cosa e sta altrove, è
//! [`kbs_core::may_read`] applicato da [`kbs_store::Store`].
//!
//! # La regola che tiene insieme le rotte
//!
//! Una risposta che dipenderebbe dall'esistenza di un corso non può esistere.
//! Quindi:
//!
//! * una persona **senza relazioni** con un corso riceve la risposta di
//!   «assente» — la stessa che riceve per un corso che non esiste;
//! * una persona **con relazioni** ma senza *quella* relazione riceve anch'essa
//!   «assente», e la coda di ratifica, l'esportazione e la coorte danno tutte
//!   la stessa risposta.
//!
//! Il risultato è che `403` non esiste in questo crate, e non per pigrizia:
//! `403` su un corso inesistente e `403` su un corso altrimenti non sarebbero
//! la stessa risposta, e la differenza direbbe a chi chiede che quel corso
//! c'è. La risposta «non lo vedi» è più povera di informazioni e più onesta,
//! ed è la stessa che arriva per un id che non è mai esistito.

use kbs_core::{Argument, ArgumentId, CourseId, PersonId, Relation};
use kbs_store::Store;

use crate::error::ApiError;

/// Le relazioni di corso che `person` ha **adesso**.
///
/// Nessun predicato: questa funzione risponde a «quali relazioni ha», e la
/// domanda «può vedere questa cosa?» ha un'altra risposta, che sta in
/// [`kbs_store::Store::read_argument`].
fn relations(
    store: &Store,
    person: &PersonId,
    course: &CourseId,
) -> Result<Vec<Relation>, ApiError> {
    Ok(store.relations_of(person, course)?)
}

/// Le relazioni, oppure «assente» se non ce ne sono.
///
/// È la soglia che mette un corso inesistente e un corso di cui non si fa
/// parte nello stesso cassone, ed è la stessa soglia per tutte le rotte di
/// corso.
fn course_relations(
    store: &Store,
    person: &PersonId,
    course: &CourseId,
) -> Result<Vec<Relation>, ApiError> {
    let relazioni = relations(store, person, course)?;
    if relazioni.is_empty() {
        return Err(ApiError::Absent);
    }
    Ok(relazioni)
}

/// Esige **una** relazione, e restituisce tutte quelle che ci sono.
///
/// Il nome della relazione è un enum di `kbs-core`, non una stringa: non
/// esiste un modo di scrivere qui «insegnante», «admin» o «docente».
///
/// `None` come relazione richiesta significa «basta una relazione qualunque»,
/// che è il caso delle rotte che non hanno bisogno di distinguere *quale*
/// relazione sia: gli argomenti di un corso li vede chiunque ne abbia una, e
/// il predicato decide argomento per argomento.
pub fn require(
    store: &Store,
    person: &PersonId,
    course: &CourseId,
    which: Option<Relation>,
) -> Result<Vec<Relation>, ApiError> {
    let relazioni = course_relations(store, person, course)?;
    if let Some(cercata) = which {
        if !relazioni.contains(&cercata) {
            return Err(ApiError::Absent);
        }
    }
    Ok(relazioni)
}

/// L'argomento, se `person` può leggerlo.
///
/// Non riapplica [`kbs_core::may_read`]: chiama
/// [`kbs_store::Store::read_argument`], che è il posto in cui il predicato
/// viene applicato. Una seconda copia del predicato in questo crate
/// divergerebbe da quella, e la divergenza sarebbe silenziosa.
pub fn read(store: &Store, person: &PersonId, argument: &ArgumentId) -> Result<Argument, ApiError> {
    Ok(store.read_argument(person, argument)?)
}

/// Il corso di un argomento, per chi lo può leggere.
///
/// Serve alle rotte che hanno l'argomento nell'URL ma non il corso, e che
/// hanno bisogno del corso **per chiedere le relazioni**: la coorte è del
/// docente del corso, e senza sapere quale sia il corso non c'è relazione da
/// chiedere. La lettura passa comunque da [`read`], quindi da
/// `read_argument` e quindi dal predicato: un argomento che non si vede dà
/// «assente» qui come in ogni altra parte.
pub fn course_of(
    store: &Store,
    person: &PersonId,
    argument: &ArgumentId,
) -> Result<CourseId, ApiError> {
    Ok(read(store, person, argument)?.course)
}
