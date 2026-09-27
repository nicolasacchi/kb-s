//! Un solo tipo di errore per tutto il crate.
//!
//! I messaggi sono in italiano e nominano **la regola** rotta, non il sintomo.
//! Chi legge questi errori è un docente o un agente che ha appena incollato
//! qualcosa e si aspetta di sapere perché non è entrato: «errore di parsing» non
//! gli dice niente, «ci sono due blocchi `artifact` e non si sa quale sia quello
//! giusto» glielo dice.
//!
//! Le varianti che non sono `Io`/`Store`/`Json` sono **le regole di questo
//! crate**, e sono tutte nomi. Nessun `unwrap()` fuori dai test: quello che non
//! si può fare è una di queste, e ognuna dice perché.

use std::io;
use std::path::PathBuf;

/// Il risultato di tutto il crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("lettura di {path}: {source}")]
    Io { path: PathBuf, source: io::Error },

    #[error("persistenza: {0}")]
    Store(#[from] kbs_store::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    // ── l'hash di corpus: che cosa è dentro e come è concatenato ────────────

    #[error("`{path}` non è un percorso relativo normalizzato: l'hash di corpus è definito sui percorsi relativi, e due forme diverse dello stesso file darebbero due hash")]
    PercorsoNonNormalizzato { path: String },

    #[error("`{path}` compare due volte nell'hash di corpus: due file con lo stesso percorso relativo sono due file diversi con un solo nome")]
    PercorsoDuplicato { path: String },

    // ── il paste: la strada che non indovina (D10.1) ───────────────────────

    #[error("il paste è vuoto: non c'è nulla da catturare")]
    PasteVuoto,

    #[error("il paste non contiene un documento HTML: nessun `<html`, `<title>` o `<body>` in {righe} righe. Una cattura che non è un documento viene rifiutata, non interpretata")]
    PasteNonDocumento { righe: usize },

    #[error("il blocco è marcato `{linguaggio}` e non `artifact`: questa strada cattura un artifact, e un blocco di altro tipo è un altro documento")]
    FencingNonSupportato { linguaggio: String },

    #[error("ci sono {blocchi} blocchi delimitati: con più di uno non si sa quale sia l'artifact, e scegliere sarebbe indovinare")]
    FencingAmbiguo { blocchi: usize },

    #[error("riga {numero} fuori dall'involucro: `{testo}`. Le chiavi riconosciute sono {chiavi}")]
    RigaNonRiconosciuta { numero: usize, testo: String, chiavi: &'static str },

    #[error("`{chiave}` ripetuta: una chiave una volta sola, perché due valori per la stessa chiave sono una domanda a cui questo rifiuto non sa rispondere")]
    ChiaveRipetuta { chiave: String },

    // ── la porta (D4) ───────────────────────────────────────────────────────

    #[error("l'artifact dichiara lo stato `{dichiarato}` e questa strada non scrive mai uno stato di pubblicazione: entra in `bozza` e la promozione passa da `kbs_intake::gate`")]
    StatoDichiaratoNonScrivibile { dichiarato: String },

    #[error("l'artifact dichiara l'origine `generated` e non porta un model lock: D10 dice che ogni generazione registra modello, hash del prompt e hash del corpus, e una dichiarazione senza lock è un diario")]
    GenerazioneSenzaLock { rel_path: String },

    #[error("nessun titolo: il titolo è il campo BM25 primario e `kbs_store::upsert_argument` non scrive un argomento senza. Problemi: {problemi}")]
    TitoloMancante { problemi: String },

    #[error("il contenuto è cambiato fra la validazione e la promozione: il verdetto vale per l'hash {del_verdetto} e il contenuto adesso è {corrente}")]
    ContenutoCambiato { del_verdetto: String, corrente: String },

    #[error("il verdetto ha un problema bloccante e la promozione è chiusa: {codice} — {messaggio}")]
    VerdettoBloccante { codice: String, messaggio: String },

    // ── il lock (D10) ───────────────────────────────────────────────────────

    #[error("`{campo}` è vuoto: un lock con un campo vuoto non identifica una generazione")]
    LockIncompleto { campo: &'static str },

    #[error("la diagnosi `{id}` non è nel registro: una generazione che dice di rispondere a una diagnosi inesistente è un riferimento rotto")]
    DiagnosiAssente { id: String },

    // ── l'interfaccia ───────────────────────────────────────────────────────

    #[error("uso: {0}")]
    Uso(String),

    #[error("comando sconosciuto `{nome}`: i verbi sono {noti}")]
    ComandoSconosciuto { nome: String, noti: &'static str },

    #[error("manca l'opzione richiesta `{nome}`")]
    OpzioneMancante { nome: String },

    #[error("l'opzione `{nome}` è stata ripetuta con due valori: un'opzione ripetuta è una richiesta ambigua")]
    OpzioneRipetuta { nome: String },

    #[error("`{protocollo}` non è un protocollo che questo binario parla: la versione è `{atteso}`")]
    ProtocolloSconosciuto { protocollo: String, atteso: &'static str },

    #[error("richiesta JSON-RPC malformata: {causa}")]
    RpcMalformata { causa: String },

    #[error("metodo MCP `{metodo}` non implementato: le operazioni sono {noti}")]
    MetodoSconosciuto { metodo: String, noti: &'static str },

    #[error("argomenti insufficienti per `{metodo}`: manca `{campo}`")]
    ArgomentoMancante { metodo: &'static str, campo: &'static str },

    #[error("l'argomento `{id}` non esiste, o non lo vedi: un evento di generazione ha bisogno del materiale che ha generato, e una riga senza argomento è una riga orfana")]
    ArgomentoAssente { id: String },

    #[error("la persona `{id}` ({ruolo}) non è nel registro: le persone si iscrivono, e `upsert_person` sovrascriverebbe il nome di chi c'è già con un id — quindi questa strada non le crea da sola")]
    PersonaAssente { id: String, ruolo: &'static str },

    #[error("il percorso `{path}` non è UTF-8: l'hash di corpus è definito su percorsi UTF-8, e di questo non si può fare un hash che sia anche riproducibile")]
    PercorsoNonUtf8 { path: String },

    #[error("nessun corso: un argomento senza perimetro di condivisione non ha a chi essere condiviso (D5). Dichiararlo con `<meta name=\"kb-course\">` o passarlo con la richiesta")]
    CorsoMancante,

}

impl Error {
    /// Il codice stabile dell'errore, in kebab-case.
    ///
    /// È la stessa forma dei codici del referto di validazione, e serve perché
    /// la CLI, il protocollo JSON-RPC e i test parlino **la stessa lingua**:
    /// un agente che legge `unpaste-non-documento` sa che cosa è successo senza
    /// fare parsing di un messaggio in italiano.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } => "io",
            Error::Store(_) => "store",
            Error::Json(_) => "json",
            Error::PercorsoNonNormalizzato { .. } => "percorso-non-normalizzato",
            Error::PercorsoDuplicato { .. } => "percorso-duplicato",
            Error::PasteVuoto => "paste-vuoto",
            Error::PasteNonDocumento { .. } => "paste-non-documento",
            Error::FencingNonSupportato { .. } => "fencing-non-supportato",
            Error::FencingAmbiguo { .. } => "fencing-ambiguo",
            Error::RigaNonRiconosciuta { .. } => "riga-non-riconosciuta",
            Error::ChiaveRipetuta { .. } => "chiave-ripetuta",
            Error::StatoDichiaratoNonScrivibile { .. } => "stato-dichiarato-non-scrivibile",
            Error::GenerazioneSenzaLock { .. } => "generazione-senza-lock",
            Error::TitoloMancante { .. } => "titolo-mancante",
            Error::ContenutoCambiato { .. } => "contenuto-cambiato",
            Error::VerdettoBloccante { .. } => "verdetto-bloccante",
            Error::LockIncompleto { .. } => "lock-incompleto",
            Error::DiagnosiAssente { .. } => "diagnosi-assente",
            Error::Uso(_) => "uso",
            Error::ComandoSconosciuto { .. } => "comando-sconosciuto",
            Error::OpzioneMancante { .. } => "opzione-mancante",
            Error::OpzioneRipetuta { .. } => "opzione-ripetuta",
            Error::ProtocolloSconosciuto { .. } => "protocollo-sconosciuto",
            Error::RpcMalformata { .. } => "rpc-malformata",
            Error::MetodoSconosciuto { .. } => "metodo-sconosciuto",
            Error::ArgomentoMancante { .. } => "argomento-mancante",
            Error::PercorsoNonUtf8 { .. } => "percorso-non-utf8",
            Error::ArgomentoAssente { .. } => "argomento-assente",
            Error::PersonaAssente { .. } => "persona-assente",
            Error::CorsoMancante => "corso-mancante",
        }
    }
}
