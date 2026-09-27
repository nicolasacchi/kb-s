//! I registri di una persona: che cosa ha dimostrato, e chi ha deciso che cosa.
//!
//! # Chi può leggere un registro, e perché qui la regola è diversa
//!
//! Il predicato di D5 riguarda **il materiale**: un argomento in bozza non si
//! vede. Un registro è un'altra cosa — è la prova di che cosa una persona ha
//! fatto, e chi ne ha diritto non è «chi vede l'argomento» ma «chi riguarda
//! quella persona»: lo studente stesso, chi ha emesso almeno uno dei giudizi, e
//! chi insegna il corso. Sono tre, e sono dichiarati in tre rigi dentro
//! `kbs-store` (`registers.rs:610-632`) perché **non** sono la stessa cosa di
//! `may_read` e accodarli a `may_read` con la scusa che «è la stessa cosa»
//! farebbe di `may_read` una funzione che risponde a due domande diverse.
//!
//! Qui non si decide nulla: si chiama il metodo gated e si traduce
//! l'errore. Il `404` di un registro che non si può leggere è lo stesso `404` di
//! un argomento che non si può leggere, per la stessa ragione: nessuno dei due
//! deve dire che l'oggetto esiste.
//!
//! # Le contestazioni viaggiano dentro la riga
//!
//! Non c'è una rotta `/contestazioni`. La contestazione è un fatto **nuovo**
//! registrato dentro il giudizio che contesta, e leggerla in una query
//! separata significa che chi legge un ricorso può vedere il voto senza vedere
//! il ricorso, o viceversa. Due query che possono disaccordarsi sono due fonti
//! di verità, e in un registro una fonte di verità che si disaccorda da sé
//! stessa è peggio di nessuna.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Serialize;

use kbs_core::{CourseId, Grading, Observation, PersonId};

use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// Chi si sta guardando.
///
/// `person` è opzionale e vale l'identità dichiarata. Non è una scorciatoia:
/// uno studente che guarda il proprio registro non deve scrivere il proprio id
/// in ogni URL, e un docente che guarda il registro di uno studente deve
/// scriverlo. Un id malformato dà «assente», non «richiesta non valida», per la
/// stessa ragione di [`crate::ids`]: la forma degli id è pubblica e non
/// distingue nulla.
#[derive(Debug, Default, serde::Deserialize)]
pub struct WhoQuery {
    /// La persona di cui si vuole il registro.
    pub person: Option<String>,
}

impl WhoQuery {
    /// La persona di cui si parla: quella dichiarata, o quella dichiarata.
    pub fn persona(&self, identita: &SharedIdentity) -> Result<PersonId, ApiError> {
        match &self.person {
            None => Ok(identita.person().clone()),
            Some(raw) => ids::person(raw).ok_or(ApiError::Absent),
        }
    }
}

/// `GET /api/v1/arguments/{id}/observations?person=…`
///
/// `200 {"observations": [...]}` · `404`.
///
/// Le osservazioni su un argomento, per la persona indicata. Chi non ha diritto
/// riceve `404` e non un elenco vuoto: un elenco vuoto è una risposta **vera**,
/// e qui non lo sarebbe — significherebbe che lo studente non ha lavorato su
/// quell'argomento, che è un'altra informazione.
pub async fn observations(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(id): Path<String>,
    Query(chi): Query<WhoQuery>,
) -> Result<Json<ObservationsResponse>, ApiError> {
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let studente = chi.persona(&identita)?;
    let persona = identita.person().clone();
    let osservazioni = state.db.read_api(|store| {
        Ok(store.observations_for(&persona, &studente, &id)?)
    })?;
    Ok(Json(ObservationsResponse { observations: osservazioni }))
}

#[derive(Debug, Serialize)]
pub struct ObservationsResponse {
    /// Le dimostrazioni, nell'ordine in cui sono state registrate: `seq` sta
    /// dentro la sessione ed è la quantità che la catena di hash di D6 ordina.
    pub observations: Vec<Observation>,
}

/// `GET /api/v1/courses/{course}/gradings?person=…`
///
/// `200 {"gradings": [...]}` · `404`.
///
/// I giudizi su un corso, tutti gli argomenti, **contestazione inclusa**. Il
/// filtro per argomento non c'è di proposito: chi legge un ricorso deve vedere
/// la storia del giudizio, e la storia è su tutti gli argomenti.
///
/// Ogni voce contiene `contested`, con `by`, `at`, `reason` e `outcome`. Un
/// `outcome` mancante significa che la contestazione è aperta, e la
/// differenza fra «nessuna contestazione» e «contestazione aperta» è il primo
/// numero che un ricorso chiede.
pub async fn gradings(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
    Query(chi): Query<WhoQuery>,
) -> Result<Json<GradingsResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let studente = chi.persona(&identita)?;
    let persona = identita.person().clone();
    let gradings = state.db.read_api(|store| {
        // Il controllo di relazione qui non aggiunge niente a
        // `student_gradings`, che lo fa già: è dichiarato per la stessa ragione
        // per cui lo è in tutte le altre rotte di corso — un corso inesistente e
        // un corso di cui non si fa parte danno la stessa risposta, e senza
        // questo controllo il primo darebbe un `404` e il secondo un elenco
        // vuoto, che sono due risposte diverse a due domande diverse ma
        // insegnerebbero la stessa cosa: che quel corso esiste.
        capability::require(store, &persona, &course, None)?;
        Ok(store.student_gradings(&persona, &studente, &course)?)
    })?;
    Ok(Json(GradingsResponse {
        course,
        gradings,
    }))
}

#[derive(Debug, Serialize)]
pub struct GradingsResponse {
    /// Il corso di cui sono i giudizi.
    pub course: CourseId,
    /// I giudizi, contestazione dentro.
    pub gradings: Vec<Grading>,
}
