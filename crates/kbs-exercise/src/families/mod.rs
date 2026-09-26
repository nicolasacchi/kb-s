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

pub mod classify;
pub mod count;
pub mod mean;
pub mod poly;
pub mod setdiff;

use crate::generator::Generator;

/// Tutte le famiglie, in un ordine dichiarato.
///
/// L'ordine è stabile e fa parte del contratto del crate: `tests/sweep.rs`
/// numera le famiglie con l'indice e i suoi fallimenti hanno un nome.
pub fn all() -> Vec<Box<dyn Generator>> {
    vec![
        Box::new(poly::PolyExpand),
        Box::new(mean::WeightedMean),
        Box::new(count::CountCoprimes),
        Box::new(classify::ClassifyReduced),
        Box::new(setdiff::SetDifference),
    ]
}

/// La famiglia con questo nome, se esiste.
pub fn by_name(name: &str) -> Option<Box<dyn Generator>> {
    all().into_iter().find(|g| g.family() == name)
}
