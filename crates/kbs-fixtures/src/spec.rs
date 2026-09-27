//! Il tipo di una voce del banco: tutto ciò che la tabella dei 40 item sa e
//! che il corpus HTML non dice da sé.
//!
//! La tabella è la **sorgente unica**: i file in `corpus/` sono ciò che
//! [`crate::render`] produce da questa tabella, e un test verifica che i file
//! committati siano identici a quelli prodotti. Una fiastra modificata a mano
//! senza aggiornare la tabella fa fallire il banco.

use crate::contract::Defect;
use crate::families::Family;
use kbs_core::{ClaimStatus, GraderKind, PublicationState};

/// Il corso è il perimetro di condivisione (D5). Nel banco ce ne sono due, e
/// la catena di prerequisiti non le attraversa: un argomento di un corso non
/// può dipendere da un argomento dell'altro.
pub const CORSO_MATEMATICA: u32 = 1;
pub const CORSO_INFORMATICA: u32 = 2;

/// Il docente che ha scritto o ratificato l'item.
pub const DOCENTE: u32 = 1;
/// La seconda docente, per non avere un solo autore in tutto il banco.
pub const DOCENTE_2: u32 = 2;

/// Come è nato l'argomento (D10). `Generato` porta il **model lock**: senza
/// prompt hash e corpus hash non c'è un registro, c'è un diario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginSpec {
    /// Scritto a mano.
    Human { by: u32 },
    /// Generato con un modello **esterno al prodotto** (D3), poi ratificato.
    Generated { by: u32, model: &'static str, prompt_hash: &'static str, generator: &'static str },
    /// Derivato da un altro argomento.
    Derived { from: &'static str },
}

impl OriginSpec {
    pub fn label(&self) -> &'static str {
        match self {
            OriginSpec::Human { .. } => "human",
            OriginSpec::Generated { .. } => "generated",
            OriginSpec::Derived { .. } => "derived",
        }
    }

    pub fn autore(&self) -> u32 {
        match self {
            OriginSpec::Human { by } | OriginSpec::Generated { by, .. } => *by,
            OriginSpec::Derived { from: _ } => DOCENTE,
        }
    }
}

/// Lo stato della ratifica, che è ciò che rende un item citabile o no (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatificaSpec {
    /// Non ratificato: leggibile, non citabile.
    Nessuna,
    /// Ratificato sul contratto corrente: citabile.
    Fresca,
    /// Ratificato su un contratto **precedente**: la ratifica non vale più.
    /// È il caso che `Invariant::StaleRatification` deve riconoscere su dati
    /// veri e non solo in un test unitario.
    Stale,
}

/// Una claim dichiarata, con lo span che la sostiene.
#[derive(Debug, Clone, Copy)]
pub struct ClaimSpec {
    pub id: &'static str,
    pub testo: &'static str,
    /// L'ancora dello span. `None` ⇒ claim non citabile: nessuno span la
    /// sostiene, e un puntatore senza testo non è verificabile.
    pub ancora: Option<&'static str>,
    /// Il testo **effettivo** dello span, che è ciò che il lettore verifica.
    pub testo_span: Option<&'static str>,
    pub stato: ClaimStatusKind,
    /// Emessa dal docente, o dal contenuto dell'argomento (D3: un modello non
    /// è un emittente).
    pub emittente: EmittenteSpec,
}

/// Lo stato della claim, in una forma che si può dichiarare in una tabella
/// `const` (il tipo di `kbs_core` non è `Copy` per via di `Retracted`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatusKind {
    Supported,
    Contradicted,
    Unciteable,
    Retracted(&'static str),
}

impl ClaimStatusKind {
    pub fn to_core(self) -> ClaimStatus {
        match self {
            ClaimStatusKind::Supported => ClaimStatus::Supported,
            ClaimStatusKind::Contradicted => ClaimStatus::Contradicted,
            ClaimStatusKind::Unciteable => ClaimStatus::Unciteable,
            ClaimStatusKind::Retracted(r) => ClaimStatus::Retracted { reason: r.to_string() },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmittenteSpec {
    /// Emessa da una persona.
    Docente { by: u32 },
    /// Emessa dal contenuto di un argomento ratificato.
    Contenuto,
    /// Emessa a partire da un'osservazione sul lavoro dello studente.
    DaLavoro { osservazione: &'static str },
}

impl EmittenteSpec {
    pub fn label(&self) -> &'static str {
        match self {
            EmittenteSpec::Docente { .. } => "teacher",
            EmittenteSpec::Contenuto => "content",
            EmittenteSpec::DaLavoro { .. } => "from-work",
        }
    }
}

/// Il verificatore deterministico di un esercizio. Nessuna delle forme chiede
/// un modello: D3 chiude la catena di grading e questa è la prova.
#[derive(Debug, Clone, Copy)]
pub enum CheckerSpec {
    Numeric { tolerance: f64 },
    Set { elementi: &'static [&'static str] },
    MultipleChoice { corretta: u8, opzioni: &'static [&'static str] },
    Equivalence { normale: &'static str },
}

impl CheckerSpec {
    pub fn label(&self) -> &'static str {
        match self {
            CheckerSpec::Numeric { .. } => "numeric",
            CheckerSpec::Set { .. } => "set",
            CheckerSpec::MultipleChoice { .. } => "multiple-choice",
            CheckerSpec::Equivalence { .. } => "equivalence",
        }
    }
}

/// Un'istanza dichiarata: il seed e i parametri determinano la risposta (D11),
/// e la risposta dichiarata è ciò che la pipeline deve riprodurre.
#[derive(Debug, Clone, Copy)]
pub struct InstanceSpec {
    pub seed: &'static str,
    /// Parametri del generatore, interi. Bastano per gli esercizi del banco e
    /// rendono la risposta attesa calcolabile a occhio.
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub attesa: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ExerciseSpec {
    pub id: &'static str,
    /// Due istanze della stessa famiglia hanno lo stesso ragionamento: è il
    /// motivo per cui copiare non funziona (D8).
    pub famiglia: &'static str,
    pub generatore: &'static str,
    pub testo: &'static str,
    pub checker: CheckerSpec,
    /// Il banco ne dichiara due per l'esercizo parametrizzato, una per gli
    /// altri: due sole non basterebbero a dimostrare che il seed cambia la
    /// risposta.
    pub istanze: &'static [InstanceSpec],
    /// Chi ha deciso il voto quando il checker risponde. Nessun modello.
    pub giudicato_da: Option<GraderKind>,
}

/// Un nodo o un arco della scena 3D. D15.1: ogni oggetto dichiarato
/// corrisponde a una `Claim`, o non entra nell'indice condiviso.
#[derive(Debug, Clone, Copy)]
pub struct SceneNode {
    pub id: &'static str,
    pub etichetta: &'static str,
    /// La claim che sostiene il nodo. Se manca, l'oggetto non entra.
    pub claim: &'static str,
    /// Se lo studente può correggere questa relazione, il guadagno
    /// pedagogico è registrabile (D15.1.3).
    pub correggibile: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SceneEdge {
    pub id: &'static str,
    pub da: &'static str,
    pub a: &'static str,
    /// L'etichetta della relazione: è qui che la precisione degli archi crolla
    /// (F1 0,14–0,28 sulle entità deboli).
    pub relazione: &'static str,
    pub claim: &'static str,
    pub correggibile: bool,
}

/// La scena 3D, in forma di dati. D15.1.4: **il rendering è un effetto, il
/// contenuto è il dato**. Perciò la scena sta in un manifest JSON separato dal
/// codice che la disegna: si può re-renderizzare, cambiare camera e luce, e il
/// manifest non cambia. Se fosse nel codice, un aggiornamento del renderer
/// cancellerebbe il materiale dello studente.
#[derive(Debug, Clone, Copy)]
pub struct SceneSpec {
    /// Percorso del manifest, relativo alla radice del corpus.
    pub manifest: &'static str,
    pub titolo: &'static str,
    pub nodi: &'static [SceneNode],
    pub archi: &'static [SceneEdge],
}

/// Una voce del banco.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Percorso relativo dentro `corpus/`. Da questo deriva l'`ArgumentId`:
    /// l'identità segue il file, non i byte (`ArgumentId::from_rel_path`).
    pub rel: &'static str,
    pub famiglia: Family,
    pub titolo: &'static str,
    pub riassunto: &'static str,
    /// Uno o due paragrafi di corpo, che è ciò che diventa indice.
    pub paragrafi: &'static [&'static str],
    pub stato: PublicationState,
    pub corso: u32,
    /// Percorsi relativi dei prerequisiti. Un percorso che punta a sé stesso è
    /// il ciclo minimo, ed è la fiastra che chiude il ciclo.
    pub prerequisiti: &'static [&'static str],
    pub origine: OriginSpec,
    pub ratifica: RatificaSpec,
    /// Le otto sezioni del contratto, senza difetti. Il difetto è applicato
    /// dopo, da [`Spec::contract`].
    pub guardian: &'static str,
    pub obiettivi: &'static str,
    pub scala: &'static str,
    pub equivoci: &'static str,
    pub esempio: &'static str,
    pub verifica: &'static str,
    pub limite: &'static str,
    pub difetto: Defect,
    pub claims: &'static [ClaimSpec],
    pub esercizi: &'static [ExerciseSpec],
    pub scena: Option<SceneSpec>,
    /// Il riferimento al runtime esterno, se l'item ne ha uno. D15.
    pub riferimento_esterno: Option<&'static str>,
    /// Se l'item carica three.js dal percorso servito dal binario. D15.
    pub carica_three_locale: bool,
}

impl Spec {
    /// Il contratto, con il difetto applicato.
    pub fn contract(&self) -> crate::contract::Contract {
        let prerequisiti = self.prerequisiti_come_testo();
        crate::contract::Contract::new([
            self.guardian,
            prerequisiti.as_str(),
            self.obiettivi,
            self.scala,
            self.equivoci,
            self.esempio,
            self.verifica,
            self.limite,
        ])
        .con_difetto(self.difetto)
    }

    /// La sezione `PREREQUISITI` non è scritta a mano: elencare gli argomenti
    /// che si devono già sapere è un fatto della tabella, e scriverlo due
    /// volte è il modo in cui una tabella e un file cominciano a contraddirsi.
    pub fn prerequisiti_come_testo(&self) -> String {
        if self.prerequisiti.is_empty() {
            return "Nessun prerequisito.".to_string();
        }
        self.prerequisiti
            .iter()
            .map(|p| {
                let nome = p.rsplit('/').next().unwrap_or(p).replace(".html", "").replace('-', " ");
                nome
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// `true` se l'item deve essere **rifiutato** dalla validazione.
    pub fn deve_essere_rifiutato(&self) -> bool {
        self.difetto.expected_code().is_some()
    }
}

/// Una riga del registro che la pipeline deve mostrare, per le claim che
/// portano un errore. D6: «un errore si registra, non si cancella».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimAttesa {
    pub claim: &'static str,
    pub rel: &'static str,
    pub stato: ClaimStatusKind,
    /// `true` se la riga deve esistere **nonostante** lo stato: la traccia
    /// dell'errore è il punto, e senza di essa il registro mente.
    pub riga_deve_esistere: bool,
    /// `true` se l'output deve sopprimerla.
    pub output_deve_sopprimerla: bool,
}
