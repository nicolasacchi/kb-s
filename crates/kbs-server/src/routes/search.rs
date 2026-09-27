//! La ricerca lessicale, che è anche il primo posto in cui l'indice e la
//! visibilità si incontrano.
//!
//! # Il predicato è applicato **dopo** il `bm25` e **prima** del limite
//!
//! È il punto in cui la ricerca incontra la visibilità, ed è l'unico: dentro
//! `kbs-store::search` (`kbs-store/src/search.rs:290-306`). Un indice condiviso
//! che restituisce una bozza a uno studente non è un problema di ranking, è una
//! porta. Nessuna delle tre cose che potrebbero sembrare una scorciatoia è
//! fatta qui: non si chiedono meno righe «per poi filtrare», non si filtra per
//! corso, non si sfiora `kbs-store::conn`.
//!
//! Il limite è applicato **dopo** il predicato per una ragione che sembra
//! controintuitiva: restringere prima farebbe restituire i primi N risultati
//! *leggibili* quando i primi N risultati *cercati* sono di un altro corso,
//! facendo sembrare vuota una ricerca che non lo è. Qui si scansiona con
//! margine, si filtra, e si restituisce ciò che è passato.

use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use kbs_core::{ArgumentId, CourseId};

use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::routes::AppState;

/// Quanti risultati, se il chiamante non lo dice.
pub const DEFAULT_LIMIT: usize = 20;

/// Il massimo che un chiamante può chiedere.
///
/// È un soffitto, non un suggerimento: senza, `limit=1000000` fa dire al
/// predicato «sì» un milione di volte, e il predicato è una query per volta.
pub const MAX_LIMIT: usize = 200;

/// `GET /api/v1/search?q=…&limit=…`
///
/// `200 {"hits": [...]}` · `400` se `q` non contiene termini cercabili o
/// `limit` è fuori scala.
///
/// Un risultato è `{argument, course, rank}`. `rank` è il `bm25` di FTS5 e
/// **più negativo è migliore**: si chiama `rank` e non `score` per mettere il
/// segno nel nome, così nessuno lo riordina al contrario. La risposta è
/// nell'ordine dell'indice, non riordinata qui: riordinare in Rust significherebbe
/// avere due ordinamenti, e i due divergono alla prima release.
pub async fn search(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(ApiError::BadRequest {
            motivo: format!("`limit` deve essere fra 1 e {MAX_LIMIT}, non {limit}"),
        });
    }
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Err(ApiError::BadRequest {
            motivo: "`q` non contiene nessun termine cercabile".into(),
        });
    }
    let persona = identita.person().clone();
    let trovati = state.db.read_api(|store| Ok(store.search(&persona, &q, limit)?))?;
    // `SearchHit` non è `Serialize` — è un tipo del livello dati, non un tipo
    // del protocollo. La proiezione è qui, dichiarata, e non un
    // `serde(remote)`: i nomi che escono da questa rotta sono i nomi che il
    // frontend dell'agente dopo leggerà, e devono stare in questo file.
    let hits = trovati
        .into_iter()
        .map(|h| Hit {
            argument: h.argument,
            course: h.course,
            rank: h.rank,
        })
        .collect();
    Ok(Json(SearchResponse { hits }))
}

/// I parametri della ricerca.
///
/// `q` è opzionale **nel tipo** e non è opzionale **in pratica**: se manca
/// arriva come stringa vuota e la rotta la rifiuta, perché un `400` qui è
/// informazione utile e un `Option<String>` che diventa `None` in silenzio
/// no.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SearchQuery {
    /// Il testo cercato.
    pub q: Option<String>,
    /// Quanti risultati, fino a [`MAX_LIMIT`].
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// I risultati che la persona dichiarata può vedere, nell'ordine
    /// dell'indice.
    pub hits: Vec<Hit>,
}

#[derive(Debug, Serialize)]
pub struct Hit {
    /// L'argomento trovato.
    pub argument: ArgumentId,
    /// Il corso in cui sta.
    pub course: CourseId,
    /// Il `bm25` di FTS5. Più negativo è migliore.
    pub rank: f64,
}
