//! La catena di grading come **dato**, non come flusso (D3, D8).
//!
//! L'ordine `deterministico → pari → umano` è un array, [`GRADING_ORDER`], e la
//! catena di un esercizio è una struttura che lo applica. Non c'è un `if` che
//! «se il deterministico non basta, passa al pari»: c'è uno stato, e lo stato
//! dice che cosa è lecito provare adesso.
//!
//! # Perché come dato
//!
//! L'ordine scritto in un `if` è un ordine che il prossimo ramo dimentica. Un
//! array è un ordine che si può leggere, testare, stampare in un registro, e
//! confrontare con la catena di hash di D6: se domani la griglia cambia, cambia
//! un array che qualcuno ha visto, non un `else` che qualcuno ha scritto.
//!
//! # Un modello non può essere l'anello mancante
//!
//! `kbs_core::GraderKind` non ha una variante modello, e questo crate non ha
//! nessun tipo che ne produca una: l'unico modo di ottenere un `GraderKind` qui è
//! scrivere una delle quattro varianti. La proprietà è per costruzione, ma è
//! anche per compilazione: [`rank`] fa un `match` **esauritivo** su
//! `GraderKind`, quindi un domani aggiungesse una quinta variante il crate non
//! compilerebbe finché qualcuno non avesse dichiarato dove metterla. È il
//! controllo più forte che si possa avere su un'assenza, ed è per questo che la
//! funzione non usa un `_ =>` di comodo.

use kbs_core::GraderKind;

use crate::error::ExerciseError;

/// La griglia del grading, nell'ordine in cui si percorre.
///
/// `Teacher` non c'è: D3 dice che il giudizio del docente vale quanto quello di
/// una persona e registra chi lo ha fatto, ma non è un anello della catena
/// automatica — è una voce nel registro delle valutazioni. `rank` lo dichiara
/// fuori catena invece di accettarlo in silenzio.
pub const GRADING_ORDER: [GraderKind; 3] = [
    GraderKind::Deterministic,
    GraderKind::Peer,
    GraderKind::Human,
];

/// La posizione dell'anello nella catena, o `None` se non è un anello.
pub fn rank(kind: GraderKind) -> Option<usize> {
    match kind {
        GraderKind::Deterministic => Some(0),
        GraderKind::Peer => Some(1),
        GraderKind::Human => Some(2),
        GraderKind::Teacher => None,
    }
}

/// Che cosa è successo a un tentativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Il giudizio è definitivo. La catena si chiude: riaprirla significherebbe
    /// che la risposta «giusta» dipende da chi l'ha guardata per ultimo.
    Conclusive(bool),
    /// Il tentativo non ha prodotto un giudizio. Solo il deterministico può
    /// essere inconcludente, e solo perché non c'è: un esercizio senza checker
    /// deterministico non entra nel percorso di pubblicazione (D8), resta
    /// speculativo, e la catena di grading è l'unico posto dove questo si vede.
    Inconclusive,
}

/// La catena di un esercizio.
///
/// Non contiene voti: contiene **quali anelli sono stati attraversati e da quale
/// si è chiusa**. I voti stanno in `kbs_core::Grading`, che porta `seq` e
/// `rubric_version` (D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradingChain {
    crossed: Vec<GraderKind>,
    closed_by: Option<GraderKind>,
}

impl Default for GradingChain {
    fn default() -> Self {
        Self::new()
    }
}

impl GradingChain {
    pub fn new() -> Self {
        GradingChain {
            crossed: Vec::new(),
            closed_by: None,
        }
    }

    /// Attraversa un anello.
    ///
    /// Rifiuta un anello fuori dalla griglia, un anello che scavalca uno ancora
    /// non provato, e un anello dopo un giudizio definitivo.
    pub fn cross(&mut self, kind: GraderKind, outcome: Outcome) -> Result<(), ExerciseError> {
        let pos = rank(kind).ok_or(ExerciseError::KindNotInChain { kind })?;
        if let Some(closed_by) = self.closed_by {
            return Err(ExerciseError::GradingChainClosed { closed_by });
        }
        if let Some(missing) = GRADING_ORDER
            .iter()
            .take(pos)
            .find(|k| !self.crossed.contains(k))
            .copied()
        {
            return Err(ExerciseError::GradingOrder {
                attempted: kind,
                missing,
            });
        }
        self.crossed.push(kind);
        if matches!(outcome, Outcome::Conclusive(_)) {
            self.closed_by = Some(kind);
        }
        Ok(())
    }

    /// True se un giudizio definitivo è già stato emesso.
    pub fn is_closed(&self) -> bool {
        self.closed_by.is_some()
    }

    /// L'anello da cui la catena si è chiusa.
    pub fn closed_by(&self) -> Option<GraderKind> {
        self.closed_by
    }

    /// Gli anelli attraversati, nell'ordine in cui sono stati attraversati.
    pub fn crossed(&self) -> &[GraderKind] {
        &self.crossed
    }

    /// Il prossimo anello lecito, se la catena non è chiusa.
    pub fn next(&self) -> Option<GraderKind> {
        if self.is_closed() {
            return None;
        }
        GRADING_ORDER
            .iter()
            .copied()
            .find(|k| !self.crossed.contains(k))
    }
}

/// La griglia è un dato, e i dati si testano come dati.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_griglia_e_il_ordine_di_d3() {
        assert_eq!(
            GRADING_ORDER,
            [
                GraderKind::Deterministic,
                GraderKind::Peer,
                GraderKind::Human
            ]
        );
        assert_eq!(rank(GraderKind::Deterministic), Some(0));
        assert_eq!(rank(GraderKind::Peer), Some(1));
        assert_eq!(rank(GraderKind::Human), Some(2));
        assert!(GRADING_ORDER.windows(2).all(|w| rank(w[0]) < rank(w[1])));
    }

    /// Nessuna variante di `GraderKind` è un modello, e nessuna delle quattro
    /// è fuori dalla griglia senza che lo si dichiari: `Teacher` è l'unica, ed è
    /// un giudizio del docente, non un anello (D3).
    #[test]
    fn tutte_le_varianti_es_rono_o_nella_griglia_o_dichiarate() {
        for kind in [
            GraderKind::Deterministic,
            GraderKind::Peer,
            GraderKind::Human,
            GraderKind::Teacher,
        ] {
            if !GRADING_ORDER.contains(&kind) {
                assert_eq!(
                    rank(kind),
                    None,
                    "{kind:?} fuori griglia deve essere dichiarato"
                );
                let mut c = GradingChain::new();
                assert_eq!(
                    c.cross(kind, Outcome::Conclusive(true)),
                    Err(ExerciseError::KindNotInChain { kind })
                );
            }
        }
    }

    #[test]
    fn il_pari_senza_il_deterministico_non_si_crea() {
        let mut c = GradingChain::new();
        assert_eq!(
            c.cross(GraderKind::Peer, Outcome::Conclusive(true)),
            Err(ExerciseError::GradingOrder {
                attempted: GraderKind::Peer,
                missing: GraderKind::Deterministic
            })
        );
        assert_eq!(
            c.cross(GraderKind::Human, Outcome::Conclusive(true)),
            Err(ExerciseError::GradingOrder {
                attempted: GraderKind::Human,
                missing: GraderKind::Deterministic
            })
        );
        assert!(
            c.crossed().is_empty(),
            "un tentativo rifiutato non lascia traccia"
        );
    }

    #[test]
    fn la_catena_si_percorre_nell_ordine() {
        let mut c = GradingChain::new();
        assert_eq!(c.next(), Some(GraderKind::Deterministic));
        assert!(c
            .cross(GraderKind::Deterministic, Outcome::Inconclusive)
            .is_ok());
        assert_eq!(c.next(), Some(GraderKind::Peer));
        // il pari non può saltare l'umano
        assert!(c
            .cross(GraderKind::Human, Outcome::Conclusive(true))
            .is_err());
        assert!(c.cross(GraderKind::Peer, Outcome::Inconclusive).is_ok());
        assert_eq!(c.next(), Some(GraderKind::Human));
        assert!(c
            .cross(GraderKind::Human, Outcome::Conclusive(false))
            .is_ok());
        assert_eq!(
            c.crossed(),
            [
                GraderKind::Deterministic,
                GraderKind::Peer,
                GraderKind::Human
            ]
        );
    }

    #[test]
    fn un_giudizio_definitivo_chiude_la_catena() {
        let mut c = GradingChain::new();
        assert!(c
            .cross(GraderKind::Deterministic, Outcome::Conclusive(false))
            .is_ok());
        assert!(c.is_closed());
        assert_eq!(c.closed_by(), Some(GraderKind::Deterministic));
        assert_eq!(c.next(), None);
        assert_eq!(
            c.cross(GraderKind::Peer, Outcome::Conclusive(true)),
            Err(ExerciseError::GradingChainClosed {
                closed_by: GraderKind::Deterministic
            })
        );
        assert_eq!(
            c.cross(GraderKind::Deterministic, Outcome::Conclusive(true)),
            Err(ExerciseError::GradingChainClosed {
                closed_by: GraderKind::Deterministic
            })
        );
    }
}
