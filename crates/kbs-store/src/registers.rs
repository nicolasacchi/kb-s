//! I tre registri di D6 e la coorte di D9.
//!
//! | registro | domanda | può cambiare? |
//! |---|---|---|
//! | `claims` | questa frase è vera? | solo lo stato, mai il testo |
//! | `observations` | chi ha dimostrato che cosa? | niente |
//! | `gradings` | chi ha deciso, con che cosa, e qualcuno ha contestato? | solo l'esito della contestazione |
//!
//! Queste sono le risposte che il modulo dà alle tre domande, e sono diverse
//! perché le domande sono diverse: un'affermazione ritrattata è un fatto nuovo
//! sullo stesso oggetto, una dimostrazione non si riscrive, e una contestazione
//! si risolve.
//!
//! # Chi può leggere un registro
//!
//! `kbs-core` ha il predicato per gli **argomenti**. Un giudizio e una
//! dimostrazione non sono un argomento: sono il registro dello studente. Qui la
//! regola è dichiarata per esteso, e sono tre righe perché sono tre:
//!
//! * chi è lo **studente** di cui è la riga;
//! * chi ha **emesso** il giudizio (`grading.graded_by`), che per un pari è il
//!   modo per rivedere il proprio lavoro senza vedere quello dei compagni;
//! * chi **insegna** il corso.
//!
//! Non c'è il resto della classe. Un pari che ha valutato il compito del
//! compagno non vede quel giudizio: la difesa procedurale primaria è la
//! contestazione, e la privacy del valutato non è un dettaglio. Se un domani
//! servisse il pari in lettura, è una relazione nuova e va dichiarata come
//! tale, non aggiunta di nascosto a `may_read`.

use rusqlite::OptionalExtension;

use kbs_core::{
    ArgumentId, Claim, Contestation, ContestationOutcome, CohortId, CohortSignal, CourseId,
    Grading, Millis, Observation, PersonId, Relation, SeqInSession, COHORT_MIN_K,
};

use crate::codec;
use crate::error::{Error, Result};
use crate::store::Store;
use crate::types::{GradingDraft, ObservationDraft, Register, SessionId};

// ── le sessioni ──────────────────────────────────────────────────────────────

impl Store {
    /// Apre una sessione di registro.
    ///
    /// Il nome è composed dal registro e dal tempo e porta con sé la nota: due
    /// sessioni aperte nella stessa millisecondo con la stessa nota sono la
    /// stessa sessione, e una `PRIMARY KEY` dice «no» invece di creare una
    /// seconda sessione che nessuno distingue.
    pub fn open_session(&mut self, register: Register, note: &str) -> Result<SessionId> {
        let at = Millis::now();
        let id = SessionId::new(format!("ses_{}_{}_{}", register.as_str(), at.0, note.len()));
        self.conn.execute(
            "INSERT INTO write_sessions (id, register, opened_at, sealed_at, note) \
             VALUES (?1, ?2, ?3, NULL, ?4)",
            rusqlite::params![id.0, register.as_str(), at.0, note],
        )?;
        Ok(id)
    }

    /// Chiude la sessione. Da qui in poi non si appende più niente: il `seq`
    /// di una sessione chiusa è definitivo, ed è la quantità che la catena di
    /// hash ordina.
    pub fn close_session(&mut self, id: &SessionId) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE write_sessions SET sealed_at = ?2 WHERE id = ?1 AND sealed_at IS NULL",
            rusqlite::params![id.0, Millis::now().0],
        )?;
        if changed == 0 {
            return Err(Error::SessionSealed { id: id.0.clone() });
        }
        Ok(())
    }

    /// Controlla che la sessione esista, sia del registro giusto e sia aperta.
    fn check_session(&self, id: &SessionId, expected: Register) -> Result<()> {
        let row: Option<(String, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT register, sealed_at FROM write_sessions WHERE id = ?1",
                [&id.0],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .optional()?;
        let Some((register, sealed_at)) = row else {
            return Err(Error::UnknownSession { id: id.0.clone() });
        };
        if sealed_at.is_some() {
            return Err(Error::SessionSealed { id: id.0.clone() });
        }
        if register.as_str() != expected.as_str() {
            return Err(Error::SessionRegister {
                id: id.0.clone(),
                expected: expected.as_str().to_string(),
                got: register,
            });
        }
        Ok(())
    }

    /// Il prossimo `seq` nella sessione.
    ///
    /// Lo assegna il registro e non chi scrive: `SeqInSession` è la quantità
    /// che la catena di hash di D6 ordina, e se la scegliesse il chiamante due
    /// scrittori darebbero lo stesso numero e la catena avrebbe due foglie con la
    /// stessa posizione. L'`UNIQUE (session_id, seq)` è la rete di sicurezza.
    fn next_seq(&self, session: &SessionId, table: &'static str) -> Result<u64> {
        let sql = format!("SELECT COALESCE(MAX(seq), 0) + 1 FROM {table} WHERE session_id = ?1");
        let next: i64 = self.conn.query_row(&sql, [&session.0], |r| r.get(0))?;
        Ok(next as u64)
    }
}

// ── claims ────────────────────────────────────────────────────────────────────

impl Store {
    /// Aggiunge un'affermazione al registro.
    ///
    /// Un id che esiste già è un errore e non un aggiornamento: un registro che
    /// sovrascrive le proprie affermazioni non è un registro, e riciclare un id
    /// fa svanire la traccia di ciò che era stato affermato prima.
    pub fn append_claim(&mut self, claim: &Claim) -> Result<()> {
        let (status, reason) = codec::claim_status_to_db(&claim.status);
        let emitter = codec::emitter_to_row(&claim.emitted_by);
        let inserted = self.conn.execute(
            "INSERT INTO claims (id, course_id, argument_id, text, span_anchor, span_text, \
                                  status, status_reason, emitted_at, \
                                  emitter, emitter_by, emitter_argument, emitter_observation) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                claim.id,
                claim.course.0,
                claim.argument.0,
                claim.text,
                claim.span_anchor,
                claim.span_text,
                status,
                reason,
                claim.emitted_at.0,
                emitter.kind,
                emitter.by,
                emitter.argument,
                emitter.observation,
            ],
        ).map_err(|e| classify("claim", &claim.id, e))?;
        debug_assert_eq!(inserted, 1);
        Ok(())
    }

    /// Ritira un'affermazione, con la ragione.
    ///
    /// La ragione non è opzionale e il `CHECK` del database lo impedisce: un
    /// ritratto senza motivo è una cancellazione travestita, e la differenza
    /// fra le due cose è tutta la ragione per cui il registro vale.
    ///
    /// Cambia **solo** lo stato. Il testo, lo span e l'emittente sono congelati da
    /// un trigger, e questa è la sola `UPDATE` che il modulo esegue su `claims`.
    pub fn retract_claim(&mut self, id: &str, reason: &str) -> Result<()> {
        if reason.trim().is_empty() {
            return Err(Error::InvalidField {
                field: "reason",
                reason: "un ritratto senza motivo è una cancellazione".into(),
            });
        }
        let changed = self.conn.execute(
            "UPDATE claims SET status = 'retracted', status_reason = ?2 WHERE id = ?1",
            rusqlite::params![id, reason],
        )?;
        if changed == 0 {
            return Err(Error::NotFound {
                kind: "claim",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    /// Le affermazioni di un argomento, per chi lo può vedere.
    ///
    /// La visibilità è quella dell'argomento che le contiene: una claim non è
    /// altro materiale con regole sue, è un'affermazione *su* quell'unità di
    /// insegnamento, e mostrarne le citabilità senza l'unità sarebbe incoerente.
    pub fn claims_for(&self, person: &PersonId, argument: &ArgumentId) -> Result<Vec<Claim>> {
        self.read_argument(person, argument)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, course_id, argument_id, text, span_anchor, span_text, status, \
                    status_reason, emitted_at, emitter, emitter_by, emitter_argument, emitter_observation \
               FROM claims WHERE argument_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([&argument.0], map_claim)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

// ── observations ──────────────────────────────────────────────────────────────

impl Store {
    /// Aggiunge un'osservazione e le assegna il `seq` nella sessione.
    ///
    /// **Il `seq` lo mette il registro**: primo valore libero, senza buchi,
    /// a partire da 1. Non è una scelta di D6 — D6 non numerizza da 1, e non
    /// spiega perché — ma è la convenzione che rende la `UNIQUE (session_id, seq)`
    /// e il trigger di append-only sufficienti a tenere il registro in ordine. Un
    /// buco è irraggiungibile da qui, il che non è una garanzia che si possa
    /// allegare: per quello, `kbs-verify` ricontrolla la contiguità da parte sua.
    ///
    /// Il resto dei campi viene da chi scrive, perché sono fatti sul lavoro dello
    /// studente e il registro non li deduce.
    pub fn append_observation(
        &mut self,
        session: &SessionId,
        draft: ObservationDraft,
    ) -> Result<Observation> {
        self.check_session(session, Register::Observations)?;
        let seq = self.next_seq(session, "observations")?;
        let (evidence, payload) = codec::evidence_to_db(&draft.evidence);
        let observation = Observation {
            id: draft.id.clone(),
            seq: SeqInSession(seq),
            student: draft.student.clone(),
            course: draft.course.clone(),
            cohort: draft.cohort.clone(),
            argument: draft.argument.clone(),
            evidence: draft.evidence.clone(),
            judged_by: draft.judged_by,
            at: draft.at,
        };
        let inserted = self.conn.execute(
            "INSERT INTO observations (id, session_id, seq, student, course_id, cohort, argument_id, \
                                        evidence, evidence_payload, judged_by, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                observation.id,
                session.0,
                observation.seq.0 as i64,
                observation.student.0,
                observation.course.0,
                observation.cohort.0,
                observation.argument.0,
                evidence,
                payload,
                observation.judged_by.map(codec::grader_to_db),
                observation.at.0,
            ],
        ).map_err(|e| classify("osservazione", &observation.id, e))?;
        debug_assert_eq!(inserted, 1);
        Ok(observation)
    }

    /// Le osservazioni di una sessione, nell'ordine che la catena di hash ordina.
    ///
    /// È il substrate del replay: senza l'ordine, «riprodurre la generazione» e
    /// «riprodurre la sequenza» non sono la stessa cosa.
    pub fn observations_in_session(&self, session: &SessionId) -> Result<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, student, course_id, cohort, argument_id, evidence, evidence_payload, \
                    judged_by, at \
               FROM observations WHERE session_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([&session.0], map_observation)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Le osservazioni su un argomento, viste da chi ha diritto di vederle.
    ///
    /// Chi non ha diritto riceve `NotReadable`, non un elenco vuoto: un elenco
    /// vuoto è una risposta vera, e qui non lo sarebbe.
    pub fn observations_for(
        &self,
        person: &PersonId,
        student: &PersonId,
        argument: &ArgumentId,
    ) -> Result<Vec<Observation>> {
        let unit = self.read_argument(person, argument)?;
        // Nessun emittente da accreditare: `Observation.judged_by` è una *specie*
        // di giudicatore (`deterministic`, `peer`, …), non una persona, e D3 vieta
        // che sia un modello. Chi ha valutato una dimostrazione non è quindi
        // tracciato come persona in questa riga, e non può avere il diritto di
        // leggerla per questa via. È un limite dichiarato del registro delle
        // dimostrazioni, non una scelta silenziosa.
        self.check_register_reader(person, &unit.course, student, &[])?;
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, student, course_id, cohort, argument_id, evidence, evidence_payload, \
                    judged_by, at \
               FROM observations WHERE argument_id = ?1 AND student = ?2 ORDER BY id",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![argument.0, student.0],
            map_observation,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

// ── gradings ──────────────────────────────────────────────────────────────────

impl Store {
    /// Aggiunge un giudizio e gli assegna il `seq` nella sessione.
    ///
    /// Il `rubric_version` referenzia `rubric_versions`: un voto senza rubric non
    /// entra nel database, e questo è un vincolo del database e non una
    /// verifica in codice che qualcuno può dimenticare di fare.
    pub fn append_grading(
        &mut self,
        session: &SessionId,
        draft: GradingDraft,
    ) -> Result<Grading> {
        self.check_session(session, Register::Gradings)?;
        let seq = self.next_seq(session, "gradings")?;
        let (cby, cat, creason, coutcome) = match &draft.contested {
            None => (None, None, None, None),
            Some(c) => {
                let (by, at, reason, outcome) = codec::contestation_to_row(c);
                (Some(by), Some(at), Some(reason), outcome.map(str::to_string))
            }
        };
        let grading = Grading {
            id: draft.id.clone(),
            seq: SeqInSession(seq),
            student: draft.student.clone(),
            course: draft.course.clone(),
            argument: draft.argument.clone(),
            kind: draft.kind,
            graded_by: draft.graded_by.clone(),
            rubric_version: draft.rubric_version.clone(),
            grade: draft.grade.clone(),
            at: draft.at,
            contested: draft.contested.clone(),
        };
        let inserted = self.conn.execute(
            "INSERT INTO gradings (id, session_id, seq, student, course_id, argument_id, kind, \
                                    graded_by, rubric_version, grade, at, \
                                    contested_by, contested_at, contested_reason, contested_outcome) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                grading.id,
                session.0,
                grading.seq.0 as i64,
                grading.student.0,
                grading.course.0,
                grading.argument.0,
                codec::grader_to_db(grading.kind),
                grading.graded_by.0,
                grading.rubric_version,
                grading.grade,
                grading.at.0,
                cby,
                cat,
                creason,
                coutcome,
            ],
        ).map_err(|e| classify("giudizio", &grading.id, e))?;
        debug_assert_eq!(inserted, 1);
        Ok(grading)
    }

    /// Apre una contestazione su un giudizio.
    ///
    /// Si apre **una volta**: una seconda contestazione non sostituisce la prima,
    /// la rifiuta. Il ricorso è un fatto, non una colonna che si aggiorna, e
    /// sostituirlo farebbe perdere la traccia del primo ricorso — che è
    /// esattamente ciò che la difesa procedurale non può permettere.
    pub fn contest_grading(&mut self, grading: &str, contestation: Contestation) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE gradings SET contested_by = ?2, contested_at = ?3, contested_reason = ?4 \
             WHERE id = ?1 AND contested_at IS NULL",
            rusqlite::params![
                grading,
                contestation.by.0,
                contestation.at.0,
                contestation.reason
            ],
        )?;
        if changed == 0 {
            return Err(self.grading_error(grading, true));
        }
        Ok(())
    }

    /// Dà l'esito a una contestazione aperta.
    ///
    /// L'esito è una decisione **successiva**: per questo l'unica `UPDATE` che
    /// questo modulo esegue su `gradings` tocca le colonne della contestazione, e
    /// un trigger rifiuta qualunque altra modifica. Il giudizio è congelato; la
    /// contestazione è viva.
    pub fn resolve_contestation(
        &mut self,
        grading: &str,
        outcome: ContestationOutcome,
    ) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE gradings SET contested_outcome = ?2 WHERE id = ?1 AND contested_at IS NOT NULL",
            rusqlite::params![grading, codec::outcome_to_db(outcome)],
        )?;
        if changed == 0 {
            return Err(self.grading_error(grading, false));
        }
        Ok(())
    }

    /// Distingue «non esiste» da «non si può»: un giudizio che non c'è non è la
    /// stessa cosa di un giudizio già contestato, e qui non c'è un predicato di
    /// visibilità in mezzo perché il chiamante lo abbia già attraversato.
    fn grading_error(&self, id: &str, already_contested: bool) -> Error {
        let exists: bool = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM gradings WHERE id = ?1)",
                [id],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if !exists {
            return Error::NotFound {
                kind: "giudizio",
                id: id.to_string(),
            };
        }
        if already_contested {
            return Error::AlreadyContested {
                grading: id.to_string(),
            };
        }
        Error::NotFound {
            kind: "contestazione",
            id: id.to_string(),
        }
    }

    /// I giudizi su un argomento, **contestazione inclusa**, in una sola query.
    ///
    /// Una sola `SELECT` è il punto: la contestazione è dentro la riga, e due
    /// query che possono disaccordarsi sono due fonti di verità. Chi legge un
    /// ricorso deve vedere il giudizio e la contestazione nello stesso atto, o
    /// rischia di rendersi conto che manca la metà.
    pub fn gradings_for(
        &self,
        person: &PersonId,
        student: &PersonId,
        argument: &ArgumentId,
    ) -> Result<Vec<Grading>> {
        let unit = self.read_argument(person, argument)?;
        let emittenti = self.graders(student, &unit.course, Some(argument))?;
        self.check_register_reader(person, &unit.course, student, &emittenti)?;
        self.gradings_of(student, &unit.course, Some(argument))
    }

    /// I giudizi di uno studente su un corso, tutti gli argomenti, contestazione
    /// inclusa.
    pub fn student_gradings(
        &self,
        person: &PersonId,
        student: &PersonId,
        course: &CourseId,
    ) -> Result<Vec<Grading>> {
        let emittenti = self.graders(student, course, None)?;
        self.check_register_reader(person, course, student, &emittenti)?;
        self.gradings_of(student, course, None)
    }

    /// Chi ha emesso almeno un giudizio su questi argomenti di questo studente.
    ///
    /// Serve a `check_register_reader`, ed è una `SELECT DISTINCT` e non una
    /// lettura del registro intero: la regola di visibilità non deve costare un
    /// caricamento di tutto per decidere chi può leggerne una parte.
    fn graders(
        &self,
        student: &PersonId,
        course: &CourseId,
        argument: Option<&ArgumentId>,
    ) -> Result<Vec<PersonId>> {
        let (sql, extra): (&str, Option<String>) = match argument {
            Some(id) => (
                "SELECT DISTINCT graded_by FROM gradings \
                  WHERE student = ?1 AND course_id = ?2 AND argument_id = ?3",
                Some(id.0.clone()),
            ),
            None => (
                "SELECT DISTINCT graded_by FROM gradings WHERE student = ?1 AND course_id = ?2",
                None,
            ),
        };
        let letto = |r: &rusqlite::Row<'_>| r.get::<_, String>(0);
        let mut stmt = self.conn.prepare(sql)?;
        let mut out = Vec::new();
        match &extra {
            Some(a) => {
                let rows = stmt.query_map(rusqlite::params![student.0, course.0, a], letto)?;
                for row in rows {
                    out.push(PersonId(row?));
                }
            }
            None => {
                let rows = stmt.query_map(rusqlite::params![student.0, course.0], letto)?;
                for row in rows {
                    out.push(PersonId(row?));
                }
            }
        }
        Ok(out)
    }

    fn gradings_of(
        &self,
        student: &PersonId,
        course: &CourseId,
        argument: Option<&ArgumentId>,
    ) -> Result<Vec<Grading>> {
        let (sql, argument_param): (&str, Option<String>) = match argument {
            Some(id) => (
                "SELECT id, seq, student, course_id, argument_id, kind, graded_by, rubric_version, \
                        grade, at, contested_by, contested_at, contested_reason, contested_outcome \
                   FROM gradings WHERE student = ?1 AND course_id = ?2 AND argument_id = ?3 \
                  ORDER BY at, id",
                Some(id.0.clone()),
            ),
            None => (
                "SELECT id, seq, student, course_id, argument_id, kind, graded_by, rubric_version, \
                        grade, at, contested_by, contested_at, contested_reason, contested_outcome \
                   FROM gradings WHERE student = ?1 AND course_id = ?2 ORDER BY at, id",
                None,
            ),
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = match &argument_param {
            Some(a) => stmt.query_map(rusqlite::params![student.0, course.0, a], map_grading)?,
            None => stmt.query_map(rusqlite::params![student.0, course.0], map_grading)?,
        };
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// La rubrica, con la sua scala: senza questo un voto è una stringa.
    pub fn rubric_version(
        &self,
        id: &str,
    ) -> Result<Option<crate::types::RubricVersion>> {
        let row = self
            .conn
            .query_row(
                "SELECT v.id, v.rubric_id, v.version, v.scale, v.defined_at, v.note \
                   FROM rubric_versions v WHERE v.id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, rubric, version, scale, defined_at, note)) = row else {
            return Ok(None);
        };
        let scale: Vec<crate::types::GradeLevel> =
            serde_json::from_str(&scale).map_err(|e| Error::Corrupt {
                table: "rubric_versions",
                field: "scale",
                reason: e.to_string(),
            })?;
        Ok(Some(crate::types::RubricVersion {
            id,
            rubric,
            version,
            scale,
            defined_at: Millis(defined_at),
            note,
        }))
    }

    /// Scrive una rubrica e/o una sua versione.
    pub fn upsert_rubric_version(&mut self, v: &crate::types::RubricVersion) -> Result<()> {
        let scale = serde_json::to_string(&v.scale).map_err(|e| Error::InvalidField {
            field: "scale",
            reason: e.to_string(),
        })?;
        self.conn.execute(
            "INSERT INTO rubric_versions (id, rubric_id, version, scale, defined_at, note) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (id) DO UPDATE SET scale = ?4, note = ?6",
            rusqlite::params![v.id, v.rubric, v.version, scale, v.defined_at.0, v.note],
        )?;
        Ok(())
    }

    /// Chi può leggere il registro di uno studente: lo studente, chi ha emesso
    /// almeno uno dei giudizi, chi insegna. Dichiarato in tre righe perché sono
    /// tre, e sono il modulo a dirlo perché qui non c'è una tabella dei ruoli (D5).
    fn check_register_reader(
        &self,
        person: &PersonId,
        course: &CourseId,
        student: &PersonId,
        emittenti: &[PersonId],
    ) -> Result<()> {
        if person == student {
            return Ok(());
        }
        if emittenti.contains(person) {
            return Ok(());
        }
        let relations = self.relations_of(person, course)?;
        if relations.contains(&Relation::Teaches) {
            return Ok(());
        }
        Err(Error::NotReadable {
            person: person.clone(),
            id: ArgumentId(format!("registro:{student}")),
            state: kbs_core::PublicationState::Bozza,
        })
    }
}

// ── rubriche, esercizi, generazioni ───────────────────────────────────────────

impl Store {
    /// Scrive una rubrica. Idempotente sull'id.
    pub fn upsert_rubric(&mut self, r: &crate::types::Rubric) -> Result<()> {
        self.conn.execute(
            "INSERT INTO rubrics (id, course_id, title, created_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (id) DO UPDATE SET title = ?3",
            rusqlite::params![r.id, r.course.0, r.title, r.created_at.0],
        )?;
        Ok(())
    }

    /// Scrive un esercizio. Il checker è una coppia (kind, payload) e la
    /// conversione è in [`crate::codec`].
    pub fn upsert_exercise(&mut self, e: &kbs_core::Exercise) -> Result<()> {
        let (checker, payload) = codec::checker_to_db(&e.checker);
        self.conn.execute(
            "INSERT INTO exercises (id, course_id, argument_id, family, generator_version, prompt, \
                                    checker, checker_payload, created_at, created_by) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT (id) DO UPDATE SET prompt = ?6, checker = ?7, checker_payload = ?8",
            rusqlite::params![
                e.id,
                e.course.0,
                e.argument.0,
                e.family,
                e.generator_version,
                e.prompt,
                checker,
                payload,
                e.created_at.0,
                e.created_by.0,
            ],
        )?;
        Ok(())
    }

    /// Legge un esercizio, checker incluso.
    pub fn exercise(&self, id: &str) -> Result<Option<kbs_core::Exercise>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, course_id, argument_id, family, generator_version, prompt, checker, \
                        checker_payload, created_at, created_by \
                   FROM exercises WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, i64>(8)?,
                        r.get::<_, String>(9)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, course, argument, family, generator, prompt, checker, payload, created_at, created_by)) = row else {
            return Ok(None);
        };
        Ok(Some(kbs_core::Exercise {
            id,
            course: CourseId(course),
            argument: ArgumentId(argument),
            family,
            generator_version: generator,
            prompt,
            checker: codec::checker_from_db(&checker, &payload)?,
            created_at: Millis(created_at),
            created_by: PersonId(created_by),
        }))
    }

    /// Salva un'istanza. Il `UNIQUE (exercise, seed)` è la costruzione che rende
    /// la copia inefficace: stessa famiglia, seed diverse, risposte diverse.
    pub fn put_instance(&mut self, i: &kbs_core::Instance) -> Result<()> {
        let params = serde_json::to_string(&i.params).map_err(|e| Error::InvalidField {
            field: "params",
            reason: e.to_string(),
        })?;
        self.conn.execute(
            "INSERT INTO instances (id, exercise, seed, rendered_prompt, expected, params) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (exercise, seed) DO NOTHING",
            rusqlite::params![i.exercise, i.seed, i.rendered_prompt, i.expected, params],
        )?;
        Ok(())
    }

    /// Le istanze di un esercizio, in ordine di seed: ordinarle è ciò che rende
    /// confrontabili due istanze della stessa famiglia.
    pub fn instances_of(&self, exercise: &str) -> Result<Vec<kbs_core::Instance>> {
        let mut stmt = self.conn.prepare(
            "SELECT exercise, seed, rendered_prompt, expected, params \
               FROM instances WHERE exercise = ?1 ORDER BY seed",
        )?;
        let rows = stmt.query_map([exercise], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (exercise, seed, rendered_prompt, expected, params) = row?;
            out.push(kbs_core::Instance {
                exercise,
                seed,
                rendered_prompt,
                expected,
                params: serde_json::from_str(&params).map_err(|e| Error::Corrupt {
                    table: "instances",
                    field: "params",
                    reason: e.to_string(),
                })?,
            });
        }
        Ok(out)
    }

    /// Registra un evento di generazione (D10).
    ///
    /// Il lock si scrive insieme all'evento, perché un lock che non ha nessun
    /// evento è un lock che non è mai stato usato: si può registrare, ma è
    /// rumore. Il contrario — un evento senza lock — è impossibile per chiave
    /// esterna.
    pub fn record_generation(&mut self, event: &crate::types::GenerationEvent) -> Result<()> {
        let lock_id = self.put_lock(&event.lock)?;
        self.conn.execute(
            "INSERT INTO generations (lock_id, argument_id, requester, at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![lock_id, event.argument.0, event.requester.0, event.at.0],
        )?;
        Ok(())
    }

    /// Gli eventi di generazione di un argomento, in ordine di tempo: è la
    /// risposta a «come è nato questo materiale».
    pub fn generations_for(&self, argument: &ArgumentId) -> Result<Vec<crate::types::GenerationEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.model_id, l.prompt_hash, l.corpus_hash, l.generator_version, l.at, \
                    g.requester, g.at \
               FROM generations g JOIN model_locks l ON l.id = g.lock_id \
              WHERE g.argument_id = ?1 ORDER BY g.at, g.id",
        )?;
        let rows = stmt.query_map([&argument.0], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (model_id, prompt_hash, corpus_hash, generator_version, lock_at, requester, at) =
                row?;
            out.push(crate::types::GenerationEvent {
                lock: kbs_core::ModelLock {
                    model_id,
                    prompt_hash,
                    corpus_hash,
                    generator_version,
                    at: Millis(lock_at),
                },
                argument: argument.clone(),
                requester: PersonId(requester),
                at: Millis(at),
            });
        }
        Ok(out)
    }
}

// ── la coorte invisibile (D9) ───────────────────────────────────────────────

impl Store {
    /// Registra un segnale di coorte.
    ///
    /// **Sotto soglia l'errore è `CohortBelowThreshold` e la riga non entra.**
    /// Non è una cancellazione: le osservazioni individuali da cui il segnale
    /// viene calcolato restano nel registro, e sono loro che si contano. La
    /// soglia è un accesso all'aggregato, non una scomparsa del dato.
    ///
    /// Il rifiuto in scrittura è deliberato oltre al `CHECK` del database: se il
    /// segnale sotto soglia non esiste, nessun percorso di lettura dimenticato
    /// può pubblicarlo. Il `CHECK` copre il caso in cui qualcuno scrive SQL a
    /// mano, e il filtro in lettura copre il caso in cui il `CHECK` un domani non
    /// ci fosse più.
    pub fn record_cohort_signal(&mut self, signal: &CohortSignal) -> Result<()> {
        if !signal.is_publishable() {
            return Err(Error::Invariant(kbs_core::Invariant::CohortBelowThreshold {
                failing: signal.failing,
                min: COHORT_MIN_K,
            }));
        }
        self.conn.execute(
            "INSERT INTO cohort_signals (course_id, cohort, argument_id, failing, total, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (course_id, cohort, argument_id) DO UPDATE SET \
                 failing = ?4, total = ?5, at = ?6",
            rusqlite::params![
                signal.course.0,
                signal.cohort.0,
                signal.argument.0,
                signal.failing as i64,
                signal.total as i64,
                signal.at.0,
            ],
        )?;
        Ok(())
    }

    /// I segnali di coorte di un argomento, **solo quelli pubblicabili**.
    ///
    /// Il filtro è in Rust **e** c'è un `CHECK` nel database. La barriera
    /// principale è il `CHECK`, e va detto con chiarezza: il filtro in lettura
    /// non è esercitabile da nessun test, perché una riga sotto soglia non può
    /// esistere. Un commento che promette una difesa che nessun test può
    /// toccare vale come un commento senza test, quindi la difesa secondaria si
    /// dichiara per quello che è: coprirebbe il caso in cui la costante di
    /// dominio cambiasse e la migrazione non seguisse. Il caso che il test
    /// copre è l'altro, quello che capita: la soglia scritta nel `CHECK` e
    /// quella di `kbs_core` che coincidono.
    pub fn cohort_signals(&self, argument: &ArgumentId) -> Result<Vec<CohortSignal>> {
        let mut stmt = self.conn.prepare(
            "SELECT course_id, cohort, argument_id, failing, total, at \
               FROM cohort_signals WHERE argument_id = ?1 ORDER BY course_id, cohort",
        )?;
        let rows = stmt.query_map([&argument.0], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (course, cohort, argument, failing, total, at) = row?;
            let signal = CohortSignal {
                course: CourseId(course),
                cohort: CohortId(cohort),
                argument: ArgumentId(argument),
                failing: failing as usize,
                total: total as usize,
                at: Millis(at),
            };
            if signal.is_publishable() {
                out.push(signal);
            }
        }
        Ok(out)
    }
}

// ── la lettura delle righe ───────────────────────────────────────────────────

fn map_claim(row: &rusqlite::Row<'_>) -> rusqlite::Result<Claim> {
    let id: String = row.get(0)?;
    let course: String = row.get(1)?;
    let argument: String = row.get(2)?;
    let text: String = row.get(3)?;
    let span_anchor: Option<String> = row.get(4)?;
    let span_text: Option<String> = row.get(5)?;
    let status_raw: String = row.get(6)?;
    let status_reason: Option<String> = row.get(7)?;
    let emitted_at: i64 = row.get(8)?;
    let emitter_raw: String = row.get(9)?;
    let emitter_by: Option<String> = row.get(10)?;
    let emitter_argument: Option<String> = row.get(11)?;
    let emitter_observation: Option<String> = row.get(12)?;

    let status =
        codec::claim_status_from_db(&status_raw, status_reason).map_err(|e| corrupt(6, e))?;
    let emitter_row = codec::EmitterRow {
        kind: match emitter_raw.as_str() {
            "teacher" => "teacher",
            "content" => "content",
            "from-work" => "from-work",
            other => {
                return Err(corrupt(
                    9,
                    Error::Corrupt {
                        table: "claims",
                        field: "emitter",
                        reason: format!("`{other}` non è un emittente noto"),
                    },
                ))
            }
        },
        by: emitter_by,
        argument: emitter_argument,
        observation: emitter_observation,
    };
    let emitted_by = codec::emitter_from_row(&emitter_row).map_err(|e| corrupt(9, e))?;

    Ok(Claim {
        id,
        course: CourseId(course),
        argument: ArgumentId(argument),
        text,
        span_anchor,
        span_text,
        status,
        emitted_at: Millis(emitted_at),
        emitted_by,
    })
}

fn map_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Observation> {
    let id: String = row.get(0)?;
    let seq: i64 = row.get(1)?;
    let student: String = row.get(2)?;
    let course: String = row.get(3)?;
    let cohort: String = row.get(4)?;
    let argument: String = row.get(5)?;
    let evidence_raw: String = row.get(6)?;
    let evidence_payload: Option<String> = row.get(7)?;
    let judged_by: Option<String> = row.get(8)?;
    let at: i64 = row.get(9)?;

    let evidence =
        codec::evidence_from_db(&evidence_raw, evidence_payload).map_err(|e| corrupt(6, e))?;
    let judged_by = match judged_by {
        None => None,
        Some(raw) => Some(codec::grader_from_db(&raw).map_err(|e| corrupt(8, e))?),
    };

    Ok(Observation {
        id,
        seq: SeqInSession(seq as u64),
        student: PersonId(student),
        course: CourseId(course),
        cohort: CohortId(cohort),
        argument: ArgumentId(argument),
        evidence,
        judged_by,
        at: Millis(at),
    })
}

fn map_grading(row: &rusqlite::Row<'_>) -> rusqlite::Result<Grading> {
    let id: String = row.get(0)?;
    let seq: i64 = row.get(1)?;
    let student: String = row.get(2)?;
    let course: String = row.get(3)?;
    let argument: String = row.get(4)?;
    let kind_raw: String = row.get(5)?;
    let graded_by: String = row.get(6)?;
    let rubric_version: String = row.get(7)?;
    let grade: String = row.get(8)?;
    let at: i64 = row.get(9)?;
    let contested_by: Option<String> = row.get(10)?;
    let contested_at: Option<i64> = row.get(11)?;
    let contested_reason: Option<String> = row.get(12)?;
    let contested_outcome: Option<String> = row.get(13)?;

    let contested = match (contested_by, contested_at, contested_reason) {
        (Some(by), Some(at), Some(reason)) => Some(
            codec::contestation_from_row(&by, at, &reason, contested_outcome.as_deref())
                .map_err(|e| corrupt(13, e))?,
        ),
        (None, None, None) => None,
        _ => {
            return Err(corrupt(
                10,
                Error::Corrupt {
                    table: "gradings",
                    field: "contested_by",
                    reason: "la contestazione c'è a metà".into(),
                },
            ))
        }
    };

    Ok(Grading {
        id,
        seq: SeqInSession(seq as u64),
        student: PersonId(student),
        course: CourseId(course),
        argument: ArgumentId(argument),
        kind: codec::grader_from_db(&kind_raw).map_err(|e| corrupt(5, e))?,
        graded_by: PersonId(graded_by),
        rubric_version,
        grade,
        at: Millis(at),
        contested,
    })
}

fn corrupt(column: usize, error: Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(error),
    )
}

/// Un errore SQLite di `UNIQUE` diventa un errore che **nomina la regola**.
///
/// Il resto delle violazioni di vincolo resta un `Sqlite` con il suo messaggio:
/// una chiave esterna mancante è un errore di chi chiama, ed è giusto che ne
/// legga la causa esatta invece di una riformulazione.
fn classify(kind: &'static str, id: &str, error: rusqlite::Error) -> Error {
    use rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY;
    use rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE;
    if let rusqlite::Error::SqliteFailure(failure, _) = &error {
        if failure.extended_code == SQLITE_CONSTRAINT_UNIQUE as i32
            || failure.extended_code == SQLITE_CONSTRAINT_PRIMARYKEY as i32
        {
            return Error::DuplicateId {
                kind,
                id: id.to_string(),
            };
        }
    }
    Error::Sqlite(error)
}
