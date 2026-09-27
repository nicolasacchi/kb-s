//! «Il mentor a tempo»: la coda di richiamo di uno studente, per argomento.
//!
//! Questo modulo è una **lettura**, e la cosa più importante che dice è che non
//! è un calendario di scadenze. Un calendario di scadenze ha bisogno di un
//! numero — quanti giorni, quante settimane, quale curva — e quel numero è una
//! affermazione sulla memoria di una persona. Qui **non c'è nessun numero**:
//! l'orizzonte è un argomento di ogni chiamata, dichiarato da chi chiama, e
//! senza orizzonte la funzione non inventa nulla e restituisce la superficie
//! intera. Vedi «L'intervallo non è un numero di questo crate» sotto.
//!
//! # Che cosa è, in una riga
//!
//! La coda di richiamo agganciata all'argomento. La formula è del corpus
//! (`16-oggetto-educativo-e-corpus.html`, `#cosa-funziona`, riga «Nota / mazzo di
//! spaced repetition»):
//!
//! > «**Scelta proposta:** Si usa come **coda di recupero** agganciata
//! > all'`argomento`, non come unità.»
//!
//! # L'unità è la coppia studente-argomento, e perché non è l'argomento solo
//!
//! Le tre candidate sono l'argomento, l'esercizio e la coppia. La coppia vince,
//! e ognuna delle due altre è esclusa per una ragione che il corpus scrive:
//!
//! * **l'argomento da solo** non è un calendario di nessuno: è il corso. La
//!   claim del progetto conta «la quota di argomenti che passano da
//!   "non dimostrato" a "dimostrato"» (`01-problema-e-tesi.html`,
//!   `#la-claim-falsificabile`) e quel conteggio è **per studente**: due
//!   persone che hanno fatto la metà degli argomenti hanno due quote diverse, e
//!   un elenco di argomenti non distingue le due.
//! * **l'esercizio da solo** è il grano della pratica, non quello del valore.
//!   `16`, `#idee`, decisione D1: «L'unità è `argomento`, non documento né
//!   item […] **Solo questa incapsula «che cosa lo studente ha dimostrato»
//!   accanto al testo.**» E `#le-sei-parti`, parte 5: le osservazioni sono
//!   «**una riga per item per studente per momento**», cioè l'item è il grano
//!   della prova, non l'unità che la claim conta.
//! * **la coppia** non esiste nel corpus e qui viene creata, dichiarata. Il
//!   `SILENZIO` è reale: nel DDL di `16` non c'è tabella, indice o vista su
//!   `(actor_id, argomento_uid)`. La ragione per crearla è che è l'unica
//!   granularità che è *contemporaneamente* per persona e per unità di valore:
//!   tutto il resto o perde la persona o perde l'argomento.
//!
//! ## Perché è per studente e non per classe
//!
//! Una coda di classe sarebbe un elenco di persone ordinate per una misura, e
//! un elenco di persone ordinate per una misura è una graduatoria. Il corpus lo
//! dice della vista del docente (`18-provenienza-e-verifica.html`,
//! `#le-due-viste-in-concreto`): «**Nessun ordinamento per rendimento.** L'elenco
//! è per argomento e per data, mai per punteggio: un elenco ordinato per valore
//! è un invito a usarlo come graduatoria». E dice anche che la discesa
//! all'individuo è un atto: «Si arriva a un singolo studente solo da una
//! tabella aggregata, con un motivo dichiarato, e la discesa scrive una riga».
//!
//! **Quindi il predicato di questa funzione è `teaches` e nient'altro**: chi
//! non insegna il corso riceve `NotReadable`, e non un elenco vuoto. Lo
//! studente **non** può chiedere il proprio calendario, e la ragione è scritta
//! sotto: un calendario dice per conto tuo che cosa ti resta da fare, che è la
//! forma più corta di un cruscotto.
//!
//! # Le righe vengono dalla vista, e la ragione è che la vista è la relazione
//!
//! La query legge `unaided_observations`, non `observations`. I lettori della
//! superficie non assistita sono tre — lo studente
//! ([`Store::observations_for`](crate::Store::observations_for)), il segnale di
//! coorte di D9 ([`Store::record_cohort_signal`]) e questo calendario — e se
//! prendessero le righe da tre posti sarebbero tre misure della stessa cosa con
//! tre denominatori. È il difetto che D16 descrive: due tabelle che si
//! verificano a vicenda passano anche quando sono sbagliate entrambe.
//!
//! Una riga con `unaided` **ignoto** non entra, e non per una scelta di questo
//! file: la vista ha il `WHERE unaided = 1` nella sua definizione, in
//! `V6__unaided.sql`, e `NULL = 1` non è vero.
//!
//! ## La differenza da `padronanza`, dichiarata perché due criteri diversi sulla
//! stessa coda di righe sono il difetto che questo prodotto non può permettersi
//!
//! La quota della claim filtra anche per `evidence = 'checked'` e
//! `judged_by = 'deterministic'`, perché «dimostrato» è un verdetto e un
//! verdetto lo dà un programma. **Il calendario non filtra per niente altro**,
//! e i due criteri oggi coincidono: gli unici due scrittori del sistema sono
//! `kbs_intake::pratica`, che scrive `Checked` con `Deterministic`, e
//! `kbs_intake::diagnosis`, che scrive `unaided = None` e quindi non entra nella
//! vista. Ogni riga che il prodotto mette in `unaided_observations` è quindi
//! anche una riga che `padronanza` conta.
//!
//! Coincidono **oggi**, e questa è la frase da rileggere per prima se un domani
//! esiste un terzo scrittore. Divergerebbero senza che nessuno lo veda, perché i
//! due moduli hanno due domande diverse e nessuno dei due può controllare l'altro.
//! Il posto in cui la divergenza si vede è qui: `last_correct` è `None` quando
//! il calendario guarda la **ricomparsa** mentre `padronanza` guarda il
//! **verdetto**. Se un terzo scrittore arriverà, la riga da scrivere è in questo
//! file e il nome del buco è `superficie.due-criteri`.
//!
//! # Che cosa c'è in una voce, e che cosa non c'è
//!
//! Tre fatti e nient'altro: quante volte lo studente ha dimostrato quell'
//! argomento senza aiuto, quando l'ha fatto l'ultima volta, e se l'ultima
//! volta l'ha fatto bene. **Non c'è il titolo, non c'è la percentuale, non c'è
//! il conteggio dei tentativi assistiti, non c'è il «giorni di ritardo».**
//!
//! * Il titolo manca per una ragione tecnica e non per modestia: portarlo
//!   significherebbe una `read_argument` per voce, e questa funzione già ha
//!   risolto l'accesso una volta sola sul corso.
//! * I giorni di ritardo mancano per una ragione di sostanza: sono il numero
//!   che diventa un indicatore di «sei in ritardo», e quel numero è
//!   esattamente ciò che il corpus vieta di mettere davanti a uno studente.
//!
//! L'ordine è **per data, mai per merito**: più vecchia ricomparsa prima, e a
//! parità di data per id, così due caricamenti della stessa pagina danno lo
//! stesso ordine. Un argomento dimostrato male ieri viene **dopo** un
//! argomento dimostrato bene il mese scorso, ed è la prova che l'ordinamento
//! non è un giudizio travestito.
//!
//! # L'intervallo non è un numero di questo crate
//!
//! Il corpus lo vieta tre volte, e le tre sono qui perché sono la giustificazione
//! di una firma con `Option<Millis>` e non con un `Millis`:
//!
//! 1. `11-evidenze-scientifiche.html`, `#la-fragilita-strutturale`: «**Nessuno
//!    dei tre risultati principali misura la ritenzione a settimane o mesi. È il
//!    buco più grave e più sistematico dell'intero campo.**» Un intervallo
//!    settiminale è un numero che nessuno ha misurato su questo campo.
//! 2. `16`, `#limiti`, voce 1: «**Nessun numero di scala è inventato, e nessuno
//!    è disponibile.** […] **la cardinalità della coda di richiamo**: sono
//!    parametri di progetto, non risultati, e questa pagina non li stima. Dove
//!    servono, la cella dice «da misurare in proprio».»
//! 3. `19-idee-livello-relazionale.html`, `#calendario-e-distacco`, idea 11: il
//!    meccanismo «**Riceve tre input: la scadenza dell'esame, la data del primo
//!    incontro, e la disponibilità dichiarata dello studente. Da questi tre
//!    calcola l'intervallo di spaziamento**». Anche l'idea che *progetta* il
//!    calendario prende l'intervallo dall'esterno: non lo scrive.
//!
//! Quindi [`Store::calendar`] non ha una soglia, una costante o un default. Il
//! chiamante passa `orizzonte` e ottiene un verdetto che è suo; se non lo
//! passa, `beyond` è `None` su ogni voce e la funzione **non sa** che cosa
//! sarebbe il ritardo. Un calendario che sa da solo quando uno studente ha
//! dimenticato è un'affermazione sulla sua memoria firmata da un componente di
//! storage.
//!
//! «Oltre l'orizzonte» non significa che lo studente ha dimenticato. Significa
//! che l'ultima volta in cui lo ha dimostrato senza aiuto è più vecchia
//! dell'istante che il docente ha dichiarato in questa chiamata, e il docente
//! ha dichiarato quell'istante.
//!
//! # Che cosa questo modulo NON è, e le ragioni sono tutte citate
//!
//! * **Non è un agente che interrompe.** `19`, `#requisiti-per-la-20`: «**Non
//!   chiede un servizio sempre acceso**: l'idea 11 non chiede niente, l'idea 26
//!   chiede che il costo di authoring lo paghi lo studente, e le altre sono o
//!   asincrone o presidiate da un docente.» Non c'è un processo che decide
//!   quando scrivere a qualcuno, e non ci sarà: un calendario che il docente
//!   apre è dentro il perimetro, un demone che decide l'interruzione è fuori.
//! * **Non è una superficie dello studente.** `18`,
//!   `#il-test-icap-su-una-verifica`: «**La vista dello studente non mostra
//!   percentuali**: mostra *righe con prova* e un bottone per contestarle.» E
//!   `09-esercizi-valutazione-grading.html`, `#limite-dashboard`: «qualsiasi
//!   metrica esposta allo studente diventa presto la funzione obiettivo del suo
//!   comportamento». Le righe con prova lo studente le ha già, per la vista
//!   `unaided_observations`: non gli si aggiunge un elenco di ciò che gli
//!   resta.
//! * **Non traccia il sonno.** `19`, `#calendario-e-distacco`: «**la componente
//!   sonno esce dal perimetro della v1**, e resta come opzione dichiarata allo
//!   studente, non come default», e il motivo è dichiarato lì: «non è supportata
//!   da nulla che questa pagina abbia potuto verificare».
//! * **Non è lo strumento che giudica la claim.** `11`,
//!   `#la-metrica-che-il-prodotto-sceglie`: «un prodotto AI che genera le proprie
//!   attività è anche, in larga misura, il generatore del proprio test finale […]
//!   la ragione strutturale per cui la valutazione va commissionata da fuori».
//!   Le righe di questo calendario vengono dagli esercizi del sistema: sono una
//!   coda di lavoro, non un verdetto.
//!
//! # La conservazione, dichiarata come va dichiarata
//!
//! Il corpus: «**La cancellazione è incompatibile con l'append-only, e non è un
//!   dettaglio implementativo** — Un registro di minori che si append e non si
//!   cancella è in tensione diretta con il diritto all'oblio. La tensione non si
//!   risolve: si progetta intorno, dichiarando cosa si conserva, per quanto, e chi
//!   ne risponde.» (`18`, `#limiti`.)
//!
//! * **Che cosa si conserva**: niente di nuovo. Questa funzione è una `SELECT` e
//!   non scrive una riga; il banco di prova lo verifica contando le osservazioni
//!   prima e dopo. Quindi **non aggiunge costo di conservazione**: il
//!   calendario non crea un secondo archivio della stessa cosa.
//! * **Per quanto**: il periodo è quello del registro delle dimostrazioni, che
//!   `18`, `#la-politica-di-conservazione`, dichiara per la padronanza come «Per
//!   la vita del corso + il quadrimestre; per l'archivio scolastico, l'obbligo di
//!   legge che sopravvive». Il minimo di legge che il corpus nomea per i log del
//!   deployer è di **sei mesi** (AI Act art. 26(6)), ed è un altro oggetto: qui
//!   non si scrive «append-only e conforme GDPR» nella stessa riga.
//! * **Chi ne risponde**: chi insegna il corso, che è l'unica relazione che
//!   apre questa strada. Lo studente non la usa e non la chiede.
//! * **Il rischio che resta, detto perché resta**: un calendario è una sequenza
//!   di eventi con timestamp, ordine e argomenti, e `18`,
//!   `#conservazione-e-minori` lo dice di un'altra forma dello stesso
//!   problema — «**la rimozione della chiave non cancella il pattern**. Una
//!   sequenza di eventi con timestamp, ordine e argomenti è un profilo». Il
//!   calendario non crea quel profilo, ma è la sua **lettura più comoda**: per
//!   questo esiste solo come discesa dichiarata dal docente e non come pagina.

use kbs_core::{ArgumentId, CourseId, Millis, PersonId, Relation};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::registers::UNAIDED_OBSERVATIONS;
use crate::Store;

/// Una voce della coda di richiamo: un argomento, e l'ultima volta che questo
/// studente lo ha dimostrato senza aiuto.
///
/// I tre fatti sono quelli che il registro sa e nient'altro. `last_correct` è
/// `None` quando l'ultima ricomparsa non è una prova verificata dal programma:
/// è «non lo so», non «sbagliato», e la distinzione è la stessa che rende
/// `unaided` nullable invece che `bool` con un default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEntry {
    /// L'argomento di cui è stato dimostrato qualcosa senza aiuto.
    pub argument: ArgumentId,
    /// Quante ricomparse non assistite ci sono su quell'argomento.
    ///
    /// È un conteggio di **righe**, non di persone: qui la persona è una sola
    /// ed è dichiarata in [`Calendar`], quindi la confusione che D9 vieta non
    /// può nascere. Non è un punteggio e non va ordinato per questo numero.
    pub unaided: usize,
    /// Quando è avvenuta l'ultima ricomparsa non assistita.
    pub last_unaided: Millis,
    /// Se l'ultima ricomparsa è stata giusta. `None` = la prova non è
    /// verificata dal programma, e nessun verdetto ne è ricavabile.
    pub last_correct: Option<bool>,
    /// Se l'ultima ricomparsa è più vecchia dell'orizzonte dichiarato.
    ///
    /// `None` quando il chiamante non ha dichiarato nessun orizzonte: in quel
    /// caso la risposta è «non lo so», ed è l'unica risposta onesta.
    pub beyond: Option<bool>,
}

impl CalendarEntry {
    /// Il verdetto sull'orizzonte, quando l'orizzonte c'è.
    ///
    /// È una funzione e non un `if` sparso: il numero che la firma di
    /// [`Store::calendar`] prende è l'unico posto in cui il prodotto ha potuto
    /// scegliere un numero, e qui non ne sceglie nessuno — confronta due
    /// istanti che il chiamante gli ha dato.
    pub fn beyond(horizon: Option<Millis>, last_unaided: Millis) -> Option<bool> {
        horizon.map(|h| last_unaided < h)
    }
}

/// La coda di richiamo di uno studente su un corso.
///
/// `horizon` è qui e non dentro le voci perché è **una dichiarazione del
/// chiamante** e va letta una volta sola: se fosse in ogni voce, due voci
/// potrebbero essere state calcolate con due orizzonti diversi e la pagina
/// mostrerebbe un verdetto e la sua spiegazione presi da due domande diverse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    /// Lo studente di cui è la coda. Una persona, dichiarata: qui non c'è una
    /// classe, e non ci sarà.
    pub student: PersonId,
    /// Il corso su cui è la coda.
    pub course: CourseId,
    /// L'orizzonte dichiarato dal chiamante, se lo ha dichiarato.
    pub horizon: Option<Millis>,
    /// Le voci, dalla ricomparsa più vecchia alla più recente.
    pub entries: Vec<CalendarEntry>,
}

impl Calendar {
    /// Quante voci sono **oltre** l'orizzonte dichiarato.
    ///
    /// È un conteggio e non una percentuale, e non è un punteggio: nessuna
    /// superficie di questo prodotto trasforma un insieme di fatti su una
    /// persona in un numero che la persona deve migliorare.
    pub fn beyond(&self) -> usize {
        self.entries.iter().filter(|e| e.beyond == Some(true)).count()
    }
}

impl Store {
    /// La coda di richiamo di uno studente su un corso: gli argomenti su cui
    /// **ha** ricomparso senza aiuto, con la data dell'ultima volta.
    ///
    /// **Gli argomenti su cui non è mai ricomparso non ci sono, e non è un
    /// difetto.** Quelli sono il denominatore della claim, non la coda di
    /// richiamo, e metterli qui significa prendere le righe da due relazioni
    /// diverse — `unaided_observations` e `arguments` — e avere due definizioni
    /// della superficie non assistita che possono divergere. Il denominatore
    /// è un mestiere diverso, e sta in `padronanza`.
    ///
    /// **Il predicato è `teaches` sul corso e nient'altro.** Lo studente non
    /// può chiedere il proprio calendario: un calendario è l'elenco di ciò che
    /// resta da fare, e un elenco di ciò che resta da fatto esposto a chi lo
    /// deve fare è un cruscotto con un obiettivo dentro. Chi non insegna
    /// riceve `NotReadable` — la stessa risposta, byte per byte, che chi non
    /// insegna riceve su un corso inesistente, perché le due domande non hanno
    /// la stessa risposta e qui la risposta è una sola.
    ///
    /// `horizon` è l'orizzonte **del chiamante**: se è `None`, `beyond` è
    /// `None` ovunque e la funzione non ha opinioni sul ritardo. Vedi il doc
    /// del modulo per perché questo crate non contiene un numero di giorni.
    pub fn calendar(
        &self,
        person: &PersonId,
        student: &PersonId,
        course: &CourseId,
        horizon: Option<Millis>,
    ) -> Result<Calendar> {
        if !self
            .relations_of(person, course)?
            .contains(&Relation::Teaches)
        {
            return Err(Error::NotReadable {
                person: person.clone(),
                id: ArgumentId(format!("calendario:{student}")),
            });
        }
        // La vista, non la tabella: la stessa relazione da cui legge lo studente
        // e da cui `count_failing` conta la classe. Tre lettori, una fonte.
        let sql = format!(
            "SELECT o.argument_id, \
                    COUNT(*), \
                    MAX(o.at), \
                    (SELECT json_extract(u.evidence_payload, '$.correct') \
                       FROM {UNAIDED_OBSERVATIONS} u \
                      WHERE u.student = o.student \
                        AND u.course_id = o.course_id \
                        AND u.argument_id = o.argument_id \
                      ORDER BY u.at DESC, u.seq DESC, u.id DESC LIMIT 1) \
               FROM {UNAIDED_OBSERVATIONS} o \
              WHERE o.student = ?1 AND o.course_id = ?2 \
              GROUP BY o.argument_id \
              ORDER BY MAX(o.at), o.argument_id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut entries = Vec::new();
        let righe = stmt.query_map(
            rusqlite::params![student.0, course.0],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )?;
        for riga in righe {
            let (argument, unaided, last, correct) = riga?;
            let last_unaided = Millis(last);
            entries.push(CalendarEntry {
                argument: ArgumentId(argument),
                unaided: unaided as usize,
                last_unaided,
                // `json_extract` su un JSON `true`/`false` dà `1`/`0`; su una
                // prova che non ha il campo dà `NULL`, ed è `last_correct:
                // None`. Il `!= 0` è qui e non nella query perché la traduzione
                // di un intero SQLite in un booleano è una decisione di
                // dominio e va leggibile.
                last_correct: correct.map(|c| c != 0),
                beyond: CalendarEntry::beyond(horizon, last_unaided),
            });
        }
        Ok(Calendar {
            student: student.clone(),
            course: course.clone(),
            horizon,
            entries,
        })
    }
}
