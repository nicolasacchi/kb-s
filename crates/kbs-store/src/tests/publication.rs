//! D4: la porta sul percorso condiviso, e i suoi due rifiuti.
//!
//! Il punto di questi test è che il gate **non è decorativo**. Un gate
//! decorativo si vede qui: nessuna scrittura, nemmeno `upsert_argument`, riesce a
//! portare un argomento in corso senza una ratifica valida.

use kbs_core::{Argument, Invariant, Millis, Origin, PublicationState};

use super::School;
use crate::error::Error;

#[test]
fn pubblicare_senza_ratifica_si_rifiuta_e_l_errore_lo_dice() {
    let mut s = School::new();
    let draft = s.draft(1);

    let err = s
        .store
        .publish(&draft.id)
        .expect_err("senza ratifica non si pubblica");
    match err {
        Error::Invariant(Invariant::CitableWithoutRatification(state)) => {
            assert_eq!(state, PublicationState::Bozza);
        }
        other => panic!("atteso CitableWithoutRatification, ottenuto {other:?}"),
    }
    // E il messaggio nomina l'invariante, non un codice numerico: chi ha sbagliato
    // deve sapere *quale regola*.
    let text = s.store.publish(&draft.id).expect_err("secondo tentativo").to_string();
    assert!(text.contains("ratificato"), "{text}");
}

#[test]
fn una_ratifica_invecchiata_non_vale_e_l_errore_nome_entrambi_gli_hash() {
    let mut s = School::new();
    let mut draft = s.draft(1);
    let hash_al_ratifica = draft.content_hash.clone();
    s.store
        .ratify(&draft.id, &s.teacher, "verificato")
        .expect("ratifica");
    s.store.publish(&draft.id).expect("pubblicazione");

    // Il docente cambia il contenuto dopo la ratifica. La ratifica vale per
    // l'hash di allora, non per quello di adesso: è la ragione per cui la
    // colonna si chiama `ratified_contract_hash`. Lo stato resta `in-corso`:
    // riscriverlo non si può, ed è il punto.
    draft.state = PublicationState::InCorso;
    draft.content_hash = "sha256:modificato".to_string();
    draft.updated_at = Millis(1_700_000_100_000);
    s.store.upsert_argument(&draft).expect("modifica");

    let err = s
        .store
        .publish(&draft.id)
        .expect_err("una ratifica invecchiata non pubblica");
    match err {
        Error::Invariant(Invariant::StaleRatification { declared, current }) => {
            assert_eq!(declared, hash_al_ratifica, "l'hash su cui vale la ratifica");
            assert_eq!(current, "sha256:modificato");
        }
        other => panic!("atteso StaleRatification, ottenuto {other:?}"),
    }
}

#[test]
fn raticando_di_nuovo_si_pubblica_e_il_registro_lo_dice() {
    let mut s = School::new();
    let mut draft = s.draft(1);
    s.store
        .ratify(&draft.id, &s.teacher, "prima verifica")
        .expect("ratifica");
    s.store.publish(&draft.id).expect("pubblicazione");

    draft.state = PublicationState::InCorso;
    draft.content_hash = "sha256:seconda_versione".into();
    s.store.upsert_argument(&draft).expect("modifica");
    assert!(s.store.publish(&draft.id).is_err(), "la ratifica vecchia non basta");

    let r = s
        .store
        .ratify(&draft.id, &s.teacher, "riletta dopo la modifica")
        .expect("nuova ratifica");
    // La ratifica è per l'hash di adesso, non per uno scelto dal chiamante.
    assert_eq!(r.contract_hash, "sha256:seconda_versione");
    assert_eq!(
        s.store.publish(&draft.id).expect("pubblicazione"),
        PublicationState::InCorso
    );
}

#[test]
fn nessuna_scrittura_riesce_a_rendere_citabile_un_argomento_senza_ratifica() {
    let mut s = School::new();
    let draft = s.draft(1);

    // La strada ovvia: dichiarare lo stato nell'oggetto.
    let furioso: Argument = Argument {
        state: PublicationState::InCorso,
        ..draft.clone()
    };
    let err = s
        .store
        .upsert_argument(&furioso)
        .expect_err("scrivere `in-corso` non è una porta");
    assert_eq!(err.rule(), "publication.gate");
    // Il messaggio dice il metodo, non solo la regola: chi ha sbagliato deve
    // sapere dove andare.
    assert!(err.to_string().contains("publish"), "{err}");

    // E la strada con una ratifica in mano: anche così la porta è `publish`.
    let con_ratifica = Argument {
        state: PublicationState::InCorso,
        ratified: Some(kbs_core::Ratification {
            by: s.teacher.clone(),
            at: Millis(1_700_000_000_000),
            contract_hash: draft.content_hash.clone(),
            note: "auto-ratificato".into(),
        }),
        ..draft.clone()
    };
    let err = s
        .store
        .upsert_argument(&con_ratifica)
        .expect_err("la ratifica non si installa da `upsert_argument`");
    assert_eq!(err.rule(), "publication.gate");
    // Il rifiuto dice il metodo giusto, che è `ratify` e non `publish`.
    assert!(
        err.to_string().contains("ratify"),
        "il rifiuto deve dire il metodo giusto: {err}"
    );

    let err = s
        .store
        .upsert_argument(&Argument { state: PublicationState::Bozza, ..con_ratifica })
        .expect_err("la ratifica non si installa da `upsert_argument`");
    assert!(
        err.to_string().contains("ratify"),
        "il rifiuto deve dire il metodo giusto: {err}"
    );

    let stored = s
        .store
        .read_argument(&s.teacher, &draft.id)
        .expect("rilettura");
    assert_eq!(stored.state, PublicationState::Bozza);
    assert!(!stored.is_citable_now(), "non è citabile");
}

#[test]
fn un_aggiornamento_non_cancella_la_ratifica() {
    let mut s = School::new();
    let live = s.published(1);
    // `Argument` ha un campo `ratified`, e chi ha l'oggetto in mano lo avrà
    // comprensibilmente in copia. Se quell'oggetto è più vecchio di una
    // ratifica, riscriverlo non deve poterla cancellare: la ratifica ha una
    // vita sua e si ritira con un metodo che lo dice.
    let mut copia_vecchia = live.clone();
    s.store
        .ratify(&live.id, &s.teacher, "riletta")
        .expect("nuova ratifica");
    copia_vecchia.ratified = None;
    let contenuto = Argument {
        summary: "Riassunto riscritto.".to_string(),
        updated_at: Millis(1_700_000_400_000),
        ..copia_vecchia
    };
    s.store.upsert_argument(&contenuto).expect("aggiornamento");
    let riletta = s
        .store
        .read_argument(&s.teacher, &live.id)
        .expect("rilettura");
    assert!(riletta.ratified.is_some(), "la ratifica è stata cancellata");
    assert_eq!(riletta.ratified.expect("ratifica").note, "riletta");
    assert_eq!(riletta.summary, "Riassunto riscritto.");
}

#[test]
fn un_argomento_pubblicato_non_torna_indietro_scrivendo() {
    let mut s = School::new();
    let live = s.published(1);

    let indietro: Argument = Argument {
        state: PublicationState::Bozza,
        ..live.clone()
    };
    let err = s
        .store
        .upsert_argument(&indietro)
        .expect_err("da `in-corso` a `bozza` non si scrive");
    assert!(matches!(
        err,
        Error::StateTransition { .. }
    ));

    // Eppure riscriverne il contenuto è lecito: è così che nasce una ratifica
    // invecchiata, e la ratifica invecchiata è un fatto che il registro deve
    // saper dire.
    let modificato: Argument = Argument {
        content_hash: "sha256:nuovo".into(),
        updated_at: Millis(1_700_000_200_000),
        ..live.clone()
    };
    s.store.upsert_argument(&modificato).expect("modifica lecita");
    let stored = s
        .store
        .read_argument(&s.teacher, &live.id)
        .expect("rilettura");
    assert_eq!(stored.state, PublicationState::InCorso);
    assert!(!stored.is_citable_now());
}

#[test]
fn ritirare_la_ratifica_togli_la_citabilita() {
    let mut s = School::new();
    let live = s.published(1);
    s.store
        .withdraw_ratification(&live.id)
        .expect("ritiro");
    let stored = s
        .store
        .read_argument(&s.teacher, &live.id)
        .expect("rilettura");
    assert!(stored.ratified.is_none());
    assert!(!stored.is_citable_now());
    s.store
        .withdraw_ratification(&live.id)
        .expect_err("non c'è più niente da ritirare");
}

#[test]
fn da_archiviato_non_si_torna_in_corso() {
    let mut s = School::new();
    let live = s.published(1);
    s.store.archive(&live.id).expect("archiviazione");
    let err = s
        .store
        .publish(&live.id)
        .expect_err("l'inverso di archive non è definito da D4");
    assert!(matches!(err, Error::StateTransition { .. }));
}

#[test]
fn un_titolo_vuoto_non_entra() {
    let mut s = School::new();
    let senza = Argument {
        title: "   ".to_string(),
        ..School::argument(&s.course, &s.teacher, 7, PublicationState::Bozza)
    };
    let err = s.store.upsert_argument(&senza).expect_err("titolo vuoto");
    assert_eq!(err.rule(), "validation");
}

#[test]
fn un_argomento_non_cambia_corso_scrivendolo() {
    // Il caso che il test non copriva prima: ogni `upsert` di questa scuola
    // riusciva con `..live.clone()`, quindi il `course_id` non era mai cambiato e
    // la colonna non era mai stata provata. Un docente che sposta la propria
    // unità in un altro corso portava con sé la ratifica — sua, e di un corso in
    // cui non insegna nessuno.
    let mut s = School::new();
    let live = s.published(1);

    // Su una riga **citabile** è il caso che conta: la ratifica resta valida e
    // l'argomento resta in corso, quindi senza il rifiuto resterebbe citabile in
    // un corso diverso da quello in cui è stata data.
    let spostato: Argument = Argument {
        course: s.other_course.clone(),
        ..live.clone()
    };
    let err = s
        .store
        .upsert_argument(&spostato)
        .expect_err("il corso è il perimetro di condivisione, non una colonna");
    assert_eq!(err.rule(), "identity.course");
    match err {
        Error::CourseReparent { ref from, ref to, .. } => {
            assert_eq!(from, &s.course);
            assert_eq!(to, &s.other_course);
        }
        other => panic!("atteso CourseReparent, ottenuto {other:?}"),
    }
    // E la riga non si è mossa: la verifica è sul database, non sull'errore.
    let ancora = s
        .store
        .read_argument(&s.teacher, &live.id)
        .expect("rilettura");
    assert_eq!(ancora.course, s.course);
    assert_eq!(ancora.state, PublicationState::InCorso);
    assert!(ancora.is_citable_now(), "la ratifica non è stata toccata");

    // E vale anche per una bozza: non è una transizione di stato, è l'identità.
    // Un argomento che cambia percorso cambia id, e un id nuovo è un argomento
    // nuovo.
    let bozza = s.draft(2);
    let err = s
        .store
        .upsert_argument(&Argument {
            course: s.other_course.clone(),
            ..bozza.clone()
        })
        .expect_err("una bozza non cambia corso da sola");
    assert_eq!(err.rule(), "identity.course");
    let ancora_bozza = s
        .store
        .read_argument(&s.teacher, &bozza.id)
        .expect("rilettura della bozza");
    assert_eq!(ancora_bozza.course, s.course);

    // Il percorso invece si può correggere, e senza toccare il corso: è un file
    // sbagliato, non un argomento spostato. Qui l'id resta quello che il
    // chiamante ha portato, e il rifiuto è un altro.
    s.store
        .upsert_argument(&Argument {
            rel_path: Some("corsi/analisi-1/lezione-02.html".into()),
            ..bozza.clone()
        })
        .expect("il percorso si corregge");
}

#[test]
fn il_ciclo_nei_prerequisiti_si_rifiuta_e_nomina_la_regola() {
    let mut s = School::new();
    let a = s.draft(1);
    let b = s.draft(2);

    // b dipende da a: legittimo.
    let b_con_a = Argument {
        prerequisites: vec![a.id.clone()],
        ..b.clone()
    };
    s.store.upsert_argument(&b_con_a).expect("dipendenza legittima");

    // a dipende da b: ciclo.
    let a_con_b = Argument {
        prerequisites: vec![b.id.clone()],
        ..a.clone()
    };
    let err = s
        .store
        .upsert_argument(&a_con_b)
        .expect_err("ciclo");
    match err {
        Error::Invariant(Invariant::PrerequisiteCycle(id)) => assert_eq!(id, b.id.0),
        other => panic!("atteso PrerequisiteCycle, ottenuto {other:?}"),
    }

    // Il ciclo non è stato scritto: il grafo è come prima.
    let riletta = s
        .store
        .read_argument(&s.teacher, &a.id)
        .expect("rilettura");
    assert!(riletta.prerequisites.is_empty());
}

#[test]
fn l_auto_prerequisito_e_un_ciclo_e_il_database_lo_dice_anche() {
    let mut s = School::new();
    let a = s.draft(1);
    let auto = Argument {
        prerequisites: vec![a.id.clone()],
        ..a.clone()
    };
    let err = s.store.upsert_argument(&auto).expect_err("auto-loop");
    assert!(matches!(
        err,
        Error::Invariant(Invariant::PrerequisiteCycle(_))
    ));

    // E anche scrivendo l'arco a mano, fuori da ogni codice di questo crate.
    let err = s
        .store
        .conn()
        .execute(
            "INSERT INTO argument_prerequisites (argument_id, prerequisite_id) VALUES (?1, ?1)",
            [&a.id.0],
        )
        .expect_err("il CHECK sul loop regge anche l'SQL a mano");
    assert!(
        err.to_string().contains("CHECK constraint failed"),
        "{err}"
    );
}

#[test]
fn riscrivere_i_prerequisiti_e_idempotente() {
    let mut s = School::new();
    let a = s.draft(1);
    let b = s.draft(2);
    let c = s.draft(3);
    let con_a = Argument {
        prerequisites: vec![b.id.clone(), c.id.clone()],
        ..a.clone()
    };
    s.store.upsert_argument(&con_a).expect("prima scrittura");
    // Riscrive esattamente gli stessi: il controllo anticiclo deve passare,
    // altrimenti l'upsert non sarebbe idempotente e ogni salvataggio
    // fallirebbe.
    s.store.upsert_argument(&con_a).expect("seconda scrittura identica");
    let letta = s.store.read_argument(&s.teacher, &a.id).expect("rilettura");
    assert_eq!(letta.prerequisites.len(), 2);
    let mut letti: Vec<String> = letta.prerequisites.iter().map(|p| p.0.clone()).collect();
    letti.sort();
    let mut attesi = vec![b.id.0.clone(), c.id.0.clone()];
    attesi.sort();
    assert_eq!(letti, attesi);

    // E un catena vera non è un ciclo: a dipende da b, b da c.
    let b_con_c = Argument {
        prerequisites: vec![c.id.clone()],
        ..b.clone()
    };
    s.store.upsert_argument(&b_con_c).expect("catena");
    let letta = s.store.read_argument(&s.teacher, &a.id).expect("rilettura");
    assert_eq!(letta.prerequisites.len(), 2);
    let letta_b = s.store.read_argument(&s.teacher, &b.id).expect("rilettura di b");
    assert_eq!(letta_b.prerequisites, vec![c.id.clone()], "b dipende da c");
}

#[test]
fn un_prerequisito_ridondante_si_deduplica_e_non_e_un_errore() {
    let mut s = School::new();
    let a = s.draft(1);
    let b = s.draft(2);
    let ripetuto = Argument {
        prerequisites: vec![b.id.clone(), b.id.clone(), b.id.clone()],
        ..a.clone()
    };
    s.store.upsert_argument(&ripetuto).expect("deduplicato");
    let letta = s.store.read_argument(&s.teacher, &a.id).expect("rilettura");
    assert_eq!(letta.prerequisites, vec![b.id]);
}

#[test]
fn la_provenienza_generata_porta_il_suo_lock_e_i_suoi_eventi() {
    let mut s = School::new();
    let lock = School::lock();
    let generato = Argument {
        id: kbs_core::ArgumentId::from_rel_path("corsi/course_0001/generato.html"),
        prerequisites: vec![],
        origin: Origin::Generated {
            lock: lock.clone(),
            by: s.teacher.clone(),
            at: Millis(1_700_000_000_000),
        },
        ..School::argument(&s.course, &s.teacher, 11, PublicationState::Bozza)
    };
    s.store.upsert_argument(&generato).expect("generato");
    s.store
        .record_generation(&crate::types::GenerationEvent {
            lock: lock.clone(),
            argument: generato.id.clone(),
            requester: s.teacher.clone(),
            at: Millis(1_700_000_000_001),
        })
        .expect("evento");

    let riletta = s
        .store
        .read_argument(&s.teacher, &generato.id)
        .expect("rilettura");
    match riletta.origin {
        Origin::Generated { lock: letta, .. } => assert_eq!(letta, lock),
        other => panic!("origine persa: {other:?}"),
    }
    let eventi = s
        .store
        .generations_for(&s.teacher, &generato.id)
        .expect("eventi");
    assert_eq!(eventi.len(), 1);
    assert_eq!(eventi[0].lock.model_id, "claude-sonnet-4-5");

    // E la provenienza di un argomento che non si può leggere non si legge: è la
    // differenza fra una lettura con predicato e una senza, e senza predicato la
    // risposta era la stessa per chiunque avesse un id.
    s.store
        .generations_for(&s.student, &generato.id)
        .expect_err("uno studente non legge la provenienza di un argomento in bozza");
}
