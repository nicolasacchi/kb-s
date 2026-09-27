//! D12: l'uscita a colonne fisse.
//!
//! Il contratto di questa sezione è che l'esportazione è **proponibile**: chi
//! riceve il file sa chi ha scritto il materiale, con quale modello, e chi si è
//! preso la responsabilità di ratificarlo. Un export di sole unità didattiche,
//! in Italia, è un export di materiale scolastico senza autore.

use kbs_core::{Argument, Millis, Origin, PublicationState};

use super::School;
use crate::FIXED_COLUMNS;

fn righe(esportazione: &str) -> Vec<Vec<String>> {
    esportazione
        .trim_end_matches('\n')
        .split('\n')
        .map(|riga| riga.split('\t').map(str::to_string).collect())
        .collect()
}

fn colonna(riga: &[String], nome: &str) -> String {
    let i = FIXED_COLUMNS.iter().position(|c| *c == nome).expect("colonna");
    riga[i].clone()
}

#[test]
fn l_intestazione_e_l_insieme_dichiarato_delle_colonne() {
    let mut s = School::new();
    s.published(1);
    let tsv = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");

    let righe = righe(&tsv);
    let intestazione = &righe[0];
    assert_eq!(intestazione, &FIXED_COLUMNS.to_vec());
    assert_eq!(tsv.lines().count(), 2, "una riga di dati");
}

#[test]
fn ogni_riga_ha_esattamente_tante_colonne_quante_dichiarate() {
    let mut s = School::new();
    // Un titolo con tab e acapo dentro: l'escape è ciò che tiene la forma fissa.
    let argomento = Argument {
        title: "Continuità\tsu R\nriga due".to_string(),
        summary: "Due\tpunti".to_string(),
        ..School::argument(&s.course, &s.teacher, 1, PublicationState::Bozza)
    };
    s.store.upsert_argument(&argomento).expect("bozza");
    s.store
        .ratify(&argomento.id, &s.teacher, "verificato")
        .expect("ratifica");
    s.store.publish(&argomento.id).expect("pubblicazione");
    s.published(2);

    let tsv = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    let righe = righe(&tsv);
    for riga in &righe {
        assert_eq!(
            riga.len(),
            FIXED_COLUMNS.len(),
            "riga con {riga:?}"
        );
    }
    // Il tab dentro il titolo non ha spostato nessuna colonna.
    let prima = &righe[1];
    assert_eq!(prima.len(), FIXED_COLUMNS.len());
    assert!(prima.iter().all(|c| !c.contains('\t')));
}

#[test]
fn una_riga_porta_la_provenienza_e_la_ratifica() {
    let mut s = School::new();
    let lock = School::lock();
    let generato = Argument {
        origin: Origin::Generated {
            lock: lock.clone(),
            by: s.teacher.clone(),
            at: Millis(1_700_000_000_000),
        },
        ..School::argument(&s.course, &s.teacher, 1, PublicationState::Bozza)
    };
    s.store.upsert_argument(&generato).expect("generato");
    s.store
        .record_generation(&crate::types::GenerationEvent {
            lock,
            argument: generato.id.clone(),
            requester: s.teacher.clone(),
            at: Millis(1_700_000_000_001),
        })
        .expect("evento");
    s.store
        .ratify(&generato.id, &s.teacher, "verificato riga per riga")
        .expect("ratifica");
    s.store.publish(&generato.id).expect("pubblicazione");

    let tsv = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    let riga = &righe(&tsv)[1];
    assert_eq!(colonna(riga, "course"), s.course.0);
    assert_eq!(colonna(riga, "slug"), "analisi-1");
    assert_eq!(colonna(riga, "argument"), generato.id.0);
    assert_eq!(colonna(riga, "rel_path"), generato.rel_path.clone().unwrap());
    assert_eq!(colonna(riga, "state"), "in-corso");
    assert_eq!(colonna(riga, "origin_kind"), "generated");
    assert_eq!(colonna(riga, "origin_model"), "claude-sonnet-4-5");
    assert_eq!(colonna(riga, "origin_prompt_hash"), "ph");
    assert_eq!(colonna(riga, "origin_corpus_hash"), "ch");
    assert_eq!(colonna(riga, "origin_generator_version"), "kbs-doc/1");
    assert_eq!(colonna(riga, "ratified_by"), s.teacher.0);
    assert_eq!(colonna(riga, "ratified_note"), "verificato riga per riga");
    assert_eq!(colonna(riga, "ratified_contract_hash"), generato.content_hash);
    assert_eq!(colonna(riga, "content_hash"), generato.content_hash);
}

#[test]
fn l_export_contiene_solo_che_e_citabile() {
    let mut s = School::new();
    s.draft(1); // bozza: invisibile all'export
    let vivo = s.published(2);
    s.store.archive(&vivo.id).expect("archiviato");
    let tsv = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    assert_eq!(
        righe(&tsv).len(),
        1,
        "solo l'intestazione: bozza e archiviato non sono citabili"
    );

    // E un argomento pubblicato e poi modificato esce di nuovo: la ratifica vale
    // per l'hash di allora, non per quello di adesso, e «è in corso» non basta
    // per essere citabile.
    let terzo = s.published(3);
    let modificato = Argument {
        content_hash: "sha256:cambiato".into(),
        updated_at: Millis(1_700_000_300_000),
        ..terzo
    };
    s.store.upsert_argument(&modificato).expect("modifica");
    let tsv = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    assert_eq!(righe(&tsv).len(), 1, "una ratifica invecchiata non esporta");
}

#[test]
fn l_export_e_riproducibile_byte_per_byte() {
    let mut s = School::new();
    s.published(1);
    s.published(2);
    s.published(3);
    let prima = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    let seconda = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    assert_eq!(prima, seconda);

    // E l'ordine è per id, non per ordine di inserimento: due database con lo
    // stesso contenuto danno lo stesso file.
    let righe = righe(&prima);
    let ids: Vec<String> = righe[1..].iter().map(|r| colonna(r, "argument")).collect();
    let mut ordinata = ids.clone();
    ordinata.sort();
    assert_eq!(ids, ordinata);
}

#[test]
fn l_export_lo_fa_il_docente_e_non_lo_fa_uno_studente() {
    let mut s = School::new();
    s.published(1);
    let err = s
        .store
        .export_fixed_columns(&s.student, &s.course)
        .expect_err("uno studente non esporta il corso");
    assert_eq!(err.rule(), "visibility");
    assert!(
        s.store
            .export_fixed_columns(&s.other_teacher, &s.course)
            .expect_err("un docente di un altro corso nemmeno")
            .to_string()
            .contains("non insegna")
    );
    assert!(s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("il docente esporta")
        .contains("ratified_note"));
}

#[test]
fn i_prerequisiti_vanno_in_una_colonna_senza_spazi_in_rosso() {
    let mut s = School::new();
    let a = s.draft(1);
    let b = s.draft(2);
    s.store
        .upsert_argument(&Argument {
            prerequisites: vec![b.id.clone()],
            ..a.clone()
        })
        .expect("prerequisito");
    s.store
        .ratify(&a.id, &s.teacher, "ok")
        .expect("ratifica");
    s.store.publish(&a.id).expect("pubblicazione");
    s.store
        .ratify(&b.id, &s.teacher, "ok")
        .expect("ratifica");
    s.store.publish(&b.id).expect("pubblicazione");

    let tsv = s
        .store
        .export_fixed_columns(&s.teacher, &s.course)
        .expect("export");
    let righe = righe(&tsv);
    let riga_di_a = righe
        .iter()
        .find(|r| colonna(r, "argument") == a.id.0)
        .expect("riga di a");
    assert_eq!(colonna(riga_di_a, "prerequisites"), b.id.0);
    let riga_di_b = righe
        .iter()
        .find(|r| colonna(r, "argument") == b.id.0)
        .expect("riga di b");
    assert_eq!(colonna(riga_di_b, "prerequisites"), "");
}
