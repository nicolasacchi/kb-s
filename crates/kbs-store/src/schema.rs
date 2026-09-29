//! L'apertura del database: pragma, guardia dell'epoch, migrazioni in avanti.
//!
//! Tre guardie, in quest'ordine, e ognuna chiude una classe di danno:
//!
//! 1. **Versione di SQLite.** Serve almeno 3.43: le tabelle sono `STRICT` e
//!    l'indice FTS5 usa `contentless_delete`. Un SQLite più vecchio non è un
//!    database peggiore, è un database diverso, e fallire in apertura è meglio
//!    che scoprirlo al primo `CREATE TABLE`.
//! 2. **Epoch.** Un binario vecchio su uno schema nuovo non è un rollback, è
//!    una corruzione silenziosa: non sa quali colonne esistono, e la sua
//!    prossima migrazione ricreerebbe tabelle che esistono già. Per questo
//!    l'epoch del database viene letta **prima** di migrare, e se supera
//!    l'epoch che il binario conosce, l'apertura si ferma.
//! 3. **Migrazioni in avanti, mai indietro.** refinery non sa fare il rollback e
//!    non fa finta di saperlo. Le migrazioni applicate non si riscrivono: il
//!    checksum in `refinery_schema_history` fa sì che una migrazione modificata
//!    faccia fallire l'apertura invece di passare inosservata.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{Error, Result};

/// Le migrazioni, incorporate nel binario.
///
/// Un binario vecchio incorpora un insieme più piccolo di migrazioni, ed è
/// esattamente questo che rende la guardia dell'epoch sufficiente: non serve
/// memorizzare da qualche parte «quale versione del binario avevo», perché
/// l'epoch del binario *è* la versione massima delle sue migrazioni.
mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

/// La versione minima di SQLite che `kbs-store` sa usare.
pub const MINIMUM_SQLITE: &str = "3.43.0";

/// L'epoch che questo binario conosce: la versione massima delle sue
/// migrazioni. Non è una costante scritta a mano, quindi non può divergere dal
/// numero di file in `migrations/`.
pub fn binary_epoch() -> i32 {
    embedded::migrations::runner()
        .get_migrations()
        .iter()
        .map(|m| m.version())
        .max()
        .unwrap_or(0)
}

/// `true` se una versione di SQLite soddisfa [`MINIMUM_SQLITE`].
///
/// Funzione pura, così la guardia è verificabile senza una SQLite vecchia: il
/// test le passa le stringhe che si vuole e controlla la risposta.
pub fn sqlite_is_supported(version: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split(['.', '-'])
            .take_while(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty())
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let found = parse(version);
    let want = parse(MINIMUM_SQLITE);
    for (f, w) in found.iter().zip(want.iter()) {
        if f != w {
            return f > w;
        }
    }
    found.len() >= want.len()
}

/// Applica i pragma e apre.
///
/// `synchronous = FULL`: il costo è un `fsync` per commit, e in un registro
/// append-only di osservazioni è il prezzo della parola «append-only». `NORMAL`
/// sarebbe più veloce e perderebbe l'ultima transazione in un crash di
/// alimentazione: un giudizio perso è un ricorso che non si può fare.
fn configure(conn: &Connection) -> Result<()> {
    let found: String = conn.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    if !sqlite_is_supported(&found) {
        return Err(Error::SqliteTooOld {
            found,
            required: MINIMUM_SQLITE,
        });
    }
    // Le chiavi esterne sono OFF per default in SQLite. Senza questa riga i
    // `REFERENCES` dello schema sarebbero commenti.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Su un database in memoria il journal WAL non esiste, e chiederlo è un
    // errore; il file è l'unico caso in cui ha senso.
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    if mode != "memory" {
        conn.pragma_update(None, "journal_mode", "WAL")?;
    }
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(())
}

/// L'epoch dichiarata dal database, o `0` se il database non è ancora stato
/// migrato.
///
/// `0` è il valore giusto e non una tolleranza: un database nuovo non ha
/// `schema_epoch`, e per un database che non ha ancora `schema_epoch` non c'è
/// niente da proteggere — la migrazione 1 lo crea.
fn read_epoch(conn: &Connection) -> Result<i32> {
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_epoch')",
        [],
        |r| r.get(0),
    )?;
    if !table_exists {
        return Ok(0);
    }
    let epoch: i32 = conn.query_row("SELECT epoch FROM schema_epoch WHERE id = 1", [], |r| {
        r.get(0)
    })?;
    Ok(epoch)
}

/// Applica le migrazioni e verifica che l'epoch finale sia quella attesa.
///
/// **L'ordine con cui refinery le applica non è l'ordine delle versioni.**
/// `find_migration_files` restituisce i file nell'ordine in cui li legge, e su
/// questo filesystem, quando le migrazioni erano sei, era `[2, 1, 4, 5, 3]`: è
/// stato proprio un test a notarlo, con l'epoch finale a 1 dopo una migrazione
/// applicata per ultima. Per questo ogni migrazione **alza** l'epoch con
/// `MAX(epoch, N)` e non lo assegna: applicarle in ordine sbagliato non deve
/// cambiare il risultato. E il controllo qui sotto resta quello che vale — dopo
/// tutte le migrazioni, l'epoch deve essere l'ultima versione, altrimenti un file
/// manca o è stato rinumerato e il binario non sa che cosa sta scrivendo.
pub fn migrate(conn: &mut Connection) -> Result<i32> {
    embedded::migrations::runner().run(conn)?;
    let (db, binary) = (read_epoch(conn)?, binary_epoch());
    if db != binary {
        return Err(Error::SchemaEpochMismatch { db, binary });
    }
    Ok(db)
}

/// Apre un database su file, migrazioni comprese.
pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    let (db, binary) = (read_epoch(&conn)?, binary_epoch());
    if db > binary {
        return Err(Error::SchemaFromTheFuture { db, binary });
    }
    migrate(&mut conn)?;
    Ok(conn)
}

/// Apre un database in memoria. Utilissimo nei test, e in un futuro
/// `kbs-verify` che non vuole toccare il disco.
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn the_version_guard_accepts_and_refuses() {
        assert!(sqlite_is_supported("3.43.0"));
        assert!(sqlite_is_supported("3.50.2"));
        assert!(sqlite_is_supported("4.0.0"));
        // Il confronto è numerico, non lessicografico: `3.9` è più vecchio di
        // `3.43` anche se come stringa è «maggiore».
        assert!(!sqlite_is_supported("3.9.0"));
        assert!(!sqlite_is_supported("3.42.9"));
        assert!(!sqlite_is_supported("3.1"));
        assert!(sqlite_is_supported("3.45.1-alpha"));
    }

    #[test]
    fn binary_epoch_matches_the_migration_files() {
        let versions: Vec<i32> = embedded::migrations::runner()
            .get_migrations()
            .iter()
            .map(|m| m.version())
            .collect();
        // Le versioni sono 1..8 in un ordine che **non** è quello: refinery le
        // legge nell'ordine del filesystem e le applica così. Ogni versione
        // alza l'epoch con `MAX`, quindi l'ordine non conta — ma l'insieme delle
        // versioni sì, e questo è il numero che la guardia confronta.
        //
        // Il numero è dichiarato qui e non dedotto: una guardia che si
        // autocostruisce l'attesa non confronta niente, e questa è la ragione per
        // cui l'elenco è una lista scritta a mano. Aggiungere una migrazione
        // significa aggiungere un numero qui, e se qualcuno lo dimentica questo
        // test è rosso — che è il comportamento voluto.
        let mut ordinata = versions.clone();
        ordinata.sort_unstable();
        assert_eq!(ordinata, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(binary_epoch(), 9);
    }
}
