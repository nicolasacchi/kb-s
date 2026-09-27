// La ricerca.
//
// # Un risultato di ricerca è un indirizzo, non un documento
//
// `/api/v1/search` risponde `{hits: [{argument, course, rank}]}` e **niente
// altro**: nessun titolo, nessun brano, nessuna anteprima. Non è una
// semplificazione, è il design — l'indice dà un ordinamento e il predicato dà
// la visibilità, e metterci dentro il testo sarebbe un secondo canale da cui
// leggere ciò che il predicato ha negato.
//
// Quindi ogni hit viene **ripreso** con una seconda chiamata a
// `/api/v1/arguments/{id}`, che passa dal predicato. È il motivo per cui la
// ricerca fa N+1 richieste, ed è un prezzo che si paga: la risposta giusta
// costa più della risposta comoda.
//
// # `rank` più negativo è meglio
//
// È il `bm25` di FTS5 e si chiama `rank` e non `score` proprio perché il
// segno è nel nome, così nessuno lo riordina al contrario. L'ordine dell'array
// è quello dell'indice e qui non viene toccato: un secondo ordinamento
// diverge dal primo alla prima release.

import { el, riempi, sezione, tabella, riquadroErrore, nulla } from "../lib/dom.js";
import { cercaConTesto } from "../lib/api.js";

/**
 * La pagina di ricerca.
 *
 * @param {{nodo: HTMLElement, q: string}} contesto
 * @returns {Promise<void>}
 */
export async function ricerca({ nodo, q }) {
  const modulo = el("form", { class: "cerca" });
  const campo = el("input", {
    type: "search",
    name: "q",
    value: q ?? "",
    placeholder: "una parola del corso",
    "aria-label": "testo cercato",
  });
  modulo.append(campo, el("button", { type: "submit" }, ["Cerca"]));
  modulo.addEventListener("submit", (evento) => {
    evento.preventDefault();
    const cercato = campo.value.trim();
    window.location.hash = cercato === "" ? "#/cerca" : `#/cerca?q=${encodeURIComponent(cercato)}`;
  });

  riempi(
    nodo,
    sezione(
      "Cerca",
      modulo,
      el("p", { class: "nota" }, [
        "La ricerca restituisce solo l'id dell'argomento, il corso e il punteggio: nessun testo. Ogni risultato viene quindi riaperto con una seconda richiesta, e ciò che vedi è quello che il predicato lascia vedere alla persona che hai dichiarato di essere.",
      ]),
    ),
  );

  if (!q) {
    nodo.querySelector(".sezione").append(
      el("p", { class: "nulla" }, [
        "Nessuna ricerca fatta. Scrivi una parola: senza termini cercabili il server risponde 400, e lo dice invece di restituire un elenco vuoto che sembrerebbe una risposta vera.",
      ]),
    );
    return;
  }

  nodo.append(el("p", { class: "caricamento" }, ["Cerco…"]));
  let righe;
  try {
    righe = await cercaConTesto(q);
  } catch (errore) {
    riempi(nodo, sezione(`Risultati per «${q}»`, riquadroErrore(errore)));
    return;
  }

  if (righe.length === 0) {
    riempi(
      nodo,
      sezione(
        `Risultati per «${q}»`,
        nulla(
          "Nessun risultato. L'indice non ha niente che corrisponda a questa ricerca fra ciò che la persona dichiarata può vedere: i risultati di altri corsi e le bozze del docente non arrivano, e non c'è modo di sapere se ci sarebbero.",
        ),
      ),
    );
    return;
  }

  riempi(
    nodo,
    sezione(
      `Risultati per «${q}»`,
      tabella(
        ["Rank", "Argomento", "Corso", "Stato"],
        righe.map((riga) => [
          el("code", { class: "rank" }, [String(riga.rank)]),
          riga.assente
            ? el("span", { class: "attenzione" }, [
                "l'indice lo indica ma l'argomento non si può leggere",
              ])
            : el("a", { href: `#/argomento/${riga.argomento.id}` }, [riga.argomento.title]),
          riga.course,
          riga.assente
            ? el("span", { class: "nota" }, ["non leggibile"])
            : statoDi(riga.argomento.state),
        ]),
      ),
      el("p", { class: "nota" }, [
        `${righe.length} risultati, nell'ordine dell'indice. Rank più negativo è migliore: è il bm25 di FTS5 e non uno score da riordinare al contrario.`,
      ]),
    ),
  );
}

/** Lo stato di pubblicazione, detto come lo dice il dominio. */
function statoDi(stato) {
  switch (stato) {
    case "in-corso":
      return "in uso";
    case "del-docente":
      return "revisione del docente";
    case "bozza":
      return "bozza";
    case "archiviato":
      return "fuori uso, conservato";
    default:
      return stato;
  }
}
