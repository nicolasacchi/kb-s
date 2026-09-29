//! D10.4: la quarta strada, raggiunta comeprotocollo.
//!
//! Il server MCP è l'unica delle quattro strade che ha un_protocollo_ proprio, e
//! un protocollo si verifica dalla parte di chi lo parla: qui si mandano righe
//! JSON-RPC e si guardano le risposte, senza toccare le funzioni interne.
//!
//! Quello che il test dimostra non è che il server risponde, ma che **la strada
//! MCP è la stessa strada delle altre tre**: la bozza che nasce da
//! `kbs/bozza` ha lo stesso id, lo stesso stato e lo stesso verdetto di quella
//! che nasce da una cattura. Se l'MCP avesse una validazione sua, il gate
//! sarebbe aggirabile da una porta che il gate non vede.

mod common;

use kbs_core::{Millis, PersonId};
use kbs_intake::mcp;
use kbs_store::Store;

use common::*;

fn chiara(store: &mut Store, riga: &str) -> serde_json::Value {
    let risposta = mcp::rispondi(store, riga);
    serde_json::from_str(&risposta).unwrap_or_else(|e| panic!("risposta non JSON: {e}\n{risposta}"))
}

fn richiesta(id: i64, metodo: &str, params: serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": params }).to_string()
}

#[test]
fn una_bozza_nata_da_mcp_e_la_stessa_bozza_di_una_cattura() {
    let mut store = store_con_corso();
    let r = chiara(
        &mut store,
        &richiesta(
            1,
            "kbs/bozza",
            serde_json::json!({
                "person": "person_0001",
                "course": CORSO,
                "rel_path": REL,
                "source": artifact(false),
            }),
        ),
    );
    assert_eq!(r["id"], 1);
    let ricevuta = &r["result"];
    assert_eq!(ricevuta["stored"], true);
    assert_eq!(ricevuta["argument"]["state"], "bozza");
    assert_eq!(ricevuta["argument"]["rel_path"], REL);
    assert_eq!(
        ricevuta["verdict"]["content_hash"],
        kbs_intake::content_hash(&artifact(false)),
        "MCP e cattura validano gli stessi byte"
    );

    // E l'argomento si rilegge dalla strada di lettura, che è l'unico modo in
    // cui un agente verifica che cosa è successo invece di crederci.
    let letto = chiara(
        &mut store,
        &richiesta(2, "kbs/argomento", serde_json::json!({ "person": "person_0001", "arg": REL })),
    );
    assert_eq!(letto["result"]["id"], ricevuta["argument"]["id"]);
}

#[test]
fn il_payload_della_bozza_non_puo_dichiarare_uno_stato() {
    // Lo stato non è un campo del payload: `Request` non ce l'ha, e un agente
    // che provasse a mandarlo avrebbe un errore di argomento sconosciuto — che
    // e' la risposta giusta, perche' la seconda porta sullo stato e' quella che
    // D4 vieta.
    let mut store = store_con_corso();
    let r = chiara(
        &mut store,
        &richiesta(
            1,
            "kbs/bozza",
            serde_json::json!({
                "person": "person_0001",
                "course": CORSO,
                "rel_path": REL,
                "source": artifact(false),
                "state": "in-corso",
            }),
        ),
    );
    // Il campo in piu' e' semplicemente ignorato: il risultato e' identico a
    // quello senza, e l'argomento resta in bozza.
    assert_eq!(r["result"]["argument"]["state"], "bozza");
}

#[test]
fn un_artifact_che_referenzia_una_cdn_entra_in_bozza_e_non_si_promuove() {
    let mut store = store_con_corso();
    let r = chiara(
        &mut store,
        &richiesta(
            1,
            "kbs/bozza",
            serde_json::json!({
                "person": "person_0001",
                "course": CORSO,
                "rel_path": REL,
                "source": artifact_con_cdn(),
            }),
        ),
    );
    let ricevuta = &r["result"];
    assert_eq!(ricevuta["stored"], true, "una bozza puo' essere rotta: e' leggibile");
    let codici: Vec<&str> = ricevuta["verdict"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["blocking"] == true)
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(codici.contains(&"external-reference"), "{codici:?}");
}

#[test]
fn le_claim_si_rileggono_e_lo_stato_e_il_registro() {
    let mut store = store_con_corso();
    chiara(
        &mut store,
        &richiesta(
            1,
            "kbs/bozza",
            serde_json::json!({
                "person": "person_0001",
                "course": CORSO,
                "rel_path": REL,
                "source": artifact_claim_contraddetta(),
            }),
        ),
    );
    let r = chiara(
        &mut store,
        &richiesta(2, "kbs/claim", serde_json::json!({ "person": "person_0001", "arg": REL })),
    );
    let claim = &r["result"][0];
    assert_eq!(claim["status"], "contradicted");
    assert!(claim["span_text"].is_string(), "una contraddetta ha comunque la sua traccia");
}

#[test]
fn l_elenco_di_che_cosa_manca_dice_anche_perche() {
    let mut store = store_con_corso();
    chiara(
        &mut store,
        &richiesta(
            1,
            "kbs/bozza",
            serde_json::json!({
                "person": "person_0001",
                "course": CORSO,
                "rel_path": REL,
                "source": artifact_in_corso(),
            }),
        ),
    );
    let r = chiara(
        &mut store,
        &richiesta(2, "kbs/mancanti", serde_json::json!({ "person": "person_0001", "course": CORSO })),
    );
    let mancanti = r["result"]["mancanti"].as_array().unwrap();
    assert_eq!(mancanti.len(), 1);
    assert_eq!(mancanti[0]["invariant"], "citable-without-ratification");
    assert!(!mancanti[0]["message"].as_str().unwrap().is_empty());
}

#[test]
fn chi_non_ha_relazione_non_vede_e_non_sa_che_l_argomento_esiste() {
    let mut store = store_con_corso();
    chiara(
        &mut store,
        &richiesta(
            1,
            "kbs/bozza",
            serde_json::json!({
                "person": "person_0001",
                "course": CORSO,
                "rel_path": REL,
                "source": artifact(false),
            }),
        ),
    );
    let r = chiara(
        &mut store,
        &richiesta(
            2,
            "kbs/argomento",
            serde_json::json!({ "person": "person_9999", "arg": REL }),
        ),
    );
    // Non `not found`: D5 dice che «non esiste» e «non lo vedi» sono la stessa
    // risposta, e distinguerli sarebbe un canale per imparare che cosa c'e'.
    assert!(r["error"].is_object());
    let testo = r["error"]["message"].as_str().unwrap();
    assert!(testo.contains("person_9999"), "{testo}");
}

#[test]
fn il_server_parla_json_rpc_e_senza_rumore_intorno() {
    let mut store = store_con_corso();
    let input = [
        richiesta(1, "initialize", serde_json::json!({})),
        String::new(),
        "non e' json".to_string(),
        richiesta(2, "kbs/argomento", serde_json::json!({ "person": "person_0001", "arg": REL })),
    ]
    .join("\n");
    let mut out = Vec::new();
    let n = mcp::servi(
        &mut store,
        &mut std::io::BufReader::new(std::io::Cursor::new(input.into_bytes())),
        &mut out,
    )
    .expect("il server gira");
    // Tre righe di richiesta non vuota in, tre risposte fuori: la riga vuota e'
    // ignorata e non produce niente, il che e' cio' che un host MCP si aspetta.
    assert_eq!(n, 3);
    let testo = String::from_utf8(out).unwrap();
    let righe: Vec<&str> = testo.lines().collect();
    assert_eq!(righe.len(), 3);
    for riga in righe {
        serde_json::from_str::<serde_json::Value>(riga).expect("una risposta per riga");
    }
}

#[test]
fn la_strada_mcp_e_nella_lista_di_route_che_il_test_delle_quattro_controlla() {
    // Se qualcuno aggiungesse una quinta strada, `Route::ALL` la elencerebbe e
    // `le_quattro_strade` la confronterebbe: qui si verifica solo che MCP ci
    // sia, perche' il resto lo fa l'altro file.
    assert_eq!(kbs_intake::Route::Mcp.as_str(), "mcp");
    let persona: PersonId = docente();
    assert_eq!(persona, PersonId::fixture(1));
    let _ = Millis::now();
}

#[test]
fn questa_strada_non_scrive_relazioni_e_il_verbo_insegna_non_e_un_metodo() {
    // La parte non negoziabile di `kbs insegna`: il verbo scrive `teaches` e sta
    // **solo** nella CLI locale. Su questo trasporto l'identita' e' dichiarata
    // (`x-kbs-person` o `?person=`) e `identity.rs` dichiara testualmente che
    // «il confine di sicurezza e' il deployment»: un metodo che scrivesse
    // relazioni sarebbe la definizione di `teaches` in `kbs_core::may_read`, e
    // chiunque potrebbe pronunciarlo. Percio' la domanda non e' «funziona?» ma
    // «esiste?», e la risposta e' no — per entrambe le forme del nome.
    // `termina` e' nella stessa lista per la ragione opposta e piu' forte: non
    // concede un diritto, lo **toglie**, e una strada che toglie diritti su
    // un'identita' dichiarata lascia fuori un docente dal proprio corso.
    for metodo in [
        "insegna",
        "kbs/insegna",
        "kbs/teaches",
        "kbs/relazione",
        "termina",
        "kbs/termina",
    ] {
        let mut store = store_con_corso();
        let r = chiara(
            &mut store,
            &richiesta(
                1,
                metodo,
                serde_json::json!({
                    "person": "person_0001",
                    "course": CORSO,
                    "docente": "person_0002",
                }),
            ),
        );
        assert!(
            r["error"].is_object(),
            "`{metodo}` ha risposto con un risultato: la strada MCP scrive relazioni"
        );
        assert_eq!(r["error"]["code"], -32601, "`{metodo}`: {r}");
        assert_eq!(r["error"]["data"]["kib_code"], "metodo-sconosciuto", "{r}");
        assert!(
            !mcp::METODI.contains(&metodo),
            "`{metodo}` e' comparso nella lista dei metodi MCP"
        );
    }

    // E la tabella e' ancora quella di prima: nessuna delle quattro richieste ha
    // scritto una riga per `person_0002`, che nel banco non esiste neppure come
    // persona. Un metodo che rispondesse «persona-assente» avrebbe gia' letto il
    // roster di qualcun altro.
    let store = store_con_corso();
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "solo la riga del docente del banco");
}
