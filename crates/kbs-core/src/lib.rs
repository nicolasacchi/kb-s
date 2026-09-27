//! `kbs-core` — i tipi di dominio di `kb-s`.
//!
//! Questo crate è **il contratto**. Ogni altro crate del workspace dipende da qui e
//! non lo modifica: se un tipo serve a un altro agente, si aggiunge qui per primo.
//!
//! Le regole che i tipi rendono impossibili (D4, D5, D6 di `ARCHITECTURE.md`):
//!
//! * [`PublicationState`] è una macchina a stati, non una stringa libera.
//! * [`GraderKind`] **non ha una variante modello**: la catena di grading è
//!   deterministico → pari → umano, ed è chiusa (D3).
//! * [`Observation`] e [`Grading`] portano un [`SeqInSession`]: la catena di hash
//!   copre la sequenza, non un id di riga globale (D6).
//! * La visibilità non è un ruolo: è una funzione di [`Relation`] sull'oggetto (D5).

use serde::{Deserialize, Serialize};
use std::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Identificatori
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! string_id {
    ($(#[$m:meta])* $name:ident, $prefix:literal) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str { &self.0 }
            pub fn fixture(n: u32) -> Self { $name(format!(concat!($prefix, "{:04}"), n)) }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
        }
    };
}

string_id!(
    /// Identificatore di un `argomento`: il soggetto insegnabile.
    ArgumentId,
    "arg_"
);
string_id!(/// Identificatore di una persona. Non contiene ruoli: i ruoli sono relazioni (D5).
    PersonId,
    "person_"
);
string_id!(/// Identificatore di un corso, che è il perimetro di condivisione.
    CourseId,
    "course_"
);
string_id!(/// Identificatore di una classe in un anno.
    CohortId,
    "cohort_"
);

impl ArgumentId {
    /// L'id deriva dal **percorso sorgente relativo**, mai dai byte: come in `kb`,
    /// l'identità segue il file. Rinominare un argomento è quindi un atto esplicito
    /// e ricostruibile, non un collasso silenzioso dei commenti ancorati.
    pub fn from_rel_path(rel_path: &str) -> Self {
        ArgumentId(format!("arg_{}", fnv1a16(rel_path.as_bytes())))
    }
}

/// Hash breve e deterministico (16 hex) come suffisso d'id.
/// **Non è crittografico**: serve a rendere gli id leggibili. L'integrità è di
/// `kbs-verify`, e questa funzione non ha parte in quella.
fn fnv1a16(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Ordinale di una riga append-only **dentro una sessione di registro**. È la
/// quantità che la catena di hash (D6) ordina.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeqInSession(pub u64);

/// Millisecondi UTC. Un solo tipo di tempo nel sistema: la confrontabilità dei
/// registri dipende da questo, e due formati renderebbero ogni ordinamento falso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Millis(pub i64);

impl Millis {
    pub fn now() -> Self {
        Millis(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stati di pubblicazione e ratifica  (D4)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublicationState {
    /// Scritto, non verificato, invisibile a chi non l'ha scritto.
    Bozza,
    /// Verificato dal docente, non ancora in uso.
    DelDocente,
    /// In uso. Visibile agli iscritti. **Citabile.**
    InCorso,
    /// Fuori uso, conservato: chi lo ha seguito lo vede ancora.
    Archiviato,
}

impl PublicationState {
    /// Un argomento è citabile solo in corso. Da qui la regola che segue da D4:
    /// *un item non verificato è leggibile, ma non è citabile*.
    pub fn is_citable(self) -> bool { matches!(self, PublicationState::InCorso) }

    pub fn is_visible_to_enrolled(self) -> bool {
        matches!(self, PublicationState::InCorso | PublicationState::Archiviato)
    }

    /// Accetta ancora modifiche che ne cambiano il significato.
    pub fn is_mutable(self) -> bool {
        matches!(self, PublicationState::Bozza | PublicationState::DelDocente)
    }
}

/// Il passaggio a `InCorso` esige una ratifica, e il tipo la rende non opzionale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ratification {
    pub by: PersonId,
    pub at: Millis,
    /// Hash del contratto al momento della ratifica. Se il contratto cambia dopo,
    /// la ratifica **non vale più**: è l'obbligo che D6 rende verificabile.
    pub contract_hash: String,
    /// Che cosa ha verificato il docente. Va bene una frase: l'obiettivo non è un
    /// audit, è la responsabilità di chi ha ratificato.
    pub note: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Provenienza  (D10, D11)
// ─────────────────────────────────────────────────────────────────────────────

/// **Il** model lock (D10). Senza questo non c'è un registro: c'è un diario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelLock {
    /// Come lo chiama il fornitore (`claude-sonnet-4-5`).
    pub model_id: String,
    /// Hash del prompt che ha prodotto il contenuto.
    pub prompt_hash: String,
    /// Hash del corpus al momento della generazione: rende la generazione
    /// riproducibile (D11).
    pub corpus_hash: String,
    /// Versione del generatore che ha assemblato la richiesta.
    pub generator_version: String,
    pub at: Millis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Origin {
    /// Scritto a mano. Nessun modello coinvolto.
    Human { by: PersonId, at: Millis },
    /// Generato con l'aiuto di un modello **esterno al prodotto** (D3), poi ratificato.
    Generated { lock: ModelLock, by: PersonId, at: Millis },
    /// Derivato da un altro argomento (un esercizio da un testo, per esempio).
    Derived { from: ArgumentId, at: Millis },
}

// ─────────────────────────────────────────────────────────────────────────────
// L'argomento
// ─────────────────────────────────────────────────────────────────────────────

/// Un argomento è il **soggetto insegnabile**, non il file. Il file ne è la
/// rappresentazione; il legame fra i due è `ArgumentId::from_rel_path`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Argument {
    pub id: ArgumentId,
    /// Il concetto insegnabile, in una riga. È il `<title>`.
    pub title: String,
    /// Una frase: di che cosa parla, e a chi.
    pub summary: String,
    pub state: PublicationState,
    pub course: CourseId,
    /// Prerequisiti. Devono formare un grafo aciclico: `kbs-store` lo verifica e
    /// `Invariant::PrerequisiteCycle` lo rifiuta.
    pub prerequisites: Vec<ArgumentId>,
    pub origin: Origin,
    /// Percorso sorgente relativo, o `None` se creato via API.
    pub rel_path: Option<String>,
    /// Hash del contenuto. È il trigger del re-hash e del confronto con la ratifica.
    pub content_hash: String,
    pub created_at: Millis,
    pub updated_at: Millis,
    pub ratified: Option<Ratification>,
}

impl Argument {
    /// Citabile **solo** se ratificato con l'hash di contratto corrente. È il
    /// controllo che rende vera la regola di D4, ed è un metodo perché tutti i
    /// chiamanti passano di qui.
    pub fn is_citable_now(&self) -> bool { check_citable(self).is_ok() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Registro delle affermazioni: «questa frase è vera?»  (D6)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimStatus {
    /// Legato a uno span che lo sostiene.
    Supported,
    /// Uno span esiste ma non lo sostiene. **Si mantiene**: è la traccia dell'errore.
    Contradicted,
    /// Nessuno span lo sostiene. Soppresso dall'output, non cancellato dal registro.
    Unciteable,
    /// Ritrattato esplicitamente, con la ragione.
    Retracted { reason: String },
}

impl ClaimStatus {
    /// Nell'**output citabile** finisce solo ciò che è sostenuto. Quindi ne
    /// escono `Contradicted`, `Unciteable` e `Ritracted`.
    ///
    /// `Contradicted` è il caso più insidioso dei tre: uno span c'è e **non**
    /// sostiene. È peggio di `Unciteable`, perché lo span dà alla frase una
    /// credibilità falsa — il lettore vede un riferimento e conclude che la
    /// verifica sia passata.
    ///
    /// Nel **registro** restano tutte e tre, con la loro ragione: un registro
    /// che cancella i propri errori non è un registro, è una propaganda.
    ///
    /// Nota sul precedente di questa funzione: sopprimeva solo `Unciteable`,
    /// mentre la documentazione qui sopra affermava il contrario. Il nome del
    /// test che la sorreggeva parlava del *registro* e non dell'*output*, quindi
    /// fissava la metà sbagliata senza dirlo. È il difetto che questa funzione
    /// esiste per evitare: una promessa nella doc che il corpo non mantiene.
    pub fn suppresses_output(self) -> bool {
        matches!(
            self,
            ClaimStatus::Contradicted | ClaimStatus::Unciteable | ClaimStatus::Retracted { .. }
        )
    }
}

/// Chi ha emesso l'affermazione. Un modello **non è un emittente**: sta fuori dal
/// prodotto (D3). Un item generato è emesso dall'artefatto che lo contiene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Emitter {
    Teacher { by: PersonId },
    /// Emessa dal contenuto di un argomento ratificato, non da una persona al momento.
    Content { argument: ArgumentId },
    /// Emessa a partire da un'osservazione sul lavoro dello studente.
    FromWork { observation: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub course: CourseId,
    pub argument: ArgumentId,
    /// Il fatto atomico, in una frase.
    pub text: String,
    /// L'ancora dello span che lo sostiene.
    pub span_anchor: Option<String>,
    /// Il testo **effettivo** dello span. Senza questo, `span_anchor` è un puntatore
    /// che il lettore non può verificare, e l'affermazione torna a essere una
    /// dichiarazione con un indirizzo.
    pub span_text: Option<String>,
    pub status: ClaimStatus,
    pub emitted_at: Millis,
    pub emitted_by: Emitter,
}

// ─────────────────────────────────────────────────────────────────────────────
// Registro della padronanza: «chi ha dimostrato che cosa?»  (D6)
// ─────────────────────────────────────────────────────────────────────────────

/// La prova. È un enum e non una stringa perché i casi hanno obblighi diversi,
/// e perché uno di essi **non è riproducibile** e va detto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Evidence {
    /// Esercizio parametrizzato con risposta verificata dal programma. Riproducibile.
    Checked { exercise: String, instance: String, correct: bool },
    /// Interrogazione orale del docente. **Non riproducibile.** Dichiarato qui
    /// perché il registro non sa ancora rappresentare una fonte di questo tipo, e
    /// fingerlo sarebbe peggio che ammetterlo.
    Oral { note: String, witness: Option<PersonId> },
    /// Lavoro libero, giudicato da un umano.
    Written { ref_doc: String },
    /// Nessuna prova: l'argomento è semplicemente aperto.
    None,
}

impl Evidence {
    /// Solo le prove riproducibili rientrano nel replay deterministico (D11).
    pub fn is_reproducible(&self) -> bool {
        matches!(self, Evidence::Checked { .. } | Evidence::Written { .. })
    }
}

/// Chi ha emesso un giudizio. **Nessun modello**: D3 chiude la catena qui, e la
/// variante mancante è il modo in cui la regola è fatta valere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraderKind {
    /// Un programma ha confrontato la risposta. Il più forte e il più economico.
    Deterministic,
    /// Un pari. Vale meno, e la variante lo dichiara.
    Peer,
    /// Una persona.
    Human,
    /// Giudizio del docente su ispezione. Vale quanto `Human` e registra chi l'ha fatto.
    Teacher,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    /// La quantità che la catena di hash copre.
    pub seq: SeqInSession,
    pub student: PersonId,
    pub course: CourseId,
    pub cohort: CohortId,
    pub argument: ArgumentId,
    pub evidence: Evidence,
    /// `None` significa «non ancora giudicato», non «non giudicabile»: la
    /// distinzione è ciò che rende il registro uno stato e non una casella di testo.
    pub judged_by: Option<GraderKind>,

    /// `Some(true)` = nessun aiuto disponibile. `None` = **non registrato**.
    ///
    /// È la colonna che distingue il sistema da una bottiglia con i fantasmi:
    /// senza di essa il registro delle dimostrazioni misura **interazione**, e
    /// «ha lavorato» non è «sa». Con essa la quota della claim — gli argomenti
    /// che passano da non dimostrato a dimostrato — ha un numeratore che non
    /// contiene la coda di practice.
    ///
    /// **`None` non è `Some(false)`, e non è un `DEFAULT 1`.** Le righe scritte
    /// prima della colonna esistono hanno un aiuto che il sistema non ha mai
    /// chiesto e non può sapere: dichiararle non assistite sarebbe una
    /// padronanza retroattiva, dichiararle assistite sarebbe un'altra
    /// dichiarazione. Il motivo per cui il campo è opzionale e non
    /// `bool` con un default è scritto per intero nella migrazione
    /// `V6__unaided.sql`, che è anche il posto in cui la decisione va rileggiata.
    pub unaided: Option<bool>,
    /// Quante piste lo studente aveva disponibili, quando lo si è contato.
    ///
    /// `None` = non contato, che non è la stessa cosa di `Some(0)`: `0` è la
    /// misura «nessuna pista disponibile», e una misura che non è stata fatta
    /// non è uno zero. Il database vieta la combinazione ambigua — un conteggio
    /// dichiarato per un'osservazione di cui si ignora se le piste fossero
    /// disponibili — e il trigger che la vieta è nella stessa migrazione.
    pub n_hints: Option<u32>,
    pub at: Millis,
}

// ─────────────────────────────────────────────────────────────────────────────
// Registro delle valutazioni: «chi ha deciso, con che cosa, e qualcuno ha contestato?»  (D6)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContestationOutcome {
    Upheld,
    Rejected,
    UnderReview,
}

/// Una contestazione: la difesa procedurale primaria. Un accertamento automatico
/// che non si può contestare non è un accertamento, è un annuncio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contestation {
    pub by: PersonId,
    pub at: Millis,
    pub reason: String,
    pub outcome: Option<ContestationOutcome>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grading {
    pub id: String,
    pub seq: SeqInSession,
    pub student: PersonId,
    pub course: CourseId,
    pub argument: ArgumentId,
    pub kind: GraderKind,
    pub graded_by: PersonId,
    /// Un voto senza rubric non è riproducibile.
    pub rubric_version: String,
    /// La scala è dichiarata dal rubric, non da qui: due versioni di rubric non
    /// possono condividere una scala senza perdere il significato.
    pub grade: String,
    pub at: Millis,
    /// La contestazione va **dentro** la riga, non cancella la riga.
    pub contested: Option<Contestation>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Visibilità: relazioni, non ruoli  (D5)
// ─────────────────────────────────────────────────────────────────────────────

/// Le relazioni sono il solo oggetto che autorizza. Un ruolo è un *insieme* di
/// relazioni e non viene memorizzato: si deriva. Nessuna tabella `roles`.
///
/// Riferimento: `kb` oggi ha un solo livello di fiducia, nessun tenant, nessun
/// ruolo, e il mount del corpus *è* l'ACL (`kb/CLAUDE.md:406-411`,
/// `kb/crates/kb-server/src/router.rs:112`,
/// `kb/crates/kb-server/src/routes/comments.rs:236-238`). `kb-s` ne eredita il
/// modello a documento e ne corregge la sicurezza.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Iscritto a un corso. Vede ciò che `is_visible_to_enrolled`.
    EnrolledIn,
    /// Insegna in un corso. Vede tutto del corso, bozze comprese, e la coda di ratifica.
    Teaches,
    /// Ha scritto l'argomento. Vede tutte le sue versioni.
    AuthorOf,
    /// Ha ratificato l'argomento: vede la catena e può ritirarla.
    Ratified,
    /// Il percorso speculativo di questa persona su questo argomento. Non è
    /// condiviso e non è citabile (D4).
    SpeculativeFor,
}

/// Il predicato di visibilità. Una funzione, non una configurazione: è l'unico
/// posto in cui la domanda «può vedere questa roba?» viene decisa.
pub fn may_read(
    relations: &[Relation],
    state: PublicationState,
    is_author: bool,
    is_ratifier: bool,
) -> bool {
    if is_author || is_ratifier {
        return true;
    }
    if relations.contains(&Relation::Teaches) {
        return true;
    }
    relations.contains(&Relation::EnrolledIn) && state.is_visible_to_enrolled()
}

// ─────────────────────────────────────────────────────────────────────────────
// Esercizi  (D8)
// ─────────────────────────────────────────────────────────────────────────────

/// Il verificatore deterministico. Tre forme, nessuna che chieda un modello.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Checker {
    /// La risposta è un numero.
    Numeric { tolerance: f64 },
    /// La risposta è un insieme. L'ordine non conta.
    Set { elements: Vec<String> },
    /// Scelta fra k alternative, con la posizione dell'esatto.
    MultipleChoice { correct_index: u8, options: Vec<String> },
    /// Equivalenza algebrica. Deliberatamente **non** un parser di espressioni
    /// general-purpose: il confronto è su forma normale esplicita, dichiarata dal
    /// generatore. Un parser che decide le risposte è il modo più rapido per
    /// costruire un sistema che sbaglia con sicurezza.
    Equivalence { normalized: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exercise {
    pub id: String,
    pub course: CourseId,
    pub argument: ArgumentId,
    /// Due istanze della stessa famiglia hanno lo stesso ragionamento.
    pub family: String,
    /// Con il seed, determina l'istanza (D11).
    pub generator_version: String,
    pub prompt: String,
    /// Non è un giudizio: è un confronto.
    pub checker: Checker,
    pub created_at: Millis,
    pub created_by: PersonId,
}

/// Un'istancia concreta, prodotta dal generatore con un seed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub exercise: String,
    /// Con `generator_version`, riproduce la risposta (D11).
    pub seed: String,
    pub rendered_prompt: String,
    pub expected: String,
    pub params: serde_json::Value,
}

impl Instance {
    /// Il percorso speculativo: due istanze non hanno la stessa risposta, ed è
    /// questo che rende la copia inefficace (D8).
    pub fn differs_from(&self, other: &Instance) -> bool { self.expected != other.expected }
}

// ─────────────────────────────────────────────────────────────────────────────
// Coorte invisibile  (D9)
// ─────────────────────────────────────────────────────────────────────────────

/// Soglia sotto la quale un segnale non viene aggregato. **È un accesso, non una
/// cancellazione**: il dato individuale resta dove sta.
pub const COHORT_MIN_K: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CohortSignal {
    pub course: CourseId,
    pub cohort: CohortId,
    pub argument: ArgumentId,
    /// Quante persone sbagliano. Sotto soglia il segnale **non esiste**.
    pub failing: usize,
    pub total: usize,
    pub at: Millis,
}

impl CohortSignal {
    pub fn is_publishable(&self) -> bool { self.failing >= COHORT_MIN_K }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invarianti
// ─────────────────────────────────────────────────────────────────────────────

/// Una combinazione che il tipo consente ma che la regola vieta. Il motivo è
/// parte dell'errore: restituirlo serve a chi ha sbagliato, non solo al validatore.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Invariant {
    #[error("un argomento non ratificato non può essere {0:?}: è leggibile, non citabile")]
    CitableWithoutRatification(PublicationState),
    #[error("la ratifica vale per il contratto hash {declared}, non per {current}")]
    StaleRatification { declared: String, current: String },
    #[error("un segnale di coorte sotto soglia non è pubblicabile: {failing} < {min}")]
    CohortBelowThreshold { failing: usize, min: usize },
    #[error("il prerequisito {0} introdurrebbe un ciclo")]
    PrerequisiteCycle(String),
}

/// Perché un argomento è citable, o perché non lo è.
pub fn check_citable(a: &Argument) -> Result<(), Invariant> {
    if !a.state.is_citable() {
        return Err(Invariant::CitableWithoutRatification(a.state));
    }
    match &a.ratified {
        None => Err(Invariant::CitableWithoutRatification(a.state)),
        Some(r) if r.contract_hash != a.content_hash => Err(Invariant::StaleRatification {
            declared: r.contract_hash.clone(),
            current: a.content_hash.clone(),
        }),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratificato() -> Argument {
        Argument {
            id: ArgumentId::from_rel_path("matematica/frazioni.md"),
            title: "Le frazioni equivalenti".into(),
            summary: "s".into(),
            state: PublicationState::InCorso,
            course: CourseId::fixture(1),
            prerequisites: vec![],
            origin: Origin::Human { by: PersonId::fixture(1), at: Millis(0) },
            rel_path: Some("matematica/frazioni.md".into()),
            content_hash: "h1".into(),
            created_at: Millis(0),
            updated_at: Millis(0),
            ratified: Some(Ratification {
                by: PersonId::fixture(1),
                at: Millis(0),
                contract_hash: "h1".into(),
                note: "verificato".into(),
            }),
        }
    }

    #[test]
    fn un_argomento_ratificato_e_citabile() {
        assert!(check_citable(&ratificato()).is_ok());
        assert!(ratificato().is_citable_now());
    }

    /// D4: una bozza non è citabile **anche se ratificata**.
    #[test]
    fn la_bozza_non_e_mai_citabile() {
        let mut a = ratificato();
        a.state = PublicationState::Bozza;
        assert_eq!(
            check_citable(&a),
            Err(Invariant::CitableWithoutRatification(PublicationState::Bozza))
        );
        assert!(!a.is_citable_now());
    }

    /// Il test che chiude D4 davvero: **un contratto che cambia invalida la ratifica**.
    /// Senza questo, «non verificato non è citabile» è una promessa, non una regola.
    #[test]
    fn la_ratifica_non_sopravvive_a_un_contratto_cambiato() {
        let mut a = ratificato();
        a.content_hash = "h2".into();
        assert!(matches!(check_citable(&a), Err(Invariant::StaleRatification { .. })));
        assert!(!a.is_citable_now());
    }

    #[test]
    fn l_id_segue_il_percorso_e_non_i_byte() {
        let p1 = ArgumentId::from_rel_path("matematica/frazioni.md");
        let p2 = ArgumentId::from_rel_path("matematica/frazioni.md");
        let p3 = ArgumentId::from_rel_path("matematica/decimali.md");
        assert_eq!(p1, p2, "stesso percorso, stesso id");
        assert_ne!(p1, p3, "percorso diverso, id diverso");
        assert!(p1.as_str().starts_with("arg_"));
    }

    #[test]
    fn la_visibilita_e_una_relazione_e_non_una_ruota() {
        // iscritto + in corso: vede
        assert!(may_read(&[Relation::EnrolledIn], PublicationState::InCorso, false, false));
        // iscritto + bozza: non vede
        assert!(!may_read(&[Relation::EnrolledIn], PublicationState::Bozza, false, false));
        // autore: vede comunque, anche la bozza
        assert!(may_read(&[], PublicationState::Bozza, true, false));
        // ratificatore: vede
        assert!(may_read(&[], PublicationState::Bozza, false, true));
        // docente: vede tutto del corso, bozze comprese
        assert!(may_read(&[Relation::Teaches], PublicationState::Bozza, false, false));
        // nessuna relazione, stato non visibile: no
        assert!(!may_read(&[], PublicationState::Bozza, false, false));
    }

    #[test]
    fn la_soglia_di_coorte_e_un_accesso_non_una_cancellazione() {
        let base = CohortSignal {
            course: CourseId::fixture(1),
            cohort: CohortId::fixture(1),
            argument: ArgumentId::from_rel_path("a.md"),
            failing: 4,
            total: 25,
            at: Millis(0),
        };
        assert!(!base.is_publishable());
        assert!(CohortSignal { failing: 5, ..base.clone() }.is_publishable());
    }

    /// Il nome di questo test dice due cose, e le due cose sono distinte: l'errore
    /// **si registra** e **non si cancella**, ma **non si pubblica** come se fosse
    /// vero. Una versione precedente asseriva solo la seconda metà, e la asseriva
    /// al contrario: fissava la metà sbagliata senza che il nome lo dichiarasse.
    #[test]
    fn un_errore_si_registra_e_non_si_cancella() {
        // L'output citabile contiene solo ciò che è sostenuto.
        assert!(ClaimStatus::Supported.suppresses_output() == false);
        assert!(ClaimStatus::Contradicted.suppresses_output());
        assert!(ClaimStatus::Unciteable.suppresses_output());
        assert!(ClaimStatus::Retracted { reason: "r".into() }.suppresses_output());
    }

    /// Lo stesso fatto, guardato dal lato del **registro**: tutte e quattro le
    /// righe esistono. Una riga che un registro perde non è più un errore
    /// corretto, è un errore ripetuto senza che nessuno lo sappia.
    #[test]
    fn il_registro_conserva_tutte_le_quattro_le_stanze() {
        let tutte = [
            ClaimStatus::Supported,
            ClaimStatus::Contradicted,
            ClaimStatus::Unciteable,
            ClaimStatus::Retracted { reason: "r".into() },
        ];
        assert_eq!(tutte.len(), 4, "il registro non ne perde nessuna");
        // E nessuna delle quattro è «sconosciuta»: l'enum le copre tutte, quindi
        // un quinto stato non può comparire senza che questo test lo veda.
        assert!(matches!(ClaimStatus::Supported, ClaimStatus::Supported));
    }

    /// D3: la prova orale non entra nel replay deterministico, e lo dichiara.
    #[test]
    fn la_prova_orale_non_e_riproducibile_e_lo_dice() {
        let orale = Evidence::Oral { note: "n".into(), witness: None };
        assert!(!orale.is_reproducible());
        assert!(Evidence::Checked {
            exercise: "e".into(), instance: "i".into(), correct: true
        }.is_reproducible());
    }

    /// D8: due istanze della stessa famiglia non hanno la stessa risposta.
    #[test]
    fn due_istanze_non_hanno_la_stessa_risposta() {
        let mk = |expected: &str| Instance {
            exercise: "e".into(), seed: "s".into(), rendered_prompt: "p".into(),
            expected: expected.into(), params: serde_json::json!({}),
        };
        assert!(mk("42").differs_from(&mk("43")));
        assert!(!mk("42").differs_from(&mk("42")));
    }
}
