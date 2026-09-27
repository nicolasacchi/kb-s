//! Le due rotte del runtime vendorizzato (D15).
//!
//! # Una rotta, due varianti
//!
//! Il percorso è quello che `kbs-doc` dichiara dentro il perimetro
//! ([`kbs_doc::THREE_RUNTIME`]) e non una stringa scritta qui: se il validatore
//! e il server dicessero percorsi diversi, un artifact potrebbe passare la
//! validazione e non trovare il runtime, e il sintomo sarebbe «la 3D non
//! parte» in una classe, il giorno in cui qualcuno ha cambiato una costante.
//!
//! La variante gzip ha un percorso proprio solo perché è comodo scaricarla a
//! mano; il browser non la usa mai, perché la negozia con `Accept-Encoding`.
//!
//! # `304` senza corpo
//!
//! `If-None-Match` che coincide con l'`ETag` della variante giusta dà `304` con
//! gli stessi header di cache e **senza corpo**. I byte del runtime sono
//! 691 KB: mandarli quando il client li ha già è la differenza fra una classe
//! che apre un artifact e una classe che aspetta.

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::routes::AppState;
use crate::vendor::{Variant, CONTENT_TYPE};

/// Il `Cache-Control` del runtime.
///
/// Un anno e `immutable`: vedi il doc di [`crate::vendor`] sul perché è lecito
/// dirlo, cioè perché il file che cambia cambia nome nel repository.
fn cache_control(max_age: u32) -> String {
    format!("public, max-age={max_age}, immutable")
}

/// La risposta di una variante, con o senza `304`.
fn risposta(variante: &Variant, if_none_match: Option<&str>, max_age: u32) -> Response {
    let mut costruita = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .header(header::ETAG, variante.etag.clone())
        .header(header::CACHE_CONTROL, cache_control(max_age))
        // Le due varianti hanno lo stesso percorso e corpi diversi: senza
        // `Vary` la cache mette in un posto la risposta giusta per la richiesta
        // sbagliata.
        .header(header::VARY, "Accept-Encoding")
        .body(axum::body::Body::from(variante.bytes.as_ref().clone()));
    if let Ok(r) = costruita.as_mut() {
        if let Some(encoding) = variante.encoding {
            r.headers_mut()
                .insert(header::CONTENT_ENCODING, encoding.parse().unwrap());
        }
    }
    let mut r = costruita.unwrap_or_else(|errore| {
        tracing::error!(%errore, "risposta del runtime");
        ApiError::Absent.into_response()
    });
    if let Some(tag) = if_none_match {
        if tag.split(',').any(|t| t.trim() == variante.etag) {
            *r.status_mut() = StatusCode::NOT_MODIFIED;
            *r.body_mut() = axum::body::Body::empty();
        }
    }
    r
}

/// `GET /three/three.module.min.js`
///
/// `200 application/javascript` con `ETag`, `Cache-Control` lungo e `Vary` ·
/// `304` se l'`ETag` coincide · `500` con `regola: D15` se il runtime non è
/// vendorizzato.
///
/// Serve su **qualsiasi** origine, artifact compresa: l'artifact sta su
/// `<argomento>.artifacts.<suffisso>` e il suo `<script type="module" src=…>`
/// è relativo, quindi arriva qui da quell'origine. Per questo è una rotta e
/// non una risorsa del fallback.
pub async fn three(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let accept = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok());
    let variante = state.vendor.per_encoding(accept)?;
    let max_age = state.config.vendor_max_age_seconds;
    let if_none_match = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok());
    Ok(risposta(variante, if_none_match, max_age))
}

/// `GET /three/three.module.min.js.gz`
///
/// Come [`three`], ma la variante compressa è sempre quella: il percorso dice
/// gzip, e dirlo anche in `Content-Encoding` è la belt-and-braces di un
/// percorso che nessun browser usa. Serve a chi scarica il file a mano per
/// metterlo in un artifact e non sa di poter usare la rotta con
/// `Accept-Encoding`.
pub async fn three_gz(State(state): State<AppState>) -> Result<Response, ApiError> {
    let max_age = state.config.vendor_max_age_seconds;
    let variante = state
        .vendor
        .per_encoding(Some("gzip"))
        .or_else(|_| state.vendor.per_encoding(None))?;
    Ok(risposta(variante, None, max_age))
}
