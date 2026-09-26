//! Il replay deterministico (D11).
//!
//! Data la tupla (hash del corpus, id dell'esercizio, versione del generatore,
//! seed) il sistema deve poter **riprodurre** l'istanza e la risposta attesa.
//! È la condizione perché un registro append-only sia difendibile davanti a un
//! contesto: se l'istanza non è riproducibile, il ricorso non ha niente su cui
//! poggiare.
//!
//! Qui stanno due cose e non tre: il **record** (che cosa è stato registrato) e
//! la **verifica** (che cosa è stato riprodotto). Il generatore sta in
//! `kbs-exercise` e non qui; il confine è il trait [`InstanceGenerator`], che
//! questo crate possiede e `kbs-exercise` implementa — così la dipendenza va in
//! un solo verso e i due crate si compongono senza circolo.
//!
//! Un replay che non torna **non è un errore**: è un verdetto. L'errore è
//! quando non si può nemmeno tentare il replay (il generatore non conosce
//! l'esercizio, la tupla non è ricostruibile). La differenza non è
//! pedanteria: un errore che dice «non posso» viene letto come un problema
//! temporaneo, un verdetto che dice «non torna» viene letto come un fatto.

use crate::canonical::{CanonicalError, canonicalize};
use crate::limits::Verified;
use kbs_core::{Instance, Millis, PersonId};
use serde::{Deserialize, Serialize};
use std::fmt;

/// La tupla di D11: ciò che rende una generazione riproducibile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayKey {
    /// Il corpus al momento della generazione. Se il corpus è cambiato, la
    /// generazione non è la stessa e va detto.
    pub corpus_hash: String,
    pub exercise: String,
    /// Il generatore, per versione: la stessa versione è un programma, una
    /// versione diversa è un altro programma.
    pub generator_version: String,
    pub seed: String,
}

impl ReplayKey {
    pub fn new(
        corpus_hash: impl Into<String>,
        exercise: impl Into<String>,
        generator_version: impl Into<String>,
        seed: impl Into<String>,
    ) -> Self {
        ReplayKey {
            corpus_hash: corpus_hash.into(),
            exercise: exercise.into(),
            generator_version: generator_version.into(),
            seed: seed.into(),
        }
    }
}

/// Ciò che il generatore riceve. La versione non la riceve: la dichiara lui
/// ([`InstanceGenerator::version`]), perché è una proprietà del programma e
/// non dell'incarico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratorKey {
    pub corpus_hash: String,
    pub exercise: String,
    pub seed: String,
}

impl GeneratorKey {
    /// La tupla di replay che questa chiave soddisfa, con la versione data.
    pub fn replay_key(&self, generator_version: &str) -> ReplayKey {
        ReplayKey::new(
            self.corpus_hash.clone(),
            self.exercise.clone(),
            generator_version,
            self.seed.clone(),
        )
    }
}

/// Il generatore: il confine con `kbs-exercise`.
///
/// Il trait è dichiarato qui perché è *questo* crate a stabilire la domanda
/// («riproduci») e il tipo della risposta ([`ReplayOutcome`]). Chi genera
/// implementa; non sa nulla dei registri.
pub trait InstanceGenerator {
    /// La versione del programma generatore. Va confrontata con quella del
    /// record: due versioni diverse non possono essere confrontate, e il
    /// confronto fallito è un verdetto, non un errore.
    fn version(&self) -> &str;

    /// La risposta del generatore all'incarico. `Err` significa «non posso
    /// generare», non «genero qualcos'altro».
    fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError>;
}

/// Che cosa è stato registrato, e da che cosa si riconosce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayRecord {
    pub key: ReplayKey,
    pub instance: Instance,
    pub recorded_at: Millis,
    pub recorded_by: PersonId,
}

impl ReplayRecord {
    /// Lega un'istanza alla tupla che l'ha prodotta.
    ///
    /// Rifiuta una tupla che non descrive l'istanza: un record che dice «seed
    /// s» e contiene l'istanza di un altro seed non è un record, è un
    /// registro che mente sul proprio replay, e un ricorso su quella base non
    /// si regge.
    pub fn new(
        key: ReplayKey,
        instance: Instance,
        recorded_at: Millis,
        recorded_by: PersonId,
    ) -> Result<Self, ReplayError> {
        if instance.exercise != key.exercise {
            return Err(ReplayError::RecordKeyMismatch {
                field: ReplayField::Exercise,
                key: key.exercise.clone(),
                instance: instance.exercise.clone(),
            });
        }
        if instance.seed != key.seed {
            return Err(ReplayError::RecordKeyMismatch {
                field: ReplayField::Seed,
                key: key.seed.clone(),
                instance: instance.seed.clone(),
            });
        }
        Ok(ReplayRecord {
            key,
            instance,
            recorded_at,
            recorded_by,
        })
    }

    /// Confronta l'istanza rigenerata con quella registrata.
    ///
    /// I `params` si confrontano in forma canonica: due documenti JSON con le
    /// stesse coppie chiave-valore sono lo stesso documento, e segnalare una
    /// differenza che l'ordinamento delle chiavi ha prodotto sarebbe un falso
    /// ricorso.
    pub fn verify(&self, regenerated: &Instance) -> Result<ReplayOutcome, ReplayError> {
        let cmp = [
            (ReplayField::Exercise, &self.instance.exercise, &regenerated.exercise),
            (ReplayField::Seed, &self.instance.seed, &regenerated.seed),
            (
                ReplayField::RenderedPrompt,
                &self.instance.rendered_prompt,
                &regenerated.rendered_prompt,
            ),
            (
                ReplayField::Expected,
                &self.instance.expected,
                &regenerated.expected,
            ),
        ];
        for (field, recorded, got) in cmp {
            if recorded != got {
                return Ok(ReplayOutcome::Mismatch {
                    field,
                    recorded: recorded.clone(),
                    regenerated: got.clone(),
                });
            }
        }
        let a = canonicalize(&self.instance.params)?;
        let b = canonicalize(&regenerated.params)?;
        if a != b {
            return Ok(ReplayOutcome::Mismatch {
                field: ReplayField::Params,
                recorded: a,
                regenerated: b,
            });
        }
        Ok(ReplayOutcome::Match)
    }
}

/// Il campo della tupla o dell'istanza che non torna.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplayField {
    CorpusHash,
    Exercise,
    GeneratorVersion,
    Seed,
    RenderedPrompt,
    Expected,
    Params,
}

impl ReplayField {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayField::CorpusHash => "corpus_hash",
            ReplayField::Exercise => "exercise",
            ReplayField::GeneratorVersion => "generator_version",
            ReplayField::Seed => "seed",
            ReplayField::RenderedPrompt => "rendered_prompt",
            ReplayField::Expected => "expected",
            ReplayField::Params => "params",
        }
    }
}

impl fmt::Display for ReplayField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Il verdetto di un replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// L'istanza rigenerata è identica a quella registrata, parametri compresi.
    Match,
    /// L'istanza rigenerata è un'altra istanza. **Entrambi** i valori sono nel
    /// verdetto: un ricorso che riporta solo «non torna» non è un ricorso.
    Mismatch {
        field: ReplayField,
        recorded: String,
        regenerated: String,
    },
}

impl ReplayOutcome {
    pub fn is_match(&self) -> bool {
        matches!(self, ReplayOutcome::Match)
    }

    pub fn mismatch(&self) -> Option<(ReplayField, &str, &str)> {
        match self {
            ReplayOutcome::Mismatch {
                field,
                recorded,
                regenerated,
            } => Some((*field, recorded, regenerated)),
            ReplayOutcome::Match => None,
        }
    }
}

impl fmt::Display for ReplayOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayOutcome::Match => f.write_str("replay fedele: l'istanza rigenerata è quella registrata"),
            ReplayOutcome::Mismatch {
                field,
                recorded,
                regenerated,
            } => write!(
                f,
                "replay non fedele su {field}: registrato «{recorded}», rigenerato «{regenerated}»"
            ),
        }
    }
}

/// Riusa la tupla registrata per rigenerare l'istancia e confrontarla.
///
/// I due preconditi (corpus e versione del generatore) sono confrontati **come
/// verdetto**, non come errore: un replay eseguito con il corpus sbagliato non
/// è un replay, ed è un fatto da cui si può fare appello.
pub fn replay<G: InstanceGenerator + ?Sized>(
    record: &ReplayRecord,
    corpus_hash: &str,
    generator: &G,
) -> Result<Verified<ReplayOutcome>, ReplayError> {
    if generator.version() != record.key.generator_version {
        return Ok(Verified::new(ReplayOutcome::Mismatch {
            field: ReplayField::GeneratorVersion,
            recorded: record.key.generator_version.clone(),
            regenerated: generator.version().to_owned(),
        }));
    }
    if corpus_hash != record.key.corpus_hash {
        return Ok(Verified::new(ReplayOutcome::Mismatch {
            field: ReplayField::CorpusHash,
            recorded: record.key.corpus_hash.clone(),
            regenerated: corpus_hash.to_owned(),
        }));
    }
    let key = GeneratorKey {
        corpus_hash: corpus_hash.to_owned(),
        exercise: record.key.exercise.clone(),
        seed: record.key.seed.clone(),
    };
    let regenerated = generator.generate(&key)?;
    Ok(Verified::new(record.verify(&regenerated)?))
}

/// Perché un replay non si è potuto tentare. Non è «il replay è diverso»:
/// quello è un verdetto.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    #[error("il generatore non ha potuto produrre l'istanza di `{exercise}`: {reason}")]
    Generator { exercise: String, reason: String },

    #[error("il record lega la tupla `{key}` all'istanza `{instance}`, che non è la stessa: {field}")]
    RecordKeyMismatch {
        field: ReplayField,
        key: String,
        instance: String,
    },

    #[error("istanza non confrontabile: {0}")]
    Canonical(#[from] CanonicalError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Un generatore finto: produce l'istanza che gli si chiede, o un'altra,
    /// secondo `drift`. Serve a provare il confine senza dipendere da
    /// `kbs-exercise`.
    struct Fake {
        version: String,
        drift: Option<String>,
    }

    impl Fake {
        fn honest(version: &str) -> Self {
            Fake {
                version: version.to_owned(),
                drift: None,
            }
        }

        fn drifting(version: &str, drift: &str) -> Self {
            Fake {
                version: version.to_owned(),
                drift: Some(drift.to_owned()),
            }
        }
    }

    impl InstanceGenerator for Fake {
        fn version(&self) -> &str {
            &self.version
        }

        fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError> {
            if key.exercise != "ex-1" {
                return Err(ReplayError::Generator {
                    exercise: key.exercise.clone(),
                    reason: "esercizio sconosciuto".into(),
                });
            }
            let a = 2i64;
            let b = 3i64;
            let expected = match &self.drift {
                None => (a * b + 1).to_string(),
                Some(_) => (a * b + 2).to_string(),
            };
            Ok(Instance {
                exercise: key.exercise.clone(),
                seed: key.seed.clone(),
                rendered_prompt: format!("Calcola {a}·{b}"),
                expected,
                params: json!({ "a": a, "b": b }),
            })
        }
    }

    fn key(seed: &str) -> ReplayKey {
        ReplayKey::new("corpus-hash-1", "ex-1", "gen-1", seed)
    }

    fn instance() -> Instance {
        Fake::honest("gen-1")
            .generate(&GeneratorKey {
                corpus_hash: "corpus-hash-1".into(),
                exercise: "ex-1".into(),
                seed: "s1".into(),
            })
            .unwrap()
    }

    fn record() -> ReplayRecord {
        ReplayRecord::new(
            key("s1"),
            instance(),
            Millis(1_700_000_000_000),
            PersonId("person_docente".into()),
        )
        .unwrap()
    }

    #[test]
    fn il_record_lega_la_tupla_all_istanza() {
        let r = record();
        assert_eq!(r.key.seed, "s1");
        assert_eq!(r.instance.expected, "7");
        assert_eq!(r.recorded_by.as_str(), "person_docente");
    }

    #[test]
    fn una_tupla_che_non_descrive_l_istanza_non_e_un_record() {
        let mut k = key("s1");
        k.seed = "s2".into();
        assert_eq!(
            ReplayRecord::new(k, instance(), Millis(0), PersonId("p".into())).err(),
            Some(ReplayError::RecordKeyMismatch {
                field: ReplayField::Seed,
                key: "s2".into(),
                instance: "s1".into()
            })
        );
        let mut k = key("s1");
        k.exercise = "ex-2".into();
        assert!(matches!(
            ReplayRecord::new(k, instance(), Millis(0), PersonId("p".into())),
            Err(ReplayError::RecordKeyMismatch {
                field: ReplayField::Exercise,
                ..
            })
        ));
    }

    /// Il replay che rifà la stessa istanza ne riproduce la risposta attesa.
    #[test]
    fn replay_riproduce_la_risposta_attesa() {
        let v = replay(&record(), "corpus-hash-1", &Fake::honest("gen-1")).unwrap();
        assert_eq!(*v.verdict(), ReplayOutcome::Match);
        assert!(v.verdict().is_match());
        assert!(v.to_string().contains("fedele"));
    }

    /// Un replay che non torna è un verdetto con due valori, non un errore.
    #[test]
    fn replay_diverso_e_un_verdetto_con_due_valori() {
        let v = replay(&record(), "corpus-hash-1", &Fake::drifting("gen-1", "x")).unwrap();
        assert_eq!(
            *v.verdict(),
            ReplayOutcome::Mismatch {
                field: ReplayField::Expected,
                recorded: "7".into(),
                regenerated: "8".into()
            }
        );
        let (field, recorded, regenerated) = v.verdict().mismatch().unwrap();
        assert_eq!(field, ReplayField::Expected);
        assert_eq!(recorded, "7");
        assert_eq!(regenerated, "8");
    }

    #[test]
    fn seed_diverso_è_una_diversa_istancia() {
        let r = ReplayRecord::new(
            key("s1"),
            instance(),
            Millis(0),
            PersonId("p".into()),
        )
        .unwrap();
        let alt = Instance {
            seed: "s2".into(),
            ..instance()
        };
        let out = r.verify(&alt).unwrap();
        assert_eq!(
            out.mismatch(),
            Some((ReplayField::Seed, "s1".into(), "s2".into()))
        );
    }

    #[test]
    fn versione_diversa_è_un_verdetto_su_quel_campo() {
        let v = replay(&record(), "corpus-hash-1", &Fake::honest("gen-2")).unwrap();
        assert_eq!(
            *v.verdict(),
            ReplayOutcome::Mismatch {
                field: ReplayField::GeneratorVersion,
                recorded: "gen-1".into(),
                regenerated: "gen-2".into()
            }
        );
    }

    #[test]
    fn corpus_diverso_è_un_verdetto_su_quel_campo() {
        let v = replay(&record(), "corpus-hash-2", &Fake::honest("gen-1")).unwrap();
        assert_eq!(
            *v.verdict(),
            ReplayOutcome::Mismatch {
                field: ReplayField::CorpusHash,
                recorded: "corpus-hash-1".into(),
                regenerated: "corpus-hash-2".into()
            }
        );
    }

    #[test]
    fn un_erore_del_generatore_è_un_erore_e_non_un_verdetto() {
        let g = Fake::honest("gen-1");
        let err = g
            .generate(&GeneratorKey {
                corpus_hash: "corpus-hash-1".into(),
                exercise: "ex-9".into(),
                seed: "s1".into(),
            })
            .unwrap_err();
        assert!(matches!(err, ReplayError::Generator { .. }));
    }

    #[test]
    fn parametri_con_chiavi_in_ordine_diverso_non_sono_un_mismatch() {
        let r = record();
        let mut alt = instance();
        // stesso documento, chiavi nell'altro ordine
        alt.params = json!({ "b": 3, "a": 2 });
        assert_eq!(r.verify(&alt).unwrap(), ReplayOutcome::Match);
    }

    #[test]
    fn parametri_diversi_sono_un_mismatch_con_entrambi_i_valori() {
        let r = record();
        let mut alt = instance();
        alt.params = json!({ "a": 2, "b": 4 });
        let out = r.verify(&alt).unwrap();
        let (field, recorded, regenerated) = out.mismatch().unwrap();
        assert_eq!(field, ReplayField::Params);
        assert_eq!(recorded, r#"{"a":2,"b":3}"#);
        assert_eq!(regenerated, r#"{"a":2,"b":4}"#);
        assert_ne!(recorded, regenerated);
    }

    #[test]
    fn la_chiave_del_generatore_si_ricompone_in_una_tupla() {
        let k = GeneratorKey {
            corpus_hash: "h".into(),
            exercise: "ex-1".into(),
            seed: "s1".into(),
        };
        assert_eq!(k.replay_key("gen-9"), ReplayKey::new("h", "ex-1", "gen-9", "s1"));
    }

    #[test]
    fn due_istanze_diverse_hanno_risposte_diverse() {
        let a = instance();
        let b = Instance {
            seed: "s2".into(),
            expected: "8".into(),
            ..a.clone()
        };
        assert!(a.differs_from(&b));
    }
}
