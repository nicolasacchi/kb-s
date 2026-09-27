//! Il README non può vantare una capacità che nessun test dimostra.
//!
//! Il problema dei README è che le frasi si aggiungono e non si tolgono: un
//! progetto scrive «supporta la ratifica», poi la feature sparisce, e la frase
//! resta. Qui la superficie delle dichiarazioni è **una tabella delimitata da
//! due marcatori**, e ogni riga è vincolata a un nome di test che deve esistere
//! davvero nel workspace.
//!
//! Le regole sono cinque e sono tutte verificabili da una riga di codice:
//!
//! 1. la tabella esiste, ha almeno dodici righe e almeno dieci sono
//!    `dimostrata`;
//! 2. ogni riga dichiara un riferimento non vuoto;
//! 3. un riferimento a un test di Cargo deve corrispondere a una funzione
//!    `fn` che esiste in qualche crate del workspace;
//! 4. un riferimento a un controllo del banco deve corrispondere a un controllo
//!    che il banco esegue davvero, e se lo stato è `dimostrata` quel controllo
//!    non può essere saltato in questa esecuzione;
//! 5. fuori dalla tabella non c'è un'altra tabella di capacità: le tabelle
//!    consentite fuori sono quelle delle sezioni che si chiamano «Non
//!    costruito» e della tabella dei confini, che è un elenco di cose che
//!    `kb-s` **non** è.
//!
//! La regola 4 è quella che rende il README onesto: un controllo saltato non
//! può essere dichiarato come dimostrato, e se un giorno la pipeline esisterà
//! questo test comincerà a pretendere che quei controlli siano verdi.

use kbs_fixtures::adapter::PipelineAssente;
use kbs_fixtures::checks::{Banco, Config, Esito};
use std::path::{Path, PathBuf};

const INIZIO: &str = "<!-- capacità: inizio -->";
const FINE: &str = "<!-- capacità: fine -->";

fn readme() -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("README.md");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Le righe della tabella delle capacità, come (capacità, riferimento, stato).
fn righe() -> Vec<(String, String, String)> {
    let testo = readme();
    let (_prima, b) = testo
        .split_once(INIZIO)
        .unwrap_or_else(|| panic!("il README non ha il marcatore {INIZIO}"));
    let (corpo, _) = b
        .split_once(FINE)
        .unwrap_or_else(|| panic!("il README non ha il marcatore {FINE}"));
    let mut out = Vec::new();
    for riga in corpo.lines().filter(|l| l.trim_start().starts_with('|')) {
        let celle: Vec<String> = riga
            .split('|')
            .map(str::trim)
            .filter(|c| !c.is_empty() && !c.chars().all(|ch| ch == '-'))
            .map(str::to_string)
            .collect();
        if celle.first().map(String::as_str) == Some("capacità") {
            continue;
        }
        if cells_len(&celle) != 3 {
            continue;
        }
        out.push((celle[0].clone(), celle[1].clone(), celle[2].clone()));
    }
    out
}

fn cells_len(c: &[String]) -> usize {
    c.len()
}

/// Il testo di tutti i sorgenti Rust del workspace, per cercare i nomi dei
/// test. Non è un indice: è una ricerca di `fn <nome>`, che è esattamente ciò
/// che «questo test esiste» vuol dire.
fn sorgenti_workspace() -> Vec<(String, String)> {
    let radice = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let crates = radice.join("crates");
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&crates) else { return out };
    for crate_ in rd.flatten() {
        let base = crate_.path();
        raccogli_rust(&base, &mut out);
    }
    out
}

fn raccogli_rust(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut voci: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    voci.sort();
    for p in voci {
        if p.is_dir() {
            if p.file_name().map(|n| n == "corpus" || n == "target").unwrap_or(false) {
                continue;
            }
            raccogli_rust(&p, out);
        } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
            if let Ok(t) = std::fs::read_to_string(&p) {
                out.push((p.to_string_lossy().to_string(), t));
            }
        }
    }
}

/// I nomi dei controlli che il banco esegue davvero, in questa esecuzione.
fn controlli_del_banco() -> Vec<(String, Esito)> {
    let p = PipelineAssente { ragione: "binario assente".into() };
    Banco::new(Config::radice_di_default(), &p)
        .esegui()
        .controlli
        .into_iter()
        .map(|c| (c.nome.to_string(), c.esito))
        .collect()
}

fn esiste_il_test(nome: &str) -> bool {
    let ago = format!("fn {nome}(");
    sorgenti_workspace()
        .iter()
        .any(|(_, t)| t.contains(&ago))
}

/// Il test che rende il README verificabile. Se una capacità aggiunta al
/// README non porta un test che esiste, questo test fallisce.
#[test]
fn ogni_capacita_dichiara_il_test_che_la_demonstra() {
    let righe = righe();
    assert!(
        righe.len() >= 12,
        "la tabella delle capacità ha {} righe: sono poche per un progetto con quindici decisioni",
        righe.len()
    );
    let dimostrate = righe.iter().filter(|(_, _, s)| s.starts_with("dimostrata")).count();
    assert!(
        dimostrate >= 10,
        "solo {dimostrate} capacità dichiarate dimostrate: il README deve mostrare anche ciò che non lo è"
    );

    let controlli = controlli_del_banco();

    for (cap, riferimento, stato) in &righe {
        // Regola 2: nessuna riga senza riferimento.
        assert!(!riferimento.trim().is_empty(), "{cap}: riga senza riferimento");
        assert!(!stato.trim().is_empty(), "{cap}: riga senza stato");

        let rif = riferimento.trim_matches('`');
        let nomi: Vec<String> = rif.split("::").map(str::to_string).collect();
        let ultimo = nomi.last().cloned().unwrap_or_default();

        if ultimo.contains('.') {
            // Regola 4: è un controllo del banco.
            let esito = controlli
                .iter()
                .find(|(n, _)| *n == ultimo)
                .map(|(_, e)| e.clone());
            match esito {
                None => panic!(
                    "{cap}: il riferimento «{ultimo}» non è un controllo che il banco esegue"
                ),
                Some(Esito::Saltato(why)) => {
                    assert!(
                        !stato.starts_with("dimostrata"),
                        "{cap}: dichiarata «{stato}» ma il controllo {ultimo} è saltato ({why})"
                    );
                    assert!(
                        stato.contains("non dimostrata"),
                        "{cap}: stato «{stato}» non dice perché la capacità non è dimostrata"
                    );
                }
                Some(Esito::Fallito(p)) => {
                    panic!("{cap}: il controllo {ultimo} è rosso: {p:?}")
                }
                Some(Esito::Superato) => {
                    assert!(
                        !stato.contains("non dimostrata"),
                        "{cap}: il controllo {ultimo} è superato e la riga lo dichiara «{stato}»"
                    );
                }
            }
            continue;
        }

        if ultimo.is_empty() || ultimo == "—" {
            // Regola 3 variante: un trattino è un riferimento dichiarato
            // vuoto, e va accettato **solo** per le righe non dimostrate.
            assert!(
                !stato.starts_with("dimostrata"),
                "{cap}: dichiarata «{stato}» senza alcun riferimento"
            );
            continue;
        }

        // Regola 3: il riferimento è un test di Cargo, e deve esistere.
        assert!(
            esiste_il_test(&ultimo),
            "{cap}: il test «{ultimo}» non esiste in nessun crate del workspace"
        );
        if !stato.starts_with("dimostrata") {
            assert!(
                stato.contains("non dimostrata"),
                "{cap}: stato «{stato}» non dice perché la capacità non è dimostrata"
            );
        }
    }
}

/// Regola 5: fuori dalla tabella delle capacità non c'è un'altra tabella che
/// venda capacità. Le tabelle consentite sono quelle dei confini («non è») e
/// quelle delle sezioni che si chiamano «Non costruito».
#[test]
fn la_superficie_delle_dichiarazioni_e_una_sola() {
    let testo = readme();
    let (fuori, dentro) = testo
        .split_once(INIZIO)
        .expect("marcatore iniziale");
    let (_dentro, _) = dentro.split_once(FINE).expect("marcatore finale");

    let titoli: Vec<&str> = testo
        .lines()
        .filter(|l| l.starts_with("## "))
        .map(|l| l.trim_start_matches("## ").trim())
        .collect();
    assert!(
        titoli.iter().any(|t| t.to_lowercase().contains("capacit")),
        "il README non ha una sezione sulle capacità: una tabella senza intestazione non dichiara niente"
    );
    let righe_fuori: Vec<&str> = fuori
        .lines()
        .filter(|l| l.trim_start().starts_with('|'))
        .collect();
    // Nessuna delle righe fuori dalla tabella può essere una dichiarazione di
    // capacità: la colonna `dimostrata da` esiste solo dentro la tabella.
    for testo_riga in &righe_fuori {
        assert!(
            !testo_riga.contains("dimostrata da"),
            "fuori dalla tabella delle capacità c'è una riga che si dichiara dimostrata: {testo_riga}"
        );
    }
}
