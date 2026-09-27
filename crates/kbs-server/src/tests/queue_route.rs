//! La coda di ratifica: che cosa aspetta, e che cosa non passa.
//!
//! D4 reso visibile dal protocollo: la coda elenca **solo** ciò che non è
//! ancora in uso, la ratifica senza nota è rifiutata, e la pubblicazione senza
//! una ratifica valida è un `409` che nomina la regola.

use axum::http::StatusCode;
use serde_json::json;

use super::Scuola;

#[tokio::test]
async fn la_coda_elenca_che_cosa_aspetta_un_docente_e_nulla_che_e_gia_pubblicato() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let in_corso = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 2);
    // Un archivio non è in coda: non aspetta nessuno.
    let archiviato = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 3);
    scuola
        .db
        .write(|store| {
            let _ = store.archive(&archiviato.id);
            Ok(())
        })
        .expect("archiviazione");

    let risposta = scuola
        .get(
            &format!("/api/v1/courses/{}/queue", scuola.corso.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = Scuola::testo(risposta).await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");

    let voci = json["queue"].as_array().expect("coda");
    assert_eq!(voci.len(), 1, "la coda ha voci che non aspettano: {corpo}");
    assert_eq!(voci[0]["argument"]["id"], bozza.id.as_str());
    assert_eq!(voci[0]["needs_ratification"], json!(true));
    assert_eq!(voci[0]["needs_publication"], json!(false));

    // I due che non ci devono essere, detti per nome: un test che conta solo
    // non distingue «manca per il filtro» da «manca perché non c'è».
    assert!(!corpo.contains(in_corso.id.as_str()), "in coda un argomento in uso: {corpo}");
    assert!(!corpo.contains(archiviato.id.as_str()), "in coda un archivio: {corpo}");
}

#[tokio::test]
async fn una_ratifica_invecchiata_rimanda_l_argomento_in_coda() {
    let mut scuola = Scuola::nuova();
    // Bozza ratificata e poi riscritta: la ratifica vale per un hash che non
    // è più quello del contenuto. Si parte dalla bozza e non da un argomento
    // in uso perché `upsert_argument` non ammette `in-corso → del-docente`: è
    // il modo in cui D4 vieta di tornare indietro da una porta, e la coda
    // dell'invecchiamento la si vede su una bozza, che è il caso normale.
    let argomento = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let id = argomento.id.clone();
    scuola
        .db
        .write(|store| {
            let _ = store.ratify(&id, &kbs_core::PersonId::fixture(1), "prima stesura")?;
            Ok(())
        })
        .expect("ratifica");
    let hash_vecchio = argomento.content_hash.clone();
    let mut rivisto = argomento.clone();
    rivisto.state = kbs_core::PublicationState::DelDocente;
    rivisto.content_hash = "sha256:rivisto".into();
    scuola
        .db
        .write(|store| {
            store.upsert_argument(&rivisto)?;
            Ok(store.read_argument(&kbs_core::PersonId::fixture(1), &id)?)
        })
        .expect("revisione");
    assert_ne!(hash_vecchio, rivisto.content_hash);

    let corpo = Scuola::testo(
        scuola
            .get(
                &format!("/api/v1/courses/{}/queue", scuola.corso.as_str()),
                &scuola.docente,
            )
            .await,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    let voci = json["queue"].as_array().expect("coda");
    assert_eq!(voci.len(), 1, "{corpo}");
    assert_eq!(
        voci[0]["needs_ratification"], json!(true),
        "una ratifica invecchiata non è una ratifica: {corpo}"
    );
}

#[tokio::test]
async fn la_coda_e_del_docente_del_corso() {
    let mut scuola = Scuola::nuova();
    scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!("/api/v1/courses/{}/queue", scuola.corso.as_str());

    // Lo studente è del corso e non vede la coda: la coda è del docente.
    assert_eq!(scuola.get(&uri, &scuola.studente).await.status(), StatusCode::NOT_FOUND);
    // Il docente di un altro corso nemmeno ha la relazione.
    assert_eq!(
        scuola.get(&uri, &scuola.altro_docente).await.status(),
        StatusCode::NOT_FOUND
    );
    // E la risposta è la stessa di un corso che non esiste.
    let inesistente = scuola
        .get("/api/v1/courses/course_9999/queue", &scuola.studente)
        .await;
    let del_corso = scuola.get(&uri, &scuola.studente).await;
    assert_eq!(super::scheletro(&inesistente), super::scheletro(&del_corso));
    assert_eq!(
        Scuola::corpo(inesistente).await,
        Scuola::corpo(del_corso).await
    );
}

#[tokio::test]
async fn pubblicare_senza_ratifica_e_rifiutato_e_dice_d4() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!(
        "/api/v1/courses/{}/queue/{}/publish",
        scuola.corso.as_str(),
        bozza.id.as_str()
    );

    let risposta = scuola.post(&uri, &scuola.docente, json!({})).await;
    assert_eq!(
        risposta.status(),
        StatusCode::CONFLICT,
        "pubblicare senza ratifica è riuscito: D4 è decorativa"
    );
    let corpo = Scuola::testo(risposta).await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    assert_eq!(json["regola"], json!("D4"), "{corpo}");
    assert!(
        String::from_utf8_lossy(corpo.as_bytes()).contains("ratifica"),
        "il corpo non dice che cosa manca: {corpo}"
    );

    // E l'argomento non è entrato in uso.
    let letto = scuola
        .db
        .read(|store| {
            Ok(store.read_argument(&kbs_core::PersonId::fixture(1), &bozza.id)?)
        })
        .expect("lettura");
    assert_eq!(letto.state, kbs_core::PublicationState::Bozza);
}

#[tokio::test]
async fn ratificare_e_pubblicare_mediante_l_api_e_la_strada_prevista() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let base = format!(
        "/api/v1/courses/{}/queue/{}",
        scuola.corso.as_str(),
        bozza.id.as_str()
    );

    let risposta = scuola
        .post(&format!("{base}/ratify"), &scuola.docente, json!({"note": "verificato"}))
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = Scuola::testo(risposta).await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    assert_eq!(json["ratification"]["note"], json!("verificato"));
    assert_eq!(
        json["ratification"]["contract_hash"],
        json!(bozza.content_hash.as_str()),
        "la ratifica vale per un altro contenuto: {corpo}"
    );

    // Dopo la ratifica l'argomento è in coda per la pubblicazione, non più per
    // la ratifica: sono due gesti distinti e D4 li tiene distinti.
    let corpo = Scuola::testo(
        scuola
            .get(
                &format!("/api/v1/courses/{}/queue", scuola.corso.as_str()),
                &scuola.docente,
            )
            .await,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    let voci = json["queue"].as_array().expect("coda");
    assert_eq!(voci.len(), 1, "{corpo}");
    assert_eq!(voci[0]["needs_ratification"], json!(false), "{corpo}");
    assert_eq!(voci[0]["needs_publication"], json!(true), "{corpo}");

    let risposta = scuola.post(&format!("{base}/publish"), &scuola.docente, json!({})).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = Scuola::testo(risposta).await;
    assert!(corpo.contains("in-corso"), "{corpo}");

    // E adesso lo studente lo vede.
    let risposta = scuola
        .get(&format!("/api/v1/arguments/{}", bozza.id.as_str()), &scuola.studente)
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
}

#[tokio::test]
async fn una_ratifica_senza_nota_e_rifiutata() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!(
        "/api/v1/courses/{}/queue/{}/ratify",
        scuola.corso.as_str(),
        bozza.id.as_str()
    );
    for nota in [json!({}), json!({"note": ""}), json!({"note": "   "})] {
        let risposta = scuola.post(&uri, &scuola.docente, nota.clone()).await;
        assert_eq!(
            risposta.status(),
            StatusCode::BAD_REQUEST,
            "una ratifica con nota {nota} è passata"
        );
    }
}

#[tokio::test]
async fn ritirare_la_ratifica_torna_alla_coda_e_non_piu_citabile() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let base = format!(
        "/api/v1/courses/{}/queue/{}",
        scuola.corso.as_str(),
        argomento.id.as_str()
    );

    let risposta = scuola.post(&format!("{base}/withdraw"), &scuola.docente, json!({})).await;
    assert_eq!(risposta.status(), StatusCode::NO_CONTENT);

    // Non è più citabile: è la ragione per cui la rotta esiste.
    let letto = scuola
        .db
        .read(|store| {
            Ok(store.read_argument(&kbs_core::PersonId::fixture(1), &argomento.id)?)
        })
        .expect("lettura");
    assert!(!letto.is_citable_now(), "un argomento senza ratifica è citabile");

    // Ritirare di nuovo è un conflitto, non un 404 che sembrerebbe «non
    // esiste»: a questo punto la visibilità è già stata provata.
    let risposta = scuola.post(&format!("{base}/withdraw"), &scuola.docente, json!({})).await;
    assert_eq!(risposta.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn uno_studente_non_ratifica_e_non_pubblica() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let base = format!(
        "/api/v1/courses/{}/queue/{}",
        scuola.corso.as_str(),
        bozza.id.as_str()
    );
    for suffisso in ["ratify", "publish", "withdraw"] {
        let risposta = scuola
            .post(&format!("{base}/{suffisso}"), &scuola.studente, json!({"note": "ok"}))
            .await;
        assert_eq!(
            risposta.status(),
            StatusCode::NOT_FOUND,
            "lo studente ha eseguito `{suffisso}`"
        );
    }
    // E nessuno dei tre ha cambiato lo stato.
    let letto = scuola
        .db
        .read(|store| {
            Ok(store.read_argument(&kbs_core::PersonId::fixture(1), &bozza.id)?)
        })
        .expect("lettura");
    assert_eq!(letto.state, kbs_core::PublicationState::Bozza);
    assert!(letto.ratified.is_none());
}

#[tokio::test]
async fn il_corso_nel_percorso_deve_essere_il_corso_dell_argomento() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    // Il docente di B che chiede di ratificare un argomento di A: il percorso
    // e l'oggetto devono dire la stessa cosa, e se non la dicono la risposta è
    // «assente» — non quale dei due sia sbagliato, che nessuno deve poter
    // scoprire.
    let uri = format!(
        "/api/v1/courses/{}/queue/{}/ratify",
        scuola.altro_corso.as_str(),
        bozza.id.as_str()
    );
    let risposta = scuola
        .post(&uri, &scuola.altro_docente, json!({"note": "ok"}))
        .await;
    assert_eq!(risposta.status(), StatusCode::NOT_FOUND);
    let letto = scuola
        .db
        .read(|store| {
            Ok(store.read_argument(&kbs_core::PersonId::fixture(1), &bozza.id)?)
        })
        .expect("lettura");
    assert!(letto.ratified.is_none(), "l'argomento è stato ratificato");
}
