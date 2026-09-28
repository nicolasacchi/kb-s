//! Il metro della padronanza in lettura: due superfici, due destinatari.
//!
//! # Le due risposte sono diverse perché i due destinatari sono diversi
//!
//! * `/padronanza` è **del singolo**: righe con la prova e il verdetto, e
//!   niente percentuali. Chi la chiede è lo studente o il suo docente, e la
//!   pagina ne fa righe.
//! * `/padronanza/quota` è **del docente**: la quota della classe, che è
//!   l'aggregato di D9 e quindi ha la soglia `k`. Sotto soglia il corpo è
//!   `{"quota": null}` e non un numero: un numero sotto soglia è un canale, e un
//!   canale non si chiude con un flag.
//!
//! Un corpo solo sarebbe stato più semplice da scrivere e avrebbe mescolato le
//! due cose: lo studente avrebbe avuto un numero di classe e il docente una
//! risposta che non distingue «non c'è» da «zero».
//!
//! # Qui non si decide il criterio
//!
//! Il criterio vive in `kbs_store::padronanza` e questa rotta lo **riporta** nel
//! corpo (`criterion`), senza toccarlo. Il motivo è che il criterio è una
//! convenzione dichiarata, e una convenzione che il lettore non può vedere è
//! una convenzione sepolta in un default: la pagina scrive «otto esercizi
//! distinti» in chiaro, e quel numero viene da qui.
//!
//! # `at` è un parametro, non un orologio
//!
//! «Adesso» è una domanda a cui l'orologio del server risponde una volta sola. Il
//! compito ritardato di cui la claim parla è una domanda con un'altra ora, e per
//! rifarla domani — o per ricalcolarla fra cinque anni, o per riprodurla (D11) —
//! occorre poter dire *quando* si sta guardando. Quindi `?at=` esiste, e quando
//! manca vale l'ora della richiesta.
//!
//! # Perché qui non c'è un filtro
//!
//! La coda di practice e le osservazioni ad aiuto ignoto sono escluse dalla
//! **vista** `unaided_observations`, dentro il database, e dal predicato del
//! criterio in `kbs-store`. Un filtro in questa rotta sarebbe un quarto posto in
//! cui la stessa frase è scritta, e il quarto posto è quello che un domani non
//! viene aggiornato. Qui non c'è `WHERE unaided`.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use kbs_core::{CohortId, CourseId, Millis, PersonId, Relation};
use kbs_store::padronanza::{CohortShare, Criterion, MasteryRow, MASTERY_FRESHNESS_WINDOW};

use crate::capability;
use crate::error::ApiError;
use crate::identity::SharedIdentity;
use crate::ids;
use crate::routes::AppState;

/// La query del metro del singolo.
///
/// `person` è opzionale e vale l'identità dichiarata, come in ogni altra rotta
/// dei registri: uno studente che guarda il proprio metro non deve scrivere il
/// proprio id in ogni URL, e un docente che guarda il metro di uno studente sì.
#[derive(Debug, Default, Deserialize)]
pub struct MeterQuery {
    /// La persona di cui si vuole il metro.
    pub person: Option<String>,
    /// L'istante a cui il metro viene valutato, in millisecondi. Se manca, l'ora
    /// della richiesta.
    pub at: Option<i64>,
}

/// La query della quota di classe.
///
/// `cohort` è l'etichetta della classe ed è **obbligatoria**: la quota è
/// l'aggregato di una classe, e chiedere «la quota» senza dire di quale è una
/// domanda a cui si può rispondere con qualunque numero.
#[derive(Debug, Default, Deserialize)]
pub struct ShareQuery {
    /// La classe. È l'etichetta della scuola, che è anche il braccio della claim.
    pub cohort: String,
    /// L'inizio della finestra dei passaggi, in millisecondi. Se manca vale `a`
    /// meno la finestra di freschezza; se è **dopo** `a` la richiesta è un `400`,
    /// perché con `from > to` il predicato di `mastery_share` smette di contare
    /// le transizioni e conta le perdite.
    pub da: Option<i64>,
    /// La fine della finestra, in millisecondi. Se manca, l'ora della richiesta.
    pub a: Option<i64>,
}

/// `GET /api/v1/courses/{course}/padronanza?person=…&at=…`
///
/// `200 {"course": …, "student": …, "at": …, "criterion": {…}, "rows": […]}` ·
/// `404` se non è lo studente e non è chi insegna il corso.
///
/// Le righe sono una per argomento, con le prove che il metro ha usato. **Non
/// ci sono percentuali**: i numeri sono conteggi, e un conteggio di prove lo
/// studente può discuterlo riga per riga; una percentuale no.
///
/// Chi non ha diritto riceve `404` e non un elenco vuoto: un elenco vuoto qui
/// significherebbe che lo studente non ha dimostrato niente, che è un'altra
/// informazione.
pub async fn meter(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
    Query(chi): Query<MeterQuery>,
) -> Result<Json<MeterResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let studente = chi.persona(&identita)?;
    let at = chi.istante();
    let righe = state.db.read_api(|store| {
        // Il controllo di relazione non aggiunge niente a `mastery_for`, che lo fa
        // già: è dichiarato per la stessa ragione per cui lo è in tutte le altre
        // rotte di corso — un corso inesistente e un corso di cui non si fa parte
        // danno la stessa risposta, e senza questo controllo il primo darebbe un
        // `404` e il secondo un elenco vuoto, che sono due risposte diverse a due
        // domande diverse che insegnerebbero la stessa cosa: che quel corso
        // esiste.
        capability::require(store, &persona, &course, None)?;
        Ok(store.mastery_for(&persona, &studente, &course, at)?)
    })?;
    Ok(Json(MeterResponse {
        course,
        student: studente,
        at,
        criterion: Criterion::OF_THE_CLAIM,
        rows: righe,
    }))
}

/// `GET /api/v1/courses/{course}/padronanza/quota?cohort=…&da=…&a=…`
///
/// `200 {"criterion": {…}, "quota": null | {…}}` · `404` se non `teaches` ·
/// `400` se la classe non è un'etichetta o se `da` è dopo `a`.
///
/// `quota: null` **non è uno zero**: sotto `COHORT_MIN_K` la quota non esiste, e
/// la pagina lo dice con una frase invece di mostrare un numero. Il criterio
/// resta nel corpo anche quando la quota non c'è, perché il criterio è una
/// costante del prodotto e non un conteggio di persone.
///
/// `da` e `a` sono gli estremi della finestra in cui si contano i passaggi da non
/// dimostrato a dimostrato. Se `da` manca vale `a` meno la finestra di
/// freschezza, ed è il default dichiarato nel metodo. **`da` dopo `a` è una
/// `400`, non una finestra raddrizzata: vedi `ShareQuery::finestra`.**
///
/// I due `400` vengono **prima** del controllo di relazione, e devono: la
/// risposta a una classe malformata o a una finestra capovolta dipende solo dai
/// parametri che ha scritto chi chiama, quindi non dice niente del corso e non
/// è un canale. La classe **assente** invece è un `null` e non un `400`, perché
/// un'assenza è una domanda incompleta e non una domanda sbagliata.
pub async fn share(
    State(state): State<AppState>,
    identita: SharedIdentity,
    Path(course): Path<String>,
    Query(chi): Query<ShareQuery>,
) -> Result<Json<ShareResponse>, ApiError> {
    let course = ids::course(&course).ok_or(ApiError::Absent)?;
    let persona = identita.person().clone();
    let classe = chi.classe()?;
    let (da, a) = chi.finestra()?;
    let quota = state.db.read_api(|store| {
        capability::require(store, &persona, &course, Some(Relation::Teaches))?;
        match classe {
            // Nessuna classe nominata non è «la classe con quota zero»: è
            // nessuna classe. La risposta è la stessa che sotto soglia, cioè
            // l'assenza del numero, e il perché non torna nel corpo — «non hai
            // detto quale classe» non rivela niente di nessuna classe. La
            // relazione si controlla **prima**, perché a uno studente questa
            // rotta risponde «assente» e non `null`.
            None => Ok(None),
            Some(cohort) => Ok(store.mastery_share(&persona, &course, &cohort, da, a)?),
        }
    })?;
    Ok(Json(ShareResponse {
        criterion: Criterion::OF_THE_CLAIM,
        quota,
    }))
}

/// Il metro di uno studente, con il criterio che l'ha prodotto.
#[derive(Debug, Serialize)]
pub struct MeterResponse {
    /// Il corso a cui il metro appartiene.
    pub course: CourseId,
    /// Di chi è il metro.
    pub student: PersonId,
    /// L'istante a cui è stato valutato: la data dell'ultima verifica è ciò che
    /// distingue una misura da una foto, e qui c'è l'istante del metro.
    pub at: Millis,
    /// Il criterio, dichiarato. Non è un campo informativo: è la soglia in
    /// chiaro, senza la quale la pagina starebbe mostrando un verdetto di cui
    /// nessuno conosce la regola.
    pub criterion: Criterion,
    /// Una riga per argomento che lo studente può vedere. Con la prova dentro.
    pub rows: Vec<MasteryRow>,
}

/// La quota di classe, e il nulla che sotto soglia non è uno zero.
#[derive(Debug, Serialize)]
pub struct ShareResponse {
    /// Il criterio della claim. Resta anche quando `quota` è `null`: è una
    /// convenzione dichiarata, non un conteggio.
    pub criterion: Criterion,
    /// La quota, se esiste. `null` significa che **non esiste** — sotto soglia
    /// `k`, o per un corso senza argomenti — e non che sia zero.
    pub quota: Option<CohortShare>,
}

impl MeterQuery {
    fn persona(&self, identita: &SharedIdentity) -> Result<PersonId, ApiError> {
        match &self.person {
            None => Ok(identita.person().clone()),
            Some(raw) => ids::person(raw).ok_or(ApiError::Absent),
        }
    }

    /// L'istante di riferimento, o l'ora della richiesta.
    fn istante(&self) -> Millis {
        self.at.map(Millis).unwrap_or_else(Millis::now)
    }
}

impl ShareQuery {
    /// L'etichetta della classe, se è stata nominata.
    ///
    /// Tre esiti e sono tre fatti distinti: **assente** (nessuna classe
    /// nominata), **rifiutata** (una classe che non è un'etichetta) e il valore.
    /// L'assente non è un errore: nessuna classe nominata non è «la classe con
    /// quota zero», è nessuna classe, e la risposta è l'assenza del numero come
    /// sotto soglia. Il rifiuto è un'altra cosa — è una stringa che torna nel
    /// corpo e finisce nell'SQL, e nessuna delle due accetta caratteri di
    /// controllo o una lunghezza assurda.
    ///
    /// `ids.rs` controlla la forma degli id **del prodotto**; la classe è
    /// un'etichetta della scuola e non ne ha una, quindi il controllo è qui ed è
    /// dichiarato per non sembrare una seconda versione di quello.
    fn classe(&self) -> Result<Option<CohortId>, ApiError> {
        let cruda = self.cohort.trim();
        if cruda.is_empty() {
            return Ok(None);
        }
        let buona = cruda.len() <= 64
            && cruda
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
        buona
            .then(|| Some(CohortId(cruda.to_string())))
            .ok_or(ApiError::BadRequest {
                motivo: "la classe è un'etichetta della scuola: nome breve, senza spazi".into(),
            })
    }

    /// La finestra dei passaggi, con i due estremi risolti.
    ///
    /// Il default di `da` è `a` meno la finestra di freschezza, e non «l'inizio
    /// del corso»: una finestra che comincia prima del primo registro non
    /// aggiunge passaggi, e una finestra lunga è soltanto una finestra in cui un
    /// numero è più piccolo e meno leggibile. Quella di default è la più corta in
    /// cui una transizione è possibile per costruzione del criterio.
    ///
    /// # `da` dopo `a` è una `400`, e non una finestra raddrizzata
    ///
    /// Il motivo per cui questa rotta non può semplicemente ignorare `da` è che
    /// `from > to` **non è un'inversione**: `mastery_share` valuta il metro due
    /// volte, una a `from` e una a `to`, e con `from` nel futuro i due verdetti
    /// divergono precisamente quando lo studente è dimostrato al giorno `to` e
    /// non lo è più al giorno `from`. `transitions` diventerebbe un **rilevatore
    /// di perdita**, su un numero che la pagina chiama «passaggi da non
    /// dimostrato a dimostrato».
    ///
    /// Raddrizzare la finestra in silenzio è economico e non è onesto: il corpo
    /// riporterebbe `from` e `to`, quindi la correzione sarebbe visibile a chi
    /// già sospetta, ma il numero resterebbe la risposta a una domanda che
    /// nessuno ha fatto. Questo crate risponde alle domande dichiarate, e per un
    /// parametro che si contraddice da solo la risposta dichiarata è l'errore —
    /// la stessa forma che dà `routes::calendario` a un `orizzonte` che non è
    /// un intero non negativo: due parametri di tempo, due rotte, una regola.
    ///
    /// `da == a` invece passa: è una finestra legittima e vuota, e la sua
    /// risposta — nessun passaggio in una finestra senza durata — è vera.
    fn finestra(&self) -> Result<(Millis, Millis), ApiError> {
        let a = self.a.map(Millis).unwrap_or_else(Millis::now);
        let da = self
            .da
            .map(Millis)
            .unwrap_or_else(|| Millis(a.0.saturating_sub(MASTERY_FRESHNESS_WINDOW.0)));
        if da.0 > a.0 {
            return Err(ApiError::BadRequest {
                motivo: format!(
                    "`da` è {} e `a` è {}: una finestra che finisce prima di cominciare \
                     non è una finestra. Omettere `da` dà la finestra di default; un \
                     `da` dopo `a` è una domanda da correggere, non da interpretare",
                    da.0, a.0
                ),
            });
        }
        Ok((da, a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `Identity` serve solo qui: fuori dai test nessuno in questo file
    // costruisce un'identità dichiarata, e lasciare l'import in alto
    // produceva un avviso «unused» in ogni build non-test — cioè nella build
    // che produce il binario che si pubblica.
    use crate::identity::Identity;

    /// Un'identità dichiarata, per la parte della query che la usa.
    fn dichiarata(persona: PersonId) -> SharedIdentity {
        std::sync::Arc::new(Identity(persona))
    }

    #[test]
    fn la_persona_mancante_vale_l_identita_dichiarata() {
        let io = dichiarata(PersonId::fixture(7));
        assert_eq!(
            MeterQuery {
                person: None,
                at: None
            }
            .persona(&io)
            .expect("l'identità dichiarata"),
            PersonId::fixture(7),
            "uno studente che guarda il proprio metro non scrive il proprio id \
             in ogni URL"
        );
        assert_eq!(
            MeterQuery {
                // `persona_0009` non è una forma che `PersonId::fixture`
                // produca, e questo id entra lo stesso: `ids::person` controlla
                // la sicurezza della stringa e non il prefisso, che è una
                // convenzione di `fixture(n)` e non un invariante del tipo — il
                // campo di `string_id!` è `pub` e la CLI conia `PersonId` da
                // stringhe arbitrarie. Quindi `persona_0009` può nominare una
                // persona vera, con relazioni e con prove, e in quel caso la
                // rotta restituisce il suo metro vero. Il `404` di questa rotta
                // è per chi non è lo studente e non insegna, non per l'id: è
                // `identity.rs` a dichiarare la regola, cioè che la
                // dichiarazione si verifica solo nella forma e mai
                // nell'esistenza.
                person: Some("persona_0009".into()),
                at: None
            }
            .persona(&io)
            .expect("un id di persona ben formato"),
            PersonId("persona_0009".into()),
            "l'id entra come è scritto: nessun prefisso viene aggiunto o tolto"
        );
        // Una stringa che non è un id non è «la persona sbagliata»: è nessuna
        // persona, e la risposta è la stessa di un id che non è mai esistito.
        assert!(MeterQuery {
            person: Some("../altro".into()),
            at: None
        }
        .persona(&io)
        .is_err());
    }

    #[test]
    fn la_classe_e_un_etichetta_e_non_un_id() {
        assert_eq!(
            ShareQuery {
                cohort: " 2A ".into(),
                da: None,
                a: None
            }
            .classe()
            .expect("2A è un'etichetta"),
            Some(CohortId("2A".into())),
            "gli spazi attorno sono rumore e non informazione"
        );
        // Assente non è «la classe con quota zero»: è nessuna classe, e la
        // risposta è l'assenza del numero. Confonderle farebbe di una richiesta
        // incompleta un'affermazione su una classe che non è stata nominata.
        for assente in ["", "   "] {
            assert_eq!(
                ShareQuery {
                    cohort: assente.into(),
                    da: None,
                    a: None
                }
                .classe()
                .expect("una classe assente non è un errore"),
                None,
                "`{assente}` è una classe non nominata, non una classe rotta"
            );
        }
        for rotta in ["2 A", "classe;drop", "a".repeat(65).as_str()] {
            assert!(
                ShareQuery {
                    cohort: rotta.into(),
                    da: None,
                    a: None
                }
                .classe()
                .is_err(),
                "`{rotta}` è nominata e non è un'etichetta: la stringa torna \
                 nel corpo e finisce nell'SQL"
            );
        }
    }

    #[test]
    fn la_finestra_di_default_e_piu_corta_della_freschezza() {
        let a = 1_700_000_000_000i64;
        let (da, fine) = ShareQuery {
            cohort: "2A".into(),
            da: None,
            a: Some(a),
        }
        .finestra()
        .expect("una finestra con un solo estremo non è capovolta");
        assert_eq!(fine, Millis(a));
        assert_eq!(
            da.0,
            a - MASTERY_FRESHNESS_WINDOW.0,
            "il default non guarda indietro più di quanto il criterio consideri \
             fresco: una finestra più lunga non aggiunge passaggi"
        );
        assert_eq!(
            ShareQuery {
                cohort: "2A".into(),
                da: Some(0),
                a: Some(a)
            }
            .finestra()
            .expect("una finestra esplicita e dritta"),
            (Millis(0), Millis(a)),
            "la finestra esplicita non si tocca"
        );
        // `da == a` non è una finestra capovolta: è una finestra senza durata,
        // e «nessun passaggio in una finestra senza durata» è vero. Rifiutarla
        // sarebbe un controllo che scopre un caso che non è rotto.
        assert_eq!(
            ShareQuery {
                cohort: "2A".into(),
                da: Some(a),
                a: Some(a)
            }
            .finestra()
            .expect("una finestra di durata zero è una finestra"),
            (Millis(a), Millis(a)),
            "l'estremo coincidente non è un'estremo fuori posto"
        );
    }

    /// La finestra capovolta è l'unico caso che questa funzione rifiuta, ed è
    /// un rifiuto per costruzione: senza la guardia, `from > to` arriva a
    /// `mastery_share` e i due verdetti divergono — `transitions` conta chi al
    /// giorno `a` è dimostrato e al giorno `from` non lo è più. Il predicato
    /// diventerebbe un rilevatore di perdite su un numero che la pagina chiama
    /// «passaggi da non dimostrato a dimostrato».
    ///
    /// Il test è sensibile alla riga che lo rende verde: togliere la guardia e
    /// `expect_err` qui sotto riceverebbe un `Ok` e il test fallirebbe. Non è un
    /// test che passa «per costruzione» perché il caso è assurdo — `da` dopo `a`
    /// è una domanda che un parametro copiato a mano produce senza accorgersene,
    /// ed è esattamente il caso in cui il numero pubblicato era falso.
    #[test]
    fn la_finestra_capovolta_e_un_errore_e_non_una_finestra() {
        let a = 1_700_000_000_000i64;
        let rifiutata = ShareQuery {
            cohort: "2A".into(),
            da: Some(a + MASTERY_FRESHNESS_WINDOW.0),
            a: Some(a),
        }
        .finestra()
        .expect_err("una finestra che finisce prima di cominciare non è una finestra");
        assert!(
            matches!(rifiutata, ApiError::BadRequest { .. }),
            "il rifiuto è un `400` e non un'assenza: un'assenza qui sarebbe la \
             stessa risposta che sotto soglia, che è un'altra cosa"
        );
        let motivo = match rifiutata {
            ApiError::BadRequest { motivo } => motivo,
            altro => panic!("{altro:?}"),
        };
        // Il motivo porta i **due numeri**, non i due nomi dei parametri: «`da` è
        // maggiore di `a`» dice a chi chiama che la domanda è sbagliata ma non
        // gli dice quale dei due valori abbia digitato male.
        for valore in [a.to_string(), (a + MASTERY_FRESHNESS_WINDOW.0).to_string()] {
            assert!(
                motivo.contains(&valore),
                "il motivo non riporta `{valore}`, quindi chi chiama non sa quale \
                 dei due estremi abbia scritto: {motivo}"
            );
        }
        // Il caso limite della guardia: un solo millisecondo di inversione è
        // già un'inversione. Se la guardia fosse `<` invece di `<=`, questa riga
        // passerebbe e il numero tornerebbe a essere quello sbagliato.
        assert!(ShareQuery {
            cohort: "2A".into(),
            da: Some(a + 1),
            a: Some(a)
        }
        .finestra()
        .is_err());
    }

    #[test]
    fn l_istante_mancante_vale_l_ora_della_richiesta() {
        assert!(
            MeterQuery {
                person: None,
                at: None
            }
            .istante()
            .0
                > 1_700_000_000_000,
            "senza `at` il metro guarda adesso, non un istante scelto dal test"
        );
        assert_eq!(
            MeterQuery {
                person: None,
                at: Some(42)
            }
            .istante(),
            Millis(42),
            "con `at` il metro guarda l'ora che gli hai chiesto: il replay (D11) \
             comincia da qui"
        );
    }

    #[test]
    fn sotto_soglia_il_corpo_e_un_null_e_non_un_numero() {
        // La forma del corpo è la prova: sotto soglia non può esserci un conteggio
        // di persone, e la proprietà si vede dal JSON e non dal fatto che la
        // funzione sia stata chiamata.
        let corpo = serde_json::to_string(&ShareResponse {
            criterion: Criterion::OF_THE_CLAIM,
            quota: None,
        })
        .expect("corpo");
        assert!(corpo.contains("\"quota\":null"), "{corpo}");
        for aggrottato in ["\"students\"", "\"transitions\"", "\"quota\":0"] {
            assert!(
                !corpo.contains(aggrottato),
                "sotto soglia `{aggrottato}` non può essere nel corpo: {corpo}"
            );
        }
        assert!(
            corpo.contains("\"threshold\""),
            "il criterio resta, perché è una convenzione e non un conteggio: {corpo}"
        );
    }
}
