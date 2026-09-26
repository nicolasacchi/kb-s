//! Il gate di pubblicazione di D8, in una funzione.
//!
//! «Un esercizio senza verificatore deterministico non entra nel percorso di
//! pubblicazione» e «il test obbligatorio su ogni item pubblicato è
//! `no_solution-leak`». Sono due regole che, in un sistema vero, vengono sempre
//! dimenticate insieme perché stanno in due posti diversi. Qui sono una funzione:
//!
//! ```
//! # use kbs_exercise::{audit, families};
//! let g = families::by_name("algebra-polinomio").expect("famiglia nota");
//! let (istanza, _checker) = audit(g.as_ref(), "seed-1").expect("unità pubblicabile");
//! assert!(istanza.rendered_prompt.contains('('));
//! ```
//!
//! Il fallimento è **duplice e distinto**: un checker rotto non è la stessa cosa
//! di un GUARDIAN che rivela, e chi deve correggere ha due cose diverse da fare.
//! Il secondo caso non è riapribile: un GUARDIAN che contiene la risposta non si
//! sistema con un asterisco.

use kbs_core::{Checker, Instance};

use crate::check::validate_checker;
use crate::error::ExerciseError;
use crate::generator::Generator;
use crate::leak::{no_solution_leak, Leak};

/// Perché l'unità non pubblica.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PublicationError {
    #[error("il verificatore deterministico non è confrontabile: {0}")]
    BrokenChecker(#[from] ExerciseError),

    #[error("il GUARDIAN rivela la risposta: {0}")]
    Leaks(#[from] Leak),
}

/// La tupla (istanza, checker) di un'unità che si può pubblicare.
///
/// Nessuna delle due viene restituita se una delle due regole fallisce: la
/// funzione non ha un modo per restituire «quasi pubblicabile».
pub fn audit(
    generator: &dyn Generator,
    seed: &str,
) -> Result<(Instance, Checker), PublicationError> {
    let (istanza, checker) = generator.build(seed)?;
    validate_checker(&checker)?;
    no_solution_leak(generator.guardian(), &istanza, &checker)?;
    Ok((istanza, checker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families;
    use crate::leak::Leak;

    /// Un GUARDIAN che contiene la risposta di quell'istanza, per esteso, nel
    /// punto in cui dovrebbe stare l'istruzione.
    struct Bucato {
        dentro: Box<dyn Generator>,
        guard: String,
    }

    impl Generator for Bucato {
        fn family(&self) -> &str {
            self.dentro.family()
        }
        fn generator_version(&self) -> &str {
            self.dentro.generator_version()
        }
        fn build(&self, seed: &str) -> Result<(Instance, Checker), ExerciseError> {
            self.dentro.build(seed)
        }
        fn template_prompt(&self) -> &str {
            self.dentro.template_prompt()
        }
        fn guardian(&self) -> &str {
            &self.guard
        }
    }

    fn bucato(dentro: Box<dyn Generator>, seed: &str) -> Bucato {
        let (i, _) = dentro.build(seed).expect("istanza generabile");
        Bucato {
            guard: format!("La risposta è {}.", i.expected),
            dentro,
        }
    }

    #[test]
    fn un_guardian_bucato_non_passa_e_un_guardian_sano_si() {
        for g in families::all() {
            let seed = "sano";
            let (i, c) = audit(g.as_ref(), seed).expect("il GUARDIAN della famiglia è sano");
            let bucato = bucato(g, seed);
            // Stesso seed, stessa istanza, stesso checker: cambia solo il
            // GUARDIAN, e cambia l'esito della pubblicazione.
            let (i2, c2) = bucato.build(seed).expect("istanza");
            assert_eq!((i2.expected, c2), (i.expected, c));
            match audit(&bucato, seed) {
                Err(PublicationError::Leaks(Leak::Reveals { .. })) => {}
                altro => panic!("il GUARDIAN bucato doveva essere bloccato, non {altro:?}"),
            }
        }
    }

    #[test]
    fn un_checker_rotto_ferma_prima_del_guardian() {
        #[derive(Debug)]
        struct Rotto;
        impl Generator for Rotto {
            fn family(&self) -> &str {
                "rotto"
            }
            fn generator_version(&self) -> &str {
                "1"
            }
            fn build(&self, seed: &str) -> Result<(Instance, Checker), ExerciseError> {
                Ok((
                    Instance {
                        exercise: "rotto".into(),
                        seed: seed.into(),
                        rendered_prompt: "p".into(),
                        expected: "a".into(),
                        params: serde_json::json!({}),
                    },
                    Checker::MultipleChoice {
                        correct_index: 7,
                        options: vec!["a".into()],
                    },
                ))
            }
            fn template_prompt(&self) -> &str {
                "p"
            }
            fn guardian(&self) -> &str {
                "testo"
            }
        }
        assert!(matches!(
            audit(&Rotto, "s"),
            Err(PublicationError::BrokenChecker(
                ExerciseError::CorrectIndexOutOfRange { index: 7, len: 1 }
            ))
        ));
    }
}
