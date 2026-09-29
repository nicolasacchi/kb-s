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
//! regola è dichiarata per esteso, e non è «tre righe» perché **non è una sola
//! regola**: sono due, e la differenza è quella che il modulo per primo ha
//! sbagliato.
//!
//! * **Tutto il registro** — lo **studente** di cui è la riga, e chi **insegna** il
//!   corso. Nient'altro.
//! * **Il proprio giudizio** — chi ha **emesso** un giudizio su un argomento di
//!   uno studente vede **quel** giudizio, con la contestazione che lo riguarda, e
//!   nient'altro del registro di quello studente.
//!
//! La seconda è la riga che va scritta con cura, perché «chi ha emesso un
//! giudizio» si prestava a una lettura molto più larga: *qualunque* giudizio
//! emesso a quello studente, su qualunque argomento del corso. E su quella
//! lettura un pari che ha valutato **un** compito leggeva il registro intero,
//! compresi i voti del docente e degli altri pari, con rubric e contestazioni. La
//! difesa procedurale primaria resta la contestazione, e la privacy del valutato
//! non è un dettaglio: rivedere il proprio giudizio è un diritto, leggere quello
//! dei compagni è un altro diritto che nessuno ha dichiarato. Sono due relazioni
//! diverse, e qui sono due righe diverse.
//!
//! # Le letture che non sono un registro
//!
//! Non tutto quello che c'è in questo file è materiale dello studente, e non tutto
//! ha la stessa regola. Sono dichiarate qui perché un lettore di questo modulo
//! deve poter sapere che cosa è coperto e che cosa no:
//!
//! * [`Store::claims_for`] e [`Store::observations_for`] passano da
//!   `read_argument`: l'argomento che le contiene decide la visibilità;
//! * [`Store::exercise`] e [`Store::instances_of`] sono il lato del generatore e
//!   del verificatore di D8, e **non** sono materiale che lo studente legge: il
//!   checker e `expected` sono la risposta, e D8 dice che l'integrità è per
//!   costruzione;
//! * [`Store::generations_for`] e [`Store::rubric_version`] sono provenienza e
//!   strumento di valutazione, e sono del docente del corso;
//! * [`Store::cohort_signals`] è un **aggregato anonimo** oltre soglia (D9), e
//!   resta senza predicato per una ragola dichiarata: non contiene persone, e il
//!   dato individuale da cui viene non si legge da lì.

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

/// La relazione che contiene la superficie **non assistita**, e la sola.
///
/// È una vista, e la vista è definita in `V6__unaided.sql` con il suo
/// `WHERE unaided = 1`. Qui dentro non c'è la frase: c'è il nome. Il motivo è
/// che la frase può essere scritta due volte e le due copie divergono — e quando
/// divergono, la divergenza è nel numero che un docente legge e nessuno vede
/// perché. Un nome di relazione è una cosa sola, e il posto in cui la definizione
/// sta è quello che ne fa la garanzia.
///
/// Questo nome entra anche in `count_failing`: il segnale di coorte di D9 conta
/// le osservazioni non assistite, perché «dove cade la classe» è una domanda su
/// che cosa gli studenti sanno fare senza aiuto. Le due letture — la del
/// studente e quella del docente — hanno la stessa fonte, ed è per questo che
/// non possono discordare.
///
/// `pub(crate)` e non `pub`: la vista si usa anche da altri moduli di questo
/// crate — il calendario di `crate::calendario` deve prendere le stesse righe
/// da qui, perché tre lettori che le prendono da tre posti sono tre misure
/// della stessa cosa con tre denominatori — ma il nome non esce dal crate:
/// fuori, il percorso pubblico è una funzione che ha già applicato il
/// predicato.
pub(crate) const UNAIDED_OBSERVATIONS: &str = "unaided_observations";

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
            unaided: draft.unaided,
            n_hints: draft.n_hints,
            at: draft.at,
        };
        let inserted = self.conn.execute(
            "INSERT INTO observations (id, session_id, seq, student, course_id, cohort, argument_id, \
                                        evidence, evidence_payload, judged_by, at, unaided, n_hints) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                // `Option<bool>` e `Option<u32>` viaggiano come sono: `NULL` su
                // `unaided` è «non registrato» e non uno `0`, e la traduzione
                // qui dentro sarebbe una decisione che nessuno ha chiesto. Il
                // `CHECK` della colonna e il trigger che vieta il conteggio
                // senza disponibilità sono in `V6__unaided.sql`.
                observation.unaided.map(|u| u as i64),
                observation.n_hints.map(|n| n as i64),
            ],
        ).map_err(|e| classify("osservazione", &observation.id, e))?;
        debug_assert_eq!(inserted, 1);
        Ok(observation)
    }

    /// Le osservazioni di una sessione, nell'ordine che la catena di hash ordina.
    ///
    /// È il substrate del replay: senza l'ordine, «riprodurre la generazione» e
    /// «riprodurre la sequenza» non sono la stessa cosa.
    ///
    /// **La domanda ha una persona, e la persona ha una relazione.** Una
    /// sessione attraversa corsi e studenti: restituisce `student`, `course_id`,
    /// `cohort`, `evidence` e `judged_by` di tutti. Senza predicato questa firma
    /// era un canale per leggere il registro delle dimostrazazioni di un istituto
    /// con un id — `ses_<registro>_<millisecondi>_<lunghezza della nota>` — che si
    /// enumera da soli. `SessionId` è un newtype trasparente su `String` e arriva
    /// da un corpo JSON: enumerare gli id non richiede nessun permesso.
    ///
    /// Il predicato è **in tutto o in niente**, e la ragione sta nella destinazione
    /// di questi dati: le righe servono a `kbs_verify::leaf_of` e `kbs_verify::Chain`,
    /// e una catena costruita su un sottoinsieme di righe non è una catena
    /// corteggiata: è una cat vera e con un buco, che non attesta niente. Quindi
    /// chi chiede deve poter leggere **tutte** le righe della sessione, e l'unico
    /// rapporto che le copre tutte è insegnare ogni corso in cui la sessione ha
    /// scritto. Chi non lo fa riceve `NotReadable`.
    ///
    /// Il limite che resta, dichiarato perché è vero: il rifiuto dice che una
    /// sessione ha delle righe, e l'id contiene un millisecondo. Non dice chi sono
    /// gli studenti, di che corso è, o che cosa hanno dimostrato, e non lo
    /// distingue da una sessione inesistente o vuota — quelle danno `[]`.
    ///
    /// **Qui si legge la tabella, non la vista `unaided_observations`.** La
    /// catena di hash copre tutte le righe della sessione, comprese quelle
    /// assistite: una catena costruita sul solo sottoinsieme non assistito non è
    /// una catena corteggiata, è una catena vera e con un buco, e il buco
    /// starebbe proprio dove un riscrittore ci metterebbe una riga. La regola
    /// «lo studente vede solo le non assistite» vale per la **sua** lettura del
    /// registro, e questa strada non è la sua: qui il predicato chiede `teaches`
    /// e nient'altro.
    pub fn observations_in_session(
        &self,
        person: &PersonId,
        session: &SessionId,
    ) -> Result<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, student, course_id, cohort, argument_id, evidence, evidence_payload, \
                    judged_by, at, unaided, n_hints \
               FROM observations WHERE session_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([&session.0], map_observation)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        let corsi: Vec<CourseId> = {
            let mut visti = Vec::new();
            for riga in &out {
                if !visti.contains(&riga.course) {
                    visti.push(riga.course.clone());
                }
            }
            visti
        };
        for corso in corsi {
            if !self
                .relations_of(person, &corso)?
                .contains(&Relation::Teaches)
            {
                return Err(Error::NotReadable {
                    person: person.clone(),
                    id: ArgumentId(format!("sessione:{}", session.0)),
                });
            }
        }
        Ok(out)
    }

    /// Le osservazioni su un argomento, viste da chi ha diritto di vederle.
    ///
    /// **Chi non ha diritto riceve `NotReadable`, non un elenco vuoto:** un elenco
    /// vuoto è una risposta vera, e qui non lo sarebbe.
    ///
    /// **E chi è lo studente vede solo le non assistite.** Non è un filtro che
    /// questa funzione si mette sopra i risultati: è la **relazione** da cui
    /// legge. La vista `unaided_observations` è definita in `V6__unaided.sql` e
    /// contiene il `WHERE unaided = 1`; qui sotto c'è il nome di una vista, non la
    /// frase che la definisce. Un percorso di lettura nuovo che sbaglia la
    /// relazione sbaglia un identificatore che si legge, non una clausola `WHERE`
    /// che si duplica e che un giorno può divergere da quella nello schema.
    ///
    /// Una riga con `unaided` **ignoto** non esce dalla vista, e per la logica a
    /// tre valori: `NULL = 1` non è vero. Non è una scelta di questo file, ed è
    /// il motivo per cui la colonna non ha un `DEFAULT 1` — vedi la migrazione.
    ///
    /// Chi insegna legge dalla tabella: la coda di practice è sua, e gli
    /// aggregati di classe sono costruiti sulle stesse righe. **Chi insegna ha la
    /// precedenza anche quando il registro è il suo** — un docente che guarda il
    /// proprio registro non è uno studente che guarda il proprio registro, e il
    /// predicato che lo distingue è `teaches`, che è una relazione come le altre
    /// (D5) e non un ruolo.
    ///
    /// `SoloMio` qui non capita e, se capita, è un `NotReadable`. Il doc sopra lo
    /// dichiara: lo scope del registro delle dimostrazioni è «lo studente o chi
    /// insegna», e un emittente di giudizi non ne fa parte. Non è una restrizione
    /// nuova — è il predicato che questa funzione già dichiarava e che il
    /// chiamante buttava via; e senza, un pari che ha emesso un giudizio
    /// leggerebbe anche la coda di practice, che è la riga che la vista serve a
    /// togliere.
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
        // leggerla per questa via.
        if let RegisterScope::SoloMio(_) =
            self.register_scope(person, &unit.course, student, None)?
        {
            return Err(Error::NotReadable {
                person: person.clone(),
                id: ArgumentId(format!("registro:{student}")),
            });
        }
        // Una relazione in più rispetto a `register_scope`, che la chiede
        // anch'essa: la vista dipende da *quale* dei due diritti ha aperto la
        // strada, e `Tutto` non distingue «sono lo studente» da «insegno il
        // corso». Sono poche righe su un corso, e la alternativa — una variante
        // nuova di `RegisterScope` — cambierebbe i due lettori del registro dei
        // giudizi per una domanda che è solo di questo.
        let insegna = self
            .relations_of(person, &unit.course)?
            .contains(&Relation::Teaches);
        let relazione = if insegna {
            "observations"
        } else {
            UNAIDED_OBSERVATIONS
        };
        let sql = format!(
            "SELECT id, seq, student, course_id, cohort, argument_id, evidence, evidence_payload, \
                    judged_by, at, unaided, n_hints \
               FROM {relazione} WHERE argument_id = ?1 AND student = ?2 ORDER BY id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
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
    ///
    /// Chi ha emesso un giudizio vede **il proprio** e nient'altro: le due
    /// regole sono distinte e il doc del modulo dice perché.
    pub fn gradings_for(
        &self,
        person: &PersonId,
        student: &PersonId,
        argument: &ArgumentId,
    ) -> Result<Vec<Grading>> {
        let unit = self.read_argument(person, argument)?;
        let scope = self.register_scope(person, &unit.course, student, Some(argument))?;
        self.gradings_of(student, &unit.course, Some(argument), &scope)
    }

    /// I giudizi di uno studente su un corso, tutti gli argomenti, contestazione
    /// inclusa.
    pub fn student_gradings(
        &self,
        person: &PersonId,
        student: &PersonId,
        course: &CourseId,
    ) -> Result<Vec<Grading>> {
        let scope = self.register_scope(person, course, student, None)?;
        self.gradings_of(student, course, None, &scope)
    }

    /// Quanto del registro di uno studente può leggere `person`.
    ///
    /// Tre esiti e sono tutti dichiarati: tutto, il proprio, niente. La
    /// distinzione fra «tutto» e «il proprio» è la riga che questo modulo aveva
    /// scritto in modo da coprire due diritti con una relazione sola, e la
    /// differenza si vede solo da chi ha emesso un giudizio: per lo studente e per
    /// il docente i due esiti coincidono, ed è per questo che il difetto è
    /// passato in mezzo a test verdi.
    fn register_scope(
        &self,
        person: &PersonId,
        course: &CourseId,
        student: &PersonId,
        argument: Option<&ArgumentId>,
    ) -> Result<RegisterScope> {
        if person == student {
            return Ok(RegisterScope::Tutto);
        }
        if self.relations_of(person, course)?.contains(&Relation::Teaches) {
            return Ok(RegisterScope::Tutto);
        }
        if self
            .graders(student, course, argument)?
            .iter()
            .any(|emittente| emittente == person)
        {
            return Ok(RegisterScope::SoloMio(person.clone()));
        }
        Err(Error::NotReadable {
            person: person.clone(),
            id: ArgumentId(format!("registro:{student}")),
        })
    }

    /// Chi ha emesso almeno un giudizio su questi argomenti di questo studente.
    ///
    /// Serve a `register_scope`, ed è una `SELECT DISTINCT` e non una
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

    /// Le righe che il chiamante ha diritto di vedere, e non una di più.
    ///
    /// Il filtro su `graded_by` è ciò che tiene separati i due diritti: senza di
    /// esso la `SELECT` restituirebbe il registro intero a chi ne ha diritto per
    /// una riga sola, ed è esattamente il difetto che questo modulo si portava
    /// dietro mentre un test verde diceva il contrario.
    fn gradings_of(
        &self,
        student: &PersonId,
        course: &CourseId,
        argument: Option<&ArgumentId>,
        scope: &RegisterScope,
    ) -> Result<Vec<Grading>> {
        let solo = match scope {
            RegisterScope::Tutto => None,
            RegisterScope::SoloMio(mio) => Some(mio),
        };
        // I parametri si accodano nello stesso ordine in cui i segnaposto
        // compaiono nella `WHERE`: `?3` è l'argomento quando c'è, `?4` è
        // l'emittente quando il chiamante ha diritto solo al proprio giudizio.
        let (per_argomento, per_emittente) = match (argument.is_some(), solo.is_some()) {
            (true, true) => (" AND argument_id = ?3", " AND graded_by = ?4"),
            (true, false) => (" AND argument_id = ?3", ""),
            (false, true) => ("", " AND graded_by = ?3"),
            (false, false) => ("", ""),
        };
        let sql = format!(
            "SELECT id, seq, student, course_id, argument_id, kind, graded_by, rubric_version, \
                    grade, at, contested_by, contested_at, contested_reason, contested_outcome \
               FROM gradings WHERE student = ?1 AND course_id = ?2{per_argomento}{per_emittente} \
              ORDER BY at, id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&student.0, &course.0];
        if let Some(id) = argument {
            params.push(&id.0);
        }
        if let Some(mio) = solo {
            params.push(&mio.0);
        }
        let rows = stmt.query_map(params.as_slice(), map_grading)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// La rubrica, con la sua scala: senza questo un voto è una stringa.
    ///
    /// **È del docente del corso, e la decisione è dichiarata.** Una rubrica non è
    /// un argomento: non ha uno stato, non si ratifica, e non c'è nessun predicato
    /// di `kbs-core` che la nomini. Applicarle `may_read` sarebbe una finzione.
    /// Quello che la rende sensibile è un'altra cosa, ed è dichiarata qui: la scala
    /// è lo **strumento** con cui il docente misura, e chi misura decide che cosa
    /// misura. È la stessa relazione che rende l'esportazione del docente
    /// (`Error::NotACourseTeacher`) e non di chiunque: chi non insegna non ha
    /// competenza sul corso di cui è strumento la valutazione.
    ///
    /// **L'id che non esiste e la rubrica che non è tua danno lo stesso
    /// errore**, come in [`Store::read_argument`], e per la stessa ragione: un
    /// `Option` qui distinguerebbe le due ipotesi e le trasformerebbe in un
    /// canale per imparare che cosa c'è in una scuola. Il tipo che torna è
    /// quindi la rubrica, non `Option<rubrica>`: un valore che può essere assente
    /// per due ragioni diverse non dovrebbe avere una forma che ne sceglie una.
    pub fn rubric_version(
        &self,
        person: &PersonId,
        id: &str,
    ) -> Result<crate::types::RubricVersion> {
        let rifiuta = || Error::NotReadable {
            person: person.clone(),
            id: ArgumentId(format!("rubrica:{id}")),
        };
        let row: Option<(String, String, String, String, i64, String, String)> = self
            .conn
            .query_row(
                "SELECT v.id, v.rubric_id, v.version, v.scale, v.defined_at, v.note, r.course_id \
                   FROM rubric_versions v JOIN rubrics r ON r.id = v.rubric_id \
                  WHERE v.id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, rubric, version, scale, defined_at, note, corso)) = row else {
            return Err(rifiuta());
        };
        if !self
            .relations_of(person, &CourseId(corso))?
            .contains(&Relation::Teaches)
        {
            return Err(rifiuta());
        }
        let scale: Vec<crate::types::GradeLevel> =
            serde_json::from_str(&scale).map_err(|e| Error::Corrupt {
                table: "rubric_versions",
                field: "scale",
                reason: e.to_string(),
            })?;
        Ok(crate::types::RubricVersion {
            id,
            rubric,
            version,
            scale,
            defined_at: Millis(defined_at),
            note,
        })
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

}

/// Quanto del registro di uno studente è leggibile, e da chi.
///
/// Le due varianti non sono un dettaglio dell'implementazione: sono le due
/// righe della regola dichiarata nel doc del modulo, e tenerle in un tipo
/// rende impossibile la confusione fra le due. Un `bool` — «può leggerlo?» —
/// avrebbe reso indistinguibili «può leggerlo tutto» da «può rileggere il
/// giudizio che ha emesso», che è il difetto che questo modulo si portava dietro.
enum RegisterScope {
    /// Lo studente di cui è la riga, e chi insegna il corso.
    Tutto,
    /// Solo i giudizi emessi da chi chiede, con la contestazione che li riguarda.
    SoloMio(PersonId),
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

    /// Legge un esercizio, checker incluso, per chi ne ha diritto.
    ///
    /// **Il checker è la risposta.** In tre delle quattro forme di
    /// `kbs_core::Checker` lo è tutta: `Set { elements }` è l'insieme delle
    /// risposte corrette, `MultipleChoice { correct_index, options }` è l'indice
    /// dell'esatto più le opzioni, `Equivalence { normalized }` è la forma
    /// normale dichiarata. D8 promette che «l'integrità è per costruzione, non per
    /// sorveglianza», e il meccanismo è che la risposta **non è nel materiale che lo
    /// studente vede**. Il materiale è separato, la risposta era a un metodo
    /// pubblico di distanza, e la firma non aveva dove mettere una persona: un
    /// `@login` non chiude un buco che non ha un soggetto.
    ///
    /// Perciò la firma prende **chi chiede** e **di quale corso**, e il predicato è
    /// in due tempi:
    ///
    /// 1. chi chiede **insegna** il corso. Non basta poter vedere l'argomento: uno
    ///    studente iscritto vede un argomento in corso, e vederlo non deve
    ///    consegnargli la risposta del suo esercizio. Il lato del generatore e
    ///    del verificatore è il lato del docente, ed è lì che la promessa di D8 si
    ///    tiene in piedi;
    /// 2. l'argomento che porta l'esercizio passa da `read_argument`, cioè dal
    ///    predicato di `kbs-core`: la bozza del docente resta sua.
    ///
    /// Il corso dichiarato dal chiamante viene confrontato con quello della riga:
    /// un `course_id` che non combacia è la stessa risposta di un esercizio che non
    /// si può leggere, perché il chiamante che sbaglia il corso non deve poterlo
    /// sapere dai diversi.
    ///
    /// Il rifiuto è `NotReadable` e non `None`: `None` è una risposta vera — «non
    /// c'è nessun esercizio con questo id» — e qui non lo sarebbe, perché
    /// l'esercizio c'è e qualcun altro lo può leggere. `Option` sparisce per
    /// questa firma e con lei la possibilità di confondere le due risposte.
    pub fn exercise(
        &self,
        person: &PersonId,
        course: &CourseId,
        id: &str,
    ) -> Result<kbs_core::Exercise> {
        let rifiuta = || Error::NotReadable {
            person: person.clone(),
            id: ArgumentId(format!("esercizio:{id}")),
        };
        let row: Option<(String, String, String, String, String, String, String, String, i64, String)> = self
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
        let Some((id, course_row, argument, family, generator, prompt, checker, payload, created_at, created_by)) = row else {
            return Err(rifiuta());
        };
        if &course_row != &course.0 {
            return Err(rifiuta());
        }
        self.check_generator_reader(person, course, &ArgumentId(argument.clone()))?;
        Ok(kbs_core::Exercise {
            id,
            course: CourseId(course_row),
            argument: ArgumentId(argument),
            family,
            generator_version: generator,
            prompt,
            checker: codec::checker_from_db(&checker, &payload)?,
            created_at: Millis(created_at),
            created_by: PersonId(created_by),
        })
    }

    /// Il predicato del lato generatore: insegnare il corso, e vedere l'argomento.
    ///
    /// È il metodo che rende vero il doc di [`Store::exercise`] e di
    /// [`Store::instances_of`], e sta in un posto solo perché due letture con la
    /// stessa promessa e due implementazioni diverse sono due bug che aspetta
    /// solo che qualcuno tocchi una delle due.
    fn check_generator_reader(
        &self,
        person: &PersonId,
        course: &CourseId,
        argument: &ArgumentId,
    ) -> Result<()> {
        self.read_argument(person, argument)?;
        if !self
            .relations_of(person, course)?
            .contains(&Relation::Teaches)
        {
            return Err(Error::NotReadable {
                person: person.clone(),
                id: argument.clone(),
            });
        }
        Ok(())
    }

    /// Il predicato del lato che **scrive** un esercizio.
    ///
    /// È [`Self::check_generator_reader`] con un nome che dice il verbo, e la
    /// ragione per cui è un metodo e non una ripetizione è che la promessa è
    /// una sola: **insegna il corso e vede l'argomento**. Il lato che scrive
    /// un esercizio scrive anche il suo checker, e il checker *è* la risposta,
    /// quindi il lato che lo scrive è per costruzione il lato che lo può
    /// leggere. Una strada che scrivesse da parte di uno studente produrrebbe
    /// righe che nessuno studente può rivedere e che il docente non ha
    /// ispezionato: il peggiore dei due, e una riga che non saprebbe nessuno
    /// di dover guardare.
    ///
    /// Il rifiuto è lo stesso di [`Self::exercise`] e lo stesso di
    /// [`Self::instances_of`], quindi la risposta è la stessa **anche** quando
    /// l'esercizio non esiste: chi chiede non impara nulla dal distinguere i
    /// due casi, e `NotReadable` resta l'unico errore di visibilità di questo
    /// crate.
    pub fn may_author(
        &self,
        person: &PersonId,
        course: &CourseId,
        argument: &ArgumentId,
    ) -> Result<()> {
        self.check_generator_reader(person, course, argument)
    }

    /// Salva un'istanza. Il `UNIQUE (exercise, seed)` è la costruzione che rende
    /// la copia inefficace: stessa famiglia, seed diverse, risposte diverse.
    ///
    /// **L'id è derivato da `(exercise, seed)`**, e non è un campo di
    /// `kbs_core::Instance`: un'istanza è una coppia, e la tabella ne ha una
    /// `PRIMARY KEY` propria che il tipo non porta. Il `ON CONFLICT (exercise,
    /// seed) DO NOTHING` resta la semantica dichiarata — la stessa istanza non si
    /// riscrive — e l'id che ne deriva è la stessa cosa, quindi i due vincoli non
    /// possono contraddirsi.
    ///
    /// Qui c'era un `INSERT` a sei segnaposto con cinque parametri: la colonna
    /// `id` non era mai legata, e **nessuna istanza è mai entrata** in questo
    /// database. È il difetto che un test assente lascia in piedi più a lungo di
    /// tutti gli altri, perché la firma sembrava già scritta.
    pub fn put_instance(&mut self, i: &kbs_core::Instance) -> Result<()> {
        let params = serde_json::to_string(&i.params).map_err(|e| Error::InvalidField {
            field: "params",
            reason: e.to_string(),
        })?;
        let id = format!("{}#{}", i.exercise, i.seed);
        self.conn.execute(
            "INSERT INTO instances (id, exercise, seed, rendered_prompt, expected, params) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (exercise, seed) DO NOTHING",
            rusqlite::params![id, i.exercise, i.seed, i.rendered_prompt, i.expected, params],
        )?;
        Ok(())
    }

    /// Le istanze di un esercizio, in ordine di seed: ordinarle è ciò che rende
    /// confrontabili due istanze della stessa famiglia.
    ///
    /// **`Instance::expected` è la risposta**, ed è per questo che la firma è
    /// quella di [`Store::exercise`] e non una firma più permissiva: lo stesso
    /// predicato, lo stesso soggetto, la stessa ragione. Il predicato sta dentro
    /// `exercise`, che questa funzione chiama per prima: è l'unico modo che
    /// l'esercizio inesistente e l'esercizio non leggibile diano la stessa
    /// risposta, ed è la stessa ragione per cui `exercise` non torna `Option`.
    pub fn instances_of(
        &self,
        person: &PersonId,
        course: &CourseId,
        exercise: &str,
    ) -> Result<Vec<kbs_core::Instance>> {
        self.exercise(person, course, exercise)?;
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
    ///
    /// **Questa era una delle letture senza predicato, e la decisione è di
    /// chiuderla.** Un `model lock` è `model_id`, `prompt_hash`, `corpus_hash`,
    /// `generator_version`, chi l'ha chiesto e quando: per un argomento di un
    /// corso in cui non si insegna, e in bozza come in corso. Un autore che
    /// leggesse la provenienza altrui imparerebbe quale modello genera il
    /// materiale della scuola concorrente, con quale prompt e su quale corpus — e
    /// il `corpus_hash` è l'hash di quello che la scuola non pubblica. Non è un
    /// documento segreto, ma è materiale di un altro perimetro di condivisione, e
    /// `CourseId` è il perimetro.
    ///
    /// Quindi la firma prende la persona e la provenienza si legge **solo** di un
    /// argomento che quella persona può leggere: `read_argument` è il predicato, e
    /// non ne esiste un secondo più permissivo.
    pub fn generations_for(
        &self,
        person: &PersonId,
        argument: &ArgumentId,
    ) -> Result<Vec<crate::types::GenerationEvent>> {
        self.read_argument(person, argument)?;
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
    /// Registra un segnale di coorte, dopo averlo confrontato con le righe da cui
    /// viene.
    ///
    /// **`failing` è un numero di persone, e il numero lo conta il registro.**
    ///
    /// D9 chiede un conteggio di **persone** che sbagliano un argomento a livello
    /// di classe. Il numero arrivava dal chiamante e non era confrontato con
    /// nessuna riga: la soglia era chiusa e il `CHECK` del database pure, ma la
    /// `UNIQUE` è su `(session_id, seq)`, quindi uno studente con cinque tentativi
    /// su un argomento era conforme, e il segnale che annunciava alla classe «cinque
    /// studenti sbagliano questo argomento» era il lavoro di uno. Nessuna delle due
    /// barriere poteva accorgersene, perché tutte e due contavano un intero
    /// ricevuto.
    ///
    /// Qui il numero si riconta dalle osservazioni e il valore dichiarato si
    /// confronta: se non tornano, l'errore è [`Error::CohortCountMismatch`] e dice
    /// entrambi i numeri. Non si sceglie silenziosamente quale dei due è vero,
    /// perché «silenzio» su un campo compilato è il modo più veloce per insegnare
    /// al chiamante che quel campo non esiste — la regola che questo crate segue già
    /// per la ratifica in `upsert_argument`.
    ///
    /// **Cosa conta come «in errore»**, dichiarato perché è una decisione e non un
    /// dettaglio: una dimostrazione il cui verificatore deterministice ha detto
    /// `correct: false`. È l'unica cosa che il registro sa contare da sé senza
    /// giudicare: un voto di pari o un'interrogazione orale non dicono «sbagliato»,
    /// e dichiarare che lo dicono sarebbe inventare un verdetto. `total` è il
    /// numero di persone che hanno una riga su quell'argomento, e la relazione
    /// `failing <= total` è garantita per costruzione invece che dal `CHECK` che
    /// prima la presidiava.
    ///
    /// **Sotto soglia l'errore è `CohortBelowThreshold` e la riga non entra.**
    /// Non è una cancellazione: le osservazioni individuali da cui il segnale
    /// viene calcolato restano nel registro, e sono loro che si contano. La
    /// soglia è un accesso all'aggregato, non una scomparsa del dato.
    ///
    /// **Conta solo le osservazioni non assistite.** Il numero che il registro
    /// riconta è preso dalla vista `unaided_observations`, quindi un tentativo
    /// assistito non entra in `failing` né in `total`: «dove cade la classe» si
    /// misura su che cosa gli studenti fanno senza aiuto. Un docente che
    /// dichiara i numeri di prima — quando il segnale conteneva anche la coda di
    /// practice — riceve `CohortCountMismatch` con **entrambi** i numeri, non un
    /// silenzio: su un campo compilato il silenzio è il modo più veloce per
    /// insegnare al chiamante che quel campo non esiste.
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
        let (failing, total) = self.count_failing(&signal.course, &signal.cohort, &signal.argument)?;
        if (signal.failing, signal.total) != (failing, total) {
            return Err(Error::CohortCountMismatch {
                declared_failing: signal.failing,
                declared_total: signal.total,
                counted_failing: failing,
                counted_total: total,
            });
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
                failing as i64,
                total as i64,
                signal.at.0,
            ],
        )?;
        Ok(())
    }

    /// Quante **persone** sbagliano, e quante hanno prodotto qualcosa, su quell'
    /// argomento in quel corso e in quella classe.
    ///
    /// `COUNT(DISTINCT student)`, non `COUNT(*)`: è la differenza fra un dato
    /// sulle persone e un dato sui tentativi, ed è tutta la differenza che D9
    /// chiede. La definizione di «in errore» è nel doc di
    /// [`Store::record_cohort_signal`] e sta qui dentro per non averne due.
    ///
    /// **Conta la vista `unaided_observations`, non la tabella**, ed è la stessa
    /// relazione da cui lo studente legge il proprio registro: il numeratore di
    /// D9 e la superficie dello studente prendono le righe dallo stesso posto e
    /// non possono discordare. Una riga con `unaided` ignoto non conta, e non
    /// per una scelta di questo file: la vista non la contiene, e il motivo è
    /// nella migrazione che la definisce.
    fn count_failing(
        &self,
        course: &CourseId,
        cohort: &CohortId,
        argument: &ArgumentId,
    ) -> Result<(usize, usize)> {
        let (failing, total): (i64, i64) = self.conn.query_row(
            &format!(
                "SELECT COUNT(DISTINCT CASE WHEN evidence = 'checked' \
                          AND json_extract(evidence_payload, '$.correct') = 0 \
                         THEN student END), \
                       COUNT(DISTINCT student) \
                  FROM {UNAIDED_OBSERVATIONS} \
                 WHERE course_id = ?1 AND cohort = ?2 AND argument_id = ?3"
            ),
            rusqlite::params![course.0, cohort.0, argument.0],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((failing as usize, total as usize))
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

    // `unaided` e `n_hints` stanno in coda, e sono gli unici due campi della
    // riga che possono valere `NULL` **per un fatto, non per una lacuna**: la
    // colonna esiste e la cella è vuota perché nessuno l'ha compilata. Un
    // `bool` qui farebbe di `0` e di «assente» la stessa cosa, che è la
    // confusione che la migrazione `V6` dichiara di non fare.
    let unaided: Option<i64> = row.get(10)?;
    let n_hints: Option<i64> = row.get(11)?;

    let evidence =
        codec::evidence_from_db(&evidence_raw, evidence_payload).map_err(|e| corrupt(6, e))?;
    let judged_by = match judged_by {
        None => None,
        Some(raw) => Some(codec::grader_from_db(&raw).map_err(|e| corrupt(8, e))?),
    };
    // Il `CHECK` della colonna è il motivo per cui qui non c'è un terzo braccio
    // per un valore che non è `0` e non è `1`: quel valore non può arrivare dal
    // database senza che il `CHECK` sia stato tolto, che è una rimozione che si
    // vede nello schema. La conversione non inventa nulla: è la stessa
    // codifica che `append_observation` scrive e la stessa che ne rilegge.
    let unaided = unaided.map(|v| v == 1);
    let n_hints = n_hints.map(|v| v as u32);

    Ok(Observation {
        id,
        seq: SeqInSession(seq as u64),
        student: PersonId(student),
        course: CourseId(course),
        cohort: CohortId(cohort),
        argument: ArgumentId(argument),
        evidence,
        judged_by,
        unaided,
        n_hints,
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
