//! La colonna che distingue il sistema da una bottiglia con i fantasmi.
//!
//! Qui non si prova che una riga ha un campo: si prova che **il campo non può
//! essere falsificato, né nel valore né nella lettura**. Sono tre le cose, e
//! ognuna ha il suo tipo di prova:
//!
//! 1. il `CHECK` e il trigger della migrazione `V6` mordono, e la riga che non
//!    doveva entrare non è entrata: il confronto è sul numero di righe, non sul
//!    fatto che qualcosa abbia restituito un errore;
//! 2. la **query** dello studente non può restituire una riga assistita, e la
//!    prova è sulla relazione che legge e sul contenuto che torna, non sul
//!    fatto che una rotta sia stata chiamata;
//! 3. il conteggio di coorte e la lettura dello studente prendono le righe
//!    dallo stesso posto, e quindi non possono discordare.

use kbs_core::{ArgumentId, CohortSignal, Evidence, Millis, PersonId, COHORT_MIN_K};

use super::School;
use crate::error::Error;
use crate::types::{ObservationDraft, Person, Register, SessionId};

const T0: i64 = 1_700_000_000_000;

/// Una dimostrazione con l'aiuto dichiarato.
///
/// `unaided` e `n_hints` sono i campi sotto prova: gli altri sono quelli che
/// ogni altra riga del banco porta, e tenerli uguali rende leggabile quale
/// differenza produce il campo nuovo e nient'altro.
fn con_aiuto(
    n: u32,
    s: &School,
    argomento: &ArgumentId,
    unaided: Option<bool>,
    n_hints: Option<u32>,
) -> ObservationDraft {
    ObservationDraft {
        id: format!("obs-{n:03}"),
        student: s.student.clone(),
        course: s.course.clone(),
        cohort: s.cohort(),
        argument: argomento.clone(),
        evidence: Evidence::Checked {
            exercise: "ex-1".into(),
            instance: format!("seed-{n}"),
            correct: false,
        },
        judged_by: None,
        unaided,
        n_hints,
        at: Millis(T0 + n as i64),
    }
}

/// Una persona nuova del banco, perché cinque tentativi di uno studente non
/// sono una classe (D9) e il test deve poterlo dire.
fn compagno(n: u32) -> PersonId {
    PersonId::fixture(50 + n)
}

fn iscritto(s: &mut School, n: u32) -> PersonId {
    let persona = compagno(n);
    s.store
        .upsert_person(&Person {
            id: persona.clone(),
            display_name: format!("Compagno {n}"),
            created_at: Millis(T0),
        })
        .expect("persona");
    persona
}

#[test]
fn lo_studente_vede_le_non_assistite_e_il_docente_le_vede_tutte() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, con_aiuto(1, &s, &arg.id, Some(true), Some(0)))
        .expect("non assistita");
    s.store
        .append_observation(&sessione, con_aiuto(2, &s, &arg.id, Some(false), Some(4)))
        .expect("assistita");
    // La terza è il caso che la migrazione crea da sé: riga precedente alla
    // colonna, aiuto ignoto.
    s.store
        .append_observation(&sessione, con_aiuto(3, &s, &arg.id, None, None))
        .expect("aiuto ignoto");

    let dello_studente = s
        .store
        .observations_for(&s.student, &s.student, &arg.id)
        .expect("il proprio registro");
    assert_eq!(
        ids(&dello_studente),
        vec!["obs-001".to_string()],
        "la coda di practice e l'aiuto ignoto non sono del suo registro"
    );
    assert_eq!(dello_studente[0].unaided, Some(true));
    assert_eq!(dello_studente[0].n_hints, Some(0));

    // Il docente insegna anche dalla coda di practice, e la riga ignota è sua.
    let del_docente = s
        .store
        .observations_for(&s.teacher, &s.student, &arg.id)
        .expect("il registro del suo studente");
    assert_eq!(ids(&del_docente), vec!["obs-001", "obs-002", "obs-003"]);
    let dichiarato: Vec<Option<bool>> = del_docente.iter().map(|o| o.unaided).collect();
    assert_eq!(dichiarato, vec![Some(true), Some(false), None]);
}

fn ids(osservazioni: &[kbs_core::Observation]) -> Vec<String> {
    osservazioni.iter().map(|o| o.id.clone()).collect()
}

#[test]
fn la_vista_non_puo_restituire_una_riga_assistita_e_il_database_lo_dice() {
    // La prova non è «la funzione filtra»: è che **la relazione** non contiene
    // la riga. Una vista è nel file del database, quindi vale per chi scrive
    // SQL a mano, per un'altra implementazione e per un dump — non solo per il
    // percorso Rust che l'ha scelta.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for (n, (unaided, n_hints)) in [
        (Some(true), Some(0)),
        (Some(false), Some(2)),
        (None, None),
    ]
    .into_iter()
    .enumerate()
    {
        s.store
            .append_observation(
                &sessione,
                con_aiuto(n as u32, &s, &arg.id, unaided, n_hints),
            )
            .expect("osservazione");
    }

    let (in_tabella, nella_vista, stonate) = s
        .store
        .conn()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM observations), \
                    (SELECT COUNT(*) FROM unaided_observations), \
                    (SELECT COUNT(*) FROM unaided_observations \
                      WHERE unaided IS NULL OR unaided <> 1)",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )
        .expect("i tre conteggi");

    assert_eq!(in_tabella, 3, "la tabella le ha tutte");
    assert_eq!(nella_vista, 1, "la vista ne ha una");
    assert_eq!(
        stonate, 0,
        "la vista contiene una riga che non è non assistita"
    );
}

#[test]
fn il_check_su_unaided_morde_e_poi_lascia_passare_il_valore_buono() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, con_aiuto(1, &s, &arg.id, Some(true), Some(0)))
        .expect("la riga buona entra");

    // `2` non è «meglio di 1»: è una terza cosa che nessuno sa cosa significhi.
    // Il `CHECK` lo dice, e la riga non entra.
    let testo = inserimento(&s, &sessione, &arg.id, "obs-due", 99, "2, 0")
        .expect_err("il CHECK rifiuta una terza cosa")
        .to_string();
    assert!(testo.contains("unaided"), "{testo}");
    assert_eq!(righe(&s), 1, "la riga rifiutata non è entrata");

    // E il valore buono, lo stesso identico, entra. Un `CHECK` che rifiuta
    // tutto non è un `CHECK`, è una porta chiusa: la prova che distingue le due
    // cose è questa, non la precedente.
    s.store
        .append_observation(&sessione, con_aiuto(2, &s, &arg.id, Some(true), Some(0)))
        .expect("la seconda riga buona entra");
    assert_eq!(righe(&s), 2);
    let non_assistite: i64 = s
        .store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM observations WHERE unaided = 1",
            [],
            |r| r.get(0),
        )
        .expect("conteggio");
    assert_eq!(non_assistite, 2);
}

#[test]
fn le_invarianti_che_non_stanno_in_una_colonna_sono_del_database() {
    // Sono le due che `append_observation` non può mai produrre, e le si
    // provano con SQL scritto a mano: è il caso per cui esistono.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, con_aiuto(1, &s, &arg.id, Some(true), Some(0)))
        .expect("riga buona");

    // Un conteggio di indizi dichiarato per una riga di cui si ignora la
    // disponibilità dell'aiuto: sapere «quante piste c'erano» presuppone sapere
    // «se ce n'era qualcuna».
    let testo = inserimento(&s, &sessione, &arg.id, "obs-ambigua", 98, "NULL, 2")
        .expect_err("l'aiuto ignoto non può avere un conteggio")
        .to_string();
    assert!(testo.contains("n_hints"), "{testo}");

    // E un conteggio negativo non è un conteggio.
    assert!(
        inserimento(&s, &sessione, &arg.id, "obs-negativa", 97, "1, -1").is_err(),
        "un conteggio di indizi negativo entra, e non doveva"
    );

    assert_eq!(righe(&s), 1, "nessuna delle due righe è entrata");
}

#[test]
fn il_flag_non_si_rizza_dopo_che_la_riga_e_stata_scritta() {
    // La domanda di sicurezza che questa colonna apre: se `unaided` si potesse
    // alzare dopo, «lo studente vede solo le non assistite» sarebbe una promessa
    // sul momento della scrittura e non sulla riga. Il trigger di append-only la
    // chiude, e questa prova è la conferma che `V6` non l'ha indebolito
    // aggiungendo colonne a una tabella che ne aveva uno.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, con_aiuto(1, &s, &arg.id, Some(false), Some(5)))
        .expect("assistita");

    let testo = s
        .store
        .conn()
        .execute(
            "UPDATE observations SET unaided = 1, n_hints = 0 WHERE id = 'obs-001'",
            [],
        )
        .expect_err("il registro è append-only")
        .to_string();
    assert!(testo.contains("append-only"), "{testo}");

    let ancora_assistita: i64 = s
        .store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM observations WHERE id = 'obs-001' AND unaided = 0",
            [],
            |r| r.get(0),
        )
        .expect("conteggio");
    assert_eq!(
        ancora_assistita, 1,
        "la riga è quella di prima, non una versione alzata"
    );
}

#[test]
fn il_conteggio_di_coorte_e_la_lettura_dello_studente_vedono_le_stesse_righe() {
    // Le due letture prendono le righe da `unaided_observations`. Se prendessero
    // da posti diversi, il numero che il docente vede in classe e l'elenco che
    // lo studente vede potrebbero essere due misure della stessa cosa con due
    // denominatori: il difetto che nessuno nota finché i due numeri non
    // coincidono.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for n in 0..COHORT_MIN_K {
        let altro = iscritto(&mut s, n as u32);
        s.store
            .append_observation(
                &sessione,
                ObservationDraft {
                    student: altro,
                    ..con_aiuto(n as u32, &s, &arg.id, Some(true), Some(0))
                },
            )
            .expect("non assistita");
    }
    // E altrettante assistite: se contassero, il segnale direbbe che dieci
    // studenti non sanno l'argomento, e nessuno dei due ha dimostrato niente
    // senza aiuto.
    for n in 0..COHORT_MIN_K {
        let altro = iscritto(&mut s, 100 + n as u32);
        s.store
            .append_observation(
                &sessione,
                ObservationDraft {
                    student: altro,
                    ..con_aiuto(200 + n as u32, &s, &arg.id, Some(false), Some(4))
                },
            )
            .expect("assistita");
    }

    s.store
        .record_cohort_signal(&CohortSignal {
            course: s.course.clone(),
            cohort: s.cohort(),
            argument: arg.id.clone(),
            failing: COHORT_MIN_K,
            total: COHORT_MIN_K,
            at: Millis(T0),
        })
        .expect("alla soglia, e con i numeri che le righe dicono");

    // E se qualcuno dichiara i numeri di prima — dieci, tutti quelli che ci sono
    // nel registro — il rifiuto porta entrambi i numeri, e non è un silenzio.
    match s
        .store
        .record_cohort_signal(&CohortSignal {
            course: s.course.clone(),
            cohort: s.cohort(),
            argument: arg.id.clone(),
            failing: 10,
            total: 10,
            at: Millis(T0),
        })
        .expect_err("le righe assistite non sono un conteggio di classe")
    {
        Error::CohortCountMismatch {
            declared_failing,
            declared_total,
            counted_failing,
            counted_total,
        } => {
            assert_eq!((declared_failing, declared_total), (10, 10), "quello che dichiarava");
            assert_eq!(
                (counted_failing, counted_total),
                (COHORT_MIN_K, COHORT_MIN_K),
                "quello che le righe non assistite dicono"
            );
        }
        other => panic!("atteso CohortCountMismatch, ottenuto {other:?}"),
    }
}

#[test]
fn la_classe_conta_persone_e_non_una_coda_di_aiuti() {
    // Cinque tentativi assistiti di una sola persona: sotto soglia. La soglia
    // guarda le persone, e la coda di practice non è nemmeno un tentativo
    // della classe.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for n in 0..COHORT_MIN_K {
        s.store
            .append_observation(
                &sessione,
                con_aiuto(n as u32, &s, &arg.id, Some(false), Some(2)),
            )
            .expect("tentativo assistito");
    }
    match s
        .store
        .record_cohort_signal(&CohortSignal {
            course: s.course.clone(),
            cohort: s.cohort(),
            argument: arg.id.clone(),
            failing: COHORT_MIN_K,
            total: COHORT_MIN_K,
            at: Millis(T0),
        })
        .expect_err("una persona sola non è una classe")
    {
        Error::CohortCountMismatch {
            counted_failing,
            counted_total,
            ..
        } => {
            assert_eq!(
                (counted_failing, counted_total),
                (0, 0),
                "una coda di practice non è un conteggio di classe"
            );
        }
        other => panic!("atteso CohortCountMismatch, ottenuto {other:?}"),
    }
    assert!(
        s.store.cohort_signals(&arg.id).expect("lettura").is_empty(),
        "un segnale che non torna con le righe non entra"
    );
}

#[test]
fn la_riga_precedente_alla_colonna_non_diventa_una_misura() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, con_aiuto(1, &s, &arg.id, Some(true), None))
        .expect("non assistita");
    s.store
        .append_observation(&sessione, con_aiuto(2, &s, &arg.id, None, None))
        .expect("aiuto ignoto");

    let (non_assistite, ignote) = s
        .store
        .conn()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM observations WHERE unaided = 1), \
                    (SELECT COUNT(*) FROM observations WHERE unaided IS NULL)",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .expect("i due conteggi");
    assert_eq!((non_assistite, ignote), (1, 1));

    // La riga ignota è nel registro, con `seq` e con la sua parte di catena:
    // esce dalla vista, non dal registro. È la differenza fra «non lo so» e
    // «non è successo».
    let nel_registro = s
        .store
        .observations_in_session(&s.teacher, &sessione)
        .expect("la catena copre tutte le righe");
    assert_eq!(nel_registro.len(), 2, "la catena non può avere un buco");
    assert_eq!(
        nel_registro.iter().filter(|o| o.unaided.is_none()).count(),
        1,
        "la catena deve coprire anche la riga di aiuto ignoto"
    );
}

#[test]
fn il_perimetro_della_regola_non_si_sposta() {
    // «Solo le non assistite» non significa «tutto quello che lo studente
    // chiede». Chi non ha relazione riceve `NotReadable` e non un elenco
    // vuoto: un elenco vuoto sarebbe una risposta **vera**, e qui non lo è.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    s.store
        .append_observation(&sessione, con_aiuto(1, &s, &arg.id, Some(true), Some(0)))
        .expect("riga");

    let rifiutata = s
        .store
        .observations_for(&s.outsider, &s.student, &arg.id)
        .expect_err("un estraneo non legge il registro di nessuno");
    let inesistente = s
        .store
        .observations_for(&s.outsider, &s.student, &ArgumentId::fixture(999))
        .expect_err("argomento inesistente");
    assert!(
        matches!(rifiutata, Error::NotReadable { .. })
            && matches!(inesistente, Error::NotReadable { .. }),
        "le due risposte sono entrambe NotReadable: {rifiutata:?} e {inesistente:?}"
    );
    // E sono gli stessi byte, una volta tolto l'id: **l'id l'ha chiesto il
    // chiamante**, tutto il resto no. È la prova che esiste già in
    // `visibility`, ripetuta qui perché la regola che sto aggiungendo passa da
    // una relazione diversa e un filtro che nascondeva righe avrebbe potuto
    // restituire un elenco vuoto — che è l'altra metà del difetto.
    let senza_id = |e: &Error| {
        e.to_string()
            .split_whitespace()
            .filter(|w| !w.starts_with("arg_") && !w.starts_with("registro:"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(
        senza_id(&rifiutata),
        senza_id(&inesistente),
        "un divieto e un'assenza non possono essere due messaggi diversi"
    );
}

/// Quante righe ci sono, e nient'altro: la prova che una riga rifiutata non è
/// entrata è un numero, non un errore.
fn righe(s: &School) -> i64 {
    s.store
        .conn()
        .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
        .expect("conteggio")
}

/// Un `INSERT` scritto a mano, con i due campi nuovi in coda.
///
/// È scritto qui e non riusato dal crate perché il punto è proprio che il
/// database lo rifiuta **senza passare da `append_observation`**: se la guardia
/// stesse solo nel crate, questa stringa entrava. Gli snapostosto ci sono
/// perché i valori non scritti a mano in una stringa SQL sono un'altra
/// generazione di guai, e qui la prova non è l'iniezione.
fn inserimento(
    s: &School,
    sessione: &SessionId,
    arg: &ArgumentId,
    id: &str,
    seq: i64,
    aiuto_e_indizi: &str,
) -> rusqlite::Result<usize> {
    s.store.conn().execute(
        &format!(
            "INSERT INTO observations (id, session_id, seq, student, course_id, cohort, \
                                        argument_id, evidence, evidence_payload, judged_by, at, \
                                        unaided, n_hints) \
             VALUES (?1, ?2, ?3, ?4, ?5, '2A', ?6, 'none', NULL, NULL, ?7, {aiuto_e_indizi})"
        ),
        rusqlite::params![id, sessione.0, seq, s.student.0, s.course.0, arg.0, T0],
    )
}
