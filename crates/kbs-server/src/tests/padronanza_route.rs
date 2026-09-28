//! I test di protocollo della rotta del metro della padronanza.
//!
//! Qui vivono i test che passano dal **router** e non dal `Store`: la promessa
//! di una rotta e' una promessa di protocollo, e cio' che si prova e' il numero
//! che arriva a chi naviga. Un `Store` che rispondesse `Ok` dimostrerebbe la
//! meta' della strada.
//!
//! (Spostati qui da `routes/padronanza.rs`, dove un test da router non e' la
//! convenzione del repository: i test HTTP stanno in `src/tests/*.rs`, quelli di
//! route sono unita' pure.)

// Niente `use super::*`: il test che vive qui dentro usa solo `Scuola`, che
// si importa sotto, e il glob del modulo padre non porta nient'altro che
// nomi che questo file non tocca. Un import che non serve in un test e'
// l'avviso che il test e' stato spostato e non riadattato.

/// Il caso falsificante che il commento di sopra scambiava per un difetto:
/// **un docente che chiede il metro di una persona che non esiste**.
///
/// Il test passa dal router e non dal `Store` perché la promessa di questa
/// rotta è una promessa di protocollo: quello che si prova è il numero che
/// arriva a chi naviga, e `404` contro `200` è un numero. Un `Store` che
/// rispondesse `Ok` dimostrerebbe la metà della strada.
///
/// Il caso è chiuso, non ipotetico: `persona_0009` è proprio l'id che
/// `la_persona_mancante_vale_l_identita_dichiarata` mostra entrare senza
/// essere riscritto, e il prefisso `person_` che `fixture(n)` usa è una
/// convenzione, non uno spazio di nomi. Quindi l'id «inesistente» qui è solo
/// una persona che nessuno ha mai registrato — e se domani lo registrasse,
/// questa stessa richiesta restituirebbe il suo metro vero. È per questo che
/// la risposta giusta non è `404`: un `404` qui sarebbe un oracolo di
/// esistenza su uno spazio di nomi arbitrario, e `identity.rs` dichiara la
/// regola da cui discende tutto il resto — la dichiarazione si verifica
/// solo nella **forma**, mai nell'esistenza.
#[tokio::test]
async fn il_docente_legge_anche_il_metro_di_una_persona_inesistente() {
    use super::Scuola;

    let mut scuola = Scuola::nuova();
    scuola.pubblicato(&scuola.corso.clone(), &scuola.docente.clone(), 1);
    // `at` è fissato nelle due richieste: senza, `Millis::now()` le farebbe
    // differire nel campo `at` e il confronto fra i due corpi misurerebbe
    // l'orologio invece dell'esistenza della persona.
    let at = 1_700_000_000_000i64;
    let uri = |persona: &str| {
        format!(
            "/api/v1/courses/{}/padronanza?person={persona}&at={at}",
            scuola.corso.as_str()
        )
    };
    let inesistente = uri("persona_0009");

    let risposta = scuola.get(&inesistente, &scuola.docente).await;
    assert_eq!(
        risposta.status(),
        axum::http::StatusCode::OK,
        "un docente che insegna il corso legge il metro che chiede, anche se \
         la persona richiesta non è mai stata registrata: `404` qui \
         diventerebbe un oracolo di esistenza"
    );
    let corpo = Scuola::testo(risposta).await;
    let risposta_json: serde_json::Value =
        serde_json::from_str(&corpo).expect("corpo json");
    assert_eq!(risposta_json["student"], "persona_0009", "{corpo}");
    let righe = risposta_json["rows"].as_array().expect("righe");
    assert!(
        !righe.is_empty(),
        "un corso con un argomento pubblicato ha almeno una riga: {corpo}"
    );
    for riga in righe {
        // Il verdetto è `no_proof` e le prove sono vuote: la risposta è
        // quella di uno studente che non ha mai lavorato, non quella di uno
        // studente che non esiste. Il prefisso non compare in nessun campo.
        assert_eq!(
            riga["verdict"],
            serde_json::json!({"not_proven": "no_proof"}),
            "{corpo}"
        );
        assert_eq!(riga["proofs"], serde_json::json!([]), "{corpo}");
    }

    // La differenza che il corpo **non** fa: lo studente iscritto, che non
    // ha mai lavorato, riceve la stessa risposta. Se le due si distinguessero
    // il `404` che non c'è avrebbe un altro nome, e la risposta sarebbe
    // l'oracolo che questo crate non apre. Il confronto è sul corpo intero,
    // `at` fissato, con l'id sostituito: due risposte diverse byte per byte.
    let del_iscritto = scuola.get(&uri(scuola.studente.as_str()), &scuola.docente).await;
    assert_eq!(del_iscritto.status(), axum::http::StatusCode::OK);
    let del_iscritto = Scuola::testo(del_iscritto).await;
    let senza_id = |c: &str| c.replace("persona_0009", "@").replace(
        scuola.studente.as_str(),
        "@",
    );
    assert_eq!(
        senza_id(&corpo),
        senza_id(&del_iscritto),
        "una persona che non esiste e uno studente che non ha mai lavorato \
         ricevono la stessa risposta: è la regola di `identity.rs`, e un \
         corpo che li distingue la tradirebbe"
    );

    // E il `404` c'è, dove deve: chi non è lo studente e non insegna non
    // legge il metro di nessuno, e riceve la risposta che riceverebbe per un
    // corso inesistente. È la relazione di **chi chiede** a decidere, non
    // l'esistenza di chi è nominato.
    let estranea = scuola.get(&inesistente, &scuola.estraneo).await;
    assert_eq!(
        estranea.status(),
        axum::http::StatusCode::NOT_FOUND,
        "il `404` è per chi non ha relazione col corso, non per la persona \
         nominata nella query"
    );
}

