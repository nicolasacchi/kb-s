//! La piegatura del testo, e che cosa costa cercare in italiano con `unicode61`.
//!
//! # Che cosa è stato misurato
//!
//! I punti qui sotto non sono opinioni: sono il comportamento del tokenizzatore
//! FTS5 `unicode61` della build SQLite che `kbs-store` linka, verificato in
//! `src/tests/italian.rs` contro l'indice reale. Se la build cambia, i test
//! cambiano esito e la dichiarazione qui sotto smette di essere vera: è il punto.
//!
//! 1. `remove_diacritics 2` piega `perché` in `perche`. Cercare `perche` trova
//!    `Perché`, e cercare `PERCHÉ` lo trova. **Questo funziona, e conviene dirlo.**
//! 2. **L'apostrofo è un errore di sintassi.** `l'acqua` non è «nessun
//!    risultato»: è `fts5: syntax error near "'"`. L'elisione italiana è la
//!    forma più comune di questa parola, e il caso più comune di errore.
//! 3. **Il trattino è un filtro di colonna.** `least-squares` → `no such column:
//!    squares`; `1-10` → `no such column: 10`. Ogni termine tecnico composto con
//!    un trattino è un errore, non una ricerca.
//! 4. **I due punti sono un filtro di colonna.** `scala: 1-10` → `no such column:
//!    scala`.
//! 5. **La virgola e il punto sono errori di sintassi.** `3,14` — che è il
//!    formato decimale italiano — non si cerca. `3.14` nemmeno.
//! 6. **Le legature e le lettere barrate sono caratteri di token che non vengono
//!    piegati.** `ł`, `ø`, `ß`, `æ`, `œ` sopravvivono come token distinti:
//!    un documento che contiene `Łukasz` **non** è raggiungibile da `lukasz`.
//!    In un corpus universitario i cognomi polacchi, romeni, croati e tedeschi
//!    non sono un'ipotesi.
//! 7. **Non c'è radice.** `chiavi` non trova `chiave`, `equazioni` non trova
//!    `equazione`. Lo `stemmer` inglese `porter`, misurato sulle stesse parole,
//!    non aiuta: è un algoritmo inglese applicato a una lingua che non è
//!    l'inglese, e può solo togliere informazione. Perciò non è abilitato.
//! 8. **Una sequenza di parole è un AND implicito, in qualunque ordine.**
//!    Misurato: `personali dati` trova un documento che contiene `dati riservati
//!    personali`, cioè parole lontane e in ordine inverso. Quindi la casella di
//!    ricerca non deve trasformare `dati personali` in una ricerca di frase: qui
//!    [`to_fts_query`] mette `AND` esplicito, che è la stessa cosa resa leggibile
//!    — non un correttivo, e va detto che non lo è.
//!
//! # Che cosa fa [`fold`]
//!
//! Minuscolo, base senza diacritici, legature e lettere barrate sciolte, e
//! **ogni carattere che non è una lettera o una cifra diventa uno spazio**.
//!
//! L'ultima regola è quella che rende sicura la costruzione della query: dopo
//! `fold`, l'unico carattere che FTS5 tratiene come metacarattere dentro una
//! stringa è `"`, e `fold` lo ha già trasformato in spazio. Ogni termine viene
//! comunque racchiuso fra virgolette, quindi la difesa è doppia e verificabile da
//! un test che passa input volutamente ostili.
//!
//! La piegatura viene applicata **sia al testo che si indicizza sia alla query**:
//! è il motivo per cui i due lati coincidono per costruzione e non per speranza.
//!
//! # I costi, dichiarati
//!
//! * La piegatura è many-to-one: `più` e `piu`, `da` e `dà` diventano lo stesso
//!   token. Il recall sale e la precisione scende. È il prezzo di una ricerca che
//!   accetta che il docente scriva `perche` invece di `perché`.
//! * Non c'è analisi morfologica italiana: nessuna radice, nessuna desinenza. Chi
//!   cerca deve pensare nella forma superficiale del materiale.
//! * L'apostrofo non è un errore **dopo** `fold`: `dell'acqua` diventa
//!   `dell acqua` e `"dell" AND "acqua"` lo trova. Non però `l'acqua` in un
//!   documento che scrive `dell'acqua`: l'elisione allunga la frase e la frase
//!   esatta non combacia. È il limite più irritante e nessuna riga di codice lo
//!   risolve: servirebbe un dizionario delle elisioni, che è un'altra
//!   dipendenza e un altro progetto.
//! * Non ci sono operatori: niente esclusione (`-termine`), niente prefissi. La
//!   ricerca è un filtro di visibilità, non un linguaggio.

/// Piega un testo per l'indice e per la query.
///
/// Vedi il doc del modulo per la misura dei limiti. La funzione è **totale**:
/// nessun input la fa fallire, perché il percorso di indicizzazione non può
/// avere un caso in cui un carattere impedisce di salvare l'artifact.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        for c in ch.to_lowercase() {
            push_folded(c, &mut out);
        }
    }
    // Gli spazi si accorpano qui e non durante la scansione: `fold` non ha
    // bisogno di sapere se il carattere precedente era già uno spazio.
    let mut collapsed = String::with_capacity(out.len());
    for token in out.split_ascii_whitespace() {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(token);
    }
    collapsed
}

/// Accoda `c` piegato, o uno spazio se non è una lettera né una cifra.
fn push_folded(c: char, out: &mut String) {
    if c.is_ascii_digit() {
        out.push(c);
        return;
    }
    // Il caso comune: ASCII minuscolo. Ci sta per primo perché è il 95% del
    // testo e una `match` su intero non lo metterebbe in cima.
    if c.is_ascii_lowercase() {
        out.push(c);
        return;
    }
    if c.is_ascii_uppercase() {
        out.push(c.to_ascii_lowercase());
        return;
    }
    match c {
        // ── vocali e consonanti accentate ────────────────────────────────────
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
        'ç' | 'ĉ' | 'ċ' | 'č' => out.push('c'),
        'ð' | 'đ' | 'ď' => out.push('d'),
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ə' => out.push('e'),
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => out.push('g'),
        'ĥ' | 'ħ' => out.push('h'),
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => out.push('i'),
        'ĵ' => out.push('j'),
        'ķ' | 'ĸ' => out.push('k'),
        'ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' => out.push('l'),
        'ñ' | 'ń' | 'ņ' | 'ň' => out.push('n'),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => out.push('o'),
        'ŕ' | 'ŗ' => out.push('r'),
        'ś' | 'ş' | 'š' => out.push('s'),
        'þ' | 'ţ' | 'ť' | 'ŧ' => out.push('t'),
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => out.push('u'),
        'ŵ' => out.push('w'),
        'ý' | 'ÿ' | 'ŷ' | 'ź' | 'ż' | 'ž' => out.push('z'),
        // ── legature e lettere barrate: token distinti per `unicode61`, quindi
        //    irraggiungibili dalla loro forma ASCII se non le sciogliamo qui ──
        'æ' => out.push_str("ae"),
        'œ' => out.push_str("oe"),
        'ß' => out.push_str("ss"),
        'ŋ' => out.push_str("ng"),
        'ĳ' => out.push_str("ij"),
        'ſ' => out.push('s'),
        'ﬀ' => out.push_str("ff"),
        'ﬁ' => out.push_str("fi"),
        'ﬂ' => out.push_str("fl"),
        'ﬃ' => out.push_str("ffi"),
        'ﬄ' => out.push_str("ffl"),
        'ﬅ' | 'ﬆ' => out.push_str("st"),
        // ── tutto il resto ───────────────────────────────────────────────────
        // Alfabeti non latini (greco, cirillico, arabo, cinese) passano: sono
        // già forme canoniche e piegarli sarebbe inventare una normalizzazione
        // che nessuno ha scritto.
        c if c.is_alphanumeric() => out.push(c),
        // Punteggiatura, spazi, apici, trattini, due punti, virgole, punti,
        // parentesi, stelle: tutto ciò che FTS5 usa come sintassi, qui diventa
        // uno spazio.
        _ => out.push(' '),
    }
}

/// Costruisce la query FTS5 da ciò che una persona ha scritto.
///
/// Restituisce `None` quando non c'è nessun termine cercabile: una query FTS5
/// vuota è un errore di sintassi, e «nessun risultato» è una risposta vera.
///
/// Ogni termine è racchiuso fra virgolette, e i termini sono uniti da `AND`
/// esplicito. Le virgolette sono la difesa: rendono inerti i metacaratteri,
/// anche se `fold` li ha già tolti. L'`AND` esplicito non è un correttivo —
/// `unicode61` mette già un AND implicito — ma rende la query leggibile in un
/// registro e immune a un cambio di default.
pub fn to_fts_query(text: &str) -> Option<String> {
    let folded = fold(text);
    let terms: Vec<String> = folded
        .split_ascii_whitespace()
        .map(|t| format!("\"{t}\""))
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(terms.join(" AND "))
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn fold_accents_apostrophes_and_ligatures() {
        assert_eq!(fold("Perché l'acqua"), "perche l acqua");
        assert_eq!(fold("Łukasz"), "lukasz");
        assert_eq!(fold("straße"), "strasse");
        assert_eq!(fold("lGoals Æsop Œuvre"), "lgoals aesop oeuvre");
    }

    #[test]
    fn fold_never_leaves_punctuation() {
        let hostile = "scala: 1-10 (a[or])\"b\"* ^x ~y ;z 3,14 3.14 -- ++ \\n\t";
        let folded = fold(hostile);
        assert!(
            folded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == ' '),
            "la piegatura ha lasciato passare {folded:?}"
        );
    }

    #[test]
    fn query_has_no_fts_metacharacter() {
        let q = to_fts_query("scala: 1-10 -acqua NEAR(x) \"y\"").expect("termini");
        assert_eq!(
            q,
            "\"scala\" AND \"1\" AND \"10\" AND \"acqua\" AND \"near\" AND \"x\" AND \"y\""
        );
    }

    #[test]
    fn query_of_punctuation_only_is_none() {
        assert!(to_fts_query("   ,,.. ***   ").is_none());
    }
}
