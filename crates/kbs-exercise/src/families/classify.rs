//! Famiglia `numeri-quale-frazione-ridotta` — **classificazione con distrattori**.
//!
//! Il ragionamento è: prendere quattro frazioni, calcolare quattro MCD,
//! dichiarare l'unica che ha MCD 1. Le frazioni sono a tre cifre perché un MCD
//! su due cifre si fa a mente e l'esercizio non sarebbe niente.
//!
//! I distrattori sono costruiti, non campionati, e la loro costruzione è la
//! parte interessante. I moltiplicatori sono `(2,2)`, `(2,6)` e `(6,4)`: hanno
//! tutti MCD maggiore di uno — altrimenti il distrattore sarebbe *anche* lui in
//! forma ridotta e la domanda avrebbe due risposte — e i loro rapporti sono
//! diversi, quindi i tre valori sono `p/q`, `p/(3q)` e `3p/(2q)`. Il primo
//! distrattore vale esattamente quanto la risposta e non è ridotto: è la trappola
//! giusta, perché lo studente che riduce tutto e sceglie «l'unica che non
//! cambia» sceglie la sbagliata.
//!
//! Campionare le alternative, invece, avrebbe prodotto con probabilità buona
//! tre frazioni senza nessun rapporto con la risposta: lo studente le avrebbe
//! eliminate a occhio e l'esercizio avrebbe misurato la fortuna invece del MCD.
//!
//! L'ordine delle opzioni dipende dal seed, quindi l'indice dell'esatto cambia da
//! uno studente all'altro: copiare l'indice del vicino non porta da nessuna
//! parte.

use kbs_core::{Checker, Instance};
use kbs_verify::{GeneratorKey, InstanceGenerator, ReplayError};
use serde_json::json;

use crate::error::ExerciseError;
use crate::generator::{Generator, replay_instance};
use crate::seed::Rng;

/// Il nome della famiglia, pubblico perché il catalogo lo dichiara: vedi
/// `poly::FAMILY` per la ragione.
pub const FAMILY: &str = "numeri-quale-frazione-ridotta";
pub const VERSION: &str = "classify-reduced/1";
const PROMPT: &str =
    "Quattro frazioni sono proposte e una sola è in forma ridotta. Rispondi con il \
testo della frazione ridotta, non con la sua posizione.";
const GUARDIAN: &str = "Riduci tutte e quattro le frazioni, anche quelle che ti sembrano già \
semplici: la risposta è l'unica che non cambia, non l'unica che ti sembra bella.";

const LETTERS: [char; 4] = ['A', 'B', 'C', 'D'];

#[derive(Debug, Clone, Copy)]
pub struct ClassifyReduced;

impl Generator for ClassifyReduced {
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
        let (p, q) = coprime_pair(&mut rng)?;
        // Moltiplicatori con MCD > 1: un distrattore già ridotto renderebbe la
        // domanda con due risposte. Rapporti distinti: i tre valori non si
        // somigliano, e solo il primo vale quanto la risposta.
        let alternative = [(2 * p, 2 * q), (2 * p, 6 * q), (6 * p, 4 * q)];
        let mut slot = [0usize; 4];
        let posti = rng.distinct_inclusive(1, 3, 3)?;
        for (k, posto) in posti.iter().enumerate() {
            slot[*posto as usize] = k + 1;
        }
        let options: Vec<String> = slot
            .iter()
            .map(|&s| {
                let (num, den) = if s == 0 { (p, q) } else { alternative[s - 1] };
                format!("{num}/{den}")
            })
            .collect();
        // Lo zero è in uno slot esattamente una volta, per costruzione: se non ci
        // fosse, la domanda non avrebbe una risposta sola e la famiglia si rifiuta
        // invece di indovinare.
        let correct_index = match slot.iter().position(|&s| s == 0) {
            Some(i) => i as u8,
            None => {
                return Err(ExerciseError::DegenerateInstance {
                    reason: "nessuno slot ospita la frazione generata".into(),
                })
            }
        };

        let elenco = options
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{}. {}", LETTERS[i], o))
            .collect::<Vec<_>>()
            .join("   ");

        let instance = Instance {
            exercise: self.exercise_id(seed),
            seed: seed.to_string(),
            rendered_prompt: format!(
                "Quale di queste frazioni è in forma ridotta (MCD fra numeratore e denominatore \
                 uguale a 1)? Rispondi con il testo della frazione.\n{elenco}"
            ),
            expected: format!("{p}/{q}"),
            params: json!({ "p": p, "q": q, "opzioni": options.clone() }),
        };
        Ok((
            instance,
            Checker::MultipleChoice {
                correct_index,
                options,
            },
        ))
    }
}

impl InstanceGenerator for ClassifyReduced {
    fn version(&self) -> &str {
        self.generator_version()
    }

    fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError> {
        replay_instance(self, key)
    }
}

/// Una coppia coprime a tre cifre, entro un numero di tentativi dichiarato.
///
/// Se il seed non trovasse nulla entro il limite, la famiglia si rifiuta invece
/// di restituire una frazione che non è coprime: un distrattore già ridotto
/// renderebbe la domanda con due risposte.
fn coprime_pair(rng: &mut Rng) -> Result<(u64, u64), ExerciseError> {
    for _ in 0..256 {
        let p = rng.range_inclusive(100, 999);
        let q = rng.range_inclusive(100, 999);
        if gcd(p, q) == 1 {
            return Ok((p, q));
        }
    }
    Err(ExerciseError::DegenerateInstance {
        reason: "nessuna coppia coprime a tre cifre in 256 tentativi".into(),
    })
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genera(seed: &str) -> (Instance, Checker) {
        ClassifyReduced.build(seed).expect("istanza generabile")
    }

    fn frazione(s: &str) -> (u64, u64) {
        let (a, b) = s
            .split_once('/')
            .expect("una frazione è numeratore/denominatore");
        (a.parse().expect("cifre"), b.parse().expect("cifre"))
    }

    #[test]
    fn una_sola_opzione_e_ridotta_e_il_testo_e_il_rispondente() {
        for seed in ["a", "b", "classe-1", "classe-2", "7", "99"] {
            let (i, c) = genera(seed);
            let Checker::MultipleChoice {
                correct_index,
                options,
            } = c
            else {
                unreachable!("il checker è una scelta multipla")
            };
            let esatta = &options[correct_index as usize];
            assert_eq!(esatta, &i.expected);
            let (num, den) = frazione(esatta);
            assert_eq!(gcd(num, den), 1, "{esatta} non è in forma ridotta");
            for (k, o) in options.iter().enumerate() {
                if k == correct_index as usize {
                    continue;
                }
                let (num, den) = frazione(o);
                assert_ne!(
                    gcd(num, den),
                    1,
                    "{o} è già ridotta: la domanda avrebbe due risposte"
                );
            }
        }
    }

    #[test]
    fn le_opzioni_son_quattro_e_distinte() {
        for seed in ["a", "b", "3", "4", "5", "6"] {
            let (_, c) = genera(seed);
            let Checker::MultipleChoice { options, .. } = c else {
                unreachable!("il checker è una scelta multipla")
            };
            assert_eq!(options.len(), 4);
            for i in 0..options.len() {
                for j in (i + 1)..options.len() {
                    assert_ne!(options[i], options[j], "opzione ripetuta in {seed}");
                }
            }
        }
    }
}
