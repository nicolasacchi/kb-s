//! Famiglia `algebra-polinomio` — manipolazione **simbolica**.
//!
//! Il ragionamento è uno solo e non cambia con i numeri: moltiplicare due
//! binomi, raccogliere i termini uguali, ordinare per grado. I coefficienti sono
//! interi a due cifre perché un prodotto fra numeri a una cifra si fa a mente e
//! il compito diventerebbe un esercizio di aritmetica, non di algebra.
//!
//! Il checker è `Equivalence`, con la **forma normale dichiarata** dal
//! generatore: tre termini, grado decrescente, coefficienti interi, nessuna
//! parentesi. Lo studente che scrive `2 ( x + 3 )` invece di `2*(x+3)` è
//! accettato (spaziature e parentesi esterne non cambiano la forma); lo studente
//! che scrive `2x+6` è rifiutato, e vale `2*(x+3)` per ogni `x`: accettarlo
//! richiederebbe un parser, e vedi `check`.

use kbs_core::{Checker, Instance};
use serde_json::json;

use crate::error::ExerciseError;
use crate::generator::Generator;
use crate::seed::Rng;

const FAMILY: &str = "algebra-polinomio";
const VERSION: &str = "poly-expand/1";
const PROMPT: &str = "Sviluppa il prodotto di due binomi di primo grado e scrivi il risultato nella forma \
normale dichiarata: tre termini in ordine decrescente di grado, coefficienti interi, nessuna parentesi.";
const GUARDIAN: &str =
    "Sviluppa senza saltare la raccolta: un prodotto di due binomi ha tre termini, \
non quattro, e quello noto è il prodotto delle costanti.";

#[derive(Debug, Clone, Copy)]
pub struct PolyExpand;

impl Generator for PolyExpand {
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
        // Coefficienti a due cifre: nessuno dei tre termini risultanti può
        // essere 0, 1 o -1, quindi la forma normale non ha casi speciali.
        let p = rng.range_inclusive(11, 99);
        let q = rng.range_inclusive(11, 99);
        let r = rng.range_inclusive(11, 99);
        let s = rng.range_inclusive(11, 99);

        let quadratic = p * r;
        let linear = p * s + q * r;
        let constant = q * s;
        let normalized = format!("{quadratic}x^{linear}x+{constant}");

        let instance = Instance {
            exercise: self.exercise_id(seed),
            seed: seed.to_string(),
            rendered_prompt: format!(
                "Sviluppa ({p}x + {q})({r}x + {s}) e scrivi il polinomio nella forma normale: \
                 tre termini, grado decrescente, coefficienti interi."
            ),
            expected: normalized.clone(),
            params: json!({ "p": p, "q": q, "r": r, "s": s }),
        };
        Ok((instance, Checker::Equivalence { normalized }))
    }
}
