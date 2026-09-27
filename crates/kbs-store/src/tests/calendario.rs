//! Il calendario, e le quattro cose che deve dimostrare.
//!
//! Qui non si asserisce che una funzione è stata chiamata: si asserisce **che
//! cosa torna**. Un test che dice «il filtro è stato applicato» passa anche
//! quando il filtro filtra tutto, e una coda di richiamo che filtra tutto è
//! una coda vuota che il docente legge come «non ha ancora fatto niente».
//!
//! I quattro fatti, e il perché di ognuno:
//!
//! 1. **la superficie non assistita è la vista, e la vista non contiene le
//!    righe assistite né quelle di aiuto ignoto** — perché il calendario, lo
//!    studente e la coorte di D9 devono prendere le righe dallo stesso posto;
//! 2. **l'orizzonte è del chiamante** — perché questo crate non contiene un
//!    numero di giorni e un test che lo lascia passare è un test che non ha
//!    letto il proprio modulo;
//! 3. **l'ordine è per data e non per merito** — perché un elenco di persone
//!    ordinato per una misura è una graduatoria;
//! 4. **la coda non scrive** — perché una vista che conserva è un secondo
//!    archivio della stessa cosa, e l'append-only è già in tensione con il
//!    diritto all'oblio senza che se ne aggiunga un terzo.

use kbs_core::{ArgumentId, CourseId, Evidence, Millis};

use crate::error::Error;
use crate::tests::School;
use crate::types::{ObservationDraft, Register, SessionId};

const T0: i64 = 1_700_000_000_000;

/// Una dimostrazione, con tutto quello che la riga può portare.
///
/// I cinque parametri sono i cinque campi che decidono se una riga entra nel
/// calendario: `unaided`, `n_hints`, il tipo di prova, il verdetto e l'istante.
/// Tenerli tutti in un unico costruttore è il modo perché un test che cambia un
/// campo per provare che il calendario lo ignora stia cambiando **quello** e non
/// un altro.
fn dimostrazione(
    s: &School,
    argomento: &ArgumentId,
    id: &str,
    unaided: Option<bool>,
    n_hints: Option<u32>,
    correct: Option<bool>,
    at: i64,
) -> ObservationDraft {
    ObservationDraft {
        id: id.to_string(),
        student: s.student.clone(),
        course: s.course.clone(),
        cohort: s.cohort(),
        argument: argomento.clone(),
        evidence: match correct {
            // `None` non è una prova non data: è una prova che il programma non
            // ha valutato, ed è l'unico modo che questa funzione ha per dire
            // «l'ultima volta non lo so».
            None => Evidence::Oral {
                note: "interrogazione".into(),
                witness: None,
            },
            Some(c) => Evidence::Checked {
                exercise: format!("ex-{id}"),
                instance: format!("seed-{id}"),
                correct: c,
            },
        },
        // Il giudicatore segue la prova: `Evidence::Oral` con
        // `judged_by = Deterministic` sarebbe una contraddizione scritta a mano,
        // e questo file non vuole essere la fonte di uno stato che nessun
        // percorso di prodotto produce.
        judged_by: Some(match correct {
            None => kbs_core::GraderKind::Human,
            Some(_) => kbs_core::GraderKind::Deterministic,
        }),
        unaided,
        n_hints,
        at: Millis(at),
    }
}

/// Scrive una dimostrazione.
///
/// Il doc non promette un id: la funzione restituisce `()` e il test tiene gli
/// id che ha scelto, perché sono parte di ciò che il test sta scrivendo e non
/// una cosa che la funzione deve sapere.
fn scrivi(
    s: &mut School,
    sessione: &SessionId,
    argomento: &ArgumentId,
    id: &str,
    unaided: Option<bool>,
    n_hints: Option<u32>,
    correct: Option<bool>,
    at: i64,
) {
    // La riga si costruisce prima: `append_observation` ha bisogno di `&mut` sul
    // negozio e `dimostrazione` ha bisogno di `&`, e metterli nella stessa
    // espressione è una riga in cui due prestiti si incontrano.
    let draft = dimostrazione(s, argomento, id, unaided, n_hints, correct, at);
    s.store
        .append_observation(sessione, draft)
        .expect("dimostrazione");
}

fn ids(calendario: &crate::calendario::Calendar) -> Vec<String> {
    calendario
        .entries
        .iter()
        .map(|v| v.argument.0.clone())
        .collect()
}

/// Quante righe ha il registro, e nient'altro.
fn righe(s: &School) -> i64 {
    s.store
        .conn()
        .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
        .expect("conteggio")
}

#[test]
fn la_coda_contiene_le_ricomparse_e_niente_altro() {
    // Tre argomenti, tre storie diverse, e la coda ne contiene uno.
    //
    // * il primo ha **solo** tentativi assistiti: è la coda di practice, e la
    //   coda di practice non è una ricomparsa;
    // * il secondo ha **solo** righe di aiuto ignoto: `NULL = 1` non è vero, e
    //   la vista non le contiene (V6, non questo file);
    // * il terzo ha una ricomparsa non assistita: è l'unica che deve comparire.
    //
    // Il confronto è su **quanti** e su **quale**: se la coda contenesse anche le
    // assistite, `ids` avrebbe tre voci; se non contenesse nulla, nessuna.
    let mut s = School::new();
    let solo_assistite = s.published(1);
    let solo_ignote = s.published(2);
    let ricomparsa = s.published(3);
    let sessione = s.session(Register::Observations);

    for n in 0..3 {
        scrivi(
            &mut s,
            &sessione,
            &solo_assistite.id,
            &format!("a{n}"),
            Some(false),
            Some(2),
            Some(n == 0),
            T0,
        );
    }
    for n in 0..2 {
        scrivi(
            &mut s,
            &sessione,
            &solo_ignote.id,
            &format!("g{n}"),
            None,
            None,
            Some(n == 0),
            T0,
        );
    }
    scrivi(
        &mut s,
        &sessione,
        &ricomparsa.id,
        "u0",
        Some(true),
        Some(0),
        Some(true),
        T0,
    );

    let calendario = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("la coda del docente");
    assert_eq!(ids(&calendario), vec![ricomparsa.id.0.clone()]);
    assert_eq!(calendario.entries[0].unaided, 1);
    assert_eq!(calendario.entries[0].last_correct, Some(true));

    // E la prova che le righe ci sono tutte e il calendario le ha escluse: un
    // elenco vuoto e un elenco giusto hanno la stessa forma, e la differenza
    // fra i due è il numero.
    assert_eq!(righe(&s), 6, "il registro ha tutte le righe che il test ha scritto");
}

#[test]
fn il_conteggio_e_di_ricomparse_e_non_di_riprove() {
    // Due ricomparse sullo stesso argomento: il conteggio è due e la data è
    // l'ultima. È la prova che la voce non sta valutando lo studente e sta
    // contando le sue ricomparse.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    scrivi(&mut s, &sessione, &arg.id, "u0", Some(true), Some(0), Some(false), T0);
    scrivi(&mut s, &sessione, &arg.id, "u1", Some(true), Some(0), Some(true), T0 + 5_000);

    let calendario = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("la coda");
    assert_eq!(calendario.entries.len(), 1);
    assert_eq!(calendario.entries[0].unaided, 2, "due ricomparse, non una persona");
    assert_eq!(calendario.entries[0].last_unaided, Millis(T0 + 5_000));
    // L'ultima, non la prima e non la migliore: la coda dice quando è successo
    // l'ultima volta, e non sceglie quale delle due conta.
    assert_eq!(calendario.entries[0].last_correct, Some(true));
}

#[test]
fn l_ultimo_verdetto_ignoto_non_e_un_verdetto_negativo() {
    // Una ricomparsa non assistita la cui prova **non** è stata valutata dal
    // programma. `last_correct` deve essere `None` e non `Some(false)`: sono due
    // fatti diversi, ed è la stessa distinzione che rende `unaided` nullable
    // invece che `bool` con un default.
    //
    // **Questa riga non la produce nessun percorso di prodotto, e il test lo
    // dice perché un lettore deve saperlo.** Gli unici due scrittori sono
    // `kbs_intake::pratica`, che scrive sempre `Evidence::Checked` con
    // `judged_by = Deterministic`, e `kbs_intake::diagnosis`, che scrive
    // `unaided = None` e quindi non entra nella vista. Lo stato è raggiungibile
    // solo da SQL scritto a mano, e il calendario deve saperlo dire senza
    // tradurlo in un verdetto: è la ragione per cui il tipo ha tre valori e non
    // due.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    scrivi(&mut s, &sessione, &arg.id, "u0", Some(true), Some(0), Some(true), T0);
    scrivi(&mut s, &sessione, &arg.id, "u1", Some(true), Some(0), None, T0 + 1_000);

    let calendario = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("la coda");
    assert_eq!(
        calendario.entries[0].last_correct, None,
        "una prova non valutata non è una risposta sbagliata"
    );
}

#[test]
fn l_orizzonte_e_del_chiamante_e_il_prodotto_non_ne_ha_uno() {
    // Tre ricomparse a tre istanti. Il prodotto non sceglie l'orizzonte: glielo
    // danno tre chiamate diverse e le tre risposte sono tre verdetti diversi.
    let mut s = School::new();
    let vecchio = s.published(1);
    let medio = s.published(2);
    let nuovo = s.published(3);
    let sessione = s.session(Register::Observations);
    scrivi(&mut s, &sessione, &vecchio.id, "u0", Some(true), Some(0), Some(true), T0);
    scrivi(&mut s, &sessione, &medio.id, "u1", Some(true), Some(0), Some(true), T0 + 10_000);
    scrivi(&mut s, &sessione, &nuovo.id, "u2", Some(true), Some(0), Some(true), T0 + 20_000);

    // Senza orizzonte la risposta è l'elenco e nessun verdetto: `None` su tutte
    // e tre, e non `false`. È la differenza fra «non lo so» e «no».
    let senza = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("senza orizzonte");
    assert_eq!(senza.horizon, None);
    assert!(
        senza.entries.iter().all(|v| v.beyond.is_none()),
        "senza orizzonte dichiarato nessuna voce ha un verdetto"
    );
    assert_eq!(senza.beyond(), 0, "e il conteggio dei «vecchi» è zero, non tre");

    // Con un orizzonte il più vecchio è oltre e il più recente no.
    let con = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, Some(Millis(T0 + 15_000)))
        .expect("con orizzonte");
    let oltre: Vec<&str> = con
        .entries
        .iter()
        .filter(|v| v.beyond == Some(true))
        .map(|v| v.argument.as_str())
        .collect();
    assert_eq!(oltre, vec![vecchio.id.as_str(), medio.id.as_str()]);
    assert_eq!(con.beyond(), 2);

    // E un orizzonte **più antico** non cambia l'elenco e restringe il verdetto:
    // la coda non è una scelta, è un fatto con un orizzonte accanto. Con
    // `T0 - 1` nessuna delle tre ricomparse è più vecchia dell'orizzonte,
    // quindi il conteggio dei «vecchi» scende a zero — e questa è la metà non
    // vacua: un prodotto che scegliesse l'orizzonte da sé non cambierebbe nulla
    // passando da una chiamata all'altra.
    let prima = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, Some(Millis(T0 - 1)))
        .expect("orizzonte precedente");
    assert_eq!(ids(&prima), ids(&senza), "l'ordine non dipende dall'orizzonte");
    assert_eq!(prima.beyond(), 0, "un orizzonte prima della prima ricomparsa non rende vecchio niente");

    // Lo stesso orizzonte spostato in avanti di un millisecondo sopra l'ultima
    // ricomparsa li rende vecchi tutti e tre: è l'unico numero che la funzione
    // calcola, e dipende interamente dall'istante che il chiamante ha passato.
    let dopo = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, Some(Millis(T0 + 20_001)))
        .expect("orizzonte successivo");
    assert_eq!(dopo.beyond(), 3);
    assert_eq!(ids(&dopo), ids(&prima), "l'elenco è lo stesso: cambia il verdetto, non la coda");
}

#[test]
fn l_elenco_e_per_data_e_mai_per_merito() {
    // L'argomento in cima è quello dimostrato **male** e più vecchio. Se
    // l'ordinamento fosse per merito, in testa ci sarebbe quello giusto.
    let mut s = School::new();
    let giusto_e_vecchio = s.published(1);
    let sbagliato_e_recente = s.published(2);
    let sbagliato_e_antico = s.published(3);
    let sessione = s.session(Register::Observations);
    scrivi(
        &mut s,
        &sessione,
        &giusto_e_vecchio.id,
        "u0",
        Some(true),
        Some(0),
        Some(true),
        T0,
    );
    scrivi(
        &mut s,
        &sessione,
        &sbagliato_e_recente.id,
        "u1",
        Some(true),
        Some(0),
        Some(false),
        T0 + 10_000,
    );
    scrivi(
        &mut s,
        &sessione,
        &sbagliato_e_antico.id,
        "u2",
        Some(true),
        Some(0),
        Some(false),
        T0 - 10_000,
    );

    let calendario = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("la coda");
    assert_eq!(
        ids(&calendario),
        vec![
            sbagliato_e_antico.id.0.clone(),
            giusto_e_vecchio.id.0.clone(),
            sbagliato_e_recente.id.0.clone(),
        ],
        "l'ordine è per data crescente e non per merito: un elenco ordinato per valore \
         è un invito a usarlo come graduatoria"
    );
}

#[test]
fn lo_studente_non_ha_una_coda_e_l_estraneo_meno() {
    // Due rifiuti, e la ragione di ognuno è diversa.
    //
    // * lo **studente** che chiede la propria coda è il caso che il corpus
    //   vieta: un elenco di ciò che resta da fare, esposto a chi lo deve fare.
    //   `18`, `#il-test-icap-su-una-verifica`: «La vista dello studente non mostra
    //   percentuali: mostra righe con prova e un bottone per contestarle».
    // * l'**estraneo** non ha nessuna relazione, e la risposta è la stessa di un
    //   corso che non esiste: due risposte diverse sarebbero un canale.
    let mut s = School::new();
    let _arg = s.published(1);
    let sessione = s.session(Register::Observations);
    scrivi(&mut s, &sessione, &_arg.id, "u0", Some(true), Some(0), Some(true), T0);

    let proprio = s
        .store
        .calendar(&s.student, &s.student, &s.course, None)
        .expect_err("lo studente non chiede la propria coda");
    let estraneo = s
        .store
        .calendar(&s.outsider, &s.student, &s.course, None)
        .expect_err("un estraneo non legge il registro di nessuno");
    let inesistente = s
        .store
        .calendar(&s.outsider, &s.student, &CourseId::fixture(9), None)
        .expect_err("un corso inesistente non è un corso che si insegna");

    assert!(
        matches!(proprio, Error::NotReadable { .. })
            && matches!(estraneo, Error::NotReadable { .. })
            && matches!(inesistente, Error::NotReadable { .. }),
        "i tre rifiuti sono la stessa risposta: {proprio:?}, {estraneo:?}, {inesistente:?}"
    );

    // E sono gli stessi byte una volta tolti gli id, che li ha chiesti il
    // chiamante: l'id della persona che chiede e l'id della coda che chiede. Un
    // divieto e un'assenza non possono essere due messaggi diversi, o la
    // differenza dice che quel corso c'è.
    //
    // Nota che `proprio` ha una persona **diversa** che chiede dallo studente
    // della coda, e nonostante ciò il testo è lo stesso: è la prova che
    // l'identità del richiedente non entra nel messaggio di rifiuto.
    let senza_id = |e: &Error| {
        e.to_string()
            .split_whitespace()
            .filter(|w| {
                !w.starts_with("arg_")
                    && !w.starts_with("calendario:")
                    && !w.starts_with("person_")
                    && !w.starts_with("course_")
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(senza_id(&estraneo), senza_id(&inesistente));
    assert_eq!(senza_id(&proprio), senza_id(&estraneo));
}

#[test]
fn la_coda_non_scrive_una_riga() {
    // La promessa di conservazione, resa un numero: prima e dopo la coda il
    // registro ha lo stesso numero di righe. Una vista che conserva è un
    // secondo archivio della stessa cosa, e «la cancellazione è incompatibile
    // con l'append-only, e non è un dettaglio implementativo» (18, `#limiti`):
    // aggiungere un archivio che nessuno cancella peggiora la tensione invece di
    // risolverla.
    let mut s = School::new();
    let arg = s.published(1);
    let sessione = s.session(Register::Observations);
    scrivi(&mut s, &sessione, &arg.id, "u0", Some(true), Some(0), Some(true), T0);
    let prima = righe(&s);
    assert_eq!(prima, 1);

    for orizzonte in [None, Some(Millis(T0 + 1)), Some(Millis(T0 - 1))] {
        let c = s
            .store
            .calendar(&s.teacher, &s.student, &s.course, orizzonte)
            .expect("la coda");
        assert_eq!(c.entries.len(), 1);
    }
    assert_eq!(righe(&s), prima, "la coda ha scritto qualcosa");
}

/// Il banco ha qualcosa da vedere, e questo è il controllo che rende gli altri
/// non vacui.
///
/// Un calendario che restituisce `[]` perché non c'è niente e uno che restituisce
/// `[]` perché la query è rotta sono la stessa risposta. Questo test fissa il
/// caso di fondo — nessuna ricomparsa, elenco vuoto, nessun errore — e poi lo
/// rovescia: la stessa coda, con una riga in più, non è più vuota.
#[test]
fn nessuna_ricomparsa_da_una_coda_vuota_e_non_un_errore() {
    let mut s = School::new();
    let arg = s.published(1);
    // L'argomento esiste ed è pubblicato: la coda è vuota perché lo studente non
    // ha ancora ricomparso, non perché non ci sia niente da guardare.
    assert_eq!(
        s.store
            .read_argument(&s.teacher, &arg.id)
            .expect("leggibile")
            .id,
        arg.id
    );
    let calendario = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("una coda vuota è una risposta vera, non un errore");
    assert!(calendario.entries.is_empty());
    assert_eq!(calendario.beyond(), 0);

    // E una volta che la riga c'è, la stessa coda non è più vuota: è la metà non
    // vacua di questo test.
    let sessione = s.session(Register::Observations);
    scrivi(
        &mut s,
        &sessione,
        &arg.id,
        "u0",
        Some(true),
        Some(0),
        Some(true),
        T0,
    );
    let dopo = s
        .store
        .calendar(&s.teacher, &s.student, &s.course, None)
        .expect("la coda con una riga");
    assert_eq!(ids(&dopo), vec![arg.id.0.clone()]);
}
