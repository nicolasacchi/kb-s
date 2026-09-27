// Il canale eventi, e che cosa si può e non si può sapere da un canale
// partizionato.
//
// # Il primo evento dice da dove comincia la vista
//
// `ciao` porta `da_evento`, il contatore del bus al momento della
// sottoscrizione. **Non è un resume**: il bus non ha un log, e dichiararlo
// all'inizio è l'unico modo perché un client che rientra sappia che deve
// andare a leggere la coda e non a fidarsi dello stream.
//
// # Che cosa questo canale non può dire
//
// Un canale partizionato per corso non trasporta gli eventi degli altri corsi,
// e chi è iscritto a un corso **non sa** se in un altro corso è successo
// qualcosa: non «non riceve», non «è filtrato» — il canale non gli è stato
// dato, e la pubblicazione su un canale che nessuno ascolta non lascia
// traccia. Un'interfaccia che mostrasse «nessuna attività» sarebbe quindi una
// bugia; quello che può dire è «questa attività», e qui sotto c'è scritto
// esattamente quello.
//
// E c'è un secondo limite, dichiarato dal server stesso: il canale di un corso
// trasporta anche eventi su argomenti che **questa** persona non può leggere, e
// il predicato li filtra a valle. Un evento che arriva qui è un evento che
// questa persona può vedere, ma un evento che non arriva non prova niente.
//
// Infine: il canale del corpo docente lo sceglie il **server**, in base alle
// relazioni, e non arriva dalla query string. Quindi un docente e uno studente
// sullo stesso corso non vedono la stessa cosa, e l'interfaccia non può sapere
// quale dei due è — deve dire che il canale è aperto e lasciare che la lista
// degli eventi parli.

import { el, testo, riempi } from "./dom.js";
import { canaleEventi, persona } from "./api.js";

/** Gli eventi che il server emette, e il loro significato in una riga. */
export const EVENTI = {
  ciao: "dichiarazione di inizio vista",
  ratificato: "un docente ha ratificato",
  pubblicato: "un argomento è entrato in uso",
  "ratifica-ritirata": "una ratifica è stata ritirata",
  lag: "il canale è indietro: la coda va riletta",
};

/**
 * Una striscia di stato del canale di un corso.
 *
 * @param {string} [corso] il corso da ascoltare; `null` non apre nulla
 * @param {(evvenimento: {nome: string, dati: object}) => void} [alCambio]
 *   chiamata a ogni evento, perché una vista possa rileggere
 * @returns {{nodo: HTMLElement, chiudi: () => void, aggiorna: (corso: string|null) => void}}
 */
export function strisciaEventi(corso, alCambio) {
  const stato = el("span", { class: "stato-canale" }, ["nessun corso aperto"]);
  const elenco = el("ul", { class: "eventi" });
  const spiegazione = el("p", { class: "nota" }, [
    "Nessun corso aperto. Il canale eventi è partizionato per corso: quello che vedi è l'attività di questo corso e non di altri, e non c'è modo di sapere se in un altro corso è successo qualcosa.",
  ]);
  const nodo = el("div", { class: "canale" }, [
    el("h2", {}, ["Attività"]),
    stato,
    spiegazione,
    elenco,
  ]);

  let sorgente = null;
  let aperto = null;

  const chiudi = () => {
    if (sorgente) {
      sorgente.close();
      sorgente = null;
    }
    aperto = null;
    riempi(elenco);
  };

  const ascolta = (evento) => {
    let dati = {};
    try {
      dati = JSON.parse(evento.data);
    } catch {
      dati = {};
    }
    if (evento.type === "ciao") {
      stato.textContent = `canale aperto su ${dati.corso} — la vista comincia dall'evento ${dati.da_evento}`;
      riempi(
        spiegazione,
        testo(
          `Il canale è partizionato: ricevi l'attività di ${dati.corso} e non quella di altri corsi, e non puoi sapere se in un altro corso è successo qualcosa. `,
        ),
        testo(
          `Prima dell'evento ${dati.da_evento} non hai ricevuto niente: non è un ripristino, è il punto da cui comincia la tua vista. La verità è la coda di ratifica, non lo stream.`,
        ),
      );
      return;
    }
    const voce = el("li", { class: `evento evento-${evento.type}` }, [
      el("strong", {}, [EVENTI[evento.type] ?? evento.type]),
      testo(" "),
      testo(`${dati.argomento ?? ""} — evento ${dati.id ?? "?"}`),
    ]);
    elenco.prepend(voce);
    while (elenco.childElementCount > 12) {
      elenco.lastElementChild.remove();
    }
    if (evento.type === "lag") {
      stato.textContent = `canale indietro di ${dati.persi ?? "?"} eventi: la coda va riletta`;
    }
    alCambio?.({ nome: evento.type, dati });
  };

  const aggiorna = (nuovo) => {
    if (nuovo === aperto) {
      return;
    }
    chiudi();
    if (!nuovo) {
      stato.textContent = "nessun corso aperto";
      return;
    }
    if (!persona()) {
      stato.textContent = `nessuna dichiarazione: il canale di ${nuovo} non si apre (401)`;
      riempi(
        spiegazione,
        testo(
          "Senza una dichiarazione di identità il server risponde 401 e il canale non si apre. Dichiarare non è autenticare: dichiara e vedrai.",
        ),
      );
      return;
    }
    aperto = nuovo;
    stato.textContent = `connessione a ${nuovo}…`;
    sorgente = canaleEventi(nuovo);
    for (const nome of Object.keys(EVENTI)) {
      sorgente.addEventListener(nome, ascolta);
    }
    sorgente.onerror = () => {
      stato.textContent = `canale di ${nuovo} non connesso`;
    };
  };

  if (corso) {
    aggiorna(corso);
  }

  return { nodo, chiudi, aggiorna };
}
