//! La verifica di una sessione, e il verdetto che porta con sé i tre limiti.
//!
//! [`verify`] non restituisce un verdetto nudo: restituisce un
//! [`Verified`](crate::limits::Verified), che è il verdetto più i tre limiti di
//! D6. Chi riceve il valore li ha per le mani, e stamparlo li stampa: non può
//! mostrare il verdetto senza mostrare quello che non è garantito. Può
//! naturalmente prendere il verdetto da solo — [`Verified::verdict`] esiste e
//! non è nascosto — ma in quel caso è una sua scelta, dichiarata, e non
//! qualcosa che il tipo finge di impedire.
//!
//! Il verdetto dice una cosa sola: se le righe **stanno in piedi** rispetto a
//! ciò che è stato dichiarato prima. Non dice se le righe sono vere. Chi vuole
//! la verità deve leggere i limiti, che sono lì per quello.

use crate::chain::{Chain, SegmentPlan, VerifyError};
use crate::hash::Hash;
use crate::limits::Verified;
use crate::session::SessionId;
use crate::witness::{Confrontation, Witness};
use kbs_core::Observation;
use serde::Serialize;
use std::fmt;

/// Che cosa è stato verificato, e contro che cosa.
#[derive(Debug, Clone)]
pub struct VerifyRequest<'a> {
    pub session: &'a SessionId,
    pub rows: &'a [Observation],
    pub plan: SegmentPlan,
    /// La testa dichiarata in precedenza, se qualcuno l'ha conservata.
    pub declared_head: Option<&'a Hash>,
    /// La copia indipendente, se qualcuno la tiene.
    pub witness: Option<&'a Witness>,
}

impl<'a> VerifyRequest<'a> {
    pub fn new(session: &'a SessionId, rows: &'a [Observation]) -> Self {
        VerifyRequest {
            session,
            rows,
            plan: SegmentPlan::default_plan(),
            declared_head: None,
            witness: None,
        }
    }

    pub fn with_plan(mut self, plan: SegmentPlan) -> Self {
        self.plan = plan;
        self
    }

    pub fn with_declared_head(mut self, head: &'a Hash) -> Self {
        self.declared_head = Some(head);
        self
    }

    pub fn with_witness(mut self, witness: &'a Witness) -> Self {
        self.witness = Some(witness);
        self
    }
}

/// Perché le righe stanno in piedi, o perché no.
///
/// Le varianti dicono **contro che cosa**: la testa dichiarata, la copia
/// indipendente, o niente. Nessuna di esse è «vero»: è «coerente».
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "coherence", rename_all = "kebab-case")]
pub enum Coherence {
    /// Le righe ricostruiscono esattamente ciò che era stato dichiarato.
    /// `anchored` dice se c'era una copia indipendente a confermarlo: senza,
    /// la coerenza è interna e non prova niente (limite 1).
    Coherent { anchored: bool },

    /// Le righe non ricostruiscono la testa dichiarata.
    ContradictsDeclaredHead { declared: Hash, recomputed: Hash },

    /// La catena presentata è un registro più vecchio di quelli già visti.
    Rollback {
        presented: Hash,
        presented_rows: usize,
        witnessed: Hash,
        witnessed_rows: usize,
    },

    /// La catena presentata non è mai stata vista dalla copia indipendente.
    ContradictsWitness {
        presented: Hash,
        presented_rows: usize,
        witnessed: Hash,
        witnessed_rows: usize,
    },

    /// La copia indipendente non torna con sé stessa: non ci si può fidare
    /// nemmeno di lei finché non è sistemata.
    BrokenWitness { declared: Option<Hash> },
}

impl Coherence {
    /// Le righe stanno in piedi. Non significa che dicano il vero: per quello
    /// c'è il terzo limite.
    pub fn is_coherent(&self) -> bool {
        matches!(self, Coherence::Coherent { .. })
    }

    /// Qualcuno di indipendente dal sistema ha visto questa testa.
    pub fn is_anchored(&self) -> bool {
        matches!(self, Coherence::Coherent { anchored: true })
    }

    pub fn is_contradiction(&self) -> bool {
        !self.is_coherent()
    }
}

impl fmt::Display for Coherence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Coherence::Coherent { anchored: true } => f.write_str(
                "coerente e ancorata: le righe ricostruiscono la testa che la copia indipendente aveva già visto",
            ),
            Coherence::Coherent { anchored: false } => f.write_str(
                "coerente ma non ancorata: le righe ricostruiscono la testa dichiarata, e nessuna copia indipendente la conferma",
            ),
            Coherence::ContradictsDeclaredHead {
                declared,
                recomputed,
            } => write!(
                f,
                "in contraddizione con la testa dichiarata: dichiarata {declared}, ricalcolata {recomputed}"
            ),
            Coherence::Rollback {
                presented,
                presented_rows,
                witnessed,
                witnessed_rows,
            } => write!(
                f,
                "rollback: la testa presentata {presented} a {presented_rows} righe era già stata vista a {witnessed} righe ({witnessed_rows})"
            ),
            Coherence::ContradictsWitness {
                presented,
                presented_rows,
                witnessed,
                witnessed_rows,
            } => write!(
                f,
                "riscrittura: la testa presentata {presented} a {presented_rows} righe non è mai stata vista; l'ultima vista era {witnessed} a {witnessed_rows} righe"
            ),
            Coherence::BrokenWitness { declared } => write!(
                f,
                "testimone incoerente: dichiara {declared:?} e dalle sue voci non si ricostruisce"
            ),
        }
    }
}

/// Il verdetto su una sessione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainVerdict {
    pub session: SessionId,
    /// La testa ricalcolata dalle righe di questa verifica.
    pub head: Hash,
    pub rows: usize,
    pub segments: usize,
    pub coherence: Coherence,
}

impl fmt::Display for ChainVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sessione `{}`: {} righe, {} segmenti, testa {} — {}",
            self.session,
            self.rows,
            self.segments,
            self.head,
            self.coherence
        )
    }
}

/// Verifica una sessione e restituisce il verdetto **con i tre limiti**.
///
/// L'ordine di prevalenza è dichiarato e non è casuale: se le righe non
/// ricostruiscono la testa dichiarata, quella è la contraddizione e va
/// detta; altrimenti si ascolta la copia indipendente; e una testa
/// dichiarata che le righe **ricostruiscono** non zittisce un testimone che
/// la contraddice — due controlli indipendenti che si dicono entrambi, non
/// uno che vince sull'altro.
///
/// Una catena riscritta da capo, senza testimone e senza testa dichiarata, si
/// verifica come [`Coherence::Coherent`] con `anchored: false`. Non è un
/// difetto di questa funzione: è il primo limite di D6, dichiarato nel valore
/// che questa funzione restituisce.
pub fn verify(req: VerifyRequest<'_>) -> Result<Verified<ChainVerdict>, VerifyError> {
    let chain = Chain::build(req.session, req.rows, req.plan)?;

    let coherence = match req.declared_head {
        Some(declared) if *declared != chain.head() => Coherence::ContradictsDeclaredHead {
            declared: *declared,
            recomputed: chain.head(),
        },
        _ => match req.witness {
            None => Coherence::Coherent { anchored: false },
            Some(witness) => {
                if witness.check().is_err() {
                    // Il testimone non si contraddice da solo: si sa almeno
                    // che non è più fidabile, il che è un verdetto, non un
                    // errore di esecuzione.
                    Coherence::BrokenWitness {
                        declared: witness.head(),
                    }
                } else {
                    match witness.confront(&chain) {
                        Confrontation::Unanchored => Coherence::Coherent { anchored: false },
                        Confrontation::Agrees { .. } => Coherence::Coherent { anchored: true },
                        Confrontation::Rollback { presented, witnessed } => Coherence::Rollback {
                            presented,
                            presented_rows: chain.len(),
                            witnessed: witnessed.head,
                            witnessed_rows: witnessed.rows,
                        },
                        Confrontation::Diverges { presented, latest } => Coherence::ContradictsWitness {
                            presented,
                            presented_rows: chain.len(),
                            witnessed: latest.head,
                            witnessed_rows: latest.rows,
                        },
                    }
                }
            }
        },
    };

    Ok(Verified::new(ChainVerdict {
        session: chain.session().clone(),
        head: chain.head(),
        rows: chain.len(),
        segments: chain.segments().len(),
        coherence,
    }))
}
