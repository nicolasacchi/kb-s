//! La strada di authoring: l'esercizio che il file dichiara e il registro che
//! lo contiene.
//!
//! Il percorso esiste perché `Store::upsert_exercise` e `Store::put_instance`
//! non avevano un solo scrittore di produzione, e la parte che conta non è che
//! scriva: è **da dove viene `instances.expected`**. I test qui sotto non
//! asseriscono che una funzione sia stata chiamata; asseriscono che cosa finisce
//! nel registro e chi non lo vede.
//!
//! # L'ordine delle prove
//!
//! 1. un file che dichiara una famiglia **del catalogo** produce righe, e
//!    l'atteso è quello del generatore anche quando il file porta un
//!    `data-atteso` falso: la risposta non entra dalla porta del file;
//! 2. quello che è finito nel registro **si riproduce** (D11), e quindi l'id
//!    della riga è la tupla e non l'etichetta del corpus;
//! 3. il referto del verbo **non contiene la risposta**, perché un referto è un
//!    canale come un altro;
//! 4. le famiglie che il corpus reale dichiara **non sono nel catalogo**, e
//!    quindi non viene scritto niente e il risultato lo dice per nome;
//! 5. **uno studente iscritto non autorizza niente**, e il registro resta vuoto;
//! 6. il verbo è della CLI locale e non è un metodo dell'MCP.

mod common;

use std::path::{Path, PathBuf};

use kbs_core::{Millis, PersonId, Relation};
use kbs_exercise::families::Family;
use kbs_exercise::Generator;
use kbs_intake::cli::{self, Uscita};
use kbs_intake::mcp;
use kbs_intake::route::{self, Request, Route};
use kbs_store::{CourseRelation, Store};
use kbs_verify::{ReplayKey, ReplayRecord};

use common::*;

/// Il file che dichiara l'esercizio.
///
/// `data-atteso` porta una stringa che **non è** la risposta di nessuna delle
/// due istanze, e il percorso non la deve leggere: se la leggesse, l'atteso
/// sarebbe entrato dalla porta del file e questo test sarebbe verde senza che
/// il generatore produca niente.
const FILE_CON_FAMIGLIA_DEL_CATALOGO: &str = r#"<!doctype html>
<html lang="it">
<head><meta charset="utf-8"><title>Sviluppa</title></head>
<body><main>
<div class="esercizio" data-esercizio="ex_poly_1" data-famiglia="algebra-polinomio" data-checker="equivalence">
<p>Sviluppa il prodotto di due binomi di primo grado e scrivi la forma normale dichiarata.</p>
<p class="istanza" data-seed="s1">istanza s1</p>
<p class="istanza" data-seed="s2">istanza s2</p>
<p class="atteso" data-atteso="RISPOSTA-CHE-IL-FILE-NON-CONOSCE">Risposta attesa: non lo so.</p>
</div>
</main></body></html>
"#;

const RISPOSTA_FALSA: &str = "RISPOSTA-CHE-IL-FILE-NON-CONOSCE";
const SEMI: [&str; 2] = ["s1", "s2"];

fn marco() -> PersonId {
    PersonId("person_0007".into())
}

fn argomento() -> kbs_core::ArgumentId {
    kbs_core::ArgumentId::from_rel_path(REL)
}

struct Corsa {
    uscita: Uscita,
    stdout: String,
    stderr: String,
}

impl Corsa {
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(self.stdout.trim())
            .unwrap_or_else(|e| panic!("stdout non è JSON: {e}\n{}", self.stdout))
    }
}

fn esegui(args: &str) -> Corsa {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut inp = std::io::Cursor::new(Vec::new());
    let uscita = cli::esegui(
        &args.split_whitespace().map(str::to_string).collect::<Vec<_>>(),
        &mut out,
        &mut err,
        &mut inp,
    );
    Corsa {
        uscita,
        stdout: String::from_utf8(out).expect("utf-8"),
        stderr: String::from_utf8(err).expect("utf-8"),
    }
}

/// Un banco su file, perché `kbs autora` apre un percorso.
///
/// L'argomento è **in uso** e non in bozza: lo studente del banco è iscritto, e
/// `may_read` per un iscritto chiede lo stato. Senza pubblicazione il predicato
/// del lato studente risponderebbe no, e mascherebbe la sola cosa che il test
/// sta provando.
fn banco(d: &tempfile::TempDir) -> PathBuf {
    let mut s = store_con_corso();
    s.upsert_person(&kbs_store::Person {
        id: marco(),
        display_name: "Marco".into(),
        created_at: Millis(0),
    })
    .expect("lo studente");
    s.add_relation(
        &CourseRelation {
            person: marco(),
            course: corso(),
            relation: Relation::EnrolledIn,
            since: Millis(0),
            until: None,
        },
        &docente(),
    )
    .expect("iscrizione");
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
    .expect("l'argomento su cui si registra entra");
    s.ratify(&argomento(), &docente(), "verificato riga per riga")
        .expect("ratifica");
    s.publish(&argomento()).expect("pubblicazione");

    let percorso = d.path().join("k.sqlite");
    s.conn()
        .execute_batch(&format!(
            "VACUUM INTO '{}'",
            percorso.display().to_string().replace('\'', "''")
        ))
        .expect("VACUUM INTO: il banco del test deve diventare un file");
    percorso
}

fn scrivi(d: &tempfile::TempDir, nome: &str, contenuto: &str) -> PathBuf {
    let p = d.path().join(nome);
    std::fs::write(&p, contenuto).expect("il file del corpus");
    p
}

fn riga(store: &Store, tabella: &str) -> i64 {
    store
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {tabella}"), [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("conteggio su {tabella}: {e}"))
}

/// Il percorso del corpus reale, che il banco legge in `--file-reali`.
fn corpus_ite(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus-ite/esercizi")
        .join(nome)
}

#[test]
fn l_atteso_viene_dal_generatore_e_il_file_non_puo_farlo_entrare() {
    let d = tempfile::tempdir().expect("cartella temporanea");
    let dbp = banco(&d);
    let file = scrivi(&d, "corpo.html", FILE_CON_FAMIGLIA_DEL_CATALOGO);

    let c = esegui(&format!(
        "autora --db {} --person person_0001 --docente person_0001 --course {CORSO} --arg {REL} --file {}",
        dbp.display(),
        file.display()
    ));
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    let v = c.json();
    assert_eq!(v["command"], "autora");
    assert_eq!(v["declared"], 1);
    // **Due righe, non una**: l'id del registro è la tupla di D11, che contiene
    // il semi, e il checker di una riga è la risposta di quell'istanza. Un
    // checker solo non può stare nella riga di due risposte diverse.
    // Il commento sopra diceva «due righe» e l'asserzione diceva due: il
    // conteggio e' di righe **di registro**, quindi un semi ne fa una. L'id porta
    // dentro la tupla di D11, che contiene il semi: e' li che le righe sono
    // distinte, non che siano due.
    assert_eq!(v["written"], 1, "un semi, una riga: {v}");
    assert_eq!(v["not_computed"], 0);
    let esito = &v["outcomes"][0];
    assert_eq!(esito["esito"], "written");
    assert_eq!(esito["declared"], "ex_poly_1", "l'etichetta del corpus torna nel referto");

    let s = Store::open(&dbp).expect("il registro");
    let generatore = Family::PolyExpand.generator();
    let mut visti = Vec::new();
    for (i, seed) in SEMI.iter().enumerate() {
        let id = esito["exercises"][i].as_str().expect("id nel registro");
        visti.push(id.to_string());
        let istanza = &s
            .instances_of(&docente(), &corso(), id)
            .expect("il docente legge le istanze del proprio esercizio")[0];
        assert_eq!(istanza.seed, *seed);
        let atteso = generatore
            .build(*seed)
            .expect("istanza generabile")
            .0
            .expected;
        assert_eq!(
            istanza.expected, atteso,
            "`{id}`: l'atteso è quello del generatore"
        );
        assert_ne!(
            istanza.expected, RISPOSTA_FALSA,
            "`{id}`: l'atteso è entrato dalla porta del file"
        );
    }
    assert_ne!(visti[0], visti[1], "due semi, due id: la copia non è la stessa riga");
}

#[test]
fn quello_che_e_finito_nel_registro_si_riproduce() {
    let d = tempfile::tempdir().expect("cartella temporanea");
    let dbp = banco(&d);
    let file = scrivi(&d, "corpo.html", FILE_CON_FAMIGLIA_DEL_CATALOGO);
    let c = esegui(&format!(
        "autora --db {} --person person_0001 --docente person_0001 --course {CORSO} --arg {REL} --file {}",
        dbp.display(),
        file.display()
    ));
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    let esito = &c.json()["outcomes"][0];
    let s = Store::open(&dbp).expect("il registro");

    // Il replay è la prova che l'id scelto è giusto: è l'unica cosa che
    // distingue «una riga consultabile» da «una riga difendibile» (D11).
    for (i, seed) in SEMI.iter().enumerate() {
        let id = esito["exercises"][i].as_str().expect("id nel registro");
        let istanza = s
            .instances_of(&docente(), &corso(), id)
            .expect("istanze")[0]
            .clone();
        let record = ReplayRecord::new(
            ReplayKey::new("kbs-c1:banco", id, "poly-expand/1", *seed),
            istanza,
            Millis(0),
            docente(),
        )
        .expect("il record lega la tupla all'istanza");
        let verdetto = record
            .verify("kbs-c1:banco", &*Family::PolyExpand.instance_generator())
            .expect("replay eseguibile");
        assert!(
            verdetto.verdict().is_match(),
            "`{id}`: il registro contiene qualcosa che il generatore non rifà"
        );
    }
}

#[test]
fn il_referto_del_verbo_non_contiene_la_risposta() {
    let d = tempfile::tempdir().expect("cartella temporanea");
    let dbp = banco(&d);
    let file = scrivi(&d, "corpo.html", FILE_CON_FAMIGLIA_DEL_CATALOGO);
    let c = esegui(&format!(
        "autora --db {} --person person_0001 --docente person_0001 --course {CORSO} --arg {REL} --file {}",
        dbp.display(),
        file.display()
    ));
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    // Un referto è un canale come un altro, e questo canale esce dalla macchina
    // del docente. La risposta sta nel registro, che ha una porta.
    for seed in SEMI {
        let atteso = Family::PolyExpand
            .generator()
            .build(seed)
            .expect("istanza generabile")
            .0
            .expected;
        assert!(
            !c.stdout.contains(&atteso),
            "il referto di `kbs autora` contiene l'atteso di `{seed}`: {}",
            c.stdout
        );
    }
    assert!(
        !c.stdout.contains(RISPOSTA_FALSA),
        "{}",
        c.stdout
    );
}

#[test]
fn le_famiglie_del_corpus_reale_non_sono_nel_catalogo_e_nulla_viene_scritto() {
    let d = tempfile::tempdir().expect("cartella temporanea");
    let dbp = banco(&d);
    // Il corpus vero, non una sua copia: il punto di questo test è che la
    // risposta onesta esce anche sul materiale che il banco legge.
    let file = corpus_ite("01-costo-fisso-e-variabile.html");
    assert!(file.exists(), "il corpus reale non è dove questo test lo cerca: {}", file.display());

    let c = esegui(&format!(
        "autora --db {} --person person_0001 --docente person_0001 --course {CORSO} --arg {REL} --file {}",
        dbp.display(),
        file.display()
    ));
    // Non è un fallimento: è una dichiarazione. Il percorso ha girato, ha
    // letto il file e sa che cosa non sa fare.
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    let v = c.json();
    assert_eq!(v["declared"], 1, "il file dichiara un esercizio: {v}");
    assert_eq!(v["written"], 0);
    assert_eq!(v["not_computed"], 1);
    let esito = &v["outcomes"][0];
    assert_eq!(esito["esito"], "not-computed");
    assert_eq!(esito["family"], "costo-fisso-e-variabile");
    // Il motivo e il catalogo viaggiano **nello stesso oggetto**: «non
    // calcolato» senza l'elenco non dice se il nome è sbagliato o se la
    // famiglia non esiste, e la coppia è il punto.
    assert_eq!(esito["reason"]["motivo"], "family-outside-catalog");
    assert_eq!(
        esito["reason"]["catalog"].as_array().map(Vec::len),
        Some(5),
        "le cinque famiglie del catalogo chiuso: {esito}"
    );

    // E soprattutto: nessuna riga. Un esercizio senza generatore non entra,
    // perché il suo checker **è** la risposta e una risposta qui sarebbe stata
    // scritta a mano.
    let s = Store::open(&dbp).expect("il registro");
    assert_eq!(riga(&s, "exercises"), 0);
    assert_eq!(riga(&s, "instances"), 0);
}

#[test]
fn lo_studente_iscritto_non_autorizza_niente_e_il_registro_resta_vuoto() {
    let d = tempfile::tempdir().expect("cartella temporanea");
    let dbp = banco(&d);
    let file = scrivi(&d, "corpo.html", FILE_CON_FAMIGLIA_DEL_CATALOGO);

    // Marco vede l'argomento — è iscritto e l'argomento è in uso — e non insegna
    // il corso. È il caso che il predicato del lato generatore chiude, ed è
    // l'unico per cui la risposta sarebbe diventata leggibile da qualcuno che non
    // doveva.
    let prima = esegui(&format!("read --db {} --person person_0007 --arg {REL}", dbp.display()));
    assert_eq!(prima.uscita, Uscita::Ok, "lo studente vede l'argomento: {}", prima.stderr);

    let c = esegui(&format!(
        "autora --db {} --person person_0007 --docente person_0007 --course {CORSO} --arg {REL} --file {}",
        dbp.display(),
        file.display()
    ));
    assert_eq!(c.uscita, Uscita::Rifiutata, "non è un guasto: è un no");
    assert_eq!(c.json()["error"]["code"], "not-author");
    assert!(c.stderr.contains("person_0007"), "stderr: {}", c.stderr);

    let s = Store::open(&dbp).expect("il registro");
    assert_eq!(riga(&s, "exercises"), 0, "il rifiuto è avvenuto prima di scrivere");
    assert_eq!(riga(&s, "instances"), 0);
}

#[test]
fn il_verbo_e_della_cli_locale_e_non_e_un_metodo_mcp() {
    // La parte non negoziabile, come per `kbs insegna`: su un trasporto dove
    // l'identità è **dichiarata** (`x-kbs-person`, `?person=`) un metodo che
    // scrive il checker di un esercizio significa «dichiarati docente e leggi le
    // risposte». La domanda non è «funziona?» ma «esiste?», e la risposta è no.
    for metodo in ["autora", "kbs/autora", "kbs/esercizio", "kbs/istanze"] {
        let mut store = store_con_corso();
        let risposta = mcp::rispondi(
            &mut store,
            &serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": metodo,
                "params": { "person": "person_0001", "course": CORSO, "file": "x.html" },
            })
            .to_string(),
        );
        let r: serde_json::Value = serde_json::from_str(&risposta).expect("risposta JSON");
        assert_eq!(
            r["error"]["code"], -32601,
            "`{metodo}` ha risposto: la strada MCP scrive esercizi — {r}"
        );
        assert!(
            !mcp::METODI.contains(&metodo),
            "`{metodo}` è comparso nella lista dei metodi MCP"
        );
    }
}
