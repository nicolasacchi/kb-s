//! Il banco di prova: una scuola piccola e vera, e un client HTTP che parla.
//!
//! I test non costruiscono risposte a mano: costruiscono una scuola — un
//! corso con un docente e uno studente iscritto, un altro corso con un altro
//! docente, un autore che non insegna niente — e fanno richieste vere al router.
//! È la situazione minima in cui la regola di D5 dice qualcosa, ed è
//! l'unica in cui una rotta possa essere sbagliata senza che lo si veda.
//!
//! # Perché un `Router` e non un `AppState`
//!
//! Perché la metà di queste proprietà sono proprietà **del protocollo**: lo
//! status, gli header, i byte. Un test che chiama `store.read_argument` non
//! prova la promessa che questo crate fa al mondo, che è «assente» e «non lo
//! vedi» danno la stessa risposta *in HTTP*. Quindi le richieste passano dal
//! router, e le risposte si confrontano come risposte.

#![allow(dead_code)]

mod artifact;
mod capability_routes;
mod cohort_route;
mod export_route;
mod guardia;
mod queue_route;
mod registers_route;
mod search_route;
mod sse_route;
mod vendor_route;

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use axum::Router;
use tower::util::ServiceExt;

use kbs_core::{
    Argument, ArgumentId, CourseId, Millis, Origin, PersonId, PublicationState, Relation,
};
use kbs_store::types::{CourseRelation, Person, Source, SourceStatus};
use tempfile::TempDir;

use crate::config::ServerConfig;
use crate::db::Db;

/// T0: un istante qualunque, ma sempre lo stesso. I millisecondi di un test
/// non sono un dato, sono un rumore.
const T0: i64 = 1_700_000_000_000;

/// La parola che cerca la ricerca, e che sta **solo** in un corso.
///
/// Se la parola fosse in un argomento di entrambi i corsi, un test di
/// isolamento potrebbe passare per la ragione sbagliata: vedrebbe un risultato
/// e non saprebbe quale dei due corsi lo ha prodotto.
pub const PAROLA_SOLO_DI_B: &str = "tachimetro";

/// La parola che cerca la ricerca, e che sta in un argomento **visibile**.
///
/// Serve al test di isolamento: il docente di A deve trovare questa e non
/// trovare [`PAROLA_SOLO_DI_B`]. Se il filtro fosse troppo largo, troverebbe
/// anche la seconda; se fosse troppo stretto, non troverebbe la prima.
pub const PAROLA_DI_A: &str = "sommabilita";

/// Una scuola di prova, su file per il database e su file per il corpus.
pub struct Scuola {
    /// Il router sotto test.
    pub app: Router,
    /// Lo stato, per chi ha bisogno del bus.
    pub stato: crate::routes::AppState,
    /// Il database, per scrivere ciò che un docente scriverebbe.
    pub db: Db,

    /// Il primo corso, con il docente e lo studente iscritto.
    pub corso: CourseId,
    /// Il secondo corso, con un altro docente. Serve a provare che un docente
    /// del primo non vede niente del secondo.
    pub altro_corso: CourseId,
    /// Il docente del primo corso.
    pub docente: PersonId,
    /// Il docente del secondo corso.
    pub altro_docente: PersonId,
    /// Lo studente iscritto al primo corso.
    pub studente: PersonId,
    /// Un secondo studente iscritto: serve a provare che un registro non è di
    /// chiunque.
    pub altra_studente: PersonId,
    /// Una persona che non insegna e non è iscritta a niente.
    pub estraneo: PersonId,
    /// L'autore di una bozza, che **non** insegna il corso in cui la bozza
    /// sta: senza questo, «l'autore vede la bozza» e «il docente vede la bozza»
    /// sarebbero lo stesso test.
    pub autore: PersonId,
    /// La radice del corpus su disco.
    pub corpus: PathBuf,

    dir: TempDir,
}

impl Scuola {
    /// Una scuola nuova: due corsi, quattro persone, e un po' di materiale.
    pub fn nuova() -> Scuola {
        let dir = TempDir::new().expect("tempdir");
        let corpus = dir.path().join("corpus");
        std::fs::create_dir_all(corpus.join("corsi/analisi-1")).expect("corpus di a");
        std::fs::create_dir_all(corpus.join("corsi/fisica-1")).expect("corpus di b");

        let db = Db::open(dir.path().join("scuola.sqlite")).expect("database");
        let corso = CourseId::fixture(1);
        let altro_corso = CourseId::fixture(2);
        let docente = PersonId::fixture(1);
        let altro_docente = PersonId::fixture(2);
        let studente = PersonId::fixture(3);
        let altra_studente = PersonId::fixture(4);
        let estraneo = PersonId::fixture(5);
        let autore = PersonId::fixture(6);

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
                (studente.clone(), "Giulia Bianchi"),
                (altra_studente.clone(), "Sara Neri"),
                (estraneo.clone(), "Marco Conti"),
                (autore.clone(), "Luigi Greco"),
            ] {
                store.upsert_person(&Person {
                    id,
                    display_name: nome.to_string(),
                    created_at: Millis(T0),
                })?;
            }
            for (person, course, relation) in [
                (docente.clone(), corso.clone(), Relation::Teaches),
                (studente.clone(), corso.clone(), Relation::EnrolledIn),
                (altra_studente.clone(), corso.clone(), Relation::EnrolledIn),
                (
                    altro_docente.clone(),
                    altro_corso.clone(),
                    Relation::Teaches,
                ),
            ] {
                store.add_relation(&CourseRelation {
                    person,
                    course,
                    relation,
                    since: Millis(T0),
                    until: None,
                })?;
            }
            Ok(())
        })
        .expect("scuola");

        let mut config = ServerConfig::new(&corpus);
        config.web_dir = dir.path().join("web");
        std::fs::create_dir_all(&config.web_dir).expect("web");
        let stato = crate::router::stato(db.clone(), config);
        // Lo stesso stato, non uno gemello: il bus che i test osservano deve
        // essere quello su cui le rotte pubblicano.
        let app = crate::routes::router(stato.clone());

        Scuola {
            app,
            stato,
            db,
            corso,
            altro_corso,
            docente,
            altro_docente,
            studente,
            altra_studente,
            estraneo,
            autore,
            corpus,
            dir,
        }
    }

    /// Un argomento, senza scriverlo.
    pub fn argomento(
        corso: &CourseId,
        autore: &PersonId,
        n: u32,
        stato: PublicationState,
    ) -> Argument {
        let rel_path = format!("corsi/{}/lezione-{n:02}.html", corso.as_str());
        Argument {
            id: ArgumentId::from_rel_path(&rel_path),
            title: format!("Lezione {n:02}: i limiti di una funzione"),
            summary: "Che cosa dice un argomento, e chi ha diritto di saperlo.".to_string(),
            state: stato,
            course: corso.clone(),
            prerequisites: Vec::new(),
            origin: Origin::Human {
                by: autore.clone(),
                at: Millis(T0),
            },
            rel_path: Some(rel_path),
            content_hash: format!("sha256:{n:064x}"),
            created_at: Millis(T0),
            updated_at: Millis(T0),
            ratified: None,
        }
    }

    /// Il file HTML dell'argomento, scritto dove il server lo cerca.
    pub fn scrivi_file(&self, argomento: &Argument, corpo: &str) {
        let rel = argomento.rel_path.as_deref().expect("rel_path");
        let percorso = self.corpus.join(rel);
        if let Some(padre) = percorso.parent() {
            std::fs::create_dir_all(padre).expect("cartella");
        }
        std::fs::write(percorso, corpo).expect("file");
    }

    /// Una bozza **non** pubblicata, di un corso, con il suo file.
    pub fn bozza(&mut self, corso: &CourseId, autore: &PersonId, n: u32) -> Argument {
        let argomento = Scuola::argomento(corso, autore, n, PublicationState::Bozza);
        self.scrivi_file(&argomento, "<!doctype html><title>bozza</title><body>bozza</body>");
        self.db
            .write(|store| Ok(store.upsert_argument(&argomento)?))
            .expect("bozza");
        argomento
    }

    /// Un argomento in uso: ratificato e pubblicato, con il suo file.
    pub fn pubblicato(&mut self, corso: &CourseId, autore: &PersonId, n: u32) -> Argument {
        let argomento = self.bozza(corso, autore, n);
        self.db
            .write(|store| {
                store.ratify(&argomento.id, autore, "verificato riga per riga")?;
                store.publish(&argomento.id)?;
                Ok(store.read_argument(autore, &argomento.id)?)
            })
            .expect("pubblicato")
    }

    /// Indicizza un argomento per la ricerca, con un corpo che contiene
    /// `parola`.
    pub fn indicizza(&self, argomento: &Argument, parola: &str) {
        self.db
            .write(|store| {
                Ok(store.index_chunk(&kbs_store::ArtifactChunk {
                    argument: argomento.id.clone(),
                    course: argomento.course.clone(),
                    kind: kbs_store::ChunkKind::Argument,
                    ord: 0,
                    rel_path: argomento.rel_path.clone().unwrap_or_default(),
                    title: argomento.title.clone(),
                    body: format!("In questa lezione si parla di {parola}."),
                    headings: String::new(),
                    code: String::new(),
                    prompt: String::new(),
                    contract: String::new(),
                    updated_at: Millis(T0),
                })?)
            })
            .expect("indicizzazione");
    }

    // ── il client ──────────────────────────────────────────────────────────

    /// Una `GET`, dichiarando chi chiede.
    pub async fn get(&self, uri: &str, persona: &PersonId) -> Response<Body> {
        let richiesta = Request::builder()
            .method("GET")
            .uri(uri)
            .header("x-kbs-person", persona.as_str())
            .body(Body::empty())
            .expect("richiesta");
        self.app.clone().oneshot(richiesta).await.expect("risposta")    }

    /// Una `GET` con header e query liberi: per l'artifact, che è servito su
    /// un altro host.
    pub async fn get_host(&self, host: &str, uri: &str, persona: &PersonId) -> Response<Body> {
        let richiesta = Request::builder()
            .method("GET")
            .uri(uri)
            .header(header::HOST, host)
            .header("x-kbs-person", persona.as_str())
            .body(Body::empty())
            .expect("richiesta");
        self.app.clone().oneshot(richiesta).await.expect("risposta")    }

    /// Una `POST` JSON, dichiarando chi chiede.
    pub async fn post(&self, uri: &str, persona: &PersonId, corpo: serde_json::Value) -> Response<Body> {
        let richiesta = Request::builder()
            .method("POST")
            .uri(uri)
            .header("x-kbs-person", persona.as_str())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(corpo.to_string()))
            .expect("richiesta");
        self.app.clone().oneshot(richiesta).await.expect("risposta")    }

    /// Il corpo di una risposta, come byte.
    pub async fn corpo(response: Response<Body>) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), 64 * 1024 * 1024)
            .await
            .expect("corpo")
            .to_vec()
    }

    /// Il corpo di una risposta, come testo.
    pub async fn testo(response: Response<Body>) -> String {
        String::from_utf8(Scuola::corpo(response).await).expect("utf-8")
    }

    /// L'etichetta dell'host di un artifact.
    pub fn host_artifact(&self, argomento: &Argument) -> String {
        crate::sandbox::artifact_host(&argomento.id, &self.stato.config.artifact_host_suffix)
    }
}

/// Una risposta ridotta a ciò che le rotte promettono di non distinguere.
///
/// Sono status, `content-type` e `cache-control`: tutto ciò che un chiamante
/// può leggere senza fare una richiesta. Il corpo si confronta a parte, e
/// [`Scuola::corpo`] lo dà come byte.
pub fn scheletro(response: &Response<Body>) -> (StatusCode, Option<String>, Option<String>) {
    (
        response.status(),
        header_string(response, header::CONTENT_TYPE),
        header_string(response, header::CACHE_CONTROL),
    )
}

/// Un header come testo, se c'è.
pub fn header_string(response: &Response<Body>, nome: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(nome)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// L'id di un argomento che **non esiste**.
///
/// Ha la forma giusta — `arg_` e sedici esadecimali — quindi il rifiuto non
/// può essere l'errore di forma: è lo stesso errore che dà un argomento che
/// esiste e non è visibile. È il punto del test.
pub fn id_inesistente() -> ArgumentId {
    ArgumentId::from_rel_path("corsi/analisi-1/lezione-99.html")
}
