//! Le quattro strade di D10, e la prova che sono una.
//!
//! Un gate che quattro strade possono ognuna aggirare non è un gate. Questo
//! file è la prova che non lo è: lo stesso contenuto, quattro strade diverse,
//! **stesso stato e stesso verdetto**. Se qualcuno aggiunge una quinta strada
//! con una validazione sua, questo test smette di coprirla e
//! `Route::ALL` smette di elencarla — due buoni motivi per non aggiungerla di
//! nascosto.

mod common;

use kbs_core::{CourseId, Millis, PersonId, PublicationState};
use kbs_intake::capture;
use kbs_intake::corpus_hash::hash_dir;
use kbs_intake::prompt::{DiagnosisRef, GenerationRequest};
use kbs_intake::route::{self, Request, Route};
use kbs_intake::scan;

use common::*;

/// La stessa richiesta, quattro strade. La `rel_path` è la stessa: se non lo
/// fosse, l'id cambierebbe e il confronto non direbbe niente.
fn richieste(sorgente: &str) -> Vec<(&'static str, Request)> {
    let by = docente();
    let per_rotta: Vec<(&'static str, Request)> = Route::ALL
        .iter()
        .map(|r| {
            (
                r.as_str(),
                Request {
                    route: *r,
                    by: by.clone(),
                    course: Some(corso()),
                    rel_path: Some(REL.to_string()),
                    source: sorgente.to_string(),
                },
            )
        })
        .collect();
    per_rotta
}

#[test]
fn le_quattro_strade_danno_lo_stesso_stato_e_lo_stesso_verdetto() {
    let sorgente = artifact_in_corso();
    let risultati: Vec<_> = richieste(&sorgente)
        .into_iter()
        .map(|(nome, r)| {
            let mut s = store_con_corso();
            let ricevuta = route::receive(&mut s, r).expect("intake riuscito");
            (nome, ricevuta)
        })
        .collect();

    let (_, prima) = &risultati[0];
    for (nome, r) in &risultati[1..] {
        assert_eq!(
            r.argument.state, prima.argument.state,
            "{nome}: stato diverso da capture"
        );
        assert_eq!(r.argument.id, prima.argument.id, "{nome}: id diverso");
        assert_eq!(
            r.argument.content_hash, prima.argument.content_hash,
            "{nome}: hash del contenuto diverso"
        );
        assert_eq!(r.argument.prerequisites, prima.argument.prerequisites, "{nome}");
        assert_eq!(r.argument.title, prima.argument.title, "{nome}");
        assert_eq!(
            r.verdict.blocking_codes(),
            prima.verdict.blocking_codes(),
            "{nome}: verdetto diverso"
        );
        assert_eq!(
            r.verdict.content_hash, prima.verdict.content_hash,
            "{nome}: il verdetto guarda un testo diverso"
        );
        assert_eq!(r.stored, prima.stored, "{nome}: uno entra e l'altro no");
    }
    assert_eq!(risultati.len(), 4, "le quattro strade di D10, non tre");
}

#[test]
fn nessuna_strade_scrive_lo_stato_di_pubblicazione() {
    // Il file dichiara `in-corso`. Nessuna strada lo prende, e tutte e quattro
    // lo dicono con lo stesso codice di invariante.
    for (nome, r) in richieste(&artifact_in_corso()) {
        let mut s = store_con_corso();
        let ricevuta = route::receive(&mut s, r).unwrap();
        assert_eq!(ricevuta.argument.state, PublicationState::Bozza, "{nome}");
        assert!(!ricevuta.argument.is_citable_now(), "{nome}: bozza non e' citabile");
        assert!(
            ricevuta.verdict.has_code("citable-without-ratification"),
            "{nome}: il verdetto deve dire che manca la ratifica, {:?}", ricevuta.verdict.diagnostics
        );
    }
}

#[test]
fn un_artifact_che_referenzia_una_cdn_e_rifiutato_da_ogni_strada() {
    for (nome, r) in richieste(&artifact_con_cdn()) {
        let mut s = store_con_corso();
        let ricevuta = route::receive(&mut s, r).unwrap();
        assert!(
            ricevuta.verdict.has_code("external-reference"),
            "{nome}: D15 vieta il riferimento fuori dalla scatella"
        );
        assert!(!ricevuta.verdict.can_publish(), "{nome}");
    }
}

#[test]
fn la_strada_del_file_rifiuta_il_cdn_esattamente_come_una_cattura_a_mano() {
    // È il test che chiede l'equalità dei due verdetti, non la loro somiglianza.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("letture")).unwrap();
    std::fs::write(dir.path().join(REL), artifact_con_cdn()).unwrap();

    let mut store = store_con_corso();
    let scansione = scan::indexa(&mut store, dir.path(), &docente()).unwrap();
    let da_file = &scansione.receipts[0].verdict;

    let mut store = store_con_corso();
    let da_cattura = capture::parse_paste(&artifact_con_cdn()).unwrap();
    let ricevuta =
        route::receive(&mut store, capture::to_request(da_cattura, docente())).unwrap();

    assert_eq!(da_file.blocking_codes(), ricevuta.verdict.blocking_codes());
    assert_eq!(da_file.content_hash, ricevuta.verdict.content_hash);
}

#[test]
fn la_strada_del_file_vede_un_artifact_che_la_strada_del_file_non_ha() {
    // Una rotta che validasse per conto proprio passerebbe da qui: lo stesso
    // file, due volte, e i due verdetti devono essere lo stesso verdetto.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("letture")).unwrap();
    std::fs::write(dir.path().join(REL), artifact(false)).unwrap();
    let mut s = store_con_corso();
    let prima = scan::indexa(&mut s, dir.path(), &docente()).unwrap();
    let seconda = scan::indexa(&mut s, dir.path(), &docente()).unwrap();
    assert_eq!(prima.report.items, seconda.report.items);
    assert_eq!(prima.corpus_hash, seconda.corpus_hash);
}

#[test]
fn il_ciclo_nei_prerequisiti_e_un_verdetto_e_non_un_crash() {
    for (nome, r) in richieste(&artifact_ciclico()) {
        let mut s = store_con_corso();
        let ricevuta = route::receive(&mut s, r).unwrap();
        assert!(ricevuta.verdict.has_code("prerequisite-cycle"), "{nome}");
        assert!(!ricevuta.stored, "{nome}: un ciclo non entra");
    }
}

#[test]
fn un_origine_generata_senza_lock_non_entra_e_non_si_disfa_a_human() {
    for (nome, r) in richieste(&artifact_generato_senza_lock()) {
        let mut s = store_con_corso();
        let ricevuta = route::receive(&mut s, r).unwrap();
        assert!(
            ricevuta.verdict.has_code("generated-without-lock"),
            "{nome}: D10 dice che ogni generazione registra un lock"
        );
        assert!(!ricevuta.stored, "{nome}: senza lock non si scrive");
        // E soprattutto: non si scrive `human`. Dichiarare che l'ha scritto
        // una persona sarebbe la bugia che il campo esiste per evitare.
        assert_eq!(ricevuta.claims.len(), 0, "{nome}");
    }
}

#[test]
fn la_cattura_e_una_strada_come_le_altre_e_rifiuta_che_non_si_capisce() {
    assert!(matches!(
        capture::parse_paste("Va bene, mandiamolo così."),
        Err(kbs_intake::Error::PasteNonDocumento { .. })
    ));
    let mut s = store_con_corso();
    let catturato = capture::parse_paste(&artifact(false)).unwrap();
    let via_cattura = route::receive(&mut s, capture::to_request(catturato, docente())).unwrap();
    assert!(via_cattura.stored);
    assert_eq!(via_cattura.argument.rel_path.as_deref(), Some("capture/la-frazione-irriducibile.html"));
}

#[test]
fn il_lock_e_il_corpus_sono_la_spine_di_tutto_questo() {
    // Non un test di un modulo: il test che lega la strada del file all'hash
    // che D11 chiede. Lo stesso corpus due volte, lo stesso lock; un byte
    // cambiato, un lock diverso.
    let richiesta = |corpus: &kbs_intake::CorpusHash| {
        GenerationRequest::try_new(
            "claude-sonnet-4-5",
            kbs_intake::GENERATOR_VERSION,
            corpus.clone(),
            "Sei un generatore di materiale di studio.",
            "Scrivi due esercizi sulle frazioni.",
            "<p>Il corpus</p>",
            "Marco non distingue la forma ridotta dalla divisione.",
        )
        .unwrap()
    };

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("letture")).unwrap();
    std::fs::write(dir.path().join(REL), artifact(false)).unwrap();
    let a = richiesta(&hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    let b = richiesta(&hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    assert_eq!(a, b, "corpus invariato, lock identico");

    std::fs::write(dir.path().join(REL), artifact(false).replace("Livello 1", "Livello 0")).unwrap();
    let c = richiesta(&hash_dir(dir.path()).unwrap()).lock(Millis(1)).unwrap();
    assert_ne!(a.corpus_hash, c.corpus_hash, "un byte cambiato, un lock diverso");
    assert_ne!(a, c);
}

#[test]
fn la_diagnosi_nel_prompt_cambia_il_lock_e_il_lock_senza_diagnosi_no() {
    let corpus = hash_dir(tempfile::tempdir().unwrap()).unwrap();
    let base = GenerationRequest::try_new(
        "claude-sonnet-4-5",
        kbs_intake::GENERATOR_VERSION,
        corpus,
        "sistema",
        "compito",
        "excerpt",
        "istruzione",
    )
    .unwrap();
    let con = base.clone().with_diagnosis(DiagnosisRef::new(
        "obs_1",
        "Marco scrive 3/6 e 2/4 uguali ma 2/6 no",
        Some(PersonId::fixture(1)),
    ));
    assert_ne!(base.prompt_hash(), con.prompt_hash());
    assert_eq!(base.lock(Millis(1)).unwrap().prompt_hash, base.prompt_hash());
    assert_eq!(con.lock(Millis(1)).unwrap().prompt_hash, con.prompt_hash());
}

#[test]
fn il_corso_manca_e_un_rifiuto_e_non_un_argomento_orfano() {
    let sorgente = artifact(false).replace(
        r#"<meta name="kb-course" content="matematica-terza">"#,
        "",
    );
    let mut s = store_con_corso();
    let e = route::receive(
        &mut s,
        Request {
            route: Route::Cli,
            by: docente(),
            course: None,
            rel_path: Some(REL_ALTRO.to_string()),
            source: sorgente,
        },
    )
    .unwrap_err();
    assert!(matches!(e, kbs_intake::Error::CorsoMancante), "{e}");
}

#[test]
fn la_riga_registrata_ha_il_corpo_del_documento_e_lo_span() {
    let mut s = store_con_corso();
    let (_, r) = richieste(&artifact(false)).into_iter().next().unwrap();
    let ricevuta = route::receive(&mut s, r).unwrap();
    assert_eq!(ricevuta.claims.len(), 1);
    let c = &ricevuta.claims[0];
    assert_eq!(c.id, format!("{}::cl_1", ricevuta.argument.id));
    assert!(c.span_text.is_some(), "una claim senza testo di span e' un indirizzo");
    let rilette = s.claims_for(&docente(), &ricevuta.argument.id).unwrap();
    assert_eq!(rilette.len(), 1);
    assert_eq!(rilette[0].text, c.text);
}

#[test]
fn un_documento_senza_titolo_non_entra_e_lo_dice() {
    let sorgente = artifact(false).replace("<title>La frazione irriducibile</title>", "");
    let mut s = store_con_corso();
    let e = route::receive(
        &mut s,
        Request {
            route: Route::Capture,
            by: docente(),
            course: Some(CourseId(CORSO.to_string())),
            rel_path: Some(REL_ALTRO.to_string()),
            source: sorgente,
        },
    )
    .unwrap_err();
    match e {
        kbs_intake::Error::TitoloMancante { problemi } => {
            assert!(problemi.contains("no-title"), "{problemi}");
        }
        altro => panic!("atteso TitoloMancante, trovato {altro:?}"),
    }
}
