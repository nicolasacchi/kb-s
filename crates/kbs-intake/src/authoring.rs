//! La strada di authoring: dal file al registro (D8).
//!
//! [`crate::scan`] indicizza un file e lo mette in bozza. Questo modulo fa
//! l'altra metà: prende l'esercizio che quel file **dichiara** e lo scrive in
//! `exercises` e `instances`, che fino a qui non avevano un solo scrittore di
//! produzione.
//!
//! # La divisione dei contributi, che è tutto il modulo
//!
//! | dal file | dal generatore |
//! |---|---|
//! | `data-esercizio` (l'etichetta del corpus) | `id` (la tupla di D11) |
//! | `data-famiglia` | `family`, `generator_version` |
//! | `data-checker` (**la forma**, non il confronto) | il `Checker` per intero |
//! | `data-seed` | `rendered_prompt`, `params`, **`expected`** |
//!
//! La riga in basso a destra è il punto. **`Instance::expected` non è un
//! parametro di nessuna funzione di questo file**: l'unico modo in cui entra
//! nel registro è che [`kbs_exercise::audit`] l'ha restituita insieme al
//! checker che la valuta, e `kbs_exercise::audit` non prende un'atteso da
//! nessuna parte — chiama il generatore e controlla quello che esce. Un
//! percorso che accettasse `expected` dal chiamante sarebbe un percorso in cui
//! la risposta la scrive il chiamante, e D8 cadrebbe nella frase che il progetto
//! scrive in tre posti: «la risposta non è nel materiale che lo studente vede».
//!
//! # Perché il file non è mai la risposta, e perché qui si vede
//!
//! Il banco lo aveva già scritto, in `checks.rs`:
//!
//! > «il file HTML rende `data-esercizio`, `data-famiglia`, `data-checker`,
//! > `data-seed` e i tre parametri, ma **non la risposta attesa**. Quindi
//! > nessuna pipeline basata su file può restituirle, e il controllo non poteva
//! > mai passare: non era una verifica, era un controllo impossibile»
//! > (`checks.rs:1554-1557`).
//!
//! Quel passaggio resta vero per il banco, e questa strada non lo smonta: qui
//! non si rilegge `data-atteso` e non si cerca la risposta in nessun testo del
//! file. La si ottiene dal programma. La differenza rispetto al banco è che il
//! banco **chiedeva** alla pipeline le istanze che aveva nella propria tabella
//! Rust, e questa strada le **produce**, una alla volta, dal seed.
//!
//! # Il primo esito onesto: la famiglia fuori catalogo
//!
//! Le famiglie di `kbs-exercise` sono un enum **chiuso di cinque**
//! (`kbs_exercise::families::Family`), e `Family::from_name` su una stringa
//! che non è una di quelle cinque restituisce `None`. Le famiglie che il
//! corpus dichiara — `costo-fisso-e-variabile`, `margine-soglia`,
//! `ottimizzazione-due-variabili`, `mappa-arco`, `decisione-sotto-vincolo`,
//! `risorsa-territorio` — **non sono nessuna delle cinque**.
//!
//! Quindi, applicata a `corpus-ite/`, questa strada **non genera le istanze**:
//! lo dice per esercizio, con il nome della famiglia e l'elenco del catalogo,
//! e **non scrive niente**. Non è un fallimento e non è un errore: è la
//! dichiarazione di un fatto, cioè che quegli esercizi sono scritti a mano e il
//! loro atteso è stato calcolato da un umano e copiato nel file. Aggiungere una
//! sesta famiglia a un enum chiuso per un esercizio che un agente ha scritto a
//! mano è una **decisione di prodotto** — quale programma genera la risposta,
//! con quale ragionamento, con quale GUARDIAN — e non un refactor di questo
//! percorso. Il percorso esiste, gira, e dice che cosa non sa fare.
//!
//! # L'id è la tupla di D11, e non è una scelta
//!
//! `instances.exercise` referenzia `exercises(id)`, e il generatore scrive
//! dentro `Instance::exercise` il valore di
//! [`::exercise_id`](kbs_exercise::Generator::exercise_id), cioè
//! `famiglia@versione#seed`. Quindi l'id della riga **non può** essere
//! `data-esercizio`: se lo fosse, [`kbs_exercise::replay_instance`] — che
//! confronta `key.exercise` con `exercise_id(seed)` — direbbe «non posso
//! generare» per ogni istanza registrata, e la risposta del registro non
//! sarebbe riproducibile. Il vincolo è nel codice di D11, non in una
//! preferenza.
//!
//! Ne segue che **un esercizio dichiarato con tre semi produce tre righe in
//! `exercises`**, una per semi, ognuna con il proprio checker: il checker *è*
//! la risposta di quell'istanza, e un checker solo non può stare nella riga di
//! tre risposte diverse. «Stesso ragionamento, parametri diversi» resta
//! leggibile, perché `family` e `generator_version` sono le stesse nelle tre
//! righe e l'id le nomina tutte e tre.
//!
//! L'etichetta del corpus (`ex_05_1`) non ha una colonna in cui stare, e
//! **non le si inventa una**: un refactor che la perda in silenzio sarebbe
//! peggio di perderla dichiarata, quindi il referto la riporta accanto agli id
//! del registro (`Declared` in [`Outcome::Written`]) e l'associazione è visibile
//! senza essere una colonna.
//!
//! # Chi può scrivere, e perché è la stessa porta della lettura
//!
//! [`Store::may_author`] è il predicato, ed è lo stesso di
//! `Store::exercise`: **insegna il corso** e **vede l'argomento**. Non è una
//! scelta conservativa: il lato che scrive un esercizio scrive anche la sua
//! risposta, quindi il lato che lo scrive è per costruzione il lato che lo può
//! leggere. Un percorso che scrivesse da parte di uno studente produrrebbe
//! righe che nessuno studente può rivedere e che il docente non ha ispezionato —
//! il peggiore dei due.
//!
//! Il verbo che chiama questo modulo sta **solo nella CLI locale**, per la
//! stessa ragione che vale a `kbs insegna` e per la ragione che
//! `kbs_server::identity` dichiara: su un trasporto dove l'identità è
//! **dichiarata, non autenticata**, un verbo che scrive il checker di un
//! esercizio significa «dichiarati docente di un corso e leggi le risposte».
//!
//! # L'ordine, che conta due volte
//!
//! Una volta per costruzione, e la funzione la rispetta: si **genera tutto**
//! e poi si **scrive tutto**. Un generatore che rifiuta il terzo seed non
//! lascia due righe a metà.
//!
//! Una volta per granularità, e questa è dichiarata: l'unità è **l'esercizio
//! dichiarato**, non il file. Un esercizio che non si calcola non impedisce
//! agli altri dello stesso file di entrare, e il referto li distingue uno per
//! uno. Il predicato invece viene chiesto **una volta sola, prima di qualunque
//! scrittura**: se chi scrive non può, non entra niente.

use kbs_core::{ArgumentId, Checker, CourseId, Millis, PersonId};
use kbs_exercise::families::{self, Family};
use kbs_exercise::{PublicationError, audit};
use kbs_store::Store;
use scraper::{Html, Selector};
use serde::Serialize;

use crate::error::{Error, Result};

/// Che cosa il file dichiara di un esercizio.
///
/// Sono quattro attributi e nient'altro: `data-esercizio`, `data-famiglia`,
/// `data-checker`, `data-seed`. **`data-atteso` non è qui**, e la sua assenza
/// è una scelta: è l'atteso che il file non deve portare, ed è il primo pezzo
/// che il percorso non legge. Vedi il doc del modulo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// `data-esercizio`: l'etichetta del corpus, non l'id del registro.
    pub id: String,
    /// `data-famiglia`.
    pub family: String,
    /// `data-checker`: **la forma** del confronto (`set`, `equivalence`, …).
    /// Serve a controllare che il generatore produca la stessa forma, non a
    /// costruire il checker.
    pub checker: Option<String>,
    /// `data-seed`, nell'ordine del documento. L'ordine non è un dettaglio:
    /// due seed producono due risposte diverse, e l'elenco è la copia
    /// inefficace che D8 rende inutile.
    pub seeds: Vec<String>,
}

/// Perché un esercizio dichiarato non è finito nel registro.
///
/// Un enum e non una stringa: il referto di questo percorso lo legge un
/// agente, e «la famiglia non è nel catalogo» e «il GUARDIAN rivela la risposta»
/// sono correzioni diverse, dette da persone diverse, in tempi diversi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "motivo", rename_all = "kebab-case")]
pub enum Reason {
    /// La famiglia non è una delle cinque del catalogo chiuso. `catalog` porta
    /// i nomi di tutte e cinque, perché «non calcolato» senza sapere che cosa
    /// c'era sarebbe un silenzio travestito — è la stessa ragione per cui
    /// `checks.rs` nomina le famiglie che ha trovato.
    FamilyOutsideCatalog { catalog: Vec<String> },
    /// Il blocco dichiara un esercizio e non dichiara `field`, e senza quel
    /// campo non c'è niente da generare.
    DeclarationIncomplete { field: &'static str },
    /// Il generatore ha rifiutato il seed, o il checker che ne è uscito non è
    /// confrontabile. `detail` è il messaggio di `kbs-exercise`, che sa dire
    /// quale dei due è stato.
    NotCalculable { seed: String, detail: String },
    /// Il GUARDIAN della famiglia contiene la risposta: l'esercizio non
    /// pubblica, e non è riapribile. È l'unico caso che nessun intervento sul
    /// seed può sistemare.
    GuardianLeaks { seed: String, detail: String },
    /// Il file dichiara una forma di confronto e il generatore ne produce
    /// un'altra. Il corpus e il catalogo dicono cose diverse sullo stesso
    /// esercizio, e scegliere una delle due è indovinare.
    DeclaredCheckerDiffers { declared: String, produced: String },
}

/// Che cosa è successo a un esercizio dichiarato.
///
/// `Written` **non porta `expected`**, e non è un campo che si è dimenticato di
/// mettere: il referto di un verbo finisce dove finisce, e questa strada
/// esiste perché la risposta resti nel registro e non nel materiale che si
/// mostra. Un referto che la stampasse sarebbe un secondo canale per la risposta,
/// e D8 ne ha uno solo perché uno basta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "esito", rename_all = "kebab-case")]
pub enum Outcome {
    /// L'esercizio e le sue istanze sono nel registro.
    Written {
        /// L'etichetta del corpus, `data-esercizio`.
        declared: String,
        family: String,
        /// `generator_version`: il programma, per versione.
        generator: String,
        /// Gli id nel registro, uno per semi, nell'ordine dei semi.
        exercises: Vec<String>,
    },
    /// L'esercizio è dichiarato e non è finito nel registro, per il motivo detto.
    NotComputed {
        declared: String,
        family: String,
        seeds: Vec<String>,
        reason: Reason,
    },
}

/// Il referto della strada: un esito per esercizio dichiarato, e tre numeri.
///
/// I tre numeri sono il referto anche quando sono zero: un referto che omette
/// «nessuna istanza calcolata» dice «non lo so», che è una riga diversa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthoringReport {
    pub declared: usize,
    pub written: usize,
    pub not_computed: usize,
    pub outcomes: Vec<Outcome>,
}

/// L'atto di authoring: chi scrive, che cosa, su che cosa, e quando.
///
/// `source` è il **contenuto** del file e non il suo percorso, perché il
/// percorso è del chiamante (che lo sa e lo mette nel suo referto) e il
/// contenuto è la sola cosa che questo modulo legge.
#[derive(Debug, Clone)]
pub struct Act {
    pub source: String,
    pub course: CourseId,
    pub argument: ArgumentId,
    /// Chi scrive l'esercizio: finisce in `exercises.created_by`.
    pub author: PersonId,
    pub at: Millis,
}

/// Legge gli esercizi che un file dichiara.
///
/// Non legge nient'altro: quattro attributi su un `div.esercizio` e i
/// `data-seed` che contiene. **`data-atteso` non viene cercato**, e questa
/// funzione è l'unico posto in cui un file del corpus viene letto per
/// l'authoring, quindi il punto in cui quella promessa è verificabile.
pub fn dichiara(source: &str) -> Result<Vec<Declared>> {
    let doc = Html::parse_document(source);
    let esercizi = selettore("div.esercizio[data-esercizio]")?;
    let semi = selettore("[data-seed]")?;
    let mut out = Vec::new();
    for el in doc.select(&esercizi) {
        // Il selettore chiede già `data-esercizio`; il filtro difende dalla
        // difformità fra i due linguaggi, che è il caso in cui una guardia
        // smette di vedere e un `unwrap` fa il danno.
        let Some(id) = el.value().attr("data-esercizio").map(str::trim) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        out.push(Declared {
            id: id.to_string(),
            family: el
                .value()
                .attr("data-famiglia")
                .unwrap_or_default()
                .trim()
                .to_string(),
            checker: el
                .value()
                .attr("data-checker")
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string),
            seeds: el
                .select(&semi)
                .filter_map(|s| s.value().attr("data-seed"))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        });
    }
    Ok(out)
}

/// I nomi delle cinque famiglie del catalogo.
///
/// Va nel referto di ogni esercizio non calcolato per una ragione sola: chi
/// legge «famiglia fuori catalogo» senza l'elenco non può sapere che cosa
/// avrebbe dovuto esserci, e la domanda giusta («è il nome sbagliato o è una
/// famiglia che non esiste?») resta senza risposta.
pub fn catalog() -> Vec<String> {
    Family::ALL.iter().map(|f| f.name().to_string()).collect()
}

/// Porta un esercizio dichiarato dentro il registro.
///
/// L'ordine è dichiarato nel doc del modulo e non è un dettaglio di
/// implementazione: prima il predicato, poi la generazione di tutto, e solo
/// dopo la scrittura.
pub fn registra(store: &mut Store, act: &Act) -> Result<AuthoringReport> {
    // Il predicato **prima di qualunque scrittura**, e una volta sola: se chi
    // scrive non può, il registro resta com'era. `NotReadable` viene tradotto
    // in [`Error::NotAuthor`] perché il chiamante ha bisogno di nominare la
    // persona e l'argomento, e non cambia la risposta in due casi: non insegna
    // il corso e non vede l'argomento danno lo stesso rifiuto, perché distinguerli
    // insegnerebbe che cosa c'è in un corso. Ogni altro errore del predicato
    // passa com'è, perché è un guasto e non un no.
    match store.may_author(&act.author, &act.course, &act.argument) {
        Ok(()) => {}
        Err(kbs_store::Error::NotReadable { person, id }) => {
            return Err(Error::NotAuthor {
                person: person.0,
                course: act.course.0.clone(),
                argument: id.0,
            });
        }
        Err(altro) => return Err(altro.into()),
    }

    let declared = dichiara(&act.source)?;
    let mut outcomes = Vec::with_capacity(declared.len());
    for d in &declared {
        outcomes.push(outcome_for(store, act, d)?);
    }
    let written = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Written { .. }))
        .count();
    Ok(AuthoringReport {
        declared: outcomes.len(),
        written,
        not_computed: outcomes.len() - written,
        outcomes,
    })
}

/// L'esito di un esercizio dichiarato: generato tutto, poi scritto tutto.
fn outcome_for(store: &mut Store, act: &Act, d: &Declared) -> Result<Outcome> {
    if d.family.is_empty() {
        return Ok(non_computed(d, Reason::DeclarationIncomplete { field: "data-famiglia" }));
    }
    if d.seeds.is_empty() {
        return Ok(non_computed(d, Reason::DeclarationIncomplete { field: "data-seed" }));
    }
    let Some(generatore) = families::by_name(&d.family) else {
        return Ok(non_computed(d, Reason::FamilyOutsideCatalog { catalog: catalog() }));
    };

    // Fase uno: generare. Niente è ancora nel registro, quindi un rifiuto a
    // metà non lascia righe: l'esercizio entra intero o non entra.
    let mut righe = Vec::with_capacity(d.seeds.len());
    for seed in &d.seeds {
        let (istanza, checker) = match audit(generatore.as_ref(), seed) {
            Ok(tuppla) => tuppla,
            Err(PublicationError::Leaks(leak)) => {
                return Ok(non_computed(
                    d,
                    Reason::GuardianLeaks { seed: seed.clone(), detail: leak.to_string() },
                ));
            }
            Err(PublicationError::BrokenChecker(errore)) => {
                return Ok(non_computed(
                    d,
                    Reason::NotCalculable { seed: seed.clone(), detail: errore.to_string() },
                ));
            }
        };
        // La forma dichiarata dal file e la forma prodotta dal generatore
        // devono essere la stessa. Se non lo sono, il corpus e il catalogo
        // dicono cose diverse sullo stesso esercizio: si registra la
        // contraddizione, non una delle due.
        let prodotta = etichetta(&checker);
        if let Some(dichiarata) = &d.checker {
            if dichiarata != prodotta {
                return Ok(non_computed(
                    d,
                    Reason::DeclaredCheckerDiffers {
                        declared: dichiarata.clone(),
                        produced: prodotta.to_string(),
                    },
                ));
            }
        }
        let esercizio = generatore
            .exercise_for(seed, act.course.clone(), act.argument.clone(), act.at, act.author.clone())
            .map_err(|errore| Error::ExerciseUnbuildable {
                seed: seed.clone(),
                detail: errore.to_string(),
            })?;
        righe.push((esercizio, istanza));
    }

    // Fase due: scrivere. `expected` è qui dentro perché l'ha messa `audit`,
    // e da nessun'altra parte del percorso.
    let mut esercizi = Vec::with_capacity(righe.len());
    for (esercizio, istanza) in &righe {
        store.upsert_exercise(esercizio)?;
        store.put_instance(istanza)?;
        esercizi.push(esercizio.id.clone());
    }
    Ok(Outcome::Written {
        declared: d.id.clone(),
        family: d.family.clone(),
        generator: righe
            .first()
            .map(|(e, _)| e.generator_version.clone())
            .unwrap_or_default(),
        exercises: esercizi,
    })
}

/// L'esito di un esercizio che non entra, costruito una volta sola.
fn non_computed(d: &Declared, reason: Reason) -> Outcome {
    Outcome::NotComputed {
        declared: d.id.clone(),
        family: d.family.clone(),
        seeds: d.seeds.clone(),
        reason,
    }
}

/// La forma del confronto, come la nomina il file.
///
/// Le quattro stringhe sono le stesse del `CHECK` di
/// `V4__esercizi_e_generazioni.sql` e le stesse di `kbs_store::codec`. Sono la
/// terza copia in questa posizione, e la ragione per cui la funzione esiste
/// invece di un `Checker::label()` su `kbs_core` è che qui serve confrontare
/// una **stringa del file** con una **forma**: la domanda è di traduzione, e
/// tenerla in un posto la rende sbagliabile una volta sola. Il `match` è
/// chiuso, quindi una quinta forma in `kbs_core` rompe la compilazione qui
/// invece di passare in silenzio.
fn etichetta(c: &Checker) -> &'static str {
    match c {
        Checker::Numeric { .. } => "numeric",
        Checker::Set { .. } => "set",
        Checker::MultipleChoice { .. } => "multiple-choice",
        Checker::Equivalence { .. } => "equivalence",
    }
}

/// Un selettore costante, o un errore che lo dice.
///
/// Non si rimanda a un elenco vuoto: un selettore che non compila è un bug di
/// questo file, e un `[]` qui significherebbe «questo file non dichiara
/// esercizi», che è la risposta di un file onesto e non di questo codice.
fn selettore(s: &str) -> Result<Selector> {
    Selector::parse(s).map_err(|e| Error::Uso(format!("il selettore `{s}` non è valido: {e}")))
}

