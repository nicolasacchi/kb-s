//! Il fallback: due server sulla stessa porta, distinti dall'header `Host`.
//!
//! # Perché è un fallback e non due router
//!
//! In `kb` la rotta degli artifact è una rotta normale con una guardia sul
//! `Host`, e tutto il resto del server vive sotto prefissi. Qui la distinzione
//! è più netta: **quando l'host ha la forma `<argomento><suffisso>`, questo
//! processo non serve l'interfaccia**, serve un artifact e nient'altro. Non c'è
//! un indice, non c'è una home, non c'è un'API: su quell'origine l'unica cosa
//! che esiste è l'artifact di quell'id e i suoi figli. È la forma più stretta
//! di hosting che si possa fare, ed è la forma giusta quando il contenuto è
//! HTML non fidato.
//!
//! # La cosa che rende sicuro `allow-same-origin`
//!
//! L'isolamento viene dall'**origine per artifact**, non dal flag `sandbox`.
//! Vedi il doc di [`crate::sandbox`], che è il posto in cui è scritto per
//! esteso e che va letto prima di toccare qualcosa qui.
//!
//! # Il predicato passa anche da qui
//!
//! Un artifact è un argomento, e servire il suo file **senza** passare da
//! `read_argument` annullerebbe D5 con una riga di routing: la bozza del
//! docente diventerebbe pubblica per chiunque conosca l'host. Quindi l'host
//! viene risolto in un `ArgumentId`, l'`ArgumentId` viene letto **come la
//! persona che chiede**, e il file si risolve dal `rel_path` che sta nel
//! database — non dall'URL. Un id non è un percorso, quindi l'URL da solo non
//! può traversare niente.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::ids;
use crate::routes::AppState;
use crate::sandbox::{self, ArtifactResolution};

/// Il fallback del router.
///
/// La decisione è una sola e sta in cima: **l'host è un host artifact?**
///
/// * sì → [`artifact`] e nient'altro;
/// * no → l'interfaccia dalla [`crate::config::ServerConfig::web_dir`], e se
///   l'interfaccia non è installata un `404` che lo dice.
///
/// Il nome del metodo è `dispatch` perché è esattamente quello che fa: due
/// server, una porta, e l'`Host` che dice quale.
pub async fn dispatch(State(state): State<AppState>, req: Request) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if sandbox::parse_artifact_id(&host, &state.config.artifact_host_suffix).is_some() {
        return artifact(state, req).await;
    }
    let uri = req.uri().clone();
    web(state, uri).await
}

/// Un artifact, letto dal corpus e servito sulla sua origine.
///
/// L'ordine è il predicato e poi il filesystem, e non è un ordine
/// qualsiasi: risolvere il file prima significherebbe che l'esistenza del file
/// è la prima cosa che si scopre, e l'esistenza del file è metà dell'esistenza
/// dell'argomento.
async fn artifact(state: AppState, req: Request) -> Response {
    let (parts, _) = req.into_parts();
    let AppState {
        db,
        config,
        ..
    } = state;

    let host = parts
        .headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let persona = match identita_da(&parts.headers, &parts.uri) {
        Ok(p) => p,
        Err(errore) => return errore.into_response(),
    };
    let etichetta = match sandbox::parse_artifact_id(host, &config.artifact_host_suffix) {
        Some(e) => e,
        // Il fallback è stato raggiunto con un host che sembrava artifact: può
        // solo voler dire che l'host è sparito fra la guardia e qui, e la
        // risposta è «assente» come in ogni altro caso.
        None => return ApiError::Absent.into_response(),
    };
    let id = match ids::argument(etichetta) {
        Some(id) => id,
        None => return ApiError::Absent.into_response(),
    };
    let resto = percorso_figlio(&parts.uri.path());

    let argomento = match db.read_api(|store| crate::capability::read(store, &persona, &id)) {
        Ok(a) => a,
        Err(errore) => return errore.into_response(),
    };
    let Some(rel_path) = argomento.rel_path.as_deref() else {
        // Un argomento senza file non ha un artifact. Non è un errore del
        // server: è un argomento creato via API, e la risposta è «assente».
        return ApiError::Absent.into_response();
    };
    let radice = match config.canonical_corpus_root() {
        Ok(r) => r,
        Err(errore) => {
            tracing::error!(errore = %errore, "radice del corpus");
            return ApiError::Absent.into_response();
        }
    };
    let risolto = match sandbox::resolve_file(&radice, rel_path, &resto) {
        Some(r) => r,
        None => return ApiError::Absent.into_response(),
    };
    let file = match std::fs::read(risolto_path(&risolto)) {
        Ok(b) => b,
        Err(errore) => {
            tracing::warn!(%errore, "artifact non leggibile");
            return ApiError::Absent.into_response();
        }
    };
    if file.len() > MAX_ARTIFACT {
        // Un file enorme è quasi sempre un file sbagliato, e servirlo intero
        // è denial of service su una rotta che non ha controllo di accesso
        // proprio. Il limite è dichiarato e non è negoziabile dal client.
        return ApiError::Absent.into_response();
    }

    let mut risposta = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            sandbox::content_type(&risolto_path(&risolto)),
        )
        .body(Body::from(file))
        .unwrap_or_else(|errore| {
            tracing::error!(%errore, "risposta di artifact");
            ApiError::Absent.into_response()
        });
    if let Some(csp) = sandbox::frame_ancestors(config.parent_origin.as_deref()) {
        risposta
            .headers_mut()
            .insert("content-security-policy", csp);
    }
    risposta.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    risposta.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    // L'artifact è HTML non fidato: senza questa intestazione il browser
    // potrebbe eseguire ciò che contiene nel contesto di questa origine. Non
    // è il meccanismo che lo protegge (quello è l'origine, vedi
    // `crate::sandbox`), ma è la difesa che chiude la porta mentre
    // l'interfaccia non c'è ancora.
    risposta.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    risposta
}

/// Quanto è grande un artifact, prima di non essere un artifact.
pub const MAX_ARTIFACT: usize = 32 * 1024 * 1024;

/// Il percorso del file risolto.
fn risolto_path(risolto: &ArtifactResolution) -> std::path::PathBuf {
    match risolto {
        ArtifactResolution::Ingresso(p) | ArtifactResolution::Figlio(p) => p.clone(),
    }
}

/// Il resto del percorso, senza la prima barra.
///
/// Il percorso della richiesta è già percent-decode da `hyper`, e
/// [`sandbox::resolve_file`] rifiuta comunque `..`, `.` e i segmenti vuoti: un
/// `%2e%2e` che arriva come `..` viene bloccato lì, non qui.
fn percorso_figlio(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

/// L'identità dichiarata, dall'header o dalla query string.
///
/// La rotta dell'artifact è una di quelle in cui il browser **non può**
/// allegare un header — è la navigazione di un `<iframe>` — quindi qui la query
/// string è il modo normale, non un ripiego scomodo. Vedi [`crate::identity`]
/// per perché una dichiarazione in una query string non è un incidente in un
/// sistema che non ha autenticazione.
fn identita_da(headers: &HeaderMap, uri: &Uri) -> Result<kbs_core::PersonId, ApiError> {
    if let Some(raw) = headers.get(crate::identity::IDENTITY_HEADER) {
        let raw = raw.to_str().map_err(|_| ApiError::IdentityMalformed)?;
        return ids::person(raw).ok_or(ApiError::IdentityMalformed);
    }
    let query = uri.query().unwrap_or_default();
    for coppia in query.split('&') {
        let Some((chiave, valore)) = coppia.split_once('=') else {
            continue;
        };
        if chiave == crate::identity::IDENTITY_QUERY {
            return ids::person(&crate::identity::percent_decode(valore))
                .ok_or(ApiError::IdentityMalformed);
        }
    }
    Err(ApiError::IdentityMissing)
}

/// L'interfaccia, dalla radice web.
///
/// Se `web/` non c'è, il server non mente: dice che l'interfaccia non è
/// installata, invece di restituire una pagina vuota che sembrerebbe un
///'interfaccia rotta. L'interfaccia è dell'agente dopo; questa rotta è il
/// punto in cui il suo lavoro si vede.
async fn web(state: AppState, uri: Uri) -> Response {
    let radice = state.config.web_dir.clone();
    if !radice.is_dir() {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"interfaccia-non-installata","motivo":"manca la radice web: è il punto di mount dell'agente delle rotte"}"#,
        )
            .into_response();
    }
    let chiedi = uri.path().trim_start_matches('/');
    let candidato: Arc<std::path::PathBuf> = if chiedi.is_empty() {
        Arc::new(radice.join("index.html"))
    } else {
        // Anche qui il percorso viene controllato **dopo** la risoluzione: un
        // `web/` con un symlink verso `/etc` non deve poter servire `/etc`.
        match sandbox::within(&radice, &radice.join(chiedi)) {
            Some(p) => Arc::new(p),
            None => return ApiError::Absent.into_response(),
        }
    };
    match std::fs::read(candidato.as_ref()) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                sandbox::content_type(candidato.as_ref()),
            )
            .body(Body::from(bytes))
            .unwrap_or_else(|_| ApiError::Absent.into_response()),
        Err(_) => ApiError::Absent.into_response(),
    }
}
