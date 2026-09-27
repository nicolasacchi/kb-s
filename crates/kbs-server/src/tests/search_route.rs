//! La ricerca, che è l'indice condiviso che incontra la visibilità.

use axum::http::StatusCode;

use super::{Scuola, PAROLA_DI_A, PAROLA_SOLO_DI_B};

#[tokio::test]
async fn una_ricerca_vuota_o_un_limit_fuori_scala_sono_richieste_non_valide() {
    let scuola = Scuola::nuova();
    for uri in [
        "/api/v1/search",
        "/api/v1/search?q=",
        "/api/v1/search?q=%20%20",
        "/api/v1/search?q=teorema&limit=0",
        "/api/v1/search?q=teorema&limit=100000",
    ] {
        let risposta = scuola.get(uri, &scuola.docente).await;
        assert_eq!(
            risposta.status(),
            StatusCode::BAD_REQUEST,
            "`{uri}` non è stata rifiutata"
        );
    }
}

#[tokio::test]
async fn il_limit_e_un_soffitto_e_una_pensiera() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.indicizza(&argomento, PAROLA_DI_A);
    let corpo = Scuola::testo(
        scuola
            .get(
                &format!("/api/v1/search?q={PAROLA_DI_A}&limit=1"),
                &scuola.docente,
            )
            .await,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    assert_eq!(json["hits"].as_array().expect("hits").len(), 1);
    assert!(
        json["hits"][0]["rank"].as_f64().expect("rank") < 0.0,
        "il bm25 è negativo quando è migliore: {corpo}"
    );
}

#[tokio::test]
async fn la_ricerca_di_uno_studente_non_trova_le_bozze_del_docente() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.indicizza(&bozza, PAROLA_SOLO_DI_B);

    let corpo = Scuola::testo(
        scuola
            .get(&format!("/api/v1/search?q={PAROLA_SOLO_DI_B}"), &scuola.studente)
            .await,
    )
    .await;
    assert!(!corpo.contains(bozza.id.as_str()), "{corpo}");

    // Il docente la trova: il predicato è applicato dopo il bm25 e prima del
    // limite, quindi «più risultati» non significa «più leakage».
    let corpo = Scuola::testo(
        scuola
            .get(&format!("/api/v1/search?q={PAROLA_SOLO_DI_B}"), &scuola.docente)
            .await,
    )
    .await;
    assert!(corpo.contains(bozza.id.as_str()), "{corpo}");
}

#[tokio::test]
async fn la_ricerca_e_ordinata_per_rank_e_non_per_id() {
    let mut scuola = Scuola::nuova();
    let prima = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let seconda = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 2);
    scuola.indicizza(&prima, PAROLA_DI_A);
    // Il secondo contiene il termine due volte: il bm25 lo premierà.
    scuola
        .db
        .write(|store| {
            Ok(store.index_chunk(&kbs_store::ArtifactChunk {
                argument: seconda.id.clone(),
                course: seconda.course.clone(),
                kind: kbs_store::ChunkKind::Argument,
                ord: 1,
                rel_path: seconda.rel_path.clone().unwrap_or_default(),
                title: seconda.title.clone(),
                body: format!("{PAROLA_DI_A} {PAROLA_DI_A} {PAROLA_DI_A}"),
                headings: String::new(),
                code: String::new(),
                prompt: String::new(),
                contract: String::new(),
                updated_at: kbs_core::Millis(1_700_000_000_000),
            })?)
        })
        .expect("indicizzazione");

    let corpo = Scuola::testo(
        scuola
            .get(&format!("/api/v1/search?q={PAROLA_DI_A}"), &scuola.docente)
            .await,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    let hits = json["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), 2, "{corpo}");
    let primo = hits[0]["rank"].as_f64().expect("rank");
    let secondo = hits[1]["rank"].as_f64().expect("rank");
    assert!(
        primo < secondo,
        "l'ordine non è per rank crescente: {corpo}"
    );
}
