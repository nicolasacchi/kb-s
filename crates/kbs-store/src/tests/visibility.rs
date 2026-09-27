//! La visibilità: `may_read` in un posto solo, e la conseguenza che ne segue.
//!
//! Ogni test qui è una riga di D5 resa eseguibile. Se `may_read` cambia, questi
//! test cambiano: è il punto.

use kbs_core::{ArgumentId, Millis, Origin, PublicationState, Relation};

use super::School;
use crate::error::Error;

#[test]
fn una_bozza_e_invisibile_all_iscritto_che_non_l_ha_scritta() {
    let mut s = School::new();
    let draft = s.draft(1);

    let err = s
        .store
        .read_argument(&s.student, &draft.id)
        .expect_err("uno studente iscritto non vede una bozza che non ha scritto");
    assert!(matches!(err, Error::NotReadable { .. }), "{err:?}");

    let visible = s
        .store
        .visible_arguments(&s.student, &s.course, None)
        .expect("elenco");
    assert!(
        visible.is_empty(),
        "l'elenco dello studente contiene {:?}",
        visible.iter().map(|a| &a.id).collect::<Vec<_>>()
    );
}

#[test]
fn la_bozza_e_visibile_a_chi_l_ha_scritta_e_al_docente() {
    let mut s = School::new();
    let draft = s.draft(1);

    let by_author = s
        .store
        .read_argument(&s.teacher, &draft.id)
        .expect("l'autore vede la propria bozza");
    assert_eq!(by_author.id, draft.id);

    // Un secondo docente dello stesso corso non l'ha scritta ma lo insegna:
    // `teaches` vede tutto il corso, bozze comprese.
    s.store
        .add_relation(&crate::types::CourseRelation {
            person: s.other_teacher.clone(),
            course: s.course.clone(),
            relation: Relation::Teaches,
            since: Millis(0),
            until: None,
        })
        .expect("relazione");
    let by_teacher = s
        .store
        .read_argument(&s.other_teacher, &draft.id)
        .expect("il docente vede la bozza del corso");
    assert_eq!(by_teacher.id, draft.id);
}

#[test]
fn uno_che_non_e_nulla_del_corso_non_vede_nulla() {
    let mut s = School::new();
    let draft = s.draft(1);
    s.store
        .read_argument(&s.outsider, &draft.id)
        .expect_err("una persona senza relazioni non vede niente");
    s.store
        .read_argument(&s.other_teacher, &draft.id)
        .expect_err("un docente di un altro corso non vede le bozze di questo");
}

#[test]
fn un_argomento_che_non_esiste_e_la_stessa_risposta_di_uno_che_non_si_vede() {
    let mut s = School::new();
    let draft = s.draft(1);

    let inesistente = s
        .store
        .read_argument(&s.student, &ArgumentId::from_rel_path("no/esiste.html"))
        .expect_err("inesistente");
    let non_visibile = s
        .store
        .read_argument(&s.student, &draft.id)
        .expect_err("non visibile");
    // Due varianti diverse ma la stessa risposta: se il chiamante potesse
    // distinguerle, «no esiste» e «non lo vedi» sarebbero due canali per
    // imparare che cosa c'è nel corso.
    assert!(matches!(inesistente, Error::NotReadable { .. }));
    assert!(matches!(non_visibile, Error::NotReadable { .. }));
}

#[test]
fn in_corso_e_visibile_agli_iscritti_e_archiviato_anche() {
    let mut s = School::new();
    let live = s.published(1);
    assert_eq!(live.state, PublicationState::InCorso);
    s.store
        .read_argument(&s.student, &live.id)
        .expect("in corso: visibile agli iscritti");

    let archived = s.published(2);
    s.store.archive(&archived.id).expect("archiviazione");
    let read = s
        .store
        .read_argument(&s.student, &archived.id)
        .expect("archiviato: chi lo ha seguito lo vede ancora");
    assert_eq!(read.state, PublicationState::Archiviato);
}

#[test]
fn il_filtro_per_stato_e_il_predicato_non_si_scambiano() {
    let mut s = School::new();
    s.draft(1);
    let live = s.published(2);

    // Filtrare per `bozza` non deve trasformare l'elenco dello studente in
    // «le bozze del docente»: il predicato resta.
    let bozze = s
        .store
        .visible_arguments(&s.student, &s.course, Some(PublicationState::Bozza))
        .expect("elenco per stato");
    assert!(bozze.is_empty());

    let del_docente = s
        .store
        .visible_arguments(&s.student, &s.course, None)
        .expect("elenco");
    assert_eq!(del_docente.len(), 1);
    assert_eq!(del_docente[0].id, live.id);
}

#[test]
fn l_argomento_derivato_e_del_chi_ha_scritto_la_sorgente() {
    let mut s = School::new();
    let source = s.draft(1);
    // `kbs_core::Origin::Derived` non ha il campo `by`: l'attribuzione risale
    // lungo la catena, altrimenti un argomento derivato non avrebbe un autore
    // e nessuno potrebbe vederne la bozza.
    let derived = kbs_core::Argument {
        id: ArgumentId::from_rel_path("corsi/course_0001/derivato.html"),
        prerequisites: vec![],
        origin: Origin::Derived {
            from: source.id.clone(),
            at: Millis(1_700_000_000_000),
        },
        ..School::argument(&s.course, &s.teacher, 9, PublicationState::Bozza)
    };
    s.store.upsert_argument(&derived).expect("derivato");

    let by_source_author = s
        .store
        .read_argument(&s.teacher, &derived.id)
        .expect("l'autore della sorgente vede il derivato");
    assert_eq!(by_source_author.id, derived.id);
    s.store
        .read_argument(&s.student, &derived.id)
        .expect_err("uno studente non vede un derivato in bozza");
}

#[test]
fn la_query_non_filtrata_e_privata_e_quella_filtrata_no() {
    // Non si può dimostrare la privacy di un metodo a runtime. Si dimostra la
    // proprietà che rende la privacy necessaria: la riga esiste, il predicato la
    // nasconde. Qui si usa `conn()` — la porta dichiarata — per mostrare che la
    // bozza c'è, e che dalla strada pubblica non si vede.
    let mut s = School::new();
    let draft = s.draft(1);

    let visible: i64 = s
        .store
        .conn()
        .query_row("SELECT COUNT(*) FROM arguments WHERE id = ?1", [&draft.id.0], |r| {
            r.get(0)
        })
        .expect("conteggio grezzo");
    assert_eq!(visible, 1, "la riga esiste: non è il predicato a cancellarla");

    let from_public = s
        .store
        .visible_arguments(&s.student, &s.course, None)
        .expect("elenco pubblico");
    assert!(from_public.is_empty());
}

#[test]
fn relazione_chiusa_non_e_piu_una_relazione() {
    let mut s = School::new();
    let live = s.published(1);
    assert!(s.store.read_argument(&s.student, &live.id).is_ok());

    // Una data **passata**: `until` futura significherebbe un'iscrizione che
    // scade fra diciotto anni, cioè ancora aperta.
    s.store
        .end_relation(&s.student, &s.course, Relation::EnrolledIn, Millis(1_700_000_001_000))
        .expect("fine iscrizione");
    assert!(
        s.store
            .relations_of(&s.student, &s.course)
            .expect("relazioni")
            .is_empty()
    );
    s.store
        .read_argument(&s.student, &live.id)
        .expect_err("fuori corso, non vede più il materiale in corso");
}

#[test]
fn una_relazione_di_corso_non_e_tutte_le_relazioni() {
    let mut s = School::new();
    // `author_of`, `ratified` e `speculative_for` non sono relazioni di corso:
    // accettarle sarebbe accettare una scrittura che nessuno legge.
    for relation in [
        Relation::AuthorOf,
        Relation::Ratified,
        Relation::SpeculativeFor,
    ] {
        let err = s
            .store
            .add_relation(&crate::types::CourseRelation {
                person: s.student.clone(),
                course: s.course.clone(),
                relation,
                since: Millis(0),
                until: None,
            })
            .expect_err("relazione non di corso");
        assert_eq!(err.rule(), "validation");
    }
}
