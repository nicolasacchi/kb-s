//! D10.1 — l'endpoint di cattura: il docente incolla.
//!
//! È la strada più economica e la più fragile, e la fragilità è tutta qui: un
//! paste è testo libero, e **il sistema non indovina**. Un paste che non si
//! capisce viene rifiutato con un motivo, non interpretato. Il motivo è la
//! parte del lavoro: a un docente che ha incollato venti righe e ha ottenuto
//! un errore, «riga 4 non riconosciuta: `Va bene, mandiamolo così`» dice dove
//! guardare, e «errore di parsing» no.
//!
//! # Il formato, e perché è stretto
//!
//! ```text
//! Titolo: Numeri razionali            \  chiavi dell'involucro, una per riga,
//! Corso: matematica-seconda          \  facoltative tutte tranne il contenuto
//! Prerequisiti: letture/01-numero.html/
//!                                     /
//! ```artifact                        \  esattamente un blocco, marcato `artifact`
//! <!doctype html> …                  /
//! ```                                /
//! ```
//!
//! Le regole sono tutte «non indovinare», e ognuna ha un test:
//!
//! * **zero o più di un blocco** → [`Error::FencingAmbiguo`]. Con due blocchi
//!   non si sa quale sia l'artifact, e scegliere il primo sarebbe una moneta
//!   truccata.
//! * **un blocco marcato con un'altra lingua** → [`Error::FencingNonSupportato`].
//!   Un ```` ```json ```` è un'altra cosa, e questa strada la rifiuta invece di
//!   mangiare il contenuto.
//! * **nessun blocco** → il testo intero è il documento, ma solo se *sa* di
//!   documento: nessun `<html`, `<title>`, `<body` o `<template` in
//!   [`Error::PasteNonDocumento`]. Una frase in italiano non viene trattata come
//!   un artifact e ripulita: verrebbe fuori un `<title>` di tre righe.
//! * **una riga fuori dalle chiavi** → [`Error::RigaNonRiconosciuta`], con il
//!   numero di riga. È il caso più comune e va detto così.
//! * **una chiave ripetuta** → [`Error::ChiaveRipetuta`]. Due valori per la
//!   stessa chiave sono una domanda, e questa strada non fa domande.
//!
//! Cosa **non** fa: non completa un contratto a sei sezioni, non indovina il
//! corso dal titolo, non sceglie fra due `rel_path`. Un paste che arriva senza
//! corso va in `bozza` di un corso dichiarato, o viene rifiutato — e il
//! rifiuto è [`Error::CorsoMancante`], che è un altro nome per «non lo so».
//!
//! # Perché l'involucro è facoltativo
//!
//! Perché il caso più frequente è un documento intero e basta, e un formato che
//! pretende un involucro per dire il titolo che il documento contiene già
//! sarebbe un formato che chiede due volte la stessa cosa. Le chiavi servono
//! per ciò che il documento **non** dice: da quale corso viene e quali
//! prerequisiti ha.

use std::collections::BTreeMap;

use kbs_core::{CourseId, PersonId};

use crate::error::{Error, Result};
use crate::route::{Request, Route};

/// Le chiavi dell'involucro, nell'ordine in cui vengono lette.
pub const CHIAVI: [&str; 5] =
    ["titolo", "corso", "stato", "prerequisiti", "rel-path"];

/// Le chiavi, in forma di frase: è ciò che l'errore mostra a chi ha sbagliato.
const CHIAVI_ETICHETTA: &str = "titolo, corso, stato, prerequisiti, rel-path";

/// Che cosa è stato incollato, una volta letto.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Captured {
    /// Il documento, senza l'involucro e senza il fencing.
    pub source: String,
    /// Il titolo dall'involucro, se c'era. **Non** fa fede sul titolo del
    /// documento: se i due differiscono, vince il documento, che è ciò che il
    /// validatore e l'indice vedranno. Qui si registra solo l'intento.
    pub titolo: Option<String>,
    pub corso: Option<CourseId>,
    pub stato: Option<String>,
    pub prerequisiti: Vec<String>,
    pub rel_path: Option<String>,
}

/// Il marker del fencing. Esatto: un blocco senza indicazione di lingua è
/// accettato (è la forma più comune), un blocco con una lingua diversa no.
const MARKER: &str = "artifact";

/// Un paste che si può ragionevolmente intendere come «un artifact, nient'altro».
///
/// Non chiama `kbs_doc::validate`: la validazione è di
/// [`crate::route::receive`] ed è la stessa per tutte e quattro le strade. Qui
/// finisce solo ciò che riguarda **la forma del paste**, che è l'unica cosa che
/// questa strada possiede.
pub fn parse_paste(input: &str) -> Result<Captured> {
    if input.trim().is_empty() {
        return Err(Error::PasteVuoto);
    }
    let righe: Vec<&str> = input.lines().collect();
    let blocchi = cerca_blocchi(&righe);
    let (corpo, fuori): (String, &[&str]) = match blocchi.len() {
        0 => {
            // Nessun blocco: tutto il testo è il documento, e **non c'è
            // involucro**. Trattare le sue righe come chiavi sarebbe la fine
            // della strada del paste.
            if !sa_di_documento(input) {
                return Err(Error::PasteNonDocumento { righe: righe.len() });
            }
            (input.to_string(), &[])
        }
        1 => {
            let (aperto, chiuso) = match blocchi[0] {
                (a, Some(c)) => (a, c),
                // Un blocco aperto e non chiuso non è un blocco: è un paste
                // troncato, e dire quale metà sia quella giusta sarebbe
                // indovinare.
                _ => return Err(Error::FencingAmbiguo { blocchi: 1 }),
            };
            let marker = marker_di(&righe[aperto]);
            if !marker.is_empty() && marker != MARKER {
                return Err(Error::FencingNonSupportato { linguaggio: marker });
            }
            let interno: String = righe[aperto + 1..chiuso].join("\n");
            if !sa_di_documento(&interno) {
                return Err(Error::PasteNonDocumento { righe: righe.len() });
            }
            // Dietro il blocco non c'e' niente: una riga non vuota dopo il
            // contenuto e' indistinguibile da un secondo tentativo, e
            // «scegliere il primo» e' esattamente l'indovinare che questa strada
            // non fa.
            if let Some(offset) = righe[chiuso + 1..].iter().position(|l| !l.trim().is_empty()) {
                return Err(Error::RigaNonRiconosciuta {
                    numero: chiuso + 2 + offset,
                    testo: righe[chiuso + 1 + offset].trim().to_string(),
                    chiavi: CHIAVI_ETICHETTA,
                });
            }
            (interno, &righe[..aperto])
        }
        n => return Err(Error::FencingAmbiguo { blocchi: n }),
    };

    let mut chiavi: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, riga) in fuori.iter().enumerate() {
        if riga.trim().is_empty() {
            continue;
        }
        let numero = i + 1;
        let Some((nome, valore)) = riga.split_once(':') else {
            return Err(Error::RigaNonRiconosciuta {
                numero,
                testo: riga.trim().to_string(),
                chiavi: CHIAVI_ETICHETTA,
            });
        };
        let nome = nome.trim().to_ascii_lowercase();
        if !CHIAVI.contains(&nome.as_str()) {
            return Err(Error::RigaNonRiconosciuta {
                numero,
                testo: riga.trim().to_string(),
                chiavi: CHIAVI_ETICHETTA,
            });
        }
        let voce = chiavi.entry(nome.clone()).or_default();
        // `prerequisiti` è l'unica chiave ripetibile: elencare i prerequisiti è
        // il suo compito, e pretenderne uno solo sarebbe una limitazione
        // arbitraria.
        if !voce.is_empty() && nome != "prerequisiti" {
            return Err(Error::ChiaveRipetuta { chiave: nome });
        }
        voce.push(valore.trim().to_string());
    }

    let one = |k: &str| chiavi.get(k).and_then(|v| v.first()).cloned();
    Ok(Captured {
        source: corpo,
        titolo: one("titolo"),
        corso: one("corso").filter(|c| !c.is_empty()).map(CourseId),
        stato: one("stato").filter(|s| !s.is_empty()),
        prerequisiti: chiavi
            .get("prerequisiti")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect(),
        rel_path: one("rel-path").filter(|p| !p.is_empty()),
    })
}

/// La richiesta per il collo di bottiglia, costruita da un paste.
///
/// Lo `stato` dell'involucro **non finisce nella richiesta**: lo stato è una
/// cosa che `receive` decide chiedendo la ratifica al negozio, e accettarlo
/// qui significherebbe avere due strade per lo stesso stato. Una chiave
/// `stato:` che dica `in-corso` non è un errore di sintassi — è semplicemente
/// ignorata, e l'argomento entra in bozza. Dire al docente che la sua
/// dichiarazione è stata ignorata è compito del verdetto, che lo fa.
pub fn to_request(captured: Captured, by: PersonId) -> Request {
    Request {
        route: Route::Capture,
        by,
        course: captured.corso,
        rel_path: captured.rel_path,
        source: captured.source,
    }
}

/// I blocchi delimitati da tre o più backtick.
///
/// Il secondo elemento è `None` quando il blocco è aperto e non chiuso: è un
/// caso suo e non una coppia che si può dedurre, perché «le righe fino in fondo»
/// sarebbero un'invenzione.
fn cerca_blocchi(righe: &[&str]) -> Vec<(usize, Option<usize>)> {
    let mut out = Vec::new();
    let mut aperto: Option<usize> = None;
    for (i, riga) in righe.iter().enumerate() {
        let t = riga.trim_start();
        if !t.starts_with("```") {
            continue;
        }
        match aperto {
            None => aperto = Some(i),
            Some(a) => {
                out.push((a, Some(i)));
                aperto = None;
            }
        }
    }
    if let Some(a) = aperto {
        out.push((a, None));
    }
    out
}

/// La lingua dichiarata sulla riga di apertura, senza i backtick.
fn marker_di(riga: &str) -> String {
    riga.trim_start().trim_start_matches('`').trim().to_ascii_lowercase()
}

/// Che cosa un testo deve mostrare perché si possa tentare di leggerlo come
/// documento.
///
/// È un test di presenza, non di validità: la validità è di `kbs-doc` e la
/// decidono le otto sezioni del contratto. Qui la domanda è più grossa — «è
/// questo un paste da cui si può tirare fuori un artifact?» — e la risposta
/// deve essere **no** per ogni testo che non abbia un segno di documento.
fn sa_di_documento(testo: &str) -> bool {
    let t = testo.to_ascii_lowercase();
    ["<html", "<title", "<body", "<template", "<!doctype"]
        .iter()
        .any(|segno| t.contains(segno))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTIFACT: &str = "<!doctype html>\n<html lang=\"it\">\n<head>\n<title>Frazione</title>\n<meta name=\"kb-course\" content=\"matematica-seconda\">\n</head>\n<body><h1 id=\"t\">Frazione</h1><p>testo</p></body>\n</html>";

    #[test]
    fn un_documento_bare_e_un_documento() {
        let c = parse_paste(ARTIFACT).unwrap();
        assert_eq!(c.source.trim(), ARTIFACT.trim());
        assert!(c.corso.is_none());
    }

    #[test]
    fn un_involucro_con_il_blocco() {
        let testo = format!("Titolo: Frazione\nCorso: matematica-seconda\n\n```artifact\n{ARTIFACT}\n```\n");
        let c = parse_paste(&testo).unwrap();
        assert_eq!(c.titolo.as_deref(), Some("Frazione"));
        assert_eq!(c.corso, Some(CourseId("matematica-seconda".into())));
        assert!(c.source.contains("<title>Frazione</title>"));
    }

    #[test]
    fn due_blocchi_sono_ambigui_e_non_una_scelta() {
        let testo = format!("```artifact\n{ARTIFACT}\n```\n```artifact\n{ARTIFACT}\n```\n");
        match parse_paste(&testo) {
            Err(Error::FencingAmbiguo { blocchi }) => assert_eq!(blocchi, 2),
            altro => panic!("atteso FencingAmbiguo, trovato {altro:?}"),
        }
    }

    #[test]
    fn un_blocco_di_un_altro_tipo_e_rifiutato() {
        let testo = format!("```json\n{{\"a\": 1}}\n```\n");
        match parse_paste(&testo) {
            Err(Error::FencingNonSupportato { linguaggio }) => assert_eq!(linguaggio, "json"),
            altro => panic!("atteso FencingNonSupportato, trovato {altro:?}"),
        }
    }

    #[test]
    fn un_blocco_marcato_artifact_e_accettato() {
        let testo = format!("```artifact\n{ARTIFACT}\n```");
        assert!(parse_paste(&testo).unwrap().source.contains("<title>"));
    }

    #[test]
    fn una_f_rase_in_italiano_non_e_un_documento() {
        match parse_paste("Va bene, mandiamolo così come prima.") {
            Err(Error::PasteNonDocumento { righe }) => assert_eq!(righe, 1),
            altro => panic!("atteso PasteNonDocumento, trovato {altro:?}"),
        }
    }

    #[test]
    fn un_paste_vuoto_e_rifiutato() {
        assert!(matches!(parse_paste("   \n  \n"), Err(Error::PasteVuoto)));
    }

    #[test]
    fn una_riga_sconosciuta_dice_il_numero() {
        let testo = format!("Titolo: Frazione\nMandiamolo così\n\n```artifact\n{ARTIFACT}\n```\n");
        match parse_paste(&testo) {
            Err(Error::RigaNonRiconosciuta { numero, testo, .. }) => {
                assert_eq!(numero, 2);
                assert_eq!(testo, "Mandiamolo così");
            }
            altro => panic!("atteso RigaNonRiconosciuta, trovato {altro:?}"),
        }
    }

    #[test]
    fn una_chiave_ripetuta_e_rifiutata_e_i_prerequisiti_no() {
        let doppio = format!("Titolo: A\nTitolo: B\n\n```artifact\n{ARTIFACT}\n```\n");
        assert!(matches!(&parse_paste(&doppio), Err(Error::ChiaveRipetuta { chiave }) if chiave == "titolo"));
        let prereq = format!(
            "Prerequisiti: a/uno.html\nPrerequisiti: a/due.html\n\n```artifact\n{ARTIFACT}\n```\n"
        );
        assert_eq!(parse_paste(&prereq).unwrap().prerequisiti, ["a/uno.html", "a/due.html"]);
    }

    #[test]
    fn un_blocco_non_chiuso_e_ambiguo() {
        let testo = format!("```artifact\n{ARTIFACT}\n");
        match parse_paste(&testo) {
            Err(Error::FencingAmbiguo { blocchi }) => assert_eq!(blocchi, 1),
            altro => panic!("atteso FencingAmbiguo, trovato {altro:?}"),
        }
    }

    #[test]
    fn un_commento_dopo_il_blocco_e_rifiutato() {
        let testo = format!("```artifact\n{ARTIFACT}\n```\nE poi mando il secondo\n");
        // 1 (apertura) + le righe del documento + 1 (chiusura) + 1 (il commento).
        let atteso = ARTIFACT.lines().count() + 3;
        assert!(matches!(&parse_paste(&testo),
            Err(Error::RigaNonRiconosciuta { numero, testo, .. }) if *numero == atteso && testo == "E poi mando il secondo"));
    }

    #[test]
    fn lo_stato_dell_involucro_non_passa_nella_richiesta() {
        let testo = format!("Stato: in-corso\n\n```artifact\n{ARTIFACT}\n```\n");
        let c = parse_paste(&testo).unwrap();
        assert_eq!(c.stato.as_deref(), Some("in-corso"));
        // La richiesta non ha un campo `stato`: è `receive` a chiedere la
        // ratifica. Qui non c'è niente da guardare, ed è il punto.
        let r = to_request(c, PersonId::fixture(1));
        assert_eq!(r.route, Route::Capture);
        assert!(r.source.contains("<title>"));
    }
}
