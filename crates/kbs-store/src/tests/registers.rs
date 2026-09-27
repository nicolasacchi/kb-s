//! I registri append-only, la contestazione e la coorte.
//!
//! Qui la prova che conta è una: le righe di registro non si riscrivono **nemmeno
//! passando dalla porta dichiarata** [`Store::conn`]. È l'unico modo che un test
//! può dare di una proprietà che nessuna firma di metodo può imporre.

use kbs_core::{
    Claim, ClaimStatus, CohortSignal, Contestation, ContestationOutcome, Emitter, Evidence,
    GraderKind, Millis, PersonId,
};

use super::School;
use crate::error::Error;
use crate::types::{GradeLevel, GradingDraft, ObservationDraft, Register, RubricVersion};

const T0: i64 = 1_700_000_000_000;

fn rubric(s: &mut School) -> String {
    s.store
        .upsert_rubric(&crate::types::Rubric {
            id: "rub-1".to_string(),
            course: s.course.clone(),
            title: "Padronanza del concetto".to_string(),
            created_at: Millis(T0),
        })
        .expect("rubrica");
    s.store
        .upsert_rubric_version(&RubricVersion {
            id: "rub-1@1".to_string(),
            rubric: "rub-1".to_string(),
            version: "1".to_string(),
            scale: vec![
                GradeLevel {
                    grade: "0".into(),
                    label: "non raggiunto".into(),
                    points: 0.0,
                },
                GradeLevel {
                    grade: "1".into(),
                    label: "raggiunto".into(),
                    points: 1.0,
                },
            ],
            defined_at: Millis(T0),
            note: "due livelli, come da consegna".to_string(),
        })
        .expect("versione");
    "rub-1@1".to_string()
}

fn observation(n: u32, s: &School, argument: &kbs_core::ArgumentId) -> ObservationDraft {
    ObservationDraft {
        id: format!("obs-{n:03}"),
        student: s.student.clone(),
        course: s.course.clone(),
        cohort: s.cohort(),
        argument: argument.clone(),
        evidence: Evidence::Checked {
            exercise: "ex-1".into(),
            instance: "seed-1".into(),
            correct: true,
        },
        judged_by: Some(GraderKind::Deterministic),
        at: Millis(T0 + n as i64),
    }
}

fn grading(n: u32, s: &School, argument: &kbs_core::ArgumentId, rubric: &str) -> GradingDraft {
    GradingDraft {
        id: format!("grd-{n:03}"),
        student: s.student.clone(),
        course: s.course.clone(),
        argument: argument.clone(),
        kind: GraderKind::Deterministic,
        graded_by: s.teacher.clone(),
        rubric_version: rubric.to_string(),
        grade: "1".to_string(),
        at: Millis(T0 + n as i64),
        contested: None,
    }
}

#[test]
fn il_seq_e_assegnato_dal_registro_e_non_da_chi_scrive() {
    let mut s = School::new();
    let arg = s.published(1);
    let session = s.session(Register::Observations);

    // Il draft non ha un campo `seq`: non è chi scrive a sceglierlo. Se ne avesse
    // uno, due scrittori darebbero lo stesso numero e la catena avrebbe due
    // foglie nella stessa posizione.
    let prima = s
        .store
        .append_observation(&session, observation(1, &s, &arg.id))
        .expect("prima osservazione");
    assert_eq!(prima.seq.0, 1);

    let seconda = s
        .store
        .append_observation(&session, observation(2, &s, &arg.id))
        .expect("seconda osservazione");
    assert_eq!(seconda.seq.0, 2);

    let lette = s
        .store
        .observations_in_session(&s.teacher, &session)
        .expect("lettura in ordine");
    assert_eq!(
        lette.iter().map(|o| o.seq.0).collect::<Vec<_>>(),
        vec![1, 2],
        "l'ordine che la catena ordina"
    );
}

#[test]
fn la_seq_riparte_in_ogni_sessione() {
    let mut s = School::new();
    let arg = s.published(1);
    let prima = s.session(Register::Observations);
    s.store
        .append_observation(&prima, observation(1, &s, &arg.id))
        .expect("prima");
    let seconda = s.session(Register::Observations);
    let riga = s
        .store
        .append_observation(&seconda, observation(2, &s, &arg.id))
        .expect("seconda sessione");
    assert_eq!(riga.seq.0, 1, "il seq sta dentro la sua sessione");
}

#[test]
fn una_sessione_chiusa_non_si_append_e_una_di_registro_sbagliato_no() {
    let mut s = School::new();
    let arg = s.published(1);
    let osservazioni = s.session(Register::Observations);
    let gradini = s.session(Register::Gradings);

    let err = s
        .store
        .append_observation(&gradini, observation(1, &s, &arg.id))
        .expect_err("registro sbagliato");
    assert_eq!(err.rule(), "session.register");

    s.store.close_session(&osservazioni).expect("chiusura");
    assert_eq!(
        s.store
            .append_observation(&osservazioni, observation(2, &s, &arg.id))
            .expect_err("sessione chiusa")
            .rule(),
        "session.sealed"
    );
    assert_eq!(
        s.store.close_session(&osservazioni).expect_err("già chiusa").rule(),
        "session.sealed"
    );
    assert_eq!(
        s.store
            .append_observation(
                &kbs_verify::SessionId::new("ses_inesistente"),
                observation(3, &s, &arg.id)
            )
            .expect_err("sessione inesistente")
            .rule(),
        "storage"
    );
}

#[test]
fn le_osservazioni_non_si_aggiornano_e_non_si_cancellano_nemmeno_da_conn() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    let riga = s
        .store
        .append_observation(&sessione, observation(1, &s, &arg.id))
        .expect("osservazione");

    let err = s
        .store
        .conn()
        .execute("UPDATE observations SET at = 1 WHERE id = ?1", [&riga.id])
        .expect_err("UPDATE su un registro append-only");
    assert!(err.to_string().contains("append-only"), "{err}");

    let err = s
        .store
        .conn()
        .execute("DELETE FROM observations WHERE id = ?1", [&riga.id])
        .expect_err("DELETE su un registro append-only");
    assert!(err.to_string().contains("append-only"), "{err}");

    let lette = s
        .store
        .observations_in_session(&s.teacher, &sessione)
        .expect("rilettura");
    assert_eq!(lette.len(), 1);
    assert_eq!(lette[0].at, riga.at);
}

#[test]
fn il_voto_e_congelato_e_la_contestazione_e_viva() {
    let mut s = School::new();
    let arg = s.published(1);
    let rubric = rubric(&mut s);
    let sessione = s.session(Register::Gradings);
    let giudizio = s
        .store
        .append_grading(&sessione, grading(1, &s, &arg.id, &rubric))
        .expect("giudizio");

    let err = s
        .store
        .conn()
        .execute("UPDATE gradings SET grade = '0' WHERE id = ?1", [&giudizio.id])
        .expect_err("il voto non si riscrive");
    assert!(err.to_string().contains("congelato"), "{err}");
    let err = s
        .store
        .conn()
        .execute("DELETE FROM gradings WHERE id = ?1", [&giudizio.id])
        .expect_err("il giudizio non si cancella");
    assert!(err.to_string().contains("append-only"), "{err}");

    // La contestazione invece si annota, e una volta sola: sostituire un ricorso
    // farebbe perdere la traccia del primo, che è ciò che la difesa procedurale
    // non può permettere.
    s.store
        .contest_grading(
            &giudizio.id,
            Contestation {
                by: s.student.clone(),
                at: Millis(T0 + 10),
                reason: "il verificatore ha letto male l'istanza".to_string(),
                outcome: None,
            },
        )
        .expect("contestazione");
    assert_eq!(
        s.store
            .contest_grading(
                &giudizio.id,
                Contestation {
                    by: s.student.clone(),
                    at: Millis(T0 + 11),
                    reason: "seconda".into(),
                    outcome: None,
                },
            )
            .expect_err("una contestazione si apre una volta")
            .rule(),
        "grading.contestation"
    );
    s.store
        .resolve_contestation(&giudizio.id, ContestationOutcome::Upheld)
        .expect("esito");
    assert_eq!(
        s.store
            .resolve_contestation("grd-inesistente", ContestationOutcome::Upheld)
            .expect_err("giudizio inesistente")
            .rule(),
        "lookup.missing"
    );
}

#[test]
fn un_giudizio_contestato_si_legge_intero_in_una_query() {
    let mut s = School::new();
    let arg = s.published(1);
    let rubric = rubric(&mut s);
    let sessione = s.session(Register::Gradings);
    let giudizio = s
        .store
        .append_grading(&sessione, grading(1, &s, &arg.id, &rubric))
        .expect("giudizio");
    s.store
        .contest_grading(
            &giudizio.id,
            Contestation {
                by: s.student.clone(),
                at: Millis(T0 + 10),
                reason: "istanza sbagliata".to_string(),
                outcome: None,
            },
        )
        .expect("contestazione");
    s.store
        .resolve_contestation(&giudizio.id, ContestationOutcome::Rejected)
        .expect("esito");

    // Una sola `SELECT`: la contestazione è dentro la riga, e due query che
    // possono disaccordarsi sono due fonti di verità.
    let letti = s
        .store
        .gradings_for(&s.teacher, &s.student, &arg.id)
        .expect("lettura");
    assert_eq!(letti.len(), 1);
    let letto = &letti[0];
    assert_eq!(letto.grade, "1");
    let contestazione = letto.contested.as_ref().expect("nella stessa riga");
    assert_eq!(contestazione.by, s.student);
    assert_eq!(contestazione.reason, "istanza sbagliata");
    assert_eq!(contestazione.outcome, Some(ContestationOutcome::Rejected));
}

#[test]
fn un_giudizio_senza_rubric_non_entra() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Gradings);
    let err = s
        .store
        .append_grading(&sessione, grading(1, &s, &arg.id, "rub-inesistente@9"))
        .expect_err("una rubric inesistente non è una rubric");
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");

    let rubric = rubric(&mut s);
    s.store
        .append_grading(&sessione, grading(2, &s, &arg.id, &rubric))
        .expect("con la rubric esistente");
    let letta = s
        .store
        .rubric_version(&s.teacher, &rubric)
        .expect("rubrica");
    assert_eq!(letta.scale.len(), 2, "il voto ha una scala");
}

#[test]
fn il_registro_di_uno_studente_lo_vedono_lui_e_il_docente() {
    let mut s = School::new();
    let arg = s.published(1);
    let rubric = rubric(&mut s);
    let sessione = s.session(Register::Gradings);
    s.store
        .append_grading(&sessione, grading(1, &s, &arg.id, &rubric))
        .expect("giudizio");

    assert_eq!(
        s.store
            .student_gradings(&s.student, &s.student, &s.course)
            .expect("lo studente vede il proprio")
            .len(),
        1
    );
    assert_eq!(
        s.store
            .student_gradings(&s.teacher, &s.student, &s.course)
            .expect("il docente vede quello del corso")
            .len(),
        1
    );
    s.store
        .student_gradings(&s.outsider, &s.student, &s.course)
        .expect_err("uno che non c'entra non vede il registro di nessuno");
    s.store
        .observations_for(&s.outsider, &s.student, &arg.id)
        .expect_err("neanche le dimostrazioni");
}

#[test]
fn chi_ha_emesso_il_giudizio_lo_rivede_e_il_compagno_no() {
    let mut s = School::new();
    let arg = s.published(1);
    let rubric = rubric(&mut s);
    let sessione = s.session(Register::Gradings);
    // Un pari, che non è docente e non è lo studente valutato. È **iscritto al
    // corso**, perché un pari è un compagno: senza l'iscrizione non potrebbe
    // neppure vedere l'argomento, e il test passerebbe per una ragione sbagliata.
    let pari = s.outsider.clone();
    s.store
        .upsert_person(&crate::types::Person {
            id: pari.clone(),
            display_name: "Pari".to_string(),
            created_at: Millis(T0),
        })
        .expect("persona");
    s.store
        .add_relation(&crate::types::CourseRelation {
            person: pari.clone(),
            course: s.course.clone(),
            relation: kbs_core::Relation::EnrolledIn,
            since: Millis(T0),
            until: None,
        })
        .expect("iscrizione del pari");
    s.store
        .append_grading(
            &sessione,
            GradingDraft {
                graded_by: pari.clone(),
                kind: GraderKind::Peer,
                ..grading(1, &s, &arg.id, &rubric)
            },
        )
        .expect("giudizio del pari");
    // **Il secondo giudizio è la riga che rende questo test possibile.** Con un
    // giudizio solo, «il pari vede una riga» e «il pari vede tutto il registro» sono
    // la stessa frase: il test passava con l'escalation in piedi, perché non c'era
    // niente che non potesse vedere. Il docente che giudica lo stesso studente su
    // un altro argomento mette sotto gli occhi la differenza.
    let altro_arg = s.published(2);
    s.store
        .append_grading(
            &sessione,
            GradingDraft {
                id: "grd-002".into(),
                argument: altro_arg.id.clone(),
                graded_by: s.teacher.clone(),
                kind: GraderKind::Teacher,
                ..grading(2, &s, &altro_arg.id, &rubric)
            },
        )
        .expect("giudizio del docente");

    let letti = s
        .store
        .student_gradings(&pari, &s.student, &s.course)
        .expect("chi ha emesso rivede il proprio giudizio");
    assert_eq!(
        letti.len(),
        1,
        "il pari vede il proprio giudizio e non quello del docente"
    );
    assert_eq!(letti[0].graded_by, pari);
    assert_eq!(letti[0].kind, GraderKind::Peer);

    // E lo stesso vale per un singolo argomento: il pari vede il proprio
    // giudizio su quell'argomento, e su quell'argomento non ce ne sono altri,
    // quindi l'escalation si vede sul corso intero. Qui si prova il percorso
    // con l'argomento, che è quello che chiama `gradings_for`.
    let sull_argomento = s
        .store
        .gradings_for(&pari, &s.student, &arg.id)
        .expect("il proprio giudizio sull'argomento");
    assert_eq!(sull_argomento.len(), 1);
    assert_eq!(sull_argomento[0].graded_by, pari);
    // E non può leggere il registro intero nemmeno passando dall'argomento
    // dell'altro giudizio.
    s.store
        .gradings_for(&pari, &s.student, &altro_arg.id)
        .expect_err("il giudizio del docente non è del pari");

    // Lo studente e il docente, invece, vedono tutto: le due righe della regola
    // non sono la stessa, e qui i due esiti coincidono.
    assert_eq!(
        s.store
            .student_gradings(&s.student, &s.student, &s.course)
            .expect("lo studente vede il proprio registro")
            .len(),
        2
    );
    assert_eq!(
        s.store
            .student_gradings(&s.teacher, &s.student, &s.course)
            .expect("il docente vede tutto il registro del corso")
            .len(),
        2
    );

    // Un altro pari non vede nulla: la difesa procedurale primaria è la
    // contestazione, e la privacy del valutato non è un dettaglio.
    let altro = PersonId::fixture(8);
    s.store
        .upsert_person(&crate::types::Person {
            id: altro.clone(),
            display_name: "Altro pari".to_string(),
            created_at: Millis(T0),
        })
        .expect("persona");
    s.store
        .student_gradings(&altro, &s.student, &s.course)
        .expect_err("un pari non vede il giudizio emesso da un altro pari");
}

#[test]
fn una_claim_si_ritira_cambiando_solo_lo_stato() {
    let mut s = School::new();
    let arg = s.published(1);
    let claim = Claim {
        id: "clm-1".to_string(),
        course: s.course.clone(),
        argument: arg.id.clone(),
        text: "La continuità è chiusa per composizione.".to_string(),
        span_anchor: Some("#continuita".to_string()),
        span_text: Some("La continuità è chiusa per composizione.".to_string()),
        status: ClaimStatus::Supported,
        emitted_at: Millis(T0),
        emitted_by: Emitter::Teacher {
            by: s.teacher.clone(),
        },
    };
    s.store.append_claim(&claim).expect("claim");

    // Il testo è congelato da un trigger, non da una promessa.
    let err = s
        .store
        .conn()
        .execute("UPDATE claims SET text = 'altra cosa' WHERE id = ?1", [&claim.id])
        .expect_err("il testo di una claim non si riscrive");
    assert!(err.to_string().contains("non si riscrive"), "{err}");

    s.store
        .retract_claim(&claim.id, "lo span non supporta l'affermazione")
        .expect("ritratto");
    let lette = s.store.claims_for(&s.teacher, &arg.id).expect("lettura");
    assert!(matches!(lette[0].status, ClaimStatus::Retracted { .. }));
    assert_eq!(lette[0].text, claim.text, "il testo è rimasto");
    assert_eq!(lette[0].span_text, claim.span_text, "lo span è rimasto");
}

#[test]
fn uno_span_senza_testo_non_entra() {
    let mut s = School::new();
    let arg = s.published(1);
    // Senza `span_text`, `span_anchor` è un indirizzo che il lettore non può
    // verificare: il vincolo è nel database, non in una validazione in codice che
    // qualcuno può dimenticare.
    let err = s
        .store
        .append_claim(&Claim {
            id: "clm-2".to_string(),
            course: s.course.clone(),
            argument: arg.id.clone(),
            text: "x".into(),
            span_anchor: Some("#da-qualche-parte".into()),
            span_text: None,
            status: ClaimStatus::Supported,
            emitted_at: Millis(T0),
            emitted_by: Emitter::Teacher {
                by: s.teacher.clone(),
            },
        })
        .expect_err("un indirizzo che il lettore non può verificare non è uno span");
    assert!(err.to_string().contains("CHECK constraint failed"), "{err}");

    s.store
        .retract_claim("clm-inesistente", "motivo")
        .expect_err("non c'è nulla da ritirare");
    s.store
        .retract_claim("clm-2", "  ")
        .expect_err("un ritratto senza motivo non è un ritratto");
}

#[test]
fn il_coorte_sotto_soglia_non_esiste_e_il_dato_individuale_resta() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);

    let sotto = CohortSignal {
        course: s.course.clone(),
        cohort: s.cohort(),
        argument: arg.id.clone(),
        failing: 4,
        total: 12,
        at: Millis(T0),
    };
    match s
        .store
        .record_cohort_signal(&sotto)
        .expect_err("sotto soglia non si registra")
    {
        Error::Invariant(kbs_core::Invariant::CohortBelowThreshold { failing, min }) => {
            assert_eq!((failing, min), (4, 5));
        }
        other => panic!("atteso CohortBelowThreshold, ottenuto {other:?}"),
    }
    assert!(
        s.store.cohort_signals(&arg.id).expect("lettura").is_empty(),
        "il segnale sotto soglia non si vede"
    );

    // Ma le osservazioni individuali ci sono: la soglia è un accesso
    // all'aggregato, non una scomparsa del dato.
    s.store
        .append_observation(
            &sessione,
            ObservationDraft {
                evidence: Evidence::Checked {
                    exercise: "ex-1".into(),
                    instance: "seed-9".into(),
                    correct: false,
                },
                ..observation(1, &s, &arg.id)
            },
        )
        .expect("osservazione");
    assert_eq!(
        s.store
            .observations_for(&s.teacher, &s.student, &arg.id)
            .expect("il docente vede la dimostrazione")
            .len(),
        1
    );
}

#[test]
fn il_segnale_di_coorte_conta_le_persone_e_confronta_il_numero() {
    // **Cinque tentativi di uno studente non sono cinque studenti.** Era esattamente
    // il buco: la soglia era chiusa, il `CHECK` del database pure, e nessuno dei
    // due guardava le righe. `UNIQUE` è su `(session_id, seq)`, quindi i cinque
    // tentativi erano perfettamente conformi e il segnale che diceva alla classe
    // «cinque studenti sbagliano questo argomento» era il lavoro di uno.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for tentativo in 1..=5u32 {
        s.store
            .append_observation(
                &sessione,
                ObservationDraft {
                    id: format!("obs-tentativo-{tentativo}"),
                    evidence: Evidence::Checked {
                        exercise: "ex-1".into(),
                        instance: format!("seed-{tentativo}"),
                        correct: false,
                    },
                    ..observation(tentativo, &s, &arg.id)
                },
            )
            .expect("tentativo");
    }
    let dichiarato = CohortSignal {
        course: s.course.clone(),
        cohort: s.cohort(),
        argument: arg.id.clone(),
        failing: 5,
        total: 5,
        at: Millis(T0),
    };
    match s
        .store
        .record_cohort_signal(&dichiarato)
        .expect_err("una persona sola non è una classe")
    {
        Error::CohortCountMismatch {
            declared_failing,
            counted_failing,
            declared_total,
            counted_total,
        } => {
            assert_eq!(declared_failing, 5, "quello che il chiamante dichiarava");
            assert_eq!(declared_total, 5);
            assert_eq!(counted_failing, 1, "quello che le righe dicono: una persona");
            assert_eq!(counted_total, 1, "e `total` è lo stesso conteggio: chi ha una riga");
        }
        other => panic!("atteso CohortCountMismatch, ottenuto {other:?}"),
    }
    assert!(
        s.store.cohort_signals(&arg.id).expect("lettura").is_empty(),
        "un segnale che non torna con le righe non entra"
    );

    // Ora il numero torna davvero: cinque persone diverse, un tentativo ciascuna, e
    // il segnale entra con quei numeri e non con altri.
    for n in 0..5u32 {
        let altro = PersonId::fixture(20 + n);
        s.store
            .upsert_person(&crate::types::Person {
                id: altro.clone(),
                display_name: format!("Compagno {n}"),
                created_at: Millis(T0),
            })
            .expect("persona");
        s.store
            .append_observation(
                &sessione,
                ObservationDraft {
                    id: format!("obs-compagno-{n}"),
                    student: altro,
                    evidence: Evidence::Checked {
                        exercise: "ex-1".into(),
                        instance: format!("seed-compagno-{n}"),
                        correct: false,
                    },
                    ..observation(100 + n, &s, &arg.id)
                },
            )
            .expect("tentativo del compagno");
    }
    // Una sesta persona che risponde bene non conta fra chi sbaglia: `failing`
    // è un conteggio di persone in errore, `total` di persone che ci hanno
    // lavorato sopra.
    let bravo = PersonId::fixture(40);
    s.store
        .upsert_person(&crate::types::Person {
            id: bravo.clone(),
            display_name: "Chi ha risposto bene".to_string(),
            created_at: Millis(T0),
        })
        .expect("persona");
    s.store
        .append_observation(
            &sessione,
            ObservationDraft {
                id: "obs-bravo".into(),
                student: bravo,
                evidence: Evidence::Checked {
                    exercise: "ex-1".into(),
                    instance: "seed-bravo".into(),
                    correct: true,
                },
                ..observation(200, &s, &arg.id)
            },
        )
        .expect("tentativo corretto");

    s.store
        .record_cohort_signal(&CohortSignal {
            failing: 6,
            total: 7,
            ..dichiarato.clone()
        })
        .expect("alla soglia, e con i numeri che le righe dicono");
    let letti = s.store.cohort_signals(&arg.id).expect("lettura");
    assert_eq!(letti.len(), 1);
    assert_eq!(letti[0].failing, 6, "sei persone, non sei righe: i 5 tentativi sono di uno");
    assert_eq!(letti[0].total, 7, "chi ha prodotto qualcosa: sette persone, compresa chi ha risposto bene");

    // E un numero che torna a metà non entra: il confronto è sui due numeri insieme.
    s.store
        .record_cohort_signal(&CohortSignal {
            failing: 6,
            total: 5,
            ..dichiarato
        })
        .expect_err("`failing <= total` è garantito dal conteggio, non dalla promessa");
}

#[test]
fn la_soglia_del_database_e_quella_del_dominio() {
    // La soglia è scritta in due posti: `kbs_core::COHORT_MIN_K` e il CHECK della
    // migrazione. Se divergono, questo test fallisce, e la correzione è una
    // migrazione nuova: una migrazione applicata non si riscrive.
    let conn = rusqlite::Connection::open_in_memory().expect("memoria");
    // La migrazione viene applicata da sola, quindi le uniche cose che le
    // mancano sono quelle delle migrazioni precedenti di cui si usa: qui
    // `schema_epoch`. Il CHECK della soglia è ciò che il test prova, e il
    // resto della migrazione èrumore che deve applicarsi pulito.
    conn.execute_batch(
        "CREATE TABLE schema_epoch (id INTEGER NOT NULL PRIMARY KEY, epoch INTEGER NOT NULL); \
         INSERT INTO schema_epoch (id, epoch) VALUES (1, 1); \
         CREATE TABLE sources (id TEXT NOT NULL PRIMARY KEY) STRICT; \
         CREATE TABLE arguments (id TEXT NOT NULL PRIMARY KEY) STRICT; \
         CREATE TABLE people (id TEXT NOT NULL PRIMARY KEY) STRICT; \
         INSERT INTO sources (id) VALUES ('course_0001'); \
         INSERT INTO arguments (id) VALUES ('arg_0001');",
    )
    .expect("schema minimo");
    conn.execute_batch(include_str!(
        "../../migrations/V4__esercizi_e_generazioni.sql"
    ))
    .expect("migrazione");
    let err = conn
        .execute(
            "INSERT INTO cohort_signals (course_id, cohort, argument_id, failing, total, at) \
             VALUES ('course_0001', '2A', 'arg_0001', 4, 10, 0)",
            [],
        )
        .expect_err("la soglia è nel CHECK");
    assert!(err.to_string().contains("CHECK constraint failed"), "{err}");

    // E a k=5 entra, e a k=4 no: il bordo è provato dai due lati.
    conn.execute(
        "INSERT INTO cohort_signals (course_id, cohort, argument_id, failing, total, at) \
         VALUES ('course_0001', '2A', 'arg_0001', ?1, 10, 0)",
        [kbs_core::COHORT_MIN_K as i64],
    )
    .expect("alla soglia esatta entra");
    let err = conn
        .execute(
            "INSERT INTO cohort_signals (course_id, cohort, argument_id, failing, total, at) \
             VALUES ('course_0001', '2B', 'arg_0001', 4, 10, 0)",
            [],
        )
        .expect_err("un punto sotto la soglia no");
    assert!(err.to_string().contains("CHECK constraint failed"));
}

#[test]
fn due_id_di_registro_non_sono_la_stessa_riga() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, observation(1, &s, &arg.id))
        .expect("prima");
    let err = s
        .store
        .append_observation(&sessione, observation(1, &s, &arg.id))
        .expect_err("lo stesso id due volte");
    // Non un errore SQLite grezzo: un errore che nomina la regola rotta.
    assert_eq!(err.rule(), "lookup.duplicate");
}

#[test]
fn un_id_inesistente_qui_non_e_un_panico() {
    let mut s = School::new();
    let err = s
        .store
        .append_claim(&Claim {
            id: "clm-x".to_string(),
            course: s.course.clone(),
            argument: kbs_core::ArgumentId::from_rel_path("no/esiste.html"),
            text: "x".into(),
            span_anchor: None,
            span_text: None,
            status: ClaimStatus::Supported,
            emitted_at: Millis(T0),
            emitted_by: Emitter::Teacher {
                by: PersonId::fixture(7),
            },
        })
        .expect_err("la persona e l'argomento non esistono");
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

#[test]
fn una_sessione_si_legge_per_intera_o_non_si_legge() {
    // La firma non aveva una persona, e `SessionId` è un newtype trasparente su
    // `String` costruito da `ses_<registro>_<millisecondi>_<lunghezza della nota>`:
    // enumerabile. Questo test è quello che manca prima della correzione, e non è
    // una formalità: senza il predicato la risposta conteneva `student`,
    // `course_id`, `cohort`, `evidence` e `judged_by` di tutti, in tutti i corsi.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, observation(1, &s, &arg.id))
        .expect("osservazione");

    // Chi insegna il corso della sessione la legge per intera: è la destinazione
    // delle righe, che sono l'input di `kbs_verify::Chain`.
    let lette = s
        .store
        .observations_in_session(&s.teacher, &sessione)
        .expect("il docente legge la sessione del proprio corso");
    assert_eq!(lette.len(), 1);
    assert_eq!(lette[0].student, s.student);

    // Lo studente è iscritto al corso e non insegna: la sessione non è sua.
    s.store
        .observations_in_session(&s.student, &sessione)
        .expect_err("lo studente non legge una sessione che non è sua");
    // E non leggere una sessione non rende leggibili le righe singole: le due
    // strade hanno soggetti diversi e lo dichiarano.
    s.store
        .observations_in_session(&s.other_teacher, &sessione)
        .expect_err("un docente di un altro corso non legge la sessione");

    // Una sessione di un altro corso: la firma accetta l'id di chiunque, e la
    // risposta è la stessa — `[]` — per una sessione che non esiste e per una
    // vuota, quindi enumerare id non dà nulla.
    let vuota = s.session(Register::Observations);
    assert!(
        s.store
            .observations_in_session(&s.student, &vuota)
            .expect("una sessione vuota è un elenco vuoto")
            .is_empty()
    );
    assert!(
        s.store
            .observations_in_session(&s.student, &kbs_verify::SessionId::new("ses_inesistente_0_0"))
            .expect("una sessione inesistente è un elenco vuoto")
            .is_empty()
    );
}

#[test]
fn il_checker_e_le_risposte_sono_del_docente_e_non_dello_studente() {
    let mut s = School::new();
    let arg = s.published(1);
    let bozza = s.draft(2);
    let esercizio = kbs_core::Exercise {
        id: "ex-1".into(),
        course: s.course.clone(),
        argument: arg.id.clone(),
        family: "algebra-polinomio".into(),
        generator_version: "kbs-exercise/1".into(),
        prompt: "Sviluppa il prodotto e mettilo in forma normale.".into(),
        // Il checker **è** la risposta: `Set { elements }` è l'insieme delle
        // risposte corrette, non una pista.
        checker: kbs_core::Checker::Set {
            elements: vec!["x2+2x+1".into()],
        },
        created_at: Millis(T0),
        created_by: s.teacher.clone(),
    };
    s.store.upsert_exercise(&esercizio).expect("esercizio");
    s.store
        .put_instance(&kbs_core::Instance {
            exercise: "ex-1".into(),
            seed: "seed-1".into(),
            rendered_prompt: "Sviluppa (x+1)(x+1).".into(),
            expected: "x2+2x+1".into(),
            params: serde_json::json!({}),
        })
        .expect("istanza");
    let esercizio_su_bozza = kbs_core::Exercise {
        id: "ex-2".into(),
        argument: bozza.id.clone(),
        ..esercizio.clone()
    };
    s.store
        .upsert_exercise(&esercizio_su_bozza)
        .expect("esercizio su bozza");

    // Il docente che insegna legge entrambi i lati: il checker e le risposte.
    let letto = s
        .store
        .exercise(&s.teacher, &s.course, "ex-1")
        .expect("il docente legge il proprio esercizio");
    assert_eq!(letto.checker, esercizio.checker);
    let istanze = s
        .store
        .instances_of(&s.teacher, &s.course, "ex-1")
        .expect("il docente legge le istanze");
    assert_eq!(istanze.len(), 1);
    assert_eq!(istanze[0].expected, "x2+2x+1");

    // **Uno studente iscritto al corso, con l'argomento in corso, non ottiene
    // niente.** È la riga che chiude il buco: `may_read` da solo aprirebbe
    // (l'argomento è in corso e lo studente è iscritto), e la risposta sarebbe a
    // una chiamata di distanza. D8 dice che l'integrità è per costruzione perché
    // la risposta non è nel materiale che lo studente vede.
    s.store
        .exercise(&s.student, &s.course, "ex-1")
        .expect_err("lo studente non legge il checker di un argomento in corso");
    s.store
        .instances_of(&s.student, &s.course, "ex-1")
        .expect_err("lo studente non legge le risposte");

    // Un corso che non si insegna: nemmeno l'id dell'esercizio è la risposta,
    // perché il rifiuto non distingue «non c'è» da «non è tuo».
    s.store
        .exercise(&s.other_teacher, &s.other_course, "ex-1")
        .expect_err("un docente di un altro corso non legge l'esercizio");
    s.store
        .exercise(&s.teacher, &s.other_course, "ex-1")
        .expect_err("il corso dichiarato non combacia con quello della riga");
    s.store
        .exercise(&s.teacher, &s.course, "ex-inesistente")
        .expect_err("un id che non esiste dà lo stesso rifiuto");

    // E l'esercizio su una bozza resta del docente, anche per il docente: la bozza
    // non è materiale che si legge da solo, e il predicato passa dall'argomento.
    let su_bozza = s
        .store
        .exercise(&s.teacher, &s.course, "ex-2")
        .expect("il docente vede il proprio esercizio anche in bozza");
    assert_eq!(su_bozza.argument, bozza.id);
    s.store
        .exercise(&s.student, &s.course, "ex-2")
        .expect_err("uno studente non vede un esercizio su una bozza");
}

#[test]
fn la_rubrica_e_lo_strumento_del_docente() {
    let mut s = School::new();
    let _ = rubric(&mut s);
    // Il docente che insegna legge la scala.
    let letta = s
        .store
        .rubric_version(&s.teacher, "rub-1@1")
        .expect("rubrica");
    assert_eq!(letta.scale.len(), 2);
    // Lo studente no: la scala è lo strumento con cui il docente misura, e chi
    // non insegna non misura.
    s.store
        .rubric_version(&s.student, "rub-1@1")
        .expect_err("uno studente non legge la scala di valutazione");
    s.store
        .rubric_version(&s.other_teacher, "rub-1@1")
        .expect_err("un docente di un altro corso non legge la rubrica");
    // E l'id che non esiste dà **lo stesso** errore: se dicesse `None`, la firma
    // avrebbe due risposte e una delle due sarebbe un canale per imparare che
    // cosa c'è in una scuola.
    // Le due domande sono poste dalla **stessa persona**: è l'unico confronto che
    // ha senso, perché il messaggio nomina anche chi chiede.
    let inesistente = s
        .store
        .rubric_version(&s.student, "rub-inesistente@1")
        .expect_err("un id che non esiste dà un rifiuto");
    let non_mia = s
        .store
        .rubric_version(&s.student, "rub-1@1")
        .expect_err("la rubrica che non è tua dà lo stesso rifiuto");
    let testo = |e: &crate::Error| {
        e.to_string()
            .split_whitespace()
            .filter(|w| !w.starts_with("rubrica:"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(testo(&inesistente), testo(&non_mia));
}
