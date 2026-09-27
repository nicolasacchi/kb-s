//! Il flusso SSE, che è il bus partizionato visto da una richiesta.
//!
//! # L'ordine delle tre cose, che è l'ordine giusto
//!
//! 1. **Si dichiara l'identità.** Senza, `401`. È la prima risposta utile
//!    perché riguarda la forma della richiesta e non l'oggetto.
//! 2. **Si chiede la relazione col corso.** Senza, `404`. Un canale eventi
//!    aperto da chi non ha relazione col corso non è un canale da negare: è
//!    un canale che non esiste, e la risposta è quella di un corso che non
//!    esiste.
//! 3. **Poi si filtra, per ogni evento, con il predicato.** Non prima: il
//!    predicato è per argomento, e l'argomento arriva con l'evento. Quindi il
//!    canale di un corso trasporta anche eventi che questa persona non deve
//!    vedere — un argomento in bozza ratificato mentre lei è iscritta — e il
//!    filtro accade lì, sul singolo evento, chiamando
//!    [`kbs_store::Store::read_argument`] con l'identità di **quel**
//!    sottoscrittore.
//!
//! Il punto 3 è l'unico in cui questo crate ha un predicato, ed è una
//! **delegazione**, non una copia: `may_read` non è chiamato qui dentro.
//!
//! # I canali che si aprono, e chi li sceglie
//!
//! Chi insegna il corso riceve dai due canali del corso: quello del corso e
//! quello del corpo docente. Chi è solo iscritto riceve dal primo e **non sa
//! che il secondo esiste** — non «non riceve», non «è filtrato»: il canale non
//! gli è stato dato, e la pubblicazione su un canale che nessuno ascolta non
//! lascia traccia. Per questo la scelta dell'ambito è fatta dal **server**, in
//! base alle relazioni, e non arriva dalla query string: un `?scope=staff`
//! sarebbe una richiesta di privilegio che il predicato soddisferebbe solo per
//! caso.
//!
//! # Nessun ripristino
//!
//! `Last-Event-ID` non viene onorato. Il primo evento è `ciao`, con il
//! contatore del bus al momento della sottoscrizione, così il client sa da dove
//! comincia la sua vista. Non è un resume: è una dichiarazione. Il bus non ha
//! un log — non può averne uno senza diventare una seconda copia della verità —
//! e dichiararlo all'inizio è l'unico modo perché un client che rientra sappia
//! che deve andare a leggere la coda e non a fidarsi dello stream.

use std::convert::Infallible;
use std::pin::Pin;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use futures_core::Stream;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;

use kbs_core::{PersonId, Relation};

use crate::bus::{CourseEvent, Scope};
use crate::capability;
use crate::db::Db;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// Il tipo di stream che `Sse::keep_alive` accetta.
///
/// È il tipo **vero**, non `Sse<…>`: `keep_alive` avvolge lo stream in un
/// `KeepAliveStream`, e scrivere la firma come se non lo facesse è il modo più
/// rapido di litigare con il compilatore. Il boxing c'è perché lo stream nasce
/// da `async_stream!` e il suo tipo non si può scrivere: il costo è una
/// allocazione per sottoscrittore, all'apertura del canale.
pub type Flusso = KeepAliveStream<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>;

/// Ogni quanti secondi si manda un commento di tenuta.
///
/// Il commento tiene viva la connessione attraverso i proxy che chiudono le
/// connessioni inattive. Non è un evento: non ha `event:`, e un client che
/// conta gli eventi non lo conta.
pub const KEEP_ALIVE: Duration = Duration::from_secs(30);

/// Quanti eventi persi prima di chiudere il canale.
///
/// Un canale che continua a funzionare dopo aver perso eventi è un canale che
/// mente: il client non sa che gli manca qualcosa. Dopo quattro persi non si sta
/// più «recuperando», si sta facendo finta, e la cosa onesta è chiudere e
/// lasciare che il client si riaccoda.
pub const LAG_DITTO: u64 = 4;

/// `GET /api/v1/courses/{course}/events`
///
/// `200 text/event-stream` · `401` senza identità dichiarata · `404` senza
/// relazione col corso.
///
/// Appena aperto il flusso manda `ciao`:
///
/// ```text
/// event: ciao
/// data: {"corso":"course_0001","da_evento":41,"persona":"person_0001"}
/// ```
///
/// `da_evento` è il contatore del bus al momento della sottoscrizione: da lì in
/// poi gli id crescono, e il client sa che **prima** di quel numero non ha
/// ricevuto niente. Non è un resume, è una dichiarazione di dove comincia la
/// sua vista. Gli eventi successivi sono `ratificato`, `pubblicato`,
/// `ratifica-ritirata` e `lag`.
pub async fn stream(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
) -> Result<Sse<Flusso>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let bus = state.bus.clone();
    let db = state.db.clone();

    let insegna = state.db.read_api(|store| {
        let relazioni = capability::require(store, &persona, &course, None)?;
        Ok(relazioni.contains(&Relation::Teaches))
    })?;

    // I canali sono scelti dal server e consegnati al flusso già decisi. Uno
    // studente non riceve il canale del corpo docente: non lo riceve *come
    // filtro*, non lo riceve.
    let mut del_corso = Some(bus.subscribe(&course, Scope::Course));
    let mut del_corpo = insegna.then(|| bus.subscribe(&course, Scope::Staff));
    let contatore = bus.contatore();

    let flusso = async_stream::stream! {
        // L'annotazione del primo `yield` fissa il tipo di errore del flusso:
        // senza, ogni altro `yield` lo deduce da sé e la firma non torna.
        yield Ok::<_, Infallible>(Event::default().event("ciao").data(
            serde_json::json!({
                "corso": course.as_str(),
                "da_evento": contatore,
                "persona": persona.as_str(),
            })
            .to_string(),
        ));

        let mut persi: u64 = 0;
        loop {
            let ricevuto = tokio::select! {
                r = da(&mut del_corso) => r,
                r = da(&mut del_corpo) => r,
            };
            let evento = match ricevuto {
                Ricevuto::Evento(e) => e,
                Ricevuto::Persi(quanti) => {
                    persi += quanti;
                    let evento = Event::default()
                        .event("lag")
                        .data(
                            serde_json::json!({
                                "motivo": "il canale è indietro: la coda va riletta",
                                "persi": persi,
                            })
                            .to_string(),
                        );
                    yield Ok(evento);
                    if persi >= LAG_DITTO as u64 {
                        break;
                    }
                    continue;
                }
                // Tutti i canali sono chiusi: il bus è stato droppato, e con
                // lui finisce il processo. Non c'è niente da recuperare.
                Ricevuto::Chiuso => break,
            };

            // Il filtro finale, per persona. Non è `may_read` qui: è
            // `read_argument`, che è `may_read` applicato da `kbs-store` a
            // quell'argomento e con quell'identità.
            if !visibile(&db, &persona, &evento) {
                continue;
            }
            yield Ok(Event::default()
                .event(evento.evento)
                .id(evento.id.to_string())
                .data(
                    serde_json::json!({
                        "argomento": evento.argomento.as_str(),
                        "corso": evento.corso.as_str(),
                        "da": evento.da.as_str(),
                        "id": evento.id,
                        "quando": evento.quando.0,
                    })
                    .to_string(),
                ));
        }
    };

    // Il boxing va dichiarato: senza il tipo di arrivo esplicito il
    // compilatore deduce `Box<AsyncStream<…>>` e la coercizione al tratto
    // oggetto non avviene da sola.
    let flusso: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> = Box::pin(flusso);
    Ok(Sse::new(flusso).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)))
}

/// Cosa è arrivato da un canale.
enum Ricevuto {
    Evento(CourseEvent),
    Persi(u64),
    Chiuso,
}

/// `recv` su un canale che potrebbe non esistere.
///
/// Il canale assente è un future che non finisce mai: è il modo idiomatico di
/// dire «questo ramo non esiste» in un `select!`, ed evita di dover scrivere
/// due varianti del ciclo per docente e per studente.
async fn da(canale: &mut Option<Receiver<CourseEvent>>) -> Ricevuto {
    match canale {
        Some(rx) => match rx.recv().await {
            Ok(evento) => Ricevuto::Evento(evento),
            Err(RecvError::Lagged(quanti)) => Ricevuto::Persi(quanti),
            Err(RecvError::Closed) => {
                *canale = None;
                Ricevuto::Chiuso
            }
        },
        None => std::future::pending().await,
    }
}

/// Questo evento riguarda qualcosa che `persona` può leggere?
///
/// Una sola lettura per evento, e passa dal predicato. Un evento di un corso
/// che la persona non conosce non arriva qui — il canale è partizionato — ma un
/// evento **del** suo corso su un argomento che non può leggere sì, e quello è
/// il caso che il filtro risolve.
fn visibile(db: &Db, persona: &PersonId, evento: &CourseEvent) -> bool {
    db.read(|store| Ok(store.read_argument(persona, &evento.argomento).is_ok()))
        .unwrap_or(false)
}

