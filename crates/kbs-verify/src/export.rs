//! L'export a colonne fisse (D12).
//!
//! «L'uscita è `rm -rf`»: il registro deve poter uscire dal sistema in un
//! formato che chi riceve non deve fidarsi del sistema per interpretare. Qui
//! il formato è una tabella a **colonne fisse**, con intestazione esplicita e
//! senza campi che possano contenere il separatore senza dichiararlo: la fuga
//! è esplicita (`\|`, `\\`, `\n`, `\r`) e il parser la ripercorre.
//!
//! L'export porta quattro cose, e le quattro servono a chi non si fida del
//! server:
//!
//! 1. **le righe**, col loro JSON canonico e la loro foglia — senza le righe il
//!    file porta la parte facile da falsificare e non quella da ricontare, e un
//!    export che non si può rileggere è un certificato che non certifica;
//! 2. la testa di ogni sessione e le prove di ogni segmento;
//! 3. le voci del testimone, senza le quali le prove sono belle e non servono;
//! 4. **i tre limiti**, riga per riga.
//!
//! Il punto 4 è il più importante e il meno ovvio. Un export che contiene una
//! catena e non i suoi limiti è un certificato che promette troppo: chi lo
//! riceve non ha modo di sapere che cosa non gli è stato garantito. Qui i
//! limiti viaggiano con i dati, e il parser **rifiuta** un export i cui limiti
//! non sono identici a quelli del codice: se qualcuno li ha addolciti, il file
//! non si apre.
//!
//! Il punto 1 è quello che è stato aggiunto per ultimo, e che pure mancava: un
//! formato di uscita che non si può reimportare non è un'uscita, è un
//! headlights-off. [`ChainExport::observations`] e [`ChainExport::witness`] sono
//! il viaggio di ritorno, e chi riceve il file fa con essi esattamente ciò che
//! avrebbe fatto se fosse stato dentro il sistema — che è tutto il punto.
//!
//! # `unaided` e `n_hints` viaggiano in `row_json`, e non hanno una colonna
//!
//! Sono i due campi che rendono misurabile la claim del prodotto — *la quota di
//! argomenti che passano da non dimostrato a dimostrato* — e sono gli ultimi due
//! campi entrati in [`kbs_core::Observation`]. Portano in `row_json` come
//! [`Evidence`] e `judged_by`, e **non** guadagnano una colonna fissa. Il motivo
//! è uno solo, ed è che la decima colonna **è** la riga: ne porta il JSON
//! canonico, e [`leaf_of`] è l'hash di esattamente quel testo. Una colonna
//! `unaided` metterebte lo stesso fatto in due posti del file, e un file con due
//! copie di un fatto è un file che può dire due cose diverse.
//!
//! Il caso è già scritto in questo file e vale per questa decisione:
//! `at_millis` non viene duplicato nella nona colonna di una riga `observation`
//! «perché la riga lo porta già, e duplicarlo qui aprirebbe la porta a due date
//! che non coincidono». `unaided` è la stessa frase.
//!
//! La disciplina del modulo — dichiarata sulle colonne del testimone, in
//! `ExportRow::Witness`: *una colonna inventata è peggio di una colonna vuota* —
//! qui è soddisfatta dal payload e non da una colonna. Il test che la regola
//! chiede, «la colonna è popolabile da ogni payload accetato?», ha una risposta
//! per `row_json` è sì e verificabile: **`unaided` è sempre presente nel JSON,
//! dichiarato esplicitamente anche quando non è registrato.** `null` e «assente»
//! sono due cose diverse nella forma canonica — la stessa proprietà che
//! [`crate::canonical`] dichiara e che un test verifica — e quindi una riga con
//! aiuto ignoto è **falsificabile**, non ambigua: un file in cui `unaided` è
//! sparito dalla colonna non si apre, e l'errore è
//! [`ExportError::RowTextMismatch`], non una lettura comoda.
//!
//! Il costo di questa scelta è uno e va detto: chi riceve l'export legge
//! `unaided` nel JSON, non in una colonna che si può indicizzare a mano. Chi
//! vuole la colonna deve aprire la decima, ed è una riga di `serde_json`.

use crate::canonical::{canonical_of, canonicalize_str, leaf_of, CanonicalError};
use crate::chain::{Chain, ConsistencyProof, SegmentPlan, VerifyError};
use crate::hash::{Hash, HashParseError};
use crate::limits::{LimitId, Limits};
use crate::session::SessionId;
use crate::witness::{Witness, WitnessEntry, WitnessError};
use kbs_core::{Millis, Observation};
use std::fmt;

/// Le colonne della tabella del registro, **prima versione**: nove, in
/// quest'ordine e in nessun altro.
///
/// Il valore non cambia. Una scuola può tenere in mano un file scritto da una
/// versione precedente di questo crate, e quel file ha questa intestazione:
/// cambiarne l'ordine renderebbe illeggibili tutti i file già in
/// circolazione, e l'unica cosa che ci si guadagnerebbe è l'ordine.
pub const CHAIN_COLUMNS: [&str; 9] = [
    "kind", "session", "rows", "seq_from", "seq_to", "index", "head", "proof", "at_millis",
];

/// Le colonne del registro **versione corrente**: le nove di prima, nello
/// stesso ordine, più `row_json` in coda.
///
/// La crescita è **additiva e in coda**, e questa è la forma che un formato a
/// colonne fisse deve avere per poter invecchiare: [`CHAIN_COLUMNS`] resta
/// leggibile, [`ChainExport::from_text`] accetta l'una e l'altra intestazione,
/// e un file di nove colonne non porta con sé righe — semplicemente non ne
/// porta, ed è un export di prove. Un file di dieci le porta, e chi lo riceve
/// può ricontare; chi riceve un file di nove non può, e non finge di poterlo.
pub const CHAIN_COLUMNS_WITH_ROWS: [&str; 10] = [
    "kind", "session", "rows", "seq_from", "seq_to", "index", "head", "proof", "at_millis",
    "row_json",
];

/// Le colonne della tabella dei limiti.
pub const LIMIT_COLUMNS: [&str; 3] = ["limit", "statement", "mitigation"];

/// Una sessione da esportare: le righe e il piano con cui la catena viene
/// costruita.
///
/// Il piano serve perché le prove di coerenza sono **posizioni nella testa**:
/// esportare le prove di una catena costruita con un piano diverso produrrebbe
/// un file in cui le prove non tornano, e un file che non torna è peggio di
/// un file assente.
#[derive(Debug, Clone, Copy)]
pub struct SessionExport<'a> {
    pub session: &'a SessionId,
    pub rows: &'a [Observation],
    pub plan: SegmentPlan,
}

/// Una riga dell'export.
///
/// `Eq` qui non c'è, e non è una mancanza: [`Observation`] non lo dichiara, e
/// un enum che contiene un tipo senza `Eq` non può averlo. La riga che conta
/// per la catena è `leaf`, non `row`: `row` è il testo, e un testo si
/// confronta.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportRow {
    /// La testa di una sessione, con l'intervallo di `seq` che copre. Le `seq`
    /// sono quelle delle righe, non la loro posizione: un registro che
    /// comincia da 1 dichiara `1..=n`, e dichiarare `0..=n-1` sarebbe una bugia
    /// in una tabella che qualcuno deve poter usare per ricontare.
    Head {
        session: String,
        rows: usize,
        from: u64,
        to: u64,
        head: Hash,
    },
    /// Un segmento, con la prova che sta in quella posizione della testa.
    Segment {
        session: String,
        index: usize,
        from: u64,
        to: u64,
        rows: usize,
        root: Hash,
        proof: String,
    },
    /// La testa delle voci del testimone.
    WitnessHead { entries: usize, head: Hash },
    /// Una voce del testimone. Le colonne `seq_from` e `seq_to` restano
    /// vuote: il testimone sa **quante** righe aveva la sessione, non come
    /// erano numerate, e una colonna inventata è peggio di una colonna
    /// vuota.
    Witness {
        session: String,
        rows: usize,
        head: Hash,
        at_millis: i64,
    },
    /// Una riga del registro, per intero: il **JSON canonico** dell'osservazione
    /// più la **foglia** che la catena ne calcola.
    ///
    /// Il payload è il testo, non il tipo: è ciò che va sulla colonna, ed è
    /// la stessa stringa che [`leaf_of`] impegna. Tenerlo come testo evita di
    /// doverlo ricanonicalizzare — e quindi ricalcolare — al momento di
    /// scrivere il file, e rende impossibile che la riga scritta e la sua
    /// foglia vengano da due forme canoniche diverse.
    ///
    /// La foglia è ciò che rende la riga falsificabile da sola: chi riceve il
    /// file la ricalcola da `row_json` e vede se combacia, senza credere a
    /// nessuna dichiarazione.
    Observation {
        session: String,
        seq: u64,
        leaf: Hash,
        row: String,
    },
}

/// L'export di un insieme di sessioni, con il testimone se c'è.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainExport {
    rows: Vec<ExportRow>,
    limits: Limits,
}

impl ChainExport {
    /// Raccoglie le sessioni e il testimone nelle colonne fisse.
    ///
    /// Le righe di ogni sessione viaggiano con la sua testa: senza di esse il
    /// file porta la parte facile da falsificare e non quella da ricontare, e
    /// un export che non si può rileggere è un certificato che non certifica.
    /// La catena viene costruita qui, dalle stesse righe: la testa dichiarata
    /// e le prove non possono venire da una catena e le righe da un'altra.
    pub fn of(
        sessions: &[SessionExport<'_>],
        witness: Option<&Witness>,
    ) -> Result<ChainExport, VerifyError> {
        let mut rows = Vec::new();
        for session in sessions {
            let chain = Chain::build(session.session, session.rows, session.plan)?;
            rows.push(ExportRow::Head {
                session: chain.session().to_string(),
                rows: chain.len(),
                from: chain.first_seq()?,
                to: chain.last_seq()?,
                head: chain.head(),
            });
            for proof in chain.consistency_proofs()? {
                rows.push(segment_row(&proof));
            }
            for row in session.rows {
                rows.push(ExportRow::Observation {
                    session: chain.session().to_string(),
                    seq: row.seq.0,
                    leaf: leaf_of(row)?,
                    row: canonical_of(row)?,
                });
            }
        }
        if let Some(w) = witness {
            if let Some(head) = w.head() {
                rows.push(ExportRow::WitnessHead {
                    entries: w.len(),
                    head,
                });
            }
            for e in w.entries() {
                rows.push(witness_row(e));
            }
        }
        Ok(ChainExport {
            rows,
            limits: Limits::ALL,
        })
    }

    pub fn rows(&self) -> &[ExportRow] {
        &self.rows
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Le righe che il file porta, nell'ordine in cui il file le porta,
    /// raggruppate per sessione.
    ///
    /// Ogni riga viene **ricontata**: il `row_json` ricanonicalizzato deve
    /// dare esattamente il testo della colonna, e la sua foglia deve essere
    /// quella dichiarata. Un file che ha cambiato una riga senza rifarne la
    /// foglia non si apre — ed è la falsificazione più economica da tentare,
    /// perché il testo è leggibile e l'hash no.
    ///
    /// Il confronto è con il **testo**, e questa è la parte che non è
    /// ovvia: `canonicalize_str` dice soltanto che il documento è in forma
    /// canonica, e a un documento in forma canonica si può togliere una chiave
    /// o aggiungerne una senza che la forma cambi. Le due manomissioni non si
    /// vedono dal valore tipizzato — un `Option` che manca è un `None`, e una
    /// chiave che [`Observation`] non conosce viene ignorata — quindi la foglia
    /// ricalcolata sul tipo resta quella dichiarata. Il documento della colonna
    /// e quello che l'osservazione produce devono essere **lo stesso testo**, e
    /// quando non lo sono la colonna porta un fatto che nessun hash copre:
    /// l'errore è [`ExportError::RowTextMismatch`].
    ///
    /// L'ordine **del file** è conservato, anche quando non è quello giusto:
    /// riordinare qui nasconderebbe a chi verifica un file con le righe
    /// scambiate, e lo scambio è esattamente ciò che la catena deve rendere
    /// visibile. A riordinare è [`Chain::build`], che rifiuta una `seq` che
    /// torna indietro o salta.
    ///
    /// `Vec` vuota per un file che non porta righe — cioè un file di
    /// [`CHAIN_COLUMNS`], la prima versione — e non un errore: quel file è un
    /// export di prove, e chi lo riceve deve poter dirlo senza indovinarlo da
    /// un fallimento. Su quel file [`Self::witness`] funziona lo stesso, che
    /// è la parte che un export di prove porta e non perde.
    pub fn observations(&self) -> Result<Vec<(SessionId, Vec<Observation>)>, ExportError> {
        let mut out: Vec<(SessionId, Vec<Observation>)> = Vec::new();
        for (at, entry) in self.rows.iter().enumerate() {
            let ExportRow::Observation {
                session,
                seq,
                leaf,
                row,
            } = entry
            else {
                continue;
            };
            let canon = canonicalize_str(row)?;
            if canon != *row {
                return Err(ExportError::RowNotCanonical { line: at });
            }
            let parsed: Observation = serde_json::from_str(row).map_err(|e| ExportError::BadRow {
                line: at,
                reason: e.to_string(),
            })?;
            // Il confronto che chiude il caso, e che non è la ricanonicalizzazione
            // del testo: è la ricanonicalizzazione **del valore**. Le due cose
            // coincidono solo se la colonna è il testo che questo valore
            // produce, ed è il testo che la foglia impegna.
            if canonical_of(&parsed)? != *row {
                return Err(ExportError::RowTextMismatch { line: at });
            }
            if leaf_of(&parsed)? != *leaf {
                return Err(ExportError::RowLeafMismatch {
                    line: at,
                    declared: *leaf,
                    recomputed: leaf_of(&parsed)?,
                });
            }
            if parsed.seq.0 != *seq {
                return Err(ExportError::RowSeqMismatch {
                    line: at,
                    declared: *seq,
                    in_row: parsed.seq.0,
                });
            }
            match out.iter_mut().find(|(s, _)| s.as_str() == session) {
                Some((_, rows)) => rows.push(parsed),
                None => out.push((SessionId::new(session.clone()), vec![parsed])),
            }
        }
        Ok(out)
    }

    /// Il testimone che il file porta, ricostruito voce per voce.
    ///
    /// `None` quando il file non ne dichiara nessuno, che è un caso da dire e
    /// non un caso da tacere: senza testimone le righe verificano come
    /// coerenti e **non ancorate**, che è il primo limite.
    ///
    /// La testa dichiarata viene confrontata con quella ricalcolata dalle
    /// voci: un export che dichiara una testa e porta voci che non la
    /// ricostruiscono non è un testimone «illeggibile», è un testimone che
    /// mente, e va detto come tale.
    pub fn witness(&self) -> Result<Option<Witness>, WitnessError> {
        let entries: Vec<WitnessEntry> = self
            .rows
            .iter()
            .filter_map(|row| match row {
                ExportRow::Witness {
                    session,
                    rows,
                    head,
                    at_millis,
                } => Some(WitnessEntry {
                    session: SessionId::new(session.clone()),
                    rows: *rows,
                    head: *head,
                    at: Millis(*at_millis),
                }),
                _ => None,
            })
            .collect();
        let witness = Witness::from_entries(entries)?;
        if let Some(ExportRow::WitnessHead { entries, head }) = self
            .rows
            .iter()
            .find(|r| matches!(r, ExportRow::WitnessHead { .. }))
        {
            if witness.len() != *entries || witness.head() != Some(*head) {
                return Err(WitnessError::Broken {
                    recomputed: witness.head(),
                    declared: Some(*head),
                });
            }
        }
        Ok(Some(witness))
    }

    /// Il testo dell'export: due tabelle, intestazione in entrambe.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&CHAIN_COLUMNS_WITH_ROWS.join("|"));
        out.push('\n');
        for row in &self.rows {
            out.push_str(&render_row(row));
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&LIMIT_COLUMNS.join("|"));
        out.push('\n');
        for limit in self.limits.iter() {
            out.push_str(&escape(limit.id.as_str()));
            out.push('|');
            out.push_str(&escape(limit.statement));
            out.push('|');
            out.push_str(&escape(limit.mitigation.unwrap_or("")));
            out.push('\n');
        }
        out
    }
    /// Rilegge l'export. Le colonne sono fisse: una riga con un numero diverso
    /// di colonne non è un export, è un file qualsiasi.
    ///
    /// Accetta **entrambe** le intestazioni, [`CHAIN_COLUMNS_WITH_ROWS`] e
    /// [`CHAIN_COLUMNS`]. Un file di nove colonne è stato scritto da una
    /// versione precedente e si apre come era: un formato a colonne fisse che
    /// si rifiuta di rileggere i propri file più vecchi non è un formato
    /// durevole, è un formato che cambia senza dirlo. Un file di nove colonne
    /// non porta righe, e [`Self::observations`] su quel file restituisce un
    /// `Vec` vuota — non un errore, e non un file che finge di portarle.
    pub fn from_text(text: &str) -> Result<ChainExport, ExportError> {
        if text.is_empty() {
            return Err(ExportError::Empty);
        }
        let mut lines = text.split('\n').enumerate();
        let mut rows = Vec::new();

        let (n, header) = lines.next().ok_or(ExportError::Empty)?;
        let width = if header == CHAIN_COLUMNS_WITH_ROWS.join("|") {
            CHAIN_COLUMNS_WITH_ROWS.len()
        } else if header == CHAIN_COLUMNS.join("|") {
            CHAIN_COLUMNS.len()
        } else {
            return Err(ExportError::BadHeader {
                line: n,
                expected: CHAIN_COLUMNS_WITH_ROWS.join("|"),
                found: header.to_owned(),
            });
        };

        let mut separator = n;
        for (n, line) in lines.by_ref() {
            if line.is_empty() {
                separator = n;
                break;
            }
            rows.push(parse_row(n, line, width)?);
        }

        let (n, header) = lines
            .next()
            .ok_or(ExportError::MissingLimitsTable { after: separator })?;
        expect_header(n, header, &LIMIT_COLUMNS.join("|"))?;

        for expected in self_limits() {
            let (n, line) = lines
                .next()
                .ok_or(ExportError::LimitRowMissing { limit: expected.id })?;
            let cols = columns(n, line, LIMIT_COLUMNS.len())?;
            if cols[0] != expected.id.as_str() {
                return Err(ExportError::LimitRowOutOfOrder {
                    line: n,
                    expected: expected.id.as_str(),
                    found: cols[0].clone(),
                });
            }
            if cols[1] != expected.statement || cols[2] != expected.mitigation.unwrap_or("") {
                return Err(ExportError::LimitsAltered { limit: expected.id });
            }
        }
        if let Some((n, line)) = lines.next() {
            if !line.is_empty() {
                return Err(ExportError::TrailingContent { line: n });
            }
        }
        Ok(ChainExport {
            rows,
            limits: Limits::ALL,
        })
    }
}

fn self_limits() -> [crate::limits::Limit; 3] {
    Limits::ALL.all()
}

fn segment_row(proof: &ConsistencyProof) -> ExportRow {
    ExportRow::Segment {
        session: proof.session.to_string(),
        index: proof.index,
        from: proof.from,
        to: proof.to,
        rows: proof.rows,
        root: proof.segment_root,
        proof: proof.to_text(),
    }
}

fn witness_row(e: &WitnessEntry) -> ExportRow {
    ExportRow::Witness {
        session: e.session.to_string(),
        rows: e.rows,
        head: e.head,
        at_millis: e.at.0,
    }
}

fn render_row(row: &ExportRow) -> String {
    let empty = String::new();
    let mut cells: Vec<String> = match row {
        ExportRow::Head {
            session,
            rows,
            from,
            to,
            head,
        } => vec![
            "head".into(),
            session.clone(),
            rows.to_string(),
            from.to_string(),
            to.to_string(),
            empty.clone(),
            head.to_hex(),
            empty.clone(),
            empty.clone(),
        ],
        ExportRow::Segment {
            session,
            index,
            from,
            to,
            rows,
            root,
            proof,
        } => vec![
            "segment".into(),
            session.clone(),
            rows.to_string(),
            from.to_string(),
            to.to_string(),
            index.to_string(),
            root.to_hex(),
            proof.clone(),
            empty.clone(),
        ],
        ExportRow::WitnessHead { entries, head } => vec![
            "witness-head".into(),
            empty.clone(),
            entries.to_string(),
            empty.clone(),
            empty.clone(),
            empty.clone(),
            head.to_hex(),
            empty.clone(),
            empty.clone(),
        ],
        ExportRow::Witness {
            session,
            rows,
            head,
            at_millis,
        } => vec![
            "witness".into(),
            session.clone(),
            rows.to_string(),
            empty.clone(),
            empty.clone(),
            empty.clone(),
            head.to_hex(),
            empty.clone(),
            at_millis.to_string(),
        ],
        ExportRow::Observation { session, seq, leaf, .. } => vec![
            "observation".into(),
            session.clone(),
            // Una riga è una riga: `rows` vale 1 e `seq_from`/`seq_to`
            // coincidono, perché dichiarare un intervallo su una riga sola
            // sarebbe una frase che non significa niente.
            "1".into(),
            seq.to_string(),
            seq.to_string(),
            empty.clone(),
            leaf.to_hex(),
            empty.clone(),
            // `at_millis` sta nella nona colonna, che per questa riga è il
            // timestamp del JSON: la riga lo porta già, e duplicarlo qui
            // aprirebbe la porta a due date che non coincidono.
            empty.clone(),
        ],
    };
    // La decima colonna è in coda e vale solo per `observation`: per i quattro
    // tipi della prima versione resta vuota, così le loro nove celle sono
    // esattamente quelle di prima, e un file vecchio resta un file vecchio.
    cells.push(match row {
        ExportRow::Observation { row, .. } => row.clone(),
        _ => empty.clone(),
    });
    for c in cells.iter_mut() {
        *c = escape(c);
    }
    cells.join("|")
}

/// `width` è il numero di colonne che l'intestazione del file dichiara: nove
/// per la prima versione, dieci per quella corrente. Una riga `observation` in
/// un file di nove colonne è un file che dichiara una cosa e ne porta un'altra,
/// e l'errore lo dice per nome invece di restituire una riga senza payload.
fn parse_row(n: usize, line: &str, width: usize) -> Result<ExportRow, ExportError> {
    let c = columns(n, line, width)?;
    let num = |col: usize, name: &'static str| -> Result<u64, ExportError> {
        c[col]
            .parse::<u64>()
            .map_err(|_| ExportError::BadNumber {
                line: n,
                column: name,
                value: c[col].clone(),
            })
    };
    let hash = |col: usize| -> Result<Hash, ExportError> {
        Hash::from_hex(&c[col]).map_err(|error| ExportError::BadHash { line: n, error })
    };
    match c[0].as_str() {
        "head" => Ok(ExportRow::Head {
            session: c[1].clone(),
            rows: num(2, "rows")? as usize,
            from: num(3, "seq_from")?,
            to: num(4, "seq_to")?,
            head: hash(6)?,
        }),
        "segment" => Ok(ExportRow::Segment {
            session: c[1].clone(),
            rows: num(2, "rows")? as usize,
            from: num(3, "seq_from")?,
            to: num(4, "seq_to")?,
            index: num(5, "index")? as usize,
            root: hash(6)?,
            proof: c[7].clone(),
        }),
        "witness-head" => Ok(ExportRow::WitnessHead {
            entries: num(2, "rows")? as usize,
            head: hash(6)?,
        }),
        "witness" => Ok(ExportRow::Witness {
            session: c[1].clone(),
            rows: num(2, "rows")? as usize,
            head: hash(6)?,
            at_millis: c[8]
                .parse::<i64>()
                .map_err(|_| ExportError::BadNumber {
                    line: n,
                    column: "at_millis",
                    value: c[8].clone(),
                })?,
        }),
        "observation" if width < CHAIN_COLUMNS_WITH_ROWS.len() => {
            Err(ExportError::RowWithoutPayload { line: n })
        }
        "observation" => {
            let canon = canonicalize_str(&c[9])?;
            if canon != c[9] {
                return Err(ExportError::RowNotCanonical { line: n });
            }
            let declared = num(3, "seq_from")?;
            if let Ok(row) = serde_json::from_str::<Observation>(&canon) {
                if row.seq.0 != declared {
                    return Err(ExportError::RowSeqMismatch {
                        line: n,
                        declared,
                        in_row: row.seq.0,
                    });
                }
            }
            Ok(ExportRow::Observation {
                session: c[1].clone(),
                seq: declared,
                leaf: hash(6)?,
                row: canon,
            })
        }
        other => Err(ExportError::UnknownKind {
            line: n,
            kind: other.to_owned(),
        }),
    }
}

fn columns(n: usize, line: &str, expected: usize) -> Result<Vec<String>, ExportError> {
    let cols = split_columns(line)?;
    if cols.len() != expected {
        return Err(ExportError::ColumnCount {
            line: n,
            found: cols.len(),
            expected,
        });
    }
    Ok(cols)
}

fn expect_header(n: usize, found: &str, expected: &str) -> Result<(), ExportError> {
    if found != expected {
        return Err(ExportError::BadHeader {
            line: n,
            expected: expected.to_owned(),
            found: found.to_owned(),
        });
    }
    Ok(())
}

/// La fuga dichiarata: `\` diventa `\\`, `|` diviente `\|`, il fine riga
/// diventa `\n` o `\r`. Nient'altro viene toccato, così il file resta
/// leggibile e ogni carattere è reversibile.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\|"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn split_columns(line: &str) -> Result<Vec<String>, ExportError> {
    let mut cols: Vec<String> = vec![String::new()];
    let mut it = line.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                // `cols` non è mai vuota: `last_mut` non può fallire e la
                // fuga non ricade su nessuna colonna.
                let last = cols.last_mut();
                match it.next() {
                    Some('\\') => push(last, '\\'),
                    Some('|') => push(last, '|'),
                    Some('n') => push(last, '\n'),
                    Some('r') => push(last, '\r'),
                    other => {
                        return Err(ExportError::BadEscape(
                            other.map(|c| c.to_string()).unwrap_or_default(),
                        ))
                    }
                }
            }
            '|' => cols.push(String::new()),
            c => push(cols.last_mut(), c),
        }
    }
    Ok(cols)
}

fn push(last: Option<&mut String>, c: char) {
    if let Some(s) = last {
        s.push(c);
    }
}

/// Perché il testo non è un export. Ogni variante dice **quale colonna** o
/// **quale riga**, perché «file non valido» non serve a nessuno.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    #[error("export vuoto: manca la tabella del registro")]
    Empty,

    #[error("riga {line}: intestazione {found:?}, attesa {expected:?}")]
    BadHeader {
        line: usize,
        expected: String,
        found: String,
    },

    #[error("riga {line}: {found} colonne, attese {expected}")]
    ColumnCount {
        line: usize,
        found: usize,
        expected: usize,
    },

    #[error("riga {line}: tipo di record sconosciuto «{kind}»")]
    UnknownKind { line: usize, kind: String },

    #[error("riga {line}: «{value}» non è un numero nella colonna {column}")]
    BadNumber {
        line: usize,
        column: &'static str,
        value: String,
    },

    #[error("riga {line}: {error}")]
    BadHash {
        line: usize,
        error: HashParseError,
    },

    #[error("fuga non valida: {0:?}")]
    BadEscape(String),

    #[error("manca la tabella dei limiti dopo la riga {after}")]
    MissingLimitsTable { after: usize },

    #[error("manca la riga del limite {limit}")]
    LimitRowMissing { limit: LimitId },

    #[error("riga {line}: atteso il limite {expected}, trovato {found}")]
    LimitRowOutOfOrder {
        line: usize,
        expected: &'static str,
        found: String,
    },

    #[error("l'export dichiara per il limite {limit} un enunciato diverso da quello del codice: le righe sono state alterate")]
    LimitsAltered { limit: LimitId },

    #[error("riga {line}: contenuto dopo la tabella dei limiti")]
    TrailingContent { line: usize },

    #[error("riga {line}: tipo di riga «observation» in un file di nove colonne: la colonna `row_json` non c'è, e una riga senza il suo corpo non è una riga")]
    RowWithoutPayload { line: usize },

    #[error("riga {line}: `row_json` non è nella forma canonica che la foglia impegna")]
    RowNotCanonical { line: usize },

    #[error("riga {line}: `row_json` è canonico ma non è il testo che l'osservazione produce: una chiave è stata tolta o aggiunta, e la foglia impegna un documento diverso da quello che la colonna dichiara")]
    RowTextMismatch { line: usize },

    #[error("riga {line}: `row_json` non è un'osservazione: {reason}")]
    BadRow { line: usize, reason: String },

    #[error("riga {line}: la foglia dichiarata {declared} non è quella che la riga produce ({recomputed}): il testo è stato cambiato senza rifarne l'hash")]
    RowLeafMismatch {
        line: usize,
        declared: Hash,
        recomputed: Hash,
    },

    #[error("riga {line}: la colonna `seq_from` dichiara {declared} e la riga {in_row}")]
    RowSeqMismatch {
        line: usize,
        declared: u64,
        in_row: u64,
    },

    #[error("riga non confrontabile con la catena: {0}")]
    Canonical(#[from] CanonicalError),
}

impl fmt::Display for ChainExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "export: {} righe, 3 limiti", self.rows.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::SegmentPlan;
    use crate::session::SessionId;
    use kbs_core::{
        ArgumentId, CohortId, CourseId, Evidence, GraderKind, Millis, Observation, PersonId,
        SeqInSession,
    };

    fn obs(seq: u64, tag: &str) -> Observation {
        Observation {
            id: format!("obs-{tag}-{seq}"),
            seq: SeqInSession(seq),
            student: PersonId(format!("person_{tag}")),
            course: CourseId("course_x".into()),
            cohort: CohortId("cohort_x".into()),
            argument: ArgumentId("arg_x".into()),
            evidence: Evidence::Checked {
                exercise: "ex-1".into(),
                instance: format!("inst-{seq}"),
                correct: true,
            },
            judged_by: Some(GraderKind::Deterministic),
            unaided: Some(seq % 5 != 4),
            n_hints: Some(if seq % 5 == 4 { 2 } else { 0 }),
            at: Millis(1_700_000_000_000 + seq as i64),
        }
    }

    fn righe(n: u64, tag: &str) -> Vec<Observation> {
        (0..n).map(|i| obs(i, tag)).collect()
    }

    fn chain(name: &str, n: u64, per: usize) -> Chain {
        Chain::build(
            &SessionId::new(name),
            &righe(n, "a"),
            SegmentPlan::every(per),
        )
        .unwrap()
    }

    fn export() -> ChainExport {
        let (s1, s2) = (SessionId::new("s1"), SessionId::new("s2"));
        let (r1, r2) = (righe(6, "a"), righe(2, "a"));
        let a = Chain::build(&s1, &r1, SegmentPlan::every(2)).unwrap();
        let mut w = Witness::new();
        w.extend(WitnessEntry {
            session: SessionId::new("s1"),
            rows: 6,
            head: a.head(),
            at: Millis(1_700_000_000_000),
        })
        .unwrap();
        ChainExport::of(
            &[
                SessionExport {
                    session: &s1,
                    rows: &r1,
                    plan: SegmentPlan::every(2),
                },
                SessionExport {
                    session: &s2,
                    rows: &r2,
                    plan: SegmentPlan::every(32),
                },
            ],
            Some(&w),
        )
        .unwrap()
    }

    #[test]
    fn intestazioni_e_colonne_fisse() {
        let text = export().to_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], CHAIN_COLUMNS_WITH_ROWS.join("|"));
        assert_eq!(lines[1].split('|').count(), 10);
        let sep = lines.iter().position(|l| l.is_empty()).unwrap();
        assert_eq!(lines[sep + 1], LIMIT_COLUMNS.join("|"));
        // tre righe di limiti e nient'altro
        assert_eq!(lines.len(), sep + 1 + 1 + 3);
        for l in &lines[sep + 1..] {
            assert_eq!(l.split('|').count(), 3);
        }
    }

    #[test]
    fn andata_e_ritorno() {
        let e = export();
        let back = ChainExport::from_text(&e.to_text()).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.limits(), Limits::ALL);
    }

    /// Una riga con l'aiuto non registrato: è il caso che una migrazione
    /// precedente alla colonna produce, ed è la riga che l'export deve
    /// portare **dichiarando** che non lo sa.
    fn ignota(seq: u64) -> Observation {
        Observation {
            unaided: None,
            n_hints: None,
            ..obs(seq, "ignota")
        }
    }

    fn una_sessione(r: &[Observation]) -> ChainExport {
        let s = SessionId::new("s");
        ChainExport::of(
            &[SessionExport {
                session: &s,
                rows: r,
                plan: SegmentPlan::every(32),
            }],
            None,
        )
        .unwrap()
    }

    #[test]
    fn laiuto_ignoto_e_un_null_esplicito_e_non_un_campo_assente() {
        // La disciplina del modulo è che una colonna vale solo se è popolata da
        // ogni payload accettato. Per `row_json` la prova è che la chiave ci
        // sta **anche quando non c'è niente da dire**: `null` è una
        // dichiarazione, l'assenza è un buco, e nella forma canonica sono due
        // cose diverse. Se la chiave sparisse, «aiuto ignoto» e «riga di una
        // versione precedente della riga» diventerebbero la stessa cosa, che è
        // esattamente la confusione che la colonna è venuta togliere.
        let e = una_sessione(&[ignota(1)]);
        let testo = e.to_text();
        let riga = testo
            .lines()
            .find(|l| l.starts_with("observation|"))
            .expect("riga di osservazione");
        assert!(riga.contains("\"unaided\":null"), "{riga}");
        assert!(riga.contains("\"n_hints\":null"), "{riga}");

        let lette = ChainExport::from_text(&testo).unwrap().observations().unwrap();
        assert_eq!(lette[0].1[0].unaided, None, "il viaggio di ritorno non inventa");
        assert_eq!(lette[0].1[0].n_hints, None);
    }

    #[test]
    fn laiuto_e_impegnato_dalla_foglia_e_non_e_una_dicitura() {
        // Il punto per cui sta in `row_json` e non in una colonna: la foglia è
        // l'hash di quel testo, quindi cambiare `unaided` cambia la foglia. Un
        // file in cui la colonna dicesse «assistita» e la riga dicesse «non
        // assistita» non potrebbe esistere; un file in cui la stessa informazione
        // sta in due posti sì.
        let base = obs(1, "x");
        let assistita = Observation {
            unaided: Some(false),
            n_hints: Some(3),
            ..base.clone()
        };
        let non_assistita = Observation {
            unaided: Some(true),
            n_hints: Some(0),
            ..base.clone()
        };
        assert_ne!(leaf_of(&assistita).unwrap(), leaf_of(&non_assistita).unwrap());
        assert_ne!(leaf_of(&base).unwrap(), leaf_of(&ignota(1)).unwrap());

        // E chi riceve il file vede la differenza, perché le righe viaggiano: due
        // righe che si distinguono solo per `unaided` hanno id e `seq` diversi,
        // altrimenti la catena le rifiuterebbe come duplicati.
        let a = Observation {
            id: "obs-assistita".into(),
            seq: SeqInSession(1),
            unaided: Some(false),
            n_hints: Some(3),
            ..obs(0, "z")
        };
        let b = Observation {
            id: "obs-non-assistita".into(),
            seq: SeqInSession(2),
            unaided: Some(true),
            n_hints: Some(0),
            ..obs(0, "z")
        };
        let e = una_sessione(&[a, b]);
        let lette = ChainExport::from_text(&e.to_text()).unwrap().observations().unwrap();
        let viste: Vec<Option<bool>> = lette[0].1.iter().map(|o| o.unaided).collect();
        assert_eq!(viste, vec![Some(false), Some(true)]);
        let indizi: Vec<Option<u32>> = lette[0].1.iter().map(|o| o.n_hints).collect();
        assert_eq!(indizi, vec![Some(3), Some(0)]);
    }

    /// La colonna e il valore devono essere **lo stesso testo**, e il caso che
    /// lo dimostra è una chiave **tolta**: un `Option` che manca vale `None`,
    /// quindi il valore tipizzato è lo stesso, la foglia ricalcolata sul tipo è
    /// quella dichiarata e il file si apriva. «Aiuto ignoto» e «riga di una
    /// versione precedente della riga» erano la stessa cosa, che è esattamente
    /// la confusione che la colonna è venuta a togliere.
    #[test]
    fn una_chiave_tolta_dalla_colonna_non_e_una_riga_aperta() {
        let e = una_sessione(&[ignota(1)]);
        let testo = e.to_text();
        let riga = testo
            .lines()
            .find(|l| l.starts_with("observation|"))
            .expect("riga di osservazione")
            .rsplit_once('|')
            .map(|(_, json)| json.to_owned())
            .expect("la riga ha la colonna `row_json`");
        assert!(riga.contains("\"n_hints\":null,"), "{riga}");

        let tolta = riga.replace("\"n_hints\":null,", "");
        assert_ne!(riga, tolta, "la chiave non era nella colonna");
        // Il documento resta canonico, e questa è la parte che rende la
        // manomissione economica: `RowNotCanonical` non la prende, e senza il
        // confronto con il valore il file si aprirebbe.
        assert_eq!(canonicalize_str(&tolta).unwrap(), tolta);

        let manomesso = testo.replacen(&riga, &tolta, 1);
        assert_ne!(manomesso, testo);
        assert!(matches!(
            ChainExport::from_text(&manomesso).unwrap().observations(),
            Err(ExportError::RowTextMismatch { .. })
        ));
    }

    #[test]
    fn le_ripartono_e_il_testimone_torna_con_loro() {
        let s = SessionId::new("s1");
        let r = righe(6, "a");
        let mut w = Witness::new();
        w.observe(&Chain::build(&s, &r, SegmentPlan::every(2)).unwrap())
            .unwrap();
        let e = ChainExport::of(
            &[SessionExport {
                session: &s,
                rows: &r,
                plan: SegmentPlan::every(2),
            }],
            Some(&w),
        )
        .unwrap();

        let back = ChainExport::from_text(&e.to_text()).unwrap();
        let lette = back.observations().unwrap();
        assert_eq!(lette.len(), 1);
        assert_eq!(lette[0].0, s);
        assert_eq!(lette[0].1, r);
        let tornato = back.witness().unwrap().unwrap();
        assert_eq!(tornato.head(), w.head());
        assert!(tornato.check().is_ok());
    }

    #[test]
    fn lesportazione_contiene_teste_prove_e_testimone() {
        let text = export().to_text();
        let a = chain("s1", 6, 2);
        assert!(text.contains(&format!("head|s1|6|0|5||{}|", a.head())));
        // tre segmenti da due righe: la prova del primo è lunga
        assert!(text.contains("segment|s1|2|0|1|0|"));
        // la voce del testimone non inventa l'intervallo di seq: sa quante
        // righe aveva, non come erano numerate
        assert!(text.contains(&format!("witness|s1|6||||{}|", a.head())));
        assert!(text.contains("witness-head||1|"));
        assert!(text.contains("left:"));
    }

    #[test]
    fn l_export_dichiara_l_intervallo_di_seq_reale() {
        // un registro che comincia da 1 dichiara `1..=3`, non `0..=2`: chi
        // riceve il file deve poter ricontare gli stessi numeri che vede nel
        // registro
        let mut righe: Vec<Observation> = (0..3).map(|i| obs(i, "a")).collect();
        for (i, r) in righe.iter_mut().enumerate() {
            r.seq = SeqInSession(i as u64 + 1);
        }
        let s = SessionId::new("s1");
        let text = ChainExport::of(
            &[SessionExport {
                session: &s,
                rows: &righe,
                plan: SegmentPlan::every(2),
            }],
            None,
        )
        .unwrap()
        .to_text();
        assert!(text.contains("head|s1|3|1|3||"), "{text}");
        assert!(text.contains("segment|s1|2|1|2|0|"), "{text}");
        assert!(text.contains("segment|s1|1|3|3|1|"), "{text}");
    }

    #[test]
    fn la_tabella_dei_limiti_viaggia_con_i_dati() {
        let text = export().to_text();
        for limit in Limits::ALL.iter() {
            assert!(
                text.lines().any(|l| l.starts_with(&format!("{}|", limit.id.as_str()))),
                "manca la riga del limite {}",
                limit.id
            );
        }
        assert!(text.contains("integrità, non verità"));
    }

    #[test]
    fn i_limiti_alterati_rendono_il_file_illeggibile() {
        let text = export().to_text();
        let addolcito = text.replace(
            "l'hash garantisce integrità, non verità: uno span che non sostiene la claim è una riga impeccabilmente conforme",
            "l'hash garantisce integrità e verità",
        );
        assert_ne!(text, addolcito);
        assert_eq!(
            ChainExport::from_text(&addolcito),
            Err(ExportError::LimitsAltered {
                limit: LimitId::Truth
            })
        );
    }

    #[test]
    fn limite_nel_ordine_sbagliato_e_un_errore() {
        let text = export().to_text();
        let mut lines: Vec<String> = text.lines().map(|s| s.to_owned()).collect();
        let sep = lines.iter().position(|l| l.is_empty()).unwrap();
        lines.swap(sep + 2, sep + 3);
        assert!(matches!(
            ChainExport::from_text(&lines.join("\n")),
            Err(ExportError::LimitRowOutOfOrder { .. })
        ));
    }

    #[test]
    fn intestazione_sbagliata_e_un_errore() {
        let text = export().to_text().replacen("kind|session", "tipo|ses", 1);
        assert!(matches!(
            ChainExport::from_text(&text),
            Err(ExportError::BadHeader { line: 0, .. })
        ));
    }

    #[test]
    fn colonne_mancanti_e_un_errore() {
        let text = export().to_text();
        let spezzata = text.replacen("segment|s1|2|0|1|0|", "segment|s1|2|0|1|", 1);
        // La riga perde una cella e resta a nove: nove colonne dove
        // l'intestazione ne dichiara dieci non sono un export, è un file che
        // ha perso qualcosa e non lo dice.
        assert!(matches!(
            ChainExport::from_text(&spezzata),
            Err(ExportError::ColumnCount {
                found: 9,
                expected: 10,
                ..
            })
        ));
    }

    #[test]
    fn tipo_sconosciuto_e_un_errore_nominato() {
        let text = export().to_text();
        let strano = text.replacen("head|s1|", "pastro|s1|", 1);
        assert_eq!(
            ChainExport::from_text(&strano),
            Err(ExportError::UnknownKind {
                line: 1,
                kind: "pastro".into()
            })
        );
    }

    #[test]
    fn numero_non_valido_e_un_errore_nominato() {
        let text = export().to_text();
        let rotto = text.replacen("head|s1|6|", "head|s1|sei|", 1);
        assert_eq!(
            ChainExport::from_text(&rotto),
            Err(ExportError::BadNumber {
                line: 1,
                column: "rows",
                value: "sei".into()
            })
        );
    }

    #[test]
    fn hash_malformata_e_un_errore_nominato() {
        let text = export().to_text();
        let a = chain("s1", 6, 2);
        let rotto = text.replacen(&a.head().to_hex(), "zz", 1);
        assert!(matches!(
            ChainExport::from_text(&rotto),
            Err(ExportError::BadHash { line: 1, .. })
        ));
    }

    #[test]
    fn la_fuga_di_separatore_e_reversibile() {
        let session = "corso|con\\barra\ne\rrighe";
        let s = SessionId::new(session);
        let r = [obs(0, "a")];
        let e = ChainExport::of(
            &[SessionExport {
                session: &s,
                rows: &r,
                plan: SegmentPlan::default_plan(),
            }],
            None,
        )
        .unwrap();
        let text = e.to_text();
        // il separatore di colonna non compare mai scoperto in un campo della
        // tabella del registro
        let sep = text.lines().position(|l| l.is_empty()).unwrap();
        let data_lines: Vec<&str> = text.lines().take(sep).skip(1).collect();
        assert!(!data_lines.is_empty());
        // il pipe sfuggito non è un separatore: lo decide il lettore, non
        // l'occhio, ed è quello che il parser conta
        for l in data_lines {
            assert_eq!(split_columns(l).unwrap().len(), 10, "riga non a dieci colonne: {l}");
        }
        assert!(text.contains("corso\\|con\\\\barra\\ne\\rrighe"));
        assert_eq!(ChainExport::from_text(&text).unwrap(), e);
    }

    #[test]
    fn fuga_non_valida_e_un_errore() {
        assert!(matches!(
            split_columns("a\\qb"),
            Err(ExportError::BadEscape(_))
        ));
        assert!(matches!(
            split_columns("a\\"),
            Err(ExportError::BadEscape(_))
        ));
    }

    #[test]
    fn file_vuoto_o_incompieto() {
        assert_eq!(ChainExport::from_text(""), Err(ExportError::Empty));
        let text = export().to_text();
        let corto: String = text
            .lines()
            .take(2)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(matches!(
            ChainExport::from_text(&corto),
            Err(ExportError::MissingLimitsTable { .. })
        ));
    }

    #[test]
    fn contenuto_dopo_i_limiti_e_un_errore() {
        let text = export().to_text();
        assert!(matches!(
            ChainExport::from_text(&format!("{text}riga in più\n")),
            Err(ExportError::TrailingContent { .. })
        ));
    }

    #[test]
    fn export_di_niente_ha_solo_le_colonne_e_i_limiti() {
        let e = ChainExport::of(&[], None).unwrap();
        let text = e.to_text();
        assert_eq!(text.lines().count(), 6);
        assert_eq!(ChainExport::from_text(&text).unwrap(), e);
    }
}
