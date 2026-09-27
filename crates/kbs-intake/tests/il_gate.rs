//! D4: due percorsi di scrittura, e la porta in mezzo.
//!
//! Le tre finestre del gate, ciascuna su dati veri: nessuna ratifica, una
//! ratifica superata, un verdetto bloccante. E la quarta, che è quella che
//! nessuno scrive: il testo è cambiato **fra** la validazione e la promozione.

mod common;

use kbs_core::{ArgumentId, Invariant, PublicationState};
use kbs_intake::gate::{self, Gate};
use kbs_intake::route::{self, Request, Route, Verdict};
use kbs_store::Store;

use common::*;

fn richiesta(sorgente: &str) -> Request {
    Request {
        route: Route::File,
        by: docente(),
        course: Some(corso()),
        rel_path: Some(REL.to_string()),
        source: sorgente.to_string(),
    }
}

fn bozza(s: &mut Store, sorgente: &str) -> (kbs_intake::Receipt, Verdict) {
    let ricevuta = route::receive(s, richiesta(sorgente)).expect("intake riuscito");
    let verdetto = ricevuta.verdict.clone();
    (ricevuta, verdetto)
}

/// L'id dell'argomento, che segue il file (`kbs_core::ArgumentId::from_rel_path`).
///
/// Non prende il store: l'id si calcola dal percorso e non dal contenuto, ed e'
/// esattamente la proprieta' che `le_quattro_strade` verifica.
fn id_di() -> ArgumentId {
    ArgumentId::from_rel_path(REL)
}

#[test]
fn la_promozione_senza_ratifica_e_rifiutata() {
    let mut s = store_con_corso();
    let (_, verdetto) = bozza(&mut s, &artifact(false));
    assert!(verdetto.can_publish(), "l'artifact e' valido: il problema e' la ratifica");
    // `promuovi` usa la ratifica che c'e' — e non c'e'. E' questa la strada che
    // «non promuovere senza ratifica» significa: `ratifica_e_promuovi` firma
    // prima, ed e' un atto del docente, non una scorciatoia.
    let e = Gate::promuovi(&mut s, &id_di(), &docente(), &verdetto)
        .expect_err("senza ratifica non si pubblica");
    // Il rifiuto è quello del negozio, non uno di questo crate: la regola è
    // quella di D4 e l'ha scritta `kbs-store`.
    match e {
        kbs_intake::Error::Store(kbs_store::Error::Invariant(
            Invariant::CitableWithoutRatification(PublicationState::Bozza),
        )) => {}
        altro => panic!("atteso CitableWithoutRatification, trovato {altro:?}"),
    }
    // E resta bozza: un rifiuto non lascia mezzo stato.
    assert_eq!(
        s.read_argument(&docente(), &id_di()).unwrap().state,
        PublicationState::Bozza
    );
}

#[test]
fn la_promozione_ratificando_e_promuovendo_chiede_un_atto_e_non_lo_presume() {
    // Il simmetrico del test precedente, e la ragione per cui le due funzioni
    // esistono separate: se `ratifica_e_promuovi` non ratificasse, sarebbe la
    // stessa porta di `promuovi` con un nome piu' gradevole.
    let mut s = store_con_corso();
    let (_, verdetto) = bozza(&mut s, &artifact(false));
    Gate::ratifica_e_promuovi(&mut s, &id_di(), &docente(), "nota", &verdetto).unwrap();
    let argomento = s.read_argument(&docente(), &id_di()).unwrap();
    let ratifica = argomento.ratified.expect("la ratifica c'e'");
    assert_eq!(ratifica.by, docente());
    assert_eq!(ratifica.contract_hash, argomento.content_hash);
    assert!(!ratifica.note.is_empty());
}

#[test]
fn una_ratifica_superata_e_rifiutata_su_dati_veri() {
    let mut s = store_con_corso();
    // 1. il docente guarda il testo e lo ratifica.
    let (_, _) = bozza(&mut s, &artifact(false));
    Gate::ratifica(&mut s, &id_di(), &docente(), "verificato a mano").unwrap();
    // 2. il testo cambia: la sezione della scala perde un livello.
    let (_, verdetto_vecchio) = bozza(&mut s, &artifact(false).replace("Livello 1", "Livello 0"));
    // 3. la ratifica vale per il testo di prima, non per questo.
    let e = Gate::promuovi(&mut s, &id_di(), &docente(), &verdetto_vecchio)
        .expect_err("una ratifica superata non vale");
    match e {
        kbs_intake::Error::Store(kbs_store::Error::Invariant(Invariant::StaleRatification {
            ref declared,
            ref current,
        })) => {
            assert_ne!(declared, current, "una ratifica superata ha due hash diversi");
        }
        altro => panic!("atteso StaleRatification, trovato {altro:?}"),
    }
}

#[test]
fn un_verdetto_bloccante_chiude_la_porta_anche_con_ratifica() {
    let mut s = store_con_corso();
    let (_, verdetto) = bozza(&mut s, &artifact_con_cdn());
    assert!(verdetto.has_code("external-reference"));
    // Con la ratifica già firmata, la porta deve comunque dire no: è D15 che
    // vieta il riferimento fuori, e non è la ratifica a decidere.
    Gate::ratifica(&mut s, &id_di(), &docente(), "verificato a mano").unwrap();
    let e = Gate::ratifica_e_promuovi(&mut s, &id_di(), &docente(), "nota", &verdetto)
        .expect_err("D15 non si aggira con una ratifica");
    match e {
        kbs_intake::Error::VerdettoBloccante { ref codice, .. } => {
            assert_eq!(codice, "external-reference")
        }
        altro => panic!("atteso VerdettoBloccante, trovato {altro:?}"),
    }
}

#[test]
fn il_contenuto_cambiato_fra_verdetto_e_promozione_e_rifiutato() {
    let mut s = store_con_corso();
    let (_, verdetto) = bozza(&mut s, &artifact(false));
    // Il file cambia dopo che il verdetto è stato calcolato.
    bozza(&mut s, &artifact(false).replace("Livello 1", "Livello 0"));
    let e = Gate::ratifica_e_promuovi(&mut s, &id_di(), &docente(), "nota", &verdetto)
        .expect_err("il verdetto guarda un altro testo");
    match e {
        kbs_intake::Error::ContenutoCambiato { del_verdetto, ref corrente } => {
            assert_ne!(del_verdetto, *corrente);
        }
        altro => panic!("atteso ContenutoCambiato, trovato {altro:?}"),
    }
}

#[test]
fn la_promozione_riuscita_rende_l_argomento_citabile() {
    let mut s = store_con_corso();
    let (_, verdetto) = bozza(&mut s, &artifact(false));
    let stato = Gate::ratifica_e_promuovi(&mut s, &id_di(), &docente(), "verificato", &verdetto)
        .expect("con ratifica e verdetto pulito si pubblica");
    assert_eq!(stato, PublicationState::InCorso);
    let argomento = s.read_argument(&docente(), &id_di()).unwrap();
    assert!(argomento.is_citable_now());
    // E la seconda promozione non è un errore: tornare indietro non è
    // definito da D4, e ripromuovere lo stesso testo no.
    assert_eq!(
        Gate::ratifica_e_promuovi(&mut s, &id_di(), &docente(), "ancora", &verdetto).unwrap(),
        PublicationState::InCorso
    );
}

#[test]
fn il_gate_non_reimplementa_la_ratifica_ma_la_chiede() {
    // Il verdetto ricalcolato dal file e quello della ricevuta dicono la stessa
    // cosa: due implementazioni della validazione sarebbero due proprietà
    // diverse, e la più debole vincerebbe per abitudine.
    let mut s = store_con_corso();
    let (_, verdetto) = bozza(&mut s, &artifact(false));
    let ricalcolato = gate::rivedi(&artifact(false));
    assert_eq!(verdetto.blocking_codes(), ricalcolato.blocking_codes());
    assert_eq!(verdetto.content_hash, ricalcolato.content_hash);
}
