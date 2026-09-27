//! Chi sta chiedendo — e perché questa non è un'autenticazione.
//!
//! # La dichiarazione, in una riga
//!
//! `kb` oggi non autentica nessuno: c'è un solo livello di fiducia e ogni
//! identità che arriva è un co-operatore a pieni diressi
//! (`kb/CLAUDE.md:406-410`), il mount del corpus *è* l'ACL
//! (`kb/crates/kb-server/src/router.rs:112`). `kbs-s` non eredita quel modello
//! di fiducia, ma **non può ancora sostituirlo**: in questo crate non esiste
//! una sessione, e costruirne una sarebbe un'altra decisione, non un dettaglio
//! di implementazione.
//!
//! Quindi [`Identity`] è un **id di persona che arriva da qualche parte**:
//!
//! ```text
//! X-Kbs-Person: person_0001        (chiamate API, fetch, XHR)
//! ?person=person_0001              ( navigazione in iframe, EventSource )
//! ```
//!
//! Le due forme hanno lo stesso valore e la stessa fragilità, e sono due forme
//! per la stessa ragione: `EventSource` e la navigazione di un `<iframe>` non
//! possono allegare un header custom. Non è un aggiramento, è l'unico modo che
//! il browser lascia.
//!
//! # Perché la query string non è un incidente
//!
//! Una dichiarazione in una query string finisce in un log di accesso e in un
//! `Referer`. Se fosse una credenziale, sarebbe un difetto. **Non lo è**, e la
//! ragione va scritta perché il giorno in cui qualcuno aggiunge
//! l'autenticazione il ragionamento va rifatto da capo: qui non c'è nulla da
//! proteggere, c'è un nome che il cliente si è attribuito. Ciò che *va*
//! protetto è l'oggetto della richiesta, e quello passa dal predicato D5.
//!
//! # Che cosa segue da «è una dichiarazione»
//!
//! * **Nessun endpoint la rafforza.** Registrare una persona, o scrivere
//!   qualcosa a suo nome, non rende la dichiarazione più vera. Il predicato
//!   D5 guarda le relazioni registrate dal docente, non il fatto che qualcuno
//!   si sia dichiarato tale.
//! * **Nessuna sessione, nessun cookie, nessun `SameSite`.** Sono meccanismi di
//!   autenticazione e questo crate non ne ha.
//! * **Il confine di sicurezza è il deployment.** Se questo server è
//!   raggiungibile da fuori, chiunque può dichiarare chi è. Il predicato
//!   continua a valere — protegge il materiale, non la persona — ma la persona
//!   è dichiarata. Va messo dietro qualcosa che autentica, o su una rete che
//!   non ha estranei.
//!
//! # L'errore di identità non è un canale
//!
//! Un'assenza di identità (`401`) e un id malformato (`400`) sono
//! decisioni prese **prima** di guardare la risorsa, e valgono uguali per ogni
//! percorso. Non dicono nulla su ciò che esiste: sono la risposta alla forma
//! della richiesta, non al suo oggetto.

use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use kbs_core::PersonId;

use crate::error::ApiError;
use crate::ids;
use crate::routes::AppState;

/// L'header con cui la dichiarazione arriva sulle chiamate API.
pub const IDENTITY_HEADER: &str = "x-kbs-person";

/// Il parametro con cui arriva quando il browser non può allegare un header:
/// navigazione di `<iframe src>` e `EventSource`.
pub const IDENTITY_QUERY: &str = "person";

/// Una persona che **dichiara** di essere qualcuno.
///
/// Il nome del tipo è la documentazione: `Claimed` non c'è, ma
/// `Identity` non promette niente di più di quello che è, e il doc qui dice
/// perché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity(pub PersonId);

impl Identity {
    /// La persona dichiarata.
    pub fn person(&self) -> &PersonId {
        &self.0
    }
}

/// Una dichiarazione appoggiata a un `Arc`, per non copiarla a ogni handler.
///
/// L'`Arc` serve a una cosa sola: tenere l'identità viva finché il canale SSE
/// è aperto, senza che ogni `poll` del flusso ne richieda una copia.
pub type SharedIdentity = Arc<Identity>;

impl FromRequestParts<AppState> for SharedIdentity {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // L'header ha la precedenza sulla query string: è il canale che il
        // chiamante controlla e che non finisce in un log. La query è il
        // ripiego per ciò che il browser non sa fare, non una seconda
        // opportunità per chi sa.
        if let Some(raw) = parts
            .headers
            .get(IDENTITY_HEADER)
            .and_then(|v| v.to_str().ok())
        {
            return declared(raw);
        }
        if let Some(raw) = query_param(parts.uri.query(), IDENTITY_QUERY) {
            return declared(&raw);
        }
        Err(ApiError::IdentityMissing)
    }
}

/// La dichiarazione è arrivata: si verifica solo la **forma**, mai l'esistenza.
///
/// Una persona che non è nel database non è un errore: è una persona senza
/// relazioni, che non vede niente — la stessa risposta che avrebbe chi non
/// fosse mai esistito. Verificare l'esistenza qui sarebbe una domanda in più
/// con una risposta che nessuno deve poter osservare.
fn declared(raw: &str) -> Result<SharedIdentity, ApiError> {
    let person = ids::person(raw).ok_or(ApiError::IdentityMalformed)?;
    Ok(Arc::new(Identity(person)))
}

/// Un parametro della query string, decodificato.
///
/// Il parser è qui, e non `form_urlencoded`, per una ragione: la query string
/// di una navigazione la scrive un `<iframe src>` costruito a mano, e
/// sbagliare l'escape in quel caso produce una dichiarazione diversa da quella
/// che si credeva. Un parser che fa `%2F` → `/` e `+` → spazio è
/// abbastanza; il charset dell'id è più stretto, quindi il resto non può
/// passare.
fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == name {
                return Some(percent_decode(value));
            }
        }
    }
    None
}

/// `application/x-www-form-urlencoded` nella sua forma minima.
///
/// Lo si dichiara invece di prenderselo da una libreria perché è *tutto* ciò
/// che serve e niente di più: `%XX` e `+`. Un valore che non decodifica è
/// restituito com'è, e sarà [`ids::person`] a rifiutarlo.
pub(crate) fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_dichiarazione_malformata_non_e_un_assente() {
        // I due errori sono distinti e lo devono restare: uno è «non hai detto
        // chi sei», l'altro è «hai detto qualcosa che non è un id». Sono
        // entrambi anteriori alla risorsa, quindi nessuno dei due è un canale.
        assert!(matches!(declared(""), Err(ApiError::IdentityMalformed)));
        assert!(declared("person_0001").is_ok());
    }

    #[test]
    fn una_dichiarazione_inesistente_e_una_persona_senza_relazioni() {
        // Non c'è un ramo che verifichi l'esistenza, e il test lo blocca: se
        // un giorno qualcuno aggiunge `store.upsert_person` qui, l'errore
        // `NotFound` che ne deriverebbe direbbe a chi chiede quali id esistono.
        let persona = declared("person_9999").expect("dichiarazione");
        assert_eq!(persona.person().as_str(), "person_9999");
    }

    #[test]
    fn la_query_string_si_decodifica() {
        assert_eq!(
            query_param(Some("a=1&person=person_0001&b=2"), IDENTITY_QUERY).as_deref(),
            Some("person_0001")
        );
        assert_eq!(
            query_param(Some("person=person%5F0001"), IDENTITY_QUERY).as_deref(),
            Some("person_0001")
        );
        assert_eq!(query_param(Some("a=1"), IDENTITY_QUERY), None);
        assert_eq!(query_param(None, IDENTITY_QUERY), None);
    }

    #[test]
    fn una_percent_incompleta_non_scappa_dal_decodificatore() {
        // `person=person_0001%ZZ` non deve diventare `person_0001%ZZ` *interpretato*:
        // deve restare una stringa che il charset rifiuta, non una che passa.
        let grezza = query_param(Some("person=person_0001%"), IDENTITY_QUERY).expect("presente");
        assert!(ids::person(&grezza).is_none(), "{grezza} non deve essere un id");
    }
}
