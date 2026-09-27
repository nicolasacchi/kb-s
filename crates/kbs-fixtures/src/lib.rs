//! `kbs-fixtures` — il banco di prova di `kb-s`.
//!
//! Il banco è due cose, e sono due cose diverse:
//!
//! 1. **Un corpus di quaranta item** che copre tutte e dodici le famiglie di
//!    media del corpus di studio e che contiene, per costruzione, ciò che
//!    deve funzionare e ciò che deve **non** funzionare: cinque fixture rotte
//!    (una sezione mancante, un `GUARDIAN` oltre i 640 byte, un contratto
//!    oltre l'hard cap, un riferimento a un CDN esterno, un ciclo nei
//!    prerequisiti), una ratifica superata, una claim contraddetta e una non
//!    citabile, una scena 3D con una claim per nodo e per arco, e un esercizio
//!    parametrizzato con due istanze.
//! 2. **Un runner** che verifica il comportamento dell'intera pipeline e che
//!    dice, in ogni esecuzione, che cosa ha verificato, che cosa ha fallito e
//!    **che cosa ha saltato e perché**.
//!
//! # Perché i fixture sono un crate e non una cartella
//!
//! Perché i fixture hanno bisogno di essere tipizzati per essere verificabili.
//! «Il corpus contiene quaranta file e almeno uno per famiglia» è un controllo
//! che si scrive in trenta secondi in Rust e non si scrive affatto in bash
//! senza diventare un altro linguaggio da mantenere. La tabella è statica, la
//! resa è pura, e i file committati sono confrontati con ciò che la tabella
//! descrive: se qualcuno sistema una fixture a mano, il banco diventa rosso e
//! nomina il file.
//!
//! # Il banco non duplica la validazione
//!
//! I controlli `corpus.*` verificano **le fixture**, non il sistema: che i
//! quattrocento byte del `GUARDIAN` siano davvero oltre i 640, che l'item
//! rotto contenga davvero l'URL della CDN. I controlli `pipeline.*` verificano
//! **il sistema** e sono saltati, con la ragione, quando la pipeline non è
//! raggiungibile. Le due famiglie non si sovrappongono, e nessuna delle due fa
//! il lavoro dell'altra.
//!
//! # Determinismo
//!
//! Nessun `Millis::now()`, nessun percorso assoluto nel referto, nessuna
//! iterazione su tabelle non ordinate. L'hash del corpus è un numero fissato in
//! un test: se cambia, è cambiato un fixture, e il referto dice quale.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod checks;
pub mod contract;
pub mod corpus;
pub mod families;
pub mod items;
pub mod render;
pub mod report;
pub mod spec;
mod table_1;
mod table_2;
mod table_3;
mod table_4;

pub use checks::{Banco, Config, Controllo, Esito, Referto};
pub use corpus::Corpus;
pub use families::Family;
pub use items::{voce, voci, DIMENSIONE};
pub use spec::Spec;

/// Il numero di fixture rotte: le cinque che il banco usa per la metà negativa.
/// È una costante perché il banco verifica che le cinque ci siano **e** che
/// siano proprio cinque.
pub const FIXTURE_ROTTE: usize = 5;
