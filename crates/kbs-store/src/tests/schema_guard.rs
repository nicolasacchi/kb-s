//! L'apertura: pragma, epoch, e il rifiuto di un database troppo nuovo.
//!
//! La guardia dell'epoch è la ragione per cui `kbs-s` può pretendere di essere
//! onesto su D11: un binario vecchio che scrive sopra uno schema nuovo non fa un
//! rollback, fa una corruzione silenziosa, e l'unica difesa è non aprire.

use rusqlite::OptionalExtension;

use super::School;
use crate::{error::Error, Store};
use crate::schema;

#[test]
fn un_database_piu_nuovo_del_binario_non_si_apre() {
    let s = School::new();
    s.store
        .conn()
        .execute("UPDATE schema_epoch SET epoch = 999 WHERE id = 1", [])
        .expect("avanzamento fittizio dell'epoch");

    match Store::open(&s.path).expect_err("non si apre") {
        Error::SchemaFromTheFuture { db, binary } => {
            assert_eq!(db, 999);
            assert_eq!(binary, schema::binary_epoch());
            assert!(db > binary);
        }
        other => panic!("atteso SchemaFromTheFuture, ottenuto {other:?}"),
    }

    // E il messaggio dice perché, non solo che cosa.
    let testo = Store::open(&s.path).expect_err("ancora").to_string();
    assert!(testo.contains("binario vecchio"), "{testo}");
}

#[test]
fn un_database_all_epoch_del_binario_si_apre_e_non_rimigra() {
    let s = School::new();
    let epoch = s.store.schema_epoch().expect("epoch");
    assert_eq!(epoch, schema::binary_epoch());

    let riaperta = s.reopen();
    assert_eq!(riaperta.schema_epoch().expect("epoch"), epoch);
    // Le migrazioni non girano due volte: se girassero, il `CREATE TABLE` darebbe
    // un errore, e un'apertura che fallisce alla seconda volta sarebbe un
    // database che si consuma.
    let tabelle: i64 = riaperta
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'arguments'",
            [],
            |r| r.get(0),
        )
        .expect("conteggio");
    assert_eq!(tabelle, 1);
}

#[test]
fn una_migrazione_applicata_non_si_riscrive() {
    // refinery confronta il checksum di ogni migrazione applicata con quello del
    // file. Se il database ne ha una con checksum diverso, l'apertura si ferma:
    // è la difesa che rende vero «forward-only» e non una convenzione.
    let s = School::new();
    s.store
        .conn()
        .execute("UPDATE refinery_schema_history SET checksum = '1' WHERE version = 2", [])
        .expect("checksum alterato");

    let err = Store::open(&s.path).expect_err("apertura rifiutata");
    let testo = err.to_string();
    assert!(
        testo.contains("different than filesystem"),
        "atteso un errore di divergenza, ottenuto: {testo}"
    );
}

#[test]
fn una_migrazione_mancante_nel_binario_non_si_apre() {
    // Il caso dell'altra metà della stessa regola: una migrazione applicata che il
    // binario non conosce è uno schema che il binario non sa scrivere.
    let s = School::new();
    s.store
        .conn()
        .execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on, checksum) \
             VALUES (9000, 'V9000__fantasma', '2026-01-01T00:00:00+00:00', '1')",
            [],
        )
        .expect("migrazione fantasma");

    let err = Store::open(&s.path).expect_err("apertura rifiutata");
    assert!(
        err.to_string().contains("missing from the filesystem"),
        "{err}"
    );
}

#[test]
fn i_pragma_che_il_layer_pretende_sono_quelli() {
    let s = School::new();
    let conn = s.store.conn();

    // Le chiavi esterne sono OFF di default in SQLite: senza questa riga i
    // `REFERENCES` dello schema sarebbero commenti, e «un voto senza rubric non
    // entra» sarebbe una preghiera.
    let fk: i64 = conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .expect("foreign_keys");
    assert_eq!(fk, 1);

    // `synchronous = FULL`: un registro append-only che perde l'ultima
    // transazione in un crash non è append-only, è Auszug.
    let sync: i64 = conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .expect("synchronous");
    assert_eq!(sync, 2, "FULL");

    let journal: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .expect("journal_mode");
    assert_eq!(journal, "wal", "su file il journal è WAL");
}

#[test]
fn l_apertura_su_un_file_nuovo_crea_lo_schema_e_su_quello_vecchio_no() {
    let s = School::new();
    assert!(s.path.exists());
    let riaperta = s.reopen();
    let fonte: Option<String> = riaperta
        .conn()
        .query_row(
            "SELECT slug FROM sources WHERE id = ?1",
            [s.course.0.as_str()],
            |r| r.get(0),
        )
        .optional()
        .expect("lettura");
    assert_eq!(fonte.as_deref(), Some("analisi-1"));
}

#[test]
fn il_pragma_wal_non_e_richiesto_su_un_database_in_memoria() {
    // `PRAGMA journal_mode = WAL` su `:memory:` è un errore, non un no-op: un
    // `open_in_memory` che lo chiede non apre.
    let store = crate::Store::open_in_memory().expect("in memoria");
    let journal: String = store
        .conn()
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .expect("journal_mode");
    assert_eq!(journal, "memory");
    assert_eq!(store.schema_epoch().expect("epoch"), schema::binary_epoch());
}

#[test]
fn il_log_delle_migrazioni_c_e_una_riga_per_migrazione() {
    let s = School::new();
    let conteggio: i64 = s
        .store
        .conn()
        .query_row("SELECT COUNT(*) FROM refinery_schema_history", [], |r| {
            r.get(0)
        })
        .expect("conteggio");
    assert_eq!(conteggio, schema::binary_epoch() as i64);
}

#[test]
fn l_epoch_del_database_e_un_intero_e_la_sua_tabella_ha_una_riga_sola() {
    let s = School::new();
    // `id = 1` con CHECK: la tabella ha una riga sola per costruzione, e non per
    // convenzione dell'applicazione. Provarlo costa una riga e vale la pena.
    s.store
        .conn()
        .execute("INSERT INTO schema_epoch (id, epoch) VALUES (2, 2)", [])
        .expect_err("una sola riga");
}
