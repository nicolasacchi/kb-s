//! Il contratto didattico: otto sezioni, budget in byte, `GUARDIAN` per primo
//! (D7 di `ARCHITECTURE.md`).
//!
//! # La regola per cui questo modulo esiste
//!
//! > Un contratto troncato è leggibile ma non eseguibile.
//!
//! Non è un commento: è [`ContractReport::executable`], ed è un **test**
//! (`un_contratto_troppo_lungo_non_e_eseguibile_anche_se_la_testa_ha_otto_sezioni`).
//!
//! # Il caso che un validatore ingenuo passa
//!
//! Il tetto è 8192 byte, e il modo naturale di implementarlo è troncare il testo
//! e poi validare la testa. Così un contratto di 20000 byte la cui coda è
//! spazzatura viene accettato, perché la testa contiene tutte e otto le
//! intestazioni. Ma l'agente che lo legge riceve esattamente quella testa, e la
//! coda che il docente aveva scritto — il `LIMITE`, di solito — è sparita: il
//! sistema ha dichiarato integro un contratto che ha perso un pezzo. Qui il
//! troncamento è **uno stato**, `truncated`, e lo stato vale più della forma:
//! [`ContractReport::executable`] restituisce `false` se e solo se il contratto
//! è troncato *o* ha un errore strutturale.
//!
//! # Il modello è quello di `kb`, con una regola in più
//!
//! `kb` estrae il prompt da `<template id="kb-prompt">` e lo tronca a
//! `PROMPT_MAX_BYTES = 8 * 1024` su un confine di carattere
//! (`kb/crates/kb-core/src/parser.rs:26,503`), registrando `prompt_size_bytes`.
//! Qui lo slot è `<template id="kb-kbprompt">`, il tetto è lo stesso 8192 e il
//! troncamento ha un nome che ne vieta l'uso. La differenza è tutta lì: in `kb`
//! un prompt troncato è un prompt corto; qui è un contratto **non eseguibile**.
//!
//! # La sintassi
//!
//! Una sezione è una riga che comincia con `##` e il cui testo è il nome della
//! sezione. `###` è un sotto-titolo dentro una sezione e non conta. I nomi sono
//! riconosciuti senza distinzione di maiuscole e riportati in maiuscolo: un
//! errore di battitura su un nome sconosciuto è un errore che si legge
//! (`Intestazione sconosciuta`), mentre forzare le maiuscole renderebbe un
//! refuso indistinguibile da una sezione mancante.
//!
//! I byte contati sono quelli del **testo decodificato**, cioè quello che l'agente
//! legge: un `&amp;` pesa un byte, non cinque.

use serde::{Deserialize, Serialize};

/// Una sezione obbligatoria e il suo budget, nell'ordine di D7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionSpec {
    pub name: &'static str,
    /// Budget in byte **del corpo** della sezione, intestazione esclusa.
    pub budget: usize,
}

/// Le otto sezioni, nell'ordine che il validatore richiede.
pub const SECTIONS: [SectionSpec; 8] = [
    SectionSpec { name: "GUARDIAN", budget: 640 },
    SectionSpec { name: "PREREQUISITI", budget: 256 },
    SectionSpec { name: "OBIETTIVI", budget: 512 },
    SectionSpec { name: "SCALA", budget: 1536 },
    SectionSpec { name: "EQUIVOCI", budget: 1024 },
    SectionSpec { name: "ESEMPIO-LAVORATO", budget: 1536 },
    SectionSpec { name: "VERIFICA", budget: 512 },
    SectionSpec { name: "LIMITE", budget: 256 },
];

/// Somma dei budget: 6272 byte.
pub const BUDGET_SUM: usize = 6272;

/// Tetto duro sul contratto intero.
pub const HARD_CAP: usize = 8192;

/// Margine fra la somma dei budget e il tetto: 1920 byte.
///
/// È il margine che una sezione può rubare alle altre. Per questo un corpo di
/// sezione sopra budget è un **avviso** e non un errore: il margine esiste
/// proprio per accogliere l'eccezione. Quello che non è un'eccezione è il tetto.
pub const MARGIN: usize = 1920;

/// Il nome della sezione in posizione `index`, se la posizione è valida.
pub fn section_name(index: usize) -> Option<&'static str> {
    SECTIONS.get(index).map(|s| s.name)
}

/// La posizione di una sezione nel suo nome, senza distinzione di maiuscole.
pub fn section_index(name: &str) -> Option<usize> {
    SECTIONS
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(name.trim()))
}

/// Tutti i nomi, per i messaggi d'errore.
pub fn section_names() -> String {
    SECTIONS
        .iter()
        .map(|s| s.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Perché un contratto non va bene. Il motivo è parte dell'errore: serve a chi
/// ha scritto il contratto, non solo al validatore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ContractError {
    #[error("manca la sezione obbligatoria {name}: le otto sezioni di D7 sono tutte obbligatorie, in quest'ordine ({all_sections})")]
    MissingSection { name: String, all_sections: String },

    #[error("la sezione {name} compare due volte (alla seconda occorrenza, byte {offset}): un contratto con sezioni ripetute non dice quale delle due vale")]
    DuplicateSection { name: String, offset: usize },

    #[error("la sezione {name} sta in posizione {found} e doveva stare in posizione {expected} ({expected_name}): l'ordine delle otto sezioni è parte del contratto")]
    OutOfOrder {
        name: String,
        expected: usize,
        expected_name: String,
        found: usize,
    },

    #[error("intestazione sconosciuta `{raw}`: le otto sezioni sono {all_sections}; per un sotto-titolo dentro una sezione si usa `###`")]
    UnknownSection { raw: String, all_sections: String },

    #[error("la sezione {name} è presente ma vuota: una sezione obbligatoria senza contenuto è una regola che non dice niente, e un lettore non può eseguire una pagina bianca")]
    EmptySection { name: String },

    #[error("GUARDIAN comincia al byte {offset} e deve stare entro i primi {limit}: il troncamento taglia la coda, e la coda è la parte che si perde")]
    GuardianTooLate { offset: usize, limit: usize },

    #[error("il contratto pesa {bytes} byte e supera il tetto di {cap}: è troncato, e un contratto troncato è leggibile ma non eseguibile")]
    OverCap { bytes: usize, cap: usize },

    #[error("la sezione {name} pesa {bytes} byte sul budget di {budget} (margine complessivo ancora disponibile: {margin})")]
    SectionOverBudget { name: String, bytes: usize, budget: usize, margin: usize },
}

/// Una sezione, misurata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionReport {
    /// Posizione nella sequenza di D7, 0-based.
    pub index: usize,
    pub name: String,
    /// Byte del sorgente dove comincia la riga `## NOME`.
    pub heading_offset: usize,
    /// Byte del corpo, intestazione e spazi di contorno esclusi.
    pub body_bytes: usize,
    pub budget: usize,
}

impl SectionReport {
    pub fn over_budget(&self) -> bool {
        self.body_bytes > self.budget
    }
}

/// L'esito della validazione di un contratto.
///
/// `errors` e `warnings` stanno nello stesso tipo di errore perché sono lo stesso
/// fatto a due severità diverse: la sezione `SCALA` a 900 byte è un contratto
/// valido che sfora il suo budget, non un contratto rotto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractReport {
    /// Il contratto come viene **memorizzato**: troncato al tetto, su un
    /// confine di carattere, come fa `kb` con il prompt. Un contratto troncato
    /// resta leggibile — ed è per questo che va etichettato.
    pub stored_text: String,
    /// Byte del contratto **non troncato**: la misura vera.
    pub total_bytes: usize,
    pub truncated: bool,
    pub sections: Vec<SectionReport>,
    pub errors: Vec<ContractError>,
    pub warnings: Vec<ContractError>,
}

impl ContractReport {
    /// **La regola.** Un contratto è eseguibile se e solo se non è troncato e non
    /// ha errori strutturali. Non esiste un percorso che la contorni: qui non si
    /// chiama il checker, non si esegue niente, e l'unica domanda è questa.
    pub fn executable(&self) -> bool {
        !self.truncated && self.errors.is_empty()
    }

    /// Il primo errore, che è quello che il chiamante mostra all'autore.
    pub fn first_error(&self) -> Option<&ContractError> {
        self.errors.first()
    }

    /// Il contratto eseguibile, o l'errore che lo ha negato.
    pub fn require(&self) -> Result<&ContractReport, &ContractError> {
        if self.executable() {
            Ok(self)
        } else {
            Err(self
                .first_error()
                .expect("non eseguibile implica almeno un errore: o troncato, o errori"))
        }
    }

    pub fn section(&self, name: &str) -> Option<&SectionReport> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

/// Valida un contratto. Non fallisce mai: fallire significherebbe non poter
/// descrivere *tutto* ciò che non va, e un validatore che si arrende alla prima
/// difficoltà non può dire «manca `LIMITE`».
///
/// Il testo in ingresso è il contenuto già decodificato dello slot
/// `<template id="kb-kbprompt">` (vedi [`crate::parser`]).
pub fn inspect(text: &str) -> ContractReport {
    let total_bytes = text.len();
    let mut errors: Vec<ContractError> = Vec::new();
    let mut warnings: Vec<ContractError> = Vec::new();

    // ── 1. Le intestazioni, in ordine di posizione nel testo ──────────────────
    struct Found {
        index: usize,
        name: String,
        offset: usize,
        body_start: usize,
    }
    let mut found: Vec<Found> = Vec::new();

    for (offset, line) in lines_with_offsets(text) {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("##") else { continue };
        // `###` è un sotto-titolo: non è una sezione e non sposta il cursore.
        if rest.starts_with('#') {
            continue;
        }
        let raw = rest.trim();
        if raw.is_empty() {
            continue;
        }
        match section_index(raw) {
            Some(index) => found.push(Found {
                index,
                name: SECTIONS[index].name.to_string(),
                offset,
                body_start: offset + line.len() + 1,
            }),
            None => errors.push(ContractError::UnknownSection {
                raw: raw.to_string(),
                all_sections: section_names(),
            }),
        }
    }

    // ── 2. Duplicati ──────────────────────────────────────────────────────────
    for (i, f) in found.iter().enumerate() {
        if found[..i].iter().any(|p| p.index == f.index) {
            errors.push(ContractError::DuplicateSection {
                name: f.name.clone(),
                offset: f.offset,
            });
        }
    }

    // ── 3. Presenza e ordine ──────────────────────────────────────────────────
    for (expected, spec) in SECTIONS.iter().enumerate() {
        match found.iter().position(|f| f.index == expected) {
            None => errors.push(ContractError::MissingSection {
                name: spec.name.to_string(),
                all_sections: section_names(),
            }),
            Some(at) => {
                if at != expected {
                    errors.push(ContractError::OutOfOrder {
                        name: spec.name.to_string(),
                        expected,
                        expected_name: spec.name.to_string(),
                        found: at + 1,
                    });
                }
            }
        }
    }

    // ── 4. Corpi, budget, vuoti ───────────────────────────────────────────────
    let mut sections: Vec<SectionReport> = Vec::new();
    for (i, f) in found.iter().enumerate() {
        let body_end = found.get(i + 1).map(|n| n.offset).unwrap_or(text.len());
        let body_start = f.body_start.min(body_end);
        let body = text[body_start..body_end].trim();
        let body_bytes = body.len();
        let report = SectionReport {
            index: f.index,
            name: f.name.clone(),
            heading_offset: f.offset,
            body_bytes,
            budget: SECTIONS[f.index].budget,
        };
        if body_bytes == 0 {
            errors.push(ContractError::EmptySection { name: f.name.clone() });
        } else if report.over_budget() {
            warnings.push(ContractError::SectionOverBudget {
                name: f.name.clone(),
                bytes: body_bytes,
                budget: report.budget,
                margin: MARGIN,
            });
        }
        sections.push(report);
    }

    // ── 5. `GUARDIAN` entro i primi 640 byte ──────────────────────────────────
    // L'offset è quello della riga `## GUARDIAN` nel contratto. Il troncamento
    // taglia la coda: se l'ostacolo sta davanti, l'agente legge la coda senza
    // sapere perché.
    if let Some(guardian) = found.iter().find(|f| f.index == 0) {
        if guardian.offset >= SECTIONS[0].budget {
            errors.push(ContractError::GuardianTooLate {
                offset: guardian.offset,
                limit: SECTIONS[0].budget,
            });
        }
    }

    // ── 6. Il tetto ───────────────────────────────────────────────────────────
    // Sulla lunghezza **non troncata**: questo è il punto dell'intero modulo.
    let truncated = total_bytes > HARD_CAP;
    if truncated {
        errors.push(ContractError::OverCap { bytes: total_bytes, cap: HARD_CAP });
    }

    let stored_text = if truncated {
        truncate_on_char_boundary(text, HARD_CAP)
    } else {
        text.to_string()
    };

    ContractReport { stored_text, total_bytes, truncated, sections, errors, warnings }
}

/// Il contratto eseguibile, o l'errore che lo ha negato. È il modo breve per
/// chi non ha bisogno del rapporto.
pub fn require(text: &str) -> Result<ContractReport, ContractError> {
    let report = inspect(text);
    match report.require() {
        Ok(_) => Ok(report),
        Err(e) => Err(e.clone()),
    }
}

/// Taglia su un confine di carattere, come fa `kb` per il prompt.
fn truncate_on_char_boundary(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut = max;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].to_string()
}

/// `(offset di byte della riga, riga senza il terminatore)`.
fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in text.split('\n') {
        out.push((offset, line.strip_suffix('\r').unwrap_or(line)));
        offset += line.len() + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un contratto nelle otto sezioni, tutte entro budget.
    fn contratto_buono() -> String {
        [
            "## GUARDIAN",
            "Non confondere la continuità con la continuità uniforme: la prima è un limite, la seconda una richiesta.",
            "## PREREQUISITI",
            "Saper leggere il grafico di una funzione.",
            "## OBIETTIVI",
            "Distinguere i due enunciati e scegliere il giusto.",
            "## SCALA",
            "Il concetto si usa su funzioni in due variabili e su successioni.",
            "## EQUIVOCI",
            "«Uniformemente continua» non vuol dire «derivabile».",
            "## ESEMPIO-LAVORATO",
            "f(x)=x^2 su [0,1] è uniformemente continua, ma non monotona decrescente.",
            "## VERIFICA",
            "Dare tre funzioni e chiedere di classificarle.",
            "## LIMITE",
            "Il grafico serve a orientarsi, non a dimostrare.",
        ]
        .join("\n")
    }

    #[test]
    fn i_budget_dichiarati_sono_il_contratto() {
        let somma: usize = SECTIONS.iter().map(|s| s.budget).sum();
        assert_eq!(somma, BUDGET_SUM);
        assert_eq!(BUDGET_SUM, 6272);
        assert_eq!(HARD_CAP, 8192);
        assert_eq!(HARD_CAP - BUDGET_SUM, MARGIN);
        assert_eq!(MARGIN, 1920);
    }

    #[test]
    fn le_otto_sezioni_nell_ordine_giusto() {
        let r = inspect(&contratto_buono());
        assert!(r.executable(), "errori: {:?}", r.errors);
        assert!(!r.truncated);
        let nomi: Vec<&str> = r.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            nomi,
            vec![
                "GUARDIAN",
                "PREREQUISITI",
                "OBIETTIVI",
                "SCALA",
                "EQUIVOCI",
                "ESEMPIO-LAVORATO",
                "VERIFICA",
                "LIMITE"
            ]
        );
        for s in &r.sections {
            assert!(s.body_bytes > 0, "{} vuota", s.name);
            assert!(!s.over_budget(), "{} sfora", s.name);
        }
    }

    #[test]
    fn sezione_mancante_rifiutata_con_il_nome() {
        // Via l'intestazione **e** la sua riga di corpo: togliere solo
        // l'intestazione lascerebbe il corpo di LIMITE attaccato a VERIFICA, e
        // il test passerebbe — o fallirebbe — per un motivo sbagliato.
        let testo: String = contratto_buono()
            .lines()
            .filter(|l| !l.starts_with("## LIMITE") && !l.starts_with("Il grafico serve"))
            .collect::<Vec<_>>()
            .join("\n");
        let r = inspect(&testo);
        assert!(!r.executable());
        match r.first_error() {
            Some(ContractError::MissingSection { name, .. }) => assert_eq!(name, "LIMITE"),
            altro => panic!("atteso MissingSection, trovato {altro:?}"),
        }
        assert!(require(&testo).is_err());
    }

    #[test]
    fn guardian_tardi_rifiutato() {
        let riempimento = "p".repeat(700);
        let testo = format!("{riempimento}\n{}", contratto_buono());
        let r = inspect(&testo);
        assert!(!r.executable());
        assert!(matches!(
            r.first_error(),
            Some(ContractError::GuardianTooLate { .. })
        ));
    }

    #[test]
    fn guardian_entro_i_primi_640_byte_accettato() {
        let riempimento = "p".repeat(600);
        let testo = format!("{riempimento}\n{}", contratto_buono());
        let r = inspect(&testo);
        assert!(r.executable(), "errori: {:?}", r.errors);
    }

    #[test]
    fn sezioni_nell_ordine_sbagliato_rifiutate() {
        let testo = contratto_buono().replace("## OBIETTIVI", "## PREREQUISITI\n## OBIETTIVI");
        let r = inspect(&testo);
        assert!(!r.executable());
        assert!(r
            .errors
            .iter()
            .any(|e| matches!(e, ContractError::OutOfOrder { .. })));
        assert!(r
            .errors
            .iter()
            .any(|e| matches!(e, ContractError::DuplicateSection { name, .. } if name == "PREREQUISITI")));
    }

    #[test]
    fn intestazione_sconosciuta_rifiutata() {
        let testo = contratto_buono().replace("## LIMITE", "## LIMITI");
        let r = inspect(&testo);
        assert!(!r.executable());
        assert!(r.errors.iter().any(|e| matches!(
            e,
            ContractError::UnknownSection { raw, .. } if raw == "LIMITI"
        )));
    }

    #[test]
    fn sezione_vuota_rifiutata() {
        let testo = contratto_buono().replace(
            "## LIMITE\nIl grafico serve a orientarsi, non a dimostrare.",
            "## LIMITE",
        );
        let r = inspect(&testo);
        assert!(!r.executable());
        assert!(r
            .errors
            .iter()
            .any(|e| matches!(e, ContractError::EmptySection { name } if name == "LIMITE")));
    }

    #[test]
    fn il_sottotitolo_non_e_una_sezione() {
        let testo = contratto_buono()
            .replace("## SCALA\n", "## SCALA\n### Caso base\ntesto\n");
        let r = inspect(&testo);
        assert!(r.executable(), "errori: {:?}", r.errors);
        assert_eq!(r.sections.len(), 8);
    }

    #[test]
    fn sezione_sopra_budget_e_avviso_non_errore() {
        let lungo = "s".repeat(700);
        let testo = contratto_buono().replace(
            "Non confondere la continuità con la continuità uniforme: la prima è un limite, la seconda una richiesta.",
            &lungo,
        );
        let r = inspect(&testo);
        // 700 < 640? No: sopra il budget di GUARDIAN, ma sotto il tetto.
        assert_eq!(lungo.len(), 700);
        assert!(r.executable(), "un margine esiste per questo: {:?}", r.errors);
        assert!(r.warnings.iter().any(|w| matches!(
            w,
            ContractError::SectionOverBudget { name, budget, .. } if name == "GUARDIAN" && *budget == 640
        )));
    }

    #[test]
    fn contratto_troppo_lungo_e_troncato_e_non_eseguibile() {
        let testo = format!("{}\ncoda lunghissima", contratto_buono());
        let testo = format!("{testo}{}", "z".repeat(HARD_CAP));
        let r = inspect(&testo);
        assert!(r.truncated);
        assert!(!r.executable());
        assert!(matches!(
            r.first_error(),
            Some(ContractError::OverCap { bytes, cap }) if *bytes > HARD_CAP && *cap == HARD_CAP
        ));
        assert!(r.stored_text.len() <= HARD_CAP);
    }

    #[test]
    fn il_caso_che_un_validatore_ingenuo_passa() {
        // Oltre 8192 byte, ma tutti e otto le nomi nelle prime 8192: la testa
        // sembra perfetta. Il troncamento non la rende perfetta.
        let testo = format!("{}{}", contratto_buono(), "z".repeat(HARD_CAP));
        let r = inspect(&testo);
        // La precondizione del test: la testa contiene davvero tutte e otto.
        for spec in SECTIONS {
            assert!(
                testo[..HARD_CAP].contains(&format!("## {}", spec.name)),
                "la testa deve contenere {}",
                spec.name
            );
        }
        assert_eq!(r.sections.len(), 8, "la testa presenta tutte le otto sezioni");
        assert!(r.truncated, "e nonostante ciò il contratto è troncato");
        assert!(!r.executable());
    }

    #[test]
    fn il_testo_memorizzato_e_troncato_su_un_confine_di_carattere() {
        // Un contratto con accenti multi-byte: il taglio non può cadere nel
        // mezzo di un carattere.
        let testo = format!("{}è{}\n", contratto_buono(), "à".repeat(9000));
        let r = inspect(&testo);
        assert!(r.truncated);
        assert!(r.stored_text.len() <= HARD_CAP);
        // Rileggere: se il taglio avesse spaccato un carattere, questo confronto
        // fallirebbe.
        assert_eq!(r.stored_text, truncate_on_char_boundary(&testo, HARD_CAP));
    }

    #[test]
    fn i_byte_contati_sono_byte_e_non_caratteri() {
        // Otto caratteri da 2 byte: 16 byte. Se contasse i caratteri, il
        // contratto starebbe nel tetto.
        let testo = format!("{}è{}\n", contratto_buono(), "à".repeat(8500));
        let r = inspect(&testo);
        assert!(r.total_bytes > HARD_CAP);
        assert!(r.truncated);
        assert!(!r.executable());
    }

    #[test]
    fn il_testo_vuoto_non_e_un_contratto() {
        let r = inspect("");
        assert!(!r.executable());
        assert_eq!(r.errors.len(), 8);
        assert!(r.total_bytes == 0);
    }
}
