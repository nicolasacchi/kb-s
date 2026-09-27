//! **Il collo di bottiglia.** Tutto ciò che entra nel corpus passa di qui.
//!
//! D10 elenca quattro strade — cattura, CLI, file, MCP — e le quattro devono
//! esistere. Quello che D10 non dice e che è la parte difficile è che **devono
//! anche non potersi aggirare**: se una strada scrive lo stato, valida con
//! regole sue e registra la provenienza a modo suo, il gate di D4 è decorazione,
//! e un gate decorativo è peggiore di nessun gate perché dà a tutti l'illusione
//! che il controllo esista.
//!
//! Perciò [`receive`] è l'unico posto in cui un `Argument` nasce, e le quattro
//! strade sono quattro modi di costruire un [`Request`]. Il test che lo
//! dimostra è in `tests/le_quattro_strade.rs`: lo stesso contenuto, quattro
//! strade, stesso stato, stesso verdetto, stesse claim.
//!
//! # Che cosa fa [`receive`], in ordine
//!
//! 1. **valida** con `kbs_doc::validate` — lo stesso validatore, e quindi lo
//!    stesso esito, per qualunque strada. Il riferimento a una CDN (D15) e il
//!    contratto troncato (D7) non hanno eccezioni: un artifact che arriva
//!    incollato è lo stesso artifact che arriva come file;
//! 2. **rifiuta** ciò che non può entrare: un titolo mancante non è un
//!    argomento (`kbs_store` non lo scriverebbe), un'origine `generated` senza
//!    model lock è una dichiarazione senza evidenza (D10);
//! 3. **scrive in `bozza`**, sempre. Lo stato dichiarato nel file è una
//!    *richiesta*, e la porta la chiede al negozio: [`apply_declared_state`] non
//!    reimplementa `kbs_store::publish`, la chiama e riporta il suo rifiuto come
//!    **avvertenza**, perché un item che aspetta la ratifica è perfettamente
//!    valido, e dire il contrario confonderebbe «non ratificato» con «rotto»;
//! 4. **registra le claim dichiarate** con il loro stato. Lo stato è quello
//!    dichiarato (`data-stato`) quando c'è, e dedotto dallo span quando non
//!    c'è — con un tetto: nessuno span significa `unciteable` comunque, e una
//!    claim che si dichiara `supported` senza span è una dichiarazione, non una
//!    affermazione sostenuta.
//!
//! # Che cosa `receive` NON fa
//!
//! * non scrive mai `in-corso`: [`crate::gate`] è l'unico che lo chiede, e lo
//!   chiede con una ratifica;
//! * non genera esercizi e non chiama un modello: D3;
//! * non cancella una riga di registro che non gli piace. Un claim non
//!   sostenuto entra `unciteable` e resta, che è il punto di D6.

use std::collections::BTreeMap;

use kbs_core::{
    Argument, ArgumentId, Claim, ClaimStatus, CourseId, Emitter, Invariant, Millis, Origin,
    PersonId, PublicationState, Ratification,
};
use kbs_store::Store;
use rusqlite::OptionalExtension;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Le quattro strade di D10, in ordine di costo crescente.
///
/// L'ordine è quello di `ARCHITECTURE.md` D10 e non è decorativo: la cattura è
/// la più economica e la più fragile, MCP è la più potente e la più aperta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    /// D10.1 — il docente incolla.
    Capture,
    /// D10.2 — l'agente shella su `kbs`.
    Cli,
    /// D10.3 — l'agente scrive un file e il sistema lo indicizza.
    File,
    /// D10.4 — il docente, dentro un agente, legge e scrive il corpus.
    Mcp,
}

impl Route {
    pub fn as_str(self) -> &'static str {
        match self {
            Route::Capture => "capture",
            Route::Cli => "cli",
            Route::File => "file",
            Route::Mcp => "mcp",
        }
    }

    pub fn from_label(s: &str) -> Option<Route> {
        match s {
            "capture" => Some(Route::Capture),
            "cli" => Some(Route::Cli),
            "file" => Some(Route::File),
            "mcp" => Some(Route::Mcp),
            _ => None,
        }
    }

    /// Tutte e quattro, nell'ordine di D10. Serve al test che le mette alla
    /// prova: se ne nascesse una quinta, il test la scoprirebbe da solo.
    pub const ALL: [Route; 4] = [Route::Capture, Route::Cli, Route::File, Route::Mcp];
}

/// Un problema, con la regola che lo nomina.
///
/// Un codice kebab-case e un messaggio. Il codice è il contratto: è quello che
/// un agente può confrontare senza leggere italiano, ed è quello che il banco
/// di prova cerca (`external-reference`, `contract-over-budget`,
/// `prerequisite-cycle`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    /// `true` se impedisce la pubblicazione. Un avvertimento si dice ad alta
    /// voce e non chiude nessuna porta.
    pub blocking: bool,
}

impl Diagnostic {
    pub fn blocking(code: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic { code: code.into(), message: message.into(), blocking: true }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic { code: code.into(), message: message.into(), blocking: false }
    }
}

/// L'esito della validazione di un intake, in una forma sola.
///
/// `kbs_doc::ArtifactReport` è il verdetto del documento; questo è il verdetto
/// **di tutto ciò che l'intake sa**: il documento, i prerequisiti, l'origine, la
/// ratifica che manca. Le due cose non sono la stessa, e tenerle separate
/// permetterebbe a una seconda validazione — che non esiste, e non deve
/// esistere — di divergere dalla prima.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Verdict {
    /// L'hash del testo che questo verdetto ha giudicato.
    ///
    /// Non è un problema né un avvertimento: è **il soggetto** del verdetto.
    /// Senza di esso la porta di D4 potrebbe controllare soltanto che il
    /// documento era valido *quando qualcuno l'ha guardato*, che è una
    /// proprietà più debole e sembra la stessa. Vedi [`crate::gate`].
    pub content_hash: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Verdict {
    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    /// `true` se nessun problema impedisce la pubblicazione.
    pub fn can_publish(&self) -> bool {
        !self.has_blocking()
    }

    pub fn has_blocking(&self) -> bool {
        self.diagnostics.iter().any(|d| d.blocking)
    }

    pub fn blocking(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.blocking)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| !d.blocking)
    }

    pub fn has_code(&self, code: &str) -> bool {
        self.diagnostics.iter().any(|d| d.code == code)
    }

    /// I soli codici bloccanti, nell'ordine in cui sono stati trovati.
    pub fn blocking_codes(&self) -> Vec<&str> {
        self.blocking().map(|d| d.code.as_str()).collect()
    }

    /// Il primo problema bloccante: quello da mostrare a chi ha sbagliato.
    pub fn first_blocking(&self) -> Option<&Diagnostic> {
        self.blocking().next()
    }
}

/// Che cosa arriva al collo di bottiglia.
///
/// `source` sono **i byte che sono arrivati**, non una descrizione: il validatore
/// li guarda e l'hash li impegna, ed è per questo che la stessa richiesta
/// ritrasmessa dà lo stesso verdetto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub route: Route,
    /// Chi manda. È l'autore (`kbs_core::Origin::Human::by`), e da lì viene la
    /// visibilità: D5 non ha ruoli, ha relazioni.
    pub by: PersonId,
    /// Il corso. Se `None`, lo prende da `<meta name="kb-course">`; se manca
    /// anche lì, è un rifiuto: un argomento senza perimetro di condivisione non
    /// ha a chi essere condiviso.
    pub course: Option<CourseId>,
    /// Il percorso relativo. Se `None`, se lo deriva dal titolo
    /// (`<strada>/<slug>.html`), perché l'id segue il file e un id che cambia a
    /// ogni intake renderebbe i commenti ancorati nonsensibili.
    pub rel_path: Option<String>,
    pub source: String,
}

/// Che cosa è uscito dal collo di bottiglia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub route: Route,
    /// Lo stato **dopo** l'intake, che per D4 non è mai `in-corso` se non c'è
    /// una ratifica nel negozio.
    pub argument: Argument,
    pub verdict: Verdict,
    /// Le claim registrate, nell'ordine in cui sono state dichiarate.
    pub claims: Vec<Claim>,
    /// `false` quando la scrittura è stata rifiutata: un grafo dei prerequisiti
    /// ciclico non entra, e l'argomento non viene creato. Il verdetto dice
    /// perché.
    pub stored: bool,
    /// I prerequisiti **dichiarati**, in attesa che esistano i nodi che
    /// puntano. `argument.prerequisites` li ha gia' dopo la scrittura; prima no,
    /// e la ragione sta in [`prepara`].
    pub declared_prerequisites: Vec<ArgumentId>,
    /// Lo stato che il documento dichiara, che nessuno scrive da solo: lo chiede
    /// il negozio, e la sua risposta entra nel verdetto.
    pub declared_state: Option<PublicationState>,
}

/// L'hash del contenuto di un artifact: `SHA256(0x14 ‖ sorgente)`.
///
/// Sul **documento intero**, non sul contratto. È più severo di
/// `kbs_core::Ratification::contract_hash`, che confronta con
/// `Argument::content_hash`: changing una riga di corpo invalida la ratifica.
/// È una scelta, e la si dichiara: la direzione severa è quella in cui non si
/// pubblica mai qualcosa che il docente non ha guardato. Il costo è che
/// correggere un refuso dopo la ratifica costa un `kbs ratify` — che è una
/// riga di comando, non un muro.
pub fn content_hash(source: &str) -> String {
    let mut h = Sha256::new();
    h.update([crate::corpus_hash::tag::CONTENT]);
    h.update(source.as_bytes());
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// L'argomento esiste gia' nel database?
///
/// Una `SELECT` su `Store::conn()`, che `kbs-store` dichiara come la porta di
/// servizio e dichiara anche perche' esiste: qui la domanda e' di manutenzione —
/// «esiste gia'?» — e non ha un predicato di persona, perche' riguarda una riga
/// e non un contenuto. Passare da `read_argument` non funzionerebbe: «non
/// esiste» e «non lo vedi» sono la stessa risposta (D5), e qui le due cose
/// vanno distinte.
fn esiste(store: &Store, id: &ArgumentId) -> bool {
    store
        .conn()
        .query_row("SELECT 1 FROM arguments WHERE id = ?1", [id.0.as_str()], |_| Ok(()))
        .is_ok()
}

/// Lo stato e la ratifica che il negozio ha **gia'** per un argomento.
///
/// Sono i due campi che D4 dichiara di proprieta' della porta e non del file, e
/// la ragione per cui questa funzione esiste e' tutta qui: una riscanione
/// ricostruisce l'`Argument` dal file, e se lo ricostruisse **senza** questi due
/// campi chiederebbe al negozio due scritture che il negozio rifiuta per
/// regola — portare `in-corso` a `bozza`, che non e' una scrittura ma una
/// transizione, e svuotare una ratifica, che D4 vieta a chiunque non sia la
/// porta. Il rifiuto del negozio e' giusto; la domanda sbagliata e' la nostra.
///
/// Una `SELECT` di servizio, come [`esiste`]: qui non si chiede «posso vederlo»
/// (D5) ma «che cosa c'e' gia' scritto», e sono due domande diverse.
fn porta_e_ratifica_memorizzate(
    store: &Store,
    id: &ArgumentId,
) -> Result<(Option<PublicationState>, Option<Ratification>)> {
    let riga: Option<(String, Option<String>, Option<i64>, Option<String>, Option<String>)> = store
        .conn()
        .query_row(
            "SELECT state, ratified_by, ratified_at, ratified_contract_hash, ratified_note \
               FROM arguments WHERE id = ?1",
            [id.0.as_str()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|e| Error::Store(kbs_store::Error::Sqlite(e)))?;
    let Some((stato, by, at, hash, nota)) = riga else {
        return Ok((None, None));
    };
    // Le colonne sono NULL quando non c'e' ratifica: leggerle come `String`
    // sarebbe un errore di tipo su una riga perfettamente valida.
    let ratifica = match (by, at, hash) {
        (Some(by), Some(at), Some(hash)) => Some(Ratification {
            by: PersonId(by),
            at: Millis(at),
            contract_hash: hash,
            note: nota.unwrap_or_default(),
        }),
        _ => None,
    };
    let stato = decode_stato(&stato)?;
    Ok((Some(stato), ratifica))
}

/// Lo stato come lo scrive il negozio. Una `SELECT` che legge una colonna
/// `TEXT` puo' solo dare successo o fallimento, e qui fallire significa che la
/// riga e' da riparare: si dice che e' successo e si restituisce l'errore.
fn decode_stato(raw: &str) -> Result<PublicationState> {
    match raw {
        "bozza" => Ok(PublicationState::Bozza),
        "del-docente" => Ok(PublicationState::DelDocente),
        "in-corso" => Ok(PublicationState::InCorso),
        "archiviato" => Ok(PublicationState::Archiviato),
        altro => Err(Error::StatoSconosciuto {
            raw: altro.to_string(),
        }),
    }
}

/// Un solo tipo di tempo nel sistema: `kbs_core::Millis` lo dichiara, e due
/// formati renderebbero ogni ordinamento falso.
fn now() -> kbs_core::Millis {
    kbs_core::Millis::now()
}

/// Il codice kebab-case di un errore di dominio, per il referto.
///
/// `kbs_core::Invariant` non ha `Serialize` — e non gliele si mette, perché è il
/// contratto di un altro crate e un nome di variante non è un'invariante. Qui la
/// traduzione è esplicita e totale: se domani `kbs-core` aggiunge una variante,
/// questo `match` non compila, che è il comportamento giusto.
pub fn invariant_code(inv: &Invariant) -> &'static str {
    match inv {
        Invariant::CitableWithoutRatification(_) => "citable-without-ratification",
        Invariant::StaleRatification { .. } => "stale-ratification",
        Invariant::CohortBelowThreshold { .. } => "cohort-below-threshold",
        Invariant::PrerequisiteCycle(_) => "prerequisite-cycle",
    }
}

/// Riceve, valida e registra. L'unica porta di scrittura di un `Argument`.
///
/// Sono due atti ([`prepara`] e [`completa`) accorpati, e sono due atti per una
/// ragione che un corpus di quarant'item rende inevitabile: **un corpus e' un
/// grafo**, e `mappe/04` puo' richiedere `laboratori/03` che la scansione non ha
/// ancora incontrato. Scrivere tutto in un colpo significherebbe o fallire con
/// una chiave esterna, o accettare un prerequisito pendente — e un prerequisito
/// che non esiste e' un argomento che non si puo' leggere.
///
/// Chi chiama per un file solo (cattura, CLI, MCP) usa [`receive`], che e'
/// questa. Chi cammina su una cartella usa [`crate::scan`], che chiama
/// [`prepara`] per tutti i file e poi [`completa`] per tutti: e' cosi' che il
/// controllo anticiclo di `kbs_store` vede il grafo **intero**, e non quello
/// parziale che si era incontrato per primo.
pub fn receive(store: &mut Store, request: Request) -> Result<Receipt> {
    let mut ricevuta = prepara(store, request.clone())?;
    completa(store, &mut ricevuta, &request)?;
    Ok(ricevuta)
}

/// Prima fase: validazione, costruzione e scrittura dell'argomento **senza**
/// grafo dei prerequisiti.
///
/// E' la fase che può sempre andare: non scrive archi, quindi non può fallire
/// perché un nodo non esiste ancora. E' anche la fase che decide tutto cio' che
/// riguarda il documento — titolo, contratto, riferimenti fuori dalla scatella
/// (D15), origine e lock (D10) — perche' quelle verifiche non dipendono
/// dall'ordine in cui la cartella si presenta.
pub fn prepara(store: &mut Store, request: Request) -> Result<Receipt> {
    let report = kbs_doc::validate(&request.source);
    let mut verdict = Verdict::default();
    for issue in &report.issues {
        verdict.push(Diagnostic {
            code: code_of_doc_issue(&issue.code).to_string(),
            message: issue.message.clone(),
            blocking: issue.severity == kbs_doc::Severity::Blocking,
        });
    }
    // D7 ha una regola che `kbs_doc` non può applicare, e il motivo è dichiarato
    // dalla funzione stessa: `guardian_oltre_budget`.
    if let Some(contratto) = &report.contract {
        if let Some(d) = guardian_oltre_budget(contratto) {
            verdict.push(d);
        }
    }

    let title = match report.parsed.title.as_deref() {
        Some(t) if !t.trim().is_empty() => t.to_string(),
        _ => {
            return Err(Error::TitoloMancante {
                problemi: if verdict.diagnostics.is_empty() {
                    "nessuno: l'artifact e' un testo senza struttura".to_string()
                } else {
                    verdict
                        .diagnostics
                        .iter()
                        .map(|d| d.code.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            })
        }
    };

    let rel_path = request.rel_path.clone().unwrap_or_else(|| {
        format!(
            "{}/{}.html",
            request.route.as_str(),
            kbs_doc::slug::kebab(&title)
        )
    });
    crate::corpus_hash::check_rel_path(&rel_path)?;
    let id = ArgumentId::from_rel_path(&rel_path);

    let course = match request.course.or_else(|| declared_course(&report)) {
        Some(c) => c,
        None => return Err(Error::CorsoMancante),
    };
    // `arguments.course_id` referenzia `sources.id`: un argomento non puo' nascere
    // senza che il suo corso esista. Registrarlo qui e non lasciarlo al chiamante
    // e' la scelta che rende le quattro strade davvero equivalenti — una strada
    // che funziona solo se il chiamante ha fatto un passo in piu' non e' la
    // stessa strada. Idempotente: se il corso c'e' gia', non si tocca nulla, e in
    // particolare non si sovrascrivono `last_scan_at` e `corpus_hash` che sono di
    // chi ha indicizzato per ultimo.
    if store.source_status(&course)?.is_none() {
        store.register_source(&kbs_store::Source {
            id: course.clone(),
            slug: course.0.clone(),
            rel_path: ".".to_string(),
            status: kbs_store::SourceStatus::Active,
            registered_at: now(),
            last_scan_at: None,
            corpus_hash: None,
        })?;
    }

    let prerequisites: Vec<ArgumentId> = declared_prerequisites(&request.source)
        .iter()
        .map(|p| ArgumentId::from_rel_path(p))
        .collect();
    let now = now();
    let content = content_hash(&request.source);
    let declared = declared_state(&report);

    // L'origine e' dichiarata dal documento. Una dichiarazione `generated` senza
    // lock e' un diario (D10): non si scrive, e non si scrive `human`, perche'
    // scrivere `human` sarebbe dichiarare il falso nel campo che esiste per non
    // doverlo dichiarare.
    let by = request.by.clone();
    let origin = match declared_origin(&report, by.clone()) {
        Ok(o) => o,
        Err(d) => {
            verdict.push(d);
            verdict.content_hash = Some(content.clone());
            return Ok(Receipt {
                route: request.route,
                argument: Argument {
                    id,
                    title,
                    summary: String::new(),
                    state: PublicationState::Bozza,
                    course,
                    prerequisites: Vec::new(),
                    origin: Origin::Human { by: request.by, at: now },
                    rel_path: Some(rel_path),
                    content_hash: content,
                    created_at: now,
                    updated_at: now,
                    ratified: None,
                },
                verdict,
                claims: Vec::new(),
                stored: false,
                declared_prerequisites: Vec::new(),
                declared_state: declared,
            });
        }
    };

    // I due campi di D4 che **non** sono del file, letti prima di costruire
    // l'argomento: lo stato che il negozio ha gia' pubblicato e la ratifica che
    // il docente ha gia' firmato. Vedi `porta_e_ratifica_memorizzate` perche'
    // senza di loro questa funzione romperebbe la regola che applica.
    let (stato_della_porta, ratifica) = porta_e_ratifica_memorizzate(store, &id)?;

    let argument = Argument {
        id,
        title,
        summary: report.parsed.body_text.chars().take(280).collect(),
        // La sola scrittura di stato che questa strada fa e' fra i due stati
        // mutabili. `in-corso` e `archiviato` passano da `apply_declared_state`,
        // che chiede la ratifica al negozio invece di fingere di essere la
        // ratifica — e se il negozio ne ha gia' una, lo stato pubblicato resta:
        // una riscanione non e' un atto del docente, e `kbs_store` rifiuta gia'
        // di portare un argomento fuori da `in-corso` con una scrittura.
        state: {
            let chiesto: PublicationState = match declared {
                Some(PublicationState::DelDocente) => PublicationState::DelDocente,
                _ => PublicationState::Bozza,
            };
            match stato_della_porta {
                Some(s) if !s.is_mutable() => s,
                _ => chiesto,
            }
        },
        course,
        // Prima fase: nessun grafo. Lo scrive [`completa`].
        prerequisites: Vec::new(),
        origin,
        rel_path: Some(rel_path),
        content_hash: content.clone(),
        created_at: now,
        updated_at: now,
        // La ratifica che c'era prima, se c'era. Portarla avanti non e'
        // reintrodurre la firma di un altro: e' la stessa firma, sugli stessi
        // byte che l'hash qui sotto confronta, ed e' l'hash che decide se vale
        // ancora (`kbs_core::check_citable`).
        ratified: ratifica,
    };

    store.upsert_argument(&argument)?;
    verdict.content_hash = Some(content);

    Ok(Receipt {
        route: request.route,
        argument,
        verdict,
        claims: Vec::new(),
        stored: true,
        declared_prerequisites: prerequisites,
        declared_state: declared,
    })
}

/// Seconda fase: il grafo dei prerequisiti, le claim, e lo stato dichiarato.
///
/// Il ciclo e' un **verdetto**, non un crash: l'item viene riportato come non
/// valido e non entra, e `stored` torna `false`. Un intake che si fermasse al
/// primo errore non permetterebbe a un insegnante di vedere *tutto* cio' che non
/// va, e quello che non si vede e' quello che si rimane.
///
/// La funzione non fallisce mai per un ciclo: e' un verdetto. Fallisce per
/// quello che e' un errore vero (il database non c'e', il documento non si
/// rilegge) e lascia al chiamante la forma.
pub fn completa(store: &mut Store, ricevuta: &mut Receipt, request: &Request) -> Result<()> {
    if !ricevuta.stored {
        return Ok(());
    }
    let id = ricevuta.argument.id.clone();
    let declared = std::mem::take(&mut ricevuta.declared_prerequisites);
    if !declared.is_empty() {
        // Un prerequisito che non esiste non si puo' scrivere, e non si puo'
        // nemmeno ignorare: un argomento che dice «prima questo» e non lo
        // dice piu' mente sul proprio ordine di lettura. Il caso normale e' che
        // l'altro file sia stato rifiutato — per un contratto rotto, per un
        // riferimento fuori dalla scatella (D15), per un'origine generata senza
        // lock (D10) — e allora sono **due** problemi da mostrare, non uno da
        // nascondere dietro l'altro.
        if let Some(mancante) = declared.iter().find(|p| !esiste(store, p)) {
            ricevuta.verdict.push(Diagnostic::blocking(
                "prerequisite-assente",
                format!(
                    "il prerequisito {mancante} non e' nel corpus: o non e' stato indicizzato, o e' stato rifiutato — e in questo caso la ragione e' nel referto, sulla riga dell'item che lo dichiara. Un argomento che richiede qualcosa che non c'e' non puo' entrare con il suo grafo, perche' dire «prima questo» e non dirlo piu' mente sul proprio ordine di lettura"
                ),
            ));
            ricevuta.stored = false;
            return Ok(());
        }
        let mut con_grafo = ricevuta.argument.clone();
        con_grafo.prerequisites = declared;
        match store.upsert_argument(&con_grafo) {
            Ok(()) => ricevuta.argument = con_grafo,
            Err(kbs_store::Error::Invariant(Invariant::PrerequisiteCycle(p))) => {
                // Il grafo non entra, quindi i suoi archi non restano: si
                // rimuovono perche' un argomento con archi a meta' e' peggio di
                // un argomento senza grafo.
                let mut senza = ricevuta.argument.clone();
                senza.prerequisites = Vec::new();
                store.upsert_argument(&senza)?;
                ricevuta.argument = senza;
                ricevuta.verdict.push(Diagnostic::blocking(
                    "prerequisite-cycle",
                    format!("il prerequisito {p} chiude un ciclo: l'argomento non entra (D6)"),
                ));
                ricevuta.stored = false;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
    }

    let report = kbs_doc::validate(&request.source);
    let claims = register_claims(
        store,
        &request.by,
        &report,
        &ricevuta.argument,
        &request.source,
        &mut ricevuta.verdict,
    )?;
    ricevuta.claims = claims;

    if let Some(d) = apply_declared_state(store, &id, ricevuta.declared_state) {
        ricevuta.verdict.push(d);
    }
    // Lo stato finale e' quello del negozio, non quello dichiarato: se la
    // ratifica c'e', `in-corso`; se no, resta bozza. Rileggerlo e' cio' che
    // impedisce alla ricevuta di mentire su uno stato.
    ricevuta.argument = store.read_argument(&request.by, &id)?;
    Ok(())
}

/// Chiede al negozio lo stato che il file dichiara, e riporta il suo rifiuto.
///
/// **Non reimplementa la regola**: chi decide è `kbs_store::publish`, e il
/// motivo del rifiuto finisce nel verdetto con il codice dell'invariante. Un
/// file che dichiara `in-corso` senza ratifica resta bozza e lo dice — ma lo
/// dice come avvertenza, non come errore: non è rotto, è in attesa.
pub fn apply_declared_state(
    store: &mut Store,
    id: &ArgumentId,
    declared: Option<PublicationState>,
) -> Option<Diagnostic> {
    let esito = match declared? {
        PublicationState::Bozza | PublicationState::DelDocente => return None,
        PublicationState::InCorso => store.publish(id),
        PublicationState::Archiviato => store.archive(id),
    };
    match esito {
        Ok(_) => None,
        Err(kbs_store::Error::Invariant(inv)) => Some(Diagnostic::warning(
            invariant_code(&inv),
            format!("{inv}: il file dichiara lo stato ma la ratifica manca o non è più valida"),
        )),
        Err(e) => Some(Diagnostic::warning("state-transition", e.to_string())),
    }
}

// ── ciò che il documento dichiara ─────────────────────────────────────────────

/// D7: `GUARDIAN` deve stare **dentro i primi 640 byte**, e questa funzione
/// dice perché sta qui e non dentro `kbs-doc`.
///
/// # Che cosa dice D7
///
/// «`GUARDIAN` deve stare dentro i primi 640 byte, perché il troncamento
/// taglia la coda». La quantità è l'estensione della sezione: quanto occupa
/// dalla sua intestazione alla sezione successiva. Il limite è il budget della
/// prima sezione, che è 640.
///
/// # Perché non basta `kbs-doc`
///
/// `kbs_doc::contract` ha il codice giusto — `ContractError::GuardianTooLate`,
/// che [`code_of_doc_issue`] traduce in `contract-guardian-out-of-budget` — ma
/// lo confronta con l'**offset** della riga `## GUARDIAN`. Per la prima sezione
/// quell'offset è sempre zero, quindi la regola non può mai fallire: un
/// `GUARDIAN` di duemila byte passa. Non è un numero che `kbs-doc` sbaglia a
/// leggere: è un numero diverso da quello che D7 nomina, e nessuna correzione
/// di lettura lo trasformerebbe in quello giusto.
///
/// # Perché qui
///
/// D7 mette questa regola nell'elenco di ciò che «il validatore deve applicare,
/// in fase di build», accanto a `contracts.truncated = 1 ⇒ non eseguibile».
/// Il troncamento è una decisione di esecuzione, e l'esecuzione è la porta: è
/// la stessa separazione che il modulo [`crate::gate`] dichiara per il
/// verdetto — «il negozio ha l'hash del documento e non può rispondere «questa
/// pagina chiama una CDN»». Qui la quantità c'è già: arriva nel
/// `ContractReport` che `kbs_doc::validate` ha prodotto, con gli offset di
/// ogni intestazione. Non si rilegge il contratto e non se ne replica la
/// grammatica: si usa il posto dove la sezione successiva comincia, che è
/// esattamente il posto dove `kbs-doc` stesso calcola la fine di una sezione.
fn guardian_oltre_budget(contratto: &kbs_doc::contract::ContractReport) -> Option<Diagnostic> {
    use kbs_doc::contract::{SECTIONS, SectionReport};

    let sezioni: &[SectionReport] = &contratto.sections;
    let (posizione, guardian) = sezioni.iter().enumerate().find(|(_, s)| s.index == 0)?;
    // La sezione finisce dove comincia la successiva; l'ultima finisce con il
    // contratto. `total_bytes` è la misura **non troncata**, che è quella giusta:
    // se il contratto è troncato, l'agente legge meno byte, e la sezione che
    // li contiene è tagliata — che è esattamente ciò che la regola vieta.
    let fine = sezioni
        .get(posizione + 1)
        .map(|s| s.heading_offset)
        .unwrap_or(contratto.total_bytes);
    let limite = SECTIONS[0].budget;
    if fine <= limite {
        return None;
    }
    Some(Diagnostic::blocking(
        "contract-guardian-out-of-budget",
        format!(
            "la sezione GUARDIAN finisce al byte {fine} del contratto e deve stare nei primi \
             {limite} (D7): il troncamento taglia la coda, e un vincolo che il lettore non vede \
             non è un vincolo. Il GUARDIAN sta al byte {} del contratto e vuol dire che la \
             sezione è lunga, non che è stata spostata",
            guardian.heading_offset
        ),
    ))
}

fn declared_course(report: &kbs_doc::ArtifactReport) -> Option<CourseId> {
    report
        .parsed
        .meta
        .get("kb-course")
        .filter(|v| !v.trim().is_empty())
        .map(|v| CourseId(v.clone()))
}

fn declared_state(report: &kbs_doc::ArtifactReport) -> Option<PublicationState> {
    let raw = report.parsed.meta.get("kb-state")?.trim();
    match raw {
        "bozza" => Some(PublicationState::Bozza),
        "del-docente" => Some(PublicationState::DelDocente),
        "in-corso" => Some(PublicationState::InCorso),
        "archiviato" => Some(PublicationState::Archiviato),
        _ => None,
    }
}

/// L'origine dichiarata, o il motivo per cui non si può onorare.
///
/// `Err(Diagnostic)` è il caso `generated` senza lock. Non è un dettaglio: è la
/// differenza fra un registro e un diario (D10), e la variante giusta per dirlo
/// è un problema bloccante, non un avvertimento.
fn declared_origin(report: &kbs_doc::ArtifactReport, by: PersonId) -> Result<Origin, Diagnostic> {
    let meta = &report.parsed.meta;
    match meta.get("kb-origin").map(|s| s.trim()).unwrap_or("human") {
        "human" => Ok(Origin::Human { by, at: Millis::now() }),
        "derived" => {
            let from = meta.get("kb-derived-from").ok_or_else(|| {
                Diagnostic::blocking(
                    "derived-without-source",
                    "l'artifact dichiara l'origine `derived` e non dice da quale argomento deriva",
                )
            })?;
            Ok(Origin::Derived { from: ArgumentId::from_rel_path(from), at: Millis::now() })
        }
        "generated" => {
            let field = |k: &str| meta.get(k).filter(|v| !v.trim().is_empty()).cloned();
            let need = |k: &str| {
                field(k).ok_or_else(|| {
                    Diagnostic::blocking(
                        "generated-without-lock",
                        format!(
                            "l'artifact dichiara l'origine `generated` e non porta `{k}`: D10 dice che ogni generazione registra modello, hash del prompt, hash del corpus e versione del generatore"
                        ),
                    )
                })
            };
            let model_id = need("kb-lock-model")?;
            let prompt_hash = need("kb-lock-prompt")?;
            let corpus_hash = need("kb-lock-corpus")?;
            let generator_version = need("kb-lock-generator")?;
            let at = meta
                .get("kb-lock-at")
                .and_then(|v| v.trim().parse::<i64>().ok())
                .map(Millis)
                .unwrap_or_else(Millis::now);
            Ok(Origin::Generated {
                lock: kbs_core::ModelLock { model_id, prompt_hash, corpus_hash, generator_version, at },
                by,
                at,
            })
        }
        altro => Err(Diagnostic::blocking(
            "unknown-origin",
            format!("`kb-origin: {altro}` non è un'origine: sono `human`, `derived`, `generated`"),
        )),
    }
}

/// I prerequisiti, dai `data-prereq`.
///
/// `scraper` serve per questo e per `data-stato`: nessuna delle due cose è nel
/// `ParsedArtifact` di `kbs-doc`, che espone attributi di `<meta>` ma non degli
/// elementi del corpo. È una duplicazione dichiarata, piccola e con un
/// proprietario possibile (`kbs-doc` è il posto giusto, e il campo giusto è
/// `ParsedClaim::declared_status`).
fn declared_prerequisites(source: &str) -> Vec<String> {
    let doc = Html::parse_document(source);
    let sel = Selector::parse("li[data-prereq]").expect("selettore statico valido");
    let mut out = Vec::new();
    for el in doc.select(&sel) {
        if let Some(p) = el.value().attr("data-prereq") {
            let p = p.trim();
            if !p.is_empty() && !out.iter().any(|x: &String| x == p) {
                out.push(p.to_string());
            }
        }
    }
    out
}

/// Lo stato dichiarato di una claim, indicizzato per id.
///
/// Vedi [`declared_prerequisites`] sulla ragione di `scraper`. La regola che
/// 这个 modulo applica è che **lo span tiene lo stato**: una claim che si
/// dichiara `supported` senza span è `unciteable`, perché `span_anchor` senza
/// testo è un indirizzo che il lettore non può verificare (D6).
fn declared_claim_statuses(source: &str) -> BTreeMap<String, ClaimStatus> {
    let doc = Html::parse_document(source);
    let sel = Selector::parse("[data-claim-id]").expect("selettore statico valido");
    let mut out: BTreeMap<String, ClaimStatus> = BTreeMap::new();
    for el in doc.select(&sel) {
        let Some(id) = el.value().attr("data-claim-id").map(str::trim) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let dichiarato = el.value().attr("data-stato").map(str::trim).and_then(parse_stato);
        out.entry(id.to_string())
            .and_modify(|stato| {
                // Una ridichiarazione non cancella la dichiarazione piu' forte:
                // `contradicted` resta `contradicted` anche se un altro elemento
                // ripete la stessa claim come `supported`.
                if matches!(stato, ClaimStatus::Supported) {
                    if let Some(nuovo) = dichiarato.clone().filter(|n| *n != ClaimStatus::Supported) {
                        *stato = nuovo;
                    }
                }
            })
            .or_insert_with(|| dichiarato.unwrap_or(ClaimStatus::Supported));
    }
    out
}

fn parse_stato(raw: &str) -> Option<ClaimStatus> {
    match raw {
        "supported" => Some(ClaimStatus::Supported),
        "contradicted" => Some(ClaimStatus::Contradicted),
        "unciteable" => Some(ClaimStatus::Unciteable),
        _ => None,
    }
}

/// L'ancora dello span, ma solo se lo span **porta testo**.
///
/// `Claim::span_anchor` senza `span_text` è un indirizzo che il lettore non può
/// verificare (D6), e `kbs-core` lo dice: la coppia si scrive insieme o non si
/// scrive. Quindi qui l'ancora esiste esattamente dove esiste il testo.
fn anchor_of(span: &kbs_doc::SpanBinding) -> Option<String> {
    match span {
        kbs_doc::SpanBinding::Bound { anchor, text } if !text.trim().is_empty() => {
            Some(anchor.clone())
        }
        _ => None,
    }
}

/// Registra le claim dichiarate, con il tetto dello span.
///
/// Un id di claim che compare due volte è una **ridichiarazione**, non un
/// errore: la scena 3D dichiara la stessa claim sullo span e sul nodo che la
/// sostiene (D15.1), e rifiutare l'artifact per questo sarebbe rifiutare il
/// modo in cui la scena è fatta. Se ne registra una e lo si dice.
fn register_claims(
    store: &mut Store,
    by: &PersonId,
    report: &kbs_doc::ArtifactReport,
    argument: &Argument,
    source: &str,
    verdict: &mut Verdict,
) -> Result<Vec<Claim>> {
    let dichiarati = declared_claim_statuses(source);
    let emittenti = declared_emitters(source);
    let mut visti: BTreeMap<String, ()> = BTreeMap::new();
    let mut out = Vec::new();
    // Le claim gia' in registro per questo argomento. Servono perche' la strada
    // del file **deve** essere rieseguibile: un agente riscrive un file, la
    // scansione gira di nuovo, e senza questo la seconda passata fallirebbe con
    // `DuplicateId` su un'id che essa stessa aveva scritto. `append_claim` ha
    // ragione a rifiutare: un registro che sovrascrive le proprie affermazioni
    // non e' un registro. La riconciliazione e' quindi qui e ha una forma sola:
    // la stessa affermazione non si registra due volte, e la stessa id con un
    // testo diverso e' un conflitto che si lascia emergere.
    let gia: BTreeMap<String, String> = store
        .claims_for(by, &argument.id)?
        .into_iter()
        .map(|c| (c.id, c.text))
        .collect();
    for parsed in &report.parsed.claims {
        if visti.insert(parsed.id.clone(), ()).is_some() {
            verdict.push(Diagnostic::warning(
                "claim-ridichiara",
                format!("la claim `{}` è dichiarata più volte: se ne registra una", parsed.id),
            ));
            continue;
        }
        // Lo span tiene: nessuno span, nessuna affermazione sostenuta. E se
        // non c'è, lo si dice ad alta voce: una claim che non trova il proprio
        // span è quasi sempre un artifact scritto con una convenzione diversa
        // da quella di `kbs-doc`, e un silenzio farebbe sembrare il registro
        // vuoto quando è mal letto.
        let declared = dichiarati.get(&parsed.id);
        let status = if !parsed.span.is_bound() {
            verdict.push(Diagnostic::warning(
                "claim-non-sostenuta",
                format!(
                    "la claim `{}` non trova lo span che la sostiene: entra in registro come `unciteable` e non nell'output",
                    parsed.id
                ),
            ));
            ClaimStatus::Unciteable
        } else {
            declared.cloned().unwrap_or_else(|| parsed.core_status())
        };
        let claim = Claim {
            id: format!("{}::{}", argument.id, parsed.id),
            course: argument.course.clone(),
            argument: argument.id.clone(),
            text: parsed.text.clone(),
            span_anchor: anchor_of(&parsed.span),
            span_text: parsed.span.text().map(str::to_string),
            status,
            emitted_at: argument.updated_at,
            emitted_by: emittente(emittenti.get(&parsed.id), &argument.id),
        };
        match gia.get(&claim.id) {
            // Stesso id, stesso testo: e' la stessa affermazione, detta due
            // volte dalla stessa strada. Non si registra e non si avvisa: un
            // avvertimento per ogni riscanzione sarebbe rumore.
            Some(testo) if *testo == claim.text => continue,
            // Stesso id, testo diverso: qui `append_claim` dice no, ed è la
            // risposta giusta — due affermazioni diverse non possono condividere
            // un'id, e sceglierne una sarebbe cancellare l'altra.
            _ => store.append_claim(&claim)?,
        }
        out.push(claim);
    }
    Ok(out)
}

fn declared_emitters(source: &str) -> BTreeMap<String, String> {
    let doc = Html::parse_document(source);
    let sel = Selector::parse("[data-claim-id][data-claim-emitter]").expect("selettore statico valido");
    let mut out = BTreeMap::new();
    for el in doc.select(&sel) {
        let (Some(id), Some(e)) = (
            el.value().attr("data-claim-id"),
            el.value().attr("data-claim-emitter"),
        ) else {
            continue;
        };
        out.insert(id.trim().to_string(), e.trim().to_string());
    }
    out
}

fn emittente(dichiarato: Option<&String>, argument: &ArgumentId) -> Emitter {
    let grezzo = dichiarato.map(|s| s.as_str()).unwrap_or("content");
    match grezzo.split_once(':') {
        Some(("teacher", persona)) => Emitter::Teacher { by: PersonId(persona.trim().to_string()) },
        Some(("work", osservazione)) => {
            Emitter::FromWork { observation: osservazione.trim().to_string() }
        }
        // `content` e qualunque cosa non riconosciuta: l'affermazione è
        // emessa dall'artifact che la contiene, che è ciò che `kbs-core` dice
        // per la variante di default.
        _ => Emitter::Content { argument: argument.clone() },
    }
}

/// Il codice kebab-case di un `kbs_doc::IssueCode`.
///
/// I codici del contratto (`contract-missing-section`,
/// `contract-guardian-out-of-budget`, `contract-over-budget`) sono quelli che
/// dichiara anche `kbs-fixtures::contract::Defect::expected_code`, e sono qui
/// per estensione dei nomi di `kbs_doc::ContractError`: è l'unico posto in cui
/// la traduzione vive, e i due elenchi non possono divergere senza che un
/// `match` smetta di compilare.
///
/// I codici dell'anagrafe (`anagrafe-campo-vuoto`, `anagrafe-data-non-iso8601`,
/// …) vengono da `kbs_doc::anagrafe::AnagrafeWarning` per estensione dei nomi,
/// come quelli del contratto: un codice che il referto mostra e una variante
/// dell'enum che lo produce non possono essere due elenchi.
///
/// Sono tutti non bloccanti, e questa traduzione li mette accanto agli altri
/// avvisi: l'anagrafe entra nel verdetto come entra una claim non verificabile
/// — una cosa che il docente vede nel referto e che nessuno subisce.
pub fn code_of_doc_issue(code: &kbs_doc::IssueCode) -> &'static str {
    use kbs_doc::anagrafe::AnagrafeWarning as A;
    use kbs_doc::contract::ContractError as C;
    use kbs_doc::IssueCode as I;
    match code {
        I::NoTitle => "no-title",
        I::NoContract => "no-contract",
        I::Contract(C::MissingSection { .. }) => "contract-missing-section",
        I::Contract(C::DuplicateSection { .. }) => "contract-duplicate-section",
        I::Contract(C::OutOfOrder { .. }) => "contract-out-of-order",
        I::Contract(C::UnknownSection { .. }) => "contract-unknown-section",
        I::Contract(C::EmptySection { .. }) => "contract-empty-section",
        I::Contract(C::GuardianTooLate { .. }) => "contract-guardian-out-of-budget",
        I::Contract(C::OverCap { .. }) => "contract-over-budget",
        I::Contract(C::SectionOverBudget { .. }) => "contract-section-over-budget",
        I::ExternalReference { .. } => "external-reference",
        I::HeadingCollision { .. } => "heading-collision",
        I::UnverifiableClaim { .. } => "unverifiable-claim",
        I::ConvertedFromMarkdown => "converted-from-markdown",
        I::Anagrafe(A::CampoVuoto { .. }) => "anagrafe-campo-vuoto",
        I::Anagrafe(A::TitoloInConflitto { .. }) => "anagrafe-titolo-in-conflitto",
        I::Anagrafe(A::LinguaInConflitto { .. }) => "anagrafe-lingua-in-conflitto",
        I::Anagrafe(A::LinguaMalformata { .. }) => "anagrafe-lingua-malformata",
        I::Anagrafe(A::DataNonIso8601 { .. }) => "anagrafe-data-non-iso8601",
        I::Anagrafe(A::ElementoNonRegistrato { .. }) => "anagrafe-elemento-non-registrato",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_quattro_strade_hanno_nomi_e_ordine_di_d10() {
        let nomi: Vec<&str> = Route::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(nomi, ["capture", "cli", "file", "mcp"]);
        for r in Route::ALL {
            assert_eq!(Route::from_label(r.as_str()), Some(r));
        }
        assert_eq!(Route::from_label("http"), None);
    }

    #[test]
    fn il_codice_del_contratto_e_il_rinomato_che_il_banco_si_aspetta() {
        use kbs_doc::contract::ContractError as C;
        assert_eq!(
            code_of_doc_issue(&kbs_doc::IssueCode::Contract(C::MissingSection {
                name: "LIMITE".into(),
                all_sections: String::new()
            })),
            "contract-missing-section"
        );
        assert_eq!(
            code_of_doc_issue(&kbs_doc::IssueCode::Contract(C::GuardianTooLate { offset: 900, limit: 640 })),
            "contract-guardian-out-of-budget"
        );
        assert_eq!(
            code_of_doc_issue(&kbs_doc::IssueCode::Contract(C::OverCap { bytes: 9000, cap: 8192 })),
            "contract-over-budget"
        );
        assert_eq!(
            code_of_doc_issue(&kbs_doc::IssueCode::ExternalReference {
                kind: "script".into(),
                url: "https://cdn.example/three.module.js".into()
            }),
            "external-reference"
        );
    }

    #[test]
    fn ogni_invariante_ha_un_codice() {
        assert_eq!(
            invariant_code(&Invariant::CitableWithoutRatification(PublicationState::Bozza)),
            "citable-without-ratification"
        );
        assert_eq!(
            invariant_code(&Invariant::StaleRatification {
                declared: "a".into(),
                current: "b".into()
            }),
            "stale-ratification"
        );
    }

    #[test]
    fn lo_hash_del_contenuto_cambia_a_un_byte() {
        assert_ne!(content_hash("<p>a</p>"), content_hash("<p>A</p>"));
        assert_eq!(content_hash("<p>a</p>"), content_hash("<p>a</p>"));
    }

    #[test]
    fn i_prerequisiti_si_leggono_e_non_si_ripetono() {
        let src = r#"<ul><li data-prereq="a/uno.html">x</li><li data-prereq="a/due.html">y</li>
            <li data-prereq="a/uno.html">x ancora</li><li>y</li></ul>"#;
        assert_eq!(declared_prerequisites(src), ["a/uno.html", "a/due.html"]);
    }

    #[test]
    fn lo_stato_dichiarato_di_una_claim_viene_dal_documento() {
        let src = r#"<p><span id="s1">testo</span></p><p data-claim-id="c1" data-claim-span="s1" data-stato="contradicted">testo</p>"#;
        let m = declared_claim_statuses(src);
        assert_eq!(m.get("c1"), Some(&ClaimStatus::Contradicted));
    }

    #[test]
    fn una_claim_senza_stato_dichiarato_e_supportata() {
        let src = r#"<p><span id="s1">testo</span></p><p data-claim-id="c1" data-claim-span="s1">testo</p>"#;
        assert_eq!(declared_claim_statuses(src).get("c1"), Some(&ClaimStatus::Supported));
    }
}
