//! Il JSON canonico: la forma che la catena di hash firma.
//!
//! Non è una questione di estetica. Due serializzatori che disagree
//! produrrebbero due teste diverse per lo stesso registro, e una catena che
//! non è univoca non è falsificabile. Perciò la canonizzazione è scritta qui,
//! con regole esplicite, ed è essa — non `serde_json::to_string` — a definire
//! la foglia.
//!
//! Le regole:
//!
//! 1. **chiavi in ordine crescente di byte UTF-8**, sempre; l'ordine di
//!    inserimento non conta e non viene conservato;
//! 2. **nessuno spazio**: nessunseparatore, nessuna indentazione;
//! 3. **gli interi restano interi**: `3` non diventa `3.0` e `3.0` non
//!    diventa `3`; la distinzione è ciò che distingue un contatore da una
//!    misura;
//! 4. **`null` è un valore, non un'assenza**: `{"a":null}` e `{}` sono
//!    documenti diversi e hanno foglie diverse;
//! 5. **unicode in chiaro**: i caratteri non-ASCII sono emessi in UTF-8, e
//!    sono fuggiti solo `"`, `\` e i caratteri di controllo;
//! 6. profondità limitata: oltre [`MAX_DEPTH`] l'errore è restituito, e lo
//!    stack non viene sfruttato fino all'abort.

use crate::hash::Hash;
use serde::Serialize;

/// La profondità oltre la quale si restituisce un errore invece di
/// ricorsare. `serde_json` ne limita già il parsing a 128, ma un `Value`
/// costruito a mano può essere più fondo, e un overflow di stack è un abort
/// del processo, non un errore applicativo.
pub const MAX_DEPTH: usize = 64;

/// Perché un valore non ha una forma canonica.
///
/// Il messaggio di `serde_json` è tenuto come testo e non come errore: è ciò
/// che serve a chi sbaglia, e così l'errore resta confrontabile nei test —
/// «quale regola è stata rotta» è una domanda a cui si deve poter rispondere
/// per esattamente.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalError {
    #[error("JSON non valido: {0}")]
    Json(String),
    #[error("valore annidato a profondità {depth}, il limite canonico è {MAX_DEPTH}")]
    TooDeep { depth: usize },
}

impl From<serde_json::Error> for CanonicalError {
    fn from(e: serde_json::Error) -> Self {
        CanonicalError::Json(e.to_string())
    }
}

/// Una destinazione per la scrittura canonica. Due implementazioni: `String`
/// per la forma leggibile, `HashSink` per firmare senza allocare la stringa.
pub trait CanonicalSink {
    /// Testo UTF-8, emesso così com'è.
    fn put_str(&mut self, s: &str);
    /// Un solo byte, che **deve** essere ASCII: è usato solo per la
    /// struttura del documento (`{`, `}`, `:`, `,`, `"`) e per le cifre.
    fn put_ascii(&mut self, b: u8);
}

impl CanonicalSink for String {
    fn put_str(&mut self, s: &str) {
        self.push_str(s);
    }
    fn put_ascii(&mut self, b: u8) {
        debug_assert!(b.is_ascii(), "byte non ASCII nel flusso canonico");
        self.push(b as char);
    }
}

/// Scrive direttamente nello stato interno di SHA-256: la foglia non costa
/// alcuna allocazione.
struct HashSink(sha2::Sha256);

impl HashSink {
    fn finish(self) -> Hash {
        use sha2::Digest;
        Hash::from_bytes(self.0.finalize().into())
    }
}

impl CanonicalSink for HashSink {
    fn put_str(&mut self, s: &str) {
        use sha2::Digest;
        self.0.update(s.as_bytes());
    }
    fn put_ascii(&mut self, b: u8) {
        use sha2::Digest;
        self.0.update([b]);
    }
}

/// La forma canonica di un valore, come testo.
pub fn canonicalize(value: &serde_json::Value) -> Result<String, CanonicalError> {
    let mut out = String::new();
    emit(value, &mut out, 0)?;
    Ok(out)
}

/// La forma canonica di un testo JSON: si analizza e poi si canonizza.
pub fn canonicalize_str(text: &str) -> Result<String, CanonicalError> {
    canonicalize(&serde_json::from_str(text)?)
}

/// La forma canonica di un valore serializzabile, senza materializzare
/// [`serde_json::Value`]: è il testo che finisce in `row_json` nell'export, e
/// deve essere lo stesso che [`leaf_of`] impegna, o la riga e la sua foglia
/// racconterebbero due documenti diversi.
pub fn canonical_of<T: Serialize>(value: &T) -> Result<String, CanonicalError> {
    canonicalize(&serde_json::to_value(value)?)
}

/// La foglia di un valore, senza materializzare la sua forma canonica.
pub fn leaf_of_value(value: &serde_json::Value) -> Result<Hash, CanonicalError> {
    let mut sink = HashSink(<sha2::Sha256 as sha2::Digest>::new());
    sink.put_ascii(crate::hash::LEAF_TAG);
    emit(value, &mut sink, 0)?;
    Ok(sink.finish())
}

/// La foglia di una riga serializzabile: `SHA256(0x00 ‖ canonical_json(row))`.
pub fn leaf_of<T: Serialize>(row: &T) -> Result<Hash, CanonicalError> {
    leaf_of_value(&serde_json::to_value(row)?)
}

fn emit<S: CanonicalSink>(value: &serde_json::Value, out: &mut S, depth: usize) -> Result<(), CanonicalError> {
    match value {
        serde_json::Value::Null => out.put_str("null"),
        serde_json::Value::Bool(true) => out.put_str("true"),
        serde_json::Value::Bool(false) => out.put_str("false"),
        serde_json::Value::Number(n) => emit_number(n, out),
        serde_json::Value::String(s) => emit_string(s, out),
        serde_json::Value::Array(items) => {
            if depth >= MAX_DEPTH {
                return Err(CanonicalError::TooDeep { depth });
            }
            out.put_ascii(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.put_ascii(b',');
                }
                emit(item, out, depth + 1)?;
            }
            out.put_ascii(b']');
        }
        serde_json::Value::Object(map) => {
            if depth >= MAX_DEPTH {
                return Err(CanonicalError::TooDeep { depth });
            }
            // L'ordine è per byte UTF-8 crescente ed è calcolato qui, non
            // ereditato dalla mappa: se un altro crate del workspace attiva
            // `preserve_order` su serde_json, questa riga non cambia.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.put_ascii(b'{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.put_ascii(b',');
                }
                emit_string(key, out);
                out.put_ascii(b':');
                emit(&map[*key], out, depth + 1)?;
            }
            out.put_ascii(b'}');
        }
    }
    Ok(())
}

fn emit_number(n: &serde_json::Number, out: &mut impl CanonicalSink) {
    // `Number` distingue l'intero dal reale e lo rende nel rendering
    // standard (`ryu`, il giro più breve che torna esattamente al valore),
    // che è deterministico sulla stessa piattaforma e su qualunque altra.
    if let Some(u) = n.as_u64() {
        put_u64(u, out);
    } else if let Some(i) = n.as_i64() {
        put_i64(i, out);
    } else {
        out.put_str(&n.to_string());
    }
}

fn put_u64(mut v: u64, out: &mut impl CanonicalSink) {
    let mut buf = [0u8; 20];
    let mut at = buf.len();
    loop {
        at -= 1;
        buf[at] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for b in &buf[at..] {
        out.put_ascii(*b);
    }
}

fn put_i64(v: i64, out: &mut impl CanonicalSink) {
    if v < 0 {
        out.put_ascii(b'-');
        // `-i64::MIN` trabocca: si passa dal valore assoluto come `u64`
        // negate, che è definito anche per il minimo.
        put_u64(v.unsigned_abs(), out);
    } else {
        put_u64(v as u64, out);
    }
}

fn emit_string(s: &str, out: &mut impl CanonicalSink) {
    out.put_ascii(b'"');
    for c in s.chars() {
        match c {
            '"' => out.put_str("\\\""),
            '\\' => out.put_str("\\\\"),
            c if (c as u32) < 0x20 => {
                // Tutti i controlli sono sotto U+0100: quattro cifre esadecimali,
                // le prime due sempre `0`. Nessuna forma breve (`\n`, `\t`):
                // una sola regola è una regola che due implementazioni
                // implementano uguale.
                let n = c as u32;
                out.put_str("\\u00");
                out.put_ascii(hex_digit((n >> 4) as u8));
                out.put_ascii(hex_digit((n & 0x0f) as u8));
            }
            c => out.put_str(c.encode_utf8(&mut [0u8; 4])),
        }
    }
    out.put_ascii(b'"');
}

fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'a' + (nibble - 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::leaf_bytes;
    use serde_json::json;

    fn c(text: &str) -> String {
        canonicalize_str(text).unwrap()
    }

    #[test]
    fn ordine_delle_chiavi_e_fisso() {
        assert_eq!(c(r#"{"b":1,"a":2}"#), r#"{"a":2,"b":1}"#);
        assert_eq!(c(r#"{"c":1,"a":2,"b":3}"#), r#"{"a":2,"b":3,"c":1}"#);
        // ordine di byte UTF-8, non ordine alfabetico di lettere
        assert_eq!(c(r#"{"à":1,"a":2}"#), r#"{"a":2,"à":1}"#);
        assert_eq!(c(r#"{"Z":1,"a":2}"#), r#"{"Z":1,"a":2}"#);
    }

    #[test]
    fn nessuno_spazio_incorreo() {
        assert_eq!(c("{ \"a\" : [ 1 , 2 ] }"), r#"{"a":[1,2]}"#);
        assert_eq!(c("{\n\t\"a\":1\n}"), r#"{"a":1}"#);
    }

    #[test]
    fn interi_restano_interi() {
        assert_eq!(c("3"), "3");
        assert_eq!(c("3.0"), "3.0");
        // `-0` è un reale per serde_json: la forma canonica lo dice, e dice
        // anche il segno, perché `-0.0` e `0.0` sono documenti diversi.
        assert_eq!(c("-0"), "-0.0");
        assert_eq!(c("-0.0"), "-0.0");
        assert_eq!(c("1e2"), "100.0");
        assert_eq!(c(&i64::MIN.to_string()), i64::MIN.to_string());
        assert_eq!(c(&u64::MAX.to_string()), u64::MAX.to_string());
        assert_eq!(c(&format!("{}", i64::MIN)), "-9223372036854775808");
    }

    #[test]
    fn intero_e_reale_da_json_diverso_restano_diversi() {
        assert_ne!(canonicalize(&json!(3)).unwrap(), canonicalize(&json!(3.0)).unwrap());
        assert_ne!(
            leaf_of_value(&json!({"n": 3})).unwrap(),
            leaf_of_value(&json!({"n": 3.0})).unwrap()
        );
    }

    #[test]
    fn floating_point_viene_dall_ordine_piu_breve() {
        assert_eq!(c("0.1"), "0.1");
        assert_eq!(c("1.0e-7"), "1e-7");
        assert_eq!(c("100000000000000000000.0"), "1e+20");
    }

    #[test]
    fn null_e_assente_sono_diversi() {
        assert_eq!(c(r#"{"a":null}"#), r#"{"a":null}"#);
        assert_eq!(c("{}"), "{}");
        assert_ne!(c(r#"{"a":null}"#), c("{}"));
        assert_ne!(
            leaf_of_value(&json!({"a": serde_json::Value::Null})).unwrap(),
            leaf_of_value(&json!({})).unwrap()
        );
    }

    #[test]
    fn unicode_in_chiaro_e_compatibile() {
        assert_eq!(c(r#""perché""#), "\"perché\"");
        assert_eq!(c(r#""👩‍🏫""#), "\"👩‍🏫\"");
        assert_eq!(c(r#""a\nb""#), r#""a\u000ab""#);
        assert_eq!(canonicalize(&json!("perché")).unwrap(), "\"perché\"");
    }

    #[test]
    fn caratteri_di_controllo_fuggiti_e_rileggibili() {
        // Il byte di controllo esce come `\u00XX`: sei caratteri di testo, non
        // un byte grezzo. Si verifica sulla lunghezza e sul prefisso per non
        // dover scrivere qui un carattere di controllo.
        let nul = c("\"a\\u0000b\"");
        assert!(nul.starts_with("\"a\\u00"), "fuga del NUL: {nul}");
        assert_eq!(nul.len(), "\"a\\u0000b\"".len());
        let us = c("\"a\\u001fb\"");
        assert!(us.starts_with("\"a\\u001f"), "fuga di U+001F: {us}");
        assert_eq!(us.len(), "\"a\\u001fb\"".len());
        assert_eq!(c("\"a\\nb\""), r#""a\u000ab""#);
        // la forma canonica riletta è identica a sé stessa
        let once = c("\"a\\nb\\tc\\\"d\\\\e\"");
        assert_eq!(c(&once), once);
    }

    #[test]
    fn oggetti_annidati_ordinati_a_ogni_livello() {
        assert_eq!(
            c(r#"{"b":{"z":1,"y":2},"a":[{"q":1,"p":2}]}"#),
            r#"{"a":[{"p":2,"q":1}],"b":{"y":2,"z":1}}"#
        );
    }

    #[test]
    fn array_ordinato_per_posizione_non_per_valore() {
        assert_eq!(c("[3,1,2]"), "[3,1,2]");
        assert_ne!(c("[1,2,3]"), c("[3,2,1]"));
    }

    #[test]
    fn chiave_ripetuta_ultima_vale_e_il_documento_e_un_valore() {
        // `serde_json` non rappresenta chiavi duplicate: l'inserimento
        // sostituisce. La canonizzazione è definita sul valore, e su un valore
        // la forma è univoca — quindi non c'è una seconda risposta possibile.
        assert_eq!(c(r#"{"a":1,"a":2}"#), r#"{"a":2}"#);
    }

    #[test]
    fn profondita_e_limitata_e_nessun_overflow() {
        let mut v = json!(1);
        for _ in 0..(MAX_DEPTH + 8) {
            v = json!([v]);
        }
        assert!(matches!(
            leaf_of_value(&v),
            Err(CanonicalError::TooDeep { .. })
        ));
    }

    #[test]
    fn json_non_valido_e_un_errore() {
        assert!(matches!(canonicalize_str("{"), Err(CanonicalError::Json(_))));
    }

    #[test]
    fn la_foglia_e_la_firma_del_documento_canonico() {
        let a = leaf_of_value(&json!({"a": 1, "b": 2})).unwrap();
        let b = leaf_bytes(br#"{"a":1,"b":2}"#);
        assert_eq!(a, b);
    }

}
