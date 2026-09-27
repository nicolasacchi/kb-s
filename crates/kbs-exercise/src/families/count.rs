//! Famiglia `aritmetica-conta-coprimi` — **conteggio** per inclusione-esclusione.
//!
//! Il ragionamento non è enumerare: è contare i numeri divisibili per ciascun
//! primo che divide `m`, e togliere le intersezioni a due a due, a tre a tre, e
//! così via. È il ragionamento della formula di Möbius, e cambia con `m` (che ha
//! uno, due, tre o quattro primi distinti) senza cambiare nella sua forma.
//!
//! `n` è grande di proposito: se `n` fosse piccolo, il compito si risolverebbe
//! contando a mano e la famiglia insegnerebbe la pazienza, non il principio. Il
//! generatore **non** enumera: enumera i divisori di `m` e applica
//! l'inclusione-esclusione, che è la stessa formula che si chiede allo studente.
//!
//! Il checker è `Numeric` con tolleranza `0`: la risposta è un intero contato
//! due volte, e due conteggi non hanno una distanza.

use kbs_core::{Checker, Instance};
use kbs_verify::{GeneratorKey, InstanceGenerator, ReplayError};
use serde_json::json;

use crate::error::ExerciseError;
use crate::generator::{Generator, replay_instance};
use crate::seed::Rng;

/// Il nome della famiglia, pubblico perché il catalogo lo dichiara: vedi
/// `poly::FAMILY` per la ragione.
pub const FAMILY: &str = "aritmetica-conta-coprimi";
pub const VERSION: &str = "count-coprime/1";
const PROMPT: &str =
    "Conta quanti interi da 1 a n sono coprimi con m, usando l'inclusione-esclusione \
sui fattori primi distinti di m e non un elenco. La risposta è un intero esatto.";
const GUARDIAN: &str =
    "Conta per inclusione-esclusione, non per tentativi: un numero escluso da due \
primi è stato contato due volte, e va aggiunto una volta sola indietro.";

#[derive(Debug, Clone, Copy)]
pub struct CountCoprimes;

impl Generator for CountCoprimes {
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
        let n = rng.range_inclusive(100_000, 9_999_999);
        let m = rng.range_inclusive(2, 997);

        let instance = Instance {
            exercise: self.exercise_id(seed),
            seed: seed.to_string(),
            rendered_prompt: format!(
                "Quanti interi k con 1 <= k <= {n} soddisfano gcd(k, {m}) = 1? \
                 Usa l'inclusione-esclusione sui fattori primi distinti di {m}."
            ),
            expected: coprimes_up_to(n, m).to_string(),
            params: json!({ "n": n, "m": m }),
        };
        Ok((instance, Checker::Numeric { tolerance: 0.0 }))
    }
}

impl InstanceGenerator for CountCoprimes {
    fn version(&self) -> &str {
        self.generator_version()
    }

    fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError> {
        replay_instance(self, key)
    }
}

/// I fattori primi distinti di `m`, in ordine crescente.
fn distinct_primes(mut m: u64) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    let mut d = 2u64;
    while d * d <= m {
        if m.is_multiple_of(d) {
            out.push(d);
            while m.is_multiple_of(d) {
                m /= d;
            }
        }
        d += 1;
    }
    if m > 1 {
        out.push(m);
    }
    out
}

/// Quanti interi da 1 a `n` sono coprimi con `m`.
///
/// Inclusione-esclusione esatta: `Σ_{S ⊆ p} (-1)^{|S|} ⌊n / ∏S⌋`, dove `p` sono
/// i primi distinti di `m`. Con `m <= 997` i primi distinti sono al più quattro,
/// quindi i sottoinsiemi sono al più sedici: il costo non dipende da `n`, che è
/// il punto dell'esercizio.
fn coprimes_up_to(n: u64, m: u64) -> u64 {
    let primes = distinct_primes(m);
    let mut total: i64 = 0;
    for maschera in 0u32..(1u32 << primes.len()) {
        let (mut prod, mut parità) = (1u64, 0i64);
        for (i, p) in primes.iter().enumerate() {
            if maschera & (1 << i) != 0 {
                prod *= p;
                parità += 1;
            }
        }
        let conteggio = if prod == 0 { 0 } else { n / prod };
        if parità % 2 == 0 {
            total += conteggio as i64;
        } else {
            total -= conteggio as i64;
        }
    }
    total.max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_contatore_conta_su_un_esempio_piccolo() {
        // da 1 a 10 i coprimi con 6 sono 1, 5, 7
        assert_eq!(coprimes_up_to(10, 6), 3);
        // da 1 a 10 i coprimi con 7 sono tutti tranne 7
        assert_eq!(coprimes_up_to(10, 7), 9);
        // m primo: n - 1
        assert_eq!(coprimes_up_to(10, 2), 5);
        // m con tre primi distinti: 2*3*5 = 30
        assert_eq!(coprimes_up_to(30, 30), 8);
        assert_eq!(distinct_primes(30), vec![2, 3, 5]);
        assert_eq!(distinct_primes(997), vec![997]);
        // 997 è primo: l'unico non coprimo fino a 9 999 999 sono i suoi multipli.
        assert_eq!(coprimes_up_to(9_999_999, 997), 9_999_999 - 9_999_999 / 997);
        assert_eq!(coprimes_up_to(1, 2), 1);
    }
}
