//! Famiglia `insiemi-differenza` — operazione su insiemi.
//!
//! Il ragionamento è la differenza: prendere `A`, togliere ciò che compare in
//! `B`, tenere il resto. Gli elementi sono interi a due cifre perché sono
//! confrontabili senza dover prima stabilire se `12 = 012`.
//!
//! Il prompt **non** chiede un ordine, e questa è una scelta che merita una
//! parola: il checker confronta su multiset, quindi chiedere l'ordine e non
//! controllarlo sarebbe un esercizio che promette più di quanto verifica — e
//! l'ordine che lo studente sceglie non ha nessun significato matematico. Il
//! prompt chiede «senza ripetizioni», che invece è verificabile ed è verificato.
//!
//! Il generatore rifiuta i semi per cui `A \ B` ha meno di due elementi: una
//! differenza vuota o di un elemento solo non è un esercizio, è un caso banale
//! che si indovina.

use kbs_core::{Checker, Instance};
use serde_json::json;

use crate::error::ExerciseError;
use crate::generator::Generator;
use crate::seed::Rng;

const FAMILY: &str = "insiemi-differenza";
const VERSION: &str = "set-difference/1";
const PROMPT: &str = "Elenca gli elementi di A che non appartengono a B. Scrivili separati da \
virgola, senza ripetizioni e senza parentesi. L'ordine non conta.";
const GUARDIAN: &str = "Controlla ogni elemento di A uno per uno: un elemento che compare anche \
in B va escluso, e un elemento escluso non torna in nessun altro punto dell'elenco.";

#[derive(Debug, Clone, Copy)]
pub struct SetDifference;

impl Generator for SetDifference {
    fn family(&self) -> &str {
        FAMILY
    }

    fn generator_version(&self) -> &str {
        VERSION
    }

    fn template_prompt(&self) -> &str {
        PROMPT
    }

    fn guardian(&self) -> &str {
        GUARDIAN
    }

    fn build(&self, seed: &str) -> Result<(Instance, Checker), ExerciseError> {
        let mut rng = Rng::from_seed(seed);
        // Il numero di tentativi è dichiarato e finito: un generatore che
        // continua a tirare finché non esce bene non è deterministico nelle sue
        // risorse, e un esercizio che a volte non esiste è un esercizio che a
        // volte non si può pubblicare.
        let (a, b) = disegna(&mut rng, 256)?;
        let elementi: Vec<String> = a
            .iter()
            .filter(|e| !b.contains(e))
            .map(|e| e.to_string())
            .collect();
        let risposta = elementi.join(", ");

        let instance = Instance {
            exercise: self.exercise_id(seed),
            seed: seed.to_string(),
            rendered_prompt: format!(
                "A = {{{}}}   B = {{{}}}\nElenca gli elementi di A che non appartengono a B.",
                lista(&a),
                lista(&b)
            ),
            expected: risposta.clone(),
            params: json!({ "a": lista(&a), "b": lista(&b) }),
        };
        Ok((instance, Checker::Set { elements: elementi }))
    }
}

/// Due insiemi la cui differenza ha almeno due elementi.
fn disegna(rng: &mut Rng, tentativi: u32) -> Result<(Vec<u64>, Vec<u64>), ExerciseError> {
    for _ in 0..tentativi {
        let a = rng.distinct_inclusive(10, 99, 6)?;
        let b = rng.distinct_inclusive(10, 99, 5)?;
        let differenza = a.iter().filter(|e| !b.contains(e)).count();
        if differenza >= 2 {
            return Ok((a, b));
        }
    }
    Err(ExerciseError::DegenerateInstance {
        reason: format!("nessun A senza almeno due elementi fuori da B in {tentativi} tentativi"),
    })
}

fn lista(v: &[u64]) -> String {
    v.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_differenza_ha_almeno_due_elementi_e_nessuno_appartiene_a_b() {
        for seed in ["a", "b", "classe-1", "3", "44"] {
            let (i, c) = SetDifference.build(seed).expect("istanza generabile");
            let Checker::Set { elements } = c else {
                unreachable!("il checker è un insieme")
            };
            assert!(elements.len() >= 2, "{seed}: differenza troppo piccola");
            let params = i.params.as_object().expect("params");
            let b = params["b"].as_str().expect("B").to_string();
            for e in &elements {
                assert!(
                    !b.contains(e.as_str()),
                    "{e} compare in B e non è in A \\ B"
                );
            }
            assert_eq!(elements.join(", "), i.expected);
        }
    }
}
