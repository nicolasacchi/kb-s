// Il lettore: un argomento, **il suo testo**, le sue claim con i loro span, e
// il modo di verificarne una.
//
// # Che cosa rende un lettore un lettore
//
// Un argomento che mostra solo provenienza, ratifica e stato non è un
// argomento: sono metadati, e un docente che apre una lezione e legge una
// tabella di hash ha aperto una tabella di hash. Quindi la rotta
// `/api/v1/arguments/{id}` porta anche il **testo del file** e qui è renderizzato
// — come **dato**, mai come documento: entra nel DOM con `el("pre", …)` e con
// un nodo di testo dentro, perché il file è HTML non fidato (D14) e questa
// pagina non è l'origine in cui può girare.
//
// E quando il testo non c'è, la pagina **lo dice e dice perché**: un riquadro
// vuoto sarebbe una bugia. Vedi `motivoTesto`.
//
// # Che cosa rende verificabile una claim
//
// Non è lo stato: è il testo dello span. `span_anchor` da solo è un puntatore
// che il lettore non può aprire, e un'affermazione con un indirizzo torna a
// essere una dichiarazione. Quindi qui la regola è: **senza `span_text` non c'è
// niente da verificare**, e la claim si legge come non verificata — non come
// vera, non come confermata.
//
// # I tre stati che il registro tiene e che qui devono restare distinguibili
//
// * `supported` — uno span la sostiene;
// * `contradicted` — uno span c'è e **non** la sostiene. È il caso più
//   insidioso dei tre, perché lo span dà alla frase una credibilità falsa: il
//   lettore vede un riferimento e conclude che la verifica sia passata. Qui è
//   marcato come contraddetto e mostra lo span che non la sostiene;
// * `unciteable` — nessuno span la sostiene;
// * `retracted` — ritrattata, con la ragione.
//
// Il registro le tiene tutte e una pagina che ne mostrasse qualcuna farebbe a
// `kbs-core` il favore che nessuno chiede: un registro che cancella i propri
// errori non è un registro.
//
// # I due percorsi della D4 non si confondono
//
// Qui si vede il percorso **pubblicato**: generato o scritto, verificato,
// ratificato, in uso, citabile. Il percorso **speculativo** — per studente,
// senza cancello, usa e getta, mai citabile — non ha nessuna rotta in questo
// server, e la pagina lo dice invece di lasciare che l'assenza si legga come
// «non c'è niente».

import { el, testo, riempi, sezione, tabella, riquadroErrore, nulla } from "../lib/dom.js";
import { get, runtimeTreDimensioni } from "../lib/api.js";
import { datiScena, disegna, riquadroScena, impronta } from "../lib/scena.js";
import { provenienzaLunga } from "../lib/provenienza.js";

/**
 * La pagina di un argomento.
 *
 * @param {{nodo: HTMLElement, id: string}} contesto
 * @returns {Promise<void>}
 */
export async function lettore({ nodo, id }) {
  riempi(nodo, el("p", { class: "caricamento" }, ["Carico l'argomento…"]));
  let argomento;
  let materiale = null;
  let claim = [];
  try {
    const [letto, claims] = await Promise.all([
      get("argomento", { id }),
      get("claim", { id }),
    ]);
    argomento = letto.argument;
    materiale = letto.testo ?? null;
    claim = claims.claims ?? [];
  } catch (errore) {
    riempi(nodo, sezione("Argomento", riquadroErrore(errore)));
    return;
  }

  // Gli archi si costruiscono sui prerequisiti **dichiarati** dall'argomento, e
  // ogni carico passa dal predicato: un prerequisito che non si può leggere
  // semplicemente non porta archi, e non è un errore da mostrare.
  const altrove = new Map();
  const caricati = await Promise.all(
    (argomento.prerequisites ?? []).map(async (pre) => {
      try {
        const [a, c] = await Promise.all([
          get("argomento", { id: pre }, { query: { testo: "senza-contenuto" } }),
          get("claim", { id: pre }),
        ]);
        return [pre, { argomento: a.argument, claim: c.claims ?? [] }];
      } catch {
        return [pre, null];
      }
    }),
  );
  for (const [pre, dati] of caricati) {
    if (dati) {
      altrove.set(pre, dati);
    }
  }

  const dati = datiScena(argomento, claim, altrove);
  riempi(nodo, [
    sezione("Argomento", ...campiArgomento(argomento)),
    sezione("Il materiale", ...sezioneMateriale(materiale)),
    sezione(
      "Che cosa afferma",
      testo(
        `${claim.length} claim nel registro di questo argomento. Una claim è un fatto atomico con lo span che lo sostiene; senza span è una dichiarazione, e qui lo dice.`,
      ),
      tabellaClaim(claim),
    ),
    await sezioneScena(dati),
    sezione(
      "I due percorsi della D4",
      el("p", {}, [
        "Questa pagina mostra il percorso pubblicato: materiale generato o scritto, verificato, ratificato dal docente, in uso e citabile. Un argomento non ratificato resta leggibile ma non citabile, e la pagina lo dice sul titolo.",
      ]),
      el("p", {}, [
        "Il percorso speculativo — per studente, senza cancello, usa e getta, mai citabile — non ha nessuna rotta in questo server. Non lo vedi qui, e la sua assenza non significa che non esista: significa che questo server non lo espone.",
      ]),
    ),
  ]);
}

/** I campi dell'argomento, con la provenienza per intero (D10). */
function campiArgomento(argomento) {
  const ratifica = argomento.ratified;
  const valida =
    ratifica && ratifica.contract_hash === argomento.content_hash;
  return [
    el("h1", {}, [argomento.title]),
    el("p", { class: "sommario" }, [argomento.summary]),
    tabella(
      ["Campo", "Valore"],
      [
        ["Stato", statoDi(argomento)],
        ["Corso", argomento.course],
        ["Id", el("code", {}, [argomento.id])],
        [
          "Citabile adesso",
          valida && argomento.state === "in-corso"
            ? "sì"
            : "no — leggibile sì, citabile no (D4)",
        ],
        [
          "Provenienza",
          provenienzaLunga(argomento.origin),
        ],
        ["Percorso sorgente", argomento.rel_path ?? "nessuno (creato via API)"],
        ["Hash del contenuto", el("code", {}, [argomento.content_hash])],
        [
          "Prerequisiti",
          (argomento.prerequisites ?? []).length === 0
            ? "nessuno"
            : (argomento.prerequisites ?? [])
                .map((p) => el("a", { href: `#/argomento/${p}` }, [p]))
                .flatMap((n, i) => (i === 0 ? [n] : [", ", n])),
        ],
        [
          "Ratifica",
          ratifica
            ? el("span", { class: valida ? "ok" : "attenzione" }, [
                `${ratifica.by} il ${new Date(ratifica.at).toISOString().slice(0, 10)} — ${ratifica.note}`,
                valida
                  ? " — vale per l'hash corrente"
                  : ` — NON vale più: la ratifica copre ${ratifica.contract_hash} e il contenuto è ${argomento.content_hash}`,
              ])
            : el("span", { class: "attenzione" }, [
                "nessuna: questo argomento non è stato verificato da un docente",
              ]),
        ],
      ],
      { colonne: [{ class: "col-chiave" }, {}] },
    ),
  ];
}

/**
 * Il materiale: il testo del file, o la ragione per cui qui non c'è.
 *
 * Il testo arriva **come dato** — un campo di un JSON — e qui entra nel DOM
 * come nodo di testo dentro un `pre`. Non è un `innerHTML` e non è un `iframe`:
 * il file è HTML non fidato (D14) e questa pagina non è l'origine in cui
 * quella cosa può girare.
 *
 * Quello che qui non c'è, e va detto invece di lasciare che si creda che ci
 * sia: il **rendering**. Qui c'è il file d'ingresso parola per parola; ciò che
 * sta sotto il suo percorso — fogli di stile, immagini, script, scena — è
 * servito da un'altra porta di questo server, e questa pagina non la apre.
 *
 * @param {{stato: string, contenuto?: string, byte?: number, motivo?: string}|null} corpo
 * @returns {Array<Node>}
 */
function sezioneMateriale(corpo) {
  if (!corpo) {
    // Il campo non è arrivato. Non è un caso che questa pagina debba
    // contemplare: una risposta senza `testo` è una risposta di un server
    // diverso, e dirlo è l'unica cosa che qui non è inventare.
    return [
      el("p", { class: "attenzione" }, [
        "Questo server non ha mandato il testo del materiale. Quello che vedi sopra è tutto ciò che espone: nessun riquadro vuoto al suo posto, perché un riquadro vuoto sembrerebbe un argomento senza contenuto.",
      ]),
    ];
  }
  if (corpo.stato !== "presente") {
    return [
      el("p", { class: "attenzione" }, [
        "Il materiale di questo argomento non è qui, e il motivo è detto per intero:",
        el("div", { class: "motivo" }, [motivoTesto(corpo.motivo, corpo.byte)]),
      ]),
    ];
  }
  return [
    el("p", { class: "nota" }, [
      `Il file d'ingresso di questo argomento, parola per parola: ${corpo.byte} byte, nessun HTML ripulito e nessuna estrazione. È il sorgente del materiale, non il materiale reso.`,
    ]),
    el("pre", { class: "testo-artifact" }, [corpo.contenuto]),
    el("p", { class: "nota" }, [
      "Ciò che sta sotto il suo percorso — fogli di stile, immagini, script — non è in questa risposta: la rotta porta il file d'ingresso e nient'altro. Il rendering dell'artifact vive sull'origine per-argomento di questo server, e questa pagina non la costruisce: per farlo dovrebbe conoscere il suffisso degli host artifact del deployment, e quel valore non esce da nessuna rotta.",
    ]),
  ];
}

/**
 * Perché il testo non c'è, detto come lo direbbe chi lo scrive.
 *
 * Nessuna di queste ragioni è un `404` e nessuna riguarda chi ha chiesto: se
 * la pagina è qui, l'argomento era leggibile, e il suo file è suo.
 *
 * @param {string} motivo
 * @param {number|undefined} byte
 * @returns {string}
 */
function motivoTesto(motivo, byte) {
  const peso = typeof byte === "number" ? ` Il file è di ${byte} byte.` : "";
  switch (motivo) {
    case "nessun-file":
      return "L'argomento non ha un percorso di file: è nato via API, e un argomento senza file non ha un sorgente da leggere.";
    case "fuori-corpus":
      return "Il percorso che il registro dichiara per questo file non porta a un file dentro il corpus. Il percorso è stato rifiutato e non è stato indovinato: correggilo e il materiale torna qui.";
    case "non-leggibile":
      return "Il file c'è nel registro ma non si è potuto leggere dal disco. Non è una risposta sulla tua identità: il predicato ha già deciso che questo argomento è tuo, e il suo file è tuo.";
    case "troppo-grande":
      return `Il file supera il tetto di testo che questa rotta porta in un JSON, dichiarato in routes/arguments.rs come MAX_TESTO.${peso} Il file c'è: su quest'origine l'artifact si serve intero, e questa pagina non lo apre.`;
    case "non-testo":
      return `Il file non è UTF-8, e un artifact è HTML (D14) mentre l'HTML è UTF-8.${peso} Mostrarlo a metà sarebbe peggio che non mostrarlo.`;
    default:
      return `Motivo non dichiarato dal server: ${motivo}.${peso}`;
  }
}

/** Lo stato, detto come lo dice il dominio. */
function statoDi(argomento) {
  switch (argomento.state) {
    case "in-corso":
      return "in uso";
    case "del-docente":
      return "revisione del docente";
    case "bozza":
      return "bozza: invisibile a chi non l'ha scritta";
    case "archiviato":
      return "fuori uso, conservato";
    default:
      return argomento.state;
  }
}

// La provenienza non è qui: è in `../lib/provenienza.js`, ed è la versione per
// intero, l'unica delle due che mostra il `model lock` (D10). Le due versioni
// stanno nello stesso file perché una variante nuova di `Origin` deve avere un
// ramo in entrambe, e le due guardie in `tests/provenienza.rs` lo pretendono.

/** La tabella delle claim, con lo stato che non è mai implicito. */
function tabellaClaim(claim) {
  if (claim.length === 0) {
    return nulla(
      "Nessuna claim registrata su questo argomento. Il registro delle affermazioni è vuoto: non è la stessa cosa che un argomento senza affermazioni verificate, è un argomento di cui nessuno ha registrato le affermazioni.",
    );
  }
  return tabella(
    ["Claim", "Stato", "Span", "Emessa da", "Quando"],
    claim.map((c) => [
      el("div", {}, [el("span", { class: "claim-testo" }, [c.text]), el("div", { class: "claim-id" }, [el("code", {}, [c.id])])]),
      statoClaim(c),
      rigaSpan(c),
      rigaEmittente(c.emitted_by),
      new Date(c.emitted_at).toISOString().slice(0, 16).replace("T", " "),
    ]),
    {
      colonne: [{}, { class: "col-stato" }, { class: "col-span" }, {}, {}],
    },
  );
}

/** Lo stato di una claim, detto per esteso e mai per mezza parola. */
function statoClaim(c) {
  if (typeof c.status === "object" && c.status !== null) {
    return el("span", { class: "stato-claim ritratta" }, [
      "ritrattata",
      el("div", { class: "motivo" }, [c.status.reason]),
    ]);
  }
  switch (c.status) {
    case "supported":
      return el("span", { class: "stato-claim sostenuta" }, ["sostenuta da uno span"]);
    case "contradicted":
      return el("span", { class: "stato-claim contraddetta" }, [
        "contraddetta",
        el("div", { class: "motivo" }, [
          "c'è uno span e non la sostiene: è più peggio di non citabile, perché il riferimento dà una credibilità che la verifica non ha confermato",
        ]),
      ]);
    case "unciteable":
      return el("span", { class: "stato-claim non-citabile" }, [
        "non citabile",
        el("div", { class: "motivo" }, ["nessuno span la sostiene"]),
      ]);
    default:
      return c.status;
  }
}

/**
 * Lo span, o la sua assenza.
 *
 * Qui non c'è un segnaposto e non c'è un trattino: se lo span non c'è, la
 * colonna dice che non c'è e che cosa vuol dire.
 */
function rigaSpan(c) {
  if (c.span_text) {
    return el("div", {}, [
      el("blockquote", { class: "span" }, [c.span_text]),
      c.span_anchor ? el("div", { class: "ancora" }, [`ancora: ${c.span_anchor}`]) : null,
    ]);
  }
  return el("div", { class: "nessuno-span" }, [
    c.span_anchor
      ? `c'è un'ancora (${c.span_anchor}) ma non il testo dello span: l'affermazione non è verificabile da qui, e va letta come non verificata.`
      : "nessuno span: questa affermazione non è verificabile, e va letta come non verificata — non come vera e non come confermata.",
  ]);
}

/** Chi ha emesso la claim. Un modello non è un emittente: sta fuori dal prodotto. */
function rigaEmittente(e) {
  switch (e.kind) {
    case "teacher":
      return el("span", {}, ["un docente, ", el("code", {}, [e.by])]);
    case "content":
      return el("span", {}, [
        "il contenuto di ",
        el("a", { href: `#/argomento/${e.argument}` }, [e.argument]),
        ", non una persona al momento",
      ]);
    case "from-work":
      return el("span", {}, [`dal lavoro dello studente (osservazione ${e.observation})`]);
    default:
      return e.kind;
  }
}

/**
 * La sezione della scena, che attende il runtime e dice se c'è.
 *
 * Il runtime è chiesto al daemon e mai a un CDN (D15). Se non è vendorizzato
 * la sezione lo dice con la sua parola — non un riquadro vuoto, che sembrerebbe
 * una scena senza dati.
 */
async function sezioneScena(dati) {
  const figli = [
    el("p", { class: "nota" }, [
      "Ogni nodo e ogni arco corrisponde a una claim con il suo span (D15.1): quello che non è una claim citabile non entra nella scena, e sotto c'è il conto di ciò che è rimasto fuori. Il rendering è un effetto: «ridisegna» cambia la camera e lascia il dato com'è.",
    ]),
  ];
  if (dati.nodi.length === 0) {
    figli.push(
      nulla(
        "Nessuna claim sostenuta e con span su questo argomento: la scena resterebbe vuota, e una scena vuota sembrerebbe un materiale che non esiste. Il conto è quello sopra.",
      ),
    );
    for (const appunto of dati.appunti) {
      figli.push(el("p", { class: "appunto" }, [appunto]));
    }
    return sezione("La scena", ...figli);
  }

  const stato = await runtimeTreDimensioni();
  if (!stato.ok) {
    figli.push(
      el("p", { class: "attenzione" }, [
        "Il runtime three.js non è vendorizzato su questo server, quindi la scena non si disegna. È un errore di installazione, non un materiale che manca: D15 dice che il runtime è nel repository e servito dal binario, perché una classe può non avere rete.",
        el("div", { class: "motivo" }, [stato.motivo]),
      ]),
    );
    for (const appunto of dati.appunti) {
      figli.push(el("p", { class: "appunto" }, [appunto]));
    }
    return sezione("La scena", ...figli);
  }

  // Il runtime c'è, ma **disegnare** può fallire: una macchina senza WebGL è
  // una macchina vera, e in quel caso la scena non si disegna. Un'eccezione
  // qui non deve portare via la pagina: le claim, gli span e la provenienza
  // sono il contenuto, e la scena è un effetto. Quindi il fallimento del
  // rendering è dichiarato qui dentro, e il resto della pagina resta.
  const riquadro = riquadroScena(dati);
  try {
    const THREE = await import(/* @vite-ignore */ stato.url);
    const maniglia = disegna(riquadro.area, dati, THREE, riquadro.scelta);
    riquadro.controlli.querySelector("#scena-ridisegna").addEventListener("click", () => {
      maniglia.ridisegna();
      // L'impronta si ristampa dopo il ridisegno: se cambiasse, il renderer
      // starebbe scrivendo il contenuto, e si vedrebbe qui.
      riquadro.controlli.querySelector("#scena-impronta").textContent = impronta(dati);
    });
  } catch (errore) {
    figli.push(
      el("p", { class: "attenzione" }, [
        "Il runtime three.js è servito da questo server, ma il browser non ha potuto disegnare: quasi certamente non c'è WebGL su questa macchina. Le claim e i loro span sono qui sopra e non dipendono dalla scena, perché il rendering è un effetto e il contenuto è il dato (D15.1.4).",
        el("div", { class: "motivo" }, [String(errore && errore.message ? errore.message : errore)]),
      ]),
    );
    for (const appunto of dati.appunti) {
      figli.push(el("p", { class: "appunto" }, [appunto]));
    }
    return sezione("La scena", ...figli);
  }

  return sezione("La scena", ...figli, riquadro.controlli, riquadro.area, riquadro.dettaglio,
    el("p", { class: "nota" }, [
      `${dati.nodi.length} nodi e ${dati.archi.length} archi. Ogni arco porta l'id della claim che lo dichiara; una relazione che il corpus non dichiara non viene disegnata.`,
    ]),
    ...dati.appunti.map((a) => el("p", { class: "appunto" }, [a])),
  );
}
