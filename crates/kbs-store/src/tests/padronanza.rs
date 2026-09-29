//! Il metro della padronanza, provato.
//!
//! Qui si prova che cosa il doc di `padronanza` dichiara, e i test sono scritti
//! nella forma che rende una violazione **visibile** invece che improbabile:
//!
//! * ogni numero che il metro produce è costruito **dalla costante**, mai
//!   scritto: se la soglia cambiasse, i dati resterebbero gli stessi e il test
//!   deve cambiare esito, altrimenti starebbe verificando che `8 < 8` è falso;
//! * l'esclusione delle righe assistite e ad aiuto ignoto è provata sul
//!   **contenuto** che torna e sul **conteggio nella tabella**: se la prova
//!   guardasse soltanto «una riga filtrata», passerebbe anche con un filtro
//!   che non filtra;
//! * la soglia di D9 è provata nelle due direzioni — sotto soglia non c'è
//!   numero, alla soglia c'è — perché una guardia che dice sempre «non c'è» non
//!   sta guarding niente;
//! * l'equivalenza fra le due braccia è provata con **la stessa forma di dati**
//!   sotto due etichette di classe diverse: un criterio dipendente dal braccio
//!   romperebbe l'uguaglianza, e il criterio che torna dentro il risultato dice
//!   quale soglia è stata usata.

use kbs_core::{
    ArgumentId, CohortId, CourseId, Evidence, GraderKind, Millis, PersonId, Relation,
    COHORT_MIN_K,
};

use super::School;
use crate::error::Error;
use crate::padronanza::{
    meter, CohortShare, Criterion, MasteryRow, Proof, Reason, Verdict, DAY_MILLIS,
    MASTERY_CLAIM_THRESHOLD, MASTERY_FRESHNESS_WINDOW, MASTERY_RETENTION_HORIZON,
};
use crate::types::{
    CourseRelation, GradeLevel, GradingDraft, ObservationDraft, Person, Register, Rubric,
    RubricVersion, SessionId,
};

const T0: i64 = 1_700_000_000_000;

/// Il numero che il corpus scrive per il metro: otto esercizi distinti.
///
/// È **una costante di questo file di test**, non un riferimento a
/// `MASTERY_CLAIM_THRESHOLD`, ed è la cosa più importante di tutto il banco: una
/// prova che costruisce i propri dati con la costante che sta per verificare non
/// sta verificando niente — abbassare la soglia abbasserebbe anche i dati e il
/// test resterebbe verde su un metro che non misura più quello che dice. Qui i
/// dati vengono da un numero scritto a mano e l'attesa dalla costante di
/// produzione, così i due non possono coincidere per costruzione.
const OTTO: usize = 8;

/// Il giorno `n` del corso, in millisecondi.
///
/// Il tempo dei test è in giorni e non in millisecondi perché tutte le costanti
/// del criterio sono giorni: un test che scrive `1_209_600_000` a mano non dice a
/// chi lo legge «quattordici giorni», e il lettore è metà del banco.
fn giorno(n: i64) -> Millis {
    Millis(T0 + n * DAY_MILLIS)
}

/// Una prova in memoria, per le prove pure di [`meter`].
fn prova(exercise: &str, giorno_: i64, correct: bool) -> Proof {
    Proof {
        observation: format!("obs-{exercise}-{giorno_}"),
        at: giorno(giorno_),
        exercise: exercise.to_string(),
        instance: format!("seed-{exercise}-{giorno_}"),
        correct,
    }
}

/// `n` esercizi distinti giusti, uno ogni `passo` giorni, a partire da `da`.
fn distinte(n: usize, da: i64, passo: i64) -> Vec<Proof> {
    (0..n)
        .map(|k| prova(&format!("ex-{k}"), da + k as i64 * passo, true))
        .collect()
}

/// Una riga di osservazione con tutto dichiarato, perché la prova è su quale
/// combinazione entra nel metro e quale no.
#[allow(clippy::too_many_arguments)]
fn osservazione(
    id: &str,
    s: &School,
    argomento: &ArgumentId,
    esercizio: &str,
    giorno_: i64,
    correct: bool,
    unaided: Option<bool>,
    judged_by: Option<GraderKind>,
    cohort: &CohortId,
    studente: &PersonId,
) -> ObservationDraft {
    ObservationDraft {
        id: id.to_string(),
        student: studente.clone(),
        course: s.course.clone(),
        cohort: cohort.clone(),
        argument: argomento.clone(),
        evidence: Evidence::Checked {
            exercise: esercizio.to_string(),
            instance: format!("seed-{id}"),
            correct,
        },
        judged_by,
        unaided,
        // Il trigger di `V6` vieta la combinazione ambigua, e la rispettiamo
        // anche qui: un aiuto ignoto non può avere un conteggio di indizi.
        n_hints: match unaided {
            None => None,
            Some(true) => Some(0),
            Some(false) => Some(2),
        },
        at: giorno(giorno_),
    }
}

/// `n` persone nuove, iscritte al corso.
///
/// L'iscrizione non è un dettaglio: senza `enrolled_in` il predicato di D5 dà
/// un metro vuoto, e un metro vuoto in un test è un test che non prova niente.
/// Gli id sono disgiunti per `offset`, perché `add_relation` è un `INSERT` e due
/// iscrizioni identiche sono un errore, non un no-op.
fn compagni(s: &mut School, n: usize, offset: u32) -> Vec<PersonId> {
    (0..n)
        .map(|k| {
            let persona = PersonId::fixture(offset + k as u32);
            s.store
                .upsert_person(&Person {
                    id: persona.clone(),
                    display_name: format!("Compagno {offset}-{k}"),
                    created_at: Millis(T0),
                })
                .expect("persona");
            s.store
                .add_relation(
                    &CourseRelation {
                        person: persona.clone(),
                        course: s.course.clone(),
                        relation: Relation::EnrolledIn,
                        since: Millis(T0),
                        until: None,
                    },
                    &persona,
                )
                .expect("iscrizione");
            persona
        })
        .collect()
}

/// Una classe: per ogni persona, `prove` esercizi distinti giusti e non
/// assistiti, uno ogni tre giorni, con l'ultima fermata al giorno `ultimo`.
fn classe(
    s: &mut School,
    label: &str,
    persone: &[PersonId],
    prove: usize,
    sessione: &SessionId,
    argomento: &ArgumentId,
    ultimo: i64,
) -> CohortId {
    let cohort = CohortId(label.to_string());
    for (n, persona) in persone.iter().enumerate() {
        for k in 0..prove {
            s.store
                .append_observation(
                    sessione,
                    osservazione(
                        &format!("obs-{label}-{n}-{k}"),
                        s,
                        argomento,
                        &format!("ex-{k}"),
                        (k as i64 * 3).min(ultimo),
                        true,
                        Some(true),
                        Some(GraderKind::Deterministic),
                        &cohort,
                        persona,
                    ),
                )
                .expect("osservazione");
        }
    }
    cohort
}

/// La riga del metro dello studente del banco, per un argomento.
fn riga_di(s: &School, argomento: &ArgumentId, at: Millis) -> MasteryRow {
    let righe = s
        .store
        .mastery_for(&s.student, &s.student, &s.course, at)
        .expect("il proprio metro");
    righe
        .iter()
        .find(|r| &r.argument == argomento)
        .expect("l'argomento è nel corso")
        .clone()
}

/// Quante righe di osservazione ci sono sul registro: tutte, assistite comprese.
fn righe_sul_registro(s: &School) -> i64 {
    s.store
        .conn()
        .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
        .expect("il conteggio")
}

// ── il criterio, in isolamento ───────────────────────────────────────────────

#[test]
fn la_soglia_conta_esercizi_distinti_e_non_risposte() {
    // Il criterio economico — «una risposta giusta» — diventa «otto risposte
    // giuste» scrivendo otto seed diversi dello stesso esercizio. Otto risposte
    // diverse allo stesso ragionamento non sono otto prove: D8 dice che il
    // ragionamento è la cosa che si possiede, ed è la stessa risposta data a
    // otto domande diverse.
    let istanze: Vec<Proof> = (0..OTTO)
        .map(|k| {
            let mut p = prova("ex-unico", k as i64, true);
            p.instance = format!("seed-{k}");
            p
        })
        .collect();
    assert_eq!(
        meter(&istanze, Criterion::OF_THE_CLAIM, giorno(40)),
        Verdict::NotProven(Reason::BelowThreshold { proved: 1 }),
        "otto seed dello stesso esercizio non sono otto esercizi"
    );

    let esercizi = distinte(OTTO, 0, 3);
    let ultimo = (OTTO as i64 - 1) * 3;
    assert_eq!(
        meter(&esercizi, Criterion::OF_THE_CLAIM, giorno(ultimo + 1)),
        Verdict::Proven,
        "otto esercizi distinti, spaziati e freschi sono la soglia superata"
    );
}

#[test]
fn una_riga_sola_non_e_dimostrato_e_il_verdetto_dice_quanti_ne_mancano() {
    let righe = distinte(OTTO - 1, 0, 3);
    let verdetto = meter(&righe, Criterion::OF_THE_CLAIM, giorno(40));
    assert_eq!(
        verdetto,
        Verdict::NotProven(Reason::BelowThreshold {
            proved: OTTO - 1,
        }),
        "sotto soglia il verdetto deve dire *quanti* esercizi distinti \
         mancano, non soltanto che non basta"
    );
    assert!(!verdetto.is_proven());
    assert_eq!(verdetto.reason(), Some(Reason::BelowThreshold {
        proved: OTTO - 1,
    }));
    assert_eq!(Verdict::Proven.reason(), None);
}

#[test]
fn otto_prove_tutte_nella_stessa_ora_non_sono_ritenzione() {
    let accalcate = distinte(OTTO, 0, 0);
    assert_eq!(
        meter(&accalcate, Criterion::OF_THE_CLAIM, giorno(1)),
        Verdict::NotProven(Reason::NotSpaced { days: 0 }),
        "otto prove nella stessa ora sono la stessa prova ripetuta: la \
         letteratura che il corpus cita parla di ritenzione, non di acquisizione"
    );
    // Le stesse otto prove, distribuite sull'orizzonte di ritenzione: cambia una
    // sola cosa — il passo fra l'una e l'altra — ed è quella che decide.
    let spaziate = distinte(OTTO, 0, 3);
    let ultimo = (OTTO as i64 - 1) * 3;
    assert_eq!(
        meter(&spaziate, Criterion::OF_THE_CLAIM, giorno(ultimo + 1)),
        Verdict::Proven
    );
}

#[test]
fn cinque_settimane_fa_e_niente_dopo_non_e_dimostrato() {
    // Il caso che il revisore attacca per primo: ha indovinato una volta, molto
    // tempo fa. Qui non è «una volta», sono otto, ma sono tutte vecchie.
    let righe = distinte(OTTO, 0, 3);
    let ultimo = (OTTO as i64 - 1) * 3;
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(ultimo + 1)),
        Verdict::Proven,
        "ieri era vero"
    );
    let quanto = 35;
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(ultimo + quanto)),
        Verdict::NotProven(Reason::Stale { days: quanto }),
        "cinque settimane fa non è adesso: la finestra di freschezza esiste \
         proprio perché il metro non deve certificare il mese scorso"
    );
    // Il compito ritardato è l'atto che rinnova il verdetto: la finestra è
    // «non più di», quindi il giorno della finestra è ancora dentro.
    let limite = MASTERY_FRESHNESS_WINDOW.0 / DAY_MILLIS;
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(ultimo + limite)),
        Verdict::Proven
    );
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(ultimo + limite + 1)),
        Verdict::NotProven(Reason::Stale {
            days: limite + 1
        }),
        "un giorno oltre la finestra è già fuori"
    );
}

#[test]
fn l_ultima_parola_e_l_ultima_verifica() {
    let mut righe = distinte(OTTO, 0, 3);
    let ultimo = (OTTO as i64 - 1) * 3;
    righe.push(prova("ex-8", ultimo + 1, false));
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(ultimo + 2)),
        Verdict::NotProven(Reason::LastWrong),
        "otto prove e poi un errore non assistito: la padronanza si regge \
         sull'ultima parola, non sul totale"
    );
    // La rivincita è una riga come tutte le altre, e nessuna riga nuova di
    // padronanza: è la stessa funzione applicata a un registro più lungo.
    righe.push(prova("ex-9", ultimo + 2, true));
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(ultimo + 3)),
        Verdict::Proven
    );
}

#[test]
fn le_condizioni_hanno_un_ordine_e_l_ordine_e_il_tempo_del_criterio() {
    // Sotto la soglia non si guarda nient'altro: non si guarda se l'ultima è
    // sbagliata, non si guarda se è vecchia.
    let sotto = {
        let mut r = distinte(1, 0, 0);
        r.push(prova("ex-1", 400, false));
        r
    };
    assert_eq!(
        meter(&sotto, Criterion::OF_THE_CLAIM, giorno(401)),
        Verdict::NotProven(Reason::BelowThreshold { proved: 1 })
    );
    // Raggiunta la soglia, la prima cosa che si guarda è l'ultima verifica, non
    // la sua età e non lo spaziamento.
    let accalcate_e_tarde = {
        let mut r = distinte(OTTO, 0, 0);
        r.push(prova("ex-ultima", 40, false));
        r
    };
    assert_eq!(
        meter(&accalcate_e_tarde, Criterion::OF_THE_CLAIM, giorno(41)),
        Verdict::NotProven(Reason::LastWrong)
    );
    // Lo spaziamento viene prima della freschezza, perché è la condizione che
    // distingue l'acquisizione dalla ritenzione: un metro che si fermasse alla
    // freschezza direbbe «tutto bene» di uno che ha risposto bene otto volte
    // tutte insieme tre settimane fa.
    let accalcate = distinte(OTTO, 0, 0);
    assert_eq!(
        meter(&accalcate, Criterion::OF_THE_CLAIM, giorno(40)),
        Verdict::NotProven(Reason::NotSpaced { days: 0 })
    );
    // E la freschezza viene per ultima, quando tutto il resto è vero.
    let spaziate = distinte(OTTO, 0, 3);
    let ultimo = (OTTO as i64 - 1) * 3;
    let quanto = 41;
    assert_eq!(
        meter(&spaziate, Criterion::OF_THE_CLAIM, giorno(ultimo + quanto)),
        Verdict::NotProven(Reason::Stale { days: quanto }),
        "spaziata, ultima parola giusta, e l'unica condizione che manca è \
         l'età dell'ultima verifica"
    );
}

#[test]
fn l_istante_filtra_le_prove_e_il_filtro_vive_nella_funzione() {
    // Le prove sono ai giorni 0, 3, 6, 9, 12, 15, 18, 21. Al giorno 10 se ne
    // sono viste quattro e il verdetto deve contare quattro: è la prova che il
    // predicato sta in `meter` e non soltanto nella `SELECT` che le ha lette.
    let righe = distinte(OTTO, 0, 3);
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(10)),
        Verdict::NotProven(Reason::BelowThreshold { proved: 4 })
    );
    assert_eq!(
        meter(&righe, Criterion::OF_THE_CLAIM, giorno(10)),
        meter(&righe[..4], Criterion::OF_THE_CLAIM, giorno(10)),
        "le stesse quattro prove, filtrate a monte, danno lo stesso verdetto: \
         l'istante è un filtro sul dato e non una proprietà di chi legge"
    );
}

#[test]
fn il_criterio_e_un_valore_e_le_sue_tre_costanti_lo_compongono() {
    assert_eq!(Criterion::OF_THE_CLAIM.threshold, MASTERY_CLAIM_THRESHOLD);
    assert_eq!(
        Criterion::OF_THE_CLAIM.retention_horizon,
        MASTERY_RETENTION_HORIZON
    );
    assert_eq!(
        Criterion::OF_THE_CLAIM.freshness_window,
        MASTERY_FRESHNESS_WINDOW
    );
    // Una soglia a zero non è una soglia bassa: dichiara dimostrato un argomento su
    // cui nessuno ha risposto, e `meter` non ha un ramo che lo sappia fare.
    assert!(
        MASTERY_CLAIM_THRESHOLD >= 1,
        "una soglia a zero rende il metro una funzione vuota: il posto in cui si \
         rompe è la costante, non un'asserzione lontana"
    );
    // Una finestra più stretta dell'orizzonte rende la soglia irraggiungibile:
    // due prove distanti più della finestra non possono stare dentro la
    // finestra, e un criterio che non si può soddisfare non è una soglia alta.
    assert!(
        MASTERY_FRESHNESS_WINDOW.0 > MASTERY_RETENTION_HORIZON.0,
        "la finestra deve contenere l'orizzonte"
    );
}

// ── il criterio, su righe vere ───────────────────────────────────────────────

#[test]
fn otto_esercizi_distinti_senza_aiuto_danno_dimostrato() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for k in 0..OTTO {
        s.store
            .append_observation(
                &sessione,
                osservazione(
                    &format!("obs-{k}"),
                    &s,
                    &arg.id,
                    &format!("ex-{k}"),
                    k as i64 * 3,
                    true,
                    Some(true),
                    Some(GraderKind::Deterministic),
                    &s.cohort(),
                    &s.student,
                ),
            )
            .expect("osservazione");
    }
    let riga = riga_di(&s, &arg.id, giorno(40));
    assert_eq!(riga.proved_exercises, OTTO);
    assert_eq!(riga.verdict, Verdict::Proven, "{riga:?}");
    assert_eq!(riga.proofs.len(), OTTO);
    assert_eq!(riga.criterion, Criterion::OF_THE_CLAIM);
    // Le prove sono **apribili**: ogni riga porta l'id della sua osservazione,
    // perché una riga che dice «otto prove» senza dire quali non è contestabile.
    let id: Vec<&str> = riga.proofs.iter().map(|p| p.observation.as_str()).collect();
    assert_eq!(id[0], "obs-0");
    assert_eq!(id[OTTO - 1], "obs-7");
    assert_eq!(riga.first_proof, Some(giorno(0)));
    assert_eq!(
        riga.last_proof,
        Some(giorno((OTTO as i64 - 1) * 3))
    );
}

#[test]
fn la_coda_di_practice_e_l_aiuto_ignoto_non_entrano_nel_numero() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    // Otto prove che il metro può contare, più due che non deve.
    for k in 0..OTTO {
        s.store
            .append_observation(
                &sessione,
                osservazione(
                    &format!("obs-ok-{k}"),
                    &s,
                    &arg.id,
                    &format!("ex-{k}"),
                    k as i64 * 3,
                    true,
                    Some(true),
                    Some(GraderKind::Deterministic),
                    &s.cohort(),
                    &s.student,
                ),
            )
            .expect("osservazione");
    }
    for (id, esercizio, unaided) in [
        ("obs-assistita", "ex-assistita", Some(false)),
        ("obs-ignota", "ex-ignota", None),
    ] {
        s.store
            .append_observation(
                &sessione,
                osservazione(
                    id,
                    &s,
                    &arg.id,
                    esercizio,
                    2,
                    true,
                    unaided,
                    Some(GraderKind::Deterministic),
                    &s.cohort(),
                    &s.student,
                ),
            )
            .expect("una riga come tutte le altre");
    }
    // Il registro le ha tutte: la prova non è che il filtro ha funzionato, è
    // che le righe esistono e il metro non le ha contate.
    assert_eq!(righe_sul_registro(&s), (OTTO + 2) as i64);

    let riga = riga_di(&s, &arg.id, giorno(40));
    assert_eq!(riga.proved_exercises, OTTO);
    assert_eq!(riga.verdict, Verdict::Proven);
    let id: Vec<&str> = riga.proofs.iter().map(|p| p.observation.as_str()).collect();
    assert!(
        !id.contains(&"obs-assistita") && !id.contains(&"obs-ignota"),
        "la coda di practice e l'aiuto ignoto non possono stare nelle prove: {id:?}"
    );

    // E adesso il caso che rende la prova non vacua: una delle otto prove non
    // assistite in meno, con le due escluse che tornerebbero a far salire il
    // numero se il filtro non reggesse.
    let mut s2 = School::new();
    let arg2 = s2.published(1);
    let sessione2 = s2.session(Register::Observations);
    for k in 0..OTTO - 1 {
        s2.store
            .append_observation(
                &sessione2,
                osservazione(
                    &format!("obs-ok-{k}"),
                    &s2,
                    &arg2.id,
                    &format!("ex-{k}"),
                    k as i64 * 3,
                    true,
                    Some(true),
                    Some(GraderKind::Deterministic),
                    &s2.cohort(),
                    &s2.student,
                ),
            )
            .expect("osservazione");
    }
    for (id, esercizio, unaided) in [
        ("obs-assistita", "ex-assistita", Some(false)),
        ("obs-ignota", "ex-ignota", None),
    ] {
        s2.store
            .append_observation(
                &sessione2,
                osservazione(
                    id,
                    &s2,
                    &arg2.id,
                    esercizio,
                    3,
                    true,
                    unaided,
                    Some(GraderKind::Deterministic),
                    &s2.cohort(),
                    &s2.student,
                ),
            )
            .expect("osservazione");
    }
    assert_eq!(righe_sul_registro(&s2), (OTTO + 1) as i64);
    let riga2 = riga_di(&s2, &arg2.id, giorno(40));
    assert_eq!(
        riga2.verdict,
        Verdict::NotProven(Reason::BelowThreshold {
            proved: OTTO - 1
        }),
        "sette prove non assistite più due escluse non fanno otto"
    );
}

#[test]
fn un_giudizio_di_pari_o_di_persona_non_muove_il_metro() {
    // «Lo stesso item, lo stesso verificatore» è la condizione che rende
    // confrontabili le due braccia: se il metro accettasse un giudizio umano, i
    // due numeri misurerebbero anche chi ha giudicato, e in una classe è il
    // docente e nell'altra è un pari.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for (k, giudicatore) in [GraderKind::Peer, GraderKind::Human, GraderKind::Teacher]
        .into_iter()
        .enumerate()
    {
        for j in 0..3 {
            s.store
                .append_observation(
                    &sessione,
                    osservazione(
                        &format!("obs-{k}-{j}"),
                        &s,
                        &arg.id,
                        &format!("ex-{k}-{j}"),
                        k as i64 * 3 + j as i64,
                        true,
                        Some(true),
                        Some(giudicatore),
                        &s.cohort(),
                        &s.student,
                    ),
                )
                .expect("osservazione");
        }
    }
    assert_eq!(righe_sul_registro(&s), 9, "le righe ci sono tutte");
    let riga = riga_di(&s, &arg.id, giorno(40));
    assert!(
        riga.proofs.is_empty(),
        "un giudizio umano non è il programma di D8: resta nel registro e non \
         muove il metro"
    );
    assert_eq!(riga.verdict, Verdict::NotProven(Reason::NoProof));
    // E il docente li vede lo stesso: il metro non è un filtro di visibilità,
    // è un criterio. Se non li vedesse, la riga che il giudizio non muove
    // sarebbe anche una riga che scompare, e sarebbe una perdita di dati.
    let del_docente = s
        .store
        .observations_for(&s.teacher, &s.student, &arg.id)
        .expect("il registro del suo studente");
    assert_eq!(del_docente.len(), 9);
}

#[test]
fn perdere_e_riprendere_non_aggiungono_una_riga_di_padronanza() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    for k in 0..OTTO {
        s.store
            .append_observation(
                &sessione,
                osservazione(
                    &format!("obs-{k}"),
                    &s,
                    &arg.id,
                    &format!("ex-{k}"),
                    k as i64 * 3,
                    true,
                    Some(true),
                    Some(GraderKind::Deterministic),
                    &s.cohort(),
                    &s.student,
                ),
            )
            .expect("osservazione");
    }
    let prima = righe_sul_registro(&s);
    assert_eq!(riga_di(&s, &arg.id, giorno(40)).verdict, Verdict::Proven);

    // La perdita è una riga di osservazione come tutte le altre.
    s.store
        .append_observation(
            &sessione,
            osservazione(
                "obs-caduta",
                &s,
                &arg.id,
                "ex-caduta",
                30,
                false,
                Some(true),
                Some(GraderKind::Deterministic),
                &s.cohort(),
                &s.student,
            ),
        )
        .expect("l'errore non assistito");
    assert_eq!(
        righe_sul_registro(&s),
        prima + 1,
        "una perdita non è una riga in più: è un verdetto diverso"
    );
    assert_eq!(
        riga_di(&s, &arg.id, giorno(31)).verdict,
        Verdict::NotProven(Reason::LastWrong)
    );

    // E la rivincita è un'altra osservazione, non un verdetto riscritto.
    s.store
        .append_observation(
            &sessione,
            osservazione(
                "obs-ritornata",
                &s,
                &arg.id,
                "ex-ritornata",
                32,
                true,
                Some(true),
                Some(GraderKind::Deterministic),
                &s.cohort(),
                &s.student,
            ),
        )
        .expect("il ritorno");
    assert_eq!(riga_di(&s, &arg.id, giorno(33)).verdict, Verdict::Proven);

    // La prova che il metro non scrive: nessuna tabella nuova. Un verdetto
    // scritto sarebbe stato un giudizio dentro il registro delle prove, e la
    // catena di hash di D6 lo avrebbe pagato come se fosse una dimostrazione.
    let mut stmt = s
        .store
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .expect("sqlite_master");
    let tabelle: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("nomi")
        .filter_map(|n| n.ok())
        .collect();
    for tabella in tabelle {
        let nome = tabella.to_lowercase();
        assert!(
            !nome.contains("padronanza") && !nome.contains("mastery") && !nome.contains("meter"),
            "il metro non ha una tabella: ha trovato `{tabella}`"
        );
    }
}

// ── la quota: le due braccia e la soglia di D9 ───────────────────────────────

#[test]
fn la_soglia_e_la_stessa_in_entrambe_le_braccia() {
    // Tre classi, **una sola soglia**, e i dati vengono da numeri scritti a mano.
    //
    // * `trattamento` ha otto esercizi distinti: **alla** soglia dichiarata, e la
    //   quota è tutta.
    // * `controllo` ne ha sette: sopra `k` persone e sotto la soglia, e la quota
    //   è zero. È la classe che rende il confronto non vacuo — se la soglia
    //   dipendesse dal braccio, o se la soglia scendesse, questa riga cambierebbe.
    // * `gemella` ha gli stessi dati di `trattamento` sotto un'altra etichetta: la
    //   quota deve essere identica, e l'etichetta è la sola cosa che cambia. È la
    //   prova che nel codice non c'è un ramo che distingue le braccia.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    let persone = COHORT_MIN_K + 1;
    let del_trattamento = compagni(&mut s, persone, 100);
    let del_controllo = compagni(&mut s, persone, 200);
    let della_gemella = compagni(&mut s, persone, 300);
    let trattamento = classe(
        &mut s,
        "trattamento",
        &del_trattamento,
        OTTO,
        &sessione,
        &arg.id,
        40,
    );
    let controllo = classe(
        &mut s,
        "controllo",
        &del_controllo,
        OTTO - 1,
        &sessione,
        &arg.id,
        40,
    );
    let gemella = classe(
        &mut s,
        "gemella",
        &della_gemella,
        OTTO,
        &sessione,
        &arg.id,
        40,
    );
    let da = giorno(0);
    let a = giorno(40);
    let quota_di = |cohort: &kbs_core::CohortId| {
        s.store
            .mastery_share(&s.teacher, &s.course, cohort, da, a)
            .expect("quota")
            .expect("sopra soglia")
    };
    let t = quota_di(&trattamento);
    let c = quota_di(&controllo);
    let g = quota_di(&gemella);

    // Il criterio è lo stesso, e l'attesa viene dalla costante di produzione.
    for (nome, quota) in [("trattamento", &t), ("controllo", &c), ("gemella", &g)] {
        assert_eq!(
            quota.criterion,
            Criterion::OF_THE_CLAIM,
            "la classe `{nome}` è stata valutata con un criterio diverso"
        );
        assert_eq!(
            quota.criterion.threshold, MASTERY_CLAIM_THRESHOLD,
            "la classe `{nome}` è stata valutata con una soglia diversa"
        );
        assert_eq!(quota.students, persone, "la classe `{nome}` ha le persone giuste");
    }
    assert_eq!(t.criterion, c.criterion, "un solo criterio per le due braccia");

    // I dati decidono, l'etichetta no.
    assert_eq!(
        t.quota, 1.0,
        "otto esercizi distinti senza aiuto: ogni persona della classe passa"
    );
    assert_eq!(
        c.quota, 0.0,
        "sette esercizi distinti non sono la soglia dichiarata: nessuno passa"
    );
    assert_eq!(g.quota, t.quota, "gli stessi dati danno la stessa quota");
    assert_eq!(g.transitions, t.transitions);
    assert_eq!(
        t.transitions, persone,
        "una transizione per persona che arriva: la quota conta il passaggio"
    );
    assert_eq!(c.transitions, 0);
}

#[test]
fn la_quota_si_muove_e_conta_una_transizione_e_non_uno_stato() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    // Chi arriva: otto prove entro il giorno 10, cioè tutte accalcate e quindi
    // fuori dalla soglia di ritenzione. Nessuno è dimostrato.
    let arrivano = compagni(&mut s, COHORT_MIN_K, 500);
    let cohort = classe(
        &mut s,
        "arrivano",
        &arrivano,
        OTTO,
        &sessione,
        &arg.id,
        10,
    );
    // Chi non arriva: le stesse prove, ma una in meno.
    let altri = compagni(&mut s, COHORT_MIN_K, 600);
    let restano = classe(
        &mut s,
        "restano",
        &altri,
        OTTO - 1,
        &sessione,
        &arg.id,
        10,
    );
    // Il compito ritardato: una verifica non assistita al giorno 30, che è
    // l'unica cosa che trasforma una soglia in una transizione.
    for (n, persona) in arrivano.iter().enumerate() {
        s.store
            .append_observation(
                &sessione,
                osservazione(
                    &format!("obs-tardiva-{n}"),
                    &s,
                    &arg.id,
                    "ex-tardiva",
                    30,
                    true,
                    Some(true),
                    Some(GraderKind::Deterministic),
                    &cohort,
                    persona,
                ),
            )
            .expect("osservazione");
    }
    let quota = s
        .store
        .mastery_share(&s.teacher, &s.course, &cohort, giorno(0), giorno(31))
        .expect("quota")
        .expect("sopra soglia");
    assert_eq!(
        quota.transitions, COHORT_MIN_K,
        "una transizione per persona: la quota conta il passaggio, non lo stato"
    );
    assert_eq!(quota.students, COHORT_MIN_K);
    assert_eq!(quota.arguments, 1);
    assert_eq!(quota.quota, 1.0);
    assert_eq!(quota.criterion, Criterion::OF_THE_CLAIM);

    // L'altra classe, alla stessa finestra: nessuno passa, e la quota è zero
    // come numero e non come assenza. La differenza fra `1.0` e `0.0` è la prova
    // che la quota è calcolata e non una costante travestita da numero.
    let nessuno = s
        .store
        .mastery_share(&s.teacher, &s.course, &restano, giorno(0), giorno(31))
        .expect("quota")
        .expect("sopra soglia");
    assert_eq!(nessuno.transitions, 0);
    assert_eq!(nessuno.quota, 0.0);

    // Chi era già dimostrato non passa una seconda volta: la finestra non si
    // sposta, quindi la stessa classe nella stessa finestra non registra nulla.
    let stessa = s
        .store
        .mastery_share(&s.teacher, &s.course, &cohort, giorno(31), giorno(31))
        .expect("quota")
        .expect("sopra soglia");
    assert_eq!(
        stessa.transitions, 0,
        "una finestra senza prove nuove non registra nessun passaggio"
    );
}

#[test]
fn sotto_soglia_d9_la_quota_non_esiste_e_alla_soglia_esiste() {
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    let sotto_persone = compagni(&mut s, COHORT_MIN_K - 1, 700);
    let sotto = classe(
        &mut s,
        "sotto",
        &sotto_persone,
        OTTO,
        &sessione,
        &arg.id,
        40,
    );
    assert!(
        s.store
            .mastery_share(&s.teacher, &s.course, &sotto, giorno(0), giorno(40))
            .expect("la quota sotto soglia è un'assenza, non un errore")
            .is_none(),
        "sotto `COHORT_MIN_K` la quota non esiste: non è uno zero e non è un \
         flag, è l'assenza del numero"
    );

    // La stessa forma di dati con una persona in più: il numero compare. Senza
    // questa seconda metà il test sopra passerebbe anche con una funzione che
    // non restituisce mai niente.
    let sopra_persone = compagni(&mut s, COHORT_MIN_K, 800);
    let sopra = classe(
        &mut s,
        "sopra",
        &sopra_persone,
        OTTO,
        &sessione,
        &arg.id,
        40,
    );
    let quota = s
        .store
        .mastery_share(&s.teacher, &s.course, &sopra, giorno(0), giorno(40))
        .expect("quota")
        .expect("alla soglia esatta la quota esiste");
    assert_eq!(quota.students, COHORT_MIN_K);
    assert!(quota.quota > 0.0);
}

// ── chi può leggere il metro ─────────────────────────────────────────────────

#[test]
fn lo_studente_legge_il_proprio_metro_e_il_docente_quello_del_suo_studente() {
    let mut s = School::new();
    s.published(1);
    let proprio = s
        .store
        .mastery_for(&s.student, &s.student, &s.course, giorno(40))
        .expect("il proprio metro");
    let del_docente = s
        .store
        .mastery_for(&s.teacher, &s.student, &s.course, giorno(40))
        .expect("il metro del suo studente");
    assert_eq!(
        proprio, del_docente,
        "è lo stesso metro: cambia solo chi chiede"
    );
    assert!(!proprio.is_empty());
}

#[test]
fn chi_non_e_lo_studente_e_non_insegna_riceve_la_risposta_di_un_corso_inesistente() {
    let s = School::new();
    let estraneo = s
        .store
        .mastery_for(&s.outsider, &s.student, &s.course, giorno(40))
        .expect_err("uno che non c'entra non legge il metro di nessuno");
    let inesistente = s
        .store
        .mastery_for(
            &s.outsider,
            &s.student,
            &CourseId("corso_inesistente".to_string()),
            giorno(40),
        )
        .expect_err("un corso che non esiste non è un corso vuoto");
    assert!(matches!(estraneo, Error::NotReadable { .. }));
    assert!(matches!(inesistente, Error::NotReadable { .. }));
    assert_eq!(
        estraneo.to_string(),
        inesistente.to_string(),
        "«non c'è» e «non lo vedi» sono lo stesso testo: la forma degli id è \
         enumerabile e non può distinguere i due casi"
    );
}

#[test]
fn un_giudice_non_ha_diritto_sulla_somma() {
    // La difesa procedurale di un giudizio è poterlo contestare, e l'ha: chi ha
    // emesso un giudizio legge **quel** giudizio. Il metro è la somma, e la
    // somma di ciò che hai valutato tu non è un diritto che ti spetta. Qui la
    // prova è su due letture della stessa persona: una che passa e una no.
    let mut s = School::new();
    let arg = s.published(1);
    s.store
        .upsert_rubric(&Rubric {
            id: "rub-1".to_string(),
            course: s.course.clone(),
            title: "Griglia del compito".to_string(),
            created_at: Millis(T0),
        })
        .expect("rubrica");
    s.store
        .upsert_rubric_version(&RubricVersion {
            id: "rub-1/1".to_string(),
            rubric: "rub-1".to_string(),
            version: "1".to_string(),
            scale: vec![GradeLevel {
                grade: "B".to_string(),
                label: "Mostra".to_string(),
                points: 1.0,
            }],
            defined_at: Millis(T0),
            note: "prima versione".to_string(),
        })
        .expect("versione");
    // Il pari che ha emesso il giudizio è iscritto al corso: senza l'iscrizione
    // non potrebbe nemmeno leggere l'argomento, e il diritto che qui si prova non
    // sarebbe quello del giudice ma quello di chi vede il materiale.
    s.store
        .add_relation(
            &CourseRelation {
                person: s.outsider.clone(),
                course: s.course.clone(),
                relation: Relation::EnrolledIn,
                since: Millis(T0),
                until: None,
            },
            &s.outsider,
        )
        .expect("iscrizione del pari");
    let sessione = s.session(Register::Gradings);
    s.store
        .append_grading(
            &sessione,
            GradingDraft {
                id: "grd-1".to_string(),
                student: s.student.clone(),
                course: s.course.clone(),
                argument: arg.id.clone(),
                kind: GraderKind::Peer,
                graded_by: s.outsider.clone(),
                rubric_version: "rub-1/1".to_string(),
                grade: "B".to_string(),
                at: Millis(T0),
                contested: None,
            },
        )
        .expect("giudizio");

    let suoi = s
        .store
        .gradings_for(&s.outsider, &s.student, &arg.id)
        .expect("chi ha emesso il giudizio legge il proprio giudizio");
    assert_eq!(
        suoi.len(),
        1,
        "il diritto minimo che copre quello che ha fatto c'è, e passa"
    );
    s.store
        .mastery_for(&s.outsider, &s.student, &s.course, giorno(40))
        .expect_err("il metro no: la somma non è un diritto di chi ha emesso un giudizio");
}

#[test]
fn la_quota_di_classe_e_solo_del_docente() {
    let s = School::new();
    let dello_studente = s
        .store
        .mastery_share(
            &s.student,
            &s.course,
            &s.cohort(),
            giorno(0),
            giorno(40),
        )
        .expect_err("uno studente non legge l'aggregato della sua classe");
    assert!(matches!(dello_studente, Error::NotReadable { .. }));
    let di_altro_corso = s
        .store
        .mastery_share(
            &s.other_teacher,
            &s.course,
            &s.cohort(),
            giorno(0),
            giorno(40),
        )
        .expect_err("un docente di un altro corso non legge questa classe");
    assert!(matches!(di_altro_corso, Error::NotReadable { .. }));
}

/// La quota è la claim, ed è un intero sopra un intero. Se il tipo cambia il
/// test smette di compilare, che è la protezione più economica che ci sia; e il
/// criterio viaggia dentro il corpo, perché una quota senza criterio è un numero
/// senza unità.
#[test]
fn la_quota_e_un_numero_su_un_numero_e_dichiara_il_criterio() {
    let q = CohortShare {
        course: CourseId("c".into()),
        cohort: CohortId("2A".into()),
        from: giorno(0),
        to: giorno(40),
        criterion: Criterion::OF_THE_CLAIM,
        students: COHORT_MIN_K,
        arguments: 40,
        transitions: 8,
        quota: 8.0 / (COHORT_MIN_K * 40) as f64,
    };
    assert!(q.quota > 0.0 && q.quota < 1.0);
    let corpo = serde_json::to_string(&q).expect("la quota è serializzabile");
    assert!(corpo.contains("\"criterion\""), "{corpo}");
    assert!(corpo.contains("\"threshold\""), "{corpo}");
    assert!(corpo.contains("\"transitions\""), "{corpo}");
}
