//! `Store`: la connessione, e le operazioni che sono operazioni.
//!
//! Qui stanno le tre cose che non sono tabelle: la **visibilità**, che è una
//! funzione e non una configurazione (D5); la **porta sul percorso condiviso**,
//! che ha un solo nome (`publish`); e l'**integrità del grafo dei prerequisiti**.
//!
//! # Le letture non filtrate sono private
//!
//! `argument_unrestricted` e `list_by_state` sono `pub(crate)`. Non è una
//! questione di stile: se l'API pubblica avesse «vedi tutto», ogni chiamante
//! dimenticherebbe di passare da `may_read` prima o poi, e una sola dimenticanza
//! basta. L'unico modo per non sbagliare è che la strada senza predicato non
//! esista. [`Store::conn`] è l'eccezione dichiarata.
//!
//! # La copertura del predicato
//!
//! Ogni metodo pubblico che restituisce materiale a una **persona** passa da
//! [`kbs_core::may_read`], senza eccezioni. Non è una promessa: le funzioni di
//! lettura non ristretta sono `pub(crate)`, e l'unico modo pubblico di ottenerle
//! è `conn()`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use kbs_core::{
    may_read, Argument, ArgumentId, CourseId, Invariant, ModelLock, Origin, PersonId,
    PublicationState, Relation,
};
use rusqlite::{types::Type, Connection, OptionalExtension, Row, ToSql};

use crate::codec;
use crate::error::{Error, Result};
use crate::types::{CourseRelation, Person, Source, SourceStatus};
use crate::schema;

/// Le colonne di un argomento, più il lock per la provenienza generata.
///
/// Una sola `SELECT` con `LEFT JOIN` invece di due: la provenienza è parte
/// dell'argomento, e un join mancante è un argomento che si legge a metà.
const ARGUMENT_SELECT: &str = "\
    SELECT a.id, a.course_id, a.title, a.summary, a.state, a.rel_path, a.content_hash, \
           a.origin_kind, a.origin_by, a.origin_at, a.origin_lock, a.origin_from, \
           a.created_at, a.updated_at, \
           a.ratified_by, a.ratified_at, a.ratified_contract_hash, a.ratified_note, \
           m.model_id, m.prompt_hash, m.corpus_hash, m.generator_version, m.at \
      FROM arguments a \
      LEFT JOIN model_locks m ON m.id = a.origin_lock";

pub struct Store {
    /// `pub(crate)` e non pubblico: i moduli fratelli (`publish`, `registers`,
    /// `search`, `export`) devono scrivere, e fuori dal crate l'unica porta è
    /// [`Store::conn`], che è dichiarata come tale.
    pub(crate) conn: Connection,
}

impl Store {
    /// Apre un database su file e porta lo schema all'epoch di questo binario.
    ///
    /// Fallisce con [`Error::SchemaFromTheFuture`] se il database è più nuovo:
    /// un binario vecchio che scrive sopra uno schema nuovo non fa un rollback,
    /// fa una corruzione silenziosa.
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        Ok(Store {
            conn: schema::open(path.as_ref())?,
        })
    }

    /// Apre un database in memoria, migrato come quello su file.
    pub fn open_in_memory() -> Result<Store> {
        Ok(Store {
            conn: schema::open_in_memory()?,
        })
    }

    /// La connessione grezza.
    ///
    /// **È una porta di servizio, e va trattata come tale.** Serve a
    /// `kbs-verify`, alle verifiche di integrità e ai test. Con questa si può
    /// scrivere una riga che nessun percorso tipizzato scriverebbe — per esempio
    /// un chunk senza indicizzarlo, che non è ricercabile finché non si chiama
    /// [`Store::reindex_all`](crate::search::Store::reindex_all).
    ///
    /// Ciò che *non* si può fare è riscrivere un registro per iscritto:
    /// `observations`, `gradings`, `claims`, `generations` e `model_locks` hanno
    /// trigger che rifiutano `UPDATE` e `DELETE`, e nessun percorso di questo
    /// crate li aggira. Non però un ripristino da backup, che è il secondo dei
    /// tre limiti dichiarati in questo crate e il primo che si cerca.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// L'epoch effettivo del database dopo le migrazioni.
    pub fn schema_epoch(&self) -> Result<i32> {
        Ok(self
            .conn
            .query_row("SELECT epoch FROM schema_epoch WHERE id = 1", [], |r| {
                r.get(0)
            })?)
    }

    // ── il corso, le persone, le relazioni ─────────────────────────────────

    /// Registra (o aggiorna) l'albero di un corso.
    pub fn register_source(&mut self, s: &Source) -> Result<()> {
        if s.slug.trim().is_empty() {
            return Err(Error::InvalidField {
                field: "slug",
                reason: "uno slug vuoto non è un nome".into(),
            });
        }
        self.conn.execute(
            "INSERT INTO sources (id, slug, rel_path, status, registered_at, last_scan_at, corpus_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT (id) DO UPDATE SET slug = ?2, rel_path = ?3, status = ?4, \
                                    last_scan_at = ?6, corpus_hash = ?7",
            rusqlite::params![
                s.id.0,
                s.slug,
                s.rel_path,
                s.status.as_str(),
                s.registered_at.0,
                s.last_scan_at.map(|t| t.0),
                s.corpus_hash,
            ],
        )?;
        Ok(())
    }

    /// Lo stato di un corso, se è registrato.
    pub fn source_status(&self, course: &CourseId) -> Result<Option<SourceStatus>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT status FROM sources WHERE id = ?1", [&course.0], |r| {
                r.get(0)
            })
            .optional()?;
        match raw {
            None => Ok(None),
            Some(s) => SourceStatus::from_db(&s).map(Some).ok_or_else(|| Error::Corrupt {
                table: "sources",
                field: "status",
                reason: format!("`{s}` non è uno stato noto"),
            }),
        }
    }

    /// Crea o aggiorna una persona. Nessun ruolo: D5.
    pub fn upsert_person(&mut self, p: &Person) -> Result<()> {
        if p.display_name.trim().is_empty() {
            return Err(Error::InvalidField {
                field: "display_name",
                reason: "una persona senza nome non si distingue".into(),
            });
        }
        self.conn.execute(
            "INSERT INTO people (id, display_name, created_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT (id) DO UPDATE SET display_name = ?2",
            rusqlite::params![p.id.0, p.display_name, p.created_at.0],
        )?;
        Ok(())
    }

    /// Aggiunge un'iscrizione o un incarico di insegnamento.
    ///
    /// Solo `enrolled_in` e `teaches` si possono scrivere: `author_of`,
    /// `ratified` e `speculative_for` sono fatti su un oggetto, non sul corso, e
    /// il predicato li deriva dagli argomenti. Rifiutarli qui è più onesto che
    /// accettarli e non farne nulla.
    pub fn add_relation(&mut self, r: &CourseRelation) -> Result<()> {
        if !matches!(r.relation, Relation::EnrolledIn | Relation::Teaches) {
            return Err(Error::InvalidField {
                field: "relation",
                reason: format!(
                    "`{:?}` non è una relazione di corso: si deriva dagli argomenti",
                    r.relation
                ),
            });
        }
        self.conn.execute(
            "INSERT INTO relations (person_id, course_id, relation, since, until) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                r.person.0,
                r.course.0,
                relation_to_db(r.relation),
                r.since.0,
                r.until.map(|t| t.0),
            ],
        )?;
        Ok(())
    }

    /// Chiude un'iscrizione o un incarico a una data. `until IS NULL` è
    /// l'iscrizione in corso.
    pub fn end_relation(
        &mut self,
        person: &PersonId,
        course: &CourseId,
        relation: Relation,
        at: kbs_core::Millis,
    ) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE relations SET until = ?4 \
             WHERE person_id = ?1 AND course_id = ?2 AND relation = ?3 AND until IS NULL",
            rusqlite::params![person.0, course.0, relation_to_db(relation), at.0],
        )?;
        if n == 0 {
            return Err(Error::NotFound {
                kind: "relazione",
                id: format!("{person}/{course}/{relation:?}"),
            });
        }
        Ok(n)
    }

    /// Le relazioni **di corso** che una persona ha verso un corso adesso.
    ///
    /// Non sono tutte: `author_of` e `ratified` sono per-argomento e li aggiunge
    /// [`Visibility::for_argument`], perché derivarli qui richiederebbe di sapere
    /// di quale argomento si parla.
    pub fn relations_of(&self, person: &PersonId, course: &CourseId) -> Result<Vec<Relation>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT relation FROM relations \
             WHERE person_id = ?1 AND course_id = ?2 AND (until IS NULL OR until > ?3)",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![person.0, course.0, kbs_core::Millis::now().0],
            |r| r.get::<_, String>(0),
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(relation_from_db(&row?)?);
        }
        Ok(out)
    }

    // ── visibilità ─────────────────────────────────────────────────────────

    /// Le relazioni di `person` verso `course`, più gli argomenti di cui è
    /// autore. È il materiale con cui si chiama [`may_read`].
    pub(crate) fn visibility(&self, person: &PersonId, course: &CourseId) -> Result<Visibility> {
        Ok(Visibility {
            person: person.clone(),
            relations: self.relations_of(person, course)?,
            authored: self.attributed_authors(course, None)?,
        })
    }

    /// Gli argomenti di cui `person` è autore, per argomento.
    ///
    /// Un argomento derivato non ha un autore proprio: `kbs_core::Origin::Derived`
    /// non ha il campo `by`, e riempirlo significherebbe inventare un autore.
    /// Quindi l'attribuzione risale lungo la catena delle derivazioni, e chi ha
    /// scritto la sorgente è autore di ciò che da essa deriva. Senza quella
    /// risalita un argomento derivato non ha autore di nessuno, e il predicato lo
    /// rende invisibile **anche a chi lo ha scritto**: un autore che non può
    /// rivedere ciò che ha derivato è un difetto, non una severità.
    ///
    /// La query ha due parti e servono entrambe. La prima è l'attribuzione
    /// diretta, quella che c'è in `origin_by`. La seconda sale la catena: il
    /// seme è ogni argomento derivato del corso, e ad ogni passo l'attribuzione
    /// viaggia con la riga finché non incontra una fonte che ha un autore, a quel
    /// punto il cammino si ferma e la riga porta l'attribuzione di quella fonte.
    /// Il `CHECK` di `V2` vieta a un derivato di avere `origin_by`, quindi la
    /// seconda parte non è una ridondanza: è l'unica che riguarda i derivati.
    ///
    /// `UNION` e non `UNION ALL`: i duplicati sopprimono il ciclo, quindi un grafo
    /// di derivazioni ciclico non fa girare la query all'infinito.
    fn attributed_authors(
        &self,
        course: &CourseId,
        only: Option<&ArgumentId>,
    ) -> Result<HashMap<String, HashSet<String>>> {
        let mut stmt = self.conn.prepare(
            "WITH RECURSIVE walk(discendente, node, autore) AS ( \
                 SELECT a.id, a.id, NULL FROM arguments a \
                  WHERE a.course_id = ?1 AND a.origin_kind = 'derived' \
                 UNION \
                 SELECT w.discendente, w.node, p.origin_by \
                   FROM walk w \
                   JOIN arguments c ON c.id = w.node \
                   JOIN arguments p ON p.id = c.origin_from \
                  WHERE p.origin_by IS NOT NULL \
                 UNION \
                 SELECT w.discendente, p.id, w.autore \
                   FROM walk w \
                   JOIN arguments c ON c.id = w.node \
                   JOIN arguments p ON p.id = c.origin_from \
                  WHERE p.origin_kind = 'derived' \
             ) \
             SELECT DISTINCT a.id, a.origin_by FROM arguments a \
              WHERE a.course_id = ?1 AND a.origin_by IS NOT NULL \
                AND (?2 IS NULL OR a.id = ?2) \
             UNION \
             SELECT DISTINCT w.discendente, w.autore FROM walk w \
              WHERE w.autore IS NOT NULL AND (?2 IS NULL OR w.discendente = ?2)",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![course.0, only.map(|i| i.0.as_str())],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        for row in rows {
            let (argument, author) = row?;
            map.entry(argument).or_default().insert(author);
        }
        Ok(map)
    }

    /// Scrive un argomento, creandolo o aggiornandolo.
    ///
    /// **Lo stato non si scrive da qui.** Un argomento entra come bozza o come
    /// revisione del docente; da `in-corso` si arriva solo con [`Store::publish`],
    /// e con nient'altro. Scrivere `in-corso` qui renderebbe la ratifica
    /// decorativa, e un gate decorativo è peggio di nessun gate (D4).
    ///
    /// I prerequisiti sono riscritti per intero, e ognuno viene controllato
    /// contro il grafo **prima** di essere inserito: un ciclo si rifiuta con
    /// [`Invariant::PrerequisiteCycle`], che è l'errore giusto perché nomina la
    /// regola e non il sintomo.
    ///
    /// **La ratifica non si scrive qui.** `Argument` la contiene come campo, ma
    /// il campo non è la porta: la porta è [`Store::ratify`]. Se `upsert_argument`
    /// scrivesse `ratified_*`, chiunque potrebbe installare una precondizione di
    /// `publish` senza passare da `ratify`, e una precondizione che si può
    /// installare da due posti è una precondizione decorativa — che è la cosa che
    /// D4 vieta esplicitamente. Perciò su un aggiornamento le colonne di ratifica
    /// restano come sono, e su un inserimento una ratifica nel valore è un
    /// errore detto chiaramente, non un silenzio. Un valore di ratifica
    /// **identico** a quello memorizzato passa, perché altrimenti un
    /// `Argument` appena letto dal database non potrebbe più essere riscritto.
    ///
    /// **Il corso non si sposta, e non è una delicatezza dell'aggiornamento.**
    /// Su una riga che esiste già, un `course_id` diverso da quello memorizzato è
    /// [`Error::CourseReparent`]: l'id di un argomento è derivato dal percorso, il
    /// percorso contiene il corso, e `CourseId` è il perimetro di condivisione.
    /// Spostare la riga lascerebbe un argomento il cui nome dice una cosa e il cui
    /// percorso dice un'altra, e porterebbe dentro un corso la ratifica di chi
    /// insegnava l'altro. La strada per spostare del materiale è un argomento
    /// nuovo, e la `ON CONFLICT` qui sotto non tocca `course_id` neppure.
    pub fn upsert_argument(&mut self, a: &Argument) -> Result<()> {
        if a.title.trim().is_empty() {
            return Err(Error::InvalidField {
                field: "title",
                reason: "un argomento senza titolo non si insegna e non si cerca".into(),
            });
        }
        let origin = codec::origin_to_row(&a.origin)?;
        if let Origin::Generated { lock, .. } = &a.origin {
            self.put_lock(lock)?;
        }
        let (rby, rat, rhash, rnote) = match &a.ratified {
            None => (None, None, None, None),
            Some(r) => (Some(r.by.0.clone()), Some(r.at.0), Some(r.contract_hash.clone()), Some(r.note.clone())),
        };

        let tx = self.conn.transaction()?;
        // Una ratifica «passa» o non passa. Se il valore che il chiamante ha
        // compilato è identico a quello già memorizzato, `upsert_argument`
        // prosegue e non lo tocca: altrimenti un `Argument` appena letto dal
        // database non potrebbe più essere riscritto, e costringerebbe chi
        // scrive a svuotare il campo a mano. Se invece è diverso — o se la riga
        // non ha ancora nessuna ratifica — è un rifiuto: installare o cambiare
        // una precondizione di `publish` da una strada senza gate renderebbe il
        // gate decorativo, che è la cosa che D4 vieta. Silenzio no: ignorare in
        // silenzio un campo compilato è il modo più veloce per insegnare a
        // chiamante che quel campo non esiste.
        if let Some(r) = &a.ratified {
            // Le colonne sono NULL quando non c'è ratifica: leggerle come
            // `String` sarebbe un errore di tipo su una riga perfettamente
            // valida, cioè un panico per un caso che capita.
            let memorizzata: Option<(Option<String>, Option<i64>, Option<String>)> = tx
                .query_row(
                    "SELECT ratified_by, ratified_at, ratified_contract_hash \
                       FROM arguments WHERE id = ?1",
                    [&a.id.0],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let stessa = matches!(
                memorizzata,
                Some((Some(by), Some(at), Some(hash)))
                    if by == r.by.0 && at == r.at.0 && hash == r.contract_hash
            );
            if !stessa {
                return Err(Error::RatificationThroughUpsert { id: a.id.clone() });
            }
        }
        let corrente: Option<(PublicationState, String)> =
            match tx
                .query_row(
                    "SELECT state, course_id FROM arguments WHERE id = ?1",
                    [&a.id.0],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .optional()?
            {
                None => None,
                Some((stato, corso)) => Some((codec::state_from_db(&stato)?, corso)),
            };
        // **Il corso non si sposta.** Non è una scelta conservativa: è la
        // conseguenza di due cose che sono già vere. `ArgumentId` è derivato dal
        // percorso e il percorso contiene il corso, quindi un argomento riparentato
        // avrebbe un id che dice `corsi/<corso vecchio>/…` e una riga che dice
        // `course_…` dell'altro: un argomento che mente sul proprio nome, e
        // `rel_path` — la stessa cosa detta due volte, in due modi diversi. E
        // `CourseId` è «il perimetro di condivisione»: spostando la riga si
        // sposta anche la ratifica, che è la responsabilità di chi insegnava il
        // corso di prima, dentro un corso in cui non insegna nessuno.
        //
        // La correzione non è una transizione e non è un'`UPDATE` più permissiva:
        // è un argomento nuovo, col suo percorso, la sua ratifica e il suo
        // `content_hash`. È l'unica risposta in cui la ratifica resta un fatto
        // su un testo e non un timbro che ha cambiato mano.
        if let Some((_, corso)) = &corrente {
            if corso != &a.course.0 {
                return Err(Error::CourseReparent {
                    id: a.id.clone(),
                    from: CourseId(corso.clone()),
                    to: a.course.clone(),
                });
            }
        }
        let current = corrente.map(|(stato, _)| stato);
        let state = writable_state(&a.id, current, a.state)?;
        tx.execute(
            "INSERT INTO arguments ( \
                 id, course_id, title, summary, state, rel_path, content_hash, \
                 origin_kind, origin_by, origin_at, origin_lock, origin_from, \
                 created_at, updated_at, \
                 ratified_by, ratified_at, ratified_contract_hash, ratified_note) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18) \
            ON CONFLICT (id) DO UPDATE SET \
                 title = ?3, summary = ?4, state = ?5, rel_path = ?6, \
                 content_hash = ?7, \
                 origin_kind = ?8, origin_by = ?9, origin_at = ?10, origin_lock = ?11, \
                 origin_from = ?12, updated_at = ?14",
            rusqlite::params![
                a.id.0,
                a.course.0,
                a.title,
                a.summary,
                codec::state_to_db(state),
                a.rel_path,
                a.content_hash,
                origin.kind,
                origin.by,
                origin.at,
                origin.lock,
                origin.from,
                a.created_at.0,
                a.updated_at.0,
                rby,
                rat,
                rhash,
                rnote,
            ],
        )?;

        // I prerequisiti si azzerano e si riscrivono. Azzerare prima è ciò che
        // rende l'upsert idempotente: senza, il controllo anticiclo troverebbe
        // l'arco che si sta per riscrivere e rifiuterebbe un grafo valido.
        tx.execute(
            "DELETE FROM argument_prerequisites WHERE argument_id = ?1",
            [&a.id.0],
        )?;
        let mut seen = HashSet::new();
        for prerequisite in &a.prerequisites {
            if prerequisite == &a.id {
                return Err(Error::Invariant(Invariant::PrerequisiteCycle(
                    prerequisite.0.clone(),
                )));
            }
            if !seen.insert(prerequisite) {
                // Un prerequisito ripetuto è un difetto di chi costruisce
                // l'argomento, non della persistenza: si deduplica e si va avanti.
                continue;
            }
            if reaches(&tx, &a.id, prerequisite)? {
                return Err(Error::Invariant(Invariant::PrerequisiteCycle(
                    prerequisite.0.clone(),
                )));
            }
            tx.execute(
                "INSERT INTO argument_prerequisites (argument_id, prerequisite_id) VALUES (?1, ?2)",
                rusqlite::params![a.id.0, prerequisite.0],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Legge un argomento **senza** predicato. `pub(crate)`: vedi il doc del
    /// modulo.
    pub(crate) fn argument_unrestricted(&self, id: &ArgumentId) -> Result<Option<Argument>> {
        let sql = format!("{ARGUMENT_SELECT} WHERE a.id = ?1");
        let args = self.fetch_arguments(&sql, rusqlite::params![id.0])?;
        Ok(args.into_iter().next())
    }

    /// Elenca gli argomenti di un corso per stato, **senza** predicato.
    /// `pub(crate)`: vedi il doc del modulo.
    pub(crate) fn list_by_state(
        &self,
        course: &CourseId,
        state: PublicationState,
    ) -> Result<Vec<Argument>> {
        let sql = format!("{ARGUMENT_SELECT} WHERE a.course_id = ?1 AND a.state = ?2 ORDER BY a.id");
        self.fetch_arguments(
            &sql,
            rusqlite::params![course.0, codec::state_to_db(state)],
        )
    }

    /// Legge un argomento per una persona, applicando il predicato.
    ///
    /// Se l'argomento non esiste la risposta è la stessa che se non lo si vede:
    /// altrimenti «non esiste» e «non lo vedi» sarebbero due canali per imparare
    /// che cosa c'è nel corso, e la forma dell'id (`arg_<hash del percorso>`) è
    /// enumerabile da chiunque abbia il corpus.
    ///
    /// «la stessa risposta» vuol dire **gli stessi byte**: l'errore non porta lo
    /// stato della riga, perché un errore che dice in che stato è la cosa che non
    /// ti fa vedere è un oracolo. Vedi il doc di [`Error::NotReadable`].
    pub fn read_argument(&self, person: &PersonId, id: &ArgumentId) -> Result<Argument> {
        let not_readable = || Error::NotReadable {
            person: person.clone(),
            id: id.clone(),
        };
        let Some(argument) = self.argument_unrestricted(id)? else {
            return Err(not_readable());
        };
        let visibility = self.visibility(person, &argument.course)?;
        if !visibility.may_see(&argument) {
            return Err(not_readable());
        }
        Ok(argument)
    }

    /// Gli argomenti che `person` può vedere in un corso, opzionalmente
    /// ristretti a uno stato.
    ///
    /// Il filtro per stato e il predicato sono due cose diverse: il primo
    /// restringe un insieme che il secondo ha già autorizzato, e invertirli
    /// restituirebbe a uno studente le bozze del docente filtrate per stato
    /// `bozza`.
    pub fn visible_arguments(
        &self,
        person: &PersonId,
        course: &CourseId,
        state: Option<PublicationState>,
    ) -> Result<Vec<Argument>> {
        let candidates = match state {
            None => self.fetch_arguments(
                &format!("{ARGUMENT_SELECT} WHERE a.course_id = ?1 ORDER BY a.id"),
                rusqlite::params![course.0],
            )?,
            Some(s) => self.fetch_arguments(
                &format!("{ARGUMENT_SELECT} WHERE a.course_id = ?1 AND a.state = ?2 ORDER BY a.id"),
                rusqlite::params![course.0, codec::state_to_db(s)],
            )?,
        };
        let visibility = self.visibility(person, course)?;
        Ok(candidates
            .into_iter()
            .filter(|a| visibility.may_see(a))
            .collect())
    }

    /// Esegue una `SELECT` di argomenti e riattacca i prerequisiti.
    ///
    /// I prerequisiti si leggono in una seconda query **per selezione**, non
    /// globalmente: un argomentino letto da solo non deve portarsi dietro il
    /// grafo dell'istituto.
    fn fetch_arguments(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Argument>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, map_argument)?;
        let mut found: Vec<Argument> = Vec::new();
        for row in rows {
            found.push(row?);
        }
        if found.is_empty() {
            return Ok(found);
        }
        let mut placeholders = String::new();
        for i in 0..found.len() {
            if i > 0 {
                placeholders.push(',');
            }
            placeholders.push('?');
        }
        let sql = format!(
            "SELECT argument_id, prerequisite_id FROM argument_prerequisites \
             WHERE argument_id IN ({placeholders}) ORDER BY argument_id, prerequisite_id"
        );
        let ids: Vec<&dyn ToSql> = found.iter().map(|a| &a.id.0 as &dyn ToSql).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(ids.as_slice(), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut by_argument: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            let (argument, prerequisite) = row?;
            by_argument
                .entry(argument)
                .or_default()
                .push(prerequisite);
        }
        for argument in found.iter_mut() {
            if let Some(list) = by_argument.get(argument.id.as_str()) {
                argument.prerequisites =
                    list.iter().map(|s| ArgumentId(s.clone())).collect();
            }
        }
        Ok(found)
    }

    /// Il model lock, per id derivato dal contenuto. `ON CONFLICT DO NOTHING`
    /// sta per una ragione sola: lo stesso lock registrato due volte è lo stesso
    /// lock, e i due eventi di generazione stanno in `generations`.
    pub(crate) fn put_lock(&self, lock: &ModelLock) -> Result<String> {
        let id = codec::lock_id(lock);
        self.conn.execute(
            "INSERT INTO model_locks (id, model_id, prompt_hash, corpus_hash, generator_version, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT DO NOTHING",
            rusqlite::params![
                id,
                lock.model_id,
                lock.prompt_hash,
                lock.corpus_hash,
                lock.generator_version,
                lock.at.0
            ],
        )?;
        Ok(id)
    }
}

impl std::fmt::Debug for Store {
    /// `Connection` non è `Debug` e non deve diventarlo: stampa il fatto che
    /// l'oggetto esiste e lascia i dettagli al database.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

/// Si può aggiungere «`from` dipende da `to`» senza creare un ciclo?
///
/// Il cammino parte da **`to`**, non da `from`: l'arco in questione è
/// `from -> to`, e il ciclo compare se `to` dipende già da `from`, direttamente
/// o per transitività. Guardare da `from` non servirebbe a niente — misurerebbe
/// i prerequisiti che `from` ha *già*, che sono esattamente quelli che il
/// controllo sta per sostituire.
///
/// `UNION` e non `UNION ALL`: i duplicati chiudono il ciclo anche su un grafo
/// già ciclico, quindi il controllo non può girare all'infinito.
fn reaches(conn: &Connection, from: &ArgumentId, to: &ArgumentId) -> Result<bool> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE walk(node) AS ( \
             SELECT ?1 \
             UNION \
             SELECT ap.prerequisite_id FROM walk \
               JOIN argument_prerequisites ap ON ap.argument_id = walk.node \
         ) \
         SELECT EXISTS(SELECT 1 FROM walk WHERE node = ?2)",
    )?;
    // `?1` è il punto di partenza (`to`), `?2` è quello da cercare (`from`).
    let found: bool = stmt.query_row(rusqlite::params![to.0, from.0], |r| r.get(0))?;
    Ok(found)
}

/// Le relazioni e l'attribuzione di autore di una persona.
pub(crate) struct Visibility {
    person: PersonId,
    relations: Vec<Relation>,
    /// `argument_id -> persone che ne sono autrici`, comprese le fonti delle
    /// derivazioni.
    authored: HashMap<String, HashSet<String>>,
}

impl Visibility {
    /// Le relazioni che valgono **per questo argomento**: quelle di corso più
    /// `author_of` e `ratified`, che sono fatti sull'oggetto.
    ///
    /// Il predicato di `kbs-core` accetta un elenco di relazioni più due
    /// booleani, e i booleani sono la forma deduplicata delle stesse due
    /// relazioni. Si passano tutti e due perché il predicato li accetta tutti e
    /// due, e il giorno in cui cambierà firma sarà un problema di un solo posto.
    pub fn for_argument(&self, argument: &Argument) -> (Vec<Relation>, bool, bool) {
        let is_author = self
            .authored
            .get(argument.id.as_str())
            .is_some_and(|set| set.contains(self.person.as_str()));
        let is_ratifier = argument
            .ratified
            .as_ref()
            .is_some_and(|r| r.by == self.person);
        let mut relations = self.relations.clone();
        if is_author {
            relations.push(Relation::AuthorOf);
        }
        if is_ratifier {
            relations.push(Relation::Ratified);
        }
        (relations, is_author, is_ratifier)
    }

    /// La domanda, in un posto solo: [`kbs_core::may_read`].
    pub fn may_see(&self, argument: &Argument) -> bool {
        let (relations, is_author, is_ratifier) = self.for_argument(argument);
        may_read(&relations, argument.state, is_author, is_ratifier)
    }
}

/// Lo stato che `upsert_argument` può scrivere.
///
/// La regola in una riga: si può scrivere uno stato mutabile, restare dovunque
/// si è, e nient'altro. Ogni altra combinazione è una transizione che non spetta
/// a `upsert_argument`, e rispondere «non lo faccio» è meglio che farlo e
/// chiamarlo `publish`.
fn writable_state(
    id: &ArgumentId,
    current: Option<PublicationState>,
    requested: PublicationState,
) -> Result<PublicationState> {
    match current {
        // Stare dove si è non è una transizione.
        Some(from) if from == requested => Ok(from),
        //Dentro gli stati mutabili si passa come si vuole: una bozza che il
        // docente rivede diventa una revisione del docente, e viceversa.
        Some(from) if from.is_mutable() && requested.is_mutable() => Ok(requested),
        // Un argomento nuovo entra come bozza o come revisione del docente. Non
        // entra in `in-corso`: la porta è `publish`.
        None if requested.is_mutable() => Ok(requested),
        _ => Err(Error::StateTransition {
            id: id.clone(),
            from: current,
            to: requested,
        }),
    }
}

fn relation_to_db(r: Relation) -> &'static str {
    match r {
        Relation::EnrolledIn => "enrolled_in",
        Relation::Teaches => "teaches",
        Relation::AuthorOf => "author_of",
        Relation::Ratified => "ratified",
        Relation::SpeculativeFor => "speculative_for",
    }
}

fn relation_from_db(raw: &str) -> Result<Relation> {
    match raw {
        "enrolled_in" => Ok(Relation::EnrolledIn),
        "teaches" => Ok(Relation::Teaches),
        other => Err(Error::Corrupt {
            table: "relations",
            field: "relation",
            reason: format!("`{other}` non è una relazione nota"),
        }),
    }
}

/// Una riga di `arguments` come `kbs_core::Argument`.
///
/// La decodifica può fallire, e dentro una `query_map` non si può propagare un
/// errore di `kbs-store`: si passa per `FromSqlConversionFailure`, che è il modo
/// di SQLite per dire «questa colonna non la so leggere» e che conserva il
/// messaggio originale. Un database scritto a mano con uno stato inesistente
/// dà un errore, non un panico.
fn map_argument(row: &Row<'_>) -> rusqlite::Result<Argument> {
    let id: String = row.get(0)?;
    let course: String = row.get(1)?;
    let title: String = row.get(2)?;
    let summary: String = row.get(3)?;
    let state_raw: String = row.get(4)?;
    let state = codec::state_from_db(&state_raw).map_err(|e| corrupt(4, e))?;
    let rel_path: Option<String> = row.get(5)?;
    let content_hash: String = row.get(6)?;

    let origin_kind: String = row.get(7)?;
    let origin_by: Option<String> = row.get(8)?;
    let origin_at: Option<i64> = row.get(9)?;
    let origin_lock: Option<String> = row.get(10)?;
    let origin_from: Option<String> = row.get(11)?;

    let created_at: i64 = row.get(12)?;
    let updated_at: i64 = row.get(13)?;

    let ratified_by: Option<String> = row.get(14)?;
    let ratified_at: Option<i64> = row.get(15)?;
    let ratified_hash: Option<String> = row.get(16)?;
    let ratified_note: Option<String> = row.get(17)?;

    let lock = read_optional_lock(row)?;

    let origin_row = codec::OriginRow {
        kind: match origin_kind.as_str() {
            "human" => "human",
            "generated" => "generated",
            "derived" => "derived",
            other => {
                return Err(corrupt(
                    7,
                    Error::Corrupt {
                        table: "arguments",
                        field: "origin_kind",
                        reason: format!("`{other}` non è una variante nota"),
                    },
                ))
            }
        },
        by: origin_by,
        at: origin_at,
        lock: origin_lock,
        from: origin_from,
    };
    let origin = codec::origin_from_row(&origin_row, lock).map_err(|e| corrupt(7, e))?;

    let ratified = match (ratified_by, ratified_at, ratified_hash) {
        (Some(by), Some(at), Some(hash)) => Some(
            codec::ratification_from_row(&by, at, &hash, ratified_note).map_err(|e| corrupt(15, e))?,
        ),
        (None, None, None) => None,
        _ => {
            return Err(corrupt(
                14,
                Error::Corrupt {
                    table: "arguments",
                    field: "ratified_by",
                    reason: "la ratifica c'è a metà".into(),
                },
            ))
        }
    };

    Ok(Argument {
        id: ArgumentId(id),
        title,
        summary,
        state,
        course: CourseId(course),
        prerequisites: Vec::new(),
        origin,
        rel_path,
        content_hash,
        created_at: kbs_core::Millis(created_at),
        updated_at: kbs_core::Millis(updated_at),
        ratified,
    })
}

/// Il modello lock della `LEFT JOIN`, che per definizione può non esserci.
fn read_optional_lock(row: &Row<'_>) -> rusqlite::Result<Option<ModelLock>> {
    let model_id: Option<String> = row.get(18)?;
    let prompt_hash: Option<String> = row.get(19)?;
    let corpus_hash: Option<String> = row.get(20)?;
    let generator_version: Option<String> = row.get(21)?;
    let at: Option<i64> = row.get(22)?;
    match (model_id, prompt_hash, corpus_hash, generator_version, at) {
        (Some(m), Some(p), Some(c), Some(g), Some(at)) => Ok(Some(ModelLock {
            model_id: m,
            prompt_hash: p,
            corpus_hash: c,
            generator_version: g,
            at: kbs_core::Millis(at),
        })),
        (None, None, None, None, None) => Ok(None),
        _ => Err(corrupt(
            18,
            Error::Corrupt {
                table: "model_locks",
                field: "id",
                reason: "il lock c'è a metà".into(),
            },
        )),
    }
}

fn corrupt(column: usize, error: Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
}
