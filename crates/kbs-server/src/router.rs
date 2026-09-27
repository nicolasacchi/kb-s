//! Il punto in cui un [`Db`] e una [`ServerConfig`] diventano un [`Router`].
//!
//! È un file di quattro righe e c'è un motivo che vale una volta sola: il
//! prossimo agente deve poter costruire l'applicazione in un test senza
//! conoscere i dettagli di montaggio, e `app(...)` è il nome che rende quella
//! cosa possibile. Tutto il resto — le rotte, i layer, il fallback — è in
//! [`crate::routes`].

use axum::Router;

use crate::config::ServerConfig;
use crate::db::Db;
use crate::routes::{router, AppState};

/// L'applicazione, pronta da servire o da interrogare.
///
/// `config` viene presa per valore e messa in un `Arc` dentro
/// [`AppState::new`]: la configurazione non cambia durante la vita del processo,
/// e passarla per riferimento lascerebbe aperta la porta a un cambiamento a
/// metà, che è il tipo di cambiamento che nessuno ricorda di aver fatto.
pub fn app(db: Db, config: ServerConfig) -> Router {
    router(AppState::new(db, config))
}

/// Lo stato, per chi ha bisogno del bus o del runtime senza montare le rotte.
///
/// Esiste per i test e per un futuro `/api/v1/health` più informativo: lo stato
/// è la cosa che le rotte condividono, e senza un modo per ottenerlo non si può
/// scrivere un test che osserva un evento pubblicato da una rotta.
pub fn stato(db: Db, config: ServerConfig) -> AppState {
    AppState::new(db, config)
}
