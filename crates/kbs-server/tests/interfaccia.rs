//! L'interfaccia, provata contro il server vero.
//!
//! # Perché questi test vivono in `tests/` e non in `src/tests/`
//!
//! Perché non toccano `src/`: chi possiede `web/` non è chi possiede le rotte,
//! e il modo in cui due proprietari si rispettano è che il secondo non scriva
//! nel file del primo. Qui si usa la libreria pubblica —
//! `kbs_server::router::app` e `kbs_server::config::ServerConfig` — che è
//! esattamente ciò che usa il binario.
//!
//! # Perché la parte difficile è questa
//!
//! Un'interfaccia senza passo di costruzione è fragile in un modo che un bundle
//! non è: se un modulo viene rinominato e l'import non cambia, la pagina
//! continua a stare in piedi e a fallire **alla prima esecuzione nel browser**.
//! Nessun controllo statico lo vede.
//!
//! E un errore di percorso in questa interfaccia è **peggiore** del normale,
//! per una ragione che è la stessa ragione per cui il progetto ha la sua
//! proprietà di sicurezza meglio mantenuta: in questo server un percorso
//! sbagliato e un percorso inesistente danno la stessa risposta — `404` e
//! `{"error":"non-trovato"}`, byte per byte uguali a «non c'è» e a «non lo
//! vedi». Quindi un errore dell'interfaccia si nasconderebbe esattamente dietro
//! la risposta che non distingue. È il caso in cui la sicurezza e la correttezza
//! dell'interfaccia hanno lo stesso nemico.
//!
//! D16 dice che un test fra due parti deve passare dal consumatore vero. Le due
//! parti sono i percorsi scritti in `web/lib/rotte.js` e le rotte montate in
//! `src/routes/mod.rs`; il consumatore vero è **il router**, e questi test lo
//! interrogano per ogni percorso che l'interfaccia nomina, senza una tabella di
//! mezzo che possa divergere.
//!
//! # Che cosa non è qui, e perché
//!
//! Non c'è un browser, e quindi non c'è un test che verifichi che un riquadro sia
//! **stato dipinto**. Quello che lo verifica è un rendering a mano, fatto durante
//! lo sviluppo e riportato nel verbale. È un limite dichiarato, non nascosto: un
//! test che dicesse «la pagina funziona» senza aver caricato una pagina sarebbe
//! un test di una cosa che non è la pagina.
//!
//! I test del backend che coprono già le rotte **non** sono duplicati. Sono
//! citati dal nome dove sono la prova; qui si verifica la sola cosa che è di
//! questa interfaccia: che la richiesta che essa manda, con la sua forma, ottenga
//! la risposta su cui fa branching.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use kbs_core::{
    Argument, ArgumentId, Claim, ClaimStatus, CohortId, CohortSignal, Contestation, Emitter,
    Evidence, Millis, Origin, PublicationState, Relation,
};
use kbs_server::config::ServerConfig;
use kbs_server::db::Db;
use kbs_store::{
    ArtifactChunk, ChunkKind, CourseRelation, GradeLevel, GradingDraft, ObservationDraft, Person,
    Register, Rubric, RubricVersion, Source, SourceStatus,
};
use tower::util::ServiceExt;

// ─────────────────────────────────────────────────────────────────────────────
// La scuola di prova
// ─────────────────────────────────────────────────────────────────────────────

/// T0: un istante qualunque, ma sempre lo stesso. I millisecondi di un test non
/// sono un dato, sono un rumore.
const T0: i64 = 1_700_000_000_000;

/// Una scuola vera, montata sul router vero, con la `web/` vera.
struct Scuola {
    app: axum::Router,
    /// Il secondo corso, per la prova che un docente del primo non vede niente
    /// del secondo. Serve anche a ricordare che `altro_corso` non è spazzatura.
    altro_corso: kbs_core::CourseId,
    altro_docente: kbs_core::PersonId,
    corso: kbs_core::CourseId,
    docente: kbs_core::PersonId,
    studente: kbs_core::PersonId,
    /// L'argomento in uso, ratificato e pubblicato: è su questo che l'interfaccia
    /// ha qualcosa da mostrare.
    pub_: Argument,
    /// Una bozza: è su questa che la coda ha lavoro da fare, e che «pubblica
    /// senza ratifica» deve rifiutare.
    bozza: Argument,
    _dir: tempfile::TempDir,
}

/// La radice `web/` del crate: gli stessi file che il binario serve in sviluppo.
///
/// Il test **non** copia `web/` in una cartella temporanea. Una copia passerebbe
/// anche quando il file che il binario serve davvero è rotto, e un test che
/// passa sul mock mentre il vero è rotto è un test che ha misurato la copia.
fn radice_web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

fn scuola() -> Scuola {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("corsi/analisi-1")).expect("corpus a");
    std::fs::create_dir_all(corpus.join("corsi/fisica-1")).expect("corpus b");

    let db = Db::open(dir.path().join("scuola.sqlite")).expect("database");
    let corso = kbs_core::CourseId::fixture(1);
    let altro_corso = kbs_core::CourseId::fixture(2);
    let docente = kbs_core::PersonId::fixture(1);
    let altro_docente = kbs_core::PersonId::fixture(2);
    let studente = kbs_core::PersonId::fixture(3);
    let pari = kbs_core::PersonId::fixture(4);
    // Sei studenti iscritti: cinque che sbagliano lo stesso esercizio sono ciò
    // che rende un segnale di coorte pubblicabile (D9, soglia k = 5), e senza
    // questi il segnale qui sotto sarebbe una dichiarazione senza prove.
    let studenti: Vec<kbs_core::PersonId> =
        (3..=8).map(kbs_core::PersonId::fixture).collect();

    db.write(|store| {
        for (id, slug, rel) in [
            (corso.clone(), "analisi-1", "corsi/analisi-1"),
            (altro_corso.clone(), "fisica-1", "corsi/fisica-1"),
        ] {
            store.register_source(&Source {
                id,
                slug: slug.to_string(),
                rel_path: rel.to_string(),
                status: SourceStatus::Active,
                registered_at: Millis(T0),
                last_scan_at: None,
                corpus_hash: None,
            })?;
        }
        for (id, nome) in [
            (docente.clone(), "Prof. Rossi"),
            (altro_docente.clone(), "Prof. Verdi"),
            (pari.clone(), "Sara Neri"),
        ] {
            store.upsert_person(&Person {
                id,
                display_name: nome.to_string(),
                created_at: Millis(T0),
            })?;
        }
        for (person, course, relation) in [
            (docente.clone(), corso.clone(), Relation::Teaches),
            (altro_docente.clone(), altro_corso.clone(), Relation::Teaches),
        ] {
            store.add_relation(&CourseRelation {
                person,
                course,
                relation,
                since: Millis(T0),
                until: None,
            })?;
        }
        for s in &studenti {
            store.upsert_person(&Person {
                id: s.clone(),
                display_name: format!("Studente {}", s.as_str()),
                created_at: Millis(T0),
            })?;
            store.add_relation(&CourseRelation {
                person: s.clone(),
                course: corso.clone(),
                relation: Relation::EnrolledIn,
                since: Millis(T0),
                until: None,
            })?;
        }
        Ok(())
    })
    .expect("scuola");

    let pub_ = argomento(&corso, 1, PublicationState::Bozza);
    let bozza = argomento(&corso, 2, PublicationState::Bozza);

    db.write(|store| {
        store.upsert_argument(&pub_)?;
        store.upsert_argument(&bozza)?;
        store.ratify(&pub_.id, &docente, "verificato riga per riga")?;
        store.publish(&pub_.id)?;

        // Le tre claim di cui l'interfaccia deve distinguere lo stato: una
        // sostenuta con lo span, una contraddetta — lo span c'è e non sostiene —
        // e una senza span, che va letta come non verificata.
        for (id, testo, stato, span) in [
            (
                "clm_1",
                "Il teorema di Rolle",
                ClaimStatus::Supported,
                Some("f continua su [a,b] e f(a)=f(b) implica un c interno con f'(c)=0"),
            ),
            (
                "clm_2",
                "Ogni funzione discontinua è monotona",
                ClaimStatus::Contradicted,
                Some("la discontinuità non implica la monotonia: la funzione resta discontinua pur essendo monotona"),
            ),
            ("clm_3", "Gli estremi", ClaimStatus::Unciteable, None),
        ] {
            store.append_claim(&Claim {
                id: id.to_string(),
                course: corso.clone(),
                argument: pub_.id.clone(),
                text: testo.to_string(),
                span_anchor: span.map(|_| id.to_string()),
                span_text: span.map(str::to_string),
                status: stato,
                emitted_at: Millis(T0),
                emitted_by: Emitter::Teacher {
                    by: docente.clone(),
                },
            })?;
        }

        // Le osservazioni che rendono un segnale di coorte **vero**: il
        // segnale non si dichiara, si conta. `count_failing` conta persone
        // distinte con esercizio verificato e risposta sbagliata, quindi un
        // segnale dichiarato a mano senza osservazioni sotto è un rifiuto —
        // ed è il rifiuto giusto.
        let sessione = store.open_session(Register::Observations, "compiti")?;
        for (n, studente) in studenti.iter().enumerate() {
            store.append_observation(
                &sessione,
                ObservationDraft {
                    id: format!("obs_{n}"),
                    student: studente.clone(),
                    course: corso.clone(),
                    cohort: CohortId("2A".into()),
                    argument: pub_.id.clone(),
                    evidence: Evidence::Checked {
                        exercise: format!("ex_{n}"),
                        instance: format!("inst_{n}"),
                        correct: n >= 5,
                    },
                    judged_by: Some(kbs_core::GraderKind::Deterministic),
                    at: Millis(T0),
                },
            )?;
        }
        store.close_session(&sessione)?;
        store.record_cohort_signal(&CohortSignal {
            course: corso.clone(),
            cohort: CohortId("2A".into()),
            argument: pub_.id.clone(),
            failing: 5,
            total: 6,
            at: Millis(T0),
        })?;

        // Un voto senza rubric non è riproducibile, e la chiave esterna lo
        // rende tale anche per chi scrive SQL: la rubrica va registrata prima.
        store.upsert_rubric(&Rubric {
            id: "rub_1".to_string(),
            course: corso.clone(),
            title: "Sufficienza".to_string(),
            created_at: Millis(T0),
        })?;
        store.upsert_rubric_version(&RubricVersion {
            id: "v1".to_string(),
            rubric: "rub_1".to_string(),
            version: "1".to_string(),
            scale: vec![GradeLevel {
                grade: "sufficiente".to_string(),
                label: "sufficiente".to_string(),
                points: 6.0,
            }],
            defined_at: Millis(T0),
            note: "una versione, una scala".to_string(),
        })?;

        // Un giudizio emesso da un pari, con la contestazione **dentro** la riga
        // e ancora aperta. `outcome: None` è «aperta», non «assente».
        let sessione = store.open_session(Register::Gradings, "prove")?;
        store.append_grading(
            &sessione,
            GradingDraft {
                id: "grd_1".to_string(),
                student: studente.clone(),
                course: corso.clone(),
                argument: pub_.id.clone(),
                kind: kbs_core::GraderKind::Peer,
                graded_by: pari,
                rubric_version: "v1".to_string(),
                grade: "sufficiente".to_string(),
                at: Millis(T0),
                contested: Some(Contestation {
                    by: studente.clone(),
                    at: Millis(T0 + 1_000),
                    reason: "ho rifatto il conto e non torna".to_string(),
                    outcome: None,
                }),
            },
        )?;
        store.close_session(&sessione)?;

        // L'indicizzazione, perché una ricerca senza indice risponderebbe
        // «nessun risultato» e il test passerebbe per la ragione sbagliata.
        store.index_chunk(&ArtifactChunk {
            argument: pub_.id.clone(),
            course: corso.clone(),
            kind: ChunkKind::Argument,
            ord: 0,
            rel_path: pub_.rel_path.clone().unwrap_or_default(),
            title: pub_.title.clone(),
            body: "In questa lezione si parla di sommabilita.".to_string(),
            headings: String::new(),
            code: String::new(),
            prompt: String::new(),
            contract: String::new(),
            updated_at: Millis(T0),
        })?;
        Ok(())
    })
    .expect("contenuto");

    // `web_dir` è la radice vera, non una copia.
    let config = ServerConfig {
        web_dir: radice_web(),
        ..ServerConfig::new(&corpus)
    };
    let app = kbs_server::router::app(db, config);

    Scuola {
        app,
        altro_corso,
        altro_docente,
        corso,
        docente,
        studente,
        pub_,
        bozza,
        _dir: dir,
    }
}

fn argomento(corso: &kbs_core::CourseId, n: u32, stato: PublicationState) -> Argument {
    let rel_path = format!("corsi/{}/lezione-{n:02}.html", corso.as_str());
    Argument {
        id: ArgumentId::from_rel_path(&rel_path),
        title: format!("Lezione {n:02}: i limiti di una funzione"),
        summary: "Che cosa dice un argomento, e chi ha diritto di saperlo.".to_string(),
        state: stato,
        course: corso.clone(),
        prerequisites: Vec::new(),
        origin: Origin::Human {
            by: kbs_core::PersonId::fixture(1),
            at: Millis(T0),
        },
        rel_path: Some(rel_path),
        content_hash: format!("sha256:{n:064x}"),
        created_at: Millis(T0),
        updated_at: Millis(T0),
        ratified: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Il client: le stesse richieste che fa `web/lib/api.js`
// ─────────────────────────────────────────────────────────────────────────────

impl Scuola {
    /// Una `GET` che dichiara chi chede nell'header, come fa `api.get`.
    async fn get(&self, uri: &str, persona: &kbs_core::PersonId) -> Response<Body> {
        self.richiesta("GET", uri, Some(persona), None).await
    }

    /// Una `GET` che mette la dichiarazione nella query string, come fa
    /// `EventSource`, che non può allegare un header.
    async fn get_query(&self, uri: &str) -> Response<Body> {
        let richiesta = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .expect("richiesta");
        self.app.clone().oneshot(richiesta).await.expect("risposta")
    }

    /// Una `POST` JSON, come fa `api.post` dalla coda.
    async fn post(
        &self,
        uri: &str,
        persona: &kbs_core::PersonId,
        corpo: serde_json::Value,
    ) -> Response<Body> {
        self.richiesta("POST", uri, Some(persona), Some(corpo.to_string())).await
    }

    async fn richiesta(
        &self,
        metodo: &str,
        uri: &str,
        persona: Option<&kbs_core::PersonId>,
        corpo: Option<String>,
    ) -> Response<Body> {
        let mut costruisci = Request::builder().method(metodo).uri(uri);
        if let Some(p) = persona {
            costruisci = costruisci.header("x-kbs-person", p.as_str());
        }
        if corpo.is_some() {
            costruisci = costruisci.header(header::CONTENT_TYPE, "application/json");
        }
        let richiesta = costruisci
            .body(corpo.map_or_else(Body::empty, Body::from))
            .expect("richiesta");
        self.app.clone().oneshot(richiesta).await.expect("risposta")
    }
}

async fn testo(response: Response<Body>) -> String {
    String::from_utf8(
        axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .expect("corpo")
            .to_vec(),
    )
    .expect("utf-8")
}

async fn json(response: Response<Body>) -> serde_json::Value {
    serde_json::from_str(&testo(response).await).expect("corpo json")
}

fn header_string(response: &Response<Body>, nome: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(nome)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

// ─────────────────────────────────────────────────────────────────────────────
// I percorsi: letti da `web/lib/rotte.js`, il posto in cui sono scritti
// ─────────────────────────────────────────────────────────────────────────────

/// Le rotte che l'interfaccia nomina, lette dal suo file.
///
/// Il parser è povero e dichiarato povero: riconosce `chiave: "percorso",` dentro
/// `export const ROTTE = {`. Se un giorno `rotte.js` cambiasse forma, qui
/// uscirebbe un elenco corto o vuoto e
/// [`ogni_rotta_dell_interfaccia_e_una_rotta_del_server`] fallirebbe — che è il
/// modo giusto di fallire, perché un test che passa perché non ha trovato niente
/// da confrontare non sta confrontando niente.
fn rotte_dall_interfaccia() -> Vec<(String, String)> {
    let sorgente = std::fs::read_to_string(radice_web().join("lib/rotte.js"))
        .expect("web/lib/rotte.js deve esistere: è il posto in cui i percorsi sono scritti");
    let dentro = sorgente
        .split_once("export const ROTTE = {")
        .and_then(|(_, resto)| resto.split_once("\n};"))
        .map(|(corpo, _)| corpo)
        .expect("ROTTE deve restare un oggetto letterale: il parser di questo test sa leggerlo");

    let mut rotte = Vec::new();
    for riga in dentro.lines() {
        let riga = riga.trim();
        let Some(aperto) = riga.find('"') else { continue };
        let chiave = riga[..aperto].trim();
        let (nome, _) = chiave.split_once(':').unwrap_or(("", ""));
        let nome = nome.trim();
        if nome.is_empty() {
            continue;
        }
        let resto = &riga[aperto + 1..];
        let fine = resto.find('"').expect("percorso chiuso");
        rotte.push((nome.to_string(), resto[..fine].to_string()));
    }
    assert!(
        rotte.len() >= 13,
        "dal file dei percorsi sono uscite {} voci: il parser non sta più leggendo quello che crede di leggere",
        rotte.len()
    );
    rotte
}

/// Sostituisce i segnaposto con id concreti di questa scuola.
fn con_id(modello: &str, scuola: &Scuola) -> String {
    modello
        .replace("{corso}", scuola.corso.as_str())
        .replace("{id}", scuola.pub_.id.as_str())
}

/// Le specifiche che un modulo importa, in forma di percorso dentro `web/`.
///
/// Copre le due forme che il codice usa — `from "…"` e `import("…")` — e non
/// usa una libreria: sono due ricerche di una sottostringa, e una dipendenza in
/// più per quello sarebbe un costo per un test.
fn import_di(modulo: &Path) -> Vec<String> {
    let sorgente = std::fs::read_to_string(modulo).expect("modulo leggibile");
    let mut trovati = Vec::new();
    for apertura in ["from \"", "import(\""] {
        let mut resto = sorgente.as_str();
        while let Some(inizio) = resto.find(apertura) {
            resto = &resto[inizio + apertura.len()..];
            let fine = resto.find('"').expect("specifica chiusa");
            let spec = &resto[..fine];
            if spec.starts_with('.') {
                trovati.push(spec.to_string());
            }
            resto = &resto[fine + 1..];
        }
    }
    trovati
}

/// Tutti i file dell'interfaccia, per estensione.
fn file_dell_interfaccia() -> Vec<PathBuf> {
    let mut file = Vec::new();
    for sotto in ["", "lib", "pagine"] {
        let Ok(voci) = std::fs::read_dir(radice_web().join(sotto)) else {
            continue;
        };
        for voce in voci.filter_map(|e| e.ok()) {
            let path = voce.path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("js") | Some("css") | Some("html")
            ) {
                file.push(path);
            }
        }
    }
    file
}

// ─────────────────────────────────────────────────────────────────────────────
// I test
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn la_radice_serve_l_interfaccia_reale_byte_per_byte() {
    let scuola = scuola();
    let risposta = scuola.get_query("/").await;
    assert_eq!(
        risposta.status(),
        StatusCode::OK,
        "la radice non risponde 200: il fallback non sta trovando `web/`"
    );
    assert_eq!(
        header_string(&risposta, header::CONTENT_TYPE).as_deref(),
        Some("text/html; charset=utf-8"),
        "la radice non è servita come HTML con charset: senza charset il browser indovina, e un documento italiano indovinato male ha le virgolette storte"
    );
    let servito = testo(risposta).await;
    let atteso = std::fs::read_to_string(radice_web().join("index.html")).expect("index.html");
    assert_eq!(
        servito, atteso,
        "i byte serviti non sono i byte del file: qualcuno li sta trasformando, e in \
         un'interfaccia senza passo di costruzione non dovrebbe esserci nessuno"
    );
}

#[tokio::test]
async fn ogni_modulo_che_la_pagina_carica_e_servito() {
    let scuola = scuola();
    let html = std::fs::read_to_string(radice_web().join("index.html")).expect("index.html");

    // Si parte da ciò che `index.html` dichiara e si segue il grafo degli import
    // fino in fondo: è il grafo che il browser chiederà, non un elenco che qui
    // si mantiene a mano.
    let mut da_vedere: Vec<PathBuf> = Vec::new();
    for apertura in ["href=\"./", "src=\"./"] {
        let mut resto = html.as_str();
        while let Some(inizio) = resto.find(apertura) {
            let dopo = &resto[inizio + apertura.len()..];
            let fine = dopo.find('"').expect("attributo chiuso");
            da_vedere.push(radice_web().join(&dopo[..fine]));
            resto = &dopo[fine..];
        }
    }
    for atteso in ["app.js", "stile.css"] {
        assert!(
            da_vedere.iter().any(|p| p.ends_with(atteso)),
            "index.html non dichiara più `{atteso}`: la pagina non ha più un ingresso"
        );
    }

    let mut esaminati = da_vedere;
    let mut i = 0;
    while i < esaminati.len() {
        let corrente = esaminati[i].clone();
        i += 1;
        if corrente.extension().and_then(|e| e.to_str()) != Some("js") {
            continue;
        }
        let cartella = corrente.parent().expect("cartella").to_path_buf();
        for spec in import_di(&corrente) {
            // `./lib/x.js` e `../lib/x.js` si risolvono sulla cartella del modulo.
            let risolto = cartella.join(&spec);
            let canonico = risolto
                .components()
                .fold(PathBuf::new(), |mut acc, c| match c {
                    std::path::Component::CurDir => acc,
                    std::path::Component::ParentDir => {
                        acc.pop();
                        acc
                    }
                    altro => acc.join(altro),
                })
                .canonicalize()
                .unwrap_or_else(|_| risolto.clone());
            if !esaminati.contains(&canonico) {
                esaminati.push(canonico);
            }
        }
    }

    for modulo in &esaminati {
        let uri = format!(
            "/{}",
            modulo
                .strip_prefix(&radice_web())
                .expect("dentro web/")
                .to_string_lossy()
        );
        let risposta = scuola.get_query(&uri).await;
        assert_eq!(
            risposta.status(),
            StatusCode::OK,
            "`{uri}` non è servito: la pagina lo carica, e il browser riceverebbe il 404 \
             di «non c'è, o non lo vedi» per un file che invece c'è"
        );
        let atteso = if uri.ends_with(".js") {
            "application/javascript; charset=utf-8"
        } else {
            "text/css; charset=utf-8"
        };
        assert_eq!(
            header_string(&risposta, header::CONTENT_TYPE).as_deref(),
            Some(atteso),
            "`{uri}` è servito con il content-type sbagliato: senza \
             `application/javascript` il browser non lo accetta come modulo ES"
        );
    }

    assert!(
        esaminati.len() >= 12,
        "il grafo contiene {} file, che sono pochi: il seguito degli import non sta funzionando",
        esaminati.len()
    );
}

/// Le rotte che questa interfaccia chiama con una `POST`.
///
/// Sono dichiarate qui perché il metodo è parte del contratto: `ratifica`,
/// `pubblica` e `ritira` esistono, e una `GET` su una di loro risponde `405`.
/// Quel `405` è una **prova che la rotta esiste** — è l'unico status che in
/// questo server distingue «rotta che c'è ma con un altro metodo» da «rotta
/// che non c'è». Ed è un'informazione che non viene dal fallback, quindi non
/// apre il canale che il fallback chiude.
const ROTTE_IN_SCRITTURA: [&str; 3] = ["ratifica", "pubblica", "ritira"];

#[tokio::test]
async fn ogni_rotta_dell_interfaccia_e_una_rotta_del_server() {
    let scuola = scuola();
    let rotte = rotte_dall_interfaccia();

    for (nome, modello) in &rotte {
        let uri = con_id(modello, &scuola);
        let risposta = scuola.get(&uri, &scuola.docente).await;
        let status = risposta.status();

        // Su `eventi` il corpo non finisce mai: qui si guarda lo status e il
        // content-type, e il corpo viene lasciato cadere.
        let atteso = if ROTTE_IN_SCRITTURA.contains(&nome.as_str()) {
            // Una rotta di scrittura risponde `405` a una `GET`, e `405` non è
            // quello che risponderebbe un percorso inesistente: quello passa dal
            // fallback e arriva al `404` che non distingue.
            StatusCode::METHOD_NOT_ALLOWED
        } else if nome == "ricerca" {
            // La ricerca senza `q` è una richiesta non valida, e lo dice: è
            // l'unico status, insieme al `405`, che distingue una rotta che c'è
            // da una che non c'è. `api.js` ha una pagina intera per questo caso.
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::OK
        };
        assert_eq!(
            status,
            atteso,
            "la rotta `{nome}` (`{uri}`) risponde {status} e non {atteso}. In questo \
             server un percorso inesistente dà la stessa risposta di «non lo vedi», \
             quindi un errore di percorso in questa interfaccia si nasconderebbe dietro \
             la proprietà di sicurezza meglio mantenuta del progetto."
        );
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "la rotta `{nome}` risponde 403: questo server non ne ha e non deve averne uno"
        );
    }

    // E nessun modulo dell'interfaccia può chiamare una rotta che `ROTTE` non
    // dichiara senza che questo test se ne accorga: è la stessa tabella che il
    // browser usa, letta dal file.
    for nome in [
        "salute", "argomenti", "argomento", "claim", "coda", "ratifica", "pubblica", "ritira",
        "osservazioni", "giudizi", "coorte", "ricerca", "export", "eventi", "three",
    ] {
        assert!(
            rotte.iter().any(|(n, _)| n == nome),
            "`{nome}` è una rotta che questa interfaccia usa e che `rotte.js` non dichiara più"
        );
    }
    assert!(
        rotte
            .iter()
            .any(|(n, m)| n == "export" && m.contains("/export")),
        "la rotta di esportazione non c'è più: senza D12 l'uscita del corpus non esce"
    );
}

#[tokio::test]
async fn la_ricerca_non_ritorna_il_testo_e_il_client_deve_riprenderlo() {
    let scuola = scuola();
    let risposta = scuola.get("/api/v1/search?q=sommabilita", &scuola.docente).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = json(risposta).await;

    let hits = corpo["hits"].as_array().expect("hits");
    assert!(
        !hits.is_empty(),
        "la ricerca non ha trovato niente: senza indice il resto del test passerebbe per la ragione sbagliata"
    );
    for hit in hits {
        let chiavi: Vec<String> = hit
            .as_object()
            .expect("hit oggetto")
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            chiavi,
            vec!["argument".to_string(), "course".to_string(), "rank".to_string()],
            "un hit porta {chiavi:?}: la rotta di ricerca non porta testo, e questa \
             interfaccia è scritta per riprendere ogni argomento con una seconda chiamata"
        );
    }

    // E l'id che l'hit porta è proprio quello che l'interfaccia riprende: la
    // seconda chiamata è sullo stesso id, e la risposta è un intero modulo, non
    // un testo.
    let id = hits[0]["argument"].as_str().expect("argument");
    assert!(id.starts_with("arg_"), "l'id dell'hit non ha la forma di un id: {id}");
    let ripresa = scuola
        .get(&format!("/api/v1/arguments/{id}"), &scuola.docente)
        .await;
    assert_eq!(ripresa.status(), StatusCode::OK);
    let testo_argomento = json(ripresa).await;
    assert!(
        testo_argomento["argument"]["title"].as_str().is_some(),
        "l'argomento ripreso non ha un titolo: la pagina non avrebbe nulla da mostrare"
    );
}

#[tokio::test]
async fn pubblicare_senza_ratifica_e_rifiutato_anche_alla_richiesta_dell_interfaccia() {
    let scuola = scuola();

    // La coda del docente, così il bozzo è davvero in coda e la pagina lo
    // offrirebbe con il bottone «pubblica» premibile.
    let coda = scuola
        .get(
            &format!("/api/v1/courses/{}/queue", scuola.corso.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(coda.status(), StatusCode::OK);
    let coda = json(coda).await;
    let voci = coda["queue"].as_array().expect("queue");
    let bozza = voci
        .iter()
        .find(|v| v["argument"]["id"] == scuola.bozza.id.as_str())
        .expect("la bozza è in coda");
    assert_eq!(bozza["needs_ratification"], serde_json::json!(true));
    assert_eq!(bozza["needs_publication"], serde_json::json!(false));

    // Esattamente la POST che `web/pagine/coda.js` manda quando si preme
    // «pubblica» su una riga che manca di ratifica.
    let uri = format!(
        "/api/v1/courses/{}/queue/{}/publish",
        scuola.corso.as_str(),
        scuola.bozza.id.as_str()
    );
    let risposta = scuola.post(&uri, &scuola.docente, serde_json::json!({})).await;
    assert_eq!(
        risposta.status(),
        StatusCode::CONFLICT,
        "pubblicare senza ratifica è stato accettato: D4 è rotta, e l'interfaccia \
         mostrerebbe una pubblicazione che non è avvenuta"
    );
    let rifiuto = json(risposta).await;
    assert_eq!(
        rifiuto["regola"],
        serde_json::json!("D4"),
        "il rifiuto non nomina la regola: l'interfaccia non avrebbe nulla di vero da \
         dire al docente, solo un codice di stato"
    );
    assert!(
        rifiuto["motivo"].as_str().is_some_and(|m| !m.is_empty()),
        "il rifiuto non ha un motivo: `regola-di-dominio` senza motivo è un codice, non una spiegazione"
    );
}

#[tokio::test]
async fn la_coorte_due_lati_elenco_vuoto_nessun_zero_e_segnali_sopra_soglia() {
    let scuola = scuola();

    // Lato sopra soglia: l'interfaccia disegna la tabella con le cifre.
    let risposta = scuola
        .get(
            &format!("/api/v1/arguments/{}/cohort", scuola.pub_.id.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = json(risposta).await;
    let segnali = corpo["signals"].as_array().expect("signals");
    assert_eq!(segnali.len(), 1, "il segnale sopra soglia non è passato");
    assert_eq!(
        segnali[0]["failing"],
        serde_json::json!(5),
        "il conteggio dei failing non è quello che il registro delle osservazioni conta"
    );
    assert_eq!(segnali[0]["total"], serde_json::json!(6));
    assert_eq!(segnali[0]["cohort"], serde_json::json!("2A"));

    // Lato sotto soglia: l'interfaccia non ha una tabella da disegnare e deve
    // dire che non c'è niente. Non un `0`, non un trattino: la risposta è
    // un elenco vuoto e non c'è un campo che dica «c'è qualcosa che non ti
    // mostriamo», perché quel campo sarebbe metà dell'informazione protetta.
    let risposta = scuola
        .get(
            &format!("/api/v1/arguments/{}/cohort", scuola.bozza.id.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let grezzo = testo(risposta).await;
    let corpo: serde_json::Value = serde_json::from_str(&grezzo).expect("json");
    assert_eq!(
        corpo["signals"].as_array().map(Vec::len),
        Some(0),
        "la risposta sotto soglia non è un elenco vuoto: {grezzo}"
    );
    for vietato in ["failing", "total", "soppress", "soglia"] {
        assert!(
            !grezzo.contains(vietato),
            "la risposta vuota contiene `{vietato}`: {grezzo}"
        );
    }
}

#[tokio::test]
async fn le_claim_arrivano_con_span_e_stato_perche_l_interfaccia_deba_distinguerli() {
    let scuola = scuola();
    let risposta = scuola
        .get(
            &format!("/api/v1/arguments/{}/claims", scuola.pub_.id.as_str()),
            &scuola.docente,
        )
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = json(risposta).await;
    let claim = corpo["claims"].as_array().expect("claims");
    assert_eq!(claim.len(), 3, "le tre claim di prova non sono arrivate tutte");

    let per_id = |id: &str| {
        claim
            .iter()
            .find(|c| c["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("manca la claim {id}"))
    };
    assert_eq!(per_id("clm_1")["status"], serde_json::json!("supported"));
    assert!(per_id("clm_1")["span_text"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(per_id("clm_2")["status"], serde_json::json!("contradicted"));
    assert!(
        per_id("clm_2")["span_text"].as_str().is_some(),
        "la claim contraddetta ha uno span che non la sostiene: senza il testo dello \
         span l'interfaccia non potrebbe mostrarlo, e il caso più insidioso diventerebbe indistinguibile"
    );
    assert_eq!(per_id("clm_3")["status"], serde_json::json!("unciteable"));
    assert!(
        per_id("clm_3")["span_text"].is_null(),
        "la claim senza span ha un testo di span: l'interfaccia la mostrerebbe come \
         verificata, e non lo è"
    );
}

#[tokio::test]
async fn i_giudizi_portano_la_contestazione_dentro_la_riga() {
    let scuola = scuola();
    let risposta = scuola
        .get(
            &format!(
                "/api/v1/courses/{}/gradings?person={}",
                scuola.corso.as_str(),
                scuola.studente.as_str()
            ),
            &scuola.docente,
        )
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    let corpo = json(risposta).await;
    let giudizi = corpo["gradings"].as_array().expect("gradings");
    assert!(
        !giudizi.is_empty(),
        "il registro è vuoto: il resto del test passerebbe senza esercitare la contestazione"
    );
    let riga = &giudizi[0];
    assert!(
        riga.get("contested").is_some(),
        "il giudizio non ha il campo `contested`: non si potrebbe distinguere «nessuno ha \
         contestato» da «c'è un ricorso aperto»"
    );
    assert!(riga["contested"]["reason"].as_str().is_some());
    assert!(
        riga["contested"]["outcome"].is_null(),
        "un ricorso aperto deve avere `outcome` nullo: è la differenza che un ricorso chiede per primo"
    );
    assert_eq!(
        riga["rubric_version"],
        serde_json::json!("v1"),
        "il giudizio non porta la versione del rubric: un voto senza rubric non è riproducibile"
    );
    assert_eq!(
        riga["kind"],
        serde_json::json!("peer"),
        "il giudizio non dichiara chi ha giudicato, e senza saperlo l'interfaccia non può \
         dire se un pari vede solo le proprie righe"
    );
}

#[tokio::test]
async fn il_canale_eventi_apre_con_la_dichiarazione_in_query_come_fa_event_source() {
    let scuola = scuola();
    // `EventSource` non può allegare un header: la dichiarazione viaggia nella
    // query string. È il modo in cui `web/lib/api.js` apre il canale, e i test
    // del backend, che usano l'header, non lo esercitano.
    let risposta = scuola
        .get_query(&format!(
            "/api/v1/courses/{}/events?person={}",
            scuola.corso.as_str(),
            scuola.docente.as_str()
        ))
        .await;
    assert_eq!(
        risposta.status(),
        StatusCode::OK,
        "il canale con `?person=` non si apre: questa interfaccia non potrebbe iscriversi"
    );
    assert_eq!(
        header_string(&risposta, header::CONTENT_TYPE).as_deref(),
        Some("text/event-stream"),
        "il canale non è un event-stream"
    );
    drop(risposta);

    // E senza dichiarazione deve dire 401, non aprire un canale muto.
    let risposta = scuola
        .get_query(&format!("/api/v1/courses/{}/events", scuola.corso.as_str()))
        .await;
    assert_eq!(risposta.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn la_pagina_dice_che_nessuno_sa_se_funziona_in_una_classe() {
    let scuola = scuola();
    let testo = testo(scuola.get_query("/").await).await;
    assert!(
        testo.contains("Nessuno sa se questo corpus funziona in una classe italiana"),
        "la pagina non contiene l'avvertenza sui limiti. Un limite che si vede solo in una \
         pagina che nessuno apre non è un limite dichiarato."
    );
    assert!(
        testo.contains("non un metodo"),
        "l'avvertenza c'è ma non dice che cosa sia: un avviso senza la sua conclusione non è un avviso"
    );
    assert!(
        testo.contains("dichiarazione") && testo.contains("non un'autenticazione"),
        "la pagina non dice che l'identità è una dichiarazione e non un'autenticazione. Il \
         primo a leggerla deve saperlo prima di fidarsi: è la ragione per cui la stessa \
         frase è scritta anche nel sorgente del server."
    );
}

#[tokio::test]
async fn nessun_file_dell_interfaccia_chiama_fuori() {
    // D15 vale per gli artifact e vale per l'interfaccia: la scuola può non
    // avere rete, e una pagina che chiama fuori non è verificabile (D6) né
    // riproducibile (D11).
    let file = file_dell_interfaccia();
    assert!(
        file.len() >= 12,
        "ho trovato {} file dell'interfaccia, che sono pochi: l'esame non starebbe guardando quello che crede",
        file.len()
    );
    for path in &file {
        let contenuto = std::fs::read_to_string(path).expect("file leggibile");
        for (n, riga) in contenuto.lines().enumerate() {
            let ripulita = riga.trim();
            // I commenti parlano di regole, non di richieste: «D15» e «nessun
            // CDN» sono le frasi che il constraint chiede di scrivere.
            if ripulita.starts_with("//") || ripulita.starts_with("*") || ripulita.starts_with("<!--") {
                continue;
            }
            for marcatore in ["https://", "http://", "//cdn.", "//unpkg", "//cdnjs"] {
                assert!(
                    !ripulita.contains(marcatore),
                    "{}:{} contiene `{marcatore}`: questa interfaccia non deve chiamare fuori, \
                     e la scuola può non avere rete (D15)",
                    path.display(),
                    n + 1
                );
            }
        }
    }
}

#[tokio::test]
async fn un_argomento_che_non_vedi_e_uno_che_non_esistono_danno_la_stessa_risposta() {
    // Non è un test dell'interfaccia ma della sua premessa: se il predicato
    // cambiasse, l'interfaccia continuerebbe a dire «non c'è, o non lo vedi» e
    // sarebbe una pagina bugiarda. Il backend lo copre
    // (`un_id_inesistente_e_un_id_invisibile_danno_la_stessa_risposta`); qui si
    // verifica che la risposta su cui l'interfaccia costruisce il suo messaggio
    // sia davvero la stessa nei due casi.
    let scuola = scuola();
    let non_visibile = scuola
        .get(
            &format!("/api/v1/arguments/{}/cohort", scuola.pub_.id.as_str()),
            &scuola.altro_docente,
        )
        .await;
    let status_visibile = non_visibile.status();
    let corpo_visibile = testo(non_visibile).await;

    let inesistente = scuola
        .get(
            "/api/v1/arguments/arg_0000000000000000/cohort",
            &scuola.docente,
        )
        .await;
    let status_inesistente = inesistente.status();
    let corpo_inesistente = testo(inesistente).await;

    assert_eq!(
        status_visibile,
        status_inesistente,
        "un argomento che non puoi vedere e uno che non esistono danno status diversi: \
         l'interfaccia direbbe «non lo vedi» dove il server non distingue"
    );
    assert_eq!(
        corpo_visibile,
        corpo_inesistente,
        "le due risposte non sono gli stessi byte: l'interfaccia potrebbe distinguerle"
    );
    let _ = &scuola.altro_corso;
}
