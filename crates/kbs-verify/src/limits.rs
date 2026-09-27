//! I tre limiti dichiarati, e il tipo [`Verified`] che li porta con sé.
//!
//! D6 chiede che i tre limiti siano **nel codice**, non nella documentazione,
//! perché sono le cose che un revisore cerca per primo. Un tipo che contiene
//! il verdetto e i limiti insieme fa due cose, e solo due, ed è bene
//! dirle con precisione invece di esagerarle:
//!
//! 1. **ogni verdetto prodotto da questo crate torna dentro [`Verified`]**, e
//!    quindi stamparlo stampa anche i tre limiti — un `println!("{v}")` non
//!    può mostrare un verdetto senza dire che cosa non è garantito;
//! 2. **non esiste una funzione pubblica di questo crate che restituisca un
//!    verdetto nudo**: il tipo restituito lo dichiara, e se qualcuno lo
//!    spoglia la firma non compila più.
//!
//! Ciò che invece [`Verified`] **non** fa è vietare al chiamante di prendere
//! il verdetto da solo: [`Verified::verdict`], [`Verified::into_parts`] e
//! [`Verified::map`] ci arrivano, e ci arrivano di proposito — un auditor ha
//! il diritto di leggere il verdetto, non solo di guardarlo stampare. Quel
//! che segue il verdetto nudo è una scelta del chiamante, e il tipo non ha
//! la pretesa di averla impedita.
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
/// Il verdetto e i limiti stanno nello stesso valore, e [`fmt::Display`] li
/// stampa entrambi. **Esiste** un modo di prendere il verdetto da solo —
/// [`Self::verdict`] per riferimento, [`Self::into_parts`] per valore, e
/// [`Self::map`] che restituisce un `Verified` dal quale [`Self::verdict`]
/// ridà il verdetto nudo — ed è dichiarato qui perché il commento non
/// prometta più di quanto il tipo faccia: non è vietato, è nominato.
///
/// Ciò che il tipo rende impossibile è un'altra cosa, ed è la cosa che
/// conta: **nessuna funzione pubblica di questo crate restituisce un
/// verdetto nudo**, quindi il percorso che porta a un verdetto passa da qui e
/// porta con sé i limiti. Se [`crate::verify`] o [`crate::replay`] restituissero
/// il tipo grezzo, il crate non compilerebbe più.
#[must_use = "un verdetto ignorato non è un verdetto: `Verified` porta con sé i tre limiti, e `Display` li stampa. `verdict()` e `into_parts()` esistono e restituiscono il verdetto da solo — se li usi, i limiti sono tuoi, leggili con `limits()`"]
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

    /// Il verdetto. **È un modo di prendere il verdetto da solo**, dichiarato
    /// come tale: i limiti restano in `self` e vanno presi con
    /// [`Self::limits`]. Chi stampa il verdetto nudo stampa la parte facile.
    pub fn verdict(&self) -> &T {
        &self.verdict
    }

    /// I tre limiti. Restituirli è l'unico modo di portarseli via.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Verdetto e limiti, per entrambi. **Non esiste `into_verdict`**, ma la
    /// coppia qui è già un verdetto nudo a portata di mano: chi scrive
    /// `let (v, _) = …` sceglie di lasciare i limiti indietro, e lo sceglie
    /// in una riga che il compilatore non può vietare. È il motivo per cui il
    /// modulo dichiara il limite come *portato con sé*, non come *impossibile
    /// da lasciare* — e per cui l'unica garanzia che questo tipo fa è che
    /// nessuna funzione pubblica del crate lo restituisca nudo.
    pub fn into_parts(self) -> (T, Limits) {
        (self.verdict, self.limits)
    }

    /// Applica una funzione al verdetto tenendo i limiti. Anche questo è un
    /// modo per arrivare al verdetto da solo — `map(...).verdict()` — e come
    /// gli altri è dichiarato, non nascosto.
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
