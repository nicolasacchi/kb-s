//! Il banco di prova: una scuola piccola e vera.
//!
//! I test non costruiscono righe a mano: costruiscono un corso, un docente, uno
//! studente iscritto, un docente di un altro corso, e un argomento in bozza. È
//! la situazione minima in cui la regola di D5 dice qualcosa.

#![allow(dead_code)]

mod catena;
mod export;
mod inventory;
mod italian_search;
mod publication;
mod registers;
mod schema_guard;
mod visibility;

use kbs_core::{
    Argument, ArgumentId, CohortId, CourseId, Millis, ModelLock, Origin, PersonId,
    PublicationState, Relation,
};
use tempfile::TempDir;

use crate::types::{CourseRelation, Person, Register, SessionId, Source, SourceStatus};
use crate::Store;

/// Una scuola di prova, su **file**.
///
/// Su file e non in memoria, perché la guardia dell'epoch, il `WAL` e
/// `synchronous` si provano solo davvero: un test che passa solo in memoria non
/// prova il caso che capita.
pub struct School {
    pub store: Store,
    pub path: std::path::PathBuf,
    pub course: CourseId,
    pub other_course: CourseId,
    pub teacher: PersonId,
    pub other_teacher: PersonId,
    pub student: PersonId,
    pub outsider: PersonId,
    dir: TempDir,
}

const T0: i64 = 1_700_000_000_000;

impl School {
    pub fn new() -> School {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("scuola.sqlite");
        let mut store = Store::open(&path).expect("apertura");

        let course = CourseId::fixture(1);
        let other_course = CourseId::fixture(2);
        let teacher = PersonId::fixture(1);
        let other_teacher = PersonId::fixture(2);
        let student = PersonId::fixture(3);
        let outsider = PersonId::fixture(4);

        for (id, slug, rel) in [
            (course.clone(), "analisi-1", "corsi/analisi-1"),
            (other_course.clone(), "fisica-1", "corsi/fisica-1"),
        ] {
            store
                .register_source(&Source {
                    id,
                    slug: slug.to_string(),
                    rel_path: rel.to_string(),
                    status: SourceStatus::Active,
                    registered_at: Millis(T0),
                    last_scan_at: None,
                    corpus_hash: None,
                })
                .expect("source");
        }
        for (id, name) in [
            (teacher.clone(), "Prof. Rossi"),
            (other_teacher.clone(), "Prof. Verdi"),
            (student.clone(), "Giulia Bianchi"),
            (outsider.clone(), "Marco Neri"),
        ] {
            store
                .upsert_person(&Person {
                    id,
                    display_name: name.to_string(),
                    created_at: Millis(T0),
                })
                .expect("persona");
        }
        for (person, course_, relation) in [
            (teacher.clone(), course.clone(), Relation::Teaches),
            (student.clone(), course.clone(), Relation::EnrolledIn),
            (
                other_teacher.clone(),
                other_course.clone(),
                Relation::Teaches,
            ),
        ] {
            store
                .add_relation(&CourseRelation {
                    person,
                    course: course_,
                    relation,
                    since: Millis(T0),
                    until: None,
                })
                .expect("relazione");
        }

        School {
            store,
            path,
            course,
            other_course,
            teacher,
            other_teacher,
            student,
            outsider,
            dir,
        }
    }

    /// Riapre il database da disco, per provare le guardie all'apertura.
    pub fn reopen(&self) -> Store {
        Store::open(&self.path).expect("riapertura")
    }

    pub fn session(&mut self, register: Register) -> SessionId {
        self.store
            .open_session(register, "test")
            .expect("sessione")
    }

    pub fn cohort(&self) -> CohortId {
        CohortId("2A".to_string())
    }

    pub fn lock() -> ModelLock {
        ModelLock {
            model_id: "claude-sonnet-4-5".into(),
            prompt_hash: "ph".into(),
            corpus_hash: "ch".into(),
            generator_version: "kbs-doc/1".into(),
            at: Millis(T0),
        }
    }

    /// Un argomento, senza scriverlo.
    pub fn argument(course: &CourseId, by: &PersonId, n: u32, state: PublicationState) -> Argument {
        let rel_path = format!("corsi/{}/lezione-{n:02}.html", course.0);
        Argument {
            id: ArgumentId::from_rel_path(&rel_path),
            title: format!("Lezione {n:02}: i limiti di una funzione"),
            summary: "Che cosa dice un argomento, e chi ha diritto di saperlo.".to_string(),
            state,
            course: course.clone(),
            prerequisites: Vec::new(),
            origin: Origin::Human {
                by: by.clone(),
                at: Millis(T0),
            },
            rel_path: Some(rel_path),
            content_hash: format!("sha256:{n:064x}"),
            created_at: Millis(T0),
            updated_at: Millis(T0),
            ratified: None,
        }
    }

    /// Scrive un argomento in bozza.
    pub fn draft(&mut self, n: u32) -> Argument {
        let a = School::argument(&self.course, &self.teacher, n, PublicationState::Bozza);
        self.store.upsert_argument(&a).expect("bozza");
        a
    }

    /// Scrive un argomento, lo ratifica e lo pubblica.
    pub fn published(&mut self, n: u32) -> Argument {
        let mut a = self.draft(n);
        self.store
            .ratify(&a.id, &self.teacher, "verificato riga per riga")
            .expect("ratifica");
        self.store.publish(&a.id).expect("pubblicazione");
        a = self
            .store
            .read_argument(&self.teacher, &a.id)
            .expect("rilettura");
        a
    }
}
