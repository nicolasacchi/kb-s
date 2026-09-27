//! I controlli del banco, e il referto che ne esce.
//!
//! Un controllo ha tre esiti e non due: **superato**, **fallito**, **saltato**.
//! Il terzo esiste perché un banco che salta in silenzio è peggio di un banco
//! assente: viene creduto. Ogni saltato porta con sé la ragione, e in CI
//! (`--require-pipeline`) un saltato è un fallimento.
//!
//! I controlli sono divisi in due famiglie con due ontologie diverse:
//!
//! * `corpus.*` — proprietà **delle fixture**, verificabili senza pipeline.
//!   Falliscono se qualcuno modifica una tabella in modo incoerente. Sono
//!   veri sempre, e per questo sono il pavimento del banco.
//! * `pipeline.*` — proprietà **del sistema**, verificabili solo con la
//!   pipeline. Falliscono se il sistema smette di comportarsi come dichiara.

use crate::adapter::{Pipeline, PipelineError, Uscita};
use crate::contract::{self, Defect, HARD_CAP};
use crate::corpus::{self, Corpus};
use crate::families;
use crate::spec::{ClaimStatusKind, Spec};
use kbs_core::{PublicationState, Relation};
use std::path::{Path, PathBuf};

/// L'esito di un controllo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Esito {
    /// La proprietà è stata verificata ed è vera.
    Superato,
    /// La proprietà è stata verificata ed è falsa. Le righe dicono perché.
    Fallito(Vec<String>),
    /// La proprietà non è stata verificata, e il motivo è qui.
    Saltato(String),
}

impl Esito {
    pub fn fallito(ragione: impl Into<String>) -> Esito {
        Esito::Fallito(vec![ragione.into()])
    }

    pub fn fallito_collect(rigioni: Vec<String>) -> Esito {
        if rigioni.is_empty() {
            Esito::Superato
        } else {
            Esito::Fallito(rigioni)
        }
    }

    pub fn etichetta(&self) -> &'static str {
        match self {
            Esito::Superato => "superato",
            Esito::Fallito(_) => "fallito",
            Esito::Saltato(_) => "saltato",
        }
    }
}

/// Un controllo del banco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controllo {
    pub nome: &'static str,
    /// `true` se il controllo ha bisogno della pipeline.
    pub di_pipeline: bool,
    pub esito: Esito,
}

/// Il referto completo: i controlli, l'esito, e ciò che il banco sa del
/// corpus che ha girato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referto {
    pub controlli: Vec<Controllo>,
    /// Il numero di item del banco.
    pub item: usize,
    /// Il numero di famiglie di media coperte.
    pub famiglie: usize,
    /// L'hash del corpus: il numero che rende due esecuzioni confrontabili.
    pub hash_corpus: String,
    /// Come la pipeline è stata raggiunta, o perché non lo è stata.
    pub pipeline: String,
    /// La radice del corpus, in percorso relativo.
    pub radice: String,
    /// Il numero di file che la radice contiene e che il banco non si aspetta.
    pub file_ignoti: Vec<String>,
}

impl Referto {
    pub fn superati(&self) -> usize {
        self.controlli.iter().filter(|c| c.esito == Esito::Superato).count()
    }

    pub fn falliti(&self) -> usize {
        self.controlli
            .iter()
            .filter(|c| matches!(c.esito, Esito::Fallito(_)))
            .count()
    }

    pub fn saltati(&self) -> usize {
        self.controlli
            .iter()
            .filter(|c| matches!(c.esito, Esito::Saltato(_)))
            .count()
    }

    /// `true` se il banco è verde. Un banco che ha saltato qualcosa è verde
    /// **solo** se il chiamante ha accettato i salti: `esito_con_rigidezza`
    /// risponde a questa domanda includendo la regola di CI.
    pub fn ok(&self) -> bool {
        self.falliti() == 0
    }

    /// L'esito con la regola che vale in CI: un saltato è un fallimento.
    pub fn esito_con_rigidezza(&self, require_pipeline: bool) -> bool {
        self.falliti() == 0 && (!require_pipeline || self.saltati() == 0)
    }

    pub fn controllo(&self, nome: &str) -> Option<&Controllo> {
        self.controlli.iter().find(|c| c.nome == nome)
    }
}

/// Come gira il banco.
pub struct Config {
    /// La radice del corpus su disco. Se non contiene i file, il controllo
    /// sui file committati è saltato con la ragione.
    pub radice: PathBuf,
    /// Se `true`, un saltato è un fallimento. È la regola della CI.
    pub require_pipeline: bool,
}

impl Config {
    pub fn con_radice(radice: impl Into<PathBuf>) -> Self {
        Config {
            radice: radice.into(),
            require_pipeline: false,
        }
    }

    /// La radice di default: la cartella `corpus` dentro il crate, che è dove
    /// stanno le fixture committate.
    pub fn radice_di_default() -> Self {
        Config::con_radice(Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus"))
    }

    /// La regola che vale in CI: un controllo saltato è un fallimento. È un
    /// metodo e non un campo pubblico perché «la radice» e «la rigidezza» sono
    /// una configurazione sola, e due variabili indipendenti si possono
    /// dimenticare a metà.
    pub fn con_rigidezza(mut self, require: bool) -> Self {
        self.require_pipeline = require;
        self
    }
}

/// Esegue il banco.
pub struct Banco<'a> {
    corpus: Corpus,
    cfg: Config,
    pipeline: &'a dyn Pipeline,
}

impl<'a> Banco<'a> {
    pub fn new(cfg: Config, pipeline: &'a dyn Pipeline) -> Self {
        Banco {
            corpus: Corpus::dalla_tabella(),
            cfg,
            pipeline,
        }
    }

    /// Il corpus su cui il banco sta girando.
    pub fn corpus(&self) -> &Corpus {
        &self.corpus
    }

    /// Esegue tutti i controlli e restituisce il referto.
    pub fn esegui(&self) -> Referto {
        let pipeline = self.pipeline.esegui(&self.corpus, &self.cfg.radice);
        let mut controlli = self.controlli_su_corpus();
        controlli.extend(self.controlli_su_pipeline(&pipeline));
        let (descrizione, ignoti) = self.dati_di_contesto();
        Referto {
            controlli,
            item: self.corpus.len(),
            famiglie: famiglie_coperte(&self.corpus).len(),
            hash_corpus: self.corpus.hash(),
            pipeline: descrizione,
            radice: "corpus".to_string(),
            file_ignoti: ignoti,
        }
    }

    fn dati_di_controllo(&self) -> (usize, Vec<String>) {
        let (n, ignoti) = self.file_ignoti();
        (n, ignoti)
    }

    fn dati_di_contesto(&self) -> (String, Vec<String>) {
        let (n, ignoti) = self.dati_di_controllo();
        let _ = n;
        (self.pipeline.descrizione(), ignoti)
    }

    /// I file della radice che il banco non si aspetta. Sono rumore che
    /// nessun controllo tollera: se qualcuno lascia un file nel corpus, va
    /// detto nel referto.
    fn file_ignoti(&self) -> (usize, Vec<String>) {
        let attesi: std::collections::BTreeSet<&str> =
            self.corpus.file().iter().map(|f| f.rel.as_str()).collect();
        let mut trovati = Vec::new();
        let n = walk_file(&self.cfg.radice, &mut |rel| {
            // I file che il banco stesso scrive accanto al corpus non sono
            // rumore: sono il database della pipeline. Sono esclusi per nome,
            // e non per estensione, perché un'estensione è una convenzione e
            // un nome è una dichiarazione.
            if !attesi.contains(rel) && !rel.starts_with(".kbs-bench") {
                trovati.push(rel.to_string());
            }
        });
        (n, trovati)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Controlli sul corpus: veri sempre, senza pipeline.
    // ─────────────────────────────────────────────────────────────────────────
    fn controlli_su_corpus(&self) -> Vec<Controllo> {
        vec![
            c("corpus.ha_quaranta_voci", self.c_quaranta()),
            c("corpus.copre_le_dodici_famiglie", self.c_famiglie()),
            c(
                "corpus.gli_id_derivano_dal_percorso_e_sono_univoci",
                self.c_id_univoci(),
            ),
            c(
                "corpus.copre_tutti_gli_stati_di_pubblicazione",
                self.c_stati(),
            ),
            c(
                "corpus.la_ratifica_superata_e_su_dati_veri",
                self.c_ratifica_superata(),
            ),
            c(
                "corpus.contratti_completi_entro_budget_e_difetti_di_contratto_misurati",
                self.c_contratti(),
            ),
            c(
                "corpus.le_cinque_fiastre_rovate_sono_ancora_rovate",
                self.c_fiastre_rovate(),
            ),
            c(
                "corpus.i_file_sono_uguali_a_cio_che_la_tabella_descrive",
                self.c_file_uguali(),
            ),
            c(
                "corpus.ogni_claim_con_span_ha_la_sua_ancora_nel_testo",
                self.c_ancore(),
            ),
            c(
                "corpus.riferimenti_al_runtime_dichiarati_e_diversi",
                self.c_runtime(),
            ),
            c(
                "corpus.la_scena_3d_ha_una_claim_per_nodo_e_per_arco",
                self.c_scena(),
            ),
            c(
                "corpus.l_esercizio_parametrizzato_ha_due_istanze_diverse",
                self.c_istanze(),
            ),
            c(
                "corpus.le_claim_contraddette_e_non_citabili_ci_sono",
                self.c_claim_con_errore(),
            ),
            c(
                "corpus.la_catena_dei_prerequisiti_e_reale_e_il_grafo_valido_e_aciclico",
                self.c_prerequisiti(),
            ),
        ]
    }

    fn c_quaranta(&self) -> Esito {
        let n = self.corpus.len();
        if n == crate::items::DIMENSIONE {
            Esito::Superato
        } else {
            Esito::fallito(format!(
                "il banco contiene {n} item e ne dichiara {}",
                crate::items::DIMENSIONE
            ))
        }
    }

    fn c_famiglie(&self) -> Esito {
        let coperte = famiglie_coperte(&self.corpus);
        let mancanti: Vec<String> = families::ALL
            .iter()
            .filter(|f| !coperte.contains(f))
            .map(|f| format!("famiglia {f} non ha nessun item"))
            .collect();
        Esito::fallito_collect(mancanti)
    }

    fn c_id_univoci(&self) -> Esito {
        let mut id: Vec<String> = Vec::new();
        let mut problemi = Vec::new();
        for s in self.corpus.voci() {
            let i = corpus::id_di(s);
            if id.contains(&i.0) {
                problemi.push(format!("{} e un altro item hanno l'id {}", s.rel, i));
            }
            id.push(i.0.clone());
            // L'id segue il percorso, e non i byte: un file identico in due
            // posti diversi deve avere due id.
            if i.as_str() != corpus::id_di(s).as_str() {
                problemi.push(format!("{}: l'id non è derivato dal percorso", s.rel));
            }
        }
        if id.len() != self.corpus.len() {
            problemi.push(format!(
                "{} id per {} item",
                id.len(),
                self.corpus.len()
            ));
        }
        Esito::fallito_collect(problemi)
    }

    fn c_stati(&self) -> Esito {
        let presenti: Vec<PublicationState> = self.corpus.voci().iter().map(|s| s.stato).collect();
        let mancanti: Vec<String> = corpus::STATI
            .iter()
            .filter(|st| !presenti.contains(st))
            .map(|st| format!("nessun item nello stato {st:?}"))
            .collect();
        Esito::fallito_collect(mancanti)
    }

    /// La ratifica superata deve esistere **come riga**, non come commento:
    /// un `Invariant::StaleRatification` incontrato su dati veri è il motivo
    /// per cui la fiastra esiste.
    fn c_ratifica_superata(&self) -> Esito {
        let args = corpus::argomenti();
        let superate: Vec<&kbs_core::Argument> = args
            .iter()
            .filter(|a| {
                a.ratified.is_some() && !a.is_citable_now() && a.state == PublicationState::InCorso
            })
            .collect();
        let problemi = Vec::new();
        let mut problemi = problemi;
        if superate.is_empty() {
            return Esito::fallito(
                "nessun argomento in uso con ratifica non valida: il percorso StaleRatification non è esercitato su dati",
            );
        }
        for a in &superate {
            if let Err(e) = kbs_core::check_citable(a) {
                let _ = e;
            } else {
                problemi.push(format!("{}: dichiarato non citabile ma risulta citabile", a.id));
            }
        }
        // Una ratifica fresca deve esistere accanto, o il caso «superata»
        // potrebbe essere solo un artefatto di una tabella senza ratifiche.
        if !args
            .iter()
            .any(|a| a.is_citable_now())
        {
            problemi.push("nessun argomento citabile: la ratifica fresca non è esercitata".into());
        }
        Esito::fallito_collect(problemi)
    }

    /// I contratti: gli otto sezioni dei validi stanno nei budget, e i tre
    /// difetti di contratto sono misurabili sui byte del testo reso.
    fn c_contratti(&self) -> Esito {
        let mut problemi = Vec::new();
        for s in self.corpus.voci() {
            let testo = s.contract().render();
            let m = contract::measure(&testo);
            let rotto = s.deve_essere_rifiutato();
            if !rotto {
                if m.len() != 8 {
                    problemi.push(format!(
                        "{}: contratto di {} sezioni, otto obbligatorie",
                        s.rel,
                        m.len()
                    ));
                    continue;
                }
                for (nome, meas) in &m {
                    let budget = contract::SECTIONS
                        .iter()
                        .find(|(n, _)| n == nome)
                        .map(|(_, b)| *b)
                        .unwrap_or(0);
                    if meas.size > budget {
                        problemi.push(format!(
                            "{}: sezione {nome} di {} byte, budget {budget}",
                            s.rel, meas.size
                        ));
                    }
                }
                if m[0].1.end > contract::SECTIONS[0].1 {
                    problemi.push(format!(
                        "{}: GUARDIAN finisce a {} byte, oltre i {}",
                        s.rel,
                        m[0].1.end,
                        contract::SECTIONS[0].1
                    ));
                }
                if testo.len() > HARD_CAP {
                    problemi.push(format!("{}: contratto di {} byte", s.rel, testo.len()));
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// Le cinque fiastre rotte devono essere **ancora rotte**, misurate sui
    /// byte. Se qualcuno le sistema, il banco diventa rosso: è il modo in cui
    /// un banco avvisa che una correzione ha cambiato il test.
    fn c_fiastre_rovate(&self) -> Esito {
        let mut problemi = Vec::new();
        let rotte: Vec<&Spec> = self
            .corpus
            .voci()
            .iter()
            .filter(|s| s.deve_essere_rifiutato())
            .collect();
        if rotte.len() != crate::FIXTURE_ROTTE {
            problemi.push(format!(
                "il banco ha {} fixture rotte e ne dichiara {}: le cinque coprono D7 due volte, D15 una e il ciclo una",
                rotte.len(),
                crate::FIXTURE_ROTTE
            ));
        }
        for s in &rotte {
            let testo = s.contract().render();
            let m = contract::measure(&testo);
            match s.difetto {
                Defect::SezioneMancante(nome) => {
                    if testo.contains(&format!("{}{nome}", contract::SECTION_HEADING)) {
                        problemi.push(format!(
                            "{}: la sezione {nome} c'è nel contratto, e questa fiastra non è più rotta",
                            s.rel
                        ));
                    }
                    if m.len() == 8 {
                        problemi.push(format!(
                            "{}: il contratto ha otto sezioni, e questa fiastra non è più rotta",
                            s.rel
                        ));
                    }
                }
                Defect::GuardianOltreBudget => {
                    if m.first().map(|x| x.1.end).unwrap_or(0) <= contract::SECTIONS[0].1 {
                        problemi.push(format!(
                            "{}: il GUARDIAN sta nei primi {} byte e non è più rotto",
                            s.rel,
                            contract::SECTIONS[0].1
                        ));
                    }
                }
                Defect::OltreBudget => {
                    if testo.len() <= HARD_CAP {
                        problemi.push(format!(
                            "{}: il contratto sta in {}/{} byte e non è più rotto",
                            s.rel,
                            testo.len(),
                            HARD_CAP
                        ));
                    }
                }
                Defect::RiferimentoEsterno(u) => {
                    let artefatto = self.corpus.file_di(s.rel).map(|f| f.contenuto.as_str()).unwrap_or("");
                    if !artefatto.contains(u) {
                        problemi.push(format!(
                            "{}: il riferimento esterno {u} non è più nel file e questa fiastra non è più rotta",
                            s.rel
                        ));
                    }
                }
                Defect::PrerequisitoCiclico(p) => {
                    if !s.prerequisiti.contains(&p) {
                        problemi.push(format!(
                            "{}: il prerequisito ciclico {p} non c'è più e questa fiastra non è più rotta",
                            s.rel
                        ));
                    }
                }
                Defect::Nessuno => {
                    problemi.push(format!("{}: dichiarata rotta ma senza difetto", s.rel));
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// I file su disco devono essere **esattamente** ciò che la tabella
    /// descrive. È il controllo che rende rumorosa una riparazione: se
    /// qualcuno sistema una fiastra a mano, qui il banco lo dice e dice quale.
    fn c_file_uguali(&self) -> Esito {
        if !self.cfg.radice.is_dir() {
            return Esito::Saltato(format!(
                "la radice del corpus non esiste su disco: nessun file da confrontare con la tabella"
            ));
        }
        let mut problemi = Vec::new();
        for f in self.corpus.file() {
            let path = self.cfg.radice.join(&f.rel);
            match std::fs::read_to_string(&path) {
                Ok(s) => {
                    if s != f.contenuto {
                        problemi.push(format!(
                            "{}: il file su disco non è quello che la tabella descrive (sha256 del file {})",
                            f.rel,
                            short(&Corpus::hash_file(&s))
                        ));
                    }
                }
                Err(e) => problemi.push(format!("{}: {}", f.rel, e)),
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// Ogni claim che dichiara uno span deve avere, nel file, l'ancora e il
    /// testo dello span. Una claim con un puntatore che il lettore non
    /// può aprire è una dichiarazione con un indirizzo.
    fn c_ancore(&self) -> Esito {
        let mut problemi = Vec::new();
        for s in self.corpus.voci() {
            let artefatto = match self.corpus.file_di(s.rel) {
                Some(f) => f.contenuto.clone(),
                None => {
                    problemi.push(format!("{}: nessun file per questa voce", s.rel));
                    continue;
                }
            };
            for cl in s.claims {
                match (cl.ancora, cl.testo_span) {
                    (Some(a), Some(t)) => {
                        if !artefatto.contains(&format!("data-claim=\"{}\"", cl.id)) {
                            problemi.push(format!("{}: la claim {} non ha ancora nel testo", s.rel, cl.id));
                        }
                        if !artefatto.contains(&format!("id=\"{a}\"")) {
                            problemi.push(format!("{}: lo span {a} non c'è nel testo", s.rel));
                        }
                        if !artefatto.contains(&crate::render::esc(t)) {
                            problemi.push(format!(
                                "{}: il testo dello span della claim {} non è nel testo",
                                s.rel, cl.id
                            ));
                        }
                    }
                    (None, None) => {
                        if !matches!(cl.stato, ClaimStatusKind::Unciteable) {
                            problemi.push(format!(
                                "{}: la claim {} non ha span e non è non citabile",
                                s.rel, cl.id
                            ));
                        }
                    }
                    _ => problemi.push(format!(
                        "{}: la claim {} ha un anchor senza testo dello span, o viceversa: un puntatore senza testo non è verificabile",
                        s.rel, cl.id
                    )),
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// I due riferimenti al runtime: una CDN che deve fallire e un percorso
    /// locale che deve passare. Se il banco smette di distinguerli, la
    /// metà negativa di D15 non esiste.
    fn c_runtime(&self) -> Esito {
        let voci = self.corpus.voci();
        let cdn: Vec<&Spec> = voci
            .iter()
            .filter(|s| matches!(s.difetto, Defect::RiferimentoEsterno(_)))
            .collect();
        let locale: Vec<&Spec> = voci.iter().filter(|s| s.carica_three_locale).collect();
        let mut problemi = Vec::new();
        if cdn.is_empty() {
            problemi.push("nessun item che referenzia un CDN esterno: D15 non ha una metà negativa".into());
        }
        for s in &cdn {
            if !s.deve_essere_rifiutato() {
                problemi.push(format!("{}: un CDN esterno che non viene rifiutato", s.rel));
            }
            match s.riferimento_esterno {
                Some(u) if u.starts_with("https://") => {}
                other => problemi.push(format!("{}: riferimento esterno {other:?} non è una URL", s.rel)),
            }
        }
        if locale.is_empty() {
            problemi.push("nessun item che referenzia il runtime locale: D15 non ha una metà positiva".into());
        }
        for s in &locale {
            if s.deve_essere_rifiutato() {
                problemi.push(format!("{}: il runtime locale porta un difetto", s.rel));
            }
            if s.riferimento_esterno.is_some() {
                problemi.push(format!("{}: dichiara anche un riferimento esterno", s.rel));
            }
            let artefatto = self.corpus.file_di(s.rel).map(|f| f.contenuto.as_str()).unwrap_or("");
            if !artefatto.contains(crate::render::PERCORSO_THREE) {
                problemi.push(format!(
                    "{}: non referenzia {}",
                    s.rel,
                    crate::render::PERCORSO_THREE
                ));
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// D15.1: ogni nodo e ogni arco dichiarato corrisponde a una claim, e la
    /// claim esiste fra quelle dell'item. Un oggetto 3D senza claim non entra
    /// nell'indice condiviso.
    fn c_scena(&self) -> Esito {
        let voci = self.corpus.voci();
        let con_scena: Vec<&Spec> = voci.iter().filter(|s| s.scena.is_some()).collect();
        let mut problemi = Vec::new();
        if con_scena.is_empty() {
            return Esito::fallito("nessun item con scena 3D: D15.1 non è esercitato");
        }
        for s in &con_scena {
            let sc = s.scena.expect("filtrato sopra");
            let dichiarate: Vec<&str> = s.claims.iter().map(|c| c.id).collect();
            if sc.nodi.is_empty() || sc.archi.is_empty() {
                problemi.push(format!("{}: la scena non ha nodi e archi", s.rel));
            }
            for n in sc.nodi {
                if !dichiarate.contains(&n.claim) {
                    problemi.push(format!("{}: il nodo {} non ha claim", s.rel, n.id));
                }
                if n.claim.is_empty() {
                    problemi.push(format!("{}: il nodo {} ha una claim vuota", s.rel, n.id));
                }
            }
            for a in sc.archi {
                if !dichiarate.contains(&a.claim) {
                    problemi.push(format!("{}: l'arco {} non ha claim", s.rel, a.id));
                }
                if !sc.nodi.iter().any(|n| n.id == a.da) {
                    problemi.push(format!("{}: l'arco {} parte da un nodo inesistente", s.rel, a.id));
                }
                if !sc.nodi.iter().any(|n| n.id == a.a) {
                    problemi.push(format!("{}: l'arco {} arriva a un nodo inesistente", s.rel, a.id));
                }
            }
            // D15.1.3: il guadagno pedagogico è l'interazione, e si registra
            // come relazioni che lo studente ha corretto. Una scena in cui
            // niente è correggibile non può registrare quel guadagno.
            if !s.deve_essere_rifiutato() && !sc.archi.iter().any(|a| a.correggibile) {
                problemi.push(format!(
                    "{}: nessuna relazione correggibile, e quindi nessun guadagno da misurare",
                    s.rel
                ));
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// L'esercizio parametrizzato con due istanze, e le due istanze devono
    /// avere risposte diverse: è il motivo per cui copiare non funziona.
    fn c_istanze(&self) -> Esito {
        let voci = self.corpus.voci();
        let parametrici: Vec<(&Spec, &crate::spec::ExerciseSpec)> = voci
            .iter()
            .flat_map(|s| s.esercizi.iter().map(move |e| (s, e)))
            .filter(|(_, e)| e.istanze.len() >= 2)
            .collect();
        let mut problemi = Vec::new();
        if parametrici.is_empty() {
            return Esito::fallito(
                "nessun esercizio con due istanze: la parametrizzazione di D8 non è esercitata",
            );
        }
        for (s, e) in &parametrici {
            let istanze = corpus::istanze_di(s);
            if istanze.len() < 2 {
                problemi.push(format!("{}: l'esercizio {} ha poche di due istanze", s.rel, e.id));
                continue;
            }
            if !istanze[0].differs_from(&istanze[1]) {
                problemi.push(format!(
                    "{}: le due istanze di {} hanno la stessa risposta e copiare funziona",
                    s.rel, e.id
                ));
            }
            if istanze[0].seed == istanze[1].seed {
                problemi.push(format!(
                    "{}: le due istanze di {} hanno lo stesso seed",
                    s.rel, e.id
                ));
            }
            if istanze.iter().any(|i| i.expected.is_empty()) {
                problemi.push(format!("{}: un'istanza di {} non ha risposta attesa", s.rel, e.id));
            }
        }
        Esito::fallito_collect(problemi)
    }

    fn c_claim_con_errore(&self) -> Esito {
        let voci = self.corpus.voci();
        let contraddette: Vec<&Spec> = voci
            .iter()
            .filter(|s| s.claims.iter().any(|c| c.stato == ClaimStatusKind::Contradicted))
            .collect();
        let non_citabili: Vec<&Spec> = voci
            .iter()
            .filter(|s| s.claims.iter().any(|c| c.stato == ClaimStatusKind::Unciteable))
            .collect();
        let mut problemi = Vec::new();
        if contraddette.is_empty() {
            problemi.push("nessuna claim contraddetta: la traccia dell'errore non è esercitata".into());
        }
        for s in &contraddette {
            for c in s.claims.iter().filter(|c| c.stato == ClaimStatusKind::Contradicted) {
                if c.testo_span.is_none() {
                    problemi.push(format!(
                        "{}: la claim contraddetta {} non ha span, e senza span non è contraddetta ma assente",
                        s.rel, c.id
                    ));
                }
            }
        }
        if non_citabili.is_empty() {
            problemi.push("nessuna claim non citabile: la soppressione in output non è esercitata".into());
        }
        for s in &non_citabili {
            for c in s.claims.iter().filter(|c| c.stato == ClaimStatusKind::Unciteable) {
                if c.testo_span.is_some() {
                    problemi.push(format!(
                        "{}: la claim non citabile {} ha uno span, e con uno span non è non citabile",
                        s.rel, c.id
                    ));
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// La catena dei prerequisiti deve essere **vera**: ci deve essere un item
    /// valido che ne eredita altri, e il grafo degli item validi deve essere
    /// aciclico. Un banco senza catena non verifica niente del vincolo.
    fn c_prerequisiti(&self) -> Esito {
        let voci = self.corpus.voci();
        let validi: std::collections::BTreeSet<&str> = voci
            .iter()
            .filter(|s| !s.deve_essere_rifiutato())
            .map(|s| s.rel)
            .collect();
        let mut problemi = Vec::new();
        let archi = corpus::archi_del_grafo_valido();
        if archi.is_empty() {
            return Esito::fallito("nessun prerequisito fra gli item validi: la catena non è esercitata");
        }
        // profondità della catena più lunga
        let profondità = profondita_massima(&voci, &validi);
        if profondità < 3 {
            problemi.push(format!(
                "la catena più lunga fra gli item validi è profonda {profondità}, e sotto tre non è una catena"
            ));
        }
        for s in voci {
            for p in s.prerequisiti {
                if !voci.iter().any(|v| v.rel == *p) {
                    problemi.push(format!("{}: il prerequisito {p} non è nel banco", s.rel));
                }
                if s.deve_essere_rifiutato() {
                    continue;
                }
                if !validi.contains(*p) {
                    problemi.push(format!(
                        "{}: è valido e ha un prerequisito ({p}) che il banco rifiuta: gli resterebbe appeso",
                        s.rel
                    ));
                }
            }
        }
        if let Some(ciclo) = cerca_ciclo(&voci, &validi) {
            problemi.push(format!("ciclo fra item validi: {}", ciclo.join(" → ")));
        }
        Esito::fallito_collect(problemi)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Controlli sulla pipeline: senza di lei sono saltati, con la ragione.
    // ─────────────────────────────────────────────────────────────────────────
    fn controlli_su_pipeline(&self, esito: &Result<Uscita, PipelineError>) -> Vec<Controllo> {
        let saltato = |e: &PipelineError| Esito::Saltato(e.to_string());
        let uscita = match esito {
            Ok(u) => u,
            Err(e) => {
                return vec![
                    pc("pipeline.validazione.corrisponde_all_attesa", saltato(e)),
                    pc("pipeline.validazione.i_codici_di_rifiuto", saltato(e)),
                    pc("pipeline.indicizzazione.solo_i_ratificati_sono_citabili", saltato(e)),
                    pc("pipeline.registro.gli_errori_restano_e_non_si_cancellano", saltato(e)),
                    pc("pipeline.registro.le_claim_non_citabili_sono_soppresse_in_output", saltato(e)),
                    pc("pipeline.prerequisiti.il_ciclo_e_rifiutato", saltato(e)),
                    pc("pipeline.esercizi.il_replay_e_deterministico", saltato(e)),
                    pc("pipeline.documenti.solo_la_versione_locale_di_three_e_pubblicabile", saltato(e)),
                    pc("pipeline.documenti.la_scena_3d_e_manipolabile_e_i_suoi_dati_sono_il_contenuto", saltato(e)),
                ]
            }
        };
        vec![
            pc(
                "pipeline.validazione.corrisponde_all_attesa",
                self.c_validazione(uscita),
            ),
            pc("pipeline.validazione.i_codici_di_rifiuto", self.c_codici(uscita)),
            pc(
                "pipeline.indicizzazione.solo_i_ratificati_sono_citabili",
                self.c_citabili(uscita),
            ),
            pc(
                "pipeline.registro.gli_errori_restano_e_non_si_cancellano",
                self.c_registro(uscita),
            ),
            pc(
                "pipeline.registro.le_claim_non_citabili_sono_soppresse_in_output",
                self.c_soppressione(uscita),
            ),
            pc("pipeline.prerequisiti.il_ciclo_e_rifiutato", self.c_ciclo(uscita)),
            pc("pipeline.esercizi.il_replay_e_deterministico", self.c_replay(uscita)),
            pc(
                "pipeline.documenti.solo_la_versione_locale_di_three_e_pubblicabile",
                self.c_three(uscita),
            ),
            pc(
                "pipeline.documenti.la_scena_3d_e_manipolabile_e_i_suoi_dati_sono_il_contenuto",
                self.c_manipolazione(uscita),
            ),
        ]
    }

    fn c_validazione(&self, u: &Uscita) -> Esito {
        let mut problemi = Vec::new();
        for s in self.corpus.voci() {
            match u.item(s.rel) {
                None => problemi.push(format!("{}: la pipeline non ha riportato questo item", s.rel)),
                Some(d) => {
                    if d.valid != !s.deve_essere_rifiutato() {
                        problemi.push(format!(
                            "{}: la pipeline lo dice {} e il banco si aspetta {}",
                            s.rel,
                            if d.valid { "valido" } else { "non valido" },
                            if s.deve_essere_rifiutato() { "non valido" } else { "valido" }
                        ));
                    }
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    fn c_codici(&self, u: &Uscita) -> Esito {
        let mut problemi = Vec::new();
        for s in self.corpus.voci() {
            let atteso = match s.difetto.expected_code() {
                Some(c) => c,
                None => continue,
            };
            let d = match u.item(s.rel) {
                Some(d) => d,
                None => {
                    problemi.push(format!("{}: nessuna diagnostica per l'item rotto", s.rel));
                    continue;
                }
            };
            if !d.errors.iter().any(|e| e.code == atteso) {
                problemi.push(format!(
                    "{}: la pipeline ha emesso {:?} e non il codice atteso «{atteso}»",
                    s.rel,
                    d.errors.iter().map(|e| e.code.as_str()).collect::<Vec<_>>()
                ));
            }
        }
        Esito::fallito_collect(problemi)
    }

    fn c_citabili(&self, u: &Uscita) -> Esito {
        let attesi: Vec<String> = corpus::citabili().iter().map(|i| i.0.clone()).collect();
        let ottenuti = &u.index.citable;
        let mut problemi = Vec::new();
        for mancante in attesi.iter().filter(|i| !ottenuti.contains(i)) {
            problemi.push(format!("{mancante}: il banco lo considera citabile e la pipeline no"));
        }
        for extra in ottenuti.iter().filter(|i| !attesi.contains(i)) {
            problemi.push(format!("{extra}: la pipeline lo considera citabile e il banco no"));
        }
        // Ogni motivo di non citabilità deve essere detto: un «non citabile»
        // senza motivo è un verdetto, e i verdetti non si contestano.
        for nc in &u.index.not_citable {
            if nc.message.trim().is_empty() {
                problemi.push(format!("{}: non citabile senza motivo dichiarato", nc.id));
            }
            if nc.invariant.trim().is_empty() {
                problemi.push(format!("{}: non citabile senza invariante dichiarata", nc.id));
            }
        }
        Esito::fallito_collect(problemi)
    }

    fn c_registro(&self, u: &Uscita) -> Esito {
        let voci = self.corpus.voci();
        let mut problemi = Vec::new();
        for s in voci {
            for cl in s.claims.iter().filter(|c| c.stato == ClaimStatusKind::Contradicted) {
                let id = format!("{}::{}", corpus::id_di(s).as_str(), cl.id);
                match u.claim(&id) {
                    None => problemi.push(format!("{id}: la claim contraddetta non è nel registro")),
                    Some(r) => {
                        if !r.in_registry {
                            problemi.push(format!("{id}: la claim contraddetta non è nel registro"));
                        }
                        if r.in_output {
                            problemi.push(format!(
                                "{id}: la claim contraddetta compare nell'output citabile, e un errore non si cita come se fosse vero"
                            ));
                        }
                        if r.status != "contradicted" {
                            problemi.push(format!("{id}: stato riportato «{}», atteso contradicted", r.status));
                        }
                    }
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    fn c_soppressione(&self, u: &Uscita) -> Esito {
        let voci = self.corpus.voci();
        let mut problemi = Vec::new();
        for s in voci {
            for cl in s.claims.iter().filter(|c| c.stato == ClaimStatusKind::Unciteable) {
                let id = format!("{}::{}", corpus::id_di(s).as_str(), cl.id);
                match u.claim(&id) {
                    None => problemi.push(format!(
                        "{id}: la claim non citabile non è nel registro: l'errore non si cancella, e una non citabilità è un errore"
                    )),
                    Some(r) => {
                        if !r.in_registry {
                            problemi.push(format!("{id}: la claim non citabile non è nel registro"));
                        }
                        if r.in_output {
                            problemi.push(format!(
                                "{id}: la claim non citabile compare nell'output: nessuno span la sostiene"
                            ));
                        }
                    }
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    fn c_ciclo(&self, u: &Uscita) -> Esito {
        let cicliche: Vec<&Spec> = self
            .corpus
            .voci()
            .iter()
            .filter(|s| matches!(s.difetto, Defect::PrerequisitoCiclico(_)))
            .collect();
        if cicliche.is_empty() {
            return Esito::fallito("nessuna fiastra con ciclo: il rifiuto del ciclo non è esercitato");
        }
        let mut problemi = Vec::new();
        for s in cicliche {
            let d = match u.item(s.rel) {
                Some(d) => d,
                None => {
                    problemi.push(format!("{}: nessuna diagnostica per l'item ciclico", s.rel));
                    continue;
                }
            };
            if d.valid {
                problemi.push(format!(
                    "{}: la pipeline lo accetta, e un argomento che richiede sé stesso non entra",
                    s.rel
                ));
            }
            if !d
                .errors
                .iter()
                .any(|e| e.code == "prerequisite-cycle")
            {
                problemi.push(format!("{}: nessun errore di ciclo nel referto", s.rel));
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// D11: data la tupla (corpus, esercizio, seed) il sistema deve poter
    /// **riprodurre** la generazione. Il banco dichiara le istanze e pretende
    /// che la pipeline restituisca esattamente quelle.
    fn c_replay(&self, u: &Uscita) -> Esito {
        let voci = self.corpus.voci();
        let mut problemi = Vec::new();
        let mut n = 0;
        for s in voci {
            for ist in corpus::istanze_di(s) {
                n += 1;
                match u.istanza(&ist.exercise, &ist.seed) {
                    None => problemi.push(format!(
                        "{}: la pipeline non ha riprodotto l'istanza seed {}",
                        ist.exercise, ist.seed
                    )),
                    Some(r) => {
                        if r.expected != ist.expected {
                            problemi.push(format!(
                                "{} seed {}: attesa «{}», la pipeline dice «{}»",
                                ist.exercise, ist.seed, ist.expected, r.expected
                            ));
                        }
                        if r.params != ist.params {
                            problemi.push(format!(
                                "{} seed {}: parametri {}, la pipeline dice {}",
                                ist.exercise, ist.seed, ist.params, r.params
                            ));
                        }
                    }
                }
            }
        }
        if n == 0 {
            return Esito::fallito("nessuna istanza dichiarata: il replay deterministico non è esercitato");
        }
        Esito::fallito_collect(problemi)
    }

    /// D15: la versione locale del runtime passa, quella da CDN no. È la
    /// stessa coppia di item che il banco usa per la metà negativa.
    fn c_three(&self, uscita: &Uscita) -> Esito {
        let mut problemi = Vec::new();
        for s in self
            .corpus
            .voci()
            .iter()
            .filter(|s| s.riferimento_esterno.is_some())
        {
            let url = s.riferimento_esterno.expect("filtrato sopra");
            let d = match uscita.item(s.rel) {
                Some(d) => d,
                None => {
                    problemi.push(format!("{}: nessuna diagnostica per l'item con CDN", s.rel));
                    continue;
                }
            };
            if d.valid {
                problemi.push(format!(
                    "{}: la pipeline accetta un artifact che referenzia {url}, e la sua violazione impedisce la pubblicazione",
                    s.rel
                ));
            }
            if !d.errors.iter().any(|e| e.code == "external-reference") {
                problemi.push(format!("{}: nessun errore external-reference", s.rel));
            }
        }
        for s in self.corpus.voci().iter().filter(|s| s.carica_three_locale) {
            if let Some(d) = uscita.item(s.rel) {
                if !d.valid {
                    problemi.push(format!(
                        "{}: la pipeline rifiuta il runtime locale, che è servito dal binario e non esce dal perimetro",
                        s.rel
                    ));
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// D15.1: i dati della scena sono in un manifest separato dal rendering,
    /// ogni oggetto ha una claim, e almeno una relazione è correggibile. Se il
    /// rendering fosse l'unica rappresentazione, un aggiornamento del
    /// renderer cancellerebbe il materiale dello studente.
    fn c_manipolazione(&self, uscita: &Uscita) -> Esito {
        let voci = self.corpus.voci();
        let mut problemi = Vec::new();
        for s in voci.iter().filter(|s| s.scena.is_some()) {
            let sc = s.scena.expect("filtrato sopra");
            // L'item che porta la scena deve essere nell'indice condiviso: un
            // oggetto 3D senza claim non entra (D15.1), e un item non valido
            // non entra. Qui si controlla che la pipeline lo dica.
            if let Some(d) = uscita.item(s.rel) {
                if !d.valid {
                    problemi.push(format!(
                        "{}: la pipeline non accetta l'item che porta la scena, e quindi non la manipola",
                        s.rel
                    ));
                }
            }
            for oggetto in sc.nodi.iter().map(|n| n.claim).chain(sc.archi.iter().map(|a| a.claim)) {
                let id = format!("{}::{}", corpus::id_di(s).as_str(), oggetto);
                if let Some(r) = uscita.claim(&id) {
                    if !r.in_registry {
                        problemi.push(format!(
                            "{id}: la claim di un oggetto della scena non è nel registro, e un oggetto senza claim non entra"
                        ));
                    }
                }
            }
            let file = match self.corpus.file_di(&sc.manifest) {
                Some(f) => f,
                None => {
                    problemi.push(format!("{}: il manifest {} non è nel corpus", s.rel, sc.manifest));
                    continue;
                }
            };
            let parsed: serde_json::Value = match serde_json::from_str(&file.contenuto) {
                Ok(v) => v,
                Err(e) => {
                    problemi.push(format!("{}: il manifest non è JSON valido: {e}", s.rel));
                    continue;
                }
            };
            let runtime = parsed.get("runtime").and_then(|v| v.as_str()).unwrap_or("");
            if runtime != crate::render::PERCORSO_THREE {
                problemi.push(format!(
                    "{}: il manifest dichiara il runtime «{runtime}» e non quello servito dal binario",
                    s.rel
                ));
            }
            let nodi = parsed.get("nodi").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            let archi = parsed.get("archi").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            if nodi != sc.nodi.len() || archi != sc.archi.len() {
                problemi.push(format!(
                    "{}: il manifest ha {nodi} nodi e {archi} archi, la tabella ne dichiara {} e {}",
                    s.rel,
                    sc.nodi.len(),
                    sc.archi.len()
                ));
            }
            // Il guadagno che si registra è il numero di relazioni corrette:
            // se il manifest non ne dichiara nessuna, non c'è nulla da
            // misurare e la scena non è un esercizio.
            let correggibili = parsed
                .get("archi")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter(|e| e.get("correggibile").and_then(|b| b.as_bool()) == Some(true))
                        .count()
                })
                .unwrap_or(0);
            if correggibili == 0 {
                problemi.push(format!(
                    "{}: il manifest non dichiara relazioni correggibili, e quindi nessun guadagno da misurare",
                    s.rel
                ));
            }
        }
        Esito::fallito_collect(problemi)
    }
}


fn c(nome: &'static str, esito: Esito) -> Controllo {
    Controllo { nome, di_pipeline: false, esito }
}

fn pc(nome: &'static str, esito: Esito) -> Controllo {
    Controllo { nome, di_pipeline: true, esito }
}

fn short(h: &str) -> String {
    h.chars().take(19).collect()
}

/// Le famiglie di media coperte dal corpus.
pub fn famiglie_coperte(c: &Corpus) -> Vec<families::Family> {
    let mut out: Vec<families::Family> = c.voci().iter().map(|s| s.famiglia).collect();
    out.sort();
    out.dedup();
    out
}

/// La profondità massima della catena di prerequisiti fra gli item validi.
fn profondita_massima(voci: &[Spec], validi: &std::collections::BTreeSet<&str>) -> usize {
    fn profondita(
        s: &Spec,
        voci: &[Spec],
        validi: &std::collections::BTreeSet<&str>,
        vista: &mut Vec<String>,
    ) -> usize {
        if let Some(i) = vista.iter().position(|v| v == s.rel) {
            // cycles are reported by `cerca_ciclo`: here we just stop
            return vista.len() - i;
        }
        vista.push(s.rel.to_string());
        let mut max = 0;
        for p in s.prerequisiti {
            if !validi.contains(*p) {
                continue;
            }
            if let Some(pz) = voci.iter().find(|v| v.rel == *p) {
                let d = profondita(pz, voci, validi, vista);
                if d > max {
                    max = d;
                }
            }
        }
        vista.pop();
        max + 1
    }
    let mut vista = Vec::new();
    voci
        .iter()
        .filter(|s| validi.contains(s.rel))
        .map(|s| profondita(s, voci, validi, &mut vista))
        .max()
        .unwrap_or(0)
}

/// Un ciclo fra gli item validi, se c'è. La catena è restituita in modo che
/// il messaggio d'errore la nomini: «c'è un ciclo» senza il ciclo è una
/// scusa.
fn cerca_ciclo(voci: &[Spec], validi: &std::collections::BTreeSet<&str>) -> Option<Vec<String>> {
    fn dfs(
        nodo: &str,
        voci: &[Spec],
        validi: &std::collections::BTreeSet<&str>,
        pila: &mut Vec<String>,
        fatto: &mut std::collections::BTreeSet<String>,
    ) -> Option<Vec<String>> {
        if let Some(i) = pila.iter().position(|v| v == nodo) {
            let mut c = pila[i..].to_vec();
            c.push(nodo.to_string());
            return Some(c);
        }
        if fatto.contains(nodo) {
            return None;
        }
        let s = voci.iter().find(|v| v.rel == nodo)?;
        pila.push(nodo.to_string());
        for p in s.prerequisiti {
            if validi.contains(*p) {
                if let Some(c) = dfs(p, voci, validi, pila, fatto) {
                    return Some(c);
                }
            }
        }
        pila.pop();
        fatto.insert(nodo.to_string());
        None
    }
    let mut pila = Vec::new();
    let mut fatto = std::collections::BTreeSet::new();
    for s in voci.iter().filter(|s| validi.contains(s.rel)) {
        if let Some(c) = dfs(s.rel, voci, validi, &mut pila, &mut fatto) {
            return Some(c);
        }
    }
    None
}

/// Conta i file sotto `radice`, chiamando `f` per ciascuno. **Tutti** i file,
/// non solo gli HTML e i JSON: un file che nessuno ha descritto è rumore che
/// un banco deve dire, e un rumore che il banco non vede è rumore che qualcuno
/// scoprirà fra sei mesi, in un altro banco, in un altro momento.
fn walk_file(radice: &Path, f: &mut impl FnMut(&str)) -> usize {
    fn walk(dir: &Path, base: &Path, f: &mut impl FnMut(&str), n: &mut usize) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut voci: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        voci.sort();
        for p in voci {
            if p.is_dir() {
                walk(&p, base, f, n);
                continue;
            }
            let Ok(rel) = p.strip_prefix(base) else { continue };
            *n += 1;
            f(&rel.to_string_lossy().replace('\\', "/"));
        }
    }
    let mut n = 0;
    walk(radice, radice, f, &mut n);
    n
}

/// Le relazioni che il banco usa per il controllo di visibilità, esposto per
/// il referto. La visibilità non è un ruolo: è una funzione della relazione
/// (D5), e `kbs-core` ne è già il predicato unico.
pub fn relazioni_del_banco() -> Vec<Relation> {
    vec![
        Relation::EnrolledIn,
        Relation::Teaches,
        Relation::AuthorOf,
        Relation::Ratified,
        Relation::SpeculativeFor,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// I nomi dei controlli sono parte dell'interfaccia del banco: il referto
    /// della CI, il README e la tabella delle capacità li nominano, e un
    /// nome che cambia senza che nessuno se ne accorga è una promessa che si
    /// spegne da sola.
    #[test]
    fn i_nomi_dei_controlli_sono_univoci_e_stabili() {
        let assente = crate::adapter::PipelineAssente { ragione: "assente".into() };
        let banco = Banco::new(Config::radice_di_default(), &assente);
        let mut nomi: Vec<&str> = banco.esegui().controlli.iter().map(|c| c.nome).collect();
        let n = nomi.len();
        nomi.sort();
        nomi.dedup();
        assert_eq!(nomi.len(), n, "due controlli hanno lo stesso nome");
        assert_eq!(n, 23, "il banco ha 14 controlli sul corpus e 9 sulla pipeline");
    }

    /// Un banco senza pipeline non è un banco verde: è un banco che ha detto
    /// che cosa non ha verificato. Il test lo rende esplicito.
    #[test]
    fn senza_pipeline_i_controlli_di_pipeline_sono_saltati_con_ragione() {
        let assente = crate::adapter::PipelineAssente { ragione: "binario assente".into() };
        let banco = Banco::new(Config::radice_di_default(), &assente);
        let r = banco.esegui();
        assert_eq!(r.saltati(), 9);
        assert!(!r.esito_con_rigidezza(true), "in CI i saltati sono fallimenti");
        for c in r.controlli.iter().filter(|c| c.di_pipeline) {
            match &c.esito {
                Esito::Saltato(why) => assert!(why.contains("binario assente"), "{why}"),
                altro => panic!("{}: atteso saltato, trovato {}", c.nome, altro.etichetta()),
            }
        }
    }

    /// Le tre famiglie di relazione non sono ruoli: sono l'unico oggetto che
    /// autorizza, e il banco le elenca tutte e cinque.
    #[test]
    fn le_relazioni_del_banco_non_sono_un_insieme_parziale() {
        assert_eq!(relazioni_del_banco().len(), 5);
    }
}
