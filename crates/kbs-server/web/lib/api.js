// Il parlare col server. L'unico file che usa `fetch`.
//
// Tre cose che questo modulo deve rendere impossibile sbagliare, e che per
// questo sono qui dentro e non in una vista:
//
// 1. **La dichiarazione non è un'autenticazione.** Ogni chiamata porta
//    `X-Kbs-Person`; il valore arriva da un campo della pagina e non è una
//    credenziale. `src/identity.rs` lo dice e qui non si finge il contrario:
//    nessuna vista mostra un nome come se fosse stato verificato.
// 2. **`404` non si distingue.** «Non c'è» e «non lo vedi» danno lo stesso
//    corpo, `{"error":"non-trovato"}`, e non c'è modo di sapere quale dei due
//    sia. Quindi `ErroreKbs` ha **una sola** forma per quel caso e non porta
//    nessun campo che distingua: una vista che volesse distinguere non avrebbe
//    niente su cui farlo.
// 3. **La ricerca non porta testo.** `/api/v1/search` risponde
//    `{argument, course, rank}` e nient'altro, e `rank` è un `bm25` di FTS5 in
//    cui **più negativo è migliore**. Un risultato di ricerca è un indirizzo,
//    non un documento: per questo `cercaConTesto` fa la ricerca e poi riprende
//    ogni argomento con una seconda chiamata, e restituisce la coppia. Il
//    test `la_ricerca_ non_ritorna_il_testo_e_il_client_riprende_l_argomento`
//    passa dal router vero e lo verifica.

import { percorso, percorsoConQuery } from "./rotte.js";

/** L'header con cui arriva la dichiarazione sulle chiamate che il browser intestà. */
export const HEADER_PERSONA = "X-Kbs-Person";

/**
 * Il corpo di un errore, tradotto in qualcosa che una vista possa mostrare.
 *
 * `assente` è **un solo caso**: non ha sottotipi, non ha `motivo`, non ha
 * `regola`. Aggiungere un campo che distingua «non c'è» da «non lo vedi» qui
 * equivarrebbe a riaprire il canale che `src/error.rs` chiude, e questa è
 * l'ultima riga in cui si può decidere di non farlo.
 */
export class ErroreKbs extends Error {
  /**
   * @param {"assente"|"identita"|"richiesta"|"regola"|"server"|"rete"} tipo
   * @param {number} status lo status HTTP, per la diagnosi
   * @param {{regola?: string, motivo?: string}} corpo il corpo del server
   */
  constructor(tipo, status, corpo = {}) {
    super(messaggioDi(tipo, corpo));
    this.tipo = tipo;
    this.status = status;
    /** La regola di `ARCHITECTURE.md` che ha detto no, quando l'ha detta. */
    this.regola = corpo.regola ?? null;
    /** Perché, in italiano, quando il server lo dice. */
    this.motivo = corpo.motivo ?? null;
  }
}

/**
 * La frase che una vista mette sotto il titolo dell'errore.
 *
 * Per `assente` la frase è **fissa** e non ha varianti: se domani ci fosse un
 * modo per dire «non lo vedi» senza dirlo, la differenza fra le due ipotesi
 * uscirebbe da qui.
 */
function messaggioDi(tipo, corpo) {
  switch (tipo) {
    case "assente":
      return "Non c'è, o non lo vedi. Questa risposta non distingue i due casi e non può.";
    case "identita":
      return "Il server non ha ricevuto una dichiarazione di identità.";
    case "richiesta":
      return corpo.motivo
        ? `Richiesta non valida: ${corpo.motivo}`
        : "Richiesta non valida.";
    case "regola":
      return corpo.motivo
        ? `${corpo.regola ? `${corpo.regola} — ` : ""}${corpo.motivo}`
        : "Una regola di dominio ha detto no.";
    case "server":
      return "Il server non ha potuto rispondere. Il dettaglio è nel suo registro, non qui.";
    default:
      return "Il server non è raggiungibile.";
  }
}

/** La persona **dichiarata** al momento. Non una credenziale: vedi `identity.rs`. */
let dichiarata = null;

/**
 * La persona dichiarata, o `null`.
 *
 * `null` non è un caso d'eccezione: il server risponde `401` e la vista deve
 * poterlo dire senza che l'interfaccia si accorga che manca qualcosa.
 */
export function persona() {
  return dichiarata;
}

/**
 * Dichiara chi sta chiedendo.
 *
 * Non autentica niente e non rende la dichiarazione più vera: il predicato D5
 * guarda le relazioni registrate dal docente, non il fatto che qualcuno si sia
 * dichiarato tale. Il nome dell'operazione lo dice, e `identita()` restituisce
 * `dichiarata` per costringere chi chiama a scrivere la parola.
 */
export function dichiara(personaId) {
  dichiarata = personaId && personaId.trim() !== "" ? personaId.trim() : null;
  return dichiarata;
}

/**
 * Una `GET` che parla col server.
 *
 * @param {string} nome una chiave di `ROTTE`
 * @param {Record<string, string>} valori i valori dei segnaposto
 * @param {{query?: Record<string, string|number|undefined>}} opzioni
 * @returns {Promise<any>} il corpo JSON
 */
export async function get(nome, valori = {}, opzioni = {}) {
  return richiedi(nome, valori, { ...opzioni, metodo: "GET" });
}

/**
 * Una `POST` JSON.
 *
 * @param {string} nome una chiave di `ROTTE`
 * @param {Record<string, string>} valori i valori dei segnaposto
 * @param {unknown} corpo il corpo da mandare
 * @returns {Promise<any>} il corpo JSON, o `null` su un `204`
 */
export async function post(nome, valori = {}, corpo = {}) {
  return richiedi(nome, valori, { metodo: "POST", corpo });
}

/** La richiesta vera, con la traduzione degli errori. */
async function richiedi(nome, valori, { metodo, corpo, query }) {
  const url = percorsoConQuery(nome, valori, query ?? {});
  const intestazioni = { Accept: "application/json" };
  if (dichiarata) {
    intestazioni[HEADER_PERSONA] = dichiarata;
  }
  if (corpo !== undefined) {
    intestazioni["Content-Type"] = "application/json";
  }

  let risposta;
  try {
    risposta = await fetch(url, {
      method: metodo,
      headers: intestazioni,
      // `no-store` anche da qui: il server già lo dichiara su `/api/`, ma la
      // dichiarazione del client è la stessa regola detta due volte, e una
      // volta sola sarebbe un'autorizzazione a metà.
      cache: "no-store",
      body: corpo === undefined ? undefined : JSON.stringify(corpo),
    });
  } catch {
    // Una rete caduta non è un `404` e non è un `403`: è un mondo diverso, e
    // dirlo è l'unica cosa che si può fare.
    throw new ErroreKbs("rete", 0);
  }

  const corpoTesto = await risposta.text();
  let json = null;
  if (corpoTesto !== "") {
    try {
      json = JSON.parse(corpoTesto);
    } catch {
      json = null;
    }
  }

  if (risposta.ok) {
    return json;
  }
  throw new ErroreKbs(tipoDi(risposta.status), risposta.status, json ?? {});
}

/** La traduzione dello status in un caso. Nessun caso in più di quelli del server. */
function tipoDi(status) {
  switch (status) {
    case 401:
    case 400:
      return status === 401 ? "identita" : "richiesta";
    case 404:
      return "assente";
    case 409:
      return "regola";
    default:
      return status >= 500 ? "server" : "richiesta";
  }
}

/**
 * Un `EventSource` sul canale degli eventi di un corso.
 *
 * `EventSource` **non può** allegare un header: la dichiarazione va nella query
 * string, ed è la stessa dichiarazione. Non è un aggiramento, è l'unico modo
 * che il browser lascia — e non è un incidente, perché qui non c'è nulla da
 * proteggere: c'è un nome che il cliente si è attribuito.
 *
 * @param {string} corso
 * @returns {EventSource}
 */
export function canaleEventi(corso) {
  const url = new URL(percorso("eventi", { corso }), window.location.href);
  if (dichiarata) {
    url.searchParams.set("person", dichiarata);
  }
  return new EventSource(url.toString());
}

/**
 * L'URL del runtime three.js, con la dichiarazione se c'è.
 *
 * Il runtime si chiede **al daemon** e non a un CDN (D15): una classe senza
 * rete deve poter aprire la scena. Per questo la rotta è verificata con una
 * `GET` prima di importarla: se il runtime non è vendorizzato il server
 * risponde `500` e dice `D15`, che è un errore di installazione e non un
 * `404`, e dire «manca» sarebbe dire la cosa sbagliata.
 *
 * @returns {Promise<{ok: true, url: string}|{ok: false, motivo: string}>}
 */
export async function runtimeTreDimensioni() {
  const url = percorso("three");
  const risposta = await fetch(url, { method: "GET", cache: "force-cache" });
  if (!risposta.ok) {
    let motivo = `il runtime non è stato servito (${risposta.status})`;
    try {
      const corpo = await risposta.json();
      if (corpo && corpo.motivo) {
        motivo = corpo.motivo;
      }
    } catch {
      // Un corpo non JSON è un errore di installazione, e la frase sopra
      // basta: qui non si indaga.
    }
    return { ok: false, motivo };
  }
  return { ok: true, url };
}

/**
 * La ricerca, con il testo che il server non manda.
 *
 * `/api/v1/search` dà `{argument, course, rank}` e basta: il testo non c'è e
 * non ci sarà, perché l'indice restituisce un ordinamento e non un documento.
 * Quindi ogni hit viene ripreso con `GET /api/v1/arguments/{id}`, che passa
 * dal predicato: un hit che il predicato non approva non arriva qui, e se
 * arrivasse lo stesso si tradurrebbe nello stesso `404` di prima.
 *
 * `rank` più negativo è migliore, e l'ordine dell'array è quello dell'indice:
 * qui non si riordina, perché un secondo ordinamento diverge dal primo alla
 * prima release.
 *
 * @param {string} q il testo cercato
 * @param {number} [limite] quanti risultati, entro il soffitto del server
 * @returns {Promise<Array<{rank: number, course: string, argomento: object|null, assente: boolean}>>}
 */
export async function cercaConTesto(q, limite) {
  const risposta = await get("ricerca", {}, { query: { q, limit: limite } });
  const hits = risposta && Array.isArray(risposta.hits) ? risposta.hits : [];
  const righe = [];
  for (const hit of hits) {
    try {
      const letto = await get("argomento", { id: hit.argument });
      righe.push({ ...hit, argomento: letto.argument, assente: false });
    } catch (errore) {
      if (!(errore instanceof ErroreKbs) || errore.tipo !== "assente") {
        throw errore;
      }
      // L'hit esisteva nell'indice e l'argomento non si può leggere. È la
      // stessa risposta che si avrebbe chiesto l'id direttamente, e la riga
      // resta con il suo id: cancellarla sarebbe mentire sul conto dell'indice.
      righe.push({ ...hit, argomento: null, assente: true });
    }
  }
  return righe;
}
