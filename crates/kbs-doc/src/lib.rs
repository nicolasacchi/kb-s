//! `kbs-doc` — il livello documento di `kb-s`.
//!
//! Qui il formato di authoring è **HTML**, non markdown (D14), e three.js è
//! **vendorizzato nel repository**, mai da CDN (D15). Non è una scelta di
//! stile: sono le due cose che rendono un artifact di studio verificabile.
//!
//! # I pezzi
//!
//! | modulo | che cosa decide |
//! |---|---|
//! | [`contract`] | D7: otto sezioni, budget in byte, `GUARDIAN` per primo, `truncated ⇒ non eseguibile` |
//! | [`parser`] | titolo, `<meta>`, intestazioni con id stabile, corpo, contratto, claim |
//! | [`offbox`] | D15: nessun riferimento fuori dalla scatella |
//! | [`validate`] | tutto ciò che impedisce a un file di essere pubblicato |
//! | [`build`] | l'artefatto spedito, e il budget con i due conti separati |
//! | [`markdown`] | il materiale preesistente, convertito e **dichiarato** |
//!
//! # La regola che questo crate esiste per applicare
//!
//! > Un contratto troncato è leggibile ma non eseguibile.
//!
//! È [`contract::ContractReport::executable`], ed è un test
//! (`il_caso_che_un_validatore_ingenuo_passa`): il caso è un contratto lungo
//! più di 8192 byte la cui testa contiene tutte e otto le intestazioni, che un
//! validatore che tronca e poi valida dà per buono.
//!
//! # Che cosa questo crate non fa
//!
//! Non chiama il checker e non esegue niente. [`validate`] verifica che un
//! riferimento a un checker esista e che il contratto sia integro; stabilire se
//! una risposta è corretta è mestiere di `kbs-exercise`, e stabilire se una
//! claim è vera è mestiere del registro (D6).
//!
//! # Errori
//!
//! Nessun `unwrap()` fuori dai test. Quello che non si può fare è un
//! [`error::DocError`] o un issue di [`validate`], e ogni errore nomina **la
//! regola** rotta, non il sintomo: un professore che ha scritto `## LIMITI`
//! invece di `## LIMITE` deve leggere «intestazione sconosciuta `LIMITI`: le otto
//! sezioni sono GUARDIAN, PREREQUISITI, …», non «errore di parsing».

pub mod build;
pub mod contract;
pub mod error;
pub mod markdown;
pub mod offbox;
pub mod parser;
pub mod scan;
pub mod slug;
pub mod testing;
pub mod validate;

// I test delle claim stanno in un modulo senza `#[cfg(test)]` perché
// documentano una parte della API pubblica e sono leggibili come specifica.
#[cfg(test)]
mod claims;

pub use build::{build, BuildOptions, BuildOutput, Budget, Gate, Resolver};
pub use contract::{ContractError, ContractReport, HARD_CAP, SECTIONS};
pub use error::DocError;
pub use markdown::convert as convert_markdown;
pub use offbox::{external as external_references, THREE_RUNTIME, VENDOR_PREFIX};
pub use parser::{parse, ParsedArtifact, ParsedClaim, SpanBinding, CONTRACT_SLOT};
pub use validate::{inspect as validate, ArtifactReport, IssueCode, Severity};
