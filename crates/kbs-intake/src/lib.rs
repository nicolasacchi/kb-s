//! `kbs-intake` — le quattro strade del docente, il model lock, e il gate.
//!
//! Questo crate risponde a una domanda sola: **se un docente contesta un
//! esercizio due anni da ora, che cosa gli si mostra?** La risposta è un
//! registro, e per essere un registro ha tre pezzi che un diario non ha: la
//! provenienza di ogni generazione (D10), la catena delle ratifiche (D4) e il
//! verdetto che ha giudicato il testo **prima** che il testo entrasse.
//!
//! # I pezzi
//!
//! | modulo | che cosa decide |
//! |---|---|
//! | [`corpus_hash`] | l'hash di corpus: che cosa è dentro, in che ordine, come è concatenato |
//! | [`prompt`] | i byte canonici del prompt e il model lock che li impegna (D10) |
//! | [`route`] | **il collo di bottiglia**: l'unica porta di scrittura di un argomento |
//! | [`capture`] | D10.1, la strada del paste, che non indovina |
//! | [`cli`] | D10.2, la CLI come protocollo, e il binario `kbs` |
//! | [`scan`] | D10.3, il file come interfaccia, alla scala di un corpus |
//! | [`mcp`] | D10.4, il docente dentro un agente |
//! | [`gate`] | D4, l'unica strada che porta `in-corso` |
//! | [`diagnosis`] | il giudizio orale del docente, che non è riproducibile e lo dice |
//! | [`pratica`] | il tentativo non assistito: l'unica strada che scrive `unaided = 1` |
//! | [`authoring`] | la strada di D8: l'esercizio dal file al registro, con la risposta prodotta dal generatore |
//!
//! # La regola che questo crate esiste per applicare
//!
//! > **Un item non verificato è leggibile, ma non è citabile.**
//!
//! È [`route::Verdict::can_publish`] da un lato e [`gate`] dall'altro, e
//! because sono due lati dello stesso oggetto: il verdetto dice se il testo
//! regge, la porta dice se il testo ratificato è ancora quello di ieri.
//!
//! # Le quattro strade sono una
//!
//! Il test che tiene insieme tutto questo è `tests/le_quattro_strade.rs`: lo
//! stesso contenuto che entra dalla cattura, dalla CLI, da un file e da MCP
//! produce **lo stesso stato e lo stesso verdetto**. Non è un test di copertura,
//! è la prova che un gate che quattro strade possono aggirare non è un gate.
//!
//! # Il buco, dichiarato
//!
//! Il corpus non può dire *quali generazioni sono nate da una diagnosi orale*:
//! `generations` non ha la colonna. Il legame sopravvive dentro `prompt_hash` —
//! quindi è verificabile ma **non interrogabile**. Il buco ha un nome,
//! `generations.senza-causa`, ed è in [`diagnosis`]. Non è stato riempito con un
//! campo di testo libero, che sarebbe sembrato tenere il legame e non lo
//! tiene.
//!
//! # Che cosa questo crate NON fa
//!
//! * **non chiama un modello**, in nessun punto, in nessuna forma (D3). Il
//!   modello è uno strumento del docente, e qui si costruisce e si hashano i byte
//!   che il docente manderà *a lui*;
//! * **non possiede lo schema**: `kbs-store` scrive, e questo crate chiama;
//! * **non è il generatore**: i programmi e le famiglie sono di
//!   `kbs-exercise`, e [`authoring`] li chiama senza reimplementarne nessuno —
//!   la ragione per cui `scan` non produce istanze è in [`scan`];
//! * **non serve HTTP**: quello è `kbs-server`, e le operazioni che espone sono
//!   le stesse di questo crate — il che è il motivo per cui la CLI è una
//!   libreria con un binario sottile e non un binario.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod authoring;
pub mod capture;
pub mod cli;
pub mod corpus_hash;
pub mod diagnosis;
pub mod error;
pub mod gate;
pub mod mcp;
pub mod prompt;
pub mod pratica;
pub mod route;
pub mod scan;

pub use error::{Error, Result};
pub use corpus_hash::{CorpusHash, CorpusHasher, hash_dir};
pub use diagnosis::Diagnosis;
pub use gate::Gate;
pub use prompt::{DiagnosisRef, GenerationRequest};
pub use route::{Diagnostic, Receipt, Request, Route, Verdict, content_hash, receive};
pub use pratica::Tentativo;

/// La versione di questo crate. Se un altro modulo chiede «con quale versione è
/// stato scritto», la domanda giusta non è questa: per lo schema è
/// `kbs_store::Store::schema_epoch`, per il protocollo è
/// [`cli::PROTOCOLLO`].
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// La versione del generatore di richieste. Entra nel lock (D10) e cambiarne il
/// valore cambia la riproducibilità di ogni generazione passata, quindi è un
/// atto esplicito e non un dettaglio.
pub const GENERATOR_VERSION: &str = "kbs-intake/1";
