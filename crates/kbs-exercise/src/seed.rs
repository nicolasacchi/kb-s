//! Il seed è una stringa e nient'altro.
//!
//! D11 chiede che data la tupla `(generator_version, seed)` la generazione sia
//! **riproducibile**. La prima conseguenza è che qui non c'è un generatore
//! casuale di sistema: nessun `thread_rng`, nessun orologio, nessun
//! `HashMap` di cui si dipenda dall'ordine di iterazione. `Rng` è splitmix64
//! seminato da FNV-1a del seed — due righe di aritmetica che si possono
//! ricontrollare a mano, e che restano uguali fra versioni di `std` perché non
//! toccano la libreria standard oltre a `wrapping_mul`.
//!
//! Il secondo motivo è politico: se il seed non è abbastanza forte, un docente
//! che lo indovina rigenera l'esercizio di un collega. Qui la forza del seed non
//! protegge un segreto (l'esercizio è del corso, non è un segreto), protegge la
//! **riproducibilità**, e la perdita di indipendenza statistica è dichiarata: il
//! modulo introduce un bias piccolo e costante, che sposta solo la distribuzione
//! dei parametri di un esercizio e non tocca nessuna proprietà di correttezza.

use crate::error::ExerciseError;

/// FNV-1a a 64 bit. Una funzione di hash, non di randomizzazione: serve a
/// trasformare una stringa in uno stato iniziale che non sia tutto zero.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Il generatore di parametri di una famiglia.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Il seme è il seed intero: due istanze dello stesso esercizio hanno lo
    /// stesso seme, quindi gli stessi parametri, quindi la stessa risposta.
    pub fn from_seed(seed: &str) -> Self {
        Rng {
            state: fnv1a64(seed.as_bytes()),
        }
    }

    /// splitmix64. Nessuna dipendenza esterna, nessuna variabile globale.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Un intero in `lo..=hi`. Il modulo è un bias piccolo e **costante**:
    /// sposta la distribuzione dei parametri, non la correttezza di nessun
    /// checker, e resta identico fra esecuzioni.
    pub fn range_inclusive(&mut self, lo: u64, hi: u64) -> u64 {
        debug_assert!(lo <= hi, "range_inclusive richiede lo <= hi");
        let span = hi - lo + 1;
        lo + self.next_u64() % span
    }

    /// `want` valori distinti in `lo..=hi`, in ordine di estrazione.
    ///
    /// Fallisce invece di andare in loop all'infinito quando il range non può
    /// contenerli: un generatore che non può onorare la propria richiesta deve
    /// dirlo, non girare.
    pub fn distinct_inclusive(
        &mut self,
        lo: u64,
        hi: u64,
        want: usize,
    ) -> Result<Vec<u64>, ExerciseError> {
        if lo > hi || (hi - lo + 1) < want as u64 {
            return Err(ExerciseError::DegenerateRange { lo, hi, want });
        }
        let mut out: Vec<u64> = Vec::with_capacity(want);
        let mut guard: u64 = 0;
        // Il limite è una difesa contro il range patologico, non una previsione:
        // con range realistici l'uscita arriva in `want + pochi` estrazioni.
        let cap = want as u64 * 64 + 1024;
        while out.len() < want {
            let v = self.range_inclusive(lo, hi);
            if !out.contains(&v) {
                out.push(v);
            }
            guard += 1;
            if guard > cap {
                return Err(ExerciseError::DegenerateInstance {
                    reason: format!("il range [{lo}, {hi}] non ha prodotto {want} valori distinti in {cap} estrazioni"),
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lo_stesso_seed_da_lo_stesso_stream() {
        let a: Vec<u64> = {
            let mut r = Rng::from_seed("classe-3");
            (0..8).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::from_seed("classe-3");
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_eq!(a, b, "D11: stesso seed, stesso stream");
    }

    #[test]
    fn seed_diversi_diversi_stream() {
        let a: Vec<u64> = {
            let mut r = Rng::from_seed("classe-3");
            (0..8).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::from_seed("classe-4");
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_ne!(a, b);
    }

    #[test]
    fn il_range_sta_dentro_e_copre_gli_estremi() {
        let mut r = Rng::from_seed("estremi");
        let mut min = u64::MAX;
        let mut max = u64::MIN;
        for _ in 0..2000 {
            let v = r.range_inclusive(11, 99);
            assert!((11..=99).contains(&v));
            min = min.min(v);
            max = max.max(v);
        }
        assert!(
            min <= 20 && max >= 90,
            "il range non è una costante: {min}..{max}"
        );
    }

    #[test]
    fn distinti_sono_davvero_distinti() {
        let mut r = Rng::from_seed("opzioni");
        let v = r
            .distinct_inclusive(100, 999, 4)
            .expect("range sufficiente");
        assert_eq!(v.len(), 4);
        for i in 0..v.len() {
            for j in (i + 1)..v.len() {
                assert_ne!(v[i], v[j]);
            }
        }
    }

    /// Un range troppo piccolo è un errore detto, non un loop che non finisce.
    #[test]
    fn un_range_impossibile_si_rifiuta() {
        let mut r = Rng::from_seed("piccolo");
        assert_eq!(
            r.distinct_inclusive(1, 2, 5),
            Err(ExerciseError::DegenerateRange {
                lo: 1,
                hi: 2,
                want: 5
            })
        );
    }
}
