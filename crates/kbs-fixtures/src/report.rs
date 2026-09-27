//! Il referto: testo per gli umani, JSON per le macchine.
//!
//! Il referto è **deterministico**: due esecuzioni sullo stesso corpus
//! producono gli stessi byte. Niente orari, niente percorsi assoluti, niente
//! iterazione su tabelle non ordinate. Un referto che cambia fra un
//! esecuzione e l'altra non può essere confrontato, e il banco serve
//! precisamente a confrontare due esecuzioni.

use crate::checks::{Esito, Referto};
use serde::Serialize;

/// Il referto in JSON, per la CI e per chi vorrà fare diff fra due esecuzioni.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefertoJson {
    pub esito: &'static str,
    pub item: usize,
    pub famiglie: usize,
    pub hash_corpus: String,
    pub pipeline: String,
    pub superati: usize,
    pub falliti: usize,
    pub saltati: usize,
    pub file_ignoti: Vec<String>,
    pub controlli: Vec<ControlloJson>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlloJson {
    pub nome: &'static str,
    pub di_pipeline: bool,
    pub esito: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problemi: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ragione: Option<String>,
}

/// Il referto in JSON.
pub fn in_json(r: &Referto, require_pipeline: bool) -> String {
    let controlli = r
        .controlli
        .iter()
        .map(|c| ControlloJson {
            nome: c.nome,
            di_pipeline: c.di_pipeline,
            esito: c.esito.etichetta(),
            problemi: match &c.esito {
                Esito::Fallito(p) => p.clone(),
                _ => Vec::new(),
            },
            ragione: match &c.esito {
                Esito::Saltato(m) => Some(m.clone()),
                _ => None,
            },
        })
        .collect();
    let doc = RefertoJson {
        esito: if r.esito_con_rigidezza(require_pipeline) { "passato" } else { "fallito" },
        item: r.item,
        famiglie: r.famiglie,
        hash_corpus: r.hash_corpus.clone(),
        pipeline: r.pipeline.clone(),
        superati: r.superati(),
        falliti: r.falliti(),
        saltati: r.saltati(),
        file_ignoti: r.file_ignoti.clone(),
        controlli,
    };
    let mut s = serde_json::to_string_pretty(&doc).unwrap_or_default();
    s.push('\n');
    s
}

/// Il referto in testo, per chi legge.
///
/// L'ordine delle sezioni è fisso: superati, falliti, saltati. I saltati
/// vengono **per ultimi** perché sono la parte che va letta con più attenzione
/// e che un referto frettoloso metterebbe in fondo a una riga sola.
pub fn in_testo(r: &Referto, require_pipeline: bool) -> String {
    let mut o = String::new();
    o.push_str("kbs-bench — banco di prova\n");
    o.push_str(&format!(
        "corpus: {} ({} item, {} famiglie di media su 12)\n",
        r.radice, r.item, r.famiglie
    ));
    o.push_str(&format!("hash del corpus: {}\n", r.hash_corpus));
    o.push_str(&format!("pipeline: {}\n", r.pipeline));
    if !r.file_ignoti.is_empty() {
        o.push_str("file non attesi nella radice del corpus:\n");
        for f in &r.file_ignoti {
            o.push_str(&format!("  - {f}\n"));
        }
    }
    o.push('\n');

    let superati: Vec<_> = r.controlli.iter().filter(|c| c.esito == Esito::Superato).collect();
    let falliti: Vec<_> = r
        .controlli
        .iter()
        .filter(|c| matches!(c.esito, Esito::Fallito(_)))
        .collect();
    let saltati: Vec<_> = r
        .controlli
        .iter()
        .filter(|c| matches!(c.esito, Esito::Saltato(_)))
        .collect();

    o.push_str(&format!("superati ({})\n", superati.len()));
    for c in &superati {
        o.push_str(&format!("  ok    {}\n", c.nome));
    }
    o.push_str(&format!("\nfalliti ({})\n", falliti.len()));
    for c in &falliti {
        o.push_str(&format!("  RED   {}\n", c.nome));
        if let Esito::Fallito(problemi) = &c.esito {
            for p in problemi {
                o.push_str(&format!("        {p}\n"));
            }
        }
    }
    o.push_str(&format!("\nsaltati ({})\n", saltati.len()));
    for c in &saltati {
        o.push_str(&format!("  SKIP  {}\n", c.nome));
        if let Esito::Saltato(m) = &c.esito {
            o.push_str(&format!("        {m}\n"));
        }
    }
    o.push('\n');
    let esito = if r.esito_con_rigidezza(require_pipeline) {
        "PASSATO"
    } else {
        "FALLITO"
    };
    o.push_str(&format!(
        "esito: {esito} — {} superati, {} falliti, {} saltati",
        r.superati(),
        r.falliti(),
        r.saltati()
    ));
    if require_pipeline && r.saltati() > 0 {
        o.push_str(&format!(
            " (in CI un saltato è un fallimento: {} saltati da spiegare)",
            r.saltati()
        ));
    }
    o.push('\n');
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Banco, Config};
    use crate::adapter::PipelineAssente;

    fn referto() -> Referto {
        let assente = PipelineAssente { ragione: "binario assente".into() };
        Banco::new(Config::radice_di_default(), &assente).esegui()
    }

    /// Il referto deve dire **quale** controllo è saltato e **perché**. Un
    /// saltato muto è la cosa che rende un banco credibile e bugiardo.
    #[test]
    fn il_referto_testuale_nomina_i_saltati_e_la_ragione() {
        let t = in_testo(&referto(), false);
        assert!(t.contains("saltati (8)"));
        assert!(t.contains("SKIP  pipeline.validazione.corrisponde_all_attesa"));
        assert!(t.contains("binario assente"));
        assert!(t.contains("esito: PASSATO"));
    }

    /// Lo stesso referto in CI diventa rosso, e il testo lo dice.
    #[test]
    fn in_ci_un_saltato_e_un_fallimento_e_il_referto_lo_dichiara() {
        let t = in_testo(&referto(), true);
        assert!(t.contains("esito: FALLITO"));
        assert!(t.contains("in CI un saltato è un fallimento"));
    }

    /// Il JSON è completo quanto il testo: nessuna delle due forme può
    /// omettere un fallimento, o una delle due mentirebbe.
    #[test]
    fn il_json_contiene_ogni_controllo_con_il_suo_esito() {
        let r = referto();
        let j = in_json(&r, false);
        let v: serde_json::Value = serde_json::from_str(&j).expect("JSON valido");
        assert_eq!(v["controlli"].as_array().expect("array").len(), r.controlli.len());
        assert_eq!(v["superati"], serde_json::json!(r.superati()));
        assert_eq!(v["saltati"], serde_json::json!(r.saltati()));
        assert_eq!(v["hash_corpus"], serde_json::json!(r.hash_corpus));
    }
}
