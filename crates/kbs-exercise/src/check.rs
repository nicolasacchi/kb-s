//! Il lato di grading di `kbs_core::Checker`.
//!
//! # Un checker confronta, non giudica
//!
//! Tutto quello che c'è in questo file accetta una risposta e ne confronta la
//! **forma** con una dichiarazione. Non c'è un punteggio, non c'è una soglia
//! «abbastanza giusta», non c'è un ordine di grandezza. Il verdetto è uno dei
//! due, e la differenza fra i due è fatta da un confronto che il docente può
//! leggere e ricontare a mano.
//!
//! # `Equivalence` non è un parser di espressioni
//!
//! Per `Equivalence` il confronto è su una **forma normale dichiarata dal
//! generatore**, e la normalizzazione applicata alla risposta è volutamente la
//! più piccola che renda sensato il termine «equivalente»: spaziature e
//! parentesi esterne bilanciate. Nessun parser, nessun albero, nessuna
//! semplificazione.
//!
//! Il motivo è scritto anche in `kbs_core`: un parser che decide le risposte è
//! il modo più rapido per costruire un sistema che sbaglia con sicurezza, perché
//! il suo criterio di accettazione smette di essere dichiarabile in un
//! contratto che lo studente può leggere. Qui la pretesa è verificabile in una
//! riga: [`normalize_equivalence`] è l'intera normalizzazione, e il test
//! `equivalenza_non_e_algebra` mostra che `2*x+3` è **rifiutato** contro la
//! forma normale `2*(x+3)`, benché i due valgano uguale per ogni `x`.
//!
//! # Una risposta non è mai un errore
//!
//! [`grade`] restituisce `Err` solo quando **l'esercizio** è rotto: indice
//! dell'esatto fuori dal vettore delle opzioni, opzioni duplicate, tolleranza non
//! confrontabile, istanza non legata al checker presentato. Qualunque stringa
//! dello studente — anche «quarantadue» su un checker numerico, anche «17/8» su
//! una scelta multipla — riceve un verdetto. Questo è il motivo per cui
//! [`ExerciseError`] non ha varianti «risposta illeggibile»: chi registra i voti
//! deve poter sapere che cosa fa di una risposta non valutabile senza leggerlo
//! in un messaggio d'errore.

use kbs_core::{Checker, Exercise, Instance};

use crate::error::ExerciseError;

/// Il verdetto. Due soli, e nessuno dei due è «più o meno giusto».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Correct,
    Incorrect,
}

impl Verdict {
    pub fn is_correct(self) -> bool {
        matches!(self, Verdict::Correct)
    }
}

/// Verifica che il checker sia confrontabile.
///
/// È il gate D8 in forma di codice: un esercizio pubblicabile ha un checker
/// *che esiste davvero*, non un enum dichiarato. Un checker che su ogni
/// risposta restituirebbe `Err` non sarebbe un esercizio verificabile, sarebbe
/// un esercizio muto.
pub fn validate_checker(checker: &Checker) -> Result<(), ExerciseError> {
    match checker {
        Checker::Numeric { tolerance } => {
            if !tolerance.is_finite() || *tolerance < 0.0 {
                return Err(ExerciseError::BadTolerance {
                    tolerance: *tolerance,
                });
            }
            Ok(())
        }
        Checker::Set { elements } => {
            if elements.is_empty() {
                return Err(ExerciseError::EmptySet);
            }
            for e in elements {
                if e.trim().is_empty() {
                    return Err(ExerciseError::EmptySetElement);
                }
            }
            for (i, e) in elements.iter().enumerate() {
                if elements[..i].iter().any(|p| p.trim() == e.trim()) {
                    return Err(ExerciseError::DuplicateSetElement {
                        element: e.trim().into(),
                    });
                }
            }
            Ok(())
        }
        Checker::MultipleChoice {
            correct_index,
            options,
        } => {
            if options.is_empty() {
                return Err(ExerciseError::NoOptions);
            }
            for (i, o) in options.iter().enumerate() {
                if o.trim().is_empty() {
                    return Err(ExerciseError::EmptyOption);
                }
                if options[..i].iter().any(|p| p.trim() == o.trim()) {
                    return Err(ExerciseError::DuplicateOption {
                        option: o.trim().into(),
                    });
                }
            }
            if (*correct_index as usize) >= options.len() {
                return Err(ExerciseError::CorrectIndexOutOfRange {
                    index: *correct_index,
                    len: options.len(),
                });
            }
            Ok(())
        }
        Checker::Equivalence { normalized } => {
            if normalized.trim().is_empty() {
                return Err(ExerciseError::EmptyNormalForm);
            }
            Ok(())
        }
    }
}

/// Il gate di pubblicazione di D8.
///
/// «Un esercizio senza verificatore deterministico non entra nel percorso di
/// pubblicazione.» In `kbs_core` il campo è un `Checker` e non un
/// `Option<Checker>`, quindi l'assenza è già esclusa dal tipo: qui si esclude il
/// caso residuo, cioè il checker *presente e rotto*.
pub fn may_publish(exercise: &Exercise) -> Result<(), ExerciseError> {
    validate_checker(&exercise.checker)
}

/// Verifica che l'istanza e il checker appartengano alla stessa generazione.
///
/// Un checker da solo non porta la risposta: la risposta sta nell'istanza, la
/// forma sta nel checker. Presentare l'istanza di `seed=a` con il checker di
/// `seed=b` non dà un voto sbagliato, dà un errore — perché senza questo
/// controllo un errore di plumbing diventerebbe indistinguibile da uno studente
/// che ha sbagliato.
///
/// Il legame è esatto per `Numeric`, `Set` ed `Equivalence`. Per
/// `MultipleChoice` è il legame più forte che il tipo consente — l'opzione che
/// il checker dichiara esatta deve essere la risposta dell'istanza — e non
/// esclude che due esercizi diversi abbiano per caso la stessa lista di opzioni.
pub fn is_bound_to(instance: &Instance, checker: &Checker) -> Result<(), ExerciseError> {
    match checker {
        Checker::Numeric { .. } => {
            if parse_number(&instance.expected).is_some() {
                Ok(())
            } else {
                Err(ExerciseError::UnboundInstance {
                    reason: format!(
                        "l'istanza dichiara la risposta {:?}, che non è un numero finito: \
                         il checker presentato non è quello di questa istanza",
                        instance.expected
                    ),
                })
            }
        }
        Checker::Set { elements } => {
            let declared: Vec<&str> = elements.iter().map(|e| e.trim()).collect();
            if parse_answer_set(&instance.expected) == declared {
                Ok(())
            } else {
                Err(ExerciseError::UnboundInstance {
                    reason: format!(
                        "l'istanza dichiara {:?} mentre il checker attende {:?}",
                        instance.expected,
                        elements.join(", ")
                    ),
                })
            }
        }
        Checker::MultipleChoice {
            correct_index,
            options,
        } => {
            // L'opzione che il checker dichiara esatta dev'essere la risposta
            // dell'istanza. Non basta che la risposta sia *una* delle opzioni:
            // senza questo controllo, l'istanza di un seed con il checker di un
            // altro prenderebbe un voto — quello sbagliato, e silenzioso.
            let esatta = options.get(*correct_index as usize);
            if esatta.is_some_and(|o| o.trim() == instance.expected.trim()) {
                Ok(())
            } else {
                Err(ExerciseError::UnboundInstance {
                    reason: format!(
                        "il checker dichiara esatta l'opzione {:?} mentre l'istanza dichiara {:?}",
                        esatta.map(|o| o.trim()).unwrap_or("nessuna"),
                        instance.expected
                    ),
                })
            }
        }
        Checker::Equivalence { normalized } => {
            if normalize_equivalence(&instance.expected) == normalize_equivalence(normalized) {
                Ok(())
            } else {
                Err(ExerciseError::UnboundInstance {
                    reason: format!(
                        "l'istanza dichiara {:?} e il checker la forma normale {:?}",
                        instance.expected, normalized
                    ),
                })
            }
        }
    }
}

/// Confronta la risposta dello studente con l'attesa.
///
/// `Err` ⟺ l'esercizio è rotto (vedi il modulo). Ogni altra risposta riceve un
/// verdetto.
pub fn grade(
    instance: &Instance,
    checker: &Checker,
    answer: &str,
) -> Result<Verdict, ExerciseError> {
    validate_checker(checker)?;
    is_bound_to(instance, checker)?;
    Ok(match checker {
        Checker::Numeric { tolerance } => compare_numeric(&instance.expected, *tolerance, answer),
        Checker::Set { elements } => compare_set(elements, answer),
        Checker::MultipleChoice {
            correct_index,
            options,
        } => compare_multiple_choice(*correct_index, options, answer),
        Checker::Equivalence { normalized } => compare_equivalence(normalized, answer),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Numeric
// ─────────────────────────────────────────────────────────────────────────────

/// Un numero finito, o `None`. `f64::from_str` accetta `inf`, `-inf` e `NaN`:
/// sono risposte che un confronto con tolleranza accetterebbe o rifiuterebbe per
/// motivi sbagliati (`NaN` non è `<=` niente), quindi il parsing le scarta.
fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Tolleranza assoluta, non relativa.
///
/// Relativa sembra più giusta e non lo è: con atteso `0.0001` e tolleranza
/// relativa all'1%, chi risponde `0.000099` ha sbagliato di `0.000001` e viene
/// accettato, mentre chi risponde `99.999999` ha sbagliato di `0.000001` e
/// viene rifiutato. La scala della risposta è arbitraria — il quaderno non
/// garantisce l'ordine di grandezza — quindi solo una distanza assoluta, in
/// unità della risposta, è difendibile davanti a uno studente.
fn compare_numeric(expected: &str, tolerance: f64, answer: &str) -> Verdict {
    match (parse_number(expected), parse_number(answer)) {
        (Some(e), Some(a)) if (a - e).abs() <= tolerance => Verdict::Correct,
        _ => Verdict::Incorrect,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Set
// ─────────────────────────────────────────────────────────────────────────────

/// La risposta di un `Set` è una lista di elementi separati da virgola.
///
/// Gli elementi sono stringhe potate degli spazi laterali.
fn parse_answer_set(answer: &str) -> Vec<&str> {
    answer
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Confronto su **multiset**, non su estensione: due liste hanno la stessa
/// risposta se hanno gli stessi elementi con le stesse molteplicità, in qualunque
/// ordine.
///
/// L'ordine non conta perché il prompt lo dichiara; la molteplicità conta perché
/// elencare due volte un elemento non è «un insieme diverso», è un insieme
/// sbagliato che un confronto su `HashSet` accetterebbe.
fn compare_set(elements: &[String], answer: &str) -> Verdict {
    let raw: Vec<&str> = answer.split(',').map(|p| p.trim()).collect();
    // Un buco («12, , 19») non descrive un insieme: è una risposta sbagliata.
    // Si scarta prima di confrontare, così un insieme atteso privo di stringhe
    // vuote non è raggiungibile per caso.
    if raw.iter().any(|p| p.is_empty()) {
        return Verdict::Incorrect;
    }
    let mut want: Vec<&str> = elements.iter().map(|e| e.trim()).collect();
    let mut have = raw;
    want.sort_unstable();
    have.sort_unstable();
    if want == have {
        Verdict::Correct
    } else {
        Verdict::Incorrect
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MultipleChoice
// ─────────────────────────────────────────────────────────────────────────────

/// Accetta due forme di risposta, in quest'ordine di precedenza:
/// 1. il testo esatto di un'opzione;
/// 2. l'indice decimale dell'opzione.
///
/// Il testo ha la precedenza perché è la forma che il prompt chiede, e
/// l'ambiguità è possibile («2» è l'indice della terza opzione *e* potrebbe
/// essere il testo di un'opzione).
///
/// Una risposta che non è nessuna delle due è semplicemente sbagliata: l'esercizio
/// è chiuso, e ogni risposta fuori dall'insieme delle opzioni è errata — compreso
/// un indice fuori intervallo, che è la risposta di uno studente che ha contato le
/// opzioni a mente male. Non è un errore: un errore sarebbe l'indice dell'esatto
/// fuori intervallo, che è un esercizio rotto (vedi `validate_checker`).
fn compare_multiple_choice(correct_index: u8, options: &[String], answer: &str) -> Verdict {
    let a = answer.trim();
    if let Some(i) = options.iter().position(|o| o.trim() == a) {
        return if i == correct_index as usize {
            Verdict::Correct
        } else {
            Verdict::Incorrect
        };
    }
    if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(i) = a.parse::<usize>() {
            if i < options.len() && i == correct_index as usize {
                return Verdict::Correct;
            }
        }
    }
    Verdict::Incorrect
}

// ─────────────────────────────────────────────────────────────────────────────
// Equivalence
// ─────────────────────────────────────────────────────────────────────────────

/// L'intera normalizzazione di `Equivalence`: via gli spazi, e toglie le
/// parentesi esterne bilanciate finché se ne tolgono.
///
/// Sono poche righe, e sono tutto. Non c'è parser, non c'è valutazione, non c'è
/// semplificazione: `2*x+3` non è `2*(x+3)` e non viene accettato come tale,
/// perché accettarlo richiederebbe un deciditore di uguaglianza polinomiale — e un
/// deciditore che decide le risposte è un giudice che non si può contrattare.
pub fn normalize_equivalence(s: &str) -> String {
    let mut cur: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    loop {
        let inner = match cur.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
            Some(i) if parens_balanced(i) => i.to_string(),
            _ => break,
        };
        cur = inner;
    }
    cur
}

fn parens_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn compare_equivalence(normalized: &str, answer: &str) -> Verdict {
    if normalize_equivalence(answer) == normalize_equivalence(normalized) {
        Verdict::Correct
    } else {
        Verdict::Incorrect
    }
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

    // ── Numeric ──────────────────────────────────────────────────────────────

    #[test]
    fn numerico_accetta_il_risultato_e_rifiuta_il_quasi() {
        let c = Checker::Numeric { tolerance: 0.0005 };
        let i = inst("42.5000");
        assert_eq!(grade(&i, &c, "42.5"), Ok(Verdict::Correct));
        // entro la tolleranza: la risposta arrotondata è la risposta.
        assert_eq!(grade(&i, &c, "42.5004"), Ok(Verdict::Correct));
        // fuori: 0.01 di errore su 42.5 non è arrotondamento.
        assert_eq!(grade(&i, &c, "42.51"), Ok(Verdict::Incorrect));
    }

    /// Il quasi-miss che un confronto disattento accetterebbe. La versione
    /// disattenta è quella relativa all'1%: `|42.51 - 42.5| / 42.5` vale 0.00023,
    /// cioè «uguale». Noi no: la tolleranza è assoluta.
    #[test]
    fn numerico_una_tolleranza_relativa_accetterebbe_un_quasi() {
        let relativo = |a: f64, e: f64| (a - e).abs() / e.abs() < 0.01;
        assert!(
            relativo(42.51, 42.5),
            "il confronto disattento dice «giusto»"
        );
        let c = Checker::Numeric { tolerance: 0.0005 };
        assert_eq!(grade(&inst("42.5000"), &c, "42.51"), Ok(Verdict::Incorrect));
    }

    #[test]
    fn numerico_la_non_somma_nessun_verbale() {
        let c = Checker::Numeric { tolerance: 0.0005 };
        for risposta in [
            "quarantadue",
            "",
            "42,5",
            "NaN",
            "inf",
            "-inf",
            "1e999",
            "0x2a",
        ] {
            assert_eq!(
                grade(&inst("42.5000"), &c, risposta),
                Ok(Verdict::Incorrect),
                "{risposta:?} non è un numero: è una risposta sbagliata, non un errore"
            );
        }
    }

    #[test]
    fn numerico_la_tolleranza_zero_e_il_confronto_esatto() {
        let c = Checker::Numeric { tolerance: 0.0 };
        let i = inst("129");
        assert_eq!(grade(&i, &c, "129"), Ok(Verdict::Correct));
        assert_eq!(grade(&i, &c, "129.0001"), Ok(Verdict::Incorrect));
    }

    #[test]
    fn una_tolleranza_non_confrontabile_e_un_esercizio_rotto() {
        for tolerance in [f64::NAN, f64::INFINITY, -0.001] {
            let c = Checker::Numeric { tolerance };
            assert!(matches!(
                validate_checker(&c),
                Err(ExerciseError::BadTolerance { .. })
            ));
        }
        assert!(validate_checker(&Checker::Numeric { tolerance: 0.0 }).is_ok());
    }

    // ── Set ──────────────────────────────────────────────────────────────────

    fn set() -> Checker {
        Checker::Set {
            elements: vec!["12".into(), "19".into()],
        }
    }

    #[test]
    fn insieme_l_ordine_non_conta() {
        let i = inst("12, 19");
        for risposta in ["12, 19", "19,12", "  19 ,  12  "] {
            assert_eq!(
                grade(&i, &set(), risposta),
                Ok(Verdict::Correct),
                "{risposta:?}"
            );
        }
        for risposta in ["12", "12, 19, 30", "12, 91", ""] {
            assert_eq!(
                grade(&i, &set(), risposta),
                Ok(Verdict::Incorrect),
                "{risposta:?}"
            );
        }
    }

    /// Il quasi-miss che un confronto su `HashSet` accetterebbe: l'elemento
    /// ripetuto non cambia l'estensione, cambia la molteplicità. Noi no.
    #[test]
    fn insieme_un_confronto_su_estensione_accetterebbe_un_quasi() {
        let disattento = |risposta: &str| -> bool {
            use std::collections::HashSet;
            let a: HashSet<&str> = risposta.split(',').map(|p| p.trim()).collect();
            a == HashSet::from(["12", "19"])
        };
        assert!(
            disattento("19, 12, 12"),
            "il confronto disattento dice «giusto»"
        );
        assert_eq!(
            grade(&inst("12, 19"), &set(), "19, 12, 12"),
            Ok(Verdict::Incorrect)
        );
    }

    #[test]
    fn insieme_un_elemento_vuoto_non_e_un_elemento() {
        let i = inst("12, 19");
        for risposta in ["12, 19,", "12,,19", "12,  , 19"] {
            assert_eq!(
                grade(&i, &set(), risposta),
                Ok(Verdict::Incorrect),
                "{risposta:?}"
            );
        }
    }

    #[test]
    fn un_insieme_con_molteplicita_dichiarata_e_un_esercizio_rotto() {
        let c = Checker::Set {
            elements: vec!["12".into(), "12".into()],
        };
        assert!(matches!(
            validate_checker(&c),
            Err(ExerciseError::DuplicateSetElement { .. })
        ));
        assert!(matches!(
            validate_checker(&Checker::Set { elements: vec![] }),
            Err(ExerciseError::EmptySet)
        ));
        assert!(matches!(
            validate_checker(&Checker::Set {
                elements: vec!["12".into(), " ".into()]
            }),
            Err(ExerciseError::EmptySetElement)
        ));
    }

    // ── MultipleChoice ───────────────────────────────────────────────────────

    fn mc() -> Checker {
        Checker::MultipleChoice {
            correct_index: 2,
            options: vec!["3/4".into(), "5/8".into(), "7/8".into(), "9/10".into()],
        }
    }

    #[test]
    fn scelta_multipla_accetta_testo_e_indice() {
        let i = inst("7/8");
        let c = mc();
        assert_eq!(grade(&i, &c, "7/8"), Ok(Verdict::Correct));
        assert_eq!(grade(&i, &c, "2"), Ok(Verdict::Correct));
        assert_eq!(grade(&i, &c, " 2 "), Ok(Verdict::Correct));
        assert_eq!(grade(&i, &c, "5/8"), Ok(Verdict::Incorrect));
        assert_eq!(grade(&i, &c, "0"), Ok(Verdict::Incorrect));
        // indice fuori intervallo: risposta sbagliata, mai un errore
        assert_eq!(grade(&i, &c, "9"), Ok(Verdict::Incorrect));
        assert_eq!(grade(&i, &c, "terza"), Ok(Verdict::Incorrect));
    }

    /// Il quasi-miss che un confronto per contenimento accetterebbe: «7/8» è
    /// sottostringa di «17/8» e `expected.contains(answer)` dice «giusto».
    #[test]
    fn scelta_multipla_un_confronto_per_containing_accetterebbe_un_quasi() {
        let c = Checker::MultipleChoice {
            correct_index: 1,
            options: vec!["4/8".into(), "7/8".into(), "9/10".into(), "1/3".into()],
        };
        assert!(
            "17/8".contains("7/8"),
            "il confronto disattento dice «giusto»"
        );
        assert_eq!(grade(&inst("7/8"), &c, "17/8"), Ok(Verdict::Incorrect));
    }

    /// Il test che D8 chiede: un indice dell'esatto fuori dal vettore è un
    /// **errore di costruzione**, non un voto. Senza questo controllo
    /// l'indicizzazione fuori limite darebbe un voto a caso, o un panico.
    #[test]
    fn indice_dell_esatto_fuori_intervallo_e_un_errore_di_costruzione() {
        let c = Checker::MultipleChoice {
            correct_index: 4,
            options: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        };
        let i = inst("a");
        assert_eq!(
            grade(&i, &c, "a"),
            Err(ExerciseError::CorrectIndexOutOfRange { index: 4, len: 4 })
        );
        assert!(matches!(
            validate_checker(&c),
            Err(ExerciseError::CorrectIndexOutOfRange { index: 4, len: 4 })
        ));
    }

    #[test]
    fn opzioni_due_volte_stesso_testo_sono_un_esercizio_rotto() {
        let c = Checker::MultipleChoice {
            correct_index: 0,
            options: vec!["a".into(), "a".into()],
        };
        assert!(matches!(
            validate_checker(&c),
            Err(ExerciseError::DuplicateOption { .. })
        ));
        let c = Checker::MultipleChoice {
            correct_index: 0,
            options: vec!["a".into(), " ".into()],
        };
        assert!(matches!(
            validate_checker(&c),
            Err(ExerciseError::EmptyOption)
        ));
        assert!(matches!(
            validate_checker(&Checker::MultipleChoice {
                correct_index: 0,
                options: vec![]
            }),
            Err(ExerciseError::NoOptions)
        ));
    }

    // ── Equivalence ─────────────────────────────────────────────────────────

    fn eq() -> Checker {
        Checker::Equivalence {
            normalized: "2*(x+3)".into(),
        }
    }

    #[test]
    fn equivalenza_accetta_cio_che_differisce_solo_per_forma_normale() {
        let i = inst("2*(x+3)");
        for risposta in [
            "2*(x+3)",
            "2*( x + 3 )",
            "(2*(x+3))",
            " 2*(x+3) ",
            "((2*(x+3)))",
        ] {
            assert_eq!(
                grade(&i, &eq(), risposta),
                Ok(Verdict::Correct),
                "{risposta:?}"
            );
        }
    }

    #[test]
    fn equivalenza_non_e_algebra() {
        let i = inst("2*(x+3)");
        // Ognuna di queste vale quanto `2*(x+3)` per ogni x. Nessuna è accettata,
        // ed è il punto: accettarle richiederebbe un parser che decide le risposte.
        for risposta in ["2*x+3", "2x+3", "6+2*x", "2*(3+x)"] {
            assert_eq!(
                grade(&i, &eq(), risposta),
                Ok(Verdict::Incorrect),
                "{risposta:?}"
            );
        }
    }

    #[test]
    fn equivalenza_rifiuta_anche_un_valore_diverso() {
        let i = inst("2*(x+3)");
        assert_eq!(grade(&i, &eq(), "2*(x+4)"), Ok(Verdict::Incorrect));
        assert_eq!(grade(&i, &eq(), "3*(x+2)"), Ok(Verdict::Incorrect));
    }

    #[test]
    fn una_forma_normale_vuota_e_un_esercizio_rotto() {
        assert!(matches!(
            validate_checker(&Checker::Equivalence {
                normalized: "  ".into()
            }),
            Err(ExerciseError::EmptyNormalForm)
        ));
    }

    // ── il legame istanza ↔ checker ──────────────────────────────────────────

    #[test]
    fn un_istanza_di_un_altro_seed_non_si_grada() {
        let c = Checker::Numeric { tolerance: 0.0 };
        assert!(matches!(
            grade(&inst("quarantadue"), &c, "42"),
            Err(ExerciseError::UnboundInstance { .. })
        ));

        let c = Checker::Equivalence {
            normalized: "2*(x+3)".into(),
        };
        assert!(matches!(
            grade(&inst("3*(x+2)"), &c, "2*(x+3)"),
            Err(ExerciseError::UnboundInstance { .. })
        ));

        let c = Checker::Set {
            elements: vec!["12".into()],
        };
        assert!(matches!(
            grade(&inst("19, 12"), &c, "12"),
            Err(ExerciseError::UnboundInstance { .. })
        ));

        let c = Checker::MultipleChoice {
            correct_index: 0,
            options: vec!["a".into(), "b".into()],
        };
        assert!(matches!(
            grade(&inst("c"), &c, "a"),
            Err(ExerciseError::UnboundInstance { .. })
        ));
    }
}
