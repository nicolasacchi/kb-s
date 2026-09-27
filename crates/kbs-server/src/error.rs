//! La traduzione da regola a status HTTP — e la risposta che non distingue.
//!
//! # `404` significa una cosa sola
//!
//! «Non c'è, oppure non lo vedi» produce **lo stesso status, gli stessi
//! header e gli stessi byte**. Non è una scelta di pulizia: `arg_<fnv16 del
//! percorso>` è enumerabile da chiunque abbia il corpus, quindi distinguere le
//! due ipotesi è un canale per imparare che cosa c'è in un corso. `kbs-store`
//! restituisce già [`kbs_store::Error::NotReadable`] in entrambi i casi
//! (`kbs-store/src/store.rs:464-481`); il compito di questo modulo è non
//! disfarle.
//!
//! Lo stesso vale per [`kbs_store::Error::NotFound`] e per
//! [`kbs_store::Error::NotACourseTeacher`]: tutti e tre arrivano a
//! [`ApiError::Absent`], che ha un solo corpo possibile —
//! [`ApiError::ABSENT_BODY`] — e nessun campo che dipenda dalla risorsa.
//!
//! # Che cosa *non* è garantito, dichiarato perché non lo è
//!
//! **La durata.** `kbs-store::read_argument` ritorna subito quando l'argomento
//! non esiste e, quando esiste, esegue ancora le query di relazione. Sono
//! due percorsi di durata diversa, e la differenza è dell'ordine dei
//! microsecondi su un file SQLite locale. Equalizzarla qui non è possibile
//! senza toccare `kbs-store`, che è un altro crate e non è di questo agente;
//! e un ritardo fisso non sarebbe una soluzione ma una messinscena: renderebbe
//! la risposta lenta, non uguale, e insegnerebbe a chi misura che la
//! mediazione esiste. La correzione sta nel punto in cui la domanda viene
//! posta, cioè in `kbs-store::read_argument`, che dovrebbe eseguire il
//! predicato anche quando la riga non c'è. Finché quel punto non cambia,
//! questa riga è la lista dei limiti.
//!
//! # I corpi di errore
//!
//! Un errore dice **quale regola** è stata rotta, in italiano, e nomina D4, D5,
//! D9 o D12 quando è quella la regola. Un errore che dice «errore interno»
//! senza altro è un bug del server: il dettaglio va nel log, non nella
//! risposta, e il log lo scrive [`tracing`].

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use kbs_store::Error as StoreError;

/// Il corpo dell'errore HTTP.
///
/// `#[serde(skip_serializing_if)]` è ciò che rende [`ApiError::ABSENT_BODY`]
/// possibile: gli campi opzionali spariscono invece di diventare `null`, e
/// quindi la risposta di «non c'è» e quella di «non lo vedi» sono la stessa
/// stringa e non due stringhe che coincidono oggi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorBody {
    /// Identificatore breve e stabile dell'errore, per il branching del client.
    pub error: &'static str,
    /// La regola di `ARCHITECTURE.md` rotta, quando c'è.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regola: Option<&'static str>,
    /// Perché, in italiano. Chi ha sbagliato è una persona che sta costruendo
    /// un corso, e la frase che la ferma è la parte utile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivo: Option<String>,
}

impl ErrorBody {
    fn new(error: &'static str) -> Self {
        ErrorBody {
            error,
            regola: None,
            motivo: None,
        }
    }

    fn rule(error: &'static str, regola: &'static str) -> Self {
        ErrorBody {
            error,
            regola: Some(regola),
            motivo: None,
        }
    }

    fn why(error: &'static str, motivo: impl Into<String>) -> Self {
        ErrorBody {
            error,
            regola: None,
            motivo: Some(motivo.into()),
        }
    }
}

/// Un errore di questo server.
///
/// Le varianti sono poche e sono **regole**, non sintomi. Una variante per ogni
/// modo in cui una cosa può andare stampa di più di quel che il modello
/// sappia dire, e finisce per riciclarsi in un `500` che non aiuta nessuno.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Non è stato dichiarato chi chiede. Non è un `403`: non è ancora stato
    /// deciso nulla su di lui.
    #[error("nessuna dichiarazione di identità")]
    IdentityMissing,

    /// L'identità dichiarata non è un id di persona. Detto prima di guardare la
    /// risorsa, quindi non parla di risorse.
    #[error("identità dichiarata non valida")]
    IdentityMalformed,

    /// «Non c'è» e «non lo vedi». Un solo corpo, [`Self::ABSENT_BODY`].
    #[error("non trovato")]
    Absent,

    /// La richiesta è malformata: non riguarda la visibilità.
    #[error("richiesta non valida: {motivo}")]
    BadRequest { motivo: String },

    /// Una regola di dominio impedisce l'operazione. Il `409` è la risposta
    /// giusta perché lo stato del mondo non è quello che la richiesta
    /// presuppone, e la risposta deve dire *quale* presupposto.
    #[error("{motivo}")]
    Conflict {
        regola: &'static str,
        motivo: String,
    },

    /// Il runtime three.js non è vendorizzato (D15). È un errore di
    /// installazione, non una risorsa mancante: un `404` qui direbbe «non
    /// esiste» e invece esiste e non è stato messo dove doveva.
    #[error("il runtime three.js non è vendorizzato in {percorso}: un artifact che chiama fuori non è verificabile (D15)")]
    ThreeNotVendored { percorso: String },

    /// Errore del livello sotto. Il dettaglio va nel log.
    #[error("errore del livello dati: {0}")]
    Store(StoreError),
}

impl ApiError {
    /// L'unico corpo che «non c'è» e «non lo vedi» possono produrre.
    ///
    /// È una costante, non una costruzione: due chiamate diverse non possono
    /// comporre due corpi diversi, quindi la proprietà non dipende da una
    /// disciplina di scrittura ma dal tipo.
    pub const ABSENT_BODY: &'static str = r#"{"error":"non-trovato"}"#;

    fn status(&self) -> StatusCode {
        match self {
            ApiError::IdentityMissing => StatusCode::UNAUTHORIZED,
            ApiError::IdentityMalformed | ApiError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            ApiError::Conflict { .. } => StatusCode::CONFLICT,
            ApiError::Absent => StatusCode::NOT_FOUND,
            ApiError::ThreeNotVendored { .. } | ApiError::Store(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Il corpo della risposta.
    ///
    /// [`ApiError::Absent`] e i [`StoreError`] che valgono come «assente»
    /// passano di qui e producono lo stesso byte: è il punto in cui la
    /// distinguibilità viene eliminata, ed è l'unico punto.
    pub fn body(&self) -> ErrorBody {
        match self {
            ApiError::IdentityMissing => ErrorBody::new("identita-assente"),
            ApiError::IdentityMalformed => ErrorBody::new("identita-non-valida"),
            ApiError::Absent => ErrorBody::new("non-trovato"),
            ApiError::BadRequest { motivo } => ErrorBody::why("richiesta-non-valida", motivo.clone()),
            ApiError::Conflict { regola, motivo } => ErrorBody {
                error: "regola-di-dominio",
                regola: Some(regola),
                motivo: Some(motivo.clone()),
            },
            ApiError::ThreeNotVendored { percorso } => ErrorBody::rule(
                "runtime-non-vendorizzato",
                // Il percorso è nell'errore, non nel corpo: il corpo è per il
                // cliente, il percorso per chi amministra il deployment.
                "D15",
            )
            .with_path(percorso),
            ApiError::Store(_) => ErrorBody::new("errore-interno"),
        }
    }
}

impl ErrorBody {
    /// Il percorso del runtime mancante, che è un dato di amministrazione.
    fn with_path(mut self, percorso: &str) -> Self {
        self.motivo = Some(format!("manca {percorso}"));
        self
    }
}

impl From<StoreError> for ApiError {
    /// La mappa è l'unico posto in cui un errore del livello dati diventa una
    /// risposta HTTP, ed è scritta variante per variante perché le tre cose
    /// diverse che `kbs-store` dichiara («non c'è», «non lo vedi», «non ti
    /// compete») hanno tre destini diversi qui.
    fn from(error: StoreError) -> Self {
        match error {
            // Le tre che non si distinguono: una sola risposta.
            StoreError::NotReadable { .. } | StoreError::NotFound { .. } => ApiError::Absent,
            // «Non insegni il corso» è una risposta sul *merito* della
            // richiesta, non sull'esistenza del corso. Chi non insegna un corso
            // che conosce riceve `404` lo stesso di chi non insegna un corso
            // inesistente: la differenza tra i due casi non deve esistere, e il
            // motivo è che l'esportazione è una via d'uscita dal prodotto e
            // l'esportazione la prende il docente.
            StoreError::NotACourseTeacher { .. } => ApiError::Absent,

            // Le invarianti di dominio sono `409` e portano la regola: sono
            // l'unico caso in cui il client ha bisogno di sapere *perché*, e la
            // ragione è che può agire (ratificare, poi ripubblicare).
            StoreError::Invariant(inv) => ApiError::Conflict {
                regola: match inv {
                    kbs_core::Invariant::CitableWithoutRatification(_) => "D4",
                    kbs_core::Invariant::StaleRatification { .. } => "D4",
                    kbs_core::Invariant::CohortBelowThreshold { .. } => "D9",
                    kbs_core::Invariant::PrerequisiteCycle(_) => "D5",
                },
                motivo: inv.to_string(),
            },

            StoreError::InvalidField { field, reason } => ApiError::BadRequest {
                motivo: format!("campo `{field}` inaccettabile: {reason}"),
            },
            StoreError::EmptyQuery => ApiError::BadRequest {
                motivo: "la ricerca non contiene nessun termine cercabile".into(),
            },
            StoreError::DuplicateId { kind, id } => ApiError::Conflict {
                regola: "D6",
                motivo: format!("{kind} `{id}` esiste già: gli id sono unici e non si riciclano"),
            },
            StoreError::StateTransition { id, from, to } => ApiError::Conflict {
                regola: "D4",
                motivo: format!("{id} è {from:?} e non si può portare a {to:?} scrivendo (D4)"),
            },
            StoreError::RatificationThroughUpsert { id } => ApiError::Conflict {
                regola: "D4",
                motivo: format!("{id}: una ratifica si registra con `ratify`, non scrivendo"),
            },
            StoreError::AlreadyContested { grading } => ApiError::Conflict {
                regola: "D6",
                motivo: format!("{grading} è già contestato: si contesta una volta, poi si risolve"),
            },
            StoreError::SessionSealed { id } | StoreError::UnknownSession { id } => {
                ApiError::Conflict {
                    regola: "D6",
                    motivo: format!("la sessione di registro `{id}` non è utilizzabile"),
                }
            }
            StoreError::SessionRegister {
                id,
                expected,
                got,
            } => ApiError::Conflict {
                regola: "D6",
                motivo: format!("la sessione `{id}` è di registro `{expected}` e non `{got}`"),
            },

            // Qui dentro non c'è niente che il client possa fare. Il
            // dettaglio — che è SQL — va nel log, e nel corpo non ci va.
            altro => {
                tracing::error!(errore = %altro, "livello dati");
                ApiError::Store(altro)
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(errore = %self, "risposta 500");
        }
        if matches!(self, ApiError::Absent) {
            // Il corpo è la costante, non la sua ricostruzione: è l'unico modo
            // che `Absent` ha di produrre byte, e la sua serializzazione non
            // può divergere da quella dichiarata in `ABSENT_BODY` perché non
            // viene calcolata affatto.
            return (
                status,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                )],
                ApiError::ABSENT_BODY,
            )
                .into_response();
        }
        (status, Json(self.body())).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assente_e_non_leggibile_producono_lo_stesso_byte() {
        let non_esiste = ApiError::from(StoreError::NotFound {
            kind: "argomento",
            id: "arg_0123456789abcdef".into(),
        });
        let non_visibile = ApiError::from(StoreError::NotReadable {
            person: kbs_core::PersonId::fixture(3),
            id: kbs_core::ArgumentId::from_rel_path("corsi/x/lezione-01.html"),
            state: kbs_core::PublicationState::Bozza,
        });
        let assente = ApiError::Absent;

        for err in [&non_esiste, &non_visibile, &assente] {
            assert_eq!(err.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(non_esiste.body(), assente.body());
        assert_eq!(non_visibile.body(), assente.body());
    }

    #[test]
    fn il_corpo_di_assente_non_contiene_l_id_ne_lo_stato() {
        // La ragione per cui la risposta è uguale è che non contiene niente
        // di variabile. Il test lo dice, perché il giorno in cui qualcuno
        // aggiunge un campo questo deve fallire e non passare.
        let corpo = ApiError::Absent.body();
        let json = serde_json::to_string(&corpo).expect("corpo");
        assert_eq!(json, ApiError::ABSENT_BODY);
        assert!(!json.contains("arg_"), "{json}");
        assert!(!json.contains("bozza"), "{json}");
    }

    #[test]
    fn la_soglia_di_coorte_e_un_409_con_la_sua_regola() {
        let err = ApiError::from(StoreError::Invariant(
            kbs_core::Invariant::CohortBelowThreshold {
                failing: 3,
                min: kbs_core::COHORT_MIN_K,
            },
        ));
        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.body().regola, Some("D9"));
    }

    #[test]
    fn pubblicare_senza_ratifica_e_un_409_d4() {
        let err = ApiError::from(StoreError::Invariant(
            kbs_core::Invariant::CitableWithoutRatification(kbs_core::PublicationState::DelDocente),
        ));
        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.body().regola, Some("D4"));
    }
}
