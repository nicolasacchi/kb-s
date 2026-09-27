//! L'anagrafe del materiale: **sei** elementi Dublin Core sul `<meta>`
//! dell'artifact (D14, idea 20·16 della ricerca).
//!
//! # Perché un modulo, e non un sistema di metadati
//!
//! Il corpus misura questa idea in **3 giorni, 0 €**, e la tiene in lista «prima
//! di tutto» per una ragione sola: *«Dublin Core come default. Costa quasi
//! nulla, non blocca niente, e rende recuperabile ciò che il docente ha scritto
//! tre anni fa»*. Un modulo di duecento righe in `kbs-doc` è il costo vero. Un
//! sistema di metadati — tabelle, vocabolari, un editor, un catalogo — è un altro
//! progetto, e l'idea non lo chiede.
//!
//! Il secondo punto del corpus è la regola che questo modulo applica più di ogni
//! altra, ed è scritta nel codice perché è quella che si dimentica:
//!
//! > «Sbagliata quando: nessuno rillegge i metadati: un campo che non consum
//! > nessuno è costo puro.»
//!
//! Da qui due conseguenze, ed entrambe sono test:
//!
//! 1. **il registro è chiuso**: sei elementi, non quindici ([`ELEMENTI`]). Un
//!    `dc.publisher` che qualcuno scrive non viene letto, e il sistema lo dice
//!    ([`AnagrafeWarning::ElementoNonRegistrato`]) invece di ignorarlo in
//!    silenzio: un campo che il sistema non legge e non segnala è un campo che
//!    il docente crede di aver compilato.
//! 2. **niente avvisi per ciò che manca**. L'assenza è uno stato
//!    ([`Origine::Assente`]), non un difetto: il caso normale è l'artifact senza
//!    un solo `dc.*`, e un avviso per campo mancante riempirebbe lo schermo del
//!    docente di cose che non ha chiesto. Gli avvisi sono per le **dichiarazioni
//!    sbagliate**, che sono l'unica cosa in cui il silenzio mente.
//!
//! # Perché sei elementi
//!
//! I quindici elementi DCMI sono un catalogo per biblioteche e musei. Qui si
//! tratta di **rendere recuperabile il materiale di un docente**, e quindi:
//!
//! | elemento | perché c'è | da dove viene |
//! |---|---|---|
//! | `dc.title` | è ciò che rende il materiale ritrovabile | dedotto dal `<title>`, che il validatore già esige ([`crate::validate::IssueCode::NoTitle`]) |
//! | `dc.creator` | fra tre anni non si sa chi può correggerlo | **solo dichiarato**: il file non sa chi ha scritto, e indovinare sarebbe inventare un fatto |
//! | `dc.date` | «tre anni fa» è una data, non un ricordo | **solo dichiarato** |
//! | `dc.language` | distingue il materiale del docente dalla sua traduzione | dedotto da `<html lang>`, che c'è su ogni artifact |
//! | `dc.identifier` | l'handle con cui si cita il materiale | dedotto da `kb-argument`, che è l'identità che il file ha **adesso**; dichiarato, è l'handle che sopravvive al suo spostamento |
//! | `dc.rights` | che cosa si può fare del materiale | **solo dichiarato**: il sistema non sceglie una licenza per il docente |
//!
//! Gli altri nove hanno un motivo di non esserci, e sono motivi diversi:
//!
//! * `description` — il riassunto è già il primo paragrafo del corpo, ed è già
//!   indicizzato. Una seconda copia è una seconda verità.
//! * `type` — esiste `kb-family`, un vocabolario **chiuso** di dodici famiglie di
//!   media; tradurlo in `dc:type` è una conversione senza consumatore.
//! * `format` — è sempre `text/html`. Un campo con un solo valore possibile è
//!   rumore, non metadato.
//! * `relation` — è il grafo dei prerequisiti (`data-prereq`), e il grafo è già
//!   un grafo. Il corpus dice che cinque suoi archi sono termini DCMI: sono già
//!   DCMI *per costruzione*, quindi la stringa non aggiungerebbe niente.
//! * `source` — per il materiale scritto a mano la fonte è il file; per il
//!   materiale generato c'è il model lock di D10, che è un'altra cosa.
//! * `publisher`, `contributor` — sono persone, e le persone in `kb-s` sono
//!   **relazioni** (D5: `author_of`, `ratified`, `teaches`), non attributi di un
//!   file.
//! * `subject`, `coverage` — nessun consumatore in `kb-s`: sono elementi che il
//!   corpus chiama costo puro, e si lasciano fuori per non dover dichiarare di
//!   non usarli.
//!
//! # Che cosa significa «manca», e perché non è la stessa cosa di `""`
//!
//! Ogni campo porta la sua **provenienza** ([`Origine`]): `Dichiarato`,
//! `Dedotto` o `Assente`. Il motivo è che un archivio che appiattisce le tre cose
//! in una stringa non sa più, quando lo si rilegge, se `dc.creator` manca perché
//! nessuno l'ha scritto o perché l'ha scritto e non lo sa. Un `<meta
//! name="dc.creator" content="">` è una **dichiarazione vuota**, e questa la
//! distingue: un campo dichiarato e vuoto è un errore di chi lo ha scritto (ed è
//! un avviso, [`AnagrafeWarning::CampoVuoto`]); un campo assente è la norma.
//!
//! Niente è dedotto fuori dal documento. In particolare il titolo **non** è
//! dedotto dal percorso: `03-schema-di-ripasso.html` non è il titolo «Schema di
//! ripasso compilato dallo studente», e un archivio che sostituisce una frase
//! con uno slug di sei parole non sta conservando il materiale, lo sta
//! riassumendo senza dirtelo.
//!
//! # Il round-trip, e dove l'export **non** è
//!
//! Il record deve tornare fuori. Qui torna in due modi, ed entrambi sono
//! verificati da test:
//!
//! * [`Anagrafe::to_metas`] e [`Anagrafe::to_front_matter`] sono gli inversi di
//!   [`crate::parser::parse`] e di [`crate::markdown::convert`]: ciò che il
//!   record legge da un artifact, lo riscrive nelle due forme in cui un docente
//!   l'ha scritto. La copia duratura è **il file** (D12: il corpus è una
//!   cartella di file e l'uscita è `rm -rf`), e quindi l'archivio che conta è
//!   l'artifact con i suoi `<meta>`, non una tabella che ne promette uno.
//! * il record viaggia dentro [`crate::validate::ArtifactReport`], cioè dentro
//!   il verdetto di ogni intake: chi giudica un documento vede anche la sua
//!   anagrafe, senza rileggerlo.
//!
//! **L'export a colonne fisse non è il posto giusto**, e la ragione va scritta
//! perché è la domanda che questo modulo farà: `kbs-verify::export` esporta il
//! **registro** — le righe, le teste di sessione, le prove, i limiti del
//! registro delle osservazioni (D6) — e le sue colonne sono un registro di
//! integrità, non un catalogo di documenti. Metterci `dc.creator` significherebbe
//! che una tabella con due intestazioni diverse contiene due cose che il
//! destinatario deve distinguere a occhio, in un file il cui contratto è
//! proprio non richiedere fiducia al sistema che l'ha scritto. Il record del
//! materiale ha un altro formato di uscita, ed è l'`rm -rf` di D12.
//!
//! # Che cosa questo modulo non fa
//!
//! Non scrive, non valida la sostanza di un diritto, non indovina un autore, non
//! sceglie una licenza, non legge `LOM`, e non tiene un archivio: tiene un
//! registro e dice quali dei suoi campi sono dichiarati.

use serde::{Deserialize, Serialize};

use crate::parser::ParsedArtifact;

/// Il prefisso dei `<meta>` che compongono l'anagrafe.
///
/// `dc.` e non `dcterms.` perché i sei elementi qui sono tutti del namespace
/// `elements/1.1`, e perché è il prefisso che l'HTML di altri sistemi porta
/// già: il parser abbassa i nomi dei meta, quindi `DC.title` e `dc.title` sono
/// lo stesso elemento, ed è il test
/// `il_prefisso_e_il_nome_del_meta_e_il_maiuscolo_non_apre_un_elemento_nuovo` a
/// fissarlo.
pub const PREFISSO: &str = "dc.";

/// Il `<meta>` che dichiara il percorso dell'argomento, da cui deriva
/// l'`ArgumentId` (`kbs_core::ArgumentId::from_rel_path`).
///
/// È la ragione per cui `dc.identifier` si deduce: **l'identità segue il file**,
/// e il file sa già qual è il suo percorso. Non si rilegge il disco per
/// scoprirlo, e il documento resta coerente con l'identità che il negozio gli
/// dà senza che nessuno debba scrivere due volte la stessa cosa.
pub const META_ARGOMENTO: &str = "kb-argument";

/// Da dove si deduce un elemento quando l'autore non lo dichiara.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Deduzione {
    /// Dal `<title>`: il campo che il validatore già esige.
    Titolo,
    /// Dall'attributo `lang` di `<html>`.
    Lingua,
    /// Da `kb-argument`, il percorso relativo dell'argomento.
    Percorso,
}

/// Un elemento dell'anagrafe, e se può essere dedotto.
///
/// `dedotto_da: None` significa che **l'assenza non si può colmare**: per
/// `dc.creator`, `dc.date` e `dc.rights` il sistema non ha niente da cui
/// dedurre, e soprattutto non deve prendersi la libertà di indovinare. È la
/// differenza fra un default che risparmia una digitata e un default che
/// inventa un fatto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elemento {
    /// Il nome del `<meta>`, che è anche il nome dell'elemento DCMI.
    pub nome: &'static str,
    pub dedotto_da: Option<Deduzione>,
}

/// I sei elementi dell'anagrafe, nell'ordine in cui [`read`] li produce.
///
/// L'ordine è parte del contratto, come per [`crate::contract::SECTIONS`]: un
/// record serializzato in un ordine diverso da quello dichiarato non è
/// confrontabile con un altro, e il confronto è l'unica cosa che si fa con un
/// archivio.
pub const ELEMENTI: [Elemento; 6] = [
    Elemento { nome: "dc.title", dedotto_da: Some(Deduzione::Titolo) },
    Elemento { nome: "dc.creator", dedotto_da: None },
    Elemento { nome: "dc.date", dedotto_da: None },
    Elemento { nome: "dc.language", dedotto_da: Some(Deduzione::Lingua) },
    Elemento { nome: "dc.identifier", dedotto_da: Some(Deduzione::Percorso) },
    Elemento { nome: "dc.rights", dedotto_da: None },
];

/// L'elemento del registro, se il nome è uno di quelli.
///
/// Il confronto è esatto e minuscolo: [`crate::parser::parse`] abbassa già i
/// nomi dei `<meta>`, quindi il caso `DC.title` è risolto a monte e non serve un
/// secondo tipo di nome qui dentro.
pub fn elemento(nome: &str) -> Option<&'static Elemento> {
    posizione(nome).map(|i| &ELEMENTI[i])
}

/// La posizione di un elemento nel registro, che è la sua posizione nel
/// record: la posizione **è** il nome, e i due non possono divergere.
pub fn posizione(nome: &str) -> Option<usize> {
    ELEMENTI.iter().position(|e| e.nome == nome)
}

/// Da dove viene il valore di un campo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origine {
    /// L'autore l'ha scritto nel `<meta>`.
    Dichiarato,
    /// Il sistema l'ha ricavato dal documento stesso. Il campo porta **da
    /// dove**: un valore dedotto e uno dichiarato si confondono facilmente, e
    /// quello che si deve poter dire a chi rilegge l'archivio è proprio quale
    /// delle due cose sia.
    Dedotto { da: Deduzione },
    /// Non c'è, e nessuno lo può sapere.
    Assente,
}

/// Un elemento dell'anagrafe, letto.
///
/// Il nome **non** sta qui dentro: sta in [`ELEMENTI`], e il campo si sa quale
/// è dalla sua posizione. È la stessa disciplina di
/// [`crate::contract::SECTIONS`], e ha due vantaggi che non sono di stile: il
/// nome non può divergere dal registro che lo elenca, e il record non
/// duplica sei stringhe a ogni validazione. Le due cose che renderebbero un
/// campo `&'static str` qui dentro — il nome e la provenienza — sono l'una
/// l'altra, perché [`Origine`] è già un enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campo {
    /// `None` quando l'elemento non c'è, **oppure** quando è dichiarato e
    /// vuoto: i due casi si distinguono in [`Campo::dichiarato`].
    pub valore: Option<String>,
    pub origine: Origine,
}

impl Campo {
    /// `true` se l'autore ha scritto il `<meta>`, sia con un valore sia vuoto.
    pub fn dichiarato(&self) -> bool {
        matches!(self.origine, Origine::Dichiarato)
    }

    /// `true` se c'è un valore da mostrare.
    pub fn presente(&self) -> bool {
        self.valore.is_some()
    }

    /// `true` se il campo è stato dichiarato e non dice niente: la situazione
    /// che un archivio non deve confondere con «non c'è».
    pub fn vuoto_dichiarato(&self) -> bool {
        self.dichiarato() && self.valore.is_none()
    }
}

/// L'anagrafe di un artifact: sei campi, ognuno con la sua provenienza.
///
/// Il tipo è `Serialize` perché è il modo in cui il record **esce** dal
/// sistema senza passare da una tabella: un `ArtifactReport` serializzato è
/// l'anagrafe di un documento, e non serve un registro per conservarla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anagrafe {
    /// I campi, **nell'ordine di [`ELEMENTI`]**: la posizione è il nome.
    campi: [Campo; ELEMENTI.len()],
}

impl Anagrafe {
    /// Il campo di un elemento, se l'elemento è nel registro.
    pub fn get(&self, nome: &str) -> Option<&Campo> {
        posizione(nome).map(|i| &self.campi[i])
    }

    /// Il valore di un elemento, senza la provenienza.
    pub fn value(&self, nome: &str) -> Option<&str> {
        self.get(nome).and_then(|c| c.valore.as_deref())
    }

    /// Tutti i campi, nell'ordine del registro.
    pub fn campi(&self) -> &[Campo; ELEMENTI.len()] {
        &self.campi
    }

    /// I campi che l'autore ha scritto e che dicono qualcosa, col loro nome.
    pub fn dichiarati(&self) -> impl Iterator<Item = (&'static str, &Campo)> {
        ELEMENTI
            .iter()
            .zip(&self.campi)
            .filter(|(_, c)| c.dichiarato() && c.presente())
            .map(|(e, c)| (e.nome, c))
    }

    /// Il numero di elementi che ci sono — dichiarati o dedotti.
    pub fn presenti(&self) -> usize {
        self.campi.iter().filter(|c| c.presente()).count()
    }

    /// Gli stessi campi, come `<meta>`: l'inverso di [`read`].
    ///
    /// Scrive **solo i dichiarati**, e questa è la parte che conta: un
    /// elemento dedotto non viene riscritto nel documento, perché riscrivere un
    /// default è il modo in cui un default smette di essere riconoscibile come
    /// tale alla lettura successiva. Il round-trip che [`read`] garantisce è
    /// quindi sul **record risolto**, non sui byte, ed è quello che serve a chi
    /// rilegge l'archivio.
    pub fn to_metas(&self) -> Vec<(&'static str, &str)> {
        self.dichiarati()
            .filter_map(|(nome, c)| c.valore.as_deref().map(|v| (nome, v)))
            .collect()
    }

    /// Gli stessi campi, come front-matter `chiave: valore`: la forma in cui il
    /// docente li scrive, e l'inverso di quello che [`crate::markdown::convert`]
    /// ne fa.
    ///
    /// Le chiavi sono già i nomi dei `<meta>` (`dc.title`), quindi il
    /// front-matter e l'HTML dicono la stessa cosa senza una tabella di
    /// traduzione in mezzo.
    pub fn to_front_matter(&self) -> String {
        self.dichiarati()
            .filter_map(|(nome, c)| c.valore.as_deref().map(|v| format!("{nome}: {v}\n")))
            .collect()
    }
}

/// Legge l'anagrafe da un artifact già passato per [`crate::parser::parse`].
///
/// Non fallisce mai e non deduce nulla che il documento non dica già: la
/// precedenza è **dichiarato, poi dedotto, poi assente**, e in questa direzione
/// soltanto, perché un default che vince una dichiarazione è un default che
/// cancella.
pub fn read(a: &ParsedArtifact) -> Anagrafe {
    let campi = std::array::from_fn(|i| {
        let e = &ELEMENTI[i];
        let dichiarato = a.meta.get(e.nome).map(|v| v.trim().to_string());
        if let Some(v) = dichiarato.as_ref().filter(|v| !v.is_empty()) {
            return Campo { valore: Some(v.clone()), origine: Origine::Dichiarato };
        }
        // Dichiarato e vuoto: `valore` è `None`, ma l'origine resta
        // `Dichiarato`. È l'unico modo in cui «c'è e non dice niente» e
        // «non c'è» restano distinguibili senza un quarto stato.
        if dichiarato.is_some() {
            return Campo { valore: None, origine: Origine::Dichiarato };
        }
        match e.dedotto_da.and_then(|da| deduci(a, da).map(|v| (da, v))) {
            Some((da, valore)) => Campo { valore: Some(valore), origine: Origine::Dedotto { da } },
            None => Campo { valore: None, origine: Origine::Assente },
        }
    });
    Anagrafe { campi }
}

/// Il valore dedotto, se il documento lo dichiara davvero.
fn deduci(a: &ParsedArtifact, da: Deduzione) -> Option<String> {
    let grezzo = match da {
        Deduzione::Titolo => a.title.clone(),
        Deduzione::Lingua => a.lang.clone(),
        Deduzione::Percorso => a.meta.get(META_ARGOMENTO).cloned(),
    }?;
    let v = grezzo.trim().to_string();
    if v.is_empty() { None } else { Some(v) }
}

/// Perché un elemento dichiarato non torna.
///
/// Solo per le **dichiarazioni**: un elemento assente non è mai un avviso
/// (vedi il modulo). Nessuno di questi blocca la pubblicazione — l'anagrafe
/// «non blocca niente», e la prova è
/// [`crate::validate::inspect`] che li pubblica tutti come `Warning`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "avviso", rename_all = "kebab-case")]
pub enum AnagrafeWarning {
    /// Il `<meta>` c'è ma non dice niente.
    #[error("`{elemento}` è dichiarato e vuoto: un campo dichiarato e vuoto non è un campo assente, e in un archivio la differenza è fra «non c'è» e «c'è e non lo so»")]
    CampoVuoto { elemento: String },
    /// `dc.title` dice una cosa e il `<title>` del documento un'altra.
    #[error("`dc.title` dice «{dichiarato}» e il <title> del documento dice «{documento}»: sono due titoli, e fra tre anni nessuno saprà quale dei due è quello dell'archivio")]
    TitoloInConflitto { dichiarato: String, documento: String },
    /// `dc.language` contraddice l'attributo `lang`, che è ciò che legge il
    /// lettore per decidere se mettere o no un dizionario accanto al testo.
    #[error("`dc.language` dice «{dichiarato}» e il documento dichiara lang=\"{documento}\": la lingua che il lettore usa e quella dell'archivio non sono la stessa")]
    LinguaInConflitto { dichiarato: String, documento: String },
    #[error("`dc.language` vale «{valore}» e non è un tag di lingua: due o tre lettere, poi sottotag con `-` — «Italian», «i t», «it/» non sono lingue. `ita` invece è un tag valido, e se contraddice il documento lo dice l'altro avviso")]
    LinguaMalformata { valore: String },
    #[error("`dc.date` vale «{valore}» e non è una data ISO 8601 (AAAA-MM-GG, o AAAA-MM-GGTHH:MM): «tre anni fa» si ordina solo con una data che un programma può confrontare")]
    DataNonIso8601 { valore: String },
    /// Un `<meta name="dc.…">` che `kb-s` non legge.
    #[error("`{nome}` è un elemento Dublin Core che questa anagrafe non legge: l'anagrafe ne ha sei e sono quelli elencati in ELEMENTI. Un campo che nessuno rilegge è costo puro")]
    ElementoNonRegistrato { nome: String },
}

/// Gli avvisi dell'anagrafe di un artifact.
///
/// Si passa il documento e non il record perché due dei controlli sono
/// **confronti con il documento**: un `dc.title` che contraddice il `<title>` è
/// una contraddizione, e una contraddizione si vede solo guardando le due
/// metà. Lo stesso vale per `lang`.
pub fn ispeziona(a: &ParsedArtifact) -> Vec<AnagrafeWarning> {
    let mut out = Vec::new();

    // Un `dc.*` fuori dal registro è un campo che il docente ha compilato e che
    // nessuno rillegge: proprio il caso in cui il silenzio mente.
    for nome in a.meta.keys().filter(|k| k.starts_with(PREFISSO)) {
        if elemento(nome.as_str()).is_none() {
            out.push(AnagrafeWarning::ElementoNonRegistrato { nome: nome.clone() });
        }
    }

    let record = read(a);
    for (e, c) in ELEMENTI.iter().zip(&record.campi) {
        if c.vuoto_dichiarato() {
            out.push(AnagrafeWarning::CampoVuoto { elemento: e.nome.to_string() });
            continue;
        }
        // I confronti valgono solo su ciò che l'autore ha dichiarato: dedurre il
        // titolo e poi scoprire che è in conflitto con il titolo sarebbe un
        // avviso che il sistema dà a se stesso.
        if !c.dichiarato() {
            continue;
        }
        let valore = c.valore.as_deref().unwrap_or_default();
        match e.nome {
            "dc.title" => {
                if let Some(t) = a.title.as_deref() {
                    if t != valore {
                        out.push(AnagrafeWarning::TitoloInConflitto {
                            dichiarato: valore.to_string(),
                            documento: t.to_string(),
                        });
                    }
                }
            }
            "dc.language" => {
                if !è_tag_di_lingua(valore) {
                    out.push(AnagrafeWarning::LinguaMalformata { valore: valore.to_string() });
                }
                if let Some(l) = a.lang.as_deref() {
                    // Si confronta il **sottotag primario**, non il tag
                    // intero: `it-CH` e `it` sono la stessa lingua con due
                    // precisazioni, e dichiarare la regione non è una
                    // contraddizione. Un avviso che si firesi su `it-CH` sarebbe
                    // un avviso che premia la pigrizia del docente.
                    if lingua_primaria(l) != lingua_primaria(valore) {
                        out.push(AnagrafeWarning::LinguaInConflitto {
                            dichiarato: valore.to_string(),
                            documento: l.to_string(),
                        });
                    }
                }
            }
            "dc.date" => {
                if !è_iso8601(valore) {
                    out.push(AnagrafeWarning::DataNonIso8601 { valore: valore.to_string() });
                }
            }
            _ => {}
        }
    }
    out
}

/// Un tag di lingua: due o tre lettere, poi sottotag con `-`.
///
/// Non è il linguaggio BCP 47 e non pretende di esserlo: è la forma che
/// `<html lang>` accetta davvero, più stretta di quella che un validatore
/// generico accetterebbe. `ita` è un tag legittimo — è il codice ISO 639-2
/// dell'italiano — mentre `Italian` e `i t` non sono una lingua: sono due modi
/// di scrivere che l'archivio non potrà ordinare.
fn è_tag_di_lingua(s: &str) -> bool {
    let mut parti = s.split('-');
    let base = parti.next().unwrap_or_default();
    if !(2..=3).contains(&base.chars().count()) || !base.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    parti.all(|p| {
        !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()) && p.len() <= 8
    })
}

/// Il sottotag primario di un tag di lingua, minuscolizzato: la parte che
/// dice *quale* lingua è, separata dalla regione e dalla variante.
fn lingua_primaria(s: &str) -> String {
    s.split('-').next().unwrap_or_default().to_ascii_lowercase()
}

/// Una data ISO 8601: `AAAA-MM-GG`, o `AAAA-MM-GGTHH:MM` con secondi, `Z` o
/// scostamento opzionali.
///
/// La validazione è **di forma, non di calendario**, e questa è una scelta:
/// `2026-02-31` passa, `2026-13-01` no. Un controllo che legge l'orologio o il
/// calendario per validare un campo diventerebbe una proprietà dell'esecuzione e
/// non del documento, e il banco — che rende confrontabili due esecuzioni
/// perché tutto il resto è funzione pura della tabella — non potrebbe più
/// confrontarle. La data resta un fatto dichiarato, e un fatto dichiarato che il
/// sistema non sa collocare è un avviso, non un blocco.
fn è_iso8601(s: &str) -> bool {
    let (data, tempo) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let mut p = data.split('-');
    let anno = p.next().unwrap_or_default();
    let mese = p.next().unwrap_or_default();
    let giorno = p.next().unwrap_or_default();
    if p.next().is_some() {
        return false;
    }
    if anno.len() != 4 || !anno.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if !è_cifre(mese, 2) || !è_cifre(giorno, 2) {
        return false;
    }
    // Il **range** dei due campi, che non è il calendario: un mese che non
    // esiste è un refuso, e un refuso che passa diventa un fatto in un
    // archivio. Il giorno 31 di febbraio passa, e va bene: sapere se il
    // febbraio del 2026 aveva 28 giorni è un calendario, e un validatore che
    // lo carica dentro un campo diventa una dipendenza.
    let (m, g) = (mese.parse::<u32>().unwrap_or(0), giorno.parse::<u32>().unwrap_or(0));
    if !(1..=12).contains(&m) || !(1..=31).contains(&g) {
        return false;
    }
    let Some(t) = tempo else { return true };
    // Il fuso (`Z` o `±HH:MM`) si stacca; quello che resta è un'ora locale.
    let (ora, fuso) = match t.find(['+', '-']) {
        Some(i) if i > 0 => (&t[..i], Some(&t[i..])),
        _ => (t.strip_suffix('Z').unwrap_or(t), None),
    };
    if let Some(f) = fuso {
        let s = f.strip_prefix('+').or_else(|| f.strip_prefix('-')).unwrap_or_default();
        let mut sp = s.split(':');
        let h = sp.next().unwrap_or_default();
        let m = sp.next().unwrap_or_default();
        if !è_cifre(h, 2) || !è_cifre(m, 2) || sp.next().is_some() {
            return false;
        }
    }
    let mut tp = ora.split(':');
    let h = tp.next().unwrap_or_default();
    let m = tp.next().unwrap_or_default();
    if !è_cifre(h, 2) || !è_cifre(m, 2) {
        return false;
    }
    match tp.next() {
        None => true,
        Some(s) => è_cifre(s, 2) && tp.next().is_none(),
    }
}

fn è_cifre(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn documento(head: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html lang="it"><head><title>Titolo</title>{head}</head>
            <body><h1 id="titolo">Titolo</h1><p>Testo.</p></body></html>"#
        )
    }

    #[test]
    fn il_documento_senza_nessun_meta_ha_un_archivio_che_non_mente_e_non_che_lavora() {
        let src = documento(r#"<meta name="kb-argument" content="letture/01-x.html">"#);
        let a = parser::parse(&src);
        let r = read(&a);

        // I tre dedotti arrivano dal documento, con la provenienza che dice da
        // dove: nessuno di loro è stato dichiarato.
        assert_eq!(r.value("dc.title"), Some("Titolo"));
        assert_eq!(r.value("dc.language"), Some("it"));
        assert_eq!(r.value("dc.identifier"), Some("letture/01-x.html"));
        for nome in ["dc.title", "dc.language", "dc.identifier"] {
            let c = r.get(nome).unwrap();
            assert!(!c.dichiarato(), "{nome} non era dichiarato");
            assert!(matches!(c.origine, Origine::Dedotto { .. }), "{nome}: {:?}", c.origine);
        }

        // E i tre che il sistema non può sapere restano assenti. Non sono
        // inventati, e non costano un avviso.
        for nome in ["dc.creator", "dc.date", "dc.rights"] {
            let c = r.get(nome).unwrap();
            assert_eq!(c.origine, Origine::Assente, "{nome}");
            assert!(!c.dichiarato() && !c.presente() && !c.vuoto_dichiarato());
        }
        assert_eq!(r.presenti(), 3);
        assert!(
            ispeziona(&a).is_empty(),
            "il caso normale non produce avvisi: {:?}",
            ispeziona(&a)
        );
    }

    #[test]
    fn un_valore_dichiarato_e_vuoto_non_e_un_valore_assente() {
        let a = parser::parse(&documento(r#"<meta name="dc.creator" content="   ">"#));
        let r = read(&a);
        let c = r.get("dc.creator").unwrap();
        assert!(c.dichiarato(), "il <meta> c'è: dichiararlo è un atto");
        assert!(!c.presente(), "ma non dice niente");
        assert!(c.vuoto_dichiarato());
        assert_ne!(c.origine, Origine::Assente);
        assert_eq!(
            ispeziona(&a),
            vec![AnagrafeWarning::CampoVuoto { elemento: "dc.creator".to_string() }]
        );
    }

    #[test]
    fn un_elemento_dichiarato_vince_sulla_deduzione() {
        let src = documento(concat!(
            r#"<meta name="kb-argument" content="letture/01-x.html">"#,
            r#"<meta name="dc.title" content="Titolo archivio">"#,
            r#"<meta name="dc.identifier" content="urn:kb-s:prove/01-x">"#,
        ));
        let r = read(&parser::parse(&src));
        assert_eq!(r.value("dc.title"), Some("Titolo archivio"));
        assert_eq!(r.value("dc.identifier"), Some("urn:kb-s:prove/01-x"));
        assert!(matches!(r.get("dc.title").unwrap().origine, Origine::Dichiarato));
    }

    #[test]
    fn il_prefisso_e_il_nome_del_meta_e_il_maiuscolo_non_apre_un_elemento_nuovo() {
        let a = parser::parse(&documento(r#"<meta name="DC.Creator" content="Anna Rossi">"#));
        let r = read(&a);
        assert_eq!(r.value("dc.creator"), Some("Anna Rossi"));
        assert!(ispeziona(&a).is_empty(), "{:?}", ispeziona(&a));
    }

    #[test]
    fn un_elemento_dublin_core_fuori_dal_registro_viene_detto_e_non_ignorato() {
        let a = parser::parse(&documento(r#"<meta name="dc.publisher" content="Ministero">"#));
        assert!(ispeziona(&a).contains(&AnagrafeWarning::ElementoNonRegistrato {
            nome: "dc.publisher".to_string()
        }));
        // E resta fuori dal record: il registro è chiuso, e un elemento che
        // nessuno legge non entra per sympathia.
        let r = read(&a);
        assert!(r.get("dc.publisher").is_none());
        assert_eq!(r.campi().len(), ELEMENTI.len());
    }

    #[test]
    fn un_titolo_dichiarato_che_contraddice_il_documento_e_un_avviso() {
        let a = parser::parse(&documento(r#"<meta name="dc.title" content="Altro titolo">"#));
        assert!(ispeziona(&a).contains(&AnagrafeWarning::TitoloInConflitto {
            dichiarato: "Altro titolo".to_string(),
            documento: "Titolo".to_string(),
        }));

        // Il titolo uguale non è un conflitto: il caso normale è silenzioso.
        let b = parser::parse(&documento(r#"<meta name="dc.title" content="Titolo">"#));
        assert!(ispeziona(&b).is_empty());
    }

    #[test]
    fn una_lingua_che_non_e_un_tag_e_un_avviso_e_una_che_contradice_il_documento_anche() {
        // `Italian` non è un tag: sette lettere. E contraddice anche il
        // documento, che è un secondo avviso e non una ripetizione — i due
        // controlli dicono due cose diverse.
        let a = parser::parse(&documento(r#"<meta name="dc.language" content="Italian">"#));
        let w = ispeziona(&a);
        assert!(w.contains(&AnagrafeWarning::LinguaMalformata { valore: "Italian".to_string() }));
        assert!(w.contains(&AnagrafeWarning::LinguaInConflitto {
            dichiarato: "Italian".to_string(),
            documento: "it".to_string(),
        }));

        // `ita` invece è un tag ben formato (ISO 639-2) e non viene accusato di
        // nulla: lo si accusa solo di non essere la lingua del documento.
        let b = parser::parse(&documento(r#"<meta name="dc.language" content="ita">"#));
        assert_eq!(
            ispeziona(&b),
            vec![AnagrafeWarning::LinguaInConflitto {
                dichiarato: "ita".to_string(),
                documento: "it".to_string(),
            }]
        );

        // Coerente col documento: silenzio. E i sottotag sono ammessi, perché
        // `it-CH` è la lingua che l'HTML accetta davvero.
        let c = parser::parse(&documento(r#"<meta name="dc.language" content="it-CH">"#));
        assert!(ispeziona(&c).is_empty());
        let d = parser::parse(&documento(r#"<meta name="dc.language" content="it">"#));
        assert!(ispeziona(&d).is_empty());
    }

    #[test]
    fn una_data_iso_8601_e_accettata_e_una_data_di_parole_no() {
        for buona in [
            "2026-09-26",
            "2026-09-26T14:30",
            "2026-09-26T14:30:15",
            "2026-09-26T14:30:15Z",
            "2026-09-26T14:30+02:00",
        ] {
            assert!(è_iso8601(buona), "{buona} è una data");
            let a = parser::parse(&documento(&format!(
                r#"<meta name="dc.date" content="{buona}">"#
            )));
            assert!(ispeziona(&a).is_empty(), "{buona}: {:?}", ispeziona(&a));
        }
        for cattiva in [
            "26/09/2026",
            "settembre 2026",
            "2026-9-26",
            "2026-09-26 14:30",
            "2026-13-01",
            "2026-09-26T14",
            "",
        ] {
            assert!(!è_iso8601(cattiva), "{cattiva} non è una data");
        }
    }

    #[test]
    fn il_record_torna_da_se_stesso_attraverso_i_meta() {
        let src = documento(concat!(
            r#"<meta name="kb-argument" content="letture/01-x.html">"#,
            r#"<meta name="dc.creator" content="Anna Rossi">"#,
            r#"<meta name="dc.date" content="2026-09-26">"#,
            r#"<meta name="dc.rights" content="Uso didattico in classe, CC BY-SA 4.0">"#,
        ));
        let prima = read(&parser::parse(&src));
        let meta = prima
            .to_metas()
            .iter()
            .map(|(n, v)| format!(r#"<meta name="{n}" content="{v}">"#))
            .collect::<String>();
        // Il round-trip passa dal consumatore vero — il parser — e non da una
        // ricerca di stringhe (D16). Il documento conserva il suo
        // `kb-argument`: `dc.identifier` è dedotto da lì, e togliere
        // l'ambiente non sarebbe un round-trip del record, sarebbe un documento
        // diverso.
        let dopo = read(&parser::parse(&documento(&format!(
            r#"<meta name="kb-argument" content="letture/01-x.html">{meta}"#
        ))));
        assert_eq!(prima, dopo);

        // I dedotti non vengono riscritti: riscrivere un default è il modo in
        // cui un default smette di essere riconoscibile.
        assert_eq!(prima.to_metas().len(), 3);
        assert!(!meta.contains("dc.title"));
    }

    #[test]
    fn il_record_torna_anche_dal_front_matter() {
        let src = documento(r#"<meta name="dc.creator" content="Anna Rossi">"#);
        let prima = read(&parser::parse(&src));
        let fm = prima.to_front_matter();
        assert_eq!(fm, "dc.creator: Anna Rossi\n");

        // La stessa chiave, passata dal convertitore di markdown, torna allo
        // stesso elemento: è la forma in cui il docente la scrive.
        let md = format!("---\ntitle: Titolo\n{fm}---\n\ntesto\n");
        let html = crate::markdown::convert(&md).html;
        let dopo = read(&parser::parse(&html));
        assert_eq!(dopo.value("dc.creator"), Some("Anna Rossi"));
        assert_eq!(dopo.get("dc.creator").unwrap().origine, Origine::Dichiarato);
        // E il titolo resta dedotto, non dichiarato: `title:` è il titolo del
        // documento, `dc.title:` è il titolo dell'archivio.
        assert!(matches!(dopo.get("dc.title").unwrap().origine, Origine::Dedotto { .. }));
    }

    #[test]
    fn ogni_deduzione_del_registro_e_davvero_collegata() {
        // Se domani aggiungi un elemento con `dedotto_da`, questo test ti dice
        // che la deduzione è cablata; senza, un `Some(..)` ci mette `None` e
        // l'elemento diventa silenziosamente sempre assente.
        for e in ELEMENTI {
            let mut head = String::new();
            let mut atteso: Option<Deduzione> = None;
            match e.dedotto_da {
                Some(Deduzione::Titolo) => atteso = Some(Deduzione::Titolo),
                Some(Deduzione::Lingua) => atteso = Some(Deduzione::Lingua),
                Some(Deduzione::Percorso) => {
                    head.push_str(r#"<meta name="kb-argument" content="a/b.html">"#);
                    atteso = Some(Deduzione::Percorso);
                }
                None => {}
            }
            let r = read(&parser::parse(&documento(&head)));
            let c = r.get(e.nome).unwrap();
            match atteso {
                Some(da) => {
                    assert!(c.presente(), "{} non deduce più", e.nome);
                    assert_eq!(c.origine, Origine::Dedotto { da }, "{}", e.nome);
                }
                None => assert_eq!(c.origine, Origine::Assente, "{} deduce da qualcosa", e.nome),
            }
        }
    }

    #[test]
    fn il_registro_e_di_sei_e_chiude() {
        assert_eq!(ELEMENTI.len(), 6);
        let nomi: Vec<&str> = ELEMENTI.iter().map(|e| e.nome).collect();
        assert_eq!(
            nomi,
            ["dc.title", "dc.creator", "dc.date", "dc.language", "dc.identifier", "dc.rights"]
        );
        // Un nome senza il prefisso non è un elemento dell'anagrafe: `dc.` è
        // ciò che distingue questi sei dai meta `kb-…`, che sono del progetto.
        for e in ELEMENTI {
            assert!(e.nome.starts_with(PREFISSO), "{}", e.nome);
        }
    }
}
