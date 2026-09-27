//! D10 e D11: il model lock è la spina dorsale, e la prova è che è
//! **deterministico**.
//!
//! Un lock che registra «il modese era claude-sonnet-4-5» è un diario. Un lock
//! che registra l'hash dei byte inviati e l'hash del corpus di quel momento è
//! un registro, e la differità si vede in un test: due richieste identiche
//! devono dare lo stesso lock, e un byte cambiato deve darne un altro.

mod common;

use kbs_core::{ArgumentId, Millis};
use kbs_intake::corpus_hash::{CorpusHasher, hash_dir};
use kbs_intake::prompt::{DiagnosisRef, GenerationRequest};
use kbs_intake::route::{self, Request, Route};
use kbs_intake::{GENERATOR_VERSION, GenerationRequest as GR};
use kbs_store::{GenerationEvent, Store};

use common::*;

const MODELLO: &str = "claude-sonnet-4-5";
const SISTEMA: &str = "Sei un autore di materiale di studio per la scuola secondaria.";
const COMPITO: &str = "Scrivi due esercizi parametrizzati sulle frazioni.";
const ISTRUZIONE: &str = "Marco non distingue la forma ridotta dalla divisione.";

fn richiesta(corpus: kbs_intake::CorpusHash) -> GenerationRequest {
    GenerationRequest::try_new(
        MODELLO,
        GENERATOR_VERSION,
        corpus,
        SISTEMA,
        COMPITO,
        "<p>estratto del corpus</p>",
        ISTRUZIONE,
    )
    .expect("richiesta completa")
}

/// Il negozio con l'argomento gia' dentro.
///
/// `generations.argument_id` referenzia `arguments.id`: un evento di
/// generazione senza il materiale che ha generato non e' un record, e' una
/// riga orfana. Il vincolo e' del database e qui viene rispettato, non aggirato.
fn store_con_materiale() -> Store {
    let mut s = store_con_corso();
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
    .expect("il materiale entra");
    s
}

fn corpus_di_prova() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("letture")).unwrap();
    std::fs::write(dir.path().join(REL), artifact(false)).unwrap();
    dir
}

#[test]
fn due_richieste_identiche_su_corpus_invariato_danno_lo_stesso_lock() {
    let dir = corpus_di_prova();
    let a = richiesta(hash_dir(dir.path()).unwrap()).lock(Millis(1_757_000_000_000)).unwrap();
    let b = richiesta(hash_dir(dir.path()).unwrap()).lock(Millis(1_757_000_000_000)).unwrap();
    assert_eq!(a, b, "stessi byte, stesso corpus, stesso lock");
    assert_eq!(a.model_id, MODELLO);
    assert_eq!(a.generator_version, GENERATOR_VERSION);
    assert!(a.prompt_hash.starts_with("kbs-prompt/1:"));
    assert!(a.corpus_hash.starts_with("kbs-c1:"));
}

#[test]
fn un_byte_di_corpus_cambiato_cambia_il_lock() {
    let dir = corpus_di_prova();
    let prima = richiesta(hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    std::fs::write(
        dir.path().join(REL),
        artifact(false).replace("Livello 1: procede a tentativi", "Livello 1: indovina"),
    )
    .unwrap();
    let dopo = richiesta(hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    assert_ne!(prima.corpus_hash, dopo.corpus_hash);
    assert_ne!(prima, dopo, "D11: la generazione non e' piu' la stessa");
    // E il prompt hash cambia con lei, perche' l'hash del corpus sta nei byte
    // del prompt: e' la ragione per cui il corpus c'e' due volte.
    assert_ne!(prima.prompt_hash, dopo.prompt_hash);
}

#[test]
fn un_file_nuovo_nel_corpus_cambia_il_lock_anche_se_il_primo_e_uguale() {
    let dir = corpus_di_prova();
    let prima = richiesta(hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    std::fs::write(dir.path().join(REL_ALTRO), artifact(false).replace("La frazione irriducibile", "Altro")).unwrap();
    let dopo = richiesta(hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    assert_ne!(prima.corpus_hash, dopo.corpus_hash, "l'hash copre l'insieme, non un file");
}

#[test]
fn l_hash_del_prompt_e_sui_byte_e_il_test_lo_ricalcola_da_parte() {
    // Il test che rende vera la frase del modulo: `prompt_hash` non è una
    // descrizione dei byte, è il loro hash. Ricalcolato qui, senza le funzioni
    // del crate.
    let r = richiesta(kbs_intake::CorpusHash::of_empty());
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update([kbs_intake::prompt::TAG]);
    h.update(r.canonical_bytes().as_bytes());
    assert_eq!(r.prompt_hash(), format!("kbs-prompt/1:{}", hex::encode(h.finalize())));
}

#[test]
fn la_diagnosi_entra_nel_lock_e_non_e_un_campo_di_texto() {
    let base = richiesta(kbs_intake::CorpusHash::of_empty());
    let con = base.clone().with_diagnosis(DiagnosisRef::new(
        "obs_2026_09_14",
        "Marco scrive 3/6 e 2/4 uguali ma 2/6 no",
        Some(kbs_core::PersonId::fixture(1)),
    ));
    assert_ne!(base.prompt_hash(), con.prompt_hash());
    let testo = con.canonical_bytes();
    assert!(testo.contains("diagnosis-id: obs_2026_09_14"));
    assert!(testo.contains("diagnosis-note: Marco scrive 3/6 e 2/4 uguali ma 2/6 no"));
    assert!(testo.contains("diagnosis-witness: person_0001"));
}

#[test]
fn il_lock_si_registra_e_si_rilegge_dal_registro() {
    let mut s = store_con_materiale();
    let id = ArgumentId::from_rel_path(REL);
    let richiesta_ = richiesta(hash_dir(&corpus_di_prova()).unwrap());
    let lock = richiesta_.lock(Millis(1_757_000_000_000)).unwrap();
    let evento = GenerationEvent {
        lock: lock.clone(),
        argument: id.clone(),
        requester: docente(),
        at: lock.at,
    };
    s.record_generation(&evento).expect("lock registrato");
    let riletti = s.generations_for(&id).unwrap();
    assert_eq!(riletti.len(), 1);
    assert_eq!(riletti[0].lock, lock, "il lock rilegto e' quello scritto");
    assert_eq!(riletti[0].lock_id(), evento.lock_id());
    assert_eq!(riletti[0].requester, docente());
}

#[test]
fn due_generazioni_identiche_hanno_lo_stesso_lock_id_e_lo_stesso_registro() {
    // E' la domanda che D11 deve poter rispondere: «questa generazione e' la
    // stessa di allora?» si risponde confrontando l'id, che e' derivato dal
    // lock e non da un contatore.
    let mut s = store_con_materiale();
    let id = ArgumentId::from_rel_path(REL);
    let lock = richiesta(kbs_intake::CorpusHash::of_empty()).lock(Millis(7)).unwrap();
    for at in [Millis(100), Millis(200)] {
        s.record_generation(&GenerationEvent {
            lock: lock.clone(),
            argument: id.clone(),
            requester: docente(),
            at,
        })
        .unwrap();
    }
    let eventi = s.generations_for(&id).unwrap();
    assert_eq!(eventi.len(), 2, "due eventi distinti");
    assert_eq!(eventi[0].lock_id(), eventi[1].lock_id(), "stesso lock, stesso id");
    assert_ne!(eventi[0].at, eventi[1].at, "ma due atti distinti");
}

#[test]
fn l_argomento_generato_conserva_il_lock_nella_sua_provenienza() {
    // D10: ogni generazione registra il lock, e la provenienza dell'argomento
    // lo porta con se'. Se si perdesse, l'argomento non saprebbe da dove
    // viene — e due anni dopo la domanda del modulo resterebbe senza risposta.
    let sorgente = artifact(false)
        .replace(
            r#"<meta name="kb-course" content="matematica-terza">"#,
            r#"<meta name="kb-course" content="matematica-terza">
<meta name="kb-origin" content="generated">
<meta name="kb-lock-model" content="claude-sonnet-4-5">
<meta name="kb-lock-prompt" content="kbs-prompt/1:abc">
<meta name="kb-lock-corpus" content="kbs-c1:def">
<meta name="kb-lock-generator" content="kbs-intake/1">"#,
        );
    let mut s = store_con_corso();
    let ricevuta = route::receive(
        &mut s,
        Request {
            route: Route::File,
            by: docente(),
            course: Some(corso()),
            rel_path: Some(REL.to_string()),
            source: sorgente,
        },
    )
    .unwrap();
    assert!(ricevuta.stored);
    match &ricevuta.argument.origin {
        kbs_core::Origin::Generated { lock, .. } => {
            assert_eq!(lock.model_id, "claude-sonnet-4-5");
            assert_eq!(lock.corpus_hash, "kbs-c1:def");
        }
        altro => panic!("l'origine deve restare generata: {altro:?}"),
    }
}

#[test]
fn l_hash_del_corso_e_dichiarato_e_non_dipende_dall_ordine_di_lettura() {
    let mut a = CorpusHasher::new();
    let mut b = CorpusHasher::new();
    for (rel, corpo) in [("a/uno.html", "1"), ("b/due.html", "2"), ("c/tre.html", "3")] {
        a.add(rel, corpo).unwrap();
    }
    for (rel, corpo) in [("c/tre.html", "3"), ("a/uno.html", "1"), ("b/due.html", "2")] {
        b.add(rel, corpo).unwrap();
    }
    assert_eq!(a.finish(), b.finish(), "readdir non ha un ordine garantito");
    assert_eq!(a.len(), 3);
}

#[test]
fn il_tipo_esportato_e_il_generatore_non_sono_la_stessa_cosa() {
    // Non un test di stile: e' il motivo per cui `CorpusHasher` e
    // `GenerationRequest` non si somigliano. Uno raccoglie e piega, l'altro
    // rende e hashia, e metterli insieme avrebbe prodotto un hash che non
    // descrive nessuno dei due.
    let _: GR = richiesta(kbs_intake::CorpusHash::of_empty());
}
