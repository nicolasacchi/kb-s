//! D7: `GUARDIAN` deve stare **dentro i primi 640 byte**.
//!
//! La regola è nel contratto (`ARCHITECTURE.md`, D7) e il motivo è dichiarato:
//! il troncamento taglia la coda, e se il vincolo che lo studente deve leggere
//! per primo è oltre il taglio, l'agente legge la coda senza sapere perché.
//!
//! Il caso è dichiarato da tempo in `common::artifact(true)` — un `GUARDIAN` di
//! oltre mille byte — e non era esercitato da nessun test. Qui lo è, sui due
//! lati della regola: il `GUARDIAN` entro i 640 byte passa, quello oltre no.

mod common;

use kbs_intake::route::{self, Request, Route, Verdict};

use common::*;

const CODICE: &str = "contract-guardian-out-of-budget";

/// La riga d'intestazione della prima sezione, che è la riga che `kbs_doc`
/// recognizes come `## GUARDIAN`.
const INTESTAZIONE: &str = "## GUARDIAN\n";

fn verdetto_di(sorgente: &str) -> Verdict {
    let mut s = store_con_corso();
    route::receive(
        &mut s,
        Request {
            route: Route::File,
            by: docente(),
            course: Some(corso()),
            rel_path: Some(REL.to_string()),
            source: sorgente.to_string(),
        },
    )
    .expect("l'intake non fallisce: un contratto rotto è un verdetto, non un crash")
    .verdict
}

/// Il `GUARDIAN` che entra nei 640 byte è un contratto che si può eseguire.
#[test]
fn un_guardian_dentro_i_640_byte_non_e_un_problema() {
    let v = verdetto_di(&artifact(false));
    assert!(v.can_publish(), "un GUARDIAN in budget non blocca: {v:?}");
    assert!(
        !v.has_code(CODICE),
        "un GUARDIAN in budget è stato dichiarato oltre budget"
    );
}

/// Il `GUARDIAN` oltre i 640 byte blocca, e dice **quanto è grande**.
///
/// Il messaggio porta i due numeri — quanto è cresciuto e quanto poteva — perché
/// un «non va bene» senza misura non serve a correggere niente.
#[test]
fn un_guardian_oltre_i_640_byte_e_rifiutato() {
    let v = verdetto_di(&artifact(true));
    let d = v
        .first_blocking()
        .expect("un GUARDIAN oltre budget è un problema bloccante");
    assert_eq!(d.code, CODICE, "il codice è il contratto con il banco: {d:?}");
    assert!(
        d.message.contains("640"),
        "il messaggio non dice il limite: {}",
        d.message
    );
    assert!(!v.can_publish(), "un GUARDIAN oltre budget non si pubblica");
}

/// La regola è sulla **posizione**, non sulla lunghezza: un `GUARDIAN` corto ma
/// non al primo posto non è un `GUARDIAN` al primo posto.
///
/// È la differenza fra la regola di D7 e la regola che `kbs_doc` applica: quella
/// guarda l'offset della riga `## GUARDIAN`, che per la prima sezione è sempre
/// zero e quindi non può mai fallire. Qui si guarda dove la sezione **finisce**,
/// che è la quantità che il troncamento morde.
#[test]
fn la_regola_guarda_onde_finisce_la_sezione_e_non_dove_comincia() {
    // Un `GUARDIAN` di poche righe sta intero nei primi 640: passa.
    let corto = verdetto_di(&artifact(false));
    assert!(!corto.has_code(CODICE), "{corto:?}");

    // Lo stesso `GUARDIAN` con 500 byte di preambolo davanti non ci sta più nei
    // 640. La regola che guarda l'inizio lo accetterebbe: l'offset della sua
    // intestazione è 502, che è sotto 640. Quella che guarda la fine no, e il
    // confronto fra le due è il punto del test.
    let n = 500;
    assert!(
        n + 2 < 640,
        "il preambolo di questo test deve lasciare l'intestazione sotto i 640 byte, \
         altrimenti il confronto non sarebbe quello dichiarato"
    );
    let spostato = verdetto_di(&con_preambolo(&artifact(false), n));
    let d = spostato
        .first_blocking()
        .expect("un GUARDIAN che finisce oltre i 640 byte è un problema");
    assert_eq!(d.code, CODICE, "la sezione finisce oltre il limite: {d:?}");
}

/// Il caso minimo: un `GUARDIAN` la cui sezione termina **esattamente** al
/// 640° byte è ancora dentro. La regola è «dentro i primi 640», e il bordo
/// incluso è ciò che distingue «dentro» da «oltre».
///
/// Il margine di un byte è deliberato: una regola di byte che non dice quale
/// parte include il bordo è una regola che si può far valere in due modi
/// opposti, e quello che perde è il lettore.
#[test]
fn il_bordo_dei_640_byte_e_dentro_e_il_641_e_oltre() {
    for (fine, deve_essere_problema) in [(640usize, false), (641, true)] {
        let fonte = artifact(false);
        // La sezione `GUARDIAN` finisce dove comincia la riga `## PREREQUISITI`,
        // che è il modo in cui la sezione è delimitata: quel punto è la fine.
        let inizio_contratto = posizione_della_contratto(&fonte);
        let inizio_guardian = fonte[inizio_contratto..]
            .find(INTESTAZIONE)
            .expect("il contratto ha la sezione")
            + inizio_contratto;
        let corpo = inizio_guardian + INTESTAZIONE.len() - inizio_contratto;
        assert!(
            fine > corpo + 2,
            "il GUARDIAN di prova è già più lungo di {fine} byte: corpo a {corpo}"
        );
        let riscritto = con_corso_di(&fonte, corpo, fine);
        let v = verdetto_di(&riscritto);
        assert_eq!(
            v.has_code(CODICE),
            deve_essere_problema,
            "una sezione che finisce a {fine} byte: {v:?}"
        );
    }
}

/// L'inizio del testo del contratto: ciò che segue `<template id="kb-kbprompt">`.
///
/// I byte della regola sono quelli **del contratto**, non quelli del file: il
/// troncamento morde il contratto, e il `<head>` non c'entra. Contarli sul file
/// darebbe un numero diverso e un test che passerebbe per la ragione sbagliata.
fn posizione_della_contratto(fonte: &str) -> usize {
    let slot = fonte.find("<template id=\"kb-kbprompt\">").expect("lo slot del contratto");
    slot + "<template id=\"kb-kbprompt\">\n".len()
}

/// Riscrve il corpo del `GUARDIAN` con riempimento, in modo che la sezione
/// termini a `fine` byte dall'inizio del contratto.
///
/// Il riempimento è ASCII senza accenti: i byte contati sono quelli che il test
/// crede di contare, e un carattere multibyte farebbe fallire il test per la
/// ragione sbagliata.
fn con_corso_di(fonte: &str, corpo: usize, fine: usize) -> String {
    const CORPO: &str = "Prima di leggere: la forma ridotta e' unica e va costruita moltiplicando numeratore e denominatore per lo stesso fattore primo, e se i due fattori non sono primi l'unificazione non e' stata fatta bene. ";
    let inizio_contratto = posizione_della_contratto(fonte);
    let inizio_guardian = inizio_contratto
        + fonte[inizio_contratto..]
            .find(INTESTAZIONE)
            .expect("il contratto ha la sezione");
    let inizio_corpo = inizio_guardian + INTESTAZIONE.len();
    // La sezione finisce alla riga `## PREREQUISITI`, e la riga vuota prima di
    // essa è separatore: si riscrive il corpo fino al separatore, e il
    // separatore resta. Toglierlo produrrebbe un contratto a sette sezioni,
    // cioè un altro difetto che mascherebbe quello che il test sta provando.
    let fine_sezione = inizio_contratto
        + fonte[inizio_contratto..]
            .find("## PREREQUISITI")
            .expect("c'è la sezione successiva");
    let inizio_separatore = fine_sezione - 2;
    assert!(
        fonte.as_bytes()[inizio_separatore..fine_sezione] == *b"\n\n",
        "il separatore fra le sezioni è di due a capo"
    );
    // La sezione finisce alla riga successiva, e il separatore di due a capo
    // ne fa parte: l'estensione è corpo più separatore, e il limite si
    // confronta con l'estensione. Per questo i due byte del separatore si
    // sottraggono, e non è un dettaglio del test: è la ragione per cui il
    // bordo è esatto.
    let lunghezza = fine - corpo - 2;
    let mut testo = String::with_capacity(lunghezza);
    while testo.len() < lunghezza {
        testo.push_str(CORPO);
    }
    testo.truncate(lunghezza);
    assert!(
        testo.is_char_boundary(lunghezza),
        "taglio su un confine di carattere"
    );
    let mut out = String::with_capacity(fonte.len() + lunghezza);
    out.push_str(&fonte[..inizio_corpo]);
    out.push_str(&testo);
    out.push_str(&fonte[inizio_separatore..]);
    out
}

/// Mette `n` byte di preambolo davanti alla prima sezione del contratto.
///
/// Il preambolo finisce con una riga vuota: senza, la prima intestazione
/// verrebbe fusa con l'ultima riga del preambolo e il contratto avrebbe sette
/// sezioni — che è un altro difetto, non quello che questo test sta provando.
fn con_preambolo(fonte: &str, n: usize) -> String {
    const RIGA: &str = "Nota di servizio per chi legge questa unita' prima di cominciare.\n";
    let inizio_contratto = posizione_della_contratto(fonte);
    let mut preambolo = String::with_capacity(n + 2);
    while preambolo.len() < n {
        preambolo.push_str(RIGA);
    }
    preambolo.truncate(n);
    preambolo.push_str("\n\n");
    let mut out = String::with_capacity(fonte.len() + preambolo.len());
    out.push_str(&fonte[..inizio_contratto]);
    out.push_str(&preambolo);
    out.push_str(&fonte[inizio_contratto..]);
    out
}
