//! D9: sotto soglia un segnale non esiste, e la risposta non lo tradisce.
//!
//! Il test non può scrivere una riga sotto soglia — il `CHECK` del database e
//! `record_cohort_signal` lo impediscono entrambi, ed è la difesa principale. Per
//! esercitare **il filtro di questo crate** il test chiama direttamente
//! `routes::cohort::pubblicabile`, che è la terza difesa e l'unica che un
//! cambio di costante o una riga scritta a mano raggiungerebbero.

use axum::http::StatusCode;

use kbs_core::{CohortId, CohortSignal, Millis, COHORT_MIN_K};

use super::Scuola;

#[tokio::test]
async fn un_segnale_sotto_soglia_non_produce_nulla_tutto() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!("/api/v1/arguments/{}/cohort", argomento.id.as_str());

    // Non c'è nessun segnale registrato. La risposta è la stessa che ci sarebbe
    // se ce ne fosse uno sotto soglia: un elenco vuoto, senza conteggi.
    let risposta = scuola.get(&uri, &scuola.docente).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = Scuola::testo(risposta).await;
    assert_eq!(
        corpo,
        format!(
            "{{\"argument\":\"{}\",\"signals\":[]}}",
            argomento.id.as_str()
        ),
        "la risposta vuota contiene qualcosa d'altro: {corpo}"
    );
    // I controlli sono su nomi di campo, non su cifre: l'id contiene
    // esadecimali e un controllo su "0" troverebbe l'id, non un conteggio.
    for vietato in ["failing", "total", "soppress", "soppresso"] {
        assert!(
            !corpo.contains(vietato),
            "la risposta vuota contiene `{vietato}`: {corpo}"
        );
    }
}

#[tokio::test]
async fn un_segnale_sopra_soglia_esce_e_uno_sotto_no() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!("/api/v1/arguments/{}/cohort", argomento.id.as_str());

    let segnale = |failing: usize| CohortSignal {
        course: scuola.corso.clone(),
        cohort: CohortId("2A".into()),
        argument: argomento.id.clone(),
        failing,
        total: 30,
        at: Millis(1_700_000_000_000),
    };
    // Sotto soglia la **scrittura** è rifiutata: il segnale non entra, e il
    // dato individuale da cui verrebbe calcolato resta dov'è.
    let errore = scuola
        .db
        .write(|store| {
            let esito = store.record_cohort_signal(&segnale(COHORT_MIN_K - 1));
            assert!(esito.is_err(), "un segnale sotto soglia è stato scritto");
            esito
        })
        .expect_err("sotto soglia");
    assert!(
        errore.to_string().contains("soglia"),
        "l'errore non nomina la soglia: {errore}"
    );

    // Sopra soglia la scrittura passa **solo se il conteggio dichiarato è
    // vero**: `record_cohort_signal` ricalcola `COUNT(DISTINCT student)` dalle
    // osservazioni e rifiuta se non coincide. Quindi qui il test non può
    // dichiarare un numero: deve costruire cinque studenti che falliscono
    // davvero l'argomento, uno ciascuno.
    //
    // Prima che il negozio ricalcolasse, questo test passava dichiarando
    // `failing: 5` senza niente dietro — e il segnale che ne usciva diceva
    // «cinque studenti sbagliano questo argomento» quando a sbagliare era
    // stato uno solo, o nessuno.
    let sessione = scuola
        .db
        .write(|store| Ok(store.open_session(kbs_store::Register::Observations, "coorte")?))
        .expect("sessione di osservazioni");
    for n in 0..COHORT_MIN_K {
        let studente = kbs_core::PersonId::fixture(100 + n as u32);
        let persona = kbs_store::Person {
            id: studente.clone(),
            display_name: format!("studente {n}"),
            created_at: Millis(1_700_000_000_000),
        };
        scuola
            .db
            .write(|store| {
                store.upsert_person(&persona)?;
                Ok(store.append_observation(
                    &sessione,
                    kbs_store::ObservationDraft {
                        id: format!("obs-{n}"),
                        student: studente,
                        course: scuola.corso.clone(),
                        cohort: CohortId("2A".into()),
                        argument: argomento.id.clone(),
                        // Un fallimento è un esercizio verificato e risposto
                        // male. `Evidence::None` non è un fallimento: è
                        // l'assenza di una prova, e il negozio conta
                        // correttamente zero.
                        evidence: kbs_core::Evidence::Checked {
                            exercise: "es-sei".into(),
                            instance: "seed-1".into(),
                            correct: false,
                        },
                        judged_by: None,
                        // Senza aiuto: il segnale di coorte conta la vista
                        // `unaided_observations`, e un fallimento assistito non
                        // dice «questa classe non sa».
                        unaided: Some(true),
                        n_hints: Some(0),
                        at: Millis(1_700_000_000_000),
                    },
                )?)
            })
            .unwrap_or_else(|e| panic!("osservazione {n}: {e}"));
    }
    // Cinque studenti distinti, quindi `COUNT(DISTINCT student)` vale 5 e il
    // totale dichiarato è l'unico che il negozio accetterà.
    let segnale = |failing: usize| CohortSignal {
        course: scuola.corso.clone(),
        cohort: CohortId("2A".into()),
        argument: argomento.id.clone(),
        failing,
        total: 5,
        at: Millis(1_700_000_000_000),
    };
    scuola
        .db
        .write(|store| Ok(store.record_cohort_signal(&segnale(COHORT_MIN_K))?))
        .expect("sopra soglia");
    let corpo = Scuola::testo(scuola.get(&uri, &scuola.docente).await).await;
    let json: serde_json::Value = serde_json::from_str(&corpo).expect("json");
    let segnali = json["signals"].as_array().expect("segnali");
    assert_eq!(segnali.len(), 1, "{corpo}");
    assert_eq!(segnali[0]["failing"], serde_json::json!(COHORT_MIN_K));
}

#[tokio::test]
async fn il_filtro_di_questo_crate_respinge_un_segnale_piccolo() {
    // La difesa che `kbs-store` non può esercitare: una riga sotto soglia non
    // può esistere, quindi il suo filtro di lettura non ha niente su cui
    // provarsi. Il filtro di questa rotta, invece, si prova — ed è quello che
    // coprirebbe il caso in cui la costante di dominio cambiasse senza che una
    // migrazione seguisse.
    let segnale = |failing: usize| CohortSignal {
        course: kbs_core::CourseId::fixture(1),
        cohort: CohortId("2A".into()),
        argument: kbs_core::ArgumentId::from_rel_path("corsi/a/uno.html"),
        failing,
        total: 30,
        at: Millis(0),
    };
    assert!(!crate::routes::cohort::pubblicabile(&segnale(0)));
    assert!(!crate::routes::cohort::pubblicabile(&segnale(1)));
    assert!(!crate::routes::cohort::pubblicabile(&segnale(COHORT_MIN_K - 1)));
    assert!(crate::routes::cohort::pubblicabile(&segnale(COHORT_MIN_K)));
    assert!(crate::routes::cohort::pubblicabile(&segnale(COHORT_MIN_K + 1)));
}

#[tokio::test]
async fn la_coorte_e_del_docente_del_corso() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let uri = format!("/api/v1/arguments/{}/cohort", argomento.id.as_str());

    // Lo studente è iscritto e non vede la coorte: un aggregato sopra soglia
    // non rende nessuno anonimo fra gli studenti che lo leggono insieme.
    let studente = scuola.get(&uri, &scuola.studente).await;
    assert_eq!(studente.status(), StatusCode::NOT_FOUND);
    // E la risposta è la stessa di un argomento che non esiste.
    let inesistente = scuola
        .get(
            &format!("/api/v1/arguments/{}/cohort", super::id_inesistente().as_str()),
            &scuola.studente,
        )
        .await;
    assert_eq!(super::scheletro(&studente), super::scheletro(&inesistente));
    assert_eq!(Scuola::corpo(studente).await, Scuola::corpo(inesistente).await);
}
