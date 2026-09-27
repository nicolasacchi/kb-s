# `kb-s`

> **English summary.** `kb-s` is a Rust workspace for keeping school course
> material with real provenance and per-student work ledgers. **No model runs
> inside the product** — no LLM, no embedder, no semantic grader. The teacher
> and an external LLM prepare the material; `kb-s` keeps it, checks it, and
> records who demonstrated what. At this commit it is **not a usable product**:
> what exists is the domain types, a deterministic 40-item proof bench, and the
> repository's public face. Every capability claimed below names the test in
> this repository that demonstrates it, and the capabilities that are *not*
> demonstrated are listed in the same table, with the reason. The project's
> falsifiable claim is not evaluated by anything in this repository, and the
> README says so in its own line.

---

## Che cosa è

`kb-s` conserva il materiale di studio di una scuola — non i documenti, gli
**argomenti**: il soggetto insegnabile, con la sua provenienza, i suoi
prerequisiti e i suoi contratti. Tiene un registro per studente di che cosa ha
dimostrato e con quale prova.

Il formato di authoring è **HTML self-contained** (`ARCHITECTURE.md` D14), non
markdown, perché l'unità di studio non è un testo: un file HTML può contenere
nello stesso documento un contratto didattico, un esercizio con verificatore e
una scena 3D.

Tre registri, tre domande, tre durate, mai confondibili (D6): le **affermazioni**
(«questa frase è vera?»), le **prove** («chi ha dimostrato che cosa?») e le
**valutazioni** («chi ha deciso, con che cosa, e qualcuno ha contestato?»).

## Che cosa non è, e chi lo fa meglio

Non è un LMS — Moodle ha 147 283 siti e una rete di istalle che non si ricostruisce.
Non è un gradebook. Non è una libreria di contenuti. Non è un chatbot con un
programma. Non è monitoraggio. Non è un rilevatore di copia: l'integrità
accademica qui è **per costruzione** (D8, esercizi parametrizzati con
verificatore deterministico), non per sorveglianza. Non è un sistema di
formazione docente. Non è un'agenzia di procurement.

Ognuno di questi confini è una **decisione**, e ognuno nomina chi lo fa meglio:

| non è | chi lo fa meglio, e perché |
|---|---|
| un LMS | Moodle, perché esiste, è installato e ha 147 283 siti |
| un gradebook | il registro elettronico che la scuola ha già, perché il MIUR lo impone |
| una libreria di contenuti | i manuali in adozione, perché c'è chi li cura e li tiene aggiornati |
| un chatbot con un programma | un LLM con un contesto, perché ha più conoscenza e non ha limiti di prezzo |
| monitoraggio | chi misura, con il consenso informato, perché il consenso informato è un requisito e non un dettaglio |
| un rilevatore di copia | l'esame scritto in presenza, perché nessun detector ha una base di errori dichiarata |
| formazione docente | l'Università, perché ha il mandato e il tempo |
| procurement | una segreteria, perché ha le procedure e le esperienze |

## Come si gira

```sh
./kc test -p kbs-fixtures                              # il banco, sotto forma di test
./kc run -p kbs-fixtures --bin kbs-bench -- run        # il banco, con il suo referto
./kc run -p kbs-fixtures --bin kbs-bench -- emit crates/kbs-fixtures/corpus
```

`kc` è un wrapper: `kb-s` usa un `CARGO_HOME` proprio perché il pacchetto
globale è condiviso con altre sessioni sulla stessa macchina. Non usare `cargo`
direttamente.

`kbs-bench run` accetta `--json`, `--corpus <cartella>`, `--kbs <binario>` e
`--require-pipeline`. Gli ultimi due sono ciò che la CI usa.

Il banco chiama la pipeline **come processo** — `kbs verify --json --db <db>
<corpus>` — perché quella è la stessa interfaccia che D10 dichiara come «la CLI
come protocollo», e perché un banco agganciato alle funzioni interne si rompe a
ogni rifattorizzazione di un crate che non lo riguarda. La forma esatta del
JSON che il banco si aspetta è documentata in `kbs_fixtures::adapter`.

**Se la pipeline non è raggiungibile, i nove controlli che la richiedono sono
SALTATI, e il referto dice perché.** Non sono superati. In locale il banco è
verde con nove saltati; in CI gira con `--require-pipeline`, dove quegli stessi
saltati sono un fallimento. Un banco che salta in silenzio viene creduto, e
un banco creduto è peggio di un banco assente.

## La claim, che resta falsificabile

> Vale la pena costruire `kb-s` **se e solo se**, in una classe che lo usa per un intero
> quadrimestre, la quota di argomenti che passano da «non dimostrato» a «dimostrato» è
> **almeno il doppio** della stessa quota misurata in una classe di controllo che segue il
> protocollo identico senza il sistema — a parità di tempi di pratica assistita, e misurata
> sul **compito a risorse chiuse**, non sulla pratica.

La soglia del fallimento è **×1,5**: sotto quella soglia il progetto è fallito
anche se è positivo. La misura va fatta su un compito ritardato (4–8 settimane) e
su almeno un compito di trasferimento, perché il risultato immediato non
distingue un tutor da un ripetitore di esempi.

## Le due decisioni che sorprenderebbero un lettore

### 1. Nessun modello dentro il prodotto, mai

Non un LLM generativo, non un embedder, non un correttore semantico. L'LLM è
**uno strumento del docente**, esterno, che produce materiale che il docente poi
ratifica. La catena di grading è `deterministic → peer → human`, e in `kbs-core`
`GraderKind` ha **quattro** varianti — `Deterministic`, `Peer`, `Human`, `Teacher` —
ma **nessuna variante «modello»**: la regola è resa impossibile dal tipo, non
documentata in un commento. `Teacher` sta in catena perché un giudizio del docente
su ispezione vale quanto `Human` e registra chi l'ha fatto; la catena a tre
livelli di D3 descrive l'ordine di preferenza, non l'elenco dei giudicanti.

Il motivo dichiarato è economico e di lock-in; il motivo **vero** è un altro, e
va detto: nessun dato dello studente esce dal prodotto, perché nel prodotto non
entra niente che lo tocchi. DPIA, FRIA, profilazione, verifica d'età, alto rischio
AI Act: non si applicano **per posizione**, non perché sono state risolte.

Una privacy ottenuta per posizione è più forte di una ottenuta per promessa,
perché non dipende dalla buona volontà di nessuno e non si erode quando
cambia il fornitore.

### 2. Il banco contiene cinque fixture **rotte**, per costruzione

Un banco le cui fixture funzionano tutte dimostra solo che le cose che
funzionavano continuano a funzionare. Qui cinque item devono **fallire**, uno
per regola: una sezione mancante delle otto, un `GUARDIAN` oltre i 640 byte, un
contratto oltre l'hard cap, un riferimento a una CDN esterna, un ciclo nei
prerequisiti. E se qualcuno le sistema a mano, il banco diventa rosso e nomina
il file: una correzione che cambia il test deve essere rumorosa, non silenziosa.

## Il limite, in una riga

> `kb-s` non sa se funziona in una classe italiana, e non lo saprà finché nessuno lo misura.

## Le capacità, e il test che dimostra ciascuna

<!-- capacità: inizio -->

| capacità | dimostrata da | stato |
|---|---|---|
| Un item non verificato è leggibile ma non citabile | `kbs-core::la_bozza_non_e_mai_citabile` | dimostrata |
| Un argomento ratificato sul contratto corrente **è** citabile | `kbs-core::un_argomento_ratificato_e_citabile` | dimostrata |
| Una ratifica non sopravvive a un contratto cambiato | `kbs-core::la_ratifica_non_sopravvive_a_un_contratto_cambiato` | dimostrata |
| …e la stessa cosa su dati veri, non su un oggetto di test | `copertura::la_ratifica_superata_e_un_dato_e_non_un_commento` | dimostrata |
| L'identità di un argomento segue il percorso, non i byte | `kbs-core::l_id_segue_il_percorso_e_non_i_byte` | dimostrata |
| La visibilità è una relazione, e non un ruolo | `kbs-core::la_visibilita_e_una_relazione_e_non_una_ruota` | dimostrata |
| La soglia di coorte è un accesso, non una cancellazione | `kbs-core::la_soglia_di_coorte_e_un_accesso_non_una_cancellazione` | dimostrata |
| Un errore si registra e non si cancella | `kbs-core::un_errore_si_registra_e_non_si_cancella` | dimostrata |
| La prova orale non entra nel replay deterministico, e lo dichiara | `kbs-core::la_prova_orale_non_e_riproducibile_e_lo_dice` | dimostrata |
| Nessun modello può emettere una claim o un giudizio | `copertura::nessun_modello_puo_emettere_una_claim_o_un_giudizio` | dimostrata |
| Il contratto ha otto sezioni con budget, e il margine è quello dichiarato | `contract::budget_e_margine_coerenti` | dimostrata |
| Un contratto completo sta nei budget, `GUARDIAN` compreso | `contract::un_contratto_completo_si_misura_entro_i_budget` | dimostrata |
| Le tre fixture di contratto rotte sono rotte, misurate sui byte | `contract::i_difetti_di_contratto_producono_testo_misurabilmente_rotto` | dimostrata |
| Il banco contiene 40 item e copre tutte e dodici le famiglie di media | `copertura::quaranta_item_e_dodici_famiglie` | dimostrata |
| Ogni stato di pubblicazione ha un item **valido** | `copertura::ogni_stato_di_pubblicazione_ha_un_item_valido` | dimostrata |
| Le cinque fixture rotte sono cinque, una per regola | `copertura::le_cinque_fixture_rotte_una_per_regola` | dimostrata |
| Un esercizio parametrizzato ha due istanze con risposte diverse | `copertura::l_esercizio_parametrizzato_ha_due_istanze_con_risposte_diverse` | dimostrata |
| Una claim contraddetta e una non citabile esistono come righe vere | `copertura::una_claim_contraddetta_e_una_non_citabile_su_dati_veri` | dimostrata |
| Ogni nodo e ogni arco della scena 3D ha una claim, e i dati stanno nel manifest | `copertura::la_scena_3d_ha_una_claim_per_ogni_oggetto_e_i_dati_stanno_nel_manifest` | dimostrata |
| Esiste un item che carica three.js dal percorso servito dal binario e uno che lo carica da una CDN | `copertura::un_riferimento_a_cdn_e_uno_al_runtime_locale` | dimostrata |
| La catena dei prerequisiti è reale e aciclica fra gli item validi | `copertura::la_catena_e_reale_e_il_ciclo_e_dichiarato` | dimostrata |
| Le osservazioni hanno una sequenza monotona e univoca | `copertura::le_osservazioni_hanno_una_sequenza_monotona_e_univoca` | dimostrata |
| La contestazione vive dentro la riga del voto, non accanto | `copertura::la_contestazione_vive_dentro_la_riga_del_voto` | dimostrata |
| Il banco è deterministico: due esecuzioni danno gli stessi byte | `determinismo::il_referto_testuale_e_identico_fra_due_esecuzioni` | dimostrata |
| Il corpus ha gli stessi byte e lo stesso hash ogni volta | `determinismo::il_corpus_ha_gli_stessi_byte_ogni_volta` | dimostrata |
| Riparare a mano una fixture rotta rende il banco rosso, e nomina il file | `riparazione::il_banco_diventa_rosso_quando_una_fiastra_rotta_viene_riparata` | dimostrata |
| Un file che nessuno ha descritto viene detto nel referto | `riparazione::un_file_che_nessuno_ha_descritto_viene_detto_nel_referto` | dimostrata |
| Un controllo saltato porta con sé la ragione | `report::il_referto_testuale_nomina_i_saltati_e_la_ragione` | dimostrata |
| In CI un saltato è un fallimento | `report::in_ci_un_saltato_e_un_fallimento_e_il_referto_lo_dichiara` | dimostrata |
| Il validatore rifiuta un contratto con una sezione mancante | `pipeline.validazione.i_codici_di_rifiuto` | non dimostrata: la pipeline non esiste, il controllo è saltato |
| Il validatore rifiuta un `GUARDIAN` oltre i 640 byte e un contratto oltre il cap | `pipeline.validazione.i_codici_di_rifiuto` | non dimostrata: la pipeline non esiste, il controllo è saltato |
| Un artifact che referenzia una CDN impedisce la pubblicazione | `pipeline.documenti.solo_la_versione_locale_di_three_e_pubblicabile` | non dimostrata: la pipeline non esiste, il controllo è saltato |
| Il ciclo nei prerequisiti impedisce l'ingresso dell'argomento | `pipeline.prerequisiti.il_ciclo_e_rifiutato` | non dimostrata: la pipeline non esiste, il controllo è saltato |
| Data la tupla (corpus, esercizio, seed) il replay riproduce la generazione | `il_replay_e_deterministico::lo_stesso_seed_da_la_stessa_istanza` | dimostrata sul generatore vero: il controllo di pipeline che la dichiarava e' stato rimosso perche' non poteva mai passare |
| Una ratifica superata rende l'argomento non citabile **nell'indice** | `pipeline.indicizzazione.solo_i_ratificati_sono_citabili` | non dimostrata: la pipeline non esiste, il controllo è saltato |
| La catena di hash copre le osservazioni e ha una consistency proof | — | non dimostrata: non c'è codice, e `D6` la dichiara come limite dichiarato |
| Un accesso applica il predicato di visibilità | — | non dimostrata: il predicato esiste ed è testato, nessun accesso passa attraverso di esso |
| La coorte raggiunge la soglia e aggrega | — | non dimostrata: la soglia è una costante e un predicato, non c'è aggregazione |
| three.js è vendorizzato e servito dal binario | — | non dimostrata: i file ci sono in `vendor/three/`, nessun test li raggiunge |
| L'uscita è `rm -rf` a colonne fisse | — | non dimostrata: non c'è codice |
| LTI 1.3 in uscita verso l'LMS | — | non dimostrata: non c'è codice |
| Il markdown in ingresso viene convertito con un flag di provenienza | — | non dimostrata: non c'è codice |

<!-- capacità: fine -->

La regola che governa questa tabella è verificata da un test, non da una
promessa: `leggimi::ogni_capacita_dichiara_il_test_che_la_demonstra`.

## Che cosa non è costruito

`CHANGELOG.md` ha la sezione **Non costruito**, ed è lì che l'elenco completo
sta. In una riga: di tutto quello che `ARCHITECTURE.md` descrive, in questo
commit esistono i tipi di dominio, il banco e le carte. Gli altri crate sono
scheletri.

## Documenti del repository

- `ARCHITECTURE.md` — quindici decisioni (D1–D15) che valgono come contratto.
- `CHANGELOG.md` — cosa c'è, e cosa non c'è.
- `LICENSE-MIT`, `LICENSE-APACHE` — il manifest dichiara `MIT OR Apache-2.0`, e
  senza i due file la dichiarazione sarebbe falsa.

## Licenza

`MIT OR Apache-2.0`.
