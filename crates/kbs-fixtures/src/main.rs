//! `kbs-bench` — il comando che gira il banco e dice che cosa ha verificato.
//!
//! Due verbi, e sono due perché sono due domande diverse:
//!
//! * `kbs-bench run` — gira il banco e stampa il referto. È il comando che la
//!   CI esegue.
//! * `kbs-bench emit` — riscrive la cartella `corpus` a partire dalla tabella.
//!   È il modo in cui i fixture sono riproducibili: nessuno li scrive a mano, e
//!   un file che non è nella tabella non viene generato.
//!
//! Il codice di uscita è 0 solo se il banco è verde. Con `--require-pipeline`
//! — che è il modo in cui gira la CI — un controllo saltato è un fallimento:
//! in CI non si accetta che un banco abbia deciso di non verificare qualcosa.

use kbs_fixtures::adapter::{Pipeline, PipelineAssente, ProcessPipeline};
use kbs_fixtures::checks::{Banco, Config};
use kbs_fixtures::corpus::Corpus;
use kbs_fixtures::report;
use std::path::PathBuf;
use std::process::ExitCode;

const AIUTO: &str = "\
kbs-bench — il banco di prova di kb-s

USO:
    kbs-bench run [opzioni]        gira il banco e stampa il referto
    kbs-bench emit <cartella>      riscrive il corpus a partire dalla tabella

OPZIONI DI `run`:
    --corpus <cartella>    la radice del corpus (default: crates/kbs-fixtures/corpus)
    --kbs <binario>        il binario della pipeline (default: KBS_BIN, poi target/debug/kbs)
    --json                 stampa il referto in JSON invece che in testo
    --require-pipeline     un controllo saltato è un fallimento: è la regola della CI

CONTRATTO CON LA PIPELINE:
    kbs-bench esegue una SEQUENZA di atti, non un comando solo:

        kbs verify  --json --db <percorso> <radice>
        kbs promote --person <id> --arg <rel> --path <file> --db <percorso> <radice>

    «verify» legge da stdout un JSON con i campi items, index, claims e
    instances. «promote» risponde e può rifiutare: un rifiuto è un verdetto,
    non un errore. La sequenza serve perché la citabilità di D4 non si vede da
    una cartella — una pipeline che indicizza non firma niente — e senza
    «promote» il banco osserverebbe solo un indice vuoto. La forma esatta è
    documentata in kbs_fixtures::adapter. Se il binario non esiste o non parla
    come previsto, i controlli che lo richiedono sono SALTATI con la ragione.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("emit") => emit(&args[1..]),
        Some("--help" | "-h" | "help") | None => {
            print!("{AIUTO}");
            ExitCode::SUCCESS
        }
        Some(altro) => {
            eprintln!(" verbo sconosciuto: {altro}\n");
            print!("{AIUTO}");
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], nome: &str) -> bool {
    args.iter().any(|a| a == nome)
}

fn valore<'a>(args: &'a [String], nome: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == nome)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn run(args: &[String]) -> ExitCode {
    let require = flag(args, "--require-pipeline");
    let json = flag(args, "--json");
    let cfg = match valore(args, "--corpus") {
        Some(p) => Config::con_radice(PathBuf::from(p)),
        None => Config::radice_di_default(),
    }
    .con_rigidezza(require);

    // La pipeline si cerca una volta sola: se non c'è, i controlli che la
    // richiedono sono saltati **tutti** con la stessa ragione, e la ragione
    // dice che cosa cercare.
    let (pipeline, assente): (Box<dyn Pipeline>, Option<String>) =
        match valore(args, "--kbs") {
            Some(p) => {
                let path = PathBuf::from(p);
                if path.exists() {
                    (Box::new(ProcessPipeline::su(path)), None)
                } else {
                    let r = format!("il binario «{p}» non è stato trovato");
                    (Box::new(PipelineAssente { ragione: r.clone() }), Some(r))
                }
            }
            None => match ProcessPipeline::cerca() {
                Ok(p) => (Box::new(p), None),
                Err(e) => {
                    let r = e.to_string();
                    (Box::new(PipelineAssente { ragione: r.clone() }), Some(r))
                }
            },
        };
    if let Some(r) = assente {
        eprintln!("kbs-bench: pipeline non raggiungibile — {r}");
        eprintln!("kbs-bench: i controlli che la richiedono sono SALTATI, non superati.");
    }

    let banco = Banco::new(cfg, pipeline.as_ref());
    let referto = banco.esegui();
    if json {
        print!("{}", report::in_json(&referto, require));
    } else {
        print!("{}", report::in_testo(&referto, require));
    }
    if referto.esito_con_rigidezza(require) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn emit(args: &[String]) -> ExitCode {
    let radice = match args.first() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("kbs-bench emit: manca la cartella di destinazione");
            return ExitCode::FAILURE;
        }
    };
    let corpus = Corpus::dalla_tabella();
    let attesi: std::collections::BTreeSet<&str> =
        corpus.file().iter().map(|f| f.rel.as_str()).collect();
    let mut scritti = 0usize;
    let mut rimossi: Vec<String> = Vec::new();
    for f in corpus.file() {
        let path = radice.join(&f.rel);
        if let Some(dir) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("kbs-bench emit: {} — {e}", dir.display());
                return ExitCode::FAILURE;
            }
        }
        match std::fs::write(&path, &f.contenuto) {
            Ok(()) => scritti += 1,
            Err(e) => {
                eprintln!("kbs-bench emit: {} — {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    // Un file che la tabella non descrive è un file che nessuno può più
    // spiegare. Si rimuove, e si dice: la rimozione è limitata a ciò che il
    // banco non si aspetta e che sta sotto la radice indicata.
    let mut visitati = Vec::new();
    if radice.is_dir() {
        raccogli(&radice, &radice, &mut visitati);
    }
    visitati.sort();
    for rel in visitati {
        if attesi.contains(rel.as_str()) || rel.starts_with('.') {
            continue;
        }
        let path = radice.join(&rel);
        if std::fs::remove_file(&path).is_ok() {
            rimossi.push(rel);
        }
    }
    println!(
        "kbs-bench emit: {scritti} file scritti in {}, {} rimossi",
        radice.display(),
        rimossi.len()
    );
    for r in &rimossi {
        println!("  - {r}");
    }
    println!("hash del corpus: {}", corpus.hash());
    ExitCode::SUCCESS
}

/// Raccoglie i percorsi relativi dei file sotto `dir`, con `base` come radice
/// del confronto. Senza `base` la ricorsione perderebbe il riferimento e
/// restituirebbe percorsi relativi alla sottodirectory.
fn raccogli(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut voci: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    voci.sort();
    for p in voci {
        if p.is_dir() {
            raccogli(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

