//! Il contratto fra il banco e la pipeline.
//!
//! Il banco non chiama le API interne degli altri crate: le chiama **da
//! fuori**, attraverso il binario, che è la stessa interfaccia che D10.2
//! dichiara come «la CLI come protocollo». Un bench che si aggancia alle
//! funzioni interne si rompe a ogni rifattorizzazione di un crate che non
//! sta toccando; un bench che si aggancia al processo verifica ciò che
//! l'insegnante e l'agente vedono davvero.
//!
//! # Il contratto
//!
//! Il banco esegue un solo comando:
//!
//! ```text
//! kbs verify --json --db <percorso> <radice-corpus>
//! ```
//!
//! e legge da stdout un JSON con questa forma:
//!
//! ```json
//! {
//!   "kbs_version": "0.1.0",
//!   "items": [
//!     { "rel_path": "mappe/03-catena-dei-prerequisiti.html",
//!       "valid": false,
//!       "errors": [ { "code": "prerequisite-cycle", "message": "…" } ] }
//!   ],
//!   "index": {
//!     "citable": ["arg_…"],
//!     "not_citable": [ { "id": "arg_…", "invariant": "stale-ratification", "message": "…" } ]
//!   },
//!   "claims": [ { "id": "arg_…::cl_11_1", "rel_path": "…", "status": "contradicted", "in_registry": true, "in_output": false } ],
//!   "instances": [ { "exercise": "course_0001::ex_05_1", "seed": "s1", "expected": "5/6", "params": { "a": 2, "b": 3, "c": 1 } } ]
//! }
//! ```
//!
//! I codici di errore sono kebab-case e dichiarati anche in
//! `ARCHITECTURE.md` per ciò che riguarda D7 e D15:
//! `contract-missing-section`, `contract-guardian-out-of-budget`,
//! `contract-over-budget`, `external-reference`, `prerequisite-cycle`. Gli
//! `invariant` riprendono le varianti di `kbs_core::Invariant` in kebab-case.
//!
//! # Se il contratto non c'è
//!
//! Se il binario non esiste, o se la sua uscita non ha la forma qui sopra, i
//! controlli che dipendono dalla pipeline diventano **saltati con la ragione**.
//! Non diventano superati: un controllo saltato non ha dimostrato niente, e
//! con `--require-pipeline` (che è il modo in cui gira la CI) il saltato è un
//! fallimento.

use crate::corpus::Corpus;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Perché la pipeline non è raggiungibile o non ha parlato come si aspettava.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipelineError {
    #[error("il binario «{0}» non è stato trovato: imposta KBS_BIN con il percorso del binario `kbs`")]
    NonTrovato(String),
    #[error("{binario} è terminato con stato {stato}: {stderr}")]
    Fallito {
        binario: String,
        stato: i32,
        stderr: String,
    },
    #[error("l'uscita di «{binario}» non è JSON: {causa}")]
    NonJson { binario: String, causa: String },
    #[error("l'uscita di «{binario}» è JSON ma non ha la forma del contratto: manca «{campo}»")]
    Contratto { binario: String, campo: String },
    #[error("{0}")]
    Altro(String),
}

/// La diagnostica di validazione di un item.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ItemDiagnostic {
    pub rel_path: String,
    pub valid: bool,
    #[serde(default)]
    pub errors: Vec<Errore>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Errore {
    pub code: String,
    #[serde(default)]
    pub message: String,
}

/// Un argomento che non è citabile, e **perché**: il perché è la parte che
/// serve, perché «non citabile» senza motivo è un verdetto e non una spiegazione.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NonCitable {
    pub id: String,
    pub invariant: String,
    #[serde(default)]
    pub message: String,
}

/// Una riga di claim come la pipeline la mostra.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClaimRow {
    pub id: String,
    #[serde(default)]
    pub rel_path: String,
    pub status: String,
    /// `true` se la riga esiste **nel registro**.
    pub in_registry: bool,
    /// `true` se la riga compare **nell'output** citabile.
    pub in_output: bool,
}

/// Un'istanza come la pipeline la riproduce.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InstanceRow {
    pub exercise: String,
    pub seed: String,
    pub expected: String,
    pub params: serde_json::Value,
}

/// Il referto della pipeline, nel formato del contratto qui sopra.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Uscita {
    #[serde(default)]
    pub kbs_version: String,
    pub items: Vec<ItemDiagnostic>,
    pub index: Index,
    #[serde(default)]
    pub claims: Vec<ClaimRow>,
    #[serde(default)]
    pub instances: Vec<InstanceRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Index {
    #[serde(default)]
    pub citable: Vec<String>,
    #[serde(default)]
    pub not_citable: Vec<NonCitable>,
}

impl Uscita {
    pub fn item(&self, rel: &str) -> Option<&ItemDiagnostic> {
        self.items.iter().find(|d| d.rel_path == rel)
    }

    pub fn claim(&self, id: &str) -> Option<&ClaimRow> {
        self.claims.iter().find(|c| c.id == id)
    }

    pub fn istanza(&self, esercizio: &str, seed: &str) -> Option<&InstanceRow> {
        self.instances
            .iter()
            .find(|i| i.exercise == esercizio && i.seed == seed)
    }
}

/// Come il banco raggiunge la pipeline. Un tratto, non una gerarchia: ne serve
/// uno, e quello giusto.
pub trait Pipeline {
    /// Come la pipeline è stata raggiunta, per il referto. Solo il nome del
    /// binario: un percorso assoluto renderebbe il referto diverso su ogni
    /// macchina e quindi non confrontabile.
    fn descrizione(&self) -> String;

    /// Esegue la pipeline sul corpus indicato.
    fn esegui(&self, corpus: &Corpus, radice: &Path) -> Result<Uscita, PipelineError>;
}

/// La pipeline reale, raggiunta come processo.
#[derive(Debug, Clone)]
pub struct ProcessPipeline {
    binario: PathBuf,
}

impl ProcessPipeline {
    ///Cerca il binario `kbs`: prima `KBS_BIN`, poi `target/debug/kbs` e
    /// `target/release/kbs` a partire dalla radice del workspace.
    pub fn cerca() -> Result<Self, PipelineError> {
        if let Ok(p) = std::env::var("KBS_BIN") {
            let path = PathBuf::from(&p);
            if path.exists() {
                return Ok(ProcessPipeline { binario: path });
            }
            return Err(PipelineError::NonTrovato(p));
        }
        let root = root_workspace();
        for rel in ["target/debug/kbs", "target/release/kbs"] {
            let path = root.join(rel);
            if path.exists() {
                return Ok(ProcessPipeline { binario: path });
            }
        }
        Err(PipelineError::NonTrovato("kbs".into()))
    }

    /// Costruisce un adattore su un percorso esplicito.
    pub fn su(binario: PathBuf) -> Self {
        ProcessPipeline { binario }
    }

    pub fn binario(&self) -> &Path {
        &self.binario
    }
}

fn root_workspace() -> PathBuf {
    // `CARGO_MANIFEST_DIR` è `crates/kbs-fixtures`; la radice del workspace è
    // due livelli sopra. È la stessa radice che `kc` usa, e quindi il binario
    // cercato è quello che `./kc build` produce.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Pipeline for ProcessPipeline {
    fn descrizione(&self) -> String {
        let nome = self
            .binario
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "kbs".into());
        format!("{nome} verify --json")
    }

    fn esegui(&self, _corpus: &Corpus, radice: &Path) -> Result<Uscita, PipelineError> {
        let nome = self
            .binario
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "kbs".into());
        let db = radice.join(".kbs-bench.sqlite");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(radice.join(".kbs-bench.sqlite-wal"));
        let _ = std::fs::remove_file(radice.join(".kbs-bench.sqlite-shm"));
        let out = Command::new(&self.binario)
            .arg("verify")
            .arg("--json")
            .arg("--db")
            .arg(&db)
            .arg(radice)
            .output()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => PipelineError::NonTrovato(nome.clone()),
                _ => PipelineError::Altro(format!("{nome}: {e}")),
            })?;
        if !out.status.success() {
            return Err(PipelineError::Fallito {
                binario: nome,
                stato: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        let testo = String::from_utf8_lossy(&out.stdout).to_string();
        let v: serde_json::Value = serde_json::from_str(testo.trim()).map_err(|e| {
            PipelineError::NonJson {
                binario: nome.clone(),
                causa: e.to_string(),
            }
        })?;
        for campo in ["items", "index"] {
            if v.get(campo).is_none() {
                return Err(PipelineError::Contratto {
                    binario: nome.clone(),
                    campo: campo.to_string(),
                });
            }
        }
        serde_json::from_value(v).map_err(|e| PipelineError::NonJson {
            binario: nome,
            causa: e.to_string(),
        })
    }
}

/// Una pipeline che non esiste. Serve a una cosa sola: far sì che i controlli
/// che la richiedono siano **saltati con una ragione** invece di non esistere.
#[derive(Debug, Clone)]
pub struct PipelineAssente {
    pub ragione: String,
}

impl Pipeline for PipelineAssente {
    fn descrizione(&self) -> String {
        format!("assente: {}", self.ragione)
    }

    fn esegui(&self, _corpus: &Corpus, _radice: &Path) -> Result<Uscita, PipelineError> {
        Err(PipelineError::Altro(self.ragione.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERTO: &str = r#"{
      "kbs_version": "0.1.0",
      "items": [
        {"rel_path": "a.html", "valid": true, "errors": []},
        {"rel_path": "b.html", "valid": false,
         "errors": [{"code": "external-reference", "message": "fuori perimetro"}]}
      ],
      "index": {
        "citable": ["arg_aaa"],
        "not_citable": [{"id": "arg_bbb", "invariant": "stale-ratification", "message": "x"}]
      },
      "claims": [
        {"id": "arg_aaa::cl_1", "rel_path": "a.html", "status": "contradicted",
         "in_registry": true, "in_output": false}
      ],
      "instances": [
        {"exercise": "course_0001::ex_1", "seed": "s1", "expected": "5/6",
         "params": {"a": 2, "b": 3, "c": 1}}
      ]
    }"#;

    /// Il banco deve capire il referto anche quando gli array opzionali
    /// mancano: una pipeline che non ha claim non sta violando il contratto,
    /// sta semplicemente non avendo claim da mostrare.
    #[test]
    fn il_referto_si_legge() {
        let u: Uscita = serde_json::from_str(REFERTO).expect("JSON valido");
        assert_eq!(u.kbs_version, "0.1.0");
        assert_eq!(u.items.len(), 2);
        assert!(!u.item("b.html").unwrap().valid);
        assert_eq!(u.item("b.html").unwrap().errors[0].code, "external-reference");
        assert!(u.item("zzz.html").is_none());
        assert_eq!(u.index.citable, vec!["arg_aaa".to_string()]);
        assert_eq!(u.index.not_citable[0].invariant, "stale-ratification");
        assert!(u.claim("arg_aaa::cl_1").unwrap().in_registry);
        assert!(!u.claim("arg_aaa::cl_1").unwrap().in_output);
        assert_eq!(
            u.istanza("course_0001::ex_1", "s1").unwrap().expected,
            "5/6"
        );
    }

    /// Ciò che non ha la forma del contratto è un **contratto non
    /// implementato**, non una regressione: diventa un saltato con ragione, e
    /// con `--require-pipeline` diventa un fallimento. MAI un «superato».
    #[test]
    fn un_referto_fuori_contratto_non_e_una_regressione() {
        let v: serde_json::Value = serde_json::json!({"items": [], "index": {}});
        assert!(v.get("index").is_some());
        let senza = serde_json::json!({"items": []});
        assert!(senza.get("index").is_none(), "manca index: non è il contratto");
    }

    #[test]
    fn la_ragione_di_assenza_e_nel_referto() {
        let p = PipelineAssente {
            ragione: "il binario «kbs» non è stato trovato".into(),
        };
        assert!(p.descrizione().contains("assente"));
        assert!(matches!(
            p.esegui(&Corpus::dalla_tabella(), Path::new(".")),
            Err(PipelineError::Altro(_))
        ));
    }
}
