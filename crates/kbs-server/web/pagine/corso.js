// Gli argomenti di un corso: l'elenco che la rotta dava e che nessuna pagina
// apriva.
//
// # Questa rotta era viva e senza porta
//
// `GET /api/v1/courses/{corso}/arguments` era dichiarata in `rotte.js`, provata
// contro il router vero, e **non chiamata da nessuna parte**: una chiave che
// nessuno apre. La home diceva «non c'è un elenco dei corsi», e quella frase
// resta vera — un corso singolo non ha una rotta che lo dica, perché i corsi
// sono relazioni e non campi. Ma gli **argomenti** di un corso che hai già scritto
// nella barra in alto si potevano elencare, e non si elencavano. Questa pagina è
// quella porta.
//
// # L'elenco vuoto è una risposta, non un errore di caricamento
//
// La rotta risponde `200 {"arguments": [], "course": …}` quando la persona ha
// una relazione col corso e non vede niente: uno studente iscritto a un corso in
// cui tutto è ancora in bozza riceve un elenco vuoto, e quel vuoto è la verità su
// che cosa può leggere. Quindi qui non c'è uno `0`, non c'è «nessun
// argomento» che suoni come un giudizio sul corso, e non c'è un ricaricamento
// che nasconda il vuoto dietro uno spinner: c'è la frase che dice che non c'è
// niente da mostrare e **perché**.
//
// Il `404` è un'altra cosa: il server lo dà quando non c'è nessuna relazione,
// e cioè quando il corso non esiste, quando non ci insegni e quando non sei
// iscritto. Le tre cose danno la stessa risposta e non c'è modo di scegliere
// fra loro, quindi anche qui non si sceglie: si dice che la risposta non
// distingue. Senza dichiarazione è un `401`, che è un'altra risposta ancora, e
// non un altro caso di questa: è l'unica che riguarda chi chiede e non che cosa
// chiede.
//
// # Il filtro è del server, e le sue parole sono un vocabolario
//
// `?state=` è un enum di `kbs-core` in kebab-case (`bozza`, `del-docente`,
// `in-corso`, `archiviato`), non una stringa libera. Qui i quattro sono scritti
// come **link**, per dare al docente una mano, e non come un vocabolario che
// decide: un nome che non è uno di questi passa al server e la risposta è sua.
// I link sono comodi; il giudizio è del server.
//
// # Il conteggio non mente sul filtro
//
// Quando non si filtra, sotto la tabella c'è il ripartito per stato di **quello
// che è stato mostrato**. Quando si filtra, il ripartito sarebbe una frase che
// promette assenze («una bozza») che l'elenco filtrato non può né confermare né
// smentire: quindi in quel caso non c'è, e la pagina dice che è filtrata.
//
// # Qui non si scrive
//
// Come la coda e i registri: questa pagina non crea, non ratifica e non
// pubblica. Lo stato di un argomento si cambia con i gesti della coda, che sono
// gesti distinti con un rifiuto ciascuno.

import { el, riempi, sezione, tabella, riquadroErrore, nulla } from "../lib/dom.js";
import { get, persona } from "../lib/api.js";
import { provenienzaCorta } from "../lib/provenienza.js";

/**
 * Gli stati di pubblicazione, con la parola che il docente legge.
 *
 * Ogni voce è `[stato, singolare, plurale]`: la chiave è il nome che
 * `kbs-core` dà allo stato ed è ciò che il filtro manda, le altre due sono una
 * traduzione per l'occhio — e il plurale serve solo al conteggio, perché
 * «1 bozze» è una riga che un docente legge due volte prima di fidarsi.
 */
const STATI = [
  ["in-corso", "in uso", "in uso"],
  ["del-docente", "revisione del docente", "revisioni del docente"],
  ["bozza", "bozza", "bozze"],
  ["archiviato", "fuori uso", "fuori uso"],
];

/**
 * Gli argomenti di un corso che la persona dichiarata può vedere.
 *
 * @param {{nodo: HTMLElement, corso: string, stato: string|null}} contesto
 *   `stato` è il filtro dalla rotta dell'interfaccia, o `null` per «tutti».
 * @returns {Promise<void>}
 */
export async function argomentiDelCorso({ nodo, corso, stato }) {
  riempi(nodo, el("p", { class: "caricamento" }, ["Carico gli argomenti…"]));

  let risposta;
  try {
    risposta = await get("argomenti", { corso }, { query: { state: stato ?? null } });
  } catch (errore) {
    riempi(nodo, [
      sezione(`Argomenti · ${corso}`, riquadroErrore(errore)),
      spiegazione(errore, stato),
    ]);
    return;
  }

  const argomenti = risposta.arguments ?? [];
  riempi(nodo, [
    sezione(
      `Argomenti · ${corso}`,
      filtro(stato),
      argomenti.length === 0 ? elencoVuoto(stato) : tabellaArgomenti(argomenti),
      // Il ripartito conta delle righe: su un elenco vuoto non ci sono righe, e
      // un conteggio di niente accanto alla frase che dice che non c'è niente
      // è una seconda risposta alla stessa domanda.
      argomenti.length === 0 ? null : stato ? notaSuFiltro(stato) : ripartito(argomenti),
    ),
  ]);
}

/** Il filtro per stato, come link: quattro stati e «tutti». */
function filtro(stato) {
  return el("p", { class: "nota" }, [
    "Filtra: ",
    ...STATI.flatMap(([chiave, etichetta], i) => [
      i === 0 ? "" : " · ",
      stato === chiave
        ? el("strong", {}, [etichetta])
        : el("a", { href: `#/argomenti?state=${chiave}` }, [etichetta]),
    ]),
    " · ",
    stato
      ? el("a", { href: "#/argomenti" }, ["tutti"])
      : el("strong", {}, ["tutti"]),
  ]);
}

/** L'elenco: stato e provenienza per ogni argomento, con il link al lettore. */
function tabellaArgomenti(argomenti) {
  return tabella(
    ["Argomento", "Stato", "Provenienza", "Ratifica"],
    argomenti.map((argomento) => [
      cellaArgomento(argomento),
      cellaStato(argomento),
      provenienzaCorta(argomento.origin),
      cellaRatifica(argomento),
    ]),
    { colonne: [{}, { class: "col-stato" }, {}, { class: "col-stato" }] },
  );
}

/** Il titolo, l'id e la riga: la riga serve a riconoscere l'argomento a scatto. */
function cellaArgomento(argomento) {
  return el("div", {}, [
    el("a", { href: `#/argomento/${argomento.id}` }, [argomento.title]),
    el("div", { class: "claim-id" }, [el("code", {}, [argomento.id])]),
    argomento.summary
      ? el("div", { class: "motivo" }, [argomento.summary])
      : null,
  ]);
}

/** Lo stato, con le parole del dominio e non con un colore solo. */
function cellaStato(argomento) {
  const voce = STATI.find(([chiave]) => chiave === argomento.state);
  return el("div", {}, [
    el("div", { class: "stato-riga" }, [argomento.state]),
    voce ? el("div", { class: "motivo" }, [voce[1]]) : null,
  ]);
}

// La provenienza non è qui: è in `../lib/provenienza.js`, ed è la versione
// corta di quella del lettore, non la versione per intero. In un elenco la
// domanda è «di chi è», e l'hash del prompt sta nella pagina dell'argomento,
// che è il posto dove lo si verifica.

/**
 * La ratifica, o la sua assenza, dette in modo che non si confondano.
 *
 * Una ratifica invecchiata rispetto al contenuto corrente è qui mostrata come
 * invecchiata, perché il confronto dei due hash è un fatto del campo, non un
 * giudizio: la pagina non ricalcola nulla, confronta le due stringhe che il
 * server ha mandate.
 */
function cellaRatifica(argomento) {
  const ratifica = argomento.ratified;
  if (!ratifica) {
    return el("span", { class: "attenzione" }, ["nessuna"]);
  }
  const valida = ratifica.contract_hash === argomento.content_hash;
  return el("span", { class: valida ? "ok" : "attenzione" }, [
    `${ratifica.by} il ${giorno(ratifica.at)}`,
    el("div", { class: "motivo" }, [
      valida ? "vale per il contenuto corrente" : "non vale più: il contenuto è cambiato dopo",
    ]),
  ]);
}

/** L'elenco vuoto, detto per esteso. */
function elencoVuoto(stato) {
  return el("div", {}, [
    el("p", { class: "nulla" }, [
      stato
        ? `Nessun argomento di questo corso è in stato «${stato}», per la persona che hai dichiarato.`
        : "Nessun argomento che tu possa vedere, in questo corso.",
    ]),
    el("p", { class: "nota" }, [
      stato
        ? "Questo è un elenco vuoto per quello stato, non un corso vuoto: gli altri stati non sono qui perché gliel'hai chiesti tu. Toglilo dal filtro per vedere tutto quello che puoi leggere."
        : "Questo è un elenco vuoto, non un corso vuoto: il server risponde così quando hai una relazione col corso e non c'è niente che ti sia leggibile. Se hai dichiarato un'altra persona, l'elenco cambia — e se non hai dichiarato nessuno, il server risponde che manca la dichiarazione.",
    ]),
  ]);
}

/** Il ripartito per stato, contato su quello che è stato mostrato. */
function ripartito(argomenti) {
  const conta = new Map();
  for (const argomento of argomenti) {
    conta.set(argomento.state, (conta.get(argomento.state) ?? 0) + 1);
  }
  // L'ordine è quello degli stati, e gli stati che questa pagina non conosce
  // vengono dopo col loro nome vero: una chiave fuori vocabolario è un fatto
  // da mostrare, non una riga da nascondere dietro un'etichetta inventata.
  const sconosciuti = [...conta.keys()].filter((s) => !STATI.some(([c]) => c === s));
  const parti = [...STATI.map(([chiave]) => chiave), ...sconosciuti]
    .filter((stato) => conta.has(stato))
    .map((stato) => {
      const n = conta.get(stato);
      const voce = STATI.find(([chiave]) => chiave === stato);
      return `${n} ${voce ? (n === 1 ? voce[1] : voce[2]) : stato}`;
    });
  return el("p", { class: "nota" }, [
    `Su questi ${argomenti.length} argomenti: ${parti.join(", ")}. Sono quelli che tu puoi vedere adesso, non il contenuto del corso: chi insegna ne vede di più, e questo elenco è già filtrato dal predicato.`,
  ]);
}

/** Che cosa significa una lista che mostra solo uno stato. */
function notaSuFiltro(stato) {
  const voce = STATI.find(([chiave]) => chiave === stato);
  return el("p", { class: "nota" }, [
    voce
      ? `L'elenco è ristretto allo stato «${stato}» (${voce[1]}). Gli altri stati non sono qui, e questa pagina non conta: dire che in questo corso non ci sono bozze quando la domanda era «in uso» sarebbe una risposta a una domanda che non hai fatto.`
      : `Lo stato «${stato}» non è uno dei quattro che questa pagina conosce, e il server lo ha accettato: il vocabolario degli stati è più avanti di questa pagina. Quello che vedi è la risposta a questa domanda, e questa pagina non sa dire se il filtro abbia ristretto qualcosa.`,
  ]);
}

/** Perché la pagina non si vede, quando non si vede. */
function spiegazione(errore, stato) {
  if (errore.tipo === "assente") {
    return sezione(
      "Perché gli argomenti non si vedono",
      el("p", {}, [
        "Gli argomenti di un corso li vede chi ha una relazione con quel corso — insegna, o è iscritto. Il server risponde la stessa cosa per un corso che non esiste e per uno di cui non fai parte, e non c'è modo di sapere quale dei due sia. Un corso in cui non c'è niente che tu possa leggere non dà un `404`: dà un elenco vuoto, e la pagina lo dice.",
      ]),
      el("p", { class: "nota" }, [
        persona()
          ? `Hai dichiarato ${persona()}. La dichiarazione non è un'autenticazione: ciò che conta è la relazione registrata per questa persona in questo corso.`
          : "Non hai dichiarato nessuna identità: senza dichiarazione il server risponde 401.",
      ]),
    );
  }
  if (errore.tipo === "identita") {
    return sezione(
      "Non hai dichiarato nessuna identità",
      el("p", {}, [
        "Il server ha risposto che manca la dichiarazione. È l'unica risposta che riguarda chi chiede e non che cosa chiede, e l'unica che si risolve da questa pagina: scrivi chi sei nella barra in alto.",
      ]),
    );
  }
  if (errore.tipo === "richiesta" && stato) {
    return sezione(
      "Il filtro non è uno stato",
      el("p", {}, [
        `«${stato}» non è uno stato di pubblicazione. Gli stati sono quattro, e sono questi:`,
      ]),
      tabella(
        ["Stato", "Che cosa vuol dire"],
        STATI.map(([chiave, uno]) => [el("code", {}, [chiave]), uno]),
      ),
    );
  }
  return null;
}

/** La data, in secondi e senza fuso: le righe si confrontano per contenuto. */
function giorno(millis) {
  return new Date(millis).toISOString().slice(0, 10);
}
