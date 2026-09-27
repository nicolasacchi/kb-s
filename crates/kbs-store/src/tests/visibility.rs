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
fn i_rifiuti_di_una_bozza_sono_gli_stessi_byte_di_una_riga_inesistente() {
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
    // **Gli stessi byte**, non la stessa variante: due errori della stessa variante
    // possono portare campi diversi, e un campo che cambia è un oracolo. `arg_<fnv16
    // del percorso>` è enumerabile da chiunque abbia il corpus, quindi ogni
    // differenza fra le due risposte — anche lo stato di pubblicazione della riga —
    // è un canale per imparare che cosa c'è in un corso.
    assert!(
        matches!(inesistente, Error::NotReadable { .. }) && matches!(non_visibile, Error::NotReadable { .. }),
        "le due risposte sono entrambe NotReadable"
    );
    let senza_id = |e: &Error| {
        e.to_string()
            .split_whitespace()
            .filter(|w| !w.starts_with("arg_"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(
        senza_id(&inesistente),
        senza_id(&non_visibile),
        "«non c'è» e «non lo vedi» non possono essere due messaggi diversi: l'id \
         lo ha chiesto il chiamante, tutto il resto no"
    );
    // E la difesa non può dipendere da chi traduce l'errore in una risposta: il
    // testo non contiene lo stato, quindi non lo contiene in nessuna
    // traduzione.
    let testo = non_visibile.to_string();
    for stato in ["Bozza", "InCorso", "DelDocente", "Archiviato"] {
        assert!(!testo.contains(stato), "il rifiuto nomina lo stato: {testo}");
    }
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
    // **L'autore della sorgente non insegna il corso.** È la riga che rende questo
    // test un test: con un autore che è anche docente, l'attribuzione lungo la
    // catena non serve a niente, perché `may_read` apre comunque sul ramo
    // `Teaches` e il test passerebbe anche con la funzione dell'attribuzione
    // cancellata dal file. Un collega che collabora a un corso di cui non è docente
    // scrive la sorgente, deriva da quella, e deve poter rivedere il derivato in
    // bozza: senza l'attribuzione risalita non potrebbe, e un autore che non vede
    // il proprio lavoro è un difetto, non una severità.
    let autore = s.outsider.clone();
    let source = {
        let a = School::argument(&s.course, &autore, 1, PublicationState::Bozza);
        s.store.upsert_argument(&a).expect("bozza della sorgente");
        a
    };
    assert!(
        s.store
            .relations_of(&autore, &s.course)
            .expect("relazioni")
            .is_empty(),
        "l'autore non ha relazioni col corso: è il caso che il predicato deve coprire"
    );
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
        ..School::argument(&s.course, &autore, 9, PublicationState::Bozza)
    };
    s.store.upsert_argument(&derived).expect("derivato");

    let by_source_author = s
        .store
        .read_argument(&autore, &derived.id)
        .expect("l'autore della sorgente vede il derivato, benché non insegni");
    assert_eq!(by_source_author.id, derived.id);
    // E un derivato di un derivato risale ancora: la catena non è lunga uno.
    let derivato_del_derivato = kbs_core::Argument {
        id: ArgumentId::from_rel_path("corsi/course_0001/derivato-del-derivato.html"),
        prerequisites: vec![],
        origin: Origin::Derived {
            from: derived.id.clone(),
            at: Millis(1_700_000_000_000),
        },
        ..School::argument(&s.course, &autore, 10, PublicationState::Bozza)
    };
    s.store
        .upsert_argument(&derivato_del_derivato)
        .expect("derivato del derivato");
    s.store
        .read_argument(&autore, &derivato_del_derivato.id)
        .expect("l'attribuzione risale lungo tutta la catena");
    // Chi non c'entra non vede nessuno dei due: il predicato non è stato allargato,
    // è stato corretto. Il docente del corso vede tutto, e va bene — insegna.
    s.store
        .read_argument(&s.student, &derived.id)
        .expect_err("uno studente non vede un derivato in bozza");
    s.store
        .read_argument(&s.other_teacher, &derivato_del_derivato.id)
        .expect_err("un docente di un altro corso non vede un derivato in bozza");
    s.store
        .read_argument(&s.teacher, &derived.id)
        .expect("il docente del corso vede il derivato: insegna");
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
