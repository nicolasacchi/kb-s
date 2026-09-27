//! La coda di ratifica: che cosa ha generato il docente e che cosa non ha
//! ancora messo in uso.
//!
//! # Perché è una superficie di prodotto e non una pagina di amministrazione
//!
//! D4 dice che esistono due strade di scrittura, e quella condivisa passa da
//! una **ratifica**. Se la ratifica sta in una pagina di amministrazione, la
//! strada di pubblicazione è una strada che si percorre per abitudine, e
//! un cancello che si attraversa per abitudine è un cancello decorativo — la
//! cosa che D4 vieta per nome. Quindi la coda ha un'API, ha un codice di
//! stato, e ha una risposta che dice al docente che cosa manca.
//!
//! # Che cosa c'è nella coda, e che cosa non c'è
//!
//! **Solo argomenti in stato mutabile** — bozza e revisione del docente. Un
//! argomento in uso o archiviato non «aspetta» nessuno, e metterlo in coda
//! sarebbe dire al docente che ha ancora lavoro da fare quando il lavoro è
//! finito. La coda è la lista del lavoro **non finito**, e la sua verità è
//! `PublicationState::is_mutable` di `kbs-core`, non una condizione scritta qui.
//!
//! Ogni voce dice **che cosa** manca, perché una coda che dice solo «da
//! guardare» fa perdere il tempo che D9 vuole non perdere:
//!
//! * `needs_ratification` — non c'è ratifica, o c'è ma è invecchiata rispetto
//!   all'hash di contenuto corrente. Il secondo caso è quello che D6 rende
//!   verificabile: una ratifica vale per un testo, e se il testo è cambiato la
//!   ratifica riguarda qualcosa che non esiste più;
//! * `needs_publication` — la ratifica c'è ed è valida, ma lo stato non è
//!   `in-corso`. Sono due gesti distinti e D4 li tiene distinti: la ratifica
//!   dichiara che il docente ha verificato, la pubblicazione dichiara che si
//!   usa.
//!
//! # Chi ci entra
//!
//! Chi insegna il corso. Non «un ruolo»: la relazione `teaches` di D5. Chi non
//! la ha riceve la risposta di «assente», che è la stessa di un corso che non
//! esiste — vedi [`crate::capability`].

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use kbs_core::{Argument, CourseId, PublicationState, Ratification, Relation};

use crate::bus::{CourseEvent, EventKind, Scope};
use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// `GET /api/v1/courses/{course}/queue`
///
/// `200 {"course": "…", "queue": [...]}` · `404` se non `teaches` il corso.
///
/// La coda è **ordinata**: prima ciò che è solo bozza, poi ciò che è
/// revisione del docente, e dentro ogni gruppo per id. L'ordine è
/// dichiarato e non è quello del database, perché una coda che cambia ordine
/// fra due caricamenti della stessa pagina fa pensare che sia cambiato
/// qualcosa.
pub async fn queue(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
) -> Result<Json<QueueResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let voci = state.db.read_api(|store| {
        capability::require(store, &persona, &course, Some(Relation::Teaches))?;
        // `visible_arguments` senza filtro di stato, poi il filtro dei soli
        // stati mutabili: il predicato viene applicato una volta sola, e la
        // coda non può quindi contenere niente che il docente non veda.
        let candidati = store.visible_arguments(&persona, &course, None)?;
        let mut voci: Vec<QueueEntry> = candidati
            .into_iter()
            .filter(|a| a.state.is_mutable())
            .map(QueueEntry::from_argument)
            .collect();
        voci.sort_by(|a, b| {
            ordine_stato(a.argument.state)
                .cmp(&ordine_stato(b.argument.state))
                .then_with(|| a.argument.id.cmp(&b.argument.id))
        });
        Ok(voci)
    })?;
    Ok(Json(QueueResponse {
        course,
        queue: voci,
    }))
}

/// L'ordine della coda: la bozza è ciò che il docente non ha ancora guardato, la
/// revisione è ciò che ha guardato e non ha ancora ratificato. Sono due lavori
/// diversi e vanno in due gruppi diversi.
///
/// La funzione è `total` perché `sort_by` lo richiede e i due stati hanno la
/// stessa forma: se un domani ci fosse un terzo stato mutabile, il `2` qui
/// direbbe ancora «dopo tutto il resto», che è la cosa giusta.
fn ordine_stato(stato: PublicationState) -> u8 {
    match stato {
        PublicationState::Bozza => 0,
        PublicationState::DelDocente => 1,
        // Non dovrebbero arrivare qui: `is_mutable` li esclude. Restano
        // comunque ordinati per bene, perché una funzione che panica su un
        // valore possibile è una funzione che aspetta un bug.
        _ => 2,
    }
}

#[derive(Debug, Serialize)]
pub struct QueueResponse {
    /// Il corso di cui è la coda.
    pub course: CourseId,
    /// Che cosa aspetta una decisione. Vuota se non aspetta niente.
    pub queue: Vec<QueueEntry>,
}

/// Una voce di coda.
///
/// `argument` è l'argomento per intero, con la sua provenienza: chi ha scritto
/// cosa, con quale modello e con quale hash di corpus (D10). Una coda che
/// mostrasse solo il titolo non permetterebbe al docente di sapere se sta
/// ratificando roba sua o roba generata, e sono due atti diversi.
#[derive(Debug, Serialize)]
pub struct QueueEntry {
    /// L'argomento.
    pub argument: Argument,
    /// Manca una ratifica, o quella che c'è non vale più per il contenuto
    /// corrente.
    pub needs_ratification: bool,
    /// La ratifica c'è ed è valida, ma l'argomento non è in uso.
    pub needs_publication: bool,
}

impl QueueEntry {
    /// Una voce dalla riga di coda dell'argomento.
    ///
    /// `needs_ratification` è `!ratificato_valido` e n'altro: la validità è
    /// quella di `kbs_core::check_citable`, quindi la stessa che governa
    /// `publish` e l'esportazione. Se qui ci fosse un secondo criterio, la
    /// coda direbbe una cosa e la porta direzione un'altra.
    pub fn from_argument(argument: Argument) -> QueueEntry {
        let ratifica_valida = argument
            .ratified
            .as_ref()
            .is_some_and(|r| r.contract_hash == argument.content_hash);
        QueueEntry {
            needs_ratification: !ratifica_valida,
            needs_publication: ratifica_valida,
            argument,
        }
    }
}

/// Il corpo di [`ratify`].
///
/// La nota non è obbligatoria per il tipo e **non è accettata se vuota**: una
/// ratifica senza nota è una responsabilità senza soggetto, e il modulo che
/// verifica la ratifica non ha modo di sapere che cosa era stato verificato.
#[derive(Debug, Deserialize)]
pub struct RatifyBody {
    /// Che cosa ha verificato il docente. Una frase va bene: l'obiettivo non è
    /// un audit, è la responsabilità di chi ha ratificato.
    #[serde(default)]
    pub note: String,
}

/// `POST /api/v1/courses/{course}/queue/{id}/ratify`
///
/// `200 {"ratification": {…}}` · `400` nota vuota · `404` · `409`.
///
/// Il `200` è la ratifica **installata**, con l'hash di contenuto su cui vale.
/// Non si può chiedere di ratificare un altro hash: `kbs-store::ratify` ratifica
/// il contenuto che c'è adesso, e se l'hash fosse un parametro un chiamante
/// potrebbe ratificare un hash che non corrisponde a nulla.
///
/// Chi ratifica deve insegnare il corso **o** essere l'autore dell'argomento: le
/// due relazioni sono quelle che D5 dichiara (`teaches` e `author_of`), e la
/// seconda serve perché il docente che ha scritto un materiale a mano non
/// debba aprire un corso per poterlo dichiarare verificato.
pub async fn ratify(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path((course, id)): Path<(String, String)>,
    Json(corpo): Json<RatifyBody>,
) -> Result<Json<RatificationResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    if corpo.note.trim().is_empty() {
        return Err(ApiError::BadRequest {
            motivo: "una ratifica senza nota è una responsabilità senza soggetto".into(),
        });
    }
    let persona = identita.person().clone();

    let ratifica = state.db.write_api(|store| {
        // L'argomento deve esistere ed essere visibile a chi ratifica: senza
        // questo controllo, la coda e la ratifica avrebbero predicati diversi.
        let argomento = capability::read(store, &persona, &id)?;
        if argomento.course != course {
            // Il percorso e l'oggetto devono dire la stessa cosa. Se non la
            // dicono, uno dei due è sbagliato e la risposta è «assente»: quale
            // dei due sia sbagliato non lo sa nessuno, e non deve saperlo
            // nessuno fuori di qui.
            return Err(ApiError::Absent);
        }
        let relazioni = capability::require(store, &persona, &course, None)?;
        if !relazioni.contains(&Relation::Teaches) && !è_autore(&argomento, &persona) {
            return Err(ApiError::Absent);
        }
        Ok(store.ratify(&id, &persona, corpo.note.trim())?)
    })?;

    state.bus.publish(
        &course,
        Scope::Staff,
        CourseEvent {
            id: state.bus.next_id(),
            corso: course.clone(),
            argomento: id,
            evento: EventKind::Ratified.as_str(),
            da: persona,
            quando: ratifica.at,
        },
    );

    Ok(Json(RatificationResponse { ratification: ratifica }))
}

/// L'argomento è di questa persona?
///
/// `Origin` è un enum e solo due varianti hanno un `by`: un argomento derivato
/// risale alla sua fonte, e risalire è la cosa giusta da fare perché
/// `kbs-store` fa la stessa cosa per il predicato (`attributed_authors`). Se qui
/// si facesse diversamente, l'autore della coda e l'autore del predicato
/// sarebbero due persone diverse, e la coda mostrerebbe a qualcuno un
/// argomento che il predicato gli nasconderebbe.
fn è_autore(argomento: &Argument, persona: &kbs_core::PersonId) -> bool {
    match &argomento.origin {
        kbs_core::Origin::Human { by, .. } | kbs_core::Origin::Generated { by, .. } => by == persona,
        // Derivato: la fonte non è un campo dell'argomento, e inventare un
        // autore qui significherebbe che l'autore della coda e quello del
        // predicato non coincidono. Chi non insegna il corso non ratifica un
        // derivato: può farlo il docente del corso, che è il caso normale.
        kbs_core::Origin::Derived { .. } => false,
    }
}

#[derive(Debug, Serialize)]
pub struct RatificationResponse {
    /// La ratifica installata, con `by`, `at`, `contract_hash` e `note`.
    pub ratification: Ratification,
}

/// `POST /api/v1/courses/{course}/queue/{id}/publish`
///
/// `200 {"state": "in-corso"}` · `404` · **`409` se non c'è una ratifica valida**.
///
/// Il `409` è il cuore di D4 reso visibile dal protocollo: senza ratifica — o
/// con una ratifica invecchiata rispetto al contenuto corrente — la risposta è
/// un conflitto che **nomina la regola**, non un `200` e non un `500`. Un item
/// non verificato è leggibile, ma non è citabile, e questa è la rotta in cui
/// quella frase diventa un codice di stato.
///
/// Chi pubblica deve insegnare il corso o esserne l'autore, come per la
/// ratifica: pubblicare è la seconda metà dello stesso atto.
pub async fn publish(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path((course, id)): Path<(String, String)>,
) -> Result<Json<PublishResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();

    let pubblicato = state.db.write_api(|store| {
        let argomento = capability::read(store, &persona, &id)?;
        if argomento.course != course {
            return Err(ApiError::Absent);
        }
        let relazioni = capability::require(store, &persona, &course, None)?;
        if !relazioni.contains(&Relation::Teaches) && !è_autore(&argomento, &persona) {
            return Err(ApiError::Absent);
        }
        // `publish` controlla la precondizione *prima* dello stato, anche
        // quando l'argomento è già in uso: «adesso è citabile» è la domanda,
        // e un argomento in uso con ratifica invecchiata non lo è.
        Ok(store.publish(&id)?)
    })?;

    state.bus.publish(
        &course,
        Scope::Course,
        CourseEvent {
            id: state.bus.next_id(),
            corso: course.clone(),
            argomento: id,
            evento: EventKind::Published.as_str(),
            da: persona,
            quando: kbs_core::Millis::now(),
        },
    );

    Ok(Json(PublishResponse {
        state: pubblicato,
    }))
}

#[derive(Debug, Serialize)]
pub struct PublishResponse {
    /// Lo stato dopo la pubblicazione. `in-corso` quando la porta si apre;
    /// da `archiviato` `publish` risponde `409`, perché da lì non si torna.
    pub state: PublicationState,
}

/// `POST /api/v1/courses/{course}/queue/{id}/withdraw`
///
/// `204` · `404` se non c'è una ratifica da ritirare.
///
/// Ritirare è l'unico modo in cui un argomento in uso torna non citabile senza
/// che nessuno ne modifichi il testo, quindi è dichiarato come una rotta e non
/// come un campo che si azzera. Chi ritira è chi insegna il corso **o** chi ha
/// ratificato: la relazione `ratified` di D5 dice esattamente questo, e senza di
/// lei un docente che cede il corso non potrebbe ritirare la propria ratifica.
pub async fn withdraw(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path((course, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let course_clonata = course.clone();

    let quando = state.db.write_api(|store| {
        let argomento = capability::read(store, &persona, &id)?;
        if argomento.course != course {
            return Err(ApiError::Absent);
        }
        let relazioni = capability::require(store, &persona, &course, None)?;
        let ratificatore = argomento
            .ratified
            .as_ref()
            .is_some_and(|r| r.by == persona);
        if !relazioni.contains(&Relation::Teaches) && !ratificatore {
            return Err(ApiError::Absent);
        }
        // `withdraw_ratification` risponde `NotFound` se non c'era una
        // ratifica. A questo punto la visibilità e la relazione sono già
        // verificate, quindi quell'assenza non è un canale: è un conflitto di
        // stato. È l'unico punto in cui «non esiste» può diventare `409` senza
        // mentire — altrove [`crate::error`] lo tiene a `404`.
        match store.withdraw_ratification(&id) {
            Ok(()) => Ok(kbs_core::Millis::now()),
            Err(kbs_store::Error::NotFound { .. }) => Err(ApiError::Conflict {
                regola: "D4",
                motivo: "non c'è una ratifica da ritirare".into(),
            }),
            Err(altro) => Err(altro.into()),
        }
    })?;

    state.bus.publish(
        &course_clonata,
        Scope::Staff,
        CourseEvent {
            id: state.bus.next_id(),
            corso: course,
            argomento: id,
            evento: EventKind::RatificationWithdrawn.as_str(),
            da: persona,
            quando,
        },
    );

    Ok(StatusCode::NO_CONTENT)
}
