//! `no_solution_leak` — la prova obbligatoria di D8 e di D15.1.
//!
//! # Che cosa significa «rivelare»
//!
//! La domanda «il GUARDIAN contiene la soluzione?» non ha una risposta ovvia, e
//! sceglierla male è il modo tipico per costruire una prova che non prova
//! niente. La scelta di questo crate è una sola, ed è **relativa al checker**:
//!
//! > Un testo **rivelala** se contiene, in posizione contigua, la sequenza di
//! > token di **una qualsiasi forma superficiale che il checker accetterebbe
//! > come risposta corretta**.
//!
//! Tutto il resto non è una rivelazione. Un GUARDIAN che scrive `42,51` per un
//! esercizio la cui risposta è `42,5` con tolleranza `0,0005` **non** rivela: il
//! checker rifiuta quella risposta, quindi lo studente che copia il GUARDIAN
//! sbaglia. Un GUARDIAN che scrive «l'opzione 2» quando la risposta è l'opzione 2
//! **rivela**, perché l'indice è una forma che il checker accetterebbe.
//!
//! La scelta è motivata da due fatti insieme. Da un lato è **verificabile**: la
//! forma corretta è quella che `check::grade` accetterebbe, e quel codice è in
//! questo crate, in poche righe, quindi la definizione non può divergere da
//! quello che il grader fa davvero. Dall'altro è **non bloccante**: se
//! «rivelasse» significasse «contenga la risposta, o qualcosa da cui la risposta
//! si legga», la prova diventerebbe un'opinione sulla formattazione: ogni
//! GUARDIAN che ripete un parametro sarebbe rosso, e un gate che rosina sempre è
//! un gate che nessuno guarda.
//!
//! # I due falsi negativi dichiarati
//!
//! 1. **La rivelazione per derivazione.** Un GUARDIAN che dice «usa la formula di
//!    Eulero e arrotonda a due cifre» non contiene nessuna forma corretta, e pure
//!    basta a far trovare la risposta. Nessuna prova sul testo lo intercetta: il
//!    testo della regola non contiene il risultato della regola. Lo contiene
//!    solo il fatto che D7 tiene il GUARDIAN entro 640 byte, e un vincolo di
//!    lunghezza è un limite di volume, non di contenuto.
//! 2. **La forma non accettata.** Un GUARDIAN che scrive `6 × 2132` per un
//!    esercizio di forma normale `2132x^2+...` non contiene nessuna forma che il
//!    checker accetterebbe, e pure consegna il coefficiente.
//! 3. **La resa abbreviata di un numero.** Se l'istanza dichiara `42.5000` e il
//!    GUARDIAN scrive `42,5`, la prova non interviene: `42,5` non è la forma
//!    dichiarata, e non c'è modo onesto di distinguerla da un GUARDIAN che
//!    abbrevia un numero per disattenzione. Il GUARDIAN che intende essere
//!    sospettoso mette la risposta per esteso — e un GUARDIAN di 640 byte che
//!    scrive un numero per esteso è, per l'occhio, un GUARDIAN che lo sta dicendo.
//!
//! Sono dichiarati perché una prova con falsi negativi noti è onesta, e una
//! prova che non li ha è soltanto non provata.
//!
//! # Perché il solo GUARDIAN
//!
//! Il testo sottoposto a prova è il GUARDIAN, che è l'unico testo libero che
//! l'unità porta. Il prompt dell'istanza è generato dai parametri e **non** può
//! essere escluso: in una scelta multipla le opzioni sono per definizione nel
//! prompt, e una delle quattro è la risposta. Includerlo renderebbe la prova
//! sempre rossa, e una prova sempre rossa è una prova spenta.

use kbs_core::{Checker, Instance};
use thiserror::Error;

use crate::check::validate_checker;
use crate::error::ExerciseError;

/// Il GUARDIAN rivela la risposta, oppure non è controllabile.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Leak {
    #[error("il contratto rivela la risposta: la forma {form:?} compare in «{snippet}» e il checker la accetterebbe come corretta")]
    Reveals { form: String, snippet: String },

    #[error("il GUARDIAN non è controllabile: {0}")]
    Uncheckable(#[from] ExerciseError),
}

/// Le forme superficiali che il checker accetterebbe come risposta corretta.
///
/// Elenco per variante, e ogni voce è una scelta dichiarata:
/// * `Numeric`: la resa dichiarata dall'istanza, e nient'altro. Non le
///   approssimazioni che la tolleranza accetterebbe (`42,5001` per `42,5`), che
///   non sono «la risposta»: sono la risposta sbagliata di chi arrotonda.
/// * `Set`: ogni elemento, e l'elenco per intero nella sua resa canonica.
/// * `MultipleChoice`: il testo dell'opzione esatta **e** il suo indice, perché il
///   checker accetta entrambi come risposta.
/// * `Equivalence`: la forma normale dichiarata. La normalizzazione del checker
///   (spazi, parentesi) non produce altre forme qui: le spaziature spariscono già
///   nella tokenizzazione, ed è il motivo per cui il GUARDIAN resta liberissimo
///   di scrivere `2 ( x + 3 )`.
pub fn correct_surfaces(
    instance: &Instance,
    checker: &Checker,
) -> Result<Vec<String>, ExerciseError> {
    validate_checker(checker)?;
    let mut out: Vec<String> = Vec::new();
    match checker {
        Checker::Numeric { .. } => {
            out.push(instance.expected.clone());
        }
        Checker::Set { elements } => {
            out.push(elements.join(", "));
            for e in elements {
                out.push(e.clone());
            }
        }
        Checker::MultipleChoice {
            correct_index,
            options,
        } => {
            let idx = *correct_index as usize;
            match options.get(idx) {
                Some(o) => out.push(o.clone()),
                // `validate_checker` ha già escluso questo caso: si lascia comunque
                // un valore, e `grade` lo rifiuterà prima di arrivare qui.
                None => out.push(String::new()),
            }
            out.push(idx.to_string());
        }
        Checker::Equivalence { normalized } => {
            out.push(normalized.clone());
        }
    }
    out.retain(|f| !tokens(f).is_empty());
    Ok(out)
}

/// La prova.
///
/// `Ok(())` = l'unità è pubblicabile. `Err(Leak::Reveals { .. })` = l'unità non
/// pubblica (D8), e il messaggio dice **quale** forma e **dove**, perché un
/// gate che non dice che cosa ha trovato costringe il docente a indovinare.
pub fn no_solution_leak(
    guardian: &str,
    instance: &Instance,
    checker: &Checker,
) -> Result<(), Leak> {
    let text = tokens(guardian);
    for form in correct_surfaces(instance, checker)? {
        let seq = tokens(&form);
        if seq.is_empty() {
            continue;
        }
        if let Some(at) = find_subsequence(&text, &seq) {
            return Err(Leak::Reveals {
                form,
                snippet: text[at..at + seq.len()].join(" "),
            });
        }
    }
    Ok(())
}

/// Posizione della prima occorrenza contigua, se c'è.
fn find_subsequence(haystack: &[String], needle: &[String]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=(haystack.len() - needle.len())).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// Ripiega il testo: minuscole e vocali accentate italiane senza accento.
///
/// Il folding serve a non dipendere dalla calligrafia di chi scrive il
/// GUARDIAN. Non è un folding Unicode completo e non vuole esserlo: se
/// l'inventario dei caratteri si allarga, il GUARDIAN che contiene la risposta in
/// caratteri esotici smette di essere rilevato — ed è un falso negativo che il
/// messaggio d'errore rende comunque ispezionabile a mano.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let lower: Vec<char> = c.to_lowercase().collect();
        for l in lower {
            out.push(match l {
                'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
                'è' | 'é' | 'ê' | 'ë' => 'e',
                'ì' | 'í' | 'î' | 'ï' => 'i',
                'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
                'ù' | 'ú' | 'û' | 'ü' => 'u',
                'ç' => 'c',
                'ñ' => 'n',
                other => other,
            });
        }
    }
    out
}

/// I token: sequenze massimali di caratteri alfanumerici del testo ripiegato.
///
/// `42,5` → `["42", "5"]`. I due punti di confine sono ciò che rende il confronto
/// **a confine di token**: «142» non contiene «42», che è la differenza fra una
/// prova e un `contains` che non prova niente.
pub fn tokens(s: &str) -> Vec<String> {
    let folded = fold(s);
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in folded.chars() {
        if c.is_alphanumeric() {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn inst(expected: &str) -> Instance {
        Instance {
            exercise: "e".into(),
            seed: "s".into(),
            rendered_prompt: "p".into(),
            expected: expected.into(),
            params: json!({}),
        }
    }

    #[test]
    fn il_folding_toglie_gli_accenti_e_le_maiuscole() {
        assert_eq!(tokens("Perché «Così»?"), vec!["perche", "cosi"]);
        assert_eq!(tokens("42,5"), vec!["42", "5"]);
        assert_eq!(tokens("x²"), vec!["x²"]);
    }

    #[test]
    fn il_confronto_e_a_confine_di_token() {
        assert_eq!(
            find_subsequence(&tokens("il totale e 1420"), &tokens("42")),
            None
        );
        assert_eq!(
            find_subsequence(&tokens("il totale e 42 giri"), &tokens("42")),
            Some(3)
        );
    }

    #[test]
    fn una_forma_in_medio_al_testo_e_un_rilevamento() {
        let c = Checker::Numeric { tolerance: 0.0005 };
        let e = no_solution_leak(
            "Ricorda: la risposta è 42.5000 e va riportata così com'è.",
            &inst("42.5000"),
            &c,
        );
        match e {
            Err(Leak::Reveals { form, snippet }) => {
                assert_eq!(form, "42.5000");
                assert_eq!(snippet, "42 5000");
            }
            altro => panic!("attesa una rivelazione, ottenuto {altro:?}"),
        }
    }

    /// La virgola decimale non è un trucco: `42,5` e `42.5000` si tokenizzano
    /// allo stesso modo, e il GUARDIAN può scrivere come vuole senza cambiare
    /// l'esito.
    #[test]
    fn la_virgola_decimale_non_conta() {
        let c = Checker::Numeric { tolerance: 0.0005 };
        assert!(matches!(
            no_solution_leak("La risposta è 42,5000.", &inst("42.5000"), &c),
            Err(Leak::Reveals { .. })
        ));
    }

    /// Il quasi-miss: una risposta che il checker **rifiuta** non è una
    /// rivelazione. È la decisione dichiarata, e questo test è quello che
    /// fallirebbe se qualcuno tornasse a una definizione più grossolana.
    ///
    /// Nota il caso `42,5` per `42.5000`: la resa abbreviata di un numero non è
    /// una forma, perché un GUARDIAN che scrive `42,5` sta scrivendo una
    /// resa diversa da quella dichiarata, e la prova non può distinguerla da una
    /// svista. È il terzo falso negativo, insieme agli altri due.
    #[test]
    fn un_quasi_miss_non_e_una_rivelazione() {
        let c = Checker::Numeric { tolerance: 0.0005 };
        for testo in [
            "Il risultato è 42,51.",
            "Il risultato è 43.",
            "Il risultato è 142.",
            "Il risultato è 42,5.",
            "Il risultato è 425.",
        ] {
            assert!(
                no_solution_leak(testo, &inst("42.5000"), &c).is_ok(),
                "{testo:?} non è la forma dichiarata"
            );
        }
    }

    #[test]
    fn l_indice_dell_opzione_esatta_è_una_forma() {
        let c = Checker::MultipleChoice {
            correct_index: 2,
            options: vec!["3/4".into(), "5/8".into(), "7/8".into(), "9/10".into()],
        };
        let i = inst("7/8");
        assert!(no_solution_leak("Tra le opzioni, scegli la numero due.", &i, &c).is_ok());
        let r = no_solution_leak("Tra le opzioni, scegli la 2.", &i, &c);
        assert!(matches!(r, Err(Leak::Reveals { .. })), "{r:?}");
    }

    #[test]
    fn gli_elementi_di_un_insieme_sono_forme() {
        let c = Checker::Set {
            elements: vec!["12".into(), "19".into()],
        };
        let i = inst("12, 19");
        assert!(no_solution_leak("Elenca ciò che in A non compare in B.", &i, &c).is_ok());
        assert!(matches!(
            no_solution_leak("Gli elementi sono 12 e 19.", &i, &c),
            Err(Leak::Reveals { .. })
        ));
    }

    #[test]
    fn la_forma_normale_con_parentesi_e_spazi_e_la_stessa_forma() {
        let c = Checker::Equivalence {
            normalized: "2*(x+3)".into(),
        };
        let i = inst("2*(x+3)");
        assert!(no_solution_leak("Attenzione alle parentesi esterne.", &i, &c).is_ok());
        assert!(matches!(
            no_solution_leak("La risposta è 2*(x+3).", &i, &c),
            Err(Leak::Reveals { .. })
        ));
    }

    #[test]
    fn un_checker_rotto_rende_la_prova_non_controllabile() {
        let c = Checker::MultipleChoice {
            correct_index: 9,
            options: vec!["a".into()],
        };
        assert!(matches!(
            no_solution_leak("testo", &inst("a"), &c),
            Err(Leak::Uncheckable(ExerciseError::CorrectIndexOutOfRange {
                index: 9,
                len: 1
            }))
        ));
    }
}
