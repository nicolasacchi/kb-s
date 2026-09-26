//! L'identificatore della sessione di registro.
//!
//! Una **sessione** è il tratto di registro che una scrittura append-only
//! chiude con una testa. `kbs-core` non ha questo tipo — non ce l'ha bisogno,
//! perché per `kbs-core` una `Observation` è una riga e basta — quindi è
//! definito qui, in `kbs-verify`, che è il crate che chiude e verifica le
//! sessioni.

use std::fmt;
use std::str::FromStr;

/// La sessione di registro a cui appartiene una testa.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        SessionId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Una sessione vuota non ha nome: è un errore di chi la costruisce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("identificatore di sessione vuoto: una sessione di registro si nomina")]
pub struct EmptySessionId;

impl FromStr for SessionId {
    type Err = EmptySessionId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(EmptySessionId);
        }
        Ok(SessionId(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_vuoto_e_rifiutato() {
        assert_eq!(SessionId::from_str(""), Err(EmptySessionId));
        assert!(SessionId::from_str("corso::a").is_ok());
    }

    #[test]
    fn display_e_as_str_coincidono() {
        let s = SessionId::new("2026::informatica::1B");
        assert_eq!(s.to_string(), s.as_str());
    }
}
