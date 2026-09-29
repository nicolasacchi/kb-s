//! La provenienza: **un** posto in cui si dice come si disegna, e tutti i `kind`
//! che il server sa mandare.
//!
//! # Perché questo file esiste e non una sezione di `interfaccia.rs`
//!
//! `interfaccia.rs` prova l'interfaccia **contro il router vero**: prende una
//! porta, manda una richiesta, confronta la risposta. Qui non c'è nessuna
//! richiesta: i due lati sono il sorgente di `kbs-core` e il sorgente di `web/`,
//! e il consumatore vero di entrambi è il browser — che questi test non hanno
//! (è un limite dichiarato in `interfaccia.rs`, non un caso dimenticato).
//!
//! # Che cosa è rotto, e da quando
//!
//! Le tre pagine avevano tre copie compatte della stessa cella, e due delle tre
//! avevano già smesso di somigliare: `coda.js` **non aveva il ramo `derived`**,
//! e un argomento derivato in coda usciva come la lettera `derivata (derived)` —
//! il nome dell'enum di Rust, in chiaro, senza link alla sorgente — mentre lo
//! stesso argomento nell'elenco del corso usciva come `derivata da <link>`. La
//! divergenza non era ipotetica: `Origin::Derived` esiste, `kb-origin: derived`
//! lo crea, e la coda lo contiene.
//!
//! Il ramo che mancava è anche quello più economico da perdere, perché è
//! l'unico dei tre che non ha `by`: la coda non usa `by` per decidere chi può
//! ratificare (`queue.rs::è_autore` restituisce `false` per `Derived` e lo
//! dichiara), quindi nessun'altra parte della pagina avrebbe notato la perdita.
//!
//! # Che cosa questo test fa, e che cosa non fa
//!
//! Fa due cose, entrambe sul **sorgente**, e le due sono complementari:
//!
//! * `la_provenienza_si_disegna_in_un_solo_luogo` — nessun file di `web/` fuori
//!   da `lib/provenienza.js` può nominare un `kind` di provenienza. È la guardia
//!   contro la copia incollata: la prossima pagina che ne scrive una propria
//!   smette di compilarlo qui, invece di divergere in silenzio.
//! * `ogni_origine_del_server_ha_un_ramo_nella_provenienza` — ogni `kind` che
//!   `kbs_core::Origin` sa serializzare ha un ramo esplicito, **sia** nella
//!   versione breve **sia** in quella lunga. Qui i `kind` non sono scritti a
//!   mano: escono dall'enum, quindi una variante nuova senza ramo in `web/`
//!   rompe qui.
//!
//! Non fa: non rende niente, non verifica una classe, non dice che la pagina si
//! dipinga. Un test che dicesse «la pagina funziona» senza aver caricato una
//! pagina misurerebbe una copia di una pagina, e lì il difetto si nasconderebbe
//! dietro il mock — che è la ragione per cui `radice_web()` non copia mai `web/`.

use std::path::{Path, PathBuf};

use kbs_core::{ArgumentId, Millis, ModelLock, Origin, PersonId};

/// T0: un istante qualunque, ma sempre lo stesso. Come in `interfaccia.rs`, i
/// millisecondi di un test sono un rumore e non un dato.
const T0: i64 = 1_700_000_000_000;

/// Il modulo che possiede la provenienza. Un percorso scritto qui, non derivato:
/// la guardia dice «qui e non altrove», e un percorso che si derivasse da
/// `web/` non potrebbe dire dove.
const MODULO: &str = "lib/provenienza.js";

/// La radice `web/`: gli stessi file che il binario serve.
fn radice_web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

/// Il sorgente di `kbs-core`, dove l'enum della provenienza è dichiarato.
///
/// Il percorso attraversa il workspace: è dichiarato qui perché i due lati di
/// questo test stanno in due crate diversi, e dichiararlo una volta vale più
/// che derivarlo da una glob che un giorno non combacia.
fn sorgente_di_core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../kbs-core/src/lib.rs")
}

// ─────────────────────────────────────────────────────────────────────────────
// I `kind`: dalla parte che li produce
// ─────────────────────────────────────────────────────────────────────────────

/// I nomi delle varianti di `kbs_core::Origin`, letti dal suo sorgente.
///
/// Il parser è povero e dichiarato povero, come quello che legge `ROTTE` in
/// `interfaccia.rs`: riconosce un identificatore all'inizio di una riga dentro
/// il corpo dell'enum. Se `lib.rs` cambiasse forma qui uscirebbe un elenco
/// vuoto, e non-vacuità è dichiarata in basso — che è la direzione in cui una
/// guardia di questa non deve poter fallire in silenzio.
fn varianti_di_origin() -> Vec<String> {
    let sorgente = std::fs::read_to_string(sorgente_di_core())
        .expect("il sorgente di kbs-core si legge: è il posto in cui `Origin` è dichiarato");
    let dopo = sorgente
        .split_once("pub enum Origin {")
        .expect("`kbs_core::Origin` è un enum dichiarato con `pub enum Origin`");
    let corpo = dopo
        .1
        .split_once("\n}")
        .expect("il corpo di `Origin` finisce con una graffa a colonna zero")
        .0;
    let mut fuori = Vec::new();
    for riga in corpo.lines() {
        let ripulita = riga.trim();
        // I commenti e gli attributi non sono varianti: `///` e `#[…]` si
        // distinguono perché cominciano con un carattere che un identificatore
        // non può cominciare con.
        if ripulita.is_empty() || ripulita.starts_with('/') || ripulita.starts_with('#') {
            continue;
        }
        let nome: String = ripulita
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        // Una variante di Rust comincia maiuscola, e `Human {` / `Generated {`
        // sono le due forme che qui compaiono: dentro questo corpo un
        // identificatore che non comincia maiuscolo non è una variante, ed è la
        // direzione in cui il parser deve scartare (`///`, `#[…]`, graffe).
        if !nome.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            continue;
        }
        fuori.push(kebab(&nome));
    }
    fuori
}

/// `CamelCase` → `kebab-case`, che è la regola che l'attributo
/// `#[serde(rename_all = "kebab-case")]` dichiara sopra `Origin`.
///
/// Non è una copia della regola di `serde`: è un tentativo, e il confronto con
/// [`kind_serializzati`] dice se il tentativo è giusto. Se un giorno `serde`
/// cambiasse caso, questo test romperebbe dicendo «correggi la conversione»
/// invece di continuare a guardare il posto sbagliato in silenzio.
fn kebab(nome: &str) -> String {
    let mut fuori = String::new();
    for (i, c) in nome.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            fuori.push('-');
        }
        fuori.push(c.to_ascii_lowercase());
    }
    fuori
}

/// I `kind` che `Origin` mette davvero sul filo, costruendo le varianti e
/// chiedendo a `serde` la chiave.
///
/// Questo è il passaggio che rende credibile il parser di sopra: i due elenchi
/// devono coincidere, e se coincidono è perché la regola che il parser applica
/// (`kebab-case`, come la dichiara l'attributo) è quella che `#[derive(Serialize)]`
/// applica. Se un giorno divergono, il messaggio dice quale delle due metà
/// aggiornare invece di lasciare che la guardia guardi il terzo.
fn kind_serializzati() -> Vec<String> {
    let persona = PersonId::fixture(1);
    let lock = ModelLock {
        model_id: "claude-sonnet-4-5".into(),
        prompt_hash: "aa".repeat(32),
        corpus_hash: "bb".repeat(32),
        generator_version: "kbs-fixtures-1".into(),
        at: Millis(T0),
    };
    let varianti = [
        Origin::Human {
            by: persona.clone(),
            at: Millis(T0),
        },
        Origin::Generated {
            lock,
            by: persona.clone(),
            at: Millis(T0),
        },
        Origin::Derived {
            from: ArgumentId::fixture(1),
            at: Millis(T0),
        },
    ];
    varianti
        .iter()
        .map(|v| {
            let json = serde_json::to_value(v).expect("`Origin` si serializza: lo fa la rotta");
            json["kind"]
                .as_str()
                .expect("`Origin` è un enum con tag: la chiave `kind` è una stringa")
                .to_string()
        })
        .collect()
}

/// I `kind` su cui questa guardia guarda: quelli dell'enum, una volta verificati
/// contro `serde`.
fn kind_da_tenere_docchio() -> Vec<String> {
    let dichiarati = varianti_di_origin();
    assert!(
        !dichiarati.is_empty(),
        "non ho trovato nessuna variante in `kbs_core::Origin`: il parser del sorgente non sta \
         guardando l'enum, e una guardia che non guarda niente è verde sempre"
    );
    let serializzati = kind_serializzati();
    assert_eq!(
        dichiarati, serializzati,
        "le varianti di `kbs_core::Origin` lette dal sorgente e i `kind` che `serde` produce non \
         coincidono. Se è una variante nuova, costruiscila anche in `kind_serializzati` — altrimenti \
         il confronto qui non la guarda, e una variante senza ramo in `web/` passerebbe. Se invece \
         è `serde` che ha cambiato forma, è la lista dei `kind` che è da correggere."
    );
    dichiarati
}

// ─────────────────────────────────────────────────────────────────────────────
// I file: dalla parte che li disegna
// ─────────────────────────────────────────────────────────────────────────────

/// Ogni file di `web/`, per estensione, con il nome relativo e il testo.
///
/// Il filesystem è la fonte e non un elenco: un file nuovo entra nella guardia
/// senza che nessuno lo aggiunga a una tabella.
fn file_come_escaped() -> Vec<(String, String)> {
    let radice = radice_web();
    let mut fuori = Vec::new();
    // `radice` resta clonata nello stack: sotto, il nome serve per il percorso
    // relativo di ogni voce, e `vec![radice]` la consumava.
    let mut stack = vec![radice.clone()];
    while let Some(dir) = stack.pop() {
        for voce in std::fs::read_dir(&dir).expect("una directory di `web/` si legge") {
            let voce = voce.expect("una voce della directory");
            let percorso = voce.path();
            if percorso.is_dir() {
                stack.push(percorso);
                continue;
            }
            if percorso.extension().and_then(|e| e.to_str()) != Some("js") {
                continue;
            }
            let nome = percorso
                .strip_prefix(&radice)
                .unwrap_or(&percorso)
                .display()
                .to_string();
            fuori.push((nome, std::fs::read_to_string(&percorso).expect("un .js si legge")));
        }
    }
    fuori.sort();
    fuori
}

/// I file che nominano un `kind` di provenienza.
///
/// Il predicato è una stringa, non un grafo: un file che disegna una provenienza
/// è un file che nomina almeno uno dei `kind` che il server può mandare. Non
/// cerca la funzione, e non deve: la guardia che cerca una funzione smette di
/// coprire la copia che le ha cambiato il nome, che è esattamente la copia che
/// non deve arrivare.
fn file_che_disegnano_provenienza(kind: &[String]) -> Vec<String> {
    file_come_escaped()
        .into_iter()
        .filter(|(_, testo)| kind.iter().any(|k| testo.contains(&format!("\"{k}\""))))
        .map(|(nome, _)| nome)
        .collect()
}

/// Il corpo di una funzione esportata, se il modulo la dichiara.
///
/// Le graffe si contano, e si contano anche quelle dentro una stringa: è un
/// parser povero e dichiarato povero come gli altri di questo progetto. Una
/// graffe disequilibrata in una stringa può dare un corpo diverso, e un corpo
/// diverso fa fallire la guardia di `ogni_origine_del_server_ha_un_ramo_nella_provenienza`
/// — la direzione in cui un difetto di montaggio deve restare visibile.
fn corpo_di(modulo: &str, nome: &str) -> Option<String> {
    let sorgente = std::fs::read_to_string(radice_web().join(modulo)).ok()?;
    let apertura = format!("export function {nome}(");
    let inizio = sorgente.find(&apertura)? + apertura.len();
    let dopo = &sorgente[inizio..];
    let graffa = dopo.find('{')?;
    let mut profondita = 0i32;
    for (scarto, c) in dopo[graffa..].char_indices() {
        match c {
            '{' => profondita += 1,
            '}' => {
                profondita -= 1;
                if profondita == 0 {
                    return Some(dopo[graffa..=graffa + scarto].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Le due guardie
// ─────────────────────────────────────────────────────────────────────────────

/// La prima guardia: **una** copia, in `web/lib/`.
///
/// Fallisce oggi, e deve: `coda.js`, `corso.js` e `lettore.js` avevano ciascuno
/// la propria. Fallisce anche domani, che è il punto: la prossima pagina che
/// copia la cella smette di compilarlo qui invece di divergire in silenzio.
#[test]
fn la_provenienza_si_disegna_in_un_solo_luogo() {
    let kind = kind_da_tenere_docchio();
    let disegnatori = file_che_disegnano_provenienza(&kind);
    assert_eq!(
        disegnatori,
        vec![MODULO.to_string()],
        "la provenienza è disegnata in {} file invece che in `{MODULO}`. Ogni copia è una \
         divergenza che nessuno guarda: se una pagina ne ha bisogno per un motivo vero, il \
         motivo va nel modulo e la pagina lo chiama. I file che la nominano sono: {:?}",
        disegnatori.len(),
        disegnatori
    );
}

/// La seconda guardia: ogni `kind` dell'enum ha un ramo, in entrambe le versioni.
///
/// La versione breve (`provenienzaCorta`) è quella delle celle di elenco — coda e
/// corso — e la versione lunga (`provenienzaLunga`) è quella del lettore, che
/// mostra per intero il `model lock`. Sono due versioni per due domande diverse
/// e non diventano una: in un elenco la domanda è «di chi è», e l'hash del
/// prompt sta nella pagina dell'argomento, che è il posto dove lo si verifica.
/// Ma «due versioni» non vuol dire «due copie»: entrambe stanno nello stesso
/// modulo, e una variante nuova dell'enum che non ha un ramo in una delle due è
/// un buco in una pagina, non in un file.
#[test]
fn ogni_origine_del_server_ha_un_ramo_nella_provenienza() {
    let kind = kind_da_tenere_docchio();
    for versione in ["provenienzaCorta", "provenienzaLunga"] {
        let corpo = corpo_di(MODULO, versione).unwrap_or_else(|| {
            panic!(
                "`{MODULO}` non esporta `{versione}`: non è la stessa funzione di ieri, o non \
                 c'è più. Le due versioni sono la breve (le celle di elenco) e la lunga (il \
                 lettore), e senza la lunga l'hash del prompt non sta più da nessuna parte."
            )
        });
        for k in &kind {
            assert!(
                corpo.contains(&format!("\"{k}\"")),
                "`{MODULO}` non ha un ramo per `origin.kind = \"{k}\"`. Un argomento con \
                 quell'origine è possibile — la enum lo dichiara e `serde` lo manda — e senza \
                 un ramo qui finisce nel ripiego, che mostra il nome dell'enum al posto della \
                 provenienza. È il buco che aveva `coda.js` per `derived`."
            );
        }
    }
}
