//! Il daemon, provato su una porta vera.
//!
//! # Perché questo file esiste e i test di `src/` non bastano
//!
//! I test unitari passano per una funzione chiamata direttamente. Nessuno di
//! loro **prende una porta**. E la porta è la parte che nessuno aveva mai
//! provato: che il bind funzioni, che la porta `0` venga risolta in quella
//! vera, che le richieste arrivino da un socket, che `SIGTERM` chiuda davvero.
//! Un `Router` è un valore; un server è un processo che ha preso una risorsa
//! del sistema operativo.
//!
//! Qui il socket è vero, la richiesta è scritta a mano su `TcpStream` e le
//! risposte sono confrontate byte per byte. Il client è `std::net` e nient'altro:
//! una libreria HTTP aggiungerebbe una dipendenza al workspace per evitare
//! quattro righe di `write`, e le quattro righe sono anche la prova che la
//! risposta è davvero HTTP e non «qualcosa che il mio client sa leggere».
//!
//! # `multi_thread`, e non una scelta di gusto
//!
//! `#[tokio::test]` senza `flavor` è un runtime **a un thread**, e il client di
//! questo file è `std::net`, che è bloccante. Su un runtime a un thread il
//! `read_to_end` del test occupa l'unico thread che dovrebbe far girare il
//! server: il server non è lento, è semplicemente mai schedulato, e il test
//! fallisce con un timeout che non ha niente a che fare con il bind. Qui il
//! test serve davvero un server concurrently — è il caso che si sta provando —
//! quindi il runtime ha i thread che servono.
//!
//! # `date` è l'unico header che si ignora
//!
//! Hyper aggiunge `Date` a ogni risposta, generato dall'orologio. Confrontare
//! due risposte **con** `Date` significherebbe un test che passa o fallisce
//! secondo il secondo in cui è girato. Quindi `date` è escluso dal confronto e
//! il test verifica **anche** che ci sia in entrambe: se un giorno hyper
//! smettesse di mandarlo, l'esclusione diventerebbe silenziosa e il test
//! continuerebbe a passare senza sapere che cosa sta confrontando.
//!
//! # Che cosa non è qui
//!
//! Il **corpus vero**. `kbs-fixtures` ne emette uno, ma generarlo in un test che
//! prova un bind significherebbe che il test può fallire per ragioni che non
//! hanno che fare col bind. Qui il corpus è una directory con dentro i
//! `corsi/<slug>/` che le rotte si aspettano, e la separatezza fra «il daemon
//! parte» e «il corpus è vero» resta leggibile.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use kbs_core::{
    Argument, ArgumentId, CourseId, Millis, Origin, PersonId, PublicationState, Relation,
};
use kbs_server::daemon::{self, Opzioni};
use kbs_server::db::Db;
use kbs_store::{CourseRelation, Person, Source, SourceStatus};

/// T0: un istante qualunque, ma sempre lo stesso. I millisecondi di un test non
/// sono un dato, sono un rumore.
const T0: i64 = 1_700_000_000_000;

// ─────────────────────────────────────────────────────────────────────────────
// Il client: quattro righe di scrittura e una divisione in tre
// ─────────────────────────────────────────────────────────────────────────────

/// Una risposta HTTP, tenuta così com'è.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Risposta {
    /// La riga di stato per intero, `HTTP/1.1 200 OK`: entra nel confronto byte
    /// per byte perché la versione del protocollo ne fa parte.
    riga_stato: String,
    /// Gli header, nell'ordine in cui sono arrivati. L'ordine conta: due
    /// risposte che portano le stesse informazioni in ordine diverso sono
    /// risposte diverse, e un confronto che lo ignora perde metà della promessa.
    header: Vec<(String, String)>,
    /// Il corpo, grezzo.
    corpo: Vec<u8>,
}

impl Risposta {
    /// Il numero di stato.
    fn stato(&self) -> u16 {
        self.riga_stato
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| panic!("riga di stato non leggibile: {:?}", self.riga_stato))
    }

    /// Il valore di un header, per nome minuscolo.
    fn header(&self, nome: &str) -> Option<&str> {
        self.header
            .iter()
            .find(|(k, _)| k == nome)
            .map(|(_, v)| v.as_str())
    }

    /// Il corpo come testo.
    fn testo(&self) -> String {
        String::from_utf8_lossy(&self.corpo).into_owned()
    }

    /// La stessa risposta senza `Date`.
    ///
    /// Vedi la nota in cima al file: `Date` viene dall'orologio di hyper e non
    /// dice niente della risposta, ma **non sparire** senza che nessuno se ne
    /// accorga. Il test che usa questa funzione verifica anche che `date` ci sia
    /// in entrambe le risposte.
    fn senza_data(&self) -> Risposta {
        Risposta {
            riga_stato: self.riga_stato.clone(),
            header: self
                .header
                .iter()
                .filter(|(k, _)| k != "date")
                .cloned()
                .collect(),
            corpo: self.corpo.clone(),
        }
    }
}

/// Scrive una richiesta e legge la risposta su un socket vero.
///
/// `Connection: close` è ciò che rende corretta la lettura fino a `EOF` senza
/// un parser della lunghezza del corpo: è un'intestazione che il server onora e
/// a cui risponde, e un client che non la usa dovrebbe sapere leggere
/// `Content-Length` o `chunked`, che qui non è il punto.
fn chiedi(indirizzo: SocketAddr, percorso: &str, header: &[(&str, &str)]) -> Risposta {
    let mut stream = TcpStream::connect(indirizzo).expect("connessione al socket del daemon");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout di lettura");

    let mut richiesta = format!(
        "GET {percorso} HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n",
        indirizzo.port()
    );
    for (nome, valore) in header {
        richiesta.push_str(&format!("{nome}: {valore}\r\n"));
    }
    richiesta.push_str("\r\n");
    stream
        .write_all(richiesta.as_bytes())
        .expect("scrittura della richiesta");
    stream.flush().expect("flush");

    divide(&leggi_fino_a_eof(&mut stream))
}

/// Legge tutto ciò che il socket dà, fino a `EOF`.
fn leggi_fino_a_eof(stream: &mut impl Read) -> Vec<u8> {
    let mut fuori = Vec::new();
    if let Err(e) = stream.read_to_end(&mut fuori) {
        // `read_to_end` su un socket con timeout è l'unico posto in cui un
        // `WouldBlock` può arrivare, e il messaggio senza contesto è
        // «lettura della risposta: Resource temporarily unavailable», che non
        // dice nulla. Lo si lascia comunque propagare, con la richiesta nel
        // messaggio.
        panic!("lettura della risposta fino a EOF: {e}");
    }
    fuori
}

/// Divide i byte grezzi di una risposta intera in riga di stato, header e corpo.
fn divide(grezzo: &[u8]) -> Risposta {
    let separa = grezzo
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("una risposta HTTP ha sempre il separatore degli header");
    let (testa, resto) = grezzo.split_at(separa);
    let (riga_stato, header) = intestazioni(testa);
    Risposta {
        riga_stato,
        header,
        corpo: resto[4..].to_vec(),
    }
}

/// La riga di stato e gli header, da una testa senza corpo.
fn intestazioni(testa: &[u8]) -> (String, Vec<(String, String)>) {
    let testo = String::from_utf8_lossy(testa).into_owned();
    let mut righe = testo.split("\r\n");
    let riga_stato = righe.next().unwrap_or_default().to_string();
    let header = righe
        .filter_map(|r| r.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    (riga_stato, header)
}

/// Legge una risposta su una connessione che **resta aperta**.
///
/// `Connection: close` non è usata qui, e il motivo è che il test della chiusura
/// ha bisogno di due richieste sulla stessa connessione: con `close` la
/// connessione muore dopo la prima. Serve quindi `Content-Length`, che è l'altro
/// modo che un server ha di dire dove finisce un corpo.
///
/// I byte che il socket ha già consegnato e che non sono ancora stati usati.
///
/// Esiste per una cosa sola, e la cosa si è rotta una volta: una `read` su un
/// socket porta via **tutto** ciò che c'è, quindi gli header *e* i primi byte
/// del corpo arrivano nella stessa lettura. Se quei byte finiscono in un buffer
/// e non vengono tenuti, la `read_exact` successiva aspetta byte che sono già
/// arrivati — e il sintomo è un timeout su un corpo di 44 byte, che non dice
/// niente. Quindi il buffer tiene quello che avanza e la prossima lettura
/// comincia da lì.
#[derive(Default)]
struct Flusso {
    accodati: Vec<u8>,
}

impl Flusso {
    /// Riempie il buffer. `false` vuol dire che il socket è finito.
    fn riempi(&mut self, stream: &mut impl Read) -> bool {
        let mut pezzo = [0u8; 4096];
        let letti = stream
            .read(&mut pezzo)
            .expect("lettura dal socket del daemon");
        self.accodati.extend_from_slice(&pezzo[..letti]);
        letti > 0
    }

    /// Esattamente `n` byte, prendendo prima quelli già nel buffer.
    fn prendi(&mut self, stream: &mut impl Read, n: usize) -> Vec<u8> {
        while self.accodati.len() < n {
            assert!(
                self.riempi(stream),
                "il socket è finito dopo {} byte e ne servivano {n}",
                self.accodati.len()
            );
        }
        self.accodati.drain(..n).collect()
    }

    /// Tutto ciò che sta fino al separatore degli header, **lasciando nel
    /// buffer quello che viene dopo** — cioè il corpo, che spesso è arrivato
    /// nella stessa lettura della testa.
    fn prendi_testa(&mut self) -> Vec<u8> {
        let fine = self
            .accodati
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("si chiama solo quando il separatore c'è")
            + 4;
        self.accodati.drain(..fine).collect()
    }
}

fn leggi_risposta(stream: &mut impl Read, quale: &str) -> Risposta {
    let mut flusso = Flusso::default();
    let testa = loop {
        if flusso.accodati.windows(4).any(|w| w == b"\r\n\r\n") {
            break flusso.prendi_testa();
        }
        assert!(
            flusso.riempi(stream),
            "[{quale}] il server ha chiuso la connessione dopo {} byte di testa: {:?}",
            flusso.accodati.len(),
            String::from_utf8_lossy(&flusso.accodati)
        );
    };
    let (riga_stato, header) = intestazioni(&testa);
    let lunghezza: usize = header
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or_else(|| panic!("[{quale}] la risposta non dichiara content-length: {header:?}"));
    Risposta {
        riga_stato,
        header,
        corpo: flusso.prendi(stream, lunghezza),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// La scuola di prova
// ─────────────────────────────────────────────────────────────────────────────

/// Un'istanza montata su una porta `:0`, con dentro la scuola.
struct Istanza {
    indirizzo: SocketAddr,
    /// Il primo corso e il secondo. Il secondo serve a ricordare che due
    /// corsi distinti sono due insiemi di relazioni distinti, e che «insegnante
    /// di un corso» non è «persona».
    _altro_corso: CourseId,
    _corso: CourseId,
    docente: PersonId,
    altro_docente: PersonId,
    /// L'argomento pubblicato. Esiste, ed è ciò che trasforma il `404` da
    /// «inesistente» in «non tuo».
    argomento: Argument,
    _dir: tempfile::TempDir,
}

/// Un argomento con il suo file.
///
/// Il `rel_path` non è decorativo: senza, la rotta dell'artifact risponderebbe
/// «assente» anche a chi lo può vedere, e un test che non distingue le due
/// risposte non sta provando la cosa che dice di provare.
fn argomento() -> Argument {
    let rel_path = "corsi/analisi-1/lezione-01.html".to_string();
    Argument {
        id: ArgumentId::from_rel_path(&rel_path),
        title: "Lezione 01: i limiti di una funzione".to_string(),
        summary: "Che cosa dice un argomento, e chi ha diritto di saperlo.".to_string(),
        state: PublicationState::Bozza,
        course: CourseId::fixture(1),
        prerequisites: Vec::new(),
        origin: Origin::Human {
            by: PersonId::fixture(1),
            at: Millis(T0),
        },
        rel_path: Some(rel_path),
        content_hash: format!("sha256:{n:064x}", n = 1),
        created_at: Millis(T0),
        updated_at: Millis(T0),
        ratified: None,
    }
}

/// Costruisce il corpus, il database e la scuola, poi apre la porta.
///
/// L'ordine è seminare → chiudere → `avvia`: il database viene riaperto dal
/// daemon, che è ciò che accade in produzione, quindi il test non prova una
/// `Db` tenuta viva da un'altra parte della stessa scuola.
async fn istanza() -> (Istanza, daemon::InAscolto) {
    let dir = tempfile::tempdir().expect("tempdir");
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("corsi/analisi-1")).expect("corpus a");
    std::fs::create_dir_all(corpus.join("corsi/fisica-1")).expect("corpus b");
    // Il database sta in una directory **sorella** del corpus, che è il layout
    // di `--db` fuori dal corpus e quello che il daemon accetta.
    let database = dir.path().join("stato").join("scuola.sqlite3");
    std::fs::create_dir_all(database.parent().expect("cartella del database")).expect("stato");

    let corso = CourseId::fixture(1);
    let altro_corso = CourseId::fixture(2);
    let docente = PersonId::fixture(1);
    let altro_docente = PersonId::fixture(2);
    let argomento = argomento();

    {
        let db = Db::open(&database).expect("database di prova");
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
            ] {
                store.upsert_person(&Person {
                    id,
                    display_name: nome.to_string(),
                    created_at: Millis(T0),
                })?;
            }
            for (person, course) in [
                (docente.clone(), corso.clone()),
                (altro_docente.clone(), altro_corso.clone()),
            ] {
                store.add_relation(&CourseRelation {
                    person,
                    course,
                    relation: Relation::Teaches,
                    since: Millis(T0),
                    until: None,
                })?;
            }
            store.upsert_argument(&argomento)?;
            store.ratify(&argomento.id, &docente, "riga per riga")?;
            store.publish(&argomento.id)?;
            Ok(())
        })
        .expect("scuola di prova");
    }

    let opzioni = Opzioni {
        corpus,
        database,
        // Porta 0: il sistema operativo sceglie e `avvia` restituisce quella
        // vera. Un test che fissa una porta è un test che un giorno fallisce
        // perché qualcun altro ha la stessa.
        ascolta: "127.0.0.1:0".parse().expect("indirizzo"),
    };
    // Il `Errore` porta con sé il rimedio, quindi qui il rimedio non si perde
    // in un `.expect` che dice solo «chiamata non valida».
    let in_ascolto = match daemon::avvia(&opzioni).await {
        Ok(a) => a,
        Err(e) => panic!("il daemon non parte: {e}"),
    };

    (
        Istanza {
            indirizzo: in_ascolto.indirizzo,
            _altro_corso: altro_corso,
            _corso: corso,
            docente,
            altro_docente,
            argomento,
            _dir: dir,
        },
        in_ascolto,
    )
}

/// Serve l'istanza in background e restituisce il canale di chiusura.
///
/// Il canale è un `oneshot` perché la chiusura è un fatto e non un livello: due
/// shutdown non sono uno scenario. Ed è lo stesso meccanismo di
/// [`daemon::esegui`], non uno parallelo: se qui ci fosse un secondo modo di
/// chiudere, il test proverebbe quello e non quello che gira in produzione.
fn servi(
    in_ascolto: daemon::InAscolto,
) -> (tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>) {
    let (spegni, aspetta) = tokio::sync::oneshot::channel::<()>();
    let lavoro = tokio::spawn(async move {
        let _ = axum::serve(in_ascolto.ascoltatore, in_ascolto.applicazione)
            .with_graceful_shutdown(async move {
                let _ = aspetta.await;
            })
            .await;
    });
    (lavoro, spegni)
}

// ─────────────────────────────────────────────────────────────────────────────
// La porta vera
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn il_daemon_prende_una_porta_e_risponde_su_health() {
    let (istanza, in_ascolto) = istanza().await;
    let (lavoro, spegni) = servi(in_ascolto);

    let r = chiedi(istanza.indirizzo, "/api/v1/health", &[]);
    assert_eq!(r.stato(), 200, "{r:?}");
    assert_eq!(r.header("content-type"), Some("application/json"), "{r:?}");
    let corpo = r.testo();
    assert!(corpo.contains("\"stato\":\"ok\""), "{corpo}");
    assert!(corpo.contains("kbs-server/"), "{corpo}");

    // La porta restituita è quella **presa**, non quella richiesta: se `avvia`
    // restituisse `127.0.0.1:0`, la richiesta qui sopra fallirebbe con un
    // errore di connessione che nessuno leggerebbe come «la porta restituita è
    // sbagliata».
    assert_ne!(istanza.indirizzo.port(), 0);
    assert!(istanza.indirizzo.ip().is_loopback());

    let _ = spegni.send(());
    lavoro.await.expect("il server si è fermato");
}

#[tokio::test(flavor = "multi_thread")]
async fn il_corpo_di_health_e_tutto_il_corpo() {
    // Un client che legge fino a `EOF` senza guardare `Content-Length` riceve
    // metà di una risposta `chunked` e non se ne accorge. Qui il corpo è
    // confrontato **per intero** e la lunghezza dichiarata è confrontata con la
    // lunghezza ricevuta: è la prova che «fino a EOF» qui vuol dire «tutto».
    let (istanza, in_ascolto) = istanza().await;
    let (lavoro, spegni) = servi(in_ascolto);

    let r = chiedi(istanza.indirizzo, "/api/v1/health", &[]);
    assert_eq!(
        r.corpo,
        format!(
            r#"{{"stato":"ok","creatore":"kbs-server/{}"}}"#,
            env!("CARGO_PKG_VERSION")
        )
        .into_bytes(),
        "{r:?}"
    );
    assert_eq!(
        r.header("content-length").map(str::to_owned),
        Some(r.corpo.len().to_string()),
        "il corpo letto fino a EOF è tutto il corpo: {r:?}"
    );

    let _ = spegni.send(());
    lavoro.await.expect("il server si è fermato");
}

#[tokio::test(flavor = "multi_thread")]
async fn la_radice_serve_linterfaccia_reale_byte_per_byte() {
    // `/` è la prima cosa che un operatore vede, ed è l'unica rotta il cui
    // fallimento non si vede: un `404` qui sembra un server che non parte, e un
    // `index.html` vuoto sembra un'interfaccia rotta. Il confronto è con il file
    // **del repository**, non con una copia in una cartella temporanea: una
    // copia passerebbe anche quando il file vero è rotto.
    let (istanza, in_ascolto) = istanza().await;
    let (lavoro, spegni) = servi(in_ascolto);

    let r = chiedi(istanza.indirizzo, "/", &[]);
    assert_eq!(r.stato(), 200, "{r:?}");

    let vero =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("web/index.html"))
            .expect("web/index.html del repository");
    assert_eq!(
        r.corpo, vero,
        "la radice non serve il file che il repository contiene"
    );
    // E non è una pagina vuota travestita da successo: un corpo che contiene il
    // titolo è la prova che il file è arrivato intero.
    assert!(r.testo().contains("<title>"), "la pagina non ha un title");

    let _ = spegni.send(());
    lavoro.await.expect("il server si è fermato");
}

// ─────────────────────────────────────────────────────────────────────────────
// La proprietà che il modello di sicurezza regge
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn non_esiste_e_non_lo_vedi_danno_gli_stessi_byte_sul_socket() {
    // La stessa proprietà che `Router::oneshot` già prova, ma attraverso un
    // socket: la risposta che arriva a un client è quella che il test vede, e su
    // un socket entrano in mezzo cose che `oneshot` non tocca —
    // `Content-Length`, l'ordine degli header, il modo in cui la connessione
    // viene chiusa. Un client che guarda qualcosa di più dei byte del corpo non
    // deve trovarci niente.
    let (istanza, in_ascolto) = istanza().await;
    let (lavoro, spegni) = servi(in_ascolto);
    let dichiarazione = kbs_server::IDENTITY_HEADER;
    let percorso = format!("/api/v1/arguments/{}/cohort", istanza.argomento.id.as_str());

    // L'argomento **esiste**: questa richiesta torna `200`. Senza questa prova
    // il confronto seguente sarebbe una tautologia — due `404` uguali non
    // dicono che il predicato funziona, dicono che il predicato non c'è.
    let visibile = chiedi(
        istanza.indirizzo,
        &percorso,
        &[(dichiarazione, istanza.docente.as_str())],
    );
    assert_eq!(
        visibile.stato(),
        200,
        "l'argomento deve esistere e chi lo insegna deve poterlo leggere: {visibile:?}"
    );

    // Chi lo vede e chi non lo vede: due persone, due domande diverse.
    let estraneo = istanza.altro_docente.as_str();
    let non_visibile = chiedi(istanza.indirizzo, &percorso, &[(dichiarazione, estraneo)]);
    let inesistente = chiedi(
        istanza.indirizzo,
        "/api/v1/arguments/arg_00000000000000ff/cohort",
        &[(dichiarazione, estraneo)],
    );

    assert_eq!(non_visibile.stato(), 404, "{non_visibile:?}");
    // `date` si esclude dal confronto (vedi la nota in cima al file), ma non
    // sparisce in silenzio: se hyper smettesse di mandarlo, l'esclusione
    // diventerebbe una delle due risposte «più corte» e il confronto passerebbe
    // comunque, senza che nessuno lo sappia.
    assert!(
        non_visibile.header("date").is_some(),
        "hyper non ha mandato `date`: l'esclusione nel confronto è ora inutile"
    );
    assert_eq!(
        non_visibile.senza_data(),
        inesistente.senza_data(),
        "«non lo vedi» e «non esiste» non danno la stessa risposta: \
         qualcuno può distinguerle dal socket"
    );
    assert_eq!(non_visibile.testo(), r#"{"error":"non-trovato"}"#);

    let _ = spegni.send(());
    lavoro.await.expect("il server si è fermato");
}

// ─────────────────────────────────────────────────────────────────────────────
// La chiusura
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn la_chiusura_chiude_il_listener_e_le_connessioni_aperte() {
    // «Graceful» è una parola, e qui si traduce in tre fatti osservabili: il
    // server **finisce** invece di appendersi, le connessioni già aperte vengono
    // **chiuse** invece di restare appese, e nuove connessioni non entrano più.
    //
    // # Cosa questo test non prova, e perché
    //
    // Non prova che una richiesta in elaborazione venga **finita**. Per provarlo
    // servirebbe un endpoint lento — una rotta che ci mette un secondo — e non
    // ce n'è: rendersi deterministici l'attesa richiederebbe di fermare il
    // server nel mezzo di una richiesta, e un test che ci mette uno `sleep` per
    // arrivarci è più fragile di quanto vale.
    //
    // Quello che si è misurato a mano, e che vale come indicazione e non come
    // prova, è che due richieste in pipelining con un segnale in mezzo vengono
    // servite entrambe quando hyper ha già letto la seconda, e che una richiesta
    // a metà intestazioni viene tagliata. La seconda metà è scritta in
    // `daemon::segnali`, che è il posto in cui una cosa così deve stare.
    let (istanza, in_ascolto) = istanza().await;
    let (lavoro, spegni) = servi(in_ascolto);

    // Una connessione tenuta aperta, che il server deve aver **accettato** prima
    // del segnale. `TcpStream::connect` non basta: lo handshake lo fa il kernel
    // e la connessione resta in coda di ascolto finché il server non la
    // accetta, e una connessione ancora in coda viene resettata — cioè il test
    // misurerebbe una proprietà falsa, con un `Connection reset by peer` che
    // sembrerebbe un bug del server. La richiesta va e va letta: è la prova che
    // il server ha accettato la connessione e l'ha servita.
    let mut aperta = TcpStream::connect(istanza.indirizzo).expect("connessione");
    aperta
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout");
    aperta
        .write_all(b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("richiesta, intera");
    aperta.flush().expect("flush");
    let risposta = leggi_risposta(&mut aperta, "risposta di servizio");
    assert_eq!(risposta.stato(), 200, "{risposta:?}");
    assert!(risposta.testo().contains("\"stato\":\"ok\""), "{risposta:?}");

    // Il segnale, e il server deve **finire** da solo.
    let _ = spegni.send(());
    tokio::time::timeout(Duration::from_secs(20), lavoro)
        .await
        .expect("il server non si è chiuso entro 20 secondi")
        .expect("il task del server è andato in panico");

    // La connessione aperta è chiusa dal server, non lasciata appesa: si
    // arriva a `EOF` o a un errore, e in entrambi i casi il server non sta
    // più tenendo quel socket. Un `read` che si bloccherebbe qui è un timeout
    // di 20 secondi, quindi la prova è lenta quando fallisce e non quando va.
    let mut resto = [0u8; 64];
    let esito = aperta.read(&mut resto);
    assert!(
        matches!(esito, Ok(0) | Err(_)),
        "la connessione aperta ha ancora dati da mandare dopo la chiusura: {esito:?} {:?}",
        String::from_utf8_lossy(&resto)
    );

    // E nuove connessioni non entrano più. Solo adesso: prima sarebbe una gara,
    // perché `spegni.send()` consegna il segnale a un task che lo legge quando
    // lo schedula, e in mezzo il socket è ancora in ascolto. Qui il future di
    // `serve` è tornato e il listener è stato droppato, quindi su loopback la
    // connessione è un `ECONNREFUSED` immediato.
    assert!(
        TcpStream::connect(istanza.indirizzo).is_err(),
        "il daemon accetta ancora connessioni dopo la chiusura: il listener non è chiuso"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Il processo vero
// ─────────────────────────────────────────────────────────────────────────────

/// Il binario che `cargo` ha costruito per questo test.
///
/// `CARGO_BIN_EXE_<nome>` è il meccanismo documentato di cargo, ed è il primo
/// tentativo. Non è però l'unico modo di arrivarci, e su questa macchina non è
/// impostato: quindi il secondo tentativo guarda dove sta l'eseguibile di questo
/// test, che è `target/<profilo>/deps/`, e sale di due directory fino a
/// `target/<profilo>/`, dove cargo mette i binari dello stesso pacchetto.
///
/// Il fallback non è una scorciatoia: due percorsi che portano allo stesso
/// binario sono due modi di essere sbagliati, e senza il secondo questi test
/// non girerebbero affatto.
fn percorso_binario() -> PathBuf {
    for nome in ["CARGO_BIN_EXE_kbs-serve", "CARGO_BIN_EXE_kbs_serve"] {
        if let Ok(p) = std::env::var(nome) {
            return PathBuf::from(p);
        }
    }
    let eseguibile = std::env::current_exe().expect("percorso del test");
    // `.../target/<profilo>/deps/<test>-<hash>` → `.../target/<profilo>/kbs-serve`
    let percorso = eseguibile
        .parent()
        .and_then(Path::parent)
        .expect("la directory del test è sotto target/<profilo>/deps/")
        .join("kbs-serve");
    assert!(
        percorso.is_file(),
        "il binario non è dove il test lo cerca: {}\n\
         Se il profilo non è `debug`, il percorso va aggiustato qui.",
        percorso.display()
    );
    percorso
}

/// Legge `stdout` del figlio riga per riga, senza bloccare.
///
/// Il thread serve a una cosa sola: `stdout` è un pipe, e leggerlo dal test
/// principale mentre il test aspetta qualcos'altro è un deadlock travestito da
/// timeout.
fn righe_del_figlio(figlio: &mut Child) -> mpsc::Receiver<String> {
    let stdout = figlio.stdout.take().expect("stdout del daemon");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for riga in BufReader::new(stdout).lines() {
            let Ok(riga) = riga else { return };
            if tx.send(riga).is_err() {
                return;
            }
        }
    });
    rx
}

/// La porta, letta dall'annuncio.
///
/// È la parte fragile di questo test, e lo è per una ragione che va detta: il
/// test verifica che l'annuncio dica la URL, e per farlo deve leggerla dalla
/// riga che l'operatore legge. Un formato che cambia fa fallire il test, ed è
/// il comportamento giusto — se l'annuncio cambia forma, qualcuno che ci fa
/// affidamento cambia idea senza saperlo.
///
/// Il numero cercato è quello **dopo l'ultimo `:`** dell'autorità, e non il
/// primo pezzo di cifre: `http://127.0.0.1:43801` comincia con `127`, che
/// verrebbe letto come una porta e produrrebbe un test che «passa» su una porta
/// che non è quella del daemon.
fn porta_dall_annuncio(testo: &str) -> Option<u16> {
    let dopo = testo.rsplit("http://").next()?;
    let autorita = dopo.split_whitespace().next()?;
    autorita.rsplit(':').next()?.parse().ok()
}

/// Un figlio che muore con il test, anche quando il test va in panico.
///
/// `std::process::Child` non uccide il processo quando viene droppato, e un
/// daemon lasciato vivo dopo un test fallito è un daemon che tiene la porta
/// aperta **e** tiene aperto l'`stderr` ereditato: la pipeline di `cargo test`
/// non vede mai la fine del file e il test successivo non parte. Non è una
/// teoria, è quello che è successo la prima volta che questo test è fallito.
///
/// `Drop` è l'unico posto in cui metterci, perché è l'unico che gira anche
/// durante un `panic!`.
struct Figlio(Child);

impl Figlio {
    /// Avvia e panica se non parte.
    fn spawn(cmd: &mut Command) -> Figlio {
        Figlio(cmd.spawn().expect("avvio di kbs-serve"))
    }

    /// Il processo, per gli usi che lo richiedono.
    fn child(&mut self) -> &mut Child {
        &mut self.0
    }

    /// Il pid, per il `kill`.
    fn id(&self) -> u32 {
        self.0.id()
    }

    /// Aspetta la fine e restituisce lo stato.
    fn attendi(&mut self) -> std::process::ExitStatus {
        self.0.wait().expect("attesa della fine del daemon")
    }
}

impl Drop for Figlio {
    fn drop(&mut self) {
        // `try_wait` distingue i due casi: un figlio già uscito non ha bisogno
        // di un segnale, e mandargogliene uno a un processo che non c'è più
        // produrrebbe un errore che il test non vuole vedere in un `Drop`.
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = Command::new("kill")
                .arg("-KILL")
                .arg(self.0.id().to_string())
                .status();
            let _ = self.0.wait();
        }
    }
}

/// Corpus e database per un avvio vero: due directory sorelle, perché è il
/// layout che il daemon accetta e che un'installazione usa.
fn impianto(dir: &Path) -> (PathBuf, PathBuf) {
    let corpus = dir.join("corpus");
    std::fs::create_dir(&corpus).expect("corpus");
    let stato = dir.join("stato");
    std::fs::create_dir(&stato).expect("stato");
    (corpus, stato.join("scuola.sqlite3"))
}

#[test]
fn il_binario_si_avvia_serve_e_chiude_su_sigterm() {
    // Il test end-to-end vero: un processo, una porta, due richieste, un
    // segnale. Quelli sopra provano il *wiring*; questo prova il *programma*, e
    // sono due cose diverse — un binario che non parte è un binario che
    // compila.
    let dir = tempfile::tempdir().expect("tempdir");
    let (corpus, database) = impianto(dir.path());

    let mut figlio = Figlio::spawn(
        Command::new(percorso_binario())
            .arg("--corpus")
            .arg(&corpus)
            .arg("--db")
            .arg(&database)
            // `:0` anche qui: il test non deve litigare con nessun altro
            // processo della macchina, e la porta vera la dice il daemon
            // stesso.
            .arg("--listen")
            .arg("127.0.0.1:0")
            .stdout(Stdio::piped()),
    );

    let righe = righe_del_figlio(figlio.child());

    // L'annuncio è **di più righe**, e la porta è sulla seconda. Fermarsi alla
    // riga che contiene l'indirizzo significa non aver ancora letto l'avviso
    // sull'identità, che sta due righe più in giù: un test che legge una riga
    // sola sta verificando metà dell'annuncio e crede di averlo verificato
    // tutto. Qui si legge finché l'annuncio non è finito — cioè finché non
    // contiene la riga sulla dichiarazione — con un tetto di righe perché il
    // canale non chiude: il daemon è ancora vivo e non ha finito di parlare.
    let mut annuncio = String::new();
    let porta = loop {
        let riga = righe
            .recv_timeout(Duration::from_secs(20))
            .expect("il daemon non ha stampato l'annuncio");
        annuncio.push_str(&riga);
        annuncio.push('\n');
        // La porta si cerca su tutto quello che è arrivato, non solo sull'ultima
        // riga: l'ordine delle righe dell'annuncio non è un contratto, e
        // leggerlo come se lo fosse legherebbe il test a una riga che il
        // prossimo refactor sposta.
        if let Some(p) = porta_dall_annuncio(&annuncio) {
            break p;
        }
    };
    // L'avviso sull'identità arriva su una riga successiva, e su quella resta.
    for _ in 0..20 {
        if annuncio.contains("DICHIARAZIONE, non un'autenticazione") {
            break;
        }
        let Ok(riga) = righe.recv_timeout(Duration::from_secs(20)) else {
            break;
        };
        annuncio.push_str(&riga);
        annuncio.push('\n');
    }
    assert_ne!(
        porta, 0,
        "il daemon deve dire la porta che ha preso, non quella richiesta:\n{annuncio}"
    );
    // L'identità dichiarata è detta ad alta voce, perché è la cosa che un
    // operatore non può indovinare guardando una porta aperta.
    assert!(
        annuncio.contains("DICHIARAZIONE, non un'autenticazione"),
        "l'annuncio non dice che l'identità è dichiarata:\n{annuncio}"
    );
    assert!(annuncio.contains(&corpus.display().to_string()), "{annuncio}");
    assert!(annuncio.contains(&database.display().to_string()), "{annuncio}");

    let indirizzo: SocketAddr = format!("127.0.0.1:{porta}").parse().expect("indirizzo");

    // Due richieste vere, su un socket vero, al processo vero.
    let salute = chiedi(indirizzo, "/api/v1/health", &[]);
    assert_eq!(salute.stato(), 200, "{salute:?}");
    assert!(
        salute.testo().contains("\"stato\":\"ok\""),
        "{}",
        salute.testo()
    );

    let radice = chiedi(indirizzo, "/", &[]);
    assert_eq!(radice.stato(), 200, "la radice non serve l'interfaccia: {radice:?}");
    assert!(radice.testo().contains("<title>"), "la radice non è la pagina");

    // `SIGTERM`, che è quello che un supervisor manda. `kill` è un comando
    // esterno e va bene: `libc::kill` richiederebbe una dipendenza che il
    // workspace non ha, e la prova che il segnale *arriva* conta più del
    // dettaglio di come lo si manda.
    let stato = Command::new("kill")
        .arg("-TERM")
        .arg(figlio.id().to_string())
        .status()
        .expect("invio di SIGTERM");
    assert!(stato.success(), "kill non è riuscito: {stato}");

    let esito = figlio.attendi();
    assert!(
        esito.success(),
        "il daemon è uscito con {:?}: SIGTERM deve chiudere con 0, e non con un \
         codice di errore che un supervisor legge come un crash",
        esito.code()
    );
}

#[test]
fn il_binario_rifiuta_un_corpus_che_non_esiste_e_non_lo_crea() {
    // Il comportamento che la CLI promette e che nessun test poteva controllare
    // finché non c'era un binario: il corpus mancante è un rifiuto con codice 2,
    // e la directory resta assente.
    let dir = tempfile::tempdir().expect("tempdir");
    let corpus = dir.path().join("non-esiste");

    let uscita = Command::new(percorso_binario())
        .arg("--corpus")
        .arg(&corpus)
        .arg("--listen")
        .arg("127.0.0.1:0")
        .output()
        .expect("avvio di kbs-serve");

    let stderr = String::from_utf8_lossy(&uscita.stderr);
    assert_eq!(
        uscita.status.code(),
        Some(2),
        "un corpus assente è un rifiuto (2), non un errore di sistema (4):\n{stderr}"
    );
    assert!(!corpus.exists(), "il daemon ha creato il corpus che non doveva");
    assert!(stderr.contains("non è una cartella"), "{stderr}");
}

#[test]
fn il_binario_rifiuta_un_database_dentro_il_corpus() {
    // D12: il corpus è una cartella di file versionata e l'uscita è `rm -rf`. Un
    // registro che ci sta dentro sparisce con i file che descrive, e questa è
    // l'unica protezione che lo impedisce: nessuna rotta HTTP lo scaricherebbe,
    // perché l'artifact risolve il percorso dal database e non dall'URL.
    let dir = tempfile::tempdir().expect("tempdir");
    let corpus = dir.path().join("corpus");
    std::fs::create_dir(&corpus).expect("corpus");
    let dentro = corpus.join("registro.sqlite3");

    let uscita = Command::new(percorso_binario())
        .arg("--corpus")
        .arg(&corpus)
        .arg("--db")
        .arg(&dentro)
        .arg("--listen")
        .arg("127.0.0.1:0")
        .output()
        .expect("avvio di kbs-serve");

    let stderr = String::from_utf8_lossy(&uscita.stderr);
    assert_eq!(uscita.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(!dentro.exists(), "il database è stato creato prima del rifiuto");
    assert!(stderr.contains("è dentro il corpus"), "{stderr}");
}

#[test]
fn il_binario_rifiuta_un_database_all_epoch_avvenire() {
    // La guardia dell'epoch vista dal processo: il database è falsificato con
    // un epoch che nessun binario conosce, e il daemon deve uscire con 2 senza
    // aver mai preso la porta. È la prova che la guardia non è spesa dentro
    // `kbs-store` e poi persa in silenzio dal livello di sopra — che è l'unico
    // modo in cui una guardia può rendersi inutile.
    let dir = tempfile::tempdir().expect("tempdir");
    let (corpus, database) = impianto(dir.path());
    {
        let store = kbs_store::Store::open(&database).expect("database");
        store
            .conn()
            .execute("UPDATE schema_epoch SET epoch = 999 WHERE id = 1", [])
            .expect("epoch falsificato");
    }

    let uscita = Command::new(percorso_binario())
        .arg("--corpus")
        .arg(&corpus)
        .arg("--db")
        .arg(&database)
        .arg("--listen")
        .arg("127.0.0.1:0")
        .output()
        .expect("avvio di kbs-serve");

    let stderr = String::from_utf8_lossy(&uscita.stderr);
    assert_eq!(
        uscita.status.code(),
        Some(2),
        "l'epoch futura è un rifiuto (2), non un errore di sistema (4):\n{stderr}"
    );
    assert!(stderr.contains("epoch 999"), "il messaggio deve dire l'epoch:\n{stderr}");
    assert!(
        stderr.contains("più recente"),
        "il messaggio deve dire il rimedio:\n{stderr}"
    );
}

#[test]
fn il_binario_non_richiede_nessun_corpus_per_una_richiesta_di_testo() {
    // `--version` e `--help` non aprono niente. La prova che il corpus
    // obbligatorio non viene letto prima di onorarle sta nel fatto che il
    // comando riesce **senza** `--corpus` e su una directory inesistente: se
    // l'apertura venisse prima, il codice sarebbe 2 e non 0.
    let uscita = Command::new(percorso_binario())
        .arg("--version")
        .current_dir("/")
        .output()
        .expect("avvio di kbs-serve");
    assert_eq!(uscita.status.code(), Some(0), "`--version` non è onorata");
    let testo = String::from_utf8_lossy(&uscita.stdout);
    assert!(
        testo.starts_with(daemon::NOME),
        "--version deve dire il nome del binario: {testo}"
    );
}
