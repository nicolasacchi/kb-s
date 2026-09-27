//! La lettura: argomenti, elenchi, claim.
//!
//! Tre rotte e una sola cosa in comune: **nessuna di them riapplica il
//! predicato**. Chiamano i metodi gated di `kbs-store` e, quando ricevono
//! `NotReadable`, la risposta che ne esce è la risposta di «assente» — la stessa
//! che danno a un id che non è mai esistito. Vedi [`crate::error`].
//!
//! Tre rotte e una sola cosa in comune: **nessuna di esse riapplica il
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use kbs_core::{Argument, Claim};

use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// `GET /api/v1/courses/{course}/arguments`
///
/// `200 {"arguments": [...], "course": "course_0001"}` — **anche vuota**.
///
/// L'elenco di un corso di cui la persona non ha relazioni è `200` con un
/// elenco vuoto, non `404`. Un `404` qui distinguerebbe «il corso non esiste»
/// da «non ci sei dentro», ed è il canale che D5 vieta: la risposta è la stessa
/// che si ottiene da un corso inesistente, che è il punto.
///
/// Lo stato (`?state=`) restringe un insieme che il predicato ha già
/// autorizzato, quindi non può riportare indietro le bozze del docente a uno
/// studente che le chiede esplicitamente.
pub async fn list(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
    axum::extract::Query(filtro): axum::extract::Query<ListQuery>,
) -> Result<Json<ArgumentsResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person();
    let argomenti = state.db.read_api(|store| {
        capability::require(store, persona, &course, None)?;
        Ok(store.visible_arguments(persona, &course, filtro.state)?)
    })?;
    Ok(Json(ArgumentsResponse {
        course: course.clone(),
        arguments: argomenti,
    }))
}

/// Il filtro opzionale della lista.
///
/// `state` è un enum di `kbs-core` deserializzato dal suo nome kebab-case
/// (`bozza`, `del-docente`, `in-corso`, `archiviato`): non è una stringa libera,
/// perché una stringa libera che non combacia con gli stati del dominio
/// produrrebbe un elenco vuoto che sembrerebbe una risposta vera.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListQuery {
    /// Limita a uno stato di pubblicazione.
    pub state: Option<kbs_core::PublicationState>,
}

#[derive(Debug, Serialize)]
pub struct ArgumentsResponse {
    /// Il corso di cui è l'elenco.
    pub course: kbs_core::CourseId,
    /// Ciò che la persona dichiarata può vedere. Può essere vuoto.
    pub arguments: Vec<Argument>,
}

/// `GET /api/v1/arguments/{id}`
///
/// `200 {"argument": {…}}` · `404`.
///
/// Il `404` è la stessa risposta di un id mai esistito, e la stessa di un id che
/// la persona non può leggere. L'id di un argomento è `arg_<hash del percorso>`
/// ed è enumerabile da chiunque abbia il corpus, quindi ogni differenza qui
/// sarebbe un canale per imparare che cosa c'è in un corso.
pub async fn read(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(id): Path<String>,
) -> Result<Json<ArgumentResponse>, ApiError> {
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let argomento = state
        .db
        .read_api(|store| capability::read(store, identita.person(), &id))?;
    Ok(Json(ArgumentResponse { argument: argomento }))
}

#[derive(Debug, Serialize)]
pub struct ArgumentResponse {
    /// L'argomento, con la sua provenienza e la sua ratifica.
    pub argument: Argument,
}

/// `GET /api/v1/arguments/{id}/claims`
///
/// `200 {"claims": [...]}` · `404`.
///
/// Una claim non è altro materiale con regole sue: è un'affermazione **su**
/// quell'unità di insegnamento, e mostrarne le citabilità senza l'unità sarebbe
/// incoerente. Quindi la visibilità è quella dell'argomento, e
/// `kbs-store::claims_for` la applica chiamando `read_argument` per primo.
///
/// Le claim soppresse e quelle ritratte ci sono, con il loro stato: un registro
/// che cancella i propri errori non è un registro (D6). Cosa farne è compito del
/// lettore, e il lettore deve poter vedere che c'è qualcosa da guardare.
pub async fn claims(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(id): Path<String>,
) -> Result<Json<ClaimsResponse>, ApiError> {
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let claims = state
        .db
        .read(|store| Ok(store.claims_for(identita.person(), &id)?))?;
    Ok(Json(ClaimsResponse { claims }))
}

#[derive(Debug, Serialize)]
pub struct ClaimsResponse {
    /// Le affermazioni sull'argomento, nello stato in cui sono.
    pub claims: Vec<Claim>,
}
