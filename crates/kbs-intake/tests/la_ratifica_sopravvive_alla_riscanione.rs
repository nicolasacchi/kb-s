//! D4: una ratifica sopravvive alla riscanione, e la ratifica superata no.
//!
//! Il caso che questo file copre è quello che nessun test copriva e che la
//! pipeline ha scoperto sbagliando: una scansione successiva **non deve
//! disfare** ciò che il docente ha firmato.
//!
//! Il meccanismo del difetto è dichiarato qui perché è la parte che si rompe
//! silenziosamente. `kbs_store::upsert_argument` accetta soltanto stati
//! mutabili: portare un argomento `in-corso` a `bozza` non è una scrittura, è
//! una transizione, e le transizioni passano da `publish` e da `archive`. Una
//! seconda scansione che ricostruisse l'`Argument` **senza** la ratifica e con
//! lo stato `bozza` chiederebbe al negozio una transizione che il negozio
//! rifiuta — e il rifiuto, riportato com'è nel referto, dice all'insegnante che
//! il suo argomento è un file rotto quando è un argomento firmato.

mod common;

use kbs_core::ArgumentId;
use kbs_intake::gate::{self, Gate};
use kbs_intake::route::Verdict;
use kbs_intake::scan;

use common::*;

/// Un corpus di un file solo, in una cartella temporanea.
fn corpus_di_un_file(sorgente: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("cartella temporanea");
    std::fs::create_dir_all(dir.path().join("letture")).expect("la sottocartella");
    std::fs::write(dir.path().join(REL), sorgente).expect("il file");
    dir
}

/// Il verdetto che ha giudicato il testo, da ricalcolare dal file: la porta di
/// D4 pretende il verdetto legato all'hash del testo che sta per firmare.
fn verdetto_del_file(sorgente: &str) -> Verdict {
    gate::rivedi(sorgente)
}

fn id() -> ArgumentId {
    ArgumentId::from_rel_path(REL)
}

#[test]
fn una_ratifica_sopravvive_a_una_seconda_scansione() {
    let sorgente = artifact(false);
    let dir = corpus_di_un_file(&sorgente);
    let mut s = store_con_corso();

    // Prima scansione: il file entra come bozza, e nessuno ha firmato.
    let prima = scan::indexa(&mut s, dir.path(), &docente()).expect("prima scansione");
    assert!(prima.report.index.citable.is_empty(), "senza ratifica non è citabile");
    assert_eq!(
        prima.report.index.not_citable.len(),
        1,
        "l'unico argomento è detto non citabile, con la ragione"
    );

    // Il docente firma e promuove: l'atto di D4.
    Gate::ratifica_e_promuovi(
        &mut s,
        &id(),
        &docente(),
        "verificato",
        &verdetto_del_file(&sorgente),
    )
    .expect("la ratifica e la promozione riescono");

    // Seconda scansione, stesso file, stesso byte.
    let seconda = scan::indexa(&mut s, dir.path(), &docente()).expect("seconda scansione");

    let d = seconda
        .report
        .items
        .iter()
        .find(|d| d.rel_path == REL)
        .expect("il referto ha una riga per file");
    assert!(
        d.valid,
        "una riscanione non può rendere rotto un file che era stato accettato: {:?}",
        d.errors
    );
    assert_eq!(
        seconda.report.index.citable,
        vec![id().to_string()],
        "l'argomento ratificato resta citabile dopo la riscanione"
    );
    assert!(
        seconda.report.index.not_citable.is_empty(),
        "un argomento ratificato non può anche essere detto non citabile: {:?}",
        seconda.report.index.not_citable
    );
}

#[test]
fn la_ratifica_non_sopravvive_a_un_contratto_cambiato() {
    let sorgente = artifact(false);
    let dir = corpus_di_un_file(&sorgente);
    let mut s = store_con_corso();

    scan::indexa(&mut s, dir.path(), &docente()).expect("prima scansione");
    Gate::ratifica_e_promuovi(
        &mut s,
        &id(),
        &docente(),
        "verificato",
        &verdetto_del_file(&sorgente),
    )
    .expect("la ratifica riesce");

    // Il docente corregge il corpo del file dopo aver firmato. È il caso che
    // `kbs_core::check_citable` chiama `StaleRatification` e che D4 fa
    // poggiare: la firma vale per i byte che il docente ha guardato, e non per
    // quelli successivi.
    let corretto = sorgente.replace(
        "Unita' che porta il concetto di forma ridotta e il suo criterio.",
        "Unita' che porta il concetto di forma ridotta, il suo criterio e un esempio.",
    );
    assert_ne!(corretto, sorgente, "la correzione ha cambiato il testo");
    std::fs::write(dir.path().join(REL), &corretto).expect("il file corretto");

    let dopo = scan::indexa(&mut s, dir.path(), &docente()).expect("scansione dopo la correzione");

    assert!(
        dopo.report.index.citable.is_empty(),
        "una ratifica su un contratto precedente non rende citabile: {:?}",
        dopo.report.index.citable
    );
    let nc = dopo
        .report
        .index
        .not_citable
        .iter()
        .find(|n| n.id == id().to_string())
        .expect("l'argomento è detto non citabile");
    assert_eq!(
        nc.invariant, "stale-ratification",
        "il motivo è la ratifica superata, non la mancanza di ratifica: {nc:?}"
    );
}

/// Il caso opposto della stessa cosa, e vale la pena che sia dichiarato: il file
/// che chiede una **promozione** senza firma non la ottiene, e la riscanione non
/// è il posto in cui ottenerla.
#[test]
fn un_file_che_dichiara_in_corso_senza_ratifica_resta_bozza() {
    let sorgente = artifact_in_corso();
    let dir = corpus_di_un_file(&sorgente);
    let mut s = store_con_corso();

    let r = scan::indexa(&mut s, dir.path(), &docente()).expect("scansione");
    assert!(
        r.report.index.citable.is_empty(),
        "un file che si dichiara in-corso non si ratifica da solo: {:?}",
        r.report.index.citable
    );
    let a = s.read_argument(&docente(), &id()).expect("leggibile");
    assert_eq!(a.state, kbs_core::PublicationState::Bozza, "resta bozza");
}
