//! Il tentativo non assistito: l'unico percorso di prodotto che scrive
//! `unaided = Some(true)`.
//!
//! Fino a questo modulo la colonna `unaided` di `V6__unaided.sql` era **inerte**:
//! l'unico scrittore era [`crate::diagnosis`], che registra correttamente `None`
//! perché una diagnosi orale non sa nulla degli aiuti. Il numeratore della claim
//! del progetto — «la quota di argomenti che passano da "non dimostrato" a
//! "dimostrato"», che `01-problema-e-tesi.html` (`#la-claim-falsificabile`) misura
//! «con la differenza osservata **sul compito a risorse chiuse, non sulla pratica
//! assistita**» — non aveva un produttore. Questo modulo è il produttore.
//!
//! # Che cosa scrive, riga per riga
//!
//! | campo della riga | valore | perché |
//! |---|---|---|
//! | `evidence` | `Checked { exercise, instance, correct }` | la prova è un confronto di programma (D8) |
//! | `judged_by` | `Deterministic` | il primo anello della catena, ed è l'unico che qui si attraversa |
//! | `unaided` | `Some(n_hints == 0)` | **derivato**, non dichiarato: vedi sotto |
//! | `n_hints` | `Some(n)` | un conteggio che il sistema ha fatto, non un default |
//! | `argument` | quello dell'esercizio | non è un campo del tentativo: vedi sotto |
//!
//! # `unaided` non è un parametro, e questa è tutta la difesa
//!
//! La firma di [`Tentativo`] **non ha un campo `unaided`**. Chi chiama non può
//! dichiarare che uno studente ha lavorato senza aiuto: può solo dire **quante
//! piste il sistema ha servito** durante quel tentativo, e la colonna segue da
//! quel numero. Il perché è la semantica che il corpus dà a `unaided` riga per
//! riga (`16-oggetto-educativo-e-corpus.html`, `#esempio-lavrato`, passo 06):
//!
//! > «`unaided = 0` perché ci sono stati due indizi: **quindi questa riga non
//! > alimenta il tasso di padronanza**, e l'indice parziale `WHERE unaided = 1`
//! > non la raccoglie. Dieci minuti dopo, alla fine della lezione, lo stesso item
//! > viene riproposto senza aiuto: `unaided = 1`, `outcome = 'correct'`. **Quella
//! > seconda riga è l'unica che conta.**»
//!
//! Quindi la colonna non descrive lo studente e non descrive la sua intenzione:
//! descrive **che cosa ha fatto il sistema**. Un sistema che serve due piste e
//! poi dichiara `unaided = 1` sta facendo la dichiarazione retroattiva che la
//! stessa migrazione `V6` vieta per la colonna (`DEFAULT 1` «dice che quegli
//! studenti hanno risposto senza aiuto: una padronanza che il sistema non ha mai
//! misurato»). Qui la regola è applicata alla sorgente invece che al default: il
//! numero è contato e la colonna è la sua conseguenza.
//!
//! # Che cosa `n_hints` è, e la parola giusta
//!
//! Il campo si chiama `n_hints` come la colonna, e la colonna si chiama così
//! perché `V6__unaided.sql` l'ha scritta. **La parola del corpus è «aiuto»**:
//! `18-provenienza-e-verifica.html` usa «aiuto» e non usa mai «hint» né
//! «indizio», e la definizione che rimanda è «il rollback dell'aiuto come
//! requisito» (pagina 05). Quindi in questo file la prosa dice *aiuto* e il
//! campo dice `n_hints`, e la discordanza è dichiarata invece di essere
//! moltiplicata.
//!
//! `n_hints` è **quante piste il sistema ha servito**, non quante lo studente ha
//! chiesto e non quante ne avrebbe potute chiedere. È l'unico conteggio che
//! questo modulo può fare senza mentire: il sistema non ha un registro delle
//! richieste, e un conteggio che non è stato fatto non è uno zero — la stessa
//! regola per cui `n_hints` è nullable e non `NOT NULL DEFAULT 0`.
//!
//! **Se l'aiuto disponibile è ignoto, questo percorso non va usato.** Un
//! tentativo di cui non si sa se c'erano piste si registra come una seconda
//! osservazione con `unaided = None`, che è il posto di
//! [`crate::diagnosis`] e la ragione per cui quel modulo scrive `None`: il
//! registro conserva «non lo so» accanto a «nessuno» perché sono due fatti
//! diversi.
//!
//! # L'argomento viene dall'esercizio, non dal chiamante
//!
//! `Tentativo` non ha un campo `argument`. La funzione chiama
//! `Store::exercise` e `Store::instances_of`, che chiedono **`teaches` sul corso**
//! (D8: «vedere l'argomento» non basta, e l'integrità è per costruzione perché la
//! risposta non è nel materiale che lo studente vede). Ne segue che:
//!
//! * non si può registrare una dimostrazione su un esercizio che non esiste, e
//!   l'esercizio inesistente e quello non leggibile danno la stessa risposta;
//! * non si può registrare una dimostrazione su un'istanza che non è stata
//!   prodotta dal generatore, e quindi la riga non è citabile;
//! * l'argomento della riga è quello che l'esercizio porta, quindi non può
//!   essere un argomento diverso da quello su cui lo studente ha lavorato.
//!
//! Sono tre invarianti che nessun test di questa funzione deve potre scavalcare,
//! e il motivo per cui l'esercizio si risolve qui dentro e non con una
//! `SELECT`.
//!
//! # Il verdetto arriva, e da dove arriva lo dichiaro
//!
//! `Tentativo::correct` è il verdetto del **verificatore deterministico**, cioè di
//! `kbs_exercise::check::grade`. Questo modulo **non lo ricalcola**, e la ragione
//! è D16 applicata al grading: una seconda copia del confronto è una seconda
//! risposta alla stessa domanda, e le due divergono. Il punto in cui `grade` e
//! questa funzione dovrebbero essere chiamate insieme **non esiste ancora**:
//! `kbs-intake` non può dipendere da `kbs-exercise` senza aggiungere una
//! dipendenza, e aggiungerla è una decisione che non è di questo modulo. È
//! dichiarato qui perché un campo `correct` senza questa riga sembrerebbe una
//! misura invece di un passaggio di valore.
//!
//! # Il verdetto è comunque un passaggio, e la riga lo dichiara
//!
//! `judged_by = Deterministic` dice **chi ha emesso il giudizio** e non **chi lo
//! ha emesso come persona**: `kbs_core::Observation::judged_by` è una *specie* di
//! giudicatore, e `kbs-store` lo dichiara (`registers.rs`, `observations_for`):
//! «`Observation.judged_by` è una *specie* di giudicatore […] non è quindi
//! tracciato come persona in questa riga». Quindi la riga che scrive qui non
//! porta il nome di chi ha registrato il tentativo, e chi lo cerca non lo trova.
//! È il buco `registro.senza-autore`, ed è il primo candidato a una migrazione: la
//! colonna che lo chiude non è un `confidence` e non è un `trust`, è
//! `observations.recorded_by TEXT REFERENCES people(id) NULL`, e il suo `NULL`
//! direbbe «registro senza questa informazione», che è la semantica che questo
//! crate usa per tutto il resto.
//!
//! # Che cosa questo modulo NON è
//!
//! * **non è il sonno.** «la componente sonno esce dal perimetro della v1»
//!   (`19-idee-livello-relazionale.html`, `#calendario-e-distacco`).
//! * **non è un interruptore.** Nessun processo decide quando questo modulo
//!   viene chiamato: lo chiama qualcuno, con un atto, e l'atto è nel registro.
//!   `19`, `#requisiti-per-la-20`: «**Non chiede un servizio sempre acceso**: l'idea
//!   11 non chiede niente».
//! * **non è una valutazione.** Il modulo registra che cosa il sistema ha
//!   verificato; non promuove nessuno e non giudica nessuno. La soglia della
//!   padronanza è un mestiere di `kbs_store::padronanza`, e questo modulo non la
//!   conosce.

use kbs_core::{CohortId, CourseId, Evidence, GraderKind, Millis, Observation, PersonId};
use kbs_store::{ObservationDraft, Register, SessionId, Store};
use sha2::Digest;

use crate::error::{Error, Result};

/// Un tentativo di uno studente su un'istanza di esercizio.
///
/// Il nome è «tentativo» e non «risposta» perché quello che si registra è
/// l'atto, non il testo: la risposta dello studente non entra nel registro, e
/// questo è deliberato (`18`, `#conservazione-e-minori`: i materiali dello
/// studente sono «dati non fidabili […] scartati a fine sessione»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tentativo {
    /// L'id, vuoto per chiedere quello derivato dal contenuto.
    pub id: String,
    /// Lo studente che ha lavorato. Deve esistere già nel registro delle persone.
    pub student: PersonId,
    /// Il corso. È anche il perimetro della relazione che autorizza a scrivere.
    pub course: CourseId,
    /// La classe in un anno: un'etichetta della scuola, non un oggetto di `kb-s`.
    pub cohort: CohortId,
    /// L'esercizio. L'argomento **non** è un campo: viene dall'esercizio.
    pub exercise: String,
    /// L'istanza, cioè il `seed` con cui il generatore l'ha prodotta.
    ///
    /// Deve essere un'istanza che esiste: una dimostrazione su un'istanza
    /// inesistente non è una misura, è una voce.
    pub instance: String,
    /// Quante piste il sistema ha servito durante questo tentativo.
    ///
    /// `0` è una misura — «nessuna pista servita» — e non un default. Chi non
    /// sa contarle non chiama questo modulo: vedi il doc sopra.
    pub n_hints: u32,
    /// Il verdetto del verificatore deterministico su questa istanza.
    pub correct: bool,
    /// Quando è successo.
    pub at: Millis,
}

impl Tentativo {
    /// L'id è derivato dal **contenuto**, non da un contatore.
    ///
    /// Come per [`crate::Diagnosis`]: un contatore darebbe a due tentativi lo
    /// stesso id dopo un ripristino da backup, che è uno dei tre limiti che
    /// `kbs-verify` dichiara. Il derived id rende visibile la collisione e rende
    /// la registrazione idempotente: lo stesso tentativo registrato due volte è
    /// lo stesso tentativo, e la seconda registrazione riceve `DuplicateId`
    /// invece di una riga in più.
    pub fn with_derived_id(mut self) -> Self {
        self.id = self.derived_id();
        self
    }

    /// L'id che avrebbe questo tentativo.
    pub fn derived_id(&self) -> String {
        let mut h = sha2::Sha256::new();
        h.update([0x2b]);
        for parte in [
            self.student.as_str(),
            self.course.as_str(),
            self.cohort.as_str(),
            self.exercise.as_str(),
            self.instance.as_str(),
        ] {
            h.update((parte.len() as u32).to_be_bytes());
            h.update(parte.as_bytes());
        }
        h.update(self.n_hints.to_be_bytes());
        h.update([u8::from(self.correct)]);
        h.update(self.at.0.to_be_bytes());
        format!("obs_{}", &hex::encode(h.finalize())[..24])
    }

    /// La disponibilità di aiuto, **derivata** dal conteggio.
    ///
    /// È una funzione pubblica e non un `if` dentro l'`INSERT` per la ragione
    /// che il doc del modulo dichiara: è l'unico punto del percorso in cui il
    /// prodotto decide che cosa la colonna dica, e renderlo ispezionabile è il
    /// modo perché un revisore possa cambiarlo e vedere che cosa cambia.
    pub fn unaided(&self) -> Option<bool> {
        Some(self.n_hints == 0)
    }

    /// La prova, nella forma di `kbs-core`.
    pub fn evidence(&self) -> Evidence {
        Evidence::Checked {
            exercise: self.exercise.clone(),
            instance: self.instance.clone(),
            correct: self.correct,
        }
    }
}

/// Registra il tentativo nel registro delle dimostrazioni.
///
/// `chi` è la persona che compie l'atto, ed è il predicato: senza `teaches` sul
/// corso la funzione non arriva nemmeno all'esercizio, quindi non registra niente.
/// È la stessa relazione che chiede `Store::exercise`, e chiederla una volta sola
/// qui evita che una seconda copia della regola diventi più permissiva.
///
/// Restituisce l'osservazione **con il `seq` che il registro le ha assegnato**:
/// è la quantità che la catena di hash di D6 ordina, e restituirla senza
/// significherebbe far credere che l'osservazione sia già nella catena.
pub fn registra(
    store: &mut Store,
    session: &SessionId,
    chi: &PersonId,
    tentativo: Tentativo,
) -> Result<Observation> {
    if tentativo.instance.trim().is_empty() {
        return Err(Error::IstanzaAssente {
            esercizio: tentativo.exercise.clone(),
            istanza: tentativo.instance.clone(),
        });
    }
    // Il predicato e il legame con l'argomento, in una chiamata.
    //
    // `NotReadable` e `NotFound` diventano **un solo messaggio**, che dice
    // «non esiste, o non lo vedi»: è la stessa scelta che fa `cli::esiste_argomento`
    // e per la stessa ragione. Distinguerli sarebbe un canale per imparare che
    // cosa c'è in un corso, e l'esercizio porta con sé l'argomento su cui la
    // dimostrazione verrebbe registrata — quindi la risposta direbbe qualcosa su
    // due corsi diversi. Un solo messaggio per i due casi è l'unica forma che non
    // apre il canale e resta leggibile da chi ha sbagliato a digitare un id.
    let esercizio = match store.exercise(chi, &tentativo.course, &tentativo.exercise) {
        Ok(e) => e,
        Err(kbs_store::Error::NotReadable { .. } | kbs_store::Error::NotFound { .. }) => {
            return Err(Error::EsercizioAssente {
                id: tentativo.exercise.clone(),
            })
        }
        Err(e) => return Err(e.into()),
    };
    // L'istanza deve essere stata prodotta dal generatore. Senza questo controllo
    // `Evidence::Checked` punterebbe a una riga che non esiste, e una prova che
    // non si può aprire non è una prova.
    if !store
        .instances_of(chi, &tentativo.course, &esercizio.id)?
        .iter()
        .any(|i| i.seed == tentativo.instance)
    {
        return Err(Error::IstanzaAssente {
            esercizio: esercizio.id,
            istanza: tentativo.instance.clone(),
        });
    }
    let draft = ObservationDraft {
        id: tentativo.id.clone(),
        student: tentativo.student.clone(),
        course: tentativo.course.clone(),
        cohort: tentativo.cohort.clone(),
        // L'argomento è quello dell'esercizio, mai quello dichiarato dal
        // chiamante: due fonti per lo stesso campo sono due risposte alla stessa
        // domanda, e le due divergono.
        argument: esercizio.argument,
        evidence: tentativo.evidence(),
        // `Deterministic` e non `Human`: la prova è un confronto di programma e
        // la catena di D8 ha un solo anello che qui si attraversa.
        judged_by: Some(GraderKind::Deterministic),
        unaided: tentativo.unaided(),
        n_hints: Some(tentativo.n_hints),
        at: tentativo.at,
    };
    Ok(store.append_observation(session, draft)?)
}

/// Apre e sigilla una sessione di `observations` intorno al tentativo.
///
/// Come per la diagnosi orale: `append_observation` pretende una sessione
/// aperta, e una sessione sigillata è ciò che rende la catena di hash calcolabile
/// su un insieme chiuso di righe. Il `note` finisce nell'id della sessione ed è
/// il posto giusto per dire **perché** si è aperta.
pub fn registra_in_una_sessione(
    store: &mut Store,
    chi: &PersonId,
    mut tentativo: Tentativo,
    nota_sessione: &str,
) -> Result<Observation> {
    if tentativo.id.trim().is_empty() {
        tentativo = tentativo.with_derived_id();
    }
    let sessione = store.open_session(Register::Observations, nota_sessione)?;
    let osservazione = registra(store, &sessione, chi, tentativo)?;
    store.close_session(&sessione)?;
    Ok(osservazione)
}
