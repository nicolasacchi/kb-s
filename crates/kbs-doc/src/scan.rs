//! Scanner di tag HTML **sul testo grezzo**, con offset in byte.
//!
//! `scraper`/`html5ever` danno l'albero, che è la forma giusta per *leggere* un
//! documento. Ma due delle regole di questo crate hanno bisogno di cose che
//! l'albero non dà:
//!
//! * **D15** — l'errore deve dire *dove*, e un offset in byte è l'unico modo di
//!   dire «riga 14, carattere 30» senza ri-serializzare il documento.
//! * **la build** — l'artefatto spedito deve essere il sorgente con dei pezzi
//!   inline, non la ri-serializzazione dell'albero: `Html::html()` riordina gli
//!   attributi e ricostruisce il documento, e un artefatto che non si riproduce
//!   byte per byte non è riproducibile (D11).
//!
//! Quindi qui si legge il sorgente e basta. Lo scanner non pretende di essere un
//! parser HTML: pretende di essere *onesto su ciò che vede*. Le regole che
//! adotta, e che sono tutte scelte documentate perché un limite è un bug che
//! aspetta:
//!
//! * `script`, `style`, `textarea`, `title` sono elementi a testo grezzo: il
//!   contenuto viene restituito in `Tag::content` e non viene riscanato, così un
//!   `<` dentro una stringa JavaScript non viene letto come un tag.
//! * I commenti, il doctype e i processing instruction non sono tag e non
//!   producono attributi.
//! * Un valore di attributo fra apici contiene qualsiasi carattere, `>`
//!   compreso; un valore senza apici finisce allo spazio o al `>`.
//! * Un `/` finale di un attributo senza apici è il segnale di elemento
//!   auto-chiuso, non parte del valore: è quello che fanno i browser, ed è
//!   l'unico modo che `<img src=a.png/>` produca `src="a.png"`.
//! * Un elemento a testo grezzo senza chiusura non è un errore: il contenuto
//!   arriva a fine sorgente e lo scanner si ferma. Non c'è `unwrap()` e non
//!   c'è `panic!` su input malformato, perché qui l'input è materiale
//!   didattico scritto a mano.

use std::ops::Range;

/// Che cosa è la cosa trovata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    Start,
    End,
    /// `<img />`, `<br/>`: lo slash è stato consumato come auto-chiusura.
    SelfClosing,
    Comment,
    Doctype,
}

impl TagKind {
    pub fn is_start_like(self) -> bool {
        matches!(self, TagKind::Start | TagKind::SelfClosing)
    }
}

/// Un attributo. `value` è il valore **decodificato** dal sorgente? No: è il
/// valore grezzo, senza decodificare le entità. Chi classifica un origine (D15)
/// guarda il prefisso, e il prefisso di un attributo con entità non cambia; chi
/// inlines un file usa il percorso relativo, che non contiene entità.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub value: String,
}

impl Attr {
    /// Nome e valore in minuscolo sul nome: gli attributi HTML sono case-insensitive.
    pub fn name_is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

/// Un tag, con la sua posizione esatta nel sorgente.
#[derive(Debug, Clone)]
pub struct Tag {
    /// Byte da `..=end` nel sorgente, `end` escluso. Per `Start`/`End`/
    /// `SelfClosing`/`Comment`/`Doctype` è il tag stesso; per un elemento a
    /// testo grezzo è il tag d'apertura.
    pub range: Range<usize>,
    pub kind: TagKind,
    /// Nome elemento in minuscolo. Stringa vuota per commenti e doctype.
    pub name: String,
    pub attrs: Vec<Attr>,
    /// Per `script`/`style`/`textarea`/`title`: l'intervallo del testo grezzo.
    pub content: Option<Range<usize>>,
    /// 1-based. Vale per `range.start`.
    pub line: usize,
}

impl Tag {
    pub fn is(&self, name: &str) -> bool {
        self.name == name
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name_is(name))
            .map(|a| a.value.as_str())
    }

    /// Il testo grezzo dell'elemento, per chi sa che esiste.
    pub fn content_of<'a>(&self, src: &'a str) -> Option<&'a str> {
        self.content.as_ref().map(|r| &src[r.clone()])
    }
}

/// Elementi il cui contenuto è testo grezzo, non markup.
const RAW_TEXT: [&str; 4] = ["script", "style", "textarea", "title"];

/// Tutti i tag del sorgente, in ordine di posizione.
pub fn tags(src: &str) -> Vec<Tag> {
    let mut out: Vec<Tag> = Vec::new();
    let line_of = LineIndex::new(src);
    let bytes = src.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        let Some(rel) = src[i..].find('<') else { break };
        let lt = i + rel;

        // `<!--` prima di tutto: il resto di un commento è rumore, non markup.
        if src[lt..].starts_with("<!--") {
            // `p` è l'offset di `-->` dentro la coda che comincia a `lt + 4`:
            // la lunghezza del commento è i 4 di `<!--`, il trovato e i 3 di `-->`.
            let end = src[lt + 4..]
                .find("-->")
                .map(|p| p + 4 + 3)
                .unwrap_or(src.len() - lt);
            out.push(Tag {
                range: lt..(lt + end).min(src.len()),
                kind: TagKind::Comment,
                name: String::new(),
                attrs: Vec::new(),
                content: None,
                line: line_of.line(lt),
            });
            i = (lt + end).min(src.len());
            continue;
        }

        // `<!DOCTYPE …>`, `<?xml …>`: non sono tag.
        if src[lt..].starts_with("<!") || src[lt..].starts_with("<?") {
            let end = src[lt..].find('>').map(|p| p + 1).unwrap_or(src.len() - lt);
            out.push(Tag {
                range: lt..(lt + end).min(src.len()),
                kind: TagKind::Doctype,
                name: String::new(),
                attrs: Vec::new(),
                content: None,
                line: line_of.line(lt),
            });
            i = (lt + end).min(src.len());
            continue;
        }

        if src[lt..].starts_with("</") {
            let name_end = src[lt + 2..]
                .find(|c: char| !is_name_char(c))
                .map(|p| lt + 2 + p)
                .unwrap_or(src.len());
            let gt = src[name_end..].find('>').map(|p| name_end + p + 1).unwrap_or(src.len());
            out.push(Tag {
                range: lt..gt,
                kind: TagKind::End,
                name: src[lt + 2..name_end].to_ascii_lowercase(),
                attrs: Vec::new(),
                content: None,
                line: line_of.line(lt),
            });
            i = gt;
            continue;
        }

        // `<<` e `< 3` non sono tag: lo scanner prosegue senza fermarsi.
        let after = src[lt + 1..].chars().next();
        if !after.is_some_and(|c| c.is_ascii_alphabetic()) {
            i = lt + 1;
            continue;
        }

        let Some((tag, next)) = parse_start_tag(src, lt, line_of.line(lt)) else {
            i = lt + 1;
            continue;
        };
        let name = tag.name.clone();
        let open_end = tag.range.end;
        let self_closing = tag.kind == TagKind::SelfClosing;
        let is_raw = RAW_TEXT.contains(&name.as_str()) && !self_closing;
        out.push(tag);

        if is_raw {
            // Contenuto grezzo fino al tag di chiusura corrispondente; se il
            // documento è troncato, fino alla fine. Nessun panico, nessun errore.
            let close = find_close_tag(src, open_end, &name);
            let (content, resume, close_range, close_line) = match &close {
                Some(end) => (
                    open_end..end.range.start,
                    end.range.end,
                    Some(end.range.clone()),
                    line_of.line(end.range.start),
                ),
                None => (open_end..src.len(), src.len(), None, 0),
            };
            if let Some(last) = out.last_mut() {
                last.content = Some(content);
            }
            if let Some(ct) = close_range {
                out.push(Tag {
                    range: ct,
                    kind: TagKind::End,
                    name,
                    attrs: Vec::new(),
                    content: None,
                    line: close_line,
                });
            }
            i = resume;
            continue;
        }

        i = next;
    }

    out
}

/// Il tag di chiusura corrispondente, se c'è.
fn find_close_tag(src: &str, from: usize, name: &str) -> Option<Tag> {
    let mut i = from;
    while i < src.len() {
        let Some(rel) = src[i..].find("</") else { return None };
        let at = i + rel;
        let name_end = src[at + 2..]
            .find(|c: char| !is_name_char(c))
            .map(|p| at + 2 + p)
            .unwrap_or(src.len());
        if src[at + 2..name_end].eq_ignore_ascii_case(name) {
            let gt = src[name_end..].find('>').map(|p| name_end + p + 1).unwrap_or(src.len());
            return Some(Tag {
                range: at..gt,
                kind: TagKind::End,
                name: name.to_string(),
                attrs: Vec::new(),
                content: None,
                line: 0,
            });
        }
        i = at + 2;
    }
    None
}

/// Un tag di apertura: nome, attributi, auto-chiusura, e dove finisce.
fn parse_start_tag(src: &str, start: usize, line: usize) -> Option<(Tag, usize)> {
    let bytes = src.as_bytes();
    let mut p = start + 1;
    while p < bytes.len() && is_name_char(src[p..].chars().next().unwrap_or(' ')) {
        p += src[p..].chars().next().map(char::len_utf8).unwrap_or(1);
    }
    let name = src[start + 1..p].to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }

    let mut attrs: Vec<Attr> = Vec::new();
    let mut self_closing = false;

    loop {
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= bytes.len() {
            // Tag non chiuso: si chiude a fine sorgente. Nessun errore fatale:
            // un artifact malformato deve produrre un errore che lo nomina,
            // non un panic.
            return Some((
                Tag {
                    range: start..src.len(),
                    kind: TagKind::Start,
                    name,
                    attrs,
                    content: None,
                    line,
                },
                src.len(),
            ));
        }
        if bytes[p] == b'>' {
            p += 1;
            break;
        }
        if bytes[p] == b'/' {
            self_closing = true;
            p += 1;
            continue;
        }

        // Nome attributo.
        let name_start = p;
        while p < bytes.len()
            && !bytes[p].is_ascii_whitespace()
            && bytes[p] != b'='
            && bytes[p] != b'>'
            && bytes[p] != b'/'
        {
            p += src[p..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
        let attr_name = src[name_start..p].to_ascii_lowercase();
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= bytes.len() || bytes[p] != b'=' {
            if !attr_name.is_empty() {
                // Attributo booleano (`defer`, `hidden`): valore stringa vuota,
                // come nei browser.
                attrs.push(Attr { name: attr_name, value: String::new() });
            }
            continue;
        }
        p += 1; // '='
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= bytes.len() {
            break;
        }

        let value = match bytes[p] {
            b'"' | b'\'' => {
                let quote = bytes[p];
                p += 1;
                let v_start = p;
                while p < bytes.len() && bytes[p] != quote {
                    p += 1;
                }
                let v = src[v_start..p].to_string();
                if p < bytes.len() {
                    p += 1;
                }
                v
            }
            _ => {
                let v_start = p;
                while p < bytes.len() && !bytes[p].is_ascii_whitespace() && bytes[p] != b'>' {
                    p += 1;
                }
                let raw = &src[v_start..p];
                if let Some(stripped) = raw.strip_suffix('/') {
                    // `<img src=a.png/>`: lo slash è auto-chiusura, non valore.
                    self_closing = true;
                    stripped.to_string()
                } else {
                    raw.to_string()
                }
            }
        };
        if !attr_name.is_empty() {
            attrs.push(Attr { name: attr_name, value });
        }
    }

    Some((
        Tag {
            range: start..p,
            kind: if self_closing { TagKind::SelfClosing } else { TagKind::Start },
            name,
            attrs,
            content: None,
            line,
        },
        p,
    ))
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':'
}

/// Numeri di riga per un offset. Costruito una volta per documento: gli errori
/// che lo usano sono una manciata, ma i tag sono centinaia e `line()` deve
/// essere una ricerca e non una scansione.
struct LineIndex {
    newlines: Vec<usize>,
}

impl LineIndex {
    fn new(src: &str) -> Self {
        LineIndex { newlines: src.match_indices('\n').map(|(i, _)| i).collect() }
    }

    /// 1-based.
    fn line(&self, offset: usize) -> usize {
        self.newlines.partition_point(|&n| n < offset) + 1
    }
}

/// Numero di riga 1-based di un offset, ricalcolando da zero.
///
/// Costa una scansione: va usata per un errore, non per un tag.
pub fn line_of(src: &str, offset: usize) -> usize {
    LineIndex::new(src).line(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<String> {
        tags(src)
            .into_iter()
            .filter(|t| t.kind.is_start_like() || t.kind == TagKind::End)
            .map(|t| t.name)
            .collect()
    }

    #[test]
    fn legge_gli_attributi_e_i_byte_esatti() {
        let src = "<img src=\"a.png\" alt='b c' width=10>";
        let t = &tags(src)[0];
        assert_eq!(t.name, "img");
        assert_eq!(t.attr("src"), Some("a.png"));
        assert_eq!(t.attr("ALT"), Some("b c"));
        assert_eq!(t.attr("width"), Some("10"));
        assert_eq!(&src[t.range.clone()], "<img src=\"a.png\" alt='b c' width=10>");
    }

    #[test]
    fn il_maggiore_dentro_un_valore_quotato_non_finisce_il_tag() {
        let src = r#"<a title="a > b">x</a>"#;
        let t = &tags(src)[0];
        assert_eq!(t.attr("title"), Some("a > b"));
        assert_eq!(&src[t.range.clone()], r#"<a title="a > b">"#);
    }

    #[test]
    fn lo_slash_finale_e_auto_chiusura_e_non_valore() {
        let src = "<img src=a.png/>";
        let t = &tags(src)[0];
        assert_eq!(t.attr("src"), Some("a.png"));
        assert_eq!(t.kind, TagKind::SelfClosing);
    }

    #[test]
    fn lo_script_e_testo_grezzo_e_non_viene_riscannerizzato() {
        let src = "<script>var a = \"<b>non un tag</b>\";</script><p>dopo</p>";
        let t = &tags(src);
        let script = &t[0];
        assert_eq!(
            script.content_of(src),
            Some("var a = \"<b>non un tag</b>\";")
        );
        assert_eq!(names(src), vec!["script", "script", "p", "p"]);
    }

    #[test]
    fn lo_style_e_testo_grezzo() {
        let src = "<style>a{content:'</p>'}</style>";
        let t = &tags(src);
        assert_eq!(t[0].content_of(src), Some("a{content:'</p>'}"));
    }

    #[test]
    fn un_commento_non_produce_tag() {
        let src = "<!-- <h2>falso</h2> --><h2>vero</h2>";
        let t = tags(src);
        // Commento, apertura di `h2`, chiusura: il commento non produce tag, ma
        // il suo contenuto smette di essere scansionato — e quello che segue,
        // sì.
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].kind, TagKind::Comment);
        assert_eq!(names(src), vec!["h2", "h2"]);
    }

    #[test]
    fn il_doctype_non_produce_tag() {
        let t = tags("<!DOCTYPE html><html></html>");
        // Doctype, apertura di `html`, chiusura: tre cose, un doctype.
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].kind, TagKind::Doctype);
        assert_eq!(t[0].name, "");
    }

    #[test]
    fn il_documento_troncato_non_fa_panic() {
        // Ogni forma di malformità che un editore può lasciare indietro.
        for src in [
            "<",
            "<p",
            "<p ",
            "<p class=",
            "<p class=\"",
            "<script>var a = 1;",
            "<style>a{",
            "<!-- mai chiuso",
            "<!DOCTYPE",
            "</p>",
            "a < b e 3 < 4",
            "<p/>",
        ] {
            let _ = tags(src);
        }
    }

    #[test]
    fn i_giorni_di_riga_sono_giusti() {
        let src = "<html>\n  <body>\n    <script src=\"x\"></script>\n  </body>\n</html>";
        let t = tags(src);
        let script = t.iter().find(|t| t.is("script")).expect("script");
        assert_eq!(script.line, 3);
        assert_eq!(line_of(src, src.find("</body>").unwrap()), 4);
    }

    #[test]
    fn l_attributo_booleano_ha_valore_vuoto() {
        let src = "<script type=\"module\" defer src=\"a.js\"></script>";
        let t = &tags(src)[0];
        assert_eq!(t.attr("defer"), Some(""));
        assert_eq!(t.attr("type"), Some("module"));
    }
}
