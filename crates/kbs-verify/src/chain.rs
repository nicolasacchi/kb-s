//! La catena di hash sulle osservazioni, per sessione di registro (D6).
//!
//! La formula è quella di D6, applicata alla sequenza delle `Observation`
//! dentro una sessione:
//!
//! ```text
//! leaf_i  = SHA256(0x00 ‖ canonical_json(row_i))
//! segment = MTH(leaf_i del segmento)          // node = SHA256(0x01 ‖ l ‖ r)
//! head    = MTH(radici di segmento)
//! ```
//!
//! La segmentazione serve a una cosa sola: poter mostrare a un terzo «questa
//! testa contiene il segmento 7, righe 224..255» senza mostrare tutto il
//! registro. La [`ConsistencyProof`] è il cammino dei fratelli che porta dalla
//! radice del segmento alla testa, e si verifica riseguendolo da capo: la
//! stessa operazione, sul posto, senza fiducia in chi la ha prodotta.
//!
//! **L'id della sessione non entra nella catena.** D6 firma la riga, non il
//! contenitore: due sessioni diverse con le stesse righe hanno la stessa
//! testa. È una conseguenza, non una scelta — ed è il motivo per cui il
//! testimone ([`crate::witness`]) è ciò che distingue una sessione dall'altra.

use crate::canonical::{CanonicalError, leaf_of};
use crate::hash::{Hash, HashParseError, node};
use crate::session::SessionId;
use kbs_core::Observation;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Quante righe in un segmento, se nessuno dice niente.
pub const DEFAULT_PER_SEGMENT: usize = 32;

/// Come le righe di una sessione vengono raggruppate in segmenti.
///
/// La scelta non cambia la testa: cambia **quanta prova** serve per parlare di
/// una parte del registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentPlan {
    pub per_segment: usize,
}

impl SegmentPlan {
    /// Un segmento ogni `per_segment` righe.
    pub fn every(per_segment: usize) -> Self {
        SegmentPlan { per_segment }
    }

    pub fn default_plan() -> Self {
        SegmentPlan {
            per_segment: DEFAULT_PER_SEGMENT,
        }
    }
}

impl Default for SegmentPlan {
    fn default() -> Self {
        SegmentPlan::default_plan()
    }
}

/// Perché la catena non è costruibile. Ogni variante nomina la regola rotta.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    #[error("sessione di registro `{session}` senza righe: non c'è una testa da attestare")]
    EmptySession { session: SessionId },

    #[error("righe fuori ordine in `{session}`: alla posizione {at} la seq è {found}, la precedente era {prev}")]
    OutOfOrder {
        session: SessionId,
        at: usize,
        prev: u64,
        found: u64,
    },

    #[error("seq non contigua in `{session}`: alla posizione {at} c'è {found}, attesa {expected}")]
    SeqGap {
        session: SessionId,
        at: usize,
        found: u64,
        expected: u64,
    },

    #[error("id di riga ripetuto in `{session}`: `{id}` compare alle posizioni {first} e {again}")]
    DuplicateRowId {
        session: SessionId,
        id: String,
        first: usize,
        again: usize,
    },

    #[error("segmentazione di {0} righe: un segmento ne contiene almeno una")]
    ZeroSegment(usize),

    #[error("il segmento {index} è vuoto: un segmento ne contiene almeno una riga")]
    EmptySegment { index: usize },

    #[error("segmento {index} inesistente: la sessione `{session}` ne ha {count}")]
    NoSuchSegment {
        session: SessionId,
        index: usize,
        count: usize,
    },

    #[error("riga non serializzabile nella forma canonica: {0}")]
    Canonical(#[from] CanonicalError),
}

/// Dove sta, rispetto al ramo seguito, il fratello da cui si sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// Il fratello è il figlio sinistro: `node(fratello, acc)`.
    Left,
    /// Il fratello è il figlio destro: `node(acc, fratello)`.
    Right,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Left => f.write_str("left"),
            Side::Right => f.write_str("right"),
        }
    }
}

/// Un gradino del cammino dalla radice del segmento alla testa di sessione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofStep {
    pub hash: Hash,
    /// Lato del **fratello**, non del ramo seguito.
    pub side: Side,
}

/// Una parte della sessione, con la sua radice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub index: usize,
    /// Prima `seq` del segmento, inclusa.
    pub from: u64,
    /// Ultima `seq` del segmento, inclusa.
    pub to: u64,
    pub rows: usize,
    pub root: Hash,
}

/// La prova che un segmento sta in quella posizione della testa di sessione.
///
/// Si verifica da sola ([`ConsistencyProof::check`]) e si verifica anche nel
/// contesto ([`Chain::check`]): il primo ricalcola la testa, il secondo
/// controlla che la prova parli di *questo* segmento di *questa* sessione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsistencyProof {
    pub session: SessionId,
    pub index: usize,
    pub from: u64,
    pub to: u64,
    pub rows: usize,
    pub segment_root: Hash,
    /// Dal gradino più profondo in su, così si rilegge riseguendolo.
    pub steps: Vec<ProofStep>,
    /// La testa che questa prova dovrebbe ricostruire.
    pub head: Hash,
}

impl ConsistencyProof {
    /// Ripercorre il cammino dalla radice del segmento fino alla testa.
    ///
    /// `Ok` porta la testa ricostruita. Non chiede fiducia in nessuno: la
    /// prova contiene tutto ciò che serve, e ciò che contiene è ciò che
    /// produce il risultato.
    pub fn check(&self) -> Result<Hash, ProofError> {
        let mut acc = self.segment_root;
        for step in &self.steps {
            acc = match step.side {
                Side::Left => node(&step.hash, &acc),
                Side::Right => node(&acc, &step.hash),
            };
        }
        if acc == self.head {
            Ok(acc)
        } else {
            Err(ProofError::RebuiltMismatch {
                rebuilt: acc,
                declared: self.head,
            })
        }
    }

    /// La prova come testo, per l'export a colonne fisse:
    /// `left:<hex>,right:<hex>`.
    pub fn to_text(&self) -> String {
        self.steps
            .iter()
            .map(|s| {
                let side = match s.side {
                    Side::Left => "left",
                    Side::Right => "right",
                };
                format!("{side}:{}", s.hash)
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn from_text(text: &str) -> Result<Vec<ProofStep>, ProofError> {
        if text.is_empty() {
            return Ok(vec![]);
        }
        text.split(',')
            .map(|part| match part.split_once(':') {
                Some(("left", hex)) => Ok(ProofStep {
                    hash: Hash::from_hex(hex)?,
                    side: Side::Left,
                }),
                Some(("right", hex)) => Ok(ProofStep {
                    hash: Hash::from_hex(hex)?,
                    side: Side::Right,
                }),
                _ => Err(ProofError::Malformed(part.to_owned())),
            })
            .collect()
    }
}

/// Perché una prova non sta in piedi.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProofError {
    #[error("la prova appartiene alla sessione `{declared}`, non a `{expected}`")]
    WrongSession {
        expected: SessionId,
        declared: SessionId,
    },

    #[error("la prova dichiara il segmento {index}, la sessione ne ha {count}")]
    NoSuchSegment {
        session: SessionId,
        index: usize,
        count: usize,
    },

    #[error("la prova copre le righe {declared_from}..={declared_to}, il segmento {index} copre {expected_from}..={expected_to}")]
    WrongRange {
        index: usize,
        expected_from: u64,
        expected_to: u64,
        declared_from: u64,
        declared_to: u64,
    },

    #[error("la prova dichiara la radice {declared}, il segmento {index} ha la radice {expected}")]
    WrongRoot {
        index: usize,
        expected: Hash,
        declared: Hash,
    },

    #[error("la prova dichiara la testa {declared}, questa sessione ha la testa {expected}")]
    WrongHead {
        expected: Hash,
        declared: Hash,
    },

    #[error("riseguendo la prova si ottiene {rebuilt}, non la testa dichiarata {declared}")]
    RebuiltMismatch { rebuilt: Hash, declared: Hash },

    #[error("hash non valido: {0}")]
    Hash(#[from] HashParseError),

    #[error("prova di coerenza malformata: {0}")]
    Malformed(String),
}

/// Un albero di Merkle costruito sulla regola di RFC 9162: l'albero di `n`
/// foglie si divide al più grande multiplo di due minore di `n`, così un
/// albero di 5 foglie è `node(MTH(0..4), foglia 4)`.
///
/// La regola conta perché la prova di inclusione è un cammino: con la regola
/// «duplica l'ultimo nodo dispari» un nodo avrebbe due padri e il cammino non
/// sarebbe definito. Qui ogni nodo ha un solo padre e il cammino è unico.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MerkleTree {
    nodes: Vec<Hash>,
    parent: Vec<Option<usize>>,
    sibling: Vec<Option<usize>>,
    is_right: Vec<bool>,
    leaves: Vec<usize>,
    root: usize,
    len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("albero senza foglie: una testa si definisce su almeno una riga")]
struct EmptyTree;

impl MerkleTree {
    fn build(leaves: &[Hash]) -> Result<Self, EmptyTree> {
        if leaves.is_empty() {
            return Err(EmptyTree);
        }
        let mut t = MerkleTree {
            nodes: Vec::with_capacity(2 * leaves.len()),
            parent: Vec::with_capacity(2 * leaves.len()),
            sibling: Vec::with_capacity(2 * leaves.len()),
            is_right: Vec::with_capacity(2 * leaves.len()),
            leaves: Vec::with_capacity(leaves.len()),
            root: 0,
            len: leaves.len(),
        };
        t.root = t.push(leaves);
        Ok(t)
    }

    /// Costruisce il sottoalbero e restituisce l'indice del suo nodo radice.
    fn push(&mut self, leaves: &[Hash]) -> usize {
        if leaves.len() == 1 {
            let i = self.nodes.len();
            self.nodes.push(leaves[0]);
            self.parent.push(None);
            self.sibling.push(None);
            self.is_right.push(false);
            self.leaves.push(i);
            return i;
        }
        let k = split(leaves.len());
        let l = self.push(&leaves[..k]);
        let r = self.push(&leaves[k..]);
        let i = self.nodes.len();
        self.nodes.push(node(&self.nodes[l], &self.nodes[r]));
        self.parent.push(None);
        self.sibling.push(None);
        self.is_right.push(false);
        self.parent[l] = Some(i);
        self.sibling[l] = Some(r);
        self.is_right[l] = false;
        self.parent[r] = Some(i);
        self.sibling[r] = Some(l);
        self.is_right[r] = true;
        i
    }

    fn root(&self) -> Hash {
        self.nodes[self.root]
    }

    /// Il cammino dalla foglia `index` alla radice, dal gradino più profondo
    /// in su.
    fn path(&self, index: usize) -> Option<Vec<ProofStep>> {
        let mut cur = *self.leaves.get(index)?;
        let mut steps = Vec::with_capacity(self.depth());
        while let Some(p) = self.parent[cur] {
            let sibling = self.sibling[cur]?;
            steps.push(ProofStep {
                hash: self.nodes[sibling],
                side: if self.is_right[cur] {
                    Side::Left
                } else {
                    Side::Right
                },
            });
            cur = p;
        }
        Some(steps)
    }

    /// I livelli dell'albero: `split` dimezza sempre, quindi un albero profondo
    /// `d` ha al più `2^d` foglie.
    fn depth(&self) -> usize {
        let mut d = 0;
        while (1usize << d) < self.len {
            d += 1;
        }
        d
    }
}

/// La più grande potenza di due strettamente minore di `n` (`n >= 2`).
fn split(n: usize) -> usize {
    debug_assert!(n >= 2);
    let k = 1usize << (usize::BITS - 1 - n.leading_zeros());
    if k == n { k / 2 } else { k }
}

/// La catena di una sessione di registro: righe in, testa fuori.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    session: SessionId,
    rows: usize,
    plan: SegmentPlan,
    segments: Vec<Segment>,
    tree: MerkleTree,
}

impl Chain {
    /// Costruisce la catena dalle righe, nell'ordine in cui sono arrivate.
    ///
    /// Le `seq` devono essere **strettamente crescenti e senza salti**. Il
    /// valore della prima riga è libero: la catena impegna la *posizione*, e
    /// il numero che lo storage usa per indicizzare non è affar suo — un
    /// registro che comincia da 1 è un registro come uno che comincia da 0, e
    /// costringere a 0 sarebbe un falso positivo su un'informazione che la
    /// crittografia già porta dentro la riga.
    ///
    /// Un salto invece è un errore: significa che una riga non c'è, e una
    /// catena con un buco non attesta un registro continuo.
    pub fn build(
        session: &SessionId,
        rows: &[Observation],
        plan: SegmentPlan,
    ) -> Result<Chain, VerifyError> {
        if rows.is_empty() {
            return Err(VerifyError::EmptySession {
                session: session.clone(),
            });
        }
        if plan.per_segment == 0 {
            return Err(VerifyError::ZeroSegment(plan.per_segment));
        }

        let mut seen: Vec<&str> = Vec::with_capacity(rows.len());
        let mut leaves: Vec<Hash> = Vec::with_capacity(rows.len());
        for (at, row) in rows.iter().enumerate() {
            let found = row.seq.0;
            if at > 0 {
                let prev = rows[at - 1].seq.0;
                if found <= prev {
                    return Err(VerifyError::OutOfOrder {
                        session: session.clone(),
                        at,
                        prev,
                        found,
                    });
                }
                if found != prev + 1 {
                    return Err(VerifyError::SeqGap {
                        session: session.clone(),
                        at,
                        found,
                        expected: prev + 1,
                    });
                }
            }
            if let Some(first) = seen.iter().position(|id| *id == row.id.as_str()) {
                return Err(VerifyError::DuplicateRowId {
                    session: session.clone(),
                    id: row.id.clone(),
                    first,
                    again: at,
                });
            }
            seen.push(row.id.as_str());
            leaves.push(leaf_of(row)?);
        }

        let mut segments = Vec::new();
        let mut roots = Vec::new();
        for (index, chunk) in leaves.chunks(plan.per_segment).enumerate() {
            let root = match MerkleTree::build(chunk) {
                Ok(t) => t.root(),
                Err(_) => return Err(VerifyError::EmptySegment { index }),
            };
            roots.push(root);
            segments.push(Segment {
                index,
                from: rows[index * plan.per_segment].seq.0,
                to: rows[index * plan.per_segment + chunk.len() - 1].seq.0,
                rows: chunk.len(),
                root,
            });
        }

        let tree = match MerkleTree::build(&roots) {
            Ok(t) => t,
            // `roots` non è mai vuota: `rows` non è vuota e ogni segmento
            // prende almeno una riga. Se lo fosse, la sessione non avrebbe
            // alcuna testa da attestare, che è ciò che dice l'errore.
            Err(_) => {
                return Err(VerifyError::EmptySession {
                    session: session.clone(),
                })
            }
        };

        Ok(Chain {
            session: session.clone(),
            rows: rows.len(),
            plan,
            segments,
            tree,
        })
    }

    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// La testa della sessione: la quantità che il testimone ricorda e che
    /// l'export porta fuori.
    pub fn head(&self) -> Hash {
        self.tree.root()
    }

    /// Quante righe copre.
    pub fn len(&self) -> usize {
        self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    pub fn plan(&self) -> SegmentPlan {
        self.plan
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// La prima `seq` coperta dalla sessione, quella delle righe — non `0`.
    pub fn first_seq(&self) -> Result<u64, VerifyError> {
        match self.segments.first() {
            Some(s) => Ok(s.from),
            None => Err(VerifyError::EmptySession {
                session: self.session.clone(),
            }),
        }
    }

    /// L'ultima `seq` coperta dalla sessione.
    pub fn last_seq(&self) -> Result<u64, VerifyError> {
        match self.segments.last() {
            Some(s) => Ok(s.to),
            None => Err(VerifyError::EmptySession {
                session: self.session.clone(),
            }),
        }
    }

    pub fn segment(&self, index: usize) -> Result<&Segment, VerifyError> {
        self.segments.get(index).ok_or_else(|| VerifyError::NoSuchSegment {
            session: self.session.clone(),
            index,
            count: self.segments.len(),
        })
    }

    /// La prova che il segmento `index` sta in quella posizione della testa.
    pub fn consistency_proof(&self, index: usize) -> Result<ConsistencyProof, VerifyError> {
        let seg = self.segment(index)?.clone();
        let steps = self.tree.path(index).ok_or_else(|| VerifyError::NoSuchSegment {
            session: self.session.clone(),
            index,
            count: self.segments.len(),
        })?;
        Ok(ConsistencyProof {
            session: self.session.clone(),
            index: seg.index,
            from: seg.from,
            to: seg.to,
            rows: seg.rows,
            segment_root: seg.root,
            steps,
            head: self.head(),
        })
    }

    /// Tutte le prove della sessione, in ordine di segmento.
    pub fn consistency_proofs(&self) -> Result<Vec<ConsistencyProof>, VerifyError> {
        let mut out = Vec::with_capacity(self.segments.len());
        for index in 0..self.segments.len() {
            out.push(self.consistency_proof(index)?);
        }
        Ok(out)
    }

    /// Verifica la prova in sé e rispetto a questa catena, e restituisce la
    /// testa ricostruita.
    pub fn check(&self, proof: &ConsistencyProof) -> Result<Hash, ProofError> {
        if proof.session != self.session {
            return Err(ProofError::WrongSession {
                expected: self.session.clone(),
                declared: proof.session.clone(),
            });
        }
        if proof.head != self.head() {
            return Err(ProofError::WrongHead {
                expected: self.head(),
                declared: proof.head,
            });
        }
        let seg = match self.segments.get(proof.index) {
            Some(s) => s,
            None => {
                return Err(ProofError::NoSuchSegment {
                    session: self.session.clone(),
                    index: proof.index,
                    count: self.segments.len(),
                })
            }
        };
        if seg.from != proof.from || seg.to != proof.to || seg.rows != proof.rows {
            return Err(ProofError::WrongRange {
                index: proof.index,
                expected_from: seg.from,
                expected_to: seg.to,
                declared_from: proof.from,
                declared_to: proof.to,
            });
        }
        if seg.root != proof.segment_root {
            return Err(ProofError::WrongRoot {
                index: proof.index,
                expected: seg.root,
                declared: proof.segment_root,
            });
        }
        proof.check()
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sessione `{}`: {} righe, {} segmenti, testa {}",
            self.session,
            self.rows,
            self.segments.len(),
            self.head()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::leaf;
    use kbs_core::{
        ArgumentId, CohortId, CourseId, Evidence, GraderKind, Millis, PersonId, SeqInSession,
    };

    fn obs(seq: u64, tag: &str) -> Observation {
        Observation {
            id: format!("obs-{tag}-{seq}"),
            seq: SeqInSession(seq),
            student: PersonId(format!("person_{tag}")),
            course: CourseId("course_x".into()),
            cohort: CohortId("cohort_x".into()),
            argument: ArgumentId("arg_x".into()),
            evidence: Evidence::Checked {
                exercise: "ex-1".into(),
                instance: format!("inst-{seq}"),
                correct: seq % 3 != 0,
            },
            judged_by: Some(GraderKind::Deterministic),
            unaided: Some(seq % 4 != 3),
            n_hints: Some(if seq % 4 == 3 { 2 } else { 0 }),
            at: Millis(1_700_000_000_000 + seq as i64),
        }
    }

    fn rows(n: u64, tag: &str) -> Vec<Observation> {
        (0..n).map(|i| obs(i, tag)).collect()
    }

    fn sess(name: &str) -> SessionId {
        SessionId::new(name)
    }

    fn build(rows: &[Observation]) -> Chain {
        Chain::build(&sess("s"), rows, SegmentPlan::default_plan()).unwrap()
    }

    #[test]
    fn split_è_la_potenza_di_due_strettamente_precedente() {
        assert_eq!(split(2), 1);
        assert_eq!(split(3), 2);
        assert_eq!(split(4), 2);
        assert_eq!(split(5), 4);
        assert_eq!(split(8), 4);
        assert_eq!(split(9), 8);
        assert_eq!(split(1024), 512);
    }

    #[test]
    fn radice_di_quattro_foglie_conosciuta() {
        let leaves: Vec<Hash> = (0..4u8).map(|i| leaf(&format!("r{i}"))).collect();
        let t = MerkleTree::build(&leaves).unwrap();
        let expected = node(&node(&leaves[0], &leaves[1]), &node(&leaves[2], &leaves[3]));
        assert_eq!(t.root(), expected);
    }

    #[test]
    fn radice_di_cinque_foglie_usa_la_regola_di_rfc9162() {
        let leaves: Vec<Hash> = (0..5u8).map(|i| leaf(&format!("r{i}"))).collect();
        let t = MerkleTree::build(&leaves).unwrap();
        let left = node(&node(&leaves[0], &leaves[1]), &node(&leaves[2], &leaves[3]));
        assert_eq!(t.root(), node(&left, &leaves[4]));
    }

    #[test]
    fn albero_vuoto_non_ha_testa() {
        assert!(MerkleTree::build(&[]).is_err());
    }

    #[test]
    fn due_sessioni_diverse_con_le_stesse_righe_hanno_la_stessa_testa() {
        let r = rows(9, "a");
        let a = Chain::build(&sess("sessione-a"), &r, SegmentPlan::every(4)).unwrap();
        let b = Chain::build(&sess("sessione-b"), &r, SegmentPlan::every(4)).unwrap();
        assert_eq!(a.head(), b.head());
        assert_ne!(a.session(), b.session());
    }

    #[test]
    fn cambiare_una_riga_qualunque_cambia_la_testa() {
        let base = rows(9, "a");
        let head = build(&base).head();

        let mut last = base.clone();
        last[8].evidence = Evidence::Oral {
            note: "interrogazione".into(),
            witness: Some(PersonId("person_t".into())),
        };
        assert_ne!(build(&last).head(), head);

        let mut first = base.clone();
        first[0].judged_by = Some(GraderKind::Human);
        assert_ne!(build(&first).head(), head);

        let mut middle = base.clone();
        middle[4].at = Millis(1);
        assert_ne!(build(&middle).head(), head);
    }

    #[test]
    fn scambiare_due_righe_cambia_la_testa() {
        let base = rows(9, "a");
        let head = build(&base).head();
        let mut swapped = base.clone();
        // le due righe si scambiano di posto, sequenza compresa: la catena
        // impegna l'ordine, non il contenuto.
        swapped.swap(1, 2);
        swapped[1].seq = SeqInSession(1);
        swapped[2].seq = SeqInSession(2);
        assert_eq!(swapped[1].id, base[2].id);
        assert_ne!(build(&swapped).head(), head);
    }

    #[test]
    fn la_segmentazione_non_cambia_la_testa() {
        let r = rows(9, "a");
        let a = Chain::build(&sess("s"), &r, SegmentPlan::every(1)).unwrap();
        let b = Chain::build(&sess("s"), &r, SegmentPlan::every(2)).unwrap();
        let c = Chain::build(&sess("s"), &r, SegmentPlan::every(64)).unwrap();
        assert_eq!(a.head(), b.head());
        assert_eq!(b.head(), c.head());
    }

    #[test]
    fn sessione_vuota_e_un_errore_e_non_una_testa_sul_nulla() {
        assert_eq!(
            Chain::build(&sess("s"), &[], SegmentPlan::default_plan()),
            Err(VerifyError::EmptySession { session: sess("s") })
        );
    }

    #[test]
    fn seq_non_contigua_e_un_errore() {
        let mut r = rows(3, "a");
        r[1].seq = SeqInSession(7);
        assert_eq!(
            Chain::build(&sess("s"), &r, SegmentPlan::default_plan()),
            Err(VerifyError::SeqGap {
                session: sess("s"),
                at: 1,
                found: 7,
                expected: 1
            })
        );
        // un buco anche subito dopo la prima riga: il valore iniziale è
        // libero, la continuità no
        let mut buco = rows(3, "a");
        buco[0].seq = SeqInSession(1);
        buco[1].seq = SeqInSession(3);
        assert!(matches!(
            Chain::build(&sess("s"), &buco, SegmentPlan::default_plan()),
            Err(VerifyError::SeqGap { at: 1, found: 3, .. })
        ));
    }

    #[test]
    fn la_prima_seq_e_libera() {
        // un registro che comincia da 1 è un registro come uno che comincia da
        // 0: la catena impegna la posizione, e la `seq` la porta dentro la
        // foglia. Obbligare a 0 sarebbe un falso positivo su una convenzione
        // dello storage.
        let da_zero = rows(3, "a");
        let mut da_uno = da_zero.clone();
        for (i, r) in da_uno.iter_mut().enumerate() {
            r.seq = SeqInSession(i as u64 + 1);
        }
        assert!(Chain::build(&sess("s"), &da_zero, SegmentPlan::default_plan()).is_ok());
        assert!(Chain::build(&sess("s"), &da_uno, SegmentPlan::default_plan()).is_ok());
        // la testa cambia, perché la `seq` è una delle cose firmate
        assert_ne!(build(&da_zero).head(), build(&da_uno).head());
    }

    #[test]
    fn righe_fuori_ordine_sono_un_errore() {
        // seq 0, 1, 0: la terza riga torna indietro rispetto alla seconda
        let mut r = rows(3, "a");
        r[2].seq = SeqInSession(0);
        assert_eq!(
            Chain::build(&sess("s"), &r, SegmentPlan::default_plan()),
            Err(VerifyError::OutOfOrder {
                session: sess("s"),
                at: 2,
                prev: 1,
                found: 0
            })
        );
    }

    #[test]
    fn id_ripetuto_e_un_errore() {
        let mut r = rows(3, "a");
        r[2].id = r[0].id.clone();
        assert_eq!(
            Chain::build(&sess("s"), &r, SegmentPlan::default_plan()),
            Err(VerifyError::DuplicateRowId {
                session: sess("s"),
                id: r[0].id.clone(),
                first: 0,
                again: 2
            })
        );
    }

    #[test]
    fn segmentazione_a_zero_righe_e_un_errore() {
        let r = rows(3, "a");
        assert_eq!(
            Chain::build(&sess("s"), &r, SegmentPlan::every(0)),
            Err(VerifyError::ZeroSegment(0))
        );
    }

    #[test]
    fn segmento_inesistente_e_un_errore_nominato() {
        let c = build(&rows(3, "a"));
        assert_eq!(
            c.segment(9),
            Err(VerifyError::NoSuchSegment {
                session: sess("s"),
                index: 9,
                count: 1
            })
        );
        assert!(matches!(
            c.consistency_proof(9),
            Err(VerifyError::NoSuchSegment { index: 9, .. })
        ));
    }

    #[test]
    fn i_segmenti_coprono_tutte_le_righe_nel_ordine() {
        let c = Chain::build(&sess("s"), &rows(10, "a"), SegmentPlan::every(4)).unwrap();
        assert_eq!(c.len(), 10);
        let segs = c.segments();
        assert_eq!(segs.len(), 3);
        assert_eq!((segs[0].from, segs[0].to, segs[0].rows), (0, 3, 4));
        assert_eq!((segs[1].from, segs[1].to, segs[1].rows), (4, 7, 4));
        assert_eq!((segs[2].from, segs[2].to, segs[2].rows), (8, 9, 2));
        assert_eq!(
            segs.iter().map(|s| s.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    /// Per ogni dimensione e per ogni piano, ogni prova ricostruisce la
    /// testa. È la verifica esaustiva del cammino: da 1 a 24 righe, piani da 1
    /// a 6 righe per segmento, tutti gli indici.
    #[test]
    fn ogni_prova_di_ogni_segmento_ricostruisce_la_testa() {
        for n in 1..=24usize {
            for per in 1..=6usize {
                let r: Vec<Observation> = (0..n as u64).map(|i| obs(i, "a")).collect();
                let c = Chain::build(&sess("s"), &r, SegmentPlan::every(per)).unwrap();
                for p in c.consistency_proofs().unwrap() {
                    assert_eq!(
                        p.check().unwrap(),
                        c.head(),
                        "n={n} per={per} idx={}",
                        p.index
                    );
                    assert_eq!(c.check(&p).unwrap(), c.head());
                }
            }
        }
    }

    #[test]
    fn il_cammino_ha_un_gradino_per_livello() {
        // otto segmenti ⇒ tre livelli: la prova scende fino alla radice con tre
        // gradini, non con uno.
        let c = Chain::build(&sess("s"), &rows(8, "a"), SegmentPlan::every(1)).unwrap();
        assert_eq!(c.consistency_proof(0).unwrap().steps.len(), 3);
        assert_eq!(c.consistency_proof(7).unwrap().steps.len(), 3);
        // un solo segmento: nessun gradino, la testa è la radice
        let one = Chain::build(&sess("s"), &rows(3, "a"), SegmentPlan::default_plan()).unwrap();
        assert!(one.consistency_proof(0).unwrap().steps.is_empty());
        assert_eq!(one.head(), one.segments()[0].root);
    }

    #[test]
    fn prova_tamperata_non_verifica() {
        let c = Chain::build(&sess("s"), &rows(8, "a"), SegmentPlan::every(2)).unwrap();
        let mut p = c.consistency_proof(2).unwrap();
        p.steps[0].hash = leaf("altro");
        assert!(matches!(
            p.check(),
            Err(ProofError::RebuiltMismatch { .. })
        ));

        let mut q = c.consistency_proof(2).unwrap();
        q.head = leaf("testa inventata");
        assert!(matches!(
            q.check(),
            Err(ProofError::RebuiltMismatch { .. })
        ));
    }

    #[test]
    fn prova_di_una_sessione_non_verifica_in_un_altra() {
        let a = Chain::build(&sess("a"), &rows(8, "a"), SegmentPlan::every(2)).unwrap();
        let b = Chain::build(&sess("b"), &rows(8, "b"), SegmentPlan::every(2)).unwrap();
        let p = a.consistency_proof(0).unwrap();
        assert!(matches!(b.check(&p), Err(ProofError::WrongSession { .. })));
    }

    #[test]
    fn prova_che_parla_di_una_parte_diversa_non_verifica() {
        let c = Chain::build(&sess("s"), &rows(8, "a"), SegmentPlan::every(2)).unwrap();
        let mut p = c.consistency_proof(0).unwrap();
        p.from += 1;
        assert!(matches!(c.check(&p), Err(ProofError::WrongRange { .. })));

        let mut q = c.consistency_proof(0).unwrap();
        q.segment_root = leaf("radice inventata");
        assert!(matches!(c.check(&q), Err(ProofError::WrongRoot { .. })));

        let mut r = c.consistency_proof(0).unwrap();
        r.index = 9;
        assert!(matches!(
            c.check(&r),
            Err(ProofError::NoSuchSegment { index: 9, .. })
        ));

        let mut s = c.consistency_proof(0).unwrap();
        s.head = leaf("testa inventata");
        assert!(matches!(c.check(&s), Err(ProofError::WrongHead { .. })));
    }

    #[test]
    fn testo_della_prova_andata_e_ritorno() {
        let c = Chain::build(&sess("s"), &rows(8, "a"), SegmentPlan::every(2)).unwrap();
        for p in c.consistency_proofs().unwrap() {
            assert_eq!(
                ConsistencyProof::from_text(&p.to_text()).unwrap(),
                p.steps
            );
        }
        assert!(ConsistencyProof::from_text("").unwrap().is_empty());
        assert!(ConsistencyProof::from_text("left:zz").is_err());
        assert!(ConsistencyProof::from_text("up:aa").is_err());
    }

    #[test]
    fn display_riferisce_sessione_righe_e_testa() {
        let c = build(&rows(3, "a"));
        let s = c.to_string();
        assert!(s.contains('s'));
        assert!(s.contains("3 righe"));
        assert!(s.contains(&c.head().to_hex()));
    }

    #[test]
    fn primo_e_ultimo_seq_sono_quelli_delle_righe() {
        let mut r = rows(5, "a");
        for (i, o) in r.iter_mut().enumerate() {
            o.seq = SeqInSession(i as u64 + 1);
        }
        let c = Chain::build(&sess("s"), &r, SegmentPlan::every(2)).unwrap();
        assert_eq!(c.first_seq().unwrap(), 1);
        assert_eq!(c.last_seq().unwrap(), 5);
        // i segmenti coprono lo stesso intervallo, spezzato
        assert_eq!(
            c.segments()
                .iter()
                .map(|s| (s.from, s.to))
                .collect::<Vec<_>>(),
            vec![(1, 2), (3, 4), (5, 5)]
        );
    }

    #[test]
    fn una_riga_senza_giudice_e_una_riga_diversa() {
        let a = obs(0, "a");
        let mut b = obs(0, "a");
        b.judged_by = None;
        assert_ne!(build(&[a]).head(), build(&[b]).head());
    }
}
