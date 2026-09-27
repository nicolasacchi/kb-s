//! Il prompt canonico e il model lock (D10).
//!
//! # Perché l'hash è sui byte e non sulla descrizione dei byte
//!
//! D10 chiede l'hash del prompt. La lettura comoda è «hash della descrizione
//! del prompt»: il testo che descrive che cosa si vuole, la somma, l'istruzione.
//! È la lettura sbagliata, e si rompe in un modo preciso: la descrizione può
//! essere più corta del prompt, può omettere un pezzo, e due prompt diversi
//! possono avere la stessa descrizione. A quel punto l'hash non identifica la
//! generazione, e D11 — riprodurre data la tupla — diventa irrealizzabile.
//!
//! Quindi qui l'oggetto [`GenerationRequest`] **rende** i byte canonici e li
//! **hasha**: [`GenerationRequest::canonical_bytes`] e
//! [`GenerationRequest::prompt_hash`] non possono divergere, perché il secondo
//! chiama il primo. Il chiamante manda al modello esterno esattamente
//! `canonical_bytes()`, e `prompt_hash` è `SHA256(0x13 ‖ quei byte)`.
//!
//! # La forma dei byte
//!
//! ```text
//! kbs-prompt/1\n
//! model: <valore>\n
//! generator: <valore>\n
//! corpus: <valore>\n
//! system: <valore>\n
//! task: <valore>\n
//! excerpt: <valore>\n
//! instruction: <valore>\n
//! diagnosis-id: <valore>\n
//! diagnosis-note: <valore>\n
//! diagnosis-witness: <valore>\n
//! ```
//!
//! Undici righe, ordine fisso, terminatore `\n`, nessuna riga vuota, e ogni
//! valore **sulla stessa riga**: `\` diventa `\\`, l'a capo diventa `\n`, il
//! ritorno a capo divane `\r`, il resto è letterale. Con questi tre escape la
//! mappa fra valore e byte è iniettiva, e un prompt non può essere cambiato
//! senza che l'hash lo veda. L'ordine delle righe è fisso perché un JSON
//! con le chiavi in ordine diverso sarebbe un prompt diverso, e un prompt
//! diverso è un'altra generazione.
//!
//! Il corpus entra **due volte**, e le due volte non è una ridondanza: la
//! riga `corpus:` è l'hash che rende la generazione riproducibile (D11),
//! `excerpt:` è il testo che il modello ha effettivamente letto. Un excerpt
//! che non sta dentro il corpus, o un corpus senza excerpt, sono entrambi
//! sbagli, ed è per questo che l'excerpt è un campo e non una conseguenza.
//!
//! # La diagnosi dentro il prompt
//!
//! `kbs_core::Evidence::Oral` esiste perché la diagnosi orale **non è
//! riproducibile** e va detto. Ma «non riproducibile» non vuol dire «ignorabile»:
//! se il materiale è stato generato in risposta a ciò che il docente ha detto a
//! uno studente, quella frase è un **input** della generazione, e sta qui dentro
//! il prompt. Quindi l'hash del prompt impegna anche l'id, la nota e il
//! testimone della diagnosi.
//!
//! Il limite è dichiarato in [`crate::diagnosis`] e vale la pena ripeterlo: il
//! corpus non ha una colonna che leghi una generazione alla diagnosi che l'ha
//! causata. Il legame sopravvive **dentro l'hash**, e quindi sopravvive solo
//! come «questa generazione ha avuto un input che non è il corpus». Non si può
//! chiedere al database «quali esercizi sono nati dalla diagnosi di Marco del
//! 14». È un buco con un nome, non una nota a margine.

use kbs_core::{Millis, ModelLock, PersonId};
use sha2::{Digest, Sha256};

use crate::corpus_hash::CorpusHash;
use crate::error::{Error, Result};

/// Il formato dei byte canonici, e la versione che li descrive.
pub const FORMAT: &str = "kbs-prompt/1";

/// Separatore di dominio dell'hash del prompt. Nessun altro costrutto di
/// `kb-s` usa `0x13`: vedi [`crate::corpus_hash::tag`].
pub const TAG: u8 = 0x13;

/// Il riferimento a una diagnosi orale, come entra nel prompt.
///
/// I tre campi sono quelli di `kbs_core::Evidence::Oral`: la nota che il
/// docente ha scritto e l'eventuale testimone. L'id è ciò che rende la
/// diagnosis *indirizzabile* nel registro.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosisRef {
    pub id: String,
    pub note: String,
    pub witness: Option<PersonId>,
}

impl DiagnosisRef {
    pub fn new(id: impl Into<String>, note: impl Into<String>, witness: Option<PersonId>) -> Self {
        DiagnosisRef { id: id.into(), note: note.into(), witness }
    }
}

/// Una richiesta di generazione, e soltanto questo.
///
/// Non c'è un metodo che «mandi» la richiesta: D3 vieta che un modello entri
/// nel prodotto, e il tipo che lo ammettesse sarebbe un modello che entra dal
/// retrobottoncino. Qui si costruisce ciò che il docente manderà **al di
/// fuori**, e si hashano i byte, e basta.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GenerationRequest {
    pub model_id: String,
    pub generator_version: String,
    pub corpus: CorpusHash,
    pub system: String,
    pub task: String,
    /// Il testo che il modello ha letto, tratto dal corpus di cui sopra.
    pub excerpt: String,
    /// La richiesta del docente: che cosa vuole, e perché adesso.
    pub instruction: String,
    pub diagnosis: Option<DiagnosisRef>,
}

impl GenerationRequest {
    /// Costruisce e **subito verifica** la richiesta.
    ///
    /// La verifica è qui e non in `canonical_bytes` perché un lock con un campo
    /// vuoto è un lock che non identifica niente, e deve essere impossibile da
    /// costruire e non impossibile da notare dopo.
    pub fn try_new(
        model_id: impl Into<String>,
        generator_version: impl Into<String>,
        corpus: CorpusHash,
        system: impl Into<String>,
        task: impl Into<String>,
        excerpt: impl Into<String>,
        instruction: impl Into<String>,
    ) -> Result<Self> {
        let r = GenerationRequest {
            model_id: model_id.into(),
            generator_version: generator_version.into(),
            corpus,
            system: system.into(),
            task: task.into(),
            excerpt: excerpt.into(),
            instruction: instruction.into(),
            diagnosis: None,
        };
        r.check()?;
        Ok(r)
    }

    /// Attacca la diagnosi orale che ha motivato la richiesta.
    pub fn with_diagnosis(mut self, diagnosis: DiagnosisRef) -> Self {
        self.diagnosis = Some(diagnosis);
        self
    }

    /// Nessuno dei quattro campi che identificano una generazione può essere
    /// vuoto. L'excerpt può esserlo: un corpus vuoto è una richiesta
    /// legittima, e rifiutarla sarebbe rifiutare un corso che non ha ancora
    /// cominciato.
    pub fn check(&self) -> Result<()> {
        for (campo, valore) in [
            ("model_id", &self.model_id),
            ("generator_version", &self.generator_version),
            ("system", &self.system),
            ("task", &self.task),
            ("instruction", &self.instruction),
        ] {
            if valore.trim().is_empty() {
                return Err(Error::LockIncompleto { campo });
            }
        }
        Ok(())
    }

    /// I byte che il modello esterno riceve. È **questo** che si manda, e non
    /// una sua descrizione.
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::with_capacity(
            self.system.len() + self.task.len() + self.instruction.len() + self.excerpt.len() + 256,
        );
        out.push_str(FORMAT);
        out.push('\n');
        for (chiave, valore) in [
            ("model", self.model_id.as_str()),
            ("generator", self.generator_version.as_str()),
            ("corpus", self.corpus.as_str()),
            ("system", self.system.as_str()),
            ("task", self.task.as_str()),
            ("excerpt", self.excerpt.as_str()),
            ("instruction", self.instruction.as_str()),
            ("diagnosis-id", self.diagnosis.as_ref().map(|d| d.id.as_str()).unwrap_or("none")),
            ("diagnosis-note", self.diagnosis.as_ref().map(|d| d.note.as_str()).unwrap_or("none")),
            (
                "diagnosis-witness",
                self.diagnosis
                    .as_ref()
                    .and_then(|d| d.witness.as_ref())
                    .map(|w| w.as_str())
                    .unwrap_or("none"),
            ),
        ] {
            out.push_str(chiave);
            out.push_str(": ");
            escape_into(valore, &mut out);
            out.push('\n');
        }
        out
    }

    /// `SHA256(0x13 ‖ canonical_bytes())`, con la definizione dentro.
    pub fn prompt_hash(&self) -> String {
        let bytes = self.canonical_bytes();
        let mut h = Sha256::new();
        h.update([TAG]);
        h.update(bytes.as_bytes());
        format!("{FORMAT}:{}", hex::encode(h.finalize()))
    }

    /// Il lock (D10): modello, hash del prompt, hash del corpus, versione del
    /// generatore, istante.
    ///
    /// `at` è un parametro e non `Millis::now()`: un lock è un **dato** che
    /// descrive una generazione passata, e la generazione è già avvenuta quando
    /// il lock si costruisce. Un lock che si.timestampa da solo sarebbe un lock
    /// che mente su quando è nato.
    pub fn lock(&self, at: Millis) -> Result<ModelLock> {
        self.check()?;
        Ok(ModelLock {
            model_id: self.model_id.clone(),
            prompt_hash: self.prompt_hash(),
            corpus_hash: self.corpus.as_str().to_string(),
            generator_version: self.generator_version.clone(),
            at,
        })
    }
}

/// Escape iniettivo: tre caratteri, e nient'altro da toccare.
///
/// Il criterio è che la funzione e la sua inversa siano entrambe definita su
/// *qualsiasi* valore: se si ammettesse un escape ambiguo, due prompt diversi
/// potrebbero avere gli stessi byte, e l'hash smetterebbe di identificare.
fn escape_into(valore: &str, out: &mut String) {
    for c in valore.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            altro => out.push(altro),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn richiesta() -> GenerationRequest {
        GenerationRequest::try_new(
            "claude-sonnet-4-5",
            "kbs-intake/1",
            CorpusHash::of_empty(),
            "Sei un generatore di materiale di studio.",
            "Scrivi un esercizio.",
            "<p>Il corpus</p>",
            "Marco non distingue la frazione irriducibile.",
        )
        .unwrap()
    }

    #[test]
    fn i_prompt_hashano_i_byte_e_il_rendering() {
        // Indipendente dal codice di produzione: se i due divergessero, qui
        // si vedrebbe. È il test che rende vera la frase del modulo.
        let r = richiesta();
        let bytes = r.canonical_bytes();
        let mut h = Sha256::new();
        h.update([TAG]);
        h.update(bytes.as_bytes());
        assert_eq!(r.prompt_hash(), format!("{FORMAT}:{}", hex::encode(h.finalize())));
    }

    #[test]
    fn i_byte_hanno_il_numero_di_righe_dichiarato() {
        let testo = richiesta().canonical_bytes();
        assert_eq!(testo.lines().count(), 11);
        assert!(testo.starts_with("kbs-prompt/1\n"));
        assert!(testo.ends_with('\n'));
    }

    #[test]
    fn un_a_capo_nel_valore_non_aggiunge_una_riga() {
        let r = GenerationRequest::try_new(
            "m", "g", CorpusHash::of_empty(), "s", "t", "e", "prima\nseconda",
        )
        .unwrap();
        let testo = r.canonical_bytes();
        assert_eq!(testo.lines().count(), 11);
        assert!(testo.contains("instruction: prima\\nseconda\n"));
    }

    #[test]
    fn due_richieste_uguali_hanno_lo_stesso_hash_e_un_byte_di_piu_no() {
        assert_eq!(richiesta().prompt_hash(), richiesta().prompt_hash());
        let altra = GenerationRequest::try_new(
            "claude-sonnet-4-5",
            "kbs-intake/1",
            CorpusHash::of_empty(),
            "Sei un generatore di materiale di studio.",
            "Scrivi un esercizio.",
            "<p>Il corpus</p>",
            "Marco non distingue la frazione irriducibile!",
        )
        .unwrap();
        assert_ne!(richiesta().prompt_hash(), altra.prompt_hash());
    }

    #[test]
    fn il_corpus_cambia_lo_stesso_prompt() {
        let mut h = CorpusHasherDiTest::nuovo();
        h.aggiungi("a.html", "<p>a</p>");
        let a = GenerationRequest::try_new(
            "m", "g", h.hash(), "s", "t", "e", "i",
        )
        .unwrap();
        h.aggiungi("b.html", "<p>b</p>");
        let b = GenerationRequest::try_new(
            "m", "g", h.hash(), "s", "t", "e", "i",
        )
        .unwrap();
        assert_ne!(a.prompt_hash(), b.prompt_hash());
        assert_ne!(a.lock(Millis(1)).unwrap().corpus_hash, b.lock(Millis(1)).unwrap().corpus_hash);
    }

    struct CorpusHasherDiTest(crate::corpus_hash::CorpusHasher);
    impl CorpusHasherDiTest {
        fn nuovo() -> Self {
            Self(crate::corpus_hash::CorpusHasher::new())
        }
        fn aggiungi(&mut self, rel: &str, body: &str) {
            self.0.add(rel, body.as_bytes()).unwrap();
        }
        fn hash(&self) -> CorpusHash {
            self.0.finish()
        }
    }

    #[test]
    fn la_diagnosi_entra_nel_prompt_e_nell_hash() {
        let senza = richiesta();
        let con = senza.clone().with_diagnosis(DiagnosisRef::new(
            "obs_2026_09_14_marco",
            "non distingue la forma ridotta dalla divisione",
            Some(PersonId::fixture(1)),
        ));
        assert_ne!(senza.prompt_hash(), con.prompt_hash());
        assert!(con.canonical_bytes().contains("diagnosis-id: obs_2026_09_14_marco"));
        assert!(con.canonical_bytes().contains("diagnosis-witness: person_0001"));
        assert!(senza.canonical_bytes().contains("diagnosis-id: none"));
    }

    #[test]
    fn un_campo_vuoto_rende_il_lock_impossibile() {
        let e = GenerationRequest::try_new("", "g", CorpusHash::of_empty(), "s", "t", "e", "i")
            .unwrap_err();
        assert!(matches!(e, Error::LockIncompleto { campo: "model_id" }));
        let e = GenerationRequest::try_new("m", "  ", CorpusHash::of_empty(), "s", "t", "e", "i")
            .unwrap_err();
        assert!(matches!(e, Error::LockIncompleto { campo: "generator_version" }));
    }

    #[test]
    fn il_lock_porta_il_timestamp_che_gli_e_stato_dato() {
        let l = richiesta().lock(Millis(1_757_000_000_000)).unwrap();
        assert_eq!(l.at, Millis(1_757_000_000_000));
        assert_eq!(l.model_id, "claude-sonnet-4-5");
        assert_eq!(l.generator_version, "kbs-intake/1");
        assert_eq!(l.corpus_hash, CorpusHash::of_empty().as_str());
    }
}
