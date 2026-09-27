//! I tipi che `kbs-store` aggiunge a quelli di `kbs-core`.
//!
//! `kbs-core` è il contratto e non si tocca. Qui ci finisce ciò che il
//! dominio non ha ancora e di cui la persistenza ha bisogno per non dover
//! inventare una seconda codifica: sono tutti wrapper di `String` o di valori
//! triviali, nessuno introduce una regola nuova. Ogni tipo di questo modulo
//! esiste per una ragione dichiarata nel suo doc, e le ragioni sono:
//!
//! * [`Source`] / [`SourceStatus`] — l'albero del corso su disco. `CourseId` è
//!   l'identità, non il luogo.
//! * [`Person`] — una persona. `kbs-core` ha `PersonId` e nient'altro, e il
//!   nome serve a chi legge, non a chi decide.
//! * [`CourseRelation`] — una relazione di corso con le sue date. `Relation` in
//!   `kbs-core` è l'enum; qui è l'iscrizione.
//! * [`Register`] — il registro a cui appartiene una sessione append-only.
//!   Il suo identificatore **non** è qui: è [`SessionId`], di `kbs-verify`, che
//!   è il crate puro e proprietario del concetto. Il store lo riusa invece di
//!   duplicarlo: due newtype con lo stesso nome che non sono lo stesso tipo sono
//!   un bug che aspetta.
//! * [`ObservationDraft`] / [`GradingDraft`] — gli stessi record di `kbs-core`
//!   **senza `seq`**: il numero lo assegna il registro, perché è la quantità
//!   che la catena di hash di D6 ordina e non può essere scelto da chi scrive.
//! * [`Rubric`] / [`RubricVersion`] / [`GradeLevel`] — senza una scala, un voto
//!   non è riproducibile (D6).
//! * [`ChunkId`] / [`ArtifactChunk`] / [`ChunkKind`] — l'unità indicizzata.
//! * [`SearchHit`] — un risultato, con il suo `rank` e l'argomento che lo
//!   contiene.
//! * [`GenerationEvent`] — un evento di generazione (D10).

use kbs_core::{ArgumentId, CohortId, CourseId, Millis, PersonId};
use serde::{Deserialize, Serialize};

/// L'identificatore di una sessione di registro.
///
/// Non è definito qui: `kbs-verify` lo possiede, perché è il crate puro e
/// senza storage, e un auditor che riceve l'export di D12 non deve dover
/// dipendere da `rusqlite` per ricontrollare una firma. Riusarlo evita il
/// doppione; se il tipo cambiasse firma, cambierebbe qui un posto solo.
pub use kbs_verify::SessionId;

/// A quale registro appartiene una sessione.
///
/// `SeqInSession` sta dentro il suo registro: un seq 1 di `observations` e un
/// seq 1 di `gradings` non sono la stessa posizione, e misurarli insieme sarebbe
/// un errore che nessuno nota subito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Register {
    Observations,
    Gradings,
}

impl Register {
    pub fn as_str(self) -> &'static str {
        match self {
            Register::Observations => "observations",
            Register::Gradings => "gradings",
        }
    }

    pub fn from_db(raw: &str) -> Option<Register> {
        match raw {
            "observations" => Some(Register::Observations),
            "gradings" => Some(Register::Gradings),
            _ => None,
        }
    }
}

/// Lo stato di un albero del corso su disco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceStatus {
    /// Il corso c'è e si indicizza.
    Active,
    /// Conservato, non più in uso. Le sue unità restano citabili.
    Archived,
}

impl SourceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceStatus::Active => "active",
            SourceStatus::Archived => "archived",
        }
    }

    pub fn from_db(raw: &str) -> Option<SourceStatus> {
        match raw {
            "active" => Some(SourceStatus::Active),
            "archived" => Some(SourceStatus::Archived),
            _ => None,
        }
    }
}

/// L'albero del corso su disco.
///
/// D12: il corpus è una cartella di file versionata su git, e il server
/// dell'istituto è un mirror. Questa riga dice dove sta quella cartella e a che
/// stato è; non è un contenitore, non ha righe figlie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: CourseId,
    /// Il nome della cartella. Unico: due corsi non possono avere lo stesso slug.
    pub slug: String,
    /// Percorso relativo alla radice del corpus.
    pub rel_path: String,
    pub status: SourceStatus,
    pub registered_at: Millis,
    /// Ultima passata di indicizzazione, e hash del corpus in quel momento:
    /// è ciò che rende riproducibile una generazione (D11).
    pub last_scan_at: Option<Millis>,
    pub corpus_hash: Option<String>,
}

/// Una persona. Nessun ruolo: i ruoli sono relazioni (D5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub display_name: String,
    pub created_at: Millis,
}

/// Una relazione di corso, con le sue date.
///
/// `since` rientra nella chiave perché una relazione si può riprendere:
/// iscritto, fuori, iscritto di nuovo sono tre fatti. «Ora è iscritto» è
/// `until IS NULL`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseRelation {
    pub person: PersonId,
    pub course: CourseId,
    pub relation: kbs_core::Relation,
    pub since: Millis,
    pub until: Option<Millis>,
}

/// Un'osservazione **prima** che il registro le assegni il `seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationDraft {
    pub id: String,
    pub student: PersonId,
    pub course: CourseId,
    pub cohort: CohortId,
    pub argument: ArgumentId,
    pub evidence: kbs_core::Evidence,
    pub judged_by: Option<kbs_core::GraderKind>,
    pub at: Millis,
}

/// Una valutazione **prima** che il registro le assegni il `seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradingDraft {
    pub id: String,
    pub student: PersonId,
    pub course: CourseId,
    pub argument: ArgumentId,
    pub kind: kbs_core::GraderKind,
    pub graded_by: PersonId,
    /// Deve esistere in `rubric_versions`: un voto senza rubric non è
    /// riproducibile, e la chiave esterna lo rende tale anche per chi scrive SQL.
    pub rubric_version: String,
    pub grade: String,
    pub at: Millis,
    pub contested: Option<kbs_core::Contestation>,
}

/// Una rubrica, e la sua versione.
///
/// Il `grading` porta l'id della **versione**, non quello della rubrica: due
/// versioni non possono condividere una scala senza perdere il significato, e
/// quindi la scala va identificata dalla versione.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rubric {
    pub id: String,
    pub course: CourseId,
    pub title: String,
    pub created_at: Millis,
}

/// Un livello della scala. `points` è la posizione nell'ordine, non il voto
/// numerico: la scala del docente è la sua, e qui si conserva quella.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradeLevel {
    /// Il valore che va in `grading.grade`.
    pub grade: String,
    pub label: String,
    pub points: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RubricVersion {
    pub id: String,
    pub rubric: String,
    pub version: String,
    pub scale: Vec<GradeLevel>,
    pub defined_at: Millis,
    pub note: String,
}

/// L'unità indicizzata.
///
/// Una riga per argomento per ora (`kind = Argument`, `ord = 0`), con i sei campi
/// che `kb` indicizza più `contract`. La forma ammette righe per sezione,
/// contratto ed esercizio: sono l'estensione naturale, e il motivo per cui
/// l'unità non si chiama «documento».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactChunk {
    pub argument: ArgumentId,
    pub course: CourseId,
    pub kind: ChunkKind,
    pub ord: i64,
    pub rel_path: String,
    pub title: String,
    pub body: String,
    pub headings: String,
    pub code: String,
    pub prompt: String,
    /// Le otto sezioni di D7. Indicizzate come contenuto, non come allegato.
    pub contract: String,
    pub updated_at: Millis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChunkId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChunkKind {
    Argument,
    Section,
    Contract,
    Exercise,
}

impl ChunkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChunkKind::Argument => "argument",
            ChunkKind::Section => "section",
            ChunkKind::Contract => "contract",
            ChunkKind::Exercise => "exercise",
        }
    }

    pub fn from_db(raw: &str) -> Option<ChunkKind> {
        match raw {
            "argument" => Some(ChunkKind::Argument),
            "section" => Some(ChunkKind::Section),
            "contract" => Some(ChunkKind::Contract),
            "exercise" => Some(ChunkKind::Exercise),
            _ => None,
        }
    }
}

/// Un risultato di ricerca.
///
/// `rank` è il `bm25` di FTS5: **più negativo è migliore**. Il nome è `rank` e
/// non `score` perché il segno è inverso e chiamarlo `score` avrebbe fatto
/// ordinare al contrario metà dei chiamanti.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub argument: ArgumentId,
    pub course: CourseId,
    pub rank: f64,
}

/// Un evento di generazione: quando, da chi, e con quale lock (D10).
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationEvent {
    pub lock: kbs_core::ModelLock,
    pub argument: ArgumentId,
    pub requester: PersonId,
    pub at: Millis,
}

impl GenerationEvent {
    /// L'id del lock, derivato dal contenuto: due lock uguali sono lo stesso
    /// lock. Esposto perché la riga di `arguments` che porta una provenienza
    /// `generated` referenzia esattamente questo id.
    pub fn lock_id(&self) -> String {
        crate::codec::lock_id(&self.lock)
    }
}
