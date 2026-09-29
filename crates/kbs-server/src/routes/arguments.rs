//! La lettura: argomenti, elenchi, claim — **e il testo del file**.
//!
//! Tre rotte e una sola cosa in comune: **nessuna di esse riapplica il
//! predicato**. Chiamano i metodi gated di `kbs-store` e, quando ricevono
//! `NotReadable`, la risposta che ne esce è la risposta di «assente» — la stessa
//! che danno a un id che non è mai esistito. Vedi [`crate::error`].
//!
//! # Perché il testo viaggia nella risposta e non in una porta accanto
//!
//! Un argomento che mostra solo i suoi metadati non è un argomento: la
//! provenienza, la ratifica e lo stato si possono leggere in un pomeriggio,
//! il materiale no. Quindi [`read`] porta anche il **testo del file**, letto dal
//! corpus e risolto con [`sandbox::resolve_file`] — le stesse tre regole che
//! usa [`crate::routes::artifact`], perché è la stessa operazione.
//!
//! Le strade che non sono stata scelte, e perché:
//!
//! * **un `iframe` verso `artifact::dispatch`** servirebbe i byte giusti senza
//!   codice nuovo, ma chiede all'interfaccia di sapere il
//!   `artifact_host_suffix` del deployment, e quel valore oggi non esce da
//!   nessuna parte: l'unica maniera di averlo sarebbe scriverlo in `web/`,
//!   dove diventerebbe una seconda copia di un dato di configurazione. Un
//!   `iframe` con il suffisso sbagliato non è un `iframe` che non si vede: è
//!   una pagina che dice «non c'è niente» dove il materiale c'è;
//! * **una rotta nuova che serva il file sotto l'origine principale** metterebbe
//!   HTML non fidato sull'origine dell'interfaccia, che è esattamente ciò che
//!   [`crate::sandbox`] esiste per impedire. Senza `allow-same-origin`
//! l'artifact perderebbe `localStorage` e sarebbe mezzo morto; con
//!   `allow-same-origin` perderebbe l'isolamento.
//!
//! Quindi il testo viaggia **come dato**, in un JSON che il browser non
//! interpreta: nessun script del materiale gira, e la pagina lo mette nel DOM
//! con un nodo di testo (`web/lib/dom.js`). Che cosa resta fuori è detto:
//! qui c'è il **file d'ingresso**, non i suoi figli, e non c'è il rendering
//! dell'artifact — quello vive sull'origine per artifact, e questa pagina non
//! la costruisce.
//!
//! E la difesa di tutto questo è la stessa delle claim: il testo è appeso a un
//! argomento che [`capability::read`] ha già dichiarato leggibile, quindi non
//! apre un canale — non c'è una seconda risposta in cui il predicato non
//! passa.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use kbs_core::{Argument, Claim};

use crate::capability;
use crate::config::ServerConfig;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;
use crate::sandbox::{self, ArtifactResolution};

/// `GET /api/v1/courses/{course}/arguments`
///
/// `200 {"arguments": [...], "course": "course_0001"}` — **anche vuota**.
///
/// L'elenco di un corso di cui la persona non ha relazioni è `200` con un
/// elenco vuoto, non `404`. Un `404` qui distinguerebbe «il corso non esiste»
/// da «non ci sei dentro», ed è il canale che D5 vieta: la risposta è la stessa
/// che si ottiene da un corso inesistente, che è il punto.
///
/// Lo stato (`?state=`) restringe un insieme che il predicato ha già
/// autorizzato, quindi non può riportare indietro le bozze del docente a uno
/// studente che le chiede esplicitamente.
pub async fn list(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
    axum::extract::Query(filtro): axum::extract::Query<ListQuery>,
) -> Result<Json<ArgumentsResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person();
    let argomenti = state.db.read_api(|store| {
        capability::require(store, persona, &course, None)?;
        Ok(store.visible_arguments(persona, &course, filtro.state)?)
    })?;
    Ok(Json(ArgumentsResponse {
        course: course.clone(),
        arguments: argomenti,
    }))
}

/// Il filtro opzionale della lista.
///
/// `state` è un enum di `kbs-core` deserializzato dal suo nome kebab-case
/// (`bozza`, `del-docente`, `in-corso`, `archiviato`): non è una stringa libera,
/// perché una stringa libera che non combacia con gli stati del dominio
/// produrrebbe un elenco vuoto che sembrerebbe una risposta vera.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListQuery {
    /// Limita a uno stato di pubblicazione.
    pub state: Option<kbs_core::PublicationState>,
}

#[derive(Debug, Serialize)]
pub struct ArgumentsResponse {
    /// Il corso di cui è l'elenco.
    pub course: kbs_core::CourseId,
    /// Ciò che la persona dichiarata può vedere. Può essere vuoto.
    pub arguments: Vec<Argument>,
}

/// `GET /api/v1/arguments/{id}`
///
/// `200 {"argument": {…}, "testo": {…}}` · `404`.
///
/// Il `404` è la stessa risposta di un id mai esistito, e la stessa di un id che
/// la persona non può leggere. L'id di un argomento è `arg_<hash del percorso>`
/// ed è enumerabile da chiunque abbia il corpus, quindi ogni differenza qui
/// sarebbe un canale per imparare che cosa c'è in un corso.
///
/// `testo` è il **file d'ingresso** dell'argomento, letto dal corpus dopo che il
/// predicato ha deciso. Non è un campo nuovo con un permesso nuovo: è
/// l'argomento che [`capability::read`] ha appena dichiarato leggibile, e il
/// testo è suo. Perché viaggi qui e non in una porta accanto, e che cosa di
/// conseguenza non fa, è nel doc del modulo.
pub async fn read(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(id): Path<String>,
) -> Result<Json<ArgumentResponse>, ApiError> {
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let argomento = state
        .db
        .read_api(|store| capability::read(store, identita.person(), &id))?;
    // Il file si legge **fuori** dalla transazione: la decisione è già presa, e
    // tenere aperta la lettura del database per il tempo di una `read` su disco
    // è un lock che si paga per ogni argomento che la pagina apre.
    let testo = testo_del_file(&argomento, &state.config);
    Ok(Json(ArgumentResponse {
        argument: argomento,
        testo,
    }))
}

#[derive(Debug, Serialize)]
pub struct ArgumentResponse {
    /// L'argomento, con la sua provenienza e la sua ratifica.
    pub argument: Argument,
    /// Il testo del file, o il motivo per cui qui non c'è.
    pub testo: Testo,
}

/// Quanto è grande il file di un argomento per viaggiare **dentro** la risposta.
///
/// [`crate::routes::artifact::MAX_ARTIFACT`] è 32 MiB ed è il tetto di un
/// artifact servito come documento. Qui il tetto è più basso e per una ragione
/// diversa: `web/pagine/lettore.js` chiama questa rotta una volta per
/// l'argomento e **una per ogni prerequisito**, quindi 32 MiB per file
/// significherebbero che una lezione con dieci prerequisiti grossi scarica
/// 320 MiB per disegnare una tabella. Un MiB sta sopra il testo di un artifact
/// normale e sotto la dimensione in cui il peso si sente.
pub const MAX_TESTO: usize = 1024 * 1024;

/// Il testo del file d'ingresso, o il motivo per cui qui non c'è.
///
/// La forma è un `enum` dichiarato e non un `Option`: `null` e `""` si
/// somigliano, e un lettore che li confondesse direbbe «non c'è niente» dove
/// la verità è «non posso mostrarlo». Ogni variante dice una cosa diversa, e
/// sono cose diverse per chi legge: un file che non c'è è un errore di chi
/// l'ha registrato, un file che non è testo è un'altra cosa.
#[derive(Debug, Serialize)]
#[serde(tag = "stato", rename_all = "kebab-case")]
pub enum Testo {
    /// Il file c'è ed è un testo.
    Presente {
        /// Il file, **parola per parola**: nessun HTML ripulito, nessuna
        /// estrazione, nessun riassunto. Chi lo mostra deve sapere che sta
        /// guardando il sorgente del materiale, non il materiale reso.
        contenuto: String,
        /// I byte del file. Maggiore dei caratteri se il file contiene
        /// accenti o emoji, ed è il numero che la pagina usa per dire quanto
        /// pesa.
        byte: usize,
    },
    /// Il file non è qui, o non è un testo che questa rotta può portare.
    Assente {
        /// Perché. Un campo che non c'è e un campo vuoto si somigliano, e la
        /// pagina non deve indovinare quale dei due sia.
        motivo: MotivoTesto,
        /// I byte del file quando il file c'è e il motivo è la sua forma.
        #[serde(skip_serializing_if = "Option::is_none")]
        byte: Option<usize>,
    },
}

/// Perché il testo non c'è, detto per intero.
///
/// Sono cinque ragioni e sono tutte vere: nessuna di loro è un errore del
/// client, nessuna è un `404`, e nessuna dice a qualcuno che non poteva leggere
/// l'argomento — perché se poteva leggerlo, il file è suo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotivoTesto {
    /// L'argomento non ha un `rel_path`: è nato via API e non ha un file.
    NessunFile,
    /// Il `rel_path` che sta nel database non porta a un file dentro il corpus.
    /// È una riga scritta a mano, o un symlink che esce: `sandbox::resolve_file`
    /// lo rifiuta e la rotta non indovina.
    FuoriCorpus,
    /// Il file c'è ma non si può leggere, o la radice del corpus non c'è più.
    NonLeggibile,
    /// Il file supera [`MAX_TESTO`].
    TroppoGrande,
    /// Il file non è UTF-8. Un artifact è HTML (D14) e l'HTML è UTF-8: un file
    /// che non lo è non è un materiale perso, è un file che questa rotta
    /// rifiuta di sb decoding invece di mostrare mojibake.
    NonTesto,
}

/// Il testo del file d'ingresso di un argomento.
///
/// `rest` è vuoto, quindi la risoluzione è sempre quella del file
/// d'ingresso: i figli — fogli di stile, immagini, script — stanno **sotto**
/// il percorso dell'argomento (vedi [`sandbox::resolve_file`]) e non
/// arrivano qui. È una parte dichiarata di ciò che questa rotta non rende, non
/// un dettaglio: la pagina lo dice.
fn testo_del_file(argomento: &Argument, config: &ServerConfig) -> Testo {
    let assente = |motivo: MotivoTesto, byte: Option<usize>| Testo::Assente { motivo, byte };
    let Some(rel_path) = argomento.rel_path.as_deref() else {
        return assente(MotivoTesto::NessunFile, None);
    };
    let radice = match config.canonical_corpus_root() {
        Ok(r) => r,
        Err(errore) => {
            // Non è un errore della richiesta: l'argomento è leggibile e lo
            // risponde. È un errore d'installazione, e va detto una volta.
            tracing::error!(%errore, "radice del corpus");
            return assente(MotivoTesto::NonLeggibile, None);
        }
    };
    let percorso = match sandbox::resolve_file(&radice, rel_path, "") {
        Some(ArtifactResolution::Ingresso(p)) => p,
        // `rest` è vuoto, quindi `Figlio` non può arrivare: se arrivasse, il
        // percorso non è quello che questa funzione crede e non lo si serve.
        _ => return assente(MotivoTesto::FuoriCorpus, None),
    };
    // La dimensione si legge **prima**: leggere un file da due giganti per
    // scoprire dopo che era troppo grande è il modo di trasformare un tetto in
    // una promessa.
    match std::fs::metadata(&percorso) {
        Ok(m) if m.len() > MAX_TESTO as u64 => {
            return assente(MotivoTesto::TroppoGrande, Some(m.len() as usize));
        }
        Ok(_) => {}
        Err(errore) => {
            tracing::warn!(%errore, "file di argomento non leggibile");
            return assente(MotivoTesto::NonLeggibile, None);
        }
    }
    let grezzo = match std::fs::read(&percorso) {
        Ok(byte) => byte,
        Err(errore) => {
            tracing::warn!(%errore, "file di argomento non leggibile");
            return assente(MotivoTesto::NonLeggibile, None);
        }
    };
    match String::from_utf8(grezzo) {
        Ok(contenuto) => Testo::Presente {
            byte: contenuto.len(),
            contenuto,
        },
        Err(errore) => assente(MotivoTesto::NonTesto, Some(errore.as_bytes().len())),
    }
}

/// `GET /api/v1/arguments/{id}/claims`
///
/// `200 {"claims": [...]}` · `404`.
///
/// Una claim non è altro materiale con regole sue: è un'affermazione **su**
/// quell'unità di insegnamento, e mostrarne le citabilità senza l'unità sarebbe
/// incoerente. Quindi la visibilità è quella dell'argomento, e
/// `kbs-store::claims_for` la applica chiamando `read_argument` per primo.
///
/// Le claim soppresse e quelle ritratte ci sono, con il loro stato: un registro
/// che cancella i propri errori non è un registro (D6). Cosa farne è compito del
/// lettore, e il lettore deve poter vedere che c'è qualcosa da guardare.
pub async fn claims(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(id): Path<String>,
) -> Result<Json<ClaimsResponse>, ApiError> {
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let claims = state
        .db
        .read(|store| Ok(store.claims_for(identita.person(), &id)?))?;
    Ok(Json(ClaimsResponse { claims }))
}

#[derive(Debug, Serialize)]
pub struct ClaimsResponse {
    /// Le affermazioni sull'argomento, nello stato in cui sono.
    pub claims: Vec<Claim>,
}
