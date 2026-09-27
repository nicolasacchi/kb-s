//! Le rotte, e il modo in cui sono montate.
//!
//! Ogni modulo è una superficie di prodotto e ha il proprio file, perché il
//! prossimo agente deve poter aprirne uno e capire in tre righe che cosa può
//! aspettarsi. La tabella completa — metodi, percorsi, status — è nel doc del
//! crate ([`crate`]), e qui non si ripete: una tabella in due posti è una
//! tabella che mente in uno dei due.

pub mod arguments;
pub mod artifact;
pub mod cohort;
pub mod events;
pub mod export;
pub mod calendario;
pub mod padronanza;
pub mod queue;
pub mod registers;
pub mod search;
pub mod vendor_route;

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::bus::Bus;
use crate::config::ServerConfig;
use crate::db::Db;
use crate::vendor::Vendor;

/// Lo stato condiviso da tutti gli handler.
///
/// Un `Arc` di tutto, perché i pezzi hanno la stessa vita: cambiano insieme
/// all'avvio del server e nessuno ha uno stato proprio.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Il database.
    pub db: Db,
    /// Il bus degli eventi, partizionato per corso.
    pub bus: Bus,
    /// Il runtime vendorizzato, letto una volta.
    pub vendor: Arc<Vendor>,
    /// Dove stanno i file, e quale è il suffisso delle origini artifact.
    pub config: Arc<ServerConfig>,
}

impl AppState {
    /// Lo stato, costruito una volta.
    pub fn new(db: Db, config: ServerConfig) -> AppState {
        let vendor = Arc::new(Vendor::load(&config.three_runtime()));
        AppState {
            db,
            bus: Bus::new(),
            vendor,
            config: Arc::new(config),
        }
    }
}

/// Il router dell'interfaccia e dell'API.
///
/// Le rotte `/api/` sono registrate **prima** del fallback, quindi il fallback
/// — che è la rotta degli artifact, scelta dall'header `Host` — le raggiunge
/// solo per ciò che non è API. È l'ordine che rende possibile servire pagine su
/// origini diverse senza che nessun percorso dell'API possa essere intercettato
/// dall'artifact hosting: le due cose non si somigliano, e un percorso che
/// finisse nel fallback sarebbe un buco.
///
/// `/three/` è una rotta **normale**, non una del fallback, e serve su
/// qualunque origine: è la rotta su cui l'artifact — che sta su un'altra
/// origine — carica il runtime vendorizzato (D15).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", axum::routing::get(health))
        .route(
            "/api/v1/courses/{course}/arguments",
            axum::routing::get(arguments::list),
        )
        .route("/api/v1/courses/{course}/queue", axum::routing::get(queue::queue))
        .route(
            "/api/v1/courses/{course}/queue/{id}/ratify",
            axum::routing::post(queue::ratify),
        )
        .route(
            "/api/v1/courses/{course}/queue/{id}/publish",
            axum::routing::post(queue::publish),
        )
        .route(
            "/api/v1/courses/{course}/queue/{id}/withdraw",
            axum::routing::post(queue::withdraw),
        )
        .route(
            "/api/v1/courses/{course}/events",
            axum::routing::get(events::stream),
        )
        .route(
            "/api/v1/courses/{course}/gradings",
            axum::routing::get(registers::gradings),
        )
        .route(
            "/api/v1/courses/{course}/export",
            axum::routing::get(export::export),
        )
        .route("/api/v1/arguments/{id}", axum::routing::get(arguments::read))
        .route(
            "/api/v1/arguments/{id}/claims",
            axum::routing::get(arguments::claims),
        )
        .route(
            "/api/v1/arguments/{id}/observations",
            axum::routing::get(registers::observations),
        )
        .route(
            "/api/v1/arguments/{id}/cohort",
            axum::routing::get(cohort::signals),
        )
        .route(
            "/api/v1/courses/{course}/calendario",
            axum::routing::get(calendario::calendario),
        )
        .route(
            "/api/v1/courses/{course}/padronanza",
            axum::routing::get(padronanza::meter),
        )
        .route(
            "/api/v1/courses/{course}/padronanza/quota",
            axum::routing::get(padronanza::share),
        )
        .route("/api/v1/search", axum::routing::get(search::search))
        .route(kbs_doc::THREE_RUNTIME, axum::routing::get(vendor_route::three))
        .route(
            "/three/three.module.min.js.gz",
            axum::routing::get(vendor_route::three_gz),
        )
        .fallback(artifact::dispatch)
        .layer(axum::middleware::from_fn(no_store))
        .with_state(state)
}

/// Risponde `no-store` a tutto ciò che sta sotto `/api/`.
///
/// Non è una buona pratica generica: è la risposta a una domanda precisa. Una
/// cache condivisa davanti a questo server — un proxy di scuola, un CDN —
/// conserverebbe `{"arguments":[…]}` di uno studente e lo servirebbe a un
/// altro, e una cache non applica predicati. Il costo è che ogni risposta
/// dell'API viaggia senza cache: sono dati di una persona, e il vantaggio
/// della cache su dati di una persona è nullo.
///
/// `/three/` è **fuori** da questo filtro, e deve esserlo: il runtime
/// vendorizzato sono 691 KB di contenuto immutabile, e mettergli `no-store`
/// costerebbe 691 KB a ogni pagina di ogni classe. Sull'origine di un artifact
/// il filtro non passa — quella rotta è nel fallback — quindi l'artifact si
/// dichiara da solo `private, no-store`: la sua visibilità dipende dalla
/// dichiarazione di identità che viaggia nella query string, e quella non va in
/// nessuna cache.
async fn no_store(req: Request, next: Next) -> Response {
    let sotto_api = req.uri().path().starts_with("/api/");
    let mut risposta = next.run(req).await;
    if sotto_api {
        risposta.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    risposta
}

/// Che cosa risponde `/api/v1/health`.
///
/// Non dice chi ha chiesto e non chiede relazioni: un endpoint di salute che
/// richiede un'identità è un endpoint che non può essere usato da un
/// monitoraggio, e un monitoraggio che non può girare è un monitoraggio che
/// non gira.
#[derive(Debug, serde::Serialize)]
pub struct Health {
    /// Sempre `ok` se la rotta ha risposto: un `500` lo dice già da solo.
    pub stato: &'static str,
    /// Il binario che ha risposto, per un bug report.
    pub creatore: &'static str,
}

async fn health() -> axum::Json<Health> {
    axum::Json(Health {
        stato: "ok",
        creatore: concat!("kbs-server/", env!("CARGO_PKG_VERSION")),
    })
}
