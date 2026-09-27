//! Il banco è deterministico, o non serve a niente.
//!
//! Un banco il cui output cambia fra un'esecuzione e l'altra non può dire
//! «qualcosa è cambiato»: dice «cambia tutto», e chi lo legge smette di
//! leggerlo. Qui la proprietà è verificata per confronto byte per byte su due
//! esecuzioni indipendenti, in testo e in JSON.

use kbs_fixtures::adapter::PipelineAssente;
use kbs_fixtures::checks::{Banco, Config};
use kbs_fixtures::corpus::Corpus;
use kbs_fixtures::report;

fn assente() -> PipelineAssente {
    PipelineAssente {
        ragione: "binario assente".into(),
    }
}

fn gira() -> kbs_fixtures::Referto {
    let p = assente();
    Banco::new(Config::radice_di_default(), &p).esegui()
}

/// Due esecuzioni producono lo stesso referto, testo identico byte per byte.
#[test]
fn il_referto_testuale_e_identico_fra_due_esecuzioni() {
    let a = report::in_testo(&gira(), false);
    let b = report::in_testo(&gira(), false);
    assert_eq!(a.len(), b.len());
    assert!(
        a == b,
        "due esecuzioni hanno prodotto referti diversi:\n---\n{a}\n---\n{b}\n---"
    );
}

/// E in JSON, che è la forma che la CI e gli strumenti di diff consumano.
#[test]
fn il_referto_json_e_identico_fra_due_esecuzioni() {
    let a = report::in_json(&gira(), false);
    let b = report::in_json(&gira(), false);
    assert_eq!(a, b);
}

/// Il corpus è la stessa cosa ogni volta: stessi file, stessi hash, e l'hash è
/// quello dichiarato in un test. Se questo test fallisce, è cambiato un
/// fixture, e il messaggio dice quale.
#[test]
fn il_corpus_ha_gli_stessi_byte_ogni_volta() {
    let a = Corpus::dalla_tabella();
    let b = Corpus::dalla_tabella();
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.file(), b.file());
    assert_eq!(
        a.hash(),
        "sha256:d7abeffaf04e3ec3238c43ac04c4d13ec30359960a82ec6c89fe0ce89b3a7370"
    );
}

/// La resa non dipende dall'ordine in cui si chiede: chiedere l'item 39 e poi
/// il primo dà gli stessi byte che chiederli in ordine inverso. Una resa che
/// dipende dallo stato interno è una resa che un test parallelo può rendere
/// diversa da un test seriale.
#[test]
fn la_resa_non_dipende_dall_ordine_in_cui_si_chiede() {
    let voci = kbs_fixtures::voci();
    let diretto: Vec<String> = voci.iter().map(kbs_fixtures::render::artifact).collect();
    let inverso: Vec<String> = voci.iter().rev().map(kbs_fixtures::render::artifact).collect();
    let mut riordinato = inverso;
    riordinato.reverse();
    assert_eq!(diretto, riordinato);
}
