//! Slug stabile derivato dal testo di un heading.
//!
//! «Stabile» qui ha un significato preciso, ed è la definizione che il
//! repository di ricerca usa per gli artefatti: lo stesso testo di heading
//! produce **sempre** lo stesso id, in qualunque build, su qualunque macchina.
//! Non c'è randomness, non c'è contatore di posizione, non c'è timestamp.
//!
//! La conseguenza è l'unico limite di questa funzione, ed è dichiarato perché
//! un limite non dichiarato è un bug che aspetta: **testi diversi che si
//! normalizzano allo stesso slug collidono**. «Equivoci: 1. X» e
//! «Equivoci — 1. X» producono entrambi `equivoci-1-x`. Il parser non
//! risolve la collisione con un suffisso, perché un suffisso numerico
//! dipenderebbe dalla posizione e quindi non sarebbe più stabile: la
//! collisione viene **riferita** (`crate::parser::HeadingCollision`) e la
//! risolve l'autore scrivendo un `id` esplicito. È la stessa scelta di `kb`, e
//! è l'unica che non produce id che cambiano quando cambia il documento.

/// Accenti e caratteri non latini che compaiono in un titolo italiano, ridotti
/// alla lettera di base.
///
/// Non è normalizzazione Unicode completa (non c'è `unicode-normalization` fra
/// le dipendenze, e introdurla per una funzione di dieci righe sarebbe un peso
/// sproporzionato): è la copertura delle lettere che la scuola italiana scrive.
/// Una lettera fuori elenco passa attraverso invariata e viene poi trattata come
/// separatore, quindi non produce mai uno slug *sbagliato*, produce uno slug
/// *più corto*.
fn base_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' | 'ŝ' | 'ş' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ğ' | 'ĝ' => 'g',
        'ł' => 'l',
        'ð' | 'đ' => 'd',
        'þ' => 't',
        'æ' => 'a',
        'œ' => 'o',
        other => other,
    }
}

/// Lo slug kebab-case di un testo.
///
/// Vuoto se il testo non contiene nulla di slugificabile (solo spazi, solo
/// punteggiatura): il chiamante decide il fallback, perché un id vuoto non è
/// un id.
pub fn kebab(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_sep = false;
    for c in text.chars() {
        let c = base_char(c).to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(c);
        } else {
            // Qualsiasi cosa non alfanumerica è un separatore: accenti già
            // ridotti, spazi, punteggiatura, simboli.
            pending_sep = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::kebab;

    #[test]
    fn slug_determinista_su_testo_italiano_con_accenti() {
        assert_eq!(kebab("Il teorema di Fermat"), "il-teorema-di-fermat");
        assert_eq!(kebab("Perché la continuità è diversa"), "perche-la-continuita-e-diversa");
        assert_eq!(kebab("Angoli e costruzioni"), "angoli-e-costruzioni");
    }

    #[test]
    fn lo_stesso_testo_da_sempre_lo_stesso_id() {
        let t = "Equivoci comuni sul concetto di limite";
        // Confrontare `kebab(t)` con sé stesso non dimostra niente: fallirebbe
        // solo se la funzione non fosse pura. Quello che regge è il valore
        // pinnato qui sotto: è la stabilità dell'id, il criterio del
        // repository di ricerca.
        assert_eq!(kebab(t), "equivoci-comuni-sul-concetto-di-limite");
    }

    #[test]
    fn ripetizioni_di_spazi_e_punteggiatura_non_alterano_lo_slug() {
        assert_eq!(kebab("Il  teorema — di  Fermat!"), "il-teorema-di-fermat");
    }

    #[test]
    fn testo_diverso_produce_slug_diverso() {
        assert_ne!(kebab("Limite superiore"), kebab("Limite inferiore"));
    }

    #[test]
    fn il_testo_vuoto_non_produce_uno_slug() {
        assert_eq!(kebab(""), "");
        assert_eq!(kebab("   —  "), "");
    }
}
