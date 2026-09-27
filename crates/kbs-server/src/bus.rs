//! Il bus degli eventi, partizionato per corso.
//!
//! # Perché qui è diverso da `kb`
//!
//! In `kb` c'è **un bus per daemon** e nessuna partizione: lo spazio degli id
//! è globale e ogni sottoscrittore vede tutto quello che succede in tutti gli
//! `kb` caricati (`kb/crates/kb-server/src/routes/events.rs:82-98`). In un
//! sistema a un livello di fiducia è una scelta accettabile, perché non esiste
//! niente da nascondere a uno studente che legge un artefatto. In `kb-s` c'è,
//! e le consequence sono tre.
//!
//! # Tre livelli, dal più forte al più debole
//!
//! 1. **Il corso è il canale.** `Bus::subscribe` accetta un `CourseId` e
//!    restituisce il ricevitore di **quel** canale. Un evento di un altro corso
//!    non ha nemmeno un percorso che lo porti a quel ricevitore: la
//!    partizione non è un filtro, è l'assenza del collegamento. È la ragione
//!    per cui il test di partizione è possibile.
//! 2. **L'ambito è il canale.** Un canale per ogni `(corso, ambito)`. Un
//!    evento che parla di uno stato non pubblicato — «ratificato», «ratifica
//!    ritirata» — va sul canale del corpo docente, che uno studente non
//!    riceve. Un evento che parla di qualcosa che è in corso va sul canale del
//!    corso, perché è la notifica che lo studente aspetta.
//! 3. **L'argomento è il predicato.** Un evento porta l'id dell'argomento di
//!    cui parla, e ogni flusso chiama [`kbs_store::Store::read_argument`] con
//!    l'identità del proprio sottoscrittore prima di emetterlo. Quindi
//!    l'ultimo livello non è una copia del predicato: è il predicato, nel
//!    posto in cui sta.
//!
//! Il primo livello è quello che rende la promessa verificabile con un test
//! («un sottoscrittore del corso A riceve zero eventi del corso B»); il
//! terzo è quello che rende vera la promessa più difficile («non riceve
//! eventi su ciò che non può leggere»).
//!
//! # Nessun ripristino, dichiarato
//!
//! Il bus ha un contatore e un buffer di `N` eventi per canale, e **non ha un
//! log**. `Last-Event-ID` non viene onorato: riprendere significherebbe
//! mentire sul buco, e mentire sul buco in un registro è il modo più veloce
//! per rendere inutile un registro.
//!
//! Il posto dove la verità sta è la coda di ratifica e i registri, che sono
//! su disco e sono append-only. L'SSE è una notifica: dice «guarda la coda»,
//! non «questa è la coda». Un sottoscrittore lento riceve un evento `lag` con
//! il numero di eventi persi, che è l'informazione che gli serve per sapere
//! che deve andare a leggere, e non un `EventSource` che si riattacca in
//! silenzio.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use kbs_core::{ArgumentId, CourseId, Millis, PersonId};
use serde::Serialize;
use tokio::sync::broadcast;

/// Quanti eventi un canale tiene prima di dichiarare il ritardo.
///
/// Non è un buffer di riproduzione: è la finestra dentro la quale un
/// sottoscrittore lento perdeevents. Il valore è abbastanza grande da coprire
/// una scheda di pagina e abbastanza piccolo da non essere un archivio: un
/// log sarebbe una seconda copia della verità, e le copie divergono.
pub const EVENT_BUFFER: usize = 256;

/// L'ambito di un evento: a chi arriva.
///
/// Non è un ruolo. `Course` non significa «tutti quelli del corso» ma «la
/// persona che ha già superato il predicato su questo argomento», che è una
/// domanda fatta dal flusso, non dal bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Qualcuno che può leggere l'argomento di cui si parla.
    Course,
    /// Solo chi insegna il corso.
    Staff,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ChannelKey {
    course: CourseId,
    scope: Scope,
}

impl ChannelKey {
    fn new(course: &CourseId, scope: Scope) -> Self {
        ChannelKey {
            course: course.clone(),
            scope,
        }
    }
}

/// Che cosa è successo.
///
/// Le varianti sono i fatti che hanno un ascoltatore, e sono fatti **di
/// stato di pubblicazione**: la ratifica, la pubblicazione, il ritiro. Non ci
/// sono eventi di lettura, di ricerca o di presenza, perché nessuno di questi è
/// un fatto che qualcun altro abbia diritto di sapere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// Un argomento è stato ratificato per l'hash di contenuto corrente.
    Ratified,
    /// Un argomento è entrato in uso. È l'unico evento che uno studente
    /// iscritto aspetta.
    Published,
    /// La ratifica è stata ritirata: l'argomento non è più citabile.
    RatificationWithdrawn,
    /// Un argomento è stato archiviato: fuori uso, conservato.
    Archived,
}

impl EventKind {
    /// Il nome che va nell'evento SSE.
    ///
    /// In italiano e in kebab-case, come tutto il resto dei nomi che questo
    /// progetto espone: il nome del tipo è la documentazione, e un nome in
    /// inglese in un elenco di altri nomi in italiano è solo rumore.
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Ratified => "ratificato",
            EventKind::Published => "pubblicato",
            EventKind::RatificationWithdrawn => "ratifica-ritirata",
            EventKind::Archived => "archiviato",
        }
    }
}

/// Un evento, con abbastanza contesto da poterlo filtrare e da poterlo mostrare.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CourseEvent {
    /// Monotono per processo. Serve al client per dire «ho visto fino a qui»,
    /// non per riprendere: il bus non ha log.
    pub id: u64,
    /// Il corso. **Non è un filtro**: il canale è percorso, quindi questo
    /// campo è qui perché il corpo dell'evento sia leggibile da solo.
    pub corso: CourseId,
    /// L'argomento di cui si parla: è su questo id che ogni flusso riapplica
    /// il predicato.
    pub argomento: ArgumentId,
    /// Che cosa è successo.
    pub evento: &'static str,
    /// Chi l'ha fatto. Un evento senza autore è un mormorio.
    pub da: PersonId,
    /// Quando, in millisecondi UTC.
    pub quando: Millis,
}

/// Il bus.
///
/// Clone è una copia di un `Arc`: tutti gli handler condividono lo stesso bus,
/// e non c'è un modo per costruirne due che non si vedano.
#[derive(Debug, Clone)]
pub struct Bus {
    canali: Arc<RwLock<HashMap<ChannelKey, broadcast::Sender<CourseEvent>>>>,
    contatore: Arc<AtomicU64>,
}

impl Default for Bus {
    fn default() -> Self {
        Bus::new()
    }
}

impl Bus {
    /// Un bus vuoto. I canali nascono alla prima sottoscrizione.
    pub fn new() -> Self {

        Bus {
            canali: Arc::new(RwLock::new(HashMap::new())),
            contatore: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Il contatore in questo momento: da dove comincia la vista di chi si
    /// abbona adesso.
    pub fn contatore(&self) -> u64 {
        self.contatore.load(Ordering::Relaxed)
    }

    /// Il ricevitore del canale `(corso, ambito)`, creato se non c'è.
    ///
    /// Il ricevitore appartiene a un solo canale, e il canale appartiene a un
    /// solo corso: da qui in poi, per costruzione, questo sottoscrittore non
    /// può ricevere niente che non sia di questo corso. Non è una promessa di
    /// filtraggio, è l'assenza di un percorso.
    pub fn subscribe(&self, course: &CourseId, scope: Scope) -> broadcast::Receiver<CourseEvent> {
        let chiave = ChannelKey::new(course, scope);
        let mut canali = self.scrivibile();
        canali
            .entry(chiave)
            .or_insert_with(|| broadcast::Sender::new(EVENT_BUFFER))
            .subscribe()
    }

    /// Pubblica un evento sul canale del suo corso e del suo ambito.
    ///
    /// Il numero di sottoscrittori non è un errore: pubblicare in una stanza
    /// vuota è la norma, non l'eccezione.
    pub fn publish(&self, corso: &CourseId, scope: Scope, evento: CourseEvent) {
        let chiave = ChannelKey::new(corso, scope);
        let mittente = {
            let canali = self.scrivibile();
            canali.get(&chiave).map(|s| s.clone())
        };
        let Some(mittente) = mittente else {
            tracing::debug!(corso = %corso, evento = evento.evento, "nessun sottoscrittore");
            return;
        };
        // `send` fallisce senza destinatari: si registra e si va avanti, perché
        // un evento che nessuno ascolta è un evento che è successo.
        if mittente.send(evento).is_err() {
            tracing::debug!(corso = %corso, "nessun sottoscrittore");
        }
    }

    /// Il prossimo id. Monotono per processo, e dichiarato tale: al prossimo
    /// avvio ricomincia da 1, e nessuno lo usa come un id di registro.
    pub fn next_id(&self) -> u64 {
        self.contatore.fetch_add(1, Ordering::Relaxed)
    }

    /// Quanti canali vivi. Serve al test di partizione e al log di avvio.
    pub fn canali_vivi(&self) -> usize {
        self.leggibile().len()
    }

    fn scrivibile(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<ChannelKey, broadcast::Sender<CourseEvent>>> {
        self.canali.write().unwrap_or_else(|v| v.into_inner())
    }

    fn leggibile(&self) -> std::sync::RwLockReadGuard<'_, HashMap<ChannelKey, broadcast::Sender<CourseEvent>>> {
        self.canali.read().unwrap_or_else(|v| v.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evento(id: u64, corso: &CourseId, argomento: &ArgumentId) -> CourseEvent {
        CourseEvent {
            id,
            corso: corso.clone(),
            argomento: argomento.clone(),
            evento: EventKind::Published.as_str(),
            da: PersonId::fixture(1),
            quando: Millis(1_700_000_000_000),
        }
    }

    #[test]
    fn un_sottoscrittore_del_corso_a_non_sente_il_corso_b() {
        let bus = Bus::new();
        let a = CourseId::fixture(1);
        let b = CourseId::fixture(2);
        let _arg = ArgumentId::from_rel_path("corsi/a/uno.html");
        let altro = ArgumentId::from_rel_path("corsi/b/due.html");

        let mut rx = bus.subscribe(&a, Scope::Course);
        bus.publish(&b, Scope::Course, evento(1, &b, &altro));
        bus.publish(&b, Scope::Staff, evento(2, &b, &altro));

        // `try_recv` non aspetta: se il partizionamento regge, il canale è
        // vuoto e la risposta è `Empty`. Se non regge, l'evento è già lì e
        // questo test lo dice.
        assert!(matches!(rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    }

    #[test]
    fn lo_stesso_corso_su_due_ambiti_sono_due_canali() {
        let bus = Bus::new();
        let a = CourseId::fixture(1);
        let arg = ArgumentId::from_rel_path("corsi/a/uno.html");
        let mut studente = bus.subscribe(&a, Scope::Course);
        let mut docente = bus.subscribe(&a, Scope::Staff);
        assert_eq!(bus.canali_vivi(), 2);

        bus.publish(&a, Scope::Staff, evento(1, &a, &arg));
        assert!(matches!(
            studente.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert!(matches!(docente.try_recv(), Ok(_)));
    }

    #[test]
    fn un_corso_senza_nessuno_non_e_un_errore() {
        let bus = Bus::new();
        bus.publish(
            &CourseId::fixture(9),
            Scope::Course,
            evento(1, &CourseId::fixture(9), &ArgumentId::from_rel_path("x.html")),
        );
        assert_eq!(bus.canali_vivi(), 0);
    }
}
