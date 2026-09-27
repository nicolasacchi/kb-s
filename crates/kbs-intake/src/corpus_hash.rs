//! L'hash di corpus: che cosa è dentro, in che ordine, e come si concatena.
//!
//! D11 chiede che data la tupla (hash del corpus, id dei chunk, versione del
//! modello) il sistema possa **riprodurre** la generazione. Il primo elemento
//! della tupla è questo, ed è l'unico dei tre che dipende da come il docente
//! tiene i suoi file. Se la definizione è vaga, la riproducibilità è una
//! promessa; quindi qui è scritta per intero, e le scelte non sono ovvie.
//!
//! # La definizione `kbs-c1`
//!
//! **L'insieme.** I file con suffisso `.html` sotto la radice, identificati dal
//! **percorso relativo** in UTF-8 con `/` come separatore. Fuori: ogni
//! componente che comincia con `.` (i nascosti, e quindi `.git`), i
//! collegamenti simbolici (un link è un alias, e un hash che cambia quando
//! cambia dove punta non è un hash di contenuto), e tutto ciò che non finisce
//! con `.html`. La scelta di `.html` e non «tutto» è dichiarata e non è
//!incidentale: il corpus di D12 è la cartella di file che il repository
//! versiona, e ciò che non è un artifact non entra nel lock.
//!
//! **Il prefixed hash.** Ogni file dà una foglia
//!
//! ```text
//! leaf = SHA256(0x10 ‖ len(rel) come u32 big-endian ‖ rel ‖ byte_del_file)
//! ```
//!
//! La lunghezza prima del percorso non è pignoleria: senza di essa
//! `("ab", "c")` e `("a", "bc")` concatenano negli stessi byte, e due corpus
//! diversi avrebbero lo stesso hash. È il motivo per cui l'hash è utile.
//!
//! **La piega.** Le foglie si ordinano per il **confronto dei byte** del
//! percorso relativo (non per ordine locale, non per ordine di `readdir`, che
//! cambia da una macchina all'altra) e si piegano a sinistra:
//!
//! ```text
//! acc₀ = SHA256(0x12)                       // dominio: corpus vuoto
//! accᵢ = SHA256(0x11 ‖ accᵢ₋₁ ‖ leafᵢ)
//! hash = "kbs-c1:" ‖ hex(acc_n)
//! ```
//!
//! Un accumulatore running invece di un albero: memoria `O(file)` di 32 byte
//! per file, una sola passata sul contenuto, e un `add` costa un `SHA256` del
//! file. «Barato» è il termine giusto e va detto: `kbs-c1` non è un Merkle tree
//! e non dà prove di inclusione. Per quello che deve fare — dire se il corpus
//! è lo stesso di quando è stata fatta la generazione — è abbondante, e il
//! prefisso `kbs-c1` dice che si può cambiare senza mentire sul passato.
//!
//! # I byte di dominio sono di questo crate, non di `kbs-verify`
//!
//! `0x10`, `0x11`, `0x12` qui; `kbs_verify::hash` usa `0x00` e `0x01` per la
//! catena delle osservazioni. Sono spazi diversi per scelta: un preambolo che
//! si potesse far coincidere fra due costruzioni sarebbe un modo per costruire
//! una collisione, e D6 chiede che la catena sia falsificabile, non che sia
//! comoda da riusare.


use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Il nome della definizione, e il prefisso di ogni hash prodotto con essa.
///
/// Va dentro l'hash e non fuori: un hash senza versione è un hash che fra due
/// anni qualcuno ripubblicherà con una definizione diversa e crederà di avere
/// un registro.
pub const DEFINITION: &str = "kbs-c1";

/// Prefisso dei byte di dominio della definizione `kbs-c1`.
pub mod tag {
    /// Foglia di un file.
    pub const LEAF: u8 = 0x10;
    /// Nodo della piega.
    pub const NODE: u8 = 0x11;
    /// Corpus vuoto, prima di qualsiasi foglia.
    pub const EMPTY: u8 = 0x12;
    /// Hash del contenuto di un artifact. Non è un hash di corpus: è il
    /// confronto fra la ratifica e il testo di oggi (`crate::route::content_hash`).
    pub const CONTENT: u8 = 0x14;
}

/// L'hash di un corpus, con la definizione che l'ha prodotto.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct CorpusHash(String);

impl CorpusHash {
    /// L'hash del corpus vuoto. Non è un valore di comodo: è il valore che
    /// dice «non c'era niente», ed è diverso da qualunque hash di un file solo.
    pub fn of_empty() -> Self {
        let mut h = Sha256::new();
        h.update([tag::EMPTY]);
        CorpusHash(format!("{DEFINITION}:{}", hex::encode(h.finalize())))
    }

    /// Costruisce l'hash da un digest già piegato.
    pub(crate) fn from_digest(digest: [u8; 32]) -> Self {
        CorpusHash(format!("{DEFINITION}:{}", hex::encode(digest)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// La definizione con cui è stato prodotto.
    pub fn definition(&self) -> &str {
        DEFINITION
    }

    /// Il digest, senza il prefisso. `None` se la stringa non è un
    /// `CorpusHash` di questa definizione.
    pub fn digest(&self) -> Option<[u8; 32]> {
        let rest = self.0.strip_prefix(DEFINITION)?.strip_prefix(':')?;
        let bytes = hex::decode(rest).ok()?;
        <[u8; 32]>::try_from(bytes.as_slice()).ok()
    }
}

impl std::fmt::Display for CorpusHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// L'accumulatore. Tiene **una foglia per percorso** e piega a `finish`.
///
/// Le foglie stanno in una `BTreeMap` e non in un `Vec` per una ragione che
/// un `Vec` non avrebbe: l'ordine in cui chi chiama `add` non deve cambiare
/// l'hash. Un `Vec` darebbe un hash che dipende da come il chiamante ha
/// percorso la directory, e `readdir` non ha un ordore garantito: lo stesso
/// corpus su due macchine darebbe due hash, e la riproducibilità di D11
/// diventerebbe una casualità.
#[derive(Debug, Default, Clone)]
pub struct CorpusHasher {
    leaves: BTreeMap<String, [u8; 32]>,
}

impl CorpusHasher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Quanti file sono dentro.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Aggiunge un file già in memoria.
    ///
    /// Generico su `AsRef<[u8]>` e non su `&[u8]`: chiamare `add("a.html",
    /// "contenuto")` è il caso normale e obbligare a scrivere `.as_bytes()`
    /// would be friction with no gain, because `str` is already a byte
    /// sequence.
    pub fn add(&mut self, rel_path: &str, content: impl AsRef<[u8]>) -> Result<()> {
        check_rel_path(rel_path)?;
        let leaf = leaf_of(rel_path.as_bytes(), content.as_ref());
        self.insert(rel_path.to_string(), leaf)
    }

    /// Aggiunge un file **a pezzi**: il contenuto non finisce in memoria.
    ///
    /// È la via che usa [`hash_dir`], ed è ciò che rende l'hash barato anche
    /// su un corpus più grosso della memoria: 64 KiB per volta, e la foglia
    /// sono 32 byte.
    pub fn add_reader<R: Read>(&mut self, rel_path: &str, mut reader: R) -> Result<()> {
        check_rel_path(rel_path)?;
        let mut h = Sha256::new();
        h.update([tag::LEAF]);
        h.update((rel_path.len() as u32).to_be_bytes());
        h.update(rel_path.as_bytes());
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|source| Error::Io { path: PathBuf::from(rel_path), source })?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        self.insert(rel_path.to_string(), h.finalize().into())
    }

    fn insert(&mut self, rel_path: String, leaf: [u8; 32]) -> Result<()> {
        if self.leaves.contains_key(&rel_path) {
            return Err(Error::PercorsoDuplicato { path: rel_path });
        }
        self.leaves.insert(rel_path, leaf);
        Ok(())
    }

    /// Piega e restituisce l'hash. Non consuma l'accumulatore: chiamarlo due
    /// volte dà due volte la stessa risposta.
    pub fn finish(&self) -> CorpusHash {
        let mut acc: [u8; 32] = {
            let mut h = Sha256::new();
            h.update([tag::EMPTY]);
            h.finalize().into()
        };
        for leaf in self.leaves.values() {
            let mut h = Sha256::new();
            h.update([tag::NODE]);
            h.update(acc);
            h.update(leaf);
            acc = h.finalize().into();
        }
        CorpusHash::from_digest(acc)
    }
}

fn leaf_of(rel: &[u8], content: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([tag::LEAF]);
    h.update((rel.len() as u32).to_be_bytes());
    h.update(rel);
    h.update(content);
    h.finalize().into()
}

/// Il percorso relativo è della forma che l'hash definisce, e nient'altro.
///
/// I cinque casi sono cinque modi in cui due corpus diversi produrrebbero lo
/// stesso hash o uno dipendente dalla macchina: il prefisso `/` (che cambia
/// cosa significa `a/b`), il `..`, il separatore `\` di Windows, il doppio
/// separatore, e il `./` iniziale. Sono rifiutati tutti e cinque, e il rifiuto
/// nomina il percorso.
pub fn check_rel_path(path: &str) -> Result<()> {
    let err = || Error::PercorsoNonNormalizzato { path: path.to_string() };
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return Err(err());
    }
    if path.contains("//") || path.contains("./") || path.ends_with('/') {
        return Err(err());
    }
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(err());
        }
    }
    Ok(())
}

/// L'hash di una directory, con la definizione `kbs-c1`.
///
/// `radice` non viene hashata: ciò che conta è il **contenuto del corpus**, non
/// dove sta. Copiare la cartella da `/srv/scuola/` a `/home/nik/` non cambia
/// il lock, e non deve.
pub fn hash_dir(radice: impl AsRef<Path>) -> Result<CorpusHash> {
    let radice = radice.as_ref();
    let mut hasher = CorpusHasher::new();
    let mut stack = vec![radice.to_path_buf()];
    let mut found = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|source| Error::Io { path: dir.clone(), source })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io { path: dir.clone(), source })?;
            let path = entry.path();
            let name = entry.file_name();
            let name = match name.to_str() {
                Some(n) => n.to_string(),
                None => continue, // un nome non UTF-8 non entra: vedi `PercorsoNonUtf8`
            };
            if name.starts_with('.') {
                continue;
            }
            // `symlink_metadata`, non `metadata`: un collegamento che punta a
            // una directory deve essere saltato, non attraversato, o il
            // percorso relativo conterrebbe `..` e l'hash dipenderebbe da
            // dove porta il link.
            let ft = std::fs::symlink_metadata(&path)
                .map_err(|source| Error::Io { path: path.clone(), source })?;
            if ft.file_type().is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !name.ends_with(".html") {
                continue;
            }
            let rel = match path.strip_prefix(radice) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let rel = match rel.to_str() {
                Some(r) => r.replace('\\', "/"),
                None => {
                    return Err(Error::PercorsoNonUtf8 {
                        path: path.display().to_string(),
                    })
                }
            };
            let file = std::fs::File::open(&path)
                .map_err(|source| Error::Io { path: path.clone(), source })?;
            hasher.add_reader(&rel, file)?;
            found += 1;
        }
    }
    debug_assert!(found == hasher.len());
    Ok(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(rel: &str, body: &str) -> [u8; 32] {
        leaf_of(rel.as_bytes(), body.as_bytes())
    }

    #[test]
    fn lo_stesso_corpus_da_lo_stesso_h() {
        let mut h1 = CorpusHasher::new();
        h1.add("a/uno.html", "<p>uno</p>").unwrap();
        h1.add("b/due.html", "<p>due</p>").unwrap();
        let mut h2 = CorpusHasher::new();
        h2.add("b/due.html", "<p>due</p>").unwrap();
        h2.add("a/uno.html", "<p>uno</p>").unwrap();
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn un_byte_diverso_cambia_l_hash() {
        let mut h1 = CorpusHasher::new();
        h1.add("a.html", "<p>uno</p>").unwrap();
        let mut h2 = CorpusHasher::new();
        h2.add("a.html", "<p>unO</p>").unwrap();
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn il_mediatore_del_percorso_non_e_ambiguo() {
        // ("ab", "c") e ("a", "bc") hanno gli stessi byte concatenati senza
        // il prefisso di lunghezza: è il caso per cui il prefisso esiste.
        assert_ne!(a("ab", "c"), a("a", "bc"));
    }

    #[test]
    fn rinominare_un_file_cambia_l_hash() {
        let mut h1 = CorpusHasher::new();
        h1.add("a/uno.html", "x").unwrap();
        let mut h2 = CorpusHasher::new();
        h2.add("a/due.html", "x").unwrap();
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn il_corpus_vuoto_ha_un_hash_proprio() {
        assert_eq!(CorpusHasher::new().finish(), CorpusHash::of_empty());
        let mut h = CorpusHasher::new();
        h.add("a.html", "x").unwrap();
        assert_ne!(h.finish(), CorpusHash::of_empty());
    }

    #[test]
    fn il_digest_e_rileggibile() {
        let h = CorpusHasher::new().finish();
        assert_eq!(h.definition(), DEFINITION);
        assert!(h.as_str().starts_with("kbs-c1:"));
        assert_eq!(h.digest().map(|d| hex::encode(d)).as_deref(), Some(h.as_str().strip_prefix("kbs-c1:").unwrap()));
    }

    #[test]
    fn i_percorsi_non_normalizzati_sono_rifiutati() {
        for p in ["/a.html", "a\\b.html", "a//b.html", "./a.html", "a/../b.html", "a/", ""] {
            assert!(
                matches!(check_rel_path(p), Err(Error::PercorsoNonNormalizzato { .. })),
                "{p} doveva essere rifiutato"
            );
        }
        assert!(check_rel_path("a/b.html").is_ok());
    }

    #[test]
    fn un_percorso_duplicato_e_un_rifiuto() {
        let mut h = CorpusHasher::new();
        h.add("a.html", "x").unwrap();
        assert!(matches!(h.add("a.html", "y"), Err(Error::PercorsoDuplicato { .. })));
    }

    #[test]
    fn add_reader_e_add_danno_lo_stesso_hash() {
        let mut a1 = CorpusHasher::new();
        a1.add("a.html", "ciao mondo").unwrap();
        let mut a2 = CorpusHasher::new();
        a2.add_reader("a.html", &b"ciao mondo"[..]).unwrap();
        assert_eq!(a1.finish(), a2.finish());
    }

    #[test]
    fn la_directory_ignora_i_nascosti_e_i_non_html() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.html"), "<p>a</p>").unwrap();
        std::fs::write(dir.path().join("sub/b.html"), "<p>b</p>").unwrap();
        std::fs::write(dir.path().join("nota.md"), "niente").unwrap();
        std::fs::write(dir.path().join(".nascosto.html"), "niente").unwrap();
        let h = hash_dir(dir.path()).unwrap();
        let mut atteso = CorpusHasher::new();
        atteso.add("a.html", "<p>a</p>").unwrap();
        atteso.add("sub/b.html", "<p>b</p>").unwrap();
        assert_eq!(h, atteso.finish());
    }

    #[test]
    fn la_directory_ignora_i_collegamenti_simbolici() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.html"), "<p>a</p>").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("a.html"), dir.path().join("b.html")).unwrap();
        let h = hash_dir(dir.path()).unwrap();
        let mut atteso = CorpusHasher::new();
        atteso.add("a.html", "<p>a</p>").unwrap();
        assert_eq!(h, atteso.finish());
    }

    #[test]
    fn spostare_la_cartella_non_cambia_l_hash() {
        let uno = tempfile::tempdir().unwrap();
        let due = tempfile::tempdir().unwrap();
        for d in [uno.path(), due.path()] {
            std::fs::write(d.join("a.html"), "<p>a</p>").unwrap();
        }
        assert_eq!(hash_dir(uno.path()).unwrap(), hash_dir(due.path()).unwrap());
    }
}
