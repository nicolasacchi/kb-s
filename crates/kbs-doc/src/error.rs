//! Gli errori della build: quando qualcosa non si può fare, e perché.

use serde::{Deserialize, Serialize};

/// Un errore che nomina **la regola** rotta, non il sintomo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum DocError {
    #[error("la build si rifiuta di produrre un artifact non pubblicabile: {motivo}")]
    Refused { motivo: String },

    #[error("riferimento locale non risolvibile `{path}`: {motivo}")]
    Unresolvable { path: String, motivo: String },

    #[error("`{path}` esce dalla radice del progetto: la build non legge fuori da dove le è stato detto di leggere")]
    PathEscape { path: String },

    #[error("il runtime vendorizzato `{path}` non è nel repository: {motivo}")]
    VendorMissing { path: String, motivo: String },
}
