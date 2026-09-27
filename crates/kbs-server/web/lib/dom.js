// Quattro costruttori di elementi. Non è un framework: è `document.createElement`
// con gli attributi già messi, e serve perché scrivere `el("p", {}, [testo(...)])`
// è più corto di `document.createElement` più quattro righe, per quattro volte
// in una pagina.
//
// Il `testo()` prende un valore e ne fa un nodo di testo, e non un `innerHTML`:
// nessuna risposta del server entra mai nel DOM come HTML, perché nessuna
// risposta di questo server è fidata quanto il suo HTML. Un titolo che
// contiene `<script>` è un titolo, non uno script.

/**
 * Un elemento.
 *
 * @param {string} tag il nome del tag
 * @param {Record<string, any>} [attributi] `class`, `id`, `href`, `onclick`, …
 *   La chiave `class` è quella che si scrive; `dataset` mette le `data-*`.
 * @param {Array<Node|string>} [figli] il contenuto
 * @returns {HTMLElement}
 */
export function el(tag, attributi = {}, figli = []) {
  const nodo = document.createElement(tag);
  for (const [chiave, valore] of Object.entries(attributi)) {
    if (valore === undefined || valore === null) {
      continue;
    }
    if (chiave === "dataset") {
      for (const [k, v] of Object.entries(valore)) {
        nodo.dataset[k] = v;
      }
    } else if (chiave.startsWith("on") && typeof valore === "function") {
      nodo.addEventListener(chiave.slice(2), valore);
    } else if (chiave === "class") {
      nodo.className = valore;
    } else if (valore === true) {
      nodo.setAttribute(chiave, "");
    } else {
      nodo.setAttribute(chiave, String(valore));
    }
  }
  for (const figlio of figli.flat()) {
    if (figlio === undefined || figlio === null) {
      continue;
    }
    nodo.append(figlio instanceof Node ? figlio : document.createTextNode(String(figlio)));
  }
  return nodo;
}

/** Un nodo di testo. */
export function testo(valore) {
  return document.createTextNode(valore === null || valore === undefined ? "" : String(valore));
}

/** Svuota un contenitore e ci mette dentro altro. */
export function riempi(contenitore, ...figli) {
  contenitore.replaceChildren(...figli.flat().filter((n) => n !== null && n !== undefined));
  return contenitore;
}

/** Una sezione con un titolo di secondo livello. */
export function sezione(titolo, ...figli) {
  return el("section", { class: "sezione" }, [el("h2", {}, [titolo]), ...figli]);
}

/**
 * Una tabella con le sue intestazioni.
 *
 * `@param {string[]} intestazioni` i titoli di colonna
 * @param {Array<Array<Node|string>>} righe le celle, in ordine
 * @param {Array<{class?: string, title?: string}>} [opzioni]` per le colonne
 */
export function tabella(intestazioni, righe, opzioni = {}) {
  const colonne = opzioni.colonne ?? intestazioni.map(() => ({}));
  return el("table", { class: "tabella" }, [
    el("thead", {}, [
      el(
        "tr",
        {},
        intestazioni.map((t, i) => el("th", { scope: "col" }, [t])),
      ),
    ]),
    el(
      "tbody",
      {},
      righe.map((riga) =>
        el(
          "tr",
          {},
          riga.map((cella, i) =>
            el("td", { class: colonne[i]?.class }, [cella instanceof Node ? cella : testo(cella)]),
          ),
        ),
      ),
    ),
  ]);
}

/** Il riquadro di un errore, con la regola che ha detto no quando l'ha detta. */
export function riquadroErrore(errore) {
  const figli = [el("strong", {}, ["Non è andata a buon fine."]), testo(" "), testo(errore.message)];
  if (errore.regola) {
    figli.push(
      testo(" "),
      el("span", { class: "regola" }, [`regola: ${errore.regola}`]),
    );
  }
  if (errore.tipo === "assente") {
    figli.push(
      el("p", { class: "nota" }, [
        "Il server risponde la stessa cosa per un argomento che non esiste e per uno che non ti è permesso leggere. Non è una scioltezza dell'interfaccia: è la risposta, ed è la stessa in entrambi i casi.",
      ]),
    );
  }
  return el("div", { class: `errore errore-${errore.tipo}`, role: "alert" }, figli);
}

/** Una riga «non c'è niente da mostrare», detta per esteso. */
export function nulla(messaggio) {
  return el("p", { class: "nulla" }, [messaggio]);
}
