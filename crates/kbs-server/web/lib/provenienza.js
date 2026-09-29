// La provenienza: chi ha scritto, con quale modello e quale hash.
//
// # Perché questo file esiste
//
// Tre pagine avevano tre copie di questa cella, in tre punti diversi, e nessun
// test le confrontava — perché sono pezzi di HTML generati in tre punti
// diversi, e un confronto fra tre stringhe incollate è un test che si scrive
// dopo che il difetto è già successo. Le copie hanno quindi cominciato a
// divergere senza che nessuno lo vedesse, e la divergenza che si è aperta non
// è una parola cambiata: **`coda.js` non aveva il ramo `derived`**, e un
// argomento derivato in coda usciva come `derivata (derived)` — il nome
// dell'enum di Rust, in chiaro — mentre lo stesso argomento nell'elenco del
// corso usciva come `derivata da <link>`. Un nome di enuma in una cella che un
// docente guarda per decidere se ratificare è un buco, non una semplificazione.
//
// Il ramo che mancava è anche quello più economico da perdere in silenzio:
// `Derived` è l'unica variante che **non ha `by`**, e la coda non usa `by` per
// decidere chi può ratificare (`queue.rs::è_autore` restituisce `false` per
// `Derived` e lo dichiara), quindi nessun'altra parte di quella pagina avrebbe
// notato che il ramo era sparito.
//
// # Due versioni, non due copie
//
// `provenienzaCorta` è la cella degli elenchi — coda e corso — e
// `provenienzaLunga` è la riga del lettore, che mostra per intero il `model
// lock`. Sono due domande diverse, non due maniere di scrivere la stessa: in un
// elenco la domanda è «di chi è», e l'hash del prompt sta nella pagina
// dell'argomento, che è il posto dove lo si verifica. Non diventano una perché
// unire le due produrrebbe una riga di tabella alta sei righe in una coda che
// si legge a colpo d'occhio.
//
// Restano però **nello stesso file**, che è il punto: una variante nuova di
// `kbs_core::Origin` deve avere un ramo in entrambe, e le due guardie in
// `tests/provenienza.rs` lo pretendono. Le `kind` da cui non si può sfuggire
// sono quelle dell'enum, e l'enum le ha scelte una volta sola.
//
// # Il ripiego non inventa
//
// Un `kind` che questa funzione non conosce esce **come se stesso**, nudo. Non
// dice «derivata (qualcosa)» e non dice «generata»: dire which è la parola che
// non si sa sarebbe dire il falso su un argomento di cui non si sa nulla. Il
// lettore e l'elenco fanno la stessa cosa, ed è la ragione per cui il ripiego
// non può essere una cella diversa in un posto diverso.

import { el } from "./dom.js";

/**
 * La provenienza breve: di chi è, senza gli hash.
 *
 * Va nelle celle di elenco, dove una riga alta sei righe non si legge.
 *
 * @param {Record<string, any>} origin `argument.origin`, come la manda la rotta
 * @returns {HTMLElement}
 */
export function provenienzaCorta(origin) {
  switch (origin.kind) {
    case "human":
      return el("span", {}, ["scritta a mano da ", el("code", {}, [origin.by])]);
    case "generated":
      return el("span", {}, [
        "generata da ",
        el("code", {}, [origin.lock.model_id]),
        " (fuori dal prodotto), registrata da ",
        el("code", {}, [origin.by]),
      ]);
    case "derived":
      return el("span", {}, [
        "derivata da ",
        el("a", { href: `#/argomento/${origin.from}` }, [origin.from]),
      ]);
    default:
      // Il `kind` nudo: nessuna parola che non sia verificata.
      return el("span", {}, [origin.kind]);
  }
}

/**
 * La provenienza per intero: chi ha scritto, con quale modello e con quali
 * hash (D10).
 *
 * Va nella pagina dell'argomento, che è il posto dove l'hash del prompt si
 * verifica — e il posto dove una tabella di hash ha senso.
 *
 * @param {Record<string, any>} origin `argument.origin`, come la manda la rotta
 * @returns {HTMLElement}
 */
export function provenienzaLunga(origin) {
  switch (origin.kind) {
    case "human":
      return el("span", {}, [
        "scritto a mano da ",
        el("code", {}, [origin.by]),
        ` il ${giorno(origin.at)} — nessun modello coinvolto`,
      ]);
    case "generated":
      return el("div", { class: "provenienza" }, [
        el("span", {}, [
          "generato fuori dal prodotto (D3) da ",
          el("code", {}, [origin.lock.model_id]),
          ", poi registrato da ",
          el("code", {}, [origin.by]),
        ]),
        el("dl", { class: "model-lock" }, [
          el("dt", {}, ["hash del prompt"]),
          el("dd", {}, [el("code", {}, [origin.lock.prompt_hash])]),
          el("dt", {}, ["hash del corpus"]),
          el("dd", {}, [el("code", {}, [origin.lock.corpus_hash])]),
          el("dt", {}, ["versione del generatore"]),
          el("dd", {}, [el("code", {}, [origin.lock.generator_version])]),
        ]),
      ]);
    case "derived":
      return el("span", {}, [
        "derivato da ",
        el("a", { href: `#/argomento/${origin.from}` }, [origin.from]),
      ]);
    default:
      return origin.kind;
  }
}

/** La data, in secondi e senza fuso: le righe si confrontano per contenuto. */
function giorno(millis) {
  return new Date(millis).toISOString().slice(0, 10);
}
