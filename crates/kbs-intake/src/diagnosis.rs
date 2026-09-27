//! La diagnosi orale del docente: l'unico giudizio umano che il sistema ha.
//!
//! Il docente interroga uno studente, conclude che su un argomento X non c'è
//! niente di fatto, e chiede del materiale. Quella conclusione è un **record**:
//! è la cosa che giustifica il materiale che verrà scritto, e se il materiale
//! non la porta con sé, due anni dopo la domanda «perché c'è questo esercizio
//!?» non ha risposta.
//!
//! # Perché `Evidence::Oral` esiste
//!
//! `kbs-core` dichiara la variante e dice perché: è **non riproducibile**. Non
//! c'è programma che rifaccia l'interrogazione, non c'è seed che riproduca
//! «Marco ha detto che 3/6 e 2/4 sono la stessa cosa ma non 1/2». E va detto
//! che non lo è, perché un registro che dichiara non riproducibile ciò che lo è
//! — e riproducibile ciò che non lo è — è peggio di un registro che non
//! esiste: fa prendere decisioni su una garanzia che non ha.
//!
//! # Il buco, dichiarato con un nome
//!
//! Il registro sa rappresentare la diagnosi: `append_observation` con
//! `Evidence::Oral` ci riesce, ed è ciò che fa [`registra`]. Il generato sa
//! citarla: l'id della diagnosi entra nei **byte canonici del prompt**
//! (vedi [`crate::prompt`]), e quindi dentro `prompt_hash`. Quindi la
//! risposta a «è stato generato in risposta a quello che il docente ha detto a
//! Marco il 14?» è **sì, e si dimostra**, ma solo come:
//!
//! > «questa generazione ha avuto un input che non è il corpus».
//!
//! Quello che **non** si può fare è chiedere al corpus *quali generazioni sono
//! nate da quella diagnosi*. Manca la colonna: `generations` ha `lock_id`,
//! `argument_id`, `requester`, `at` — nessun riferimento all'osservazione che
//! ha motivato la richiesta. Il nome del buco è
//! **`generations.senza-causa`**.
//!
//! Non è una nota a margine e non è risolvibile qui: `kbs-intake` non possiede
//! lo schema (D12 e `kbs-store` sì), e riempirlo con un campo di testo libero
//! sarebbe la cosa peggiore — un campo che sembra tenere il legame e non lo
//! tiene. Il workaround onesto che c'è adesso è dentro l'hash, e si dichiara
//! per quello che è: **il legame è incriptato ma non interrogabile**.
//!
//! Il costo di questo buco, detto come costo: un ricorso su un esercizio
//! generato non può risalire alla diagnosi per elencare le altre generazioni
//! della stessa causa. Si risale al prompt hash e al registro a mano. Quando il
//! buco verrà chiuso, il lavoro fatto con `[crate::prompt]` non cambia: cambia
//! solo la possibilità di chiedere.

use kbs_core::{
    ArgumentId, CohortId, CourseId, Evidence, GraderKind, Millis, Observation, PersonId,
};
use kbs_store::{ObservationDraft, Register, Store};
use sha2::Digest;

use crate::error::{Error, Result};
use crate::prompt::DiagnosisRef;

/// Una diagnosi orale, pronta per il registro.
///
/// `argument` è **l'argomento di cui si parla**, non quello che la diagnosi
/// produce: il docente ha visto che Marco non distingue la forma ridotta, e la
/// forma ridotta è un argomento che già esiste. Quello che verrà generato è
/// un altro argomento, e sarà lui a puntare qui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    pub id: String,
    pub student: PersonId,
    pub course: CourseId,
    pub cohort: CohortId,
    pub argument: ArgumentId,
    /// Che cosa il docente ha visto. Va bene una frase: l'obiettivo non è un
    /// verbale, e un verbale che nessuno rilegge non è un registro.
    pub note: String,
    /// Chi era presente, se qualcuno lo era. `kbs_core::Evidence::Oral` lo
    /// chiede ed è `Option`: una diagnosi a due occhi è più forte di una a
    /// uno, ma pretenderla renderebbe il campo sempre pieno e quindi inutile.
    pub witness: Option<PersonId>,
    pub at: Millis,
}

impl Diagnosis {
    /// L'id è derivato dal **contenuto** della diagnosi, non da un contatore.
    ///
    /// Un contatore darebbe a due diagnosi lo stesso id dopo un ripristino da
    /// backup, che è uno dei tre limiti che `kbs-verify` dichiara; qui l'id
    /// derivato rende visibile la collisione invece di nasconderla. E rende la
    /// registrazione idempotente: la stessa osservazione, registrata due volte,
    /// è la stessa osservazione.
    pub fn with_derived_id(mut self) -> Self {
        self.id = self.derived_id();
        self
    }

    /// L'id che avrebbe questa diagnosi.
    pub fn derived_id(&self) -> String {
        let mut h = sha2::Sha256::new();
        h.update([0x15]);
        for parte in [
            self.student.as_str(),
            self.course.as_str(),
            self.cohort.as_str(),
            self.argument.as_str(),
            self.note.as_str(),
        ] {
            h.update((parte.len() as u32).to_be_bytes());
            h.update(parte.as_bytes());
        }
        h.update(self.witness.as_ref().map(|w| w.as_str()).unwrap_or("").as_bytes());
        h.update(self.at.0.to_be_bytes());
        format!("obs_{}", &hex::encode(h.finalize())[..24])
    }

    /// La prova, nella forma di `kbs-core`.
    ///
    /// Il riferimento è costruito qui e non è un campo: è l'unica prova che
    /// questa funzione scrive, e scriverlo qui significa che non se ne può
    /// dimenticare la forma.
    pub fn evidence(&self) -> Evidence {
        Evidence::Oral {
            note: self.note.clone(),
            witness: self.witness.clone(),
        }
    }
}

/// Registra la diagnosi nel registro delle dimostrazioni.
///
/// Restituisce l'osservazione **con il `seq` che il registro le ha assegnato**:
/// è la quantità che la catena di hash di D6 ordina, e restituirla senza
/// significherebbe far credere che l'osservazione sia già nella catena.
///
/// La sessione è `observations`, ed è la sola: `SeqInSession` sta dentro il suo
/// registro e un seq 1 di `observations` non è un seq 1 di `gradings`.
pub fn registra(
    store: &mut Store,
    session: &kbs_store::SessionId,
    diagnosi: Diagnosis,
) -> Result<Observation> {
    if diagnosi.note.trim().is_empty() {
        return Err(Error::LockIncompleto { campo: "note" });
    }
    let draft = ObservationDraft {
        id: diagnosi.id.clone(),
        student: diagnosi.student.clone(),
        course: diagnosi.course.clone(),
        cohort: diagnosi.cohort.clone(),
        argument: diagnosi.argument.clone(),
        evidence: diagnosi.evidence(),
        // `Teacher` e non `Human`: sono due giudicatori diversi in `kbs-core` e
        // la differenza è che il docente ha visto lo studente, mentre `Human`
        // è un giudizio sul materiale. Scegliere `Human` qui sarebbe dire che
        // l'interrogazione non è avvenuta.
        judged_by: Some(GraderKind::Teacher),
        at: diagnosi.at,
    };
    Ok(store.append_observation(session, draft)?)
}

/// Apre e sigila una sessione di `observations` intorno alla registrazione.
///
/// Serve perché `append_observation` pretende una sessione aperta, e perché una
/// sessione sigillata è ciò che rende la catena di hash calcolabile su un
/// insieme chiuso di righe. Il `note` finisce nell'id della sessione, ed è il
/// posto giusto per dire **perché** si è aperta.
pub fn registra_in_una_sessione(
    store: &mut Store,
    mut diagnosi: Diagnosis,
    nota_sessione: &str,
) -> Result<Observation> {
    if diagnosi.id.trim().is_empty() {
        diagnosi = diagnosi.with_derived_id();
    }
    let sessione = store.open_session(Register::Observations, nota_sessione)?;
    let osservazione = registra(store, &sessione, diagnosi)?;
    store.close_session(&sessione)?;
    Ok(osservazione)
}

/// Il riferimento a una diagnosi, per il prompt.
///
/// **Verifica che la diagnosi esista** prima di costruire il riferimento: è il
/// punto in cui il buco `generations.senza-causa` si fa sentire, e il modo più
/// economico per non propagarlo è non poter costruire un riferimento a qualcosa
/// che non c'è. Un riferimento a una diagnosi inesistente sarebbe un hash che
/// impegna una parola e non un fatto.
pub fn referenzia(
    store: &Store,
    by: &PersonId,
    student: &PersonId,
    argomento: &ArgumentId,
    id: &str,
) -> Result<DiagnosisRef> {
    let osservazioni = store.observations_for(by, student, argomento)?;
    let trovata = osservazioni
        .into_iter()
        .find(|o| o.id == id)
        .ok_or_else(|| Error::DiagnosiAssente { id: id.to_string() })?;
    let Evidence::Oral { note, witness } = trovata.evidence else {
        return Err(Error::DiagnosiAssente {
            id: format!("{id} (ma non è orale: è {})",nome_prova(&trovata.evidence)),
        });
    };
    Ok(DiagnosisRef::new(trovata.id, note, witness))
}

fn nome_prova(e: &Evidence) -> &'static str {
    match e {
        Evidence::Checked { .. } => "checked",
        Evidence::Oral { .. } => "oral",
        Evidence::Written { .. } => "written",
        Evidence::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnosi() -> Diagnosis {
        Diagnosis {
            id: String::new(),
            student: PersonId::fixture(7),
            course: CourseId("matematica-terza".into()),
            cohort: CohortId("2026-terza".into()),
            argument: ArgumentId::from_rel_path("letture/03-frazione-irriducibile.html"),
            note: "Marco scrive 3/6 e 2/4 uguali ma 2/6 no: non distingue il denominatore".into(),
            witness: Some(PersonId::fixture(2)),
            at: Millis(1_757_000_000_000),
        }
    }

    #[test]
    fn l_id_deriva_dal_contenuto_e_non_da_un_contatore() {
        let a = diagnosi().with_derived_id();
        let b = diagnosi().with_derived_id();
        assert_eq!(a.id, b.id);
        assert!(a.id.starts_with("obs_"));
        let mut diverso = diagnosi();
        diverso.note = "un'altra frase".into();
        assert_ne!(diverso.with_derived_id().id, a.id);
    }

    #[test]
    fn la_prova_e_orale_e_dichiara_di_non_essere_riproducibile() {
        let e = diagnosi().evidence();
        assert!(matches!(e, Evidence::Oral { .. }));
        assert!(!e.is_reproducible(), "è il punto di questa variante");
    }
}
