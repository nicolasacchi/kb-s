//! La resa di una voce in artifact HTML.
//!
//! D14: il materiale di studio è **HTML self-contained**, non markdown. Il
//! front-matter è `<meta>`, il contratto è il template `kb-kbprompt`, e la
//! validazione guarda l'HTML risultante e non una sintassi intermedia.
//!
//! La resa è una funzione pura della tabella: stesse voci, stessi byte. È la
//! ragione per cui `kbs-bench emit` può rigenerare il corpus e un test può
//! verificare che i file committati siano quelli che la tabella descrive.

use crate::families::Family;
use crate::spec::{ClaimSpec, EmittenteSpec, InstanceSpec, OriginSpec, SceneEdge, SceneNode, Spec};
use kbs_core::{Millis, PublicationState};
use serde_json::{json, Map, Value};

/// Il nome esatto dello slot del contratto, deciso in `kbs-doc` (D7).
pub const SLOT_CONTRATTO: &str = "kb-kbprompt";


/// Il percorso del runtime three.js servito dal binario (D15). È la stessa
/// stringa che la tabella dichiara per l'item che lo carica: due costanti per
/// un percorso sono due occasioni di scriverlo diversamente.
pub const PERCORSO_THREE: &str = crate::table_4::THREE_LOCALE;

/// L'epoca di riferimento del banco: 2026-09-26T00:00:00Z, in millisecondi.
///
/// Tutti i tempi del banco derivano da qui. Un banco che usa `Millis::now()`
/// produce hash diversi a ogni esecuzione e non può più dire se qualcosa è
/// cambiato o se è passato il tempo.
pub const EPOCA: i64 = 1_790_380_800_000;

/// Il tempo di creazione dell'item di indice `i`: un'ora per item, a partire
/// dall'epoca. La spaziatura rende leggibile l'ordine nel referto.
pub fn creato_a(i: usize) -> Millis {
    Millis(EPOCA + (i as i64) * 3_600_000)
}

/// Il tempo dell'ultima modifica: dieci minuti dopo la creazione.
pub fn aggiornato_a(i: usize) -> Millis {
    Millis(EPOCA + (i as i64) * 3_600_000 + 600_000)
}

/// Il tempo della ratifica: un minuto dopo la modifica, quando c'è.
pub fn ratificato_a(i: usize) -> Millis {
    Millis(EPOCA + (i as i64) * 3_600_000 + 660_000)
}

/// Il testo così com'è finisce nell'HTML, con i caratteri che hanno significato
/// in HTML evasi.
///
/// È pubblica perché il banco verifica che gli span dichiarati siano
/// **davvero nel file**, e il confronto va fatto sul testo che finisce nel
/// file: un apostrofo dentro uno span renderebbe il confronto sul testo grezzo
/// falso, e un controllo che fallisce per la ragione sbagliata è peggio di un
/// controllo che non esiste.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn slug_claim(id: &str) -> String {
    let n: String = id.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("span-{n}")
}

fn stato_label(s: PublicationState) -> &'static str {
    match s {
        PublicationState::Bozza => "bozza",
        PublicationState::DelDocente => "del-docente",
        PublicationState::InCorso => "in-corso",
        PublicationState::Archiviato => "archiviato",
    }
}

/// Il nome locale del corso. I due corsi del banco sono due perimetri di
/// condivisione distinti, ed è ciò che rende il predicato di visibilità
/// verificabile.
pub fn corso_label(n: u32) -> &'static str {
    match n {
        crate::spec::CORSO_MATEMATICA => "matematica-seconda",
        crate::spec::CORSO_INFORMATICA => "informatica-terza",
        _ => "corso-sconosciuto",
    }
}

/// L'artifact HTML dell'item, come byte.
pub fn artifact(s: &Spec) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"it\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<title>");
    out.push_str(&esc(s.titolo));
    out.push_str("</title>\n");
    out.push_str(&format!(
        "<meta name=\"kb-argument\" content=\"{}\">\n",
        esc(s.rel)
    ));
    out.push_str(&format!(
        "<meta name=\"kb-course\" content=\"{}\">\n",
        corso_label(s.corso)
    ));
    out.push_str(&format!(
        "<meta name=\"kb-family\" content=\"{}\">\n",
        s.famiglia.slug()
    ));
    out.push_str(&format!(
        "<meta name=\"kb-state\" content=\"{}\">\n",
        stato_label(s.stato)
    ));
    out.push_str(&format!(
        "<meta name=\"kb-origin\" content=\"{}\">\n",
        s.origine.label()
    ));
    // D10: l'origine `generated` porta il suo lock, `derived` porta il suo
    // sorgente. È l'unico posto in cui il lock entra in un artifact, e quindi
    // l'unico posto in cui può mancare: una tabella che dichiara una
    // generazione e una resa che non ne scrive il record è un diario.
    meta_di_origine(s, &mut out);

    if let Some(u) = s.riferimento_esterno {
        // L'attributo dichiara il riferimento *anche se l'artifact non lo usa*:
        // è una dichiarazione di dipendenza, e dichiararla è il primo passo per
        // non accorgersi che il runtime è fuori dal perimetro.
        out.push_str(&format!(
            "<meta name=\"kb-external-runtime\" content=\"{}\">\n",
            esc(u)
        ));
    }
    out.push_str("</head>\n<body>\n<main>\n");
    out.push_str(&format!("<h1 id=\"titolo\">{}</h1>\n", esc(s.titolo)));
    out.push_str(&format!(
        "<p class=\"riassunto\">{}</p>\n",
        esc(s.riassunto)
    ));
    if !s.prerequisiti.is_empty() {
        out.push_str("<ul class=\"prerequisiti\">\n");
        for p in s.prerequisiti {
            out.push_str(&format!("<li data-prereq=\"{}\">{}</li>\n", esc(p), esc(p)));
        }
        out.push_str("</ul>\n");
    }
    for p in s.paragrafi {
        out.push_str(&format!("<p>{}</p>\n", esc(p)));
    }
    // Le claim si dichiarano nella convenzione che `kbs-doc` legge: un elemento
    // con `data-claim` (il fatto atomico), `data-claim-id` (l'identità) e
    // `data-claim-span` (l'id dello span che la sostiene). I tre campi restano
    // separati perché lo span deve sopravvivere a un cambio di parola nella
    // claim: è la ragione per cui sono tre attributi e non uno.
    //
    // `data-stato` e `data-claim-emitter` sono la dichiarazione dell'autore, che
    // il lettore del formato non può ricavare e `kbs_intake` legge. Una claim
    // senza span viene dichiarata `unciteable` e senza `data-claim-span`: è la
    // forma che D6 vuole nel registro, e ometterla sarebbe cancellare l'errore
    // invece di registrarlo.
    for c in s.claims {
        out.push_str(&claim(c));
        out.push_str(&format!(
            "<p class=\"claim-nota\">Affermazione <code>{}</code>: {}</p>\n",
            esc(c.id),
            esc(c.testo)
        ));
    }
    out.push_str(&corpo_di_famiglia(s));
    if s.carica_three_locale || s.riferimento_esterno.is_some() {
        out.push_str("<div id=\"scena\" data-scene=\"");
        out.push_str(&esc(s.scena.map(|sc| sc.manifest).unwrap_or("")));
        out.push_str("\"></div>\n");
        let src = match s.riferimento_esterno {
            Some(u) => u,
            None => PERCORSO_THREE,
        };
        out.push_str(&format!(
            "<script type=\"module\" src=\"{}\"></script>\n",
            esc(src)
        ));
    }
    out.push_str("</main>\n");
    out.push_str(&format!(
        "<template id=\"{SLOT_CONTRATTO}\">\n",
    ));
    out.push_str(&s.contract().render());
    out.push_str("</template>\n");
    out.push_str("</body>\n</html>\n");
    out
}

/// Una claim dichiarata, come `kbs-doc` la legge.
///
/// La funzione è separata da `artifact` perché è l'unica forma in cui una
/// claim entra in un artifact: tenere insieme le due cose significa che il
/// giorno in cui la convenzione cambia il posto da cambiare è uno solo.
fn claim(c: &ClaimSpec) -> String {
    let emittente = match c.emittente {
        EmittenteSpec::Docente { by } => format!("teacher:person_{by:04}"),
        EmittenteSpec::Contenuto => "content".to_string(),
        EmittenteSpec::DaLavoro { osservazione } => format!("work:{osservazione}"),
    };
    let (ancora, testo_span) = match (c.ancora, c.testo_span) {
        (Some(a), Some(t)) => (Some(a), Some(t)),
        (None, None) => (None, None),
        // La tabella non può dichiarare un'ancora senza testo dello span:
        // `corpus.ogni_claim_con_span_ha_la_sua_ancora_nel_testo` lo rifiuta
        // prima, e qui il caso non è raggiungibile. Un `panic!` in un renderer
        // sarebbe una maniera costosa di dire la stessa cosa, quindi la
        // combinazione non viene resa e il controllo resta là dov'è.
        _ => return String::new(),
    };
    let mut out = String::from("<p class=\"claim\"");
    out.push_str(&format!(" data-claim=\"{}\"", esc(c.testo)));
    out.push_str(&format!(" data-claim-id=\"{}\"", esc(c.id)));
    if let Some(a) = ancora {
        out.push_str(&format!(" data-claim-span=\"{}\"", esc(a)));
    }
    out.push_str(&format!(" data-stato=\"{}\"", claim_stato(c.stato)));
    out.push_str(&format!(" data-claim-emitter=\"{}\"", esc(&emittente)));
    out.push('>');
    if let (Some(a), Some(t)) = (ancora, testo_span) {
        out.push_str(&format!("<span id=\"{}\">{}</span>", esc(a), esc(t)));
    }
    out.push_str("</p>\n");
    out
}

/// I `<meta>` che dichiarano il **model lock** di un item generato (D10).
///
/// Sono cinque, non quattro: ai quattro campi del lock si aggiunge
/// `kb-lock-at`, che è il momento della generazione. Senza i quattro campi non
/// c'è un record, c'è una promessa; e un record senza quando è successo non
/// distingue due generazioni dello stesso prompt sullo stesso corpus, che sono
/// due eventi diversi.
///
/// La funzione è pubblica e serve anche a `crate::corpus`, che deve costruire
/// lo stesso `ModelLock` che il file dichiara: due scritture della stessa cosa
/// in due posti sono due fonti, e le due fonti divergono alla prima ricorrenza.
pub fn lock_di(s: &Spec) -> kbs_core::ModelLock {
    let (model, prompt_hash, generator) = match s.origine {
        OriginSpec::Generated { model, prompt_hash, generator, .. } => (model, prompt_hash, generator),
        // Non succede: la si chiama solo su un item `generated`, e lo dice il
        // chiamante. Un `panic!` in un renderer sarebbe peggio che un valore
        // dichiaratamente vuoto, quindi il caso di qui è un lock che non nomina
        // nessuna generazione — che è esattamente ciò che non è.
        _ => ("", "", ""),
    };
    kbs_core::ModelLock {
        model_id: model.to_string(),
        prompt_hash: prompt_hash.to_string(),
        // L'hash del corpus **al momento della generazione**. Il banco non ha
        // una cronologia delle generazioni, quindi dichiara un hash derivato
        // dall'item e dall'epoca: è un valore che identifica una coppia
        // (corpus, momento) e non un'altra, che è ciò che D11 chiede a un lock.
        corpus_hash: crate::corpus::Corpus::hash_file(&format!("corpus-al-{}:{EPOCA}", s.rel)),
        generator_version: generator.to_string(),
        at: Millis(EPOCA),
    }
}


/// I `<meta>` dell'origine, nell'ordine in cui il documento li dichiara.
fn meta_di_origine(s: &Spec, out: &mut String) {
    match s.origine {
        OriginSpec::Generated { .. } => {
            let l = lock_di(s);
            out.push_str(&format!(
                "<meta name=\"kb-lock-model\" content=\"{}\">\n",
                esc(&l.model_id)
            ));
            out.push_str(&format!(
                "<meta name=\"kb-lock-prompt\" content=\"{}\">\n",
                esc(&l.prompt_hash)
            ));
            out.push_str(&format!(
                "<meta name=\"kb-lock-corpus\" content=\"{}\">\n",
                esc(&l.corpus_hash)
            ));
            out.push_str(&format!(
                "<meta name=\"kb-lock-generator\" content=\"{}\">\n",
                esc(&l.generator_version)
            ));
            out.push_str(&format!(
                "<meta name=\"kb-lock-at\" content=\"{}\">\n",
                l.at.0
            ));
        }
        OriginSpec::Derived { from } => {
            // `derived` senza il sorgente non è un'origine: è il caso che
            // `kbs_intake` chiama `derived-without-source`. Dichiarare il
            // sorgente è dichiarare da dove viene, ed è tutto ciò che
            // l'artefatto può dire.
            out.push_str(&format!(
                "<meta name=\"kb-derived-from\" content=\"{}\">\n",
                esc(from)
            ));
        }
        OriginSpec::Human { .. } => {}
    }
}

fn claim_stato(s: crate::spec::ClaimStatusKind) -> &'static str {
    match s {
        crate::spec::ClaimStatusKind::Supported => "supported",
        crate::spec::ClaimStatusKind::Contradicted => "contradicted",
        crate::spec::ClaimStatusKind::Unciteable => "unciteable",
        crate::spec::ClaimStatusKind::Retracted(_) => "retracted",
    }
}

/// Il corpo specifico della famiglia: è ciò che rende un item di `prove`
/// diverso da un item di `mazzi`. Senza questo, dodici famiglie sarebbero
/// dodici nomi per lo stesso file.
fn corpo_di_famiglia(s: &Spec) -> String {
    let mut out = String::new();
    match s.famiglia {
        Family::Letture => {
            out.push_str("<section class=\"citazione\">\n<p>Fonte primaria citata in questa unità.</p>\n</section>\n");
        }
        Family::Mappe => {
            out.push_str("<section class=\"mappa\">\n<h2 id=\"mappa\">La mappa</h2>\n");
            for (n, node) in s
                .scena
                .map(|sc| sc.nodi)
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                out.push_str(&format!(
                    "<p data-nodo=\"{}\" data-claim-id=\"{}\">{} {}{}</p>\n",
                    esc(node.id),
                    esc(node.claim),
                    n + 1,
                    esc(&node.etichetta),
                    if node.correggibile { " (correggibile)" } else { "" }
                ));
            }
            for e in s
                .scena
                .map(|sc| sc.archi)
                .unwrap_or_default()
                .iter()
            {
                out.push_str(&format!(
                    "<p data-arco=\"{}\" data-claim-id=\"{}\">{} — {} {}{}</p>\n",
                    esc(e.id),
                    esc(e.claim),
                    esc(e.da),
                    esc(e.relazione),
                    esc(e.a),
                    if e.correggibile { " (correggibile)" } else { "" }
                ));
            }
            out.push_str("</section>\n");
        }
        Family::VideoAudio => {
            out.push_str("<section class=\"trascrizione\">\n<h2 id=\"trascrizione\">Trascrizione</h2>\n");
            for (i, p) in s.paragrafi.iter().enumerate() {
                out.push_str(&format!(
                    "<p data-frammento=\"{}\" data-tempo=\"{:02}:{:02}\">{}</p>\n",
                    i + 1,
                    i * 4,
                    i * 30,
                    esc(p)
                ));
            }
            out.push_str("</section>\n");
        }
        Family::Esercizi => {
            out.push_str("<section class=\"esercizi\">\n");
            for e in s.esercizi {
                out.push_str(&format!(
                    "<div class=\"esercizio\" data-esercizio=\"{}\" data-famiglia=\"{}\" data-checker=\"{}\">\n<p>{}</p>\n",
                    esc(e.id),
                    esc(e.famiglia),
                    e.checker.label(),
                    esc(e.testo)
                ));
                for inst in e.istanze {
                    out.push_str(&format!(
                        "<p class=\"istanza\" data-seed=\"{}\">istanza {}, {}, {}</p>\n",
                        esc(inst.seed),
                        inst.a,
                        inst.b,
                        inst.c
                    ));
                }
                out.push_str("</div>\n");
            }
            if s.esercizi.is_empty() {
                out.push_str("<p class=\"senza-verificatore\">Nessun verificatore deterministico: l'esercizio non entra nel percorso di pubblicazione.</p>\n");
            }
            out.push_str("</section>\n");
        }
        Family::Scritture => {
            out.push_str("<section class=\"consegne\">\n<h2 id=\"consegne\">Consegne</h2>\n<ol><li>Leggi.</li><li>Scrivi.</li><li>Dichiara i limiti.</li></ol>\n</section>\n");
        }
        Family::Prove => {
            out.push_str("<table class=\"distribuzione\">\n<caption>Distribuzione delle risposte per item</caption>\n<thead><tr><th>Item</th><th>Corrette</th></tr></thead>\n<tbody>\n");
            for (i, pct) in [62u32, 71, 55, 78, 66].iter().enumerate() {
                out.push_str(&format!(
                    "<tr><td>item-{}</td><td>{}%</td></tr>\n",
                    i + 1,
                    pct
                ));
            }
            out.push_str("</tbody>\n</table>\n");
        }
        Family::Dialoghi => {
            out.push_str("<section class=\"scambi\">\n");
            for (i, q) in s.paragrafi.iter().enumerate() {
                out.push_str(&format!(
                    "<blockquote data-ancoraggio=\"riga\" data-scope=\"{}\"><p>{}</p></blockquote>\n",
                    slug_claim(&format!("cl_{:02}_q", i + 1)),
                    esc(q)
                ));
            }
            out.push_str("</section>\n");
        }
        Family::Appunti => {
            out.push_str("<table class=\"schema\">\n<tbody>\n<tr><th>Definizione</th><td></td></tr>\n<tr><th>Esempio</th><td></td></tr>\n<tr><th>Controesempio</th><td></td></tr>\n<tr><th>Domanda aperta</th><td></td></tr>\n</tbody>\n</table>\n");
        }
        Family::Laboratori => {
            out.push_str("<section class=\"foglio\">\n<h2 id=\"foglio\">Foglio di calcolo</h2>\n<table><tbody>\n");
            for (i, r) in s.paragrafi.iter().enumerate() {
                out.push_str(&format!(
                    "<tr><td>riga {}</td><td>{}</td></tr>\n",
                    i + 1,
                    esc(r)
                ));
            }
            out.push_str("</tbody></table>\n</section>\n");
        }
        Family::Rubrica => {
            out.push_str("<table class=\"rubrica\">\n<thead><tr><th>Descrittore</th><th>Livello 1</th><th>Livello 2</th></tr></thead>\n<tbody>\n");
            for d in ["coerenza", "prove", "limiti", "chiarezza"] {
                out.push_str(&format!(
                    "<tr data-descrittore=\"{d}\"><td>{d}</td><td></td><td></td></tr>\n"
                ));
            }
            out.push_str("</tbody>\n</table>\n");
        }
        Family::Mazzi => {
            out.push_str("<section class=\"carte\">\n");
            for i in 1..=22 {
                out.push_str(&format!(
                    "<article class=\"carta\" data-carta=\"{i}\"><p>fronte</p><p>retro</p></article>\n"
                ));
            }
            out.push_str("</section>\n");
        }
        Family::Produzioni => {
            out.push_str("<section class=\"produzione\">\n<h2 id=\"produzione\">La produzione</h2>\n<ol><li>Registra.</li><li>Raccogli la domanda di ritorno.</li><li>Risppondi.</li></ol>\n</section>\n");
        }
    }
    out
}

/// Il manifest di una scena, in JSON.
///
/// D15.1.4: **il rendering è un effetto, il contenuto è il dato**. I dati
/// stanno qui, in un file che il codice di rendering non tocca: si può
/// re-renderizzare, cambiare camera e luce, e questo file non cambia. Se i
/// dati stessero nel codice, un aggiornamento del renderer cancellerebbe il
/// materiale dello studente.
pub fn manifest(s: &Spec) -> String {
    let sc = s.scena.expect("il manifest si chiede solo a un item con scena");
    let mut m = Map::new();
    m.insert("titolo".into(), json!(sc.titolo));
    m.insert("argomento".into(), json!(s.rel));
    m.insert(
        "runtime".into(),
        json!(match s.riferimento_esterno {
            Some(u) => u,
            None => PERCORSO_THREE,
        }),
    );
    m.insert(
        "nodi".into(),
        Value::Array(
            sc.nodi
                .iter()
                .map(|n| {
                    json!({
                        "id": n.id,
                        "etichetta": n.etichetta,
                        "claim": n.claim,
                        "correggibile": n.correggibile,
                        "posizione": [n.x, n.y, n.z],
                    })
                })
                .collect(),
        ),
    );
    m.insert(
        "archi".into(),
        Value::Array(
            sc.archi
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "da": e.da,
                        "a": e.a,
                        "relazione": e.relazione,
                        "claim": e.claim,
                        "correggibile": e.correggibile,
                    })
                })
                .collect(),
        ),
    );
    let mut s2 = serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default();
    s2.push('\n');
    s2
}

/// Il nome del file di una scena, relativo alla radice del corpus.
pub fn percorso_manifest(s: &Spec) -> String {
    s.scena
        .map(|sc| sc.manifest.to_string())
        .unwrap_or_default()
}

/// I parametri di un'istanza, in JSON: sono ciò che il generatore deve
/// riprodurre byte per byte perché il replay sia deterministico (D11).
pub fn parametri(inst: &InstanceSpec) -> Value {
    json!({ "a": inst.a, "b": inst.b, "c": inst.c })
}

/// L'id dell'argomento che ospita la scena, per il referto.
pub fn claim_di(nodo: &SceneNode) -> &str {
    nodo.claim
}

/// L'id dell'argomento che ospita l'arco, per il referto.
pub fn claim_di_arco(arco: &SceneEdge) -> &str {
    arco.claim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_epoca_del_banco_e_fissa() {
        // 2026-09-26T00:00:00Z. Se l'epoca cambia, cambiano tutti gli hash dei
        // fixture e il banco smette di essere confrontabile con i referti
        // precedenti: va deciso, non trascorso.
        assert_eq!(EPOCA, 1_790_380_800_000);
        assert!(creato_a(0) < creato_a(1));
        assert!(aggiornato_a(3) > creato_a(3));
        assert!(ratificato_a(3) > aggiornato_a(3));
    }
}
