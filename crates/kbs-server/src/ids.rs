//! I parsing degli identificatori che arrivano da un URL.
//!
//! `kbs-core` espone i tipi come `String` trasparenti e li *costruisce*, ma non
//! li *riconosce*: `ArgumentId::from_rel_path` dice come si fa un id, non come
//! si verifica che quello che è arrivato nell'URL sia un id. Questo modulo è
//! quella metà, e vive qui perché `kbs-core` è il contratto e non si tocca.
//!
//! Due regole, e sono deliberatamente diverse l'una dall'altra.
//!
//! * **Persona e corso**: qualunque stringa non vuota di `[A-Za-z0-9._-]`, fino
//!   a 64 caratteri. Un id di corso è un dato della scuola e `kb-s` non ne
//!   controlla la forma.
//! * **Argomento**: `arg_` seguito da esattamente 16 caratteri esadecimali
//!   minuscoli, e nient'altro. È l'unica forma che `kbs-core` produce, e la sua
//!   forma è pubblica — `fnv1a16` del percorso sorgente — quindi accettare un
//!   id di argomento di forma libera darebbe a chi chiama un modo per sondare
//!   il database con stringhe arbitrarie. Un id che non ha questa forma **non
//!   esiste**, e la risposta è la stessa di un id che esiste ma non è visibile:
//!   vedi [`crate::error`].
//!
//! Nessuno dei due parsing può produrre un percorso: il charset esclude `/`,
//! `\`, il byte nullo e il punto, e la lunghezza è limitata. Un id non è
//! mai usato per comporre un percorso di filesystem in questo crate — per
//! l'artifact vedi [`crate::sandbox`], dove la rotta del filesystem viene
//! dal database e non dall'URL.

use kbs_core::{ArgumentId, CourseId, PersonId};

/// Charset ammesso in un identificatore: alfanumerici, `-`, `_`, `.`.
///
/// Nessun separatore di percorso, nessun punto percorso (`..`), nessun byte
/// nullo: la lista è negata per elencarla, non per enumerarla.
fn is_safe(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with('.')
        && !s.ends_with('.')
        && !s.contains("..")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Una persona, se la stringa è un id di persona ben formato.
pub fn person(raw: &str) -> Option<PersonId> {
    is_safe(raw).then(|| PersonId(raw.to_string()))
}

/// Un corso, se la stringa è un id di corso ben formato.
pub fn course(raw: &str) -> Option<CourseId> {
    is_safe(raw).then(|| CourseId(raw.to_string()))
}

/// Un argomento, se la stringa è **esattamente** la forma che `kbs-core`
/// produce.
///
/// `arg_` + 16 esadecimali minuscoli. Non si accetta un prefisso diverso, non
/// si accetta una lunghezza diversa, e non si «normalizza»: normalizzare
/// significherebbe avere due modi di scrivere lo stesso id, e il secondo
/// sarebbe quello che qualcuno usa per aggirare una lista.
pub fn argument(raw: &str) -> Option<ArgumentId> {
    let Some(hex) = raw.strip_prefix("arg_") else {
        return None;
    };
    if hex.len() != 16 {
        return None;
    }
    if !hex
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(ArgumentId(raw.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_id_di_persona_e_una_stringa_senza_percorso() {
        assert!(person("person_0001").is_some());
        assert!(person("a.b-c_d").is_some());
        assert!(person("").is_none());
        assert!(person("../../etc/passwd").is_none());
        assert!(person("person 0001").is_none());
        assert!(person("/person_0001").is_none());
        assert!(person(&"p".repeat(65)).is_none());
    }

    #[test]
    fn un_id_di_argomento_ammette_solo_la_forma_di_kbs_core() {
        let reale = ArgumentId::from_rel_path("corsi/analisi-1/lezione-01.html");
        assert_eq!(argument(reale.as_str()), Some(reale));

        assert!(argument("arg_ABCDEF0123456789").is_none(), "maiuscole");
        assert!(argument("arg_0123456789abcde").is_none(), "15 caratteri");
        assert!(argument("arg_0123456789abcdef0").is_none(), "17 caratteri");
        assert!(argument("x_0123456789abcdef").is_none(), "prefisso");
        assert!(argument("arg_0123456789abcdeg").is_none(), "non esadecimale");
    }

    #[test]
    fn nessun_id_puo_comporre_un_percorso() {
        // Non basta che il charset lo impedisca per il percorso: il test
        // elenca i vettori, così il divieto è esercitato e non dichiarato.
        for cattivo in [
            "../segreto",
            "..%2fsegreto",
            "a/../../b",
            "a\\b",
            "a\0b",
            "./a",
        ] {
            assert!(person(cattivo).is_none(), "{cattivo} non è un id");
            assert!(course(cattivo).is_none(), "{cattivo} non è un id");
            assert!(argument(cattivo).is_none(), "{cattivo} non è un id");
        }
    }
}
