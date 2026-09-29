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
<corpus>` e `kbs promote --person <id> --arg <rel> --path <file> --db <db>
<corpus>` — perché quella è la stessa interfaccia che D10 dichiara come «la CLI
come protocollo», e perché un banco agganciato alle funzioni interne si rompe a
ogni rifattorizzazione di un crate che non lo riguarda. La forma esatta del
JSON che il banco si aspetta è documentata in `kbs_fixtures::adapter`.

Quel `verify` del banco non porta `--person`, ed è l'unico caso in cui il
default `person_0000` è un **contratto dichiarato**
(`kbs_fixtures::spec::OPERATORE`): il comando lo dice lo stesso, su stderr e
in `avvisi` nel referto. Le righe qui sotto portano `--person` perché quelle
sono le forme di un operatore, non quella del banco.

**Il banco esegue una sequenza di atti, non un comando solo**, perché la
citabilità di D4 non si può osservare da una cartella: una pipeline che
indicizza non firma niente, e la ratifica è un atto separato, di una persona.
Il banco fa quindi quattro atti — `verify` su un database vuoto, `promote` su
ciò che la tabella dichiara ratificato di fresco, `verify` sullo stesso
database, e la stessa strada su una copia con un contratto riscritto sotto una
ratifica già firmata — e ne deduce quattro cose. Sono quattro controlli e non
uno: un controllo che può fallire per quattro ragioni viene riportato per la
ragione sbagliata.

**Se la pipeline non è raggiungibile, gli undici controlli che la richiedono
sono SALTATI, e il referto dice perché.** Non sono superati. In locale il banco
è verde con undici saltati; in CI gira con `--require-pipeline`, dove quegli
stessi saltati sono un fallimento. Un banco che salta in silenzio viene creduto,
e un banco creduto è peggio di un banco assente.

## Il server: due comandi

Il daemon è `kbs-serve`. Si chiama così e non `kbs` perché `kbs` è già il
binario di `kbs-intake`, e `kbs-fixtures` lo cerca in `target/debug`: due binari
con lo stesso nome nella stessa cartella di build non falliscono, è l'ultimo
che compila che vince, e in silenzio.

```sh
./kc build -p kbs-server --bin kbs-serve                        # costruiscilo
./target/debug/kbs-serve --corpus crates/kbs-fixtures/corpus --db /tmp/kb-s.sqlite3
```

`--db` è facoltativo e sta **fuori** dal corpus per D12: il corpus è una
cartella di file versionata e l'uscita è `rm -rf`, e un registro che ci sta
dentro sparisce con i file che descrive. Il daemon lo rifiuta se glielo metti
dentro, e non crea da solo un corpus che non esiste — su un corpus vuoto ogni
artifact risponderebbe «non c'è», che è la stessa risposta di «non lo vedi», e
il motivo sarebbe invisibile.

All'avvio il daemon stampa due cose che un operatore non può indovinare: la URL,
e il fatto che **l'identità è una dichiarazione**.

```
kbs-serve 0.1.0
  ascolto     http://127.0.0.1:8787
  corpus      crates/kbs-fixtures/corpus
  database    /tmp/kb-s.sqlite3 (epoch 9; questo binario ne conosce 9)
  identita'   una DICHIARAZIONE, non un'autenticazione: chiunque raggiunga
              questa porta puo' dichiarare chi e' con X-Kbs-Person o ?person=.
              Il predicato D5 protegge il materiale, non la persona.
  chiusura    Ctrl-C (SIGINT) o SIGTERM: le richieste gia' ricevute finiscono,
              e il listener chiude subito. Una richiesta ancora in arrivo —
              meta' intestazioni, nessuna riga vuota — viene tagliata: non ha
              ancora una richiesta da finire, solo dei byte.
```

Di default ascolta su **loopback**, e `--listen 0.0.0.0:8787` lo dice a voce
alta quando lo fai: chiunque raggiunga quella porta può dichiarare chi è, e il
predicato D5 continua a valere — protegge il materiale, non la persona.

### Un database vero, non una directory vuota

`kbs-serve` da solo parte e serve l'interfaccia, ma il database che crea è
vuoto: nessun corso, nessuna relazione, e quindi ogni risposta è «non lo vedi».
Per vedere materiale, il database lo deve costruire `kbs`:

```sh
./kc build -p kbs-intake --bin kbs
./target/debug/kbs verify --db /tmp/kb-s.sqlite3 --person person_0001 \
    crates/kbs-fixtures/corpus
./target/debug/kbs-serve --corpus crates/kbs-fixtures/corpus --db /tmp/kb-s.sqlite3
```

`verify` indicizza e **non firma niente**: gli argomenti che produce restano in
`bozza`, e la bozza la vede solo chi ha una relazione col corso — il predicato
D5 è già valido durante la stesura, ed è per quello che la coda di ratifica non
espone nulla a chi non insegna. Diventa materiale del corso con
`kbs ratify` e poi `kbs promote`, che sono atti separati e firmati da una
persona (D4): un banco che indicizza non promote niente, e questo è il punto.
`--person` dichiara chi agisce, e chi non dichiara niente non vede niente: con
una eccezione, dichiarata subito sotto.

`verify` però **non crea relazioni**: indicizza e basta, e `--person` è chi
indicizza. Senza quell'opzione il comando scrive come `person_0000` — una
stringa scritta nel codice che diventa l'`origin_by` di tutto quello che
entra, e che chiunque può dichiarare come propria: il predicato apre a
`is_author` prima di guardare le relazioni, quindi quella persona vede tutto
ciò che ha indicizzato. Il default esiste perché il banco lo dichiara come
parte del suo contratto (`kbs_fixtures::spec::OPERATORE`), ma **non è
silenzioso**: quando manca `--person`, il comando scrive un avviso su stderr
e mette `avvisi` nel referto, e l'avviso nomina la persona, che cosa diventa e
chi la può vedere. La riga qui sopra porta `--person` per quello: chi copia
questa riga non eredita l'identità di nessuno.

```sh
./target/debug/kbs insegna --db /tmp/kb-s.sqlite3 \
    --person person_0001 --course matematica-seconda --docente person_0001
```

`--docente` è la persona che **insegna** e `--person` è **chi ha registrato la
riga**: sono due domande diverse e `relations` le tiene separate, perché la
seconda è la provenienza di un atto (`relations.recorded_by`). Il verbo scrive
`teaches` e nient'altro, ed è **solo** della CLI locale: sull'HTTP e sull'MCP
l'identità è dichiarata (`x-kbs-person`), e un metodo che concedesse diritti su
quel trasporto sarebbe la definizione letterale di `teaches` in
`kbs_core::may_read`. Non crea persone: il registro delle persone è della
scuola. E non chiude relazioni: chiude `kbs termina`, che è un altro atto e ha
un altro nome.

```sh
./target/debug/kbs termina --db /tmp/kb-s.sqlite3 \
    --person person_0001 --course matematica-seconda --docente person_0001
```

Un docente che lascia il corso resta `teaches` per sempre senza quel verbo: era
una relazione che il prodotto apriva e non chiudeva mai, e quel diritto non
aveva via d'uscita. `termina` scrive `until` e **chi l'ha chiuso**
(`relations.ended_by`), che è un fatto diverso da chi l'ha aperto
(`relations.recorded_by`): sono due colonne perché sono due atti. Da quel
momento `teaches` non apre più niente per quella persona, e il predicato D5 fa
il resto. Come `insegna` è **solo** della CLI locale, e su quel trasporto la
ragione è più forte: dichiarare un'identità per **togliere** un diritto
significherebbe che chiunque raggiunga la porta può lasciare fuori un docente
dal proprio corso. Il verbo è idempotente e dice quale delle due cose è
successa (`recorded`, `already`); un incarico che non è mai esistito è un
rifiuto (`relazione-assente`), perché `already` a chi non ha mai insegnato
sarebbe una risposta falsa.

`Ctrl-C` chiude: le richieste già ricevute finiscono, il listener chiude subito,
e il processo esce con `0`.

### I due comandi di un docente che parte da zero

Il percorso che una scuola percorre — indicizzare, dichiarare chi insegna,
ratificare, promuovere — è **un comando**, e l'ordine è dentro il comando:

```sh
./kc build -p kbs-intake --bin kbs
./target/debug/kbs ciclo crates/kbs-fixtures/corpus \
    --db /tmp/kb-s.sqlite3 --person person_0001 --docente person_0001
```

`ciclo` esegue `verify` → `insegna` → `ratify` → `promote` e **si ferma al
primo passo che non riesce**, dicendo quale e perché: il nome del passo è su
stderr, e il referto di quello che è già riuscito resta su stdout, perché un
docente che si ferma al terzo passo ha bisogno di sapere che i primi due sono
a posto. `--person person_0001 --docente person_0001` è la forma che parte da
un database vuoto: `--docente` non ha un default (un default scriverebbe
`teaches` a qualcuno che non l'ha chiesto) e la persona che insegna deve essere
nel registro delle persone — `--person` la registra, `--docente` no. Su
`--person` e `--docente` diversi, il collega deve già essere nel roster.

Il secondo comando è quello con cui si controlla che sia successo:

```sh
./target/debug/kbs read --db /tmp/kb-s.sqlite3 \
    --person person_0001 --arg letture/prova.html
```

`ciclo` è **idempotente**: rieseguito sullo stesso corpus non duplica
relazioni né righe, e la seconda esecuzione lo dichiara invece di lasciarlo
dedurre da un `ok: true` identico (`relazioni.gia`, `ratifiche.gia`,
`promozioni.gia`). Un file che la validazione boccia, o che la porta rifiuta,
non ferma il ciclo — è già nel referto di `verify` — ma finisce in
`non_promossi` con la sua ragione, e **il comando esce non zero**: metà
materiale pubblicato non è un successo.

**Il ciclo ha un buco, e lo dichiara: non produce le istanze degli esercizi.**
Il campo `esercizi.istanze` del referto è sempre `0`, e accanto ci sono i
numeri misurati sul corpus (`dichiarati`, `famiglie`, `fuori_catalogo`): su
`corpus-ite/` gli esercizi dichiarano famiglie che non sono le cinque del
catalogo chiuso di `kbs-exercise`, quindi nessuna istanza è calcolabile e
nessun generatore le produrrebbe. Il verbo che le scrive è `kbs autora`, che
su quelle famiglie non scrive niente e dice quale famiglia manca. Un percorso
che dicesse «fatto» senza dirlo mentirebbe: il campo `testo` del referto ha
una sezione `NON FATTO` per questo.

### Fuori dal checkout: `--web` e `--vendor`

Il binario compilato porta con sé due percorsi assoluti: la cartella `web/`
accanto al crate e `vendor/` alla radice del repository. Sono giusti per chi
esegue dal checkout e **sbagliati appena il binario viene spostato** — in
un'immagine Docker, in un pacchetto installato, in un `scp` su un'altra
macchina — perché lì la directory in cui qualcuno ha compilato non esiste.
Quando è così, i due percorsi vanno dichiarati:

```sh
./target/debug/kbs-serve \
  --corpus /srv/kb/corpus \
  --db /srv/kb/kb-s.sqlite3 \
  --web /opt/kb-s/web \
  --vendor /opt/kb-s/vendor
```

`--vendor` è la cartella che **contiene** `three/`, cioè
`/opt/kb-s/vendor/three/three.module.min.js` (D15: il runtime è vendorizzato
e servito dal binario, non scaricato dal client). `--web` è la cartella dei
file dell'interfaccia.

Senza i due flag il daemon fa esattamente quello che faceva prima, avvisa e
parte: l'interfaccia mancante è un avviso perché l'API e gli artifact si
servono lo stesso. **Con** i due flag la validazione è un rifiuto, e il
rifiuto avviene all'avvio, non al primo uso:

```sh
$ ./target/debug/kbs-serve --corpus /srv/kb/corpus --web /opt/kb-s/interfaccia
l'interfaccia /opt/kb-s/interfaccia non è una cartella.
Il daemon non la crea: `--web` è una dichiarazione, e dichiarare un percorso
che non esiste significa che il deployment è sbagliato — sostituirlo in
silenzio servirebbe i file di un'altra installazione.
…
# codice 2 (rifiutato da una regola)
```

È la stessa regola del corpus mancante, e per la stessa ragione: il daemon non
crea la cartella che gli hai dichiarato, perché una cartella vuota fa
rispondere «non c'è» a tutto e la ragione diventerebbe invisibile. Lo stesso
vale per `--vendor` senza `three/three.module.min.js`: senza quel file
`Vendor::load` non fallisce, lascia il vendor senza runtime, e ogni artifact
che chiama fuori dal proprio foglio risponderebbe `500` alla prima
visualizzazione — un deployment rotto che si annuncia come un bug del
materiale.

`--web` e `--vendor` non sono obbligatori. Ripetuti sono un errore che li
nomina, come `--corpus`, `--db` e `--listen`: una bandierina ripetuta è un
comando ambiguo, e un override ambiguo servirebbe il percorso sbagliato in
silenzio. Il testo completo è `kbs-serve --help`.

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
| Su un corpus che nessuno ha ratificato l'indice condiviso è vuoto | `la_seconda_strada_di_d4::atto_1_su_un_database_vuoto_nessun_item_e_citabile` | dimostrata |
| La ratifica entra nel sistema solo come atto del docente | `la_seconda_strada_di_d4::atto_2_la_promozione_e_l_atto_del_docente_e_la_porta_la_ragiona` | dimostrata |
| Un argomento ratificato entra nell'indice condiviso, e non altri | `la_seconda_strada_di_d4::atto_3_dopo_la_promozione_e_citabile_esattamente_il_gruppo_promosso` | dimostrata |
| Una ratifica superata rende l'argomento non citabile **nell'indice**, e nessun altro esce | `la_seconda_strada_di_d4::atto_4_il_contratto_riscritto_sotto_una_ratifica_viva_esce_dal_citabile` | dimostrata |
| Indicizzare, dichiarare chi insegna, ratificare e promuovere sono **un comando**, e l'argomento firmato è citabile | `il_ciclo_completo::il_ciclo_indicizza_insegna_ratifica_e_promuove` | dimostrata |
| Un percorso che non ha promosso tutto esce non zero e nomina il file che non è passato | `il_ciclo_completo::un_file_che_non_si_puo_promuovere_non_finisce_finito_e_il_referto_lo_dice` | dimostrata |
| Il percorso completo non duplica relazioni né righe, e la seconda esecuzione lo dichiara | `il_ciclo_completo::due_esecuzioni_sullo_stesso_corpus_non_doppiano_nulla` | dimostrata |
| Chi indicizza senza dichiarare persona riceve un avviso che nomina l'identità che verrà usata | `la_cli::verify_senza_persona_lo_dice_su_stderr_e_nel_referto` | dimostrata |
| Un docente che lascia il corso non è più `teaches`, e la fine dice chi l'ha chiusa | `la_cli::l_incarico_che_finisce_non_apre_piu_niente_e_dice_chi_l_ha_chiuso` | dimostrata |
| Un incarico già finito e un incarico mai esistito danno due risposte diverse | `la_cli::un_incarico_già_finto_e_un_incarico_che_non_c_e_dicono_cose_diverse` | dimostrata |
| Il ciclo completo non produce le istanze degli esercizi, e il referto lo dichiara | — | non dimostrata: nessun codice le produce, e `kbs autora` è l'unico percorso che le scrive |
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
