//! Famiglia `statistica-media-pesata` — computazione **reale**.
//!
//! Il ragionamento è la media pesata: pesare, sommare, dividere per la somma dei
//! pesi. I valori sono interi a due cifre (i voti in centesimi) e i pesi sono
//! cifre (1..9), perché un peso a due cifre non è un peso: è un numero.
//!
//! Il checker è `Numeric` con tolleranza assoluta dichiarata `0,0005`: la risposta
//! attesa è una resa a quattro decimali, e la tolleranza copre il resto della
//! divisione esatta senza aprire la porta a un errore che un altro studente non
//! avrebbe. Perché assoluta e non relativa: vedi `check::compare_numeric`.
//!
//! Qui la risposta **non** è quasi mai un intero, che è il motivo per cui la
//! famiglia serve a qualcosa: un checker che accettasse solo interi renderebbe
//! l'esercizio non valutabile per due terzi delle istanze.

use kbs_core::{Checker, Instance};
use serde_json::json;

use crate::error::ExerciseError;
use crate::generator::Generator;
use crate::seed::Rng;

const FAMILY: &str = "statistica-media-pesata";
const VERSION: &str = "mean-weighted/1";
const TOLERANCE: f64 = 0.0005;
const PROMPT: &str =
    "Calcola la media pesata dei valori dati, ciascuno con il suo peso, e riportala \
con quattro decimali. La tolleranza di confronto è assoluta e vale 0,0005.";
const GUARDIAN: &str = "Mostra il peso di ogni valore prima di dividere: la media pesata non è la \
somma dei valori, e il denominatore è la somma dei pesi, non il numero dei valori.";

#[derive(Debug, Clone, Copy)]
pub struct WeightedMean;

impl Generator for WeightedMean {
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
        // Tre voci, non quattro: la media pesata è una frazione con denominatore
        // piccolo (la somma dei pesi), e lo spazio delle risposte è quello di una
        // griglia di razionali. È un limite strutturale di questa famiglia, non
        // un difetto della generazione, e `tests/sweep.rs` lo dichiara scegliendo
        // per questa famiglia uno sweep più corto. Passare a quattro voci non
        // aiuta: sposta la griglia, non la allarga.
        let x = [0u64, 1, 2].map(|_| rng.range_inclusive(11, 99));
        let w = [0u64, 1, 2].map(|_| rng.range_inclusive(1, 9));

        let numeratore: u64 = (0..3).map(|i| x[i] * w[i]).sum();
        let denominatore: u64 = w.iter().sum();
        let media = numeratore as f64 / denominatore as f64;

        let instance = Instance {
            exercise: self.exercise_id(seed),
            seed: seed.to_string(),
            rendered_prompt: format!(
                "Calcola la media pesata di {} con pesi {}, e riportala con quattro decimali.",
                lista(&x),
                lista(&w)
            ),
            expected: format!("{media:.4}"),
            params: json!({ "x": x, "w": w }),
        };
        Ok((
            instance,
            Checker::Numeric {
                tolerance: TOLERANCE,
            },
        ))
    }
}

fn lista(v: &[u64; 3]) -> String {
    v.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
