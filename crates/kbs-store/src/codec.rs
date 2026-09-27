//! La traduzione fra i tipi di `kbs-core` e le colonne.
//!
//! Una variante con un payload si scrive in due modi, e la scelta è per
//! criterio, non per caso:
//!
//! * **colonne vere** quando i campi sono in numero fisso e sono oggetti del
//!   dominio che si vuole poter interrogare: `Origin`, `Emitter`, la
//!   contestazione. `arguments.origin_by` è una colonna perché «chi ha scritto
//!   questo» è una domanda che il database deve saper rispondere da solo.
//! * **kind + payload JSON** quando il payload è una lista di lunghezza
//!   variabile: `Evidence`, `Checker`. `Set { elements }` in colonne nullable
//!   sarebbe una bugia sulla forma; e mettere il JSON in una colonna sola, senza
//!   il `kind`, renderebbe la variante invisibile a `CHECK`.
//!
//! In entrambi i casi la decodifica di una riga sbagliata non fa panic: restituisce
//! [`Error::Corrupt`] dicendo quale campo non torna.

use kbs_core::{
    Checker, ClaimStatus, Contestation, ContestationOutcome, Emitter, Evidence, GraderKind,
    Origin, PublicationState, Ratification,
};
use serde_json::json;

use crate::error::{Error, Result};

/// Una colonna che il database ha accettato e che questo crate non sa leggere.
///
/// Il CHECK del database garantisce che il valore sia fra quelli previsti; un
/// valore fuori elenco significa che lo schema e il codice non parlano più, e la
/// risposta giusta è dirlo invece di tirare fuori un valore a caso.
fn bad(table: &'static str, field: &'static str, found: &str) -> Error {
    Error::Corrupt {
        table,
        field,
        reason: format!("`{found}` non è un valore noto"),
    }
}

// ── PublicationState ─────────────────────────────────────────────────────────

pub fn state_to_db(s: PublicationState) -> &'static str {
    match s {
        PublicationState::Bozza => "bozza",
        PublicationState::DelDocente => "del-docente",
        PublicationState::InCorso => "in-corso",
        PublicationState::Archiviato => "archiviato",
    }
}

pub fn state_from_db(raw: &str) -> Result<PublicationState> {
    match raw {
        "bozza" => Ok(PublicationState::Bozza),
        "del-docente" => Ok(PublicationState::DelDocente),
        "in-corso" => Ok(PublicationState::InCorso),
        "archiviato" => Ok(PublicationState::Archiviato),
        other => Err(bad("arguments", "state", other)),
    }
}

// ── Origin: cinque colonne ───────────────────────────────────────────────────

/// Il `model_locks.id` che corrisponde a un lock.
///
/// `kbs-core` non ha un id per il model lock, e senza id non si può scrivere la
/// provenienza in modo referenziale. L'id è derivato dal contenuto del lock: due
/// lock uguali sono lo stesso lock, e la `UNIQUE` sulle quattro colonne in
/// `model_locks` fa sì che non ne possano esistere due con lo stesso id.
pub fn lock_id(lock: &kbs_core::ModelLock) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for part in [
        lock.model_id.as_str(),
        lock.prompt_hash.as_str(),
        lock.corpus_hash.as_str(),
        lock.generator_version.as_str(),
    ] {
        h.update(part.as_bytes());
        h.update([0x1f]);
    }
    format!("lock_{}", hex::encode(&h.finalize()[..16]))
}

#[derive(Debug, Clone)]
pub struct OriginRow {
    pub kind: &'static str,
    pub by: Option<String>,
    pub at: Option<i64>,
    pub lock: Option<String>,
    pub from: Option<String>,
}

pub fn origin_to_row(origin: &Origin) -> Result<OriginRow> {
    Ok(match origin {
        Origin::Human { by, at } => OriginRow {
            kind: "human",
            by: Some(by.0.clone()),
            at: Some(at.0),
            lock: None,
            from: None,
        },
        Origin::Generated { lock, by, at } => OriginRow {
            kind: "generated",
            by: Some(by.0.clone()),
            at: Some(at.0),
            lock: Some(lock_id(lock)),
            from: None,
        },
        Origin::Derived { from, at } => OriginRow {
            kind: "derived",
            by: None,
            at: Some(at.0),
            lock: None,
            from: Some(from.0.clone()),
        },
    })
}

pub fn origin_from_row(row: &OriginRow, lock: Option<kbs_core::ModelLock>) -> Result<Origin> {
    let at = row
        .at
        .ok_or(Error::Corrupt {
            table: "arguments",
            field: "origin_at",
            reason: "assente".into(),
        })?;
    let at = kbs_core::Millis(at);
    let person = |p: &Option<String>| -> Result<kbs_core::PersonId> {
        p.clone()
            .ok_or(Error::Corrupt {
                table: "arguments",
                field: "origin_by",
                reason: "assente".into(),
            })
            .map(kbs_core::PersonId)
    };
    match row.kind {
        "human" => Ok(Origin::Human {
            by: person(&row.by)?,
            at,
        }),
        "generated" => {
            let lock = lock.ok_or(Error::Corrupt {
                table: "arguments",
                field: "origin_lock",
                reason: "manca il model lock".into(),
            })?;
            Ok(Origin::Generated {
                lock,
                by: person(&row.by)?,
                at,
            })
        }
        "derived" => Ok(Origin::Derived {
            from: row
                .from
                .clone()
                .ok_or(Error::Corrupt {
                    table: "arguments",
                    field: "origin_from",
                    reason: "assente".into(),
                })
                .map(kbs_core::ArgumentId)?,
            at,
        }),
        other => Err(bad("arguments", "origin_kind", other)),
    }
}

// ── Emitter: tre colonne ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EmitterRow {
    pub kind: &'static str,
    pub by: Option<String>,
    pub argument: Option<String>,
    pub observation: Option<String>,
}

pub fn emitter_to_row(e: &Emitter) -> EmitterRow {
    match e {
        Emitter::Teacher { by } => EmitterRow {
            kind: "teacher",
            by: Some(by.0.clone()),
            argument: None,
            observation: None,
        },
        Emitter::Content { argument } => EmitterRow {
            kind: "content",
            by: None,
            argument: Some(argument.0.clone()),
            observation: None,
        },
        Emitter::FromWork { observation } => EmitterRow {
            kind: "from-work",
            by: None,
            argument: None,
            observation: Some(observation.clone()),
        },
    }
}

pub fn emitter_from_row(row: &EmitterRow) -> Result<Emitter> {
    let need = |p: &Option<String>, field: &'static str| -> Result<String> {
        p.clone().ok_or(Error::Corrupt {
            table: "claims",
            field,
            reason: "assente".into(),
        })
    };
    match row.kind {
        "teacher" => Ok(Emitter::Teacher {
            by: kbs_core::PersonId(need(&row.by, "emitter_by")?),
        }),
        "content" => Ok(Emitter::Content {
            argument: kbs_core::ArgumentId(need(&row.argument, "emitter_argument")?),
        }),
        "from-work" => Ok(Emitter::FromWork {
            observation: need(&row.observation, "emitter_observation")?,
        }),
        other => Err(bad("claims", "emitter", other)),
    }
}

// ── ClaimStatus ──────────────────────────────────────────────────────────────

pub fn claim_status_to_db(s: &ClaimStatus) -> (&'static str, Option<String>) {
    match s {
        ClaimStatus::Supported => ("supported", None),
        ClaimStatus::Contradicted => ("contradicted", None),
        ClaimStatus::Unciteable => ("unciteable", None),
        ClaimStatus::Retracted { reason } => ("retracted", Some(reason.clone())),
    }
}

pub fn claim_status_from_db(status: &str, reason: Option<String>) -> Result<ClaimStatus> {
    match status {
        "supported" => Ok(ClaimStatus::Supported),
        "contradicted" => Ok(ClaimStatus::Contradicted),
        "unciteable" => Ok(ClaimStatus::Unciteable),
        "retracted" => Ok(ClaimStatus::Retracted {
            reason: reason.ok_or(Error::Corrupt {
                table: "claims",
                field: "status_reason",
                reason: "stato `retracted` senza ragione".into(),
            })?,
        }),
        other => Err(bad("claims", "status", other)),
    }
}

// ── GraderKind ───────────────────────────────────────────────────────────────

pub fn grader_to_db(k: GraderKind) -> &'static str {
    match k {
        GraderKind::Deterministic => "deterministic",
        GraderKind::Peer => "peer",
        GraderKind::Human => "human",
        GraderKind::Teacher => "teacher",
    }
}

pub fn grader_from_db(raw: &str) -> Result<GraderKind> {
    match raw {
        "deterministic" => Ok(GraderKind::Deterministic),
        "peer" => Ok(GraderKind::Peer),
        "human" => Ok(GraderKind::Human),
        "teacher" => Ok(GraderKind::Teacher),
        other => Err(bad("observations", "judged_by", other)),
    }
}

// ── Evidence: kind + payload ─────────────────────────────────────────────────

pub fn evidence_to_db(e: &Evidence) -> (&'static str, Option<String>) {
    match e {
        Evidence::Checked {
            exercise,
            instance,
            correct,
        } => (
            "checked",
            Some(
                json!({ "exercise": exercise, "instance": instance, "correct": correct })
                    .to_string(),
            ),
        ),
        Evidence::Oral { note, witness } => (
            "oral",
            Some(
                json!({ "note": note, "witness": witness.as_ref().map(|w| w.0.as_str()) })
                    .to_string(),
            ),
        ),
        Evidence::Written { ref_doc } => (
            "written",
            Some(json!({ "ref_doc": ref_doc }).to_string()),
        ),
        // Nessuna prova: dichiararlo è diverso dal non dirlo. Un `NULL` qui
        // significa `Evidence::None`, non «dato mancante».
        Evidence::None => ("none", None),
    }
}

pub fn evidence_from_db(kind: &str, payload: Option<String>) -> Result<Evidence> {
    let parse = |raw: Option<String>| -> Result<serde_json::Value> {
        let raw = raw.ok_or(Error::Corrupt {
            table: "observations",
            field: "evidence_payload",
            reason: "assente".into(),
        })?;
        serde_json::from_str(&raw).map_err(|e| Error::Corrupt {
            table: "observations",
            field: "evidence_payload",
            reason: e.to_string(),
        })
    };
    match kind {
        "checked" => {
            let v = parse(payload)?;
            Ok(Evidence::Checked {
                exercise: str_field(&v, "exercise")?.to_string(),
                instance: str_field(&v, "instance")?.to_string(),
                correct: bool_field(&v, "correct")?,
            })
        }
        "oral" => {
            let v = parse(payload)?;
            Ok(Evidence::Oral {
                note: str_field(&v, "note")?.to_string(),
                witness: v
                    .get("witness")
                    .and_then(|w| w.as_str())
                    .map(|w| kbs_core::PersonId(w.to_string())),
            })
        }
        "written" => {
            let v = parse(payload)?;
            Ok(Evidence::Written {
                ref_doc: str_field(&v, "ref_doc")?.to_string(),
            })
        }
        "none" => Ok(Evidence::None),
        other => Err(bad("observations", "evidence", other)),
    }
}

// ── Checker: kind + payload ──────────────────────────────────────────────────

pub fn checker_to_db(c: &Checker) -> (&'static str, String) {
    match c {
        Checker::Numeric { tolerance } => ("numeric", json!({ "tolerance": tolerance }).to_string()),
        Checker::Set { elements } => ("set", json!({ "elements": elements }).to_string()),
        Checker::MultipleChoice {
            correct_index,
            options,
        } => (
            "multiple-choice",
            json!({ "correct_index": correct_index, "options": options }).to_string(),
        ),
        Checker::Equivalence { normalized } => {
            ("equivalence", json!({ "normalized": normalized }).to_string())
        }
    }
}

pub fn checker_from_db(kind: &str, payload: &str) -> Result<Checker> {
    let v: serde_json::Value = serde_json::from_str(payload).map_err(|e| Error::Corrupt {
        table: "exercises",
        field: "checker_payload",
        reason: e.to_string(),
    })?;
    Ok(match kind {
        "numeric" => Checker::Numeric {
            tolerance: v.get("tolerance").and_then(|x| x.as_f64()).ok_or(Error::Corrupt {
                table: "exercises",
                field: "checker_payload",
                reason: "`tolerance` mancante o non numerico".into(),
            })?,
        },
        "set" => Checker::Set {
            elements: v
                .get("elements")
                .and_then(|x| x.as_array())
                .ok_or(Error::Corrupt {
                    table: "exercises",
                    field: "checker_payload",
                    reason: "`elements` mancante o non un elenco".into(),
                })?
                .iter()
                .map(|e| {
                    e.as_str().map(str::to_string).ok_or(Error::Corrupt {
                        table: "exercises",
                        field: "checker_payload",
                        reason: "un elemento di `elements` non è testo".into(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        },
        "multiple-choice" => Checker::MultipleChoice {
            correct_index: v
                .get("correct_index")
                .and_then(|x| x.as_u64())
                .ok_or(Error::Corrupt {
                    table: "exercises",
                    field: "checker_payload",
                    reason: "`correct_index` mancante o non un intero".into(),
                })? as u8,
            options: v
                .get("options")
                .and_then(|x| x.as_array())
                .ok_or(Error::Corrupt {
                    table: "exercises",
                    field: "checker_payload",
                    reason: "`options` mancante o non un elenco".into(),
                })?
                .iter()
                .map(|e| {
                    e.as_str().map(str::to_string).ok_or(Error::Corrupt {
                        table: "exercises",
                        field: "checker_payload",
                        reason: "un\'opzione non è testo".into(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        },
        "equivalence" => Checker::Equivalence {
            normalized: str_field(&v, "normalized")?.to_string(),
        },
        other => return Err(bad("exercises", "checker", other)),
    })
}

fn str_field<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Corrupt {
            table: "exercises",
            field: "checker_payload",
            reason: format!("`{key}` mancante o non testuale"),
        })
}

fn bool_field(v: &serde_json::Value, key: &str) -> Result<bool> {
    v.get(key)
        .and_then(|x| x.as_bool())
        .ok_or_else(|| Error::Corrupt {
            table: "observations",
            field: "evidence_payload",
            reason: format!("`{key}` mancante o non booleano"),
        })
}

// ── ContestationOutcome ──────────────────────────────────────────────────────

pub fn outcome_to_db(o: ContestationOutcome) -> &'static str {
    match o {
        ContestationOutcome::Upheld => "upheld",
        ContestationOutcome::Rejected => "rejected",
        ContestationOutcome::UnderReview => "under-review",
    }
}

pub fn outcome_from_db(raw: &str) -> Result<ContestationOutcome> {
    match raw {
        "upheld" => Ok(ContestationOutcome::Upheld),
        "rejected" => Ok(ContestationOutcome::Rejected),
        "under-review" => Ok(ContestationOutcome::UnderReview),
        other => Err(bad("gradings", "contested_outcome", other)),
    }
}

// ── Ratification e Contestation: blocchi di colonne ──────────────────────────

pub fn contestation_to_row(c: &Contestation) -> (String, i64, String, Option<&'static str>) {
    (
        c.by.0.clone(),
        c.at.0,
        c.reason.clone(),
        c.outcome.map(outcome_to_db),
    )
}

pub fn contestation_from_row(by: &str, at: i64, reason: &str, outcome: Option<&str>) -> Result<Contestation> {
    Ok(Contestation {
        by: kbs_core::PersonId(by.to_string()),
        at: kbs_core::Millis(at),
        reason: reason.to_string(),
        outcome: outcome.map(outcome_from_db).transpose()?,
    })
}

pub fn ratification_from_row(by: &str, at: i64, hash: &str, note: Option<String>) -> Result<Ratification> {
    Ok(Ratification {
        by: kbs_core::PersonId(by.to_string()),
        at: kbs_core::Millis(at),
        contract_hash: hash.to_string(),
        note: note.unwrap_or_default(),
    })
}

#[cfg(test)]
mod unit {
    use super::*;
    use kbs_core::{Millis, PersonId};

    #[test]
    fn states_round_trip() {
        for s in [
            PublicationState::Bozza,
            PublicationState::DelDocente,
            PublicationState::InCorso,
            PublicationState::Archiviato,
        ] {
            assert_eq!(state_from_db(state_to_db(s)).unwrap(), s);
        }
    }

    #[test]
    fn a_row_that_lies_about_its_state_is_an_error_not_a_panic() {
        let e = state_from_db("pubblicato").unwrap_err();
        assert_eq!(e.rule(), "storage.corrupt");
    }

    #[test]
    fn evidence_round_trips() {
        let all = [
            Evidence::Checked {
                exercise: "ex1".into(),
                instance: "in1".into(),
                correct: true,
            },
            Evidence::Oral {
                note: "risposta".into(),
                witness: Some(PersonId::fixture(1)),
            },
            Evidence::Written {
                ref_doc: "compito-3".into(),
            },
            Evidence::None,
        ];
        for e in all {
            let (k, p) = evidence_to_db(&e);
            assert_eq!(evidence_from_db(k, p).unwrap(), e);
        }
    }

    #[test]
    fn checker_round_trips() {
        let all = [
            Checker::Numeric { tolerance: 0.5 },
            Checker::Set {
                elements: vec!["a".into(), "b".into()],
            },
            Checker::MultipleChoice {
                correct_index: 2,
                options: vec!["x".into(), "y".into(), "z".into()],
            },
            Checker::Equivalence {
                normalized: "x^2+1".into(),
            },
        ];
        for c in all {
            let (k, p) = checker_to_db(&c);
            assert_eq!(checker_from_db(k, &p).unwrap(), c);
        }
    }

    #[test]
    fn lock_id_is_derived_from_the_lock() {
        let lock = kbs_core::ModelLock {
            model_id: "m".into(),
            prompt_hash: "p".into(),
            corpus_hash: "c".into(),
            generator_version: "g".into(),
            at: Millis(0),
        };
        assert_eq!(lock_id(&lock), lock_id(&lock));
        let other = kbs_core::ModelLock {
            corpus_hash: "c2".into(),
            ..lock.clone()
        };
        assert_ne!(lock_id(&lock), lock_id(&other));
    }
}
