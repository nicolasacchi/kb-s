//! Come è fatto il deployment: dove sta il corpus, dove il runtime
//! vendorizzato, e quale è il suffisso delle origini artifact.
//!
//! # Perché i due percorsi compilati sono un default, non un vincolo
//!
//! Il corpus e il database stanno dove li mette chi installa, quindi hanno un
//! valore di default e si sovrascrivono. Il runtime three.js è un'altra
//! cosa: D15 dice che è **nel repository** e servito dal binario, e quindi il
//! suo posto è una conseguenza della build. Finché l'unico modo di eseguire
//! il binario è dal checkout, «la radice del repository» è la risposta
//! giusta. Ma un binario compilato porta dentro di sé un percorso assoluto,
//! e un'immagine Docker non contiene la directory in cui qualcuno ha
//! compilato: `web_dir` e `vendor_dir` diventavano allora due percorsi
//! inesistenti e il daemon non si pubblicava. Perciò i due sono **dichiarabili
//! a runtime** ([`ServerConfig::con_percorsi`]) e dichiararli non è una
//! cortesia: è l'unico modo in cui il binario fuori dal checkout funziona.
//! Ciò che non cambia è la validazione: D15 vuole il runtime presente, e un
//! percorso dichiarato che non lo contiene è un rifiuto, non un avviso.
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
        ServerConfig::three_runtime_alla(&self.vendor_dir)
    }

    /// La configurazione con i due percorsi dichiarati a runtime.
    ///
    /// Il binario compilato porta dentro di sé due percorsi assoluti — la
    /// cartella `web/` accanto al crate e `vendor/` alla radice del repository
    /// — e sono giusti per chi esegue dal checkout e sbagliati appena il
    /// binario viene spostato: un'immagine Docker non contiene la directory
    /// in cui qualcuno ha compilato. Perciò i due sono sovrascrivibili, e la
    /// differenza tra «non dichiarato» e «dichiarato e non trovato» è un
    /// `Option`: la validazione di quella distinzione sta in
    /// [`crate::daemon::controlla_percorsi`], perché è lì che la regola è.
    pub fn con_percorsi(mut self, web: Option<&Path>, vendor: Option<&Path>) -> Self {
        if let Some(web) = web {
            self.web_dir = web.to_path_buf();
        }
        if let Some(vendor) = vendor {
            self.vendor_dir = vendor.to_path_buf();
        }
        self
    }

    /// Il percorso del runtime three.js dentro un albero `vendor/`, senza
    /// costruire una configurazione.
    ///
    /// Una funzione associata e non un trucco: la validazione di `--vendor`
    /// avviene prima di sapere qual è il corpus, e costruire una
    /// `ServerConfig` con un corpus fittizio per ottenere una concatenazione
    /// sarebbe una riga che il lettore deve smontare. `None` qui non esiste:
    /// la concatenazione è vera per qualsiasi radice, l'esistenza del file è
    /// un'altra domanda e la fa `daemon::controlla_percorsi`.
    pub fn three_runtime_alla(vendor: &Path) -> PathBuf {
        vendor.join("three").join("three.module.min.js")
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
///
/// Fuori dal repository questo percorso non esiste, ed è per questo che
/// `--vendor` esiste: non per cambiare il default, ma per sostituirlo dove il
/// default non può arrivare.
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

    #[test]
    fn i_due_percorsi_dichiarati_sostituiscono_il_default_compilato() {
        // Il caso da coprire è metà e metà: `--web` da solo non deve
        // spostare il vendor, e viceversa. Un override che tocca l'altro
        // campo per pigrizia porterebbe a servire il runtime di
        // un'installazione diversa, che è esattamente il difetto che i due
        // flag sono nati per chiudere.
        let base = ServerConfig::new("/tmp/corpus");
        let solo_web = base.clone().con_percorsi(Some(Path::new("/opt/ui")), None);
        assert_eq!(solo_web.web_dir, PathBuf::from("/opt/ui"));
        assert_eq!(solo_web.vendor_dir, base.vendor_dir);

        let solo_vendor = base.clone().con_percorsi(None, Some(Path::new("/opt/vend")));
        assert_eq!(solo_vendor.vendor_dir, PathBuf::from("/opt/vend"));
        assert_eq!(solo_vendor.web_dir, base.web_dir);
        // E il runtime segue il vendor dichiarato, non quello compilato.
        assert_eq!(
            solo_vendor.three_runtime(),
            ServerConfig::three_runtime_alla(Path::new("/opt/vend"))
        );
    }

    #[test]
    fn nessun_dichiarato_lascia_esattamente_il_default_compilato() {
        let base = ServerConfig::new("/tmp/corpus");
        let invariata = base.clone().con_percorsi(None, None);
        assert_eq!(invariata.web_dir, base.web_dir);
        assert_eq!(invariata.vendor_dir, base.vendor_dir);
    }
}
