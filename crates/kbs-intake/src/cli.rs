//! D10.2 — la CLI come protocollo.
//!
//! L'agente shella su `kbs` e non chiama funzioni: è già così in `kb`
//! (`README.md:232-238`) ed è il motivo per cui questo modulo è una libreria
//! con un binario sottile e non un binario con la logica dentro. Un banco che
//! si aggancia alle funzioni interne si rompe a ogni rifattorizzazione di un
//! crate che non lo riguarda; un banco che si aggancia al processo verifica
//! ciò che l'insegnante e l'agente vedono davvero.
//!
//! # Il contratto, in quattro righe
//!
//! * **una sola forma di uscita**: un JSON su stdout, sempre con `protocol`,
//!   `kbs_version`, `command` e `ok`;
//! * **il risultato è in top level**, non annidato in un `result`. Non è una
//!   scelta estetica: `kbs_fixtures::adapter` deserializza il documento che
//!   esce da `kbs verify` e pretende i campi `items` e `index` **in top
//!   level**, e un involucro che li anniderebbe romperebbe il banco. Un
//!   protocollo che mette i dati dove l'altra parte non li cerca è un
//!   protocollo rotto, non uno più elegante;
//! * **il codice di uscita significa qualcosa**: [`Uscita`] e non «0 o 1», e
//!   ogni valore ha una frase che lo spiega;
//! * **il motivo su stderr, sempre**: un agente che non legge `stdout` deve
//!   poter dire all'umano che cosa è andato storto, e un umano che non legge
//!   `stdout` deve poter leggere la ragione.
//!
//! # I codici di uscita
//!
//! | codice | vuol dire |
//! |---|---|
//! | 0 | è successo quello che si chiedeva, e l'ho verificato |
//! | 2 | la richiesta è stata **rifiutata da una regola**: non crederci |
//! | 3 | la richiesta non è nemmeno well-formed: opzioni mancanti, verbo ignoto |
//! | 4 | il sistema non ha potuto rispondere: file, database, json |
//!
//! Il 2 e il 3 non sono la stessa cosa e non si somigliano: il 3 è «non ti ho
//! capito» e si corregge guardando `--help`, il 2 è «ti ho capito e ti dico di
//! no» e si corregge cambiando il materiale. Un solo codice per i due casi
//! farebbe di ogni errore di sintassi un errore di merito.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use kbs_core::{ArgumentId, CohortId, CourseId, Millis, PersonId, PublicationState, Relation};
use kbs_store::{CourseRelation, Person, Store};
use serde::{Deserialize, Serialize};

use crate::authoring;
use crate::capture;
use crate::corpus_hash::{self, CorpusHash};
use crate::diagnosis::{self, Diagnosis};
use crate::error::{Error, Result};
use crate::gate::{self, Gate};
use crate::prompt::{DiagnosisRef, GenerationRequest};
use crate::pratica::{self, Tentativo};
use crate::route::{self, Route, Verdict};
use crate::scan;

/// La versione del protocollo. Cambia quando un campo cambia significato.
///
/// Il prefisso `kbs-cli/` invece di un semplice `1`: il nome del protocollo e
/// il suo numero sono la stessa stringa, e un protocollo che si chiama `1` non
/// dice di che cosa è la versione.
pub const PROTOCOLLO: &str = "kbs-cli/1";

/// La persona che indica un corpus quando il chiamante non ne dichiara una.
///
/// Non è un ruolo (D5): è un `PersonId`, e non ha relazioni di corso. Ciò che
/// la rende capace di rileggere le bozze è che è **l'autore** di ciò che ha
/// indicizzato, e `kbs_store::for_argument` lo deriva da lì.
pub const PERSONA_SISTEMA: &str = "person_0000";

/// Il codice di uscita. Un tipo, perché `main` non deve fare aritmetica su
/// interi e un `2` scritto a mano in tre posti è tre posti da tenere d'accordo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uscita {
    /// Quello che si chiedeva è stato fatto, e l'ho verificato.
    Ok,
    /// Una regola ha detto no. L'insegnante non deve crederci.
    Rifiutata,
    /// La richiesta non è well-formed.
    Uso,
    /// Il sistema non ha potuto rispondere.
    Sistema,
}

impl Uscita {
    pub fn code(self) -> u8 {
        match self {
            Uscita::Ok => 0,
            Uscita::Rifiutata => 2,
            Uscita::Uso => 3,
            Uscita::Sistema => 4,
        }
    }

    /// La classe di un errore è la stessa per tutti i verbi, e la decisione è
    /// qui e non in `main`: `main` non sa che cosa sia una regola.
    pub fn da_errore(e: &Error) -> Uscita {
        match e {
            Error::Uso(_)
            | Error::ComandoSconosciuto { .. }
            | Error::OpzioneMancante { .. }
            | Error::OpzioneRipetuta { .. } => Uscita::Uso,
            Error::Io { .. } | Error::Store(_) | Error::Json(_) => Uscita::Sistema,
            _ => Uscita::Rifiutata,
        }
    }
}

impl From<Uscita> for ExitCode {
    fn from(u: Uscita) -> Self {
        ExitCode::from(u.code())
    }
}

/// L'errore in forma machine-readable: codice e messaggio.
///
/// Il codice è [`Error::code`], kebab-case, ed è la stessa lingua del referto di
/// validazione. Un agente che legge `external-reference` sa che cosa è
/// successo senza fare parsing di un messaggio in italiano; un umano legge il
/// messaggio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Errore {
    pub code: String,
    pub message: String,
}

impl From<&Error> for Errore {
    fn from(e: &Error) -> Self {
        Errore { code: e.code().to_string(), message: e.to_string() }
    }
}

const AIUTO: &str = "\
kbs — le quattro strade di intake, il model lock e il gate di pubblicazione

USO:
    kbs <verbo> [opzioni] [argomenti]

VERBI:
    capture            legge un paste da stdin e lo mette in bozza (D10.1)
    verify             indicizza una cartella e stampa il referto (D10.3)
    lock               stampa il lock di una richiesta, con i suoi byte (D10)
    diagnose           registra una diagnosi orale (D6, D10)
    tenta              registra un tentativo non assistito: la riga `unaided = 1`
    generate           registra un lock e la sua generazione
    generations        gli eventi di generazione di un argomento
    ratify             ratifica per l'hash di contenuto corrente (D4)
    promote            promuove a in-corso: esige verdetto e ratifica valida
    read               rilettura di un argomento
    insegna            registra che una persona insegna un corso: scrive `teaches`
                       solo in CLI, e serve perché `teaches` non è derivabile da
                       nient'altro: senza quella riga restano chiusi l'esercizio,
                       lo scrutinio e l'export. --docente è la persona che
                       insegna, --person è chi ha registrato la riga.
    autora            porta nel registro l'esercizio che un file dichiara:
                       scrive `exercises` col checker e `instances` coll'atteso
                       prodotto dal generatore, mai scritto dal chiamante.
                       Solo in CLI, come `insegna`. --docente e chi scrive
                       l'esercizio e deve insegnare il corso, --file e il
                       file che lo dichiara, --arg e l'argomento su cui va
                       registrato. Un esercizio di una famiglia che
                       `kbs-exercise` non conosce non viene scritto, e il
                       risultato dice quale famiglia manca e quali sono le
                       cinque del catalogo.
    ciclo              il percorso completo su un corpus, in una esecuzione:
                       verify (indica) → insegna (la relazione `teaches`) →
                       ratify → promote. Si ferma al primo passo che non
                       riesce e dice quale e perché. La radice del corpus è
                       l'argomento; --docente è la persona che insegna,
                       --person l'operatore che firma le ratifiche.
                       Idempotente: sullo stesso corpus una seconda esecuzione
                       non duplica relazioni né righe, e lo dice (`recorded`,
                       `already`, `gia`). NON produce le istanze degli
                       esercizi e il referto lo dichiara: quel lavoro è il
                       verbo `autora`, che su una famiglia fuori catalogo non
                       scrive niente e dice quale famiglia manca.
    mcp                server MCP su stdio (D10.4)
    version            la versione di questo binario

OPZIONI COMUNI:
    --db <percorso>    il database. Obbligatorio per ogni verbo che scrive.
    --person <id>      chi agisce. Obbligatorio ovunque.
    --course <id>      il corso, quando non lo dichiara il documento.
    --json             accettata e senza effetto: il referto è già JSON

CODICI DI USCITA:
    0  quello che si chiedeva è stato fatto e verificato
    2  la regola ha detto no
    3  la richiesta non è well-formed
    4  il sistema non ha potuto rispondere
";

/// La versione di questo binario.
pub fn versione() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Esegue una riga di comando. `args` non include il nome del binario.
///
/// `out`, `err` e `stdin` sono parametri e non la STD della libreria: un
/// protocollo che scrive con `println!` non è testabile, e questo è un
/// protocollo.
pub fn esegui(
    args: &[String],
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
    stdin: &mut dyn Read,
) -> Uscita {
    let comando = args.first().map(String::as_str).unwrap_or("help");
    let resto = if args.is_empty() { &[][..] } else { &args[1..] };

    if matches!(comando, "help" | "--help" | "-h") {
        let _ = write!(out, "{AIUTO}");
        return Uscita::Ok;
    }
    if matches!(comando, "version" | "--version" | "-V") {
        let _ = writeln!(out, "{}", versione());
        return Uscita::Ok;
    }

    let (uscita, mut valore, rifiuto) = match dispatch(comando, resto, stdin, out) {
        Esito::Fatto(v) => (Uscita::Ok, v, None),
        Esito::NonFatto { passo, rapporto, errore } => {
            // Il motivo su stderr **anche** quando lo stdout non è silenzioso,
            // e con il nome del passo davanti: chi legge solo stderr deve
            // sapere *dove* il percorso si è fermato, non solo perché.
            let _ = match passo {
                Some(p) => writeln!(
                    err,
                    "kbs: il passo `{p}` non è riuscito: {errore} [{}]",
                    errore.code()
                ),
                None => writeln!(err, "kbs: {errore} [{}]", errore.code()),
            };
            (Uscita::da_errore(&errore), rapporto, Some(Errore::from(&errore)))
        }
    };

    // L'involucro e' aggiunto **al** valore, non attorno: `items` e `index`
    // restano in top level, che e' dove `kbs_fixtures::adapter` li cerca.
    if let Some(obj) = valore.as_object_mut() {
        obj.insert("protocol".into(), PROTOCOLLO.into());
        obj.insert("kbs_version".into(), versione().into());
        obj.insert("command".into(), comando.into());
        obj.insert("ok".into(), serde_json::Value::Bool(uscita == Uscita::Ok));
        if let Some(r) = &rifiuto {
            obj.insert(
                "error".into(),
                serde_json::to_value(r).unwrap_or(serde_json::Value::Null),
            );
        }
    }
    let _ = writeln!(out, "{}", serde_json::to_string(&valore).unwrap_or_default());
    uscita
}

fn dispatch(
    comando: &str,
    args: &[String],
    stdin: &mut dyn Read,
    out: &mut dyn std::io::Write,
) -> Esito {
    let mut inp = std::io::BufReader::new(&mut *stdin);
    match comando {
        "capture" => cattura(args, &mut inp).into(),
        "verify" => verifica(args).into(),
        "lock" => lock(args).into(),
        "diagnose" => diagnostica(args).into(),
        "generate" => genera(args).into(),
        "generations" => generazioni(args).into(),
        "ratify" => ratifica(args).into(),
        "promote" => promuovi(args).into(),
        "read" => leggi(args).into(),
        "tenta" => tenta(args).into(),
        "mcp" => servi_mcp(args, &mut inp, out).into(),
        "insegna" => insegna(args).into(),
        "autora" => autora(args).into(),
        "ciclo" => ciclo(args),
        altro => Esito::NonFatto {
            passo: None,
            rapporto: serde_json::json!({}),
            errore: Error::ComandoSconosciuto {
                nome: altro.to_string(),
                noti: "capture, verify, lock, diagnose, tenta, generate, generations, ratify, promote, read, insegna, autora, ciclo, mcp, version, help",
            },
        },
    }
}

/// Il referto di un verbo e il modo in cui è finito.
///
/// `Fatto` è il caso normale, e per tutti i verbi è l'unico: sono loro a dire
/// quando qualcosa non è riuscito, e lo dicono restituendo un errore.
///
/// `NonFatto` esiste per `ciclo`, che è l'unico verbo che può **fermarsi a
/// metà** e che deve poterlo dichiarare. Il referto di quello che è riuscito
/// viaggia con la ragione del fermo, perché un protocollo che a metà percorso
/// risponde `{}` costringe il docente a rilanciare i verbi uno per uno per
/// scoprire che cosa era già a posto — e quello che non si vede è quello che
/// si rimane.
enum Esito {
    Fatto(serde_json::Value),
    NonFatto {
        /// Il passo in cui il percorso si è fermato, quando il verbo ne ha
        /// uno. `None` per un errore che avviene prima di cominciare.
        passo: Option<&'static str>,
        rapporto: serde_json::Value,
        errore: Error,
    },
}

impl From<Result<serde_json::Value>> for Esito {
    fn from(r: Result<serde_json::Value>) -> Self {
        match r {
            Ok(valore) => Esito::Fatto(valore),
            Err(errore) => Esito::NonFatto {
                passo: None,
                rapporto: serde_json::json!({}),
                errore,
            },
        }
    }
}

/// `kbs mcp`, con la sua forma: il server serve e poi il protocollo chiude.
///
/// È una funzione perché `dispatch` non torna più `Result` e il `?` qui
/// dentro avrebbe trasformato un errore del server in un ramo del protocollo.
fn servi_mcp(
    args: &[String],
    inp: &mut dyn std::io::BufRead,
    out: &mut dyn std::io::Write,
) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = o.una("person")?.map(|s| PersonId(s.to_string()));
    let mut store = apri(&o)?;
    if let Some(p) = &by {
        registra_persona(&mut store, p)?;
    }
    crate::mcp::servi(&mut store, inp, out)?;
    Ok(serde_json::json!({ "servito": true }))
}

// ── gli argomenti ────────────────────────────────────────────────────────────

/// Le opzioni già analizzate.
///
/// Una `BTreeMap` e non una `HashMap`: il referto di un rifiuto deve essere
/// uguale sulla stessa macchina in due esecuzioni, e l'ordine d'inserimento di
/// una `HashMap` non lo garantisce.
#[derive(Debug, Default)]
struct Opzioni {
    valori: BTreeMap<String, Vec<String>>,
    bandiere: Vec<String>,
    posizionali: Vec<String>,
}

/// Le bandiere che **non** prendono valore.
///
/// Sono dichiarate e non dedotte: un parser che deduca «questa opzione vuole un
/// valore?» dal fatto che ne segue una finisce per mangiare `--json` e poi
/// fallire sul primo argomento, che è il modo più veloce per avere una CLI che
/// fa cose diverse da come si crede.
const BANDIERE: [&str; 1] = ["json"];

impl Opzioni {
    fn analizza(args: &[String]) -> Result<Opzioni> {
        let mut o = Opzioni::default();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            let Some(nome) = a.strip_prefix("--") else {
                o.posizionali.push(a.clone());
                i += 1;
                continue;
            };
            let (nome, inline) = match nome.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (nome.to_string(), None),
            };
            if BANDIERE.contains(&nome.as_str()) {
                if inline.is_some() {
                    return Err(Error::OpzioneRipetuta { nome: nome.clone() });
                }
                o.bandiere.push(nome);
                i += 1;
                continue;
            }
            let valore = match inline {
                Some(v) => v,
                None => {
                    i += 1;
                    args.get(i)
                        .cloned()
                        .ok_or(Error::OpzioneMancante { nome: nome.clone() })?
                }
            };
            let slot = o.valori.entry(nome.clone()).or_default();
            if !slot.is_empty() {
                return Err(Error::OpzioneRipetuta { nome });
            }
            slot.push(valore);
            i += 1;
        }
        Ok(o)
    }

    fn una(&self, nome: &str) -> Result<Option<&str>> {
        Ok(self.valori.get(nome).map(|v| v[0].as_str()))
    }

    fn richiesta(&self, nome: &'static str) -> Result<&str> {
        self.una(nome)?
            .ok_or_else(|| Error::OpzioneMancante { nome: nome.to_string() })
    }
}

fn apri(o: &Opzioni) -> Result<Store> {
    Ok(Store::open(o.richiesta("db")?)?)
}

fn persona(o: &Opzioni, nome: &'static str) -> Result<PersonId> {
    Ok(PersonId(o.richiesta(nome)?.to_string()))
}

fn json<T: Serialize>(v: T) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(v)?)
}

fn registra_persona(store: &mut Store, by: &PersonId) -> Result<()> {
    Ok(store.upsert_person(&Person {
        id: by.clone(),
        display_name: "operatore".into(),
        created_at: Millis(0),
    })?)
}

/// La persona esiste gia' nel registro?
///
/// Una `SELECT` sul `Store::conn()`, che `kbs-store` dichiara come la porta di
/// servizio e che dichiara anche perche' esiste. La domanda che vi si fa e' di
/// manutenzione — «esiste gia'?» — e non ha un predicato di persona per la
/// semplice ragione che riguarda una riga, non un contenuto. Il costo della
/// alternativa sarebbe peggiore: `upsert_person` sovrascrive `display_name`, e
/// creare uno studente con il suo id come nome significa rinominare uno
/// studente vero ogni volta che una diagnosi viene registrata.
fn esiste_persona(store: &Store, id: &PersonId) -> Result<bool> {
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM people WHERE id = ?1", [id.0.as_str()], |r| {
            r.get(0)
        })
        .map_err(|e| Error::Store(kbs_store::Error::Sqlite(e)))?;
    Ok(n > 0)
}

/// L'argomento esiste, o lo vede `by`?
///
/// La domanda passa dal predicato (`read_argument`) invece che da una `SELECT`,
/// perche' un id che non si puo' leggere e un id che non esiste sono la stessa
/// risposta: distinguerli sarebbe un canale per imparare che cosa c'e' nel corso.
fn esiste_argomento(store: &Store, by: &PersonId, id: &kbs_core::ArgumentId) -> Result<()> {
    match store.read_argument(by, id) {
        Ok(_) => Ok(()),
        Err(kbs_store::Error::NotReadable { .. }) | Err(kbs_store::Error::NotFound { .. }) => {
            Err(Error::ArgomentoAssente { id: id.to_string() })
        }
        Err(e) => Err(e.into()),
    }
}

fn persona_presente(store: &Store, id: &PersonId, ruolo: &'static str) -> Result<PersonId> {
    if esiste_persona(store, id)? {
        return Ok(id.clone());
    }
    Err(Error::PersonaAssente { id: id.to_string(), ruolo })
}

fn at(o: &Opzioni) -> Millis {
    o.una("at")
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .map(Millis)
        .unwrap_or_else(Millis::now)
}

// ── i verbi ──────────────────────────────────────────────────────────────────

/// D10.1. Il paste arriva su stdin, perché un paste è roba che sta negli
/// appunti e non in un file.
fn cattura(args: &[String], stdin: &mut dyn Read) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let mut testo = String::new();
    stdin
        .read_to_string(&mut testo)
        .map_err(|source| Error::Io { path: PathBuf::from("<stdin>"), source })?;
    let catturato = capture::parse_paste(&testo)?;
    let mut store = apri(&o)?;
    registra_persona(&mut store, &by)?;
    let richiesta = capture::to_request(catturato, by);
    json(route::receive(&mut store, richiesta)?)
}

/// D10.3. Indicizza la cartella e stampa il referto.
///
/// `--json` è accettato e non cambia niente: il referto **è** JSON, sempre.
/// Accettarlo serve a chi lo scrive per abitudine (ed è il banco), e
/// rifiutarlo sarebbe un test di stile travestito da robustezza. È per questo
/// che `--json` consuma il valore che segue senza diventare un errore: con
/// `--json` in coda a `verify`, il valore successivo è la radice.
fn verifica(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let radice = o
        .posizionali
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| Error::Uso("manca la radice del corpus".into()))?;
    let by = o
        .una("person")?
        .map(|s| PersonId(s.to_string()))
        .unwrap_or_else(|| PersonId(PERSONA_SISTEMA.to_string()));
    let mut store = apri(&o)?;
    registra_persona(&mut store, &by)?;
    Ok(serde_json::to_value(scan::indexa(&mut store, &radice, &by)?.report)?)
}

/// D10. Il lock. Stampa i byte canonici **e** il lock, perché un lock senza i
/// byte è un'affermazione e i byte sono la prova: chi li riceve può ricalcolare
/// l'hash senza fidarsi.
fn lock(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let corpus = match o.una("corpus")? {
        Some(_) => corpus_hash::hash_dir(o.richiesta("corpus")?)?,
        None => CorpusHash::of_empty(),
    };
    let richiesta = costruisci_richiesta(&o, corpus)?;
    let l = richiesta.lock(at(&o))?;
    Ok(serde_json::json!({
        "canonical_bytes": richiesta.canonical_bytes(),
        "lock": l,
        "diagnosis": richiesta.diagnosis,
    }))
}

fn costruisci_richiesta(o: &Opzioni, corpus: CorpusHash) -> Result<GenerationRequest> {
    let instruction = match (o.una("instruction")?, o.una("instruction-file")?) {
        (_, Some(f)) => std::fs::read_to_string(f)
            .map_err(|source| Error::Io { path: PathBuf::from(f), source })?,
        (Some(i), None) => i.to_string(),
        (None, None) => return Err(Error::OpzioneMancante { nome: "instruction".into() }),
    };
    let mut r = GenerationRequest::try_new(
        o.richiesta("model")?,
        o.richiesta("generator")?,
        corpus,
        o.richiesta("system")?,
        o.richiesta("task")?,
        o.una("excerpt")?.unwrap_or(""),
        instruction,
    )?;
    if let Some(id) = o.una("diagnosis")? {
        // La diagnosi entra nel prompt con la nota che il chiamante dichiara, e
        // non con quella del registro: da solo, `kbs lock` non ha un database
        // cui chiedere. È il buco `generations.senza-causa` in faccia, e il
        // modo di non propagarlo è non far finta di essere legati a qualcosa:
        // la nota vuota è dichiarata, non inventata.
        r = r.with_diagnosis(DiagnosisRef::new(id, "", None));
    }
    Ok(r)
}

/// D6. La diagnosi orale entra nel registro delle dimostrazioni.
fn diagnostica(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let mut store = apri(&o)?;
    registra_persona(&mut store, &by)?;
    // Lo studente e l'eventuale testimone devono gia' essere nel registro: qui
    // non si creano persone (vedi `esiste_persona`), e il nome di uno studente
    // non e' una cosa che questa strada puo' dedurre da un id.
    let studente = persona_presente(&store, &persona(&o, "student")?, "studente")?;
    let testimone = match o.una("witness")? {
        Some(w) => Some(persona_presente(&store, &PersonId(w.to_string()), "testimone")?),
        None => None,
    };
    let argomento = ArgumentId::from_rel_path(o.richiesta("arg")?);
    esiste_argomento(&store, &by, &argomento)?;
    let diagnosi = Diagnosis {
        id: o.una("source")?.unwrap_or("").to_string(),
        student: studente,
        course: CourseId(o.richiesta("course")?.to_string()),
        cohort: CohortId(o.richiesta("cohort")?.to_string()),
        argument: argomento,
        note: o.richiesta("note")?.to_string(),
        witness: testimone,
        at: at(&o),
    }
    .with_derived_id();
    json(diagnosis::registra_in_una_sessione(
        &mut store,
        diagnosi,
        "diagnosi orale dalla CLI",
    )?)
}

/// Il tentativo non assistito: la riga che alimenta il numeratore della claim.
///
/// **`--aiuto` è obbligatoria e non ha un default.** `0` è la dichiarazione
/// «nessuna pista servita durante il tentativo», e chi la scrive è **chi
/// amministra la sessione**: in questa versione nessun codice serve le piste e
/// nessun codice le conta, e il doc di [`pratica`] lo dichiara. Un valore
/// assente è «non lo so», che è una riga diversa e va registrata altrove — ed è
/// la ragione per cui la colonna `n_hints` è nullable e non `NOT NULL DEFAULT
/// 0`. Un default a zero qui scriverebbe «nessuna pista disponibile» per ogni
/// tentativo di cui nessuno ha contato niente, che è la dichiarazione retroattiva
/// che `V6__unaided.sql` vieta per la colonna.
///
/// **`--esito` non viene ricalcolato qui.** È il verdetto del verificatore
/// deterministico di `kbs-exercise`, e questo crate non lo rivaluta: una seconda
/// copia del confronto è una seconda risposta alla stessa domanda. Vedi il doc
/// di [`pratica`], che dichiara anche il punto in cui i due dovrebbero essere
/// chiamati insieme e che oggi non esiste.
///
/// Il corpo della risposta è l'osservazione con il suo `seq`, il suo `unaided` e
/// il suo `n_hints`: un agente che ha registrato un tentativo deve poter leggere
/// dalla risposta che cosa è finito nel registro, senza rileggerlo.
fn tenta(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let mut store = apri(&o)?;
    registra_persona(&mut store, &by)?;
    // Lo studente deve gia' essere nel registro, come per la diagnosi: questa
    // strada non crea persone.
    let studente = persona_presente(&store, &persona(&o, "student")?, "studente")?;
    let aiuto: u32 = o
        .richiesta("aiuto")?
        .trim()
        .parse()
        .map_err(|_| {
            Error::Uso(
                "--aiuto e' un intero non negativo: e' un conteggio di piste servite durante il tentativo, e un conteggio che non e' stato fatto non e' uno zero"
                    .to_string(),
            )
        })?;
    let esito = match o.richiesta("esito")? {
        "corretto" => true,
        "sbagliato" => false,
        altro => {
            return Err(Error::Uso(format!(
                "--esito e' `corretto` o `sbagliato`, non `{altro}`"
            )))
        }
    };
    let tentativo = Tentativo {
        id: o.una("source")?.unwrap_or("").to_string(),
        student: studente,
        course: CourseId(o.richiesta("course")?.to_string()),
        cohort: CohortId(o.richiesta("cohort")?.to_string()),
        exercise: o.richiesta("exercise")?.to_string(),
        instance: o.richiesta("instance")?.to_string(),
        n_hints: aiuto,
        correct: esito,
        at: at(&o),
    }
    .with_derived_id();
    json(pratica::registra_in_una_sessione(
        &mut store,
        &by,
        tentativo,
        "tentativo non assistito dalla CLI",
    )?)
}

/// Registra un lock e l'evento di generazione.
///
/// L'evento ha bisogno di un argomento: il materiale generato. Se non c'è
/// ancora, non c'è evento — e questa è la risposta giusta, perché un lock senza
/// evento è un lock che non è mai stato usato, e `kbs_store::record_generation`
/// lo dice già nel suo doc.
fn genera(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let corpus = match o.una("corpus")? {
        Some(_) => corpus_hash::hash_dir(o.richiesta("corpus")?)?,
        None => CorpusHash::of_empty(),
    };
    let richiesta = costruisci_richiesta(&o, corpus)?;
    let l = richiesta.lock(at(&o))?;
    let id = ArgumentId::from_rel_path(o.richiesta("arg")?);
    let mut store = apri(&o)?;
    registra_persona(&mut store, &by)?;
    esiste_argomento(&store, &by, &id)?;
    let evento = kbs_store::GenerationEvent {
        lock: l.clone(),
        argument: id.clone(),
        requester: by.clone(),
        at: l.at,
    };
    let lock_id = evento.lock_id();
    store.record_generation(&evento)?;
    Ok(serde_json::json!({
        "argument": id,
        "lock_id": lock_id,
        "lock": l,
        "canonical_bytes": richiesta.canonical_bytes(),
    }))
}

fn generazioni(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let store = apri(&o)?;
    let id = ArgumentId::from_rel_path(o.richiesta("arg")?);
    // `generations_for` è gated: il registro delle generazioni non è un
    // registro pubblico. Chi chiede le generazioni di un argomento è una
    // persona che ha una relazione con quell'argomento, e non un ruoto che
    // «sa» di generazioni: la domanda «ha il diritto di vederle?» ha una
    // risposta sola, ed è `kbs_core::may_read`.
    let by = persona(&o, "person")?;
    json(store
        .generations_for(&by, &id)?
        .iter()
        .map(|e| {
            serde_json::json!({
                "argument": e.argument,
                "requester": e.requester,
                "at": e.at.0,
                "lock_id": e.lock_id(),
                "lock": e.lock,
            })
        })
        .collect::<Vec<_>>())
}

/// D4. La ratifica da sola: registra l'atto, non promuove.
fn ratifica(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let mut store = apri(&o)?;
    let id = ArgumentId::from_rel_path(o.richiesta("arg")?);
    json(Gate::ratifica(&mut store, &id, &by, o.una("note")?.unwrap_or(""))?)
}

/// D4. La porta.
///
/// Se c'è `--path`, il verdetto è ricalcolato dal file: il file è la fonte, e
/// rivalidarlo è più onesto che fidarsi di un verdetto vecchio. Se non c'è, il
/// verdetto è vuoto e la porta non ha niente su cui appoggiarsi — il che è
/// corretto: promotare un argomento di cui non si ha il testo significa
/// promuoverlo alla cieca, e `--path` rende esplicito che il testo serve.
fn promuovi(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let mut store = apri(&o)?;
    let id = ArgumentId::from_rel_path(o.richiesta("arg")?);
    let verdetto = match o.una("path")? {
        Some(p) => {
            let sorgente = std::fs::read_to_string(p)
                .map_err(|source| Error::Io { path: PathBuf::from(p), source })?;
            gate::rivedi(&sorgente)
        }
        None => Verdict::default(),
    };
    let stato = Gate::ratifica_e_promuovi(
        &mut store,
        &id,
        &by,
        o.una("note")?.unwrap_or("promozione dalla CLI"),
        &verdetto,
    )?;
    Ok(serde_json::json!({ "state": stato, "verdict": verdetto }))
}

fn leggi(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let store = apri(&o)?;
    let id = ArgumentId::from_rel_path(o.richiesta("arg")?);
    json(store.read_argument(&by, &id)?)
}

/// `kbs insegna` — la relazione che mancava.
///
/// # Il buco che chiude
///
/// `relations` era una tabella che nessun codice di produzione scriveva.
/// `Store::add_relation` esisteva ed era testato, e i suoi soli chiamanti erano
/// test; `scan::indexa` dichiarava che «la sua relazione col corso la stabilisce
/// il chiamante», e il chiamante non esisteva.
///
/// Il predicato distingue due cose, e va detto con precisione **quale** delle
/// due era chiusa. `kbs_core::may_read` apre a `is_author` e a `is_ratifier`
/// **prima** di guardare le relazioni: chi ha scritto un argomento lo rivede,
/// e quello non è il buco. Il buco è tutto il resto, che chiede `teaches` e non
/// ha colpo di scena: `Store::exercise` (D8), `register_scope` in
/// `registers.rs` (lo scrutinio), `export_fixed_columns` (D12, la porta
/// `NotACourseTeacher`) e `kbs_server::capability::require`, che per ogni
/// rotta di corso chiama `course_relations` e restituisce `Absent` se la lista
/// è vuota. Un collega che insegnerebbe lo stesso corso, un amministratore
/// della scuola, il docente davanti ai suoi studenti: **nessuno poteva
/// diventare `teaches`**, perché nessuna strada di prodotto scriveva la tabella
/// in cui `teaches` sta. Il caso «perché non vedo niente» è chiuso per un
/// docente in una classe di prova, che è l'unico caso in cui questa riga ha un
/// nome.
///
/// # Perché solo in CLI locale, e questa parte non è negoziabile
///
/// Su HTTP o su MCP questo verbo sarebbe un bypass completo di D5.
/// `kbs_server::identity` dichiara che l'identità è **dichiarata, non
/// autenticata** — arriva nell'header `x-kbs-person` o in `?person=` — e che «il
/// confine di sicurezza è il deployment», cioè «se questo server è
/// raggiungibile da fuori, chiunque può dichiarare chi è». Su quel trasporto un
/// verbo che scrive relazioni significa, letteralmente: *dichiarati docente di
/// un corso e leggi tutto quello che c'è*, e l'unico controllo che il sistema
/// dichiara di avere è il predicato di visibilità, che la dichiarazione
/// appena concessa soddisfa da sola. Non è un rischio teorico: è la definizione
/// di `teaches` in `kbs_core::may_read`.
///
/// In CLI locale il costo è una riga e il rischio è zero, perché chi esegue il
/// comando è già dentro la macchina che possiede il database: il file è suo, e
/// `sqlite3` è a due passi. È la differenza fra *conquistare il diritto* e
/// *avere il diritto in tasca*. Perciò questo verbo non è nell'MCP e non è
/// nell'HTTP, e `la_strada_mcp` lo prova per nome: se un giorno l'MCP lo
/// espone, quel test è rosso e la domanda torna a essere una domanda.
///
/// # Che cosa scrive, e che cosa non scrive
///
/// Scrive **una sola** relazione: `teaches`. `Store::add_relation` rifiuta
/// `author_of`, `ratified` e `speculative_for` — sono fatti su un oggetto, non
/// sul corso, e il predicato li deriva dagli argomenti — e quel rifiuto **non è
/// stato allargato**: il predicato della guardia è lo stesso di prima, e un
/// verbo che scrive relazioni non è il posto giusto per aggiungerne una quarta.
///
/// # Perché `--course` e `--docente` e non due argomenti posizionali
///
/// `Opzioni` mette i posizionali in un campo che **nessun verbo usa**, e
/// l'unica eccezione è `verify`, che ha un solo ingresso — la radice del
/// corpus. Un atto con due soggetti (`corso` e `persona`) in posizione è un
/// atto in cui l'ordine conta e nessuno lo dichiara, e `kbs insegna A B`
/// accetterebbe due richieste diverse a seconda di quale sia stato scritto per
/// primo. Le opzioni nominate sono la forma che il resto della CLI usa e
/// quella che un agente può comporre senza leggere `--help` per capire quale
/// dei due fosse il corso.
///
/// Non crea **il docente** e non crea il corso; registra l'operatore, come fa
/// `capture`. Una relazione che non si potesse appoggiare a `people` o a
/// `sources` non darebbe a nessuno nessun diritto —
/// il predicato parte dal corso, e `relations.course_id` referenzia `sources` —
/// quindi le due esistenze sono verificate prima e il rifiuto è detto
/// (`persona-assente`, `corso-assente`) invece di lasciarsi indurre da un
/// `FOREIGN KEY constraint failed` che non nomina nessuno.
///
/// # La provenienza, che è il punto
///
/// `--person` non è il docente: è **chi ha registrato la riga**, e finisce in
/// `relations.recorded_by` (la colonna di `V8`). `relations` era l'unica
/// tabella che decide chi vede cosa e l'unica senza provenienza, mentre
/// `claims`, `observations` e `gradings` portano chi ha emesso. Il predicato di
/// D5 dice «questa persona insegna», e da `V8` il registro può anche dire **chi
/// lo ha dichiarato** — che è la domanda che sorge quando qualcuno che non è
/// docente vede un corso.
///
/// # Perché è idempotente
///
/// La PK di `relations` comprende `since` perché una relazione si può
/// riprendere. Un verbo eseguito due volte con `--at` diverso produrrebbe due
/// righe e una seconda che sembra una ripresa di ruolo che nessuno ha chiesto.
/// Quindi il verbo prima chiede e poi scrive: se `teaches` è già aperta
/// (`until IS NULL`), non scrive e lo dice (`"recorded": false,
/// "already": true`). Un atto che è già avvenuto non viene ripetuto per far
/// rumore, e la risposta dice quale dei due è successo invece di farlo
/// indovinare da un `ok: true` identico nei due casi.
///
/// # Che cosa questo verbo non è
///
/// * **non è `iscrivi`**: l'iscrizione è un'altra relazione e un altro
///   percorso; qui c'è il docente, che è la relazione senza la quale nessuna
///   delle altre strade è utilizzabile.
/// * **non è un ruolo**: `teaches` è una relazione in una tabella di relazioni,
///   non una colonna su `people`. D5 resta quello che è.
/// * **non chiude relazioni**: `Store::end_relation` esiste e questa CLI non lo
///   espone. Un verbo che finisce un incarico è un'altra domanda, e rispondere
///   a mezza fa più danno di non rispondere.
fn insegna(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let mut store = apri(&o)?;
    // Chi agisce viene registrato, come in `capture` e nella strada MCP: è
    // l'operatore, e senza la riga in `people` la provenienza che questa
    // funzione sta per scrivere non avrebbe a chi puntare. Il **docente** no,
    // e la ragione è la stessa che in `persona_presente`: `upsert_person`
    // sovrascrive `display_name`, e creare un collega col suo id come nome
    // significa rinominare un collega vero.
    registra_persona(&mut store, &by)?;
    let corso = CourseId(o.richiesta("course")?.to_string());
    let docente = persona(&o, "docente")?;
    insegna_relazione(&mut store, &by, &docente, &corso, at(&o))
}

/// La relazione `teaches`, senza la riga di comando.
///
/// `kbs insegna` e `kbs ciclo` chiamano **questa** e non due implementazioni:
/// il controllo di idempotenza — «`teaches` è già aperta, allora non si
/// scrive» — è una regola, e una regola in due posti è una regola che un
/// giorno i due posti smettono di concordare. Chi sceglie il passo è il
/// chiamante; chi scrive la regola è questa funzione.
fn insegna_relazione(
    store: &mut Store,
    by: &PersonId,
    docente: &PersonId,
    corso: &CourseId,
    since: Millis,
) -> Result<serde_json::Value> {
    let docente = persona_presente(store, docente, "docente")?;
    // `sources.status` non è controllato, e la ragione è che nessuna regola di
    // questo schema legge lo stato di un corso per decidere di una relazione:
    // controllarlo qui significherebbe duplicare, in un punto solo, una regola
    // che altrove non esiste. Un corso archiviato accetta l'incarico e non lo
    // usa: è un fatto registrato, non un permesso revocato.
    if store.source_status(corso)?.is_none() {
        return Err(Error::CorsoAssente { id: corso.0.clone() });
    }
    let gia_insegna = store.relations_of(&docente, corso)?.contains(&Relation::Teaches);
    if !gia_insegna {
        store.add_relation(
            &CourseRelation {
                person: docente.clone(),
                course: corso.clone(),
                relation: Relation::Teaches,
                since,
                until: None,
            },
            by,
        )?;
    }
    Ok(serde_json::json!({
        "course": corso.0.clone(),
        "relation": "teaches",
        "docente": docente.0,
        "registrato_da": by.0.clone(),
        "since": since.0,
        "recorded": !gia_insegna,
        "already": gia_insegna,
    }))
}

/// `kbs autora` — l'esercizio dal file al registro (D8).
///
/// # Il buco che chiude
///
/// `Store::upsert_exercise` e `Store::put_instance` esistevano, erano testati, e
/// non avevano **nessun chiamante di produzione**: `exercises` e `instances`
/// erano due tabelle che il sistema sapeva leggere e non sapeva riempire. Il
/// progetto aveva costruito l'archivio con i suoi registri e non aveva
/// costruito un corso. Qui c'è la strada, e la parte che conta non è che
/// scriva: è **da dove viene `expected`**.
///
/// # Perché l'atteso non è un'opzione
///
/// Nessuna funzione di [`authoring`] prende un'atteso. L'esercizio e le sue
/// istanze escono da `kbs_exercise::audit`, che chiama il generatore e
/// controlla il checker che ne è uscito; il valore che finisce in
/// `instances.expected` è quello che il programma ha prodotto per quel seed.
/// Un percorso che accettasse `expected` dal chiamante sarebbe un percorso in
/// cui la risposta la scrive il chiamante, e D8 — «l'integrità è per
/// costruzione perché **la risposta non è nel materiale che lo studente vede**»
/// — cadrebbe.
///
/// Ed è per questo che il file non viene interrogato: `data-atteso` esiste nei
/// file di `corpus-ite/` e questa strada non lo legge. Se lo leggesse, il
/// percorso avrebbe la risposta a portata di mano e la premessa del progetto
/// sarebbe un'occasione, non una costruzione.
///
/// # Perché solo in CLI locale, e la parte non è negoziabile
///
/// Su HTTP e su MCP l'identità è **dichiarata, non autenticata**
/// (`kbs_server::identity`: «il confine di sicurezza è il deployment»). Un
/// verbo che scrive `exercises` e `instances` su quel trasporto significa
/// *dichiarati docente di un corso e leggete le risposte di ogni esercizio*, e
/// l'unico controllo che il sistema dichiara di avere è il predicato di
/// visibilità — che la dichiarazione appena concessa soddisfa da sola. Come
/// `kbs insegna`, quindi: solo in CLI, e `la_strada_mcp` lo prova per nome.
///
/// # Chi scrive, e cosa dice il risultato
///
/// `--docente` è **chi scrive l'esercizio** e va in `exercises.created_by`:
/// `--person` è l'operatore, che registra l'atto, e non diventa l'autore. Le
/// due cose sono diverse perché `kbs insegna` le ha distinte e un atto che
/// registrarebbe la propria autorialità sarebbe un atto che nessuno potrebbe
/// contestare.
///
/// Il predicato è `Store::may_author`: **insegna il corso e vede
/// l'argomento**, cioè lo stesso di `Store::exercise`. Il lato che scrive il
/// checker è per costruzione il lato che lo può leggere, quindi la porta è una
/// sola e non due.
///
/// Il risultato è un esito per esercizio dichiarato, e i numeri
/// `declared`/`written`/`not_computed` ci sono anche quando sono zero: un
/// referto che omette «nessuna istanza calcolata» dice «non lo so», che è una
/// riga diversa da «non ce n'è».
fn autora(args: &[String]) -> Result<serde_json::Value> {
    let o = Opzioni::analizza(args)?;
    let by = persona(&o, "person")?;
    let file = o.richiesta("file")?;
    let mut store = apri(&o)?;
    registra_persona(&mut store, &by)?;
    // Il docente che scrive deve già esistere: `upsert_person` sovrascrive
    // `display_name`, e creare un collega col suo id come nome significa
    // rinominare un collega vero. La stessa ragione di `insegna`.
    let docente = persona_presente(&store, &persona(&o, "docente")?, "docente")?;
    let corso = CourseId(o.richiesta("course")?.to_string());
    if store.source_status(&corso)?.is_none() {
        return Err(Error::CorsoAssente { id: corso.0.clone() });
    }
    let argomento = ArgumentId::from_rel_path(o.richiesta("arg")?);
    let sorgente = std::fs::read_to_string(file)
        .map_err(|source| Error::Io { path: PathBuf::from(file), source })?;
    let atto = authoring::Act {
        source: sorgente,
        course: corso.clone(),
        argument: argomento.clone(),
        author: docente.clone(),
        at: at(&o),
    };
    let referto = authoring::registra(&mut store, &atto)?;
    Ok(serde_json::json!({
        "file": file,
        "course": corso.0,
        "argument": argomento.0,
        "docente": docente.0,
        "registrato_da": by.0,
        "catalog": authoring::catalog(),
        "declared": referto.declared,
        "written": referto.written,
        "not_computed": referto.not_computed,
        "outcomes": referto.outcomes,
    }))
}

/// `kbs ciclo` — il percorso completo, in una esecuzione.
///
/// # Che cosa fa, e in che ordine
///
/// `verify` → `insegna` → `ratify` → `promote`, che è l'ordine in cui un
/// docente li eseguirebbe a mano e che nessun `--help` spiegava. Ogni passo è
/// la **stessa funzione** che il suo verbo chiama: [`scan::indexa`],
/// [`insegna_relazione`], [`Gate::ratifica`], [`Gate::promuovi`]. Nessuna
/// scorciatoia e nessuna seconda copia della regola: il verbo composto
/// esegue i verbi, non li imita.
///
/// L'ordine non è decorativo, e la parte che lo rende obbligato è `insegna`
/// prima di `ratify`: `Gate::ratifica_e_promuovi` e `Gate::promuovi` leggono
/// l'argomento **attraverso il predicato di D5**, e un docente che non ha
/// ancora la relazione non lo vede. Su un corpus indicizzato da un operatore e
/// insegnato da un altro, la relazione è la premessa della firma, non un
/// di più.
///
/// # Perché si ferma al primo passo che non riesce
///
/// Un percorso che continua dopo un errore mente su quello che è riuscito: il
/// docente vede un referto pieno di numeri e non sa quali corrispondono a un
/// atto avvenuto. Quindi qui la parola **passo** è un passo del percorso e non
/// un file: un file che la validazione boccia non ferma il ciclo — è già nel
/// referto di `verify`, e diventa una riga di `non_promossi` con la sua
/// ragione — mentre un passo che non può essere eseguito (il database non si
/// apre, il docente non è nel roster, la porta rifiuta il testo) ferma tutto e
/// dice quale era.
///
/// # L'uscita non è mai «tutto bene» se qualcosa manca
///
/// Se un solo file non è arrivato a `in-corso`, il comando esce con il codice
/// della regola e il referto porta la lista. `ok: true` su un corpus
/// metà pubblicato sarebbe l'unica bugia che questo protocollo non si
/// concede, perché `ok` vuol dire «quello che si chiedeva è stato fatto e
/// l'ho verificato» e metà fatto non è fatto.
///
/// # Idempotente, e come lo si vede
///
/// I quattro passi lo sono per costruzione, non per una guardia messa
/// all'ingresso: `scan::indexa` registra un corso solo se non c'è e conserva
/// la ratifica che c'era ([`route`] dichiara perché), le claim si
/// riconciliano per id e testo, [`insegna_relazione`] non riscrive una
/// `teaches` già aperta, e qui la ratifica viene firmata **solo** se quella
/// che c'è non vale più per l'hash corrente. Il referto conta le due forme
/// (`ratifiche.nuove` e `ratifiche.gia`, `relazioni.nuove` e
/// `relazioni.gia`) perché un `ok: true` identico nelle due esecuzioni
/// costringerebbe a indovinare quale delle due è successa.
///
/// # Che cosa non fa, e perché il referto lo dice
///
/// **Non produce le istanze degli esercizi.** Quello è il verbo
/// [`autora`], che chiama il generatore e ne scrive l'atteso; qui non viene
/// chiamato, e su un corpus come `corpus-ite/` non chiamerebbe nulla lo
/// stesso: le famiglie che quei file dichiarano non sono le cinque del
/// catalogo chiuso di `kbs-exercise`, e `authoring` per quelle non scrive e
/// dice quale famiglia manca. Il numero di esercizi dichiarati e le famiglie
/// fuori catalogo sono **misurati** sul corpus, non dichiarati: un referto che
/// dice «istanze non prodotte» senza dire quante ne erano dichiarate è metà
/// di un silenzio.
///
/// # Perché solo in CLI locale
///
/// Come [`insegna`] e [`autora`], e per la stessa ragione: scrive la relazione
/// che apre D5, e su HTTP o su MCP l'identità è dichiarata. Un percorso che
/// dichiara l'insegnante e pubblica tutto quello che c'è è la definizione
/// letterale di `teaches` in `kbs_core::may_read`.
fn ciclo(args: &[String]) -> Esito {
    let o = match Opzioni::analizza(args) {
        Ok(o) => o,
        Err(errore) => return Esito::NonFatto { passo: None, rapporto: serde_json::json!({}), errore },
    };
    let radice = match o.posizionali.first().map(PathBuf::from) {
        Some(r) => r,
        None => {
            return Esito::NonFatto {
                passo: None,
                rapporto: serde_json::json!({}),
                errore: Error::Uso("manca la radice del corpus".into()),
            }
        }
    };
    let by = match persona(&o, "person") {
        Ok(p) => p,
        Err(errore) => return Esito::NonFatto { passo: None, rapporto: serde_json::json!({}), errore },
    };
    // Il docente non ha un default, e non può averlo: `--docente` che
    // manca significa «-nessun insegnante», che è una relazione inesistente
    // e non un default. `--person` che manca significherebbe `recorded_by` a
    // vuoto, che è il buco che `V8` chiude.
    let docente = match persona(&o, "docente") {
        Ok(p) => p,
        Err(errore) => return Esito::NonFatto { passo: None, rapporto: serde_json::json!({}), errore },
    };
    let nota = o.una("note").ok().flatten().unwrap_or("ciclo della CLI").to_string();
    let since = at(&o);

    let mut r = RefertoCiclo::nuovo(&radice, &by, &docente);
    let mut store = match apri(&o) {
        Ok(s) => s,
        Err(errore) => return r.fermo("verify", errore),
    };
    if let Err(errore) = registra_persona(&mut store, &by) {
        return r.fermo("verify", errore);
    }

    // ── passo 1: verify ─────────────────────────────────────────────────────
    let scansione = match scan::indexa(&mut store, &radice, &by) {
        Ok(s) => s,
        Err(errore) => return r.fermo("verify", errore),
    };
    r.corpus_hash = scansione.corpus_hash.as_str().to_string();
    r.file = scansione.report.items.len();
    r.entrati = scansione.receipts.iter().filter(|x| x.stored).count();
    r.verifica = serde_json::to_value(&scansione.report)
        .unwrap_or(serde_json::Value::Null);

    // I sorgenti si leggono **una volta** e si tengono: le dichiarazioni degli
    // esercizi e il verdetto alla porta devono descrivere gli stessi byte, e
    // due letture in due momenti diversi potrebbero non farlo. Il corpus di un
    // corso è una cartella di documenti, non un archivio: tenerlo in memoria
    // per la durata di un processo che dura un secondo è il prezzo di quella
    // garanzia.
    let mut sorgenti: BTreeMap<String, String> = BTreeMap::new();
    match scan::html_files(&radice) {
        Ok(files) => {
            for (rel, path) in files {
                match std::fs::read_to_string(&path) {
                    Ok(sorgente) => {
                        sorgenti.insert(rel, sorgente);
                    }
                    Err(source) => return r.fermo("verify", Error::Io { path, source }),
                }
            }
        }
        Err(errore) => return r.fermo("verify", errore),
    }
    if let Err(errore) = r.conta_esercizi(&sorgenti) {
        return r.fermo("verify", errore);
    }

    // ── passo 2: insegna ────────────────────────────────────────────────────
    // Il corso non è un'opzione: è quello che il corpus dichiara. Una cartella
    // che ne dichiara due viene insegnata per entrambi, perché il docente che
    // ha indicizzato le due ha appena detto di volerle insegnare, e chiedere
    // `--course` obbligatorio aggiungerebbe una risposta che il corpus ha già
    // dato — con il rischio di un `--course` scritto a mano che nomina un
    // corso che nel corpus non c'è, e di un ciclo che si ferma su
    // `corso-assente` per una domanda che non era quella.
    let mut corsi: BTreeSet<CourseId> = BTreeSet::new();
    for ricevuta in &scansione.receipts {
        // Solo dagli argomenti **entrati**: un file rifiutato porta il corso
        // segnaposto che `scan::rifiuto` gli mette, e insegnare un corso che
        // nessun file dichiara produrrebbe una relazione che non apre niente.
        if ricevuta.stored {
            corsi.insert(ricevuta.argument.course.clone());
        }
    }
    for corso in &corsi {
        match insegna_relazione(&mut store, &by, &docente, corso, since) {
            Ok(relazione) => {
                r.relazioni += 1;
                if relazione["recorded"].as_bool() == Some(true) {
                    r.relazioni_nuove += 1;
                } else {
                    r.relazioni_gia += 1;
                }
            }
            Err(errore) => return r.fermo("insegna", errore),
        }
    }
    r.corsi = corsi.iter().map(|c| c.0.clone()).collect();

    // ── passo 3: ratify ─────────────────────────────────────────────────────
    let mut da_promuovere: Vec<(ArgumentId, String)> = Vec::new();
    for ricevuta in &scansione.receipts {
        let rel = ricevuta.argument.rel_path.clone().unwrap_or_default();
        if !ricevuta.stored {
            r.non_promossi.push(non_promosso(&rel, &ricevuta.verdict, "il file non è entrato in register"));
            continue;
        }
        // Un verdetto bloccante non è un passo fallito: è la risposta della
        // validazione, già riportata da `verify` file per file. Fermarsi
        // qui significherebbe che un unico file rotto impedisce a tutti gli
        // altri trenta di diventare materiale — e il docente che aspetta il
        // resto avrebbe la risposta «non è riuscito» senza sapere che i
        // trenta ci sono già.
        if let Some(bloccante) = ricevuta.verdict.first_blocking() {
            r.non_promossi.push(NonPromosso {
                rel_path: rel,
                codice: bloccante.code.clone(),
                motivo: bloccante.message.clone(),
            });
            continue;
        }
        // `archiviato` non torna indietro: D4 non ha l'inverso di `archive`, e
        // `Store::publish` lo rifiuta. È un atto deliberato del docente, quindi
        // non è un guasto — ma è materiale che questo percorso non ha
        // pubblicato, e va detto come tutto il resto.
        if ricevuta.argument.state == PublicationState::Archiviato {
            r.non_promossi.push(NonPromosso {
                rel_path: rel,
                codice: "archiviato".into(),
                motivo: "l'argomento è archiviato e questa CLI non ha la strada che lo riapre: `Store::end_relation` e l'inverso di `archive` non sono esposti".into(),
            });
            continue;
        }
        let id = ricevuta.argument.id.clone();
        let fresco = ratifica_fresca(&ricevuta.argument);
        if ricevuta.argument.state == PublicationState::InCorso && fresco {
            // Era gia' ratificata **e** gia' in corso: il percorso non rifirma
            // e non ripromuove, ma le due cose ci sono e vanno dette. Il
            // `continue` le nascondeva: la seconda esecuzione riportava zero
            // ratifiche nuove e zero gia' fatte, che e' indistinguibile da
            // «non c'era niente» — l'ambiguita' che questo protocollo dichiara
            // di non avere.
            r.gia_ratificate += 1;
            r.gia_in_corso += 1;
            continue;
        }
        // La ratifica si rifirma **solo** se quella che c'è non vale più per
        // l'hash corrente: è il caso di un file cambiato dopo la firma, che è
        // esattamente l'atto che `Store::ratify` documenta. Rifirmare gli
        // stessi byte lascerebbe la seconda esecuzione diversa dalla prima
        // per un `ratified_at` che nessuno ha guardato.
        if !fresco {
            if let Err(errore) = Gate::ratifica(&mut store, &id, &docente, &nota) {
                return r.fermo("ratify", errore);
            }
            r.ratificate += 1;
        } else {
            r.gia_ratificate += 1;
        }
        da_promuovere.push((id, rel));
    }

    // ── passo 4: promote ────────────────────────────────────────────────────
    for (id, rel) in &da_promuovere {
        // Il verdetto si ricalcola dal file, come in `kbs promote --path`: il
        // testo è la fonte, e la porta deve giudicare i byte che sta per
        // firmare, non un verdetto ricordato.
        let sorgente = match sorgenti.get(rel) {
            Some(s) => s,
            None => {
                return r.fermo(
                    "promote",
                    Error::Io {
                        path: radice.join(rel),
                        source: std::io::Error::other(
                            "il file che la scansione ha indicizzato non è più fra quelli della radice",
                        ),
                    },
                )
            }
        };
        let verdetto = gate::rivedi(sorgente);
        match Gate::promuovi(&mut store, id, &docente, &verdetto) {
            Ok(_) => r.promosse += 1,
            Err(errore) => {
                // Il file che la porta chiude entra nel referto **prima** di
                // fermare il ciclo: un fermo che non nomina il file che non è
                // passato costringe il docente a ricalcolare a mano quale dei
                // trenta fosse, e la domanda giusta è sempre «quale».
                r.non_promossi.push(NonPromosso {
                    rel_path: rel.clone(),
                    codice: errore.code().to_string(),
                    motivo: errore.to_string(),
                });
                return r.fermo("promote", errore);
            }
        }
    }

    if r.non_promossi.is_empty() {
        return Esito::Fatto(r.json());
    }
    // Tutto il percorso è stato provato e qualcosa non è arrivato in porto.
    // L'errore è quello del **primo** file mancante, con la sua ragione: un
    // `Errore` qui non è un'uscita di servizio, è la dichiarazione che `ok` è
    // falso, e la ragione più utile da mostrare è quella del file da aprire per
    // prima.
    let primo = &r.non_promossi[0];
    r.fermo(
        "promote",
        Error::VerdettoBloccante {
            codice: primo.codice.clone(),
            messaggio: format!(
                "il ciclo ha promosso {} file su {}; il primo che non è passato è `{}`: {}",
                r.promosse + r.gia_in_corso,
                r.file,
                primo.rel_path,
                primo.motivo
            ),
        },
    )
}

/// La ratifica che c'è vale ancora per i byte che ci sono adesso?
///
/// È la domanda che `kbs_core::check_citable` si pone e che qui viene
/// riformulata per decidere **se firmare**, non per decidere se è citabile:
/// le due risposte coincidono oggi, e se un giorno smettono il ciclo
/// rifirmerà un testo che nessuno più può citare — che è innocuo — invece di
/// saltare la firma su un testo citabile, che non lo è.
fn ratifica_fresca(argomento: &kbs_core::Argument) -> bool {
    argomento
        .ratified
        .as_ref()
        .is_some_and(|ratifica| ratifica.contract_hash == argomento.content_hash)
}

/// Un file che il percorso non ha promosso, e perché.
#[derive(Debug, Clone, Serialize)]
struct NonPromosso {
    rel_path: String,
    codice: String,
    motivo: String,
}

/// Un file che non è entrato: il verdetto ne sa qualcosa, e se non dice niente
/// si dice almeno che non è entrato.
fn non_promosso(rel: &str, verdetto: &Verdict, perche: &str) -> NonPromosso {
    match verdetto.first_blocking() {
        Some(bloccante) => NonPromosso {
            rel_path: rel.to_string(),
            codice: bloccante.code.clone(),
            motivo: bloccante.message.clone(),
        },
        None => NonPromosso {
            rel_path: rel.to_string(),
            codice: "non-entrato".into(),
            motivo: perche.to_string(),
        },
    }
}

/// Il referto di `ciclo`, costruito passo passo.
///
/// I campi sono dichiarati anche quando sono vuoti: è la stessa ragione per
/// cui [`scan::Report`] dichiara `instances` invece di ometterlo. Un referto
/// che non ha un campo non dice «zero», dice «non lo so».
struct RefertoCiclo {
    radice: String,
    docente: String,
    operatore: String,
    corsi: Vec<String>,
    corpus_hash: String,
    file: usize,
    entrati: usize,
    relazioni: usize,
    relazioni_nuove: usize,
    relazioni_gia: usize,
    ratificate: usize,
    gia_ratificate: usize,
    promosse: usize,
    gia_in_corso: usize,
    non_promossi: Vec<NonPromosso>,
    esercizi_dichiarati: usize,
    famiglie: Vec<String>,
    famiglie_fuori_catalogo: Vec<String>,
    catalogo: Vec<String>,
    verifica: serde_json::Value,
}

impl RefertoCiclo {
    fn nuovo(radice: &PathBuf, by: &PersonId, docente: &PersonId) -> Self {
        RefertoCiclo {
            radice: radice.display().to_string(),
            docente: docente.0.clone(),
            operatore: by.0.clone(),
            corsi: Vec::new(),
            corpus_hash: String::new(),
            file: 0,
            entrati: 0,
            relazioni: 0,
            relazioni_nuove: 0,
            relazioni_gia: 0,
            ratificate: 0,
            gia_ratificate: 0,
            promosse: 0,
            gia_in_corso: 0,
            non_promossi: Vec::new(),
            esercizi_dichiarati: 0,
            famiglie: Vec::new(),
            famiglie_fuori_catalogo: Vec::new(),
            catalogo: authoring::catalog(),
            verifica: serde_json::Value::Null,
        }
    }

    /// Il percorso si è fermato al passo `passo`: il referto di quello che è
    /// riuscito viaggia con la ragione, perché il docente che ha fermato il
    /// ciclo al terzo passo ha bisogno di sapere che i primi due sono a posto.
    fn fermo(&self, passo: &'static str, errore: Error) -> Esito {
        Esito::NonFatto { passo: Some(passo), rapporto: self.json(), errore }
    }

    /// Conta gli esercizi che il corpus dichiara e le famiglie che il
    /// catalogo di `kbs-exercise` non conosce.
    ///
    /// La lettura è quella di [`authoring::dichiara`], non una seconda: un
    /// parser degli esercizi in due posti è un parser che un giorno conta una
    /// cosa diversa dall'altro, e il numero che finisce nel referto è proprio
    /// quello che il docente usa per capire che cosa manca.
    fn conta_esercizi(&mut self, sorgenti: &BTreeMap<String, String>) -> Result<()> {
        for sorgente in sorgenti.values() {
            for dichiarato in authoring::dichiara(sorgente)? {
                self.esercizi_dichiarati += 1;
                if !self.famiglie.contains(&dichiarato.family) {
                    self.famiglie.push(dichiarato.family.clone());
                }
                if !self.catalogo.contains(&dichiarato.family)
                    && !self.famiglie_fuori_catalogo.contains(&dichiarato.family)
                {
                    self.famiglie_fuori_catalogo.push(dichiarato.family.clone());
                }
            }
        }
        self.famiglie.sort();
        self.famiglie_fuori_catalogo.sort();
        Ok(())
    }

    /// Il referto, in JSON.
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "corso": self.corsi,
            "docente": self.docente,
            "operatore": self.operatore,
            "radice": self.radice,
            "corpus_hash": self.corpus_hash,
            "file": self.file,
            "file_entrati": self.entrati,
            "relazioni": {
                "totali": self.relazioni,
                "nuove": self.relazioni_nuove,
                "gia": self.relazioni_gia,
            },
            "ratifiche": { "nuove": self.ratificate, "gia": self.gia_ratificate },
            "promozioni": { "nuove": self.promosse, "gia": self.gia_in_corso },
            "non_promossi": self.non_promossi,
            "esercizi": {
                "dichiarati": self.esercizi_dichiarati,
                "famiglie": self.famiglie,
                "fuori_catalogo": self.famiglie_fuori_catalogo,
                "catalogo": self.catalogo,
                // Sempre zero, e **non** un conteggio: questo percorso non
                // chiama il generatore. `kbs autora` è il verbo che lo fa, e
                // su una famiglia fuori catalogo non scrive niente lo stesso.
                "istanze": 0,
            },
            "verifica": self.verifica,
            "testo": self.testo(),
        })
    }

    /// Il referto in parole, per chi ha lanciato il comando e non ha letto
    /// il JSON.
    ///
    /// Un campo `testo` e non un secondo canale: il protocollo dichiara una
    /// sola forma di uscita, e la forma è JSON. La frase ci sta dentro, e ci
    /// sta perché un docente non è un agente.
    fn testo(&self) -> String {
        let mut t = String::new();
        t.push_str(&format!(
            "ciclo {}\n  corpus      {}\n  hash        {}\n  file        {} ricevuti, {} entrati\n  corsi       {}\n  relazioni   {} nuove, {} già aperte\n  ratifiche   {} nuove, {} già valide\n  promozioni  {} nuove, {} già in corso",
            versione(),
            self.radice,
            self.corpus_hash,
            self.file,
            self.entrati,
            if self.corsi.is_empty() { "nessuno".to_string() } else { self.corsi.join(", ") },
            self.relazioni_nuove,
            self.relazioni_gia,
            self.ratificate,
            self.gia_ratificate,
            self.promosse,
            self.gia_in_corso,
        ));
        if !self.non_promossi.is_empty() {
            t.push_str(&format!("\nNON PROMOSSI ({}):", self.non_promossi.len()));
            for r in &self.non_promossi {
                t.push_str(&format!("\n  - {} [{}]: {}", r.rel_path, r.codice, r.motivo));
            }
        }
        t.push_str(&format!(
            "\nNON FATTO:\n  - le istanze degli esercizi: {} esercizi dichiarati nel corpus, {} istanze prodotte. Le \
famiglie sono un catalogo chiuso e questo percorso non chiama il generatore: il verbo che le \
scrive è `kbs autora`, e su una famiglia che il catalogo non conosce non scrive niente e dice \
quale famiglia manca{}.",
            self.esercizi_dichiarati,
            0,
            if self.famiglie_fuori_catalogo.is_empty() {
                String::new()
            } else {
                format!("; qui fuori catalogo: {}", self.famiglie_fuori_catalogo.join(", "))
            },
        ));
        t
    }
}

/// La strada `cli` della tabella `Route`, esposta perché un chiamante che
/// costruisce richieste per conto proprio (l'MCP, un test) possa dichiarare da
/// dove arriva senza duplicare l'enum.
pub const ROTTA_CLI: Route = Route::Cli;

#[cfg(test)]
mod tests {
    use super::*;

    fn linea(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn i_codici_di_uscita_significano_qualcosa() {
        assert_eq!(Uscita::Ok.code(), 0);
        assert_eq!(Uscita::Rifiutata.code(), 2);
        assert_eq!(Uscita::Uso.code(), 3);
        assert_eq!(Uscita::Sistema.code(), 4);
        assert_eq!(Uscita::da_errore(&Error::PasteVuoto), Uscita::Rifiutata);
        assert_eq!(
            Uscita::da_errore(&Error::ComandoSconosciuto {
                nome: "x".into(),
                noti: "y"
            }),
            Uscita::Uso
        );
    }

    #[test]
    fn un_verbo_ignoto_esce_3_e_lo_dice_su_stderr() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdin = std::io::empty();
        let u = esegui(&linea("pippo"), &mut out, &mut err, &mut stdin);
        assert_eq!(u, Uscita::Uso);
        let e = String::from_utf8(err).unwrap();
        assert!(e.contains("comando-sconosciuto"), "{e}");
        let o: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(o["ok"], false);
        assert_eq!(o["protocol"], PROTOCOLLO);
        assert_eq!(o["error"]["code"], "comando-sconosciuto");
    }

    #[test]
    fn una_opzione_senza_valore_esce_3() {
        let o = Opzioni::analizza(&linea("--db")).unwrap_err();
        assert!(matches!(o, Error::OpzioneMancante { .. }));
    }

    #[test]
    fn un_opzione_ripetuta_esce_3() {
        let o = Opzioni::analizza(&linea("--db a --db b")).unwrap_err();
        assert!(matches!(o, Error::OpzioneRipetuta { .. }));
    }

    #[test]
    fn help_esce_zero_e_non_e_un_json() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdin = std::io::empty();
        assert_eq!(esegui(&linea("help"), &mut out, &mut err, &mut stdin), Uscita::Ok);
        assert!(String::from_utf8(out).unwrap().contains("CODICI DI USCITA"));
    }

    #[test]
    fn version_esce_zero() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdin = std::io::empty();
        assert_eq!(esegui(&linea("version"), &mut out, &mut err, &mut stdin), Uscita::Ok);
        assert_eq!(String::from_utf8(out).unwrap().trim(), env!("CARGO_PKG_VERSION"));
    }
}
