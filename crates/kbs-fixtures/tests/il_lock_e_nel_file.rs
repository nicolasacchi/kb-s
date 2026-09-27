//! D10: ogni generazione registra un model lock, e l'artifact lo porta.
//!
//! Senza id del modello, hash del prompt, hash del corpus e versione del
//! generatore non c'è un registro, c'è un diario. È la ragione per cui
//! `kbs-intake` rifiuta un artifact che dichiara `kb-origin: generated` e non
//! porta quei quattro campi.
//!
//! Qui si verifica la metà che è di questo crate: che la resa dell'artifact
//! porti **quel che la tabella dichiara**. Una tabella che dichiara un lock e un
//! resa che non lo scrive sono due fatti che non si parlano, e l'artifact
//! generato senza lock è un diario.

use kbs_fixtures::spec::OriginSpec;
use kbs_fixtures::{render, voci};

/// I quattro campi che `kbs_intake::route` nomina quando legge `kb-origin:
/// generated`. Sono dichiarati qui perché sono un contratto fra due crate, e un
/// contratto scritto da una parte sola è un contratto che l'altra parte non
/// rispetta senza che nessuno lo veda.
const QUATTRO_CAMPI: [&str; 4] = [
    "kb-lock-model",
    "kb-lock-prompt",
    "kb-lock-corpus",
    "kb-lock-generator",
];

/// I `<meta>` di un artifact, per nome.
///
/// Non è un parser HTML: è una scansione delle righe `<meta name="…"
/// content="…">` che `render::artifact` scrive, e fallisce sul primo `name` che
/// non è in forma. Un parser generico qui sarebbe un altro modo di avere due
/// letture del file, e due letture divergono.
fn meta(artifact: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for riga in artifact.lines() {
        let riga = riga.trim();
        let Some(resto) = riga.strip_prefix("<meta name=\"") else {
            continue;
        };
        let (nome, resto) = resto
            .split_once("\" content=\"")
            .unwrap_or_else(|| panic!("un <meta> senza nome e contenuto: {riga}"));
        let contenuto = resto
            .strip_suffix("\">")
            .unwrap_or_else(|| panic!("un <meta> non chiuso: {riga}"));
        out.push((nome.to_string(), contenuto.to_string()));
    }
    out
}

fn valore<'a>(meta: &'a [(String, String)], nome: &str) -> Option<&'a str> {
    meta.iter()
        .find(|(n, _)| n == nome)
        .map(|(_, c)| c.as_str())
}

/// La ragione senza la quale l'artifact non entra: un'origine `generated` senza
/// lock. Il banco di prova la chiama `generated-without-lock` e la pipeline
/// emette lo stesso codice; qui il confronto è sul campo, che è la cosa che il
/// file porta o non porta.
fn generati<'a>(voci: &'a [kbs_fixtures::Spec]) -> Vec<&'a kbs_fixtures::Spec> {
    voci.iter()
        .filter(|s| matches!(s.origine, OriginSpec::Generated { .. }))
        .collect()
}

#[test]
fn il_corpus_ha_item_generati() {
    let tabella = voci();
    assert!(
        !generati(&tabella).is_empty(),
        "nessun item generato: il lock di D10 non sarebbe esercitato"
    );
}

/// Ogni item generato porta i quattro campi del lock, e non vuoti.
///
/// Un campo presente e vuoto è il caso peggiore: sembra dichiarato e non
/// identifica niente, quindi è un diario che ha l'aria di un registro.
#[test]
fn un_item_generato_porta_tutti_i_quattro_campi_del_lock() {
    let tabella = voci();
    for s in generati(&tabella) {
        let m = meta(&render::artifact(s));
        for campo in QUATTRO_CAMPI {
            let v = valore(&m, campo)
                .unwrap_or_else(|| panic!("{}: l'artifact non dichiara `{campo}`", s.rel));
            assert!(!v.trim().is_empty(), "{}: `{campo}` è vuoto", s.rel);
        }
    }
}

/// I valori del lock scritti nel file sono **quelli della tabella**, non altri.
///
/// È la parte che distingue un lock da un campo riempito per far passare una
/// validazione: il modello, l'hash del prompt e la versione del generatore
/// arrivano dalla tabella e devono arrivare identici.
#[test]
fn i_valori_del_lock_arrivano_dalla_tabella() {
    let tabella = voci();
    for s in generati(&tabella) {
        let OriginSpec::Generated {
            model, prompt_hash, generator, ..
        } = s.origine
        else {
            continue;
        };
        let m = meta(&render::artifact(s));
        assert_eq!(
            valore(&m, "kb-lock-model"),
            Some(model),
            "{}: il modello scritto non è quello dichiarato",
            s.rel
        );
        assert_eq!(
            valore(&m, "kb-lock-prompt"),
            Some(prompt_hash),
            "{}: l'hash del prompt scritto non è quello dichiarato",
            s.rel
        );
        assert_eq!(
            valore(&m, "kb-lock-generator"),
            Some(generator),
            "{}: la versione del generatore scritta non è quella dichiarata",
            s.rel
        );
    }
}

/// Un item scritto a mano non porta un lock, e non lo porta a caso.
///
/// La regola è che il lock è una **dichiarazione d'origine**: dichiarare
/// `human` e scrivere un lock sarebbe dire due cose incompatibili, e
/// dichiarare `generated` senza lock è il caso che l'intake rifiuta.
#[test]
fn un_item_scritto_a_mano_non_porta_nessun_lock() {
    let tabella = voci();
    for s in tabella
        .iter()
        .filter(|s| matches!(s.origine, OriginSpec::Human { .. }))
    {
        let m = meta(&render::artifact(s));
        for campo in QUATTRO_CAMPI {
            assert_eq!(
                valore(&m, campo),
                None,
                "{}: un item `human` non porta `{campo}`",
                s.rel
            );
        }
        assert_eq!(valore(&m, "kb-origin"), Some("human"), "{}", s.rel);
    }
}

/// L'hash del corpus che il lock dichiara è l'hash del corpus.
///
/// D11 dice che data la tupla (hash del corpus, id dei chunk, versione del
/// modello) il replay è possibile. Un lock con un hash di corpus inventato non
/// rende replayabile niente, e il peggio è che sembra che lo faccia.
#[test]
fn l_hash_del_corpus_del_lock_e_un_hash() {
    let tabella = voci();
    for s in generati(&tabella) {
        let m = meta(&render::artifact(s));
        let h = valore(&m, "kb-lock-corpus").expect("dichiarato");
        assert!(
            h.starts_with("sha256:") && h.len() == "sha256:".len() + 64,
            "{}: l'hash del corpus del lock non è un SHA-256: {h}",
            s.rel
        );
    }
}
