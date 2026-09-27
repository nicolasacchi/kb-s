//! Le famiglie implementate.
//!
//! Cinque famiglie, e la scelta di che cosa sia una famiglia è il punto:
//!
//! | famiglia | ragionamento | checker |
//! |---|---|---|
//! | [`poly::PolyExpand`] | manipolazione simbolica | `Equivalence` |
//! | [`mean::WeightedMean`] | computazione reale | `Numeric` con tolleranza |
//! | [`count::CountCoprimes`] | conteggio per inclusione-esclusione | `Numeric` esatto |
//! | [`classify::ClassifyReduced`] | classificazione con distrattori costruiti | `MultipleChoice` |
//! | [`setdiff::SetDifference`] | operazione su insiemi | `Set` |
//!
//! Non sono cinque riscritture dello stesso modello con numeri diversi: cambiano
//! il **tipo di risposta** (un polinomio, una decimale, un intero contato, un
//! testo fra quattro, un elenco), il **metodo** (sviluppo, divisione,
//! inclusione-esclusione, MCD, differenza) e il **checker** che li confronta.
//! Una famiglia che non produce due istanze con lo stesso ragionamento e
//! parametri diversi non è una famiglia, e non è qui.
//!
//! Nessuna famiglia è «la stessa cosa con numeri diversi»: le quattro forme di
//! risposta sono quattro forme di errore diverse, ed è quello che il vicino di
//! banco non può copiare.
//!
//! # Il catalogo è chiuso, e lo è per D11
//!
//! Le famiglie sono un enum, [`Family`], e non una lista di costruttori: le due
//! `match` di [`Family::generator`] e [`Family::instance_generator`] non hanno
//! braccio `_`, quindi **aggiungere una famiglia senza implementare
//! `kbs_verify::InstanceGenerator` non compila**. È la stessa forma che
//! `kbs-fixtures` usa per chiudere l'elenco dei giudicanti, e nasce dalla stessa
//! ragione: un elenco che accetta silenziosamente una voce nuova è un elenco
//! che non dice più niente.

pub mod classify;
pub mod count;
pub mod mean;
pub mod poly;
pub mod setdiff;

use kbs_verify::InstanceGenerator;

use crate::generator::Generator;

/// Il catalogo chiuso delle famiglie.
///
/// È un enum e non una lista per **un** motivo, ed il motivo è D11:
/// [`Self::instance_generator`] e [`Self::generator`] sono `match` **senza
/// braccio `_`**, quindi se si aggiunge una variante e non si scrive il suo
/// `match` il crate non compila. Una famiglia che non implementa
/// [`InstanceGenerator`] è una famiglia che nessuno può riprodurre, e il buco
/// sarebbe esattamente quello che D11 dichiara di non avere: un registro che
/// non sa rifare quello che ha scritto.
///
/// Il confronto con `kbs-fixtures::tests::copertura` è voluto ed è lo stesso:
/// lì il compilatore chiude l'elenco dei giudicanti, qui chiude quello delle
/// famiglie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    PolyExpand,
    WeightedMean,
    CountCoprimes,
    ClassifyReduced,
    SetDifference,
}

impl Family {
    /// Le famiglie, nell'ordine dichiarato.
    ///
    /// L'ordine è stabile e fa parte del contratto del crate: `tests/sweep.rs`
    /// numera le famiglie con l'indice e i suoi fallimenti hanno un nome. È
    /// l'unica cosa qui che il compilatore **non** deduce — un enum non si
    /// enumera da solo — ed è dichiarata perché non si confonda con le due
    /// `match` chiuse, che invece non possono sbagliare.
    pub const ALL: [Family; 5] = [
        Family::PolyExpand,
        Family::WeightedMean,
        Family::CountCoprimes,
        Family::ClassifyReduced,
        Family::SetDifference,
    ];

    /// Il nome della famiglia, quale lo dichiara il suo generatore.
    ///
    /// La stringa non è scritta qui: viene dalla famiglia, quindi il
    /// catalogo e il generatore non possono dire due nomi diversi.
    pub fn name(self) -> &'static str {
        match self {
            Family::PolyExpand => poly::FAMILY,
            Family::WeightedMean => mean::FAMILY,
            Family::CountCoprimes => count::FAMILY,
            Family::ClassifyReduced => classify::FAMILY,
            Family::SetDifference => setdiff::FAMILY,
        }
    }

    /// Il generatore della famiglia: la messa in forma dell'esercizio.
    pub fn generator(self) -> Box<dyn Generator> {
        match self {
            Family::PolyExpand => Box::new(poly::PolyExpand),
            Family::WeightedMean => Box::new(mean::WeightedMean),
            Family::CountCoprimes => Box::new(count::CountCoprimes),
            Family::ClassifyReduced => Box::new(classify::ClassifyReduced),
            Family::SetDifference => Box::new(setdiff::SetDifference),
        }
    }

    /// Il generatore di replay della stessa famiglia (D11).
    ///
    /// Il `match` è chiuso e il valore di ritorno è
    /// `Box<dyn InstanceGenerator>`: il braccio di una famiglia che non
    /// implementa il trait **non compila**, che è il controllo che questo
    /// crate deve a D11.
    pub fn instance_generator(self) -> Box<dyn InstanceGenerator> {
        match self {
            Family::PolyExpand => Box::new(poly::PolyExpand),
            Family::WeightedMean => Box::new(mean::WeightedMean),
            Family::CountCoprimes => Box::new(count::CountCoprimes),
            Family::ClassifyReduced => Box::new(classify::ClassifyReduced),
            Family::SetDifference => Box::new(setdiff::SetDifference),
        }
    }

    /// La famiglia con questo nome, se è nel catalogo.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// Tutte le famiglie, in un ordine dichiarato: il catalogo [`Family`] per
/// [`Family::generator`], nell'ordine che `Family::ALL` dichiara.
pub fn all() -> Vec<Box<dyn Generator>> {
    Family::ALL.into_iter().map(Family::generator).collect()
}

/// La famiglia con questo nome, se esiste.
pub fn by_name(name: &str) -> Option<Box<dyn Generator>> {
    Family::from_name(name).map(Family::generator)
}
