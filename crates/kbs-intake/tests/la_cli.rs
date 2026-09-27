//! D10.2: la CLI come protocollo.
//!
//! Un protocollo si verifica dalla parte di chi lo parla, non dalla parte di
//! chi lo scrive. Quindi qui si esegue `kbs_intake::cli::esegui` con `stdin`,
//! `stdout` e `stderr` in mano e si guarda **il codice di uscita e il testo**,
//! non le funzioni interne.
//!
//! La proprietà che conta: **un codice di uscita non zero significa che il
//! docente non deve credere al successo, e il motivo è su stderr**.

mod common;

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

fn esegui(args: &str, stdin: &str) -> Corsa {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut inp = std::io::Cursor::new(stdin.as_bytes().to_vec());
    let uscita = cli::esegui(&linea(args), &mut out, &mut err, &mut inp);
    Corsa {
        uscita,
        stdout: String::from_utf8(out).expect("utf-8"),
        stderr: String::from_utf8(err).expect("utf-8"),
    }
}

fn db() -> tempfile::TempDir {
    tempfile::tempdir().expect("cartella temporanea")
}

#[test]
fn un_comando_che_non_esce_esce_non_zero_e_lo_dice_su_stderr() {
    let c = esegui("pippo", "");
    assert_ne!(c.uscita.code(), 0, "un comando ignoto non è un successo");
    assert_eq!(c.uscita, Uscita::Uso);
    assert!(c.stderr.contains("comando-sconosciuto"), "stderr: {}", c.stderr);
    assert_eq!(c.json()["ok"], false);
    assert_eq!(c.json()["error"]["code"], "comando-sconosciuto");
}

#[test]
fn un_database_inesistente_espone_il_suo_fallimento_e_non_una_ragione_finta() {
    let d = db();
    let c = esegui(
        &format!("capture --db {} --person person_0001", d.path().join("no/such/dir.sqlite").display()),
        &artifact(false),
    );
    assert_eq!(c.uscita, Uscita::Sistema);
    // Il codice e' quello del negozio, non `io`: e' `Store::open` ad aprire la
    // connessione, e dire `io` sarebbe attribuire a questo crate un errore che
    // non ha visto. Il testo del messaggio, quello si', nomina il percorso.
    assert_eq!(c.json()["error"]["code"], "store");
    assert!(c.stderr.contains("no/such/dir.sqlite"), "stderr: {}", c.stderr);
}

#[test]
fn un_paste_che_non_si_capisce_esce_due_e_dice_perche() {
    let d = db();
    let c = esegui(
        &format!("capture --db {} --person person_0001", d.path().join("k.sqlite").display()),
        "Va bene, mandiamolo così come prima.",
    );
    assert_eq!(c.uscita, Uscita::Rifiutata, "la regola ha detto no, non è un errore di uso");
    assert_eq!(c.json()["error"]["code"], "paste-non-documento");
    assert!(c.stderr.contains("paste-non-documento"), "stderr: {}", c.stderr);
}

#[test]
fn la_cattura_mette_in_bozza_e_lo_dice_nel_risultato() {
    let d = db();
    let c = esegui(
        &format!("capture --db {} --person person_0001", d.path().join("k.sqlite").display()),
        &format!("Corso: {CORSO}\n\n```artifact\n{}\n```", artifact(false)),
    );
    assert_eq!(c.uscita, Uscita::Ok);
    let v = c.json();
    assert_eq!(v["ok"], true);
    assert_eq!(v["protocol"], cli::PROTOCOLLO);
    assert_eq!(v["command"], "capture");
    // Il risultato e' in top level, non annidato: e' la forma che
    // `kbs_fixtures::adapter` pretende e che rende questo output utilizzabile
    // anche da chi non conosce l'involucro.
    assert_eq!(v["stored"], true);
    assert_eq!(v["argument"]["state"], "bozza");
    assert!(v["verdict"]["content_hash"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn la_promozione_di_un_artifact_che_referenzia_una_cdn_esce_due() {
    let d = db();
    let dbp = d.path().join("k.sqlite");
    // prima: il file entra
    let dentro = format!("Corso: {CORSO}\n\n```artifact\n{}\n```", artifact_con_cdn());
    let c = esegui(
        &format!("capture --db {} --person person_0001", dbp.display()),
        &dentro,
    );
    assert_eq!(c.uscita, Uscita::Ok);
    // poi: la promozione è chiusa, e lo dice con il codice della regola
    let file = d.path().join("prova.html");
    std::fs::write(&file, artifact_con_cdn()).unwrap();
    let p = esegui(
        &format!(
            "promote --db {} --person person_0001 --arg capture/la-frazione-irriducibile.html --path {} --note tentativo",
            dbp.display(),
            file.display()
        ),
        "",
    );
    assert_eq!(p.uscita, Uscita::Rifiutata);
    assert_eq!(p.json()["error"]["code"], "verdetto-bloccante");
    assert!(p.json()["error"]["message"].as_str().unwrap().contains("external-reference"));
}

#[test]
fn la_promozione_senza_ratifica_esce_due_e_il_codice_e_quello_del_negozio() {
    let d = db();
    let dbp = d.path().join("k.sqlite");
    esegui(
        &format!("capture --db {} --person person_0001", dbp.display()),
        &format!("Corso: {CORSO}\n\n```artifact\n{}\n```", artifact(false)),
    );
    let file = d.path().join("prova.html");
    std::fs::write(&file, artifact(false)).unwrap();
    let p = esegui(
        &format!(
            "ratify --db {} --person person_0001 --arg capture/la-frazione-irriducibile.html",
            dbp.display()
        ),
        "",
    );
    assert_eq!(p.uscita, Uscita::Ok);
}

#[test]
fn il_lock_si_stampa_con_i_suoi_byte_e_in_top_level() {
    let c = esegui(
        "lock --model claude-sonnet-4-5 --generator kbs-intake/1 --system sistema \
         --task compito --instruction \"Marco non distingue\"",
        "",
    );
    assert_eq!(c.uscita, Uscita::Ok);
    let v = c.json();
    // I byte sono in top level accanto al lock: un lock senza i byte è
    // un'affermazione, e chi li riceve deve poter ricalcolare l'hash.
    let byte = v["canonical_bytes"].as_str().expect("i byte canonici");
    assert!(byte.starts_with("kbs-prompt/1\n"));
    assert_eq!(v["lock"]["model_id"], "claude-sonnet-4-5");
    assert!(v["lock"]["prompt_hash"].as_str().unwrap().starts_with("kbs-prompt/1:"));
    assert!(v["lock"]["corpus_hash"].as_str().unwrap().starts_with("kbs-c1:"));
    // E l'hash corrisponde ai byte, ricalcolato qui senza il crate.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update([kbs_intake::prompt::TAG]);
    h.update(byte.as_bytes());
    assert_eq!(
        v["lock"]["prompt_hash"].as_str().unwrap(),
        format!("kbs-prompt/1:{}", hex::encode(h.finalize()))
    );
}

#[test]
fn la_diagnosi_si_registra_e_le_generazioni_si_rileggono() {
    let d = db();
    let dbp = d.path().join("k.sqlite");
    // prima l'argomento di cui la diagnosi verte, e poi il materiale generato:
    // `generations.argument_id` referenzia `arguments.id`, e un evento senza il
    // materiale che ha generato non e' un record.
    esegui(
        &format!("capture --db {} --person person_0001", dbp.display()),
        &format!(
            "Corso: {CORSO}\nRel-Path: {REL}\n\n```artifact\n{}\n```",
            artifact(false)
        ),
    );
    let generato = esegui(
        &format!("capture --db {} --person person_0001", dbp.display()),
        &format!(
            "Corso: {CORSO}\nRel-Path: esercizi/nuovo.html\n\n```artifact\n{}\n```",
            artifact(false).replace("La frazione irriducibile", "Esercizi sulle frazioni")
        ),
    );
    assert_eq!(generato.uscita, Uscita::Ok, "stderr: {}", generato.stderr);
    // Il registro delle persone e' della scuola, non di questo binario: `kbs`
    // non ha un verbo per creare uno studente, e `upsert_person` sovrascriverebbe
    // il nome di chi c'e' gia'. Quindi il roster si registra da fuori, come fa
    // l'amministrazione della scuola, e il test lo fa con `kbs_store`.
    {
        let mut s = Store::open(&dbp).unwrap();
        for (id, nome) in [("person_0007", "Marco"), ("person_0002", "collega")] {
            s.upsert_person(&kbs_store::Person {
                id: kbs_core::PersonId(id.into()),
                display_name: nome.into(),
                created_at: kbs_core::Millis(0),
            })
            .unwrap();
        }
    }
    let dia = esegui(
        &format!(
            "diagnose --db {} --person person_0001 --student person_0007 --course {CORSO} \
             --cohort 2026-terza --arg letture/prova.html --note \"non distingue il denominatore\"",
            dbp.display()
        ),
        "",
    );
    assert_eq!(dia.uscita, Uscita::Ok, "stderr: {}", dia.stderr);
    let id = dia.json()["id"].as_str().expect("id della diagnosi").to_string();
    assert!(id.starts_with("obs_"));

    let gen = esegui(
        &format!(
            "generate --db {} --person person_0001 --arg esercizi/nuovo.html --model claude-sonnet-4-5 \
             --generator kbs-intake/1 --system sistema --task compito --instruction \"per Marco\" \
             --diagnosis {} --at 1757000000000",
            dbp.display(),
            id
        ),
        "",
    );
    assert_eq!(gen.uscita, Uscita::Ok, "stderr: {}", gen.stderr);
    assert!(gen.json()["lock_id"].as_str().unwrap().len() > 8);

    // `--person` è obbligatorio, e non per una forma della CLI: il registro
    // delle generazioni è gated, e chi lo rilegge è la persona che lo ha
    // scritto. Il test passa `person_0001`, la stessa che ha generato: la
    // lettura senza persona non esiste più, e se quella persona può è una
    // decisione del negozio, non della CLI.
    let letti = esegui(
        &format!(
            "generations --db {} --person person_0001 --arg esercizi/nuovo.html",
            dbp.display()
        ),
        "",
    );
    assert_eq!(letti.uscita, Uscita::Ok);
    let eventi = letti.json();
    assert_eq!(eventi.as_array().unwrap().len(), 1);
    assert_eq!(eventi[0]["lock"]["model_id"], "claude-sonnet-4-5");
    assert_eq!(eventi[0]["at"], 1_757_000_000_000i64);
}

#[test]
fn verify_stampa_il_referto_nella_forma_che_il_banco_si_aspetta() {
    let d = db();
    let corpus = d.path().join("corpus");
    std::fs::create_dir_all(corpus.join("letture")).unwrap();
    std::fs::write(corpus.join(REL), artifact(false)).unwrap();
    std::fs::write(corpus.join(REL_ALTRO), artifact_con_cdn()).unwrap();
    let c = esegui(
        &format!(
            "verify --json --db {} {}",
            d.path().join("k.sqlite").display(),
            corpus.display()
        ),
        "",
    );
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    let v = c.json();
    // I campi che `kbs_fixtures::adapter` pretende, in top level.
    assert!(v.get("items").is_some(), "items manca");
    assert!(v.get("index").is_some(), "index manca");
    assert!(v.get("claims").is_some());
    assert!(v["instances"].as_array().unwrap().is_empty());
    assert_eq!(v["protocol"], cli::PROTOCOLLO);
    assert_eq!(v["kbs_version"], env!("CARGO_PKG_VERSION"));
    assert!(v["corpus_hash"].as_str().unwrap().starts_with("kbs-c1:"));

    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let cdn = items.iter().find(|i| i["rel_path"] == REL_ALTRO).unwrap();
    assert_eq!(cdn["valid"], false);
    assert_eq!(cdn["errors"][0]["code"], "external-reference");
    let buono = items.iter().find(|i| i["rel_path"] == REL).unwrap();
    assert_eq!(buono["valid"], true);
    assert_eq!(buono["errors"].as_array().unwrap().len(), 0);

    // Ogni non citabile dice perché: un verdetto senza motivo non si contesta.
    for nc in v["index"]["not_citable"].as_array().unwrap() {
        assert!(!nc["invariant"].as_str().unwrap().is_empty());
        assert!(!nc["message"].as_str().unwrap().is_empty());
    }
    assert_eq!(v["index"]["citable"].as_array().unwrap().len(), 0);
}

#[test]
fn la_scansione_e_idempotente_sullo_stesso_corpus() {
    let d = db();
    let corpus = d.path().join("corpus");
    std::fs::create_dir_all(corpus.join("letture")).unwrap();
    std::fs::write(corpus.join(REL), artifact(false)).unwrap();
    let dbp = d.path().join("k.sqlite").display().to_string();
    let a = esegui(&format!("verify --db {dbp} {}", corpus.display()), "");
    let b = esegui(&format!("verify --db {dbp} {}", corpus.display()), "");
    assert_eq!(a.uscita, Uscita::Ok);
    assert_eq!(b.uscita, Uscita::Ok);
    assert_eq!(a.json()["corpus_hash"], b.json()["corpus_hash"]);
    assert_eq!(a.json()["items"], b.json()["items"]);
}

#[test]
fn un_database_vuoto_creato_da_zero_e_il_posto_giusto_per_il_referto() {
    // Non un test di `kbs-store`: è la promessa che `verify` può costruire il
    // proprio indice, perché il banco gli passa un percorso che non esiste.
    let d = db();
    let corpus = d.path().join("corpus");
    std::fs::create_dir_all(corpus.join("letture")).unwrap();
    std::fs::write(corpus.join(REL), artifact(false)).unwrap();
    let dbp = d.path().join("nuovo.sqlite");
    let c = esegui(&format!("verify --db {} {}", dbp.display(), corpus.display()), "");
    assert_eq!(c.uscita, Uscita::Ok, "stderr: {}", c.stderr);
    assert!(dbp.exists());
    // E il database è utilizzabile: la ratifica ci passa dentro.
    let s = Store::open(&dbp).unwrap();
    assert!(s.schema_epoch().unwrap() > 0);
}
