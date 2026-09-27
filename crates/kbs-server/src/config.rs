//! Come è fatto il deployment: dove sta il corpus, dove il runtime
//! vendorizzato, e quale è il suffisso delle origini artifact.
//!
//! # Perché quasi tutto ha un default e il vendor no
//!
//! Il corpus e il database stanno dove li mette chi installa, quindi hanno un
//! valore di default e si sovrascrivono. Il runtime three.js è un'altra
//! cosa: D15 dice che è **nel repository** e servito dal binario, e quindi il
//! suo posto è una conseguenza della build, non una scelta di runtime. Perciò
//! il default è il percorso del repository accanto al crate e non un valore
//! «da configurare»: se qualcuno lo sposta, l'errore è esplicito e nomina
//! D15, non un `404` che sembrerebbe un percorso sbagliato.
//!
//! # `parent_origin`
//!
//! Se è `None` non si manda `frame-ancestors`, e questo è il default
//!giuridico: il CSP non dichiarato non vieta niente, e dichiarare
//! `frame-ancestors` sbagliatobloccherebbe l'interfaccia invece di proteggerla.
//! In un deployment vero il campo va compilato, perché senza `frame-ancestors`
//! chiunque può incorniciare un artifact — e un artifact è materiale didattico
//! di un corso.

use std::path::{Path, PathBuf};

/// Il suffisso degli host artifact in sviluppo.
///
/// Non è una seconda copia: è il valore di `kb`, e [`crate::sandbox`] è il
/// posto in cui è dichiarato. Due costanti uguali in due moduli sono due
/// costanti che divergono al primo deploy.
pub const DEV_HOST_SUFFIX: &str = crate::sandbox::DEFAULT_HOST_SUFFIX;

/// La configurazione del server.
///
/// I campi sono tutti pubblici e volutamente pochi: un oggetto di
/// configurazione con trenta campi è un oggetto che nessuno legge, e quello
/// che nessuno legge è quello che nessuno aggiorna.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// La radice del corpus: sotto questa cartella stanno gli artifact, e
    /// sotto questa cartella **solo**. Vedi [`crate::sandbox`].
    pub corpus_root: PathBuf,
    /// La cartella `vendor/` del repository: dentro c'è `three/`.
    pub vendor_dir: PathBuf,
    /// Dove si servono i file dell'interfaccia, se esistono.
    pub web_dir: PathBuf,
    /// Il suffisso degli host artifact: `<argomento><suffisso>`.
    pub artifact_host_suffix: String,
    /// L'origine che può incorniciare un artifact. `None` non manda
    /// `frame-ancestors`.
    pub parent_origin: Option<String>,
    /// Quanto tempo il browser tiene il runtime three.js.
    pub vendor_max_age_seconds: u32,
}

impl ServerConfig {
    /// Una configurazione con i default di sviluppo.
    pub fn new(corpus_root: impl Into<PathBuf>) -> Self {
        ServerConfig {
            corpus_root: corpus_root.into(),
            vendor_dir: default_vendor_dir(),
            web_dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("web"),
            artifact_host_suffix: DEV_HOST_SUFFIX.to_string(),
            parent_origin: None,
            vendor_max_age_seconds: 31_536_000,
        }
    }

    /// Il percorso del runtime three.js vendorizzato, che è anche quello che
    /// va servito: D15 lo vuole minimizzato e gzip-ato, e i due file sono
    /// già in `vendor/three/`.
    pub fn three_runtime(&self) -> PathBuf {
        self.vendor_dir.join("three").join("three.module.min.js")
    }

    /// Il corpus normalizzato, una volta sola.
    ///
    /// Il percorso risolto è quello con cui ogni file servito viene confrontato
    /// con il suo `starts_with`: se la radice contiene un symlink, il confronto
    /// va fatto **dopo** la risoluzione, altrimenti `starts_with` accetta un
    /// percorso che punta fuori dalla radice.
    pub fn canonical_corpus_root(&self) -> std::io::Result<PathBuf> {
        self.corpus_root.canonicalize()
    }
}

/// La radice del repository, dedotta dal percorso del crate.
///
/// `CARGO_MANIFEST_DIR` è la posizione del crate nel repository, quindi
/// `../../` è la radice del workspace, dove `vendor/` sta per convenzione di
/// D15. In un'installazione fuori dal repository il percorso non esiste e la
/// rotta del runtime lo dice con un errore che nomina la regola.
fn default_vendor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("vendor")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_default_del_vendor_e_la_radice_del_repository() {
        let cfg = ServerConfig::new("/tmp/corpus");
        assert!(cfg.vendor_dir.ends_with("vendor"), "{:?}", cfg.vendor_dir);
        assert!(cfg.three_runtime().ends_with("three/three.module.min.js"));
        // Il percorso dichiarato deve esistere in questo repository: se il
        // vendor non fosse dove D15 dice, il test deve accorgersene prima che
        // lo faccia un artifact in una classe.
        assert!(
            cfg.three_runtime().is_file(),
            "il runtime vendorizzato non è in {:?}",
            cfg.three_runtime()
        );
    }

    #[test]
    fn senza_parent_origin_non_si_dichiara_frame_ancestors() {
        let cfg = ServerConfig::new("/tmp/corpus");
        assert!(cfg.parent_origin.is_none());
    }
}
