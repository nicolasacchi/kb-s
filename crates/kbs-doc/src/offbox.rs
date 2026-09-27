//! D15: **three.js è vendorizzato, mai da CDN** — e la regola è del validatore,
//! non una convenzione.
//!
//! # Perché è un errore e non uno stile
//!
//! Un `<script src="https://cdn…">` non è un dettaglio: è una rotta dati che
//! esce dal perimetro, un punto di fallimento a rialbero, e rende
//! l'artifact **non verificabile** (D6 — non si può dire da dove viene quegli
//! byte) e **non riproducibile** (D11 — domani il file è diverso, e nessuno se
//! ne accorge). Per questo la violazione è un errore che impedisce la
//! pubblicazione e non un avviso.
//!
//! # Che cosa è «fuori dalla scatella»
//!
//! * `https://…`, `http://…`, `//host/…` — un'altra origine.
//! * **qualsiasi schema diverso da `data:`**, compreso `javascript:` e `blob:`.
//!   `javascript:` non esce dalla scatola, ma è comunque una rotta che il
//!   validatore non può rendere verificabile né riproducibile, e la regola
//!   «uno schema è un codice, e i codici non sono contenuto» la copre senza
//!   dover elencare i codici pericolosi uno per uno.
//! * `data:` **no**: è contenuto inline, resta sulla scatola, e ci mettere
//!   dentro anche la propria figura è esattamente ciò che [`crate::build`]
//!   fa.
//!
//! # Che cosa *non* è fuori dalla scatella, e va detto
//!
//! Un collegamento ipertestuale in prosa (`<a href="https://…”>`) è **consentito**
//! e non è un errore. Vietare i link esterni significherebbe vietare allo
//! studente di leggere la fonte; la regola riguarda ciò che l'artifact
//! **carica**, non ciò che l'artifact **cita**. È una distinzione che un validatore
//! che desse il same-listing a tutti gli `href` sbaglierebbe, quindi ha un test
//! suo (`un_link_esterno_in_prosa_e_permesso`).
//!
//! Il runtime vendorizzato è servito da `kbs-server` da `vendor/three/` al
//! percorso [`THREE_RUNTIME`]; il percorso è dichiarato qui perché è il
//! validatore a decidere che quel percorso è dentro il perimetro.

use crate::scan::{self, TagKind};
use serde::{Deserialize, Serialize};

/// Prefisso servito dal binario di `kb-s` per il runtime vendorizzato.
pub const VENDOR_PREFIX: &str = "/three/";

/// Il percorso del runtime three.js vendorizzato, minimizzato (D15).
pub const THREE_RUNTIME: &str = "/three/three.module.min.js";

/// Dove sta un riferimento, e sotto quale tipo di tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceKind {
    ScriptSrc,
    LinkHref,
    ImageSrc,
    /// `srcset` di `<img>`: la stessa immagine, quattro bypass in un attributo.
    ImageSrcset,
    /// `<source>`, `<video>`, `<audio>`, `<track>`, `<embed>`.
    MediaSrc,
    /// `<iframe src>`, `<object data>`.
    FrameSrc,
    /// `@import` in un foglio di stile, dentro o fuori da `<style>`.
    CssImport,
    /// `url(…)` in un foglio di stile.
    CssUrl,
    /// `url(…)` in un attributo `style=`.
    StyleAttrUrl,
}

impl ReferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReferenceKind::ScriptSrc => "script src",
            ReferenceKind::LinkHref => "link href",
            ReferenceKind::ImageSrc => "img src",
            ReferenceKind::ImageSrcset => "img srcset",
            ReferenceKind::MediaSrc => "media src",
            ReferenceKind::FrameSrc => "frame src",
            ReferenceKind::CssImport => "@import",
            ReferenceKind::CssUrl => "url()",
            ReferenceKind::StyleAttrUrl => "url() in style=",
        }
    }
}

/// Se un riferimento resta dentro il perimetro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// Un'altra origine, o uno schema. **Non pubblicabile.**
    External,
    /// Percorso relativo o radice-relativo: resta sulla scatola.
    SameOrigin,
    /// `data:`: contenuto inline.
    Inline,
    /// Solo un frammento: nessun caricamento.
    Fragment,
}

/// Un riferimento trovato nel sorgente, con la riga in cui sta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub url: String,
    pub origin: Origin,
    /// 1-based.
    pub line: usize,
}

/// Classifica un URL. La regola è sullo **schema**, non sul dominio: un
/// `https://` verso un host che si controlla è comunque fuori dalla scatella,
/// perché «lo controlliamo noi» è vero solo finché lo controlliamo.
pub fn classify(url: &str) -> Origin {
    let u = url.trim();
    if u.is_empty() {
        return Origin::Fragment;
    }
    if u.starts_with('#') {
        return Origin::Fragment;
    }
    let lower = u.to_ascii_lowercase();
    if lower.starts_with("data:") {
        return Origin::Inline;
    }
    if u.starts_with("//") {
        // Protocol-relative: eredita lo schema della pagina, che è http in
        // sviluppo e https in produzione. Fuori dalla scatella in entrambi i casi.
        return Origin::External;
    }
    // Uno schema è lettera, poi lettera/digit/`+`/`-`/`.`, poi `:` — e deve
    // arrivare prima di qualsiasi `/`, `?` o `#`, altrimenti è un percorso con
    // due punti nel nome.
    if let Some(colon) = u.find(':') {
        let candidate = &u[..colon];
        let looks_like_scheme = !candidate.is_empty()
            && candidate.starts_with(|c: char| c.is_ascii_alphabetic())
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
        let before_a_path = !u[..colon].contains(['/', '?', '#']);
        if looks_like_scheme && before_a_path {
            return Origin::External;
        }
    }
    Origin::SameOrigin
}

/// Tutti i riferimenti che l'artifact chiede al mondo, con la loro origine.
pub fn references(src: &str) -> Vec<Reference> {
    let mut out: Vec<Reference> = Vec::new();
    for tag in scan::tags(src) {
        match tag.kind {
            TagKind::Start | TagKind::SelfClosing => {}
            _ => continue,
        }
        let mut push = |kind: ReferenceKind, url: &str, line: usize| {
            let url = url.trim();
            if url.is_empty() {
                return;
            }
            let origin = classify(url);
            if origin == Origin::Fragment {
                return;
            }
            out.push(Reference { kind, url: url.to_string(), origin, line });
        };

        match tag.name.as_str() {
            "script" => {
                if let Some(v) = tag.attr("src") {
                    push(ReferenceKind::ScriptSrc, v, tag.line);
                }
            }
            "link" => {
                if let Some(v) = tag.attr("href") {
                    push(ReferenceKind::LinkHref, v, tag.line);
                }
            }
            "img" => {
                if let Some(v) = tag.attr("src") {
                    push(ReferenceKind::ImageSrc, v, tag.line);
                }
                if let Some(v) = tag.attr("srcset") {
                    for candidate in srcset_candidates(v) {
                        push(ReferenceKind::ImageSrcset, &candidate, tag.line);
                    }
                }
            }
            "source" | "video" | "audio" | "track" | "embed" => {
                if let Some(v) = tag.attr("src") {
                    push(ReferenceKind::MediaSrc, v, tag.line);
                }
            }
            "iframe" => {
                if let Some(v) = tag.attr("src") {
                    push(ReferenceKind::FrameSrc, v, tag.line);
                }
            }
            "object" => {
                if let Some(v) = tag.attr("data") {
                    push(ReferenceKind::FrameSrc, v, tag.line);
                }
            }
            "style" => {
                if let Some(css) = tag.content_of(src) {
                    out.extend(css_references(css, tag.line, ReferenceKind::CssUrl));
                }
            }
            _ => {}
        }

        // Il `kind` è passato perché `url(…)` dentro l'attributo `style` è un
        // caso diverso dal `url(…)` di un foglio: chi corregge l'errore cerca
        // l'attributo, non il foglio. Senza questo parametro la variante
        // esisteva ma non era raggiungibile — e un test che la chiedeva per
        // nome era l'unico modo di accorgersene.
        if let Some(css) = tag.attr("style") {
            out.extend(css_references(css, tag.line, ReferenceKind::StyleAttrUrl));
        }
    }
    out
}

/// Solo quelli che escono dalla scatella.
pub fn external(src: &str) -> Vec<Reference> {
    references(src)
        .into_iter()
        .filter(|r| r.origin == Origin::External)
        .collect()
}

/// `srcset` è `url [descrittore], url [descrittore]`: si prende il primo token di
/// ogni candidato e si butta il resto.
fn srcset_candidates(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|c| c.split_whitespace().next())
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect()
}

/// `@import` e `url(…)` dentro un pezzo di CSS.
///
/// Gli `@import` vengono contati una volta: un `@import url(…)` produce un
/// solo errore, non due, perché il lettore che lo corregge ha una cosa sola da
/// correggere.
fn css_references(css: &str, line: usize, url_kind: ReferenceKind) -> Vec<Reference> {
    let lower = css.to_ascii_lowercase();
    let mut out: Vec<Reference> = Vec::new();
    let mut import_spans: Vec<(usize, usize)> = Vec::new();

    let mut at = 0usize;
    while let Some(p) = lower[at..].find("@import") {
        let start = at + p;
        // Fino a `;`, a fine riga, o a `)`.
        let rest = &css[start..];
        let end = rest
            .find(|c: char| c == ';' || c == '\n')
            .unwrap_or(rest.len());
        import_spans.push((start, start + end));
        if let Some(url) = extract_url(&css[start..start + end]) {
            out.push(Reference {
                kind: ReferenceKind::CssImport,
                url,
                origin: Origin::SameOrigin, // riclassificato sotto
                line,
            });
        } else if let Some(q) = quoted_after_keyword(&css[start..start + end]) {
            out.push(Reference {
                kind: ReferenceKind::CssImport,
                url: q,
                origin: Origin::SameOrigin,
                line,
            });
        }
        at = start + "@import".len();
    }

    let mut u = 0usize;
    while let Some(p) = lower[u..].find("url(") {
        let start = u + p;
        let end = css[start..]
            .find(')')
            .map(|q| start + q + 1)
            .unwrap_or(css.len());
        let inside_import = import_spans
            .iter()
            .any(|(a, b)| start >= *a && start < *b);
        if !inside_import {
            if let Some(url) = extract_url(&css[start..end]) {
                out.push(Reference {
                    kind: url_kind,
                    url,
                    origin: Origin::SameOrigin, // riclassificato sotto
                    line,
                });
            }
        }
        u = start + "url(".len();
    }

    out.into_iter()
        .map(|mut r| {
            r.origin = classify(&r.url);
            r
        })
        .filter(|r| r.origin != Origin::Fragment)
        .collect()
}

/// `url(…)` con o senza apici.
fn extract_url(s: &str) -> Option<String> {
    let start = s.find("url(")? + 4;
    let inner = s[start..].trim_start();
    let inner = inner.strip_suffix(')')?;
    let inner = inner.trim();
    let unquoted = if inner.len() >= 2
        && ((inner.starts_with('"') && inner.ends_with('"'))
            || (inner.starts_with('\'') && inner.ends_with('\'')))
    {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

/// `@import "foglio.css";` — la forma senza `url()`.
fn quoted_after_keyword(s: &str) -> Option<String> {
    let after = s.find("@import")? + "@import".len();
    let rest = s[after..].trim_start();
    let rest = rest.strip_prefix(['"', '\''])?;
    let end = rest.find(['"', '\''])?;
    if end == 0 {
        None
    } else {
        Some(rest[..end].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn classificazione_degli_origini() {
        assert_eq!(classify("https://cdn.jsdelivr.net/npm/three"), Origin::External);
        assert_eq!(classify("http://x/y.js"), Origin::External);
        assert_eq!(classify("//cdn.example.com/x.js"), Origin::External);
        assert_eq!(classify("javascript:alert(1)"), Origin::External);
        assert_eq!(classify("blob:https://x/1"), Origin::External);
        assert_eq!(classify("data:image/png;base64,AAA"), Origin::Inline);
        assert_eq!(classify("/three/three.module.min.js"), Origin::SameOrigin);
        assert_eq!(classify("style.css"), Origin::SameOrigin);
        assert_eq!(classify("./fig/a.png"), Origin::SameOrigin);
        assert_eq!(classify("../fig/a.png"), Origin::SameOrigin);
        assert_eq!(classify("#esempio"), Origin::Fragment);
        // Due punti nel nome di un percorso non sono uno schema.
        assert_eq!(classify("capitoli/3.2:esercizi.html"), Origin::SameOrigin);
    }

    #[test]
    fn nessun_riferimento_esterno_in_un_artifact_puro() {
        let src = r#"<html><head><title>t</title></head><body>
            <link rel="stylesheet" href="style.css">
            <img src="fig/a.png" alt="a">
            <script src="app.js"></script>
        </body></html>"#;
        assert!(external(src).is_empty(), "{:?}", references(src));
    }

    #[test]
    fn ciascun_vettore_e_coperto() {
        let casi: &[(&str, ReferenceKind)] = &[
            ("<script src=\"https://cdn.example.com/three.js\"></script>", ReferenceKind::ScriptSrc),
            ("<link rel=\"stylesheet\" href=\"https://cdn.example.com/a.css\">", ReferenceKind::LinkHref),
            ("<img src=\"https://cdn.example.com/a.png\">", ReferenceKind::ImageSrc),
            ("<iframe src=\"https://example.com/x\"></iframe>", ReferenceKind::FrameSrc),
            ("<object data=\"https://example.com/x\"></object>", ReferenceKind::FrameSrc),
            ("<style>@import url(https://cdn.example.com/a.css);</style>", ReferenceKind::CssImport),
            ("<style>@import \"https://cdn.example.com/a.css\";</style>", ReferenceKind::CssImport),
            ("<style>body{background:url(https://cdn.example.com/bg.png)}</style>", ReferenceKind::CssUrl),
            ("<p style=\"background:url(https://cdn.example.com/bg.png)\">x</p>", ReferenceKind::StyleAttrUrl),
        ];
        for (src, kind) in casi {
            let trovati = external(src);
            assert!(
                trovati.iter().any(|r| r.kind == *kind && r.origin == Origin::External),
                "{src} non ha prodotto {kind:?}, ha prodotto {trovati:?}"
            );
        }
    }

    #[test]
    fn srcset_e_una_quarta_via() {
        let src = "<img srcset=\"https://cdn.example.com/a.png 1x, b.png 2x\">";
        assert_eq!(external(src).len(), 1);
        assert_eq!(external(src)[0].kind, ReferenceKind::ImageSrcset);
    }

    #[test]
    fn import_url_e_conta_una_volta_sola() {
        let src = "<style>@import url(\"https://cdn.example.com/a.css\");</style>";
        assert_eq!(external(src).len(), 1);
        assert_eq!(external(src)[0].kind, ReferenceKind::CssImport);
    }

    #[test]
    fn un_riferimento_esterno_dentro_lo_script_non_e_un_riferimento() {
        // Il contenuto di uno `<script>` è JavaScript: una stringa `https://`
        // lì dentro è un valore, non un caricamento. Lo scanner non lo riscatena.
        let src = "<script>const url = \"https://api.example.com/dati\"; fetch(url);</script>";
        assert!(external(src).is_empty());
    }

    #[test]
    fn un_link_esterno_in_prosa_e_permesso() {
        let src = "<p>Come scrive <a href=\"https://it.wikipedia.org/wiki/Tesi\">la tesi</a>.</p>";
        assert!(external(src).is_empty(), "{:?}", references(src));
    }

    #[test]
    fn il_runtime_vendorizzato_e_dentro_il_perimetro() {
        let src = format!("<script type=\"module\" src=\"{THREE_RUNTIME}\"></script>");
        assert!(external(&src).is_empty());
        assert!(classify(THREE_RUNTIME) == Origin::SameOrigin);
        assert!(THREE_RUNTIME.starts_with(VENDOR_PREFIX));
    }

    #[test]
    fn la_riga_del_riferimento_e_giusta() {
        let src = "<html>\n<body>\n\n<script src=\"https://cdn.example.com/a.js\"></script>\n</body>\n</html>";
        assert_eq!(external(src)[0].line, 4);
    }

    #[test]
    fn nessun_panic_su_documenti_malformati() {
        for src in [
            "",
            "<script src=",
            "<script src=\"https://cdn.example.com/a.js\"",
            "<style>@import url(",
            "<img srcset=",
            "<",
        ] {
            let _ = external(src);
        }
    }
}
