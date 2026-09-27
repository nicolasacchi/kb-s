//! La copertura del banco è ciò che lo distingue da un elenco di file.
//!
//! Un corpus di quaranta item che non copre le dodici famiglie di media, che
//! non ha una ratifica superata e che non ha una sola fixture rotta, è un
//! corpus che passa la validazione e non verifica niente. Qui ogni
//! proprietà richiesta dal banco è verificata **sui dati**, e i controlli
//! nominati sono gli stessi che il runner esegue: un test che ne verifica uno
//! diverso sarebbe un secondo banco, e due banche non si confrontano.

use kbs_core::{
    check_citable, ClaimStatus, Invariant, Millis, PublicationState, SeqInSession,
};
use kbs_fixtures::adapter::PipelineAssente;
use kbs_fixtures::checks::{famiglie_coperte, Banco, Config, Esito};
use kbs_fixtures::contract::Defect;
use kbs_fixtures::corpus;
use kbs_fixtures::families::{self, Family};
use kbs_fixtures::items;
use kbs_fixtures::spec::{RatificaSpec, Spec};

fn banco() -> kbs_fixtures::Referto {
    let p = PipelineAssente { ragione: "binario assente".into() };
    Banco::new(Config::radice_di_default(), &p).esegui()
}

/// Quaranta item, dodici famiglie: il numero minimo che copre il catalogo del
/// corpus di studio senza che il banco diventi un corpus.
#[test]
fn quaranta_item_e_dodici_famiglie() {
    assert_eq!(items::DIMENSIONE, 40);
    assert_eq!(items::voci().len(), 40);
    assert_eq!(famiglie_coperte(&kbs_fixtures::Corpus::dalla_tabella()).len(), 12);
    let mut presenti: Vec<Family> = items::voci().iter().map(|s| s.famiglia).collect();
    presenti.sort();
    presenti.dedup();
    assert_eq!(presenti, families::ALL.to_vec());
}

/// Ogni stato di pubblicazione deve avere almeno un item **valido**, non solo
/// un item che il banco rifiuta: altrimenti «c'è una bozza nel banco» può
/// voler dire «c'è una bozza rotta».
#[test]
fn ogni_stato_di_pubblicazione_ha_un_item_valido() {
    for st in [
        PublicationState::Bozza,
        PublicationState::DelDocente,
        PublicationState::InCorso,
        PublicationState::Archiviato,
    ] {
        let n = items::voci()
            .iter()
            .filter(|s| s.stato == st && !s.deve_essere_rifiutato())
            .count();
        assert!(n > 0, "nessun item valido nello stato {st:?}");
    }
}

/// Le cinque fixture rotte, una per regola: D7 tre volte, D15 una, ciclo una.
#[test]
fn le_cinque_fixture_rotte_una_per_regola() {

    let voci = items::voci();
    let rotte: Vec<&Spec> = voci
        .iter()
        .filter(|s| s.deve_essere_rifiutato())
        .collect();
    assert_eq!(rotte.len(), kbs_fixtures::FIXTURE_ROTTE);
    // Il confronto è per **codice** e non per valore: due difetti con lo
    // stesso codice sono lo stesso difetto, e un test che li confronta per
    // valore dovrebbe ripetere la URL della CDN per poter passare.
    const CODICI: [&str; 5] = [
        "contract-missing-section",
        "contract-guardian-out-of-budget",
        "contract-over-budget",
        "external-reference",
        "prerequisite-cycle",
    ];
    for atteso in CODICI {
        let n = rotte
            .iter()
            .filter(|s| s.difetto.expected_code() == Some(atteso))
            .count();
        assert_eq!(n, 1, "attesa una fixture con il codice {atteso}, trovate {n}");
    }
    // E nessuna fixture con un codice fuori dai cinque: un sesto difetto
    // sarebbe un difetto che nessun controllo verifica.
    for s in &rotte {
        let c = s.difetto.expected_code().expect("ogni difetto ha un codice");
        assert!(CODICI.contains(&c), "{}: codice {c} fuori dai cinque", s.rel);
    }
}

/// La ratifica superata, verificata **come dato**: un argomento in uso, con
/// ratifica, il cui hash di contratto non è quello corrente, e che `kbs-core`
/// dichiara non citabile per la ragione giusta.
#[test]
fn la_ratifica_superata_e_un_dato_e_non_un_commento() {
    let argomenti = corpus::argomenti();
    let superata = argomenti
        .iter()
        .find(|a| {
            a.ratified.is_some()
                && a.state == PublicationState::InCorso
                && !a.is_citable_now()
        })
        .expect("nessun argomento in uso con ratifica superata");
    assert_eq!(superata.rel_path.as_deref(), Some("prove/02-prova-finale.html"));
    match check_citable(superata) {
        Err(Invariant::StaleRatification { declared, current }) => {
            assert_ne!(declared, current, "gli hash coincidono: non è una ratifica superata");
            assert_eq!(current, superata.content_hash);
        }
        altro => panic!("attesa StaleRatification, trovata {altro:?}"),
    }
    // E accanto, un argomento in uso senza ratifica: la citabilità dipende
    // dalla ratifica e non dallo stato.
    let senza = argomenti
        .iter()
        .find(|a| a.state == PublicationState::InCorso && a.ratified.is_none())
        .expect("nessun argomento in uso senza ratifica");
    assert!(matches!(
        check_citable(senza),
        Err(Invariant::CitableWithoutRatification(_))
    ));
    // E un argomento in uso ratificato che **è** citabile: senza questo, il
    // banco dimostrerebbe solo che niente è citabile.
    assert!(argomenti.iter().any(|a| a.is_citable_now()));
}

/// Le claim che portano un errore: una contraddetta con lo span che non la
/// regge, e una non citabile senza span. Entrambe restano nel registro.
#[test]
fn una_claim_contraddetta_e_una_non_citabile_su_dati_veri() {
    let voci = items::voci();
    let contraddetta = voci
        .iter()
        .flat_map(|s| s.claims.iter().map(move |c| (s, c)))
        .find(|(_, c)| c.testo == "Ridurre una frazione diminuisce il denominatore o lo lascia invariato.");
    let (_item, cl) = contraddetta.expect("nessuna claim contraddetta");
    assert!(cl.testo_span.is_some(), "una claim contraddetta ha uno span: non sostiene, ma c'è");

    let non_citabile = voci
        .iter()
        .flat_map(|s| s.claims.iter().map(move |c| (s, c)))
        .find(|(_, c)| c.testo_span.is_none())
        .expect("nessuna claim non citabile");
    let (_, nc) = non_citabile;
    assert!(!nc.testo_span.is_some());
    // E la regola «un errore si registra, non si cancella», applicata alle righe
    // che il banco costruisce. «Non si cancella» riguarda il **registro**, non
    // l'output: una claim che il proprio documento non sostiene resta
    // iscritta e con la sua traccia, e non viene pubblicata come se reggesse.
    // Le due cose sono diverse, e confonderle è il modo in cui un registro
    // smette di essere un registro.
    let core = nc.stato.to_core();
    assert!(matches!(core, ClaimStatus::Unciteable));
    assert!(core.suppresses_output(), "una claim non citabile non esce dall'output");
    assert!(
        ClaimStatus::Contradicted.suppresses_output(),
        "una claim contraddetta non esce dall'output citabile: si cita un'affermazione che il proprio documento non sostiene, e un errore non si cita come se fosse vero"
    );
}

/// La scena 3D: una claim per nodo e per arco, almeno una relazione
/// correggibile, e i dati in un file **separato** dal rendering.
#[test]
fn la_scena_3d_ha_una_claim_per_ogni_oggetto_e_i_dati_stanno_nel_manifest() {
    let voci = items::voci();
    let con_scena: Vec<&Spec> = voci.iter().filter(|s| s.scena.is_some()).collect();
    assert!(!con_scena.is_empty());
    for s in con_scena {
        let sc = s.scena.expect("filtrato sopra");
        let dichiarate: Vec<&str> = s.claims.iter().map(|c| c.id).collect();
        for n in sc.nodi {
            assert!(dichiarate.contains(&n.claim), "{}: nodo senza claim", s.rel);
        }
        for a in sc.archi {
            assert!(dichiarate.contains(&a.claim), "{}: arco senza claim", s.rel);
        }
        if !s.deve_essere_rifiutato() {
            assert!(
                sc.archi.iter().any(|a| a.correggibile),
                "{}: nessuna relazione correggibile",
                s.rel
            );
        }
        // I dati della scena stanno in un manifest, non nel codice che la
        // disegna: è ciò che rende il rendering un effetto (D15.1.4).
        let corpus = kbs_fixtures::Corpus::dalla_tabella();
        let manifest = corpus.file_di(sc.manifest).expect("manifest nel corpus");
        let v: serde_json::Value =
            serde_json::from_str(&manifest.contenuto).expect("manifest JSON");
        assert_eq!(
            v["nodi"].as_array().expect("nodi").len(),
            sc.nodi.len(),
            "{}: i nodi del manifest non sono quelli della tabella",
            s.rel
        );
        assert!(manifest.contenuto.contains(kbs_fixtures::render::PERCORSO_THREE)
            || s.riferimento_esterno.is_some());
    }
}

/// L'esercizio parametrizzato: due istanze, due seed, due risposte.
#[test]
fn l_esercizio_parametrizzato_ha_due_istanze_con_risposte_diverse() {
    let voci = items::voci();
    let (item, esercizio) = voci
        .iter()
        .flat_map(|s| s.esercizi.iter().map(move |e| (s, e)))
        .find(|(_, e)| e.istanze.len() == 2)
        .expect("nessun esercizio con due istanze");
    let istanze = corpus::istanze_di(item);
    let dichiarate: Vec<_> = istanze.iter().filter(|i| i.exercise.ends_with(esercizio.id)).collect();
    assert_eq!(dichiarate.len(), 2);
    assert!(dichiarate[0].differs_from(&dichiarate[1]));
    assert_ne!(dichiarate[0].seed, dichiarate[1].seed);
    assert_ne!(dichiarate[0].params, dichiarate[1].params);
    // Lo stesso ragionamento, risposte diverse: è la risposta all'integrità
    // accademica per costruzione e non per sorveglianza.
    assert_eq!(esercizio.famiglia, "somma-frazioni");
}

/// I due riferimenti al runtime: la CDN che deve fallire e il percorso locale
/// che deve passare. Se il banco smette di distinguerli, la metà negativa di
/// D15 non esiste.
#[test]
fn un_riferimento_a_cdn_e_uno_al_runtime_locale() {
    let voci = items::voci();
    let cdn: Vec<&Spec> = voci
        .iter()
        .filter(|s| matches!(s.difetto, Defect::RiferimentoEsterno(_)))
        .collect();
    assert_eq!(cdn.len(), 1);
    assert!(cdn[0].riferimento_esterno.unwrap_or_default().starts_with("https://"));
    assert!(cdn[0].deve_essere_rifiutato());

    let locale: Vec<&Spec> = voci.iter().filter(|s| s.carica_three_locale).collect();
    assert_eq!(locale.len(), 1);
    assert!(!locale[0].deve_essere_rifiutato());
    assert!(locale[0].riferimento_esterno.is_none());
    let artefatto = kbs_fixtures::Corpus::dalla_tabella()
        .file_di(locale[0].rel)
        .expect("artefatto")
        .contenuto
        .clone();
    assert!(artefatto.contains(kbs_fixtures::render::PERCORSO_THREE));
}

/// Il grafo dei prerequisiti: una catena vera fra gli item validi, e un item
/// che chiude un ciclo.
#[test]
fn la_catena_e_reale_e_il_ciclo_e_dichiarato() {
    let voci = items::voci();
    let validi: std::collections::BTreeSet<&str> = voci
        .iter()
        .filter(|s| !s.deve_essere_rifiutato())
        .map(|s| s.rel)
        .collect();
    // Una catena lunga: almeno quattro item validi in fila.
    let mut lunga = 0;
    for s in &voci {
        let mut catena = vec![s.rel];
        let mut cur = s;
        while let Some(p) = cur.prerequisiti.first() {
            if catena.contains(p) || !validi.contains(*p) {
                break;
            }
            catena.push(p);
            cur = voci.iter().find(|v| v.rel == *p).expect("prerequisito nel banco");
        }
        lunga = lunga.max(catena.len());
    }
    assert!(lunga >= 4, "la catena più lunga è {lunga}: il vincolo non è esercitato");

    // Il ciclo: un item che si dichiara prerequisito di sé stesso.
    let ciclico = voci
        .iter()
        .find(|s| matches!(s.difetto, Defect::PrerequisitoCiclico(_)))
        .expect("nessun item ciclico");
    assert!(ciclico.prerequisiti.contains(&ciclico.rel));
    // E nessun item valido dipende da un item che il banco rifiuta: altrimenti
    // gli resterebbe appeso un prerequisito che non esiste.
    for s in voci.iter().filter(|s| !s.deve_essere_rifiutato()) {
        for p in s.prerequisiti {
            assert!(
                validi.contains(*p),
                "{} dipende da {p}, che il banco rifiuta",
                s.rel
            );
        }
    }
}

/// Il banco, visto dalla parte che un lettore guarda per primo: il referto
/// dichiara i controlli saltati con la loro ragione, e in CI i saltati sono
/// fallimenti.
#[test]
fn il_referto_dichiara_i_saltati_e_in_ci_sono_fallimenti() {
    let r = banco();
    assert_eq!(r.falliti(), 0, "{}", report_da(&r));
    assert_eq!(r.saltati(), 11, "undici controlli dipendono dalla pipeline");
    assert!(r.ok(), "senza pipeline il banco non deve fallire");
    assert!(!r.esito_con_rigidezza(true), "in CI i saltati sono fallimenti");
    for c in r.controlli.iter().filter(|c| c.di_pipeline) {
        assert!(matches!(c.esito, Esito::Saltato(_)), "{} non è saltato", c.nome);
    }
}

fn report_da(r: &kbs_fixtures::Referto) -> String {
    kbs_fixtures::report::in_testo(r, false)
}

/// I tipi di dominio che il banco costruisce devono restare coerenti: un
/// `SeqInSession` deve essere monotono e una catena di osservazioni non deve
/// ripetersi. È la proprietà che rende D6 verificabile quando la catena di
/// hash arriverà.
#[test]
fn le_osservazioni_hanno_una_sequenza_monotona_e_univoca() {
    let oss = corpus::osservazioni();
    assert!(!oss.is_empty());
    let mut seq: Vec<u64> = oss.iter().map(|o| o.seq.0).collect();
    let copia = seq.clone();
    seq.sort_unstable();
    assert_eq!(seq, copia, "le osservazioni non sono in ordine di sequenza");
    let n = seq.len();
    seq.dedup();
    assert_eq!(seq.len(), n, "due osservazioni hanno lo stesso SeqInSession");
    assert_eq!(seq[0], SeqInSession(1).0);
    // Nessun modello giudica: è D3 chiusa, ed è verificabile perché il tipo
    // non ha una variante che lo permetterebbe.
    for o in &oss {
        assert!(o.judged_by.is_some(), "un'osservazione senza giudicato");
    }
}

/// Una valutazione con contestazione dentro la riga, e non accanto.
#[test]
fn la_contestazione_vive_dentro_la_riga_del_voto() {
    let g = corpus::valutazione_con_contestazione();
    let c = g.contested.expect("la contestazione è dentro la riga");
    assert!(!c.reason.is_empty());
    assert!(c.outcome.is_some());
    assert_eq!(g.kind, kbs_core::GraderKind::Deterministic);
    // I tempi sono derivati dall'epoca fissa, non da `now()`: un banco i cui
    // tempi cambiano non è confrontabile.
    assert!(g.at.0 > Millis(0).0);
}

/// La ratifica **superata** esiste nel banco, e porta l'hash di un contratto
/// diverso da quello corrente.
/// Il caso è la condizione che `kbs_core::check_citable` trasforma in
/// `StaleRatification`, e su dati veri: un fixture in cui le due righe
/// coinciderebbero non eserciterebbe il percorso che D4 fa poggiare su
/// `check_citable`. Confrontare una variante di enum con sé stessa non dice
/// niente; dire che l'hash dichiarato è diverso da quello corrente sì.
#[test]
fn la_ratifica_superata_porte_un_hash_che_non_e_il_corrente() {
    // Il fixture non è scritto per nome: cercarlo per nome significherebbe che
    // il test si rompe quando la tabella cresce, e si romperebbe nel modo più
    // costoso — dichiarando che manca un caso che invece c'è.
    let s = items::voci()
        .into_iter()
        .find(|s| s.ratifica == RatificaSpec::Stale)
        .expect("il banco dichiara almeno una ratifica superata");
    assert_eq!(s.ratifica, RatificaSpec::Stale, "{} non dichiara una ratifica superata", s.rel);
    let argomento = corpus::argomento(&s, 0);
    let ratifica = argomento.ratified.as_ref().expect("una ratifica superata è una ratifica");
    assert_ne!(
        ratifica.contract_hash, argomento.content_hash,
        "se i due hash coincidessero la ratifica non sarebbe superata e il percorso \
         di `StaleRatification` non sarebbe esercitato"
    );
    assert!(matches!(
        check_citable(&argomento),
        Err(Invariant::StaleRatification { .. })
    ));
    // E la riga superata è un caso reale, non una costruzione: nel banco c'è
    // esattamente un item con ratifica superata, e dichiararlo evita che il
    // caso sparisca senza che nessuno lo noti.
    let superate: usize = items::voci()
        .iter()
        .filter(|s| s.ratifica == RatificaSpec::Stale)
        .count();
    let _ = s.rel;
    assert_eq!(superate, 1, "il banco deve avere un solo item con ratifica superata");
}

/// D3, resa **impossibile dal tipo**: nessun modello può emettere una claim o
/// un giudizio.
///
/// Il compilatore fa la parte difficile, ed è una parte vera: `chiusura` è un
/// `match` **senza** braccio `_` su un enum, quindi se `kbs-core` aggiunge una
/// variante il file smette di compilare. Non è una promessa, è un fatto, e
/// l'array che c'è qui sotto non lo era: un array di lunghezza fissa con
/// costruttori espliciti accetta silenziosamente una variante nuova, e
/// l'asserzione che lo confrontava con l'elenco non se ne accorgerebbe.
///
/// L'elenco che resta serve a un'altra cosa, ed è dichiarato: l'etichetta che
/// finisce nel registro e che un lettore umano legge. Quella non la deduce il
/// compilatore.
#[test]
fn nessun_modello_puo_emettere_una_claim_o_un_giudizio() {
    use kbs_core::{Emitter, GraderKind, Origin};

    /// L'etichetta di ogni giudicante, in un `match` chiuso.
    ///
    /// Il nome è quello che `kbs-core` serializza in kebab-case, ed è quello
    /// che finisce nel registro.
    fn etichetta_di_giudicante(k: GraderKind) -> &'static str {
        match k {
            GraderKind::Deterministic => "deterministic",
            GraderKind::Peer => "peer",
            GraderKind::Human => "human",
            GraderKind::Teacher => "teacher",
        }
    }

    /// L'etichetta di ogni emittente, in un `match` chiuso.
    fn etichetta_di_emittente(e: &Emitter) -> &'static str {
        match e {
            Emitter::Teacher { .. } => "teacher",
            Emitter::Content { .. } => "content",
            Emitter::FromWork { .. } => "fromwork",
        }
    }

    for k in [
        GraderKind::Deterministic,
        GraderKind::Peer,
        GraderKind::Human,
        GraderKind::Teacher,
    ] {
        let nome = etichetta_di_giudicante(k);
        // Il tipo chiama la variante `FromWork` e lo schema dichiarato la
        // chiama `from-work`: il confronto è sul nome senza trattini, che è
        // quello che finisce nel registro e che un lettore legge.
        let s = format!("{k:?}").to_lowercase();
        assert_eq!(s, nome, "l'etichetta del giudicante non è più quella del tipo");
        assert!(!s.contains("model") && !s.contains("llm") && !s.contains("ai"));
    }

    for e in [
        Emitter::Teacher { by: kbs_core::PersonId::fixture(1) },
        Emitter::Content { argument: kbs_core::ArgumentId::fixture(1) },
        Emitter::FromWork { observation: "obs".into() },
    ] {
        let nome = etichetta_di_emittente(&e);
        let s = format!("{e:?}").to_lowercase().replace('-', "");
        assert!(s.starts_with(nome), "l'etichetta dell'emittente non è più quella del tipo: {s}");
        assert!(!s.contains("model") && !s.contains("llm"));
    }
    // L'unico posto in cui un modello compare è l'origine: un **record** di
    // provenienza, non un attore. Il record porta id, hash del prompt, hash del
    // corpus e versione del generatore (D10), e senza hash del corpus non
    // c'è un registro, c'è un diario.
    let generato = Origin::Generated {
        lock: kbs_core::ModelLock {
            model_id: "claude-sonnet-4-5".into(),
            prompt_hash: "sha256:abc".into(),
            corpus_hash: "sha256:def".into(),
            generator_version: "kbs-intake/0.1.0".into(),
            at: Millis(0),
        },
        by: kbs_core::PersonId::fixture(1),
        at: Millis(0),
    };
    match &generato {
        Origin::Generated { lock, by, .. } => {
            assert!(!lock.prompt_hash.is_empty());
            assert!(!lock.corpus_hash.is_empty());
            assert!(!lock.generator_version.is_empty());
            assert_eq!(by.as_str(), "person_0001", "a ratificare c'è sempre una persona");
        }
        altro => panic!("l'origine generata deve restare generata: {altro:?}"),
    }
}
