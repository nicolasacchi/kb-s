//! Il bus partizionato, visto da due richieste aperte insieme.
//!
//! Il test che conta è il primo: due sottoscrittori aperti su due corsi
//! diversi, un atto su ciascuno, e nessuno dei due deve vedere l'atto dell'altro.
//! Il canale di B viene **letto** nello stesso test, perché un canale che non
//! riceve nulla passerebbe anche rotto: se «A non sente B» fosse l'unica
//! asserzione, basterebbe un bus che non trasmette niente.

use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::util::ServiceExt;

use super::Scuola;

/// Quanto si aspetta un frame prima di dichiararlo assente.
///
/// Breve e dichiarato: un test che aspetta «un po'» è un test che ogni tanto è
/// lento senza dirlo.
const ATTESA: u64 = 2_000;

/// Il frame successivo, o `None` se non arriva.
///
/// Un frame SSE è un blocco di righe terminate da una riga vuota, e il
/// keep-alive è un commento che non ha `event:`: un test che contasse i byte
/// senza guardare il nome dell'evento conterebbe anche i keep-alive, che qui
/// devono stare fuori dal conto.
async fn frame_entro(risposta: &mut Response<Body>, millis: u64) -> Option<String> {
    let atteso = tokio::time::timeout(Duration::from_millis(millis), risposta.body_mut().frame())
        .await
        .ok()?;
    // `frame()` dà `Result<Option<Frame>, _>`: il primo `?` passa dal
    // `Result` del timeout, il secondo dal `Result` del body, e il terzo
    // libera l'`Option` del frame.
    let dati = atteso?.ok()?.into_data().ok()?;
    Some(String::from_utf8(dati.to_vec()).ok()?)
}

/// Apre il canale eventi di un corso, come persona.
///
/// La sottoscrizione al bus avviene **nell'handler**, non quando il flusso
/// viene letto: è la ragione per cui questa funzione ritorna prima che il test
/// produca un atto, e la ragione per cui l'ordine delle due righe qui sotto è
/// quello giusto.
async fn apri(scuola: &Scuola, corso: &str, persona: &kbs_core::PersonId) -> Response<Body> {
    let richiesta = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/courses/{corso}/events"))
        .header("x-kbs-person", persona.as_str())
        .body(Body::empty())
        .expect("richiesta");
    scuola.app.clone().oneshot(richiesta).await.expect("risposta")
}

/// Il nome dell'evento di un frame.
fn nome_evento(frame: &str) -> Option<&str> {
    frame
        .lines()
        .find_map(|riga| riga.strip_prefix("event: "))
        .map(str::trim)
}

/// Il primo evento **vero** di un canale, saltando il `ciao`.
///
/// Il `ciao` è dichiarazione di offsets, non un fatto accaduto, quindi non deve
/// essere contato come «l'ho sentito».
async fn evento(risposta: &mut Response<Body>) -> Option<String> {
    loop {
        let frame = frame_entro(risposta, ATTESA).await?;
        if frame.contains(": keep-alive") {
            continue;
        }
        if nome_evento(&frame) == Some("ciao") {
            continue;
        }
        return Some(frame);
    }
}

#[tokio::test]
async fn un_sottoscrittore_del_corso_a_non_riceve_zero_eventi_del_corso_b() {
    let mut scuola = Scuola::nuova();
    let di_a = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let di_b = scuola.bozza(&scuola.altro_corso.clone(), &scuola.altro_docente.clone(), 1);

    let mut canale_a = apri(&scuola, scuola.corso.as_str(), &scuola.docente).await;
    let mut canale_b = apri(&scuola, scuola.altro_corso.as_str(), &scuola.altro_docente).await;
    assert_eq!(canale_a.status(), StatusCode::OK);
    assert_eq!(canale_b.status(), StatusCode::OK);
    assert_eq!(
        super::header_string(&canale_a, header::CONTENT_TYPE).as_deref(),
        Some("text/event-stream")
    );

    // Su B: ratifica e pubblicazione, l'atto più rumoroso che ci sia.
    let base_b = format!(
        "/api/v1/courses/{}/queue/{}",
        scuola.altro_corso.as_str(),
        di_b.id.as_str()
    );
    assert_eq!(
        scuola
            .post(
                &format!("{base_b}/ratify"),
                &scuola.altro_docente,
                json!({"note": "ok"})
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        scuola
            .post(&format!("{base_b}/publish"), &scuola.altro_docente, json!({}))
            .await
            .status(),
        StatusCode::OK
    );
    // Su A: lo stesso atto.
    let base_a = format!(
        "/api/v1/courses/{}/queue/{}",
        scuola.corso.as_str(),
        di_a.id.as_str()
    );
    assert_eq!(
        scuola
            .post(
                &format!("{base_a}/ratify"),
                &scuola.docente,
                json!({"note": "ok"})
            )
            .await
            .status(),
        StatusCode::OK
    );

    // Su A anche la pubblicazione, cosi' il canale di A ha **due** eventi
    // propri da mostrare: con uno solo, «non ha sentito B» si dimostrerebbe
    // anche con un canale che non trasmette niente.
    assert_eq!(
        scuola
            .post(&format!("{base_a}/publish"), &scuola.docente, json!({}))
            .await
            .status(),
        StatusCode::OK
    );

    // Il canale di B riceve i suoi due eventi. L'ordine fra i due canali di
    // una persona che insegna non è garantito — `select!` sceglie a caso fra i
    // rami pronti — quindi il test raccoglie e confronta, e non assume il primo.
    let eventi_b = eventi(&mut canale_b, 2).await;
    assert_eq!(eventi_b.len(), 2, "B: {eventi_b:?}");
    for e in &eventi_b {
        assert!(e.contains(di_b.id.as_str()), "B ha ricevuto {e}");
        assert!(!e.contains(di_a.id.as_str()), "B ha ricevuto {e}");
    }

    // Quello di A riceve i suoi due eventi e nessuno di B: il primo
    // asserimento è «sono due», il secondo è che nessuno dei due nomina B.
    let eventi_a = eventi(&mut canale_a, 2).await;
    assert_eq!(eventi_a.len(), 2, "A: {eventi_a:?}");
    for e in &eventi_a {
        assert!(e.contains(di_a.id.as_str()), "A ha ricevuto {e}");
        assert!(!e.contains(di_b.id.as_str()), "A ha ricevuto un evento di B: {e}");
    }

    // E dopo, più niente. A questo punto B ha pubblicato due volte e A una: se
    // il bus non fosse partizionato, qualcosa di B sarebbe ancora in coda e
    // arriverebbe adesso.
    assert!(
        frame_entro(&mut canale_a, 300).await.is_none(),
        "il canale di A ha ricevuto altro dopo i propri eventi"
    );
}

/// I prossimi `quanti` eventi veri di un canale.
async fn eventi(risposta: &mut Response<Body>, quanti: usize) -> Vec<String> {
    let mut raccolti = Vec::new();
    while raccolti.len() < quanti {
        match evento(risposta).await {
            Some(e) => raccolti.push(e),
            None => break,
        }
    }
    raccolti
}

#[tokio::test]
async fn uno_studente_non_apre_il_canale_di_un_corso_a_cui_non_appartiene() {
    let scuola = Scuola::nuova();
    let risposta = apri(&scuola, scuola.altro_corso.as_str(), &scuola.studente).await;
    assert_eq!(risposta.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn il_canale_senza_identita_dichiarata_risponde_401() {
    let scuola = Scuola::nuova();
    let richiesta = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/courses/{}/events", scuola.corso.as_str()))
        .body(Body::empty())
        .expect("richiesta");
    let risposta = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    assert_eq!(risposta.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn il_ciao_dichiara_da_dove_comincia_la_vista() {
    let scuola = Scuola::nuova();
    let mut canale = apri(&scuola, scuola.corso.as_str(), &scuola.studente).await;
    let primo = frame_entro(&mut canale, ATTESA).await.expect("nessun ciao");
    assert_eq!(nome_evento(&primo), Some("ciao"), "{primo}");
    assert!(primo.contains("\"da_evento\":1"), "{primo}");
    assert!(primo.contains(scuola.corso.as_str()), "{primo}");
    assert!(primo.contains(scuola.studente.as_str()), "{primo}");
}

#[tokio::test]
async fn il_predicato_filtra_anche_un_evento_del_proprio_corso() {
    // Terzo livello: stesso corso, canale gia` del corso, ma l'evento riguarda
    // una bozza che lo studente non puo` leggere. Il secondo livello — la
    // partizione per corso — non fermerebbe niente, perche' l'evento e' proprio
    // del suo corso. A fermarlo e' il predicato, applicato all'evento.
    //
    // L'evento e' pubblicato a mano sul bus perche' le rotte di questo server,
    // per costruzione, non pubblicano mai un evento di corso su un argomento
    // che un iscritto non puo' leggere: e' il caso che nessun atto produce, e
    // quindi nessun test attraverso le rotte lo raggiungerebbe.
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    let mut canale = apri(&scuola, scuola.corso.as_str(), &scuola.studente).await;
    let _ = frame_entro(&mut canale, ATTESA).await.expect("ciao");

    scuola.stato.bus.publish(
        &scuola.corso,
        crate::bus::Scope::Course,
        crate::bus::CourseEvent {
            id: scuola.stato.bus.next_id(),
            corso: scuola.corso.clone(),
            argomento: bozza.id.clone(),
            evento: "pubblicato",
            da: scuola.docente.clone(),
            quando: kbs_core::Millis(0),
        },
    );

    assert!(
        evento(&mut canale).await.is_none(),
        "uno studente ha ricevuto la notizia di una bozza che non puo` leggere"
    );
}
