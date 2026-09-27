// La scena 3D — e la parte di questo file che conta.
//
// # Il rendering è un effetto, il contenuto è il dato  (D15.1.4)
//
// Il file è diviso in due metà che non si chiamano mai fra loro:
//
// * `datiScena` è **puro**: prende un argomento e le sue claim e restituisce
//   un oggetto con nodi, archi e un conto di ciò che è rimasto fuori. Non
//   importa niente di three, non crea un canvas, non legge il DOM. Si può
//   chiamare due volte e dà due volte lo stesso risultato.
// * `disegna` prende quei dati e un runtime e disegna. Cambiare camera,
//   ricaricare la pagina o cambiare renderer **non cambia i dati**: il
//   pulsante «ridisegna» chiama `disegna` di nuovo sugli stessi `dati`, e
//   sotto la scena c'è l'impronta dei dati, così si vede che è rimasta
//   quella. Se la scena fosse l'unica rappresentazione, un aggiornamento del
//   renderer cancellerebbe il materiale dello studente.
//
// # Ogni nodo e ogni arco è una claim  (D15.1.1)
//
// Un oggetto 3D senza claim non entra nell'indice condiviso: resta
// speculativo. Quindi:
//
// * un **nodo** è una claim con `span_text` e stato `supported` — cioè una
//   claim che si può citare. Una claim contraddetta, non citabile o ritratta
//   **non** diventa un nodo, e il conto di quante non lo sono è scritto sotto
//   la scena, perché una scena che mostra solo ciò che regge e tace il resto è
//   una propaganda, non una visualizzazione;
// * un **arco** porta con sé l'id della claim che lo giustifica. Non esiste un
//   arco che non risalga a una claim: se il corpus non dichiara una relazione,
//   la scena non la disegna, e lo dice nel conto.
//
// La relazione che il corpus dichiara è una sola e non è inventata qui:
// `emitted_by: {kind: "content", argument: X}` dice che la claim è stata
// emessa dal contenuto dell'argomento ratificato `X`, e
// `Argument.prerequisites` dice che `X` viene prima. L'arco va dalla claim
// emessa al nodo sostenuto di `X`, e porta l'id della claim emessa.
//
// # Nessuna risposta visibile  (D15.1.2)
//
// Qui dentro non c'è nessun esercizio e nessuna risposta: il modulo non riceve
// `Exercise` né `Instance` e non sa che esistano. Il `GUARDIAN` di un artifact
// non può diventare un oggetto ruotabile perché qui non c'è niente da
// ruotare che sia una soluzione.
//
// Il runtime è importato **dinamicamente** e solo dopo che la rotta ha
// risposto `200` (`api.runtimeTreDimensioni`): se il runtime non è
// vendorizzato, il server risponde `500` e dice `D15`, e la scena dice che il
// runtime non c'è invece di mostrare un riquadro vuoto che sembrerebbe una
// scena senza dati.

import { el, testo, riempi } from "./dom.js";

/**
 * Il dato della scena. Nessun three, nessun DOM.
 *
 * @param {object} argomento l'argomento letto
 * @param {Array<object>} claim le claim di quell'argomento
 * @param {Map<string, {argomento: object, claim: Array<object>}>} altrove
 *   gli altri argomenti già caricati, per id: serve a costruire gli archi
 * @returns {{nodi: Array<object>, archi: Array<object>, scartate: object, appunti: Array<string>}}
 */
export function datiScena(argomento, claim, altrove = new Map()) {
  const appunti = [];
  const citabili = claim.filter((c) => eCitabile(c));
  const scartate = {
    contraddette: claim.filter((c) => c.status === "contradicted").length,
    nonCitabili: claim.filter((c) => c.status === "unciteable").length,
    ritratte: claim.filter((c) => typeof c.status === "object" && c.status !== null).length,
    senzaSpan: claim.filter((c) => !c.span_text).length,
  };

  // Le posizioni sono deterministiche: stesso corpus, stessa figura. Una scena
  // che si muove a ogni ricaricamento fa pensare che sia cambiato qualcosa, e
  // in una scena l'unica cosa che conta è che ciò che si vede corrisponda a ciò
  // che il registro dice.
  const nodi = citabili.map((c, i) => ({
    claim: c.id,
    testo: c.text,
    span: c.span_text,
    argomento: c.argument,
    posizione: posizioneFilla(i, citabili.length),
  }));

  const perId = new Map(nodi.map((n) => [n.claim, n]));
  const archi = [];
  for (const c of citabili) {
    if (!c.emitted_by || c.emitted_by.kind !== "content") {
      continue;
    }
    const fonte = altrove.get(c.emitted_by.argument);
    if (!fonte) {
      continue;
    }
    const arrivo = fonte.claim.find((altra) => eCitabile(altra));
    if (!arrivo) {
      continue;
    }
    const nodoPartenza = perId.get(c.id);
    const nodoArrivo = perId.get(arrivo.id) ?? {
      claim: arrivo.id,
      testo: arrivo.text,
      span: arrivo.span_text,
      argomento: arrivo.argument,
      posizione: posizioneFilla(nodi.length, nodi.length + 1),
    };
    if (nodoPartenza === nodoArrivo) {
      continue;
    }
    archi.push({
      // L'id della claim che dichiara la relazione: senza, l'arco sarebbe un
      // segno che non risponde a niente.
      claim: c.id,
      da: nodoPartenza.claim,
      a: nodoArrivo.claim,
    });
  }

  if (archi.length === 0) {
    appunti.push(
      "Nessun arco: il corpus non dichiara nessuna relazione fra le claim di questo argomento, e una relazione non dichiarata non viene disegnata.",
    );
  }
  if (scartate.senzaSpan > 0) {
    appunti.push(
      `${scartate.senzaSpan} claim non hanno span: senza il testo dello span l'affermazione è una dichiarazione con un indirizzo, e in scena non c'è.`,
    );
  }
  if (scartate.contraddette > 0) {
    appunti.push(
      `${scartate.contraddette} claim sono contraddette: uno span c'è e non sostiene, quindi non entrano nell'output citabile e non diventano nodi.`,
    );
  }

  return { nodi, archi, scartate, appunti, argomento: argomento.id };
}

/** Una claim entra nella scena solo se si può citare e ha il testo dello span. */
function eCitabile(claim) {
  return claim.status === "supported" && Boolean(claim.span_text);
}

/** La posizione di un nodo: fillotassi su sfera, quindi stabile e senza casualità. */
function posizioneFilla(indice, totale) {
  if (totale <= 1) {
    return [0, 0, 0];
  }
  const t = (indice + 0.5) / totale;
  const phi = Math.acos(1 - 2 * t);
  const theta = Math.PI * (1 + Math.sqrt(5)) * indice;
  const r = 6;
  return [
    r * Math.sin(phi) * Math.cos(theta),
    r * Math.sin(phi) * Math.sin(theta),
    r * Math.cos(phi),
  ];
}

/**
 * L'impronta dei dati: ciò che la scena mostra, in una riga.
 *
 * Serve a rendere **visibile** la separazione di D15.1.4: se il pulsante
 * «ridisegna» cambiasse questa riga, il renderer starebbe scrivendo il
 * contenuto, e si vedrebbe.
 */
export function impronta(dati) {
  return `${dati.nodi.length} nodi, ${dati.archi.length} archi`;
}

/**
 * Disegna i dati con un runtime three.
 *
 * @param {HTMLElement} contenitore dove disegnare
 * @param {ReturnType<typeof datiScena>} dati il dato, **non** ricalcolato qui
 * @param {object} THREE il runtime vendorizzato
 * @param {(claim: string) => object} dettaglio come ottenere la claim
 * @returns {{ridisegna: (distanza: number) => void, libera: () => void, scelta: (claim: string|null) => void}}
 */
export function disegna(contenitore, dati, THREE, dettaglio) {
  const scena = new THREE.Scene();
  scena.background = new THREE.Color(0x101418);

  const macchina = new THREE.PerspectiveCamera(50, 16 / 9, 0.1, 200);
  macchina.position.set(0, 0, 18);

  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setSize(contenitore.clientWidth || 640, contenitore.clientHeight || 360);
  renderer.outputColorSpace = THREE.SRGBColorSpace;

  scena.add(new THREE.AmbientLight(0xffffff, 0.7));
  const luce = new THREE.DirectionalLight(0xffffff, 0.8);
  luce.position.set(5, 8, 12);
  scena.add(luce);

  const geometria = new THREE.SphereGeometry(0.35, 16, 12);
  const materiale = new THREE.MeshStandardMaterial({ color: 0x7fb2ff });
  const gruppi = new Map();
  for (const nodo of dati.nodi) {
    const mesh = new THREE.Mesh(geometria, materiale);
    mesh.position.set(nodo.posizione[0], nodo.posizione[1], nodo.posizione[2]);
    mesh.userData.claim = nodo.claim;
    scena.add(mesh);
    gruppi.set(nodo.claim, mesh);
  }

  const punti = [];
  for (const arco of dati.archi) {
    const da = gruppi.get(arco.da) ?? cercaPosizione(dati, arco.da);
    const a = gruppi.get(arco.a) ?? cercaPosizione(dati, arco.a);
    punti.push(da[0], da[1], da[2], a[0], a[1], a[2]);
  }
  let linee = null;
  if (punti.length > 0) {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(punti, 3));
    linee = new THREE.LineSegments(geo, new THREE.LineBasicMaterial({ color: 0x4b5a6a }));
    scena.add(linee);
  }

  riempi(contenitore, renderer.domElement);

  // L'interazione è il rapporto, non la vista (D15.1.3): si clicca un nodo e
  // si legge la claim che lo giustifica. Ruotare la scena non insegna niente;
  // scegliere fra due affermazioni e vederne lo span sì.
  const raggio = new THREE.Raycaster();
  const puntatore = new THREE.Vector2();
  const scegli = (evento) => {
    const rett = renderer.domElement.getBoundingClientRect();
    puntatore.x = ((evento.clientX - rett.left) / rett.width) * 2 - 1;
    puntatore.y = -((evento.clientY - rett.top) / rett.height) * 2 + 1;
    raggio.setFromCamera(puntatore, macchina);
    const colpiti = raggio.intersectObjects([...gruppi.values()], false);
    if (colpiti.length > 0) {
      scegli.claim = colpiti[0].object.userData.claim;
      dettaglio?.(scegli.claim);
      for (const [id, mesh] of gruppi) {
        mesh.material = id === scegli.claim
          ? new THREE.MeshStandardMaterial({ color: 0xffd479 })
          : materiale;
      }
    }
  };
  renderer.domElement.addEventListener("click", scegli);

  let trascinata = null;
  let rotazione = 0;
  let inclinazione = 0;
  const orbita = (evento) => {
    trascinata = { x: evento.clientX, y: evento.clientY };
  };
  const sposta = (evento) => {
    if (!trascinata) {
      return;
    }
    rotazione += (evento.clientX - trascinata.x) * 0.005;
    inclinazione += (evento.clientY - trascinata.y) * 0.005;
    inclinazione = Math.max(-1.4, Math.min(1.4, inclinazione));
    trascinata = { x: evento.clientX, y: evento.clientY };
    disegna.aggiorna();
  };
  const molla = () => {
    trascinata = null;
  };
  renderer.domElement.addEventListener("mousedown", orbita);
  renderer.domElement.addEventListener("mousemove", sposta);
  window.addEventListener("mouseup", molla);

  disegna.aggiorna = () => {
    const r = 18;
    macchina.position.set(
      r * Math.cos(inclinazione) * Math.sin(rotazione),
      r * Math.sin(inclinazione),
      r * Math.cos(inclinazione) * Math.cos(rotazione),
    );
    macchina.lookAt(0, 0, 0);
    renderer.render(scena, macchina);
  };
  disegna.ridisegna = (distanza = 18) => {
    rotazione += 0.6;
    const r = distanza;
    macchina.position.set(
      r * Math.cos(inclinazione) * Math.sin(rotazione),
      r * Math.sin(inclinazione),
      r * Math.cos(inclinazione) * Math.cos(rotazione),
    );
    macchina.lookAt(0, 0, 0);
    renderer.render(scena, macchina);
  };
  disegna.scegli = (claim) => {
    scegli.claim = claim;
  };
  disegna.libera = () => {
    renderer.domElement.removeEventListener("click", scegli);
    renderer.domElement.removeEventListener("mousedown", orbita);
    renderer.domElement.removeEventListener("mousemove", sposta);
    window.removeEventListener("mouseup", molla);
    geometria.dispose();
    materiale.dispose();
    linee?.geometry.dispose();
    linee?.material.dispose();
    renderer.dispose();
    contenitore.replaceChildren();
  };
  disegna.aggiorna();
  return disegna;
}

/** La posizione di un nodo che non è stato disegnato in questa scena. */
function cercaPosizione(dati, claim) {
  const nodo = dati.nodi.find((n) => n.claim === claim);
  return nodo ? nodo.posizione : [0, 0, 0];
}

/** Il riquadro che contiene la scena e la sua spiegazione. */
export function riquadroScena(dati, etichetta) {
  const area = el("div", { class: "scena-area" });
  const dettaglio = el("div", { class: "scena-dettaglio" }, [
    el("p", { class: "nota" }, [
      "Clicca un nodo per leggere la claim e il suo span. Trascina per cambiare la camera: cambia la vista, non il dato.",
    ]),
  ]);

  const controlli = el("div", { class: "scena-controlli" }, [
    el("button", { type: "button", id: "scena-ridisegna" }, ["Ridisegna"]),
    el("span", { class: "impronta", id: "scena-impronta" }, [impronta(dati)]),
  ]);

  const scelta = (claim) => {
    if (!claim) {
      riempi(dettaglio, el("p", { class: "nota" }, [
        "Clicca un nodo per leggere la claim che lo giustifica.",
      ]));
      return;
    }
    const nodo = dati.nodi.find((n) => n.claim === claim);
    if (!nodo) {
      return;
    }
    riempi(dettaglio, [
      el("h3", {}, [`Claim ${nodo.claim}`]),
      el("p", {}, [nodo.testo]),
      el("blockquote", { class: "span" }, [nodo.span]),
      el("p", { class: "nota" }, [`Argomento: ${nodo.argomento}`]),
    ]);
  };

  return {
    area,
    dettaglio,
    scelta,
    controlli,
    etichetta: etichetta ?? "Nessuna claim citabile con span: la scena resterebbe vuota, e una scena vuota sembrerebbe un materiale che non esiste.",
    appunti: dati.appunti,
  };
}
