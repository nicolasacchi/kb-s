//! L'indice lessicale: il percorso di sincronizzazione e il percorso di query.
//!
//! # Chi entra nell'indice e chi no
//!
//! Nell'indice entra ciò che è stato **indicizzato**: un chunk di un argomento
//! esistente. Non entrano i pezzi del percorso speculativo, perché non sono
//! chunk di nessun argomento e D4 li tiene fuori dall'indice condiviso per
//! costruzione, non per filtro.
//!
//! Un argomento in bozza, invece, è indicizzato e **non è visibile**: la
//! visibilità dell'indice è decisa dal predicato, in lettura, dalla stessa parte
//! che decide la visibilità di un argomento. È la stessa funzione, e questo è il
//! punto: due filtri di visibilità sarebbero due implementazioni della regola di
//! D5, e la seconda deriverebbe.
//!
//! # Il confine di fiducia, dichiarato
//!
//! Le righe di `artifact_chunks` di una bozza sono **fisicamente** nell'indice.
//! Chi legge il file `.sqlite` le vede tutte, predicato o no. È un fatto, e
//! merita una frase: il database è un artefatto derivato (D12: il corpus è una
//! cartella di file su git, il server è un mirror), e va trattato come materiale
//! fidato. Il predicato protegge da un *richiamo* sbagliato, non da chi ha il
//! file in mano. Un database che contiene tutto e un database che contiene solo
//! il published avrebbero lo stesso costo di distribuzione, e solo il secondo
//! richiederebbe un trigger che decide la visibilità — cioè una seconda
//! implementazione della regola.
//!
//! # Il percorso di sincronizzazione è codice, non trigger
//!
//! `artifact_fts` è contentless: i suoi contenuti sono il testo **già piegato** da
//! [`crate::italian::fold`], e un trigger SQL non può piegare. Quindi l'indice
//! è alimentato da qui, e per questo [`Store::reindex_all`] esiste: se qualcosa
//! è stato scritto con `conn()` senza passare da [`Store::index_chunk`],
//! `reindex_all` rimette le cose a posto. Il prezzo è dichiarato nella
//! migrazione che crea la tabella.

use std::collections::HashMap;

use kbs_core::{ArgumentId, PersonId};

use crate::error::{Error, Result};
use crate::italian;
use crate::store::Store;
use crate::types::{ArtifactChunk, ChunkId, SearchHit};

/// Quante righe l'SQL porta al predicato prima che il filtro decida.
///
/// Il predicato gira in Rust, quindi la `SELECT` deve sovra-recuperare: se
/// limitasse prima, i primi N risultati potrebbero essere tutti invisibili e la
/// risposta sarebbe «nessun risultato» quando ce ne sono. Il sovrappasso è
/// dichiarato invece che nascosto, e il tetto è qui perché una classe di cento
/// studenti non deve poter trasformare una ricerca in una scansione.
const PREDICATE_HEADROOM: usize = 20;
const MAX_SCAN: usize = 2_000;

impl Store {
    /// Indicizza un chunk, o lo aggiorna se esiste già.
    ///
    /// L'id della riga è `(argument, kind, ord)`: riindicizzare un artifact
    /// riscritto non crea una seconda riga, e l'indice non accumula le versioni
    /// vecchie. Un indice che cresce a ogni build è un indice che non si può più
    /// fidare.
    pub fn index_chunk(&mut self, chunk: &ArtifactChunk) -> Result<ChunkId> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO artifact_chunks ( \
                 argument_id, course_id, kind, ord, rel_path, title, body, headings, code, \
                 prompt, contract, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
             ON CONFLICT (argument_id, kind, ord) DO UPDATE SET \
                 rel_path = ?5, title = ?6, body = ?7, headings = ?8, code = ?9, \
                 prompt = ?10, contract = ?11, updated_at = ?12",
            rusqlite::params![
                chunk.argument.0,
                chunk.course.0,
                chunk.kind.as_str(),
                chunk.ord,
                chunk.rel_path,
                chunk.title,
                chunk.body,
                chunk.headings,
                chunk.code,
                chunk.prompt,
                chunk.contract,
                chunk.updated_at.0,
            ],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM artifact_chunks WHERE argument_id = ?1 AND kind = ?2 AND ord = ?3",
            rusqlite::params![chunk.argument.0, chunk.kind.as_str(), chunk.ord],
            |r| r.get(0),
        )?;
        let folded = [
            italian::fold(&chunk.title),
            italian::fold(&chunk.body),
            italian::fold(&chunk.headings),
            italian::fold(&chunk.code),
            italian::fold(&chunk.prompt),
            italian::fold(&chunk.contract),
        ];
        // `contentless_delete = 1` fa di questo un DELETE vero. Senza, l'indice
        // accumulerebbe le versioni precedenti e ogni ricerca restituirebbe anche
        // il testo che non c'è più: la stessa malattia del registro che riscrive
        // le proprie righe, solo più silenziosa.
        tx.execute("DELETE FROM artifact_fts WHERE rowid = ?1", [id])?;
        tx.execute(
            "INSERT INTO artifact_fts (rowid, title, body, headings, code, prompt, contract) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, folded[0], folded[1], folded[2], folded[3], folded[4], folded[5]],
        )?;
        tx.commit()?;
        Ok(ChunkId(id))
    }

    /// Toglie un chunk dall'indice e dal database.
    pub fn unindex_chunk(&mut self, id: ChunkId) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM artifact_fts WHERE rowid = ?1", [id.0])?;
        tx.execute("DELETE FROM artifact_chunks WHERE id = ?1", [id.0])?;
        tx.commit()?;
        Ok(())
    }

    /// Toglie tutti i chunk di un argomento. Ritorna quanti ne aveva.
    pub fn unindex_argument(&mut self, argument: &ArgumentId) -> Result<usize> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM artifact_fts WHERE rowid IN (SELECT id FROM artifact_chunks WHERE argument_id = ?1)",
            [&argument.0],
        )?;
        let removed = tx.execute(
            "DELETE FROM artifact_chunks WHERE argument_id = ?1",
            [&argument.0],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// Ricostruisce l'indice da `artifact_chunks`.
    ///
    /// Serve dopo una scrittura passata da [`Store::conn`], che è l'unico modo
    /// di avere contenuto nell'indice senza passare da
    /// [`Store::index_chunk`]. Ritorna quante righe ha indicizzato, così il
    /// chiamante può capire se aveva qualcosa da riallineare.
    pub fn reindex_all(&mut self) -> Result<usize> {
        let rows: Vec<(i64, String, String, String, String, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, title, body, headings, code, prompt, contract FROM artifact_chunks",
            )?;
            let mapped = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?;
            let mut collected = Vec::new();
            for row in mapped {
                collected.push(row?);
            }
            collected
        };
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM artifact_fts", [])?;
        for (id, title, body, headings, code, prompt, contract) in &rows {
            tx.execute(
                "INSERT INTO artifact_fts (rowid, title, body, headings, code, prompt, contract) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    italian::fold(title),
                    italian::fold(body),
                    italian::fold(headings),
                    italian::fold(code),
                    italian::fold(prompt),
                    italian::fold(contract),
                ],
            )?;
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// Cerca, per una persona, e restituisce solo ciò che quella persona può
    /// vedere.
    ///
    /// Il predicato è applicato **dopo** il `bm25` e **prima** del limite: è il
    /// punto in cui la ricerca incontra la visibilità, ed è l'unico. Un indice
    /// condiviso che restituisce una bozza a uno studente non è un problema di
    /// ranking, è una porta.
    ///
    /// L'ordine è per `rank` crescente, perché `bm25` è negativo e più negativo
    /// è migliore. Il nome del campo lo dice, così nessuno lo riordina al contrario.
    pub fn search(
        &self,
        person: &PersonId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let match_query = italian::to_fts_query(query).ok_or(Error::EmptyQuery)?;
        let scan = limit.saturating_mul(PREDICATE_HEADROOM).min(MAX_SCAN);
        // `bm25()` si può usare solo dove la tabella FTS è quella che si sta
        // interrogando: unirla ad `artifact_chunks` nella stessa `SELECT` fa
        // fallire SQLite con «unable to use function bm25 in the requested
        // context», e la cosa è stata verificata, non dedotta. Quindi qui si
        // chiedono solo `rowid` e `rank`, e la risoluzione `rowid -> chunk ->
        // argomento` avviene dopo, in Rust.
        let mut best: HashMap<String, (f64, String)> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT rowid, bm25(artifact_fts) AS rank FROM artifact_fts \
                  WHERE artifact_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![match_query, scan as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
            })?;
            let mut rowids = Vec::new();
            for row in rows {
                let (rowid, rank) = row?;
                rowids.push((rowid, rank));
            }
            if rowids.is_empty() {
                return Ok(Vec::new());
            }
            let mut segnaposto = String::new();
            for (i, _) in rowids.iter().enumerate() {
                if i > 0 {
                    segnaposto.push(',');
                }
                segnaposto.push('?');
            }
            let sql = format!(
                "SELECT id, argument_id, course_id FROM artifact_chunks WHERE id IN ({segnaposto})"
            );
            let chiavi: Vec<&dyn rusqlite::ToSql> =
                rowids.iter().map(|(rowid, _)| rowid as &dyn rusqlite::ToSql).collect();
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(chiavi.as_slice(), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            let mut corsi: HashMap<i64, (String, String)> = HashMap::new();
            for row in rows {
                let (id, argument, course) = row?;
                corsi.insert(id, (argument, course));
            }
            // Un argomento con più chunk non deve comparire due volte: si tiene
            // il miglior `rank` e il predicato gira una volta sola per argomento.
            for (rowid, rank) in rowids {
                let Some((argument, course)) = corsi.remove(&rowid) else {
                    // Un `rowid` senza chunk non è un errore: è la firma di un
                    // indice non allineato, e `reindex_all` lo sistema. Saltarlo
                    // è giusto; mentire sul risultato non lo sarebbe.
                    continue;
                };
                match best.get(&argument) {
                    Some((migliore, _)) if *migliore <= rank => {}
                    _ => {
                        best.insert(argument, (rank, course));
                    }
                }
            }
        }
        let mut out: Vec<SearchHit> = best
            .into_iter()
            .map(|(argument, (rank, course))| SearchHit {
                argument: ArgumentId(argument),
                course: kbs_core::CourseId(course),
                rank,
            })
            .collect();
        // `total_cmp` e non `partial_cmp`: `bm25` può restituire un `NaN` su una
        // query degenere (un termine presente in tutti i documenti rende
        // l'IDF infinita), e un `NaN` nell'ordinamento è un `panic` se si usa
        // `partial_cmp`. `total_cmp` mette i `NaN` in coda e non si ferma: un
        // risultato strano è già la risposta, e va data.
        out.sort_by(|a, b| {
            a.rank
                .total_cmp(&b.rank)
                .then_with(|| a.argument.cmp(&b.argument))
        });

        let mut visible: Vec<SearchHit> = Vec::new();
        for hit in out {
            let course = hit.course.clone();
            let visibility = self.visibility(person, &course)?;
            let Ok(argument) = self.argument_unrestricted(&hit.argument) else {
                continue;
            };
            if let Some(argument) = argument {
                if visibility.may_see(&argument) {
                    visible.push(hit);
                }
            }
            if visible.len() >= limit {
                break;
            }
        }
        Ok(visible)
    }
}
