//! I segnali di coorte: l'unica cosa che questo server non può sbagliare di
//! forma.
//!
//! # Un segnale che è soltanto piccolo non è un segnale
//!
//! D9: sotto `COHORT_MIN_K` un segnale **non esiste**. Non è un segnale con
//! `failing: 0`, non è un segnale con `failing: 3` e un flag `soppresso`, non è
//! un segnale con un `k` dichiarato. Non è un segnale.
//!
//! La ragione è che «in questa classe sette studenti sbaglano il terzo
//! teorema» con `k = 3` è, per chi lo legge, «tre studenti sbagliano il terzo
//! teorema» — e con sette nomi in una classe di venticinque, un docente che
//! conosce i propri studenti li trova. Il soglia non protegge il dato individuale
//! per metaforismo: protegge contro l'inferenza da un conteggio.
//!
//! Perciò la risposta sotto soglia è `{"signals": []}`. Nessun numero, nessun
//! segnaposto, nessun campo che dica «c'è qualcosa che non ti mostriamo»: quel
//! campo sarebbe un canale — uno studente che lo vede sa che su quell'argomento
//! qualcuno ha sbagliato, il che è metà dell'informazione.
//!
//! # Il filtro è doppio, e perché
//!
//! `kbs-store` rifiuta in scrittura un segnale sotto soglia e ha un `CHECK` nel
//! database che lo impedisce comunque; la lettura ne filtra un secondo livello
//! per il caso in cui la costante di dominio cambi e la migrazione non
//! segua. Qui il terzo livello è [`pubblicabile`], ed è quello che il test
//! esercita: una funzione che prende un `CohortSignal` e decide, con la stessa
//! soglia di `kbs-core`.
//!
//! Un filtro che nessun test può toccare vale come un filtro che non c'è. Il
//! caso che il test copre è proprio quello: il segnale sotto soglia che arriva
//! comunque, per una riga scritta a mano o per una costante cambiata, e che
//! qui non produce un corpo.
//!
//! # Chi li vede
//!
//! Chi insegna il corso. Il segnale di coorte è una domanda del docente —
//! «dove cade la classe?» — e D9 lo dice: *zero minuti aggiuntivi per il
//! docente*. Non è un dato della classe da mostrare alla classe: un aggregato
//! sopra soglia non rende nessuno anonimo **fra** gli studenti che lo leggono
//! insieme.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use kbs_core::{CohortSignal, COHORT_MIN_K, Relation};

use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// Un segnale di coorte esiste solo se è pubblicabile.
///
/// La soglia è quella di `kbs_core`, non una copia: se le due divergono, la
/// risposta di questa rotta e quella di `kbs-store` dicono cose diverse sulla
/// stessa riga, e il lettore non ha modo di sapere quale sia quella giusta.
pub fn pubblicabile(segnale: &CohortSignal) -> bool {
    segnale.failing >= COHORT_MIN_K
}

/// `GET /api/v1/arguments/{id}/cohort`
///
/// `200 {"argument": "…", "signals": [...]}` · `404` se non `teaches` il corso.
///
/// `signals` è **vuota** e non c'è altro: sotto soglia la risposta è questa e
/// non ne esiste una più corta. Vedi il doc del modulo.
pub async fn signals(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(id): Path<String>,
) -> Result<Json<SignalsResponse>, ApiError> {
    let id = ids::argument(&id).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let segnali = state.db.read_api(|store| {
        // Prima il corso, poi la relazione: senza sapere di quale corso si
        // parla non c'è relazione da chiedere, e la lettura dell'argomento è
        // comunque passata dal predicato.
        let corso = capability::course_of(store, &persona, &id)?;
        capability::require(store, &persona, &corso, Some(Relation::Teaches))?;
        let grezzi = store.cohort_signals(&id)?;
        Ok(grezzi.into_iter().filter(pubblicabile).collect::<Vec<_>>())
    })?;
    Ok(Json(SignalsResponse { argument: id, signals: segnali }))
}

#[derive(Debug, Serialize)]
pub struct SignalsResponse {
    /// L'argomento di cui sono i segnali.
    pub argument: kbs_core::ArgumentId,
    /// I segnali pubblicabili. Vuota se non ce ne sono: **senza** conteggi
    /// aggrediti, senza `k`, senza segnaposto.
    pub signals: Vec<CohortSignal>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kbs_core::{ArgumentId, CohortId, CourseId, Millis};

    fn segnale(failing: usize) -> CohortSignal {
        CohortSignal {
            course: CourseId::fixture(1),
            cohort: CohortId("2A".into()),
            argument: ArgumentId::from_rel_path("corsi/a/uno.html"),
            failing,
            total: 30,
            at: Millis(1_700_000_000_000),
        }
    }

    #[test]
    fn quattro_non_e_un_segnale_e_cinque_si() {
        assert!(!pubblicabile(&segnale(4)));
        assert!(pubblicabile(&segnale(5)));
        assert_eq!(COHORT_MIN_K, 5);
    }
}
