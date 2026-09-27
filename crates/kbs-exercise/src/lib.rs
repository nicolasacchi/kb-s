//! `kbs-exercise` — esercizi parametrizzati con verificatore deterministico.
//!
//! Questo crate è la D8 di `ARCHITECTURE.md`: il pezzo in cui **l'integrità
//! accademica è una proprietà della costruzione e non un progetto di
//! sorveglianza**. Niente di ciò che segue guarda uno studente, neanche per
//! sbaglio.
//!
//! # Le sei cose che ci sono
//!
//! 1. [`Generator`]: una **famiglia** di esercizi e una messa in forma
//!    deterministica da `(generator_version, seed)`. Nessun orologio, nessun
//!    filesystem, nessuna casualità di sistema: la stessa tupla produce lo stesso
//!    byte, rendering compreso (D11).
//! 2. [`check`]: il lato di grading di `kbs_core::Checker`. Un checker confronta
//!    forme dichiarate, non giudica; e per `Equivalence` confronta una forma
//!    normale **dichiarata**, non un'espressione valutata.
//! 3. [`leak`]: `no_solution_leak`, la prova che il GUARDIAN non contiene la
//!    risposta, con la decisione su che cosa «contenere» significhi scritta in
//!    testa al modulo e verificata da un test che fallisce quando il GUARDIAN
//!    è bucato.
//! 4. [`grading`]: la catena `deterministico → pari → umano` come **dato**,
//!    non come flusso di controllo.
//! 5. [`publication::audit`]: il gate, che è la somma di 2 e 3 in una funzione —
//!    perché due regole che si ricordano separatamente vengono sempre dimenticate
//!    insieme.
//! 6. [`generator::replay_instance`]: il ponte a `kbs_verify`, che è dove D11
//!    diventa una domanda e non una descrizione. Il trait
//!    `kbs_verify::InstanceGenerator` è dichiarato in `kbs-verify` — è quel
//!    crate a stabilire la domanda — e **qui** è implementato, una volta per
//!    famiglia, in un `match` chiuso sul catalogo. La dipendenza va in un solo
//!    verso, e la direzione è questa: senza l'implementazione, D11 è una frase
//!    su un registro che nessun generatore può difendere.
//!
//! # Che cosa non c'è, e perché
//!
//! * **Nessun modello.** Non in `GradingOrder`, non in `Checker`, non in nessun
//!   tipo di questo crate. Non è una promessa di documentazione: `kbs_core::GraderKind`
//!   non ha la variante, e le funzioni che lo consumano fanno un `match`
//!   esauritivo, quindi un modello non può introdursi qui senza rompere la
//!   compilazione.
//! * **Nessuno storage, nessun HTTP, nessuna UI.** Le istanze sono valori che
//!   escono e rientrano; chi le conserva è `kbs-store`, e non è un dettaglio:
//!   tenere insieme generazione e persistenza invites a mettere dentro il
//!   generatore tutto ciò che serviva altrove.
//! * **Nessun parser di espressioni.** Vedi `check`, e leggi il perché: è la
//!   decisione che rende questa parte dell'integrità verificabile.

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]
pub mod check;
pub mod error;
pub mod families;
pub mod generator;
pub mod grading;
pub mod leak;
pub mod publication;
pub mod seed;

pub use check::{
    grade, is_bound_to, may_publish, normalize_equivalence, validate_checker, Verdict,
};
pub use error::ExerciseError;
pub use families::{all, by_name, Family};
pub use generator::{Generator, replay_instance};
pub use grading::{rank, GradingChain, Outcome, GRADING_ORDER};
pub use leak::{no_solution_leak, Leak};
pub use publication::{audit, PublicationError};
pub use seed::Rng;
