//! Il metro della padronanza: «non dimostrato → dimostrato», e che cosa lo rende vero.
//!
//! Questo modulo non è un'attività: è il **rendiconto** di un'attività, e va
//! costruito dopo la coda di richiamo e non al suo posto. Qui non si genera
//! niente e non si accetta niente: si legge il registro delle dimostrazioni e se
//! ne ricava una frase.
//!
//! # La domanda a cui questo modulo risponde
//!
//! Il prodotto esiste per un numero: *la quota di argomenti che passano da
//! «non dimostrato» a «dimostrato»*, confrontata fra la classe che usa il
//! sistema e la classe che segue lo stesso protocollo senza. Quel numero era
//! dichiarato e non computabile, perché non c'era un criterio: una riga con
//! `unaided = 1` dice che lo studente ha risposto senza aiuti, e non dice
//! niente di un argomento.
//!
//! # La transizione, e perché non è «una risposta giusta»
//!
//! «Dimostrato» è una proprietà di un **argomento**, non di un esercizio e non
//! di uno studente. Quindi la domanda non è «ha indovinato?» ma «su questo
//! argomento quante volte ha risposto da solo, e con quale tenuta?». Il
//! criterio che ne segue è qui sotto, in quattro righe, ed è **il criterio
//! della claim**: nessun altro, e non uno per braccio.
//!
//! # Stato o soglia? — Soglia, ricalcolata, ed è la metà del mestiere
//!
//! La padronanza **non è uno stato in cui si entra**. È una soglia che si
//! attraversa, e che si attraversa di nuovo quando le condizioni tornano vere.
//! Le due cose si comportano in modo opposto quando la padronanza decade, e il
//! corpus ha già deciso quale delle due è sbagliata: un distintivo che compare
//! quando la soglia si supera e resta nel profilo *«è irreversibile: la
//! padronanza decade, la soglia si ricalcola, il badge resta. Un oggetto che
//! non può essere revocato è un oggetto che mente»*. Qui non c'è un badge: c'è
//! una funzione pura del registro, che a un istante dà un verdetto e a un
//! istante dopo può dare il verdetto opposto.
//!
//! Ne segue la conseguenza che il metro **non scrive niente**. Non c'è una
//! tabella di padronanza, non c'è una colonna `mastered`, non c'è una riga da
//! appendere quando lo studente «passa». Il passaggio è un fatto *dedotto* da un
//! registro che già esiste, e la ragione è che un verdetto scritto è un verdetto
//! che un giorno non si sa perché è cambiato.
//!
//! # La perdita: nessuna riga nuova, ed è il punto
//!
//! «La padronanza si può perdere?» — sì, in due modi, ed entrambi sono già
//! scritti nel registro:
//!
//! 1. **l'ultima verifica non assistita è fallita**: la padronanza si regge
//!    sull'ultima parola, non sul totale. Otto prove giuste e poi un errore non
//!    assistito dicono che lo sai fino a tre settimane fa, e non adesso;
//! 2. **l'ultima verifica è troppo vecchia**: la padronanza decade, e un metro
//!    che non si muove demotiva.
//!
//! E qui sta la parte che il vincolo append-only rende interessante: **perdere
//! non richiede nessuna riga nuova**. La riga che fa perdere la padronanza è
//! un'osservazione come tutte le altre — `unaided = 1`, risposta sbagliata — e il
//! fatto che sia *l'ultima* è una proprietà della **query**, non del dato. Un
//! registro che avesse bisogno di una riga «ho perso la padronanza» per dire
//! che uno studente ha perso la padronanza starebbe scrivendo un giudizio dentro
//! il registro delle prove: un giudizio che un giorno qualcuno conterebbe come
//! se fosse una dimostrazione, e che la catena di hash di D6 pagherebbe come se
//! fosse una prova.
//!
//! Quindi: la perdita è calcolata, la rivincita è calcolata, e nessuna delle due
//! è una scrittura. La **monotonia sta nelle prove** (una prova non si riscrive
//! e non si cancella: sono i trigger di `V3`), **non nei verdetti**.
//!
//! # Il criterio, per intero
//!
//! Un argomento è **dimostrato** per uno studente, a un istante `at`, se e solo
//! se fra le osservazioni **non assistite, verificate da un programma e
//! corrette** su quell'argomento, registrate fino a `at`:
//!
//! 1. **soglia** — almeno [`MASTERY_CLAIM_THRESHOLD`] **esercizi distinti**
//!    distinti dimostrati senza aiuto. Sono esercizi, non istanze: otto seed
//!    diversi della stessa domanda sono otto risposte diverse allo stesso
//!    ragionamento, e D8 dice che il ragionamento è la cosa che si possiede. Un
//!    conteggio, non una percentuale: chi smette di tentare deve poter smettere
//!    senza che la misura si riempia da sola.
//! 2. **ultima parola** — l'ultima verifica non assistita è riuscita.
//! 3. **spaziamento** — le dimostrazioni coprono almeno
//!    [`MASTERY_RETENTION_HORIZON`]: fra la più vecchia e la più recente passano
//!    due settimane. Otto prove nella stessa ora sono la stessa prova ripetuta, e
//!    la letteratura che il corpus cita parla di **ritenzione**, non di
//!    acquisizione.
//! 4. **freschezza** — l'ultima verifica risale a non più di
//!    [`MASTERY_FRESHNESS_WINDOW`] fa.
//!
//! L'ordine dei controlli è l'ordine in cui il criterio è scritto, e la ragione
//! che il metro dà è **la prima condizione non soddisfatta**: un verdetto con due
//! motivi deve sceglierne uno, e chi lo sceglie a caso dice al lettore una cosa
//! che non è la ragione. Ogni motivo è quindi un [`Reason`] distinto, e i test li
//! distinguono uno a uno.
//!
//! # Perché «una sola risposta giusta» non è il criterio
//!
//! La risposta ovvia — «un argomento è dimostrato se esiste un'osservazione non
//! assistita corretta» — è economica e sbagliata in un modo preciso: uno
//! studente che ha indovinato una volta, o che ha risposto bene alla prima
//! occasione cinque settimane fa, non è nello stesso stato di uno che risponde
//! oggi da solo. Il metro dice tre cose su quel caso, e sono le tre che un
//! revisore attaccherebbe per prime:
//!
//! * **una volta sola** non basta: sotto soglia il verdetto dice *quanti*
//!   esercizi distinti mancano, e il numero è nel verdetto;
//! * **cinque settimane fa e niente dopo** non basta: la finestra di freschezza
//!   lo dichiara vecchio, e la frase che esce è «l'ultima verifica risale a 35
//!   giorni fa», non uno zero. È la risposta giusta, perché il compito ritardato
//!   di cui la claim parla è esattamente questo caso: il metro non deve poter
//!   certificare il mese scorso, e il compito a risorse chiuse è l'atto che
//!   rinnova il verdetto;
//! * **la prima volta, tutta insieme** non basta: lo spaziamento di due settimane
//!   è la condizione che distingue l'acquisizione dalla ritenzione.
//!
//! # Lo strumento non cambia durante l'anno, e come lo si garantisce
//!
//! *«Un cambio di modello rimette a zero la scala, e un registro append-only che
//! aggrega sull'intero corso finisce per confrontare due strumenti diversi»* — è
//! l'esposizione reale, e la risposta è **non usare nel metro nessuno strumento
//! che possa cambiare**: le osservazioni che il metro guarda sono
//! `judged_by = 'deterministic'` e `evidence = 'checked'`, cioè un programma
//! (D8) e nient'altro. D3 chiude fuori dal prodotto ogni modello, quindi nessun
//! cambio di modello può muovere questo numero; un giudizio di pari o di persona
//! resta nel registro e il docente lo vede, ma **non muove il metro**, perché il
//! metro deve confrontare lo stesso verificatore nelle due braccia — «lo stesso
//! argomento, stesso item, stesso verificatore, stesso criterio di soglia» — e
//! un giudizio umano non è riproducibile (D11) né identico fra due classi.
//!
//! L'esposizione che resta, dichiarata e non risolta: un `exercises.id` che
//! cambiasse `generator_version` sotto lo stesso id metterebbe due strumenti
//! dentro lo stesso conteggio. In questo repository non esiste il percorso che
//! lo faccia — il generatore crea un esercizio nuovo e la riga non si riscrive —
//! ed è qui dichiarato perché è il tipo di difetto che un registro append-only
//! non può difendere da solo.
//!
//! # I due scrittori, e perché i due criteri coincidono per costruzione
//!
//! Questo modulo non sa chi scrive, quindi la coerenza con le altre superfici
//! che leggono la stessa vista non può dipendere da lui. Oggi i due scrittori di
//! `observations` sono `kbs_intake::pratica`, che scrive prove verificate da un
//! programma e dichiara `unaided` secondo che le piste ci fossero, e
//! `kbs_intake::diagnosis`, che scrive `unaided = None` e quindi non entra
//! nella vista. **Ogni riga che il prodotto mette nella vista è quindi anche una
//! riga che questo metro conta**: la coincidenza è una proprietà degli scrittori
//! e non un caso, ed è dichiarata qui perché un terzo scrittore la deve leggere
//! prima di esistere. Se domani la pagina del calendario e questa non dicono la
//! stessa cosa sulla stessa riga, il difetto è in quello scrittore.
//!
//! **L'esposizione che resta, dichiarata e voluta**: il calendario distingue tre
//! stati dell'ultima ricomparsa — riuscita, fallita, e «non valutata» — perché
//! una coda di richiamo non deve poter dire «dimostrato», che è un verdetto. Il
//! metro non ha lo stato «non lo so»: o dice il motivo o non dice niente, e una
//! riga che il calendario chiama «non valutata» qui è una riga che non entra
//! nel conteggio. I due non diranno mai la stessa frase sulla stessa riga, ed è
//! voluto: sono due domande diverse («quanto mi resta da fare» e «lo so?»).
//!
//! # L'altra metà dichiarata: il metro è piatto
//!
//! Una sola verifica non assistita riuscita dopo un errore rimette il verdetto a
//! «dimostrato», perché il criterio è un criterio e non una curva: il conteggio
//! delle prove non si azzera. È una scelta e ha un prezzo — il metro misura
//! «l'ultima parola» più di quanto misuri «la tenuta» — e il prezzo si paga in
//! informazione e non in segreto: la riga porta il conteggio degli esercizi
//! dimostrati e la data dell'ultima verifica, e l'insegnante vede il metro
//! intero. Il segnale precoce che il corpus chiama più economico ne resta un
//! altro, e questo modulo non lo simula.
//!
//! # Perché `unaided = 1` non è scritto qui
//!
//! La superficie non assistita è la **vista** `unaided_observations`, definita in
//! `V6__unaided.sql` con `WHERE unaided = 1`, ed è la ragione per cui quel
//! predicato non compare in questo file e non è un `WHERE` in una rotta: un
//! filtro in Rust è un filtro che marcisce, la vista viaggia col database e vale
//! per chi scrive SQL a mano e per un dump. Le due righe che la vista esclude
//! sono quelle che il metro non deve mai contare: `unaided = 0` (la coda di
//! practice) e `unaided IS NULL` (**non registrato**, che non vuol dire *no* —
//! sono le righe scritte da un sistema che non chiedeva nulla in proposito, e
//! dichiararle una cosa o l'altra sarebbe una padronanza retroattiva). Il metro
//! quindi non sa che esistano: non le chiede.
//!
//! # Il lato docente: la soglia di D9, e non una seconda superficie
//!
//! La quota di classe è l'aggregato del docente e nient'altro. Sotto
//! `COHORT_MIN_K` **non esiste**, e non è «0» né «0 con un flag»: è `None`,
//! cioè l'assenza del valore, la stessa risposta che dà `{"signals": []}` per i
//! segnali di coorte. La soglia è quella di `kbs_core` e non una copia, per la
//! ragione che `routes::cohort` dichiara: due soglie che divergono fanno dire al
//! database e alla rotta cose diverse sulla stessa riga. E il conteggio è di
//! **persone**, non di righe — `failing = 3` che è il lavoro di uno studente è
//! il difetto che `CohortCountMismatch` esiste per impedire, e qui la regola è
//! la stessa.
//!
//! # La conservazione, dichiarata
//!
//! * **Che cosa resta**: le righe di `observations`, e nient'altro. Il metro non
//!   scrive e non ha una tabella: conservare il metro è conservare il registro
//!   che si conserva già, e non esiste una copia del metro da cancellare a
//!   parte.
//! * **Per quanto**: il metro non ha un termine proprio e non ne chiede uno. Il
//!   termine è quello del registro, e **questo repository non lo dichiara**:
//!   la dichiarazione spetta all'istituto che tiene i dati, non al software che
//!   li legge. Il minimo di riferimento — la conservazione dei registri
//!   dell'impianto, ≥ 6 mesi (AI Act art. 26(6)) — è il pavimento che la pagina
//!   dell'istituto deve dichiarare, non una scelta di questo crate.
//! * **Chi ne risponde**: chi tiene il registro, cioè l'istituto, e per nome chi
//!   lo amministra. Il software non diventa il titolare del dato diventando
//!   preciso.
//! * **La tensione, non risolta**: la cancellazione è incompatibile con
//!   l'append-only, e non è un dettaglio implementativo. Qui i trigger di `V3`
//!   vietano `DELETE` su `observations`, quindi **questo modulo non cancella
//!   niente e non promette di saperlo fare**: il diritto all'oblio è una
//!   decisione dell'istituto — una procedura di esportazione e poi rimozione
//!   delle righe di quella persona — e non una riga di questo file. Dichiarare il
//!   vincolo è il compito di questo modulo; risolverlo è un'altra migrazione, e
//!   non è scritta qui.
//!
//! # Che cosa questo modulo NON fa
//!
//! * **non scrive**: nessuna tabella, nessuna colonna, nessuna riga. Il verdetto
//!   è una funzione del registro, e una funzione che scrive non è una funzione;
//! * **non decide chi è dimostrato**: dice *che cosa dice il registro*, con un
//!   criterio dichiarato. La soglia di padronanza è una convenzione e il corpus
//!   lo scrive: non può essere sepolta in un default, quindi qui ha un nome e
//!   delle righe di documentazione;
//! * **non mostra percentuali allo studente**: i numeri che tornano sono
//!   conteggi (prove, esercizi distinti, giorni), e la quota di classe è
//!   dell'insegnante.

use std::collections::{BTreeMap, BTreeSet};

use kbs_core::{ArgumentId, CohortId, CourseId, Evidence, Millis, PersonId, Relation, COHORT_MIN_K};
use serde::{Deserialize, Serialize};

use crate::codec;
use crate::error::{Error, Result};
use crate::registers::UNAIDED_OBSERVATIONS;
use crate::Store;

/// Ciò che una riga del registro deve avere perché il metro la conti.
///
/// È la parte del predicato che **non** sta nella vista, ed è una costante
/// perché le due query la scrivano una volta sola: il verificatore deterministico
/// è ciò che tiene lo strumento costante durante l'anno (vedi il doc del
/// modulo), e la prova `checked` è ciò che rende la risposta un confronto e non
/// un giudizio.
const ELIGIBILI: &str = "AND judged_by = 'deterministic' AND evidence = 'checked'";

/// Un giorno, in millisecondi.
///
/// Le costanti del criterio sono in millisecondi e non in giorni perché il
/// registro è in millisecondi, e una conversione dentro la query è una
/// conversione che può essere sbagliata senza che nessuno se ne accorga.
pub const DAY_MILLIS: i64 = 86_400_000;

/// **La soglia della claim**: quanti esercizi distinti devono essere stati
/// dimostrati senza aiuto perché un argomento sia «dimostrato».
///
/// È una costante sola, e non una per braccia, per la ragione che il corpus
/// dichiara: *«stesso argomento, stesso item, stesso verificatore, stesso
/// criterio di soglia»*. Una soglia che dipende dalla classe rende privo di
/// significato il confronto fra trattamento e controllo, e un confronto privo di
/// significato è un confronto che non può fallire — cioè una demo.
///
/// Il numero è quello che il corpus scrive per questa idea: «padroneggi questo
/// argomento se rispondi correttamente 8 item su 10 a distanza di almeno due
/// settimane». Otto è il numeratore; il denominatore dieci **non** è una
/// percentuale ma il numero di item della prova chiusa, e qui non c'è un
/// denominatore: si contano le prove che ci sono. Una percentuale sulle prove
/// tentate si riempie da sola quando lo studente smette di tentare, e chi smette
/// di tentare è esattamente il caso che il metro deve saper dire.
///
/// Nessuna fonte del corpus dice che otto è il numero giusto, e nessuna dirà che
/// è sbagliato: per questo è una costante con nome e non un letterale sparso in
/// una query. Cambiarne il valore è una dichiarazione, e deve cambiare nelle due
/// braccia insieme.
///
/// **Deve essere almeno 1.** Una soglia a zero non è una soglia bassa: è un metro
/// che dichiara dimostrato un argomento su cui non c'è nessuna prova, e che quindi
/// non misura niente. Il vincolo è dichiarato qui e non solo nei test, perché il
/// posto in cui si rompe è questo numero.
pub const MASTERY_CLAIM_THRESHOLD: usize = 8;

/// L'orizzonte di ritenzione: due settimane.
///
/// Non è la soglia, ed è per questo che ha un nome diverso: è la **geometria**
/// della misura, e dice *a che distanza* due prove devono trovarsi perché il
/// metro misuri ritenzione e non acquisizione.
pub const MASTERY_RETENTION_HORIZON: Millis = Millis(14 * DAY_MILLIS);

/// La finestra di freschezza: quattro settimane.
///
/// Anch'essa non è la soglia: è la finestra entro la quale l'ultima verifica non
/// assistita dice ancora qualcosa sul presente.
///
/// Perché quattro settimane e non un anno: la claim si misura su un **compito
/// ritardato di 4–8 settimane**, e se la finestra fosse più larga di quattro
/// settimane il compito ritardato non misurerebbe niente — il metro direbbe già
/// «dimostrato» a chi lo aveva dimostrato un mese prima, e il differenziale
/// sparirebbe. La finestra è volutamente più stretta del compito ritardato: il
/// metro non deve poter certificare il mese scorso, e l'atto che rinnova il
/// verdetto è il compito.
///
/// Deve restare **più larga** di [`MASTERY_RETENTION_HORIZON`]: due prove
/// distanti più della finestra non possono stare entrambe dentro la finestra, e
/// un criterio che non si può soddisfare non è una soglia alta, è un muro.
pub const MASTERY_FRESHNESS_WINDOW: Millis = Millis(28 * DAY_MILLIS);

/// Il criterio della claim, per intero.
///
/// È un valore e non una funzione per due ragioni: i viaggiatori devono poterlo
/// **mostrare** — la vista giustificata scrive la soglia in chiaro, e una soglia
/// che il lettore non può vedere è una soglia sepolta in un default — e i test
/// devono poterlo **confrontare** fra le due braccia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Criterion {
    /// Quanti esercizi distinti, dimostrati senza aiuto.
    pub threshold: usize,
    /// La distanza minima fra la più vecchia e la più recente dimostrazione.
    pub retention_horizon: Millis,
    /// Quanto tempo fa può essere l'ultima verifica perché il metro parli del
    /// presente.
    pub freshness_window: Millis,
}

impl Criterion {
    /// Il criterio della claim, e **l'unico** che le strade pubbliche ammettono.
    ///
    /// Non è una copia di [`MASTERY_CLAIM_THRESHOLD`] con un altro nome: è il
    /// riferimento alle tre costanti di sopra, quindi cambiarne una cambia il
    /// criterio ovunque e non in un posto solo.
    pub const OF_THE_CLAIM: Criterion = Criterion {
        threshold: MASTERY_CLAIM_THRESHOLD,
        retention_horizon: MASTERY_RETENTION_HORIZON,
        freshness_window: MASTERY_FRESHNESS_WINDOW,
    };
}

/// Una prova che il metro ha usato.
///
/// È una riga di `observations` ridotta a ciò che il metro ne sa, e l'id della
/// osservazione resta dentro perché l'evidenza di una riga del metro deve essere
/// **apribile**: una riga che dice «otto prove» senza dire quali non è
/// contestabile, e una contestazione che non può indicare la riga non è una
/// contestazione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Proof {
    /// La riga del registro da cui viene, che `kbs-verify` ricalcola.
    pub observation: String,
    /// Quando è stata registrata.
    pub at: Millis,
    /// L'esercizio: il ragionamento che lo studente possiede o non possiede.
    pub exercise: String,
    /// L'istanza: il seed con cui l'esercizio è stato reso. Serve a distinguere
    /// due risposte alla stessa domanda, non a contarne due.
    pub instance: String,
    /// Se il confronto del programma è riuscito.
    pub correct: bool,
}

/// Perché un argomento **non** è dimostrato.
///
/// I motivi sono distinti e non si sommano: sono le quattro condizioni del
/// criterio, e la ragione restituita è la prima non soddisfatta. Un verdetto con
/// due motivi deve sceglierne uno, e chi lo sceglie a caso dice al lettore una
/// cosa che non è la ragione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Nessuna prova non assistita su questo argomento: non c'è niente da
    /// valutare. «Non c'è niente» e «non lo vedi» sono risposte diverse, e
    /// restano distinte finché il predicato le distingue.
    NoProof,
    /// Le prove ci sono ma non sono abbastanza, o non sono abbastanza
    /// **distinte**. `proved` è il numero di esercizi distinti dimostrati, e serve
    /// a dirgli quanti ne mancano.
    BelowThreshold { proved: usize },
    /// L'ultima verifica non assistita è fallita. La padronanza si regge
    /// sull'ultima parola e non sul totale: otto prove e poi un errore dicono che
    /// lo sai fino a tre settimane fa.
    LastWrong,
    /// Le dimostrazioni sono abbastanza ma sono tutte accalcate: la più vecchia e
    /// la più recente distano meno dell'orizzonte di ritenzione.
    NotSpaced { days: i64 },
    /// L'ultima verifica è più vecchia della finestra. Non è uno zero: è la
    /// constatazione che il metro non può parlare del presente, e la ragione per
    /// cui il compito ritardato è l'atto che rinnova il verdetto.
    Stale { days: i64 },
}

/// Il verdetto del metro a un istante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Le quattro condizioni sono vere, adesso, su questo argomento.
    Proven,
    /// Non lo sono, e il motivo è dentro.
    NotProven(Reason),
}

impl Verdict {
    /// Se l'argomento è dimostrato a questo istante.
    pub fn is_proven(&self) -> bool {
        matches!(self, Verdict::Proven)
    }

    /// Il motivo, quando non è dimostrato.
    pub fn reason(&self) -> Option<Reason> {
        match self {
            Verdict::Proven => None,
            Verdict::NotProven(r) => Some(*r),
        }
    }
}

/// Il metro per un argomento: il verdetto e le prove che lo producono.
///
/// È una riga di uno studente per un argomento, e non un numero: la quota è
/// un'altra cosa, sta in [`CohortShare`] ed è del docente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MasteryRow {
    /// Di chi è il metro.
    pub student: PersonId,
    /// Su quale argomento.
    pub argument: ArgumentId,
    /// Il verdetto a questo istante.
    pub verdict: Verdict,
    /// Il criterio con cui è stato calcolato, **dentro la riga**: una soglia che
    /// il lettore non può vedere è una soglia sepolta in un default.
    pub criterion: Criterion,
    /// Le prove che il metro ha usato, in ordine di tempo. Nessuna prova
    /// assistita e nessuna prova ad aiuto ignoto: non sono state chieste.
    pub proofs: Vec<Proof>,
    /// Quanti **esercizi distinti** sono stati dimostrati senza aiuto. Un
    /// conteggio e mai una percentuale: la percentuale sul numero di prove
    /// tentate si riempie da sola quando lo studente smette di tentare.
    pub proved_exercises: usize,
    /// Quando è stata scritta la prima prova non assistita, se c'è.
    pub first_proof: Option<Millis>,
    /// Quando è stata scritta l'ultima, giusta o sbagliata: la data dell'ultima
    /// verifica, che è ciò che rende il metro una misura e non una foto.
    pub last_proof: Option<Millis>,
}

/// La quota di classe: la claim, calcolata su una classe.
///
/// **Esiste solo sopra `COHORT_MIN_K`.** Sotto soglia la funzione restituisce
/// `None` e non un numero: è la stessa regola di `CohortSignal` e vale per la
/// stessa ragione — «tre studenti su un argomento» con `k = 3` è, per un docente
/// che conosce i propri studenti, un elenco di nomi travestito da percentuale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CohortShare {
    /// Il corso.
    pub course: CourseId,
    /// La classe: l'etichetta della scuola, che è anche il braccio della claim.
    pub cohort: CohortId,
    /// L'inizio della finestra in cui si contano i passaggi.
    pub from: Millis,
    /// La fine: l'istante a cui il metro è valutato.
    pub to: Millis,
    /// Il criterio, identico per ogni classe e dichiarato qui, perché una quota
    /// senza criterio è un numero senza unità.
    pub criterion: Criterion,
    /// Persone, non righe: quante persone hanno almeno una prova non assistita su
    /// questo corso.
    pub students: usize,
    /// Gli argomenti del corso, che sono il denominatore.
    pub arguments: usize,
    /// Quanti (studente, argomento) sono passati da non dimostrato a dimostrato
    /// nella finestra.
    pub transitions: usize,
    /// `transitions / (students × arguments)`, in `0..=1`. È del docente: la
    /// vista dello studente non ha percentuali.
    pub quota: f64,
}

/// Il metro, in una funzione.
///
/// È puro e prende il criterio come parametro per una ragione sola: una funzione
/// che fissasse la soglia dentro non potrebbe essere esercitata con soglie diverse,
/// e un criterio che non si può esercitare è un criterio che finisce col non
/// essere quello che dice di essere. La costanza è [`Criterion::OF_THE_CLAIM`], e
/// **le due strade pubbliche di questo crate non ne ammettono un'altra**: i test
/// lo verificano confrontando il criterio che torna nei risultati, nelle due
/// braccia.
///
/// `at` è l'istante di riferimento, e **le prove successive non contano**: il
/// filtro sta qui e non nella query che le ha lette, perché un predicato che
/// vive solo nella `SELECT` è un predicato che un richiamo futuro dimentica.
pub fn meter(proofs: &[Proof], criterion: Criterion, at: Millis) -> Verdict {
    let utili: Vec<&Proof> = proofs.iter().filter(|p| p.at <= at).collect();
    // Il caso vuoto è un verdetto, non un caso speciale: nessuna prova non
    // assistita su questo argomento è `NoProof`, e non è `BelowThreshold` con
    // zero esercizi. Sono due frasi diverse — «non hai mai provato» e «hai
    // provato e non bastava» — e una pagina che le scrive uguali mente su quale
    // delle due sia vera.
    let Some(ultima_prova) = utili.last() else {
        return Verdict::NotProven(Reason::NoProof);
    };
    let esatte: Vec<&Proof> = utili.iter().copied().filter(|p| p.correct).collect();
    let distinti: BTreeSet<&str> = esatte.iter().map(|p| p.exercise.as_str()).collect();
    if distinti.len() < criterion.threshold {
        return Verdict::NotProven(Reason::BelowThreshold {
            proved: distinti.len(),
        });
    }
    if !ultima_prova.correct {
        return Verdict::NotProven(Reason::LastWrong);
    }
    // Le due date che lo spaziamento confronta sono la prima e l'ultima delle
    // **esatte**, non delle prove: un errore non assistito fra due dimostrazioni
    // non allunga l'arco, e non deve allungarlo.
    let prima_esatta = esatte.first().expect("sopra la soglia le esatte ci sono");
    let ultima_esatta = esatte.last().expect("sopra la soglia le esatte ci sono");
    let arco = ultima_esatta.at.0 - prima_esatta.at.0;
    if arco < criterion.retention_horizon.0 {
        return Verdict::NotProven(Reason::NotSpaced {
            days: giorni(arco),
        });
    }
    let distanza = at.0 - ultima_prova.at.0;
    if distanza > criterion.freshness_window.0 {
        return Verdict::NotProven(Reason::Stale {
            days: giorni(distanza),
        });
    }
    Verdict::Proven
}

/// Quanti giorni interi in un intervallo.
///
/// Interi, e non arrotondati: fra 28 giorni e due ore la risposta è `28`, e il
/// secondo che si arrotonda è il dettaglio che fa litigare due braccia della
/// claim su un numero che non è il numero.
fn giorni(millis: i64) -> i64 {
    millis / DAY_MILLIS
}

impl Store {
    /// Il metro di uno studente su un corso: una riga per argomento.
    ///
    /// La visibilità è **più stretta** di `observations_for`, ed è una scelta:
    /// chi ha emesso un giudizio vede **quel** giudizio e non ha diritto sul
    /// giudizio che l'insieme di quei giudizi produce. Il metro è la frase che
    /// dice se uno studente padroneggia un argomento, e quella frase è più
    /// eloquente di ciascuna delle sue righe. Chi può leggerla sono due — lo
    /// studente stesso, chi insegna il corso — e chi non è nessuno dei due
    /// riceve [`Error::NotReadable`].
    ///
    /// `at` è l'istante a cui il metro viene valutato, e lo sceglie il chiamante:
    /// «adesso» è una domanda a cui l'orologio risponde una volta sola, e la
    /// stessa domanda con un'altra ora è un'altra domanda.
    pub fn mastery_for(
        &self,
        person: &PersonId,
        student: &PersonId,
        course: &CourseId,
        at: Millis,
    ) -> Result<Vec<MasteryRow>> {
        self.may_read_mastery(person, course, student)?;
        // Gli argomenti che `person` può vedere: il metro copre ciò che gli è
        // stato dato, e un argomento in bozza non gli è stato dato.
        let argomenti = self.visible_arguments(person, course, None)?;
        let prove = self.proofs_of_student(student, course, at)?;
        let mut per_argomento: BTreeMap<ArgumentId, Vec<Proof>> = BTreeMap::new();
        for (argomento, prova) in prove {
            per_argomento.entry(argomento).or_default().push(prova);
        }
        let criterio = Criterion::OF_THE_CLAIM;
        Ok(argomenti
            .into_iter()
            .map(|a| {
                let prove = per_argomento.remove(&a.id).unwrap_or_default();
                Self::mastery_row(student.clone(), a.id, criterio, prove, at)
            })
            .collect())
    }

    /// La quota della claim su una classe, in una finestra.
    ///
    /// «Passa da non dimostrato a dimostrato» è una **transizione**, e una
    /// transizione ha due estremi: [`CohortShare::from`] e [`CohortShare::to`].
    /// Il solo stato finale sarebbe una foto, e una foto non distingue «ha
    /// imparato con il sistema» da «aveva già imparato prima»: sono gli stessi
    /// numeri con una parola di differenza, e quella parola è la claim.
    ///
    /// `cohort` è l'etichetta della classe ed è anche il braccio: la funzione è
    /// una sola e non ha un ramo che distingue trattamento e controllo, quindi il
    /// criterio non può diventare dipendente dal braccio senza che il tipo lo
    /// impedisca. Il criterio che torna dentro [`CohortShare`] è la prova che il
    /// chiamante non abbia cambiato le carte.
    ///
    /// Sotto `COHORT_MIN_K` restituisce `None` — la quota non esiste e non è uno
    /// zero. Lo stesso `None` torna per un corso senza argomenti: in entrambi i
    /// casi la risposta è «non c'è un numero», ed è la stessa frase.
    ///
    /// **Solo chi insegna.** Un aggregato di classe non è un dato della classe da
    /// mostrare alla classe: D9 lo dice, e un aggregato sopra soglia non rende
    /// nessuno anonimo *fra* gli studenti che lo leggono insieme.
    pub fn mastery_share(
        &self,
        person: &PersonId,
        course: &CourseId,
        cohort: &CohortId,
        from: Millis,
        to: Millis,
    ) -> Result<Option<CohortShare>> {
        if !self
            .relations_of(person, course)?
            .contains(&Relation::Teaches)
        {
            return Err(Error::NotReadable {
                person: person.clone(),
                id: ArgumentId(format!("registro:{}", cohort.0)),
            });
        }
        let argomenti = self.visible_arguments(person, course, None)?;
        if argomenti.is_empty() {
            return Ok(None);
        }
        let prove = self.proofs_of_cohort(course, cohort, to)?;
        // Persone, non righe: D9 conta chi, e contare le ripetizioni di uno
        // studente è il difetto che `CohortCountMismatch` esiste per impedire.
        let persone: BTreeSet<PersonId> = prove.iter().map(|(p, _, _)| p.clone()).collect();
        if persone.len() < COHORT_MIN_K {
            return Ok(None);
        }
        let criterio = Criterion::OF_THE_CLAIM;
        let mut transizioni = 0usize;
        for studente in &persone {
            for argomento in &argomenti {
                let prove_di: Vec<Proof> = prove
                    .iter()
                    .filter(|(p, a, _)| p == studente && a == &argomento.id)
                    .map(|(_, _, prova)| prova.clone())
                    .collect();
                let prima = meter(&prove_di, criterio, from);
                let dopo = meter(&prove_di, criterio, to);
                if !prima.is_proven() && dopo.is_proven() {
                    transizioni += 1;
                }
            }
        }
        let denominatore = persone.len() * argomenti.len();
        Ok(Some(CohortShare {
            course: course.clone(),
            cohort: cohort.clone(),
            from,
            to,
            criterion: criterio,
            students: persone.len(),
            arguments: argomenti.len(),
            transitions: transizioni,
            quota: transizioni as f64 / denominatore as f64,
        }))
    }

    /// Una riga del metro, con le prove già raggruppate per argomento.
    fn mastery_row(
        student: PersonId,
        argument: ArgumentId,
        criterion: Criterion,
        mut proofs: Vec<Proof>,
        at: Millis,
    ) -> MasteryRow {
        proofs.retain(|p| p.at <= at);
        proofs.sort_by_key(|p| p.at);
        let distinti: BTreeSet<&str> = proofs
            .iter()
            .filter(|p| p.correct)
            .map(|p| p.exercise.as_str())
            .collect();
        MasteryRow {
            student,
            argument,
            verdict: meter(&proofs, criterion, at),
            criterion,
            first_proof: proofs.first().map(|p| p.at),
            last_proof: proofs.last().map(|p| p.at),
            proved_exercises: distinti.len(),
            proofs,
        }
    }

    /// Chi può leggere il metro di uno studente.
    ///
    /// Due diritti, non tre, e il terzo mancante è una scelta: `observations_for`
    /// lascia leggere a chi ha emesso un giudizio, e quella è la difesa
    /// procedurale minima di **quel** giudizio. Il metro è un'altra cosa — è la
    /// somma, e la somma di ciò che hai valutato tu non è un diritto che ti
    /// spetta. Chi non è lo studente e non insegna riceve [`Error::NotReadable`],
    /// che è la stessa risposta di una riga che non c'è: le due non si
    /// distinguono e non possono distinguersi.
    fn may_read_mastery(
        &self,
        person: &PersonId,
        course: &CourseId,
        student: &PersonId,
    ) -> Result<()> {
        if person == student {
            return Ok(());
        }
        if self
            .relations_of(person, course)?
            .contains(&Relation::Teaches)
        {
            return Ok(());
        }
        Err(Error::NotReadable {
            person: person.clone(),
            id: ArgumentId(format!("registro:{student}")),
        })
    }

    /// Le prove che il metro può usare per uno studente su un corso.
    ///
    /// La superficie non assistita è la vista, e questo `SELECT` non la ripete:
    /// ciò che resta qui è ciò che la vista **non** dice, cioè il verificatore e la
    /// specie della prova. Le due righe che il metro non deve contare —
    /// `unaided = 0` e `unaided IS NULL` — sono fuori da qui senza che nessun
    /// codice di questo crate lo decida, che è la forma giusta: una decisione
    /// presa dal predicato di una vista non può marcire.
    fn proofs_of_student(
        &self,
        student: &PersonId,
        course: &CourseId,
        at: Millis,
    ) -> Result<Vec<(ArgumentId, Proof)>> {
        let sql = format!(
            "SELECT argument_id, id, at, evidence_payload FROM {UNAIDED_OBSERVATIONS} \
              WHERE student = ?1 AND course_id = ?2 {ELIGIBILI} AND at <= ?3 \
              ORDER BY at, seq"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let righe = stmt.query_map(
            rusqlite::params![student.0, course.0, at.0],
            |r| -> rusqlite::Result<_> {
                Ok((
                    ArgumentId(r.get::<_, String>(0)?),
                    Proof {
                        observation: r.get(1)?,
                        at: Millis(r.get::<_, i64>(2)?),
                        exercise: String::new(),
                        instance: String::new(),
                        correct: false,
                    },
                    r.get::<_, String>(3)?,
                ))
            },
        )?;
        let mut out = Vec::new();
        for riga in righe {
            let (argomento, mut prova, payload) = riga?;
            // La specie è già `'checked'` nel predicato, e il tipo torna comunque
            // a essere guardato: una riga che non è di questa forma non viene
            // contata in silenzio, viene lasciata fuori.
            if let Evidence::Checked {
                exercise,
                instance,
                correct,
            } = codec::evidence_from_db("checked", Some(payload))?
            {
                prova.exercise = exercise;
                prova.instance = instance;
                prova.correct = correct;
                out.push((argomento, prova));
            }
        }
        Ok(out)
    }

    /// Le prove che il metro può usare per una classe su un corso.
    ///
    /// La stessa frase di [`Self::proofs_of_student`] con la classe al posto
    /// dello studente. È scritta due volte e non è un refuso: una `WHERE` con due
    /// filtri opzionali è una `WHERE` in cui un filtro dimenticato non si vede, e
    /// un metro che conta una classe intera sbagliata è un numero che fa fallire
    /// il progetto nella direzione giusta, che è la peggiore in cui fallire.
    fn proofs_of_cohort(
        &self,
        course: &CourseId,
        cohort: &CohortId,
        at: Millis,
    ) -> Result<Vec<(PersonId, ArgumentId, Proof)>> {
        let sql = format!(
            "SELECT student, argument_id, id, at, evidence_payload FROM {UNAIDED_OBSERVATIONS} \
              WHERE course_id = ?1 AND cohort = ?2 {ELIGIBILI} AND at <= ?3 \
              ORDER BY student, argument_id, at, seq"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let righe = stmt.query_map(rusqlite::params![course.0, cohort.0, at.0], |r| {
            rusqlite::Result::Ok((
                PersonId(r.get::<_, String>(0)?),
                ArgumentId(r.get::<_, String>(1)?),
                Proof {
                    observation: r.get(2)?,
                    at: Millis(r.get::<_, i64>(3)?),
                    exercise: String::new(),
                    instance: String::new(),
                    correct: false,
                },
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for riga in righe {
            let (studente, argomento, mut prova, payload) = riga?;
            if let Evidence::Checked {
                exercise,
                instance,
                correct,
            } = codec::evidence_from_db("checked", Some(payload))?
            {
                prova.exercise = exercise;
                prova.instance = instance;
                prova.correct = correct;
                out.push((studente, argomento, prova));
            }
        }
        Ok(out)
    }
}
