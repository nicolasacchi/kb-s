//! D6 attraverso il database: la catena di hash costruita su ciò che **esce da
//! SQLite** deve essere la stessa di quella costruita in memoria.
//!
//! `lib.rs` promette che `observations_in_session` restituisce
//! `kbs_core::Observation` nell'ordine del `seq`, «così `kbs_verify::leaf_of`
//! lavora sul tipo di dominio senza conversioni». È la promessa più importante
//! che questo crate faccia sulla faccia di `kbs-verify`, e fino a prima di questo
//! test **nessun test la attraversava**: ogni test di D6 costruiva le righe a
//! mano (`kbs-verify/tests/contract.rs`) o le leggeva da una sessione già in
//! memoria.
//!
//! Il buco che lascia è grossolane e invisibile: se `codec` perdesse
//! `evidence_payload`, normalizzasse `at` o cambiasse il discriminante di
//! `Evidence` fra la scrittura e la lettura, la catena costruita sulle righe
//! lette divergerebbe da quella in memoria — e **tutti i test di D6 resterebbero
//! verdi**, perché nessuno di loro attraversa il database.
//!
//! # Perché si confrontano i byte canonici e non i valori tipizzati
//!
//! `leaf = SHA256(0x00 ‖ canonical_json(riga))`: la catena nonDepends dai valori
//! di Rust, dipende dalla loro **forma canonica**. Due `Observation` uguali come
//! valori ma serializzate in modo diverso — chiavi in ordine diverso, un `1.0`
//! contro un `1`, un intero contro una stringa — sono due foglie diverse. Un
//! confronto di `PartialEq` passerebbe, un confronto di byte no, ed è il confronto
//! di byte quello che la catena guarda.

use kbs_core::{ArgumentId, CohortId, Evidence, GraderKind, Millis, Observation};
use kbs_verify::{Chain, SegmentPlan, canonical_of};

use super::School;
use crate::types::{ObservationDraft, Register};

const T0: i64 = 1_700_000_000_000;

/// Una riga per ciascuna forma di prova, e con e senza giudicatore.
///
/// La varietà non è decorativa: `Evidence` è un enum con quattro varianti e
/// `judged_by` è opzionale, e sono esattamente i campi che un codec può
/// trasformare senza che nessuno se ne accorga. `Oral` porta un `witness` che
/// attraversa il payload JSON, ed è la riga che crollerebbe per prima se il
/// payload non tornasse.
fn riga(n: u32, s: &School, argomento: &ArgumentId) -> ObservationDraft {
    let evidence = match n % 4 {
        0 => Evidence::Checked {
            exercise: "ex-1".into(),
            instance: format!("seed-{n}"),
            correct: n % 8 == 0,
        },
        1 => Evidence::Oral {
            note: format!("interrogazione numero {n}"),
            witness: Some(s.teacher.clone()),
        },
        2 => Evidence::Written {
            ref_doc: format!("compito-{n}.pdf"),
        },
        _ => Evidence::None,
    };
    ObservationDraft {
        id: format!("obs-catena-{n:03}"),
        student: s.student.clone(),
        course: s.course.clone(),
        cohort: CohortId("2A".into()),
        argument: argomento.clone(),
        evidence,
        judged_by: match n % 3 {
            0 => None,
            1 => Some(GraderKind::Deterministic),
            _ => Some(GraderKind::Teacher),
        },
        at: Millis(T0 + n as i64),
    }
}

#[test]
fn la_catena_sulle_righe_riavviate_e_la_stessa_di_quella_in_memoria() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);

    // Nove righe: tre segmenti da tre, così l'albero ha più di un livello e una
    // perdita di una riga sposterebbe la testa invece di accorciarla.
    let in_memoria: Vec<Observation> = (1..=9)
        .map(|n| {
            s.store
                .append_observation(&sessione, riga(n, &s, &arg.id))
                .expect("osservazione")
        })
        .collect();
    assert_eq!(in_memoria.len(), 9);

    let rilette = s
        .store
        .observations_in_session(&s.teacher, &sessione)
        .expect("rilettura della sessione");
    assert_eq!(rilette.len(), in_memoria.len(), "nessuna riga perduta o allungata");

    // 1. I **byte canonici** coincidono, riga per riga. È il confronto di cui
    //    parla il doc del modulo: la catena non vede `Observation`, vede la sua
    //    forma canonica.
    for (i, (in_memoria, riletta)) in in_memoria.iter().zip(rilette.iter()).enumerate() {
        let a = canonical_of(in_memoria).expect("forma canonica in memoria");
        let b = canonical_of(riletta).expect("forma canonica riletta");
        assert_eq!(a, b, "riga {i}: la forma canonica cambia andando e tornando");
    }
    // E non è una forma vuota: se il codec perdesse il payload, i due lati
    // divergerebbero, ma è bene che il test dica anche che i byte confrontati
    // contengono davvero la prova.
    // `rilette[0]` è la prima riga, `n = 1`: è quella con la prova orale, che è
    // l'unica che porta dentro `note` e `witness` — cioè l'unica che crollerebbe
    // per prima se il payload non facesse il giro di SQLite.
    let campione = canonical_of(&rilette[0]).expect("forma canonica");
    assert!(
        campione.contains("interrogazione numero 1") && campione.contains("person_0001"),
        "il payload della prova orale non è passato: {campione}"
    );

    // 2. La **catena** coincide: stessa testa, stesso piano di segmenti.
    let piano = SegmentPlan::every(3);
    let catena_memoria = Chain::build(&sessione, &in_memoria, piano).expect("catena in memoria");
    let catena_riavviata = Chain::build(&sessione, &rilette, piano).expect("catena riavviata");
    assert_eq!(
        catena_memoria.head(),
        catena_riavviata.head(),
        "la catena costruita sulle righe lette dal database è un'altra catena"
    );

    // 3. E la stessa cosa vale **riaprendo il file**: la promessa di `lib.rs` è
    //    sul dato che esce dal database, non su quello che è rimasto nel
    //    processo che l'ha scritto.
    let riaperta = s.reopen();
    let rilette_due = riaperta
        .observations_in_session(&s.teacher, &sessione)
        .expect("rilettura dopo riapertura");
    let catena_riaperta = Chain::build(&sessione, &rilette_due, piano).expect("catena riaperta");
    assert_eq!(
        catena_riavviata.head(),
        catena_riaperta.head(),
        "riaprire il database non può cambiare la testa"
    );
}

#[test]
fn una_riga_persa_sposta_la_testa_e_un_test_che_lo_dice() {
    // **Il contro-prova del test sopra.** Se la catena non dipendesse dalle righe,
    // il confronto delle teste passerebbe anche con un bridge rotto: togliere una
    // riga deve spostare la testa, e una riga cambiata deve spostarla. Senza
    // questa riga, «le due teste sono uguali» potrebbe voler dire «le due catene
    // sono entrambe costanti».
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    let righe: Vec<Observation> = (1..=5)
        .map(|n| {
            s.store
                .append_observation(&sessione, riga(n, &s, &arg.id))
                .expect("osservazione")
        })
        .collect();
    let piano = SegmentPlan::every(2);
    let intera = Chain::build(&sessione, &righe, piano).expect("catena intera");
    let senza_ultima = Chain::build(&sessione, &righe[..4], piano).expect("catena corta");
    assert_ne!(
        intera.head(),
        senza_ultima.head(),
        "una riga in meno deve cambiare la testa"
    );
    let mut cambiata = righe.clone();
    cambiata[2].at = Millis(T0 + 9_999);
    assert_ne!(
        intera.head(),
        Chain::build(&sessione, &cambiata, piano)
            .expect("catena cambiata")
            .head(),
        "un campo cambiato deve cambiare la testa"
    );
}
