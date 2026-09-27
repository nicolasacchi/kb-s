//! Il corpus: i 40 artefatti, i loro hash, e i tipi di dominio che ne
//! derivano.
//!
//! Qui la tabola diventa [`kbs_core::Argument`], [`kbs_core::Claim`],
//! [`kbs_core::Exercise`] e [`kbs_core::Instance`]. Non è una traduzione
//! cosmetica: è il momento in cui le regole di `kbs-core` diventano
//! **verificabili su dati reali** e non su un oggetto costruito a mano in un
//! test. La ratifica superata, per esempio, non è qui un `Err` atteso: è una
//! riga di tabella con un hash di contratto diverso da quello corrente.

use crate::items;
use crate::render;
use crate::spec::{ClaimStatusKind, CheckerSpec, RatificaSpec, Spec};
use kbs_core::{
    Argument, ArgumentId, Checker, Claim, Contestation, ContestationOutcome, CourseId, Emitter,
    Evidence, Exercise, GraderKind, Instance, Millis, Origin, PersonId, PublicationState,
    Ratification, SeqInSession,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// Un file del corpus: percorso relativo e byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Relativo alla radice del corpus, con `/` come separatore.
    pub rel: String,
    pub contenuto: String,
}

/// Il corpus: quaranta artefatti HTML e i manifest delle scene.
#[derive(Debug, Clone)]
pub struct Corpus {
    file: Vec<File>,
    voci: Vec<Spec>,
}

impl Corpus {
    /// Costruisce il corpus dalla tabella. La funzione è pura: stesse voci,
    /// stessi byte, e l'hash del corpus è quello di ieri.
    pub fn dalla_tabella() -> Self {
        Self::da_voci(items::voci())
    }

    /// Costruisce un corpus da voci che non sono necessariamente quelle della
    /// tabella. È il modo in cui il banco produce una **copia** da mettere
    /// sotto la pipeline: stessa tabella, con una voce cambiata, e quindi un
    /// corpus diverso — di cui il banco sa dire che differisce in un file e in
    /// uno solo.
    pub fn da_voci(voci: Vec<Spec>) -> Self {
        let mut file = Vec::with_capacity(voci.len() + 4);
        for s in &voci {
            file.push(File {
                rel: s.rel.to_string(),
                contenuto: render::artifact(s),
            });
        }
        for s in &voci {
            if s.scena.is_some() {
                file.push(File {
                    rel: render::percorso_manifest(s),
                    contenuto: render::manifest(s),
                });
            }
        }
        file.sort_by(|a, b| a.rel.cmp(&b.rel));
        Corpus { file, voci }
    }

    /// Il corpus con il **contratto** di un item riscritto, e con nient'altro
    /// cambiato: è la correzione che un docente fa dopo aver firmato, non un
    /// altra unità. Il resto dei file resta identico byte per byte, e un test
    /// lo verifica — se la copia differisse in un punto oltre al contratto,
    /// il caso che il banco crede di esercitare non sarebbe quello.
    ///
    /// La riscrittura non è una frase inventata: è la **sezione
    /// `PREREQUISITI` svuotata**, cioè l'argomento che non è più richiesto.
    /// È la correzione più piccola che cambi davvero l'hash del contratto, ed
    /// è la stessa che il banco racconta in `prove/02-prova-finale.html`.
    pub fn con_contratto_riscritto(&self, rel: &str) -> Corpus {
        let voci: Vec<Spec> = self
            .voci
            .iter()
            .map(|s| {
                if s.rel == rel {
                    Spec {
                        prerequisiti: &[],
                        ..*s
                    }
                } else {
                    *s
                }
            })
            .collect();
        Corpus::da_voci(voci)
    }

    /// Scrive il corpus sotto `dir`, creando le cartelle che mancano. È il
    /// modo in cui il banco mette una **copia** sotto la pipeline: la copia è
    /// ciò che il banco modifica, e il corpus di lavoro non lo tocca mai.
    pub fn scrivi_in(&self, dir: &Path) -> std::io::Result<()> {
        for f in &self.file {
            let p = dir.join(&f.rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, f.contenuto.as_bytes())?;
        }
        Ok(())
    }

    /// I file, in ordine di percorso.
    pub fn file(&self) -> &[File] {
        &self.file
    }

    /// Le voci della tabella, in ordine di percorso.
    pub fn voci(&self) -> &[Spec] {
        &self.voci
    }

    /// Quanti item ha il banco.
    pub fn len(&self) -> usize {
        self.voci.len()
    }

    pub fn is_empty(&self) -> bool {
        self.voci.is_empty()
    }

    /// Il file di un percorso, se c'è.
    pub fn file_di(&self, rel: &str) -> Option<&File> {
        self.file.iter().find(|f| f.rel == rel)
    }

    /// I soli artefatti HTML, senza i manifest: sono gli **item**.
    pub fn artefatti(&self) -> impl Iterator<Item = &File> {
        self.file.iter().filter(|f| f.rel.ends_with(".html"))
    }

    /// L'hash di un file: SHA-256 dei byte, esadecimale.
    pub fn hash_file( contenuto: &str) -> String {
        let mut h = Sha256::new();
        h.update(contenuto.as_bytes());
        format!("sha256:{}", hex::encode(h.finalize()))
    }

    /// L'hash del corpus intero: SHA-256 sulla sequenza dei file, ciascuno
    /// identificato dal percorso e dal proprio hash.
    ///
    /// È il numero che rende il banco deterministico in modo verificabile: se
    /// cambia, è cambiato un fixture, e il referto dice quale.
    pub fn hash(&self) -> String {
        let mut h = Sha256::new();
        for f in &self.file {
            h.update(f.rel.as_bytes());
            h.update([0u8]);
            h.update(Self::hash_file(&f.contenuto).as_bytes());
            h.update([b'\n']);
        }
        format!("sha256:{}", hex::encode(h.finalize()))
    }

    /// La mappa percorso → hash dei singoli file, per il referto.
    pub fn hash_per_file(&self) -> BTreeMap<&str, String> {
        self.file
            .iter()
            .map(|f| (f.rel.as_str(), Self::hash_file(&f.contenuto)))
            .collect()
    }
}

/// L'id dell'argomento: segue il percorso, non i byte. Rinominare un
/// argomento è quindi un atto esplicito, e non un collasso silenzioso.
pub fn id_di(s: &Spec) -> ArgumentId {
    ArgumentId::from_rel_path(s.rel)
}

fn corso(n: u32) -> CourseId {
    CourseId::fixture(n)
}

fn persona(n: u32) -> PersonId {
    PersonId::fixture(n)
}

/// Il contratto come hash: è l'hash del testo reso delle otto sezioni, ed è
/// ciò che la ratifica confronta. Per una ratifica **superata** si usa un hash
/// dichiarato a mano che non corrisponde a nessun contratto: è il caso reale,
/// in cui il contratto è stato modificato dopo la firma.
pub fn hash_del_contratto(s: &Spec) -> String {
    Corpus::hash_file(&s.contract().render())
}

/// L'hash di contratto con cui l'item è stato ratificato quando la ratifica
/// è dichiarata come **superata**: un hash che non è quello corrente, e che il
/// banco rende esplicito.
pub const HASH_CONTRATTO_SUPERATO: &str = "sha256:ratificata-sul-testo-precedente";

/// L'argomento dell'item, in `kbs_core::Argument`.
pub fn argomento(s: &Spec, indice: usize) -> Argument {
    let content_hash = hash_del_contratto(s);
    // Una ratifica **superata** porta l'hash del contratto di quando è stata
    // firmata, e l'argomento porta quello di oggi. I due devono **diversi**:
    // è la condizione che `check_citable` trasforma in `StaleRatification`, e
    // un fixture in cui coincidono non eserciterebbe il percorso.
    let hash_ratificato = match s.ratifica {
        RatificaSpec::Stale => HASH_CONTRATTO_SUPERATO.to_string(),
        _ => content_hash.clone(),
    };
    let nota = match s.ratifica {
        RatificaSpec::Stale => " (ratifica del testo precedente)",
        _ => "",
    };
    let ratifica = match s.ratifica {
        RatificaSpec::Nessuna => None,
        RatificaSpec::Fresca | RatificaSpec::Stale => Some(Ratification {
            by: persona(s.origine.autore()),
            at: render::ratificato_a(indice),
            contract_hash: hash_ratificato,
            note: format!("verificato: {}{nota}", s.famiglia.nome()),
        }),
    };
    Argument {
        id: id_di(s),
        title: s.titolo.to_string(),
        summary: s.riassunto.to_string(),
        state: s.stato,
        course: corso(s.corso),
        prerequisites: s.prerequisiti.iter().map(|p| ArgumentId::from_rel_path(p)).collect(),
        origin: origin_di(s, indice),
        rel_path: Some(s.rel.to_string()),
        content_hash,
        created_at: render::creato_a(indice),
        updated_at: render::aggiornato_a(indice),
        ratified: ratifica,
    }
}

fn origin_di(s: &Spec, indice: usize) -> Origin {
    match s.origine {
        crate::spec::OriginSpec::Human { by } => Origin::Human {
            by: persona(by),
            at: render::creato_a(indice),
        },
        crate::spec::OriginSpec::Generated { by, .. } => Origin::Generated {
            // Lo stesso lock che il file dichiara, dalla stessa funzione: se il
            // file e la tabella costruissero il lock separatamente, la prima
            // ricorrenza li dividerebbe e nessuno dei due se ne accorgerebbe.
            lock: crate::render::lock_di(s),
            by: persona(by),
            at: render::creato_a(indice),
        },
        crate::spec::OriginSpec::Derived { from } => Origin::Derived {
            from: ArgumentId::from_rel_path(from),
            at: render::creato_a(indice),
        },
    }
}

/// Le claim dell'item, in `kbs_core::Claim`.
///
/// Gli id sono derivati dall'id del claim dichiarato e dall'id dell'argomento,
/// e sono quindi univoci **per costruzione**: due claim dello stesso item non
/// possono avere lo stesso id, e due item non possono produrre lo stesso id.
pub fn claims_di(s: &Spec, indice: usize) -> Vec<Claim> {
    let arg = id_di(s);
    s.claims
        .iter()
        .map(|c| Claim {
            id: format!("{}::{}", arg.as_str(), c.id),
            course: corso(s.corso),
            argument: arg.clone(),
            text: c.testo.to_string(),
            span_anchor: c.ancora.map(|a| format!("{}#{}", s.rel, a)),
            span_text: c.testo_span.map(|t| t.to_string()),
            status: c.stato.to_core(),
            emitted_at: Millis(render::creato_a(indice).0 + 1),
            emitted_by: match c.emittente {
                crate::spec::EmittenteSpec::Docente { by } => Emitter::Teacher { by: persona(by) },
                crate::spec::EmittenteSpec::Contenuto => Emitter::Content { argument: arg.clone() },
                crate::spec::EmittenteSpec::DaLavoro { osservazione } => {
                    Emitter::FromWork { observation: format!("obs-{osservazione}") }
                }
            },
        })
        .collect()
}

/// Gli esercizi dell'item, in `kbs_core::Exercise`.
pub fn esercizi_di(s: &Spec, indice: usize) -> Vec<Exercise> {
    s.esercizi
        .iter()
        .map(|e| Exercise {
            id: format!("{}::{}", s.corso, e.id),
            course: corso(s.corso),
            argument: id_di(s),
            family: e.famiglia.to_string(),
            generator_version: e.generatore.to_string(),
            prompt: e.testo.to_string(),
            checker: checker_di(&e.checker),
            created_at: render::creato_a(indice),
            created_by: persona(s.origine.autore()),
        })
        .collect()
}

fn checker_di(c: &CheckerSpec) -> Checker {
    match c {
        CheckerSpec::Numeric { tolerance } => Checker::Numeric { tolerance: *tolerance },
        CheckerSpec::Set { elementi } => Checker::Set {
            elements: elementi.iter().map(|s| s.to_string()).collect(),
        },
        CheckerSpec::MultipleChoice { corretta, opzioni } => Checker::MultipleChoice {
            correct_index: *corretta,
            options: opzioni.iter().map(|s| s.to_string()).collect(),
        },
        CheckerSpec::Equivalence { normale } => Checker::Equivalence { normalized: normale.to_string() },
    }
}

/// Le istanze dichiarate, in `kbs_core::Instance`. Il `seed` e i parametri
/// sono ciò che rende la risposta riproducibile; `expected` è ciò che la
/// pipeline deve restituire, e il banco lo confronta byte per byte.
pub fn istanze_di(s: &Spec) -> Vec<Instance> {
    let mut out = Vec::new();
    for e in s.esercizi {
        for i in e.istanze {
            out.push(Instance {
                exercise: format!("{}::{}", s.corso, e.id),
                seed: i.seed.to_string(),
                rendered_prompt: String::new(),
                expected: i.attesa.to_string(),
                params: render::parametri(i),
            });
        }
    }
    out
}

/// Gli argomenti del banco, in ordine di percorso.
pub fn argomenti() -> Vec<Argument> {
    items::voci()
        .iter()
        .enumerate()
        .map(|(i, s)| argomento(s, i))
        .collect()
}

/// Gli argomenti che il banco considera citabili: lo dice `kbs-core`, non il
/// banco. Il banco non ha una regola propria di citabilità perché due regole
/// sarebbero una fonte sola e sembrerebbero due.
pub fn citabili() -> Vec<ArgumentId> {
    argomenti()
        .into_iter()
        .filter(|a| a.is_citable_now())
        .map(|a| a.id)
        .collect()
}

/// Le osservazioni di padronanza: una per istanza, giudicata dal checker.
pub fn osservazioni() -> Vec<kbs_core::Observation> {
    let voci = items::voci();
    let mut out = Vec::new();
    let mut seq = 0u64;
    for (i, s) in voci.iter().enumerate() {
        for ist in istanze_di(s) {
            seq += 1;
            out.push(kbs_core::Observation {
                id: format!("obs-{:04}", seq),
                seq: SeqInSession(seq),
                student: persona(100 + (seq % 3) as u32),
                course: corso(s.corso),
                cohort: kbs_core::CohortId::fixture(s.corso),
                argument: id_di(s),
                evidence: Evidence::Checked {
                    exercise: ist.exercise.clone(),
                    instance: ist.seed.clone(),
                    correct: true,
                },
                judged_by: s
                    .esercizi
                    .iter()
                    .find(|e| e.id == ist.exercise.rsplit("::").next().unwrap_or(""))
                    .and_then(|e| e.giudicato_da),
                at: Millis(render::aggiornato_a(i).0 + seq as i64),
            });
        }
    }
    out
}

/// Una valutazione con contestazione, per mostrare che la contestazione sta
/// **dentro** la riga e non cancella la riga (D6).
pub fn valutazione_con_contestazione() -> kbs_core::Grading {
    kbs_core::Grading {
        id: "grd-0001".into(),
        seq: SeqInSession(1),
        student: persona(101),
        course: corso(crate::spec::CORSO_MATEMATICA),
        argument: id_di(&items::voce("esercizi/01-somma-di-frazioni.html").expect("voce fissa")),
        kind: GraderKind::Deterministic,
        graded_by: persona(crate::spec::DOCENTE),
        rubric_version: "rubrica-tesina-2".into(),
        grade: "7/100".into(),
        at: render::ratificato_a(0),
        contested: Some(Contestation {
            by: persona(101),
            at: Millis(render::ratificato_a(0).0 + 86_400_000),
            reason: "la risposta data è 7/12, non 5/6: l'istanza non è la mia".into(),
            outcome: Some(ContestationOutcome::UnderReview),
        }),
    }
}

/// Gli id di tutte le claim del banco, per il controllo di copertura.
pub fn claim_ids() -> Vec<(String, ClaimStatusKind)> {
    let mut out = Vec::new();
    for s in items::voci() {
        for c in s.claims {
            out.push((c.id.to_string(), c.stato));
        }
    }
    out
}

/// Le claim che non devono mai comparire nell'output ma devono comparire nel
/// registro: è la regola «un errore si registra, non si cancella» applicata
/// a righe vere.
pub fn claim_da_registrare() -> Vec<crate::spec::ClaimAttesa> {
    let mut out = Vec::new();
    for s in items::voci() {
        for c in s.claims {
            let stato = c.stato;
            let riga = !matches!(stato, ClaimStatusKind::Supported);
            out.push(crate::spec::ClaimAttesa {
                claim: c.id,
                rel: s.rel,
                stato,
                riga_deve_esistere: riga || c.ancora.is_some(),
                output_deve_sopprimerla: matches!(stato, ClaimStatusKind::Unciteable),
            });
        }
    }
    out
}

/// Il grafo dei prerequisiti degli item **validi**, come coppie (da, a).
///
/// Gli item con difetto sono esclusi: il banco verifica che la parte che deve
/// entrare sia aciclica, e la parte che non deve entrare è già coperta dal
/// controllo sui cinque difetti.
pub fn archi_del_grafo_valido() -> Vec<(String, String)> {
    items::voci()
        .iter()
        .filter(|s| !s.deve_essere_rifiutato())
        .flat_map(|s| {
            s.prerequisiti
                .iter()
                .map(move |p| (p.to_string(), s.rel.to_string()))
        })
        .collect()
}

/// La radice del corpus dentro il repository, in percorso relativo.
pub const RADICE_RELATIVA: &str = "crates/kbs-fixtures/corpus";

/// Tutti gli stati di pubblicazione, in ordine di dichiarazione.
pub const STATI: [PublicationState; 4] = [
    PublicationState::Bozza,
    PublicationState::DelDocente,
    PublicationState::InCorso,
    PublicationState::Archiviato,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// L'hash del corpus è la prova di determinismo del banco: è un
    /// **numero**, e un numero che cambia significa che è cambiato un fixture.
    /// Il valore è fissato qui, non calcolato, perché un test che verifica
    /// `hash(corpus) == hash(corpus)` non verifica niente.
    #[test]
    fn l_hash_del_corpus_e_fissato() {
        let c = Corpus::dalla_tabella();
        assert_eq!(
            c.hash(),
            "sha256:ef203faaa5c4076424aa4f62de8022e071186c63c980360277ef3b8bfec30c3d"
        );
    }
}
