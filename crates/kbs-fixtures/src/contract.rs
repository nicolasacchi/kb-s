//! Il contratto didattico: otto sezioni, budget per sezione, hard cap.
//!
//! Questo modulo **costruisce** contratti, non li valida. La validazione è di
//! `kbs-doc` e il banco non la duplica: qui il banco scrive i fixture, e li
//! scrive anche in modo **deliberatamente sbagliato**, perché una fiastra che
//! è rotta per costruzione è l'unica che può dimostrare che il validatore
//! rifiuta qualcosa.
//!
//! La misura dei byte è calcolata sulla stringa resa con [`measure`], che
//! localizza le intestazioni che questo stesso modulo ha scritto. Non è un
//! parser di HTML generico e non pretende di esserlo: se `kbs-doc` accetta una
//! sintassi diversa, l'unica cosa che va cambiata è [`SECTION_HEADING`] e la
//! funzione che la compone — ed è un posto solo.
//!
//! Riferimento: `ARCHITECTURE.md` D7.

use serde::{Deserialize, Serialize};

/// Le otto sezioni obbligatorie, nell'ordine, con il budget in byte.
pub const SECTIONS: [(&str, usize); 8] = [
    ("GUARDIAN", 640),
    ("PREREQUISITI", 256),
    ("OBIETTIVI", 512),
    ("SCALA", 1_536),
    ("EQUIVOCI", 1_024),
    ("ESEMPIO-LAVORATO", 1_536),
    ("VERIFICA", 512),
    ("LIMITE", 256),
];

/// Somma dei budget: 6 272 byte.
pub const BUDGET_SOMMA: usize = 6_272;

/// Hard cap sul contratto intero (D7).
pub const HARD_CAP: usize = 8_192;

/// Margine fra somma dei budget e hard cap: 1 920 byte.
pub const MARGINE: usize = HARD_CAP - BUDGET_SOMMA;

/// Come è scritta un'intestazione di sezione. È il lato «fixture» del
/// contratto con `kbs-doc`: un modulo con due costanti e una funzione.
pub const SECTION_HEADING: &str = "## ";

/// Il difetto che una fiastra porta con sé, se lo porta.
///
/// Un item senza difetti è un item che il validatore deve accettare. Un item
/// con un difetto è un item che il validatore deve **rifiutare**: senza questi
/// cinque, la metà negativa del banco non esiste e il banco può solo
/// dimostrare che ciò che funziona continua a funzionare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "difetto", rename_all = "kebab-case")]
pub enum Defect {
    /// Nessun difetto: l'item deve passare la validazione.
    Nessuno,
    /// Una delle otto intestazioni manca del tutto.
    SezioneMancante(&'static str),
    /// Il corpo di `GUARDIAN` supera i 640 byte: il troncamento taglia la coda,
    /// quindi il vincolo che lo studente deve leggere per primo sparisce.
    GuardianOltreBudget,
    /// Il contratto supera l'hard cap di 8 192 byte.
    OltreBudget,
    /// Referenzia un runtime esterno (CDN). D15: la sua violazione impedisce la
    /// pubblicazione, perché un artifact che chiama fuori non è verificabile
    /// né riproducibile.
    RiferimentoEsterno(&'static str),
    /// Il grafo dei prerequisiti ha un ciclo: l'item non entra.
    PrerequisitoCiclico(&'static str),
}

impl Defect {
    /// Il codice che la pipeline deve emettere per rifiutare l'item. I codici
    /// sono parte del contratto fra banco e pipeline, e sono dichiarati anche
    /// in `ARCHITECTURE.md` per quello che riguarda D7 e D15.
    pub fn expected_code(&self) -> Option<&'static str> {
        match self {
            Defect::Nessuno => None,
            Defect::SezioneMancante(_) => Some("contract-missing-section"),
            Defect::GuardianOltreBudget => Some("contract-guardian-out-of-budget"),
            Defect::OltreBudget => Some("contract-over-budget"),
            Defect::RiferimentoEsterno(_) => Some("external-reference"),
            Defect::PrerequisitoCiclico(_) => Some("prerequisite-cycle"),
        }
    }

    /// Perché la fiastra è rotta, in italiano. Va nel referto: un banco che
    /// dice «fallito» senza dire quale regola è stata rotta non serve a
    /// correggere niente.
    pub fn spiegazione(&self) -> String {
        match self {
            Defect::Nessuno => "nessun difetto: l'item deve passare la validazione".into(),
            Defect::SezioneMancante(s) => {
                format!("manca la sezione {s} delle otto obbligatorie (D7)")
            }
            Defect::GuardianOltreBudget => {
                "il corpo di GUARDIAN supera i 640 byte, e il troncamento taglia la coda (D7)".into()
            }
            Defect::OltreBudget => format!(
                "il contratto supera l'hard cap di {HARD_CAP} byte (D7)"
            ),
            Defect::RiferimentoEsterno(u) => {
                format!("referenzia {u}, fuori dal perimetro (D15)")
            }
            Defect::PrerequisitoCiclico(p) => {
                format!("il prerequisito {p} chiude un ciclo: l'argomento non entra")
            }
        }
    }
}

/// Una sezione del contratto, pronta per essere resa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: &'static str,
    pub body: String,
}

/// Il contratto di un item: otto sezioni in ordine, più i difetti che lo
/// rendono non pubblicabile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    /// `None` per una sezione che la fiastra omette di proposito.
    pub sections: Vec<Option<Section>>,
}

impl Contract {
    /// Costruisce un contratto a otto sezioni in ordine.
    pub fn new(bodies: [&str; 8]) -> Self {
        let sections = SECTIONS
            .iter()
            .zip(bodies)
            .map(|((name, _), body)| Some(Section { name, body: body.to_string() }))
            .collect();
        Contract { sections }
    }

    /// Applica un difetto, restituendo il contratto alterato.
    pub fn con_difetto(self, difetto: Defect) -> Self {
        let mut c = self;
        match difetto {
            Defect::SezioneMancante(nome) => {
                if let Some(slot) = c.sections.iter_mut().find(|s| {
                    s.as_ref().map(|s| s.name == nome).unwrap_or(false)
                }) {
                    *slot = None;
                }
            }
            Defect::GuardianOltreBudget => {
                let riempimento = "La regola che lo studente deve applicare, enunciata per esteso, \
                    con i casi in cui vale e i casi in cui non vale, e con l'esempio del \
                    controesempio che la distingue dalla regola vicina. ".repeat(6);
                for slot in c.sections.iter_mut() {
                    if let Some(s) = slot {
                        if s.name == "GUARDIAN" {
                            s.body = riempimento.clone();
                        }
                    }
                }
            }
            Defect::OltreBudget => {
                // Tre sezioni gonfiate oltre il loro budget: è l'unico modo per
                // superare l'hard cap, perché la somma dei budget (6 272) è
                // minore del cap (8 192). Il testo è un riempimento dichiarato:
                // una fiastra che non si rompe non prova niente.
                let riempimento = "Testo di riempimento dichiarato come difetto: serve a far \
                    superare il cap di 8 192 byte, e non ha valore didattico alcuno. "
                    .repeat(24);
                for slot in c.sections.iter_mut() {
                    if let Some(s) = slot {
                        if matches!(s.name, "SCALA" | "EQUIVOCI" | "ESEMPIO-LAVORATO") {
                            s.body = riempimento.clone();
                        }
                    }
                }
            }
            Defect::Nessuno
            | Defect::RiferimentoEsterno(_)
            | Defect::PrerequisitoCiclico(_) => {}
        }
        c
    }

    /// Il testo del contratto come va nel template.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for s in self.sections.iter().flatten() {
            out.push_str(SECTION_HEADING);
            out.push_str(s.name);
            out.push('\n');
            out.push_str(s.body.trim_end());
            out.push('\n');
            if s.name != self.sections.iter().flatten().last().map(|l| l.name).unwrap_or("") {
                out.push('\n');
            }
        }
        out
    }
}

/// La misura del contratto reso: dove inizia e finisce ogni sezione, in byte.
///
/// Serve al banco per una cosa sola: dimostrare che le **cinque** fiastre
/// rotte sono ancora rotte. Se qualcuno le sistema e non aggiorna la tabella,
/// la misura non torna più e il banco diventa rosso invece che verde — che è
/// il modo in cui un banco dice «la tua correzione ha cambiato il test».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    /// Byte occupati dalla sezione, intestazione compresa.
    pub size: usize,
    /// Byte fino alla fine della sezione, dall'inizio del contratto.
    pub end: usize,
}

/// Misura tutte le sezioni presenti nel testo reso.
pub fn measure(testo: &str) -> Vec<(&'static str, Measurement)> {
    let mut out = Vec::new();
    let bytes = testo.as_bytes();
    let mut starts: Vec<(&'static str, usize)> = Vec::new();
    for (name, _) in SECTIONS {
        let needle = format!("{SECTION_HEADING}{name}\n");
        if let Some(pos) = testo.find(&needle) {
            starts.push((name, pos));
        }
    }
    for (idx, (name, pos)) in starts.iter().enumerate() {
        let fine = match starts.get(idx + 1) {
            // la sezione successiva inizia dopo una riga vuota di separazione
            Some((_, p)) => *p,
            None => bytes.len(),
        };
        let size = fine - pos;
        out.push((
            *name,
            Measurement {
                size,
                end: pos + size,
            },
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completo() -> Contract {
        Contract::new([
            "Non usare la regola senza dire perché.", "Nessuno.", "Capire la regola.",
            "Scala nominale in quattro livelli.", "Non confondere A con B.",
            "Esempio numerico svolto.", "Verifica con il foglio.", "Vale solo in ℚ.",
        ])
    }

    /// La somma dei budget è 6 272 e il cap è 8 192: la differenza è il margine
    /// che D7 dichiara. Se un numero cambia, il margine dichiarato non è più
    /// quello reale.
    #[test]
    fn budget_e_margine_coerenti() {
        let somma: usize = SECTIONS.iter().map(|(_, b)| b).sum();
        assert_eq!(somma, BUDGET_SOMMA);
        assert_eq!(MARGINE, 1_920);
        assert!(BUDGET_SOMMA < HARD_CAP);
        assert_eq!(SECTIONS[0].0, "GUARDIAN");
        assert_eq!(SECTIONS[0].1, 640);
    }

    #[test]
    fn un_contratto_completo_si_misura_entro_i_budget() {
        let testo = completo().render();
        let m = measure(&testo);
        assert_eq!(m.len(), 8, "il contratto completo ha otto sezioni");
        for (nome, meas) in &m {
            let budget = SECTIONS.iter().find(|(n, _)| n == nome).unwrap().1;
            assert!(meas.size <= budget, "{nome}: {} byte > budget {budget}", meas.size);
        }
        let guardian = m[0].1;
        assert!(guardian.end <= 640, "GUARDIAN deve stare nei primi 640 byte");
        assert!(testo.len() <= HARD_CAP);
    }

    /// Le tre fiastre rotte per contratto. Il test che le tiene rotte.
    #[test]
    fn i_difetti_di_contratto_producono_testo_misurabilmente_rotto() {
        let senza = completo().con_difetto(Defect::SezioneMancante("LIMITE")).render();
        assert_eq!(measure(&senza).len(), 7, "sette sezioni: manca LIMITE");
        assert!(!senza.contains("## LIMITE"));

        let guardian = completo().con_difetto(Defect::GuardianOltreBudget).render();
        let m = measure(&guardian);
        assert!(m[0].1.end > 640, "GUARDIAN oltre i 640 byte");

        let over = completo().con_difetto(Defect::OltreBudget).render();
        assert!(over.len() > HARD_CAP, "il contratto deve superare l'hard cap");
    }

    #[test]
    fn ogni_difetto_ha_un_codice_e_una_spiegazione() {
        for d in [
            Defect::Nessuno,
            Defect::SezioneMancante("LIMITE"),
            Defect::GuardianOltreBudget,
            Defect::OltreBudget,
            Defect::RiferimentoEsterno("https://cdn.jsdelivr.net/npm/three/+esm"),
            Defect::PrerequisitoCiclico("mappe/03-catena-dei-prerequisiti.html"),
        ] {
            if d.expected_code().is_none() {
                assert_eq!(d, Defect::Nessuno, "{d:?} non dovrebbe avere un codice");
            } else {
                assert!(!d.spiegazione().is_empty(), "{d:?} non ha spiegazione");
            }
        }
    }
}
