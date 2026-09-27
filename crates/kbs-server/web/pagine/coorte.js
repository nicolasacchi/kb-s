// I segnali di coorte — e il posto in cui una pagina può mentire per bene.
//
// # Sotto soglia non c'è un numero piccolo: non c'è niente  (D9)
//
// Il server risponde `{"argument": "…", "signals": []}` e non c'è un corpo più
// corto di quello. Nessun `failing: 0`, nessun flag `soppresso`, nessun campo
// che dica «c'è qualcosa che non ti mostriamo»: quel campo sarebbe un canale —
// uno studente che lo vede saprebbe che su quell'argomento qualcuno ha sbagliato,
// che è metà dell'informazione che la soglia protegge.
//
// Quindi qui **non** c'è uno `0`, **non** c'è un trattino, **non** c'è uno
// «nessun dato» che sembri un dato. C'è una frase che dice che non c'è niente da
// mostrare, e la ragione per cui non lo si può sapere. Il motivo è dichiarato
// per intero perché la soglia è un accesso, non una cancellazione: il dato
// individuale esiste, e questa pagina non lo mostra perché sotto `k` non è un
// segnale.
//
// # Chi vede questa pagina
//
// Chi insegna il corso. Il segnale di coorte è una domanda del docente — «dove
// cade la classe?» — e non è un dato della classe da mostrare alla classe.

import { el, riempi, sezione, tabella, riquadroErrore } from "../lib/dom.js";
import { get, persona } from "../lib/api.js";

/**
 * La pagina dei segnali di coorte di un argomento.
 *
 * @param {{nodo: HTMLElement, id: string}} contesto
 * @returns {Promise<void>}
 */
export async function coorte({ nodo, id }) {
  riempi(nodo, el("p", { class: "caricamento" }, ["Carico i segnali…"]));
  let risposta;
  try {
    risposta = await get("coorte", { id });
  } catch (errore) {
    riempi(nodo, sezione(`Coorte · ${id}`, riquadroErrore(errore), spiegazione(errore)));
    return;
  }

  const segnali = risposta.signals ?? [];
  riempi(
    nodo,
    sezione(
      `Coorte · ${id}`,
      segnali.length === 0 ? nullaSenzaSoglia(id) : tabellaSegnali(segnali),
      el("p", { class: "nota" }, [
        "I segnali sono aggregati e anonimi. Sotto la soglia non viene prodotto alcun segnale: la risposta è un elenco vuoto, e la pagina lo dice — non lo trasforma in uno zero, perché uno zero è un'affermazione che nessuno ha potuto verificare.",
      ]),
    ),
  );
}

/**
 * L'elenco vuoto, detto per esteso.
 *
 * Qui non compare nessun numero. Non è una dimenticanza: è la regola. Un
 * «0 studenti sbagliano» sarebbe un dato — e sarebbe falso, perché sotto soglia
 * il dato non è zero, è **assente**.
 */
function nullaSenzaSoglia(id) {
  return el("div", { class: "vuoto-senza-soglia" }, [
    el("p", { class: "nulla" }, [
      "Nessun segnale di coorte su questo argomento. Non è che gli studenti non sbaglino: è che sotto la soglia di D9 un segnale non esiste, e quindi non c'è nulla da mostrare — non uno zero, non un segnaposto, non una riga con un trattino.",
    ]),
    el("p", { class: "nota" }, [
      "La soglia è un accesso, non una cancellazione: il dato individuale delle osservazioni esiste e si legge nei registri. Quello che qui non esiste è l'aggregato, perché con un numero piccolo chi legge trova i nomi conoscendo i propri studenti. E non c'è modo di sapere se sotto soglia ci sarebbe stato un segnale: saperlo sarebbe già metà dell'informazione.",
    ]),
  ]);
}

/** Il segnale, quando esiste, con le sue cifre. */
function tabellaSegnali(segnali) {
  return tabella(
    ["Coorte", "Chi sbaglia", "Sulla classe", "Quando"],
    segnali.map((s) => [
      s.cohort,
      el("strong", {}, [String(s.failing)]),
      `su ${s.total}`,
      new Date(s.at).toISOString().slice(0, 10),
    ]),
    { colonne: [{}, { class: "col-stato" }, {}, {}] },
  );
}

/** Perché la pagina non si vede, quando non si vede. */
function spiegazione(errore) {
  if (errore.tipo !== "assente") {
    return null;
  }
  return el("p", { class: "nota" }, [
    "I segnali di coorte sono del docente che insegna il corso. Il server risponde la stessa cosa per un argomento che non esiste e per uno su cui non insegni, e non distingue i due casi.",
    persona() ? ` Hai dichiarato ${persona()}, e la dichiarazione non è un'autenticazione.` : "",
  ]);
}
