//! L'inventario delle letture, verificato meccanicamente.
//!
//! `lib.rs` dichiara un inventario **chiuso** delle strade pubbliche di
//! lettura, e una dichiarazione chiusa che nessuno controlla è una dichiarazione
//! che invecchia. Qui il controllo è meccanico: si legge il sorgente del crate e
//! si contano le firme, quindi un `pub fn` nuovo che consegna materiale di una
//! persona senza chiedere chi è è un test che fallisce, non una riga che nessuno
//! rilegge.
//!
//! # I tre controlli, e perché sono tre
//!
//! 1. **Ogni `pub fn` che restituisce un'`Observation` prende un `&PersonId`.**
//!    È l'antidoto al difetto che è costato più caro: una lettura del registro
//!    delle dimostrazioni che non sa chi chiede.
//! 2. **Ogni lettura pubblica che consegna contenuto di un corso e che
//!    non prende una persona è nominata nell'inventario di `lib.rs`.** La
//!    qualificazione è nella frase e non è un dettaglio: una lettura che
//!    prende una persona risponde già alla domanda «di chi è», ed è il caso
//!    senza persona che il predicato non può coprire. Non basta che ci sia: il
//!    nome deve comparire come collegamento, così l'inventario è un elenco che
//!    si può consultare e non un paragrafo che si può dimenticare.
//! 3. **La scansione trova le firme.** Una guardia che non trova niente passa
//!    sempre, e una guardia che passa sempre è un commento con un `assert`.
//!
//! Il metodo è una scansione del sorgente e non del database: è l'unico modo che
//! ha un test per dire «questa firma prende una persona», dato che la firma è la
//! proprietà.

use std::path::{Path, PathBuf};

/// Le firme che consegnano una riga di registro **senza** prendere una persona.
///
/// Sono [`Store::append_observation`](crate::Store::append_observation) e
/// [`Store::append_grading`](crate::Store::append_grading), ed è un'eccezione
/// dichiarata per una ragione sola: sono **scritture**, e restituiscono la riga
/// che hanno appena scritto, con il `seq` che il registro le ha assegnato. Una
/// firma che prende una persona e non la usa sarebbe una coperta: questo crate
/// dichiara di non autenticare e non autorizzare in scrittura, e la firma lo
/// dice. Le altre scritture non compaiono nell'elenco perché restituiscono `()` e
/// quindi non sono strade di lettura.
const SCRITTURE: &[&str] = &["append_observation", "append_grading"];

/// Una firma pubblica, ridotta a quello che serve alla guardia.
struct Firma {
    modulo: String,
    nome: String,
    parametri: String,
    ritorno: String,
}

impl Firma {
    /// Se la strada pubblica consegna contenuto che appartiene a un corso.
    ///
    /// I tipi elencati qui sono quelli di `kbs-core` (o aggiunti da questo crate)
    /// che descrivono **materiale**: un argomento, un esercizio, una risposta,
    /// un giudizio, una dimostrazione, una rubrica, un evento di generazione, un
    /// segnale di coorte. **Un aggregato è materiale quanto la sua riga**: il
    /// metro di uno studente, la quota di una classe e la coda di richiamo sono
    /// tre letture di corso come un argomento, e una guardia che le lascia fuori
    /// ascolterebbe in silenzio il giorno in cui una di loro perdesse la
    /// persona. Un id, uno stato o una stringa non sono materiale, e la guardia
    /// non ha niente da dire su quelli.
    fn consegna_contenuto(&self) -> bool {
        [
            "Observation",
            "Grading",
            "Claim",
            "Argument",
            "Exercise",
            "Instance",
            "RubricVersion",
            "GenerationEvent",
            "CohortSignal",
            "Proof",
            "MasteryRow",
            "CohortShare",
            "Calendar",
        ]
        .iter()
        .any(|t| contiene_parola(&self.ritorno, t))
    }

    fn prende_persona(&self) -> bool {
        contiene_parola(&self.parametri, "PersonId")
    }
}

#[test]
fn ogni_lettura_del_registro_delle_dimostrazioni_prende_una_persona() {
    let trasgresse: Vec<String> = firme_del_crate()
        .into_iter()
        .filter(|f| contiene_parola(&f.ritorno, "Observation"))
        .filter(|f| !f.prende_persona())
        .filter(|f| !SCRITTURE.contains(&f.nome.as_str()))
        .map(|f| format!("{}::{}", f.modulo, f.nome))
        .collect();
    assert!(
        trasgresse.is_empty(),
        "una lettura del registro delle dimostrazioni che non sa chi chiede:\n  {}\n\
         Il predicato è il modello di sicurezza (D5): senza `&PersonId` non c'è \
         nessuna domanda a cui rispondere.",
        trasgresse.join("\n  ")
    );
}

#[test]
fn ogni_lettura_senza_predicato_e_nominata_in_inventario() {
    // L'inventario di `lib.rs` deve citare il metodo per esteso: un nome che
    // compare solo in un commento di un modulo non è un inventario.
    let lib = include_str!("../lib.rs");
    let mancanti: Vec<String> = firme_del_crate()
        .into_iter()
        .filter(|f| f.consegna_contenuto() && !f.prende_persona())
        .filter(|f| !SCRITTURE.contains(&f.nome.as_str()))
        .map(|f| f.nome.clone())
        .filter(|n| !lib.contains(&format!("[`Store::{n}`]")))
        .collect();
    assert!(
        mancanti.is_empty(),
        "letture pubbliche che l'inventario di `lib.rs` non nomina:\n  {}\n\
         L'inventario è dichiarato esaustivo, quindi una strada che non c'è dentro \
         non è una strada: è una dimenticanza.",
        mancanti.join("\n  ")
    );
}

#[test]
fn la_scansione_trova_le_firme_e_non_e_una_guardia_vuota() {
    // Il numero è alto abbastanza da non poter essere un caso: se un domani
    // `firme` smettesse di riconoscere le firme, i due test sopra passerebbero
    // senza controllare niente, e qui si vedrebbe.
    let firme = firme_del_crate();
    assert!(
        firme.len() > 50,
        "la scansione ha trovato {} firme: non è un crate con le firme in `src`",
        firme.len()
    );
    assert!(
        firme.iter().any(|f| f.nome == "read_argument"),
        "la scansione non vede nemmeno `read_argument`"
    );
}

// ── la scansione ─────────────────────────────────────────────────────────────

fn src_del_crate() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// I file di cui il crate è fatto: tutto `src/`, esclusi i test.
///
/// La directory e non un elenco di file: un modulo nuovo entra nella guardia
/// senza che nessuno debba ricordarsi di aggiungerlo.
fn file_del_crate() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![src_del_crate()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // `src/tests` è il banco di prova e non il crate: le sue funzioni
                // non sono parte dell'API, e la guardia non le riguarda.
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn firme_del_crate() -> Vec<Firma> {
    let mut out = Vec::new();
    for path in file_del_crate() {
        let Ok(sorgente) = std::fs::read_to_string(&path) else {
            continue;
        };
        let modulo = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.extend(firme(&sorgente, &modulo));
    }
    out
}

/// Le firme pubbliche di un file.
///
/// Non è un parser: è una lettura di ciò che il crate scrive davvero, e le
/// quattro cose di cui ha bisogno sono la parola `pub fn`, il nome, la parentesi
/// tonda che chiude i parametri e il `{` o il `;` che chiude la firma. Una firma
/// che nessuna delle quattro regole riconosce è saltata, ed è per questo che il
/// formato del codice è una cosa che questo test rende visibile: cambiare il modo
/// in cui le firme sono scritte cambia questo file.
fn firme(sorgente: &str, modulo: &str) -> Vec<Firma> {
    let mut out = Vec::new();
    let mut da_cercare = 0;
    while let Some(trovato) = sorgente[da_cercare..].find("pub fn") {
        let start = da_cercare + trovato;
        let dopo = start + "pub fn".len();
        da_cercare = dopo;
        // `pub fn` deve stare a confine di tokenio: altrimenti si aggancia a una
        // parola che lo contiene, e una guardia che aggancia cose a caso non è
        // una guardia. Davanti un identificatore, dietro uno spazio.
        let prima = start == 0
            || sorgente[..start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric() && c != '_');
        let spazio = sorgente[dopo..].starts_with(|c: char| c.is_whitespace());
        if !(prima && spazio) {
            continue;
        }
        let resto = &sorgente[dopo..];
        let inizio_nome = dopo + (resto.len() - resto.trim_start().len());
        let nome: String = sorgente[inizio_nome..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if nome.is_empty() {
            continue;
        }
        let fine_nome = inizio_nome + nome.len();
        let Some(aperta) = sorgente[fine_nome..].find('(').map(|p| fine_nome + p) else {
            continue;
        };
        let mut profondita = 0usize;
        let mut chiusa = None;
        for (i, c) in sorgente[aperta..].char_indices() {
            match c {
                '(' => profondita += 1,
                ')' => {
                    profondita -= 1;
                    if profondita == 0 {
                        chiusa = Some(aperta + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(chiusa) = chiusa else { continue };
        // Il ritorno finisce al `{` che apre il corpo o al `;` della dichiarazione.
        let fine = sorgente[chiusa + 1..]
            .find(['{', ';'])
            .map(|p| chiusa + 1 + p)
            .unwrap_or(sorgente.len());
        out.push(Firma {
            modulo: modulo.to_string(),
            nome,
            parametri: sorgente[aperta + 1..chiusa].to_string(),
            ritorno: sorgente[chiusa + 1..fine].trim().to_string(),
        });
    }
    out
}

/// Una parola intera dentro un testo.
///
/// «`Observation`» e «`ObservationDraft`» sono due cose diverse, e una guardia
/// che le confonde segnalerebbe metà delle firme sbagliate.
fn contiene_parola(testo: &str, parola: &str) -> bool {
    let mut da_cercare = 0;
    while let Some(trovato) = testo[da_cercare..].find(parola) {
        let start = da_cercare + trovato;
        let fine = start + parola.len();
        let prima_ok = start == 0
            || testo[..start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric() && c != '_');
        let dopo_ok = testo[fine..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_alphanumeric() && c != '_');
        if prima_ok && dopo_ok {
            return true;
        }
        da_cercare = fine;
    }
    false
}
