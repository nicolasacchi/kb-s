// I registri: che cosa ha dimostrato uno studente, e chi ha deciso che cosa.
//
// # Le contestazioni viaggiano dentro la riga
//
// Non c'è una rotta `/contestazioni` e qui non ce n'è una sezione separata.
// Chi legge un ricorso deve vedere il voto e la contestazione nello stesso atto:
// due query che possono disaccordarsi sono due fonti di verità, e in un
// registro una fonte di verità che si disaccorda da sé stessa è peggio di
// nessuna. Quindi il ricorso sta nella cella del giudizio.
//
// # «Vedi parte del registro e non tutto» è una risposta, non un errore
//
// Il registro è leggibile da tre persone: lo studente stesso, chi insegna il
// corso, e **chi ha emesso almeno uno dei giudici** su quello studente in
// quello corso. Il terzo — un pari che ha valutato una riga — vede **solo le
// righe che ha emesso lui**. Non è un elenco troncato per errore e non è un
// permesso revocato: è il minimo diritto che copre quello che ha fatto, e il
// resto del registro non lo riguarda.
//
// Qui sotto c'è **scritto**, non sottinteso: se le righe che vedi hanno tutte
// lo stesso emittente e non sei il docente né lo studente, è quello il motivo.
//
// # Qui non si scrive
//
// Il server non ha rotte di scrittura dei registri, e non è una dimenticanza:
// appendere un giudizio o un'osservazione è un atto del docente che passa dalla
// CLI e dal file (D10). Una pagina che accettasse il voto trasformerebbe il
// registro da atto a modulo da compilare. Quindi i bottoni qui non ci sono, e la
// pagina dice dove si scrive.

import { el, riempi, sezione, tabella, riquadroErrore, nulla } from "../lib/dom.js";
import { get, persona } from "../lib/api.js";

/**
 * La pagina dei registri.
 *
 * @param {{nodo: HTMLElement, corso: string, studente: string, argomento: string|null}} contesto
 * @returns {Promise<void>}
 */
export async function registri({ nodo, corso, studente, argomento }) {
  if (!studente) {
    riempi(
      nodo,
      sezione(
        "Registri",
        nulla(
          "Serve l'id dello studente di cui vuoi il registro. Il server non ha una rotta che dica «i miei studenti»: le relazioni le registra il docente e nessuna pagina le elenca.",
        ),
      ),
    );
    return;
  }

  riempi(nodo, el("p", { class: "caricamento" }, ["Carico i registri…"]));

  const richieste = [
    ["giudizi", () => get("giudizi", { corso }, { query: { person: studente } })],
  ];
  if (argomento) {
    richieste.push([
      "osservazioni",
      () => get("osservazioni", { id: argomento }, { query: { person: studente } }),
    ]);
  }

  const esiti = [];
  for (const [nome, chiama] of richieste) {
    try {
      esiti.push([nome, await chiama()]);
    } catch (errore) {
      esiti.push([nome, { errore }]);
    }
  }

  const figli = [
    sezione(`Registro di ${studente} · ${corso}`, notaSullAmbito(esiti)),
  ];

  for (const [nome, esito] of esiti) {
    if (esito.errore) {
      figli.push(
        sezione(nome === "giudizi" ? "Giudizi" : "Osservazioni", riquadroErrore(esito.errore)),
      );
      continue;
    }
    const righe = nome === "giudizi" ? esito.giudizi ?? [] : esito.osservazioni ?? [];
    figli.push(
      sezione(
        nome === "giudizi" ? "Giudizi" : "Osservazioni",
        nome === "giudizi" ? tabellaGiudizi(righe) : tabellaOsservazioni(righe),
        notaEmittenti(nome, righe),
      ),
    );
  }

  figli.push(
    sezione(
      "Dove si scrive",
      el("p", {}, [
        "Non qui. Questo server non ha rotte di scrittura dei registri: appendere un giudizio o un'osservazione è un atto del docente che passa dalla CLI e dal file di sorgente, e una pagina che accettasse il voto trasformerebbe il registro da atto in un modulo da compilare.",
      ]),
    ),
  );

  riempi(nodo, figli);
}

/** Il giudizio con la contestazione dentro la stessa riga. */
function tabellaGiudizi(giudizi) {
  if (giudizi.length === 0) {
    return nulla(
      "Nessun giudizio in questo registro. Un elenco vuoto qui è una risposta vera: non ci sono giudizi registrati che tu possa vedere, e non è la stessa cosa che non ce ne siano.",
    );
  }
  return tabella(
    ["Quando", "Argomento", "Voto", "Chi ha deciso", "Come", "Contestazione"],
    giudizi.map((g) => [
      quando(g.at),
      el("a", { href: `#/argomento/${g.argument}` }, [g.argument]),
      el("span", { class: "voto" }, [g.grade]),
      el("code", {}, [g.graded_by]),
      cellaGrado(g.kind, g.rubric_version),
      cellaContestazione(g.contested),
    ]),
    { colonne: [{}, {}, { class: "col-stato" }, {}, {}, { class: "col-contestazione" }] },
  );
}

/** L'osservazione: che cosa ha dimostrato lo studente, e con quale prova. */
function tabellaOsservazioni(osservazioni) {
  if (osservazioni.length === 0) {
    return nulla(
      "Nessuna osservazione su questo argomento. «Non c'è niente» qui non vuol dire che lo studente non abbia lavorato: vuol dire che nessuna osservazione su questo argomento è leggibile da te.",
    );
  }
  return tabella(
    ["Quando", "Studente", "Prova", "Chi ha giudicato", "Riproducibile"],
    osservazioni.map((o) => [
      quando(o.at),
      el("code", {}, [o.student]),
      cellaProva(o.evidence),
      o.judged_by
        ? cellaGrado(o.judged_by, "")
        : el("span", { class: "attenzione" }, ["non ancora giudicato"]),
      o.evidence.kind === "checked" || o.evidence.kind === "written"
        ? el("span", { class: "ok" }, ["sì"])
        : el("span", { class: "attenzione" }, ["no"]),
    ]),
  );
}

/** La prova, con il suo tipo e la sua riproducibilità dichiarata. */
function cellaProva(prova) {
  switch (prova.kind) {
    case "checked":
      return el("div", {}, [
        `esercizio verificato dal programma: ${prova.correct ? "risposta giusta" : "risposta sbagliata"}`,
        el("div", { class: "motivo" }, [
          `${prova.exercise} · istanza ${prova.instance}`,
        ]),
      ]);
    case "oral":
      return el("div", {}, [
        "interrogazione orale",
        el("div", { class: "motivo" }, [
          "non riproducibile: nessun programma può rifarla",
          prova.witness ? ` · testimone ${prova.witness}` : "",
        ]),
        el("div", { class: "motivo" }, [prova.note]),
      ]);
    case "written":
      return el("span", {}, [prova.ref_doc]);
    case "none":
      return el("span", {}, ["nessuna prova: l'argomento è semplicemente aperto"]);
    default:
      return prova.kind;
  }
}

/** Chi ha emesso il giudizio, e con quale regola. */
function cellaGrado(grado, rubric) {
  const nomi = {
    deterministic: "un programma",
    peer: "un pari",
    human: "una persona",
    teacher: "il docente",
  };
  return el("div", {}, [
    nomi[grado] ?? grado,
    rubric ? el("div", { class: "motivo" }, [`rubric ${rubric}`]) : null,
  ]);
}

/**
 * La contestazione, o la sua assenza, dette in modo che non si confondano.
 *
 * `contested: null` e `contested: {outcome: null}` sono due cose diverse: la
 * prima è «nessuno ha contestato», la seconda è «c'è un ricorso aperto». È la
 * differenza che un ricorso chiede per primo, e qui non è una casella di testo.
 */
function cellaContestazione(contestazione) {
  if (!contestazione) {
    return el("span", { class: "nota" }, ["nessuna contestazione"]);
  }
  const esiti = {
    upheld: "accolta",
    rejected: "respinta",
    "under-review": "in esame",
  };
  return el("div", { class: "contestazione" }, [
    el("strong", {}, ["contestata"]),
    el("div", { class: "motivo" }, [
      `da ${contestazione.by} il ${quando(contestazione.at)}`,
    ]),
    el("div", {}, [contestazione.reason]),
    el("div", { class: "motivo" }, [
      contestazione.outcome
        ? `esito: ${esiti[contestazione.outcome] ?? contestazione.outcome}`
        : "esito: aperta, non ancora decisa",
    ]),
  ]);
}

/**
 * Perché potresti vedere solo parte del registro.
 *
 * Questo è il punto che la pagina deve **dire** e non lasciare implicito: chi
 * vede poche righe senza essere il docente e senza essere lo studente è un pari
 * che ha emesso almeno uno di quei giudizi, e vede solo i propri.
 */
function notaSullAmbito(esiti) {
  const conErrori = esiti.filter(([, e]) => e.errore);
  const giudizi = esiti.find(([nome]) => nome === "giudizi")?.[1]?.giudizi ?? [];
  const io = persona();
  if (conErrori.length > 0) {
    return null;
  }
  const chi = new Set(giudizi.map((g) => g.graded_by));
  if (giudizi.length > 0 && chi.size === 1 && io && !chi.has(io)) {
    return el("p", { class: "nota ambito" }, [
      "Vedi giudizi emessi da una sola persona e non sei tu: sei un pari che ha emesso almeno uno di questi giudizi, e il registro che ti viene mostrato contiene solo le tue righe. Il resto del registro di questo studente non è tuo, e la sua assenza non è un errore. Chi vede tutto è lo studente e chi insegna il corso.",
    ]);
  }
  return el("p", { class: "nota ambito" }, [
    "Il registro è leggibile dallo studente, da chi insegna il corso e da chi ha emesso almeno uno dei giudici. Quest'ultimo vede solo le proprie righe: se ne vedi poche, è per quello, non perché il resto sia sparito.",
  ]);
}

/** Chi ha emesso quante righe, detto sotto la tabella. */
function notaEmittenti(nome, righe) {
  if (nome === "osservazioni" || righe.length === 0) {
    return null;
  }
  const chi = [...new Set(righe.map((g) => g.graded_by))];
  return el("p", { class: "nota" }, [
    `${righe.length} giudizi, emessi da ${chi.length === 1 ? "una sola persona" : `${chi.length} persone`} (${chi.join(", ")}).`,
  ]);
}

/** La data, in secondi e senza fuso: i registri si confrontano per contenuto. */
function quando(millis) {
  return new Date(millis).toISOString().slice(0, 16).replace("T", " ");
}
