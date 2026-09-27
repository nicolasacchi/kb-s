// Il metro della padronanza: che cosa hai dimostrato, e con quale prova.
//
// # Qui non c'è una percentuale, e non è una scelta di gusto
//
// «La vista dello studente non mostra percentuali: mostra righe con prova e un
// bottone per contestarle.» Una percentuale è una somma che lo studente non può
// smontare, e una somma che non si può smontare diventa presto la funzione
// obiettivo di chi la guarda. Qui i numeri sono tre e sono tutti conteggi: quanti
// esercizi distinti, quante prove, quanti giorni fa. E sono numeri che si
// aprono: ogni riga ha la sua prova, e ogni prova è la riga del registro da cui
// viene.
//
// # Il verdetto non ha un colore di successo
//
// Nel codice esiste una classe `ok` e questa pagina non la usa per il verdetto.
// Il corpus lo dice per il metro: «il numero non si muove da solo, non ha un
// colore che segnala il successo, e non ha una barra che si riempie col tempo».
// Un verde è una barra che si riempie col tempo.
//
// # La vista giustificata si apre e si chiude
//
// Ogni riga ha un `perché` chiuso. Non è una sezione in fondo alla pagina e non è
// la disposizione di default: si apre quando lo chiedi e si chiude quando lo
// chiedi. Il metro è il verdetto; la giustificazione è un altro atto.
//
// # Che cosa si contesta
//
// Non il verdetto: il metro è una funzione del registro e un calcolo non si
// contesta, si ricontesta la prova che lo ha prodotto. Perciò il bottone porta
// ai registri, dove la contestazione sta dentro la riga del giudizio, che è
// l'unica cosa che in questo software è difendibile davanti a un ricorso.
//
// # Sotto soglia la quota non è uno zero
//
// La quota di classe è l'aggregato del docente (D9) e sotto `k` non esiste. La
// pagina lo dice con una frase, con le stesse parole della pagina dei segnali di
// coorte, perché sono la stessa regola e due modi di dirla sarebbero due
// convenzioni.

import { el, riempi, sezione, tabella, riquadroErrore, nulla } from "../lib/dom.js";
import { get, persona } from "../lib/api.js";

/** I millisecondi di un giorno, per scrivere gli orizzonti in giorni. */
const GIORNO = 86_400_000;

/**
 * La pagina del metro.
 *
 * @param {{nodo: HTMLElement, corso: string|null, studente: string|null,
 *          classe?: string|null, at?: string|null}} contesto
 * @returns {Promise<void>}
 */
export async function padronanza({ nodo, corso, studente, classe, at }) {
  if (!corso) {
    riempi(
      nodo,
      sezione(
        "Il metro della padronanza",
        nulla("Scrivi l'id del corso nella barra in alto: il metro è di un corso, e questo server non ha un modo di dire quali corsi hai."),
      ),
    );
    return;
  }
  if (!studente) {
    riempi(
      nodo,
      sezione(
        "Il metro della padronanza",
        nulla("Serve l'id dello studente di cui vuoi il metro. Il server non ha una rotta che dica «i miei studenti»: le relazioni le registra il docente e nessuna pagina le elenca."),
      ),
    );
    return;
  }

  riempi(nodo, el("p", { class: "caricamento" }, ["Calcolo il metro…"]));

  const query = { person: studente };
  if (at) {
    query.at = at;
  }
  let risposta;
  try {
    risposta = await get("padronanza", { corso }, { query });
  } catch (errore) {
    riempi(nodo, sezione(`Il metro · ${studente}`, riquadroErrore(errore), notaAmbito(errore)));
    return;
  }

  const righe = risposta.rows ?? [];
  riempi(nodo, [
    sezione(
      `Il metro · ${studente} · ${corso}`,
      introduzione(risposta.criterion, risposta.at),
      righe.length === 0 ? nullaRighe() : tabellaRighe(righe, risposta.criterion, studente),
    ),
    await sezioneQuota({ nodo, corso, classe }),
    sezione("Che cosa resta, quanto tempo, e chi ne risponde", conservazione()),
    sezione("Dove si scrive", doveSiScrive()),
  ]);
}

/** Il criterio in chiaro, e l'istante a cui è stato guardato. */
function introduzione(criterio, at) {
  return el("p", { class: "nota" }, [
    "Questo non è una percentuale e non è un livello. È il criterio con cui è stato letto il registro delle tue dimostrazazioni, dichiarato per intero: un argomento è ",
    el("strong", {}, ["dimostrato"]),
    ` quando ${criterio.threshold} esercizi distinti sono stati dimostrati senza aiuto, l'ultima verifica non assistita è riuscita, le dimostrazioni coprono almeno ${giorni(criterio.retention_horizon)} giorni e l'ultima verifica risale a non più di ${giorni(criterio.freshness_window)} giorni fa.`,
    " È una convenzione dichiarata, non una verità scoperta: nessuno ha misurato che otto sia il numero giusto, e nessuno dirà che è sbagliato.",
    el("br", {}),
    `Valutato a ${quando(at)}. La colonna «ultima verifica» è la data che distingue una misura da una fotografia.`,
  ]);
}

/** Il verdetto detto per intero, mai trasformato in numero. */
function dichiara(verdetto, criterio) {
  const dentro = typeof verdetto === "string" ? null : verdetto?.not_proven;
  if (verdetto === "proven") {
    return el("span", { class: "col-stato" }, ["dimostrato"]);
  }
  if (dentro === "no_proof") {
    return frase("nessuna dimostrazione non assistita su questo argomento");
  }
  if (dentro === "last_wrong") {
    return frase("l'ultima verifica non assistita è fallita: la padronanza si regge sull'ultima parola, non sul totale");
  }
  if (dentro?.below_threshold) {
    const mancanti = criterio.threshold - dentro.below_threshold.proved;
    return frase(
      mancanti > 0
        ? `gli esercizi distinti dimostrati senza aiuto sono ${dentro.below_threshold.proved}, ne mancano ${mancanti} alla soglia di ${criterio.threshold}`
        : `gli esercizi distinti sono ${dentro.below_threshold.proved}`,
    );
  }
  if (dentro?.not_spaced) {
    return frase(
      `le dimostrazioni coprono ${dentro.not_spaced.days} giorni e l'orizzonte di ritenzione è ${giorni(criterio.retention_horizon)}: risposte giuste tutte insieme sono la stessa prova ripetuta`,
    );
  }
  if (dentro?.stale) {
    return frase(
      `l'ultima verifica risale a ${dentro.stale.days} giorni fa e la finestra è ${giorni(criterio.freshness_window)}: il metro non certifica il mese scorso, e il compito ritardato è l'atto che lo rinnova`,
    );
  }
  // Un verdetto che questo software non conosce non è un verdetto positivo. Il
  // fallback esiste perché la risposta viene da un altro crate e «non lo so» non
  // può diventare «dimostrato» per silenzio.
  return frase("il verdetto non è fra quelli che questo software conosce: nessuna percentuale qui, e nessuna assunzione");
}

/** Una frase, senza colore di successo e senza barra. */
function frase(testo) {
  return el("span", { class: "col-stato" }, [testo]);
}

/** Le righe del metro: una per argomento, con la prova dentro. */
function tabellaRighe(righe, criterio, studente) {
  return tabella(
    ["Argomento", "Il verdetto", "Le prove", "Ultima verifica", "Perché", "Contesta"],
    righe.map((r) => [
      el("a", { href: `#/argomento/${r.argument}` }, [r.argument]),
      dichiara(r.verdetto, criterio),
      el("details", { class: "prove" }, [
        el("summary", {}, [`${r.proofs.length} ${r.proofs.length === 1 ? "prova" : "prove"} · ${r.proved_exercises} esercizi distinti`]),
        r.proofs.length === 0
          ? el("p", { class: "nota" }, ["Nessuna prova non assistita da leggere qui: le prove assistite e quelle ad aiuto ignoto non entrano nel metro, e restano nei registri."])
          : tabellaProve(r.proofs),
      ]),
      r.last_proof ? quando(r.last_proof) : "—",
      el("details", { class: "perche" }, [
        el("summary", {}, ["perché"]),
        percheRiga(r, criterio),
      ]),
      el("a", { href: `#/registri?person=${studente}&argomento=${r.argument}` }, ["contesta la prova"]),
    ]),
    { colonne: [{}, { class: "col-stato" }, {}, {}, {}, {}] },
  );
}

/** Le prove di una riga: quando, su che cosa, e se è riuscita. */
function tabellaProve(prove) {
  return tabella(
    ["Quando", "Esercizio", "Istanza", "Esito", "Riga"],
    prove.map((p) => [
      quando(p.at),
      el("code", {}, [p.exercise]),
      el("code", {}, [p.instance]),
      p.correct ? "riuscita" : "fallita",
      el("code", {}, [p.observation]),
    ]),
  );
}

/** Il perché di una riga: chiuso, e aperto solo se lo chiedi. */
function percheRiga(r, criterio) {
  return el("p", { class: "nota" }, [
    `Soglia: ${criterio.threshold} esercizi distinti dimostrati senza aiuto. Ultima parola: l'ultima verifica non assistita deve essere riuscita. Ritenzione: le dimostrazioni devono coprire almeno ${giorni(criterio.retention_horizon)} giorni. Freschezza: l'ultima verifica deve risalire a non più di ${giorni(criterio.freshness_window)} giorni fa.`,
    r.first_proof
      ? ` Prima prova non assistita il ${quando(r.first_proof)}.`
      : " Nessuna prova non assistita registrata.",
    " Il verdetto è una funzione del registro e non si scrive da nessuna parte: la perdita e la rivincita sono l'ultima parola e la data dell'ultima verifica, non due righe da aggiungere.",
  ]);
}

/** Un corso senza argomenti visibili: un elenco vuoto che è una risposta vera. */
function nullaRighe() {
  return nulla("Nessun argomento da valutare in questo corso. Non è che non ci sia niente da dimostrare: è che non ci sono argomenti che tu possa vedere.");
}

/**
 * La quota della classe, che è del docente.
 *
 * @returns {Promise<HTMLElement|null>}
 */
async function sezioneQuota({ corso, classe }) {
  if (!classe) {
    return null;
  }
  let risposta;
  try {
    // `quotaPadronanza`, come la chiama `rotte.js`: un nome che `ROTTE` non
    // dichiara fa lanciare `percorso`, e lanciare è la risposta giusta — un
    // percorso inventato tornerebbe `404`, che è la risposta che non distingue.
    risposta = await get("quotaPadronanza", { corso }, { query: { cohort: classe } });
  } catch (errore) {
    return sezione(
      `Quota della classe ${classe}`,
      riquadroErrore(errore),
      el("p", { class: "nota" }, [
        "La quota di una classe è del docente che insegna il corso. Il server risponde la stessa cosa per un corso che non esiste e per uno su cui non insegni, e non distingue i due casi.",
        persona() ? ` Hai dichiarato ${persona()}, e la dichiarazione non è un'autenticazione.` : "",
      ]),
    );
  }
  if (risposta.quota === null) {
    return sezione(`Quota della classe ${classe}`, quotaAssente(risposta.criterion));
  }
  return sezione(`Quota della classe ${classe}`, tabellaQuota(risposta.quota, risposta.criterion), notaQuota());
}

/** Sotto `k` la quota non esiste, e la pagina lo dice con una frase. */
function quotaAssente(criterio) {
  return el("div", { class: "vuoto-senza-soglia" }, [
    el("p", { class: "nulla" }, [
      "Nessuna quota di classe. Non è che la classe non abbia dimostrato niente: è che sotto la soglia di D9 l'aggregato non esiste, e quindi non c'è nulla da mostrare — non uno zero, non un segnaposto.",
    ]),
    el("p", { class: "nota" }, [
      "La soglia è un accesso, non una cancellazione: le osservazioni individuali esistono e si leggono nei registri. Quello che qui non esiste è la somma, perché con un numero piccolo chi legge trova i nomi conoscendo i propri studenti. Il criterio che valeva è comunque dichiarato: ",
      el("strong", {}, [`${criterio.threshold} esercizi distinti senza aiuto, almeno ${giorni(criterio.retention_horizon)} giorni fra la prima e l'ultima, ultima verifica entro ${giorni(criterio.freshness_window)} giorni.`]),
    ]),
  ]);
}

/** La quota, quando esiste, con le cifre e il denominatore. */
function tabellaQuota(quota, criterio) {
  const denominatore = quota.students * quota.arguments;
  return [
    tabella(
      ["Cosa", "Quanta"],
      [
        ["Persone osservate", el("strong", {}, [String(quota.students)])],
        ["Argomenti del corso", String(quota.arguments)],
        ["Passaggi da non dimostrato a dimostrato", el("strong", {}, [String(quota.transitions)])],
        ["Quota", `${quota.transitions} su ${denominatore} = ${quota.quota.toFixed(3)}`],
        ["Finestra", `${quando(quota.from)} → ${quando(quota.to)}`],
        ["Criterio", `${criterio.threshold} esercizi distinti, ${giorni(criterio.retention_horizon)} giorni di ritenzione, ${giorni(criterio.freshness_window)} giorni di freschezza`],
      ],
      { colonne: [{ class: "col-chiave" }, {}] },
    ),
    el("p", { class: "nota" }, [
      "La quota conta i passaggi, non gli stati finali: un argomento già dimostrato prima della finestra non passa una seconda volta. E il denominatore è persone per argomenti, non righe.",
    ]),
  ];
}

/** Che cosa dice la claim, e che cosa questa pagina non fa. */
function notaQuota() {
  return el("p", { class: "nota" }, [
    "La claim del prodotto è che questa quota sia almeno il doppio della stessa quota in una classe di controllo che segue il protocollo identico senza il sistema, con lo stesso criterio di soglia. Questa pagina mostra una sola classe e non fa il confronto: il confronto è fra due braccia e va fatto leggendo la stessa rotta con la classe dell'altra braccia, e il criterio è dichiarato in entrambe le risposte perché due soglie diverse renderebbero il confronto privo di significato.",
  ]);
}

/** La conservazione, dichiarata dove la legge chi la legge. */
function conservazione() {
  return el("ul", { class: "indice" }, [
    el("li", {}, [
      el("strong", {}, ["Che cosa resta. "]),
      "Le righe del registro delle dimostrazioni, e nient'altro. Il metro non scrive e non ha una tabella: conservare il metro è conservare il registro che si conserva già, e non esiste una copia del metro da cancellare a parte.",
    ]),
    el("li", {}, [
      el("strong", {}, ["Per quanto. "]),
      "Il metro non ha un termine proprio e non ne chiede uno: il termine è quello del registro, e lo dichiara l'istituto che tiene i dati, non il software che li legge. Il minimo di riferimento per i registri dell'impianto è di sei mesi (AI Act art. 26(6)) ed è un pavimento della pagina dell'istituto, non una scelta di questo programma.",
    ]),
    el("li", {}, [
      el("strong", {}, ["Chi ne risponde. "]),
      "Chi tiene il registro, cioè l'istituto, e per nome chi lo amministra. Un software non diventa il titolare del dato diventando preciso.",
    ]),
    el("li", {}, [
      el("strong", {}, ["La tensione, non risolta. "]),
      "La cancellazione è incompatibile con l'append-only e non è un dettaglio implementativo: i trigger del registro vietano la cancellazione, quindi questo programma non cancella niente e non promette di saperlo fare. Il diritto all'oblio è una procedura dell'istituto — esportare e poi rimuovere le righe di quella persona — e non è una riga di codice. Dichiarare il vincolo è il compito di questa pagina; risolverlo è un'altra decisione.",
    ]),
  ]);
}

/** Dove si scrivono le dimostrazioni. */
function doveSiScrive() {
  return el("p", {}, [
    "Non qui. Questo server non ha rotte di scrittura dei registri: appendere un'osservazione è un atto del docente che passa dalla CLI e dal file di sorgente, e una pagina che accettasse la risposta trasformerebbe il registro da atto in un modulo da compilare. Il metro qui è solo lettura, e lo è perché non ha niente da scrivere.",
  ]);
}

/** Perché la pagina non si vede, quando non si vede. */
function notaAmbito(errore) {
  if (errore.tipo !== "assente") {
    return null;
  }
  return el("p", { class: "nota" }, [
    "Il metro di uno studente si legge da lui e da chi insegna il corso. Il server risponde la stessa cosa per un corso che non esiste e per uno su cui non insegni, e non distingue i due casi: la forma degli id è pubblica e un errore di visibilità che dice in che stato è la riga che non ti fa vedere è un canale.",
  ]);
}

/** La data, in secondi e senza fuso: i registri si confrontano per contenuto. */
function quando(millis) {
  return new Date(millis).toISOString().slice(0, 16).replace("T", " ");
}

/** Un orizzonte o una finestra, in giorni interi. */
function giorni(millis) {
  return Math.round(millis / GIORNO);
}
