//! Il tentativo non assistito: l'unica strada che scrive `unaided = 1`.
//!
//! Fino a questo file la colonna di `V6__unaided.sql` era **inerte**: nessun
//! percorso di prodotto produceva la riga che il numeratore della claim conta, e
//! la claim del progetto — «la quota di argomenti che passano da "non dimostrato"
//! a "dimostrato"», misurata «sul compito a risorse chiuse, non sulla pratica
//! assistita» (`01-problema-e-tesi.html`, `#la-claim-falsificabile`) — non aveva
//! un produttore.
//!
//! Qui si prova che il produttore esiste e che produce la riga giusta. I test non
//! asseriscono che una funzione sia stata chiamata: asseriscono **che cosa
//! finisce nel registro** e **che cosa lo studente vede**, che sono le due cose
//! che la colonna promette.
//!
//! # L'ordine delle prove
//!
//! 1. la riga esiste ed è `unaided = Some(true)` quando il sistema non ha
//!    servito piste, ed è la sola che lo studente vede;
//! 2. la stessa strada con due piste servite scrive `unaided = Some(false)` e
//!    **non** entra nella superficie dello studente né nel conteggio di classe:
//!    aiuto e apprendimento vanno in direzioni opposte (`16`,
//!    `#esempio-lavrato`, passo 06) e una riga che confonde le due cose rende
//!    falso il numero;
//! 3. l'id deriva dal contenuto, quindi lo stesso tentativo registrato due volte
//!    è lo stesso tentativo e non una riga in più;
//! 4. un'istanza che il generatore non ha prodotto non entra, e non lascia una
//!    riga;
//! 5. **il binario** `kbs tenta` scrive la riga e la stampa, e senza `--aiuto`
//!    rifiuta invece di scegliere uno zero: questa è la prova che il percorso è
//!    di prodotto e non un aiutante di test.

mod common;

use std::path::Path;

use kbs_core::{ArgumentId, Checker, CohortId, Evidence, Exercise, Instance, Millis, PersonId};
use kbs_intake::cli;
use kbs_intake::pratica::{self, Tentativo};
use kbs_intake::route::{self, Request, Route};
use kbs_store::Store;

use common::*;

const MARCO: &str = "person_0007";
const COORTE: &str = "2026-terza";
const ESERCIZIO: &str = "ex-forma-ridotta";
const ISTANZA: &str = "seed-1";
const T0: i64 = 1_757_000_000_000;

fn marco() -> PersonId {
    PersonId(MARCO.to_string())
}

fn argomento() -> ArgumentId {
    ArgumentId::from_rel_path(REL)
}

/// Il negozio del banco: corso, docente che lo insegna, studente iscritto,
/// argomento in uso, esercizio e la sua istanza.
///
/// L'esercizio e l'istanza sono scritti a mano **perché il test non sta provando
/// la loro creazione** — e qui c'è un buco dichiarato: nessun percorso di
/// prodotto chiama `upsert_exercise` o `put_instance`, quindi la strada di
/// authoring di D8 non esiste ancora e queste due righe sono ciò che un futuro
/// percorso dovrà sostituire. Se quel percorso non arriva, `kbs tenta` non ha
/// niente su cui lavorare e lo dice con `esercizio-assente`.
fn banco() -> Store {
    let mut s = store_con_corso();
    s.upsert_person(&kbs_store::Person {
        id: marco(),
        display_name: "Marco".into(),
        created_at: Millis(0),
    })
    .expect("studente");
    s.add_relation(
        &kbs_store::CourseRelation {
            person: marco(),
            course: corso(),
            relation: kbs_core::Relation::EnrolledIn,
            since: Millis(0),
            until: None,
        },
        &docente(),
    )
    .expect("iscrizione");
    route::receive(
        &mut s,
        Request {
            route: Route::File,
            by: docente(),
            course: Some(corso()),
            rel_path: Some(REL.to_string()),
            source: artifact(false),
        },
    )
    .expect("l'argomento su cui si lavora entra");
    // L'argomento va **in uso**, non in bozza: lo studente è iscritto, e
    // `may_read` per un iscritto chiede `state.is_visible_to_enrolled()`. Senza
    // ratifica e pubblicazione, la lettura del registro dello studente risponde
    // `NotReadable` — che è la risposta giusta, e che in questo banco
    // mascherebbe la sola cosa che il test sta provando.
    s.ratify(&argomento(), &docente(), "verificato riga per riga")
        .expect("ratifica");
    s.publish(&argomento()).expect("pubblicazione");
    s.upsert_exercise(&Exercise {
        id: ESERCIZIO.into(),
        course: corso(),
        argument: argomento(),
        family: "forma-ridotta".into(),
        generator_version: "kbs-exercise/1".into(),
        prompt: "Riduci 3/6. Scrivi la risposta e basta.".into(),
        checker: Checker::Set {
            elements: vec!["1/2".into()],
        },
        created_at: Millis(T0),
        created_by: docente(),
    })
    .expect("esercizio");
    s.put_instance(&Instance {
        exercise: ESERCIZIO.into(),
        seed: ISTANZA.into(),
        rendered_prompt: "Riduci 3/6.".into(),
        expected: "1/2".into(),
        params: serde_json::json!({ "n": 3, "d": 6 }),
    })
    .expect("istanza");
    s
}

/// Il banco su un percorso, per il verbo della CLI.
///
/// `common::store_con_corso()` apre in memoria e un database in memoria non
/// sopravvive a un processo: `kbs tenta` apre un percorso, quindi il suo test ha
/// bisogno di un banco su file. `VACUUM INTO` fa le due cose in una frase e, se
/// un domani il SQLite non lo supporta, il test **fallisce qui** invece di
/// passare perché non ha lavorato.
fn banco_su_file(path: &Path) {
    let s = banco();
    s.conn()
        .execute_batch(&format!(
            "VACUUM INTO '{}'",
            path.to_string_lossy().replace('\'', "''")
        ))
        .expect("VACUUM INTO: il banco del test deve diventare un file per la CLI");
}

fn tentativo(n_hints: u32) -> Tentativo {
    Tentativo {
        id: String::new(),
        student: marco(),
        course: corso(),
        cohort: CohortId(COORTE.to_string()),
        exercise: ESERCIZIO.into(),
        instance: ISTANZA.into(),
        n_hints,
        correct: true,
        at: Millis(T0),
    }
    .with_derived_id()
}

/// Il registro delle dimostrazioni letto dalla superficie dello studente.
fn vista_dello_studente(s: &Store) -> Vec<kbs_core::Observation> {
    s.observations_for(&marco(), &marco(), &argomento())
        .expect("lo studente legge il proprio registro")
}

fn righe(s: &Store) -> i64 {
    s.conn()
        .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
        .expect("conteggio")
}

#[test]
fn un_tentativo_senza_piste_serve_scrive_la_riga_che_la_claim_conta() {
    let mut s = banco();
    let osservazione = pratica::registra_in_una_sessione(
        &mut s,
        &docente(),
        tentativo(0),
        "tentativo non assistito",
    )
    .expect("il tentativo entra");

    // I campi che la colonna promette, uno per uno.
    assert_eq!(osservazione.unaided, Some(true), "nessuna pista servita");
    assert_eq!(
        osservazione.n_hints,
        Some(0),
        "e zero è una misura, non un default"
    );
    assert_eq!(
        osservazione.judged_by,
        Some(kbs_core::GraderKind::Deterministic)
    );
    assert_eq!(
        osservazione.evidence,
        Evidence::Checked {
            exercise: ESERCIZIO.into(),
            instance: ISTANZA.into(),
            correct: true,
        }
    );
    // L'argomento è quello dell'esercizio, non uno dichiarato dal chiamante: il
    // `Tentativo` non ha un campo `argument`, quindi non c'è niente da divergere.
    assert_eq!(osservazione.argument, argomento());
    assert_eq!(
        osservazione.seq.0,
        1,
        "il registro assegna il seq, non il chiamante"
    );

    // E la riga è nella superficie non assistita: lo studente la vede, perché è
    // quello che la vista `unaided_observations` contiene e nient'altro.
    let dello_studente = vista_dello_studente(&s);
    assert_eq!(dello_studente.len(), 1);
    assert_eq!(dello_studente[0].id, osservazione.id);
}

/// `unaided` è **derivato**, e la derivazione è una funzione pubblica.
///
/// Il punto del test è che `Tentativo` non ha un campo `unaided`: la difesa è nel
/// tipo, quindi nessun chiamante può dichiarare che uno studente ha lavorato senza
/// aiuto. Può solo dire un conteggio, e la colonna segue dal conteggio.
#[test]
fn la_disponibilita_di_aiuto_deriva_da_un_conteggio_e_non_si_dichiara() {
    assert_eq!(tentativo(0).unaided(), Some(true));
    assert_eq!(tentativo(1).unaided(), Some(false));
    assert_eq!(tentativo(9).unaided(), Some(false));
}

#[test]
fn due_piste_servite_e_una_riga_che_lo_studente_e_la_classe_non_vedono() {
    // È la metà negativa, ed è quella che rende la colonna utile: senza, la coda
    // di practice finirebbe nel numeratore e il numero della claim sarebbe
    // gonfiato da tutto quello che il sistema ha già fatto per lo studente.
    let mut s = banco();
    pratica::registra_in_una_sessione(
        &mut s,
        &docente(),
        tentativo(2),
        "tentativo assistito",
    )
    .expect("il tentativo assistito entra anche lui");

    let dal_docente = s
        .observations_for(&docente(), &marco(), &argomento())
        .expect("il docente legge tutto");
    assert_eq!(dal_docente.len(), 1, "la coda di practice è del docente");
    assert_eq!(dal_docente[0].unaided, Some(false));
    assert_eq!(dal_docente[0].n_hints, Some(2));

    assert!(
        vista_dello_studente(&s).is_empty(),
        "una riga assistita non è una padronanza e non entra nella vista dello studente"
    );

    // E il conteggio di coorte di D9, che prende le righe dalla stessa vista, non
    // la vede: la classe non è una coda di aiuti.
    match s
        .record_cohort_signal(&kbs_core::CohortSignal {
            course: corso(),
            cohort: CohortId(COORTE.to_string()),
            argument: argomento(),
            failing: 5,
            total: 5,
            at: Millis(T0),
        })
        .expect_err("una sola persona che ha avuto aiuto non è una classe")
    {
        kbs_store::Error::CohortCountMismatch {
            counted_failing,
            counted_total,
            ..
        } => assert_eq!(
            (counted_failing, counted_total),
            (0, 0),
            "la coda di aiuti non entra nel conteggio di classe"
        ),
        altro => panic!("atteso CohortCountMismatch, ottenuto {altro:?}"),
    }
}

#[test]
fn l_id_deriva_dal_contenuto_e_il_tentativo_e_idempotente() {
    let mut s = banco();
    let prima = pratica::registra_in_una_sessione(
        &mut s,
        &docente(),
        tentativo(0),
        "tentativo non assistito",
    )
    .expect("il primo entra");
    assert!(prima.id.starts_with("obs_"));

    // Lo stesso tentativo, due volte: la seconda registrazione è la stessa
    // osservazione e il registro lo dice. Un id da contatore darebbe qui una
    // seconda riga, e due righe uguali sono un conteggio gonfiato.
    let errore = pratica::registra_in_una_sessione(
        &mut s,
        &docente(),
        tentativo(0),
        "tentativo non assistito",
    )
    .expect_err("lo stesso tentativo due volte");
    assert!(
        matches!(
            errore,
            kbs_intake::Error::Store(kbs_store::Error::DuplicateId { .. })
        ),
        "atteso DuplicateId, ottenuto {errore:?}"
    );
    assert_eq!(righe(&s), 1, "una riga sola nel registro");
}

#[test]
fn un_istanza_che_il_generatore_non_ha_prodotto_non_entra() {
    let mut s = banco();
    let falso = Tentativo {
        instance: "seed-che-non-esiste".into(),
        ..tentativo(0)
    }
    .with_derived_id();

    let errore = pratica::registra_in_una_sessione(
        &mut s,
        &docente(),
        falso,
        "tentativo su un'istanza inesistente",
    )
    .expect_err("l'istanza non è stata prodotta");
    assert!(
        matches!(errore, kbs_intake::Error::IstanzaAssente { .. }),
        "atteso IstanzaAssente, ottenuto {errore:?}"
    );
    assert_eq!(righe(&s), 0, "il rifiuto non ha lasciato una riga");
}

#[test]
fn chi_non_insegna_il_corso_non_registra_niente() {
    // Il predicato è dentro `Store::exercise`, che chiede `teaches`. Una strada
    // che accettasse la scrittura da chi non insegna scriverebbe la claim del
    // prodotto su un corso in cui chi scrive non ha diritti.
    let mut s = banco();
    s.upsert_person(&kbs_store::Person {
        id: PersonId::fixture(9),
        display_name: "estraneo".into(),
        created_at: Millis(0),
    })
    .expect("estraneo");
    let errore = pratica::registra_in_una_sessione(
        &mut s,
        &PersonId::fixture(9),
        tentativo(0),
        "tentativo di chi non insegna",
    )
    .expect_err("un estraneo non registra dimostrazioni");
    assert!(
        matches!(errore, kbs_intake::Error::EsercizioAssente { .. }),
        "atteso EsercizioAssente, ottenuto {errore:?}"
    );
    assert_eq!(righe(&s), 0);
}

/// La riga di argv del verbo, con `--aiuto` in più o in meno.
fn argv_tenta(db: &Path, aiuto: Option<&str>) -> Vec<String> {
    let mut argv = vec![
        "tenta".to_string(),
        "--db".into(),
        db.to_string_lossy().into_owned(),
        "--person".into(),
        docente().0,
        "--student".into(),
        MARCO.into(),
        "--course".into(),
        corso().0,
        "--cohort".into(),
        COORTE.into(),
        "--exercise".into(),
        ESERCIZIO.into(),
        "--instance".into(),
        ISTANZA.into(),
        "--esito".into(),
        "corretto".into(),
        "--at".into(),
        T0.to_string(),
    ];
    if let Some(n) = aiuto {
        argv.push("--aiuto".into());
        argv.push(n.to_string());
    }
    argv
}

fn esegui(argv: &[String]) -> (cli::Uscita, serde_json::Value, String) {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let mut inp: &[u8] = b"";
    let uscita = cli::esegui(argv, &mut out, &mut err, &mut inp);
    let corpo = String::from_utf8(out).expect("stdout utf-8");
    let valore = serde_json::from_str(corpo.trim())
        .unwrap_or_else(|e| panic!("stdout non è json: {corpo} ({e})"));
    (uscita, valore, String::from_utf8_lossy(&err).into_owned())
}

/// Il percorso di prodotto: il binario, non una funzione chiamata a mano.
///
/// `kbs tenta` è l'unica strada che un agente o un docente possono eseguire
/// senza scrivere codice, ed è quella su cui un banco vero si aggancia. Il test
/// chiama `cli::esegui` — cioè il protocollo, con lo stesso stdout e lo stesso
/// stderr che avrebbe un processo — e poi **rilettura il file** per essere sicuro
/// che la riga sia stata scritta e non solo stampata.
#[test]
fn il_binario_kbs_tenta_scrive_la_riga_che_la_claim_conta() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("scuola.sqlite");
    banco_su_file(&db);

    let (uscita, valore, errore) = esegui(&argv_tenta(&db, Some("0")));
    assert_eq!(uscita, cli::Uscita::Ok, "stderr: {errore}");
    assert_eq!(valore["ok"], serde_json::json!(true));
    assert_eq!(valore["command"], "tenta");
    // Il corpo è l'osservazione: l'agente legge dalla risposta che cosa è finito
    // nel registro, senza rileggerlo.
    assert_eq!(valore["unaided"], serde_json::json!(true));
    assert_eq!(valore["n_hints"], serde_json::json!(0));
    assert_eq!(valore["judged_by"], serde_json::json!("deterministic"));
    assert_eq!(valore["argument"], serde_json::json!(argomento().0));
    assert_eq!(valore["seq"], serde_json::json!(1));

    // E la riga è nel file che il verbo ha scritto: non è un'eco della risposta.
    let s = Store::open(&db).expect("il database scritto dalla CLI si riapre");
    let letta = vista_dello_studente(&s);
    assert_eq!(letta.len(), 1, "la riga c'è nel file, non solo nella risposta");
    assert_eq!(letta[0].unaided, Some(true));
}

/// `--aiuto` non ha un default, e la ragione è nel numero.
///
/// `0` è la misura «nessuna pista servita dal sistema». Un valore assente è
/// «non lo so», che è una riga diversa: il rifiuto è `3` (uso), non `2` (regola),
/// perché è una richiesta malformata e non un diniego. Ed è la stessa distinzione
/// per cui `V6__unaided.sql` ha lasciato `n_hints` nullable invece di
/// `NOT NULL DEFAULT 0`: «un conteggio che non è stato fatto non è uno zero».
#[test]
fn il_conteggio_di_piste_si_dichiara_e_non_ha_un_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("scuola.sqlite");
    banco_su_file(&db);

    let (uscita, valore, errore) = esegui(&argv_tenta(&db, None));
    assert_eq!(
        uscita,
        cli::Uscita::Uso,
        "senza `--aiuto` la richiesta non è well-formed, non è un rifiuto di merito: {errore}"
    );
    assert_eq!(valore["ok"], serde_json::json!(false));
    assert!(
        errore.contains("aiuto"),
        "il motivo su stderr nomina l'opzione mancante: {errore}"
    );

    // E un conteggio negativo è un conteggio che non è un conteggio.
    let (uscita, _, _) = esegui(&argv_tenta(&db, Some("-2")));
    assert_eq!(uscita, cli::Uscita::Uso, "un conteggio negativo è uso, non misura");

    // Il database è quello di prima: nessuna delle due richieste ha scritto.
    let s = Store::open(&db).expect("il database si riapre");
    assert_eq!(righe(&s), 0, "un rifiuto non lascia una riga");
}

/// La giunzione: un tentativo di prodotto è la prova che il metro conta.
///
/// Fin qui i due mezzi sono provati separatamente — `pratica` scrive la riga
/// non assistita, `padronanza` la conta — ma nessuno dei due test attraversava
/// l'altro. Il difetto che questa prova copre è quindi possibile e avrebbe
///passato tutto: una colonna che nessuno scrive, o un metro che legge una
/// forma di prova che `pratica` non produce mai. Le due metà verrebbero verdi
/// e la claim resterebbe non calcolabile.
///
/// Il numero non puo' essere `Proven` qui, e non e' un difetto: una sola prova
/// su un esercizio non raggiunge la soglia. Ciò che si prova e' che la prova
/// **entra nel conto** — `proved` sale a uno — e che una riga assistita non ci
/// entra. Un test che si fermasse a `!is_proven()` passerebbe anche con il
/// metro che ignora ogni prova.
#[test]
fn un_tentativo_di_prodotto_e_la_prova_che_il_metro_conta() {
    let mut s = banco();
    pratica::registra_in_una_sessione(
        &mut s,
        &docente(),
        tentativo(0),
        "tentativo non assistito",
    )
    .expect("il tentativo entra");

    let righe = s
        .mastery_for(&marco(), &marco(), &corso(), Millis(T0 + 1))
        .expect("lo studente legge il proprio metro");
    let riga = righe
        .iter()
        .find(|r| r.argument == argomento())
        .expect("l'argomento su cui si e' lavorato ha una riga di metro");

    assert_eq!(
        riga.proved_exercises, 1,
        "una risposta senza aiuto su un esercizio e' una prova, e il metro la conta: \
         è questo il passaggio che chiude la claim"
    );
    assert_eq!(riga.proofs.len(), 1, "e la prova che ha usato è quella, non un'altra");
    assert!(
        !riga.verdict.is_proven(),
        "una prova sola non raggiunge la soglia, e dirlo è il verdetto giusto"
    );

    // La metà negativa, che è quella che rende la colonna utile: la stessa
    // risposta, la stessa istanza, due piste servite — e il metro non la vede.
    // Se la contasse, la soglia si riempirebbe con quello che il sistema ha
    // già fatto per lo studente.
    let mut s2 = banco();
    pratica::registra_in_una_sessione(
        &mut s2,
        &docente(),
        tentativo(3),
        "tentativo assistito",
    )
    .expect("il tentativo assistito entra anche lui");
    let righe2 = s2
        .mastery_for(&marco(), &marco(), &corso(), Millis(T0 + 1))
        .expect("il metro anche quando non ha niente da contare");
    let riga2 = righe2
        .iter()
        .find(|r| r.argument == argomento())
        .expect("la riga c''e anche quando il conto e' zero");
    assert_eq!(
        riga2.proved_exercises, 0,
        "una risposta con due piste non e' una dimostrazione: è la coda di practice"
    );
    assert!(riga2.proofs.is_empty(), "e il metro non la elenca fra le prove usate");
}
