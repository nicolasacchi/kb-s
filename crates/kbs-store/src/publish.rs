//! La porta sul percorso condiviso (D4).
//!
//! D4 è la regola che questo file rende vera:
//!
//! > **Un item non verificato è leggibile, ma non è citabile.**
//!
//! Due percorsi di scrittura, e sono diversi nel modo in cui vengono usati:
//!
//! * il **percorso di pubblicazione** è questo: il docente (con l'aiuto di un
//!   LMM esterno) prepara, verifica, **ratifica**, e solo allora l'argomento
//!   entra nell'indice condiviso e diventa citabile;
//! * il **percorso speculativo** è per-studente, non ha gate, non entra
//!   nell'indice condiviso e non è citabile.
//!
//! Se i due percorsi scrivessero nello stesso modo il gate sarebbe decorazione,
//! e un gate decorativo è peggio di nessun gate perché dà a tutti l'illusione
//! che il controllo esista. Per questo [`Store::upsert_argument`] rifiuta di
//! scrivere `in-corso` e l'unica porta è [`Store::publish`].
//!
//! # I tre limiti di D6, dichiarati qui perché sono i primi che un revisore cerca
//!
//! 1. Chi riscrive l'intera catena da capo produce una catena coerente e
//!    indistinguibile senza una copia indipendente.
//! 2. Un rollback da backup è indistinguibile da una riscrittura.
//! 3. L'hash garantisce **integrità, non verità**: uno span che non sostiene la
//!    claim è una riga impeccabilmente conforme.
//!
//! Nessuno dei tre è risolto da questo crate, e sono qui perché un sistema
//! onesto dichiara da sé i propri limiti.

use kbs_core::{
    ArgumentId, Invariant, Millis, PersonId, PublicationState, Ratification,
};

use crate::error::{Error, Result};
use crate::store::Store;

impl Store {
    /// Porta un argomento in corso, **se e solo se** la ratifica vale per il
    /// contenuto corrente.
    ///
    /// I due rifiuti hanno nomi diversi e sono i due errori giustiti:
    ///
    /// * nessuna ratifica → [`Invariant::CitableWithoutRatification`];
    /// * ratifica con `contract_hash` diverso dal `content_hash` corrente →
    ///   [`Invariant::StaleRatification`], che riporta i due hash.
    ///
    /// Il secondo è quello che rende vero D6: se il contratto è cambiato dopo la
    /// ratifica, la ratifica riguarda un testo che non esiste più, e «era già
    /// stato verificato» è una frase senza soggetto.
    ///
    /// Chiamare `publish` su un argomento già in corso non è un errore: ritorna
    /// lo stato. Da `archiviato` invece non si torna: l'inverso di `archive` non
    /// è definito da D4, e inventarlo sarebbe inventare una porta.
    pub fn publish(&mut self, id: &ArgumentId) -> Result<PublicationState> {
        let argument = self
            .argument_unrestricted(id)?
            .ok_or_else(|| Error::NotFound {
                kind: "argomento",
                id: id.0.clone(),
            })?;
        // La precondizione si controlla **prima** dello stato, e anche quando
        // l'argomento è già in corso: `publish` significa «adesso è citabile»,
        // e un argomento in corso con ratifica invecchiata non lo è. Se il
        // controllo venisse dopo, `publish` su un argomento già pubblicato
        // risponderebbe «ok» e la ratifica morta passerebbe inosservata.
        let Some(ratification) = argument.ratified.as_ref() else {
            return Err(Error::Invariant(Invariant::CitableWithoutRatification(
                argument.state,
            )));
        };
        if ratification.contract_hash != argument.content_hash {
            return Err(Error::Invariant(Invariant::StaleRatification {
                declared: ratification.contract_hash.clone(),
                current: argument.content_hash.clone(),
            }));
        }
        // Da qui in avanti è solo questione di stato: la porta è aperta.
        if argument.state == PublicationState::InCorso {
            return Ok(PublicationState::InCorso);
        }
        if argument.state == PublicationState::Archiviato {
            return Err(Error::StateTransition {
                id: id.clone(),
                from: Some(PublicationState::Archiviato),
                to: PublicationState::InCorso,
            });
        }
        self.conn.execute(
            "UPDATE arguments SET state = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id.0, crate::codec::state_to_db(PublicationState::InCorso), Millis::now().0],
        )?;
        Ok(PublicationState::InCorso)
    }

    /// Fuori uso, conservato. Chi lo ha seguito lo vede ancora: per questo
    /// `PublicationState::is_visible_to_enrolled` include `Archiviato`.
    ///
    /// Vale da qualunque stato, e non si torna indietro.
    pub fn archive(&mut self, id: &ArgumentId) -> Result<PublicationState> {
        let changed = self.conn.execute(
            "UPDATE arguments SET state = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![
                id.0,
                crate::codec::state_to_db(PublicationState::Archiviato),
                Millis::now().0
            ],
        )?;
        if changed == 0 {
            return Err(Error::NotFound {
                kind: "argomento",
                id: id.0.clone(),
            });
        }
        Ok(PublicationState::Archiviato)
    }

    /// Ratifica l'argomento per l'**hash di contenuto corrente**.
    ///
    /// Il `contract_hash` non è un parametro: è il contenuto che c'è adesso. Se
    /// fosse un parametro, un chiamante potrebbe ratificare un hash che non
    /// corrisponde a nulla e la ratifica varrebbe per sempre senza volerlo.
    ///
    /// Ratificare di nuovo un argomento già ratificato è il modo per rimettere in
    /// ordine una ratifica invecchiata: è l'azione che il docente fa quando ha
    /// cambiato il contratto e l'ha riletto.
    pub fn ratify(
        &mut self,
        id: &ArgumentId,
        by: &PersonId,
        note: &str,
    ) -> Result<Ratification> {
        let argument = self
            .argument_unrestricted(id)?
            .ok_or_else(|| Error::NotFound {
                kind: "argomento",
                id: id.0.clone(),
            })?;
        let ratification = Ratification {
            by: by.clone(),
            at: Millis::now(),
            contract_hash: argument.content_hash.clone(),
            note: note.to_string(),
        };
        self.conn.execute(
            "UPDATE arguments SET ratified_by = ?2, ratified_at = ?3, \
                                ratified_contract_hash = ?4, ratified_note = ?5, \
                                updated_at = ?6 \
             WHERE id = ?1",
            rusqlite::params![
                id.0,
                ratification.by.0,
                ratification.at.0,
                ratification.contract_hash,
                ratification.note,
                ratification.at.0,
            ],
        )?;
        Ok(ratification)
    }

    /// Ritira la ratifica. È l'unico modo in cui un argomento in corso torna
    /// non citabile senza cambiarne il testo, e per questo non è un campo libero:
    /// si dichiara esplicitamente.
    pub fn withdraw_ratification(&mut self, id: &ArgumentId) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE arguments SET ratified_by = NULL, ratified_at = NULL, \
                                ratified_contract_hash = NULL, ratified_note = NULL, \
                                updated_at = ?2 \
             WHERE id = ?1 AND ratified_at IS NOT NULL",
            rusqlite::params![id.0, Millis::now().0],
        )?;
        if changed == 0 {
            return Err(Error::NotFound {
                kind: "ratifica",
                id: id.0.clone(),
            });
        }
        Ok(())
    }
}
