//! D10.4 — MCP in ingresso: il docente, dentro un agente, legge e scrive il
//! corpus.
//!
//! `kb` oggi non ha un server MCP: è un rifiuto registrato in `ARCHITECTURE.md`
//! D10, e qui diventa il prodotto. Questo modulo è un server **JSON-RPC 2.0 su
//! stdio**, che è il trasporto che gli host MCP parlano, ed è deliberatamente
//! piccolo: quattro operazioni.
//!
//! # Le quattro operazioni, e perché sono queste
//!
//! | metodo | che cosa fa | perché serve |
//! |---|---|---|
//! | `kbs/argomento` | rilettura di un argomento | «che cosa dice questo, e in che stato?» |
//! | `kbs/mancanti` | che cosa manca nel percorso di pubblicazione | «devo ancora ratificare, o è già a posto?» |
//! | `kbs/bozza` | crea una bozza | l'agente ha scritto, il docente lo mette dentro |
//! | `kbs/claim` | rilettura delle claim di un argomento | «che cosa è dichiarato, e che cosa è sostenuto?» |
//!
//! # Che cosa **non** c'è, e perché
//!
//! * **`kbs/promuovi`.** La promozione è l'atto del docente e passa da una
//!   ratifica esplicita. esporre «promuovi» come una chiamata MCP significherebbe
//!   che un agente può ratificare per lui, e un ratificatore che non ha
//!   ratificato è la versione informatica di un gate decorativo (D4). La strada
//!   esiste: [`crate::Gate`], e l'ha usata per conto proprio.
//! * **`kbs/elimina`.** Nessuna cancellazione. D6 è un registro append-only, e
//!   un'operazione che cancella è un'operazione che nessun altro registro di
//!   questo sistema può onorare.
//! * **`kbs/rivela_tutto`.** Nessun elenco di tutti gli argomenti. La visibilità
//!   è una funzione della relazione (D5) e ogni lettura passa da
//!   `kbs_store::read_argument`; un `SELECT` che restituisca tutto sarebbe un
//!   canale per imparare che cosa c'è nel corso, e `kbs-store` lo dice
//!   esplicitamente nella ragione di `NotReadable`.
//! * **Il payload della bozza non può contenere uno stato.** Una bozza entra in
//!   `bozza` e la porta la chiede a `crate::gate`; un `state` nel JSON sarebbe
//!   una seconda strada per lo stato, ed è la strada che D4 vieta.
//!
//! Il protocollo di trasporto è la parte noiosa e va detto come è fatto: una
//! richiesta JSON-RPC per riga, una risposta per riga, e `id` assente = notifica
//! (che non produce risposta). Le risposte d'errore usano il codice JSON-RPC
//! standard e il messaggio porta anche il **codice kebab-case** di `kbs-intake`,
//! perché un agente deve poter distinguere «non ti ho capito» da «la regola ha
//! detto no» senza leggere italiano.

use std::io::{BufRead, Write};

use kbs_core::{ArgumentId, CourseId, Millis, PersonId};
use kbs_store::Store;

use crate::error::{Error, Result};
use crate::route::{self, Request, Route, Verdict};

/// La versione del protocollo dichiarata in `initialize`.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Il nome del server, per `initialize`.
pub const NOME: &str = "kbs";

/// I metodi che questo server conosce.
///
/// La lista è nell'errore di metodo sconosciuto: un agente che chiede
/// `kbs/promuovi` riceve un elenco di quello che c'è, e non un no muto. Un no
/// muto fa ritentare; un elenco fa capire.
pub const METODI: [&str; 5] = [
    "initialize",
    "kbs/argomento",
    "kbs/mancanti",
    "kbs/bozza",
    "kbs/claim",
];

/// Serve il loop su stdio finché l'input finisce.
///
/// `out` riceve una risposta JSON-RPC per riga. Il loop non usa `async`: sono
/// richieste e risposte locali, e un server che non fa I/O di rete non ha
/// bisogno di un runtime — che è anche il motivo per cui questo file non ha
/// `tokio` fra le dipendenze.
pub fn servi(
    store: &mut Store,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<usize> {
    let mut n = 0;
    for riga in input.lines() {
        let riga = match riga {
            Ok(r) => r,
            Err(source) => {
                return Err(Error::Io { path: std::path::PathBuf::from("<stdin>"), source })
            }
        };
        if riga.trim().is_empty() {
            continue;
        }
        n += 1;
        let risposta = rispondi(store, &riga);
        let _ = writeln!(out, "{}", risposta);
        let _ = out.flush();
    }
    Ok(n)
}

/// Una riga di richiesta → una riga di risposta.
///
/// La funzione è pubblica e senza stato per un motivo pratico: è il modo più
/// economico di testare il protocollo senza un processo, e un protocollo che si
/// può testare solo lanciando un binario è un protocollo che non si testa.
pub fn rispondi(store: &mut Store, riga: &str) -> String {
    let richiesta: serde_json::Value = match serde_json::from_str(riga) {
        Ok(v) => v,
        Err(e) => {
            return errore(None, -32700, "parse-error", &format!("JSON non valido: {e}"))
        }
    };
    let id = richiesta.get("id").cloned();
    let Some(metodo) = richiesta.get("method").and_then(|m| m.as_str()) else {
        return errore(id, -32600, "invalid-request", "manca `method`");
    };
    if richiesta.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return errore(id, -32600, "invalid-request", "`jsonrpc` deve essere \"2.0\"");
    }
    let params = richiesta.get("params").cloned().unwrap_or(serde_json::json!({}));

    match dispacci(store, metodo, &params) {
        Ok(result) => esito(id, result),
        Err(e) => {
            // -32601 è «metodo sconosciuto», -32602 «parametri sbagliati», e
            // tutto il resto è un rifiuto di una regola: il agente deve poter
            // distinguere «hai sbagliato a chiedere» da «la regola ha detto no».
            let (code, testo) = match &e {
                Error::MetodoSconosciuto { .. }
                | Error::RpcMalformata { .. }
                | Error::ArgomentoMancante { .. } => (-32601, e.to_string()),
                Error::DiagnosiAssente { .. } => (-32602, e.to_string()),
                _ => (-32000, e.to_string()),
            };
            errore(id, code, e.code(), &testo)
        }
    }
}

fn dispacci(
    store: &mut Store,
    metodo: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    match metodo {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": NOME, "version": env!("CARGO_PKG_VERSION") },
            "metodi": METODI,
        })),
        "kbs/argomento" => {
            let by = persona(params, "person")?;
            let id = argomento(params)?;
            let argomento = store.read_argument(&by, &id)?;
            Ok(serde_json::to_value(argomento)?)
        }
        "kbs/claim" => {
            let by = persona(params, "person")?;
            let id = argomento(params)?;
            let claim = store.claims_for(&by, &id)?;
            Ok(serde_json::to_value(claim)?)
        }
        "kbs/mancanti" => {
            let by = persona(params, "person")?;
            let corso = CourseId(testo(params, "course")?.to_string());
            let visibili = store.visible_arguments(&by, &corso, None)?;
            let mancanti: Vec<serde_json::Value> = visibili
                .iter()
                .filter(|a| !a.is_citable_now())
                .map(|a| {
                    let inv = kbs_core::check_citable(a).expect_err("filtrato sopra");
                    serde_json::json!({
                        "id": a.id,
                        "rel_path": a.rel_path,
                        "state": a.state,
                        "invariant": route::invariant_code(&inv),
                        "message": inv.to_string(),
                    })
                })
                .collect();
            Ok(serde_json::json!({ "course": corso, "mancanti": mancanti }))
        }
        "kbs/bozza" => {
            let by = persona(params, "person")?;
            let sorgente = testo(params, "source")?.to_string();
            let corso = testo(params, "course").ok().map(|c| CourseId(c.to_string()));
            let rel = testo(params, "rel_path").ok().map(str::to_string);
            let richiesta = Request {
                route: Route::Mcp,
                by,
                course: corso,
                rel_path: rel,
                source: sorgente,
            };
            let ricevuta = route::receive(store, richiesta)?;
            Ok(serde_json::to_value(ricevuta)?)
        }
        altro => Err(Error::MetodoSconosciuto {
            metodo: altro.to_string(),
            noti: "initialize, kbs/argomento, kbs/mancanti, kbs/bozza, kbs/claim",
        }),
    }
}

fn testo<'a>(params: &'a serde_json::Value, campo: &'static str) -> Result<&'a str> {
    params
        .get(campo)
        .and_then(|v| v.as_str())
        .ok_or(Error::ArgomentoMancante { metodo: "kbs", campo })
}

fn persona(params: &serde_json::Value, campo: &'static str) -> Result<PersonId> {
    Ok(PersonId(testo(params, campo)?.to_string()))
}

fn argomento(params: &serde_json::Value) -> Result<ArgumentId> {
    let grezzo = testo(params, "arg")?;
    // Si accetta sia `arg_…` sia il percorso relativo: un agente che ha letto un
    // file conosce il percorso, e chiedergli di fare l'hash da sé sarebbe una
    // fonte di id sbagliati che non serve a nessuno.
    if grezzo.starts_with("arg_") {
        Ok(ArgumentId(grezzo.to_string()))
    } else {
        Ok(ArgumentId::from_rel_path(grezzo))
    }
}

fn esito(id: Option<serde_json::Value>, result: serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn errore(
    id: Option<serde_json::Value>,
    code: i64,
    kib: &str,
    message: &str,
) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "data": { "kib_code": kib }, "message": message },
    })
    .to_string()
}

/// Il verdetto di una bozza, per un cliente che lo vuole ricalcolato.
///
/// Esposto perché la CLI e l'MCP devono poter produrre lo stesso verdetto e non
/// ne deve esistere due versioni: questo è l'unico.
pub fn verdetto_di(sorgente: &str) -> Verdict {
    crate::gate::rivedi(sorgente)
}

/// Il tempo di una diagnosi: un solo tipo nel sistema.
pub fn adesso() -> Millis {
    Millis::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[test]
    fn initialize_dichiara_il_protocollo_e_i_metodi() {
        let r = rispondi(&mut store(), r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(v["id"], 1);
    }

    #[test]
    fn un_metodo_sconosciuto_elenca_quelli_c_e() {
        let r = rispondi(&mut store(), r#"{"jsonrpc":"2.0","id":2,"method":"kbs/promuovi"}"#);
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["error"]["code"], -32601);
        assert!(v["error"]["data"]["kib_code"].as_str().unwrap().contains("metodo"));
        assert!(v["error"]["message"].as_str().unwrap().contains("kbs/bozza"));
    }

    #[test]
    fn una_riga_non_json_e_un_errore_di_parse_e_non_un_panico() {
        let r = rispondi(&mut store(), "{ questo non è json");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["error"]["code"], -32700);
    }

    #[test]
    fn un_parametro_mancante_e_nominato() {
        let r = rispondi(&mut store(), r#"{"jsonrpc":"2.0","id":3,"method":"kbs/argomento"}"#);
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["error"]["code"], -32601);
        assert!(v["error"]["message"].as_str().unwrap().contains("person"));
    }

    #[test]
    fn l_id_accetta_il_percorso_relativo_e_l_id() {
        let da_path = argomento(&serde_json::json!({ "arg": "a/b.html" })).unwrap();
        let da_id = argomento(&serde_json::json!({ "arg": da_path.to_string() })).unwrap();
        assert_eq!(da_path, da_id);
    }
}
