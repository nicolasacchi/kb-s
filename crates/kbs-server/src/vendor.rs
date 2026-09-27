//! Il runtime three.js vendorizzato: una rotta, due varianti, due `ETag`.
//!
//! # Perché è in `kbs-server` e non in `kbs-doc`
//!
//! D15 è una regola sul **confine**, e il confine è questo processo. Un
//! artifact che carica `https://cdn…` esce dal perimetro della scuola: non è
//! verificabile (D6 — non si può dire da dove viene il byte) e non è
//! riproducibile (D11 — domani il file è diverso e nessuno se ne accorge).
//! `kbs-doc` fa la parte che si può fare senza rete, cioè **rifiutare** quei
//! riferimenti a validazione; servire il runtime è l'altra metà della regola, e
//! l'altra metà è un server.
//!
//! # Perché due varianti e due `ETag`
//!
//! Il file gzip-ato è in `vendor/three/three.module.min.js.gz` accanto a
//! quello pieno, e D15 li chiede entrambi. Servire sempre il `.gz` a chi non
//! ha `Accept-Encoding: gzip` significa mandare byte incomprimibili a un
//! client che non li sa decomprimere; servire sempre quello pieno significa far
//! pagare a ogni classe 691 KB di testo quando ne ha 170 KB di rete. La scelta
//! è sull'`Accept-Encoding`, quindi i due corpi hanno **due `ETag` diversi**: un
//! `ETag` solo, con una cache che ha già l'altra variante, serve una risposta
//! sbagliata.
//!
//! `Vary: Accept-Encoding` è l'altra metà della stessa frase, e senza è la
//! variante giusta a finire nella cache sbagliata.
//!
//! # La cache è lunga, e per una ragione
//!
//! Un anno, e `immutable`. Il nome del file è quello del file prodotto dalla
//! build e versionato dal repository, quindi cambiarlo significa cambiare il
//! file, e il client ha già in cache la risposta giusta. Invalidare tutto a ogni
//! deploy costerebbe 691 KB a ogni pagina di ogni classe, in una scuola con
//! rete scarsa, per un file che non è cambiato.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::error::ApiError;

/// Il tipo di contenuto del runtime.
pub const CONTENT_TYPE: &str = "application/javascript; charset=utf-8";

/// Il nome del file del runtime, preso dal percorso che `kbs-doc` ha
/// dichiarato.
///
/// Non è una costante scritta qui: è lo stesso nome che il validatore ha
/// deciso essere dentro il perimetro, quindi i due non possono divergere senza
/// che un test se ne accorga.
fn file_name() -> &'static str {
    Path::new(kbs_doc::THREE_RUNTIME)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("three.module.min.js")
}

/// Il nome del file gzip-ato, derivato da quello pieno.
///
/// La variante compressa non ha un nome proprio: è *lo stesso file* in una
/// codifica di trasporto diversa, e chiamarla `three.module.min.js.gz` nella
/// rotta esporrebbe una scelta di encoding a chi scrive l'URL.
fn gzip_name() -> String {
    format!("{}.gz", file_name())
}

/// Una variante del runtime, pronta in memoria.
#[derive(Debug, Clone)]
pub struct Variant {
    /// I byte, così come sono in rete.
    pub bytes: Arc<Vec<u8>>,
    /// `ETag` forte: l'hash dei byte serviti, non del file su disco.
    pub etag: String,
    /// `Content-Encoding`, se questa variante è compressa.
    pub encoding: Option<&'static str>,
}

/// Il runtime vendorizzato, letto una volta all'avvio.
///
/// In memoria e non letto a ogni richiesta: sono 691 KB, e un `sha256` di 691
/// KB per ogni pagina di ogni classe è il tipo di costo che si paga per
/// decine e non si nota. Una volta all'avvio è una riga di log.
#[derive(Debug, Clone)]
pub struct Vendor {
    /// Il percorso dichiarato, per gli errori che lo nominano.
    pub dichiarato: PathBuf,
    piano: Option<Variant>,
    compresso: Option<Variant>,
}

impl Vendor {
    /// Carica il runtime. Se non c'è, **non fallisce**: la rotta lo dirà con
    /// un errore che nomina D15, e il resto del server continua a funzionare.
    ///
    /// Fallire all'avvio sarebbe una scelta — un deployment senza three.js non
    /// funziona — ma renderebbe un artifact 3D, che è una parte del materiale
    /// e non tutto, un motivo per non poter servire neppure le pagine di
    /// algebra. La scelta è di chi amministra il deployment, non del processo.
    pub fn load(runtime: &Path) -> Vendor {
        let piano = std::fs::read(runtime).ok().map(|b| variant(b, None));
        if piano.is_none() {
            tracing::error!(
                percorso = %runtime.display(),
                "D15: il runtime three.js non è vendorizzato: un artifact che lo cerca non può partire"
            );
        }
        let compresso = std::fs::read(runtime.with_file_name(gzip_name()))
            .ok()
            .map(|b| variant(b, Some("gzip")));
        Vendor {
            dichiarato: runtime.to_path_buf(),
            piano,
            compresso,
        }
    }

    /// Il runtime c'è?
    pub fn presente(&self) -> bool {
        self.piano.is_some()
    }

    /// La variante giusta per un `Accept-Encoding`.
    ///
    /// `gzip` è accettato solo se è nella lista: `gzip;q=0` è un «non lo
    /// voglio» e rispettarlo è la differenza fra un client educato e uno che
    /// riceve una risposta che non sa leggere. La variante compressa serve solo
    /// se esiste: un `.gz` mancante non è un errore da segnalare al client ma
    /// un file che va servito per intero.
    pub fn per_encoding(&self, accept: Option<&str>) -> Result<&Variant, ApiError> {
        if accetta_gzip(accept) {
            if let Some(v) = &self.compresso {
                return Ok(v);
            }
        }
        self.piano.as_ref().ok_or_else(|| ApiError::ThreeNotVendored {
            percorso: self.dichiarato.display().to_string(),
        })
    }
}

/// Una variante, con il suo `ETag`.
fn variant(bytes: Vec<u8>, encoding: Option<&'static str>) -> Variant {
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let etag = format!("\"{}\"", hex::encode(hasher.finalize()));
    Variant {
        bytes: Arc::new(bytes),
        etag,
        encoding,
    }
}

/// L'`Accept-Encoding` accetta gzip?
///
/// Regola semplice e dichiarata: si cerca `gzip` come token, e un token con
/// `q=0` vale un no.
pub fn accetta_gzip(accept: Option<&str>) -> bool {
    let Some(accept) = accept else {
        return false;
    };
    for token in accept.split(',') {
        let token = token.trim();
        let (nome, parametri) = match token.split_once(';') {
            Some((nome, parametri)) => (nome.trim(), parametri),
            None => (token, ""),
        };
        if !nome.eq_ignore_ascii_case("gzip") {
            continue;
        }
        let rifiutato = parametri
            .split(';')
            .filter_map(|p| p.trim().strip_prefix("q="))
            .any(|q| matches!(q.trim(), "0" | "0.0" | "0.00" | "0.000"));
        if !rifiutato {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;

    fn vendor_reale() -> Vendor {
        let cfg = ServerConfig::new("/tmp/non-esiste");
        Vendor::load(&cfg.three_runtime())
    }

    #[test]
    fn il_runtime_vendorizzato_esiste_e_ha_due_varianti() {
        let v = vendor_reale();
        assert!(v.presente(), "il runtime vendorizzato manca: D15");
        assert!(v.compresso.is_some(), "manca la variante gzip");
        let piano = v.piano.as_ref().expect("variante piana");
        assert!(piano.bytes.len() > 100_000, "il runtime sembra vuoto");
        assert!(piano.etag.starts_with('"') && piano.etag.ends_with('"'));
    }

    #[test]
    fn le_due_varianti_hanno_etag_diversi() {
        // Un solo `ETag` per due corpi diversi fa servire dalla cache la
        // risposta sbagliata: è il caso che `Vary: Accept-Encoding` da solo non
        // copre, perché la cache può rispondere prima di ricalcolare.
        let v = vendor_reale();
        let piano = v.piano.as_ref().expect("piana");
        let compresso = v.compresso.as_ref().expect("compressa");
        assert_ne!(piano.etag, compresso.etag);
        assert_eq!(compresso.encoding, Some("gzip"));
        assert!(piano.bytes.len() > compresso.bytes.len());
    }

    #[test]
    fn la_scelta_della_variante_segue_accept_encoding() {
        let v = vendor_reale();
        let con_gzip = v.per_encoding(Some("gzip, deflate, br")).expect("gzip");
        assert_eq!(con_gzip.encoding, Some("gzip"));
        let senza = v.per_encoding(None).expect("nessuno");
        assert_eq!(senza.encoding, None);
        let solo_br = v.per_encoding(Some("br, deflate")).expect("br");
        assert_eq!(solo_br.encoding, None);
        // `gzip;q=0` è un no esplicito e va rispettato.
        let rifiutato = v.per_encoding(Some("gzip;q=0, deflate")).expect("q=0");
        assert_eq!(rifiutato.encoding, None);
    }

    #[test]
    fn un_runtime_assente_dice_d15_e_non_404() {
        let v = Vendor::load(Path::new("/non/esiste/three.module.min.js"));
        assert!(!v.presente());
        let errore = v.per_encoding(None).expect_err("deve fallire");
        let corpo = errore.body();
        assert_eq!(corpo.regola, Some("D15"));
        assert_eq!(corpo.error, "runtime-non-vendorizzato");
    }
}
