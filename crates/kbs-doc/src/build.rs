//! La build: l'artefatto spedito.
//!
//! # Cosa fa
//!
//! Inline il foglio di stile e gli script propri dell'artifact, trasforma le
//! figure locali in `data:` URI, e **non** tocca il runtime three.js: quello è
//! vendorizzato nel repository e servito da `kbs-server` al percorso
//! [`offbox::THREE_RUNTIME`] con `Cache-Control` lungo ed `ETag` (D15). Inline
//! di 692 KB di JavaScript in ogni artifact che ha una scena sarebbe il
//! contrario di quello che D15 chiede.
//!
//! # Il budget: perché è una voce e non una nota
//!
//! Un artifact da 40 KB che carica 171 KB di runtime non è un artifact da
//! 40 KB, e dirlo è il primo atto di onestà di D15. Quindi [`Budget`] tiene
//! **separati** due conti:
//!
//! * [`Budget::artifact_text_bytes`] — i byte del file. È ciò che l'ho scritto.
//! * [`Budget::declared_runtime_bytes`] — i byte che il lettore scarica *oltre* a
//!   quelli, per ogni runtime referenziato per percorso.
//!
//! e ogni voce di [`Budget::lines`] dichiara in quale dei due conti entra. Un
//! peso che non si sa dove sia contato non è un peso che si è contato.
//!
//! # Sulla fedeltà al sorgente
//!
//! La build fa chirurgia di stringhe, non ri-serializzazione: il file prodotto è
//! il sorgente con dei pezzi sostituiti, byte per byte identico altrove. Non è
//! una questione di stile — un artifact la cui impaginazione cambia a ogni
//! build non ha un `content_hash` stabile, e senza un hash stabile non c'è né
//! ratifica (D4) né replay (D11).

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::error::DocError;
use crate::offbox::{self, Origin};
use crate::scan;
use crate::validate::{self, ArtifactReport};

/// Tetto di default sul runtime **trasferito** (gzip) che un artifact può
/// dichiarare.
///
/// La classe non ha rete garantita, ma ha una rete lenta e una memoria
/// scolastica: 256 KiB compressi è il punto in cui un artifact con scena 3D
/// smette di essere un materiale di studio e diventa un problema. Il tetto è un
/// **avviso**, non un errore: la decisione di un docente di insegnare con una
/// scena pesante è legittima, e il sistema deve dirglielo, non impedirglielo.
pub const DEFAULT_RUNTIME_BUDGET_GZIP: usize = 262_144;

/// Se la build può produrre un artifact che non si pubblica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Rifiuta. È il modo del percorso di pubblicazione (D4).
    Strict,
    /// Produce il file e riporta gli issue. Serve a guardare un artifact in
    /// lavorazione senza fingere che sia a posto.
    Lenient,
}

/// Dove la build legge i file locali.
///
/// Un resolver è un'interfaccia e non `std::fs` perché il percorso di intake
/// può avere i sorgenti in un archivio, in un/git tree, o in un test.
pub trait Resolver {
    /// I byte di un percorso relativo all'artifact, o un errore che dice perché
    /// non ci sono.
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;
}

/// Un resolver che legge dal filesystem, rifiutando ogni percorso che esce
/// dalla radice.
///
/// Il rifiuto non è paranoia da parser: `<img src="../../../etc/passwd">` in un
/// artifact — o in un `<link href>` che la build inlines — è una lettura di file
/// arbitrari causata da un file di contenuto, e la build è il posto in cui
/// quella lettura diventa reale.
#[derive(Debug, Clone)]
pub struct FileResolver {
    root: PathBuf,
}

impl FileResolver {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FileResolver { root: root.into() }
    }

    /// Il percorso assoluto corrispondente, se è dentro la radice.
    ///
    /// Non richiede che il file esista: [`FileResolver::read`] è il posto in cui
    /// l'esistenza conta, e lì il controllo passa da `..` a `canonicalize`, che
    /// è l'unico che vede attraverso un symlink.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, DocError> {
        let p = Path::new(path);
        if p.is_absolute() {
            // Un percorso assoluto nella build è un percorso del server
            // (`/three/…`), non un file: si rimuove la radice e si prova sotto
            // di essa, perché è lì che `kbs-server` lo serve.
            return self.resolve(&strip_root(p).to_string_lossy());
        }
        for c in p.components() {
            if matches!(c, Component::ParentDir) {
                return Err(DocError::PathEscape { path: path.to_string() });
            }
        }
        Ok(self.root.join(p))
    }

    /// Il file, se esiste e se è dentro la radice **anche risolvendo i symlink**.
    pub fn resolve_existing(&self, path: &str) -> Result<PathBuf, DocError> {
        let full = self.resolve(path)?;
        let canon_root = self
            .root
            .canonicalize()
            .map_err(|e| DocError::Unresolvable { path: path.to_string(), motivo: e.to_string() })?;
        let canon = full
            .canonicalize()
            .map_err(|e| DocError::Unresolvable { path: path.to_string(), motivo: e.to_string() })?;
        if canon.starts_with(&canon_root) {
            Ok(canon)
        } else {
            Err(DocError::PathEscape { path: path.to_string() })
        }
    }
}

impl Resolver for FileResolver {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let full = self.resolve_existing(path).map_err(|e| e.to_string())?;
        std::fs::read(&full).map_err(|e| format!("{}: {e}", full.display()))
    }
}

/// `/three/three.module.min.js` → `three/three.module.min.js`.
fn strip_root(p: &Path) -> PathBuf {
    p.strip_prefix("/").unwrap_or(p).to_path_buf()
}

/// Come la build è configurata.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Dove sta `vendor/`. Se c'è, il peso del runtime vendorizzato viene
    /// misurato sui byte reali; se non c'è, il runtime referenziato resta
    /// dichiarato ma non pesato, e il budget dice che non è stato pesato.
    pub vendor_dir: Option<PathBuf>,
    /// Tetto sul runtime trasferito, in byte gzip.
    pub runtime_budget_gzip: usize,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions { vendor_dir: None, runtime_budget_gzip: DEFAULT_RUNTIME_BUDGET_GZIP }
    }
}

impl BuildOptions {
    /// Le opzioni con `vendor/`, cioè la configurazione del repository.
    pub fn for_repo(vendor_dir: impl Into<PathBuf>) -> Self {
        BuildOptions {
            vendor_dir: Some(vendor_dir.into()),
            ..BuildOptions::default()
        }
    }
}

/// In quale conto entra una voce di budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CountedIn {
    /// Nel testo dell'artifact: lo contiene e lo trasporta.
    ArtifactText,
    /// Fuori dal testo: dichiarato a parte, scaricato a parte.
    DeclaredRuntime,
    /// Fuori da ogni conto: un riferimento che la build non ha potuto portare
    /// dentro, e che resta un buco dichiarato.
    Unresolved,
}

/// Una riga di budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetLine {
    pub label: String,
    pub raw_bytes: usize,
    pub gzip_bytes: usize,
    pub counted_in: CountedIn,
    /// `Some` quando il peso è **stato misurato** sui byte reali (il file, o il
    /// suo `.gz` accanto), `None` quando è solo dichiarato.
    pub measured: bool,
}

/// Il budget di un artifact, con i due conti tenuti separati.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Byte del file prodotto.
    pub artifact_bytes: usize,
    pub lines: Vec<BudgetLine>,
    pub runtime_budget_gzip: usize,
}

impl Budget {
    /// I byte del testo dell'artifact. **Non** include i runtime dichiarati.
    pub fn artifact_text_bytes(&self) -> usize {
        self.artifact_bytes
    }

    /// I byte di runtime che il lettore scarica oltre al testo.
    pub fn declared_runtime_bytes(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.counted_in == CountedIn::DeclaredRuntime)
            .map(|l| l.gzip_bytes)
            .sum()
    }

    /// I byte grezzi di runtime dichiarati: il numero che spaventa e che va
    /// detto per intero, non solo nella sua versione compressa.
    pub fn declared_runtime_raw_bytes(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.counted_in == CountedIn::DeclaredRuntime)
            .map(|l| l.raw_bytes)
            .sum()
    }

    /// Riferimenti che la build non ha potuto portare dentro il file: il buco
    /// è dichiarato, non nascosto.
    pub fn unresolved(&self) -> Vec<&BudgetLine> {
        self.lines
            .iter()
            .filter(|l| l.counted_in == CountedIn::Unresolved)
            .collect()
    }

    /// I runtime che sforano il tetto dichiarato.
    pub fn over_runtime_budget(&self) -> Vec<&BudgetLine> {
        self.lines
            .iter()
            .filter(|l| l.counted_in == CountedIn::DeclaredRuntime && l.gzip_bytes > self.runtime_budget_gzip)
            .collect()
    }

    /// Il totale che il lettore scarica davvero: testo più runtime.
    pub fn transferred_bytes(&self) -> usize {
        self.artifact_text_bytes() + self.declared_runtime_bytes()
    }

    /// Le righe in forma di testo: è quello che la build stampa.
    pub fn render(&self) -> String {
        let mut out = format!("artifact: {} B\n", self.artifact_text_bytes());
        for l in &self.lines {
            let conto = match l.counted_in {
                CountedIn::ArtifactText => "nel testo",
                CountedIn::DeclaredRuntime => "dichiarato a parte",
                CountedIn::Unresolved => "NON risolto",
            };
            let misurato = if l.measured { "" } else { " (non misurato)" };
            out.push_str(&format!(
                "  {:<28} {:>9} B grezzi {:>9} B gzip  [{}]{}\n",
                l.label, l.raw_bytes, l.gzip_bytes, conto, misurato
            ));
        }
        if !self.over_runtime_budget().is_empty() {
            out.push_str(&format!(
                "  ATTENZIONE: runtime oltre il tetto di {} B gzip\n",
                self.runtime_budget_gzip
            ));
        }
        out
    }
}

/// L'artefatto spedito, con il conto.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub html: String,
    pub budget: Budget,
    pub report: ArtifactReport,
}

/// Produce l'artifact.
///
/// In [`Gate::Strict`] la build **rifiuta** un artifact che non si pubblica: non
/// è una cortesia, è la ragione per cui un gate esiste. Un gate che lascia
/// passare il file e si limita a segnalarlo è un gate decorativo, e D4 dice che
/// un gate decorativo è peggio di nessun gate.
pub fn build(
    src: &str,
    opts: &BuildOptions,
    resolver: &dyn Resolver,
    gate: Gate,
) -> Result<BuildOutput, DocError> {
    let report = validate::inspect(src);
    if gate == Gate::Strict && !report.can_publish() {
        let motivi: Vec<String> = report
            .blocking()
            .map(|i| format!("  - {}", i.message))
            .collect();
        return Err(DocError::Refused { motivo: motivi.join("\n") });
    }

    let mut lines: Vec<BudgetLine> = Vec::new();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();

    for tag in scan::tags(src) {
        if !tag.kind.is_start_like() {
            continue;
        }
        match tag.name.as_str() {
            "link" if is_stylesheet(&tag) => {
                let Some(href) = tag.attr("href") else { continue };
                if offbox::classify(href) != Origin::SameOrigin {
                    continue; // lontano: se è arrivato fin qui, `Gate::Lenient`.
                }
                let bytes = resolver
                    .read(href)
                    .map_err(|motivo| DocError::Unresolvable { path: href.to_string(), motivo })?;
                let css = escape_raw_text(&String::from_utf8_lossy(&bytes), "style");
                lines.push(weight(href, &bytes, CountedIn::ArtifactText));
                edits.push((
                    tag.range.clone(),
                    format!("<style data-inlined-from=\"{href}\">\n{css}\n</style>"),
                ));
            }
            "script" => {
                let Some(src_attr) = tag.attr("src") else { continue };
                if offbox::classify(src_attr) != Origin::SameOrigin {
                    continue;
                }
                let is_module = tag
                    .attr("type")
                    .is_some_and(|t| t.eq_ignore_ascii_case("module"));
                if is_module && src_attr.starts_with(offbox::VENDOR_PREFIX) {
                    // Il runtime vendorizzato: referenziato per percorso, non
                    // inline. Il suo peso è dichiarato, non incluso nel testo.
                    if let Some(l) = vendor_weight(opts, src_attr)? {
                        lines.push(l);
                    }
                    continue;
                }
                let bytes = resolver
                    .read(src_attr)
                    .map_err(|motivo| DocError::Unresolvable { path: src_attr.to_string(), motivo })?;
                let js = escape_raw_text(&String::from_utf8_lossy(&bytes), "script");
                lines.push(weight(src_attr, &bytes, CountedIn::ArtifactText));
                // Gli altri attributi sopravvivono: togliere `class` o `nonce`
                // a uno script inline significa che il file che si produce non
                // è il file che l'autore ha scritto.
                let attrs = attrs_without(&tag, "src");
                edits.push((
                    tag.range.clone(),
                    format!("<script{attrs} data-inlined-from=\"{src_attr}\">{js}"),
                ));
            }
            "img" => {
                let Some(src_attr) = tag.attr("src") else { continue };
                if offbox::classify(src_attr) != Origin::SameOrigin {
                    continue;
                }
                let bytes = resolver.read(src_attr).map_err(|motivo| DocError::Unresolvable {
                    path: src_attr.to_string(),
                    motivo,
                })?;
                let Some(mime) = image_mime(src_attr) else {
                    lines.push(BudgetLine {
                        label: src_attr.to_string(),
                        raw_bytes: bytes.len(),
                        gzip_bytes: gzip_len(&bytes),
                        counted_in: CountedIn::Unresolved,
                        measured: true,
                    });
                    continue;
                };
                let data = data_uri(&mime, &bytes);
                lines.push(weight(src_attr, &bytes, CountedIn::ArtifactText));
                let attrs = attrs_without(&tag, "src");
                edits.push((
                    tag.range.clone(),
                    format!("<img{attrs} src=\"{data}\" data-inlined-from=\"{src_attr}\">"),
                ));
            }
            _ => {}
        }
    }

    // Le sostituzioni si applicano dalla fine: gli offset restano validi.
    edits.sort_by_key(|(r, _)| r.start);
    let mut html = src.to_string();
    for (range, replacement) in edits.into_iter().rev() {
        html.replace_range(range, &replacement);
    }

    let report = validate::inspect(&html);
    let mut lines = lines;
    lines.push(BudgetLine {
        label: "testo dell'artifact".to_string(),
        raw_bytes: html.len(),
        gzip_bytes: gzip_len(html.as_bytes()),
        counted_in: CountedIn::ArtifactText,
        measured: true,
    });

    Ok(BuildOutput {
        budget: Budget { artifact_bytes: html.len(), lines, runtime_budget_gzip: opts.runtime_budget_gzip },
        html,
        report,
    })
}

/// Gli attributi del tag tranne `skip`, riserializzati.
///
/// La riserializzazione è fedele per quello che conta — nome e valore, con
/// gli apici messi come li aveva messi l'autore — e non pretende di essere
/// byte-identica: un attributo senza apici ne riacquisisce uno, che è
/// equivalente.
fn attrs_without(tag: &scan::Tag, skip: &str) -> String {
    let mut out = String::new();
    for a in &tag.attrs {
        if a.name_is(skip) {
            continue;
        }
        if a.value.is_empty() {
            out.push(' ');
            out.push_str(&a.name);
            continue;
        }
        out.push(' ');
        out.push_str(&a.name);
        out.push_str("=\"");
        out.push_str(&a.value.replace('&', "&amp;").replace('"', "&quot;"));
        out.push('"');
    }
    out
}

/// Neutralizza un `</script` o un `</style` che chiuderebbero per sbaglio il
/// tag che li contiene.
///
/// Diventa `<\/script`: è un escape valido sia dentro una stringa JavaScript
/// sia dentro una stringa CSS, ed è quello che fanno i bundler. **Il limite,
/// dichiarato:** in JavaScript in modalità `u` un escape di identità dentro una
/// espressione regolare è un errore di sintassi, quindi un sorgente che
/// contiene la sequenza `</script` *in una regex* inlining si rompe. In
/// materiale didattico la sequenza compare in una stringa, e il caso della
/// regex è qui dichiarato perché è l'unico in cui la protezione non regge.
fn escape_raw_text(text: &str, tag: &str) -> String {
    let needle = format!("</{tag}");
    if text.to_ascii_lowercase().contains(&needle) {
        text.replace("</", r"<\/")
    } else {
        text.to_string()
    }
}

fn is_stylesheet(tag: &scan::Tag) -> bool {
    tag.attr("rel")
        .is_some_and(|r| r.split_ascii_whitespace().any(|p| p.eq_ignore_ascii_case("stylesheet")))
}

fn weight(label: &str, bytes: &[u8], counted_in: CountedIn) -> BudgetLine {
    BudgetLine {
        label: label.to_string(),
        raw_bytes: bytes.len(),
        gzip_bytes: gzip_len(bytes),
        counted_in,
        measured: true,
    }
}

/// Il peso di un runtime vendorizzato, misurato sui byte del repository.
///
/// Se accanto al file c'è il `.gz` — cioè se qualcuno ha già deciso che è
/// quello che si serve — il peso trasferito è quello del `.gz`, non una
/// ricompressione fatta qui: due numeri diversi per lo stesso file sono due bug
/// che si sommano. Se il `.gz` non c'è, il peso è quello calcolato qui e la
/// riga lo dice con `measured`.
fn vendor_weight(opts: &BuildOptions, url: &str) -> Result<Option<BudgetLine>, DocError> {
    let Some(vendor) = &opts.vendor_dir else {
        // Nessuna directory vendor: il runtime è dichiarato ma non pesato. Il
        // numero non esiste e la riga lo dice, invece di valutarlo a zero.
        return Ok(Some(BudgetLine {
            label: url.to_string(),
            raw_bytes: 0,
            gzip_bytes: 0,
            counted_in: CountedIn::DeclaredRuntime,
            measured: false,
        }));
    };
    let rel = strip_root(Path::new(url));
    let file = vendor.join(&rel);
    let raw = std::fs::read(&file).map_err(|e| DocError::VendorMissing {
        path: url.to_string(),
        motivo: format!("{}: {e}", file.display()),
    })?;
    let gz = file.with_extension(
        file.extension()
            .and_then(|e| e.to_str())
            .map(|e| format!("{e}.gz"))
            .unwrap_or_else(|| "gz".to_string()),
    );
    let (gzip_bytes, measured) = match std::fs::metadata(&gz) {
        Ok(m) => (m.len() as usize, true),
        Err(_) => (gzip_len(&raw), true),
    };
    Ok(Some(BudgetLine {
        label: url.to_string(),
        raw_bytes: raw.len(),
        gzip_bytes,
        counted_in: CountedIn::DeclaredRuntime,
        measured,
    }))
}

/// I byte gzip di `bytes`, al livello di default.
pub fn gzip_len(bytes: &[u8]) -> usize {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    // `write_all` su un buffer in memoria non può fallire per contenuto; su
    // allocazione può, e in quel caso il numero che riportiamo è il greggio:
    // dichiarare più di quanto si scarica sarebbe una bug verso il basso.
    match enc.write_all(bytes) {
        Ok(()) => enc.finish().map(|v| v.len()).unwrap_or(bytes.len()),
        Err(_) => bytes.len(),
    }
}

fn image_mime(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        // Un'estensione sconosciuta non si indovina: `data:text/html` su una
        // riga immagine è un vettore, non un formato.
        _ => return None,
    })
}

/// `data:` URI. Serve [`base64`](?) no: si implementa qui per non aggiungere una
/// dipendenza a una funzione di trenta righe.
fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Un resolver di test: una mappa, e nessun filesystem.
    struct Map(BTreeMap<String, Vec<u8>>);

    impl Map {
        fn new(pairs: &[(&str, &str)]) -> Self {
            Map(pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                .collect())
        }
    }

    impl Resolver for Map {
        fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: non c'è"))
        }
    }

    /// Un artifact valido con `head_extra` in testa.
    ///
    /// Il foglio di stile **non** è nel modello di base: ogni test che chiama
    /// `build` deve fornire esattamente i file che il proprio sorgente chiede,
    /// perché un resolver che risponde «non c'è» a qualcosa che il test non
    /// intendeva chiedere fa fallire il test per un motivo che non è il suo.
    fn sorgente(head_extra: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html><head><title>T</title>
{head_extra}
<template id="kb-kbprompt">
{contratto}
</template>
</head><body><p>x</p></body></html>"#,
            contratto = crate::testing::contratto_buono()
        )
    }

    /// Il modello di base più un foglio di stile locale.
    fn sorgente_con_stile() -> String {
        sorgente("<link rel=\"stylesheet\" href=\"style.css\">")
    }

    #[test]
    fn il_foglio_di_stile_viene_inline_e_il_documento_resta_un_file() {
        let src = sorgente_con_stile();
        let out = build(
            &src,
            &BuildOptions::default(),
            &Map::new(&[("style.css", "h1{color:red}")]),
            Gate::Strict,
        )
        .expect("build");
        assert!(out.html.contains("<style data-inlined-from=\"style.css\">"));
        assert!(out.html.contains("h1{color:red}"));
        assert!(!out.html.contains("<link rel=\"stylesheet\""));
        // Il resto del sorgente è intatto: nessuna ri-serializzazione.
        assert!(out.html.starts_with("<!DOCTYPE html><html><head><title>T</title>"));
        assert!(out.report.can_publish());
    }

    #[test]
    fn lo_script_proprio_viene_inline() {
        let src = sorgente("<script src=\"app.js\"></script>");
        let out = build(&src, &BuildOptions::default(), &Map::new(&[("app.js", "var a=1;")]), Gate::Strict)
            .expect("build");
        assert!(out.html.contains("var a=1;"));
        assert!(!out.html.contains("src=\"app.js\""));
    }

    #[test]
    fn la_figura_locale_diventa_data_uri() {
        let src = sorgente("").replace("<p>x</p>", "<img src=\"a.png\" alt=\"figura\">");
        let out = build(&src, &BuildOptions::default(), &Map::new(&[("a.png", "PNG")]), Gate::Strict)
            .expect("build");
        assert!(out.html.contains("src=\"data:image/png;base64,"));
        assert!(out.html.contains("alt=\"figura\""));
        // I tre byte `PNG` in base64.
        assert!(out.html.contains("UE5H"));
    }

    #[test]
    fn il_gate_strict_rifiuta_un_artifact_non_pubblicabile() {
        let src = sorgente("<script src=\"https://cdn.example.com/three.js\"></script>");
        let err = build(&src, &BuildOptions::default(), &Map::new(&[]), Gate::Strict)
            .expect_err("la build non deve produrre un artifact che chiama fuori");
        assert!(matches!(err, DocError::Refused { .. }));
        assert!(err.to_string().contains("D15"));
    }

    #[test]
    fn il_gate_lenient_produce_e_dichiara() {
        let src = sorgente("<script src=\"https://cdn.example.com/three.js\"></script>");
        let out = build(&src, &BuildOptions::default(), &Map::new(&[]), Gate::Lenient).expect("build");
        assert!(out.html.contains("cdn.example.com"), "il file resta com'era");
        assert!(!out.report.can_publish());
        assert!(out.report.has_blocking());
    }

    #[test]
    fn un_riferimento_locale_mancante_e_un_errore_nomegggiato() {
        let src = sorgente_con_stile();
        let err = build(&src, &BuildOptions::default(), &Map::new(&[]), Gate::Strict)
            .expect_err("manca style.css");
        match err {
            DocError::Unresolvable { path, .. } => assert_eq!(path, "style.css"),
            altro => panic!("atteso Unresolvable, trovato {altro:?}"),
        }
    }

    #[test]
    fn il_resolver_non_esce_dalla_radice() {
        let r = FileResolver::new(".");
        assert!(r.resolve("../../etc/passwd").is_err());
        assert!(r.resolve("a/../../etc/passwd").is_err());
        assert!(r.resolve("style.css").is_ok());
        // E non esce neppure quando il file non c'è: il percorso si rifiuta prima.
        assert!(r.resolve_existing("../../etc/passwd").is_err());
    }

    #[test]
    fn il_base64_e_standard() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[test]
    fn uno_script_che_contiene_il_tag_di_chiusura_non_rompe_il_documento() {
        let src = sorgente("<script src=\"app.js\"></script>");
        let out = build(
            &src,
            &BuildOptions::default(),
            &Map::new(&[("app.js", "var s = \"</script>\";")]),
            Gate::Strict,
        )
        .expect("build");
        assert!(out.html.contains("<\\/script>"));
        // L'HTML risultante ha esattamente un `<script>` e un `</script>`: quello
        // che lo chiude è quello del tag, non quello dentro la stringa.
        let aperture = out.html.matches("<script").count();
        assert_eq!(aperture, 1, "un solo `<script>`: quello del tag");
        let chiuse = out.html.matches("</script>").count();
        assert_eq!(chiuse, 1, "un solo `</script>`: quello del tag");
    }

    #[test]
    fn gli_attributi_dello_script_inline_sopravvivono() {
        let src = sorgente("<script src=\"app.js\" class=\"x\" defer></script>");
        let out = build(&src, &BuildOptions::default(), &Map::new(&[("app.js", "var a=1;")]), Gate::Strict)
            .expect("build");
        assert!(out.html.contains("class=\"x\""));
        assert!(out.html.contains("defer"));
    }

    #[test]
    fn il_gzip_e_piu_piccolo_del_grezzo_su_un_testo() {
        let t = "a".repeat(10_000);
        let g = gzip_len(t.as_bytes());
        assert!(g > 0 && g < 10_000, "{g}");
    }
}
