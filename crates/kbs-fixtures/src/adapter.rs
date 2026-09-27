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
//! Il banco esegue atti, e un atto è **un processo**: un verbo, i suoi
//! argomenti, la radice su cui agisce.
//!
//! ```text
//! kbs verify  --json --db <percorso> <radice-corpus>
//! kbs promote --person <id> --arg <rel> --path <file> --db <percorso> <radice-corpus>
//! ```
//!
//! `verify` legge da stdout un JSON con questa forma:
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
//! `promote` non produce un referto: produce una risposta, e la risposta può
//! essere «no». Un «no» **non è un errore del banco**: è un verdetto del
//! sistema, e il banco deve poterlo leggere e nominarlo. Per questo un atto
//! rifiutato sta dentro [`Eseguito`] e non dentro un `Err`.
//!
//! # Perché una sequenza e non un comando solo
//!
//! Il banco deve poter fare ciò che una scuola fa, e una scuola non valida
//! una volta sola: ratifica, poi rilegge. `verify` da solo può **leggere** una
//! cartella, e da una cartella non esce nessuna ratifica: la ratifica è un
//! atto separato, di una persona, e senza `promote` il banco non può mai
//! osservare che cosa cambia quando qualcuno agisce.
//!
//! Per questo [`Pipeline`] ha due metodi e non uno: [`Pipeline::esegui`] è un
//! atto solo, e [`Pipeline::sequenza`] ne esegue diversi **sullo stesso
//! database**. Il confine di processo non cambia: `esegui` non chiama funzioni
//! interne, e `sequenza` nemmeno.
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

/// Un atto della pipeline: **un processo**, con i suoi argomenti e la radice
/// su cui agisce.
///
/// Un atto e non una funzione è la scelta che tiene il banco sul confine di
/// processo anche quando deve fare più di una cosa. Due atti possono avere
/// due radici diverse, ed è così che il banco fa parlare due copie dello
/// stesso corpus senza confonderle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atto {
    /// Il verbo: `verify` oppure `promote`.
    pub verbo: &'static str,
    /// La radice del corpus su cui l'atto agisce.
    pub radice: PathBuf,
    /// L'item di cui l'atto parla, quando ne parla uno solo. `verify`
    /// indicizza una cartella e non ha un soggetto; `promote` ne ha uno, e
    /// senza saperlo il banco non saprebbe a chi attribuire un rifiuto.
    pub soggetto: Option<String>,
    /// Gli argomenti, nell'ordine in cui arrivano al processo. `--db` non è
    /// qui: lo mette [`Pipeline::sequenza`], che è l'unica a sapere dove
    /// tiene il database.
    pub argomenti: Vec<String>,
}

impl Atto {
    /// `kbs verify --json <radice>`: l'indicizzazione, e il solo atto che
    /// produce un referto.
    pub fn verify(radice: &Path) -> Atto {
        Atto {
            verbo: "verify",
            radice: radice.to_path_buf(),
            soggetto: None,
            argomenti: vec!["--json".to_string()],
        }
    }

    /// `kbs promote --person <chi> --arg <rel> --path <file> <radice>`.
    ///
    /// `--path` non è un'opzione decorativa: senza il testo la porta non ha
    /// un verdetto su cui appoggiarsi, e promotare alla cieca non è
    /// promuovere. Il banco passa quindi il file **sotto la radice dell'atto**,
    /// non quella di un altro atto: è ciò che permette a due atti di
    /// promuovere lo stesso item in due corpus diversi.
    pub fn promote(radice: &Path, chi: &str, rel: &str) -> Atto {
        Atto {
            verbo: "promote",
            radice: radice.to_path_buf(),
            soggetto: Some(rel.to_string()),
            argomenti: vec![
                "--person".to_string(),
                chi.to_string(),
                "--arg".to_string(),
                rel.to_string(),
                "--path".to_string(),
                radice.join(rel).to_string_lossy().into_owned(),
                "--note".to_string(),
                "promossa dal banco: atto del docente".to_string(),
            ],
        }
    }
}

/// Una sessione: la cartella che contiene il database, e il database.
///
/// La pipeline **non crea** il database: lo riceve. È la ragione per cui due
/// sequenze consecutive parlano dello stesso database quando il banco dà
/// loro la stessa sessione, ed è ciò che rende possibile promuovere, cambiare
/// un contratto e verificare di nuovo senza ricominciare da capo.
///
/// Il database sta **fuori** dalla radice del corpus, e il motivo è
/// dichiarato: `walk_file` conta ogni file sotto la radice, e un database
/// dentro la radice finisce nel conteggio dei file che il banco fa. Il banco
/// si conterebbe da sé. In più un file che nessuno ha descritto è rumore, e
/// D12 vuole che la cartella del corso sia esattamente i file del corso.
#[derive(Debug)]
pub struct Session {
    cartella: tempfile::TempDir,
    db: PathBuf,
}

impl Session {
    /// Una sessione nuova, con un database che non esiste ancora.
    pub fn nuova() -> Result<Session, PipelineError> {
        let cartella = tempfile::tempdir().map_err(|e| PipelineError::Altro(e.to_string()))?;
        let db = cartella.path().join("kbs-bench.sqlite");
        Ok(Session { cartella, db })
    }

    /// Il database. Non esiste finché il primo atto non lo ha creato.
    pub fn db(&self) -> &Path {
        &self.db
    }

    /// La cartella che contiene il database.
    pub fn cartella(&self) -> &Path {
        self.cartella.path()
    }
}

/// Che cosa ha fatto un atto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eseguito {
    /// L'atto, come il banco l'ha chiesto.
    pub atto: Atto,
    /// Lo stato di uscita del processo.
    pub stato: i32,
    /// Il referto, quando l'atto ne produce uno. `None` per `promote`, che
    /// risponde e non referta.
    pub referto: Option<Uscita>,
    /// Perché l'atto è stato rifiutato. Vuota quando l'atto è riuscito: un
    /// rifiuto senza motivo è un verdetto, e i verdetti non si contestano.
    pub motivo: String,
}

impl Eseguito {
    /// `true` se il processo è uscito con stato zero.
    pub fn ok(&self) -> bool {
        self.stato == 0
    }

    /// Il motivo di un rifiuto, o una stringa vuota se l'atto è riuscito.
    pub fn motivo(&self) -> &str {
        &self.motivo
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

    /// D4, la seconda strada: più atti, **sullo stesso database**, ciascuno
    /// con la sua radice.
    ///
    /// È il metodo che permette al banco di osservare una ratifica che entra
    /// nel sistema. `esegui` è il caso particolare di un atto solo, e non ha
    /// un percorso diverso: due strade per la stessa operazione sono due
    /// operazioni che possono divergere.
    fn sequenza(
        &self,
        corpus: &Corpus,
        sessione: &Session,
        atti: &[Atto],
    ) -> Result<Vec<Eseguito>, PipelineError>;
}

/// La risposta a un atto rifiutato, come la dice la pipeline: un codice e un
/// messaggio. `promote` risponde anche quando rifiuta, e la risposta è più
/// utile dello stderr perché porta il codice.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rifiuto {
    pub code: String,
    #[serde(default)]
    pub message: String,
}

/// L'involucro in cui arriva un rifiuto.
#[derive(Debug, Deserialize)]
struct Involucro {
    #[serde(default)]
    error: Option<Rifiuto>,
}

/// Il motivo di un rifiuto, letto dal corpo della risposta e, se il corpo non
/// parla, dallo stderr.
fn motivo_di(nome: &str, atto: &Atto, stato: i32, stdout: &str, stderr: &str) -> String {
    if stato == 0 {
        return String::new();
    }
    if let Ok(v) = serde_json::from_str::<Involucro>(stdout) {
        if let Some(e) = v.error {
            return if e.message.trim().is_empty() {
                e.code
            } else {
                format!("{}: {}", e.code, e.message.trim())
            };
        }
    }
    let s = stderr.trim();
    if s.is_empty() {
        format!("{nome} {} ha uscito con stato {stato} senza dire perché", atto.verbo)
    } else {
        s.to_string()
    }
}

/// Il referto di un atto `verify`, letto secondo il contratto qui sopra.
fn leggi_referto(nome: &str, testo: &str) -> Result<Uscita, PipelineError> {
    let v: serde_json::Value = serde_json::from_str(testo.trim()).map_err(|e| PipelineError::NonJson {
        binario: nome.to_string(),
        causa: e.to_string(),
    })?;
    for campo in ["items", "index"] {
        if v.get(campo).is_none() {
            return Err(PipelineError::Contratto {
                binario: nome.to_string(),
                campo: campo.to_string(),
            });
        }
    }
    serde_json::from_value(v).map_err(|e| PipelineError::NonJson {
        binario: nome.to_string(),
        causa: e.to_string(),
    })
}

/// Il referto dell'atto numero `i`, se l'atto lo ha prodotto. Un `verify` che
/// non parla non è un `verify` che ha parlato male: è una pipeline che ha
/// smesso di rispondere, e la differenza fra le due cose è il motivo con cui
/// il controllo viene saltato.
pub(crate) fn referto_di(
    nome: &str,
    eseguiti: &[Eseguito],
    i: usize,
) -> Result<Uscita, PipelineError> {
    match eseguiti.get(i) {
        Some(e) => e.referto.clone().ok_or_else(|| PipelineError::Contratto {
            binario: nome.to_string(),
            campo: format!("il referto dell'atto {}", e.atto.verbo),
        }),
        None => Err(PipelineError::Contratto {
            binario: nome.to_string(),
            campo: format!("l'atto numero {i}, che non è stato eseguito"),
        }),
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

    /// Il nome del binario, senza il percorso: il referto deve essere uguale
    /// su ogni macchina, e un percorso assoluto non lo è.
    fn nome(&self) -> String {
        self.binario
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "kbs".into())
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
        // Il banco esegue due verbi, e il referto deve dirlo: un referto che
        // nomina solo `verify` fa credere che il banco non abbia mai chiesto
        // una ratifica, che è esattamente la cosa che la seconda strada di D4
        // è venuta a verificare.
        format!("{} verify --json, poi {} promote", self.nome(), self.nome())
    }

    fn esegui(&self, corpus: &Corpus, radice: &Path) -> Result<Uscita, PipelineError> {
        let nome = self.nome();
        let sessione = Session::nuova()?;
        let eseguiti = self.sequenza(corpus, &sessione, &[Atto::verify(radice)])?;
        referto_di(&nome, &eseguiti, 0)
    }

    fn sequenza(
        &self,
        _corpus: &Corpus,
        sessione: &Session,
        atti: &[Atto],
    ) -> Result<Vec<Eseguito>, PipelineError> {
        let nome = self.nome();
        let mut eseguiti = Vec::with_capacity(atti.len());
        for atto in atti {
            let out = Command::new(&self.binario)
                .arg(atto.verbo)
                .args(&atto.argomenti)
                .arg("--db")
                .arg(sessione.db())
                .arg(&atto.radice)
                .output()
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => PipelineError::NonTrovato(nome.clone()),
                    _ => PipelineError::Altro(format!("{nome}: {e}")),
                })?;
            let stato = out.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // Solo `verify` referta, e solo se è riuscito: un `verify` rifiutato
            // non ha un indice da leggere, e fingersi che ce l'abbia produrrebbe
            // un controllo verde su un assenso che nessuno ha dato.
            let referto = if stato == 0 && atto.verbo == "verify" {
                Some(leggi_referto(&nome, &stdout)?)
            } else {
                None
            };
            eseguiti.push(Eseguito {
                atto: atto.clone(),
                stato,
                referto,
                motivo: motivo_di(&nome, atto, stato, &stdout, &stderr),
            });
        }
        Ok(eseguiti)
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

    fn sequenza(
        &self,
        _corpus: &Corpus,
        _sessione: &Session,
        _atti: &[Atto],
    ) -> Result<Vec<Eseguito>, PipelineError> {
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

    /// L'atto di promozione porta con sé il **file**, e lo porta dalla radice
    /// dell'atto. Se `--path` puntasse altrove, due atti su due copie
    /// promuoverebbero lo stesso item sul testo dell'altro, e il banco
    /// misurerebbe una ratifica che non è mai stata firmata.
    #[test]
    fn latto_di_promozione_legge_il_testo_sotto_la_sua_radice() {
        let a = Atto::promote(Path::new("/corpus/a"), "person_0000", "prove/01.html");
        assert_eq!(a.verbo, "promote");
        assert_eq!(a.soggetto.as_deref(), Some("prove/01.html"));
        let pa = a
            .argomenti
            .windows(2)
            .find(|w| w[0] == "--path")
            .map(|w| w[1].clone())
            .expect("--path mancante");
        assert_eq!(pa, "/corpus/a/prove/01.html");
        let b = Atto::promote(Path::new("/altra/copia"), "person_0000", "prove/01.html");
        let pb = b
            .argomenti
            .windows(2)
            .find(|w| w[0] == "--path")
            .map(|w| w[1].clone())
            .expect("--path mancante");
        assert_ne!(pa, pb, "due radici diverse devono leggere due testi diversi");
    }

    /// Un rifiuto è un verdetto, non un incidente: deve arrivare al banco con
    /// un **motivo**, e il motivo che la pipeline mette in modo esplicito sta
    /// nel corpo della risposta, non nello stderr.
    #[test]
    fn il_rifiuto_di_un_atto_ha_un_motivo() {
        let atto = Atto::promote(Path::new("/corpus"), "person_0000", "a.html");
        let corpo = r#"{"ok":false,"error":{"code":"store","message":"lo stato è Archiviato"}}"#;
        let m = motivo_di("kbs", &atto, 4, corpo, "rumore");
        assert_eq!(m, "store: lo stato è Archiviato");
        // E se il corpo non parla, lo stderr parla: un motivo non si perde.
        let m2 = motivo_di("kbs", &atto, 2, "", "kbs: uso");
        assert_eq!(m2, "kbs: uso");
        // E se non parla nessuno dei due, il motivo lo dice: un rifiuto
        // muto è comunque un rifiuto e va detto come tale.
        let m3 = motivo_di("kbs", &atto, 9, "", "");
        assert!(m3.contains("senza dire perché"), "{m3}");
        // Un atto riuscito non ha motivo.
        assert_eq!(motivo_di("kbs", &atto, 0, corpo, "rumore"), "");
    }

    /// La sessione è ciò che permette a due sequenze di parlare dello stesso
    /// database, ed è l'unica cosa che rende osservabile la seconda strada di
    /// D4. Il database sta **fuori** da qualunque radice di corpus.
    #[test]
    fn la_sessione_tiene_il_database_e_lo_tiene_fuori_dal_corpus() {
        let s = Session::nuova().expect("tempdir");
        assert!(!s.db().exists(), "il database non esiste prima del primo atto");
        assert!(s.db().starts_with(s.cartella()));
        assert!(!s.db().exists());
    }

    /// `esegui` è `sequenza` con un atto: due strade per la stessa operazione
    /// sarebbero due operazioni che possono divergere, e il banco deve
    /// riferire su una sola.
    #[test]
    fn la_pipeline_assente_rifiuta_anche_le_sequenze() {
        let p = PipelineAssente { ragione: "binario assente".into() };
        let s = Session::nuova().expect("tempdir");
        let out = p.sequenza(&Corpus::dalla_tabella(), &s, &[Atto::verify(Path::new("."))]);
        assert_eq!(out.unwrap_err().to_string(), "binario assente");
    }
}
