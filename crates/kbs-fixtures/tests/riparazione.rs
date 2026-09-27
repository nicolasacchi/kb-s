//! Un banco che diventa verde quando qualcuno sistema una fixture rotta è un
//! banco che ha smesso di verificare.
//!
//! Qui si simula la riparazione più naturale: qualcuno trova un item che non
//! passa la validazione, ne corregge il file a mano — riga nuova, carattere
//! nuovo — e lo committa. Il banco deve diventare rosso e **nominare il file**.
//!
//! La simulazione è su una copia in una cartella temporanea: il corpus
//! committato resta com'era, e il test non lascia nulla dietro.

use kbs_fixtures::adapter::PipelineAssente;
use kbs_fixtures::checks::{Banco, Config, Esito};
use kbs_fixtures::corpus::Corpus;
use kbs_fixtures::report;
use std::path::{Path, PathBuf};

const CDN: &str = "https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.min.js";
/// La versione locale, che è ciò che chiunque riparerebbe la fixture.
const LOCALE: &str = "/three/three.module.js";

/// Copia il corpus in una cartella temporanea e ne restituisce la radice.
fn copia_in_temporanea(nome: &str) -> PathBuf {
    let radice = std::env::temp_dir().join(format!("kbs-bench-{nome}"));
    let _ = std::fs::remove_dir_all(&radice);
    std::fs::create_dir_all(&radice).expect("cartella temporanea");
    for f in Corpus::dalla_tabella().file() {
        let p = radice.join(&f.rel);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).expect("sottocartella");
        }
        std::fs::write(&p, &f.contenuto).expect("fixture copiata");
    }
    radice
}

fn referto(radice: &Path) -> kbs_fixtures::Referto {
    let p = PipelineAssente { ragione: "binario assente".into() };
    Banco::new(Config::con_radice(radice), &p).esegui()
}

fn ragioni(r: &kbs_fixtures::Referto, controllo: &str) -> Vec<String> {
    match r.controllo(controllo).map(|c| &c.esito) {
        Some(Esito::Fallito(p)) => p.clone(),
        altro => panic!("{controllo}: atteso fallimento, trovato {}", altro
            .map(|e| e.etichetta())
            .unwrap_or("(assente)")),
    }
}

/// Il caso normale: il corpus committato è verde su tutti i controlli che non
/// hanno bisogno della pipeline. Se questo test fallisce, il problema è nel
/// banco o nelle fixture, e va guardato per primo.
#[test]
fn il_corpus_committato_e_verde_su_tutti_i_controlli_su_corpus() {
    let radice = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let r = referto(&radice);
    let falliti: Vec<&str> = r
        .controlli
        .iter()
        .filter(|c| !c.di_pipeline && c.esito != Esito::Superato)
        .map(|c| c.nome)
        .collect();
    assert!(
        falliti.is_empty(),
        "controlli su corpus non superati: {falliti:?}\n{}",
        report::in_testo(&r, false)
    );
}

/// **Il test che rende il banco onesto.** Si ripara a mano la fixture che
/// referenzia la CDN, e il banco deve:
///
/// 1. accorgersi che il file non è più quello che la tabella descrive, e
/// 2. dire **quale** file è stato modificato.
#[test]
fn il_banco_diventa_rosso_quando_una_fiastra_rotta_viene_riparata() {
    let radice = copia_in_temporanea("riparazione");
    let bersaglio = radice.join("mappe/05-scena-3d-da-cdn.html");
    let prima = std::fs::read_to_string(&bersaglio).expect("fixture presente");
    assert!(prima.contains(CDN), "la fixture non conteneva la CDN: il test non prova niente");

    // La riparazione: dalla CDN al percorso servito dal binario. È la
    // correzione giusta dal punto di vista di chi la fa, e proprio per questo
    // deve rompere il banco: la tabella dice ancora che quell'item è rotto.
    let dopo = prima.replace(CDN, LOCALE);
    assert_ne!(prima, dopo, "la riparazione non ha cambiato nulla");
    std::fs::write(&bersaglio, &dopo).expect("fixture riparata");

    let r = referto(&radice);
    let ragioni = ragioni(&r, "corpus.i_file_sono_uguali_a_cio_che_la_tabella_descrive");
    assert!(
        ragioni.iter().any(|x| x.contains("mappe/05-scena-3d-da-cdn.html")),
        "il banco è rosso ma non nomina il file riparato: {ragioni:?}"
    );
    assert!(!r.ok(), "il banco deve essere rosso dopo una riparazione");
    let testo = report::in_testo(&r, false);
    assert!(testo.contains("esito: FALLITO"));
    assert!(testo.contains("RED   corpus.i_file_sono_uguali_a_cio_che_la_tabella_descrive"));

    let _ = std::fs::remove_dir_all(&radice);
}

/// ## Perché qui non c'è un test sulla «seconda via di riparare»
///
/// Un tempo questo file aveva `il_banco_diventa_rosso_quando_una_fiastra_
/// rotta_viene_dichiarata_sana`, e il test era falso in due modi insieme: la
/// sua documentazione prometteva che «le rotte sono quattro» e il suo corpo non
/// cambiava niente, e non asseriva mai un fallimento. Un test che mente sul
/// proprio nome è peggio di nessun test, perché qualcuno ci crederà: legge
/// «il banco diventa rosso», e il banco resta verde.
///
/// La premessa, poi, non è esprimibile a runtime. «Dichiarare sana una fixture
/// rotta» vuol dire cambiare `Spec::difetto` da `Defect::…` a `Defect::Nessuno`,
/// e la tabella è un `const` in compilazione: cambiarla è un atto di build, non
/// una mossa che un test può fare su una copia temporanea del corpus. La copia
/// temporanea contiene i **file**, e i file sono la metà che il test sorella
/// [`il_banco_diventa_rosso_quando_una_fiastra_rotta_viene_riparata`] già
/// esercita.
///
/// Quello che resta scoperto, e resta scoperto finché non si scrive: **che il
/// numero di fixture rotte sia un'aspettativa che il banco sorveglia.** Oggi il
/// banco lo dice in un solo modo, ed è dichiarato qui perché è un buco e non un
/// dettaglio: `FIXTURE_ROTTE` è una costante che `copertura.rs` confronta con il
/// conteggio reale di `Spec::deve_essere_rifiutato`, e quel confronto gira
/// **sulla tabella compilata**. Se qualcuno sistema una fixture rotta e cambia
/// la tabella, il controllo `corpus.le_cinque_fiastre_rovate_sono_ancora_rovate`
/// misura i byte del file e si rifiuta di accettare un file sano — quindi
/// l'errore **è** visto. Ciò che non è visto è il caso opposto: una fixture
/// dichiarata sana che nessuno ha riparato. Per coprirlo serve una seconda
/// tabella, o un banco che accetti la tabella come argomento; nessuna delle due
/// cose esiste, e inventarle qui sarebbe un'altra fonte di verità.

/// Il caso inverso: **aggiungere** un file al corpus che nessuno ha descritto
/// è rumore, e il banco lo dice invece di ignorarlo. Un file che non è nella
/// tabella è un file che nessuno può più spiegare.
#[test]
fn un_file_che_nessuno_ha_descritto_viene_detto_nel_referto() {
    let radice = copia_in_temporanea("file-ignoto");
    std::fs::write(radice.join("leonardo.txt"), "maestro\n").expect("file fuori tabella");
    let r = referto(&radice);
    assert!(
        r.file_ignoti.iter().any(|f| f == "leonardo.txt"),
        "il referto non nomina il file non atteso: {:?}",
        r.file_ignoti
    );
    let testo = report::in_testo(&r, false);
    assert!(testo.contains("file non attesi nella radice del corpus"));
    let _ = std::fs::remove_dir_all(&radice);
}
