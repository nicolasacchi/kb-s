//! D11: il replay deterministico, verificato dove il generatore sta.
//!
//! Questo file sostituisce il controllo
//! `pipeline.esercizi.il_replay_e_deterministico`, che il banco non può esercitare. Il motivo è scritto dove
//! il controllo era, in `src/checks.rs`, e non qui: quel controllo chiedeva a
//! una pipeline che non genera esercizi le risposte che il banco ha nella
//! **propria** tabella, e il file HTML non porta la risposta attesa. Non era
//! una verifica, era un controllo impossibile.
//!
//! Qui la domanda è quella vera: **data la tupla (versione del generatore,
//! seed), il generatore restituisce la stessa istanza?** Due righe la rendono
//! vera, ed entrambe sono sul generatore vero, chiamato due volte.

use kbs_exercise::families::all;
use kbs_exercise::Generator;

/// Le famiglie sono cinque, e il numero è dichiarato: un banco che non esercita
/// tutte le famiglie esercita una famiglia, e un generatore che nessuno chiama è
/// un generatore di cui non si sa se funziona.
const FAMIGLIE_ATTESE: usize = 5;

#[test]
fn il_workspace_ha_le_famiglie_e_il_generatore_sa_dirle() {
    let famiglie = all();
    assert_eq!(
        famiglie.len(),
        FAMIGLIE_ATTESE,
        "il numero di famiglie è cambiato: se è una decisione, va scritta qui"
    );
    for g in &famiglie {
        assert!(!g.family().trim().is_empty(), "una famiglia senza nome");
        assert!(
            !g.generator_version().trim().is_empty(),
            "{}: una famiglia senza versione del generatore non è riproducibile",
            g.family()
        );
    }
}

/// Stesso seed, stessa istanza: è la definizione di replay.
///
/// Il confronto è sull'intera istanza, non su un campo: un generatore che
/// restituisce lo stesso numero con lo stesso testo ma parametri diversi non
/// sta riproducendo, sta somigliando. E il secondo richiamo è lo stesso seed
/// passato come stringa nuova, perché una cache per costruzione che riconosce
/// la *stessa* `String` non sta dimostrando niente.
#[test]
fn lo_stesso_seed_da_la_stessa_istanza() {
    for g in all() {
        for seed in ["s1", "s2"] {
            let a = g.build(seed).expect("il generatore onora il proprio seed");
            let b = g.build(seed).expect("il generatore onora il proprio seed");
            let (a, _) = a;
            let (b, _) = b;
            assert_eq!(
                a, b,
                "{} con seed {seed}: due chiamate hanno dato istanze diverse",
                g.family()
            );
            assert_eq!(a.seed, seed, "{}: l'istanza non porta il seed", g.family());
        }
    }
}

/// Il seme cambia la risposta: due istanze della stessa famiglia non possono
/// essere copiate l'una dall'altra.
///
/// È la ragione per cui D8 chiede un generatore con seed e non un esercizio con
/// un numero: se il seed non cambiasse niente, il seed sarebbe una decorazione.
#[test]
fn il_seed_cambia_la_risposta() {
    for g in all() {
        let (a, _) = g.build("s1").expect("il generatore onora il proprio seed");
        let (b, _) = g.build("s2").expect("il generatore onora il proprio seed");
        assert!(
            a.differs_from(&b),
            "{}: i semi s1 e s2 hanno prodotto la stessa risposta «{}»",
            g.family(),
            a.expected
        );
    }
}

/// L'id dell'istanza dice da dove viene, senza bisogno di una tabella.
///
/// È ciò che rende il replay riferibile a un registro: l'id è derivato da
/// famiglia, versione e seed, quindi due esercizi con lo stesso id sono lo
/// stesso esercizio e due id diversi non si possono confondere in un ricorso.
#[test]
fn l_id_dice_da_dove_viene() {
    for g in all() {
        let id = g.exercise_id("s1");
        assert!(id.contains(g.family()), "{id}: l'id non nomina la famiglia");
        assert!(
            id.contains(g.generator_version()),
            "{id}: l'id non nomina la versione del generatore"
        );
        assert!(id.contains("s1"), "{id}: l'id non nomina il seed");
        assert_eq!(id, g.exercise_id("s1"), "lo stesso seed dà lo stesso id");
        assert_ne!(id, g.exercise_id("s2"), "un seed diverso dà un id diverso");
    }
}

/// L'istanza porta i parametri che la rendono riproducibile, e sono quelli.
///
/// Un'istanza che porta la risposta ma non i parametri con cui è stata ottenuta
/// non è riproducibile: il ricorso di D11 consiste nel ricalcolare, e un
/// generatore senza i suoi parametri non si può far girare.
#[test]
fn l_istanza_porta_i_suoi_parametri() {
    for g in all() {
        let (i, _) = g.build("s1").expect("il generatore onora il proprio seed");
        assert!(
            i.params.is_object() && !i.params.as_object().expect("un oggetto").is_empty(),
            "{}: l'istanza non porta parametri, e senza parametri il replay è \
             un ricordo e non una riproduzione",
            g.family()
        );
    }
}

/// Il testo del GUARDIAN non cambia con il seed.
///
/// È statico per famiglia, ed è una scelta dichiarata in `kbs-exercise`: se
/// cambiasse a ogni istanza, `no_solution_leak` ripeterebbe mille volte la stessa
/// frase e non coprirebbe niente di nuovo. Qui si verifica che la scelta sia
/// ancora vera, perché è il tipo di cosa che si rompe senza accorgersene.
#[test]
fn il_guardian_e_statico_per_famiglia() {
    for g in all() {
        assert_eq!(
            g.guardian(),
            g.guardian(),
            "{}: il GUARDIAN non è stabile", g.family()
        );
        assert!(!g.guardian().trim().is_empty(), "{}: GUARDIAN vuoto", g.family());
    }
}
