//! Il costo di `?testo=senza-contenuto`, provato a byte.
//!
//! # Perché un file nuovo
//!
//! Le prove di questa riduzione erano in `tests/interfaccia.rs`, che è un file
//! di integrazione come questo: Rust non condivide gli helper fra un file di
//! test e l'altro, quindi qui si ricostruisce **il minimo** — un corso, un
//! docente, uno studente, un argomento — invece di duplicare la scuola di prova
//! dell'altro file. Ciò che serve qui è una rotta e un predicato, non
//! l'interfaccia.
//!
//! # Che cosa è provato
//!
//! * **il costo non cresce**: la risposta ridotta di un file piccolo e quella
//!   di un file grande hanno gli stessi byte;
//! * **il parametro che accorcia non apre una porta**: «non lo vedi» e «non
//!   esiste» restano la stessa risposta con il parametro e senza, e così le
//!   risposte in cui il file non c'è o è troppo grande;
//! * **la risposta dice che cosa contiene**: il default e `?testo=intero` sono
//!   la stessa risposta, e la ridotta dice `stato`, `byte` e nient'altro.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use kbs_core::{Argument, ArgumentId, Millis, Origin, PublicationState, Relation};
use kbs_server::config::ServerConfig;
use kbs_server::db::Db;
use kbs_server::routes::arguments::MAX_TESTO;
use kbs_store::{CourseRelation, Person, Source, SourceStatus};
use tower::util::ServiceExt;

/// T0: un istante qualunque, ma sempre lo stesso.
const T0: i64 = 1_700_000_000_000;

/// Una scuola minima: un corso, un docente che insegna in esso, uno studente che
/// non c'è iscritto e un argomento pubblicato.
///
/// Lo studente serve a una cosa sola — essere qualcuno a cui il predicato
/// dice no — e il corso è uno solo perché qui non si prova la partizione.
struct Scuola {
    app: axum::Router,
    db: Db,
    corpus: PathBuf,
    argomento: Argument,
    docente: kbs_core::PersonId,
    /// Non iscritto: per lui l'argomento non esiste.
    estraneo: kbs_core::PersonId,
    _dir: tempfile::TempDir,
}

fn scuola() -> Scuola {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("corsi/analisi-1")).expect("corpus");

    let db = Db::open(dir.path().join("scuola.sqlite")).expect("database");
    let corso = kbs_core::CourseId::fixture(1);
    let docente = kbs_core::PersonId::fixture(1);
    let estraneo = kbs_core::PersonId::fixture(2);

    db.write(|store| {
        store.register_source(&Source {
            id: corso.clone(),
            slug: "analisi-1".to_string(),
            rel_path: "corsi/analisi-1".to_string(),
            status: SourceStatus::Active,
            registered_at: Millis(T0),
            last_scan_at: None,
            corpus_hash: None,
        })?;
        for (id, nome) in [
            (docente.clone(), "Prof. Rossi"),
            (estraneo.clone(), "Esterno"),
        ] {
            store.upsert_person(&Person {
                id,
                display_name: nome.to_string(),
                created_at: Millis(T0),
            })?;
        }
        store.add_relation(
            &CourseRelation {
                person: docente.clone(),
                course: corso.clone(),
                relation: Relation::Teaches,
                since: Millis(T0),
                until: None,
            },
            &docente,
        )?;
        Ok(())
    })
    .expect("scuola");

    let rel_path = "corsi/analisi-1/lezione-01.html".to_string();
    let argomento = Argument {
        id: ArgumentId::from_rel_path(&rel_path),
        title: "Lezione 01: i limiti di una funzione".to_string(),
        summary: "Che cosa dice un argomento.".to_string(),
        state: PublicationState::Bozza,
        course: corso.clone(),
        prerequisites: Vec::new(),
        origin: Origin::Human {
            by: docente.clone(),
            at: Millis(T0),
        },
        rel_path: Some(rel_path),
        content_hash: "sha256:00".to_string(),
        created_at: Millis(T0),
        updated_at: Millis(T0),
        ratified: None,
    };
    db.write(|store| {
        store.upsert_argument(&argomento)?;
        store.ratify(&argomento.id, &docente, "verificato")?;
        store.publish(&argomento.id)?;
        Ok(())
    })
    .expect("argomento");

    // Lo stato che la scuola tiene è quello **pubblicato**: `upsert_argument`
    // non torna da `in-corso` a uno stato mutabile, quindi un fixture fermato
    // alla bozza renderebbe impossibile cambiare il `rel_path` — che è il modo
    // in cui qui si cambia il file che la rotta legge.
    let argomento = Argument {
        state: PublicationState::InCorso,
        ..argomento
    };

    let app = kbs_server::router::app(db.clone(), ServerConfig::new(corpus.clone()));
    Scuola {
        app,
        db,
        corpus,
        argomento,
        docente,
        estraneo,
        _dir: dir,
    }
}

impl Scuola {
    /// Una `GET` che dichiara chi chiede, come fa `web/lib/api.js`.
    async fn get(&self, uri: &str, persona: &kbs_core::PersonId) -> (StatusCode, Vec<u8>) {
        let richiesta = Request::builder()
            .method("GET")
            .uri(uri)
            .header("x-kbs-person", persona.as_str())
            .body(Body::empty())
            .expect("richiesta");
        let risposta = self.app.clone().oneshot(richiesta).await.expect("risposta");
        let status = risposta.status();
        let corpo = axum::body::to_bytes(risposta.into_body(), 16 * 1024 * 1024)
            .await
            .expect("corpo")
            .to_vec();
        (status, corpo)
    }

    /// Il `GET` dell'argomento di questa scuola, con o senza `?testo=`.
    async fn argomento(&self, query: &str, persona: &kbs_core::PersonId) -> (StatusCode, Vec<u8>) {
        self.get(
            &format!("/api/v1/arguments/{}{query}", self.argomento.id.as_str()),
            persona,
        )
        .await
    }

    /// Scrive un file nel corpus, creandone la cartella.
    fn scrivi(&self, rel_path: &str, contenuto: &[u8]) {
        let percorso = self.corpus.join(rel_path);
        std::fs::create_dir_all(percorso.parent().expect("cartella")).expect("cartella");
        std::fs::write(percorso, contenuto).expect("file");
    }

    /// Lo stesso argomento con un altro `rel_path`: la rotta legge il file
    /// **da qui**, quindi i modi in cui il file cambia si provocano cambiando
    /// questa riga e nient'altro — e l'id resta lo stesso, che è ciò che rende
    /// confrontabili le due risposte.
    fn con_rel_path(&self, rel_path: &str) {
        let mut a = self.argomento.clone();
        a.rel_path = Some(rel_path.to_string());
        self.db
            .write(|store| {
                store.upsert_argument(&a)?;
                Ok(())
            })
            .expect("registrazione");
    }
}

fn json(corpo: &[u8]) -> serde_json::Value {
    serde_json::from_slice(corpo).unwrap_or_else(|e| panic!("corpo json ({e}): {corpo:?}"))
}

/// Un file di `n` byte, tutto ASCII, e quindi `byte` del file e `byte` della
/// risposta intera coincidono: la prova del punto 3 non deve confondere la
/// differenza fra caratteri e byte con la differenza che sta provando.
fn ascii_di(n: usize) -> Vec<u8> {
    vec![b'a'; n]
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Il costo si misura, e non cresce
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn il_costo_della_riduzione_non_cresce_con_la_dimensione_del_file() {
    // I due file sono **entrambi** sotto `MAX_TESTO` e **dello stesso
    // argomento**: un file a `MAX_TESTO` verrebbe rifiutato come `troppo-grande`
    // e misurerebbe la lunghezza di un altro tag, e due argomenti diversi
    // misurerebbero anche i loro metadati e i loro prerequisiti.
    let piccolo = 4_096usize;
    let grande = MAX_TESTO - 1; // sotto il tetto, di quanto si può
    assert!(grande > piccolo * 100, "i due file devono essere proprio diversi");

    let scuola = scuola();
    let piccolo_rel = "corsi/analisi-1/piccol1.html";
    let grande_rel = "corsi/analisi-1/grande1.html";
    scuola.scrivi(piccolo_rel, &ascii_di(piccolo));
    scuola.scrivi(grande_rel, &ascii_di(grande));

    scuola.con_rel_path(piccolo_rel);
    let (status_piccolo, ridotta_piccola) = scuola
        .argomento("?testo=senza-contenuto", &scuola.docente)
        .await;
    assert_eq!(status_piccolo, StatusCode::OK);

    scuola.con_rel_path(grande_rel);
    let (status_grande, ridotta_grande) = scuola
        .argomento("?testo=senza-contenuto", &scuola.docente)
        .await;
    assert_eq!(status_grande, StatusCode::OK);

    // I due nomi hanno la stessa lunghezza (`piccol1.html`, `grande1.html`: sette
    // caratteri ciascuno, perche' l'id dell'argomento deriva dal percorso e un nome
    // piu' lungo cambierebbe la risposta per una ragione che non c'entra), e
    // l'argomento è lo stesso: quindi l'unica cosa che può cambiare fra le due
    // risposte ridotte è il numero che dice quanto è grande il file. Tutto il
    // resto è l'argomento, che è lo stesso, e i suoi metadati.
    assert_eq!(piccolo_rel.len(), grande_rel.len());

    // La misura che c'è dentro è quella giusta per ciascun file, e sono misure
    // diverse: un file da un megabyte e uno da quattro kilobyte non possono
    // dare la stessa risposta ridotta neppure per caso.
    let mut testo_piccolo = json(&ridotta_piccola)["testo"].clone();
    let mut testo_grande = json(&ridotta_grande)["testo"].clone();
    assert_eq!(testo_piccolo["byte"].as_u64(), Some(piccolo as u64));
    assert_eq!(testo_grande["byte"].as_u64(), Some(grande as u64));
    assert_eq!(testo_piccolo["stato"], "senza-contenuto", "{testo_piccolo}");
    assert_eq!(testo_grande["stato"], "senza-contenuto", "{testo_grande}");

    // Ora il confronto vero: azzerata la misura — che è l'unica cosa che
    // *deve* cambiare, perché è l'unica cosa che cambia nel file — le due schede
    // sono identiche. Il peso della risposta ridotta è quello dei metadati.
    testo_piccolo["byte"] = serde_json::json!(0);
    testo_grande["byte"] = serde_json::json!(0);
    assert_eq!(
        testo_piccolo, testo_grande,
        "la scheda di due file diversi non è la stessa scheda: porta qualcosa \
         che viene dal file"
    );

    // E la differenza di lunghezza fra le due risposte ridotte è al più quella
    // delle cifre della misura: un campo pesante che entrasse qui farebbe
    // esplodere questa differenza, e questa prova diventerebbe rossa da sola.
    let differenza = ridotta_grande.len().abs_diff(ridotta_piccola.len());
    assert!(
        differenza <= 4,
        "le due risposte ridotte differiscono di {differenza} byte: una riduzione \
         il cui peso cambia con il file non sta togliendo il contenuto"
    );

    // La prova ha una controparte, altrimenti misurerebbe una risposta che non
    // contiene niente da misurare: il file grande esiste ed è portato per
    // intero dalla risposta senza parametro, e la risposta ridotta è
    // centinaia di volte più piccola di quel file.
    let intero_grande = scuola.argomento("", &scuola.docente).await.1;
    assert!(
        intero_grande.len() > grande,
        "la risposta intera del file grande non porta il contenuto: la riduzione \
         non avrebbe niente da togliere e le misure sopra non misurerebbero niente"
    );
    assert!(
        ridotta_grande.len() * 100 < intero_grande.len(),
        "la risposta ridotta ({}) non è molto più piccola di quella intera ({}): \
         non ha tolto il contenuto",
        ridotta_grande.len(),
        intero_grande.len()
    );
    assert!(
        ridotta_grande.len() < piccolo,
        "la risposta ridotta di un file da quasi un megabyte pesa {} byte: più \
         del file da quattro kilobyte che è il suo unico contenuto",
        ridotta_grande.len()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Il parametro che accorcia non apre una porta
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn il_parametro_non_distingue_chi_non_puo_e_che_non_esiste() {
    let scuola = scuola();
    const SEGRETO: &[u8] = b"<p>questo materiale non deve uscire per chi non puo' leggerlo</p>";
    scuola.con_rel_path("corsi/analisi-1/lezione-01.html");
    scuola.scrivi("corsi/analisi-1/lezione-01.html", SEGRETO);

    let (status_assente, corpo_assente) = scuola
        .get("/api/v1/arguments/arg_0000000000000000", &scuola.estraneo)
        .await;

    for query in ["", "?testo=senza-contenuto", "?testo=intero"] {
        let (status_negato, corpo_negato) = scuola.argomento(query, &scuola.estraneo).await;
        assert_eq!(
            status_negato, status_assente,
            "un argomento che non puo' leggere con `{query}` non dà «non esiste»"
        );
        assert_eq!(
            String::from_utf8_lossy(&corpo_negato),
            String::from_utf8_lossy(&corpo_assente),
            "con `{query}` la risposta negativa non è la stessa di «non esiste»"
        );
        assert!(
            !String::from_utf8_lossy(&corpo_negato).contains("non deve uscire"),
            "il corpo della risposta negata contiene il materiale: {corpo_negato:?}"
        );
    }
}

#[tokio::test]
async fn il_parametro_non_apre_una_porta_su_un_file_che_non_c_e_nemmeno() {
    // Un argomento **senza** file: la risposta ridotta e quella intera sono la
    // stessa risposta, perché la riduzione toglie una cosa che lì non c'è. Se
    // il parametro cambiasse qualcosa, significherebbe che distingue due stati
    // che il lettore vede come uno.
    let scuola = scuola();
    let mut senza_file = scuola.argomento.clone();
    senza_file.rel_path = None;
    scuola
        .db
        .write(|store| {
            store.upsert_argument(&senza_file)?;
            Ok(())
        })
        .expect("registrazione");

    let (status_intero, intero) = scuola.argomento("", &scuola.docente).await;
    let (status_ridotto, ridotto) = scuola
        .argomento("?testo=senza-contenuto", &scuola.docente)
        .await;
    assert_eq!(status_intero, StatusCode::OK);
    assert_eq!(status_ridotto, status_intero);
    assert_eq!(
        intero, ridotto,
        "su un argomento senza file il parametro cambia la risposta: sta \
         introducendo un terzo stato che il vocabolario non dichiara"
    );
    assert_eq!(json(&ridotto)["testo"]["motivo"], "nessun-file", "{:?}", json(&ridotto));
}

#[tokio::test]
async fn il_parametro_non_apre_una_porta_su_un_file_troppo_grande() {
    // Il tetto vale anche per la risposta ridotta, e vale **con la sua
    // misura**: «non te lo porto» e «non c'è» sono due risposte diverse e solo
    // la prima è vera. Il parametro che accorcia non può trasformare il
    // rifiuto in un silenzio.
    let scuola = scuola();
    let grande = "corsi/analisi-1/grande1.html";
    scuola.con_rel_path(grande);
    scuola.scrivi(grande, &ascii_di(MAX_TESTO + 1));

    let (status_intero, intero) = scuola.argomento("", &scuola.docente).await;
    let (status_ridotto, ridotto) = scuola
        .argomento("?testo=senza-contenuto", &scuola.docente)
        .await;
    assert_eq!(status_intero, StatusCode::OK, "il tetto non è un errore: è un motivo");
    assert_eq!(status_ridotto, status_intero);
    assert_eq!(
        intero, ridotto,
        "un file troppo grande risponde diversamente con e senza il parametro"
    );
    let corpo = json(&ridotto);
    assert_eq!(corpo["testo"]["motivo"], "troppo-grande", "{corpo}");
    assert_eq!(
        corpo["testo"]["byte"].as_u64(),
        Some((MAX_TESTO + 1) as u64),
        "un file troppo grande dice quanto è grande, con il parametro come senza"
    );
}

#[tokio::test]
async fn un_valore_che_non_combacia_e_rifiutato_prima_del_predicato() {
    // Lo stesso `400` per un id che non c'è, per uno che non si vede e per uno
    // che si vede: il parametro non può diventare il modo di imparare che cosa
    // c'è in un corso, perché arriva prima della decisione.
    let scuola = scuola();
    scuola.con_rel_path("corsi/analisi-1/lezione-01.html");
    scuola.scrivi("corsi/analisi-1/lezione-01.html", b"<p>materiale</p>");

    let (status_inesistente, corpo_inesistente) = scuola
        .get("/api/v1/arguments/arg_0000000000000000?testo=piccolo", &scuola.estraneo)
        .await;
    assert_eq!(status_inesistente, StatusCode::BAD_REQUEST);

    let (status_negato, corpo_negato) = scuola.argomento("?testo=piccolo", &scuola.estraneo).await;
    assert_eq!(status_negato, status_inesistente, "il predicato è arrivato prima del 400");
    assert_eq!(corpo_negato, corpo_inesistente);

    let (status_legibile, corpo_legibile) = scuola.argomento("?testo=piccolo", &scuola.docente).await;
    assert_eq!(status_legibile, status_inesistente);
    assert_eq!(corpo_legibile, corpo_inesistente);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. La risposta dice che cosa contiene, e come chiederne meno
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn la_risposta_ridotta_dice_che_cosa_contiene_e_come_chiederne_meno() {
    let scuola = scuola();
    // Il titolo dell'argomento contiene «Lezione 01», quindi il marcatore che
    // non deve uscire è una parola che sta **solo** nel file: una prova che
    // cerca un pezzo di contenuto che si trova anche nei metadati non sta
    // provando niente.
    let contenuto = b"<!doctype html>\n<title>Lezione 01</title>\n<p>marcatore-unico-del-file</p>\n";
    scuola.con_rel_path("corsi/analisi-1/lezione-01.html");
    scuola.scrivi("corsi/analisi-1/lezione-01.html", contenuto);

    let (status_default, intero) = scuola.argomento("", &scuola.docente).await;
    let (status_dichiarato, intero_dichiarato) = scuola.argomento("?testo=intero", &scuola.docente).await;
    assert_eq!(status_default, StatusCode::OK);
    assert_eq!(status_dichiarato, status_default);
    assert_eq!(
        intero, intero_dichiarato,
        "il default e `?testo=intero` non sono la stessa risposta: mettere il \
         parametro cambia qualcosa che non doveva cambiare"
    );

    let (status_ridotto, ridotto) = scuola
        .argomento("?testo=senza-contenuto", &scuola.docente)
        .await;
    assert_eq!(status_ridotto, status_default);
    assert!(
        ridotto.len() < intero.len(),
        "la risposta ridotta non è più piccola di quella intera: non ha tolto niente"
    );

    let intero_json = json(&intero);
    let ridotto_json = json(&ridotto);

    // La misura è la stessa, e non un'approssimazione: è quella del file, letta
    // dal filesystem, che è l'unico fatto che permette a chi ha la scheda di
    // decidere se chiedere il contenuto con un secondo `GET`.
    assert_eq!(
        ridotto_json["testo"]["byte"].as_u64(),
        intero_json["testo"]["byte"].as_u64(),
        "la risposta ridotta non porta la misura vera del file"
    );
    assert_eq!(
        ridotto_json["testo"]["byte"].as_u64(),
        Some(contenuto.len() as u64)
    );
    assert_eq!(ridotto_json["testo"]["stato"], "senza-contenuto", "{ridotto_json}");
    assert_eq!(intero_json["testo"]["stato"], "presente", "{intero_json}");
    assert_eq!(
        intero_json["testo"]["contenuto"].as_str(),
        Some(String::from_utf8_lossy(contenuto).as_ref()),
        "la risposta intera non porta il contenuto del file"
    );
    assert!(
        ridotto_json["testo"].get("contenuto").is_none(),
        "la risposta ridotta ha un campo `contenuto`: {ridotto_json}"
    );
    assert!(
        !String::from_utf8_lossy(&ridotto).contains("marcatore-unico-del-file"),
        "il corpo della risposta ridotta contiene il contenuto del file"
    );

    // Ridurre non è un'altra risorsa: l'argomento è lo stesso, campo per campo.
    assert_eq!(
        ridotto_json["argument"], intero_json["argument"],
        "la riduzione ha cambiato l'argomento"
    );
}

#[tokio::test]
async fn una_risposta_ridotta_non_legge_il_file() {
    // La riduzione guarda la statistica del file e non lo decodifica: per
    // questo un file che **non** è UTF-8 risponde `senza-contenuto` con la sua
    // misura, mentre la risposta intera dice `non-testo`. Non è un bug di una
    // forma: è la prova che il contenuto non è stato letto, e che chiedere di
    // meno fa sapere meno cose.
    let scuola = scuola();
    let binario = "corsi/analisi-1/lezione-01.bin";
    scuola.con_rel_path(binario);
    scuola.scrivi(binario, &[0xff, 0xfe, 0x00, 0x80]);

    let intero = json(&scuola.argomento("", &scuola.docente).await.1);
    assert_eq!(intero["testo"]["motivo"], "non-testo", "{intero}");

    let ridotto = json(&scuola.argomento("?testo=senza-contenuto", &scuola.docente).await.1);
    assert_eq!(ridotto["testo"]["stato"], "senza-contenuto", "{ridotto}");
    assert_eq!(ridotto["testo"]["byte"].as_u64(), Some(4));
    assert_eq!(ridotto["argument"], intero["argument"]);
}
