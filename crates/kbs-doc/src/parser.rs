//! Il parser dell'artifact: titolo, `<meta>`, intestazioni con id stabile,
//! testo del corpo, contratto, claim.
//!
//! # Il modello è quello di `kb`, con una correzione
//!
//! `kb` estrae il corpo con `SKIP_TAGS = ["script", "style", "template",
//! "noscript"]` (`kb/crates/kb-core/src/parser.rs:1051`). Ma la sua
//! `headings_in_order_doc` (`kb/crates/kb-core/src/parser.rs:959`) chiama
//! `el.text()` su ogni `h1..h6` **senza** la stessa esclusione: un `<h2>` dentro
//! un `<template>` o dentro un `<noscript>` finisce nell'indice come se fosse un
//! titolo del documento. È il tipo di baco che si vede solo quando un artifact
//! è già indicizzato e i titoli non quadrano.
//!
//! Qui l'esclusione vale per **ogni** campo di testo — corpo, intestazioni,
//! codice — ed è applicata in un'unica discesa dell'albero, perché un campo che
//! estrae testo con regole diverse dagli altri è un campo che un giorno
//! racconterà una storia diversa dal documento. Il test
//! `un_h2_in_un_template_non_e_una_intestazione_del_documento` copre esattamente
//! quel baco, e `il_contratto_viene_preso_dallo_slot_esatto` copre il caso che
//! riguarda questo crate.
//!
//! # Cosa non fa
//!
//! Non chiama il checker, non esegue niente, non giudica una claim. Verifica che
//! un riferimento a un checker esista e che il contratto sia integro: il resto
//! è mestiere di `kbs-exercise`.

use std::collections::BTreeMap;
use std::ops::Deref;

use ego_tree::NodeRef;
use kbs_core::ClaimStatus;
use scraper::{Html, Node, Selector};
use serde::{Deserialize, Serialize};

use crate::offbox::{self, Reference};

/// L'id dello slot del contratto didattico (D7, D14). Esatto: `kb` usa
/// `template[id="kb-prompt"]` (`kb/crates/kb-core/src/parser.rs:327`), qui la
/// pagina è un'altra e il contratto è un altro.
pub const CONTRACT_SLOT: &str = "kb-kbprompt";

/// Il `<meta>` che dichiara che l'artifact è stato convertito da markdown (D14).
pub const PROVENANCE_META: &str = "kb-provenance";

/// Il valore che quel `<meta>` assume su un artifact convertito.
pub const PROVENANCE_CONVERTED: &str = "converted-from-markdown";

/// Elementi il cui testo non è prosa del documento.
///
/// Copiato da `kb` (`parser.rs:1051`): uno `<script>` è codice, uno `<style>` è
/// presentazione, un `<template>` è uno slot — il contratto — e un `<noscript>`
/// è un piano di riserva. Nessuno dei quattro è ciò che lo studente legge, e
/// nessuno dei quattro va nell'indice come se lo fosse.
pub const TEXT_EXCLUDED: [&str; 4] = ["script", "style", "template", "noscript"];

/// Un'intestazione, con l'id con cui si può linkarla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    /// 1..=6.
    pub level: u8,
    /// L'id dichiarato dall'autore, o quello derivato dal testo.
    pub id: String,
    pub text: String,
    /// `true` se l'id è stato derivato dal testo. Due titoli con lo stesso
    /// testo si urtano solo quando l'id è derivato.
    pub derived_id: bool,
}

/// Due intestazioni con lo stesso id.
///
/// Non viene risolto: viene **riferito**. Un suffisso numerico dipenderebbe
/// dalla posizione, quindi non sarebbe stabile fra un commit e l'altro, e un id
/// che cambia da solo è peggio di un id duplicato: i link che puntavano al
/// primo titolo smettono di funzionare senza che nessuno se ne accorga. La
/// risoluzione è dell'autore, e consiste in un `id` esplicito.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingCollision {
    pub id: String,
    pub first_text: String,
    pub second_text: String,
}

/// Che cosa uno span legato a una claim contiene davvero.
///
/// La distinzione è il punto, ed è quella che D6 chiede: una claim con
/// `span_anchor` e senza testo è un **indirizzo**, e un indirizzo non si
/// verifica. `kbs-core` lo dice in `Claim::span_text` — «senza questo,
/// `span_anchor` è un puntatore che il lettore non può verificare, e
/// l'affermazione torna a essere una dichiarazione con un indirizzo» — e qui è
/// la forma che il parser produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stato", rename_all = "kebab-case")]
pub enum SpanBinding {
    /// Lo span esiste e porta testo: la claim è **sostenuta**, non dimostrata.
    /// Che il testo la sostenga è un giudizio, e quel giudizio sta nel registro
    /// delle osservazioni (D6), non qui.
    Bound { anchor: String, text: String },
    /// La claim dichiara uno span e l'ancora c'è, ma l'ancora non porta testo.
    /// È il caso peggiore: sembra verificato e non lo è.
    AnchorOnly { anchor: String },
    /// La claim dichiara uno span e quell'id non esiste nel documento.
    Missing { anchor: String },
    /// La claim non dichiara nessuno span.
    Undeclared,
}

impl SpanBinding {
    pub fn is_bound(&self) -> bool {
        matches!(self, SpanBinding::Bound { .. })
    }

    /// Il testo effettivo dello span, se c'è.
    pub fn text(&self) -> Option<&str> {
        match self {
            SpanBinding::Bound { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Una frase che dice perché la claim non è sostenuta. Esiste perché
    /// «non supportata» senza spiegazione è indistinguibile da un bug.
    pub fn reason(&self) -> &'static str {
        match self {
            SpanBinding::Bound { .. } => "",
            SpanBinding::AnchorOnly { .. } => {
                "l'ancora esiste ma non contiene testo: è un indirizzo, non uno span"
            }
            SpanBinding::Missing { .. } => "l'ancora dichiarata non esiste nel documento",
            SpanBinding::Undeclared => "la claim non dichiara alcuno span",
        }
    }
}

/// Una claim dichiarata nell'artifact, prima di diventare riga di registro.
///
/// La dichiarazione è un elemento con `data-claim` (il fatto atomico),
/// `data-claim-id` (l'identità) e `data-claim-span` (l'id dello span che la
/// sostiene). I tre elementi — fatto, claim, span — restano separati perché è
/// la forma che il registro delle affermazioni richiede (D6) e l'unica in cui
/// lo span sopravvive a un cambio di parola nella claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedClaim {
    /// `data-claim-id`, o `clm_N` se l'autore non l'ha dato.
    pub id: String,
    /// Il fatto atomico, in una frase.
    pub text: String,
    pub span: SpanBinding,
}

impl ParsedClaim {
    /// Lo stato che questa claim avrebbe in `kbs-core`.
    ///
    /// È **dedotto dallo span**, non da un giudizio: `Supported` significa in
    /// D6 «legato a uno span che lo sostiene», e se lo span non è stato
    /// trovato la claim non può entrare come supportata. Diventa
    /// `Unciteable`, che non è una cancellazione: «nessuno span lo sostiene»,
    /// la riga resta nel registro e l'output è soppresso
    /// (`ClaimStatus::suppresses_output`).
    pub fn core_status(&self) -> ClaimStatus {
        if self.span.is_bound() {
            ClaimStatus::Supported
        } else {
            ClaimStatus::Unciteable
        }
    }

    pub fn is_supported(&self) -> bool {
        self.span.is_bound()
    }
}

/// Un artifact letto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedArtifact {
    pub title: Option<String>,
    /// `<meta name=…>` e `<meta property=…>`, per nome. `BTreeMap` perché
    /// l'ordine dei meta non deve cambiare l'hash di un artifact che l'ha solo
    /// riscritto in ordine diverso.
    pub meta: BTreeMap<String, String>,
    pub headings: Vec<Heading>,
    pub heading_collisions: Vec<HeadingCollision>,
    /// Testo del corpo, con [`TEXT_EXCLUDED`] escluso e spazi normalizzati.
    pub body_text: String,
    /// Il testo di `<code>` e `<pre>`, con la stessa esclusione.
    pub code_text: String,
    /// Il testo di ogni elemento che ha un `id`: la mappa con cui si risolvono
    /// gli span delle claim.
    pub anchors: BTreeMap<String, String>,
    /// Il contenuto decodificato di `<template id="kb-kbprompt">`.
    pub contract_text: Option<String>,
    pub claims: Vec<ParsedClaim>,
    /// Riferimenti trovati con la loro origine: vedi [`crate::offbox`].
    pub references: Vec<Reference>,
}

impl ParsedArtifact {
    /// `true` se l'artifact dichiara di essere stato convertito da markdown.
    pub fn converted_from_markdown(&self) -> bool {
        self.meta
            .get(PROVENANCE_META)
            .is_some_and(|v| v == PROVENANCE_CONVERTED)
    }

    /// Le claim che il lettore **non** può verificare, con la ragione.
    pub fn unverified_claims(&self) -> impl Iterator<Item = &ParsedClaim> {
        self.claims.iter().filter(|c| !c.is_supported())
    }
}

/// Legge un artifact. Non fallisce mai e non fa `panic!` su input malformato:
/// un artifact rotto deve produrre un errore che lo nomina, non far cadere il
/// processo che lo indicizza.
pub fn parse(src: &str) -> ParsedArtifact {
    let doc = Html::parse_document(src);
    let sel = Selectors::new();

    let title = doc
        .select(&sel.title)
        .next()
        .map(|e| collapse(&text_of(*e.deref())))
        .filter(|t| !t.is_empty());

    let mut meta = BTreeMap::new();
    for e in doc.select(&sel.meta) {
        let el = e.value();
        if let Some(name) = el.attr("name").or_else(|| el.attr("property")) {
            if let Some(content) = el.attr("content") {
                meta.insert(name.to_ascii_lowercase(), content.to_string());
            }
        }
    }

    let mut acc = Walk::default();
    if let Some(body) = doc.select(&sel.body).next() {
        walk(*body.deref(), &mut acc);
    }

    let heading_collisions = collisions(&acc.headings);

    // Gli span si risolvono **dopo** la discesa, quando la mappa degli id è
    // completa: una claim può dichiarare uno span che viene dopo nel documento,
    // e risolverla durante la discesa la renderebbe dipendente dall'ordine.
    let claims = acc
        .claims
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let span = match c.span_anchor {
                None => SpanBinding::Undeclared,
                Some(anchor) => {
                    let key = anchor.strip_prefix('#').unwrap_or(&anchor);
                    match acc.anchors.get(key) {
                        Some(text) if !text.trim().is_empty() => SpanBinding::Bound {
                            anchor: key.to_string(),
                            text: text.clone(),
                        },
                        Some(_) => SpanBinding::AnchorOnly { anchor: key.to_string() },
                        None => SpanBinding::Missing { anchor: key.to_string() },
                    }
                }
            };
            ParsedClaim {
                id: if c.id.is_empty() { format!("clm_{}", i + 1) } else { c.id },
                text: c.text,
                span,
            }
        })
        .collect();

    // Il contratto si legge **senza** normalizzare gli spazi: le sue otto
    // sezioni sono righe, e un a capo trasformato in spazio trasformerebbe un
    // contratto valido in un testo senza intestazioni. È l'unico campo che non
    // subisce la normalizzazione, ed è per questo che l'esclude esplicitamente
    // e non per omissione.
    let contract_text = doc
        .select(&sel.contract)
        .next()
        .map(|e| text_verbatim(*e.deref()).trim().to_string())
        .filter(|s| !s.is_empty());

    ParsedArtifact {
        title,
        meta,
        headings: acc.headings,
        heading_collisions,
        body_text: acc.body_text,
        code_text: acc.code_text,
        anchors: acc.anchors,
        contract_text,
        claims,
        references: offbox::references(src),
    }
}

struct Selectors {
    title: Selector,
    meta: Selector,
    body: Selector,
    contract: Selector,
}

impl Selectors {
    fn new() -> Self {
        fn s(q: &str) -> Selector {
            Selector::parse(q).expect("selettore statico")
        }
        Selectors {
            title: s("title"),
            meta: s("meta"),
            body: s("body"),
            contract: s(&format!("template[id=\"{CONTRACT_SLOT}\"]")),
        }
    }
}

#[derive(Default)]
struct Acc {
    headings: Vec<Heading>,
    anchors: BTreeMap<String, String>,
    body_text: String,
    code_text: String,
    claims: Vec<ClaimStub>,
}

type Walk = Acc;

struct ClaimStub {
    id: String,
    text: String,
    span_anchor: Option<String>,
}

/// Una sola discesa del corpo. Le esclusioni valgono per ogni campo, e qui è
/// l'unico posto in cui sono decise.
fn walk(node: NodeRef<'_, Node>, acc: &mut Walk) {
    for child in node.children() {
        match child.value() {
            Node::Text(t) => push_normalized(&mut acc.body_text, t),
            Node::Element(el) => {
                let name = el.name.local.to_lowercase();
                if is_excluded(&name) {
                    // L'intero sottoalbero è fuori dal testo del documento: qui
                    // non si scende. È la differenza rispetto a
                    // `headings_in_order_doc` di `kb`.
                    continue;
                }
                let block = is_block(&name);
                if block {
                    acc.body_text.push('\n');
                }
                if let Some(id) = el.id() {
                    acc.anchors.insert(id.to_string(), collapse(&text_of(child)));
                }
                if let Some(fatto) = el.attr("data-claim") {
                    acc.claims.push(ClaimStub {
                        id: el.attr("data-claim-id").unwrap_or("").to_string(),
                        text: fatto.trim().to_string(),
                        span_anchor: el.attr("data-claim-span").map(str::to_string),
                    });
                }
                if let Some(level) = heading_level(&name) {
                    let text = collapse(&text_of(child));
                    let id = match el.id() {
                        Some(i) => i.to_string(),
                        None => heading_id(&text, level),
                    };
                    acc.headings.push(Heading {
                        level,
                        id,
                        text,
                        derived_id: el.id().is_none(),
                    });
                }
                if name == "code" || name == "pre" {
                    acc.code_text.push_str(&text_of(child));
                    acc.code_text.push('\n');
                }
                walk(child, acc);
                if block {
                    acc.body_text.push('\n');
                }
            }
            _ => {}
        }
    }
}

/// `true` se il nome dell'elemento è fra [`TEXT_EXCLUDED`].
pub fn is_excluded(name: &str) -> bool {
    TEXT_EXCLUDED.contains(&name)
}

fn heading_level(name: &str) -> Option<u8> {
    match name {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "tr"
            | "blockquote"
            | "pre"
            | "figcaption"
            | "header"
            | "footer"
            | "nav"
            | "aside"
            | "main"
            | "table"
            | "ul"
            | "ol"
            | "dl"
            | "dt"
            | "dd"
            | "hr"
            | "br"
    )
}

/// Il testo di un sottoalbero, con [`TEXT_EXCLUDED`] escluso.
fn text_of(node: NodeRef<'_, Node>) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out
}

/// Il testo di un sottoalbero **senza normalizzare gli spazi**: a capo, tab e
/// spazi multipli sopravvivono. Serve per il contratto, che è fatto di righe.
fn text_verbatim(node: NodeRef<'_, Node>) -> String {
    let mut out = String::new();
    collect_verbatim(node, &mut out);
    out
}

fn collect_verbatim(node: NodeRef<'_, Node>, out: &mut String) {
    for child in node.children() {
        match child.value() {
            Node::Text(t) => out.push_str(t),
            // Il contenuto di un `<template>` **non** sta nei figli
            // dell'elemento: sta in un `Node::Fragment` che html5ever crea
            // come primo figlio (la regola dei «template contents»). I walker
            // che scendono solo su `Text` ed `Element` vedono un template
            // vuoto, e il contratto di D7 sparisce senza che nessuno se ne
            // accorga. È il buco che questo `Node::Fragment` chiude.
            Node::Fragment => collect_verbatim(child, out),
            Node::Element(el) => {
                if is_excluded(&el.name.local.to_lowercase()) {
                    continue;
                }
                collect_verbatim(child, out);
            }
            _ => {}
        }
    }
}

fn collect_text(node: NodeRef<'_, Node>, out: &mut String) {
    for child in node.children() {
        match child.value() {
            Node::Text(t) => push_normalized(out, t),
            // Come sopra: i «template contents» stanno in un frammento. Qui il
            // frammento si attraversa e il suo contenuto si scarta, perché il
            // contenuto di un template non è prosa del documento.
            Node::Fragment => collect_text(child, out),
            Node::Element(el) => {
                if is_excluded(&el.name.local.to_lowercase()) {
                    continue;
                }
                collect_text(child, out);
            }
            _ => {}
        }
    }
}

fn push_normalized(out: &mut String, t: &str) {
    if t.trim().is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with([' ', '\n']) {
        out.push(' ');
    }
    for c in t.chars() {
        out.push(if c.is_whitespace() { ' ' } else { c });
    }
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// L'id di un'intestazione il cui id non è stato dichiarato.
///
/// Un titolo senza testo non ha slug: si usa `h{level}`, perché un id vuoto non
/// è un id, e un id che non si può linkare è un titolo che non si può citare.
pub fn heading_id(text: &str, level: u8) -> String {
    let slug = crate::slug::kebab(text);
    if slug.is_empty() {
        format!("h{level}")
    } else {
        slug
    }
}

fn collisions(headings: &[Heading]) -> Vec<HeadingCollision> {
    let mut out = Vec::new();
    for (i, h) in headings.iter().enumerate() {
        if let Some(first) = headings[..i].iter().find(|p| p.id == h.id) {
            out.push(HeadingCollision {
                id: h.id.clone(),
                first_text: first.text.clone(),
                second_text: h.text.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<!DOCTYPE html>
<html lang="it">
<head>
  <meta charset="utf-8">
  <title>Il teorema di Fermat</title>
  <meta name="kb-course" content="mat-1">
</head>
<body>
  <h1>Il teorema di Fermat</h1>
  <p>Prose <em>reale</em>.</p>
  <template id="kb-kbprompt">## GUARDIAN
non confondere</template>
  <script>var segreto = 1;</script>
  <style>h1{color:red}</style>
  <noscript>serve javascript</noscript>
  <pre><code>let x = 1;</code></pre>
</body>
</html>"#;

    #[test]
    fn titolo_e_meta() {
        let a = parse(DOC);
        assert_eq!(a.title.as_deref(), Some("Il teorema di Fermat"));
        assert_eq!(a.meta.get("kb-course").map(String::as_str), Some("mat-1"));
    }

    #[test]
    fn il_corpo_non_contiene_script_stile_noscript_ne_contratto() {
        let a = parse(DOC);
        assert!(a.body_text.contains("Prose reale"), "{}", a.body_text);
        assert!(!a.body_text.contains("segreto"));
        assert!(!a.body_text.contains("color:red"));
        assert!(!a.body_text.contains("serve javascript"));
        assert!(
            !a.body_text.contains("GUARDIAN"),
            "il contratto non è prosa del documento: {}",
            a.body_text
        );
    }

    #[test]
    fn un_h2_in_un_template_non_e_una_intestazione_del_documento() {
        // Il baco di `kb` (`parser.rs:959`): `headings_in_order_doc` non
        // applica `SKIP_TAGS`. Qui non deve ripetersi.
        let doc = r#"<html><body>
            <h2>Vero</h2>
            <template id="altro"><h2>Dentro il template</h2></template>
            <noscript><h2>Nel noscript</h2></noscript>
        </body></html>"#;
        let a = parse(doc);
        let titoli: Vec<&str> = a.headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(titoli, vec!["Vero"]);
    }

    #[test]
    fn le_intestazioni_hanno_id_stabile_e_derivato() {
        let a = parse(DOC);
        let h1 = &a.headings[0];
        assert_eq!(h1.level, 1);
        assert_eq!(h1.id, "il-teorema-di-fermat");
        assert!(h1.derived_id);
        // Lo stesso sorgente, due letture, lo stesso id.
        assert_eq!(a.headings, parse(DOC).headings);
    }

    #[test]
    fn lo_stesso_testo_due_volte_e_una_collisione_riferita() {
        let doc = "<html><body><h2>Equivoci</h2><p>x</p><h2>Equivoci</h2></body></html>";
        let a = parse(doc);
        assert_eq!(a.headings.len(), 2);
        assert_eq!(a.heading_collisions.len(), 1);
        assert_eq!(a.heading_collisions[0].id, "equivoci");
    }

    #[test]
    fn un_id_esplicito_risolve_la_collisione() {
        let doc = r#"<html><body>
            <h2 id="equivoci-1">Equivoci</h2>
            <h2 id="equivoci-2">Equivoci</h2>
        </body></html>"#;
        let a = parse(doc);
        assert!(a.heading_collisions.is_empty());
        assert!(a.headings.iter().all(|h| !h.derived_id));
    }

    #[test]
    fn il_contratto_viene_preso_dallo_slot_esatto() {
        let a = parse(DOC);
        assert_eq!(
            a.contract_text.as_deref(),
            Some("## GUARDIAN\nnon confondere")
        );
    }

    #[test]
    fn nessuno_slot_nessun_contratto() {
        let a = parse("<html><body><p>x</p></body></html>");
        assert!(a.contract_text.is_none());
    }

    #[test]
    fn il_codice_e_estratto() {
        let a = parse(DOC);
        assert!(a.code_text.contains("let x = 1"), "{}", a.code_text);
    }

    #[test]
    fn nessun_panic_su_ingresso_malformato() {
        for src in [
            "",
            "<html",
            "<body><h2>",
            "&&&",
            r#"<html><body><template id="kb-kbprompt">"#,
            "<!--",
            "<body><template id=\"kb-kbprompt\"><template>",
        ] {
            let a = parse(src);
            let _ = a.body_text.len();
        }
    }
}
