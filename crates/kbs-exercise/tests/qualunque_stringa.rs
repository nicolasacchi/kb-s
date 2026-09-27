//! «Qualunque stringa dello studente riceve un verdetto» — misurata, non letta.
//!
//! `check` lo dichiara da quattro righe (modulo `check`, «Una risposta non è mai
//! un errore»): `grade` restituisce `Err` solo quando **l'esercizio** è rotto, e
//! le quattro funzioni di confronto restituiscono un `Verdict`, non un `Result`.
//! La pretesa è vera; quello che mancava era la misura.
//!
//! I test precedenti passavano risposte non numeriche e indici fuori intervallo —
//! cioè le forme sbagliate *brevi*. Nessuno passava una stringa arbitraria: dieci
//! kilobyte di rumore, caratteri di controllo, `usize::MAX` come indice, la
//! stringa vuota. Un contratto dichiarato più largo del contratto provato è un
//! contratto che si può perdere senza accorgersene, e qui il modo di perderlo è
//! ovvio: un `trim` generoso, un `.unwrap()` su un `parse`, un'indicizzazione che
//! esce dal vettore, una normalizzazione che non termina.
//!
//! Le istanze sono **vere**: vengono da [`kbs_exercise::families`] con i loro
//! checker, non da una struct costruita a mano. Un test che misura «qualunque
//! stringa» su un esercizio inesistente non misura la promessa, che parla di
//! esercizi.
//!
//! Il secondo pezzo è la tolleranza, e ha due metà che servono entrambe: il
//! quasi-miss che una tolleranza troppo larga accetterebbe e questa rifiuta, **e**
//! la risposta che entra nella tolleranza e che un confronto troppo severo
//! rifiuterebbe. Con una metà sola, un confronto che accetta tutto passerebbe
//! quanto un confronto che rifiuta tutto.

use kbs_core::{Checker, Instance};
use kbs_exercise::check::Verdict;
use kbs_exercise::{Family, grade};

const SEMI: [&str; 3] = ["classe-0", "classe-3", "vuoto"];

/// Le quattro forme di risposta che la promessa copre, nell'ordine in cui
/// `sort_unstable` le mette. Una promessa su quattro checker, provata su tre,
/// è una promessa più stretta di quella scritta.
const FORME: [&str; 4] = ["Equivalence", "MultipleChoice", "Numeric", "Set"];

/// Il nome della forma di risposta che quel checker confronta: serve perché un
/// fallimento dica *quale* dei quattro confronti ha ceduto, e non «un checker».
fn forma_del_checker(checker: &Checker) -> &'static str {
    match checker {
        Checker::Numeric { .. } => "Numeric",
        Checker::Set { .. } => "Set",
        Checker::MultipleChoice { .. } => "MultipleChoice",
        Checker::Equivalence { .. } => "Equivalence",
    }
}

/// Le risposte che nessuno scriverebbe, e che il sistema deve comunque confrontare.
///
/// Sono costruite **sull'istanza** (da `attesa`) per una ragione sola: perché
/// ciascuna sia sicuramente sbagliata. Una stringa «quarantadue» è quasi sempre
/// sbagliata, ma non sempre: la media pesata può valere esattamente 42,0000, e
/// un test che l'affermasse sbagliata ogni volta che il seed lo permette
/// sarebbe un test che a volte misura il seed invece che il codice. Qui ogni
/// caso porta un carattere che nessuna risposta dichiarata porta, o una forma
/// che il confronto di quella famiglia non può produrre — quindi «sbagliata» è
/// un fatto, non una speranza.
fn risposte_arbitrarie(attesa: &str) -> Vec<(&'static str, String)> {
    let rumore_di_cifre = "7".repeat(10_000);
    let rumore_di_parole = "risposta".repeat(1_250);
    vec![
        ("stringa vuota", String::new()),
        ("solo spazi", "   \t ".to_string()),
        ("dieci kB di cifre", rumore_di_cifre),
        ("dieci kB di parole", rumore_di_parole),
        // «solo zeri» non può stare qui: `000…0` è l'indice 0 scritto in
        // ventimila cifre, e su una famiglia il cui esatto è all'indice 0
        // quella risposta è *giusta*. Il comportamento è dichiarato in
        // `un_indice_in_cifre_e_ancora_l_indice`.
        ("zeri e virgole", "0,".repeat(5_000)),
        ("rumore con virgole", "abc,".repeat(2_500)),
        ("NUL davanti", format!("\u{0}{attesa}")),
        ("NUL dietro", format!("{attesa}\u{0}")),
        ("campanello ed escape", format!("\u{7}\u{1b}[31m{attesa}\u{8}")),
        ("cifre piene", "４２".to_string()),
        ("cifre arabo-indiche", "٤٢".to_string()),
        ("zerobenzio", format!("\u{feff}{attesa}")),
        ("indice usize::MAX", usize::MAX.to_string()),
        ("indice che trabocca usize", "9".repeat(10_000)),
        ("indice con NUL", "2\u{0}".to_string()),
        ("indice con segno", "+2".to_string()),
        ("indice con punto", "2.0".to_string()),
        ("indice in esadecimale", "0x2".to_string()),
        ("parentesi non bilanciate", "((2*(x+3))".to_string()),
        ("parentesi annidate", format!("{}2*(x+3){}", "(".repeat(512), ")".repeat(512))),
        ("solo parentesi", "()".repeat(128)),
        ("NaN", "NaN".to_string()),
        ("infinito", "inf".to_string()),
        ("meno infinito", "-inf".to_string()),
    ]
}

/// Il controllo di non vacuità: la risposta dichiarata dell'istanza passa, e le
/// quattro forme ci sono tutte. Senza questo, un `grade` che restituisse
/// `Incorrect` a tutto passerebbe il test sotto.
#[test]
fn la_risposta_dichiarata_passa_su_tutte_le_quattro_forme() {
    let mut forme = Vec::new();
    for famiglia in Family::ALL {
        for seed in SEMI {
            let (istanza, checker) = genera(famiglia, seed);
            assert_eq!(
                grade(&istanza, &checker, &istanza.expected),
                Ok(Verdict::Correct),
                "{} con seed {seed}: la risposta dichiarata non passa",
                famiglia.name()
            );
            let forma = forma_del_checker(&checker);
            if !forme.contains(&forma) {
                forme.push(forma);
            }
        }
    }
    forme.sort_unstable();
    assert_eq!(
        forme, FORME,
        "il test non ha esercitato tutte e quattro le forme di risposta"
    );
}

/// Qualunque stringa riceve un verdetto, e non un voto.
///
/// Nessuna delle risposte arbitrarie solleva un errore: è questa la promessa, e
/// un `Err` qui è un `grade` che ha smesso di onorare la sua firma. Nessuna
/// viene accettata: la forma di ogni caso è una forma che il confronto di quella
/// famiglia non può produrre, quindi «sbagliata» è un fatto.
#[test]
fn qualunque_stringa_riceve_un_verdetto_e_nessun_voto() {
    let mut forme = Vec::new();
    for famiglia in Family::ALL {
        for seed in SEMI {
            let (istanza, checker) = genera(famiglia, seed);
            let forma = forma_del_checker(&checker);
            if !forme.contains(&forma) {
                forme.push(forma);
            }
            for (nome, risposta) in risposte_arbitrarie(&istanza.expected) {
                let esito = grade(&istanza, &checker, &risposta);
                assert!(
                    esito.is_ok(),
                    "{forma} ({} con seed {seed}): {nome:?} non ha ricevuto un verdetto: {esito:?}",
                    famiglia.name()
                );
                assert_eq!(
                    esito.expect("un verdetto, non un errore"),
                    Verdict::Incorrect,
                    "{forma} ({} con seed {seed}): {nome:?} è stata accettata",
                    famiglia.name()
                );
            }
        }
    }
    forme.sort_unstable();
    assert_eq!(
        forme, FORME,
        "il test non ha esercitato tutte e quattro le forme di risposta"
    );
}

/// Le spaziature non cambiano il verdetto: uno studente che scrive « 42.5 » o
/// «12, 19 » ha risposto, e la risposta è la stessa.
///
/// È il rovescio della tolleranza: un confronto che normalizzasse troppo, o che
/// non normalizzasse affatto, si romperebbe qui — e il difetto sarebbe invisibile
/// sui quasi-miss.
#[test]
fn le_spaziature_intorno_alla_risposta_non_cambiano_il_verdetto() {
    for famiglia in Family::ALL {
        for seed in SEMI {
            let (istanza, checker) = genera(famiglia, seed);
            for risposta in [
                format!("  {}  ", istanza.expected),
                format!("\t{}\n", istanza.expected),
            ] {
                assert_eq!(
                    grade(&istanza, &checker, &risposta),
                    Ok(Verdict::Correct),
                    "{} con seed {seed}: gli spazi attorno alla risposta l'hanno cambiata",
                    famiglia.name()
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// La tolleranza: stretta abbastanza da prendere il quasi-miss
// ─────────────────────────────────────────────────────────────────────────────

/// Il confine della tolleranza è chiuso: la relazione è `<=`.
///
/// I numeri sono scelti perché siano **esatti in virgola mobile** — `42.5`,
/// `0.25` e `42.75` non hanno una rappresentazione approssimata — quindi il
/// confronto dice quello che dice e non quello che direbbe un arrotondamento.
///
/// Il caso `42.5005` con tolleranza `0.0005`, cioè la tolleranza esatta scritta
/// in decimali, è deliberatamente **fuori** da questo test: `42.5005` non è
/// rappresentabile, e la distanza calcolata sui due `f64` vale `0.000500000002…`,
/// cioè oltre la tolleranza. È il comportamento giusto — la tolleranza vale nello
/// spazio dei numeri che il confronto manipola, non dei numeri scritti sulla
/// lavagna — ed è il motivo per cui qui i numeri sono esatti invece di «umani».
#[test]
fn il_confine_della_tolleranza_e_chiuso() {
    let istanza = istanza_con("42.5");
    let meta = Checker::Numeric { tolerance: 0.25 };

    assert_eq!(grade(&istanza, &meta, "42.5"), Ok(Verdict::Correct));
    // esattamente a metà strada: `|a - e| == tolleranza`, e la relazione è `<=`
    assert_eq!(grade(&istanza, &meta, "42.75"), Ok(Verdict::Correct));
    assert_eq!(grade(&istanza, &meta, "42.25"), Ok(Verdict::Correct));
    // un centesimo oltre il confine, in entrambe le direzioni
    assert_eq!(grade(&istanza, &meta, "42.7501"), Ok(Verdict::Incorrect));
    assert_eq!(grade(&istanza, &meta, "42.2499"), Ok(Verdict::Incorrect));
}

/// Una tolleranza troppo larga è **acchiettata**: il quasi-miss che una
/// tolleranza relativa accetterebbe, questa la rifiuta.
///
/// La seconda riga è il caso in cui la tolleranza relativa è più larga in
/// senso assoluto: su una risposta di qualche milione, sbagliare di uno è lo
/// 0,00002%, e un confronto relativo all'1% lo dichiara «giusto». La scala della
/// risposta è arbitraria — il quaderno non garantisce l'ordine di grandezza —
/// quindi la distanza che conta è assoluta ed è in unità della risposta.
#[test]
fn una_tolleranza_troppo_larga_acchietterebbe_un_quasi() {
    // La versione disattenta: errore relativo all'1%.
    let relativo = |a: f64, e: f64| (a - e).abs() <= e.abs() * 0.01;

    // 1 · ordine di grandezza piccolo: la media pesata, che dichiara 0,0005.
    let media = genera(Family::WeightedMean, "tolleranza-1");
    let valore: f64 = media.0.expected.parse().expect("media decimale");
    let quasi = valore + 0.01;
    assert!(
        relativo(quasi, valore),
        "il confronto disattento dice «giusto»: se non lo dicesse, questo test non misurerebbe niente"
    );
    assert_eq!(
        grade(&media.0, &media.1, &quasi.to_string()),
        Ok(Verdict::Incorrect),
        "la media pesata ha accettato un quasi-miss che la tolleranza relativa accetterebbe"
    );

    // 2 · ordine di grandezza grande: il conteggio, che dichiara tolleranza zero.
    let conteggio = genera(Family::CountCoprimes, "tolleranza-2");
    let interi: f64 = conteggio.0.expected.parse().expect("intero contato");
    assert!(
        interi > 100_000.0,
        "l'istanza contata deve essere grande, altrimenti questo caso non prova niente: {interi}"
    );
    let sbagliato = interi - 1.0;
    assert!(
        relativo(sbagliato, interi),
        "il confronto disattento dice «giusto» su un intero sbagliato di uno fra milioni"
    );
    assert_eq!(
        grade(&conteggio.0, &conteggio.1, &sbagliato.to_string()),
        Ok(Verdict::Incorrect),
        "una tolleranza relativa accetterebbe un intero sbagliato su milioni"
    );
}

/// Il rovescio: quello che entra nella tolleranza dichiarata è accettato.
///
/// Un confronto troppo severo — che pretendesse la stringa identica, o che
/// usasse `<` invece di `<=` — rifiuterebbe qui. È la metà che rende il test
/// precedente una misura e non un braccio: «la tolleranza è stretta» resta vero
/// anche quando la si implementa con `0.0`.
#[test]
fn la_risposta_entro_la_tolleranza_e_accettata() {
    let istanza = istanza_con("42.5");
    let stretta = Checker::Numeric { tolerance: 0.0005 };
    for risposta in ["42.5", "42.5000", "42.5004", "42.4996"] {
        assert_eq!(
            grade(&istanza, &stretta, risposta),
            Ok(Verdict::Correct),
            "{risposta} con tolleranza 0,0005"
        );
    }

    // E la media pesata vera, che dichiara quattro decimali e tolleranza
    // 0,0005: l'arrotondamento a quattro decimali è dentro, e un centesimo di
    // scarto è fuori. La distanza di un centesimo è cento volte la tolleranza,
    // quindi qui nessun `f64` decide il risultato.
    //
    // L'arrotondamento a **due** decimali non è asserito, e la ragione va
    // scritta: metà delle volte la media è già a due decimali (`42,5000`), e
    // allora «42,50» è la risposta giusta — il verdetto dipenderebbe dal seed,
    // non dal codice. Un test che cambia verdetto quando cambia il seme non
    // misura il confronto.
    let media = genera(Family::WeightedMean, "tolleranza-3");
    let valore: f64 = media.0.expected.parse().expect("media decimale");
    assert_eq!(
        grade(&media.0, &media.1, &format!("{:.4}", valore + 0.0004)),
        Ok(Verdict::Correct),
        "quattro decimali: dentro la tolleranza dichiarata"
    );
    assert_eq!(
        grade(&media.0, &media.1, &format!("{:.4}", valore + 0.01)),
        Ok(Verdict::Incorrect),
        "un centesimo di scarto: cento volte la tolleranza dichiarata"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Le altre tre forme, e le loro regole scritte
// ─────────────────────────────────────────────────────────────────────────────

/// Nella scelta multipla il **testo** ha la precedenza sull'indice, e la
/// precedenza è dichiarata in `check` (`compare_multiple_choice`) senza essere
/// provata.
///
/// Il caso è la parte interessante: quando un'opzione è essa stessa un numero,
/// le due forme dicono cose diverse. Con opzioni `["1", "0", "x", "y"]` ed
/// esatto all'indice 1, la risposta «1» è **sbagliata** — il testo «1» è
/// l'opzione 0. Un confronto che accettasse l'indice avrebbe accettato qui, e
/// avrebbe premiato uno studente che ha indicato la posizione dell'opzione
/// giusta guardando l'opzione sbagliata.
#[test]
fn nella_scelta_multipla_il_testo_ha_la_precedenza_sull_indice() {
    let numeriche = Checker::MultipleChoice {
        correct_index: 1,
        options: vec!["1".into(), "0".into(), "x".into(), "y".into()],
    };
    let istanza = istanza_con("0");
    assert_eq!(grade(&istanza, &numeriche, "0"), Ok(Verdict::Correct));
    // «1» è il testo dell'opzione 0, che non è l'esatta: l'indice non c'entra
    assert_eq!(grade(&istanza, &numeriche, "1"), Ok(Verdict::Incorrect));

    // E l'indice dell'esatta resta accettato quando non è il testo di nessuna
    // opzione, che è il caso reale delle famiglie: le opizioni sono frazioni.
    let frazioni = Checker::MultipleChoice {
        correct_index: 2,
        options: vec!["3/4".into(), "5/8".into(), "7/8".into(), "9/10".into()],
    };
    assert_eq!(
        grade(&istanza_con("7/8"), &frazioni, "2"),
        Ok(Verdict::Correct)
    );
    assert_eq!(
        grade(&istanza_con("7/8"), &frazioni, "1"),
        Ok(Verdict::Incorrect)
    );
}

/// L'indice si legge in cifre decimali, e gli zeri iniziali non lo cambiano:
/// «0», «00» e ventimila zeri sono tutti l'opzione 0.
///
/// Questo comportamento è stato **scoperto da una stringa arbitraria** che si
/// era rivelata sbagliata: il corpus di questo file passava ventimila zeri e li
/// vedeva accettati su una famiglia il cui esatto era all'indice 0. Il verdetto
/// è corretto — l'intenzione dello studente è l'indice 0, ed è l'opzione
/// giusta — ma fino a qui era un incidente, non una scelta. Ora è dichiarato.
/// Se un giorno si volesse rifiutare gli zeri iniziali, il posto giusto resta
/// `compare_multiple_choice`, e il rifiuto sarebbe un verdetto: la stringa
/// «00000» è una risposta dello studente, non un esercizio rotto.
#[test]
fn un_indice_in_cifre_e_ancora_l_indice() {
    let checker = Checker::MultipleChoice {
        correct_index: 0,
        options: vec!["a".into(), "b".into(), "c".into(), "d".into()],
    };
    let istanza = istanza_con("a");
    for risposta in ["0".to_string(), "00".to_string(), "0".repeat(10_000)] {
        assert_eq!(
            grade(&istanza, &checker, &risposta),
            Ok(Verdict::Correct),
            "{risposta:?} (indice 0 scritto in cifre) non è stato letto come indice 0"
        );
    }
}

/// L'equivalenza non è un parser, e i refusi intorno alla forma normale sono
/// risposte sbagliate.
///
/// Le parentesi esterne **bilanciate** si tolgono; una parentesi di troppo, o di
/// poco, cambia la risposta. È la differenza fra la normalizzazione dichiarata —
/// poche righe, nessun parser — e qualunque cosa che «abbellisca» la risposta
/// dello studente.
#[test]
fn una_parentesi_di_troppo_non_e_una_parentesi_in_più() {
    let istanza = istanza_con("2*(x+3)");
    let checker = Checker::Equivalence {
        normalized: "2*(x+3)".into(),
    };
    for (risposta, atteso) in [
        ("2*(x+3)", Verdict::Correct),
        ("(2*(x+3))", Verdict::Correct),
        ("(2*(x+3)", Verdict::Incorrect),
        ("2*(x+3))", Verdict::Incorrect),
        ("(2*(x+3))(", Verdict::Incorrect),
        ("2*(x+3", Verdict::Incorrect),
    ] {
        assert_eq!(
            grade(&istanza, &checker, risposta),
            Ok(atteso),
            "{risposta:?} su una forma normale dichiarata"
        );
    }
}

/// Nell'insieme un buco non è un elemento mancante, e una risposta con due
/// virgole non è un insieme.
///
/// Il caso che i test precedenti non prendevano è il buco **fra spaziature**:
/// `« , , »` si potrebbe leggere come un insieme di due elementi vuoti, e non lo
/// è — ed è una risposta che uno scanner produce senza volerlo.
#[test]
fn nell_insieme_un_buco_non_e_un_elemento() {
    let (istanza, checker) = genera(Family::SetDifference, "insieme-1");
    for risposta in [
        "  ,  ".to_string(),
        " , ".to_string(),
        ",,".to_string(),
        format!("{},", istanza.expected),
        format!(",{}", istanza.expected),
        format!("{},,{}", istanza.expected, istanza.expected),
        format!("{} , ", istanza.expected),
    ] {
        assert_eq!(
            grade(&istanza, &checker, &risposta),
            Ok(Verdict::Incorrect),
            "{risposta:?} non descrive un insieme"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// L'istanza e il checker veri di una famiglia, su un seme.
fn genera(famiglia: Family, seed: &str) -> (Instance, Checker) {
    famiglia
        .generator()
        .build(seed)
        .unwrap_or_else(|e| panic!("{} con seed {seed} è generabile: {e}", famiglia.name()))
}

/// Un'istanza con una risposta dichiarata, per i confronti che hanno bisogno di
/// una risposta nota. I test sopra passano dalle famiglie vere; qui la risposta
/// è dichiarata perché il test è sul **confronto** e non sul generatore.
fn istanza_con(expected: &str) -> Instance {
    Instance {
        exercise: "confronto".into(),
        seed: "s".into(),
        rendered_prompt: "p".into(),
        expected: expected.into(),
        params: serde_json::json!({}),
    }
}
