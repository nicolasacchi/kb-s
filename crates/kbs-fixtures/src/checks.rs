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

use crate::adapter::{self, Atto, Eseguito, Pipeline, PipelineError, Session, Uscita};
use crate::contract::{self, Defect, HARD_CAP};
use crate::corpus::{self, Corpus};
use crate::families;
use crate::spec::{self, ClaimStatusKind, RatificaSpec, Spec};
use kbs_core::{PublicationState, Relation};
use std::collections::BTreeSet;
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

/// La prova della **seconda strada di D4**: gli atti, e ciò che ne è uscito.
///
/// La prima strada di D4 è quella che il banco già faceva: una pipeline che
/// indicizza una cartella e dice quali argomenti sono citabili. Ma da una
/// cartella **non esce nessuna ratifica** — la ratifica è un atto separato, di
/// una persona — quindi quella strada misurava un input che nel sistema non
/// esiste: il banco costruiva un `Argument` con `ratified: Some(…)` che stava
/// solo nella tabella Rust, e lo confrontava con un database in cui nulla era
/// ratificato. Il confronto era destinato a fallire, e non perché il sistema
/// sbagliasse.
///
/// La seconda strada è quella che una scuola percorre: si legge, si decide,
/// si firma, si rilegge. Qui sotto ci sono i quattro atti, e ognuno ha un
/// controllo con un nome suo.
struct D4<'a> {
    /// La tabella, per tradurre un `ArgumentId` in un percorso relativo.
    corpus: &'a Corpus,
    /// `verify` sul database appena creato: nessuno ha ancora ratificato
    /// niente, e quindi niente è citabile.
    vuoto: Uscita,
    /// `promote` per ogni item che la tabella dichiara ratificato di fresco.
    promozioni: Vec<Eseguito>,
    /// `verify` sullo **stesso** database, dopo le promozioni.
    dopo: Uscita,
    /// La copia, che è la seconda strada applicata a un contratto cambiato
    /// sotto una ratifica viva.
    copia: Copia,
    /// La cartella della copia. Vive finché la prova vive: è ciò che tiene
    /// i file su disco, e senza di lei la copia non esisterebbe più quando
    /// i controlli la leggono.
    _cartella: tempfile::TempDir,
}

/// La seconda strada sulla copia: promuovere, cambiare il contratto,
/// verificare di nuovo.
struct Copia {
    /// `verify` sulla copia prima di promuovere: anche qui nessuno ha
    /// ratificato, ed è lo stesso punto di partenza.
    vuoto: Uscita,
    /// `promote` sulla copia, per gli stessi item del corpus di lavoro.
    promozioni: Vec<Eseguito>,
    /// `verify` sulla copia dopo la promozione e **prima** della riscrittura:
    /// l'item riscritto è qui citabile, e senza questa riga il banco
    /// confronterebbe una transizione che non ha osservato.
    prima_del_cambio: Uscita,
    /// `verify` sulla copia **dopo** che il contratto di un item è stato
    /// riscritto sotto la ratifica che quel docente aveva già firmato.
    dopo_il_cambio: Uscita,
    /// Il percorso relativo dell'item il cui contratto è stato riscritto.
    rel_riscritto: String,
}

impl<'a> D4<'a> {
    /// Gli item che la seconda strada ha promosso, per percorso relativo.
    fn promossi(&self, promozioni: &[Eseguito]) -> BTreeSet<String> {
        promozioni
            .iter()
            .filter(|e| e.ok())
            .filter_map(|e| e.atto.soggetto.clone())
            .collect()
    }

    /// I percorsi relativi degli id che l'indice dichiara citabili. Un id che
    /// il banco non conosce resta come è: un id sconosciuto è un problema
    /// della pipeline e va detto per intero, non tradotto in silenzio.
    fn citati(&self, uscita: &Uscita) -> BTreeSet<String> {
        uscita
            .index
            .citable
            .iter()
            .map(|id| self.rel_di(id).unwrap_or_else(|| id.clone()))
            .collect()
    }

    /// Il percorso relativo di un `ArgumentId`, se la tabella lo conosce.
    fn rel_di(&self, id: &str) -> Option<String> {
        self.corpus
            .voci()
            .iter()
            .find(|s| corpus::id_di(s).as_str() == id)
            .map(|s| s.rel.to_string())
    }
}

impl<'a> Banco<'a> {
    /// Le voci che la tabella dichiara ratificate di fresco: sono quelle su
    /// cui il docente ha firmato sul contratto di oggi, e quindi le uniche su
    /// cui ha senso chiedere alla porta di accettare.
    fn fresche(&self) -> Vec<&Spec> {
        self.corpus
            .voci()
            .iter()
            .filter(|s| s.ratifica == RatificaSpec::Fresca)
            .collect()
    }

    /// L'item su cui il banco riscrive il contratto sotto una ratifica viva.
    ///
    /// Le quattro condizioni sono tutte necessarie e sono dichiarate una per
    /// una, perché ognuna esclude un caso in cui il caso non sarebbe quello:
    /// una ratifica fresca (senza, non c'è ratifica da far superare), in uso
    /// (un item archiviato non si promuove), senza difetto (un contratto rotto
    /// uscirebbe dall'indice per un'altra ragione, e il banco misurerebbe la
    /// validazione invece della ratifica), e con almeno un prerequisito (senza,
    /// riscrivere il contratto non cambierebbe niente e l'hash resterebbe
    /// quello).
    fn item_riscrivibile(&self) -> Option<&Spec> {
        self.corpus.voci().iter().find(|s| {
            s.ratifica == RatificaSpec::Fresca
                && s.stato == PublicationState::InCorso
                && s.difetto == Defect::Nessuno
                && !s.prerequisiti.is_empty()
        })
    }

    /// I quattro atti della seconda strada di D4, eseguiti come processi.
    ///
    /// L'ordine è l'ordine in cui una scuola li farebbe, ed è dichiarato
    /// perché un ordine diverso sarebbe un'altra verifica:
    ///
    /// 1. `verify` su un database vuoto — nessuno ha ratificato, e quindi
    ///    l'indice non deve citare niente;
    /// 2. `promote` su tutto ciò che la tabella dichiara ratificato di fresco
    ///    — l'atto del docente, l'unico modo in cui una ratifica entra;
    /// 3. `verify` sullo **stesso** database — e adesso l'indice deve citare
    ///    esattamente il gruppo promosso, né uno di meno né uno di più;
    /// 4. la stessa strada su una **copia**, con il contratto di un item
    ///    riscritto sotto la ratifica già firmata — l'item deve uscire
    ///    dall'indice per `stale-ratification`, e nessun altro.
    fn seconda_strada(&self) -> Result<D4<'_>, PipelineError> {
        let nome = self.pipeline.descrizione();
        let fresche = self.fresche();
        if fresche.is_empty() {
            return Err(PipelineError::Altro(
                "la tabella non dichiara nessuna ratifica fresca: la seconda strada di D4 non ha su che cosa agire"
                    .into(),
            ));
        }
        let riscrivibile = self.item_riscrivibile().ok_or_else(|| {
            PipelineError::Altro(
                "nessun item in uso, ratificato di fresco, senza difetti e con un prerequisito: il caso della ratifica superata non può essere costruito"
                    .into(),
            )
        })?;
        let rel_riscritto = riscrivibile.rel.to_string();

        // ── atti 1, 2 e 3: il corpus di lavoro, un database, tre letture ──────
        let radice = self.cfg.radice.clone();
        let sessione = Session::nuova()?;
        let mut atti = vec![Atto::verify(&radice)];
        for s in &fresche {
            atti.push(Atto::promote(&radice, spec::OPERATORE, s.rel));
        }
        atti.push(Atto::verify(&radice));
        let eseguiti = self.pipeline.sequenza(&self.corpus, &sessione, &atti)?;
        let n = fresche.len();
        let vuoto = adapter::referto_di(&nome, &eseguiti, 0)?;
        let promozioni = eseguiti[1..=n].to_vec();
        let dopo = adapter::referto_di(&nome, &eseguiti, n + 1)?;

        // ── atto 4: la stessa strada su una copia, con un contratto riscritto ──
        let cartella = tempfile::tempdir().map_err(|e| PipelineError::Altro(e.to_string()))?;
        let radice_copia = cartella.path().join("corpus");
        self.corpus.scrivi_in(&radice_copia).map_err(|e| {
            PipelineError::Altro(format!("{}: {e}", radice_copia.display()))
        })?;
        let sessione_copia = Session::nuova()?;
        let mut atti_copia = vec![Atto::verify(&radice_copia)];
        for s in &fresche {
            atti_copia.push(Atto::promote(&radice_copia, spec::OPERATORE, s.rel));
        }
        atti_copia.push(Atto::verify(&radice_copia));
        let prima = self.pipeline.sequenza(&self.corpus, &sessione_copia, &atti_copia)?;
        let c_vuoto = adapter::referto_di(&nome, &prima, 0)?;
        let c_promozioni = prima[1..=n].to_vec();
        let c_prima = adapter::referto_di(&nome, &prima, n + 1)?;

        // **Qui il banco cambia il contratto.** È un atto di banco e non un
        // atto di pipeline: il banco è il docente che corregge il proprio
        // contratto, e l'adattatore non deve saper scrivere un corpus. La
        // riscrittura è dichiarata e non casuale — vedi
        // `Corpus::con_contratto_riscritto` — perché un caso che il banco
        // crede di esercitare e non è quello è peggio di un caso assente.
        self.corpus
            .con_contratto_riscritto(&rel_riscritto)
            .scrivi_in(&radice_copia)
            .map_err(|e| PipelineError::Altro(format!("{}: {e}", radice_copia.display())))?;

        // E adesso la terza lettura, sullo **stesso** database della copia:
        // la ratifica esiste ancora, e vale per un testo che non è più
        // quello che è stato firmato.
        let terza = self
            .pipeline
            .sequenza(&self.corpus, &sessione_copia, &[Atto::verify(&radice_copia)])?;
        let c_dopo_il_cambio = adapter::referto_di(&nome, &terza, 0)?;

        Ok(D4 {
            corpus: &self.corpus,
            vuoto,
            promozioni,
            dopo,
            copia: Copia {
                vuoto: c_vuoto,
                promozioni: c_promozioni,
                prima_del_cambio: c_prima,
                dopo_il_cambio: c_dopo_il_cambio,
                rel_riscritto,
            },
            _cartella: cartella,
        })
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
        let prima = self.pipeline.esegui(&self.corpus, &self.cfg.radice);
        // La seconda strada è una richiesta **separata** e non un prolungo
        // della prima: se non parte, i suoi controlli sono saltati con la
        // ragione, e non si accodano a quelli che la prima strada ha già
        // prodotto. Un controllo che eredita l'esito di un altro è un
        // controllo che non ha verificato niente.
        let d4 = self.seconda_strada();
        let mut controlli = self.controlli_su_corpus();
        controlli.extend(self.controlli_su_pipeline(&prima, &d4));
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
    /// testo dello span — e deve dichiararli **nella convenzione che `kbs-doc`
    /// legge**: `data-claim` per il fatto, `data-claim-id` per l'identità,
    /// `data-claim-span` per l'ancora. Una claim con un puntatore che il lettore
    /// non può aprire è una dichiarazione con un indirizzo, e una claim
    /// dichiarata in una convenzione che il lettore non riconosce è una claim
    /// che nel registro non c'è.
    ///
    /// Il confronto è sui tre attributi, non sulla loro presenza: `id="…"`
    /// compare in un file per altre ragioni, e una ricerca di `id` non distingue
    /// «l'ancora della claim» da «un'id che qualcun altro usa».
    ///
    /// Il test che tiene insieme questo controllo e il lettore vero è
    /// `tests/le_claim_sono_il_che_il_legge_ne_ricava.rs`: qui si verifica che
    /// la resa dichiari ciò che la tabella dice, lì che `kbs-doc` ne ricavi
    /// proprio la claim. I due insiemi insieme sono la prova, e ciascuno da
    /// solono lascerebbe un buco.
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
                        if !artefatto.contains(&format!("data-claim-id=\"{}\"", cl.id)) {
                            problemi.push(format!(
                                "{}: la claim {} non dichiara il suo id nella convenzione del lettore",
                                s.rel, cl.id
                            ));
                        }
                        if !artefatto.contains(&format!("data-claim-span=\"{a}\"")) {
                            problemi.push(format!(
                                "{}: la claim {} non dichiara l'ancora {a} come suo span",
                                s.rel, cl.id
                            ));
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
    fn controlli_su_pipeline(
        &self,
        esito: &Result<Uscita, PipelineError>,
        d4: &Result<D4<'_>, PipelineError>,
    ) -> Vec<Controllo> {
        let saltato = |e: &PipelineError| Esito::Saltato(e.to_string());
        let uscita = match esito {
            Ok(u) => u,
            Err(e) => {
                return vec![
                    pc("pipeline.validazione.corrisponde_all_attesa", saltato(e)),
                    pc("pipeline.validazione.i_codici_di_rifiuto", saltato(e)),
                    pc("pipeline.registro.gli_errori_restano_e_non_si_cancellano", saltato(e)),
                    pc("pipeline.registro.le_claim_non_citabili_sono_soppresse_in_output", saltato(e)),
                    pc("pipeline.prerequisiti.il_ciclo_e_rifiutato", saltato(e)),
                    pc("pipeline.documenti.solo_la_versione_locale_di_three_e_pubblicabile", saltato(e)),
                    pc("pipeline.documenti.la_scena_3d_e_manipolabile_e_i_suoi_dati_sono_il_contenuto", saltato(e)),
                    pc("pipeline.d4.su_un_corpus_non_ratificato_niente_e_citabile", saltato(e)),
                    pc("pipeline.d4.la_promozione_e_l_atto_del_docente", saltato(e)),
                    pc("pipeline.d4.dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso", saltato(e)),
                    pc("pipeline.d4.il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile", saltato(e)),
                ]
            }
        };
        // I quattro controlli di D4 hanno la loro **propria** richiesta e la
        // propria ragione di saltato: se la seconda strada non parte, il
        // motivo è quello della seconda strada e non quello della prima. Un
        // controllo che eredita la ragione di un altro non sa perché è
        // saltato, e chi legge il referto deve poter saperlo.
        let mut controlli = vec![
            pc(
                "pipeline.validazione.corrisponde_all_attesa",
                self.c_validazione(uscita),
            ),
            pc("pipeline.validazione.i_codici_di_rifiuto", self.c_codici(uscita)),
            pc(
                "pipeline.registro.gli_errori_restano_e_non_si_cancellano",
                self.c_registro(uscita),
            ),
            pc(
                "pipeline.registro.le_claim_non_citabili_sono_soppresse_in_output",
                self.c_soppressione(uscita),
            ),
            pc("pipeline.prerequisiti.il_ciclo_e_rifiutato", self.c_ciclo(uscita)),
            pc(
                "pipeline.documenti.solo_la_versione_locale_di_three_e_pubblicabile",
                self.c_three(uscita),
            ),
            pc(
                "pipeline.documenti.la_scena_3d_e_manipolabile_e_i_suoi_dati_sono_il_contenuto",
                self.c_manipolazione(uscita),
            ),
        ];
        match d4 {
            Ok(d4) => {
                controlli.push(pc(
                    "pipeline.d4.su_un_corpus_non_ratificato_niente_e_citabile",
                    self.c_d4_vuoto(d4),
                ));
                controlli.push(pc(
                    "pipeline.d4.la_promozione_e_l_atto_del_docente",
                    self.c_d4_promozione(d4),
                ));
                controlli.push(pc(
                    "pipeline.d4.dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso",
                    self.c_d4_citabili(d4),
                ));
                controlli.push(pc(
                    "pipeline.d4.il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile",
                    self.c_d4_superata(d4),
                ));
            }
            Err(e) => {
                controlli.push(pc(
                    "pipeline.d4.su_un_corpus_non_ratificato_niente_e_citabile",
                    saltato(e),
                ));
                controlli.push(pc(
                    "pipeline.d4.la_promozione_e_l_atto_del_docente",
                    saltato(e),
                ));
                controlli.push(pc(
                    "pipeline.d4.dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso",
                    saltato(e),
                ));
                controlli.push(pc(
                    "pipeline.d4.il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile",
                    saltato(e),
                ));
            }
        }
        controlli
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

    // ─────────────────────────────────────────────────────────────────────────
    // D4, la seconda strada. Quattro controlli e non uno: uno per atto.
    //
    // Un controllo che può fallire per quattro ragioni è un controllo che
    // verrà riportato per la ragione sbagliata, e chi legge il referto non ha
    // modo di saperlo. Ognuno dei quattro ha una precondizione propria e
    // guarda una cosa sola.
    // ─────────────────────────────────────────────────────────────────────────

    /// **Atto 1.** `verify` su un database in cui nessuno ha agito: l'indice
    /// non deve citare niente, e ogni item deve dire perché non è citabile.
    ///
    /// Il motivo dell'invariante è dichiarato e non indiziato: su un database
    /// vuoto l'unica ragione possibile è che **manca la ratifica**, e se la
    /// pipeline dicesse qualcos'altro starebbe misurando un'altra cosa. Il
    /// controllo dichiara anche che la tabella ha qualcosa di ratificabile:
    /// un indice vuoto su un corpus in cui non c'è niente da ratificare è
    /// vero e non dimostra niente.
    fn c_d4_vuoto(&self, d4: &D4<'_>) -> Esito {
        if self.fresche().is_empty() {
            return Esito::fallito(
                "la tabella non dichiara nessuna ratifica fresca: un indice vuoto non dimostra che la ratifica sia ciò che manca",
            );
        }
        let mut problemi = Vec::new();
        if !d4.vuoto.index.citable.is_empty() {
            let mut nomi: Vec<&str> = d4
                .vuoto
                .index
                .citable
                .iter()
                .map(String::as_str)
                .collect();
            nomi.sort();
            problemi.push(format!(
                "{} argomenti sono citabili su un database in cui nessuno ha ratificato ({}): un item non ratificato è leggibile, e leggibile non è citabile",
                nomi.len(),
                nomi.join(", ")
            ));
        }
        for s in self.corpus.voci() {
            let id = corpus::id_di(s);
            let riga = d4.vuoto.index.not_citable.iter().find(|n| n.id == id.as_str());
            match riga {
                None => problemi.push(format!(
                    "{rel}: la pipeline non ha detto perché non è citabile, e un verdetto senza motivo non si contesta",
                    rel = s.rel
                )),
                Some(nc) => {
                    if nc.invariant.trim().is_empty() {
                        problemi.push(format!("{rel}: non citabile senza invariante dichiarata", rel = s.rel));
                    }
                    if nc.message.trim().is_empty() {
                        problemi.push(format!("{rel}: non citabile senza motivo dichiarato", rel = s.rel));
                    }
                    if nc.invariant == "citable-without-ratification" {
                        continue;
                    }
                    problemi.push(format!(
                        "{}: l'invariante è «{}» e su un database vuoto l'unica ragione possibile è che la ratifica manca",
                        s.rel, nc.invariant
                    ));
                }
            }
        }
        Esito::fallito_collect(problemi)
    }

    /// **Atto 2.** `promote` su tutto ciò che la tabella dichiara ratificato di
    /// fresco: l'atto del docente, e l'unico modo in cui una ratifica entra
    /// nel sistema.
    ///
    /// Il controllo non chiede che la porta accetti tutto: un rifiuto può
    /// essere giusto — un item archiviato non si porta a in uso, e la tabella
    /// ne ha. Chiede tre cose, e sono le tre che rendono l'atto una verifica:
    /// che ogni item **dichiarato in uso** sia stato promosso (una porta che
    /// chiude a tutto non sta verificando niente), che ogni rifiuto **dica
    /// perché**, e che qualcosa sia stato promosso (una porta che non apre mai
    /// l'ha esercitata altrettanto poco).
    fn c_d4_promozione(&self, d4: &D4<'_>) -> Esito {
        let fresche = self.fresche();
        if d4.promozioni.len() != fresche.len() {
            return Esito::fallito(format!(
                "{} atti di promozione eseguiti per {} item ratificati di fresco: la sequenza non è quella che il banco ha chiesto",
                d4.promozioni.len(),
                fresche.len()
            ));
        }
        let mut problemi = Vec::new();
        let mut promossi = 0usize;
        for (i, (s, e)) in fresche.iter().zip(d4.promozioni.iter()).enumerate() {
            if e.atto.soggetto.as_deref() != Some(s.rel) {
                problemi.push(format!(
                    "l'atto di promozione numero {} è su {:?} e non su {}: i risultati non sono attribuibili",
                    i + 1,
                    e.atto.soggetto,
                    s.rel
                ));
                continue;
            }
            if e.ok() {
                promossi += 1;
                continue;
            }
            // Un rifiuto è lecito, ma solo dove la tabella non dice che
            // l'item è in uso. Su un item che la tabella dichiara in uso la
            // porta non può chiudere, e se chiude è perché qualcosa nel
            // contratto non sta regendo: il motivo del rifiuto lo dice, e va
            // detto anche in questo caso.
            if s.stato == PublicationState::InCorso {
                problemi.push(format!(
                    "{rel}: la tabella lo dichiara in uso e la porta ha rifiutato di promuoverlo — {motivo}",
                    rel = s.rel,
                    motivo = e.motivo()
                ));
            }
            if e.motivo().trim().is_empty() {
                problemi.push(format!(
                    "{rel}: la porta ha rifiutato la promozione senza dire perché",
                    rel = s.rel
                ));
            }
        }
        if promossi == 0 {
            problemi.push(
                "nessuna promozione accettata: l'atto del docente non è mai entrato, e gli atti 3 e 4 avrebbero misurato un sistema a cui nessuno ha chiesto niente"
                    .to_string(),
            );
        }
        Esito::fallito_collect(problemi)
    }

    /// **Atto 3.** `verify` sullo stesso database, dopo le promozioni: e adesso
    /// l'indice deve citare **esattamente** il gruppo promosso.
    ///
    /// È il controllo che chiude D4 sul corpus, ed è quello che non si può
    /// raggirare: i due insiemi sono confrontati in entrambe le direzioni. Un
    /// item promosso e non citato significa che la ratifica non rende
    /// citabile; un item citato e non promosso significa che è entrato
    /// nell'indice condiviso senza che nessuno l'avesse firmato.
    fn c_d4_citabili(&self, d4: &D4<'_>) -> Esito {
        let promossi = d4.promossi(&d4.promozioni);
        if promossi.is_empty() {
            return Esito::fallito(
                "nessun item promosso: il confronto con il citabile sarebbe tra due insiemi vuoti e direbbe niente",
            );
        }
        let citati = d4.citati(&d4.dopo);
        let mut problemi = Vec::new();
        for mancante in promossi.difference(&citati) {
            problemi.push(format!(
                "{mancante}: la porta l'ha promosso e l'indice non lo cita — la ratifica non rende citabile"
            ));
        }
        for extra in citati.difference(&promossi) {
            problemi.push(format!(
                "{extra}: l'indice lo cita e nessuno l'ha promosso — un item non ratificato non entra nell'indice condiviso"
            ));
        }
        Esito::fallito_collect(problemi)
    }

    /// **Atto 4.** La stessa strada su una copia, con il contratto di un item
    /// riscritto sotto la ratifica che quel docente aveva già firmato: e
    /// l'item deve uscire dall'indice, per `stale-ratification` e non per
    /// un'altra ragione.
    ///
    /// Le quattro cose che il controllo verifica sono tutte necessarie, e
    /// ognuna chiude una falsificazione diversa:
    ///
    /// * l'item **era** citabile prima della riscrittura — senza questo il
    ///   banco confronterebbe uno stato, non una transizione;
    /// * dopo la riscrittura **non** lo è più, e la riga che lo dichiara porta
    ///   l'invariante `stale-ratification`;
    /// * il messaggio nomina **due hash diversi** — «superata» senza i due hash
    ///   è una parola, e una parola non è una verifica;
    /// * l'item è ancora **valido** — se fosse uscito perché il contratto lo
    ///   rifiuta, il banco avrebbe misurato la validazione e avrebbe
    ///   dichiarato che la ratifica è superata.
    ///
    /// E le due estremità della transizione, perché il controllo guarda uno
    /// **spostamento** e non uno stato: sulla copia, prima di promuovere, non
    /// era citabile nessuno; dopo la riscrittura non è citabile proprio
    /// quell'altro. Se il primo capo non stasse, l'uscita dall'indice
    /// potrebbe dipendere da qualcosa che è successo prima e non dalla
    /// riscrittura.
    /// E l'ultima riga: **nessun altro** item esce. Una riscrittura che
    /// svuota l'indice non ha dimostrato che la ratifica conti, ha dimostrato
    /// che qualcosa si è rotto.
    fn c_d4_superata(&self, d4: &D4<'_>) -> Esito {
        let c = &d4.copia;
        let id = match self
            .corpus
            .voci()
            .iter()
            .find(|s| s.rel == c.rel_riscritto)
            .map(corpus::id_di)
        {
            Some(id) => id,
            None => {
                return Esito::fallito(format!(
                    "{}: il banco ha riscritto un contratto che la tabella non conosce, e il caso non è quello che dichiara di esercitare",
                    c.rel_riscritto
                ))
            }
        };
        let rel = c.rel_riscritto.as_str();
        let promossi = d4.promossi(&c.promozioni);
        if !promossi.contains(rel) {
            return Esito::fallito(format!(
                "{rel}: la porta non l'ha promosso sulla copia, e senza una ratifica viva non c'è nessuna ratifica da superare"
            ));
        }

        let mut problemi = Vec::new();
        if d4.citati(&c.vuoto).contains(rel) {
            problemi.push(format!(
                "{rel}: era già citabile sulla copia prima di qualsiasi promozione, e la transizione che il banco misura non è quella che dichiara"
            ));
        }
        if !d4.citati(&c.prima_del_cambio).contains(rel) {
            problemi.push(format!(
                "{rel}: prima della riscrittura del contratto non era citabile, e il banco avrebbe confrontato uno stato con uno stato"
            ));
        }
        if d4.citati(&c.dopo_il_cambio).contains(rel) {
            problemi.push(format!(
                "{rel}: il contratto è stato riscritto dopo la firma e l'indice lo cita ancora — la ratifica vale per un testo che non è più quello firmato"
            ));
        }
        let riga = c
            .dopo_il_cambio
            .index
            .not_citable
            .iter()
            .find(|n| n.id == id.as_str());
        match riga {
            None => problemi.push(format!(
                "{rel}: la pipeline non ha detto perché non è più citabile, e un item che esce dall'indice in silenzio non è un item che esce per la ratifica"
            )),
            Some(nc) => {
                if nc.invariant != "stale-ratification" {
                    problemi.push(format!(
                        "{rel}: l'invariante è «{}» e quello della ratifica superata è «stale-ratification»",
                        nc.invariant,
                        rel = rel
                    ));
                }
                let mut hash: Vec<&str> = nc
                    .message
                    .split("sha256:")
                    .skip(1)
                    .map(|h| h.split(|c: char| !c.is_ascii_hexdigit()).next().unwrap_or(""))
                    .filter(|h| !h.is_empty())
                    .collect();
                hash.sort_unstable();
                hash.dedup();
                if hash.len() < 2 {
                    problemi.push(format!(
                        "{rel}: il motivo non nomina i due hash che non coincidono, e «superata» senza i due hash è una parola: {motivo}",
                        motivo = nc.message
                    ));
                }
            }
        }
        match c.dopo_il_cambio.item(rel) {
            None => problemi.push(format!(
                "{rel}: la pipeline non ha riportato l'item, e non si può dire che sia ancora valido"
            )),
            Some(d) if !d.valid => problemi.push(format!(
                "{rel}: il contratto riscritto non è valido, e l'item sarebbe uscito dall'indice per la validazione e non per la ratifica superata: {:?}",
                d.errors.iter().map(|e| e.code.as_str()).collect::<Vec<_>>()
            )),
            Some(_) => {}
        }
        for uscito in d4
            .citati(&c.prima_del_cambio)
            .difference(&d4.citati(&c.dopo_il_cambio))
            .filter(|r| *r != rel)
        {
            problemi.push(format!(
                "{uscito}: è uscito dall'indice insieme all'item con il contratto riscritto, e la sua ratifica non è stata toccata"
            ));
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

    // ─────────────────────────────────────────────────────────────────────────
    // Qui NON c'è `pipeline.esercizi.il_replay_e_deterministico`, e la ragione
    // è dichiarata perché un controllo che non c'è e una ragione che non si
    // legge sono la stessa cosa dal punto di vista di chi legge il referto.
    //
    // Il controllo chiedeva alla **pipeline** le istanze che il banco ha nella
    // **propria tabella Rust** (`spec::ExerciseSpec`). Non sono nel corpus: il
    // file HTML rende `data-esercizio`, `data-famiglia`, `data-checker`,
    // `data-seed` e i tre parametri, ma **non** la risposta attesa. Quindi
    // nessuna pipeline basata su file può restituirle, e il controllo non
    // poteva mai passare: non era una verifica, era un controllo impossibile.
    //
    // Il replay deterministico è di `kbs-exercise` e va verificato lì, senza
    // passare dalla pipeline: è il posto in cui il generatore sta, e un
    // controllo che lo raggiunge attraverso un processo che non genera nulla
    // sta misurando la strada, non la proprietà.
    //
    // **Che cosa resta non testato, e dove il replay è adesso esercitato.**
    // Il banco non verifica più che gli esercizi *dichiarati* abbiano le
    // risposte che dichiara: quelle risposte sono valori nella tabella, e
    // nessuna parte del sistema le ricalcola, quindi un test su di esse
    // confronterebbe la tabella con sé stessa. Il replay come proprietà —
    // stesso seed, stessa istanza; seed diverso, risposta diversa — è esercitato
    // in `tests/il_replay_e_deterministico.rs`, che chiama il generatore
    // direttamente.
    //
    // Le famiglie dichiarate dal banco (`somma-di-frazioni`, `algoritmo-euclide`,
    // `conta-nodi-scena`, …) non sono le famiglie di `kbs-exercise`, che ne ha
    // cinque e diverse. È la ragione per cui il test sostitutivo non può
    // asserire le attese della tabella: asserirebbe che un generatore produce
    // la risposta di un generatore che non esiste.



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
    /// rendering fosse l'unica rappresentazione, un aggiornamento del renderer
    /// cancellerebbe il materiale dello studente.
    ///
    /// # Perché questo controllo guarda **una** scena e non tutte
    ///
    /// Il corpus contiene due scene 3D ed è voluto: una che carica il runtime
    /// vendorizzato e una che lo carica da una CDN.
    /// `pipeline.documenti.solo_la_versione_locale_di_three_e_pubblicabile`
    /// pretende che la seconda sia **rifiutata** con `external-reference`,
    /// perché è la metà negativa di D15.
    ///
    /// Applicare a lei anche le proprietà di una scena che funziona — il
    /// runtime dichiarato è quello servito dal binario, e c'è una relazione
    /// correggibile — è pretendere che una fixture sia due cose: quella che
    /// dimostra che il runtime vendorizzato passa, e quella che dimostra che
    /// quello da CDN no. Le due richieste non possono valere insieme, e un
    /// banco che le chiede entrambe è un banco che non può essere verde.
    ///
    /// La divisione è per **ruolo della fixture**, non per esclusione: qui si
    /// esercita la scena che deve funzionare, e la scena che deve fallire la
    /// verifica `c_three`, che è dove il suo fallimento è il punto. Se un
    /// giorno il corpus avrà una terza scena, la decisione su quale delle due
    /// famiglie appartiene è da prendere qui, per nome.
    fn c_manipolazione(&self, uscita: &Uscita) -> Esito {
        let voci = self.corpus.voci();
        let mut problemi = Vec::new();
        for s in voci
            .iter()
            .filter(|s| s.scena.is_some() && s.riferimento_esterno.is_none())
        {
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
        assert_eq!(n, 25, "il banco ha 14 controlli sul corpus e 11 sulla pipeline");
    }

    /// Un banco senza pipeline non è un banco verde: è un banco che ha detto
    /// che cosa non ha verificato. Il test lo rende esplicito.
    #[test]
    fn senza_pipeline_i_controlli_di_pipeline_sono_saltati_con_ragione() {
        let assente = crate::adapter::PipelineAssente { ragione: "binario assente".into() };
        let banco = Banco::new(Config::radice_di_default(), &assente);
        let r = banco.esegui();
        assert_eq!(r.saltati(), 11);
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
