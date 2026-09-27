//! D12: l'uscita è `rm -rf`.
//!
//! Il corpus è una cartella di file versionata su git, e il server
//! dell'istituto è un mirror. Quindi il database non è il padrone di niente, e
//! un formato di interscambio che perda la provenienza non è un formato
//! d'interscambio: è una copia che qualcuno dovrà riconciliare a mano.
//!
//! Il formato è **a colonne fisse**, TSV con intestazione, ed è qui dichiarato
//! il suo insieme di colonne. «A colonne fisse» significa tre cose, e le tre
//! contano:
//!
//! 1. **il numero e l'ordine delle colonne sono dichiarati** in
//!    [`FIXED_COLUMNS`], e un lettore può indicizzarle per posizione senza
//!    leggere l'intestazione;
//! 2. **l'intestazione c'è**, e vale quanto la lista: chi legge con un foglio di
//!    calcolo non deve sapere il formato;
//! 3. **la riga è ordinata** per id, così due esecuzioni sullo stesso stato
//!    danno lo stesso byte. Un export non riproducibile non è un backup, e D11
//!    chiede riproducibilità anche quando nessuno sta guardando.
//!
//! # Quali righe
//!
//! Solo gli argomenti **citabili**, cioè `in-corso` con ratifica valida. Esportare
//! una bozza significherebbe pubblicare materiale non ratificato, che è la cosa
//! che D4 vieta. La colonna `state` c'è lo stesso, ed è sempre `in-corso`: un
//! formato a colonne fisse che cambia forma secondo il contenuto non è a colonne
//! fisse.
//!
//! # Chi può esportare
//!
//! Chi insegna il corso. L'esportazione è la via d'uscita dal prodotto, e una
//! via d'uscita che uno studente può prendere non è una via d'uscita: è una
//! stampa. La porta è [`Error::NotACourseTeacher`].

use kbs_core::{check_citable, ArgumentId, CourseId, Origin, PersonId, PublicationState, Relation};

use crate::error::{Error, Result};
use crate::store::Store;
use crate::types::SourceStatus;

/// L'insieme di colonne dell'esportazione, in ordine.
///
/// Ogni colonna ha una ragione per esserci, e la ragione è una domanda che chi
/// riceve l'export deve poter fare senza chiedere:
///
/// | # | colonna | perché |
/// |---|---|---|
/// | 1 | `course` | a quale corso appartiene il materiale |
/// | 2 | `slug` | com'è chiamata la cartella del corso su disco |
/// | 3 | `argument` | l'identità stabile, derivata dal percorso |
/// | 4 | `rel_path` | il file, per riaprirlo |
/// | 5 | `title` | il concetto insegnabile, in una riga |
/// | 6 | `summary` | di che cosa parla e a chi |
/// | 7 | `state` | sempre `in-corso`; c'è perché la forma è fissa |
/// | 8 | `content_hash` | l'identità del contenuto, per il re-hash |
/// | 9 | `origin_kind` | `human`, `generated` o `derived` |
/// | 10 | `origin_model` | il modello, se c'è stato (D10) |
/// | 11 | `origin_prompt_hash` | il prompt, per hash |
/// | 12 | `origin_corpus_hash` | il corpus al momento della generazione (D11) |
/// | 13 | `origin_generator_version` | la versione del generatore |
/// | 14 | `ratified_by` | **chi si è preso la responsabilità** |
/// | 15 | `ratified_at` | quando |
/// | 16 | `ratified_contract_hash` | su quale contenuto (D6) |
/// | 17 | `ratified_note` | che cosa aveva verificato |
/// | 18 | `prerequisites` | il grafo, in forma piatta |
/// | 19 | `created_at` | millisecondi UTC |
/// | 20 | `updated_at` | millisecondi UTC |
///
/// Le colonne 9-13 e 14-17 sono quelle che rendono l'export **proponibile**: chi
/// riceve il file sa chi ha scritto, con quale modello e chi ha ratificato. Un
/// export senza quelle è un export di materiale scolastico, e in Italia un
/// materiale scolastico ha un autore.
pub const FIXED_COLUMNS: [&str; 20] = [
    "course",
    "slug",
    "argument",
    "rel_path",
    "title",
    "summary",
    "state",
    "content_hash",
    "origin_kind",
    "origin_model",
    "origin_prompt_hash",
    "origin_corpus_hash",
    "origin_generator_version",
    "ratified_by",
    "ratified_at",
    "ratified_contract_hash",
    "ratified_note",
    "prerequisites",
    "created_at",
    "updated_at",
];

/// Il carattere di separazione. Il tab, e non la virgola: i valori contengono
/// virgole (i titoli italiani sono pieni di elisioni e virgole) e il TSV con
/// tab non richiede un parser di quoting.
const SEP: char = '\t';

impl Store {
    /// Esporta un corso a colonne fisse, per chi insegna quel corso.
    ///
    /// L'output è il file di testo completo, intestazione compresa, con `\n` come
    /// fine riga. Il formato è TSV senza quoting e con escape esplicito: `\t`,
    /// `\n` e `\\` diventano `\\t`, `\\n` e `\\\\`, così ogni riga ha sempre
    /// esattamente [`FIXED_COLUMNS`].len() campi e un valore con un tab dentro non
    /// sposta le colonne.
    ///
    /// Le righe sono quelle **citabili** ([`check_citable`]), non semplicemente
    /// quelle in stato `in-corso`: un argomento pubblicato e poi modificato ha
    /// una ratifica vecchia e non è più citabile, e mandarlo in export
    /// significherebbe pubblicare materiale che nessuno ha ratificato nella
    /// forma in cui è stato scritto.
    pub fn export_fixed_columns(&self, person: &PersonId, course: &CourseId) -> Result<String> {
        let relations = self.relations_of(person, course)?;
        if !relations.contains(&Relation::Teaches) {
            return Err(Error::NotACourseTeacher {
                person: person.clone(),
                course: course.clone(),
            });
        }
        let slug: String = self.conn.query_row(
            "SELECT slug FROM sources WHERE id = ?1 AND status = ?2",
            rusqlite::params![course.0, SourceStatus::Active.as_str()],
            |r| r.get(0),
        )?;

        let mut out = String::new();
        out.push_str(&FIXED_COLUMNS.join(&SEP.to_string()));
        out.push('\n');
        for argument in self.list_by_state(course, PublicationState::InCorso)? {
            if check_citable(&argument).is_err() {
                continue;
            }
            let (origin_kind, model_id, prompt_hash, corpus_hash, generator_version) =
                match &argument.origin {
                    Origin::Human { .. } => ("human", "", "", "", ""),
                    Origin::Generated { lock, .. } => (
                        "generated",
                        lock.model_id.as_str(),
                        lock.prompt_hash.as_str(),
                        lock.corpus_hash.as_str(),
                        lock.generator_version.as_str(),
                    ),
                    Origin::Derived { .. } => ("derived", "", "", "", ""),
                };
            // `check_citable` ha appena detto che è citabile, quindi la
            // ratifica c'è. «Dovrebbe esserci» non è un controllo: se un domani
            // le due regole divergono, la risposta è un errore che lo dice e non
            // un panic nel mezzo di un'esportazione.
            let Some(ratified) = argument.ratified.as_ref() else {
                return Err(Error::Corrupt {
                    table: "arguments",
                    field: "ratified_by",
                    reason: "argomento dichiarato citabile senza ratifica".into(),
                });
            };
            let fields: Vec<String> = vec![
                course.0.clone(),
                slug.clone(),
                argument.id.0.clone(),
                argument.rel_path.clone().unwrap_or_default(),
                argument.title.clone(),
                argument.summary.clone(),
                "in-corso".to_string(),
                argument.content_hash.clone(),
                origin_kind.to_string(),
                model_id.to_string(),
                prompt_hash.to_string(),
                corpus_hash.to_string(),
                generator_version.to_string(),
                ratified.by.0.clone(),
                ratified.at.0.to_string(),
                ratified.contract_hash.clone(),
                ratified.note.clone(),
                self.prerequisite_line(&argument.id)?,
                argument.created_at.0.to_string(),
                argument.updated_at.0.to_string(),
            ];
            out.push_str(
                &fields
                    .iter()
                    .map(|f| escape(f))
                    .collect::<Vec<_>>()
                    .join(&SEP.to_string()),
            );
            out.push('\n');
        }
        Ok(out)
    }

    /// I prerequisiti di un argomento, in una colonna, separati da spazi.
    ///
    /// Gli id non contengono spazi (`arg_` + 16 hex), quindi lo spazio è un
    /// separatore esatto e non serve un quoting. L'ordine è quello del database
    /// (`ORDER BY`), quindi la riga è riproducibile.
    fn prerequisite_line(&self, argument: &ArgumentId) -> Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT prerequisite_id FROM argument_prerequisites \
             WHERE argument_id = ?1 ORDER BY prerequisite_id",
        )?;
        let rows = stmt.query_map([&argument.0], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out.join(" "))
    }
}

/// Escape TSV: il tab, il ritorno a capo e la barra rovescia.
///
/// Una riga che contiene un tab non sposta le colonne, e una riga che contiene
/// un `\n` non finge di essere due righe. Il formato resta a colonne fisse anche
/// quando i dati hanno caratteri strani, che è l'unico modo in cui «a colonne
/// fisse» è una garanzia e non una speranza.
fn escape(field: &str) -> String {
    if !field.contains(['\t', '\n', '\\']) {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len() + 8);
    for c in field.chars() {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out
}
