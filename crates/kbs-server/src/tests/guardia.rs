//! La guardia che dice che qui non ci sono ruoli.
//!
//! D5 vieta i ruoli come oggetto memorizzato, e il vincolo del mio compito è
//! che **in questo crate non ci sia nessun controllo per ruolo**. Una
//! proprietà di un crate non si verifica leggendolo: si verifica con un test
//! che fallisce se la proprietà viene rotta. E la forma più economica di quel
//! test è chiedere al sorgente di questo crate stesso se contiene la parola.
//!
//! # Perché si legge il sorgente e non l'API
//!
//! perché la cosa da impedire non è un tipo pubblico: è un `if` dentro un
//! handler. Un `if persona.ruolo == "docente"` compila, si usa, e non lascia
//! traccia in nessuna superficie pubblica. Solo il sorgente lo vede.
//!
//! # Perché i commenti sono esclusi
//!
//! perché questo crate **parla** di ruoli: il modulo `capability` dice che non
//! ce ne sono, e dirlo è il punto. Se la guardia contasse le occorrenze nelle
//! prose, il modo per farla passare sarebbe non più scrivere perché esistono —
//! cioè renderebbe silenzioso l'unico avvertimento che c'è. La guardia
//! controlla il **codice**, e lascia la prosa libera.

use std::path::{Path, PathBuf};

/// Le parole che un controllo per ruolo userebbe.
///
/// `ruolo` e `role` sono entrambe: il codice di questo repository scrive in
/// italiano e i campi dei tipi sono in inglese, quindi un tentativo finirebbe
/// probabilmente per chiamarsi `role`.
const PAROLE: [&str; 4] = ["ruolo", "ruoli", "role", "roles"];

fn sorgenti() -> Vec<PathBuf> {
    fn raccogli(dir: &Path, fuori: &mut Vec<PathBuf>) {
        let Ok(voci) = std::fs::read_dir(dir) else {
            return;
        };
        for voce in voci.flatten() {
            let percorso = voce.path();
            if percorso.is_dir() {
                raccogli(&percorso, fuori);
            } else if percorso.extension().is_some_and(|e| e == "rs") {
                fuori.push(percorso);
            }
        }
    }
    let radice = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut fuori = Vec::new();
    raccogli(&radice, &mut fuori);
    // Questo file non si scansiona: contiene le parole che cerca, per
    // costruzione, ed è il solo che le contiene.
    // È l'unica eccezione, ed è dichiarata — una guardia che si esclude senza
    // dirlo è una guardia che si può escludere ovunque.
    fuori.retain(|p| !p.ends_with("guardia.rs"));
    fuori.sort();
    assert!(!fuori.is_empty(), "non ho trovato sorgenti in {radice:?}");
    fuori
}

/// Il sorgente senza commenti.
///
/// Lo stripper lavora a **byte**, non a caratteri, e non è un caso: lavorare a
/// caratteri richiede di trovare gli indici con `find` e poi di sommare
/// indici relativi a una fetta — e un italiano con le accentate fa esplodere
/// quella somma su un confine non valido. A byte non esistono i confini.
///
/// Non deve essere un parser: deve togliere la prosa e lasciare il codice.
pub(crate) fn senza_commenti(sorgente: &str) -> String {
    let b = sorgente.as_bytes();
    let mut fuori = String::with_capacity(sorgente.len());
    let mut i = 0;
    let mut in_blocco = false;
    while i < b.len() {
        if in_blocco {
            if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                in_blocco = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if b[i] == b'/' && i + 1 < b.len() {
            if b[i + 1] == b'/' {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if b[i + 1] == b'*' {
                in_blocco = true;
                i += 2;
                continue;
            }
        }
        let inizio = i;
        i += lunghezza_utf8(b[i]);
        fuori.push_str(&sorgente[inizio..i]);
    }
    fuori
}

/// Quanti byte occupa il carattere che comincia con questo byte iniziale.
///
/// Un byte di continuazione non può trovarsi all'inizio di un carattere UTF-8
/// valido, quindi il caso `0b10xxxxxx` non si presenta e il valore di
/// riserva è innocuo.
fn lunghezza_utf8(primo: u8) -> usize {
    match primo {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[test]
fn in_questo_crate_non_c_e_un_controllo_per_ruolo() {
    let mut colpevoli = Vec::new();
    for percorso in sorgenti() {
        let sorgente = std::fs::read_to_string(&percorso).expect("sorgente");
        let codice = senza_commenti(&sorgente).to_lowercase();
        for parola in PAROLE {
            if let Some(posizione) = codice.find(parola) {
                let riga = codice[..posizione].matches('\n').count() + 1;
                colpevoli.push(format!(
                    "{}:{} contiene `{parola}` nel codice",
                    percorso.display(),
                    riga + 1
                ));
            }
        }
    }
    assert!(
        colpevoli.is_empty(),
        "D5 vieta i ruoli, e qui ce n'è uno:\n{}",
        colpevoli.join("\n")
    );
}

#[test]
fn il_predicato_non_e_richiamato_da_questo_crate() {
    // La regola parallela: il predicato è di `kbs-core` e viene applicato da
    // `kbs-store`. Un secondo richiamo qui dentro sarebbe una seconda risposta
    // alla stessa domanda, e le due divergerebbero al primo caso limite.
    let mut chiamate = Vec::new();
    for percorso in sorgenti() {
        let sorgente = std::fs::read_to_string(&percorso).expect("sorgente");
        let codice = senza_commenti(&sorgente);
        if codice.contains("may_read") {
            chiamate.push(percorso.display().to_string());
        }
    }
    assert!(
        chiamate.is_empty(),
        "kbs_core::may_read è chiamato in:\n{}",
        chiamate.join("\n")
    );
}

#[test]
fn lo_stripper_di_commenti_funziona_su_un_caso_noto() {
    // Una guardia che non si prova è una guardia che non funziona: se lo
    // stripper non toglie i commenti, il primo test passa perché non cerca
    // niente, e il secondo fallisce su ogni file.
    let sorgente = "// ruolo\nlet a = 1; /* ruolo\nlet b = 2; */ let c = 3; // fine\nlet d = 4;\n";
    let codice = senza_commenti(sorgente);
    assert!(!codice.contains("ruolo"), "{codice}");
    assert!(codice.contains("let a = 1;"), "{codice}");
    // Quello che sta dentro il commento a blocco sparisce con il commento: non
    // è unagerarchia, è un commento.
    assert!(!codice.contains("let b = 2;"), "{codice}");
    // E quello che viene dopo torna.
    assert!(codice.contains("let c = 3;"), "{codice}");
    assert!(codice.contains("let d = 4;"), "{codice}");
}
