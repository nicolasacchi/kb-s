//! La coda di richiamo di uno studente: la superficie che il docente apre.
//!
//! # Una rotta che non interrompe nessuno
//!
//! Il corpus chiama questa idea «il mentor a tempo» e le dà una proprietà che
//! è quasi un complimento: «**Non chiede un servizio sempre acceso**: l'idea 11
//! non chiede niente» (`19-idee-livello-relazionale.html`,
//! `#requisiti-per-la-20`). Quindi qui non c'è un demone, non c'è una coda che
//! scivola da sola, non c'è un `POST` che promette di avvisare qualcuno, e non
//! c'è un'`Email`. **Il calendario si apre quando qualcuno lo apre**, e questa è
//! la differenza fra una coda di richiamo e un sistema di solleciti: il primo è
//! un posto dove si guarda, il secondo è un posto da cui si viene disturbati, e
//! il secondo è un servizio.
//!
//! # Chi entra, e perché lo studente non c'è
//!
//! `teaches` sul corso, e nient'altro. Lo studente **non** può chiedere il
//! proprio calendario, e la ragione è del corpus e non un dettaglio di
//! implementazione: `09-esercizi-valutazione-grading.html` (`#limite-dashboard`)
//! scrive che «qualsiasi metrica esposta allo studente diventa presto la
//! funzione obiettivo del suo comportamento», e `18-provenienza-e-verifica.html`
//! (`#il-test-icap-su-una-verifica`) che «**La vista dello studente non mostra
//! percentuali**: mostra *righe con prova* e un bottone per contestarle». Lo
//! studente ha già le righe con prova: sono
//! `GET /api/v1/arguments/{id}/observations`, che leggono la vista
//! `unaided_observations`. Quello che gli manca non è la riprova, è l'elenco di
//! ciò che non ha ancora fatto — e quello è un cruscotto.
//!
//! Il predicato **non è riapplicato qui**: è dentro
//! [`kbs_store::Store::calendar`], che chiede `teaches` e restituisce
//! `NotReadable` altrimenti. Una seconda copia del predicato in questo crate
//! sarebbe una seconda risposta alla stessa domanda, e le due divergono. Quello
//! che questa rotta controlla è solo la regola **di corso**, che è di questo
//! livello: un corso in cui la persona non ha relazioni dà la risposta di
//! «assente», come in tutte le altre rotte di corso.
//!
//! # L'orizzonte è un parametro, e questa è la parte che un revisore deve
//! guardare per prima
//!
//! `?orizzonte=<millisecondi UTC>` è l'istante oltre il quale una ricomparsa non
//! assistita è «vecchia». **Non c'è un default e non c'è una soglia in
//! codice**, e la ragione è in `kbs_store::calendario`: `11` (`#la-fragilita-
//! strutturale`) scrive che «**Nessuno dei tre risultati principali misura la
//! ritenzione a settimane o mesi. È il buco più grave e più sistematico
//! dell'intero campo**», e `16` (`#limiti`) che «**Nessun numero di scala è
//! inventato, e nessuno è disponibile**». Un calendario che sceglie da solo il
//! proprio intervallo sceglie un numero sulla memoria di una persona, e lo
//! sceglie in un modulo di storage.
//!
//! **Senza `?orizzonte` la risposta è l'elenco e non un verdetto**: ogni voce
//! ha `beyond: null`, che in JSON è `null` e non `false`. La differenza è la
//! differenza fra «non lo so» e «no», ed è la stessa che rende `unaided`
//! nullable invece che `bool` con un default.
//!
//! # Che cosa non c'è in questa risposta, e le tre ragioni
//!
//! * **Nessuna percentuale e nessun punteggio.** Il corpo è la lista delle voci e
//!   nient'altro. Una coda è un lavoro da fare, non un voto.
//! * **Nessun «giorni di ritardo».** È il numero che diventa un indicatore di
//!   «sei in ritardo», e un indicatore di ritardo in una pagina che il docente
//!   apre mentre lo studente guarda è un obiettivo esterno.
//! * **Nessun titolo e nessun testo di argomento.** Le voci portano id, conteggio
//!   e date: il testo sta in `GET /api/v1/arguments/{id}` e portarlo qui
//!   significherebbe che questa rotta consegna materiale di un corso con un
//!   predicato diverso da quello che l'ha autorizzata.
//!
//! # Il buco dichiarato: la discesa all'individuo non scrive una riga
//!
//! Il corpus è severo su questo punto (`18`, `#le-due-viste-in-concreto`): «**Il
//! drill-down individuale esiste ed è un atto.** Si arriva a un singolo studente
//! solo da una tabella aggregata, con un motivo dichiarato, e la discesa scrive
//! una riga. Non c'è una pagina "i miei studenti"». E: «ogni discesa dal livello
//! classe al livello individuale è essa stessa una riga», con chi, quando e perché.
//!
//! **Questa rotta non scrive quella riga**, perché non c'è una tabella in cui
//! scriverla e questo modulo non scrive migrazioni. È quindi una **non
//! conformità dichiarata** al requisito, non un requisito soddisfatto a metà: il
//! DDL che la chiude è
//!
//! ```sql
//! -- V7: la discesa all'individuo è un atto, e l'atto è una riga.
//! CREATE TABLE register_reads (
//!     id          TEXT NOT NULL PRIMARY KEY,
//!     by          TEXT NOT NULL REFERENCES people (id),
//!     student     TEXT NOT NULL REFERENCES people (id),
//!     course_id   TEXT NOT NULL REFERENCES sources (id),
//!     motivo      TEXT NOT NULL CHECK (length(trim(motivo)) > 0),
//!     at          INTEGER NOT NULL
//! ) STRICT;
//! CREATE INDEX register_reads_by_student
//!     ON register_reads (course_id, student, at);
//! ```
//!
//! E finché quel DDL non esiste, il motivo dichiarato non è esigibile: questa
//! rotta non lo chiede, perché chiedere un motivo che nessuno registra è una
//! promessa che il prodotto non può mantenere, ed è il difetto che il corpus
//! chiama «un termine inventato è peggio di un termine assente».
//!
//! # Una cosa che questa rotta non fa e che va detta ad alta voce
//!
//! **Non scrive.** Non c'è un `POST` che registra un tentativo: la strada di
//! scrittura è `kbs-intake`, e passa dalla persona che la esegue e ne risponde.
//! Una `POST` qui avrebbe il difetto che il perimetro del progetto vieta per
//! nome: nessuna autenticazione, quindi un `POST` che scrive `unaided = 1` è un
//! numeratore della claim che chiunque può gonfiare. Vedi
//! `kbs-intake::pratica` e il suo verb `kbs tenta`.

use axum::extract::{Path, Query, State};
use axum::Json;

use kbs_core::{Millis, PersonId};
use kbs_store::calendario::Calendar;

use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::registers::WhoQuery;
use crate::routes::AppState;

/// I parametri della coda.
///
/// `person` è opzionale e vale l'identità dichiarata, come in tutte le rotte dei
/// registri: chi guarda il registro di uno studente deve scrivere il suo id. Qui
/// la differenza è che **uno studente non può arrivarci**: il predicato di
/// `Store::calendar` chiede `teaches`, e senza quella relazione la risposta è
/// quella di «assente».
#[derive(Debug, Default, serde::Deserialize)]
pub struct CalendarQuery {
    /// La persona di cui si vuole la coda.
    pub person: Option<String>,
    /// L'orizzonte dichiarato, in millisecondi UTC. Assente = nessun verdetto.
    ///
    /// È una `String` e non un `Option<Millis>` **per poter rifiutare** un valore
    /// che non è un intero con un messaggio che dice quale regola è rotta: il
    /// `QueryRejection` di axum risponderebbe `400` con un corpo che non nomina
    /// niente, e questa rotta ha l'obbligo di nominare.
    pub orizzonte: Option<String>,
}

/// L'orizzonte del chiamante, in millisecondi UTC.
///
/// Due esiti e una rifiuta: `None` quando l'opzione non c'è, il valore quando
/// c'è, e `BadRequest` quando non è un intero non negativo. Un orizzonte del
/// passato è accettato e non è un errore: è un istante, e gli istanti del passato
/// sono quelli che il docente sceglie quando guarda indietro.
fn orizzonte(raw: Option<&str>) -> Result<Option<Millis>, ApiError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let grezzo = raw.trim();
    if grezzo.is_empty() {
        return Err(ApiError::BadRequest {
            motivo: "`orizzonte` è vuoto: un orizzonte assente si omette, e un orizzonte \
                      vuoto è un orizzonte che il prodotto dovrebbe scegliere da sé"
                .into(),
        });
    }
    let n: i64 = grezzo.parse().map_err(|_| ApiError::BadRequest {
        motivo: format!(
            "`orizzonte` è `{grezzo}` e l'unità dichiarata sono i millisecondi UTC: un \
             numero di giorni qui sarebbe un intervallo di recupero, e un intervallo di \
             recupero è un'affermazione sulla memoria dello studente che questo prodotto \
             non ha misurato e non sceglie"
        ),
    })?;
    if n < 0 {
        return Err(ApiError::BadRequest {
            motivo: format!(
                "`orizzonte` è {n}, che è prima del 1970: nessuna ricomparsa registrata \
                 può essere più vecchia di un istante che non è mai arrivato"
            ),
        });
    }
    Ok(Some(Millis(n)))
}

/// `GET /api/v1/courses/{course}/calendario?person=…&orizzonte=…`
///
/// `200 {"student": …, "course": …, "horizon": …, "entries": […]}` · `404` se
/// non `teaches` il corso · `400` se `orizzonte` non è un intero non negativo.
///
/// `entries` è **vuota** e non c'è altro quando lo studente non ha ancora
/// ricomparso senza aiuto su nessun argomento del corso. Un elenco vuoto qui è
/// una risposta vera — non è mai successo niente — quindi non viene trasformato
/// in `404`, che qui significherebbe che il corso non c'è.
///
/// `horizon` torna **nella risposta** accanto a `entries` e non dentro ogni voce,
/// perché un verdetto e la sua spiegazione non possono viaggiare separati.
pub async fn calendario(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
    Query(query): Query<CalendarQuery>,
) -> Result<Json<Calendar>, ApiError> {
    let corso = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person();
    let studente = chi(&query, &identita)?;
    // `dichiarato` e non `orizzonte`: la funzione e la variabile si chiamerebbero
    // uguali, e una riga in cui una funzione è mascherata da un valore è una riga
    // in cui il prossimo non sa quale delle due sta leggendo.
    let dichiarato = orizzonte(query.orizzonte.as_deref())?;
    // Un solo prestito del `Store`: due `read_api` sarebbero due lock e due
    // istanti in cui le relazioni possono cambiare, e la risposta sarebbe
    // composta da due letture che non hanno visto lo stesso mondo.
    let calendario = state.db.read_api(|store| {
        // La regola **di corso** è di questo livello: un corso in cui la persona
        // non ha relazioni dà la risposta di «assente». La regola
        // **di autorizzazione** è di `kbs-store` e non la si riscrive: vedi il
        // doc del modulo.
        capability::require(store, persona, &corso, None)?;
        Ok(store.calendar(persona, &studente, &corso, dichiarato)?)
    })?;
    Ok(Json(calendario))
}

/// La persona di cui si chiede la coda.
///
/// Riusa [`WhoQuery`] invece di ripeterne la regola: un id di persona malformato
/// dà «assente» e non «richiesta non valida», e la ragione è che la forma degli
/// id è pubblica e non distingue nessuno. Una seconda copia di quella frase
/// sarebbe una seconda risposta alla stessa domanda.
fn chi(query: &CalendarQuery, identita: &SharedIdentity) -> Result<PersonId, ApiError> {
    WhoQuery {
        person: query.person.clone(),
    }
    .persona(identita)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kbs_core::CourseId;

    fn rifiutata(raw: Option<&str>) -> ApiError {
        orizzonte(raw).expect_err("rifiutata")
    }

    /// L'assenza di orizzonte **non** diventa un numero.
    ///
    /// È la prova che questo prodotto non ha un intervallo di recupero: se un
    /// domani `orizzonte` restituisse `Some(Millis(604_800_000))` quando
    /// l'opzione non c'è, questa riga continuerebbe a passare e la funzione
    /// avrebbe cominciato a scegliere la memoria dello studente al posto suo.
    #[test]
    fn senza_orizzonte_non_c_e_un_numero() {
        assert_eq!(orizzonte(None).expect("assente"), None);
        assert!(matches!(rifiutata(Some("   ")), ApiError::BadRequest { .. }));
    }

    /// Quello che il docente dichiara entra; quello che non è un istante no.
    ///
    /// I cattivi sono scelti per coprire i tre modi in cui si sbaglia: un'unità
    /// al posto di un numero («settimane»), una notazione che sembra un numero
    /// (`1e9`), e un numero negativo che sarebbe un orizzonte mai arrivato.
    #[test]
    fn un_orizzonte_entra_e_uno_male_no() {
        assert_eq!(
            orizzonte(Some(" 1700000000000 ")).expect("un intero"),
            Some(Millis(1_700_000_000_000))
        );
        for cattivo in ["settimane", "1e9", "-1", "7 giorni", "0x10"] {
            assert!(
                matches!(rifiutata(Some(cattivo)), ApiError::BadRequest { .. }),
                "{cattivo} è passato"
            );
        }
    }

    /// Il corpo della risposta non contiene nessun numero che sia una quota.
    ///
    /// Non è un controllo sul sorgente: è un controllo sul valore che esce,
    /// costruito con le chiavi che la rotta promette. Una risposta che
    /// aggiungesse `quota` o `percentuale` renderebbe falsa questa lista, ed è
    /// esattamente il cambiamento che un revisore deve notare.
    #[test]
    fn il_corpo_non_ha_una_quota() {
        let calendario = Calendar {
            student: PersonId::fixture(3),
            course: CourseId::fixture(1),
            horizon: None,
            entries: Vec::new(),
        };
        let corpo = serde_json::to_value(&calendario).expect("json");
        // Le chiavi si confrontano come **insieme**, non in ordine: `serde_json`
        // espone una `Map` che ordina, e un test che pretende l'ordine di
        // dichiarazione starebbe asserendo una proprietà di `BTreeMap`, non
        // della risposta.
        let mut chiavi = corpo
            .as_object()
            .expect("oggetto")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        chiavi.sort();
        assert_eq!(
            chiavi,
            vec!["course", "entries", "horizon", "student"],
            "le chiavi della risposta sono il contratto: aggiungerne una è aggiungere un \
             numero che qualcuno potrebbe usare come graduatoria"
        );
        assert!(
            corpo["horizon"].is_null(),
            "senza orizzonte dichiarato la risposta dice `null` e non `false`"
        );
    }
}
