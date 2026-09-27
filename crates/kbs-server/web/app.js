// L'ingresso: stato, instrada, monta.
//
// Lo stato che questa pagina tiene è **due stringhe** — chi hai dichiarato e di
// che corso — e nient'altro. Non c'è una sessione, non c'è un cookie e non c'è
// un token: `src/identity.rs` dice che questo server non autentica nessuno, e
// un'interfaccia che tenesse uno stato di autenticazione starebbe promettendo
// qualcosa che il server non fa.
//
// La dichiarazione si prende dalla query string all'avvio, perché è così che il
// backend la accetta sulla navigazione e così che un link con `?person=…` resta
// condivisibile. Non è un segreto, e condividerlo non è un incidente.

import { el, testo, riempi, sezione, riquadroErrore, nulla } from "./lib/dom.js";
import { dichiara, persona } from "./lib/api.js";
import { strisciaEventi } from "./lib/eventi.js";
import { coda } from "./pagine/coda.js";
import { lettore } from "./pagine/lettore.js";
import { ricerca } from "./pagine/ricerca.js";
import { registri } from "./pagine/registri.js";
import { coorte } from "./pagine/coorte.js";

/** Il corso corrente. Cambia dalla barra in alto e basta. */
let corso = null;

/**
 * La dichiarazione e il corso, presi dalla query string **prima di ogni altro
 * codice**.
 *
 * `?person=` è il modo in cui il backend accetta l'identità su una navigazione
 * e su un `EventSource`, ed è così che l'interfaccia la prende. Il corso è
 * accettato per la stessa ragione: un link alla coda di ratifica è una cosa che
 * si manda a un collega, e un link che chiede di digitare il corso prima di
 * funzionare non è un link.
 *
 * Entrambi sono dichiarazioni e nomi, non segreti: condividerli non è un
 * incidente, ed è la ragione per cui `src/identity.rs` lo dichiara in tre
 * punti.
 */
const parametri = new URLSearchParams(window.location.search);
dichiara(parametri.get("person") ?? "");
corso = (parametri.get("corso") ?? "").trim() || null;

/** L'elemento dove va la vista. */
const area = document.getElementById("vista");

/** La striscia degli eventi, che sopravvive al cambio di vista. */
const eventi = strisciaEventi(null, (evento) => {
  // Un evento non è uno stato: è un avviso che il mondo è cambiato. L'unica
  // reazione onesta è rileggere, e ogni vista decide da sola come.
  if (evento.nome === "ratificato" || evento.nome === "pubblicato" || evento.nome === "ratifica-ritirata") {
    instrada();
  }
});
document.getElementById("eventi").append(eventi.nodo);

// ── la dichiarazione ─────────────────────────────────────────────────────────

const campoPersona = document.getElementById("persona");
const campoCorso = document.getElementById("corso");

campoPersona.addEventListener("change", () => {
  dichiara(campoPersona.value);
  campoPersona.value = persona() ?? "";
  aggiornaDichiarazione();
  instrada();
});

campoCorso.addEventListener("change", () => {
  corso = campoCorso.value.trim() || null;
  campoCorso.value = corso ?? "";
  eventi.aggiorna(corso);
  instrada();
});

/** Il riquadro che dice che cosa una dichiarazione è, e che cosa non è. */
function aggiornaDichiarazione() {
  const riquadro = document.getElementById("dichiarazione");
  const chi = persona();
  riempi(
    riquadro,
    testo(
      chi
        ? `Hai dichiarato ${chi}. `
        : "Non hai dichiarato nessuno. ",
    ),
    testo(
      chi
        ? "È una dichiarazione, non un'autenticazione: nessuno l'ha verificata, e chiunque possa raggiungere questo server può dichiarare chi è. Ciò che protegge il materiale è la relazione che il docente ha registrato, non questa casella."
        : "Senza dichiarazione il server risponde 401. Dichiarare non è autenticare: dichiara e vedrai, ma nessuno ti riconoscerà.",
    ),
  );
}

// ── l'instradamento ──────────────────────────────────────────────────────────

/** La rotta dell'interfaccia, in forma leggibile. */
function rottaCorrente() {
  const grezzo = window.location.hash.replace(/^#/, "");
  const [percorso_, query] = grezzo.split("?");
  const parti = percorso_.split("/").filter(Boolean);
  return {
    vista: parti[0] ?? "",
    argomento: parti[1] ?? null,
    query: new URLSearchParams(query ?? ""),
  };
}

const VISTE = {
  corsi: paginaCorsi,
  argomento: (c) => lettore({ nodo: area, id: c.argomento }),
  cerca: (c) => ricerca({ nodo: area, q: c.query.get("q") ?? "" }),
  coda: paginaCoda,
  registri: paginaRegistri,
  coorte: (c) => coorte({ nodo: area, id: c.argomento }),
};

async function instrada() {
  const corrente = rottaCorrente();
  // Il canale eventi segue il corso della barra in alto, e non la vista: un
  // canale aperto sul corso sbagliato mostrerebbe l'attività di un corso che
  // non è quello che si sta guardando, e la partizione tornerebbe una
  // decorazione.
  eventi.aggiorna(corso);
  aggiornaNavigazione(corrente.vista);
  const vista = VISTE[corrente.vista] ?? paginaCorsi;
  riempi(area);
  try {
    await vista(corrente);
  } catch (errore) {
    riempi(area, sezione("Qualcosa non ha funzionato", riquadroErrore(errore)));
  }
}

function aggiornaNavigazione(vista) {
  for (const link of document.querySelectorAll("[data-vista]")) {
    link.classList.toggle("attiva", link.dataset.vista === vista);
  }
}

// ── le pagine ────────────────────────────────────────────────────────────────

/**
 * La pagina iniziale: che cosa si può fare da qui, e che cosa non si può.
 *
 * Non c'è un indice dei corsi perché il server non ha una rotta che lo dia. È
 * un buco vero — e un buco detto è un buco che un docente può aggirare
 * chiedendo l'id, mentre un buco taciuto sembrerebbe che l'id sia qualcosa che
 * l'interfaccia dovrebbe sapere.
 */
function paginaCorsi() {
  riempi(area, [
    sezione(
      "Da dove cominciare",
      el("p", {}, [
        "Questa è l'interfaccia del corpus di una scuola. Serve a tre cose: leggere un argomento e le sue affermazioni, decidere che cosa entra in uso, e guardare i registri di uno studente.",
      ]),
      el("p", { class: "nota" }, [
        "Non c'è un elenco dei corsi: questo server non ha una rotta che dica «i corsi in cui sei», e i corsi sono relazioni, non un campo. Scrivi l'id del corso nella barra in alto.",
      ]),
      el("ul", { class: "indice" }, [
        el("li", {}, [
          el("a", { href: "#/cerca" }, ["Cerca"]),
          " — trova un argomento per una parola. La ricerca dà solo indirizzi: ogni risultato viene riaperto.",
        ]),
        el("li", {}, [
          el("a", { href: "#/coda" }, ["Coda di ratifica"]),
          " — che cosa aspetta una tua decisione, e dove si ratifica, pubblica e ritira.",
        ]),
        el("li", {}, [
          el("a", { href: "#/registri" }, ["Registri"]),
          " — che cosa ha dimostrato uno studente e chi ha deciso che cosa.",
        ]),
      ]),
    ),
    sezione(
      "Che cosa questo software non è",
      el("ul", { class: "indice" }, [
        el("li", {}, [
          "Non c'è un modello dentro il prodotto. L'LLM è uno strumento del docente, esterno: qui dentro non gira niente e nessun dato dello studente esce, perché nel prodotto non entra niente che lo tocchi.",
        ]),
        el("li", {}, [
          "Lo studente non usa un modello. Legge il materiale, scrive esercizi e chiede al docente.",
        ]),
        el("li", {}, [
          "Questa interfaccia non scrive registri e non crea materiale: quelle sono azioni che passano dalla CLI e dal file di sorgente, e trasformarle in bottoni le renderebbe moduli da compilare.",
        ]),
      ]),
    ),
  ]);
}

/** La coda, con il corso preso dalla barra in alto. */
async function paginaCoda() {
  if (!corso) {
    riempi(
      area,
      sezione(
        "Coda di ratifica",
        nulla("Scrivi l'id del corso nella barra in alto: la coda è di un corso, e questo server non ha un modo di dire quali corsi hai."),
      ),
    );
    return;
  }
  await coda({ nodo: area, corso });
}

/** I registri, con corso e studente dalla barra in alto e dalla rotta. */
async function paginaRegistri(corrente) {
  if (!corso) {
    riempi(
      area,
      sezione(
        "Registri",
        nulla("Scrivi l'id del corso nella barra in alto: il registro è di uno studente in un corso."),
      ),
    );
    return;
  }
  const studente = corrente.query.get("person") ?? persona();
  await registri({
    nodo: area,
    corso,
    studente,
    argomento: corrente.query.get("argomento") ?? null,
  });
}

// ── l'avvio ──────────────────────────────────────────────────────────────────

/**
 * La dichiarazione e il corso dalla query string.
 *
 * `?person=` è il modo in cui il backend accetta l'identità su una navigazione
 * e su un `EventSource`, ed è così che l'interfaccia la prende. Il corso è
 * accettato per la stessa ragione: un link alla coda di ratifica è una cosa che
 * si manda a un collega, e un link che chiede di digitare il corso prima di
 * funzionare non è un link.
 *
 * Entrambi sono dichiarazioni e nomi, non segreti: condividerli non è un
 * incidente, ed è la ragione per cui `src/identity.rs` lo dichiara in tre punti.
 */
campoPersona.value = persona() ?? "";
campoCorso.value = corso ?? "";
aggiornaDichiarazione();
window.addEventListener("hashchange", instrada);
instrada();
