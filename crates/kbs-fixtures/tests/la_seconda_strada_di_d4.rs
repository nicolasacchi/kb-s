//! La **seconda strada di D4**, esercitata contro il binario vero.
//!
//! # Che cosa c'è qui e che cosa non c'è
//!
//! Qui non ci sono test che costruiscono un `kbs_core::Argument` con
//! `ratified: Some(…)` e lo confrontano con un indice. Un oggetto così non
//! esiste in nessun file e in nessun database: esiste solo nella tabella Rust
//! del banco, e confrontarlo con il sistema è confrontare un'intenzione con
//! una realtà. Il risultato è un test che è rosso per costruzione e che non
//! diventa verde correggendo il sistema.
//!
//! Qui ci sono gli atti che una scuola percorre davvero, e ognuno attraversa
//! **il processo**:
//!
//! 1. `kbs verify` su un database vuoto — nessuno ha ratificato, e quindi
//!    l'indice non cita niente;
//! 2. `kbs promote` su ciò che il banco dichiara ratificato di fresco —
//!    l'atto del docente, e l'unico modo in cui una ratifica entra;
//! 3. `kbs verify` sullo **stesso** database — e adesso l'indice cita
//!    esattamente il gruppo promosso;
//! 4. la stessa strada su una **copia**, con il contratto di un item
//!    riscritto sotto la ratifica già firmata — l'item esce dall'indice per
//!    `stale-ratification`, e non esce nessun altro.
//!
//! # Perché la sessione è eseguita una volta sola
//!
//! Perché gli atti 1, 2 e 3 sono **una** sessione della pipeline, non tre: il
//! banco li esegue una volta e ne ricava tre letture, ed è la stessa cosa che
//! fanno i quattro controlli del referto. Rieseguire la sequenza in ogni test
//! sarebbe la stessa verifica quattro volte al quadruplo del costo, e un test
//! che ripete il setup non è più indipendente: è più lento.
//!
//! Ogni test legge un capo diverso della stessa corsa, e **ciascuno fallisce
//! da solo** se il sistema ha smesso di onorare il proprio atto: se la porta
//! smette di accettare, fallisce il test della promozione; se la ratifica
//! smette di rendere citabile, fallisce il test dell'indice; se l'indice
//! ignora la ratifica, fallisce il test del database vuoto. Condividere la
//! corsa non condivide le conclusioni.
//!
//! # Perché questi test falliscono se il binario non c'è
//!
//! Perché un test che salta quando non può verificare è un test che un giorno
//! non verifica niente e non lo dice. Qui il binario è un **atto** della
//! verifica: se manca, la capacità non è dimostrata, e la riga del README che
//! la dichiara `dimostrata` sarebbe falsa. La CI costruisce il workspace prima
//! di girare i test, quindi il binario c'è; e su una macchina in cui non c'è,
//! il messaggio dice come averlo invece di passare in silenzio.
//!
//! `KBS_BIN` ha la precedenza, come nel banco.

use kbs_core::PublicationState;
use kbs_fixtures::adapter::{Atto, Eseguito, Pipeline, ProcessPipeline, Session, Uscita};
use kbs_fixtures::contract::Defect;
use kbs_fixtures::corpus::{self, Corpus};
use kbs_fixtures::items;
use kbs_fixtures::spec::{self, RatificaSpec, Spec};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Il binario della pipeline, o un messaggio che dice come averlo.
fn pipeline() -> ProcessPipeline {
    ProcessPipeline::cerca().unwrap_or_else(|e| {
        panic!(
            "{e}\n\
             Questo test parla con il binario `kbs` e non ha niente da dire senza di lui.\n\
             Costruiscilo con `./kc build -p kbs-intake --bin kbs`, oppure punta `KBS_BIN` \
             al binario che vuoi usare."
        )
    })
}

/// La radice del corpus su disco: quella del crate, che è ciò che il banco
/// indicizza. Non è la radice del workspace, e non è una copia: gli atti 1, 2
/// e 3 devono parlare del corpus che è committato, altrimenti il banco
/// misurerebbe qualcosa che nessuno vedrà.
fn radice_del_banco() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// Le voci che il banco dichiara ratificate di fresco: le uniche su cui ha
/// senso chiedere alla porta di accettare.
static FRESCHE: LazyLock<Vec<Spec>> = LazyLock::new(|| {
    items::voci()
        .into_iter()
        .filter(|s| s.ratifica == RatificaSpec::Fresca)
        .collect()
});

fn fresche() -> &'static [Spec] {
    &FRESCHE
}

/// L'item su cui il banco riscrive il contratto sotto una ratifica viva. Le
/// quattro condizioni sono le stesse del banco, e sono dichiarate perché ognuna
/// esclude un caso in cui il caso non sarebbe quello: senza ratifica non c'è
/// niente da far superare, un item fuori uso non si promuove, un contratto
/// rotto uscirebbe dall'indice per la validazione, e senza prerequisiti da
/// togliere la riscrittura non cambierebbe l'hash.
static RISCRIVIBILE: LazyLock<Spec> = LazyLock::new(|| {
    items::voci()
        .into_iter()
        .find(|s| {
            s.ratifica == RatificaSpec::Fresca
                && s.stato == PublicationState::InCorso
                && s.difetto == Defect::Nessuno
                && !s.prerequisiti.is_empty()
        })
        .expect("il banco dichiara almeno un item in uso, ratificato di fresco, senza difetti e con un prerequisito")
});

fn riscrivibile() -> &'static Spec {
    &RISCRIVIBILE
}

fn gira(p: &ProcessPipeline, sessione: &Session, atti: &[Atto]) -> Vec<Eseguito> {
    p.sequenza(&Corpus::dalla_tabella(), sessione, atti)
        .unwrap_or_else(|e| panic!("la pipeline non ha eseguito la sequenza: {e}"))
}

fn referto(eseguiti: &[Eseguito], i: usize) -> Uscita {
    eseguiti
        .get(i)
        .and_then(|e| e.referto.clone())
        .unwrap_or_else(|| panic!("l'atto numero {i} non ha prodotto un referto"))
}

/// I percorsi relativi degli id che l'indice dichiara citabili. Un id che il
/// banco non conosce resta come è: un id sconosciuto è un problema della
/// pipeline e va detto per intero, non tradotto in silenzio.
fn citati(c: &Corpus, u: &Uscita) -> BTreeSet<String> {
    u.index
        .citable
        .iter()
        .map(|id| {
            c.voci()
                .iter()
                .find(|s| corpus::id_di(s).as_str() == id)
                .map(|s| s.rel.to_string())
                .unwrap_or_else(|| id.clone())
        })
        .collect()
}

/// Gli atti 1, 2 e 3 sul corpus di lavoro: una verifica, una promozione per
/// ogni ratifica fresca, e una seconda verifica sullo **stesso** database.
fn atti_del_banco(radice: &Path) -> Vec<Atto> {
    let mut atti = vec![Atto::verify(radice)];
    for s in fresche() {
        atti.push(Atto::promote(radice, spec::OPERATORE, s.rel));
    }
    atti.push(Atto::verify(radice));
    atti
}

// ─────────────────────────────────────────────────────────────────────────────
// La corsa, eseguita una volta sola
// ─────────────────────────────────────────────────────────────────────────────

/// Gli atti 1, 2 e 3, e le tre letture che se ne ricavano.
struct Corpo {
    /// `verify` sul database appena creato.
    vuoto: Uscita,
    /// Un atto di promozione per ogni ratifica fresca della tabella.
    promozioni: Vec<Eseguito>,
    /// `verify` sullo stesso database, dopo le promozioni.
    dopo: Uscita,
    _sessione: Session,
}

/// La strada sulla copia: promuovere, cambiare il contratto, verificare.
struct Copia {
    prima: Uscita,
    promozioni: Vec<Eseguito>,
    dopo_il_cambio: Uscita,
    _cartella: tempfile::TempDir,
    _sessione: Session,
}

static CORPO: LazyLock<Corpo> = LazyLock::new(|| {
    {
        let p = pipeline();
        let sessione = Session::nuova().expect("sessione");
        let atti = atti_del_banco(&radice_del_banco());
        let eseguiti = gira(&p, &sessione, &atti);
        let n = fresche().len();
        Corpo {
            vuoto: referto(&eseguiti, 0),
            promozioni: eseguiti[1..=n].to_vec(),
            dopo: referto(&eseguiti, n + 1),
            _sessione: sessione,
        }
    }
});

fn corpo() -> &'static Corpo {
    &CORPO
}

static COPIA: LazyLock<Copia> = LazyLock::new(|| {
    {
        let p = pipeline();
        let c = Corpus::dalla_tabella();
        let rel = riscrivibile().rel;
        let cartella = tempfile::tempdir().expect("cartella");
        let radice = cartella.path().join("corpus");
        // La copia, prima: identica al banco. E il banco lo deve poter dire.
        c.scrivi_in(&radice)
            .unwrap_or_else(|e| panic!("{}: {e}", radice.display()));

        let sessione = Session::nuova().expect("sessione");
        let eseguiti = gira(&p, &sessione, &atti_del_banco(&radice));
        let n = fresche().len();
        let prima = referto(&eseguiti, n + 1);
        let promozioni = eseguiti[1..=n].to_vec();

        // **Il banco corregge il proprio contratto, dopo la firma.** È il
        // docente che lo fa, non la pipeline: l'atto è dichiarato qui perché
        // un caso che il banco crede di esercitare e non è quello è peggio di
        // un caso assente.
        c.con_contratto_riscritto(rel)
            .scrivi_in(&radice)
            .unwrap_or_else(|e| panic!("{}: {e}", radice.display()));

        // E la terza lettura, sullo **stesso** database: la ratifica esiste
        // ancora, e vale per un testo che non è più quello firmato.
        let terza = gira(&p, &sessione, &[Atto::verify(&radice)]);
        Copia {
            prima,
            promozioni,
            dopo_il_cambio: referto(&terza, 0),
            _cartella: cartella,
            _sessione: sessione,
        }
    }
});

fn copia() -> &'static Copia {
    &COPIA
}

/// Gli item che la porta ha promosso, per percorso relativo.
fn promossi(atti: &[Eseguito]) -> BTreeSet<String> {
    atti.iter()
        .filter(|e| e.ok())
        .filter_map(|e| e.atto.soggetto.clone())
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Atto 1 — su un database in cui nessuno ha agito, niente è citabile
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn atto_1_su_un_database_vuoto_nessun_item_e_citabile() {
    let c = Corpus::dalla_tabella();
    let u = &corpo().vuoto;

    assert!(
        u.index.citable.is_empty(),
        "un database in cui nessuno ha ratificato ha {} argomenti citabili ({:?}): un item non ratificato \
         è leggibile, e leggibile non è citabile",
        u.index.citable.len(),
        u.index.citable
    );

    // Non è un indice vuoto per un corpus vuoto: ogni item della tabella deve
    // dire perché non è citabile, e la ragione deve essere quella giusta.
    assert_eq!(u.index.not_citable.len(), items::DIMENSIONE);
    for spec in items::voci() {
        let id = corpus::id_di(&spec);
        let riga = u
            .index
            .not_citable
            .iter()
            .find(|n| n.id == id.as_str())
            .unwrap_or_else(|| panic!("{}: la pipeline non ha detto perché non è citabile", spec.rel));
        assert_eq!(
            riga.invariant, "citable-without-ratification",
            "{}: l'invariante è «{}» e su un database vuoto l'unica ragione possibile è che la ratifica manca",
            spec.rel, riga.invariant
        );
        assert!(
            !riga.message.trim().is_empty(),
            "{}: non citabile senza motivo dichiarato",
            spec.rel
        );
    }
    // E il corpus ha qualcosa di ratificabile: un indice vuoto su un corpus in
    // cui non c'è niente da ratificare è vero e non dimostra niente.
    assert!(
        !fresche().is_empty(),
        "la tabella non dichiara nessuna ratifica fresca: l'indice vuoto non dimostra che la ratifica sia ciò che manca"
    );
    assert!(citati(&c, u).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Atto 2 — la promozione è l'atto del docente, ed entra
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn atto_2_la_promozione_e_l_atto_del_docente_e_la_porta_la_ragiona() {
    let promozioni = &corpo().promozioni;
    assert_eq!(
        promozioni.len(),
        fresche().len(),
        "un atto di promozione per ogni ratifica fresca, e non uno di meno"
    );

    let mut accettate = 0usize;
    let mut rifiutate_in_uso = Vec::new();
    for (spec, e) in fresche().iter().zip(promozioni) {
        assert_eq!(e.atto.soggetto.as_deref(), Some(spec.rel));
        if e.ok() {
            accettate += 1;
            continue;
        }
        // Un rifiuto è un verdetto, e un verdetto si contesta solo se ha una
        // ragione. «La porta ha detto no» senza dire perché non è una risposta.
        assert!(
            !e.motivo().trim().is_empty(),
            "{}: la porta ha rifiutato la promozione senza dire perché",
            spec.rel
        );
        if spec.stato == PublicationState::InCorso {
            rifiutate_in_uso.push(format!("{} — {}", spec.rel, e.motivo()));
        }
    }
    assert!(
        rifiutate_in_uso.is_empty(),
        "la tabella li dichiara in uso e la porta ha rifiutato di promuoverli: {:?}",
        rifiutate_in_uso
    );
    assert!(
        accettate > 0,
        "nessuna promozione accettata: l'atto del docente non è mai entrato, e gli atti 3 e 4 avrebbero misurato un sistema a cui nessuno ha chiesto niente"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Atto 3 — dopo la promozione, è citabile esattamente il gruppo promosso
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn atto_3_dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso() {
    let c = Corpus::dalla_tabella();
    let corpo = corpo();
    let promossi = promossi(&corpo.promozioni);
    assert!(
        !promossi.is_empty(),
        "nessun item promosso: il confronto con il citabile sarebbe tra due insiemi vuoti e direbbe niente"
    );

    let citati = citati(&c, &corpo.dopo);
    let mancanti: Vec<&String> = promossi.difference(&citati).collect();
    assert!(
        mancanti.is_empty(),
        "promossi e non citati: {mancanti:?} — la ratifica non rende citabile"
    );
    let extra: Vec<&String> = citati.difference(&promossi).collect();
    assert!(
        extra.is_empty(),
        "citati e non promossi: {extra:?} — un item non ratificato non entra nell'indice condiviso"
    );
    assert_eq!(citati.len(), promossi.len());
}

// ─────────────────────────────────────────────────────────────────────────────
// Atto 4 — un contratto riscritto sotto una ratifica viva esce dal citabile
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn atto_4_il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile() {
    let c = Corpus::dalla_tabella();
    let spec = *riscrivibile();
    let id = corpus::id_di(&spec);
    let rel = spec.rel;
    let copia = copia();

    // **Era** citabile prima della riscrittura. Senza questo il banco
    // confronterebbe uno stato con uno stato, e non una transizione.
    let prima = citati(&c, &copia.prima);
    assert!(
        prima.contains(rel),
        "{rel}: sulla copia, dopo la promozione, non era citabile, e non c'è nessuna ratifica viva da far superare"
    );
    assert!(
        promossi(&copia.promozioni).contains(rel),
        "{rel}: la porta non l'ha promosso sulla copia"
    );

    let u = &copia.dopo_il_cambio;
    let citati = citati(&c, u);
    assert!(
        !citati.contains(rel),
        "{rel}: il contratto è stato riscritto dopo la firma e l'indice lo cita ancora — la ratifica vale per un testo che non è più quello firmato"
    );
    let riga = u
        .index
        .not_citable
        .iter()
        .find(|n| n.id == id.as_str())
        .unwrap_or_else(|| panic!("{rel}: la pipeline non ha detto perché non è più citabile"));
    assert_eq!(
        riga.invariant, "stale-ratification",
        "{rel}: l'invariante è «{}» e quello della ratifica superata è «stale-ratification»",
        riga.invariant
    );
    // «Superata» senza i due hash è una parola, e una parola non è una verifica.
    let mut hash: Vec<&str> = riga
        .message
        .split("sha256:")
        .skip(1)
        .map(|h| h.split(|c: char| !c.is_ascii_hexdigit()).next().unwrap_or(""))
        .filter(|h| !h.is_empty())
        .collect();
    hash.sort_unstable();
    hash.dedup();
    assert!(
        hash.len() >= 2,
        "{rel}: il motivo non nomina i due hash che non coincidono: {}",
        riga.message
    );

    // L'item è ancora **valido**: se fosse uscito perché il contratto lo
    // rifiuta, il banco avrebbe misurato la validazione e avrebbe dichiarato
    // che la ratifica è superata.
    let d = u
        .item(rel)
        .unwrap_or_else(|| panic!("{rel}: la pipeline non ha riportato l'item"));
    assert!(
        d.valid,
        "{rel}: il contratto riscritto non è valido ({:?}) e l'item sarebbe uscito per la validazione, non per la ratifica",
        d.errors.iter().map(|e| e.code.as_str()).collect::<Vec<_>>()
    );

    // E nessun altro esce: una riscrittura che svuota l'indice non ha
    // dimostrato che la ratifica conti, ha dimostrato che qualcosa si è rotto.
    let usciti: Vec<&String> = prima.difference(&citati).filter(|r| *r != rel).collect();
    assert!(
        usciti.is_empty(),
        "usciti dall'indice insieme all'item riscritto, senza che la loro ratifica fosse toccata: {usciti:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// La copia è una copia: un file e uno solo
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn la_copia_differisce_dal_banco_in_un_file_e_in_uno_solo() {
    let c = Corpus::dalla_tabella();
    let rel = riscrivibile().rel;
    let riscritta = c.con_contratto_riscritto(rel);
    assert_ne!(
        c.hash(),
        riscritta.hash(),
        "la copia è identica al banco: il caso non è esercitato"
    );

    let diversi: Vec<&str> = c
        .file()
        .iter()
        .filter(|f| {
            riscritta
                .file()
                .iter()
                .find(|g| g.rel == f.rel)
                .is_some_and(|g| g.contenuto != f.contenuto)
        })
        .map(|f| f.rel.as_str())
        .collect();
    assert_eq!(
        diversi,
        vec![rel],
        "la copia differisce in {diversi:?} e non in {rel} soltanto: il banco misurerebbe un caso che non è quello che dichiara"
    );

    // E la differenza è nel **contratto**, che è ciò che la ratifica confronta.
    let prima = c.file_di(rel).expect("il file del banco");
    let dopo = riscritta.file_di(rel).expect("il file della copia");
    assert!(prima.contenuto.contains("## PREREQUISITI"));
    assert!(
        dopo.contenuto.contains("Nessun prerequisito."),
        "il contratto riscritto non ha perso il prerequisito: l'hash non cambia e il banco starebbe misurando niente"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Il banco, visto dai suoi stessi controlli
// ─────────────────────────────────────────────────────────────────────────────

/// I quattro controlli di D4 esistono, hanno un nome ciascuno, e nessuno dei
/// quattro è un ramo di un altro: quattro atti, quattro nomi.
///
/// Il test non ha bisogno della pipeline, e non la chiede: qui si verifica che
/// i controlli siano **nominati e distinti**. Che siano verdi è la parte che il
/// banco dichiara, e che il passo di CI che lo gira verifica.
#[test]
fn i_quattro_controlli_di_d4_hanno_quattro_nomi_distinti() {
    use kbs_fixtures::adapter::PipelineAssente;
    use kbs_fixtures::checks::{Banco, Config, Esito};
    let assente = PipelineAssente { ragione: "binario assente".into() };
    let r = Banco::new(Config::radice_di_default(), &assente).esegui();
    for n in [
        "pipeline.d4.su_un_corpus_non_ratificato_niente_e_citabile",
        "pipeline.d4.la_promozione_e_l_atto_del_docente",
        "pipeline.d4.dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso",
        "pipeline.d4.il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile",
    ] {
        let c = r
            .controllo(n)
            .unwrap_or_else(|| panic!("{n}: il banco non esegue questo controllo"));
        assert!(
            c.di_pipeline,
            "{n}: è un controllo sulla pipeline, e senza pipeline è saltato con la ragione"
        );
        assert!(
            matches!(c.esito, Esito::Saltato(_)),
            "{n}: senza pipeline un controllo è saltato, mai superato"
        );
    }
}

/// E con la pipeline vera, il banco è verde e i quattro controlli sono fra
/// quelli che ha superati. Questo è il test che dice che la seconda strada è
/// stata percorsa e non solo scritta.
#[test]
fn il_banco_verde_ha_i_quattro_controlli_di_d4_superati() {
    use kbs_fixtures::checks::{Banco, Config};
    let p = pipeline();
    let r = Banco::new(Config::radice_di_default(), &p).esegui();
    for n in [
        "pipeline.d4.su_un_corpus_non_ratificato_niente_e_citabile",
        "pipeline.d4.la_promozione_e_l_atto_del_docente",
        "pipeline.d4.dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso",
        "pipeline.d4.il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile",
    ] {
        let c = r
            .controllo(n)
            .unwrap_or_else(|| panic!("{n}: il banco non esegue questo controllo"));
        assert_eq!(
            c.esito.etichetta(),
            "superato",
            "{n}: {}",
            match &c.esito {
                kbs_fixtures::Esito::Fallito(p) => p.join(" / "),
                kbs_fixtures::Esito::Saltato(w) => format!("saltato — {w}"),
                kbs_fixtures::Esito::NonValutabile(w) => format!("non valutabile — {w}"),
                kbs_fixtures::Esito::Superato => "superato".into(),
            }
        );
    }
    assert_eq!(
        r.falliti(),
        0,
        "il banco ha controlli rossi: {:?}",
        r.controlli
            .iter()
            .filter(|c| matches!(c.esito, kbs_fixtures::Esito::Fallito(_)))
            .map(|c| c.nome)
            .collect::<Vec<_>>()
    );
}
