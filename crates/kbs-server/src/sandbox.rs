//! L'artifact su un'origine sua, e perché il `sandbox` non è ciò che lo
//! protegge.
//!
//! # La cosa che va detta prima di tutto il resto
//!
//! `kb` serve ogni artifact sulla **sua** origine — `<id>.artifacts.<suffisso>`,
//! con la rotta scelta dal header `Host` — dentro un `<iframe>` il cui
//! `sandbox` contiene `allow-scripts allow-same-origin allow-popups
//! allow-popups-to-escape-sandbox allow-forms allow-modals allow-downloads`
//! (`kb/crates/kb-core/src/iframe.rs:28-30`).
//!
//! **`allow-same-origin` è sicuro solo perché l'origine è diversa.** Se i due
//! artifact fossero serviti dalla stessa origine dell'interfaccia, quell'`iframe`
//! avrebbe accesso al DOM e a `localStorage` della pagina che lo contiene, e
//! `allow-scripts` farebbe il resto: un artifact è HTML **scritto dall'esterno**
//! — lo genera un modello che sta fuori dal prodotto (D3) e lo ratifica un
//! docente. È codice che gira con i permessi della pagina che lo ospita.
//!
//! Quindi: **non toccare il suffisso.** Se un domani qualcuno «semplifica»
//! servendo gli artifact da `/a/<id>` sulla stessa origine, non sta togliendo
//! una formalità: sta togliendo l'unica cosa che rende sicuro quel flag. Il
//! flag è una difesa in più; l'origine è la difesa.
//!
//! # Perché l'id dell'argomento funziona come etichetta di host
//!
//! In `kb` l'etichetta è un hash a 12 caratteri del percorso e il file si
//! risolve per nome di file. Qui l'etichetta è un `ArgumentId`
//! (`arg_<16 esadecimali>`), che è un'etichetta DNS valida, e il file si
//! risolve dal **database**, non dall'URL. La differenza non è di stile:
//!
//! * l'URL nomina un **identificatore**, mai un percorso, quindi da solo non
//!   può traversare il filesystem;
//! * risolvere l'id richiede [`kbs_store::Store::read_argument`], quindi
//!   **la capacità di leggerlo**, quindi il predicato D5. Una bozza servita
//!   come file statico sarebbe D5 annullato da una riga di routing, e il
//!   mount del corpus tornerebbe a essere l'ACL — esattamente il difetto che
//!   `kb` ha e che `kb-s` corregge.
//!
//! # I figli di un artifact
//!
//! Un artifact è una pagina: ha figli (`style.css`, immagini, pagine
//! sorelle). Servono, ma **solo quelli che stanno sotto il suo percorso**:
//! `<corpus>/<rel_path>/…`, mai `<corpus>/<directory di rel_path>/…`. La
//! differenza è la sorella unpublished: un artifact che uno studente può
//! leggere non deve poter prendere il file di una bozza che sta nella stessa
//! cartella. È l'unico posto in cui un artifact potrebbe sfuggire al
//! predicato senza passare da una rotta di lettura, quindi è chiuso.

use std::path::{Path, PathBuf};

use axum::http::HeaderValue;

use kbs_core::ArgumentId;

/// I flag del `sandbox`, **identici a quelli di `kb`**, carattere per
/// carattere.
///
/// Se la lista cambiasse, due cose vere insieme diventerebbero una sola: la
/// protezione che il flag dà e il costo che ha. `allow-same-origin` è nel
/// elenco perché senza `localStorage` dentro l'artifact lancia
/// `SecurityError` e l'artifact è mezzo morto; è accettabile **solo** perché
/// l'origine è quella dell'artifact e non quella dell'interfaccia. Vedi la
/// nota in cima al modulo.
///
/// Il formato è lo spazio singolo, perché è quello che `iframe.rs` dichiara e
/// che il test di kb pinssa: due spazi sembrano la stessa lista e un parser
/// rigido fa la differenza.
pub const SANDBOX_FLAGS: &str = "allow-scripts allow-same-origin allow-popups \
                                 allow-popups-to-escape-sandbox allow-forms \
                                 allow-modals allow-downloads";

/// Il suffisso degli host artifact in sviluppo. Stesso valore di `kb`.
pub const DEFAULT_HOST_SUFFIX: &str = ".artifacts.localhost";

/// L'etichetta dell'host di un artifact.
///
/// Non accetta nient'altro: l'etichetta è un `ArgumentId` e un `ArgumentId` ha
/// una forma sola ([`crate::ids`]). Accettare anche un nome di file significherebbe
/// avere due modi di dire la stessa cosa, e il secondo sarebbe quello che
/// qualcuno userebbe per evitare una lista.
pub fn artifact_host(argument: &ArgumentId, suffix: &str) -> String {
    format!("{}{}", argument.as_str(), suffix)
}

/// L'etichetta dell'host di un artifact, se l'host la contiene.
///
/// La grammatica è quella di `kb` (`kb/crates/kb-core/src/iframe.rs:53-76`) e
/// non un caso simile: `[a-z0-9._-]+`, niente etichetta vuota, niente etichetta
/// che comincia o finisce con un punto, niente `..`. Quelle quattro righe non
/// sono pignoleria, sono il controllo che impedisce a `Host:` di diventare un
/// percorso.
///
/// * `a.b.artifacts.localhost` → `Some("a.b")`
/// * `artifacts.localhost` → `None` (etichetta vuota)
/// * `..artifacts.localhost` → `None` (inizia per punto)
/// * `a..b.artifacts.localhost` → `None` (contiene `..`)
/// * `a/b.artifacts.localhost` → `None` (non è un'etichetta)
/// * `localhost` → `None` (nessun suffisso: è l'host dell'interfaccia)
pub fn parse_artifact_id<'h>(host: &'h str, suffix: &str) -> Option<&'h str> {
    // La porta non fa parte dell'identità: `Host: x.artifacts.localhost:4000`
    // e `Host: x.artifacts.localhost` sono lo stesso artifact.
    let senza_porta = host.split(':').next().unwrap_or(host);
    let etichetta = senza_porta.strip_suffix(suffix)?;
    if etichetta.is_empty() || etichetta.starts_with('.') || etichetta.ends_with('.') {
        return None;
    }
    if etichetta.contains("..") {
        return None;
    }
    if !etichetta
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return None;
    }
    Some(etichetta)
}

/// L'origine come la vede il browser, data un header `Host:`.
///
/// Lo schema è fisso a `https` **perché qui serve solo a confrontare origini
/// fra loro**, e in quel confronto lo schema è uguale per tutte. Quello che
/// distingue è l'host, ed è l'host che questo modulo promette di tenere
/// diverso.
pub fn origin_of(host: &str) -> String {
    let senza_porta = host.split(':').next().unwrap_or(host);
    format!("https://{senza_porta}")
}

/// Il valore di `Content-Security-Policy: frame-ancestors`, se c'è.
///
/// `None` quando l'origine del genitore non è configurata, e `None` vuol dire
/// «nessun header», non «header vuoto»: un `frame-ancestors` sbagliato
/// bloccherebbe l'interfaccia invece di proteggerla, e sbagliare in quel
/// senso è più grave che non dichiarare nulla. Chi espone questo server alla
/// rete compila il campo; vedi [`crate::config::ServerConfig::parent_origin`].
pub fn frame_ancestors(parent_origin: Option<&str>) -> Option<HeaderValue> {
    let padre = parent_origin?;
    if padre.is_empty() {
        return None;
    }
    HeaderValue::from_str(&format!("frame-ancestors {padre};")).ok()
}

/// Che cosa è stato risolto per una richiesta su un host artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactResolution {
    /// Il file d'ingresso dell'argomento.
    Ingresso(PathBuf),
    /// Un file che sta sotto il percorso dell'argomento.
    Figlio(PathBuf),
}

/// Il percorso di un file del corpus, se ci sta dentro.
///
/// La normalizzazione non basta: `corpus/../segreto` normalizza **fuori** dal
/// corpus, quindi il confronto avviene **dopo** [`Path::canonicalize`], che
/// risolve anche i symlink. Un confronto fatto prima lascerebbe passare un
/// symlink che punta fuori, e un symlink in un corpus è una cosa che un
/// insegnante fa senza pensarci.
pub fn within(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let risolto = candidate.canonicalize().ok()?;
    let radice = root.canonicalize().ok()?;
    if risolto.starts_with(&radice) {
        Some(risolto)
    } else {
        None
    }
}

/// Il file da servire per `rel_path` dell'argomento e per il resto del
/// percorso della richiesta.
///
/// `rest` è ciò che segue `/` nell'URL dell'artifact, **decodificato** dal
/// chiamante. Vuoto significa «il file d'ingresso».
///
/// Le regole sono tre e sono tutte di chiusura:
///
/// 1. `rel_path` non può essere assoluto né contenere `..`: viene dal
///    database, ma il database è una fonte di dati e una fonte di dati si
///    scrive anche a mano;
/// 2. il risultato deve stare **dentro** la radice del corpus, controllo fatto
///    dopo la risoluzione dei symlink ([`within`]);
/// 3. un figlio sta sotto il **percorso dell'argomento**, non sotto la sua
///    cartella: una bozza sorella non è un figlio.
pub fn resolve_file(
    root: &Path,
    rel_path: &str,
    rest: &str,
) -> Option<ArtifactResolution> {
    if rel_path.is_empty() || Path::new(rel_path).is_absolute() {
        return None;
    }
    if rel_path.split(['/', '\\']).any(|c| c == ".." || c == ".") {
        return None;
    }
    let base = root.join(rel_path);
    if rest.is_empty() {
        return within(root, &base).map(ArtifactResolution::Ingresso);
    }
    if rest.split(['/', '\\']).any(|c| c == ".." || c == "." || c.is_empty()) {
        return None;
    }
    if Path::new(rest).is_absolute() {
        return None;
    }
    // Sotto il percorso dell'argomento, non accanto: vedi il doc del modulo.
    let figlio = base.join(rest);
    within(root, &figlio).map(ArtifactResolution::Figlio)
}

/// Il tipo di contenuto di un file, per estensione.
///
/// `text/html` e `application/javascript` hanno il `charset` perché senza è
/// che il browser indovina, e un artifact italiano indovinato male è un
/// artifact con le virgolette storte. Il resto è una tabella corta e non un
/// `mime_guess`: i file che servono qui sono quelli di un artifact, e
/// elencarli è più onesto che indovinarli.
pub fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("corsi/analisi-1")).expect("cartelle");
        fs::write(
            dir.path().join("corsi/analisi-1/lezione-01.html"),
            "<!doctype html><title>lezione</title>",
        )
        .expect("file");
        dir
    }

    #[test]
    fn i_flag_del_sandbox_sono_quelli_di_kb() {
        // Il valore atteso è scritto qui per esteso, e non referenziato dalla
        // costante: un test che confronta una costante con sé stessa non prova
        // niente, e questo è il posto in cui qualcuno potrebbe cambiare la
        // lista senza accorgersene.
        let atteso = concat!(
            "allow-scripts allow-same-origin allow-popups ",
            "allow-popups-to-escape-sandbox allow-forms ",
            "allow-modals allow-downloads"
        );
        assert_eq!(SANDBOX_FLAGS, atteso);
    }

    #[test]
    fn l_isolamento_viene_dall_origine_e_non_dal_flag() {
        // I due artifact hanno lo **stesso** sandbox: se il flag li separasse,
        // le origini potrebbero coincidere. È l'origine che le separa, e
        // questa è l'asserzione che tiene vivo il suffisso.
        let primo = ArgumentId::from_rel_path("corsi/analisi-1/lezione-01.html");
        let secondo = ArgumentId::from_rel_path("corsi/analisi-1/lezione-02.html");
        let interfaccia = "scuola.example.it";

        let h_primo = artifact_host(&primo, DEFAULT_HOST_SUFFIX);
        let h_secondo = artifact_host(&secondo, DEFAULT_HOST_SUFFIX);

        // Il flag che sarebbe pericoloso su un'origine condivisa **c'è**. Se
        // qualcuno lo toglie per prudenza, l'artifact perde `localStorage` e si
        // rompe — ma non è un buco. Il buco sarebbe l'opposto: le origini che
        // coincidono. Ecco perché l'asserzione non è «i flag sono uguali»,
        // che è la stessa costante confrontata con sé stessa: è «i flag ci sono
        // **e** le origini no».
        assert!(
            SANDBOX_FLAGS.contains("allow-same-origin"),
            "senza allow-same-origin l'artifact perde localStorage"
        );
        let o_primo = origin_of(&h_primo);
        let o_secondo = origin_of(&h_secondo);
        let o_interfaccia = origin_of(interfaccia);
        assert_ne!(o_primo, o_interfaccia, "l'artifact non vive sull'origine dell'interfaccia");
        assert_ne!(o_primo, o_secondo, "due artifact, due origini");
        assert_ne!(o_interfaccia, origin_of("altra.example.it"));
    }

    #[test]
    fn l_etichetta_dell_host_rispetta_la_grammatica() {
        let s = DEFAULT_HOST_SUFFIX;
        assert_eq!(parse_artifact_id("arg_0123456789abcdef.artifacts.localhost", s), Some("arg_0123456789abcdef"));
        assert_eq!(parse_artifact_id("arg_0123456789abcdef.artifacts.localhost:4000", s), Some("arg_0123456789abcdef"));
        assert_eq!(parse_artifact_id("artifacts.localhost", s), None, "etichetta vuota");
        assert_eq!(parse_artifact_id(".artifacts.localhost", s), None, "inizia per punto");
        assert_eq!(parse_artifact_id("a..b.artifacts.localhost", s), None, "contiene ..");
        assert_eq!(parse_artifact_id("a/b.artifacts.localhost", s), None, "non è un'etichetta");
        assert_eq!(parse_artifact_id("localhost", s), None, "è l'host dell'interfaccia");
        assert_eq!(parse_artifact_id("a.artifacts.localhost.evil.it", s), None, "suffisso non in coda");
    }

    #[test]
    fn un_figlio_sta_sotto_l_argomento_e_non_accanto() {
        let dir = temp_root();
        let root = dir.path();
        fs::create_dir_all(root.join("corsi/analisi-1/lezione-01")).expect("figli");
        fs::write(root.join("corsi/analisi-1/lezione-01/stile.css"), "body{}").expect("stile");
        fs::write(root.join("corsi/analisi-1/bozza.html"), "segreto").expect("sorella");

        // Il figlio dentro il percorso dell'argomento: sì.
        assert!(matches!(
            resolve_file(root, "corsi/analisi-1/lezione-01", "stile.css"),
            Some(ArtifactResolution::Figlio(_))
        ));
        // La sorella accanto: no, e questo è il caso che riaprirebbe la
        // bozza a chi legge l'artifact.
        assert!(resolve_file(root, "corsi/analisi-1/lezione-01", "../bozza.html").is_none());
        assert!(resolve_file(root, "corsi/analisi-1/lezione-01", "stile.css\0").is_none());
    }

    #[test]
    fn l_ingresso_si_risolve_e_il_percorso_assoluto_no() {
        let dir = temp_root();
        assert!(matches!(
            resolve_file(dir.path(), "corsi/analisi-1/lezione-01.html", ""),
            Some(ArtifactResolution::Ingresso(_))
        ));
        assert!(resolve_file(dir.path(), "/etc/passwd", "").is_none());
        assert!(resolve_file(dir.path(), "../../../etc/passwd", "").is_none());
        assert!(resolve_file(dir.path(), "corsi/analisi-1/../../../etc/passwd", "").is_none());
        assert!(resolve_file(dir.path(), "", "").is_none());
    }

    #[test]
    fn un_symlink_che_esce_dal_corpus_non_e_servito() {
        let dir = temp_root();
        let fuori = tempfile::TempDir::new().expect("fuori");
        let segreto = fuori.path().join("segreto.html");
        fs::write(&segreto, "segreto").expect("file");
        std::os::unix::fs::symlink(&segreto, dir.path().join("corsi/analisi-1/scusa.html")).expect("symlink");

        // Il confronto di prefisso è fatto dopo la risoluzione: senza questo
        // il symlink passerebbe, perché la sua *stringa* sta dentro il corpus.
        assert!(resolve_file(dir.path(), "corsi/analisi-1/scusa.html", "").is_none());
    }

    #[test]
    fn frame_ancestors_assente_non_diventa_un_header_vuoto() {
        assert!(frame_ancestors(None).is_none());
        assert!(frame_ancestors(Some("")).is_none());
        let valore = frame_ancestors(Some("https://scuola.example.it")).expect("header");
        assert_eq!(valore.to_str().expect("testo"), "frame-ancestors https://scuola.example.it;");
    }
}
