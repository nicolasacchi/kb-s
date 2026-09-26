//! Il generatore: una famiglia di esercizi e la sua messa in forma.
//!
//! D8 chiede che un esercizio sia prodotto da un generatore con seed, e che due
//! istanze della stessa famiglia abbiano **lo stesso ragionamento e parametri
//! diversi**. Qui questo è un tratto, non una convenzione: [`Generator`] è
//! `Send + Sync` e ha due metodi obbligatori, `build` e `guardian`, e un
//! esercizio è una funzione di `(generator_version, seed)` senza altri input.
//!
//! # Perché `build` restituisce `Result` e non `Instance`
//!
//! Il tratto nella specifica è `fn generate(&self, seed: &str) -> Instance`, e
//! questo crate se ne discosta di proposito: un generatore che non può onorare
//! la propria richiesta — un set di parametri che non ammette la proprietà
//! richiesta, un numero di opzioni insufficiente — deve poter **rifiutare**, e
//! deve poter dire perché. Il prezzo è che `generate` restituisce
//! `Result<Instance, ExerciseError>` invece di `Instance`; il vantaggio è che
//! nessun `unwrap` finisce in un percorso che produce un voto.
//!
//! `generate` esiste comunque, e per ogni seed che passa vale esattamente la
//! firma della specifica meno il `Result`: è il metodo fornito, e le famiglie
//! implementano una cosa sola.
//!
//! # Cosa non c'è
//!
//! Nessun accesso al filesystem, nessun orologio, nessun processo, nessuna
//! rete. Un generatore è una funzione pura da una stringa a un valore: è la
//! condizione perché D11 (replay) sia una proprietà e non una speranza.

use kbs_core::{ArgumentId, Checker, CourseId, Exercise, Instance, Millis, PersonId};

use crate::error::ExerciseError;

/// Una famiglia di esercizi.
pub trait Generator: Send + Sync {
    /// Il nome della famiglia. Due istanze dello stesso ragionamento.
    fn family(&self) -> &str;

    /// La versione del generatore. Entra nella tupla di D11: cambiarla cambia
    /// la riproducibilità, quindi è un atto esplicito e versionato, non un
    /// dettaglio.
    fn generator_version(&self) -> &str;

    /// La tupla (istanza, checker) per un seed.
    ///
    /// Le due cose nascono insieme perché sono la stessa generazione: l'una
    /// porta i parametri e la risposta, l'altra la forma del confronto. Separarle
    /// sarebbe un modo per sbagliare pairing (vedi `check::is_bound_to`).
    fn build(&self, seed: &str) -> Result<(Instance, Checker), ExerciseError>;

    /// Il GUARDIAN del contratto: l'unico testo libero che l'unità porta, e il
    /// solo testo sottoposto a `no_solution_leak`.
    ///
    /// È statico per famiglia, e questa è una scelta: se il GUARDIAN cambiasse
    /// a ogni istanza, la prova D8 ripeterebbe mille volte la stessa frase e non
    /// coprirebbe niente di nuovo.
    fn guardian(&self) -> &str;

    /// Il testo dell'esercizio **senza i parametri**: il ragionamento, che è
    /// ciò che due istanze della stessa famiglia condividono.
    fn template_prompt(&self) -> &str;

    /// L'istanza per un seed. È il metodo della specifica; vedi il modulo.
    fn generate(&self, seed: &str) -> Result<Instance, ExerciseError> {
        self.build(seed).map(|(i, _)| i)
    }

    /// L'id dell'esercizio: famiglia, versione e seed, in modo che l'id
    /// dica da dove viene senza bisogno di una tabella.
    fn exercise_id(&self, seed: &str) -> String {
        format!("{}@{}#{}", self.family(), self.generator_version(), seed)
    }

    /// L'esercizio di `kbs_core`, con il binding al corso e all'argomento.
    ///
    /// Il `Checker` qui è quello dell'istanza del `seed` indicato: il
    /// percorso di pubblicazione registra l'esercizio nell'istanza che il
    /// docente ha ispezionato, e le altre istanze sono la stessa famiglia con
    /// altri parametri. È una convenzione dichiarata, non una garanzia: chi
    /// registra un esercizio deve dire quale seed ha guardato.
    fn exercise_for(
        &self,
        seed: &str,
        course: CourseId,
        argument: ArgumentId,
        at: Millis,
        by: PersonId,
    ) -> Result<Exercise, ExerciseError> {
        let (_, checker) = self.build(seed)?;
        Ok(Exercise {
            id: self.exercise_id(seed),
            course,
            argument,
            family: self.family().to_string(),
            generator_version: self.generator_version().to_string(),
            prompt: self.template_prompt().to_string(),
            checker,
            created_at: at,
            created_by: by,
        })
    }
}
