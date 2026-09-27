//! D6: la claim porta con sé **chi** l'ha emessa, e chi l'ha emessa esiste.
//!
//! Il registro delle affermazioni risponde a «chi ha dimostrato che cosa?». Se
//! la risposta è sempre «l'artifact che la contiene», il registro non distingue
//! un'affermazione scritta da un'affermazione osservata, e la ripetizione (D11)
//! non ha più niente su cui basarsi.
//!
//! Il caso che questo file copre è di scrittura, non di lettura: un corpus
//! scritto da più persone nomina persone che il database non ha ancora. È la
//! stessa riga che `scan::indexa` scrive per i **corsi** — `arguments.course_id`
//! referenzia `sources`, e un corso che nessuno ha registrato non è un corso —
//! applicata alle persone: `claims.emitted_by` referenzia `people`, e una
//! persona che nessuno ha registrato non è una persona.

mod common;

use kbs_core::Emitter;
use kbs_intake::scan;

use common::*;

/// Una seconda docente, che il negozio iniziale non conosce: è il punto.
const COLLEGA: &str = "person_0002";

fn corpus_di_un_file(sorgente: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("cartella temporanea");
    std::fs::create_dir_all(dir.path().join("letture")).expect("la sottocartella");
    std::fs::write(dir.path().join(REL), sorgente).expect("il file");
    dir
}

/// Il punto della precondizione, dichiarato come precondizione e non come
/// aspettativa: il negozio del test non conosce la collega, ed è per questo
/// che la scansione deve registrarla.
#[test]
fn la_collega_non_e_registrata_prima_della_scansione() {
    let s = store_con_corso();
    let r = s
        .read_argument(&docente(), &kbs_core::ArgumentId::from_rel_path(REL))
        .expect_err("l'argomento non c'è: il negozio è vuoto");
    assert!(
        matches!(r, kbs_store::Error::NotFound { .. } | kbs_store::Error::NotReadable { .. }),
        "il negozio di prova non deve avere l'argomento: {r:?}"
    );
    assert!(
        !esiste_la_persona(&s, COLLEGA),
        "la collega non deve essere nel registro delle persone"
    );
}

/// La persona è nel registro, letta dal database.
///
/// `kbs-store` non espone un `read_person`: il nome di una persona lo
/// possiede chi l'ha scritta, e leggerlo non è un'operazione di dominio che
/// il negozio deve offrire. Qui si legge la riga, ed è il posto giusto perché
/// la domanda è di un test.
fn esiste_la_persona(s: &kbs_store::Store, id: &str) -> bool {
    s.conn()
        .query_row("SELECT 1 FROM people WHERE id = ?1", [id], |_| Ok(()))
        .is_ok()
}

/// Il nome con cui la persona è registrata.
fn nome_della_persona(s: &kbs_store::Store, id: &str) -> Option<String> {
    s.conn()
        .query_row(
            "SELECT display_name FROM people WHERE id = ?1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .ok()
}

/// Una scansione che trova una claim emessa da una persona non registrata la
/// registra, e la persona con lei.
///
/// Il fallimento che questo test copre è un `FOREIGN KEY constraint failed`: la
/// riga di registro è rifiutata dal database, e con lei tutta la scansione. Il
/// sintomo — un errore SQLite senza nome — è il punto: senza questo test, un
/// corpus scritto da due persone fallirebbe per motivi che il referto non sa
/// spiegare.
#[test]
fn una_claim_emessa_da_una_persona_registra_la_persona() {
    let sorgente = artifact_con_emittente(COLLEGA);
    let dir = corpus_di_un_file(&sorgente);
    let mut s = store_con_corso();

    let scansione = scan::indexa(&mut s, dir.path(), &docente()).expect("la scansione non fallisce");
    let d = scansione
        .report
        .items
        .iter()
        .find(|d| d.rel_path == REL)
        .expect("il referto ha una riga per file");
    assert!(d.valid, "una claim con un emittente non rende il file rotto: {:?}", d.errors);

    assert!(
        esiste_la_persona(&s, COLLEGA),
        "la scansione ha registrato una claim emessa da {COLLEGA} senza registrare {COLLEGA}"
    );

    let riga = scansione
        .report
        .claims
        .iter()
        .find(|c| c.id.ends_with("::cl_1"))
        .expect("la claim dichiarata è nel referto");
    assert!(riga.in_registry, "la claim non è nel registro: {riga:?}");

    // E l'emittente nel registro è la persona, non l'artifact.
    let in_registro = s
        .claims_for(&docente(), &kbs_core::ArgumentId::from_rel_path(REL))
        .expect("le claim dell'argomento");
    let emittenti: Vec<&Emitter> = in_registro.iter().map(|c| &c.emitted_by).collect();
    assert!(
        emittenti
            .iter()
            .any(|e| matches!(e, Emitter::Teacher { by } if by.as_str() == COLLEGA)),
        "la claim porta l'emittente {emittenti:?} e non la persona che l'ha emessa"
    );
}

/// Una persona già nel registro non viene rinominata dalla scansione.
///
/// `upsert_person` sovrascrive `display_name`, e un nome è una cosa che
/// qualcun altro può aver scritto. Una scansione che riscrive i nomi delle
/// persone è una scansione che cancella i nomi: il caso è in `cli::esiste_persona`
/// ed è dichiarato anche lì.
#[test]
fn una_persona_già_registrata_conserva_il_suo_nome() {
    let persona = kbs_core::PersonId(COLLEGA.to_string());
    let mut s = store_con_corso();
    s.upsert_person(&kbs_store::Person {
        id: persona.clone(),
        display_name: "Ada".into(),
        created_at: kbs_core::Millis(0),
    })
    .expect("la persona è registrata");

    let dir = corpus_di_un_file(&artifact_con_emittente(COLLEGA));
    scan::indexa(&mut s, dir.path(), &docente()).expect("la scansione non fallisce");

    assert_eq!(
        nome_della_persona(&s, COLLEGA).as_deref(),
        Some("Ada"),
        "la scansione ha riscritto il nome di una persona che esisteva già"
    );
}

/// La persona registrata dalla scansione ha un nome, e un nome che non mente.
///
/// Il nome è l'unico posto in cui il sistema può dire «questa affermazione è
/// di Ada» a qualcuno che non sta leggendo l'id. Un id come nome è un indirizzo
/// senza indirizzato, che è la stessa cosa che D6Dice di uno span senza testo.
#[test]
fn la_persona_registrata_ha_un_nome_e_non_un_id() {
    let mut s = store_con_corso();
    let dir = corpus_di_un_file(&artifact_con_emittente(COLLEGA));
    scan::indexa(&mut s, dir.path(), &docente()).expect("la scansione non fallisce");

    let registrato = nome_della_persona(&s, COLLEGA).expect("la persona è nel registro");
    assert!(
        !registrato.trim().is_empty(),
        "la persona è stata registrata senza nome"
    );
    assert_ne!(
        registrato.trim(),
        COLLEGA,
        "il nome è l'id: un indirizzo senza indirizzato"
    );
}
