//! I tre limiti dichiarati, e il tipo [`Verified`] che non lascia scordarli.
//!
//! D6 chiede che i tre limiti siano **nel codice**, non nella documentazione,
//! perché sono le cose che un revisore cerca per primo. Un tipo che contiene
//! il verdetto e i limiti insieme rende impossibile la dimenticanza: non
//! esiste un modo di ottenere il verdetto da solo, e [`Verified`] stampa i
//! limiti ogni volta che viene stampato il verdetto.
//!
//! I tre limiti sono dichiarati come un array di lunghezza tre: toglierne uno
//! non è una modifica, è un errore di compilazione.

use std::fmt;

/// Quale dei tre limiti.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LimitId {
    /// 1. La riscrittura integrale è indistinguibile senza una copia indipendente.
    Rewrite,
    /// 2. Il rollback da backup è indistinguibile da una riscrittura.
    Rollback,
    /// 3. L'hash garantisce integrità, non verità.
    Truth,
}

impl LimitId {
    /// L'identificatore usato nell'export a colonne fisse.
    pub fn as_str(self) -> &'static str {
        match self {
            LimitId::Rewrite => "rewrite",
            LimitId::Rollback => "rollback",
            LimitId::Truth => "truth",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "rewrite" => Some(LimitId::Rewrite),
            "rollback" => Some(LimitId::Rollback),
            "truth" => Some(LimitId::Truth),
            _ => None,
        }
    }
}

impl fmt::Display for LimitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Un limite: cosa non è garantito, e che cosa ci si mette contro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Limit {
    pub id: LimitId,
    /// Che cosa **non** è garantito. Frase negativa, perché è una frase
    /// negativa: nessun verdetto di questo crate promette più di quanto è
    /// scritto qui.
    pub statement: &'static str,
    /// Che cosa il sistema offre **contro** il limite. `None` quando
    /// l'onestà richiede di ammettere che non c'è rimedio.
    pub mitigation: Option<&'static str>,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.id, self.statement)?;
        if let Some(m) = self.mitigation {
            write!(f, "\n  contro: {m}")?;
        }
        Ok(())
    }
}

/// I tre limiti, tutti e tre, sempre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Limits {
    items: [Limit; 3],
}

impl Limits {
    /// L'unico modo di ottenere i limiti: sono tre e sono quelli.
    pub const ALL: Limits = Limits {
        items: [
            Limit {
                id: LimitId::Rewrite,
                statement: "chi riscrive l'intera catena da capo produce una catena coerente, indistinguibile senza una copia indipendente delle righe",
                mitigation: Some("il testimone: la copia indipendente della testa, conservata fuori dal sistema da chi non ci ha accesso"),
            },
            Limit {
                id: LimitId::Rollback,
                statement: "un rollback da un backup è indistinguibile da una riscrittura senza un ancoraggio esterno e monotono",
                mitigation: Some("il testimone cresce solo per append e rifiuta una voce con un numero di righe non crescente"),
            },
            Limit {
                id: LimitId::Truth,
                statement: "l'hash garantisce integrità, non verità: uno span che non sostiene la claim è una riga impeccabilmente conforme",
                mitigation: None,
            },
        ],
    };

    /// I limiti in ordine di dichiarazione.
    pub fn all(self) -> [Limit; 3] {
        self.items
    }

    /// Quanti limiti. Sono tre; il metodo esiste per l'asserzione dei test e
    /// per un chiamante che deve contarli.
    pub fn len(self) -> usize {
        self.items.len()
    }

    pub fn is_empty(self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(self) -> impl Iterator<Item = Limit> {
        self.items.into_iter()
    }

    /// Il limite con quell'id. `LimitId` ha tre varianti e i limiti sono
    /// tre: la corrispondenza è totale, e il Compiler la vede tale.
    pub fn get(self, id: LimitId) -> Limit {
        let at = match id {
            LimitId::Rewrite => 0,
            LimitId::Rollback => 1,
            LimitId::Truth => 2,
        };
        self.items[at]
    }
}

impl fmt::Display for Limits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, l) in self.items.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{l}")?;
        }
        Ok(())
    }
}

/// Un verdetto **con** i tre limiti.
///
/// Non esiste un modo di prendere il verdetto da solo: `into_parts` restituisce
/// entrambi, e li chiama entrambi. `Display` stampa il verdetto e poi i
/// limiti, quindi anche un `println!("{verdetto}")` mostra che cosa non è
/// garantito.
#[must_use = "un verdetto senza i tre limiti dichiarati non viene stampato: usa Display, verdict() o into_parts()"]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Verified<T> {
    verdict: T,
    limits: Limits,
}

impl<T> Verified<T> {
    pub fn new(verdict: T) -> Self {
        Verified {
            verdict,
            limits: Limits::ALL,
        }
    }

    /// Il verdetto. I limiti sono a portata di mano con [`Self::limits`].
    pub fn verdict(&self) -> &T {
        &self.verdict
    }

    /// I tre limiti. Restituirli è l'unico modo di portarseli via.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Verdetto e limiti, per entrambi. Non esiste `into_verdict`.
    pub fn into_parts(self) -> (T, Limits) {
        (self.verdict, self.limits)
    }

    /// Applica una funzione al verdetto tenendo i limiti.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Verified<U> {
        Verified {
            verdict: f(self.verdict),
            limits: self.limits,
        }
    }
}

impl<T: fmt::Display> fmt::Display for Verified<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\n\nlimiti dichiarati:\n{}", self.verdict, self.limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i_limiti_sono_tre_e_nella_dichiarazione() {
        let ids: Vec<LimitId> = Limits::ALL.iter().map(|l| l.id).collect();
        assert_eq!(
            ids,
            vec![LimitId::Rewrite, LimitId::Rollback, LimitId::Truth]
        );
        assert_eq!(Limits::ALL.len(), 3);
    }

    #[test]
    fn ogni_limite_dichiara_il_suo_tema() {
        let rewrite = Limits::ALL.get(LimitId::Rewrite);
        assert!(rewrite.statement.contains("riscrive"));
        assert!(rewrite.mitigation.is_some());
        let rollback = Limits::ALL.get(LimitId::Rollback);
        assert!(rollback.statement.contains("rollback"));
        assert!(rollback.mitigation.is_some());
        let truth = Limits::ALL.get(LimitId::Truth);
        assert!(truth.statement.contains("verità"));
        assert!(truth.mitigation.is_none(), "il terzo limite non ha rimedio: va detto");
    }

    #[test]
    fn nessun_enunciato_promette_piu_di_quanto_verifica() {
        for l in Limits::ALL.iter() {
            assert!(!l.statement.is_empty());
            assert!(!l.statement.contains("valid"), "l'enunciato promette: {}", l.statement);
        }
    }

    #[test]
    fn display_di_verified_stampa_anche_i_limiti() {
        let v = Verified::new("coerente");
        let s = v.to_string();
        assert!(s.contains("coerente"));
        assert!(s.contains("limiti dichiarati"));
        assert!(s.contains("integrità, non verità"));
    }

    #[test]
    fn verdetto_e_limiti_escono_insieme() {
        let v = Verified::new(7u8);
        assert_eq!(v.verdict(), &7);
        let (verdict, limits) = v.into_parts();
        assert_eq!(verdict, 7);
        assert_eq!(limits, Limits::ALL);
    }

    #[test]
    fn map_conserva_i_limiti() {
        let v = Verified::new(2u8).map(|x| x * 21);
        assert_eq!(*v.verdict(), 42);
        assert_eq!(v.limits(), Limits::ALL);
    }
}
