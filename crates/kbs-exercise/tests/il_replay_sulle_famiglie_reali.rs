//! D11 chiuso: una famiglia vera, un record vero, il replay vero.
//!
//! `kbs_verify::replay` dichiarava il trait `InstanceGenerator` e si
//! dichiarava «implementato da `kbs-exercise`», ma fino a ieri nessuna famiglia
//! lo implementava: la prova della riproducibilità girava contro uno stub
//! costruito a mano dentro `kbs-verify`, e quindi non provava che i generatori
//! veri fossero riproducibili — provava che uno stub lo era. È la forma di difetto
//! di D16 con le parti scambiate: qui la terza parte, il generatore vero, non
//! esisteva.
//!
//! Perciò ogni test di questo file passa dal **consumatore vero**: il record si
//! costruisce con `ReplayRecord::new`, la riproduzione si chiede a
//! [`kbs_verify::replay`], e la risposta che il replay riproduce viene poi
//! passata al confronto vero, [`kbs_exercise::grade`]. Nessuna delle tre
//! famiglie — generatore, registro, replay — viene confrontata con una copia di
//! sé stessa.
//!
//! I semi sono semi, non coppie scelte a mano, e sono quattro per famiglia:
//! l'errore da dimostrare non è «due istanze possono coincidere» ma «questa
//! istanza, con questa tupla, si rifà».

use kbs_core::{Checker, Instance, Millis, PersonId};
use kbs_exercise::check::Verdict;
use kbs_exercise::{Family, grade};
use kbs_verify::{
    GeneratorKey, InstanceGenerator, ReplayError, ReplayField, ReplayKey, ReplayOutcome,
    ReplayRecord, replay,
};

/// L'hash del corpus al momento della generazione. È un precondito del replay,
/// non un dettaglio: con un hash diverso il replay non torna, e «non torna» è
/// un verdetto.
const CORPUS: &str = "sha256:corpus-di-prova";

/// Quattro semi, e non uno: la tupla di D11 è `(corpus, esercizio, versione,
/// seed)` e la prova che conta è che *quel* seed si rifà, non che il primo
/// torni per caso.
const SEMI: [&str; 4] = ["classe-0", "classe-1", "classe-7", "vuoto"];

fn istanza_di(famiglia: Family, seed: &str) -> (Instance, Checker) {
    famiglia
        .generator()
        .build(seed)
        .unwrap_or_else(|e| panic!("{} con seed {seed} è generabile: {e}", famiglia.name()))
}

/// Il record che il registro avrebbe scritto quando la generazione è avvenuta.
///
/// Usa [`ReplayRecord::new`], non un costruttore di comodo: è il costruttore che
/// rifiuta una tupla che non descrive l'istanza, e una prova del replay che
/// ammettesse record costruiti a mano starebbe provando metà della catena.
fn record_di(famiglia: Family, seed: &str) -> ReplayRecord {
    let (istanza, _) = istanza_di(famiglia, seed);
    let chiave = ReplayKey::new(
        CORPUS,
        istanza.exercise.clone(),
        famiglia.instance_generator().version(),
        seed,
    );
    ReplayRecord::new(
        chiave,
        istanza,
        Millis(1_700_000_000_000),
        PersonId("person_docente".into()),
    )
    .unwrap_or_else(|e| panic!("{} con seed {seed} è un record: {e}", famiglia.name()))
}

fn chiave_di(famiglia: Family, seed: &str) -> GeneratorKey {
    GeneratorKey {
        corpus_hash: CORPUS.into(),
        exercise: famiglia.generator().exercise_id(seed),
        seed: seed.into(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Il replay vero, sulle famiglie vere
// ─────────────────────────────────────────────────────────────────────────────

/// Il test che D11 chiede e che non esisteva: **un'istanza di una famiglia
/// vera, rigenerata dal suo generatore vero, è la stessa istanza registrata —
/// risposta attesa compresa.**
///
/// Il confronto passa da `kbs_verify::replay`, non da una ricerca di stringhe e
/// non da un confronto manuale delle quattro istanze: `replay` è il codice che
/// un ricorso userebbe, e se i due preconditi (corpus e versione) non tenessero
/// il verdetto cambierebbe, che è una parte della stessa proprietà.
///
/// Sull'`expected` riprodotta c'è la terza parte: la risposta che il replay
/// rigenera viene girata da `grade` con il checker vero della famiglia, e
/// deve essere accettata. Un replay che riproduce fedelmente una risposta che il
/// confronto rifiuta non è una riproduzione: è una copia.
#[test]
fn una_istanza_reale_riprodotta_riproduce_la_risposta_attesa() {
    for famiglia in Family::ALL {
        for seed in SEMI {
            let generatore = famiglia.instance_generator();
            let record = record_di(famiglia, seed);
            let out = replay(&record, CORPUS, generatore.as_ref()).unwrap_or_else(|e| {
                panic!("{} con seed {seed}: replay tentabile: {e}", famiglia.name())
            });
            assert_eq!(
                *out.verdict(),
                ReplayOutcome::Match,
                "{} con seed {seed}: {}",
                famiglia.name(),
                out.verdict()
            );

            // La terza parte: la risposta rigenerata viene girata da `grade` col
            // checker vero della famiglia, e deve essere accettata. Un replay
            // che riproduce fedelmente una risposta che il confronto rifiuta
            // non è una riproduzione: è una copia.
            let rigenerata = famiglia
                .instance_generator()
                .generate(&chiave_di(famiglia, seed))
                .unwrap_or_else(|e| panic!("{} con seed {seed}: rigenerabile: {e}", famiglia.name()));
            assert_eq!(
                rigenerata.expected, record.instance.expected,
                "{} con seed {seed}: la risposta rigenerata non è quella registrata",
                famiglia.name()
            );
            let (_, checker) = istanza_di(famiglia, seed);
            assert_eq!(
                grade(&rigenerata, &checker, &rigenerata.expected),
                Ok(Verdict::Correct),
                "{} con seed {seed}: la risposta riprodotta non passa il confronto vero",
                famiglia.name()
            );
        }
    }
}

/// La prova che il precedente non è vacuo: un generatore che **deriva** dalla
/// famiglia vera e sbaglia la risposta deve produrre un verdetto di non
/// fedeltà, con entrambi i valori dentro.
///
/// Senza questo, «Match» potrebbe dire soltanto che due chiamate uguali sono
/// uguali — che è vero anche di un generatore che sbaglia sempre, purché sbagli
/// due volte uguale. Il replay che non torna è un verdetto con due valori, e
/// questo test verifica che porti quelli giusti.
#[test]
fn un_replay_che_non_torna_su_una_famiglia_reale_e_un_verdetto_con_due_valori() {
    for famiglia in Family::ALL {
        let seed = SEMI[0];
        let record = record_di(famiglia, seed);
        let che_deriva = CheDeriva {
            dentro: famiglia.instance_generator(),
            scarto: "quasi-".into(),
        };
        let out = replay(&record, CORPUS, &che_deriva).expect("il replay si tenta");
        assert_eq!(
            *out.verdict(),
            ReplayOutcome::Mismatch {
                field: ReplayField::Expected,
                recorded: record.instance.expected.clone(),
                regenerated: format!("quasi-{}", record.instance.expected),
            },
            "{}: il replay non fedele non ha detto su quale campo",
            famiglia.name()
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Il catalogo è chiuso: ogni famiglia è riproducibile
// ─────────────────────────────────────────────────────────────────────────────

/// Ogni famiglia del catalogo implementa `InstanceGenerator`, e lo implementa
/// bene: la versione che dichiara è quella del generatore, e la tupla che le si
/// dà produce l'istanza che il generatore produce.
///
/// La proprietà che chiede la compilazione è un'altra, ed è dichiarata dove
/// vive: `Family::instance_generator` è un `match` **senza braccio `_`** su
/// [`Family`], quindi se domani si aggiunge una variante e non si scrive il suo
/// braccio il crate non compila — e il braccio ha tipo
/// `Box<dyn InstanceGenerator>`, quindi il compilatore non accetterebbe una
/// famiglia che non implementa il trait. Qui il catalogo viene enumerato e
/// ogni voce viene esercitata; il *compile-time* è in `families/mod.rs`, ed è
/// un fatto, non una promessa.
#[test]
fn ogni_famiglia_del_catalogo_sa_rigenerare_se_stessa() {
    for famiglia in Family::ALL {
        let generatore = famiglia.generator();
        let replay = famiglia.instance_generator();
        assert_eq!(
            replay.version(),
            generatore.generator_version(),
            "{}: la versione dichiarata al replay non è quella del generatore",
            famiglia.name()
        );
        assert!(!replay.version().is_empty(), "{}: versione vuota", famiglia.name());

        for seed in SEMI {
            let chiave = chiave_di(famiglia, seed);
            let attesa = generatore
                .generate(seed)
                .unwrap_or_else(|e| panic!("{} con seed {seed}: generabile: {e}", famiglia.name()));
            let ottenuta = replay
                .generate(&chiave)
                .unwrap_or_else(|e| panic!("{} con seed {seed}: rigenerabile: {e}", famiglia.name()));
            assert_eq!(
                ottenuta, attesa,
                "{} con seed {seed}: la stessa tupla ha dato due istanze",
                famiglia.name()
            );
        }
    }
}

/// Il catalogo e il registro sono la stessa cosa, nello stesso ordine.
///
/// Qui finisce la parte che il compilatore deduce e comincia quella che non
/// deduce, e va detto con precisione dove sta il confine. Le due `match` sono
/// chiuse: una famiglia che non implementa `InstanceGenerator` non entra, punto.
/// `Family::ALL` invece è un array, e un array di lunghezza fissa accetta
/// silenziosamente una variante che nessuno ci mette: una famiglia nuova
/// scriverebbe nelle `match`, non comparirebbe nel registro, e questo test
/// passerebbe lo stesso. È lo stesso buco che `kbs-fixtures` dichiara per
/// l'elenco dei giudicanti, e la ragione per cui lì l'array è rimasto: serve a
/// una cosa che il compilatore non fa (l'ordine, che è un contratto di questo
/// crate) e non finge di fare un'altra.
///
/// Quello che questo test garantisce è quindi più stretto e più onesto: il
/// registro contiene esattamente le famiglie del catalogo, nello stesso ordine,
/// e ogni nome del catalogo si trova per nome.
#[test]
fn il_catalogo_e_il_registro_dicono_le_stesse_famiglie_nello_stesso_ordine() {
    let registro = kbs_exercise::all();
    assert_eq!(
        registro.len(),
        Family::ALL.len(),
        "il registro e il catalogo hanno cardinalità diverse: una famiglia è nel catalogo e non nel registro, o viceversa"
    );
    for (famiglia, generatore) in Family::ALL.into_iter().zip(&registro) {
        assert_eq!(
            generatore.family(),
            famiglia.name(),
            "il registro ha le famiglie in un ordine diverso dal catalogo"
        );
        let cercata = kbs_exercise::by_name(famiglia.name())
            .unwrap_or_else(|| panic!("{} è nel catalogo: si trova per nome", famiglia.name()));
        assert_eq!(cercata.family(), famiglia.name());
        assert_eq!(cercata.generator_version(), generatore.generator_version());
    }
    assert!(kbs_exercise::by_name("famiglia-che-non-esiste").is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// Il rifiuto: «non posso generare» è un errore, non un verdetto
// ─────────────────────────────────────────────────────────────────────────────

/// Un generatore a cui si chiede l'esercizio di un'altra famiglia si rifiuta.
///
/// La distinzione è quella dichiarata in `kbs_verify::replay`: l'errore è quando
/// il replay non si può nemmeno tentare, il verdetto quando non torna. Produrre
/// l'istanza di un'altra famiglia sarebbe «genero qualcos'altro», che il
/// contratto del trait esclude per nome.
#[test]
fn un_generatore_non_produce_l_istanza_di_un_altra_famiglia() {
    for chiedente in Family::ALL {
        for diversa in Family::ALL {
            if chiedente == diversa {
                continue;
            }
            let chiave = chiave_di(diversa, SEMI[0]);
            let errore = chiedente
                .instance_generator()
                .generate(&chiave)
                .expect_err("una famiglia non genera l'esercizio di un'altra");
            assert!(
                matches!(errore, ReplayError::Generator { .. }),
                "{} su {}: il rifiuto non è un errore di generatore: {errore:?}",
                chiedente.name(),
                diversa.name()
            );
            assert!(
                errore.to_string().contains(chiedente.name()),
                "il rifiuto non dice quale generatore ha rifiutato: {errore}"
            );
        }
    }
}

/// Un id che non è di nessuna famiglia è un rifiuto, non un'istanza a caso: e
/// un seed che non combacia con l'id è lo stesso rifiuto, perché l'id *è* la
/// tupla.
#[test]
fn una_tupla_impossibile_e_un_rifiuto() {
    let generatore = Family::PolyExpand.instance_generator();
    let seed = SEMI[0];

    let fuori_catalogo = generatore.generate(&GeneratorKey {
        corpus_hash: CORPUS.into(),
        exercise: "famiglia-che-non-esiste@1#classe-0".into(),
        seed: seed.into(),
    });
    assert!(matches!(fuori_catalogo, Err(ReplayError::Generator { .. })));

    // id di un seed, seed di un altro: la tupla non descrive niente
    let incongruente = generatore.generate(&GeneratorKey {
        corpus_hash: CORPUS.into(),
        exercise: Family::PolyExpand.generator().exercise_id("classe-0"),
        seed: "classe-1".into(),
    });
    assert!(matches!(incongruente, Err(ReplayError::Generator { .. })));
}

// ─────────────────────────────────────────────────────────────────────────────
// I due preconditi, su dati veri
// ─────────────────────────────────────────────────────────────────────────────

/// Corpus diverso e versione diversa sono **verdetti**, non errori: sono due
/// fatti da cui si può fare appello, non due problemi temporanei.
///
/// È la stessa proprietà che `kbs-verify` prova con il suo stub, qui su una
/// famiglia e un record veri: se la differenza di versione fosse un errore, chi
/// riceve l'appello non potrebbe dire *che cosa* è cambiato, e l'appello è
/// precisamente la domanda su quello.
#[test]
fn corpus_diverso_e_versione_diversa_sono_verdetti_su_quel_campo() {
    for famiglia in Family::ALL {
        let seed = SEMI[0];
        let record = record_di(famiglia, seed);
        let replay_di = famiglia.instance_generator();

        let corpus = replay(&record, "sha256:corpus-di-un-altro", replay_di.as_ref())
            .expect("il replay si tenta anche con il corpus sbagliato");
        assert_eq!(
            *corpus.verdict(),
            ReplayOutcome::Mismatch {
                field: ReplayField::CorpusHash,
                recorded: CORPUS.into(),
                regenerated: "sha256:corpus-di-un-altro".into(),
            },
            "{}: il corpus sbagliato non è un verdetto sul corpus",
            famiglia.name()
        );

        let versione = replay(&record, CORPUS, &Vecciosta::nuova(replay_di))
            .expect("il replay si tenta anche con la versione sbagliata");
        assert_eq!(
            *versione.verdict(),
            ReplayOutcome::Mismatch {
                field: ReplayField::GeneratorVersion,
                recorded: famiglia.instance_generator().version().into(),
                regenerated: format!("{}+pippo", famiglia.instance_generator().version()),
            },
            "{}: la versione sbagliata non è un verdetto sulla versione",
            famiglia.name()
        );
    }
}

/// Un record che dice una tupla e contiene l'istanza di un'altra non è un
/// record. Provato su dati veri: la tupla nasce da un'istanza vera e viene
/// alterata di un solo campo, quindi il rifiuto parla di un record che esiste
/// per davvero e mente sul proprio replay — che è la forma che non regge un
/// ricorso.
#[test]
fn una_tupla_che_non_descrive_l_istanza_non_e_un_record() {
    let record = record_di(Family::SetDifference, SEMI[0]);
    let mut chiave = record.key.clone();
    chiave.seed = "classe-1".into();
    let errore = ReplayRecord::new(
        chiave,
        record.instance.clone(),
        record.recorded_at,
        record.recorded_by.clone(),
    )
    .expect_err("la tupla non descrive l'istanza");
    assert!(matches!(
        errore,
        ReplayError::RecordKeyMismatch {
            field: ReplayField::Seed,
            ..
        }
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
/// Un generatore che risponde come la famiglia vera e sbaglia la risposta.
///
/// Sta qui, e non dentro `kbs-verify`, per una ragione: è l'unico modo che ha
/// il test di distinguere «il replay fedele» da «due chiamate uguali». Un doppione
/// costruito in `kbs-verify` non avrebbe famiglia, non avrebbe seed, e non
/// avrebbe niente da mentire.
struct CheDeriva {
    dentro: Box<dyn InstanceGenerator>,
    scarto: String,
}

impl InstanceGenerator for CheDeriva {
    fn version(&self) -> &str {
        self.dentro.version()
    }

    fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError> {
        let mut istanza = self.dentro.generate(key)?;
        istanza.expected = format!("{}{}", self.scarto, istanza.expected);
        Ok(istanza)
    }
}

/// Un generatore che si dichiara un'altra versione: il replay deve dirlo sul
/// campo, e non rigenerare lo stesso esercizio sotto un nome diverso.
///
/// La versione dichiarata è quella del generatore avvolto più un suffisso, e
/// non una stringa fissa: un test che dichiarasse «poly-expand/1+pippo» anche
/// sulla famiglia della media pesata starebbe provando che la stringa è quella,
/// non che il verdetto cade sul campo giusto.
struct Vecciosta {
    dentro: Box<dyn InstanceGenerator>,
    versione: String,
}

impl Vecciosta {
    fn nuova(dentro: Box<dyn InstanceGenerator>) -> Self {
        let versione = format!("{}+pippo", dentro.version());
        Vecciosta { dentro, versione }
    }
}

impl InstanceGenerator for Vecciosta {
    fn version(&self) -> &str {
        &self.versione
    }

    fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError> {
        self.dentro.generate(key)
    }
}
