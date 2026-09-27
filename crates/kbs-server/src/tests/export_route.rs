//! D12: l'uscita è `rm -rf`, e l'esportazione è del docente.
//!
//! Il formato è di `kbs-store` e non viene toccato qui; quello che questo
//! crate deve garantire è **chi** lo ottiene e con quale silenzio quando non lo
//! ottiene.

use axum::http::StatusCode;

use crate::routes::export;

use super::Scuola;

#[tokio::test]
async fn l_esportazione_e_del_docente_del_corso() {
    let mut scuola = Scuola::nuova();
    scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!("/api/v1/courses/{}/export", scuola.corso.as_str());

    let risposta = scuola.get(&uri, &scuola.docente).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    assert_eq!(
        super::header_string(&risposta, axum::http::header::CONTENT_TYPE).as_deref(),
        Some(export::CONTENT_TYPE)
    );
    let corpo = Scuola::testo(risposta).await;
    let colonne: Vec<&str> = corpo.lines().next().expect("intestazione").split('\t').collect();
    assert_eq!(
        colonne,
        kbs_store::FIXED_COLUMNS.to_vec(),
        "l'intestazione non è quella dichiarata"
    );
    assert!(corpo.contains("Lezione 01"), "{corpo}");
}

#[tokio::test]
async fn chi_non_insegna_il_corso_non_esporta_e_non_si_distingue_da_un_corso_inesistente() {
    let mut scuola = Scuola::nuova();
    scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!("/api/v1/courses/{}/export", scuola.corso.as_str());

    // Lo studente è del corso: sa che il corso esiste, e la risposta gli dice
    // solo che l'esportazione non è sua.
    let studente = scuola.get(&uri, &scuola.studente).await;
    assert_eq!(studente.status(), StatusCode::NOT_FOUND);

    // Una persona senza relazioni riceve la stessa risposta di un corso che non
    // esiste: se le due differissero, l'esistenza del corso sarebbe un canale.
    let estranea = scuola.get(&uri, &scuola.estraneo).await;
    let inesistente = scuola.get("/api/v1/courses/course_9999/export", &scuola.estraneo).await;
    assert_eq!(super::scheletro(&estranea), super::scheletro(&inesistente));
    assert_eq!(
        Scuola::corpo(estranea).await,
        Scuola::corpo(inesistente).await
    );

    // E il docente dell'altro corso non esporta il primo.
    assert_eq!(
        scuola.get(&uri, &scuola.altro_docente).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn l_esportazione_non_espone_le_bozze() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 2);
    let corpo = Scuola::testo(
        scuola
            .get(
                &format!("/api/v1/courses/{}/export", scuola.corso.as_str()),
                &scuola.docente,
            )
            .await,
    )
    .await;
    assert!(!corpo.contains(bozza.id.as_str()), "una bozza è in export: {corpo}");
    // Le righe sono quelle citabili, quindi la sola riga dati è quella
    // pubblicata: l'intestazione più una.
    assert_eq!(corpo.lines().count(), 2, "{corpo}");
}

#[tokio::test]
async fn l_esportazione_ha_un_nome_di_file_e_non_viene_memorizzata() {
    let mut scuola = Scuola::nuova();
    scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let risposta = scuola
        .get(
            &format!("/api/v1/courses/{}/export", scuola.corso.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(
        super::header_string(&risposta, axum::http::header::CONTENT_DISPOSITION).as_deref(),
        Some("attachment; filename=\"course_0001.tsv\"")
    );
    assert_eq!(
        super::header_string(&risposta, axum::http::header::CACHE_CONTROL).as_deref(),
        Some("no-store")
    );
}
