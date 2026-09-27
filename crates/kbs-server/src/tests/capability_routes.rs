//! Il predicato, visto da fuori.
//!
//! Qui stanno le quattro promesse che questo crate fa al mondo e che un
//! chiamante può verificare senza leggere una riga di codice:
//!
//! * uno studente iscritto non legge una bozza; l'autore e il docente sì;
//! * un docente di un corso non legge niente di un altro corso, per rotta e per
//!   ricerca;
//! * «non c'è» e «non lo vedi» sono la stessa risposta, byte per byte;
//! * un insegnante che non è del corso non riceve niente della sua coda, della
//!   sua coorte o del suo export.

use axum::http::StatusCode;

use kbs_core::PublicationState;

use super::{scheletro, Scuola, PAROLA_DI_A, PAROLA_SOLO_DI_B};

#[tokio::test]
async fn lo_studente_iscritto_non_legge_una_bozza_l_autore_e_il_docente_si() {
    let mut scuola = Scuola::nuova();
    // La bozza è dell'autore, che **non** insegna il corso: senza questo,
    // «l'autore la vede» e «il docente la vede» sarebbero lo stesso test.
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.autore.clone(), 1);
    let uri = format!("/api/v1/arguments/{}", bozza.id.as_str());

    let risposta = scuola.get(&uri, &scuola.studente).await;
    assert_eq!(risposta.status(), StatusCode::NOT_FOUND, "lo studente legge la bozza");

    let risposta = scuola.get(&uri, &scuola.docente).await;
    assert_eq!(risposta.status(), StatusCode::OK, "il docente non legge la propria classe");
    let corpo = Scuola::testo(risposta).await;
    assert!(corpo.contains("Lezione 01"), "{corpo}");
    assert!(corpo.contains(&bozza.content_hash), "manca l'hash di contenuto");

    let risposta = scuola.get(&uri, &scuola.autore).await;
    assert_eq!(
        risposta.status(),
        StatusCode::OK,
        "l'autore non vede il proprio argomento: `author_of` non è una relazione di corso"
    );
}

#[tokio::test]
async fn un_docente_di_un_corso_non_legge_niente_dell_altro() {
    let mut scuola = Scuola::nuova();
    let pubblicato = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    // Anche **pubblicato**: il predicato non distingue per stato, e un
    // argomento in uso di un altro corso è materiale quanto una bozza.
    let altrui = scuola.pubblicato(&scuola.altro_corso.clone(), &scuola.altro_docente.clone(), 2);
    scuola.indicizza(&pubblicato, PAROLA_DI_A);
    scuola.indicizza(&altrui, PAROLA_SOLO_DI_B);

    // Per rotta.
    let uri = format!("/api/v1/arguments/{}", altrui.id.as_str());
    assert_eq!(
        scuola.get(&uri, &scuola.docente).await.status(),
        StatusCode::NOT_FOUND,
        "il docente di A legge un argomento in uso di B"
    );
    // E per elenco: senza relazione col corso la risposta è «assente», la
    // stessa di un corso inesistente. Non un elenco vuoto: `[]` è una risposta
    // vera e qui significherebbe «il corso c'è e dentro non vedi niente»,
    // che è metà della verità che la risposta non deve dare.
    let lista = scuola
        .get(
            &format!("/api/v1/courses/{}/arguments", scuola.altro_corso.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(lista.status(), StatusCode::NOT_FOUND);
    // E per ricerca: il termine che sta solo in B non trova niente.
    let ricerca = scuola
        .get(
            &format!("/api/v1/search?q={PAROLA_SOLO_DI_B}"),
            &scuola.docente,
        )
        .await;
    let corpo = Scuola::testo(ricerca).await;
    assert!(
        !corpo.contains(altrui.id.as_str()),
        "la ricerca ha restituito un argomento di B: {corpo}"
    );

    // Il contro-prova: il termine che sta in A torna. Se il filtro fosse
    // chiuso troppo, questo test passerebbe senza coprire niente.
    let ricerca = scuola
        .get(&format!("/api/v1/search?q={PAROLA_DI_A}"), &scuola.docente)
        .await;
    let corpo = Scuola::testo(ricerca).await;
    assert!(
        corpo.contains(pubblicato.id.as_str()),
        "la ricerca non ha trovato il proprio argomento: {corpo}"
    );
}

#[tokio::test]
async fn il_docente_di_b_trova_il_suo_e_non_quello_di_a() {
    let mut scuola = Scuola::nuova();
    let di_a = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let di_b = scuola.pubblicato(&scuola.altro_corso.clone(), &scuola.altro_docente.clone(), 2);
    scuola.indicizza(&di_a, PAROLA_DI_A);
    scuola.indicizza(&di_b, PAROLA_SOLO_DI_B);

    let corpo = Scuola::corpo(scuola
            .get(&format!("/api/v1/search?q={PAROLA_DI_A}"), &scuola.altro_docente)
            .await)
        .await;
    let corpo = String::from_utf8(corpo).expect("utf-8");
    assert!(!corpo.contains(di_a.id.as_str()), "{corpo}");
    let corpo = Scuola::corpo(scuola
            .get(&format!("/api/v1/search?q={PAROLA_SOLO_DI_B}"), &scuola.altro_docente)
            .await)
        .await;
    let corpo = String::from_utf8(corpo).expect("utf-8");
    assert!(corpo.contains(di_b.id.as_str()), "{corpo}");
}

#[tokio::test]
async fn un_id_inesistente_e_un_id_invisibile_danno_la_stessa_risposta() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.autore.clone(), 1);

    let inesistente = scuola
        .get(
            &format!("/api/v1/arguments/{}", super::id_inesistente().as_str()),
            &scuola.studente,
        )
        .await;
    let invisibile = scuola
        .get(&format!("/api/v1/arguments/{}", bozza.id.as_str()), &scuola.studente)
        .await;

    assert_eq!(
        scheletro(&inesistente),
        scheletro(&invisibile),
        "status o header diversi fra «non esiste» e «non lo vedi»"
    );
    let corpo_inesistente = Scuola::corpo(inesistente).await;
    let corpo_invisibile = Scuola::corpo(invisibile).await;
    assert_eq!(
        corpo_inesistente, corpo_invisibile,
        "i corpi differiscono: è il canale che D5 vieta"
    );
    assert_eq!(
        String::from_utf8(corpo_invisibile.clone()).expect("utf-8"),
        crate::ApiError::ABSENT_BODY
    );
}

#[tokio::test]
async fn la_risposta_di_assente_non_contiene_il_percorso_ne_lo_stato() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.autore.clone(), 1);
    let corpo = Scuola::testo(
        scuola
            .get(&format!("/api/v1/arguments/{}", bozza.id.as_str()), &scuola.studente)
            .await,
    )
    .await;
    assert!(!corpo.contains("bozza"), "lo stato trapela: {corpo}");
    assert!(!corpo.contains("lezione-01"), "il percorso trapela: {corpo}");
    assert!(!corpo.contains(bozza.id.as_str()), "l'id trapela: {corpo}");
}

#[tokio::test]
async fn un_id_malformato_e_assente_e_non_una_richiesta_non_valida() {
    let scuola = Scuola::nuova();
    // Non ha la forma di `kbs-core`, quindi non è un id: la risposta è «assente»,
    // non «richiesta non valida». Il motivo è che la forma di un id è pubblica e
    // distinguerla da un id che non esiste non dice niente a chi non ha già il
    // corpus, ma fa sembrare che questo server accetti forme diverse.
    let risposta = scuola
        .get("/api/v1/arguments/lezione-01", &scuola.docente)
        .await;
    assert_eq!(risposta.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn senza_identita_dichiarata_non_si_passa() {
    let mut scuola = Scuola::nuova();
    let pubblicato = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let richiesta = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/api/v1/arguments/{}", pubblicato.id.as_str()))
        .body(axum::body::Body::empty())
        .expect("richiesta");
    let risposta = tower::util::ServiceExt::oneshot(scuola.app.clone(), richiesta)
        .await
        .expect("risposta");
    assert_eq!(risposta.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn l_elenco_di_un_corso_estraneo_e_come_quello_di_un_corso_inesistente() {
    let mut scuola = Scuola::nuova();
    scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);

    // Il corso esiste e non ci sei dentro.
    let estranea = scuola
        .get(
            &format!("/api/v1/courses/{}/arguments", scuola.corso.as_str()),
            &scuola.estraneo,
        )
        .await;
    // Il corso non esiste.
    let inesistente = scuola
        .get("/api/v1/courses/course_9999/arguments", &scuola.estraneo)
        .await;
    assert_eq!(estranea.status(), StatusCode::NOT_FOUND);
    assert_eq!(super::scheletro(&estranea), super::scheletro(&inesistente));
    assert_eq!(Scuola::corpo(estranea).await, Scuola::corpo(inesistente).await);
}

#[tokio::test]
async fn lo_studente_vede_gli_argomenti_in_uso_e_non_le_bozze() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let pubblicato = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 2);

    let corpo = Scuola::corpo(scuola
            .get(
                &format!("/api/v1/courses/{}/arguments", scuola.corso.as_str()),
                &scuola.studente,
            )
            .await)
        .await;
    let corpo = String::from_utf8(corpo).expect("utf-8");
    assert!(corpo.contains(pubblicato.id.as_str()), "{corpo}");
    assert!(!corpo.contains(bozza.id.as_str()), "la bozza è nell'elenco: {corpo}");
}

#[tokio::test]
async fn il_filtro_per_stato_non_riporta_indietro_le_bozze() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let corpo = Scuola::corpo(scuola
            .get(
                &format!(
                    "/api/v1/courses/{}/arguments?state=bozza",
                    scuola.corso.as_str()
                ),
                &scuola.studente,
            )
            .await)
        .await;
    let corpo = String::from_utf8(corpo).expect("utf-8");
    // Lo studente chiede esplicitamente le bozze. Il filtro per stato
    // restringe un insieme che il predicato ha già autorizzato, quindi non può
    // riportare indietro niente: l'inverso dei due controlli è il modo
    // classico di restituire le bozze del docente a uno studente.
    assert!(!corpo.contains(bozza.id.as_str()), "{corpo}");
    assert_eq!(corpo.matches("arg_").count(), 0, "{corpo}");
}

#[tokio::test]
async fn uno_studente_vede_le_claim_del_suo_corso_e_non_le_bozze() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let pubblicato = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 2);
    let argomento = pubblicato.clone();
    scuola
        .db
        .write(|store| {
            Ok(store.append_claim(&kbs_core::Claim {
                id: "clm-1".into(),
                course: argomento.course.clone(),
                argument: argomento.id.clone(),
                text: "Ogni funzione continua è integrabile.".into(),
                span_anchor: Some("#esempio".into()),
                span_text: Some("si veda l'esempio 3".into()),
                status: kbs_core::ClaimStatus::Supported,
                emitted_at: kbs_core::Millis(1_700_000_000_000),
                emitted_by: kbs_core::Emitter::Content {
                    argument: argomento.id.clone(),
                },
            })?)
        })
        .expect("claim");

    let uri = format!("/api/v1/arguments/{}/claims", pubblicato.id.as_str());
    let corpo = Scuola::corpo(scuola.get(&uri, &scuola.studente).await)
        .await;
    let corpo = String::from_utf8(corpo).expect("utf-8");
    assert!(corpo.contains("clm-1"), "{corpo}");

    // Le claim di una bozza sono invisibili come la bozza: una claim non è
    // altro materiale con regole sue.
    let uri = format!("/api/v1/arguments/{}/claims", bozza.id.as_str());
    assert_eq!(
        scuola.get(&uri, &scuola.studente).await.status(),
        StatusCode::NOT_FOUND
    );
    let risposta = scuola.get(&uri, &scuola.docente).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = Scuola::testo(risposta).await;
    assert_eq!(corpo.matches("\"id\"").count(), 0, "{corpo}");
}

#[tokio::test]
async fn l_archiviato_resta_visibile_agli_iscritti() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let id = argomento.id.clone();
    let studente = scuola.studente.clone();
    scuola
        .db
        .write(|store| {
            let _ = store.archive(&id);
            Ok(())
        })
        .expect("archiviazione");
    let letto = scuola
        .db
        .read(|store| Ok(store.read_argument(&studente, &id)?))
        .expect("lettura");
    assert_eq!(letto.state, PublicationState::Archiviato);
    let risposta = scuola
        .get(&format!("/api/v1/arguments/{}", id.as_str()), &studente)
        .await;
    assert_eq!(
        risposta.status(),
        StatusCode::OK,
        "l'archiviato non è più visibile a chi lo ha seguito"
    );
}

