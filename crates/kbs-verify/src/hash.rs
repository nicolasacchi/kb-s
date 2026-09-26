//! I due costruttori della catena di hash (D6) e il tipo [`Hash`].
//!
//! Le formule sono scritte qui una volta sola e sono le uniche: nessun altro
//! modulo del workspace può costruire un hash della catena, quindi non esiste
//! un secondo serializzatore che possa produrre un valore concorrente.
//!
//! * `leaf = SHA256(0x00 ‖ canonical_json(row))`
//! * `node = SHA256(0x01 ‖ left ‖ right)`
//!
//! Il byte iniziale **è** il separatore di dominio. Senza di esso una foglia
//! potrebbe essere riletta come un nodo: `node(l, r)` ha come argomento
//! 64 byte, `leaf(s)` ne ha `1 + |s|`, e le due famiglie si intersecano su
//! input di 64 byte — cioè esattamente sulle stringhe JSON di una riga corta.
//! Il test `separatore_di_dominio` mostra che non si intersecano.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

/// Separatore di dominio delle foglie.
pub const LEAF_TAG: u8 = 0x00;

/// Separatore di dominio dei nodi.
pub const NODE_TAG: u8 = 0x01;

/// `SHA256(0x00 ‖ bytes)`, dove `bytes` è la forma canonica JSON della riga.
pub fn leaf(canonical: &str) -> Hash {
    leaf_bytes(canonical.as_bytes())
}

/// Come [`leaf`], su byte già codificati in UTF-8.
pub fn leaf_bytes(canonical: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update([LEAF_TAG]);
    h.update(canonical);
    Hash(h.finalize().into())
}

/// `SHA256(0x01 ‖ left ‖ right)`.
pub fn node(left: &Hash, right: &Hash) -> Hash {
    let mut h = Sha256::new();
    h.update([NODE_TAG]);
    h.update(left.0);
    h.update(right.0);
    Hash(h.finalize().into())
}

/// Il digest della catena: 32 byte, resi come 64 caratteri esadecimali.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Lunghezza del digest in byte.
    pub const LEN: usize = 32;

    /// Lunghezza della forma esadecimale.
    pub const HEX_LEN: usize = 64;

    pub fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Hash(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; Self::LEN] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(Self::HEX_LEN);
        for b in self.0 {
            s.push(hex_digit(b >> 4));
            s.push(hex_digit(b & 0x0f));
        }
        s
    }
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + (nibble - 10)) as char,
    }
}

fn unhex(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.to_hex())
    }
}

/// Perché un esadecimale non è un [`Hash`]. Il motivo fa parte dell'errore.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HashParseError {
    #[error("hash di {found} caratteri, attesi {expected}: un hash della catena è 32 byte in esadecimale")]
    Length { found: usize, expected: usize },
    #[error("carattere non esadecimale in un hash: {0:?}")]
    NotHex(char),
}

impl FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Hash::from_hex(s)
    }
}

impl Hash {
    pub fn from_hex(s: &str) -> Result<Self, HashParseError> {
        if s.len() != Self::HEX_LEN {
            return Err(HashParseError::Length {
                found: s.len(),
                expected: Self::HEX_LEN,
            });
        }
        let mut bytes = [0u8; Self::LEN];
        let mut half = s.char_indices();
        for byte in bytes.iter_mut() {
            let hi = half.next().map(|(_, c)| c);
            let lo = half.next().map(|(_, c)| c);
            match (hi, lo) {
                (Some(hi), Some(lo)) => match (unhex(hi), unhex(lo)) {
                    (Some(hi), Some(lo)) => *byte = (hi << 4) | lo,
                    _ => return Err(HashParseError::NotHex(hi)),
                },
                (Some(c), None) | (None, Some(c)) => return Err(HashParseError::NotHex(c)),
                (None, None) => unreachable!("la lunghezza è già stata controllata"),
            }
        }
        Ok(Hash(bytes))
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Hash::from_hex(&s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esadecimale_andata_e_ritorno() {
        let h = leaf("{\"a\":1}");
        assert_eq!(h.to_hex().len(), Hash::HEX_LEN);
        assert_eq!(Hash::from_hex(&h.to_hex()).unwrap(), h);
    }

    #[test]
    fn esadecimale_maiuscolo_e_accettato() {
        let h = leaf("{\"a\":1}");
        let upper = h.to_hex().to_uppercase();
        assert_eq!(Hash::from_hex(&upper).unwrap(), h);
    }

    #[test]
    fn esadecimale_troppo_corto_e_troppo_lungo() {
        assert_eq!(
            Hash::from_hex("ab"),
            Err(HashParseError::Length { found: 2, expected: 64 })
        );
        assert!(matches!(
            Hash::from_hex(&"z".repeat(64)),
            Err(HashParseError::NotHex('z'))
        ));
    }

    #[test]
    fn nodo_e_commutativo_per_costruzione_ma_non_simmetrico() {
        let a = leaf("{\"a\":1}");
        let b = leaf("{\"b\":2}");
        assert_ne!(node(&a, &b), node(&b, &a));
    }

    /// Il byte iniziale separa i due costruttori: nessun input del primo
    /// coincide con nessun input del secondo.
    #[test]
    fn separatore_di_dominio() {
        let inputs = [
            "1",
            "null",
            "true",
            "[]",
            "{}",
            "{\"a\":1}",
            "{\"at\":0,\"id\":\"x\"}",
            "{\"id\":\"x\",\"at\":0}",
            "{\"a\":[1,2,3],\"b\":{\"c\":null}}",
        ];
        let leaves: Vec<Hash> = inputs.iter().map(|s| leaf(s)).collect();
        for l in &leaves {
            for r in &leaves {
                assert_ne!(node(l, r), *l, "node ha prodotto una foglia");
            }
            for other in &leaves {
                assert_ne!(node(l, other), *l);
            }
        }
    }

    /// Una stringa JSON di 64 byte produce una foglia il cui input è
    /// `0x00 ‖ 64 byte`: la stessa lunghezza dell'input di un nodo. Il tag
    /// è l'unica cosa che le distingue, e il test lo dimostra invertendo i
    /// byte iniziali.
    #[test]
    fn il_tag_e_l_unica_differenza_su_input_della_stessa_lunghezza() {
        let sixty_four: String = std::iter::repeat('a').take(64).collect();
        let l = leaf(&sixty_four);
        let r = leaf("b");
        let via_nodo = {
            // node(l, r) con il tag 0x01 al posto di 0x00 sul primo byte
            let mut h = Sha256::new();
            h.update([LEAF_TAG]);
            h.update(l.0);
            h.update(r.0);
            Hash(h.finalize().into())
        };
        assert_ne!(node(&l, &r), via_nodo);
    }
}
