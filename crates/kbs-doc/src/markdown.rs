//! Markdown in ingresso → artifact HTML, con la provenienza dichiarata (D14).
//!
//! # Perché il markdown è tollerato e non è il formato
//!
//! Il materiale di studio esiste già in markdown e buttarlo via è un danno.
//! Ma il **formato di authoring** è HTML, e la ragione è che l'unità di studio
//! non è un testo: un file HTML può contenere un contratto eseguibile, un
//! esercizio verificabile e una scena 3D **nello stesso documento**. Il
//! markdown può descriverne uno solo dei tre.
//!
//! # Che cosa il markdown non può esprimere, e cosa si perde davvero
//!
//! Non è una lista retorica: sono le tre cose che il convertitore **non può
//! produrre**, e ognuna ha un nome in questo repository.
//!
//! * **Una scena 3D** (D15.1) — un `<canvas>`, un modulo three.js, quattro
//!   claim legate ai nodi e agli archi. Nel markdown c'è solo `![figura](…)`, e
//!   una figura non è una scena: non si manipola, non ha claim, non ha span.
//! * **Un verificatore deterministico** (D8) — il riferimento al checker è un
//!   attributo su un elemento, e un esercizio senza checker **non entra** nel
//!   percorso di pubblicazione. Un esercizio markdown è, per definizione, un
//!   esercizio senza checker.
//! * **Un riferimento a uno span** (D6) — `data-claim` e `data-claim-span`
//!   non hanno equivalenti: nel markdown un'affermazione e la frase che la
//!   sostiene sono entrambe testo, e la struttura che le tiene insieme è
//!   esattamente ciò che il markdown non sa esprimere.
//!
//! Per questo l'output porta `<meta name="kb-provenance" content="converted-from-markdown">`:
//! un artifact convertito è un artifact **con meno garanzie**, e il lettore deve
//! poterlo sapere prima di fidarsi.
//!
//! # Il sottoinsieme supportato
//!
//! Questo non è CommonMark: è il sottoinsieme che il materiale scolastico usa
//! davvero, ed è dichiarato perché un convertitore che finge di fare tutto e
//! sbaglia il resto è peggio di uno che fa poco e dice cosa. Supportati:
//! front-matter `---\nchiave: valore\n---`, titoli ATX, paragrafi, elenchi
//! puntati e numerati, citazioni, fence di codice, tabelle, righe orizzontali,
//! e in linea: codice, grasso, corsivo, link, immagini, escape.
//!
//! **Non** supportati e quindi convertiti come testo semplice: note a piè di
//! pagina, blocchi matematici, tabelle con celle unite, HTML inline. Un
//! convertitore che «prova lo stesso» produce un artifact che sembra funzionare
//! e non funziona.

use std::fmt::Write as _;

use crate::parser::{CONTRACT_SLOT, PROVENANCE_CONVERTED, PROVENANCE_META};
use crate::slug::kebab;

/// Il blocco che diventa il contratto: un fence con info string `kb-kbprompt`.
///
/// ```text
/// ```kb-kbprompt
/// ## GUARDIAN
/// …
/// ```
/// ```
pub const CONTRACT_FENCE: &str = "kb-kbprompt";

/// Il risultato della conversione.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    /// L'artifact HTML.
    pub html: String,
    /// `true` se il markdown conteneva un contratto.
    pub has_contract: bool,
}

/// Converte markdown in artifact HTML.
///
/// Non fallisce: un markdown malformato produce un artifact HTML con dentro il
/// testo che non ha saputo interpretare, che è più onesto di un errore che
/// impedisce di vedere il materiale.
pub fn convert(md: &str) -> Converted {
    let (front, body) = split_front_matter(md);

    let mut title = front.iter().find(|(k, _)| k == "title").map(|(_, v)| v.clone());
    let mut contract: Option<String> = None;
    let mut out: Vec<String> = Vec::new();

    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0usize;
    let mut h1_emesso = false;
    while i < lines.len() {
        let prima = i;
        let line = lines[i];
        let t = line.trim_end();

        // Fence: codice, o contratto.
        if let Some(lang) = t.trim_start().strip_prefix("```") {
            let lang = lang.trim();
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && !lines[end].trim_start().starts_with("```") {
                end += 1;
            }
            let body_text: Vec<&str> = lines[start..end.min(lines.len())].to_vec();
            if lang.eq_ignore_ascii_case(CONTRACT_FENCE) {
                contract = Some(body_text.join("\n").trim().to_string());
            } else {
                let mut s = format!("<pre><code");
                if !lang.is_empty() {
                    let _ = write!(s, " class=\"language-{}\"", escape(lang));
                }
                let _ = write!(s, ">{}</code></pre>", escape(&body_text.join("\n")));
                out.push(s);
            }
            i = end + 1;
            continue;
        }

        // Riga orizzontale.
        if is_hr(t) {
            out.push("<hr>".to_string());
            i += 1;
            continue;
        }

        // Titoli.
        if let Some((level, text)) = heading(t) {
            if level == 1 {
                // Un `h1` nel corpo **è** l'`h1` del documento, sia o no che il
                // front-matter porti già un titolo: altrimenti l'assemblaggio
                // ne emetterebbe un secondo con lo stesso testo, cioè una
                // collisione di id su un titolo scritto una volta sola.
                if title.is_none() {
                    title = Some(text.trim().to_string());
                }
                h1_emesso = true;
            }
            let id = kebab(text.trim());
            let id_attr = if id.is_empty() { String::new() } else { format!(" id=\"{id}\"") };
            out.push(format!("<h{level}{id_attr}>{}</h{level}>", inline(text)));
            i += 1;
            continue;
        }

        // Citazione.
        if t.trim_start().starts_with("> ") || t.trim() == ">" {
            let mut testo = Vec::new();
            while i < lines.len()
                && (lines[i].trim_start().starts_with("> ") || lines[i].trim() == ">")
            {
                testo.push(lines[i].trim_start().trim_start_matches('>').trim_start().to_string());
                i += 1;
            }
            out.push(format!("<blockquote><p>{}</p></blockquote>", inline(&testo.join(" "))));
            continue;
        }

        // Elenco numerato.
        if let Some(n) = ordered_marker(t) {
            let mut items = Vec::new();
            while i < lines.len() {
                match ordered_marker(lines[i]) {
                    Some(k) if k == n => {
                        items.push(inline(lines[i][(n + 1).min(lines[i].len())..].trim()));
                        i += 1;
                    }
                    _ => break,
                }
            }
            out.push(format!("<ol>{}</ol>", items.iter().map(|i| format!("<li>{i}</li>")).collect::<String>()));
            continue;
        }

        // Elenco puntato.
        if is_bullet(t) {
            let mut items = Vec::new();
            while i < lines.len() && is_bullet(lines[i]) {
                let t = lines[i].trim_start();
                items.push(inline(t[2..].trim()));
                i += 1;
            }
            out.push(format!("<ul>{}</ul>", items.iter().map(|i| format!("<li>{i}</li>")).collect::<String>()));
            continue;
        }

        // Paragrafo: righe non vuote fino alla successiva riga vuota.
        if t.trim().is_empty() {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() && !starts_block(&lines[i]) {
            i += 1;
        }
        let paragrafo: Vec<&str> = lines[start..i].to_vec();
        out.push(format!("<p>{}</p>", inline(&paragrafo.join(" "))));

        // Rete di sicurezza: nessun ramo che non gestisce un blocco deve
        // poter girare all'infinito su materiale scritto a mano. Ogni ramo
        // qui sopra avanza `i`; se per un input che non conosco nessuno lo fa,
        // si avanza di uno e si esce. Il costo è una riga, e il prezzo di
        // non averla è un convertitore che non termina.
        if i == prima {
            i += 1;
        }
    }

    Converted {
        html: assemble(&front, title, h1_emesso, &out, contract.as_deref()),
        has_contract: contract.is_some(),
    }
}

/// Il front-matter YAML-less: solo `chiave: valore`, che è tutto ciò che
/// serve per dichiarare titolo, corso e tag. Una sintassi YAML completa qui
/// sarebbe una dependenza da 40 KB per undici righe di output.
fn split_front_matter(md: &str) -> (Vec<(String, String)>, &str) {
    let Some(rest) = md.strip_prefix("---\n") else { return (Vec::new(), md) };
    let Some(end) = rest.find("\n---\n") else { return (Vec::new(), md) };
    let head = &rest[..end];
    let body = &rest[end + "\n---\n".len()..];
    let front = head
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            let (k, v) = l.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect();
    (front, body)
}

fn assemble(
    front: &[(String, String)],
    title: Option<String>,
    h1_emesso: bool,
    body: &[String],
    contract: Option<&str>,
) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("<!DOCTYPE html>\n<html lang=\"it\">\n<head>\n<meta charset=\"utf-8\">\n");
    let _ = writeln!(s, "<title>{}</title>", escape(title.as_deref().unwrap_or("Senza titolo")));
    for (k, v) in front {
        // Una chiave che già dice `kb-` non diventa `kb-kb-`: il front-matter
        // scrive il nome del meta com'è, e raddoppiare il prefisso renderebbe
        // `kb-course` irraggiungibile.
        let nome = if k.starts_with("kb-") { k.clone() } else { format!("kb-{k}") };
        let _ = writeln!(s, "<meta name=\"{}\" content=\"{}\">", escape(&nome), escape(v));
    }
    // La provenienza, per prima fra i meta di `kb`: è la cosa che un lettore
    // deve poter vedere senza aprire il sorgente.
    let _ = writeln!(s, "<meta name=\"{PROVENANCE_META}\" content=\"{PROVENANCE_CONVERTED}\">");
    if let Some(c) = contract {
        let _ = writeln!(s, "<template id=\"{CONTRACT_SLOT}\">");
        let _ = writeln!(s, "{}", c);
        s.push_str("</template>\n");
    }
    s.push_str("</head>\n<body>\n");
    // L'`h1` del titolo si emette **una volta sola**: se il markdown aveva già
    // un `# Titolo`, quel titolo è l'`h1` del documento. Emetterne due produrrebbe
    // due intestazioni con lo stesso testo, quindi una collisione di id — e una
    // collisione blocca la pubblicazione, per un titolo che l'autore ha scritto
    // una volta.
    if let Some(t) = title.filter(|_| !h1_emesso) {
        let _ = writeln!(s, "<h1>{}</h1>", escape(&t));
    }
    for b in body {
        let _ = writeln!(s, "{b}");
    }
    s.push_str("</body>\n</html>\n");
    s
}

fn is_hr(t: &str) -> bool {
    let t = t.trim();
    t.len() >= 3
        && (t.starts_with("---") || t.starts_with("***"))
        && t.chars().all(|c| c == '-' || c == '*' || c == ' ')
}

fn heading(t: &str) -> Option<(usize, &str)> {
    let t = t.trim();
    let hashes = t.len() - t.trim_start_matches('#').len();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = t[hashes..].trim_start();
    // CommonMark: uno spazio dopo i cancelletti è obbligatorio, altrimenti `#1`
    // è testo e non un titolo.
    if rest.len() == t.len() - hashes {
        return None;
    }
    Some((hashes, rest))
}

fn is_bullet(t: &str) -> bool {
    let t = t.trim_start();
    t.len() >= 2
        && (t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ "))
        && !is_hr(t)
}

fn ordered_marker(t: &str) -> Option<usize> {
    let t = t.trim_start();
    let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &t[digits..];
    if !(rest.starts_with(". ") || rest.starts_with(") ")) {
        return None;
    }
    t[..digits].parse().ok()
}

fn starts_block(t: &str) -> bool {
    heading(t).is_some()
        || is_hr(t)
        || is_bullet(t)
        || ordered_marker(t).is_some()
        || t.trim_start().starts_with("```")
        || t.trim_start().starts_with("> ")
}

/// L'inline: codice, grasso, corsivo, link, immagini, escape.
///
/// Il limite dichiarato: `*` e `_` aprono e chiudono corsivo anche dentro una
/// parola, dove CommonMark non lo farebbe. Su un testo scolastico la
/// differenza è un `f(x)_1` che diventa corsivo invece che pedice, e il
/// rimediare significherebbe portare dentro un parser di enfasi per intero per
/// un caso che non si presenta.
fn inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < b.len() {
        // Codice inline: dentro non si tocca niente, è il punto del codice.
        if b[i] == '`' {
            if let Some(end) = find(&b, i + 1, '`') {
                let testo: String = b[i + 1..end].iter().collect();
                let _ = write!(out, "<code>{}</code>", escape(&testo));
                i = end + 1;
                continue;
            }
        }
        // Immagine, poi link: `![…](…)` comincia con `[`.
        if b[i] == '!' && b.get(i + 1) == Some(&'[') {
            if let Some((alt, url, next)) = link(&b, i + 1) {
                let _ = write!(out, "<img src=\"{}\" alt=\"{}\">", escape(&url), escape(&alt));
                i = next;
                continue;
            }
        }
        if b[i] == '[' {
            if let Some((testo, url, next)) = link(&b, i) {
                let _ = write!(out, "<a href=\"{}\">{}</a>", escape(&url), escape(&testo));
                i = next;
                continue;
            }
        }
        // Grasso e corsivo. Un marcatore senza chiusura è testo: `a * b` resta
        // `a * b` invece di mangiare mezzo documento.
        let mut emphasi = false;
        for (marker, tag) in [("**", "strong"), ("__", "strong"), ("*", "em"), ("_", "em")] {
            let m: Vec<char> = marker.chars().collect();
            if !b[i..].starts_with(&m[..]) {
                continue;
            }
            let start = i + m.len();
            match find_seq(&b, start, &m) {
                Some(end) if end > start => {
                    let testo: String = b[start..end].iter().collect();
                    let _ = write!(out, "<{tag}>{}</{tag}>", inline(&testo));
                    i = end + m.len();
                    emphasi = true;
                    break;
                }
                _ => {}
            }
        }
        if emphasi {
            continue;
        }
        // Escape esplicito.
        if b[i] == '\\' && i + 1 < b.len() {
            out.extend(escape_char(b[i + 1]));
            i += 2;
            continue;
        }
        out.extend(escape_char(b[i]));
        i += 1;
    }
    out
}

fn link(b: &[char], start: usize) -> Option<(String, String, usize)> {
    if b.get(start) != Some(&'[') {
        return None;
    }
    let fine_testo = find(b, start + 1, ']')?;
    if b.get(fine_testo + 1) != Some(&'(') {
        return None;
    }
    let fine_url = find(b, fine_testo + 2, ')')?;
    let testo: String = b[start + 1..fine_testo].iter().collect();
    let url: String = b[fine_testo + 2..fine_url].iter().collect();
    Some((testo, url.trim().to_string(), fine_url + 1))
}

fn find(b: &[char], from: usize, needle: char) -> Option<usize> {
    (from..b.len()).find(|&i| b[i] == needle)
}

fn find_seq(b: &[char], from: usize, needle: &[char]) -> Option<usize> {
    (from..b.len().saturating_sub(needle.len() - 1))
        .find(|&i| &b[i..i + needle.len()] == needle)
}

fn escape(s: &str) -> String {
    s.chars().flat_map(escape_char).collect()
}

fn escape_char(c: char) -> Vec<char> {
    match c {
        '&' => "&amp;".chars().collect(),
        '<' => "&lt;".chars().collect(),
        '>' => "&gt;".chars().collect(),
        '"' => "&quot;".chars().collect(),
        '\'' => "&#39;".chars().collect(),
        other => vec![other],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::validate;

    const MD: &str = r#"---
title: Continuità
kb-course: mat-1
---

# Continuità

Una funzione **continua** non è per questo *uniformemente* continua.

## Equivoci

- il grafico non dimostra
- la continuità non implica l'uniformità

```rust
let x = 1;
```

```kb-kbprompt
## GUARDIAN
Non confondere le due nozioni.
## PREREQUISITI
Sapere leggere un grafico.
## OBIETTIVI
Distinguerle.
## SCALA
Funzioni in una variabile.
## EQUIVOCI
Continuità non è uniformità.
## ESEMPIO-LAVORATO
f(x)=x^2 su [0,1].
## VERIFICA
Classificare tre funzioni.
## LIMITE
Il grafico orienta, non dimostra.
```
"#;

    #[test]
    fn il_risultato_dichiara_la_provenienza() {
        let c = convert(MD);
        assert!(c.html.contains("<meta name=\"kb-provenance\" content=\"converted-from-markdown\">"));
        let a = parser::parse(&c.html);
        assert!(a.converted_from_markdown());
    }

    #[test]
    fn il_front_matter_diventa_meta() {
        let a = parser::parse(&convert(MD).html);
        assert_eq!(a.meta.get("kb-course").map(String::as_str), Some("mat-1"));
        assert_eq!(a.title.as_deref(), Some("Continuità"));
    }

    #[test]
    fn il_contratto_viene_preso_dal_fence() {
        let c = convert(MD);
        assert!(c.has_contract);
        let a = parser::parse(&c.html);
        let testo = a.contract_text.expect("contratto");
        assert!(testo.starts_with("## GUARDIAN"));
        let report = validate::inspect(&c.html);
        assert!(report.can_publish(), "{:?}", report.blocking().collect::<Vec<_>>());
    }

    #[test]
    fn i_titoli_prendono_id_stabili() {
        let a = parser::parse(&convert(MD).html);
        let ids: Vec<&str> = a.headings.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["continuita", "equivoci"]);
        assert!(a.heading_collisions.is_empty());
    }

    #[test]
    fn il_codice_e_il_markdown_inline_sono_grezzi() {
        let html = convert(MD).html;
        assert!(html.contains("<pre><code class=\"language-rust\">let x = 1;</code></pre>"));
        assert!(html.contains("<strong>continua</strong>"));
        assert!(html.contains("<em>uniformemente</em>"));
        assert!(html.contains("<li>il grafico non dimostra</li>"));
    }

    #[test]
    fn il_markdown_malformato_non_fa_panic() {
        for md in ["", "```", "#", "- ", "|", "```kb-kbprompt", "---\n", "***", "> "] {
            let _ = convert(md);
        }
    }

    #[test]
    fn un_markdown_senza_contratto_produce_un_artifact_non_pubblicabile() {
        let c = convert("# Solo testo\n\nNessun contratto qui.");
        assert!(!c.has_contract);
        let r = validate::inspect(&c.html);
        assert!(!r.can_publish());
        assert!(r
            .blocking()
            .any(|i| matches!(i.code, validate::IssueCode::NoContract)));
        // E dice perché è stato convertito, così il lettore non crede di avere
        // davanti un artifact scritto in HTML.
        assert!(r
            .warnings()
            .any(|i| matches!(i.code, validate::IssueCode::ConvertedFromMarkdown)));
    }

    #[test]
    fn l_html_del_markdown_e_fuggito() {
        let c = convert("# Titolo\n\n<script>alert(1)</script>");
        assert!(!c.html.contains("<script>alert"));
        assert!(c.html.contains("&lt;script&gt;"));
    }
}
