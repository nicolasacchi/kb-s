//! Le prove di D8 e di D11, sulle famiglie vere.
//!
//! Tutte queste prove girano su **semi**, non su coppie scelte a mano: la
//! coppia scelta a mano dimostra che due istanze diverse possono avere risposte
//! diverse, che è la cosa più ovvia del mondo. Lo sweep dimostra che vale per
//! 1024 semi consecutivi, che è la proprietà che regge in una classe.
//!
//! # Lo sweep di unicità è più corto di 1024 per due famiglie, e perché
//!
//! Lo sweep di ogni famiglia è dichiarato in [`SWEEP`] con la sua aritmetica. La
//! risposta di un esercizio scolastico vive in un intervallo piccolo — una media
//! pesata sta fra 11 e 99, una risposta corretta ha quattro valori possibili — e
//! nulla in questo crate può allargare un intervallo senza rendere l'esercizio
//! irreale. Quindi:
//!
//! * dove lo spazio delle risposte è grande (coefficienti, conteggi, sottoinsiemi)
//!   lo sweep è di 1024 semi e nessuna risposta si ripete;
//! * dove lo spazio è piccolo (una media è una frazione con denominatore ≤ 27, una
//!   risposta di classificazione è una frazione ridotta) lo sweep è corto quanto
//!   basta a restare senza collisioni, e il limite è dichiarato qui invece di
//!   essere nascosto dietro una soglia arrotondata.
//!
//! Il numero che conta per D8 è il **massimo degli studenti serviti da una
//! classe**, che è 30: nessuna famiglia, nemmeno le due corte, ha collisioni
//! sotto i 64 semi.

use kbs_core::{ArgumentId, CourseId, Millis, PersonId};
use kbs_core::{Checker, Instance};
use kbs_exercise::check::{normalize_equivalence, Verdict};
use kbs_exercise::families;
use kbs_exercise::generator::Generator;
use kbs_exercise::grading::{GradingChain, Outcome};
use kbs_exercise::leak::no_solution_leak;
use kbs_exercise::publication::audit;
use kbs_exercise::{grade, may_publish};

/// Lo sweep di unicità per famiglia, e il perché della sua lunghezza.
const SWEEP: [(&str, usize, &str); 5] = [
    (
        "algebra-polinomio",
        1024,
        "la risposta è una terna di prodotti di interi a due cifre: spazio dell'ordine di 10^7",
    ),
    (
        "statistica-media-pesata",
        64,
        "la risposta è una frazione con denominatore ≤ 27: spazio dell'ordine di 10^4, \
         e sotto i 64 semi non si ripete",
    ),
    (
        "aritmetica-conta-coprimi",
        1024,
        "la risposta è un intero fino a 10^7: spazio dell'ordine di 10^7",
    ),
    (
        "numeri-quale-frazione-ridotta",
        256,
        "la risposta è una frazione coprime a tre cifre per numeratore e denominatore: \
         spazio dell'ordine di 5 · 10^5",
    ),
    (
        "insiemi-differenza",
        1024,
        "la risposta è un sottoinsieme di un universo di 90 elementi: spazio di 2^90",
    ),
];

/// Lo sweep del gate di pubblicazione: mille e ventiquattro semi per famiglia.
const GATE: usize = 1024;

fn semi(i: usize) -> String {
    format!("classe-{i}")
}

fn generatore(nome: &str) -> Box<dyn Generator> {
    families::by_name(nome).unwrap_or_else(|| panic!("famiglia nota: {nome}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// D11 — la stessa tupla produce la stessa istanza, byte per byte
// ─────────────────────────────────────────────────────────────────────────────

/// D11: la riproducibilità è esatta, e «esatta» comprende il **rendering**.
///
/// Se domani il rendering dipendesse da un orologio, da un `HashMap`, o da
/// `rand::thread_rng`, questo test lo mostrerebbe: confronta l'intera istanza
/// (che contiene `rendered_prompt`, `expected` e `params`) e poi la sua
/// serializzazione, che è ciò che finisce in un registro o in una prova.
#[test]
fn lo_stesso_seed_da_anche_la_stessa_istanza() {
    for g in families::all() {
        for i in 0..256 {
            let seed = semi(i);
            let (a, ca) = g.build(&seed).expect("istanza generabile");
            let (b, cb) = g.build(&seed).expect("istanza generabile");
            assert_eq!(
                a,
                b,
                "{} con seed {seed}: l'istanza non è riproducibile",
                g.family()
            );
            assert_eq!(ca, cb);
            assert_eq!(
                serde_json::to_string(&a).expect("istanza serializzabile"),
                serde_json::to_string(&b).expect("istanza serializzabile"),
                "{} con seed {seed}: la serializzazione non è riproducibile",
                g.family()
            );
        }
    }
}

/// La versione del generatore entra nella tupla: due famiglie con lo stesso nome
/// di versione non possono produrre la stessa istanza, e ogni famiglia ha una
/// versione non vuota.
#[test]
fn la_tupla_e_famiglia_e_versione_e_seed() {
    for g in families::all() {
        assert!(!g.family().is_empty());
        assert!(!g.generator_version().is_empty());
        let seed = "vuoto";
        let (i, _) = g.build(seed).expect("istanza generabile");
        assert_eq!(i.exercise, g.exercise_id(seed));
        assert!(i.exercise.starts_with(g.family()));
        assert!(i.exercise.contains(g.generator_version()));
    }
    // Due famiglie diverse non possono condividere nome.
    let tutte = families::all();
    let nomi: Vec<&str> = tutte.iter().map(|g| g.family()).collect();
    let mut unici = nomi.clone();
    unici.sort_unstable();
    unici.dedup();
    assert_eq!(nomi.len(), unici.len());
}

// ─────────────────────────────────────────────────────────────────────────────
// D8 — nessuna istanza di una famiglia ha la risposta di un'altra
// ─────────────────────────────────────────────────────────────────────────────

/// Il test che rende D8 una proprietà e non uno slogan.
///
/// Tre cose insieme, perché la prima da sola è debole:
/// * i **parametri** sono tutti distinti (è la lettera di D8: stesso ragionamento,
///   parametri diversi);
/// * i **prompt resi** sono tutti distinti (due studenti non leggono lo stesso
///   testo);
/// * le **risposte** sono tutte distinte (copiare non funziona).
#[test]
fn nessuna_istanza_di_una_famiglia_ha_la_risposta_di_un_altra() {
    for (nome, n, perché) in SWEEP {
        let g = generatore(nome);
        let mut risposte: Vec<String> = Vec::with_capacity(n);
        let mut parametri: Vec<String> = Vec::with_capacity(n);
        let mut prompt: Vec<String> = Vec::with_capacity(n);
        for i in 0..n {
            let (istanza, _) = g.build(&semi(i)).expect("istanza generabile");
            risposte.push(istanza.expected.clone());
            parametri.push(istanza.params.to_string());
            prompt.push(istanza.rendered_prompt.clone());
        }
        let uniche = |v: &[String]| {
            let mut c = v.to_vec();
            c.sort_unstable();
            c.dedup();
            c.len()
        };
        assert_eq!(
            uniche(&risposte),
            n,
            "{nome}: due istanze su {n} condividono la risposta — {perché}"
        );
        assert_eq!(
            uniche(&parametri),
            n,
            "{nome}: due semi producono gli stessi parametri"
        );
        assert_eq!(
            uniche(&prompt),
            n,
            "{nome}: due semi producono lo stesso testo"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// D8 — il gate di pubblicazione, su mille e ventiquattro istanze per famiglia
// ─────────────────────────────────────────────────────────────────────────────

/// `no_solution_leak` su 1024 istanze per famiglia, e il gate completo insieme.
///
/// Il secondo asserto dentro il ciclo è quello che rende la prima metà non
/// vacua: per ogni istanza, lo stesso GUARDIAN con la risposta incollata dentro
/// **deve** essere bloccato. Una prova che passa sempre e non può fallire non è
/// una prova, e questa può fallire per costruzione.
#[test]
fn il_guardian_non_rivela_la_risposta_su_mille_istanze() {
    for g in families::all() {
        for i in 0..GATE {
            let seed = semi(i);
            let (istanza, checker) = g.build(&seed).expect("istanza generabile");
            if let Err(e) = no_solution_leak(g.guardian(), &istanza, &checker) {
                panic!("{} con seed {seed}: GUARDIAN che rivela: {e}", g.family());
            }
            audit(g.as_ref(), &seed).unwrap_or_else(|e| {
                panic!("{} con seed {seed}: gate di pubblicazione: {e}", g.family())
            });
        }
    }
}

/// Il controllo di non vacuità, da solo: la prova del GUARDIAN sa fallire.
#[test]
fn un_guardian_con_la_risposta_incollata_e_fermato() {
    for g in families::all() {
        for i in 0..64 {
            let seed = semi(i);
            let (istanza, checker) = g.build(&seed).expect("istanza generabile");
            let bucato = format!("Attenzione: la risposta è {}.", istanza.expected);
            assert!(
                no_solution_leak(&bucato, &istanza, &checker).is_err(),
                "{} con seed {seed}: il GUARDIAN bucato non è stato fermato",
                g.family()
            );
        }
    }
}

/// Ogni esercizio delle famiglie è pubblicabile: il gate D8 passa e `may_publish`
/// conferma che il checker è confrontabile.
#[test]
fn ogni_esercizio_pubblicabile() {
    for g in families::all() {
        let esercizio = g
            .exercise_for(
                &semi(0),
                CourseId::fixture(1),
                ArgumentId::from_rel_path("prove/prova-1.md"),
                Millis(0),
                PersonId::fixture(1),
            )
            .expect("esercizio costruibile");
        assert!(
            may_publish(&esercizio).is_ok(),
            "{}: l'esercizio non pubblica ({:?})",
            g.family(),
            may_publish(&esercizio)
        );
        assert_eq!(esercizio.family, g.family());
        assert_eq!(esercizio.generator_version, g.generator_version());
        assert_eq!(esercizio.prompt, g.template_prompt());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Il confronto accetta la risposta giusta e rifiuta il quasi-miss
// ─────────────────────────────────────────────────────────────────────────────

/// Per ogni famiglia, la risposta dichiarata è accettata e un quasi-miss no.
///
/// Il quasi-miss è costruito dal **checker**, non a caso: si parte dalla risposta
/// e se ne altera la forma nel modo che quel checker potrebbe scambiare per
/// uguale. Se un family's quasi-miss fosse accettato, la copia del vicino
/// funzionerebbe — che è la cosa che D8 nega.
#[test]
fn la_risposta_dichiarata_passa_e_il_quasi_miss_no() {
    for g in families::all() {
        for i in 0..128 {
            let seed = semi(i);
            let (istanza, checker) = g.build(&seed).expect("istanza generabile");
            assert_eq!(
                grade(&istanza, &checker, &istanza.expected),
                Ok(Verdict::Correct),
                "{} con seed {seed}: la risposta dichiarata non passa",
                g.family()
            );
            let quasi = quasi_miss(&istanza, &checker);
            assert_eq!(
                grade(&istanza, &checker, &quasi),
                Ok(Verdict::Incorrect),
                "{} con seed {seed}: il quasi-miss {quasi:?} passa per {istanza:?}",
                g.family()
            );
        }
    }
}

/// Una risposta sbagliata che un confronto disattento accetterebbe, per ciascuna
/// delle quattro forme di checker. Le quattro righe sono i quattro modi in cui un
/// sistema di grading diventa fiducioso a torto.
#[test]
fn quattro_quasi_miss_che_un_confronto_disattento_accetterebbe() {
    // 1. Numerico: tolleranza relativa all'1% invece che assoluta.
    let (i, c) = generatore("statistica-media-pesata")
        .build("quasi-1")
        .expect("istanza");
    let valore: f64 = i.expected.parse().expect("media decimale");
    let relativo = format!("{:.4}", valore * 1.005);
    assert!(
        (valore * 1.005 - valore).abs() / valore < 0.01,
        "il confronto disattento dice «giusto»"
    );
    assert_eq!(grade(&i, &c, &relativo), Ok(Verdict::Incorrect));
    // il vero quasi-miss: stesso ordine di grandezza, sbagliato di 0.01
    assert_eq!(
        grade(&i, &c, &format!("{:.4}", valore + 0.01)),
        Ok(Verdict::Incorrect)
    );

    // 2. Set: confronto su estensione, che ignora la molteplicità.
    let (i, c) = generatore("insiemi-differenza")
        .build("quasi-2")
        .expect("istanza");
    let Checker::Set { elements } = &c else {
        unreachable!("insieme")
    };
    let ripetuto = format!("{}, {}", elements[0], elements.join(", "));
    use std::collections::HashSet;
    let estensione: HashSet<&str> = elements.iter().map(|e| e.as_str()).collect();
    let data: HashSet<&str> = i.expected.split(", ").collect();
    assert_eq!(estensione, data, "le due liste hanno la stessa estensione");
    assert_eq!(grade(&i, &c, &ripetuto), Ok(Verdict::Incorrect));

    // 3. Scelta multipla: confronto per contenimento di sottostringa.
    let (i, c) = generatore("numeri-quale-frazione-ridotta")
        .build("quasi-3")
        .expect("istanza");
    let quasi = format!("1{}", i.expected);
    assert!(
        quasi.contains(&i.expected),
        "il confronto disattento dice «giusto»"
    );
    assert_eq!(grade(&i, &c, &quasi), Ok(Verdict::Incorrect));
    // e l'indice fuori intervallo è una risposta sbagliata, non un voto
    assert_eq!(grade(&i, &c, "9"), Ok(Verdict::Incorrect));

    // 4. Equivalenza: forma normale diversa ma valore uguale.
    let (i, c) = generatore("algebra-polinomio")
        .build("quasi-4")
        .expect("istanza");
    let Checker::Equivalence { normalized } = &c else {
        unreachable!("equivalenza")
    };
    let spaziato = format!("{} ", normalized.replace('+', " + "));
    assert_eq!(
        normalize_equivalence(&spaziato),
        normalize_equivalence(normalized)
    );
    assert_eq!(
        grade(&i, &c, &spaziato),
        Ok(Verdict::Correct),
        "gli spazi non cambiano la forma"
    );
    // un valore diverso nella stessa forma: rifiutato
    let altro = normalized.replace('+', "+1+");
    assert_eq!(grade(&i, &c, &altro), Ok(Verdict::Incorrect));
}

/// L'indice dell'esatto fuori dal vettore è un errore di costruzione: nessuna
/// risposta, neanche quella giusta, arriva a un voto.
#[test]
fn un_indice_dell_esatto_fuori_intervallo_non_da_nessun_voto() {
    let (istanza, _) = generatore("numeri-quale-frazione-ridotta")
        .build("rotto")
        .expect("istanza");
    let rotto = Checker::MultipleChoice {
        correct_index: 200,
        options: vec!["1/2".into(), "2/3".into(), "3/4".into(), "4/5".into()],
    };
    assert!(grade(&istanza, &rotto, &istanza.expected).is_err());
    assert!(grade(&istanza, &rotto, "1/2").is_err());
    assert!(grade(&istanza, &rotto, "nonsense").is_err());
}

/// Un'istanza non si grada con il checker di un altro seed: l'errore distingue il
/// plumbing rotto dallo studente che ha sbagliato.
///
/// Il test è **per variante**, perché la verificabilità dipende da che cosa il
/// checker porta: `Numeric` porta solo la tolleranza, che è la stessa per tutti i
/// seed della famiglia, e non c'è nulla che possa legare un'istanza al suo
/// checker. È dichiarato in `check::is_bound_to` ed è il motivo per cui la coppia
/// nasce unita in `Generator::build`. Le altre tre forme portano dati
/// dell'istanza e l'incrocio è un errore.
#[test]
fn due_istanze_diverse_non_si_gradano_incrociate() {
    for g in families::all() {
        let (a, ca) = g.build("a").expect("istanza");
        let (b, cb) = g.build("b").expect("istanza");
        if a.expected == b.expected {
            // I due semi hanno prodotto lo stesso esercizio: non c'è incrocio da
            // provare, e fingere il contrario sarebbe un test di comodo.
            continue;
        }
        if matches!(cb, Checker::Numeric { .. }) {
            assert_eq!(grade(&a, &cb, &a.expected), Ok(Verdict::Correct));
            assert_eq!(grade(&a, &ca, &b.expected), Ok(Verdict::Incorrect));
            continue;
        }
        for (istanza, altra, checker) in [(&a, &b, &cb), (&b, &a, &ca)] {
            assert!(
                grade(istanza, checker, &istanza.expected).is_err(),
                "{}: l'istanza di un seed ha preso un voto dal checker dell'altro",
                g.family()
            );
            assert!(
                grade(istanza, checker, &altra.expected).is_err(),
                "{}: l'incrocio inverso ha preso un voto",
                g.family()
            );
        }
    }
}

/// La catena di grading parte dal deterministico e si chiude lì.
///
/// Questo è il collegamento fra `check` e `grading`: il verdetto del checker è
/// un `Outcome::Conclusive`, e una catena che si chiude al primo anello non può
/// essere riaperta da un pari. Un esercizio con checker deterministico che non
/// decide non è un esercizio: è una catena che riparte dal primo anello.
#[test]
fn il_verdetto_deterministico_chiude_la_catena() {
    use kbs_core::GraderKind;
    for g in families::all() {
        let (istanza, checker) = g.build("catena").expect("istanza");
        let mut catena = GradingChain::new();
        assert_eq!(
            catena.cross(GraderKind::Peer, Outcome::Conclusive(true)),
            Err(kbs_exercise::ExerciseError::GradingOrder {
                attempted: GraderKind::Peer,
                missing: GraderKind::Deterministic
            }),
            "{}: il pari è partito senza il deterministico",
            g.family()
        );
        let verdetto = grade(&istanza, &checker, &istanza.expected).expect("checker confrontabile");
        catena
            .cross(
                GraderKind::Deterministic,
                Outcome::Conclusive(verdetto.is_correct()),
            )
            .expect("il deterministico è il primo anello");
        assert!(catena.is_closed());
        assert!(catena
            .cross(GraderKind::Human, Outcome::Conclusive(true))
            .is_err());
    }
}

/// Il quasi-miss è costruito dal checker: è la risposta che quel confronto potrebbe
/// scambiare per giusta.
fn quasi_miss(istanza: &Instance, checker: &Checker) -> String {
    match checker {
        Checker::Numeric { tolerance } => {
            let valore: f64 = istanza.expected.parse().unwrap_or_default();
            // fuori tolleranza di ordine 1, e molto oltre per le tolleranze strette
            let scarto = if *tolerance >= 1.0 {
                *tolerance + 1.0
            } else {
                0.01 + 10.0 * tolerance
            };
            format!("{:.4}", valore + scarto)
        }
        Checker::Set { elements } => elements[..elements.len() - 1].join(", "),
        Checker::MultipleChoice {
            correct_index,
            options,
        } => (0..options.len())
            .find(|i| *i != *correct_index as usize)
            .map(|i| options[i].clone())
            .unwrap_or_default(),
        Checker::Equivalence { normalized } => format!("{normalized}+1"),
    }
}
