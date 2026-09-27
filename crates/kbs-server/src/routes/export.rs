//! L'uscita è `rm -rf` (D12).
//!
//! # Perché la porta è `teaches` e non «un permesso»
//!
//! L'esportazione è una **via d'uscita dal prodotto**: quello che esce qui
//! non è più governato da nessuna delle regole di questo server. Una via
//! d'uscita che uno studente può prendere non è una via d'uscita, è una
//! stampa. Quindi la porta è `kbs_store::Error::NotACourseTeacher`, e questo
//! modulo la traduce in `404` come tutto il resto (vedi [`crate::error`]): un
//! `403` distinguerebbe «il corso esiste e non sei il docente» da «il corso non
//! esiste», ed è l'informazione che la risposta non deve dare.
//!
//! Chi è il docente del corso lo sa già: o è l'iscritto e sa di essere
//! iscritto, o non ha relazione e riceve la stessa risposta di un corso
//! inesistente. Nessuno dei due impara niente.
//!
//! # Il formato è dichiarato e non dedotto
//!
//! Le colonne sono [`kbs_store::FIXED_COLUMNS`], in quell'ordine, con
//! l'intestazione. «A colonne fisse» significa tre cose: il numero e l'ordine
//! sono dichiarati, l'intestazione c'è, e la riga è ordinata per id così due
//! esecuzioni sullo stesso stato danno lo stesso byte. Un export non
//! riproducibile non è un backup, e D11 chiede riproducibilità anche quando
//! nessuno sta guardando.
//!
//! Le righe sono quelle **citabili** (`in-corso` con ratifica valida sul
//! contenuto corrente), non tutte le righe in uso: mandare in export un
//! argomento che nessuno ha ratificato nella forma in cui è stato scritto
//! significherebbe pubblicare materiale non ratificato, che è la cosa che D4
//! vieta per nome.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::Response;

use kbs_core::{CourseId, Relation};

use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// Il tipo di contenuto dell'esportazione.
///
/// `tab-separated-values` con `charset=utf-8`: un foglio di calcolo apre
/// l'estensione `.tsv` senza istruzioni, e i titoli italiani hanno accenti che
/// senza `charset` diventano mojibake.
pub const CONTENT_TYPE: &str = "text/tab-separated-values; charset=utf-8";

/// `GET /api/v1/courses/{course}/export`
///
/// `200 text/tab-separated-values` con l'intestazione e le righe citabili ·
/// `404` se non `teaches` il corso.
///
/// Il nome del file è il course id, ed è `.tsv` perché è quello che il
/// destinatario aprirà.
pub async fn export(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
) -> Result<Response, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let testo = state.db.read_api(|store| {
        // Il controllo esplicito è la stessa cosa che fa
        // `export_fixed_columns` e nella stessa forma: due controlli che
        // dicono la stessa cosa non è ridondanza, è il motivo per cui una
        // rimozione futura di uno dei due non apre un buco. La traduzione
        // dell'errore, però, la fa solo `kbs-store`.
        capability::require(store, &persona, &course, Some(Relation::Teaches))?;
        Ok(store.export_fixed_columns(&persona, &course)?)
    })?;

    let nome = nome_file(&course);
    Ok(Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{nome}\""),
        )
        // L'export è di una persona e di un momento: non entra in nessuna
        // cache, nemmeno privata. La rotta sotto `/api/` prende già
        // `no-store` dal middleware; qui si dice lo stesso in un header che
        // sopravvive a un proxy che riscrive.
        .header(header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from(testo))
        .map_err(|errore| {
            tracing::error!(%errore, "risposta di esportazione");
            ApiError::Absent
        })?)
}

/// Il nome del file di esportazione.
///
/// `course_0001.tsv` e non uno slug: lo slug è un dato della fonte, che
/// `kbs-store` potrebbe non avere registrato, e un nome di file che dipende da
/// un dato assente diventa un nome di file che cambia. L'id del corso è sempre
/// presente, perché è nella rotta.
fn nome_file(course: &CourseId) -> String {
    format!("{}.tsv", course.as_str())
}
