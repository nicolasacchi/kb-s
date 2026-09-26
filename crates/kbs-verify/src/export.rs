//! L'export a colonne fisse (D12).
//!
//! «L'uscita è `rm -rf`»: il registro deve poter uscire dal sistema in un
//! formato che chi riceve non deve fidarsi del sistema per interpretare. Qui
//! il formato è una tabella a **colonne fisse**, con intestazione esplicita e
//! senza campi che possano contenere il separatore senza dichiararlo: la fuga
//! è esplicita (`\|`, `\\`, `\n`, `\r`) e il parser la ripercorre.
//!
//! L'export porta tre cose, e le tre servono a chi non si fida del server:
//!
//! 1. la testa di ogni sessione e le prove di ogni segmento;
//! 2. le voci del testimone, senza le quali le prove sono belle e non servono;
//! 3. **i tre limiti**, riga per riga.
//!
//! Il punto 3 è il più importante e il meno ovvio. Un export che contiene una
//! catena e non i suoi limiti è un certificato che promette troppo: chi lo
//! riceve non ha modo di sapere che cosa non gli è stato garantito. Qui i
//! limiti viaggiano con i dati, e il parser **rifiuta** un export i cui limiti
//! non sono identici a quelli del codice: se qualcuno li ha addolciti, il file
//! non si apre.

use crate::chain::{Chain, ConsistencyProof, VerifyError};
use crate::hash::{Hash, HashParseError};
use crate::limits::{LimitId, Limits};
use crate::witness::{Witness, WitnessEntry};
use std::fmt;

/// Le colonne della tabella del registro, in quest'ordine e in nessun altro.
pub const CHAIN_COLUMNS: [&str; 9] = [
    "kind", "session", "rows", "seq_from", "seq_to", "index", "head", "proof", "at_millis",
];

/// Le colonne della tabella dei limiti.
pub const LIMIT_COLUMNS: [&str; 3] = ["limit", "statement", "mitigation"];

/// Una riga dell'export.
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// L'export di un insieme di sessioni, con il testimone se c'è.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainExport {
    rows: Vec<ExportRow>,
    limits: Limits,
}

impl ChainExport {
    /// Raccoglie le sessioni e il testimone nelle colonne fisse.
    pub fn of(chains: &[Chain], witness: Option<&Witness>) -> Result<ChainExport, VerifyError> {
        let mut rows = Vec::new();
        for chain in chains {
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

    /// Il testo dell'export: due tabelle, intestazione in entrambe.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&CHAIN_COLUMNS.join("|"));
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
    pub fn from_text(text: &str) -> Result<ChainExport, ExportError> {
        if text.is_empty() {
            return Err(ExportError::Empty);
        }
        let mut lines = text.split('\n').enumerate();
        let mut rows = Vec::new();

        let (n, header) = lines.next().ok_or(ExportError::Empty)?;
        expect_header(n, header, &CHAIN_COLUMNS.join("|"))?;

        let mut separator = n;
        for (n, line) in lines.by_ref() {
            if line.is_empty() {
                separator = n;
                break;
            }
            rows.push(parse_row(n, line)?);
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
    };
    for c in cells.iter_mut() {
        *c = escape(c);
    }
    cells.join("|")
}

fn parse_row(n: usize, line: &str) -> Result<ExportRow, ExportError> {
    let c = columns(n, line, CHAIN_COLUMNS.len())?;
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
            at: Millis(1_700_000_000_000 + seq as i64),
        }
    }

    fn chain(name: &str, n: u64, per: usize) -> Chain {
        let rows: Vec<Observation> = (0..n).map(|i| obs(i, "a")).collect();
        Chain::build(&SessionId::new(name), &rows, SegmentPlan::every(per)).unwrap()
    }

    fn export() -> ChainExport {
        let a = chain("s1", 6, 2);
        let mut w = Witness::new();
        w.extend(WitnessEntry {
            session: SessionId::new("s1"),
            rows: 6,
            head: a.head(),
            at: Millis(1_700_000_000_000),
        })
        .unwrap();
        ChainExport::of(&[a, chain("s2", 2, 32)], Some(&w)).unwrap()
    }

    #[test]
    fn intestazioni_e_colonne_fisse() {
        let text = export().to_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], CHAIN_COLUMNS.join("|"));
        assert_eq!(lines[1].split('|').count(), 9);
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
        let c = Chain::build(&SessionId::new("s1"), &righe, SegmentPlan::every(2)).unwrap();
        let text = ChainExport::of(&[c], None).unwrap().to_text();
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
        assert!(matches!(
            ChainExport::from_text(&spezzata),
            Err(ExportError::ColumnCount { found: 8, .. })
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
        let chain = Chain::build(
            &SessionId::new(session),
            &[obs(0, "a")],
            SegmentPlan::default_plan(),
        )
        .unwrap();
        let e = ChainExport::of(&[chain], None).unwrap();
        let text = e.to_text();
        // il separatore di colonna non compare mai scoperto in un campo della
        // tabella del registro
        let sep = text.lines().position(|l| l.is_empty()).unwrap();
        let data_lines: Vec<&str> = text.lines().take(sep).skip(1).collect();
        assert!(!data_lines.is_empty());
        // il pipe sfuggito non è un separatore: lo decide il lettore, non
        // l'occhio, ed è quello che il parser conta
        for l in data_lines {
            assert_eq!(split_columns(l).unwrap().len(), 9, "riga non a nove colonne: {l}");
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
