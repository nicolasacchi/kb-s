// La coda di ratifica: la pagina che un docente apre per decidere.
//
// # Perché la pubblicazione è un bottone che si può premere e che rifiuta
//
// D4 dice che la strada condivisa passa da una ratifica, e che un cancello
// attraversato per abitudine è un cancello decorativo. Un bottone disabilitato
// su ogni riga non ratificata sarebbe esattamente un cancello decorativo: il
// docente non vedrebbe mai la regola, la imparerebbe come un limite
// dell'interfaccia e non come una regola del dominio. Quindi il bottone **resta
// premibile**, dichiara nella sua etichetta che verrà rifiutato, e quando il
// server risponde `409` la pagina mostra il `409` con la regola che lo ha detto
// (`D4`) e il motivo.
//
// # Dopo una ratifica si rilegge, non si presume
//
// Il `200` di `ratifica` è la ratifica **installata**. Non è la coda, non è lo
// stato dell'argomento, e soprattutto non dice che l'argomento sia entrato in
// uso: entrare in uso è un secondo atto, e la porta li tiene distinti. Quindi
// dopo ogni scrittura questa pagina **rilascia la coda e la ricarica**, e
// quello che mostra è quello che il server dice adesso. Una riga che si sposta
// da sola dopo una ratifica sarebbe un'assunzione travestita da risultato.
//
// # La coda contiene solo lavori non finiti
//
// Il server mette in coda gli argomenti in stato mutabile — bozza e revisione
// del docente. Un argomento già in uso o archiviato non «aspetta» nessuno, e
// non lo si vede qui.

import { el, testo, riempi, sezione, tabella, riquadroErrore, nulla } from "../lib/dom.js";
import { get, post, persona } from "../lib/api.js";
import { provenienzaCorta } from "../lib/provenienza.js";

/**
 * La coda di ratifica di un corso.
 *
 * @param {{nodo: HTMLElement, corso: string}} contesto
 * @returns {Promise<{rileggi: () => Promise<void>}>}
 */
export async function coda({ nodo, corso }) {
  const rileggi = async () => {
    riempi(nodo, el("p", { class: "caricamento" }, ["Carico la coda…"]));
    let risposta;
    try {
      risposta = await get("coda", { corso });
    } catch (errore) {
      riempi(nodo, [
        sezione(`Coda di ratifica · ${corso}`, riquadroErrore(errore)),
        spiegazionePer(errore),
      ]);
      return;
    }
    const voci = risposta.queue ?? [];
    riempi(nodo, [
      sezione(
        `Coda di ratifica · ${corso}`,
        el("p", { class: "nota" }, [
          "Ciò che aspetta una decisione. Solo argomenti in stato mutabile: un argomento già in uso non aspetta nessuno.",
        ]),
        voci.length === 0
          ? nulla(
              "La coda è vuota: in questo corso non c'è nessun argomento che aspetti una decisione. Non è un elenco nascosto — è un elenco vuoto, e la coda è la verità su che cosa manca.",
            )
          : tabella(
              ["Argomento", "Provenienza", "Che cosa manca", "Azioni"],
              voci.map((voce) => [
                cellaArgomento(voce.argument),
                provenienzaCorta(voce.argument.origin),
                cellaCheCosaMancano(voce),
                cellaAzioni(corso, voce, rileggi),
              ]),
            ),
      ),
      sezione(
        "Che cosa vuol dire ogni gesto",
        tabella(
          ["Gesto", "Che cosa dichiara", "Se non è possibile"],
          [
            [
              "ratifica",
              "che il docente ha verificato il contenuto, e su quale hash vale",
              el("span", {}, [
                "400 se la nota è vuota: una ratifica senza nota è una responsabilità senza soggetto",
              ]),
            ],
            [
              "pubblica",
              "che il materiale entra in uso",
              el("span", {}, [
                "409, regola D4, senza una ratifica valida per il contenuto corrente",
              ]),
            ],
            [
              "ritira",
              "che l'argomento torna non citabile senza che nessuno ne modifichi il testo",
              "404 se non c'è una ratifica da ritirare",
            ],
          ],
          { colonne: [{}, {}, { class: "col-stato" }] },
        ),
      ),
    ]);
  };

  await rileggi();
  return { rileggi };
}

/** La cella dell'argomento: titolo, stato e link al lettore. */
function cellaArgomento(argomento) {
  return el("div", {}, [
    el("a", { href: `#/argomento/${argomento.id}` }, [argomento.title]),
    el("div", { class: "claim-id" }, [el("code", {}, [argomento.id])]),
    el("div", { class: "stato-riga" }, [argomento.state]),
  ]);
}

// La provenienza non è qui: è in `../lib/provenienza.js`, che la disegna per
// tutte e tre le pagine che la mostrano. Qui la coda aveva una copia sua che
// aveva perso il ramo `derived` — e un argomento derivato in coda usciva come
// `derivata (derived)`, il nome dell'enum di Rust in chiaro, senza il link alla
// sorgente che l'elenco del corso mostrava. La copia non era un dettaglio:
// `Derived` è l'unica variante che non ha `by`, e questa pagina non usa `by` per
// decidere chi può ratificare, quindi nessun'altra riga avrebbe notato che il
// ramo era sparito.

/**
 * Che cosa manca, detto con le parole del server.
 *
 * `needs_ratification` e `needs_publication` arrivano dalla coda e non sono
 * ricalcolati qui: se la pagina avesse un secondo criterio, la coda direbbe una
 * cosa e la porta direzione un'altra.
 */
function cellaCheCosaMancano(voce) {
  const a = voce.argument;
  if (voce.needs_ratification) {
    return el("span", { class: "attenzione" }, [
      "manca la ratifica",
      el("div", { class: "motivo" }, [
        a.ratified
          ? "quella che c'è è invecchiata rispetto al contenuto corrente: vale per un testo che è cambiato, quindi non vale più per questo"
          : "non c'è mai stata",
      ]),
    ]);
  }
  return el("span", { class: "ok" }, [
    "ratificata; manca la pubblicazione",
    el("div", { class: "motivo" }, [
      "la ratifica è valida per l'hash corrente, ma l'argomento non è ancora in uso",
    ]),
  ]);
}

/** I tre gesti, uno per riga, e nessuno disabilitato. */
function cellaAzioni(corso, voce, rileggi) {
  const id = voce.argument.id;
  const esito = el("div", { class: "esito" });

  const campoNota = el("input", {
    type: "text",
    class: "nota-ratifica",
    placeholder: "che cosa hai verificato",
    "aria-label": `nota di ratifica per ${voce.argument.title}`,
  });

  const ratifica = el("button", { type: "button" }, ["Ratifica"]);
  const pubblica = el("button", { type: "button" }, [
    voce.needs_ratification
      ? "Pubblica (verrà rifiutato: senza ratifica)"
      : "Pubblica",
  ]);
  const ritira = voce.needs_publication
    ? el("button", { type: "button" }, ["Ritira la ratifica"])
    : null;

  const esegui = async (azione, etichetta) => {
    riempi(esito, el("span", { class: "in-corso" }, [`${etichetta}…`]));
    try {
      if (azione === "ratifica") {
        const risposta = await post("ratifica", { corso, id }, { note: campoNota.value });
        riempi(esito, [
          el("span", { class: "ok" }, [
            `ratifica installata da ${risposta.ratification.by}`,
          ]),
          el("div", { class: "motivo" }, [
            `vale per l'hash ${risposta.ratification.contract_hash}: la nota dice «${risposta.ratification.note}»`,
          ]),
          el("div", { class: "motivo" }, [
            "rilascio la coda e la rileggo: quello che vedo adesso è quello che il server dice adesso.",
          ]),
        ]);
      } else if (azione === "pubblica") {
        const risposta = await post("pubblica", { corso, id });
        riempi(esito, el("span", { class: "ok" }, [`ora è ${risposta.state}`]));
      } else {
        await post("ritira", { corso, id });
        riempi(esito, el("span", { class: "ok" }, [
          "ratifica ritirata: l'argomento non è più citabile",
        ]));
      }
    } catch (errore) {
      riempi(esito, riquadroErrore(errore));
      if (errore.tipo === "regola") {
        riempi(
          esito,
          riquadroErrore(errore),
          el("p", { class: "nota" }, [
            "Questa è la regola di dominio che ha detto no, resa visibile dal protocollo. Il rifiuto non è un errore dell'interfaccia: è D4.",
          ]),
        );
      }
    }
    // Dopo **qualsiasi** esito — riuscito o rifiutato — la coda viene
    // riletta. Un `200` di `pubblica` non dice che l'argomento sia in uso
    // *adesso*, e un `409` non cambia nulla: in entrambi i casi l'unica cosa
    // che l'interfaccia sa è ciò che il server dice adesso, e presumere il
    // seguito sarebbe inventare.
    await rileggi();
  };

  ratifica.addEventListener("click", () => esegui("ratifica", "ratifico"));
  pubblica.addEventListener("click", () => esegui("pubblica", "pubblico"));
  ritira?.addEventListener("click", () => esegui("ritira", "ritiro"));

  return el("div", { class: "azioni" }, [
    campoNota,
    el("div", { class: "bottoni" }, [ratifica, pubblica, ritira].filter(Boolean)),
    esito,
  ]);
}

/**
 * La spiegazione di un `404` sulla coda, che è una cosa precisa.
 *
 * Un `404` sulla coda non è un corso inesistente né un errore: è «non insegni
 * questo corso» o «non c'è». La pagina non può scegliere fra le due e non finge
 * di saperlo.
 */
function spiegazionePer(errore) {
  if (errore.tipo !== "assente") {
    return null;
  }
  return sezione(
    "Perché la coda non si vede",
    el("p", {}, [
      "La coda è del docente che insegna il corso. Il server risponde la stessa cosa per un corso che non esiste e per un corso di cui non insegni, e non c'è modo di sapere quale dei due sia.",
    ]),
    el("p", { class: "nota" }, [
      persona()
        ? `Hai dichiarato ${persona()}. La dichiarazione non è un'autenticazione: ciò che conta è la relazione che il docente ha registrato per questa persona in questo corso.`
        : "Non hai dichiarato nessuna identità: senza dichiarazione il server risponde 401.",
    ]),
  );
}
