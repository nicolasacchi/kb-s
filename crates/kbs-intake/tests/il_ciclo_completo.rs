//! Il ciclo completo, in una esecuzione: `verify` → `insegna` → `ratify` →
//! `promote`.
//!
//! Il banco guarda `cli::esegui` e non le funzioni interne, come tutti i banchi
//! di questa CLI: ciò che vale è quello che l'insegnante vede, cioè il codice
//! di uscita, il JSON e la riga su stderr.
//!
//! Le tre cose verificate sono le tre che il verbo **dichiara**, e sono le
//! tre che un revisore attaccherebbe per prime:
//!
//! * **l'ordine** — la relazione c'è quando la porta guarda, e l'argomento
//!   firmato è citabile alla fine;
//! * **l'idempotenza** — due esecuzioni sullo stesso corpus non scrivono due
//!   volte la stessa riga, e la seconda lo dichiara (`gia`) invece di farlo
//!   indovinare da un `ok: true` identico;
//! * **il referto** — un percorso che non ha fatto tutto esce non zero, dice
//!   *quale* file non è arrivato in porto, e dichiara il buco degli esercizi.

mod common;

use kbs_core::{ArgumentId, PersonId};
use kbs_intake::cli::{self, Uscita};
use kbs_store::Store;

use common::*;

fn linea(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
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
    let mut inp = std::io::empty();
    let uscita = cli::esegui(&linea(args), &mut out, &mut err, &mut inp);
    Corsa {
        uscita,
        stdout: String::from_utf8(out).expect("utf-8"),
        stderr: String::from_utf8(err).expect("utf-8"),
    }
}

/// Un corpus di un file solo, come lo troverebbe un docente che parte da zero.
fn corpus(sorgente: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("cartella temporanea");
    std::fs::create_dir_all(dir.path().join("letture")).expect("la sottocartella");
    std::fs::write(dir.path().join(REL), sorgente).expect("il file");
    dir
}

fn riga(corpus: &std::path::Path, dbp: &std::path::Path) -> String {
    format!(
        "ciclo {} --db {} --person person_0001 --docente person_0001",
        corpus.display(),
        dbp.display()
    )
}

#[test]
fn il_ciclo_indicizza_insegna_ratifica_e_promuove() {
    let corpus = corpus(&artifact(false));
    let db = tempfile::tempdir().expect("cartella temporanea");
    let dbp = db.path().join("k.sqlite");

    let c = esegui(&riga(corpus.path(), &dbp));
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    let v = c.json();
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "ciclo");
    // I numeri che un docente guarda per primo: quanti file sono entrati.
    assert_eq!(v["file"], 1);
    assert_eq!(v["file_entrati"], 1);
    assert_eq!(v["corso"], serde_json::json!([CORSO]));
    // E i tre atti, uno per uno, nelle due forme in cui possono finire.
    assert_eq!(v["relazioni"]["nuove"], 1);
    assert_eq!(v["ratifiche"]["nuove"], 1);
    assert_eq!(v["promozioni"]["nuove"], 1);
    assert_eq!(v["relazioni"]["gia"], 0);
    assert_eq!(v["ratifiche"]["gia"], 0);
    assert_eq!(v["promozioni"]["gia"], 0);
    assert_eq!(v["non_promossi"].as_array().expect("la lista").len(), 0);

    // Il buco dichiarato: questo percorso non produce le istanze degli
    // esercizi, e il referto lo dice invece di farlo scoprire. Su questo
    // artifact gli esercizi dichiarati sono zero, e le istanze sono zero per
    // una ragione diversa — il campo è dichiarato lo stesso.
    assert_eq!(v["esercizi"]["istanze"], 0);
    assert_eq!(v["esercizi"]["dichiarati"], 0);
    let testo = v["testo"].as_str().expect("il referto in parole");
    assert!(testo.contains("NON FATTO"), "{testo}");
    assert!(testo.contains("istanze"), "{testo}");

    // Il punto di tutto il percorso: l'argomento è citabile. Non è una
    // promessa sul registro — è `kbs_core::check_citable` che lo dice, e lo
    // dice sugli stessi byte che il docente ha indicizzato.
    let s = Store::open(&dbp).expect("il database");
    let argomento = s
        .read_argument(&PersonId("person_0001".into()), &ArgumentId::from_rel_path(REL))
        .expect("l'argomento");
    assert!(
        argomento.is_citable_now(),
        "un argomento ratificato sul contratto corrente e promosso è citabile"
    );
}

#[test]
fn due_esecuzioni_sullo_stesso_corpus_non_doppiano_nulla() {
    let corpus = corpus(&artifact(false));
    let db = tempfile::tempdir().expect("cartella temporanea");
    let dbp = db.path().join("k.sqlite");

    let prima = esegui(&riga(corpus.path(), &dbp));
    assert_eq!(prima.uscita, Uscita::Ok, "stderr: {}", prima.stderr);
    let seconda = esegui(&riga(corpus.path(), &dbp));
    assert_eq!(seconda.uscita, Uscita::Ok, "stderr: {}", seconda.stderr);

    // La seconda esecuzione **dichiara** che non ha scritto niente. Un `ok`
    // identico nelle due forme senza dire quale è successa sarebbe
    // l'ambiguità che questo protocollo dichiara di non avere.
    let v = seconda.json();
    assert_eq!(v["relazioni"]["nuove"], 0);
    assert_eq!(v["relazioni"]["gia"], 1);
    assert_eq!(v["ratifiche"]["nuove"], 0);
    assert_eq!(v["ratifiche"]["gia"], 1);
    assert_eq!(v["promozioni"]["nuove"], 0);
    assert_eq!(v["promozioni"]["gia"], 1);

    // E le tabelle hanno una riga sola. `relations` ha `since` nella PK e
    // `arguments` ha l'id che segue il percorso: senza questa domanda un
    // verbo rieseguito a mano riemPIREbbe il registro di riprese di ruolo che
    // nessuno ha chiesto.
    let s = Store::open(&dbp).expect("il database");
    let relazioni: i64 = s
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM relations WHERE relation = 'teaches'",
            [],
            |r| r.get(0),
        )
        .expect("il conteggio");
    assert_eq!(relazioni, 1);
    let argomenti: i64 = s
        .conn()
        .query_row("SELECT COUNT(*) FROM arguments", [], |r| r.get(0))
        .expect("il conteggio");
    assert_eq!(argomenti, 1);
}

#[test]
fn un_file_che_non_si_puo_promuovere_non_finisce_finito_e_il_referto_lo_dice() {
    // L'artifact che referenzia una CDN: entra, e la porta lo chiude (D15). Un
    // ciclo che lo promotes lo farebbe sparire dal referto, e un ciclo che si
    // fermasse sul primo file non direbbe nulla degli altri.
    let corpus = corpus(&artifact_con_cdn());
    let db = tempfile::tempdir().expect("cartella temporanea");
    let dbp = db.path().join("k.sqlite");

    let c = esegui(&riga(corpus.path(), &dbp));
    // Non è un errore d'uso e non è un incidente: è «non è citabile», e il
    // codice è quello della regola, non quello del sistema.
    assert_eq!(c.uscita, Uscita::Rifiutata, "stderr: {}", c.stderr);
    let v = c.json();
    assert_eq!(v["ok"], false);
    // Il file **è** stato ricevuto: il referto lo conta lo stesso, perché
    // «non è entrato» e «non è stato promosso» sono due righe diverse.
    assert_eq!(v["file"], 1);
    let non_promossi = v["non_promossi"].as_array().expect("la lista");
    assert_eq!(non_promossi.len(), 1, "un file che non entra ha una riga");
    assert_eq!(non_promossi[0]["rel_path"], REL);
    assert!(
        !non_promossi[0]["codice"].as_str().unwrap_or("").is_empty(),
        "la riga nomina il codice, non solo il fatto"
    );
    assert!(
        !non_promossi[0]["motivo"].as_str().unwrap_or("").is_empty(),
        "la riga nomina la ragione"
    );
    // E la ragione è su stderr, perché stderr è la lingua dell'umano e il
    // JSON è quella dell'agente.
    assert!(c.stderr.contains("verdetto-bloccante"), "stderr: {}", c.stderr);
}

#[test]
fn il_ciclo_non_ha_un_docente_di_default() {
    // Un default su `--docente` scriverebbe `teaches` a qualcuno che non l'ha
    // chiesto, che è la definizione di `teaches` in `kbs_core::may_read`. Il
    // default su `--person` scrivrebbe `recorded_by` a vuoto. Le due esistono,
    // quindi l'esito è `opzione-mancante` e il codice è 3 — e il ciclo non
    // scrive niente, nemmeno il database.
    let corpus = corpus(&artifact(false));
    let db = tempfile::tempdir().expect("cartella temporanea");
    let dbp = db.path().join("k.sqlite");

    let c = esegui(&format!(
        "ciclo {} --db {} --person person_0001",
        corpus.path().display(),
        dbp.display()
    ));
    assert_eq!(c.uscita, Uscita::Uso, "stderr: {}", c.stderr);
    assert_eq!(c.json()["error"]["code"], "opzione-mancante");
    assert!(!dbp.exists(), "un ciclo fermato prima di cominciare non apre il database");
}
