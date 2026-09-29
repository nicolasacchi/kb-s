//! Il file come interfaccia (D10.3), alla scala di un corpus intero.
//!
//! Un agente scrive un file, il sistema lo indicizza e ne valida il contratto.
//! Questo modulo è quella strada applicata a una cartella: fa la stessa cosa
//! che [`crate::route::receive`] fa per un file, e la fa chiamando
//! `receive`. Non c'è una seconda validazione e non c'è un secondo scrittore:
//! se qui ci fosse, «validato in intake» e «validato in scansione» sarebbero
//! due proprietà diverse, e la più debole vincerebbe per abitudine.
//!
//! # Il referto, e perché ha questa forma
//!
//! L'uscita di `kbs verify --json` è il contratto fra questo binario e
//! `kbs-fixtures::adapter`, che lo chiama **come processo**. La forma è
//! documentata lì e non qui, per non avere due documentazioni che divergono:
//! `items` con la diagnostica per file, `index` con i citabili e i non citabili
//! **con la ragione**, `claims` con la traccia di registro, `instances`.
//!
//! `instances` è **vuoto**, e va detto perché è una scelta e non una
//! dimenticanza: questa funzione non genera esercizi, e non è il posto che li
//! genera. Il banco di prova pretende da *questa* pipeline le istanze che ha
//! nella **propria tabella Rust** (`kbs_fixtures::spec::ExerciseSpec`), e
//! quelle non sono nel corpus. Il replay deterministico è una proprietà di
//! `kbs-exercise` e va verificato lì; metterlo in un banco che chiama un
//! processo che non genera nulla sta misurando la strada, non la proprietà.
//!
//! La strada che porta l'esercizio dal file al registro è
//! [`crate::authoring`], e la ragione per cui è un modulo separato e non una
//! riga qui è che **l'ordine conta**: `indexa` valida e mette in bozza, e
//! `authoring` pretende che l'argomento ci sia già. Farli nello stesso passaggio
//! scriverebbe l'esercizio su un argomento che il validatore può ancora
//! rifiutare.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kbs_core::{ClaimStatus, CourseId, Millis};
use kbs_store::{Person, Source, SourceStatus, Store};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use crate::corpus_hash::{self, CorpusHash};
use crate::error::{Error, Result};

/// I corsi che un documento dichiara.
///
/// Vengono presi da `kbs_doc::parse` e non da un parser qui: i `<meta>` sono
/// gia' nel `ParsedArtifact`, e rileggerli sarebbe una seconda lettura che puo'
/// divergere dalla prima.
fn corsi_dichiarati(sorgente: &str) -> Vec<CourseId> {
    kbs_doc::parse(sorgente)
        .meta
        .get("kb-course")
        .filter(|v| !v.trim().is_empty())
        .map(|v| CourseId(v.clone()))
        .into_iter()
        .collect()
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Io { path: path.to_path_buf(), source })
}
use crate::route::{self, Diagnostic, Receipt, Request, Route, Verdict};

/// Le persone che il documento dice di aver emesso le claim.
///
/// Si legge `data-claim-emitter` e non `kb-origin`: l'origine nomina chi ha
/// scritto il file, che è l'operatore della strada, mentre l'emittente nomina
/// chi ha emesso **l'affermazione**, che è una domanda diversa e D6 la pone
/// esplicitamente («chi ha dimostrato che cosa?»). Le due persone non
/// coincidono quasi mai, e confonderle significa registrare ogni affermazione
/// come se l'avesse scritta chi ha caricato il file.
fn emittenti_dichiarati(source: &str) -> Vec<kbs_core::PersonId> {
    let doc = Html::parse_document(source);
    let sel = match Selector::parse("[data-claim-emitter]") {
        Ok(s) => s,
        // Il selettore è una costante di questo modulo: se non compila è un bug
        // di compilazione, non un caso da gestire a runtime. Un `Err` qui
        // produce un corpus senza persone, che è il guasto silenzioso che la
        // funzione esiste per evitare, quindi si dice che non se ne sa niente.
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for el in doc.select(&sel) {
        let Some(grezzo) = el.value().attr("data-claim-emitter") else {
            continue;
        };
        // `teacher:<persona>` è l'unica forma che nomina una persona. Le altre
        // (`work:<osservazione>`, `content`) nominano un'altra cosa, e
        // registrarle come persone sarebbe inventare un essere umano.
        if let Some(persona) = grezzo.trim().strip_prefix("teacher:") {
            let persona = persona.trim();
            if !persona.is_empty() {
                out.push(kbs_core::PersonId(persona.to_string()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// La persona c'è già nel registro delle persone?
///
/// Una `SELECT` di servizio, come quella dei prerequisiti in `route`: qui non si
/// chiede «posso vederla» (D5) ma «esiste già la riga», e sono due domande
/// diverse. Serve perché `upsert_person` sovrascrive `display_name`: riscrivere
/// il nome di una persona che esisteva già è cancellare il nome che qualcun
/// altro aveva scritto.
fn esiste_persona(store: &Store, id: &kbs_core::PersonId) -> Result<bool> {
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM people WHERE id = ?1", [id.0.as_str()], |r| {
            r.get(0)
        })
        .map_err(|e| Error::Store(kbs_store::Error::Sqlite(e)))?;
    Ok(n > 0)
}

/// La diagnostica di un item, nella forma del banco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDiagnostic {
    pub rel_path: String,
    pub valid: bool,
    #[serde(default)]
    pub errors: Vec<Errore>,
}

/// Un errore, come lo vuole il banco: codice e messaggio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Errore {
    pub code: String,
    #[serde(default)]
    pub message: String,
}

impl From<&Diagnostic> for Errore {
    fn from(d: &Diagnostic) -> Self {
        Errore { code: d.code.clone(), message: d.message.clone() }
    }
}

/// Un argomento che non è citabile, e **perché**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonCitable {
    pub id: String,
    /// Il codice kebab-case della variante di `kbs_core::Invariant`.
    pub invariant: String,
    pub message: String,
}

/// Una riga di claim come la pipeline la mostra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimRow {
    /// `arg_…::cl_11_1`: l'id del registro è `<argomento>::<claim nel documento>`.
    pub id: String,
    #[serde(default)]
    pub rel_path: String,
    pub status: String,
    /// `true` se la riga esiste **nel registro**. `receive` la scrive sempre,
    /// anche quando è un errore: D6 dice che un errore si registra, non si
    /// cancella, e una non citabilità è un errore.
    pub in_registry: bool,
    /// `true` se la riga compare **nell'output** citabile.
    pub in_output: bool,
}

/// L'indice: che cosa è citabile e che cosa no, con la ragione.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Index {
    #[serde(default)]
    pub citable: Vec<String>,
    #[serde(default)]
    pub not_citable: Vec<NonCitable>,
}

/// Il referto intero, compresi i campi che restano vuoti.
///
/// I campi vuoti sono **dichiarati**: un referto che omette `instances` non
/// dice «non ci sono istanze», dice «non so», e un banco che non distingue le
/// due cose sta verificando il nulla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub kbs_version: String,
    /// L'hash del corpus, con la sua definizione. È la stessa tupla che entra
    /// nel lock (D11), e qui si vede perché è utile: due scansioni della stessa
    /// cartella danno lo stesso valore, e un byte di differenza no.
    pub corpus_hash: String,
    pub items: Vec<ItemDiagnostic>,
    pub index: Index,
    pub claims: Vec<ClaimRow>,
    /// Sempre vuoto, e il motivo è nel doc del modulo.
    pub instances: Vec<serde_json::Value>,
}

impl Report {
    /// La forma che `kbs_fixtures::adapter::Uscita` deserializza. `instances`
    /// è `[]` per la ragione dichiarata sopra.
    pub fn to_json(&self) -> String {
        // `serde_json` non può fallire su un valore che è già stato
        // serializzabile: la chiave `result` sta in un `Value` costruito a
        // mano, quindi l'unica alternativa sarebbe `unwrap`, che qui è
        // vietato. Si ricade sul testo e si dice perché è infallibile.
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!("{{\"kbs_version\":\"{}\",\"errore\":\"{e}\"}}", self.kbs_version)
        })
    }
}

/// Il risultato di una scansione: il referto e le ricevute, in memoria.
#[derive(Debug, Clone)]
pub struct Scan {
    pub report: Report,
    pub corpus_hash: CorpusHash,
    pub receipts: Vec<Receipt>,
}

/// Indicizza una cartella e ne ricava il referto.
///
/// `by` è l'operatore: è l'autore di ciò che non ha un autore proprio, ed è
/// l'unico che può rileggere le bozze. `by` non è un ruolo (D5): è una persona,
/// e la sua relazione col corso **non** la stabilisce questa funzione — la
/// registra il verbo `kbs insegna`, che chiama `kbs_store::add_relation`. Fino
/// a quel verbo la riga qui sopra era una dichiarazione che nessun codice di
/// produzione onorava: `relations` restava vuota e ogni percorso che chiede
/// `teaches` (l'esercizio, lo scrutinio, l'export) era chiuso a tutti.
pub fn indexa(
    store: &mut Store,
    radice: &Path,
    by: &kbs_core::PersonId,
) -> Result<Scan> {
    let corpus_hash = corpus_hash::hash_dir(radice)?;
    let mut receipts = Vec::new();
    let mut files = html_files(radice)?;
    files.sort();
    // I corsi che la cartella dichiara vanno registrati **prima** del primo
    // argomento: `arguments.course_id` referenzia `sources`, e un corso che
    // nessuno ha registrato non è un corso che si può scrivere. È la stessa
    // riga di D12 — «la cartella del corso, e a che stato è» — ed è l'unico
    // posto in cui questa tabella si scrive da questa parte.
    let mut corsi: BTreeSet<CourseId> = BTreeSet::new();
    // Le **persone** che la cartella dichiara, per la stessa ragione dei corsi e
    // non per simmetria: `claims.emitted_by` referenzia `people`, e una claim
    // che dice «emessa da Ada» non si puo' scrivere in un database in cui Ada
    // non c'e'. Senza questa riga un corpus scritto da due persone fallisce con
    // un `FOREIGN KEY constraint failed` che non nomina nessuno, e il referto
    // non sa che cosa dire.
    //
    // Chi si registra e' un'altra cosa rispetto al nome: una persona che esiste
    // gia' conserva il suo nome, perche' `upsert_person` sovrascrive
    // `display_name` e un nome e' una cosa che qualcun altro puo' aver scritto.
    let mut persone: BTreeSet<kbs_core::PersonId> = BTreeSet::new();

    // **Due passate, e non per ottimizzare.** La prima crea tutti gli argomenti,
    // la seconda scrive il grafo dei prerequisiti. Il motivo e' che un corpus e'
    // un grafo: `mappe/04` puo' richiedere `laboratori/03`, e in ordine di
    // percorso `laboratori/03` arriva prima di `mappe/04` ma `prove/01` puo'
    // richiedere `prove/02`, che arriva dopo. In una passata sola si avrebbe
    // o una chiave esterna che salta — che e' quello che succede — o un
    // prerequisito pendente, che e' peggio. Con due passate, in piu', il
    // controllo anticiclo di `kbs_store` vede il grafo intero e non quello
    // parziale, quindi un ciclo che si chiude su un file incontrato dopo
    // viene preso dove con una passata sola sarebbe sfuggito.
    let mut richieste: Vec<(String, Request)> = Vec::new();
    for (rel, path) in &files {
        let sorgente = read(path)?;
        for emittente in emittenti_dichiarati(&sorgente) {
            persone.insert(emittente);
        }
        for corso in corsi_dichiarati(&sorgente) {
            if corsi.insert(corso.clone()) {
                store.register_source(&Source {
                    id: corso.clone(),
                    slug: corso.0.clone(),
                    // Relativo al corpus, e "." perche' questa scansione *e'* la
                    // radice: `sources.rel_path` e' un percorso relativo (il
                    // vincolo e' del database) e la radice di una scansione non
                    // ha nome proprio.
                    rel_path: ".".to_string(),
                    status: SourceStatus::Active,
                    registered_at: Millis::now(),
                    last_scan_at: Some(Millis::now()),
                    corpus_hash: Some(corpus_hash.as_str().to_string()),
                })?;
            }
        }
        richieste.push((
            rel.clone(),
            Request {
                route: Route::File,
                by: by.clone(),
                course: None,
                rel_path: Some(rel.clone()),
                source: sorgente,
            },
        ));
    }

    // Le persone si registrano **dopo** aver letto tutta la cartella e
    // **prima** del primo argomento: `prepara` chiama `upsert_argument`, che
    // non scrive claim, ma `completa` le chiama — e le claim sono la prima
    // riga che porta `emitted_by`. Registrarle qui, e non in `prepara`, tiene
    // la responsabilita' in un posto solo: la stessa riga che registra i corsi.
    for persona in &persone {
        if !esiste_persona(store, persona)? {
            store.upsert_person(&Person {
                id: persona.clone(),
                // Un nome dichiarato, non un id. Il corpus non porta il nome e
                // il sistema non indovina: quello che c'e' e' un placeholder
                // esplicito, e un placeholder che si legge «operatore» non
                // finge di essere un nome proprio.
                display_name: "emittente dichiarato dal corpus".into(),
                created_at: Millis::now(),
            })?;
        }
    }

    let mut scartate: Vec<(String, Error, PathBuf)> = Vec::new();
    for (rel, richiesta) in &richieste {
        match route::prepara(store, richiesta.clone()) {
            Ok(r) => receipts.push(r),
            // Un file che non entra e' un file che il referto deve **riportare**,
            // non uno che fa fallire la scansione: un insegnante che ha
            // incollato per errore un `.txt` dentro la cartella deve vedere
            // l'errore per quello, e gli altri trenta item devono essere
            // indicizzati lo stesso.
            Err(e) => {
                let path = files
                    .iter()
                    .find(|(r, _)| r == rel)
                    .map(|(_, p)| p.clone())
                    .unwrap_or_default();
                scartate.push((rel.clone(), e, path));
            }
        }
    }
    for (rel, e, path) in &scartate {
        receipts.push(Receipt {
            route: Route::File,
            argument: rifiuto(by, rel),
            verdict: Verdict {
                content_hash: Some(route::content_hash(&read(path)?)),
                diagnostics: vec![Diagnostic {
                    code: e.code().to_string(),
                    message: e.to_string(),
                    blocking: true,
                }],
            },
            claims: Vec::new(),
            stored: false,
            declared_prerequisites: Vec::new(),
            declared_state: None,
        });
    }
    for (_, richiesta) in &richieste {
        if let Some(r) = receipts.iter_mut().find(|r| {
            r.argument.rel_path.as_deref() == richiesta.rel_path.as_deref()
        }) {
            route::completa(store, r, richiesta)?;
        }
    }

    let report = Report {
        kbs_version: env!("CARGO_PKG_VERSION").to_string(),
        corpus_hash: corpus_hash.as_str().to_string(),
        items: receipts.iter().map(item_diagnostic).collect(),
        index: Index {
            citable: receipts
                .iter()
                .filter(|r| r.argument.is_citable_now())
                .map(|r| r.argument.id.to_string())
                .collect(),
            not_citable: receipts
                .iter()
                .filter_map(|r| match kbs_core::check_citable(&r.argument) {
                    Ok(()) => None,
                    Err(inv) => Some(NonCitable {
                        id: r.argument.id.to_string(),
                        invariant: route::invariant_code(&inv).to_string(),
                        message: inv.to_string(),
                    }),
                })
                .collect(),
        },
        claims: receipts.iter().flat_map(claim_row).collect(),
        instances: Vec::new(),
    };
    Ok(Scan { report, corpus_hash, receipts })
}

fn item_diagnostic(r: &Receipt) -> ItemDiagnostic {
    let rel = r.argument.rel_path.clone().unwrap_or_default();
    ItemDiagnostic {
        rel_path: rel,
        valid: r.verdict.can_publish(),
        errors: r.verdict.blocking().map(Errore::from).collect(),
    }
}

fn claim_row(r: &Receipt) -> impl Iterator<Item = ClaimRow> + '_ {
    let rel = r.argument.rel_path.clone().unwrap_or_default();
    r.claims.iter().map(move |c| ClaimRow {
        id: c.id.clone(),
        rel_path: rel.clone(),
        status: status_label(&c.status).to_string(),
        in_registry: true,
        in_output: !c.status.clone().suppresses_output(),
    })
}

fn status_label(s: &ClaimStatus) -> &'static str {
    match s {
        ClaimStatus::Supported => "supported",
        ClaimStatus::Contradicted => "contradicted",
        ClaimStatus::Unciteable => "unciteable",
        ClaimStatus::Retracted { .. } => "retracted",
    }
}

/// Un argomento di comodo per un file che non è entrato.
///
/// Non finisce da nessuna parte: esiste solo perché il referto ha una tabella
/// `items` con una riga per file, e una riga senza argomento non può dire
/// l'id che il banco cerca. `stored: false` è la parte che conta, ed è
/// l'unica che [`crate::route::receive`] garantisce: nessun file rifiutato
/// scrive. Il `content_hash` vuoto è la prova che non è stato scritto.
fn rifiuto(by: &kbs_core::PersonId, rel: &str) -> kbs_core::Argument {
    kbs_core::Argument {
        id: kbs_core::ArgumentId::from_rel_path(rel),
        title: rel.to_string(),
        summary: String::new(),
        state: kbs_core::PublicationState::Bozza,
        course: kbs_core::CourseId("course_rifiutato".into()),
        prerequisites: Vec::new(),
        origin: kbs_core::Origin::Human { by: by.clone(), at: Millis(0) },
        rel_path: Some(rel.to_string()),
        content_hash: String::new(),
        created_at: Millis(0),
        updated_at: Millis(0),
        ratified: None,
    }
}
pub fn html_files(radice: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let mut stack = vec![radice.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|source| Error::Io { path: dir.clone(), source })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io { path: dir.clone(), source })?;
            let path = entry.path();
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') {
                continue;
            }
            let ft = std::fs::symlink_metadata(&path)
                .map_err(|source| Error::Io { path: path.clone(), source })?;
            if ft.file_type().is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !name.ends_with(".html") {
                continue;
            }
            let rel = path
                .strip_prefix(radice)
                .map_err(|_| Error::Io { path: path.clone(), source: std::io::Error::other("fuori dalla radice") })?;
            let rel = rel.to_str().map(|r| r.replace('\\', "/")).ok_or_else(|| Error::PercorsoNonUtf8 {
                path: path.display().to_string(),
            })?;
            out.push((rel.to_string(), path));
        }
    }
    Ok(out)
}
