// I percorsi, in un posto solo.
//
// Il motivo di questo file è che i percorsi sono scritti **qui** e da nessun'
// altra parte, e il test `ogni_rotta_dell_interfaccia_e_una_rotta_del_server`
// legge *questo* file e interroga il router vero con i percorsi che ha trovato.
// Non è una tabella duplicata: è l'unica, ed è la stessa che il browser usa.
//
// Le rotte sono quelle di `src/routes/mod.rs::router`, non quelle di una
// documentazione: il percorso di un argomento è `/api/v1/arguments/{id}` e non
// `/api/v1/arguments?id=`, il corso sta in `/api/v1/courses/{corso}/…` e
// l'esportazione è `/api/v1/courses/{corso}/export`.

/** I modelli di percorso, uno per rotta, con i segnaposto dichiarati. */
export const ROTTE = {
  // L'unica rotta che non chiede relazioni: serve anche a un monitoraggio.
  salute: "/api/v1/health",

  // Lettura del percorso **pubblicato**: ciò che la persona dichiarata vede.
  argomenti: "/api/v1/courses/{corso}/arguments",
  argomento: "/api/v1/arguments/{id}",
  claim: "/api/v1/arguments/{id}/claims",

  // Coda di ratifica (D4). Le tre scritte sono gesti distinti e la porta li
  // tiene distinti: senza ratifica, `pubblica` risponde `409` e nomina `D4`.
  coda: "/api/v1/courses/{corso}/queue",
  ratifica: "/api/v1/courses/{corso}/queue/{id}/ratify",
  pubblica: "/api/v1/courses/{corso}/queue/{id}/publish",
  ritira: "/api/v1/courses/{corso}/queue/{id}/withdraw",

  // I registri (D6). `person` è opzionale e vale l'identità dichiarata.
  osservazioni: "/api/v1/arguments/{id}/observations",
  giudizi: "/api/v1/courses/{corso}/gradings",

  // Coorte (D9). Sotto soglia risponde `{"signals": []}` e non un altro corpo.
  coorte: "/api/v1/arguments/{id}/cohort",

  // Calendario e metro (idee 11 e 12). Il calendario è la coda di richiamo del
  // docente; il metro è il rendiconto di quella coda, e la sua quota è
  // l'aggregato di D9: sotto soglia risponde `{"quota": null}`.
  calendario: "/api/v1/courses/{corso}/calendario",
  padronanza: "/api/v1/courses/{corso}/padronanza",
  quotaPadronanza: "/api/v1/courses/{corso}/padronanza/quota",

  // Ricerca: restituisce `{argument, course, rank}` e **nessun testo** (vedi
  // `api.js`, che è costretto a riprendere l'argomento con una seconda chiamata).
  ricerca: "/api/v1/search",

  // Esportazione a colonne fisse (D12). Va chiamata con `scarica`, non con
  // `fetch`: il browser deve poterla salvare come file.
  export: "/api/v1/courses/{corso}/export",

  // Eventi: partizionati per corso, e il canale del corpo docente lo sceglie
  // il server in base alle relazioni, non la query string.
  eventi: "/api/v1/courses/{corso}/events",

  // three.js vendorizzato (D15). Nessun CDN: la scuola può non avere rete.
  three: "/three/three.module.min.js",
};

/**
 * Riempi i segnaposto di un percorso.
 *
 * Un segnaposto che non trova il suo valore è un errore, non una stringa
 * vuota: `percorso("argomento", {})` deve interrompere chi chiama, perché
 * l'URL che ne uscirebbe sarebbe `/api/v1/arguments/undefined` e il server
 * risponderebbe `404` — cioè la risposta che *non distingue* — e il difetto
 * sparirebbe dentro un'assenza legittima.
 *
 * @param {string} nome una chiave di {@link ROTTE}
 * @param {Record<string, string>} valori i valori dei segnaposto
 * @returns {string} il percorso, con ogni valore percent-encoded
 */
export function percorso(nome, valori = {}) {
  const modello = ROTTE[nome];
  if (!modello) {
    throw new Error(`rotta sconosciuta: ${nome}`);
  }
  return modello.replace(/\{(\w+)\}/g, (_, chiave) => {
    const valore = valori[chiave];
    if (valore === undefined || valore === null || valore === "") {
      throw new Error(`manca il segnaposto {${chiave}} della rotta ${nome}`);
    }
    return encodeURIComponent(String(valore));
  });
}

/**
 * Il percorso di una rotta con la query string, saltando i valori vuoti.
 *
 * Saltarli è una cortesia, non una cancellazione: un `?state=` vuoto è una
 * domanda diversa da nessuna domanda, e mandarlo sarebbe dire al server
 * «filtra per lo stato con il nome vuoto».
 *
 * @param {string} nome una chiave di {@link ROTTE}
 * @param {Record<string, string>} valori i valori dei segnaposto
 * @param {Record<string, string|number|undefined|null>} query i parametri
 * @returns {string}
 */
export function percorsoConQuery(nome, valori, query = {}) {
  const base = percorso(nome, valori);
  const coppie = Object.entries(query).filter(
    ([, v]) => v !== undefined && v !== null && v !== "",
  );
  if (coppie.length === 0) {
    return base;
  }
  const qs = coppie
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`)
    .join("&");
  return `${base}?${qs}`;
}
