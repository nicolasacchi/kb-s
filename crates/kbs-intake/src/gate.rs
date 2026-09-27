//! Il gate di D4: l'unica strada che porta un argomento in `in-corso`.
//!
//! Due percorsi di scrittura, e sono diversi nel modo in cui vengono usati
//! (D4). Quello che questo modulo fa è **non reimplementare** nessuna delle due
//! regole: chi decide se una ratifica vale è `kbs_store::publish`, e chi decide
//! che cosa si ratifica è `kbs_store::ratify`. Qui si aggiunge una sola cosa,
//! che il negozio non può sapere: **il verdetto**.
//!
//! # Perché il verdetto è parte del gate
//!
//! `kbs-store` non ha il documento: ha l'hash del documento. Può quindi
//! rispondere «la ratifica vale per questi byte» e non può rispondere «questa
//! pagina chiama una CDN» (D15) né «questo contratto è troncato» (D7). Se la
//! promozione non guardasse il verdetto, un artifact che referenzia una CDN
//! diventerebbe citabile nel momento in cui il docente lo ratifica, e la
//! ratifica sarebbe diventata la scorciatoia che D15 vieta.
//!
//! # Le tre finestre, e perché sono chiuse
//!
//! 1. **Verdetto bloccante** → [`Error::VerdettoBloccante`]. Un riferimento
//!    fuori dalla scatella non si ratifica.
//! 2. **Contenuto cambiato** → [`Error::ContenutoCambiato`]. Il verdetto vale per
//!    l'hash che copre; se il contenuto è cambiato, il verdetto è di un altro
//!    testo e ratificare sarebbe firmare un testo che il docente non ha
//!    guardato.
//! 3. **Nessuna ratifica, o ratifica superata** → l'errore del negozio,
//!    [`kbs_core::Invariant::CitableWithoutRatification`] o
//!    [`kbs_core::Invariant::StaleRatification`], che arrivano intatti.
//!
//! Il test che le copre è `tests/il_gate.rs`, e la terza finestra è
//! esercitata su dati reali: ratifica, si cambia il file, si ripromuove.

use kbs_core::{Argument, ArgumentId, PersonId, PublicationState, Ratification};
use kbs_store::Store;

use crate::error::{Error, Result};
use crate::route::{Diagnostic, Verdict};

/// La porta. Un oggetto senza stato: le funzioni sono libere e il nome dice
/// quale porta è.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate;

impl Gate {
    /// Registra la ratifica per l'hash di contenuto **corrente**.
    ///
    /// Non è la porta: è l'atto che la porta consuma. Esposto perché ritirare
    /// una ratifica (`withdraw_ratification`) e firmarne una nuova sono gesti
    /// separati, e non hanno bisogno di passare da qui.
    pub fn ratifica(
        store: &mut Store,
        id: &ArgumentId,
        by: &PersonId,
        nota: &str,
    ) -> Result<Ratification> {
        Ok(store.ratify(id, by, nota)?)
    }

    /// La porta completa: verifica il verdetto, verifica che il contenuto sia
    /// ancora quello, ratifica, e pubblica.
    ///
    /// L'ordine è quello giusto e non è decorativo: prima il verdetto (che non
    /// cambia da solo), poi l'hash (che cambia quando il file cambia), poi la
    /// ratifica (che vale per l'hash di adesso), poi la pubblicazione. Se si
    /// ratificasse prima di verificare l'hash, una ratifica potrebbe essere
    /// registrata per un testo che il verdetto ha già bocciato.
    pub fn ratifica_e_promuovi(
        store: &mut Store,
        id: &ArgumentId,
        by: &PersonId,
        nota: &str,
        verdetto: &Verdict,
    ) -> Result<PublicationState> {
        let argomento = store.read_argument(by, id)?;
        verifica(verdetto, &argomento)?;
        store.ratify(id, by, nota)?;
        Ok(store.publish(id)?)
    }

    /// La porta senza ratifica nuova: usa la ratifica che c'è già, se c'è.
    ///
    /// È il modo in cui si rimette in ordine una ratifica che era stata
    /// superata da un cambiamento del testo **senza** firmarne una nuova — cioè
    /// non si può, ed è il punto: se la ratifica vale per un hash che non è più
    /// quello di oggi, questa funzione restituisce
    /// [`kbs_core::Invariant::StaleRatification`] e l'unica via è
    /// [`Gate::ratifica_e_promuovi`], che è un atto di nuovo.
    pub fn promuovi(
        store: &mut Store,
        id: &ArgumentId,
        by: &PersonId,
        verdetto: &Verdict,
    ) -> Result<PublicationState> {
        let argomento = store.read_argument(by, id)?;
        verifica(verdetto, &argomento)?;
        Ok(store.publish(id)?)
    }
}

/// Le due verifiche che il negozio non può fare.
fn verifica(verdetto: &Verdict, argomento: &Argument) -> Result<()> {
    if let Some(bloccante) = verdetto.first_blocking() {
        return Err(Error::VerdettoBloccante {
            codice: bloccante.code.clone(),
            messaggio: bloccante.message.clone(),
        });
    }
    match verdetto.content_hash.as_deref() {
        Some(del_verdetto) if del_verdetto != argomento.content_hash => Err(Error::ContenutoCambiato {
            del_verdetto: del_verdetto.to_string(),
            corrente: argomento.content_hash.clone(),
        }),
        _ => Ok(()),
    }
}

/// Il verdetto di un intake è legato all'hash del testo che ha giudicato:
/// `receive` lo imposta, e senza di esso la porta controllerebbe soltanto che
/// il documento era valido *quando qualcuno l'ha guardato*, che è una proprietà
/// più debole e sembra la stessa.
///
/// Questa funzione esiste per chi costruisce un verdetto a mano — la CLI che
/// rivalida un file, l'MCP che ne riceve uno dal docente — e serve a non
/// dimenticare di legarlo.
pub fn bind(verdetto: &mut Verdict, content_hash: &str) {
    verdetto.content_hash = Some(content_hash.to_string());
}

/// L'hash a cui il verdetto è legato, se c'è.
pub fn content_hash_of(verdetto: &Verdict) -> Option<&str> {
    verdetto.content_hash.as_deref()
}

/// Rivalida un documento e ne ricava il verdetto, legato al suo hash.
///
/// È la via che la CLI usa quando promuove: il file è la fonte, e rivalidarlo è
/// più onesto che fidarsi di un verdetto vecchio. Il percorso di intake che ha
/// prodotto la ricevuta può non essere più un file (una cattura), e in quel caso
/// si usa il verdetto della ricevuta, che è comunque legato a un hash.
pub fn rivedi(sorgente: &str) -> Verdict {
    let report = kbs_doc::validate(sorgente);
    let mut verdetto = Verdict::default();
    for issue in &report.issues {
        verdetto.push(Diagnostic {
            code: crate::route::code_of_doc_issue(&issue.code).to_string(),
            message: issue.message.clone(),
            blocking: issue.severity == kbs_doc::Severity::Blocking,
        });
    }
    bind(&mut verdetto, &crate::route::content_hash(sorgente));
    verdetto
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_verdetto_porta_l_hash_a_cui_si_riferisce() {
        let mut v = Verdict::default();
        assert_eq!(content_hash_of(&v), None);
        bind(&mut v, "sha256:aa");
        assert_eq!(content_hash_of(&v), Some("sha256:aa"));
        bind(&mut v, "sha256:bb");
        assert_eq!(content_hash_of(&v), Some("sha256:bb"), "l'ultimo hash lega il verdetto");
        assert!(v.can_publish(), "il vincolo non è un problema: la porta lo legge");
    }

    #[test]
    fn rivedi_trova_un_riferimento_esterno() {
        let v = rivedi(
            "<!doctype html><html><head><title>T</title></head><body><h1 id=\"t\">T</h1>\
             <script src=\"https://cdn.example/three.module.js\"></script></body></html>",
        );
        assert!(v.has_code("external-reference"));
        assert!(!v.can_publish());
        assert!(content_hash_of(&v).unwrap().starts_with("sha256:"));
    }
}
