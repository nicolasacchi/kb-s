//! La validazione di un artifact: tutto ciò che impedisce a un file di essere
//! pubblicato, in un posto solo.
//!
//! # Che cosa blocca e che cosa no
//!
//! **Blocca** (l'artifact non è pubblicabile):
//!
//! * un riferimento fuori dalla scatella (D15) — non verificabile (D6), non
//!   riproducibile (D11);
//! * un contratto assente, rotto o troncato (D7);
//! * un titolo assente: è il campo BM25 primario, e un artifact senza titolo
//!   è un artifact che non si trova;
//! * due intestazioni con lo stesso id: i link ai commenti e alle claim che
//!   puntano a un titolo diventano ambigui, e l'ambiguità silenziosa in un
//!   registro è peggio di un errore.
//!
//! **Non blocca**, ma è detto ad alta voce:
//!
//! * una claim senza span verificabile: la claim diventa `Unciteable` (D6) e
//!   resta nel registro. Il registro non è un cancello di pubblicazione, è un
//!   registro di errori inclusi.
//! * un artifact convertito da markdown: si pubblica, ma il lettore deve
//!   poter sapere che non ha la stessa garanzia di uno scritto in HTML (D14).
//! * una sezione di contratto sopra il suo budget: c'è un margine di 1920 byte
//!   per questo.

use serde::{Deserialize, Serialize};

use crate::contract::{self, ContractError, ContractReport};
use crate::offbox::{self, Origin};
use crate::parser::{self, ParsedArtifact};

/// Quanto è grave un problema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Impedisce la pubblicazione.
    Blocking,
    /// Va detto, non impedisce.
    Warning,
}

/// Un problema, con il motivo e dove sta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: IssueCode,
    pub message: String,
    /// 1-based, quando il problema è in una riga del sorgente.
    pub line: Option<usize>,
}

impl Issue {
    fn blocking(code: IssueCode, message: String) -> Self {
        Issue { severity: Severity::Blocking, code, message, line: None }
    }

    fn warning(code: IssueCode, message: String) -> Self {
        Issue { severity: Severity::Warning, code, message, line: None }
    }
}

/// Che cosa è andato storto, in forma machine-readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "codice", rename_all = "kebab-case")]
pub enum IssueCode {
    /// Nessun `<title>`.
    NoTitle,
    /// Nessuno `<template id="kb-kbprompt">`.
    NoContract,
    /// Il contratto c'è ma non regge D7 (troncato compreso).
    Contract(ContractError),
    /// Un riferimento esce dalla scatola (D15).
    ExternalReference { kind: String, url: String },
    /// Due intestazioni con lo stesso id.
    HeadingCollision { id: String, occurrences: usize },
    /// Una claim che il lettore non può verificare.
    UnverifiableClaim { claim_id: String, reason: String },
    /// L'artifact dichiara di essere stato convertito da markdown (D14).
    ConvertedFromMarkdown,
}

/// L'esito della validazione di un artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReport {
    pub parsed: ParsedArtifact,
    /// `None` se l'artifact non ha nessuno slot del contratto.
    pub contract: Option<ContractReport>,
    pub issues: Vec<Issue>,
}

impl ArtifactReport {
    /// La domanda che D4 e D7 rendono vera: questo file si pubblica?
    ///
    /// Un contratto troncato non è eseguibile, e un artifact il cui contratto
    /// non è eseguibile non si pubblica: si può leggere e si può mostrare, che è
    /// esattamente ciò che D7 consente a un contratto troncato.
    pub fn can_publish(&self) -> bool {
        !self.has_blocking()
    }

    pub fn has_blocking(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Blocking)
    }

    pub fn blocking(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Blocking)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Warning)
    }

    /// Il primo problema che impedisce la pubblicazione: quello da mostrare.
    pub fn first_blocking(&self) -> Option<&Issue> {
        self.blocking().next()
    }

    /// Il contratto eseguibile, se lo è. Il percorso breve per chi non ha bisogno
    /// del resto del rapporto.
    pub fn executable_contract(&self) -> Option<&ContractReport> {
        self.contract.as_ref().filter(|c| c.executable())
    }
}

/// Valida un artifact. Non fallisce mai: l'esito è il rapporto.
pub fn inspect(src: &str) -> ArtifactReport {
    let parsed = parser::parse(src);
    let mut issues: Vec<Issue> = Vec::new();

    if parsed.title.is_none() {
        issues.push(Issue::blocking(
            IssueCode::NoTitle,
            "nessun <title>: è il campo che rende l'artifact trovabile".to_string(),
        ));
    }

    let contract = parsed.contract_text.as_deref().map(contract::inspect);
    match &contract {
        None => issues.push(Issue::blocking(
            IssueCode::NoContract,
            format!(
                "nessun <template id=\"{}\">: senza contratto l'unità non è eseguibile (D7)",
                parser::CONTRACT_SLOT
            ),
        )),
        Some(c) => {
            for e in &c.errors {
                issues.push(Issue::blocking(IssueCode::Contract(e.clone()), e.to_string()));
            }
            for w in &c.warnings {
                issues.push(Issue::warning(IssueCode::Contract(w.clone()), w.to_string()));
            }
        }
    }

    for r in &parsed.references {
        if r.origin == Origin::External {
            let mut issue = Issue::blocking(
                IssueCode::ExternalReference {
                    kind: r.kind.as_str().to_string(),
                    url: r.url.clone(),
                },
                format!(
                    "{} verso `{}` (riga {}): un artifact che chiama fuori non è \
                     verificabile (D6) né riproducibile (D11), quindi non si pubblica (D15)",
                    r.kind.as_str(),
                    r.url,
                    r.line
                ),
            );
            issue.line = Some(r.line);
            issues.push(issue);
        }
    }

    if !parsed.heading_collisions.is_empty() {
        for c in &parsed.heading_collisions {
            let occurrences = parsed
                .headings
                .iter()
                .filter(|h| h.id == c.id)
                .count();
            issues.push(Issue::blocking(
                IssueCode::HeadingCollision { id: c.id.clone(), occurrences },
                format!(
                    "due intestazioni hanno l'id `{}` (\"{}\" e \"{}\"): un link a un \
                     titolo è ambiguo, e l'ambiguità in un registro è silenziosa. \
                     Serve un id esplicito su una delle due",
                    c.id, c.first_text, c.second_text
                ),
            ));
        }
    }

    for claim in parsed.unverified_claims() {
        issues.push(Issue::warning(
            IssueCode::UnverifiableClaim {
                claim_id: claim.id.clone(),
                reason: claim.span.reason().to_string(),
            },
            format!(
                "la claim `{}` non è sostenuta: {}. Va nel registro come \
                 unciteable, non nell'output",
                claim.id,
                claim.span.reason()
            ),
        ));
    }

    if parsed.converted_from_markdown() {
        issues.push(Issue::warning(
            IssueCode::ConvertedFromMarkdown,
            "l'artifact dichiara di essere stato convertito da markdown: non porta \
             con sé una scena 3D né un verificatore, e la sua garanzia non è quella \
             di un artifact scritto in HTML (D14)"
                .to_string(),
        ));
    }

    ArtifactReport { parsed, contract, issues }
}

/// Il primo problema che impedisce la pubblicazione, se c'è.
pub fn blocking_issue(src: &str) -> Option<Issue> {
    inspect(src).first_blocking().cloned()
}

/// Solo i riferimenti che escono dalla scatola. È la scorciatoia per il
/// validatore e per i test della regola D15.
pub fn external_references(src: &str) -> Vec<offbox::Reference> {
    offbox::external(src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{HARD_CAP, SECTIONS};

    fn buono() -> String {
        format!(
            r#"<!DOCTYPE html><html lang="it"><head><title>Titolo</title></head><body>
            <h2>Equivoci</h2><p>Testo.</p>
            <template id="kb-kbprompt">{}</template>
            </body></html>"#,
            crate::testing::contratto_buono()
        )
    }

    #[test]
    fn un_artifact_buono_si_pubblica() {
        let r = inspect(&buono());
        assert!(r.can_publish(), "{:?}", r.blocking().collect::<Vec<_>>());
        assert!(r.executable_contract().is_some());
    }

    #[test]
    fn senza_titolo_non_si_pubblica() {
        let src = buono().replace("<title>Titolo</title>", "");
        let r = inspect(&src);
        assert!(!r.can_publish());
        assert!(r
            .blocking()
            .any(|i| matches!(i.code, IssueCode::NoTitle)));
    }

    #[test]
    fn senza_contratto_non_si_pubblica() {
        let src = buono().replace("kb-kbprompt", "altro-slot");
        let r = inspect(&src);
        assert!(!r.can_publish());
        assert!(r
            .blocking()
            .any(|i| matches!(i.code, IssueCode::NoContract)));
        assert!(r.executable_contract().is_none());
    }

    #[test]
    fn un_contratto_troncato_non_si_pubblica() {
        let testo = format!("{}\n{}", crate::testing::contratto_buono(), "z".repeat(HARD_CAP));
        let src = format!(
            r#"<html><head><title>T</title></head><body><template id="kb-kbprompt">{}</template></body></html>"#,
            testo
        );
        let r = inspect(&src);
        let c = r.contract.as_ref().expect("contratto presente");
        assert!(c.truncated);
        assert!(!c.executable());
        assert!(!r.can_publish());
        assert!(r
            .blocking()
            .any(|i| matches!(i.code, IssueCode::Contract(ContractError::OverCap { .. }))));
    }

    #[test]
    fn un_cdn_non_si_pubblica_e_sa_dove() {
        let src = buono().replace(
            "</head>",
            "<script src=\"https://cdn.example.com/three.js\"></script></head>",
        );
        let r = inspect(&src);
        assert!(!r.can_publish());
        let issue = r
            .blocking()
            .find(|i| matches!(i.code, IssueCode::ExternalReference { .. }))
            .expect("riferimento esterno bloccante");
        assert!(issue.line.is_some());
        assert!(issue.message.contains("D15"));
    }

    #[test]
    fn il_runtime_vendorizzato_non_e_un_problema() {
        let src = format!("<script type=\"module\" src=\"{}\"></script>", offbox::THREE_RUNTIME);
        assert!(external_references(&src).is_empty());
    }

    #[test]
    fn una_collisione_di_id_blocca() {
        let src = buono().replace("<h2>Equivoci</h2>", "<h2>Equivoci</h2><h2>Equivoci</h2>");
        let r = inspect(&src);
        assert!(!r.can_publish());
        assert!(r.blocking().any(|i| matches!(
            i.code,
            IssueCode::HeadingCollision { ref id, occurrences: 2 } if id == "equivoci"
        )));
    }

    #[test]
    fn una_claim_non_verificabile_e_un_avviso_che_non_blocca() {
        let src = buono().replace(
            "<p>Testo.</p>",
            "<p>Testo.</p><span data-claim=\"fatto\" data-claim-id=\"c1\" data-claim-span=\"nope\"></span>",
        );
        let r = inspect(&src);
        assert!(r.can_publish(), "una claim non verificabile non è un artifact rotto");
        let w = r
            .warnings()
            .find(|i| matches!(i.code, IssueCode::UnverifiableClaim { .. }))
            .expect("avviso sulla claim");
        assert!(w.message.contains("c1"));
        assert!(r.parsed.claims[0].core_status() == kbs_core::ClaimStatus::Unciteable);
    }

    #[test]
    fn un_avviso_contrattuale_su_budget_non_blocca() {
        let src = format!(
            r#"<html><head><title>T</title></head><body><template id="kb-kbprompt">{}</template></body></html>"#,
            crate::testing::contratto_buono().replace(
                "Non confondere la continuità con la continuità uniforme: la prima è un limite, la seconda una richiesta.",
                &"s".repeat(SECTIONS[0].budget + 1)
            )
        );
        let r = inspect(&src);
        assert!(r.can_publish(), "il margine di 1920 byte esiste per questo");
        assert!(r.warnings().any(|i| matches!(
            i.code,
            IssueCode::Contract(ContractError::SectionOverBudget { .. })
        )));
    }

    #[test]
    fn nessun_panic_su_ingresso_malformato() {
        for src in ["", "<", "<html", "&&&", "<template id=\"kb-kbprompt\">"] {
            let r = inspect(src);
            let _ = r.can_publish();
        }
    }
}
