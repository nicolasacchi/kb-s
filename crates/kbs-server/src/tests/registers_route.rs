//! I registri di una persona, e la contestazione che viaggia col giudizio.

use axum::http::StatusCode;

use kbs_core::{Contestation, ContestationOutcome, Evidence, GraderKind};

use super::Scuola;

/// Prepara un corso con un giudizio e una contestazione su una bozza
/// dell'insegnante.
///
/// La rubrica va registrata prima: `gradings.rubric_version` la referenzia, e
/// un voto senza rubric non entra nel database. Quindi la rubrica non è una
/// formalità di questo test, è un vincolo che il test esercita per poter
/// arrivare al resto.
fn con_giudizio_contestato(scuola: &mut Scuola) -> (kbs_core::ArgumentId, String) {
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let versione = "rub-v1".to_string();
    let corso = scuola.corso.clone();
    let giudizio = scuola
        .db
        .write(|store| {
            store.upsert_rubric(&kbs_store::Rubric {
                id: "rub-1".into(),
                course: corso.clone(),
                title: "Padronanza".into(),
                created_at: kbs_core::Millis(0),
            })?;
            store.upsert_rubric_version(&kbs_store::RubricVersion {
                id: versione.clone(),
                rubric: "rub-1".into(),
                version: "1".into(),
                scale: vec![kbs_store::GradeLevel {
                    grade: "A".into(),
                    label: "pieno".into(),
                    points: 1.0,
                }],
                defined_at: kbs_core::Millis(0),
                note: "prima versione".into(),
            })?;
            let sessione = store.open_session(kbs_store::Register::Gradings, "test")?;
            let giudizio = store.append_grading(
                &sessione,
                kbs_store::GradingDraft {
                    id: "grd-1".into(),
                    student: kbs_core::PersonId::fixture(3),
                    course: corso.clone(),
                    argument: argomento.id.clone(),
                    kind: GraderKind::Human,
                    graded_by: kbs_core::PersonId::fixture(1),
                    rubric_version: versione.clone(),
                    grade: "A".into(),
                    at: kbs_core::Millis(1_700_000_000_000),
                    contested: None,
                },
            )?;
            store.close_session(&sessione)?;
            // `contest_grading` registra solo che la contestazione c'è: l'esito
            // è una decisione **successiva** e ha il suo metodo. Scriverlo
            // insieme sarebbe un voto che si riscrive da solo, che è la cosa
            // che il trigger vieta.
            store.contest_grading(
                &giudizio.id,
                Contestation {
                    by: kbs_core::PersonId::fixture(3),
                    at: kbs_core::Millis(1_700_000_001_000),
                    reason: "la prova non era quella assegnata".into(),
                    outcome: None,
                },
            )?;
            store.resolve_contestation(&giudizio.id, ContestationOutcome::UnderReview)?;
            Ok(giudizio.id)
        })
        .expect("giudizio");
    (argomento.id, giudizio)
}

#[tokio::test]
async fn lo_studente_vede_il_proprio_registro_con_la_contestazione_dentro() {
    let mut scuola = Scuola::nuova();
    let (_, giudizio) = con_giudizio_contestato(&mut scuola);
    let uri = format!("/api/v1/courses/{}/gradings", scuola.corso.as_str());

    // Senza `?person`: l'identità dichiarata è quella che si guarda.
    let corpo = Scuola::testo(scuola.get(&uri, &scuola.studente).await).await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    let gradings = json["gradings"].as_array().expect("gradings");
    assert_eq!(gradings.len(), 1, "{corpo}");
    assert_eq!(gradings[0]["id"], giudizio.as_str());
    // La contestazione è **dentro** la riga: chi legge un ricorso deve vedere
    // il voto e la contestazione nello stesso atto.
    assert_eq!(
        gradings[0]["contested"]["reason"],
        serde_json::json!("la prova non era quella assegnata")
    );
    assert_eq!(gradings[0]["contested"]["outcome"], serde_json::json!("under-review"));
}

#[tokio::test]
async fn il_docente_vede_il_registro_del_proprio_corso() {
    let mut scuola = Scuola::nuova();
    con_giudizio_contestato(&mut scuola);
    let uri = format!(
        "/api/v1/courses/{}/gradings?person={}",
        scuola.corso.as_str(),
        scuola.studente.as_str()
    );
    let risposta = scuola.get(&uri, &scuola.docente).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = Scuola::testo(risposta).await;
    assert!(corpo.contains("la prova non era quella assegnata"), "{corpo}");
}

#[tokio::test]
async fn uno_studente_non_vede_il_registro_di_un_altro() {
    let mut scuola = Scuola::nuova();
    con_giudizio_contestato(&mut scuola);
    let uri = format!(
        "/api/v1/courses/{}/gradings?person={}",
        scuola.corso.as_str(),
        scuola.studente.as_str()
    );
    // L'altra studente è del corso e non ha emesso nessun giudizio. Nota il
    // `?person`: senza, `person` varrebbe l'identità dichiarata e l'altra
    // studente leggerebbe il proprio registro — che è vuoto, e quindi una
    // risposta vera.
    let risposta = scuola.get(&uri, &scuola.altra_studente).await;
    assert_eq!(risposta.status(), StatusCode::NOT_FOUND);
    let inesistente = scuola
        .get(
            &format!(
                "/api/v1/courses/{}/gradings?person={}",
                scuola.corso.as_str(),
                super::id_inesistente().as_str()
            ),
            &scuola.altra_studente,
        )
        .await;
    assert_eq!(super::scheletro(&risposta), super::scheletro(&inesistente));
    assert_eq!(Scuola::corpo(risposta).await, Scuola::corpo(inesistente).await);
}

#[tokio::test]
async fn un_docente_di_un_altro_corso_non_vede_i_registri() {
    let mut scuola = Scuola::nuova();
    con_giudizio_contestato(&mut scuola);
    let uri = format!("/api/v1/courses/{}/gradings", scuola.corso.as_str());
    assert_eq!(
        scuola.get(&uri, &scuola.altro_docente).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn le_osservazioni_di_un_argomento_appaiono_a_chi_ha_diritto() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let id = argomento.id.clone();
    let osservazione = scuola
        .db
        .write(|store| {
            let sessione = store.open_session(kbs_store::Register::Observations, "test")?;
            let osservazione = store.append_observation(
                &sessione,
                kbs_store::ObservationDraft {
                    id: "obs-1".into(),
                    student: kbs_core::PersonId::fixture(3),
                    course: kbs_core::CourseId::fixture(1),
                    cohort: kbs_core::CohortId("2A".into()),
                    argument: id.clone(),
                    evidence: Evidence::Checked {
                        exercise: "ex-1".into(),
                        instance: "seed-7".into(),
                        correct: true,
                    },
                    judged_by: Some(GraderKind::Deterministic),
                    unaided: Some(true),
                    n_hints: Some(0),
                    at: kbs_core::Millis(1_700_000_000_000),
                },
            )?;
            store.close_session(&sessione)?;
            Ok(osservazione.id)
        })
        .expect("osservazione");

    let uri = format!("/api/v1/arguments/{}/observations", id.as_str());
    let corpo = Scuola::testo(scuola.get(&uri, &scuola.studente).await).await;
    assert!(corpo.contains(&osservazione), "{corpo}");

    // Chi non ha diritto riceve «assente» e non un elenco vuoto: un elenco
    // vuoto sarebbe una risposta vera, e qui non lo è. Il `?person` serve,
    // perché senza l'altra studente leggerebbe il proprio — che è vuoto.
    let risposta = scuola
        .get(
            &format!("{uri}?person={}", scuola.studente.as_str()),
            &scuola.altra_studente,
        )
        .await;
    assert_eq!(risposta.status(), StatusCode::NOT_FOUND);
}

/// Lo studente che guarda il proprio registro **non vede la coda di practice**.
///
/// La prova è sul corpo della risposta, non sul fatto che la rotta sia stata
/// chiamata: un test che asserisce «la rotta risponde 200» passa anche quando
/// la risposta porta la coda di practice, che è il difetto che la regola
/// vieta. Qui si asserisce che l'id della riga assistita **non c'è nel testo**,
/// e che l'id della riga non assistita c'è.
#[tokio::test]
async fn il_registro_dello_studente_non_contiene_le_osservazioni_assistite() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 2);
    let id = argomento.id.clone();
    let (pura, assistita) = scuola
        .db
        .write(|store| {
            let sessione = store.open_session(kbs_store::Register::Observations, "test")?;
            let comune = |n: &str, unaided: Option<bool>, n_hints: Option<u32>| {
                kbs_store::ObservationDraft {
                    id: n.into(),
                    student: kbs_core::PersonId::fixture(3),
                    course: kbs_core::CourseId::fixture(1),
                    cohort: kbs_core::CohortId("2A".into()),
                    argument: id.clone(),
                    evidence: Evidence::Checked {
                        exercise: "ex-1".into(),
                        instance: format!("seed-{n}"),
                        correct: true,
                    },
                    judged_by: Some(GraderKind::Deterministic),
                    unaided,
                    n_hints,
                    at: kbs_core::Millis(1_700_000_000_000),
                }
            };
            let pura = store.append_observation(&sessione, comune("obs-pura", Some(true), Some(0)))?;
            let assistita = store.append_observation(
                &sessione,
                comune("obs-assistita", Some(false), Some(3)),
            )?;
            // E la riga che la migrazione produce da sé: precolumn, ignota.
            store.append_observation(&sessione, comune("obs-ignota", None, None))?;
            store.close_session(&sessione)?;
            Ok((pura.id, assistita.id))
        })
        .expect("osservazioni");

    let uri = format!("/api/v1/arguments/{}/observations", id.as_str());
    let corpo = Scuola::testo(scuola.get(&uri, &scuola.studente).await).await;
    assert!(corpo.contains(&pura), "la riga non assistita c'è: {corpo}");
    assert!(
        !corpo.contains(&assistita),
        "la coda di practice è nel registro dello studente: {corpo}"
    );
    assert!(
        !corpo.contains("obs-ignota"),
        "una riga di aiuto ignoto è nel registro dello studente: {corpo}"
    );

    // Il docente insegna anche dalla coda di practice: la stessa rotta, la
    // stessa risposta, e le tre righe. Una regola che funzionasse filtrando
    // sul ruolo avrebbe tolto anche al docente, e questa è la metà del test.
    //
    // Il `?person=` non è un di più: senza, `WhoQuery::persona` dà l'identità,
    // e l'identità del docente è il docente — che di osservazioni ne ha zero.
    // La rotta lo dichiara: «un docente che guarda il registro di uno studente
    // deve scriverlo». Chiedere «il registro» senza dire di chi è la domanda
    // che dà un elenco vuoto, e un elenco vuoto qui significa «non ha lavorato
    // su quell'argomento», che è un'altra informazione.
    let uri_docente = format!("{uri}?person={}", scuola.studente.as_str());
    let corpo_docente = Scuola::testo(scuola.get(&uri_docente, &scuola.docente).await).await;
    assert!(corpo_docente.contains(&pura), "{corpo_docente}");
    assert!(corpo_docente.contains(&assistita), "{corpo_docente}");
    assert!(corpo_docente.contains("obs-ignota"), "{corpo_docente}");
}
