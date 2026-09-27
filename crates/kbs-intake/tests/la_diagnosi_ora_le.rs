//! La diagnosi orale: l'unico giudizio umano che il sistema ha, e il buco che
//! ne nasce.
//!
//! `kbs_core::Evidence::Oral` esiste perché la diagnosi **non è
//! riproducibile** e va detto. Qui si verifica che entri nel registro come una
//! specie distinta di fonte, che il materiale generato possa citarla, e —
//! quello che conta di più — che il buco `generations.senza-causa` sia
//! **dichiarato** e non ricoperto da un campo di testo.

mod common;

use kbs_core::{CohortId, Evidence, GraderKind, Millis};
use kbs_intake::diagnosis::{self, Diagnosis};
use kbs_intake::prompt::GenerationRequest;
use kbs_intake::route::{self, Request, Route};
use kbs_store::Store;

use common::*;

const MARCO: &str = "person_0007";
const CORSO_ID: &str = "matematica-terza";
const COORTE: &str = "2026-terza";

fn diagnosi() -> Diagnosis {
    Diagnosis {
        id: String::new(),
        student: kbs_core::PersonId(MARCO.to_string()),
        course: kbs_core::CourseId(CORSO_ID.to_string()),
        cohort: CohortId(COORTE.to_string()),
        argument: kbs_core::ArgumentId::from_rel_path(REL),
        note: "Marco scrive 3/6 e 2/4 uguali ma 2/6 no: non distingue il denominatore".into(),
        witness: Some(kbs_core::PersonId::fixture(2)),
        at: Millis(1_757_000_000_000),
    }
    .with_derived_id()
}

/// Un negozio con la diagnosi registrata e l'argomento su cui verte.
fn store_con_diagnosi() -> Store {
    let mut s = store_con_corso();
    s.upsert_person(&kbs_store::Person {
        id: kbs_core::PersonId(MARCO.to_string()),
        display_name: "Marco".into(),
        created_at: Millis(0),
    })
    .unwrap();
    s.upsert_person(&kbs_store::Person {
        id: kbs_core::PersonId::fixture(2),
        display_name: "collega".into(),
        created_at: Millis(0),
    })
    .unwrap();
    route::receive(
        &mut s,
        Request {
            route: Route::File,
            by: docente(),
            course: Some(corso()),
            rel_path: Some(REL.to_string()),
            source: artifact(false),
        },
    )
    .expect("l'argomento di cui si parla entra");
    let osservazione =
        diagnosis::registra_in_una_sessione(&mut s, diagnosi(), "interrogazione del 14").unwrap();
    assert!(!osservazione.id.is_empty());
    s
}

#[test]
fn la_diagnosi_entra_nel_registro_come_una_specie_distinta_di_prova() {
    let s = store_con_diagnosi();
    let argomento = kbs_core::ArgumentId::from_rel_path(REL);
    let osservazioni =
        s.observations_for(&docente(), &kbs_core::PersonId(MARCO.to_string()), &argomento).unwrap();
    assert_eq!(osservazioni.len(), 1);
    let o = &osservazioni[0];
    match &o.evidence {
        Evidence::Oral { note, witness } => {
            assert!(note.contains("non distingue il denominatore"));
            assert_eq!(witness, &Some(kbs_core::PersonId::fixture(2)));
        }
        altro => panic!("la prova deve essere orale, trovata {altro:?}"),
    }
    assert!(!o.evidence.is_reproducible(), "D6: e' dichiarato che non si riproduce");
    assert_eq!(o.judged_by, Some(GraderKind::Teacher), "l'ha visto un docente");
    assert_eq!(o.student, kbs_core::PersonId(MARCO.to_string()));
    assert_eq!(o.cohort, CohortId(COORTE.to_string()));
}

#[test]
fn il_seq_l_assigna_il_registro_e_riparte_da_uno_a_ogni_sessione() {
    // `SeqInSession` sta **dentro la sua sessione**: due osservazioni nella
    // stessa sessione hanno seq 1 e 2, e la seconda sessione ricomincia da 1.
    // Non e' un difetto, e' la ragione per cui la catena di hash di D6 e' per
    // sessione e ha un testimone: un seq 1 di una sessione e un seq 1 di
    // un'altra non sono la stessa posizione, e trattarli come se lo fossero
    // produrrebbe una catena che non ordina niente.
    let mut s = store_con_diagnosi();
    let sessione = s.open_session(kbs_store::Register::Observations, "terza").unwrap();
    let prima = diagnosis::registra(&mut s, &sessione, Diagnosis { id: "obs_a".into(), ..diagnosi() }).unwrap();
    let seconda = diagnosis::registra(&mut s, &sessione, Diagnosis { id: "obs_b".into(), ..diagnosi() }).unwrap();
    assert_eq!(prima.seq.0, 1);
    assert_eq!(seconda.seq.0, 2);
    s.close_session(&sessione).unwrap();

    let altra = s.open_session(kbs_store::Register::Observations, "quarta").unwrap();
    let terza = diagnosis::registra(&mut s, &altra, Diagnosis { id: "obs_c".into(), ..diagnosi() }).unwrap();
    assert_eq!(terza.seq.0, 1, "una sessione nuova ricomincia");
    s.close_session(&altra).unwrap();

    // E la catena copre quello che dice di coprire: le righe della sessione,
    // nell'ordine del seq, non nell'ordine di arrivo.
    // Il registro delle osservazioni è gated: si legge con una persona, e la
    // persona che ha scritto la sessione è quella che può rileggerla. Non è
    // una restrizione che il test aggira: è la regola, e il test la esercita
    // per quello che è.
    let righe = s.observations_in_session(&docente(), &altra).unwrap();
    assert_eq!(righe.len(), 1);
    assert_eq!(righe[0].seq, terza.seq);
}

#[test]
fn il_materiale_generato_pu_o_citare_la_diagnosi() {
    let s = store_con_diagnosi();
    let riferimento = diagnosis::referenzia(
        &s,
        &docente(),
        &kbs_core::PersonId(MARCO.to_string()),
        &kbs_core::ArgumentId::from_rel_path(REL),
        &diagnosi().id,
    )
    .expect("la diagnosi e' nel registro");
    let richiesta = GenerationRequest::try_new(
        "claude-sonnet-4-5",
        kbs_intake::GENERATOR_VERSION,
        kbs_intake::CorpusHash::of_empty(),
        "sistema",
        "compito",
        "excerpt",
        "materiale sulle frazioni",
    )
    .unwrap()
    .with_diagnosis(riferimento.clone());
    // Il legame sta nei **byte del prompt**, quindi dentro l'hash: verificabile.
    assert!(richiesta
        .canonical_bytes()
        .contains(&format!("diagnosis-id: {}", riferimento.id)));
    assert!(richiesta.canonical_bytes().contains("diagnosis-witness: person_0002"));
    let senza = GenerationRequest::try_new(
        "claude-sonnet-4-5",
        kbs_intake::GENERATOR_VERSION,
        kbs_intake::CorpusHash::of_empty(),
        "sistema",
        "compito",
        "excerpt",
        "materiale sulle frazioni",
    )
    .unwrap();
    assert_ne!(
        senza.prompt_hash(),
        richiesta.prompt_hash(),
        "senza la diagnosi il lock e' un altro lock"
    );
}

#[test]
fn un_riferimento_a_una_diagnosi_inesistente_e_rifiutato() {
    let s = store_con_diagnosi();
    let e = diagnosis::referenzia(
        &s,
        &docente(),
        &kbs_core::PersonId(MARCO.to_string()),
        &kbs_core::ArgumentId::from_rel_path(REL),
        "obs_non_esiste",
    )
    .expect_err("una diagnosi inesistente non si cita");
    assert!(matches!(e, kbs_intake::Error::DiagnosiAssente { .. }), "{e}");
}

#[test]
fn la_diagnosi_e_idempotente_perche_l_id_deriva_dal_contenuto() {
    let a = diagnosi();
    let b = diagnosi();
    assert_eq!(a.id, b.id, "la stessa osservazione due volte e' la stessa");
    let mut altro = diagnosi();
    altro.note = "una frase diversa".into();
    assert_ne!(altro.with_derived_id().id, a.id);
}

#[test]
fn una_diagnosi_senza_nota_non_entra() {
    let mut s = store_con_corso();
    let sessione = s.open_session(kbs_store::Register::Observations, "vuota").unwrap();
    let e = diagnosis::registra(
        &mut s,
        &sessione,
        Diagnosis { note: "   ".into(), ..diagnosi() },
    )
    .expect_err("una diagnosi senza nota non e' una diagnosi");
    assert!(matches!(e, kbs_intake::Error::LockIncompleto { campo: "note" }), "{e}");
}

#[test]
fn il_buco_e_dichiarato_e_non_e_stato_riempito_con_un_campo_di_testo() {
    // Questo test non puo' dimostrare l'assenza di una colonna senza toccare lo
    // schema, che non e' di questo crate. Dimostra la cosa che puo': il
    // riferimento alla diagnosi passa **dentro l'hash** e da nessuna parte
    // altrove, e il tipo che lo trasporta non ha un campo libero in cui
    // scrivere «motivo: Marco non distingue…». Se qualcuno aggiungesse quel
    // campo per schivare il buco, questo test comincerebbe a mentire.
    let riferimento = kbs_intake::DiagnosisRef::new("obs_x", "nota", None);
    let json = serde_json::to_value(&riferimento).unwrap();
    let chiavi: Vec<&String> = json.as_object().unwrap().keys().collect();
    assert_eq!(chiavi, ["id", "note", "witness"], "il riferimento ha tre campi e non ne aggiunge");
}
