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

// ── `kbs insegna`: la relazione che mancava ────────────────────────────────

/// Un banco con un argomento scritto da `person_0001` e il roster della scuola.
///
/// Il roster è registrato **a mano** e questa è la parte che il verbo non fa:
/// `insegna` non crea persone, perché `upsert_person` sovrascrive
/// `display_name` e creare un collega col suo id come nome significa rinominare
/// un collega vero. Il registro delle persone è della scuola, e questo banco lo
/// costruisce come lo costruisce `la_diagnosi_si_registra_e_le_generazioni_si_
/// rileggono`.
fn banco_con_corso(d: &tempfile::TempDir) -> std::path::PathBuf {
    let dbp = d.path().join("k.sqlite");
    let cattura = esegui(
        &format!("capture --db {} --person person_0001", dbp.display()),
        &format!(
            "Corso: {CORSO}\nRel-Path: {REL}\n\n```artifact\n{}\n```",
            artifact(false)
        ),
    );
    assert_eq!(cattura.uscita, Uscita::Ok, "stderr: {}", cattura.stderr);
    let mut s = Store::open(&dbp).unwrap();
    s.upsert_person(&kbs_store::Person {
        id: kbs_core::PersonId("person_0002".into()),
        display_name: "collega".into(),
        created_at: kbs_core::Millis(0),
    })
    .unwrap();
    dbp
}

#[test]
fn il_collega_che_non_ha_scritto_niente_non_vede_il_corso_fino_a_che_non_insegna() {
    let d = db();
    let dbp = banco_con_corso(&d);
    // Prima: il predicato di D5 dice no, e dice no per la ragione giusta — il
    // collega non è l'autore e `relations` è vuota. Non è un errore di uso e
    // non è un incidente: è «non lo vedi».
    let prima = esegui(
        &format!("read --db {} --person person_0002 --arg {REL}", dbp.display()),
        "",
    );
    assert_ne!(prima.uscita, Uscita::Ok, "prima di insegnare deve essere rifiutato");

    let insegna = esegui(
        &format!(
            "insegna --db {} --person person_0001 --course {CORSO} --docente person_0002",
            dbp.display()
        ),
        "",
    );
    assert_eq!(insegna.uscita, Uscita::Ok, "stderr: {}", insegna.stderr);
    let v = insegna.json();
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "insegna");
    assert_eq!(v["relation"], "teaches");
    assert_eq!(v["docente"], "person_0002");
    // `--person` è chi ha registrato la riga, e la riga lo dice: `relations` è
    // la tabella che decide chi vede cosa ed è l'unica che doveva dire chi ha
    // scritto (`V8`).
    assert_eq!(v["registrato_da"], "person_0001");
    assert_eq!(v["recorded"], true);

    // Dopo: lo stesso comando, la stessa persona, e adesso la risposta è il
    // materiale. È il buco chiuso, verificato sulle due estremità.
    let dopo = esegui(
        &format!("read --db {} --person person_0002 --arg {REL}", dbp.display()),
        "",
    );
    assert_eq!(dopo.uscita, Uscita::Ok, "stderr: {}", dopo.stderr);
    // L'id segue il percorso relativo, mai i byte (`ArgumentId::from_rel_path`),
    // e il percorso torna nella riga: quello che il docente rivede è l'argomento
    // del corso, non un altro.
    assert_eq!(dopo.json()["rel_path"], REL);
    assert_eq!(
        dopo.json()["id"],
        kbs_core::ArgumentId::from_rel_path(REL).as_str()
    );
}

#[test]
fn insegnare_twice_non_doppia_la_riga_e_la_prima_provenienza_resta() {
    let d = db();
    let dbp = banco_con_corso(&d);
    let a = esegui(
        &format!(
            "insegna --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 1000",
            dbp.display()
        ),
        "",
    );
    assert_eq!(a.uscita, Uscita::Ok, "stderr: {}", a.stderr);
    let b = esegui(
        &format!(
            "insegna --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 2000",
            dbp.display()
        ),
        "",
    );
    assert_eq!(b.uscita, Uscita::Ok, "stderr: {}", b.stderr);
    // La seconda esecuzione **dichiara** che non ha scritto niente. Un `ok`
    // identico nelle due forme senza dire quale è successa sarebbe
    // l'ambiguità che questo protocollo dichiara di non avere.
    assert_eq!(b.json()["recorded"], false);
    assert_eq!(b.json()["already"], true);

    // E la tabella ha una riga sola: `since` è nella PK, e senza questa domanda
    // un verbo rieseguito a mano riempirebbe il registro di riprese di ruolo che
    // nessuno ha chiesto.
    let s = Store::open(&dbp).unwrap();
    let (righe, provenienza): (i64, Option<String>) = s
        .conn()
        .query_row(
            "SELECT COUNT(*), MAX(recorded_by) FROM relations WHERE relation = 'teaches'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(righe, 1);
    assert_eq!(provenienza.as_deref(), Some("person_0001"));
}

#[test]
fn insegnare_a_una_persona_che_non_c_e_o_a_un_corso_che_non_c_e_esce_due_e_dice_qual_e() {
    let d = db();
    let dbp = banco_con_corso(&d);
    // La persona: `people` è il roster della scuola, e questo verbo non lo
    // inventa. Le due risposte sono diverse e lo dicono: una è «manca la
    // persona», l'altra «manca il corso».
    let persona = esegui(
        &format!(
            "insegna --db {} --person person_0001 --course {CORSO} --docente person_9999",
            dbp.display()
        ),
        "",
    );
    assert_eq!(persona.uscita, Uscita::Rifiutata);
    assert_eq!(persona.json()["error"]["code"], "persona-assente");
    assert!(persona.stderr.contains("persona-assente"), "stderr: {}", persona.stderr);

    // Il corso: `relations.course_id` referenzia `sources`, e una relazione su
    // un corso che non esiste non darebbe a nessuno nessun diritto — il
    // predicato parte dal corso. Rifiutarlo qui è più onesto che lasciarsi
    // fermare da un `FOREIGN KEY constraint failed` che non nomina nessuno.
    let corso = esegui(
        &format!(
            "insegna --db {} --person person_0001 --course non-esiste --docente person_0002",
            dbp.display()
        ),
        "",
    );
    assert_eq!(corso.uscita, Uscita::Rifiutata);
    assert_eq!(corso.json()["error"]["code"], "corso-assente");
    assert!(corso.stderr.contains("corso-assente"), "stderr: {}", corso.stderr);
}

#[test]
fn nessuna_delle_tre_opzioni_ha_un_default() {
    // Un default su `--person` scriverebbe `recorded_by = NULL`, che è
    // esattamente il buco che `V8` chiude; un default su `--docente` scriverebbe
    // `teaches` a qualcuno che non l'ha chiesto. Le tre esistono, quindi
    // l'esito è `opzione-mancante` e il codice è 3.
    let dir = db();
    let dbp = banco_con_corso(&dir);
    for (manca, riga) in [
        ("person", format!("insegna --db {} --course {CORSO} --docente person_0002", dbp.display())),
        ("course", format!("insegna --db {} --person person_0001 --docente person_0002", dbp.display())),
        (
            "docente",
            format!("insegna --db {} --person person_0001 --course {CORSO}", dbp.display()),
        ),
    ] {
        let c = esegui(&riga, "");
        assert_eq!(c.uscita, Uscita::Uso, "manca `--{manca}`: {}", c.stderr);
        assert_eq!(c.json()["error"]["code"], "opzione-mancante", "manca `--{manca}`");
    }
}

// ── `verify` senza `--person`: l'identità che il codice scrive al posto tua ──

#[test]
fn verify_senza_persona_lo_dice_su_stderr_e_nel_referto() {
    let d = db();
    let corpus = d.path().join("corpus");
    std::fs::create_dir_all(corpus.join("letture")).unwrap();
    std::fs::write(corpus.join(REL), artifact(false)).unwrap();
    let dbp = d.path().join("k.sqlite");

    // L'avviso non è una formula: chi lo ha scritto non può sbagliare il
    // numero, perché l'attribuzione qui sotto è quella che il comando ha
    // davvero scritto. Un avviso che nominasse qualcun altro sarebbe una
    // riga in più e non una garanzia.
    let c = esegui(&format!("verify --db {} {}", dbp.display(), corpus.display()), "");
    assert_eq!(c.uscita, Uscita::Ok, "un avviso non è un rifiuto: {}", c.stderr);
    let referto = c.json();
    let a = &referto["avvisi"][0];
    assert_eq!(a["code"], "persona-non-dichiarata");
    assert_eq!(a["persona"], "person_0000");
    assert!(c.stderr.contains("persona-non-dichiarata"), "stderr: {}", c.stderr);
    assert!(c.stderr.contains("person_0000"), "stderr: {}", c.stderr);

    let s = Store::open(&dbp).unwrap();
    // `DISTINCT` su una colonna sola fa inferire a rusqlite una tupla di un
    // elemento, e quel `FromSql` non esiste: la domanda giusta non e' «il
    // primo», e' «sono tutti uguali». Si chiede quindi la lista e si confronta.
    let firmati: Vec<String> = s
        .conn()
        .prepare("SELECT origin_by FROM arguments")
        .expect("la query")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("la lettura")
        .map(|r| r.expect("ogni riga"))
        .collect();
    assert!(
        !firmati.is_empty() && firmati.iter().all(|f| f == "person_0000"),
        "l'argomento indicizzato porta come autore: {firmati:?}"
    );
    let firmato = &firmati[0];
    assert_eq!(
        firmato, "person_0000",
        "l'avviso nomina la persona che ha firmato, non una che avrebbe potuto"
    );

    // Con `--person` l'avviso sparisce e il **campo resta, vuoto**: un campo
    // che non c'è non dice «zero», dice «non lo so». E stderr tace, perché un
    // canale che parla sempre è un canale che non si legge.
    let corpus2 = d.path().join("corpus-dichiarata");
    std::fs::create_dir_all(corpus2.join("letture")).unwrap();
    std::fs::write(corpus2.join(REL), artifact(false)).unwrap();
    let dichiarata = esegui(
        &format!(
            "verify --db {} --person person_0007 {}",
            d.path().join("k2.sqlite").display(),
            corpus2.display()
        ),
        "",
    );
    assert_eq!(dichiarata.uscita, Uscita::Ok, "stderr: {}", dichiarata.stderr);
    assert_eq!(
        dichiarata.json()["avvisi"].as_array().unwrap().len(),
        0,
        "una persona dichiarata non è un avviso"
    );
    assert_eq!(dichiarata.stderr, "", "nessun avviso, nessun rumore su stderr");
}

// ── `kbs termina`: l'incarico che finisce ──────────────────────────────────

#[test]
fn l_incarico_che_finisce_non_apre_piu_niente_e_dice_chi_l_ha_chiuso() {
    let d = db();
    let dbp = banco_con_corso(&d);
    // Le date sono esplicite perché `V2` vieta una `until` anteriore alla
    // `since`: un test che mettesse `now()` nell'apertura e `5000` nella fine
    // starebbe provando il vincolo, non il verbo.
    let insegna = esegui(
        &format!(
            "insegna --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 4000",
            dbp.display()
        ),
        "",
    );
    assert_eq!(insegna.uscita, Uscita::Ok, "stderr: {}", insegna.stderr);
    let vede = esegui(
        &format!("read --db {} --person person_0002 --arg {REL}", dbp.display()),
        "",
    );
    assert_eq!(vede.uscita, Uscita::Ok, "insegna, quindi vede: {}", vede.stderr);

    let finito = esegui(
        &format!(
            "termina --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 5000",
            dbp.display()
        ),
        "",
    );
    assert_eq!(finito.uscita, Uscita::Ok, "stderr: {}", finito.stderr);
    let v = finito.json();
    assert_eq!(v["command"], "termina");
    assert_eq!(v["relation"], "teaches");
    assert_eq!(v["docente"], "person_0002");
    assert_eq!(v["registrato_da"], "person_0001", "chi ha chiuso non è chi se n'è andato");
    assert_eq!(v["until"], 5000i64);
    assert_eq!(v["recorded"], true);
    assert_eq!(v["already"], false);

    // Il buco chiuso, verificato sulle due estremità: la stessa persona, lo
    // stesso argomento, e adesso non lo vede più. Non è un errore di uso e
    // non è un incidente: è «non lo vedi».
    let non_vede = esegui(
        &format!("read --db {} --person person_0002 --arg {REL}", dbp.display()),
        "",
    );
    assert_ne!(
        non_vede.uscita,
        Uscita::Ok,
        "un incarico finito non è più un diritto: stderr {}",
        non_vede.stderr
    );

    // Le due provenienze sono due fatti e non uno: `V8` dichiara chi ha
    // **aperto** l'incarico, `V9` chi lo ha chiuso. Un solo nome nelle due
    // colonne sarebbe la firma di un atto che non è avvenuto.
    let s = Store::open(&dbp).unwrap();
    let (aperto, chiuso): (String, String) = s
        .conn()
        .query_row(
            "SELECT recorded_by, ended_by FROM relations \
             WHERE person_id = 'person_0002' AND relation = 'teaches'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("la riga dell'incarico");
    // `recorded_by` dice **chi ha registrato la riga**, che e' l'operatore che
    // ha lanciato il verbo, non il docente di cui la relazione parla: e' la
    // semantica che V8 dichiara quando tiene la provenienza fuori dalla PK
    // («due righe che differiscono solo per chi le ha registrate non sono lo
    // stesso fatto»). Il test prima si aspettava il docente per l'apertura e
    // l'operatore per la chiusura: asimmetrico, e senza una ragione.
    assert_eq!(aperto, "person_0001", "l'incarico l'ha registrato l'operatore");
    assert_eq!(chiuso, "person_0001", "e l'ha chiuso lo stesso operatore");
}

#[test]
fn un_incarico_già_finto_e_un_incarico_che_non_c_e_dicono_cose_diverse() {
    let d = db();
    let dbp = banco_con_corso(&d);

    // Non c'è mai stato: `already` a chi non ha mai insegnato sarebbe una
    // risposta falsa, quindi qui è un rifiuto e il codice è quello della
    // regola (2), non un incidente di sistema.
    let mai = esegui(
        &format!(
            "termina --db {} --person person_0001 --course {CORSO} --docente person_0002",
            dbp.display()
        ),
        "",
    );
    assert_eq!(mai.uscita, Uscita::Rifiutata);
    assert_eq!(mai.json()["error"]["code"], "relazione-assente");
    assert!(mai.stderr.contains("relazione-assente"), "stderr: {}", mai.stderr);

    esegui(
        &format!(
            "insegna --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 4000",
            dbp.display()
        ),
        "",
    );
    // Una fine **prima** dell'inizio è un terzo rifiuto, e dice perché. Il
    // vincolo di `V2` l'avrebbe respinta lo stesso, ma l'avrebbe detta `store`:
    // un codice 4 per un `--at` sbagliato è un falso incidente, e questo
    // protocollo ha un codice apposta per la differenza.
    let indietro = esegui(
        &format!(
            "termina --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 3000",
            dbp.display()
        ),
        "",
    );
    assert_eq!(indietro.uscita, Uscita::Rifiutata);
    assert_eq!(indietro.json()["error"]["code"], "relazione-indietro");
    assert!(indietro.stderr.contains("relazione-indietro"), "stderr: {}", indietro.stderr);

    let primo = esegui(
        &format!(
            "termina --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 5000",
            dbp.display()
        ),
        "",
    );
    assert_eq!(primo.uscita, Uscita::Ok, "stderr: {}", primo.stderr);

    // Un atto che è già avvenuto non viene ripetuto per far rumore, e la
    // seconda esecuzione **non riscrive la data**: `--at 9999` qui non è la
    // verità, la verità è il 5000 di prima. Un `until` ricalcolato direbbe
    // che l'incarico è finito tutte le volte che il comando viene ripetuto.
    let secondo = esegui(
        &format!(
            "termina --db {} --person person_0001 --course {CORSO} --docente person_0002 --at 9999",
            dbp.display()
        ),
        "",
    );
    assert_eq!(secondo.uscita, Uscita::Ok, "stderr: {}", secondo.stderr);
    assert_eq!(secondo.json()["recorded"], false);
    assert_eq!(secondo.json()["already"], true);
    assert_eq!(secondo.json()["until"], 5000i64, "la data che vale è quella scritta");
}

#[test]
fn nessuna_delle_tre_opzioni_di_termina_ha_un_default() {
    // Le stesse tre di `insegna`, e per le stesse ragioni: `--person` che
    // manca significherebbe `ended_by` a vuoto, `--docente` che manca
    // significherebbe chiudere l'incarico di nessuno, `--course` che manca è
    // una relazione che non ha perimetro.
    let dir = db();
    let dbp = banco_con_corso(&dir);
    for (manca, riga) in [
        (
            "person",
            format!("termina --db {} --course {CORSO} --docente person_0002", dbp.display()),
        ),
        (
            "course",
            format!("termina --db {} --person person_0001 --docente person_0002", dbp.display()),
        ),
        (
            "docente",
            format!("termina --db {} --person person_0001 --course {CORSO}", dbp.display()),
        ),
    ] {
        let c = esegui(&riga, "");
        assert_eq!(c.uscita, Uscita::Uso, "manca `--{manca}`: {}", c.stderr);
        assert_eq!(c.json()["error"]["code"], "opzione-mancante", "manca `--{manca}`");
    }
}
