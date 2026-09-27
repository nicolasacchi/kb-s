//! Il [`Store`] dietro un `Arc`, e i due modi in cui si prende.
//!
//! # Perché un `Mutex` e non un pool
//!
//! `rusqlite::Connection` è `Send` ma non è `Sync`, e lo stato di `kbs-s` sta
//! in un file locale. Un pool di connessioni servirebbe a un server che
//! riceve richieste da più thread mentre il database è su un disco lento e
//! rete: qui il database è accanto al processo, le transazioni sono
//! dell'ordine del microsecondo, e il collo di bottiglia vero è il rendering.
//! Un `Mutex` dice la verità sul vincolo invece di fingere di averlo risolto.
//!
//! # Il lavoro bloccante dentro un handler `async`
//!
//! Le transazioni che stanno dentro [`Db::read`] e [`Db::write`] sono
//! **sincrone** e non attraversano mai un `.await`: la guardia cade prima che
//! l'handler ritorni, e quindi non può essere tenuta viva da un future.
//! Questo blocca il thread dell'esecutore per la durata della transazione,
//! che è la durata di un `SELECT` su un file locale. Se un giorno il
//! database smette di stare sul disco accanto al processo, la correzione è
//! `spawn_blocking` e non un'altra forma di `await` tenendo la guardia: un
//! `await` con la guardia in mano rende il lock async e serve una
//! [`tokio::sync::Mutex`], che è un altro tipo di problema.
//!
//! # Il veleno
//!
//! Un `Mutex` avvelenato significa che un handler è andato in panico **mentre
//! teneva la guardia**. Il database è in transazione, e il rollback di SQLite
//! è la fine della transazione: lo stato su disco è coerente. Fermare il
//! server per un panico che il database ha già gestito sarebbe trasformare un
//! bug in un'interruzione, quindi la guardia si riprende. Il panico si vede
//! nei log, che è dove un bug deve stare.

use std::sync::{Arc, Mutex, MutexGuard};

use kbs_store::{Result as StoreResult, Store};

use crate::error::ApiError;

/// Lo stato del mondo, condiviso da tutti gli handler.
#[derive(Debug, Clone)]
pub struct Db(Arc<Mutex<Store>>);

impl Db {
    /// Apre un database su file, alla versione di questo binario.
    pub fn open(path: impl AsRef<std::path::Path>) -> StoreResult<Self> {
        Ok(Db(Arc::new(Mutex::new(Store::open(path)?))))
    }

    /// Un database in memoria, migrato come quello su file.
    pub fn in_memory() -> StoreResult<Self> {
        Ok(Db(Arc::new(Mutex::new(Store::open_in_memory()?))))
    }

    /// Prende in prestito il [`Store`] per una lettura, nello spazio d'errore
    /// del livello dati.
    ///
    /// Per una closure che chiama solo metodi di `kbs-store`. La traduzione in
    /// [`ApiError`] la fa l'handler, una volta, con `?`.
    pub fn read<T>(&self, f: impl FnOnce(&Store) -> StoreResult<T>) -> StoreResult<T> {
        let guardia = self.lock();
        f(&guardia)
    }

    /// Prende in prestito il [`Store`] per una scrittura, nello spazio d'errore
    /// del livello dati.
    pub fn write<T>(&self, f: impl FnOnce(&mut Store) -> StoreResult<T>) -> StoreResult<T> {
        let mut guardia = self.lock();
        f(&mut guardia)
    }

    /// Come [`Db::read`], ma per una closure che passa anche da
    /// [`crate::capability`], che risponde già in [`ApiError`].
    ///
    /// I due metodi invece di uno solo con l'errore come parametro: un metodo
    /// generico sull'errore non si lascia inferire in una closure che usa `?` —
    /// e una closure che non si lascia inferire è una closure che il prossimo
    /// riscrive sbagliando. Qui la distinzione è visibile nella chiamata.
    pub fn read_api<T>(&self, f: impl FnOnce(&Store) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let guardia = self.lock();
        f(&guardia)
    }

    /// Come [`Db::write`], ma nello spazio d'errore di questo crate.
    pub fn write_api<T>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let mut guardia = self.lock();
        f(&mut guardia)
    }

    fn lock(&self) -> MutexGuard<'_, Store> {
        self.0.lock().unwrap_or_else(|avvelenata| avvelenata.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_scrittura_e_visibile_alla_lettura_successiva() {
        // Non «due letture danno lo stesso numero», che non prova niente: la
        // guardia è rilasciata a fine closure, e se non lo fosse la seconda
        // lettura si bloccherebbe — o peggio, restituirebbe uno zero.
        let db = Db::in_memory().expect("database");
        db.write(|store| {
            Ok(store.upsert_person(&kbs_store::Person {
                id: kbs_core::PersonId::fixture(1),
                display_name: "Prof. Rossi".into(),
                created_at: kbs_core::Millis(0),
            })?)
        })
        .expect("scrittura");
        let letto = db
            .read(|store| {
                Ok(store
                    .relations_of(&kbs_core::PersonId::fixture(1), &kbs_core::CourseId::fixture(1))?
                    .len())
            })
            .expect("lettura");
        assert_eq!(letto, 0);
        assert!(db.read(|s| s.schema_epoch()).is_ok());
    }

    #[test]
    fn il_veleno_non_ferma_il_server() {
        let db = Db::in_memory().expect("database");
        let p = db.clone();
        let risultato = std::thread::spawn(move || {
            p.read(|_| -> kbs_store::Result<()> {
                panic!("un handler è andato in panico tenendo la guardia")
            })
        })
        .join();
        assert!(risultato.is_err(), "il panico deve propagarsi al thread");
        // Dopo il panico il database è ancora utilizzabile: è la transazione a
        // essere finita, non il server.
        assert!(db.read(|s| s.schema_epoch()).is_ok());
    }
}
