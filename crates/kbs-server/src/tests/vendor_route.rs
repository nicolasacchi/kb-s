//! D15: il runtime three.js è vendorizzato, e si serve come si deve.
//!
//! Il test che conta qui non è «il file c'è» — l'altro modulo lo verifica
//! all'avvio — ma «un artifact che lo referenzia lo ottiene, con gli header che
//! lo tengono in cache», e che il percorso sia **quello che il validatore ha
//! dichiarato dentro il perimetro**.

use axum::http::{header, Request, StatusCode};
use tower::util::ServiceExt;

use super::Scuola;

#[tokio::test]
async fn il_runtime_si_serve_sulla_rotta_che_kbs_doc_ha_dichiarato() {
    let scuola = Scuola::nuova();
    assert_eq!(kbs_doc::THREE_RUNTIME, "/three/three.module.min.js");

    let richiesta = Request::builder()
        .method("GET")
        .uri(kbs_doc::THREE_RUNTIME)
        .header(header::ACCEPT_ENCODING, "gzip, deflate, br")
        .body(axum::body::Body::empty())
        .expect("richiesta");
    let risposta = scuola.app.clone().oneshot(richiesta).await.expect("risposta");

    assert_eq!(risposta.status(), StatusCode::OK);
    assert_eq!(
        super::header_string(&risposta, header::CONTENT_TYPE).as_deref(),
        Some("application/javascript; charset=utf-8")
    );
    // Gli header di cache: è questo il motivo per cui il runtime è in un file
    // versionato e non in una CDN.
    let cache = super::header_string(&risposta, header::CACHE_CONTROL).expect("cache-control");
    assert!(cache.contains("max-age=31536000"), "{cache}");
    assert!(cache.contains("immutable"), "{cache}");
    let etag = super::header_string(&risposta, header::ETAG).expect("etag");
    assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");
    assert_eq!(
        super::header_string(&risposta, header::VARY).as_deref(),
        Some("Accept-Encoding")
    );
    // E la variante scelta è quella che il client ha chiesto.
    assert_eq!(
        super::header_string(&risposta, header::CONTENT_ENCODING).as_deref(),
        Some("gzip")
    );
}

#[tokio::test]
async fn un_client_senza_gzip_riceve_i_byte_interi() {
    let scuola = Scuola::nuova();
    let richiesta = Request::builder()
        .method("GET")
        .uri(kbs_doc::THREE_RUNTIME)
        .body(axum::body::Body::empty())
        .expect("richiesta");
    let risposta = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    assert_eq!(
        super::header_string(&risposta, header::CONTENT_ENCODING),
        None,
        "un client senza Accept-Encoding riceve byte compressi"
    );
    let corpo = Scuola::corpo(risposta).await;
    assert!(corpo.len() > 100_000, "il runtime sembra vuoto");
}

#[tokio::test]
async fn un_etag_corrispondente_da_304_senza_corpo() {
    let scuola = Scuola::nuova();
    let richiesta = Request::builder()
        .method("GET")
        .uri(kbs_doc::THREE_RUNTIME)
        .body(axum::body::Body::empty())
        .expect("richiesta");
    let prima = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    let etag = super::header_string(&prima, header::ETAG).expect("etag");

    let richiesta = Request::builder()
        .method("GET")
        .uri(kbs_doc::THREE_RUNTIME)
        .header(header::IF_NONE_MATCH, &etag)
        .body(axum::body::Body::empty())
        .expect("richiesta");
    let seconda = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    assert_eq!(seconda.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        super::header_string(&seconda, header::CACHE_CONTROL).as_deref(),
        Some("public, max-age=31536000, immutable"),
        "il 304 perde gli header di cache e la cache si ricalcola ogni volta"
    );
    assert!(Scuola::corpo(seconda).await.is_empty(), "il 304 ha un corpo");
}

#[tokio::test]
async fn un_artifact_che_referenzia_il_runtime_supera_la_validazione_e_lo_ottiene() {
    // Il percorso dentro l'artifact è quello che `kbs-doc` dichiara, quindi il
    // validatore lo accetta: se i due percorsi divergessero, l'artifact
    // passerebbe la validazione e non partirebbe, e il sintomo sarebbe «la 3D
    // non funziona» in una classe.
    let pagina = super::artifact::PAGINA;
    assert!(
        kbs_doc::external_references(pagina).is_empty(),
        "la pagina di prova non dovrebbe avere riferimenti fuori dalla scatola"
    );

    let scuola = Scuola::nuova();
    let richiesta = Request::builder()
        .method("GET")
        .uri(kbs_doc::THREE_RUNTIME)
        .header(header::HOST, "arg_0123456789abcdef.artifacts.localhost")
        .body(axum::body::Body::empty())
        .expect("richiesta");
    // Il runtime si serve **anche** sull'origine di un artifact: è la rotta su
    // cui l'artifact lo carica, e l'artifact sta su un'altra origine. Se questa
    // risposta fosse un 404, ogni 3D in ogni classe sarebbe rotta.
    let risposta = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    assert_eq!(risposta.status(), StatusCode::OK);
}
