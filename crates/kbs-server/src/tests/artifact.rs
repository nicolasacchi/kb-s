//! L'artifact sulla sua origine, e il runtime vendorizzato che ci carica.
//!
//! Qui si prova la cosa che rende sicuro `allow-same-origin`: **l'origine
//! cambia, il flag no**. Un test che controllasse solo i flag passerebbe anche
//! se il suffisso degli host sparisse, e con il suffisso sparito
//! `allow-same-origin` sarebbe un buco.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::util::ServiceExt;

use kbs_core::{Argument, CourseId, Millis, Origin, PersonId, PublicationState};

use super::Scuola;

/// Una pagina di artifact che carica il runtime vendorizzato, come farà
/// l'interfaccia.
///
/// Il riferimento è a `/three/…` e non a un CDN: è la regola D15, ed è
/// `kbs_doc` a decidere che quel percorso è dentro il perimetro. Il test sotto
/// verifica che `kbs_doc` lo accetti e che il server lo serva.
pub const PAGINA: &str = r#"<!doctype html>
<html lang="it">
<head>
<meta charset="utf-8">
<title>Lezione 01</title>
<script type="module" src="/three/three.module.min.js"></script>
</head>
<body><h1 id="lezione-01">Lezione 01</h1></body>
</html>
"#;

#[tokio::test]
async fn un_artifact_si_serve_sulla_sua_origine_e_solo_a_chi_lo_puo_leggere() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.scrivi_file(&argomento, PAGINA);
    let host = scuola.host_artifact(&argomento);
    assert!(
        host.ends_with(".artifacts.localhost"),
        "l'host non ha il suffisso: {host}"
    );

    let risposta = scuola.get_host(&host, "/", &scuola.studente).await;
    assert_eq!(risposta.status(), StatusCode::OK);
    assert_eq!(
        super::header_string(&risposta, header::CONTENT_TYPE).as_deref(),
        Some("text/html; charset=utf-8")
    );
    // Gli header si leggono prima di consumare il corpo.
    let cache = super::header_string(&risposta, header::CACHE_CONTROL);
    let referrer = super::header_string(&risposta, header::REFERRER_POLICY);
    let corpo = Scuola::testo(risposta).await;
    assert!(corpo.contains("three.module.min.js"), "{corpo}");
    // L'artifact non entra in nessuna cache: la sua visibilità dipende dalla
    // dichiarazione di identità che viaggia nella query string.
    assert_eq!(cache.as_deref(), Some("private, no-store"));
    assert_eq!(referrer.as_deref(), Some("no-referrer"));
}

#[tokio::test]
async fn la_bozza_non_si_serve_come_file_e_il_rifiuto_e_lo_stesso_di_un_id_inesistente() {
    let mut scuola = Scuola::nuova();
    let bozza = scuola.bozza(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.scrivi_file(&bozza, PAGINA);
    let host = scuola.host_artifact(&bozza);

    let rifiutata = scuola.get_host(&host, "/", &scuola.studente).await;
    assert_eq!(
        rifiutata.status(),
        StatusCode::NOT_FOUND,
        "una bozza è stata servita come file statico: D5 annullato da una riga di routing"
    );

    // Lo stesso studente, su un id che non esiste: la stessa risposta.
    let inesistente = scuola
        .get_host(
            &format!("{}.artifacts.localhost", super::id_inesistente().as_str()),
            "/",
            &scuola.studente,
        )
        .await;
    assert_eq!(super::scheletro(&rifiutata), super::scheletro(&inesistente));
    assert_eq!(Scuola::corpo(rifiutata).await, Scuola::corpo(inesistente).await);
}

#[tokio::test]
async fn l_artifact_richiede_che_qualcuno_dichiari_chi_e() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.scrivi_file(&argomento, PAGINA);
    let host = scuola.host_artifact(&argomento);

    // Un `<iframe src>` non può allegare un header, quindi la dichiarazione
    // arriva nella query string: è il modo normale su quell'origine, non un
    // ripiego. Vedi `crate::identity` perché una dichiarazione in una query
    // string non è un incidente in un sistema che non ha autenticazione.
    let richiesta = Request::builder()
        .method("GET")
        .uri("/?person=person_0003")
        .header(header::HOST, &host)
        .body(Body::empty())
        .expect("richiesta");
    let risposta = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    assert_eq!(risposta.status(), StatusCode::OK, "la query string non è accettata");

    // Senza dichiarazione, nessuna risposta.
    let richiesta = Request::builder()
        .method("GET")
        .uri("/")
        .header(header::HOST, &host)
        .body(Body::empty())
        .expect("richiesta");
    let risposta = scuola.app.clone().oneshot(richiesta).await.expect("risposta");
    assert_eq!(risposta.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn l_host_del_padre_non_serve_artifact() {
    let mut scuola = Scuola::nuova();
    let argomento = scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    scuola.scrivi_file(&argomento, PAGINA);

    // Sull'host dell'interfaccia lo stesso percorso non serve l'artifact: è
    // l'interfaccia, o il suo 404. È la metà della promessa «origini diverse»
    // che il test delle origini non copre da solo.
    let risposta = scuola
        .get_host("scuola.example.it", "/", &scuola.docente)
        .await;
    assert_ne!(
        risposta.status(),
        StatusCode::OK,
        "l'artifact è servito anche sull'origine dell'interfaccia"
    );
}

/// Un argomento il cui `rel_path` è una **cartella**: è la forma di un artifact
/// multi-pagina, e l'unica in cui ha figli.
fn argumento_a_cartella() -> Argument {
    Argument {
        id: kbs_core::ArgumentId::from_rel_path("corsi/course_0001/lezione-01"),
        title: "Lezione 01".to_string(),
        summary: "Un artifact con figli.".to_string(),
        state: PublicationState::Bozza,
        course: CourseId::fixture(1),
        prerequisites: Vec::new(),
        origin: Origin::Human {
            by: PersonId::fixture(1),
            at: Millis(0),
        },
        rel_path: Some("corsi/course_0001/lezione-01".to_string()),
        content_hash: "sha256:cartella".to_string(),
        created_at: Millis(0),
        updated_at: Millis(0),
        // La ratifica **non** si scrive insieme all'argomento: `upsert_argument`
        // la rifiuta, perché una precondizione di `publish` installabile da due
        // posti è una precondizione decorativa (D4). Va da `ratify`.
        ratified: None,
    }
}

#[tokio::test]
async fn un_figlio_sta_sotto_l_argomento_e_una_sorella_no() {
    let scuola = Scuola::nuova();
    let argomento = argumento_a_cartella();
    scuola
        .db
        .write(|store| {
            store.upsert_argument(&argomento)?;
            let _ = store.ratify(&argomento.id, &PersonId::fixture(1), "verificato");
            Ok(store.publish(&argomento.id)?)
        })
        .expect("pubblicazione");

    let dir = scuola.corpus.join("corsi/course_0001/lezione-01");
    std::fs::create_dir_all(&dir).expect("cartella");
    std::fs::write(dir.join("stile.css"), "body{color:#000}").expect("stile");
    // La sorella: un file accanto, che è una bozza di un altro argomento.
    std::fs::write(
        scuola.corpus.join("corsi/course_0001/lezione-01.html"),
        "<!doctype html><title>segreto</title>",
    )
    .expect("sorella");

    let host = scuola.host_artifact(&argomento);
    // Il figlio dentro il percorso dell'argomento: sì.
    let risposta = scuola
        .get_host(&host, "/stile.css", &scuola.studente)
        .await;
    assert_eq!(risposta.status(), StatusCode::OK);
    assert_eq!(
        super::header_string(&risposta, header::CONTENT_TYPE).as_deref(),
        Some("text/css; charset=utf-8")
    );

    // La sorella, per traversata: no. È il caso che riaprirebbe una bozza a chi
    // legge un artifact, e l'unica cosa che lo chiude è che il percorso del
    // figlio sta sotto l'argomento e non accanto.
    for percorso in [
        "/../lezione-01.html",
        "/..%2Flezione-01.html",
        "/%2e%2e/lezione-01.html",
    ] {
        let risposta = scuola.get_host(&host, percorso, &scuola.studente).await;
        assert_eq!(
            risposta.status(),
            StatusCode::NOT_FOUND,
            "`{percorso}` ha raggiunto la sorella"
        );
    }
}
