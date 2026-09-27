//! Il processo: che cosa si apre, dove si ascolta, e quando si esce.
//!
//! Tutta la logica del binario sta qui e non in `main.rs`, per la ragione che
//! vale in `kbs-intake` e vale uguale: il binario è un involucro, e questa
//! libreria è ciò che un test può avviare davvero, su una porta davvero, senza
//! `argv` e senza `fork`. `main.rs` è il nome del processo, il parsing della
//! riga di comando e nient'altro.
//!
//! # Perché il nome è `kbs-serve` e non `kbs`
//!
//! `kbs-intake` dichiara già `[[bin]] name = "kbs"`, e
//! `kbs-fixtures::adapter::ProcessPipeline` cerca `target/debug/kbs`: è il banco
//! che chiama quel processo (D10.2, «la CLI come protocollo»). Due crate che
//! producono un binario omonimo nella stessa cartella di build **non
//! falliscono**: l'ultimo che compila vince, e in silenzio. Il banco che oggi
//! chiama `target/debug/kbs` avrebbe potuto iniziare a parlare con il daemon e
//! non se ne sarebbe accorto, perché i due parlano entrambi HTTP. Il nome è
//! quindi una difesa del banco, non una preferenza estetica.
//!
//! # I codici di uscita
//!
//! Gli stessi quattro di `kbs-intake`, e non per uniformità ma perché sono gli
//! stessi quattro significati:
//!
//! | codice | vuol dire |
//! |---|---|
//! | 0 | partito e chiuso con un segnale |
//! | 2 | **rifiutato da una regola**: corpus assente, database dentro il corpus, epoch futura |
//! | 3 | non ho capito gli argomenti |
//! | 4 | il sistema non ha potuto rispondere: apertura, bind |
//!
//! Il 2 è «ti ho capito e ti dico di no, e il rimedio è nel messaggio». Qui il
//! rimedio cambia da caso a caso — una directory, un flag, un binario più
//! nuovo — e la tabella di `kbs` parla di un rimedio solo. Quello che non
//! cambia è che il 2 non è mai un errore di sintassi: `--corpus` che manca è 3,
//! `--corpus` che punta a una directory che non c'è è 2.
//!
//! # Il database non lo crea nessuno, e non è dentro il corpus
//!
//! Il default è `kb-s.sqlite3` nella **directory di lavoro**, e la ragione per
//! cui non è «dentro il corpus» è D12: il corpus è una cartella di file
//! versionata su git e l'uscita è `rm -rf`. Un SQLite con i suoi `-wal` e
//! `-shm` dentro un albero che si versiona è rumore nel diff, e dentro un
//! albero che si cancella è un registro che sparisce con i file che dovrebbe
//! descrivere. Il server dell'istituto è un mirror (D12), non il padrone del
//! registro.
//!
//! Con `--db` lo si può mettere dove si vuole, **tranne dentro il corpus**, e
//! il rifiuto è esplicito: vedi [`controlla_db`], che dice anche perché la
//! direzione opposta è ammessa. Non è una protezione HTTP — un file dentro il
//! corpus non è scaricabile, perché la rotta degli artifact risolve il
//! percorso da `rel_path` nel database e non dall'URL. È una difesa contro un
//! errore di chi avvia.
//!
//! # Il corpus non viene creato
//!
//! Una radice corpus creata dal daemon è una radice **vuota**, e su una radice
//! vuota ogni artifact è `404` — la stessa risposta di «non lo vedi». Chi
//! avvia un'istanza con il percorso sbagliato avrebbe un server sano che non
//! serve niente, e la ragione sarebbe invisibile. Un daemon che si rifiuta di
//! partire e nomina il percorso che ha cercato costa una riga di stderr e
//! risparmia un'ora di debug. Vedi [`controlla_corpus`].
//!
//! # L'identità è una dichiarazione, e si dice all'avvio
//!
//! [`crate::identity`] spiega che [`Identity`](crate::Identity) è un id che
//! *arriva da qualche parte*. Un operatore che lancia il daemon e vede una
//! porta aperta senza altro ha diritto a un avvertimento, ed è l'unico momento
//! in cui quell'avvertimento viene letto. Perciò l'avviso è su **stdout**,
//! all'avvio, e non in un file di log.
//!
//! # Quello che questo modulo non ha
//!
//! **Un `tracing_subscriber`.** Gli eventi `tracing` emessi dagli handler
//! arrivano a un sottoscrittore che qui non c'è, e quindi non vengono
//! stampati: il registro accessi di questa istanza è quello del processo che
//! la avvia. È un limite dichiarato, non nascosto — aggiungere
//! `tracing-subscriber` porterebbe dentro una dipendenza che oggi il workspace
//! non ha, e il binario continua a essere utile senza.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use kbs_store::{Error as StoreError, binary_epoch};
use tokio::net::TcpListener;

use crate::config::ServerConfig;
use crate::db::Db;
use crate::router::app;

/// Il nome del binario, e la stringa che `--version` stampa.
///
/// `kbs-serve` e non `kbs`: vedi la nota sul nome in cima a questo modulo.
pub const NOME: &str = "kbs-serve";

/// L'indirizzo di default: **loopback**.
///
/// Un daemon che di default si mette in ascolto su tutta la rete è un
/// piedistallo per un prodotto il cui modello di sicurezza è «non esiste» e
/// «non è tuo» indistinguibili, e in cui l'identità è dichiarata. Chi vuole
/// esporlo lo scrive esplicitamente, e [`annuncio`] lo dice a voce alta quando
/// lo fa.
pub const INDIRIZZO_PREDEFINITO: &str = "127.0.0.1:8787";

/// Il nome del file di database quando `--db` non c'è.
pub const DATABASE_PREDEFINITO: &str = "kb-s.sqlite3";

/// La versione, quella del crate.
pub const VERSIONE: &str = env!("CARGO_PKG_VERSION");

/// Che cosa può impedire al daemon di partire.
///
/// Le varianti sono distinte perché i rimedi sono distinti, e un errore che
/// porta con sé il rimedio è un errore che non costa un giro di domande.
#[derive(Debug, thiserror::Error)]
pub enum Errore {
    /// Gli argomenti non sono un comando.
    #[error("{0}")]
    Uso(String),

    /// `--corpus` punta a una directory che non c'è o non è una directory.
    #[error(
        "il corpus {percorso} non è una cartella.\n\
         Il daemon non la crea: una radice vuota fa rispondere «non c'è» a ogni \
         artifact, che è la stessa risposta di «non lo vedi» — e il motivo \
         diventerebbe invisibile.\n\
         Indica la cartella che contiene gli artifact, oppure creala e rilancia."
    )]
    CorpusAssente { percorso: PathBuf },

    /// `--db` sta dentro `--corpus`.
    #[error(
        "il database {database} è dentro il corpus {corpus}.\n\
         D12: il corpus è una cartella di file versionata e l'uscita è `rm -rf`; \
         un SQLite con i suoi file `-wal` e `-shm` dentro è rumore nel diff e \
         un registro che sparisce con i file che dovrebbe descrivere.\n\
         Mettilo fuori, per esempio con --db ./kb-s.sqlite3."
    )]
    DatabaseNelCorpus {
        database: PathBuf,
        corpus: PathBuf,
    },

    /// Il database è all'epoch di un binario più nuovo di questo.
    #[error(
        "il database {percorso} è all'epoch {epoch} e questo binario conosce \
         l'epoch {binario}: è stato scritto da una versione più nuova di {NOME} {VERSIONE}.\n\
         Avvia il binario che ha scritto il database, o un binario più recente. \
         Rifiutare è metà del lavoro della guardia: un binario vecchio che \
         scrive sopra uno schema nuovo non fa un rollback, fa una corruzione \
         silenziosa."
    )]
    EpochFutura {
        percorso: PathBuf,
        epoch: i32,
        binario: i32,
    },

    /// Qualunque altro errore di `kbs-store` in apertura.
    #[error("il database {percorso} non si apre: {causa}")]
    Apertura { percorso: PathBuf, causa: StoreError },

    /// La porta non si può prendere.
    #[error("l'indirizzo {indirizzo} non si può prendere: {causa}")]
    Ascolto {
        indirizzo: SocketAddr,
        causa: std::io::Error,
    },
}

/// Il codice di uscita del processo.
///
/// I numeri sono quelli di `kbs-intake`, per il motivo detto in cima; la doc di
/// [`Errore`] spiega perché il 2 non ha un rimedio unico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Esito {
    /// Partito e chiuso con un segnale.
    Ok,
    /// Rifiutato da una regola: vedi [`Errore`].
    Rifiutata,
    /// Gli argomenti non sono un comando.
    Uso,
    /// Il sistema non ha potuto rispondere.
    Sistema,
}

impl Esito {
    /// Il numero che il processo restituisce.
    pub fn codice(self) -> u8 {
        match self {
            Esito::Ok => 0,
            Esito::Rifiutata => 2,
            Esito::Uso => 3,
            Esito::Sistema => 4,
        }
    }

    /// Il codice con cui esce un errore.
    ///
    /// La distinzione che conta è una sola: l'epoch futura è una **regola** e
    /// non un guasto, e un supervisore che la vedesse come 4 proverrebbe a
    /// riavviare in ciclo, cioè esattamente il danno che la guardia evita.
    pub fn da_errore(e: &Errore) -> Esito {
        match e {
            Errore::Uso(_) => Esito::Uso,
            Errore::CorpusAssente { .. } | Errore::DatabaseNelCorpus { .. } => Esito::Rifiutata,
            Errore::EpochFutura { .. } => Esito::Rifiutata,
            Errore::Apertura { .. } | Errore::Ascolto { .. } => Esito::Sistema,
        }
    }
}

impl From<Esito> for std::process::ExitCode {
    fn from(e: Esito) -> Self {
        std::process::ExitCode::from(e.codice())
    }
}

/// Che cosa il daemon è stato avviato a fare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opzioni {
    /// La radice del corpus: sotto questa cartella stanno gli artifact.
    pub corpus: PathBuf,
    /// Il file SQLite.
    pub database: PathBuf,
    /// Dove mettersi in ascolto.
    pub ascolta: SocketAddr,
}

impl Opzioni {
    /// Le opzioni con i default, sopra un corpus.
    ///
    /// Serve al test; il corpus come obbligo lo impone [`analizza`], che è
    /// l'unico posto in cui si legge la riga di comando.
    pub fn con_corpus(corpus: impl Into<PathBuf>) -> Self {
        Opzioni {
            corpus: corpus.into(),
            database: PathBuf::from(DATABASE_PREDEFINITO),
            ascolta: INDIRIZZO_PREDEFINITO
                .parse()
                .expect("l'indirizzo di default è un SocketAddr: è una costante"),
        }
    }
}

/// Il testo di `--help`.
///
/// Una funzione e non una costante, perché la stampa volge [`NOME`] e
/// [`VERSIONE`], e una costante che li contiene li ripeterebbe: due nomi del
/// binario in due posti sono due nomi che divergono al primo rename.
pub fn uso() -> String {
    format!(
        "\
{NOME} {VERSIONE} — il server di kb-s (D2, D5, D12, D15)

USO:
    {NOME} --corpus <cartella> [--db <file>] [--listen <indirizzo>]

OPZIONI:
    --corpus <cartella>   la radice del corpus: sotto questa cartella stanno
                          gli artifact. Obbligatoria, e deve esistere già: il
                          daemon non crea un corpus vuoto, perché su un corpus
                          vuoto ogni artifact risponde «non c'è» e il motivo
                          non si vede.
    --db <file>           il database. Default: ./{DATABASE_PREDEFINITO} nella
                          directory di lavoro — fuori dal corpus per D12.
    --listen <indirizzo>  dove mettersi in ascolto. Default: {INDIRIZZO_PREDEFINITO}
                          (loopback). Un indirizzo non-loopback espone a chiunque
                          raggiunga la porta: l'identità qui è una dichiarazione.
    -h, --help            questo testo.
    -V, --version         la versione.

CODICI DI USCITA:
    0  partito e chiuso con un segnale
    2  rifiutato da una regola (corpus assente, database dentro il corpus,
       database all'epoch di un binario più nuovo)
    3  gli argomenti non sono un comando
    4  il sistema non ha potuto rispondere (apertura, bind)
"
    )
}

/// Che cosa gli argomenti chiedono al daemon.
///
/// `Testo` è `--help` e `--version`: sono richieste che il daemon onora
/// **senza** aprire niente, e aprire il database per `--version` sarebbe un
/// modo elegante per non poter chiedere la versione di un'istanza rotta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Richiesta {
    /// Stampare e uscire.
    Testo(String),
    /// Avviare.
    Avvio(Opzioni),
}

/// Il valore che segue una bandierina.
///
/// Una funzione e non una closure dentro il ciclo: la closure dovrebbe
/// prendere in prestito il contatore per avanzarlo, e il ciclo lo avanza anche
/// alla fine dell'iterazione, quindi due prestiti sullo stesso `i` non possono
/// sovrapporsi. Un `&mut i` esplicito dice la stessa cosa senza dover accorciare
/// a mano la durata del prestito.
fn prossimo(args: &[String], i: &mut usize, nome: &str) -> Result<String, Errore> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| Errore::Uso(format!("{nome} vuole un valore\n\n{}", uso())))
}

/// Legge gli argomenti.
///
/// Le bandierine sono in inglese come in `kbs` (`--db`, `--person`, `--course`)
/// e la prosa è in italiano come nel resto della CLI: la bandierina è un nome
/// che un altro programma può copiare, il testo è per chi legge.
///
/// Il ciclo è scritto a mano e non con un parser perché i casi sono tre e la
/// regola che conta è una sola: `--flag` senza valore non è un errore
/// ignorabile, è un errore che nomina la bandierina; e `--flag` ripetuta non è
/// «l'ultima vince», è un comando ambiguo. Accettare l'ultima è quello che fa
/// `getopt`, ed è quello che un umano non si aspetta quando lo script è
/// sbagliato: silenziosamente si serve il corpus sbagliato.
pub fn analizza(args: &[String]) -> Result<Richiesta, Errore> {
    let mut corpus: Option<PathBuf> = None;
    let mut database: Option<PathBuf> = None;
    let mut ascolta: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].clone();
        match arg.as_str() {
            "-h" | "--help" => return Ok(Richiesta::Testo(uso())),
            "-V" | "--version" => return Ok(Richiesta::Testo(format!("{NOME} {VERSIONE}\n"))),
            "--corpus" => {
                let v = prossimo(args, &mut i, "--corpus")?;
                if corpus.replace(PathBuf::from(v)).is_some() {
                    return Err(Errore::Uso(format!("--corpus ripetuta\n\n{}", uso())));
                }
            }
            "--db" => {
                let v = prossimo(args, &mut i, "--db")?;
                if database.replace(PathBuf::from(v)).is_some() {
                    return Err(Errore::Uso(format!("--db ripetuta\n\n{}", uso())));
                }
            }
            "--listen" => {
                let v = prossimo(args, &mut i, "--listen")?;
                if v.parse::<SocketAddr>().is_err() {
                    return Err(Errore::Uso(format!(
                        "--listen vuole un indirizzo `ip:porta`, non `{v}`\n\n{}",
                        uso()
                    )));
                }
                if ascolta.replace(v).is_some() {
                    return Err(Errore::Uso(format!("--listen ripetuta\n\n{}", uso())));
                }
            }
            altro => {
                return Err(Errore::Uso(format!(
                    "argomento sconosciuto `{altro}`\n\n{}",
                    uso()
                )));
            }
        }
        i += 1;
    }

    let corpus = corpus.ok_or_else(|| Errore::Uso(format!("manca --corpus\n\n{}", uso())))?;
    let ascolta = match ascolta {
        Some(a) => a
            .parse()
            .expect("`analizza` ha già validato l'indirizzo con lo stesso tipo"),
        None => INDIRIZZO_PREDEFINITO
            .parse()
            .expect("l'indirizzo di default è una costante valida"),
    };

    Ok(Richiesta::Avvio(Opzioni {
        corpus,
        database: database.unwrap_or_else(|| PathBuf::from(DATABASE_PREDEFINITO)),
        ascolta,
    }))
}

/// Un ascolto pronto: la porta presa e l'applicazione montata.
///
/// Due campi e non una tupla, perché i due hanno vite diverse: la porta la
/// legge subito chi ha avviato, l'applicazione la serve il test. E il fatto che
/// [`avvia`] **restituisca** la porta già assegnata è la ragione per cui il
/// test end-to-end può girare su `:0` e non litigare con nessun altro processo
/// della macchina.
#[derive(Debug)]
pub struct InAscolto {
    /// La porta presa, con la porta vera se `--listen` era `:0`.
    pub indirizzo: SocketAddr,
    /// Il socket.
    pub ascoltatore: TcpListener,
    /// Il router, montato come in produzione.
    pub applicazione: axum::Router,
}

/// Apre il database, monta le rotte e prende la porta.
///
/// Fallisce **prima** di prendere la porta se qualcosa non torna: un processo
/// che ha già aperto il socket e poi si rifiuta di partire lascia un
/// `TIME_WAIT` e un messaggio d'errore su un indirizzo che l'operatore aveva
/// visto comparire. L'ordine è apri → monta → prendi, e ognuno dei tre è
/// rifiutato in un modo che nomina la causa.
pub async fn avvia(opzioni: &Opzioni) -> Result<InAscolto, Errore> {
    let corpus = controlla_corpus(&opzioni.corpus)?;
    controlla_db(&corpus, &opzioni.database)?;
    let db = apri(&opzioni.database)?;

    let config = ServerConfig::new(&corpus);
    if !config.web_dir.is_dir() {
        // Non è un errore: il daemon può servire l'API e gli artifact anche
        // senza interfaccia, e la rotta del fallback dice già
        // «interfaccia non installata» invece di una pagina vuota. Avvisare qui
        // serve a non confondere «non c'è una UI» con «non c'è niente».
        eprintln!(
            "avviso: l'interfaccia non è installata in {}: le rotte dell'API \
             rispondono, `/` risponderà «interfaccia non installata».",
            config.web_dir.display()
        );
    }

    let applicazione = app(db, config);
    let ascoltatore =
        TcpListener::bind(opzioni.ascolta)
            .await
            .map_err(|causa| Errore::Ascolto {
                indirizzo: opzioni.ascolta,
                causa,
            })?;
    let indirizzo = ascoltatore
        .local_addr()
        .map_err(|causa| Errore::Ascolto {
            indirizzo: opzioni.ascolta,
            causa,
        })?;

    Ok(InAscolto {
        indirizzo,
        ascoltatore,
        applicazione,
    })
}

/// Apre il database, e distingue l'epoch futura dal resto.
///
/// `Db::open` propaga già `SchemaFromTheFuture`: qui la si traduce in un
/// errore che nomina i due epoch e il rimedio, e non in un `Apertura` che la
/// confonderebbe con «il file non c'è» e uscirebbe con il codice 4.
fn apri(percorso: &Path) -> Result<Db, Errore> {
    Db::open(percorso).map_err(|causa| match causa {
        StoreError::SchemaFromTheFuture { db, binary } => Errore::EpochFutura {
            percorso: percorso.to_path_buf(),
            epoch: db,
            binario: binary,
        },
        other => Errore::Apertura {
            percorso: percorso.to_path_buf(),
            causa: other,
        },
    })
}

/// Rifiuta un corpus che non è una cartella, e lo rende canonico.
///
/// Si canonizza qui e si passa avanti il risultato perché il confronto di
/// [`controlla_db`] e quello di `sandbox` vanno fatti sulla radice **risolta**:
/// con un symlink, `starts_with` accetterebbe un percorso che punta fuori dalla
/// radice. Vedi [`crate::config::ServerConfig::canonical_corpus_root`].
fn controlla_corpus(corpus: &Path) -> Result<PathBuf, Errore> {
    if !corpus.is_dir() {
        return Err(Errore::CorpusAssente {
            percorso: corpus.to_path_buf(),
        });
    }
    corpus
        .canonicalize()
        .map_err(|_| Errore::CorpusAssente {
            percorso: corpus.to_path_buf(),
        })
}

/// Rifiuta un database che sta dentro il corpus.
///
/// Solo in questa direzione, e la scelta merita una riga: `--corpus
/// /srv/kb/corpus --db /srv/kb/kb.sqlite3` è il layout normale di un'installazione
/// — il registro e il corpus sono fratelli sotto una directory che è
/// dell'installazione, non del corpus — e rifiutarlo sarebbe un falso
/// positivo su un caso che capita ogni giorno. Nell'altra direzione il
/// pericolo di D12 non c'è: la directory che contiene il database non è il
/// corpus, e `rm -rf` del corpus non la tocca.
///
/// Il confronto è fatto **sulla cartella che contiene il database**, non sul
/// file: il file potrebbe non esistere ancora — è SQLite a crearlo — e
/// `canonicalize` su un file inesistente fallirebbe, e il rifiuto che ne
/// uscirebbe sarebbe «path inesistente», che è la risposta sbagliata alla
/// domanda giusta.
fn controlla_db(corpus: &Path, database: &Path) -> Result<(), Errore> {
    let contenitore = match database.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        // Un percorso senza directory sta nella directory di lavoro e non c'è
        // modo di saperlo qui senza leggere il processo: si lascia passare, e
        // `--help` dice che il default è relativo alla directory corrente.
        _ => return Ok(()),
    };
    let risolto = contenitore
        .canonicalize()
        .unwrap_or_else(|_| contenitore.to_path_buf());
    if risolto.starts_with(corpus) {
        return Err(Errore::DatabaseNelCorpus {
            database: database.to_path_buf(),
            corpus: corpus.to_path_buf(),
        });
    }
    Ok(())
}

/// Il futuro che finisce quando arriva un segnale di chiusura.
///
/// `SIGINT` e `SIGTERM`: il primo è `Ctrl-C` e l'utente lo vede, il secondo è
/// il supervisor. Sono gli unici due che un operatore manda, e ignorarli
/// lascerebbe il database con una transazione a metà, che è la ragione per cui
/// l'append-only di D11 esiste.
///
/// Un handler che non si può installare **non** è un handler che non ascolta:
/// è un handler che uccide il processo al primo `SIGTERM`, cioè esattamente il
/// caso che questo modulo promette di evitare. Quindi l'installazione fallita
/// si dice e si continua ad aspettare: il processo resta vivo e l'operatore
/// vede perché non può chiuderlo pulitamente.
///
/// # Cosa «in grazia» vuol dire, e cosa non vuol dire
///
/// Vuol dire che le richieste **già ricevute per intero** vengono servite
/// prima che il processo esca: provato in `tests/daemon.rs` con due richieste
/// in pipelining e un segnale in mezzo, e la seconda risposta arriva.
///
/// Non vuol dire che una connessione con **metà intestazioni** venga
/// aspettata. Non viene: hyper chiude la connessione e il client legge un
/// `EOF` senza risposta. La ragione è che metà intestazioni non sono una
/// richiesta — sono dei byte — e aspettarli significherebbe aspettare che il
/// client finisca di parlare, cosa che un server non può né deve fare. Un
/// operatore che vede la connessione chiudersi durante un upload lungo deve
/// sapere che è questo, e non un crash.
pub async fn segnali() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interruzione = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{NOME}: SIGINT non installabile ({e}): chiusura non pulita.");
                std::future::pending::<()>().await;
                return;
            }
        };
        let mut terminazione = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{NOME}: SIGTERM non installabile ({e}): chiusura non pulita.");
                std::future::pending::<()>().await;
                return;
            }
        };
        tokio::select! {
            _ = interruzione.recv() => {}
            _ = terminazione.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Le righe che l'operatore legge all'avvio.
///
/// Vanno su `stdout` e non in un file di log per una ragione sola: il momento
/// in cui servono è il primo, e il primo schermo è l'unico che l'operatore
/// guarda. Le funi del corpus e del database arrivano come argomenti e non da
/// una `println!` nascosta dentro [`avvia`], perché sono anche ciò che il test
/// verifica.
pub fn annuncio(indirizzo: SocketAddr, corpus: &Path, database: &Path, epoch: i32) -> String {
    let dove = if indirizzo.ip().is_loopback() {
        format!("http://{indirizzo}")
    } else {
        format!("http://{indirizzo}   *** NON è loopback ***")
    };
    format!(
        "\
{NOME} {VERSIONE}
  ascolto     {dove}
  corpus      {}
  database    {} (epoch {epoch}; questo binario ne conosce {})
  identita'   una DICHIARAZIONE, non un'autenticazione: chiunque raggiunga
              questa porta puo' dichiarare chi e' con X-Kbs-Person o ?person=.
              Il predicato D5 protegge il materiale, non la persona.
  chiusura    Ctrl-C (SIGINT) o SIGTERM: le richieste gia' ricevute finiscono,
              e il listener chiude subito. Una richiesta ancora in arrivo —
              meta' intestazioni, nessuna riga vuota — viene tagliata: non ha
              ancora una richiesta da finire, solo dei byte.
",
        corpus.display(),
        database.display(),
        binary_epoch(),
    )
}

/// Avvia e resta in ascolto fino a un segnale.
///
/// Restituisce il codice di uscita invece di chiamare `exit`: `exit` in una
/// funzione che un test chiama uccide il test, e una firma che nessun test può
/// chiamare è una firma che nessun test chiamerà.
pub async fn esegui(opzioni: Opzioni) -> Esito {
    let in_ascolto = match avvia(&opzioni).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{NOME}: {e}");
            return Esito::da_errore(&e);
        }
    };

    print!("{}", annuncio(
        in_ascolto.indirizzo,
        &opzioni.corpus,
        &opzioni.database,
        binary_epoch(),
    ));

    let (spegni, aspetta) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        segnali().await;
        let _ = spegni.send(());
    });

    let esito = axum::serve(in_ascolto.ascoltatore, in_ascolto.applicazione)
        .with_graceful_shutdown(async move {
            let _ = aspetta.await;
        })
        .await;

    match esito {
        Ok(()) => {
            eprintln!("{NOME}: chiuso.");
            Esito::Ok
        }
        Err(e) => {
            eprintln!("{NOME}: il server si è fermato con un errore: {e}");
            Esito::Sistema
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linea(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn opzioni(args: &str) -> Result<Opzioni, Errore> {
        match analizza(&linea(args))? {
            Richiesta::Avvio(o) => Ok(o),
            Richiesta::Testo(t) => Err(Errore::Uso(t)),
        }
    }

    // ── gli argomenti ───────────────────────────────────────────────────────

    #[test]
    fn il_corpus_e_obbligatorio_e_il_resto_ha_un_default() {
        let e = analizza(&linea("")).unwrap_err();
        assert!(matches!(e, Errore::Uso(_)));
        assert!(e.to_string().contains("--corpus"));

        let o = opzioni("--corpus corpus/").unwrap();
        assert_eq!(o.corpus, PathBuf::from("corpus/"));
        assert_eq!(o.database, PathBuf::from("kb-s.sqlite3"));
        assert_eq!(o.ascolta.to_string(), INDIRIZZO_PREDEFINITO);
        // Il default del database è relativo: sta nella directory di lavoro e
        // non dentro il corpus. È la forma che il test può osservare, e che un
        // giorno qualcuno potrebbe cambiare in `<corpus>/…` senza accorgersene.
        assert!(!o.database.is_absolute());
        assert!(o.ascolta.ip().is_loopback());
    }

    #[test]
    fn una_bandierina_senza_valore_e_un_errore_che_la_nomina() {
        for flag in ["--corpus", "--db", "--listen"] {
            let e = analizza(&linea(flag)).unwrap_err();
            assert!(matches!(e, Errore::Uso(_)), "{flag} dovrebbe essere errore d'uso");
            assert!(e.to_string().contains(flag), "il messaggio deve nominare {flag}");
        }
    }

    #[test]
    fn una_bandierina_ripetuta_e_ambigua_e_non_unultima_vincente() {
        // `--corpus a --corpus b` è un comando ambiguo. Prendere l'ultimo è
        // quello che fa `getopt`, ed è quello che un umano non si aspetta
        // quando lo script è sbagliato: si serve il corpus sbagliato in
        // silenzio.
        for (args, flag) in [
            ("--corpus a --corpus b", "--corpus"),
            ("--corpus c --db a --db b", "--db"),
            ("--corpus c --listen 127.0.0.1:1 --listen 127.0.0.1:2", "--listen"),
        ] {
            let e = opzioni(args).unwrap_err();
            assert!(matches!(e, Errore::Uso(_)), "{flag} ripetuta: {e}");
            assert!(e.to_string().contains("ripetut"), "{flag}: {e}");
        }
    }

    #[test]
    fn un_indirizzo_che_non_e_un_indirizzo_non_diventa_il_default() {
        // Il caso da coprire è il silenzio: `--listen casino` sostituito dal
        // default produrrebbe un server in ascolto dove l'operatore non aveva
        // chiesto, e l'unico indizio sarebbe che l'interfaccia non si vede.
        let e = opzioni("--corpus c --listen casino").unwrap_err();
        assert!(e.to_string().contains("--listen"));
        assert!(opzioni("--corpus c --listen 0.0.0.0:9000").is_ok());
    }

    #[test]
    fn help_e_version_non_aprono_nessuna_istanza() {
        // `Richiesta::Testo` e non `Opzioni`: aprire il database per
        // `--version` sarebbe un modo elegante per non poter chiedere la
        // versione dell'istanza rotta che si vuole proprio diagnosticare.
        for flag in ["-h", "--help"] {
            let Richiesta::Testo(t) = analizza(&linea(flag)).unwrap() else {
                panic!("{flag} deve essere una richiesta di testo");
            };
            assert!(t.contains("--corpus"));
            assert!(t.to_lowercase().contains("dichiarazione"));
        }
        let Richiesta::Testo(t) = analizza(&linea("--version")).unwrap() else {
            panic!("--version deve essere una richiesta di testo");
        };
        assert!(t.starts_with(NOME), "il nome stampato è quello del binario: {t}");
    }

    // ── le regole di avvio ──────────────────────────────────────────────────

    #[test]
    fn un_corpus_che_non_esiste_non_viene_creato() {
        let tmp = tempfile::tempdir().unwrap();
        let assente = tmp.path().join("non-esiste");
        let e = controlla_corpus(&assente).unwrap_err();
        assert!(matches!(e, Errore::CorpusAssente { .. }));
        // Il punto del rifiuto: dopo il rifiuto la directory **non c'è**, e
        // quindi nessun artifact risponderà «non c'è» per un motivo invisibile.
        assert!(!assente.exists());
        assert!(e.to_string().contains("non la crea"));
    }

    #[test]
    fn un_file_che_non_e_una_cartella_non_e_un_corpus() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("corpus.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(
            controlla_corpus(&file).unwrap_err(),
            Errore::CorpusAssente { .. }
        ));
    }

    #[test]
    fn un_database_dentro_il_corpus_e_rifiutato_e_uno_vicino_no() {
        let tmp = tempfile::tempdir().unwrap();
        let corpus = tmp.path().join("corpus");
        std::fs::create_dir(&corpus).unwrap();
        let canon = corpus.canonicalize().unwrap();

        // Dentro: il caso ovvio. Il file non esiste ancora — è SQLite a
        // crearlo — e il rifiuto è comunque quello giusto.
        let dentro = canon.join("kb.sqlite3");
        assert!(matches!(
            controlla_db(&canon, &dentro).unwrap_err(),
            Errore::DatabaseNelCorpus { .. }
        ));
        assert!(!dentro.exists());

        // Fuori ma accanto: `kb-s.sqlite3` sta nella directory di lavoro, che è
        // il padre di `corpus`. È il default, e `--corpus /srv/kb/corpus
        // --db /srv/kb/kb.sqlite3` è il layout di un'installazione vera: un
        // rifiuto qui sarebbe un falso positivo su un caso che capita ogni
        // giorno, e l'unico modo di accorgersene sarebbe un'ora di debug.
        assert!(controlla_db(&canon, &canon.parent().unwrap().join("kb-s.sqlite3")).is_ok());
        // Una directory il cui nome **inizia** come il corpus non è il corpus.
        assert!(controlla_db(&canon, &canon.parent().unwrap().join("corpus-vecchio/kb.sqlite3")).is_ok());
    }

    #[test]
    fn il_corpus_dentro_la_cartella_del_database_e_ammesso() {
        // La direzione opposta non è un pericolo di D12 e rifiutarla sarebbe
        // un falso positivo: la directory che contiene il database non è il
        // corpus, e `rm -rf` del corpus non la tocca. Il test esiste perché
        // una regola che si allarga «per sicurezza» a un caso normale è il
        // modo più comune in cui una difesa diventa un ostacolo.
        let tmp = tempfile::tempdir().unwrap();
        let radice = tmp.path().join("holder");
        let corpus = radice.join("corpus");
        std::fs::create_dir_all(&corpus).unwrap();
        let canon = corpus.canonicalize().unwrap();
        assert!(controlla_db(&canon, &radice.join("kb.sqlite3")).is_ok());
    }

    #[test]
    fn un_symlink_verso_il_corpus_non_trae_passo_al_database() {
        // Il confronto è fatto dopo la canonizzazione: senza,
        // `--db ./fuori/kb.sqlite3` con `fuori -> corpus` passerebbe, e il
        // database finirebbe dentro il corpus senza che nessuno lo abbia
        // scritto due volte.
        let tmp = tempfile::tempdir().unwrap();
        let corpus = tmp.path().join("corpus");
        std::fs::create_dir(&corpus).unwrap();
        let fuori = tmp.path().join("fuori");
        std::os::unix::fs::symlink(&corpus, &fuori).unwrap();
        let canon = corpus.canonicalize().unwrap();
        assert!(controlla_db(&canon, &fuori.join("kb.sqlite3")).is_err());
    }

    #[tokio::test]
    async fn un_database_all_epoch_avvenire_non_si_apre_e_non_si_serve() {
        // La prova che la guardia parla **attraverso** il daemon: il database
        // viene falsificato con un epoch che nessun binario conosce, e il
        // daemon deve rifiutarsi di partire prima di prendere la porta.
        // Il corpus sta in una directory **sorella** del database, non nella
        // stessa: è il layout di `--db` fuori dal corpus, ed è così che
        // l'errore che si sta provando è l'epoch e non il percorso.
        let tmp = tempfile::tempdir().unwrap();
        let radice = tmp.path().join("casa");
        std::fs::create_dir(&radice).unwrap();
        let percorso = radice.join("futuro.sqlite3");
        {
            let store = kbs_store::Store::open(&percorso).unwrap();
            store
                .conn()
                .execute("UPDATE schema_epoch SET epoch = 999 WHERE id = 1", [])
                .unwrap();
        }

        let corpus = radice.join("corpus");
        std::fs::create_dir(&corpus).unwrap();
        let opzioni = Opzioni {
            corpus,
            database: percorso.clone(),
            ascolta: "127.0.0.1:0".parse().unwrap(),
        };
        let e = avvia(&opzioni).await.unwrap_err();
        let Errore::EpochFutura { epoch, binario, .. } = &e else {
            panic!("atteso EpochFutura, ottenuto {e}");
        };
        assert_eq!(*epoch, 999);
        assert_eq!(*binario, binary_epoch());
        // Il codice è 2, non 4: un supervisore che legge 4 riavvia, e
        // riavviare un binario vecchio sopra uno schema nuovo è il danno.
        assert_eq!(Esito::da_errore(&e), Esito::Rifiutata);
        // E il messaggio dice che cosa fare, non solo che cosa è successo.
        assert!(e.to_string().contains("più recente"));
    }

    #[tokio::test]
    async fn un_corpus_assente_non_raggiunge_il_database() {
        // L'ordine conta: se il database fosse aperto per primo, un corpus
        // sbagliato lascerebbe comunque un file SQLite creato sul disco, e il
        // prossimo avvio con il percorso giusto troverebbe un database che non
        // ha chiesto nessuno.
        let tmp = tempfile::tempdir().unwrap();
        let database = tmp.path().join("mai-dovuto-esistere.sqlite3");
        let opzioni = Opzioni {
            corpus: tmp.path().join("non-esiste"),
            database: database.clone(),
            ascolta: "127.0.0.1:0".parse().unwrap(),
        };
        assert!(matches!(
            avvia(&opzioni).await.unwrap_err(),
            Errore::CorpusAssente { .. }
        ));
        assert!(!database.exists());
    }

    // ── l'annuncio ──────────────────────────────────────────────────────────

    #[test]
    fn lannuncio_dice_la_url_e_che_lidentita_e_dichiarata() {
        let a = annuncio(
            "127.0.0.1:8787".parse().unwrap(),
            Path::new("/srv/corpus"),
            Path::new("/srv/kb-s.sqlite3"),
            5,
        );
        assert!(a.contains("http://127.0.0.1:8787"));
        assert!(a.contains("/srv/corpus"));
        assert!(a.contains("epoch 5"));
        // Le due cose che un operatore non può indovinare.
        assert!(a.contains("DICHIARAZIONE, non un'autenticazione"));
        assert!(a.contains("Ctrl-C"));
        assert!(a.contains("D5"));

        // Un bind non-loopback va detto ad alta voce: è la differenza fra una
        // porta che solo l'operatore vede e una porta che chiunque raggiunge.
        let b = annuncio(
            "0.0.0.0:8787".parse().unwrap(),
            Path::new("/srv/corpus"),
            Path::new("/srv/kb-s.sqlite3"),
            5,
        );
        assert!(b.contains("NON è loopback"), "{b}");
    }

    #[test]
    fn i_codici_di_uscita_sono_quelli_del_progetto() {
        assert_eq!(Esito::Ok.codice(), 0);
        assert_eq!(Esito::Rifiutata.codice(), 2);
        assert_eq!(Esito::Uso.codice(), 3);
        assert_eq!(Esito::Sistema.codice(), 4);
        // E la forma in cui il processo li restituisce è `From<Esito>`, che è
        // l'unico posto in cui un codice diventa un intero.
        assert_eq!(
            std::process::ExitCode::from(Esito::Rifiutata),
            std::process::ExitCode::from(2u8)
        );
    }
}
