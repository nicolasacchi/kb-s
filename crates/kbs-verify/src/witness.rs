//! Il testimone: la copia indipendente che rende visibile la riscrittura.
//!
//! Il primo limite di D6 dice che chi riscrive l'intera catena produce una
//! catena coerente, indistinguibile **senza una copia indipendente**. Il
//! testimone è quella copia: non è dentro il sistema, è tenuto da chi non vi
//! ha accesso, e contiene solo la testa — che è tutto ciò che serve per
//! accorgersi che il registro è cambiato.
//!
//! Non è inutile che contenga una cosa sola. Con le righe si potrebbe mostrare
//! *quali* righe sono cambiate; con la testa si mostra solo *che* il registro è
//! cambiato. È il punto: la copia indipendente minima.
//!
//! Il testimone cresce **per append** e rifiuta una voce con un numero di righe
//! non crescente. È il secondo limite che diventa manipolabile: un rollback da
//! un backup presenta una testa *vecchia*, e la monotonia è ciò che distingue
//! «sta indietro nel tempo» da «è stato riscritto».
//!
//! Anche il testimone è una catena di hash: ogni voce impegna la precedente, e
//! una voce riscritta rompe [`Witness::check`]. Non è a prova di riscrittura
//! integrale — chi riscrive tutto riscrive anche la catena — ed è il primo
//! limite applicato al rimedio del primo limite.

use crate::canonical::leaf_of;
use crate::chain::Chain;
use crate::hash::{Hash, node};
use crate::session::SessionId;
use kbs_core::Millis;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Una voce del testimone: «a questo momento, la sessione aveva questa testa».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessEntry {
    pub session: SessionId,
    /// Quante righe aveva la sessione. Essendo il registro append-only,
    /// questo numero è anche la lunghezza del prefisso che la testa impegna.
    pub rows: usize,
    pub head: Hash,
    pub at: Millis,
}

impl WitnessEntry {
    /// L'ultima `seq` coperta, se la voce copre qualcosa.
    pub fn last_seq(&self) -> Option<u64> {
        self.rows.checked_sub(1).map(|r| r as u64)
    }
}

/// Il registro delle teste viste, tenuto fuori dal sistema.
///
/// Ogni voce impegna la precedente con `node`, quindi il testimone è a sua
/// volta una catena e [`Witness::head`] ne è la testa.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Witness {
    entries: Vec<WitnessEntry>,
    /// La testa delle voci, ricalcolabile e quindi verificabile.
    head: Option<Hash>,
}

impl Witness {
    pub fn new() -> Self {
        Witness::default()
    }

    pub fn entries(&self) -> &[WitnessEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// La voce più recente della sessione: quella che il testimone considera
    /// «lo stato da cui non si torna indietro».
    pub fn latest(&self, session: &SessionId) -> Option<&WitnessEntry> {
        self.entries.iter().rev().find(|e| &e.session == session)
    }

    /// La testa del testimone, `None` se non ha voci.
    pub fn head(&self) -> Option<Hash> {
        self.head
    }

    /// Ricalcola la testa dalle voci. Serve a [`Witness::check`].
    pub fn recompute(&self) -> Option<Hash> {
        let mut acc: Option<Hash> = None;
        for e in &self.entries {
            let leaf = match leaf_of(e) {
                Ok(l) => l,
                Err(_) => return None,
            };
            acc = Some(match acc {
                None => leaf,
                Some(prev) => node(&prev, &leaf),
            });
        }
        acc
    }

    /// Verifica che le voci e la testa dichiarata tornino.
    ///
    /// Non prova che le voci siano vere: prova che nessuno le abbia cambiate
    /// senza rifare la catena. Il limite 1 vale anche per il testimone.
    pub fn check(&self) -> Result<(), WitnessError> {
        let recomputed = self.recompute();
        if recomputed != self.head {
            return Err(WitnessError::Broken {
                recomputed,
                declared: self.head,
            });
        }
        Ok(())
    }

    /// Aggiunge una voce. Rifiuta tutto ciò che farebbe tornare indietro il
    /// testimone.
    pub fn extend(&mut self, entry: WitnessEntry) -> Result<&WitnessEntry, WitnessError> {
        if entry.rows == 0 {
            return Err(WitnessError::NoRows {
                rows: entry.rows,
                min: 1,
            });
        }
        self.check()?;
        if let Some(latest) = self.latest(&entry.session) {
            if entry.rows <= latest.rows {
                return Err(WitnessError::NotMonotonic {
                    session: entry.session.clone(),
                    have: latest.rows,
                    got: entry.rows,
                });
            }
        }
        let leaf = leaf_of(&entry).map_err(|_| WitnessError::Broken {
            recomputed: None,
            declared: self.head,
        })?;
        self.head = Some(match self.head {
            None => leaf,
            Some(prev) => node(&prev, &leaf),
        });
        self.entries.push(entry);
        // La voce è appena stata spinta: l'ultima esiste per costruzione.
        match self.entries.last() {
            Some(e) => Ok(e),
            None => Err(WitnessError::Broken {
                recomputed: None,
                declared: self.head,
            }),
        }
    }

    /// Registra la testa corrente di una catena. È il gesto che lo studente fa
    /// quando copia fuori il registro: dopo questo, un rollback è visibile.
    pub fn observe(&mut self, chain: &Chain) -> Result<&WitnessEntry, WitnessError> {
        self.extend(WitnessEntry {
            session: chain.session().clone(),
            rows: chain.len(),
            head: chain.head(),
            at: Millis::now(),
        })
    }

    /// Mette la catena presentata a confronto con la copia indipendente.
    ///
    /// Non distingue la riscrittura dalla cancellazione, e non dice di
    /// distinguerle. Distingue ciò che la copia indipendente può distinguere,
    /// che basta per sapere che il registro di cui si fidava non è più quello.
    pub fn confront(&self, chain: &Chain) -> Confrontation {
        let presented = chain.head();
        let latest = match self.latest(chain.session()) {
            Some(e) => e,
            // Nessuna copia indipendente di questa sessione: la catena è
            // coerente ma non ancorata, che è il caso peggiore e va detto.
            None => return Confrontation::Unanchored,
        };
        if latest.head == presented {
            return Confrontation::Agrees {
                entry: latest.clone(),
            };
        }
        // Una testa già vista a un punto più indietro è un rollback: la
        // catena presentata è un prefisso noto, e i numeri di riga lo dicono.
        if let Some(earlier) = self
            .entries
            .iter()
            .find(|e| &e.session == chain.session() && e.head == presented)
        {
            return Confrontation::Rollback {
                presented,
                witnessed: earlier.clone(),
            };
        }
        Confrontation::Diverges {
            presented,
            latest: latest.clone(),
        }
    }
}

/// Che cosa ha da dire il testimone davanti a una catena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confrontation {
    /// Il testimone non conosce questa sessione: la catena è coerente e non
    /// ancorata a nulla di indipendente.
    Unanchored,
    /// La testa presentata è l'ultima che il testimone aveva visto.
    Agrees { entry: WitnessEntry },
    /// La testa presentata era già stata vista, ma a un punto più indietro: il
    /// registro presentato è un rollback.
    Rollback {
        presented: Hash,
        witnessed: WitnessEntry,
    },
    /// La testa presentata non è mai stata vista: il registro è stato
    /// riscritto, o le voci sono state perse. Il testimone non distingue le
    /// due cose, e non dice di distinguerle.
    Diverges {
        presented: Hash,
        latest: WitnessEntry,
    },
}

impl Confrontation {
    /// Coerente anche quando è una contraddizione: la domanda è sempre se la
    /// catena *sta in piedi*, non se dice il vero.
    pub fn is_coherent(&self) -> bool {
        !matches!(
            self,
            Confrontation::Rollback { .. } | Confrontation::Diverges { .. }
        )
    }

    pub fn as_rollback(&self) -> bool {
        matches!(self, Confrontation::Rollback { .. })
    }
}

impl fmt::Display for Confrontation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Confrontation::Unanchored => write!(
                f,
                "testimone muto: nessuna copia indipendente di questa sessione"
            ),
            Confrontation::Agrees { entry } => write!(
                f,
                "il testimone conferma la testa di {} righe, fino a seq {}",
                entry.rows,
                entry.last_seq().map(|s| s.to_string()).unwrap_or("-".into())
            ),
            Confrontation::Rollback { presented, witnessed } => write!(
                f,
                "rollback: la testa presentata {presented} era già stata vista a {} righe (seq {}), dopo la quale il testimone ne ha viste altre",
                witnessed.rows,
                witnessed.last_seq().map(|s| s.to_string()).unwrap_or("-".into())
            ),
            Confrontation::Diverges { presented, latest } => write!(
                f,
                "riscrittura: la testa presentata {presented} non è mai stata vista; l'ultima vista era {} a {} righe",
                latest.head, latest.rows
            ),
        }
    }
}

/// Perché il testimone non è la copia indipendente che uno si aspetterebbe.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WitnessError {
    #[error("una voce di testimone con {rows} righe non attesta niente: ne servono almeno {min}")]
    NoRows { rows: usize, min: usize },

    #[error("testimone non monotono per `{session}`: la voce con {got} righe non può stare dopo quella con {have}")]
    NotMonotonic {
        session: SessionId,
        have: usize,
        got: usize,
    },

    #[error("testimone incoerente: dalle voci si ricostruisce {recomputed:?}, il testimone dichiara {declared:?}")]
    Broken {
        recomputed: Option<Hash>,
        declared: Option<Hash>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{Chain, SegmentPlan};
    use kbs_core::{
        ArgumentId, CohortId, CourseId, Evidence, GraderKind, Millis, Observation, PersonId,
        SeqInSession,
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
                instance: format!("inst-{tag}-{seq}"),
                correct: true,
            },
            judged_by: Some(GraderKind::Deterministic),
            at: Millis(1_700_000_000_000 + seq as i64),
        }
    }

    fn sess(name: &str) -> SessionId {
        SessionId::new(name)
    }

    /// Una catena di `n` righe, della sessione `name`, con le righe marcate da
    /// `tag`: due catene con lo stesso `tag` sono la stessa catena, con `tag`
    /// diverso sono due registri diversi della stessa lunghezza.
    fn chain(name: &str, n: u64, tag: &str) -> Chain {
        let rows: Vec<Observation> = (0..n).map(|i| obs(i, tag)).collect();
        Chain::build(&sess(name), &rows, SegmentPlan::default_plan()).unwrap()
    }

    fn entry(session: &str, rows: usize, head: Hash, at: i64) -> WitnessEntry {
        WitnessEntry {
            session: sess(session),
            rows,
            head,
            at: Millis(at),
        }
    }

    #[test]
    fn il_testimone_vuoto_non_ha_testa_e_verifica() {
        let w = Witness::new();
        assert!(w.is_empty());
        assert_eq!(w.head(), None);
        assert!(w.check().is_ok());
        assert_eq!(w.confront(&chain("s", 3, "a")), Confrontation::Unanchored);
    }

    #[test]
    fn estendendo_la_testa_cresce_e_verifica() {
        let mut w = Witness::new();
        w.extend(entry("s", 3, chain("s", 3, "a").head(), 10)).unwrap();
        assert_eq!(w.len(), 1);
        assert!(w.check().is_ok());
        let h1 = w.head().unwrap();
        w.extend(entry("s", 5, chain("s", 5, "a").head(), 20))
            .unwrap();
        assert_ne!(w.head().unwrap(), h1);
        assert!(w.check().is_ok());
    }

    #[test]
    fn una_voce_senza_righe_e_rifiutata() {
        let mut w = Witness::new();
        let err = w
            .extend(entry("s", 0, chain("s", 1, "a").head(), 10))
            .unwrap_err();
        assert_eq!(err, WitnessError::NoRows { rows: 0, min: 1 });
        assert!(w.is_empty());
    }

    #[test]
    fn una_voce_non_monotona_e_rifiutata() {
        let mut w = Witness::new();
        w.extend(entry("s", 5, chain("s", 5, "a").head(), 10))
            .unwrap();
        // la stessa testa con un numero di righe non crescente: il testimone
        // non torna indietro
        assert_eq!(
            w.extend(entry("s", 5, chain("s", 5, "a").head(), 20))
                .err(),
            Some(WitnessError::NotMonotonic {
                session: sess("s"),
                have: 5,
                got: 5
            })
        );
        assert_eq!(
            w.extend(entry("s", 3, chain("s", 3, "a").head(), 20))
                .err(),
            Some(WitnessError::NotMonotonic {
                session: sess("s"),
                have: 5,
                got: 3
            })
        );
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn la_monotonia_e_per_sessione() {
        let mut w = Witness::new();
        w.extend(entry("a", 5, chain("a", 5, "a").head(), 10))
            .unwrap();
        w.extend(entry("b", 2, chain("b", 2, "a").head(), 11))
            .unwrap();
        w.extend(entry("b", 3, chain("b", 3, "a").head(), 12))
            .unwrap();
        assert!(w.check().is_ok());
        assert_eq!(w.latest(&sess("a")).unwrap().rows, 5);
        assert_eq!(w.latest(&sess("b")).unwrap().rows, 3);
    }

    #[test]
    fn una_voce_manomessa_rompe_la_catena_del_testimone() {
        let mut w = Witness::new();
        w.extend(entry("s", 3, chain("s", 3, "a").head(), 10))
            .unwrap();
        w.extend(entry("s", 5, chain("s", 5, "a").head(), 20))
            .unwrap();
        let head_before = w.head().unwrap();

        w.entries[0].rows = 4;
        assert!(matches!(w.check(), Err(WitnessError::Broken { .. })));
        assert_ne!(w.recompute().unwrap(), head_before);
    }

    #[test]
    fn non_si_estende_un_testimone_rotto() {
        let mut w = Witness::new();
        w.extend(entry("s", 3, chain("s", 3, "a").head(), 10))
            .unwrap();
        w.entries[0].head = chain("s", 9, "a").head();
        assert!(matches!(
            w.extend(entry("s", 5, chain("s", 5, "a").head(), 20)),
            Err(WitnessError::Broken { .. })
        ));
    }

    #[test]
    fn un_testimone_deserializzato_e_verificabile() {
        let mut w = Witness::new();
        w.extend(entry("s", 3, chain("s", 3, "a").head(), 10))
            .unwrap();
        let text = serde_json::to_string(&w).unwrap();
        let back: Witness = serde_json::from_str(&text).unwrap();
        assert!(back.check().is_ok());
        assert_eq!(back, w);

        let mut rotta: Witness = serde_json::from_str(&text).unwrap();
        rotta.entries[0].rows = 99;
        assert!(matches!(rotta.check(), Err(WitnessError::Broken { .. })));
    }

    #[test]
    fn il_testimone_conferma_la_testa_vista() {
        let mut w = Witness::new();
        let c = chain("s", 4, "a");
        w.extend(entry("s", 4, c.head(), 10)).unwrap();
        let got = w.confront(&c);
        assert!(matches!(got, Confrontation::Agrees { .. }));
        assert!(got.is_coherent());
        assert!(got.to_string().contains("conferma"));
    }

    #[test]
    fn il_testimone_rileva_un_rollback() {
        let mut w = Witness::new();
        w.extend(entry("s", 3, chain("s", 3, "a").head(), 10))
            .unwrap();
        w.extend(entry("s", 7, chain("s", 7, "a").head(), 20))
            .unwrap();
        // il server è stato ripristinato da un backup di tre righe
        let vecchia = chain("s", 3, "a");
        let got = w.confront(&vecchia);
        match &got {
            Confrontation::Rollback {
                presented,
                witnessed,
            } => {
                assert_eq!(*presented, vecchia.head());
                assert_eq!(witnessed.rows, 3);
            }
            other => panic!("atteso un rollback, ottenuto {other:?}"),
        }
        assert!(got.as_rollback());
        assert!(!got.is_coherent());
        assert!(got.to_string().contains("rollback"));
    }

    #[test]
    fn il_testimone_rileva_una_riscrittura() {
        let mut w = Witness::new();
        let onesta = chain("s", 3, "a");
        w.extend(entry("s", 3, onesta.head(), 10)).unwrap();
        // tre righe, tre seq, tre giudici: un registro riscritto da capo che
        // non ha niente a che fare con quello visto
        let riscritta = chain("s", 3, "b");
        assert_eq!(riscritta.len(), onesta.len());
        let got = w.confront(&riscritta);
        assert_eq!(
            got,
            Confrontation::Diverges {
                presented: riscritta.head(),
                latest: entry("s", 3, onesta.head(), 10)
            }
        );
        assert!(!got.is_coherent());
        assert!(got.to_string().contains("riscrittura"));
    }

    #[test]
    fn una_sessione_non_vista_e_non_ancorata() {
        let mut w = Witness::new();
        w.extend(entry("a", 3, chain("a", 3, "a").head(), 10))
            .unwrap();
        assert_eq!(
            w.confront(&chain("b", 3, "a")),
            Confrontation::Unanchored
        );
        assert!(w.confront(&chain("b", 3, "a")).to_string().contains("muto"));
    }

    #[test]
    fn observe_registra_la_testa_corrente() {
        let mut w = Witness::new();
        let c = chain("s", 2, "a");
        w.observe(&c).unwrap();
        assert_eq!(w.latest(&sess("s")).unwrap().head, c.head());
        // osservare un registro più corto è un rollback, e il testimone lo rifiuta
        assert!(matches!(
            w.observe(&chain("s", 1, "a")),
            Err(WitnessError::NotMonotonic { .. })
        ));
    }
}
