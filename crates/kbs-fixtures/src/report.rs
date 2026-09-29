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
    /// I controlli che su questo corpus non erano valutabili. Va nel JSON
    /// perché è l'unico posto in cui «ho verificato e va bene» e «non ho
    /// potuto verificare» restano distinguibili da una macchina: il testo
    /// umano li mette in due sezioni diverse, ma chi consuma il referto in CI
    /// legge i numeri, e un referto senza questo numero è un referto in cui
    /// ventitré controlli tacciono.
    pub non_valutabili: usize,
    /// La fonte del corpus: `file-reali` se il banco ha letto una cartella di
    /// file, `None` se ha letto la propria tabella. Va nel JSON perché due
    /// referti con gli stessi numeri ma fonti diverse non dicono la stessa
    /// cosa, e un diff fra due referti che non guarda la fonte non lo vede.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fonte: Option<&'static str>,
    /// La radice del corpus, in percorso relativo. È un campo e non una
    /// convenzione del testo perché un referto JSON che non dice da dove ha
    /// letto i file non è confrontabile con un altro.
    pub radice: String,
    pub file_ignoti: Vec<String>,
    pub controlli: Vec<ControlloJson>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlloJson {
    pub nome: &'static str,
    pub di_pipeline: bool,
    pub esito: &'static str,
    /// `true` se il controllo non era valutabile su questo corpus. È
    /// esplicito e non dedotto da `esito`, perché è la domanda che un
    /// consumatore del referto farà per primo: «questa riga dice qualcosa
    /// sul mio corpus?». Un campo che si deduce leggendo una stringa è un
    /// campo che ogni consumatore deve reimplementare, e la prima
    /// reimplementazione che sbaglia fa leggere «non valutabile» come «superato».
    pub non_valutabile: bool,
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
            non_valutabile: matches!(c.esito, Esito::NonValutabile(_)),
            problemi: match &c.esito {
                Esito::Fallito(p) => p.clone(),
                _ => Vec::new(),
            },
            // La ragione vale per i due esiti che non sono un verdetto:
            // il saltato e il non valutabile. Sono diversi perché uno è un atto
            // che non è stato possibile compiere e l'altro una premessa che
            // il corpus non dichiara, ma hanno la stessa bisogna di essere
            // spiegati, e un campo solo per i due evita che il secondo finisca
            // in un campo che nessuno legge.
            ragione: match &c.esito {
                Esito::Saltato(m) | Esito::NonValutabile(m) => Some(m.clone()),
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
        non_valutabili: r.non_valutabili(),
        fonte: r.fonte,
        radice: r.radice.clone(),
        file_ignoti: r.file_ignoti.clone(),
        controlli,
    };
    let mut s = serde_json::to_string_pretty(&doc).unwrap_or_default();
    s.push('\n');
    s
}

/// Il referto in testo, per chi legge.
///
/// L'ordine delle sezioni è fisso: superati, falliti, non valutabili, saltati.
/// I non valutabili vengono **prima** dei saltati e non per priorità estetica:
/// un saltato è un atto che il banco non ha potuto compiere e che con un
/// binario in più si fa da solo, un non valutabile è una premessa che il
/// corpus non dichiara e che nessun binario può far comparire. Il secondo è la
/// notizia che si legge e non si archivia, e metterlo per ultimo accanto ai
/// saltati lo renderebbe indistinguibile da un atto mancante.
pub fn in_testo(r: &Referto, require_pipeline: bool) -> String {
    let mut o = String::new();
    o.push_str("kbs-bench — banco di prova\n");
    // «su 12» è una promessa che il banco fa a sé stesso, e su un corpus di
    // file reali non vale: lì `famiglie` sono le famiglie che i file
    // dichiarano, e scrivere «5 famiglie di media su 12» sopra un corpus che ne
    // dichiara cinque discipline sarebbe una frase falsa detta da un campo vero.
    o.push_str(&match r.fonte {
        Some(_) => format!(
            "corpus: {} ({} file, {} famiglie dichiarate)\n",
            r.radice, r.item, r.famiglie
        ),
        None => format!(
            "corpus: {} ({} item, {} famiglie di media su 12)\n",
            r.radice, r.item, r.famiglie
        ),
    });
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
    let non_valutabili: Vec<_> = r
        .controlli
        .iter()
        .filter(|c| matches!(c.esito, Esito::NonValutabile(_)))
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
    o.push_str(&format!("\nnon valutabili ({})\n", non_valutabili.len()));
    for c in &non_valutabili {
        o.push_str(&format!("  N/V   {}\n", c.nome));
        if let Esito::NonValutabile(m) = &c.esito {
            o.push_str(&format!("        {m}\n"));
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
        "esito: {esito} — {} superati, {} falliti, {} non valutabili, {} saltati",
        r.superati(),
        r.falliti(),
        r.non_valutabili(),
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
        assert!(t.contains("saltati (11)"));
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

    /// Il JSON dichiara i non valutabili in un campo esplicito, per
    /// controllo e in totale. Un referto JSON senza quel campo dice «ho
    /// guardato tutto e non ho trovato niente» anche quando ha guardato cinque
    /// file su ventuno: la CI legge i numeri, e i numeri senza questo campo
    /// sono un referto in cui ventitré controlli tacciono.
    #[test]
    fn il_json_dichiara_i_non_valutabili_in_un_campo_esplicito() {
        let r = referto();
        let v: serde_json::Value =
            serde_json::from_str(&in_json(&r, false)).expect("JSON valido");
        assert_eq!(v["non_valutabili"], serde_json::json!(0));
        assert!(v["fonte"].is_null(), "il banco di tabella non ha una fonte dichiarata");
        // Il campo è presente su **ogni** controllo, anche quando vale `false`:
        // un campo che compare solo quando è vero obbliga chi consuma il
        // referto a distinguere «assente» da «falso», e la distinzione serve
        // solo a chi ha sbagliato qualcosa.
        for c in v["controlli"].as_array().expect("array") {
            assert_eq!(c["non_valutabile"], serde_json::json!(false));
        }
    }
}
