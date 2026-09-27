//! FTS5 in italiano: che cosa funziona, che cosa no, e quanto costa.
//!
//! I test di questa sezione hanno due compiti. Il primo è il requisito: la
//! ricerca deve trovare un termine **dentro una sezione del contratto**, perché
//! il contratto è contenuto e non allegato (D2). Il secondo è più importante:
//! ogni debolezza del tokenizzatore che il doc di `italian` dichiara deve avere
//! qui un test che la dimostra, così la dichiarazione non è una prosa.

use kbs_core::{ArgumentId, Millis};

use super::School;
use crate::italian;
use crate::types::{ArtifactChunk, ChunkKind};

/// Le otto sezioni di D7, nei budget dati.
fn contratto() -> &'static str {
    "GUARDIAN: non pubblicare un contratto senza fonte esterna verificabile.\n\
     PREREQUISITI: la continuità su R.\n\
     OBIETTIVI: riconoscere la continuità e darne una definizione.\n\
     SCALA: 1-10, dove 1 è «non raggiunto» e 10 è «padroneggiato».\n\
     EQUIVOCI: continuità non è continuità uniforme.\n\
     ESEMPIO-LAVORATO: la funzione costante è continua in ogni punto.\n\
     VERIFICA: dimostrare che 1 - x² non è continua in 1.\n\
     LIMITE: la continuità non basta per la Riemann-integrabilità."
}

/// Indicizza un argomento con corpo e contratto.
fn indicizza(s: &mut School, n: u32, title: &str, body: &str, contract: &str) -> ArgumentId {
    let argument = s.published(n);
    s.store
        .index_chunk(&ArtifactChunk {
            argument: argument.id.clone(),
            course: s.course.clone(),
            kind: ChunkKind::Argument,
            ord: 0,
            rel_path: "corsi/analisi-1/lezione.html".to_string(),
            title: title.to_string(),
            body: body.to_string(),
            headings: "Continuità\nLimite".to_string(),
            code: "f(x) = 1 - x*x".to_string(),
            prompt: "Che cosa dice GUARDIAN?".to_string(),
            contract: contract.to_string(),
            updated_at: Millis(1_700_000_000_000),
        })
        .expect("indicizzazione");
    argument.id
}

#[test]
fn un_termine_dentro_una_sezione_del_contratto_si_trova() {
    let mut s = School::new();
    let id = indicizza(
        &mut s,
        1,
        "Continuità",
        "Una funzione continua non ha salti.",
        contratto(),
    );

    // `Riemann-integrabilità` sta dentro la sezione LIMITE del contratto. Un
    // termine che esiste solo lì è la prova che il contratto è indicizzato come
    // contenuto e non come allegato.
    let trovati = s
        .store
        .search(&s.teacher, "Riemann-integrabilità", 10)
        .expect("ricerca");
    assert_eq!(trovati.len(), 1);
    assert_eq!(trovati[0].argument, id);

    // E le altre sette sezioni, per ogni sezione, non per la prima che capita.
    for termine in [
        "GUARDIAN",
        "PREREQUISITI",
        "OBIETTIVI",
        "SCALA",
        "EQUIVOCI",
        "ESEMPIO-LAVORATO",
        "VERIFICA",
        "LIMITE",
    ] {
        let trovati = s
            .store
            .search(&s.teacher, termine, 10)
            .expect("ricerca");
        assert!(
            trovati.iter().any(|h| h.argument == id),
            "la sezione {termine} del contratto non è indicizzata"
        );
    }
}

#[test]
fn la_ricerca_applica_il_predicato_e_una_bozza_non_esce() {
    let mut s = School::new();
    let pubblicato = indicizza(&mut s, 1, "Continuità", "sale e scende", contratto());

    // Lo stesso materiale in bozza: indicizzato, invisibile a uno studente.
    let bozza = School::argument(&s.course, &s.teacher, 2, kbs_core::PublicationState::Bozza);
    s.store.upsert_argument(&bozza).expect("bozza");
    s.store
        .index_chunk(&ArtifactChunk {
            argument: bozza.id.clone(),
            course: s.course.clone(),
            kind: ChunkKind::Argument,
            ord: 0,
            rel_path: "corsi/analisi-1/bozza.html".to_string(),
            title: "Continuità".to_string(),
            body: "sale e scende".to_string(),
            headings: String::new(),
            code: String::new(),
            prompt: String::new(),
            contract: contratto().to_string(),
            updated_at: Millis(1_700_000_000_000),
        })
        .expect("indicizzazione della bozza");

    let dello_studente = s
        .store
        .search(&s.student, "continuità", 10)
        .expect("ricerca dello studente");
    assert_eq!(
        dello_studente.len(),
        1,
        "allo studente non deve arrivare la bozza"
    );
    assert_eq!(dello_studente[0].argument, pubblicato);

    let del_docente = s
        .store
        .search(&s.teacher, "continuità", 10)
        .expect("ricerca del docente");
    assert_eq!(del_docente.len(), 2, "al docente arrivano entrambe");

    let del_docente_di_un_altro_corso = s
        .store
        .search(&s.other_teacher, "continuità", 10)
        .expect("ricerca");
    assert!(del_docente_di_un_altro_corso.is_empty());
}

#[test]
fn l_apostrofo_italiano_che_per_l_tokenizzatore_e_un_errore_qui_e_una_ricerca() {
    let mut s = School::new();
    indicizza(
        &mut s,
        1,
        "Continuità",
        "La continuità di una funzione costante.",
        "LIMITE: la continuità di f non basta: l'acqua bolle a 100 gradi.",
    );

    // Il termine che in italiano si scrive con l'apostrofo è «acqua». Trattato
    // come linguaggio FTS5 è un errore di sintassi, e l'errore è la prova che la
    // pipeline serve.
    // Il termine grezzo va in un **parametro**, non nella stringa SQL: è così
    // che lo scriverebbe un chiamante che non sa nulla di FTS5, ed è
    // l'errore che si vede (`fts5: syntax error near "'"`).
    let grezzo: Result<i64, _> = s.store.conn().query_row(
        "SELECT COUNT(*) FROM artifact_fts WHERE artifact_fts MATCH ?1",
        ["l'acqua"],
        |r| r.get(0),
    );
    assert!(
        grezzo.is_err(),
        "se questa query non fallisce, la pipeline non serve più: l'apostrofo non è più un errore"
    );

    // Dalla strada del crate la stessa ricerca risponde.
    let trovati = s.store.search(&s.teacher, "l'acqua", 10).expect("ricerca");
    assert_eq!(trovati.len(), 1, "«l'acqua» si cerca e si trova");

    // E la forma senza elisione trova la stessa cosa: la piegatura mette a
    // spazio l'apostrofo da entrambe le parti.
    assert_eq!(
        s.store
            .search(&s.teacher, "acqua", 10)
            .expect("ricerca")
            .len(),
        1
    );
}

#[test]
fn il_trattino_il_due_punti_e_la_virgola_decimale_che_erano_sintassi() {
    let mut s = School::new();
    indicizza(
        &mut s,
        1,
        "Continuità",
        "least-squares e 3,14: la scala va da 1 a 10.",
        "SCALA: 1-10, dove 1 è «non raggiunto».",
    );

    // Prima la prova del danno, per ciascuno dei tre: grezzi, sono un errore di
    // sintassi o un filtro di colonna. Il trattino e i due punti diventano
    // «no such column», la virgola e il punto un errore vero. Sono i messaggi
    // che il docente si troverebbe davanti senza la pipeline.
    for (grezzo, atteso) in [
        ("least-squares", "no such column"),
        ("scala: 1-10", "no such column"),
        ("1-10", "no such column"),
        ("3,14", "syntax error"),
        ("3.14", "syntax error"),
    ] {
        let esito: Result<i64, String> = s
            .store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM artifact_fts WHERE artifact_fts MATCH ?1",
                [grezzo],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string());
        match esito {
            Err(messaggio) => assert!(
                messaggio.contains(atteso),
                "{grezzo:?} ha detto `{messaggio}`, non `{atteso}`"
            ),
            Ok(n) => assert_eq!(n, 0, "{grezzo:?} ha restituito risultati, non un errore"),
        }
    }

    // E poi, dalla strada del crate, gli stessi termini sono ricerche.
    for query in [
        "least-squares",
        "3,14",
        "scala: 1-10",
        "1-10",
        "least squares",
    ] {
        let trovati = s
            .store
            .search(&s.teacher, query, 10)
            .unwrap_or_else(|e| panic!("la ricerca {query:?} è fallita: {e}"));
        assert!(!trovati.is_empty(), "{query:?} non trova nulla");
    }
}

#[test]
fn le_legature_e_le_lettere_barrate_sono_raggiungibili_dalla_forma_ascii() {
    let mut s = School::new();
    indicizza(
        &mut s,
        1,
        "Corpus europeo",
        "Kowalski, Østergaard, Šafář e Łukasz hanno seguito il corso.",
        contratto(),
    );

    // Il confronto va fatto su un indice che **non** è il nostro, perché il
    // nostro contiene già il testo piegato e quindi risponderebbe sì per forza.
    // Qui si indicizza il testo grezzo con lo stesso tokenizzatore di D2, che è
    // la configurazione che `italian` rende insufficiente.
    let confronto = rusqlite::Connection::open_in_memory().expect("memoria");
    confronto
        .execute_batch(
            "CREATE VIRTUAL TABLE grezzo USING fts5(w, tokenize = \"unicode61 remove_diacritics 2\"); \
             INSERT INTO grezzo(rowid, w) VALUES (1, 'Kowalski, Ostergaard e Łukasz hanno seguito il corso.');",
        )
        .expect("indice di confronto");
    let solo_unicode61: i64 = confronto
        .query_row("SELECT COUNT(*) FROM grezzo WHERE grezzo MATCH ?1", ["lukasz"], |r| {
            r.get(0)
        })
        .expect("conteggio");
    assert_eq!(
        solo_unicode61, 0,
        "il tokenizzatore da solo non raggiunge il testo con le lettere barrate: è il motivo di `italian`"
    );
    // E il termine originale, invece, lo trova: la piegatura non perde il testo,
    // lo sposta su una forma raggiungibile.
    let con_accento: i64 = confronto
        .query_row("SELECT COUNT(*) FROM grezzo WHERE grezzo MATCH ?1", ["łukasz"], |r| {
            r.get(0)
        })
        .expect("conteggio");
    assert_eq!(con_accento, 1, "il testo originale resta indicizzabile");

    for query in ["Lukasz", "Kowalski", "Ostergaard"] {
        let trovati = s
            .store
            .search(&s.teacher, query, 10)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(trovati.len(), 1, "{query} non è raggiungibile");
    }
}

#[test]
fn gli_accenti_sono_piegati_da_entrambe_le_parti() {
    let mut s = School::new();
    indicizza(&mut s, 1, "Continuità", "Perché la funzione è continua.", contratto());

    for query in ["perché", "perche", "PERCHE", "Perché"] {
        let trovati = s
            .store
            .search(&s.teacher, query, 10)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(trovati.len(), 1, "{query} non piega");
    }
}

#[test]
fn non_c_e_radice_e_il_documento_lo_dice() {
    let mut s = School::new();
    indicizza(&mut s, 1, "Continuità", "Le chiavi e le equazioni.", contratto());

    assert_eq!(
        s.store
            .search(&s.teacher, "chiavi", 10)
            .expect("ricerca")
            .len(),
        1
    );
    // Nessuna radice: chi cerca deve pensare nella forma superficiale. È un
    // costo dichiarato, e questo test è la prova che è un costo e non una scelta
    // che si è fatta per caso.
    assert!(
        s.store.search(&s.teacher, "chiave", 10).expect("ricerca").is_empty(),
        "`chiave` trova `chiavi`: lo stemming è abilitato, che non è"
    );

    // E lo `stemmer` inglese non aiuta: misurato sulle stesse parole italiane,
    // `porter` dà lo stesso risultato di nessuno stemming. Un algoritmo
    // inglese applicato a una lingua che non è l'inglese non può fare da
    // confronto, e abilitarlo significherebbe solo perdere informazione.
    let con_porter = rusqlite::Connection::open_in_memory().expect("memoria");
    con_porter
        .execute_batch(
            "CREATE VIRTUAL TABLE porterizzato USING fts5(w, tokenize = \"porter unicode61\"); \
             INSERT INTO porterizzato(rowid, w) VALUES (1, 'Le chiavi e le equazioni.');",
        )
        .expect("indice");
    let trovate: i64 = con_porter
        .query_row("SELECT COUNT(*) FROM porterizzato WHERE porterizzato MATCH ?1", ["chiave"], |r| {
            r.get(0)
        })
        .expect("conteggio");
    assert_eq!(trovate, 0, "`porter` non stemmatizza l'italiano: è un altro motivo per non usarlo");
}

#[test]
fn piu_parole_sono_un_and_e_non_una_frase() {
    let mut s = School::new();
    indicizza(
        &mut s,
        1,
        "Continuità",
        "dati riservati personali degli studenti.",
        contratto(),
    );

    // `personali` e `dati` sono lontani e in ordine inverso nel testo. Se una
    // sequenza di parole fosse una frase, questa ricerca non troverebbe nulla.
    // Misura: `unicode61` mette un AND implicito, e quindi la trova. È la
    // differenza fra FTS5 e FTS3, ed è il motivo per cui la documentazione di
    // `italian` non promette una frase.
    assert_eq!(
        s.store
            .search(&s.teacher, "personali dati", 10)
            .expect("ricerca")
            .len(),
        1
    );
    let grezzo: i64 = s
        .store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM artifact_fts WHERE artifact_fts MATCH ?1",
            ["personali dati"],
            |r| r.get(0),
        )
        .expect("conteggio");
    assert_eq!(grezzo, 1, "l'AND implicito trova, l'ordine non conta");
}

#[test]
fn una_query_di_sola_puntegiatura_e_nessuna_richiesta() {
    let mut s = School::new();
    indicizza(&mut s, 1, "Continuità", "testo", contratto());
    // Una query FTS5 vuota è un errore di sintassi, e «nessun risultato» è una
    // risposta vera. Le due cose non si confondono.
    assert_eq!(
        s.store.search(&s.teacher, "  ,,. ***  ", 10).expect_err("query vuota").rule(),
        "search.empty"
    );
    assert!(italian::to_fts_query("  ,,. ").is_none());
}

#[test]
fn togliere_il_chunk_toglie_anche_il_risultato() {
    let mut s = School::new();
    let id = indicizza(&mut s, 1, "Continuità", "sale e scende", contratto());
    let chunk = s
        .store
        .index_chunk(&ArtifactChunk {
            argument: id.clone(),
            course: s.course.clone(),
            kind: ChunkKind::Section,
            ord: 1,
            rel_path: "corsi/analisi-1/lezione.html#limite".to_string(),
            title: "Limite".to_string(),
            body: "Il limite superiore non esiste.".to_string(),
            headings: String::new(),
            code: String::new(),
            prompt: String::new(),
            contract: String::new(),
            updated_at: Millis(1_700_000_000_000),
        })
        .expect("secondo chunk");
    assert_eq!(
        s.store
            .search(&s.teacher, "superiore", 10)
            .expect("ricerca")
            .len(),
        1
    );

    s.store.unindex_chunk(chunk).expect("rimozione");
    assert!(
        s.store
            .search(&s.teacher, "superiore", 10)
            .expect("ricerca")
            .is_empty(),
        "un indice che non dimentica non è un indice"
    );
    assert_eq!(
        s.store.unindex_argument(&id).expect("rimozione argomento"),
        1
    );
    assert!(s.store.search(&s.teacher, "continuità", 10).expect("ricerca").is_empty());
}

#[test]
fn reindicizzare_un_chunk_non_accumula_le_versioni() {
    let mut s = School::new();
    let id = indicizza(&mut s, 1, "Continuità", "prima versione del corpo", contratto());
    let righe = s
        .store
        .reindex_all()
        .expect("reindicizzazione");
    assert_eq!(righe, 1, "una riga indicizzata, non due");

    // Riscrivere lo stesso chunk `(argument, kind, ord)` non crea una seconda
    // riga e l'indice non restituisce più il testo vecchio.
    s.store
        .index_chunk(&ArtifactChunk {
            argument: id.clone(),
            course: s.course.clone(),
            kind: ChunkKind::Argument,
            ord: 0,
            rel_path: "corsi/analisi-1/lezione.html".to_string(),
            title: "Continuità".to_string(),
            body: "seconda versione del corpo".to_string(),
            headings: String::new(),
            code: String::new(),
            prompt: String::new(),
            contract: contratto().to_string(),
            updated_at: Millis(1_700_000_100_000),
        })
        .expect("riscrittura");
    assert!(s
        .store
        .search(&s.teacher, "prima versione", 10)
        .expect("ricerca")
        .is_empty());
    assert_eq!(
        s.store
            .search(&s.teacher, "seconda versione", 10)
            .expect("ricerca")
            .len(),
        1
    );
    assert_eq!(s.store.reindex_all().expect("reindicizzazione"), 1);
}

#[test]
fn reindex_all_ripara_un_chunk_scritto_fuori_dalla_strada() {
    // L'unico buco della sincronizzazione è `conn()`: ci si può scrivere un chunk
    // senza indicizzarlo. Il buco è dichiarato, e la riparazione è
    // `reindex_all`. Qui il buco è aperto per davvero e poi riparato.
    let mut s = School::new();
    let arg = s.published(1);
    s.store
        .conn()
        .execute(
            "INSERT INTO artifact_chunks (argument_id, course_id, kind, ord, rel_path, title, \
                                            body, headings, code, prompt, contract, updated_at) \
             VALUES (?1, ?2, 'argument', 9, 'corsi/analisi-1/fuori-strada.html', 'Fuori strada', \
                     'parola-segreta', '', '', '', '', 0)",
            rusqlite::params![arg.id.0, s.course.0],
        )
        .expect("chunk scritto a mano");
    assert!(
        s.store
            .search(&s.teacher, "segreta", 10)
            .expect("ricerca")
            .is_empty(),
        "un chunk non indicizzato non è ricercabile: è il buco dichiarato"
    );
    assert_eq!(s.store.reindex_all().expect("reindicizzazione"), 1);
    assert_eq!(
        s.store
            .search(&s.teacher, "segreta", 10)
            .expect("ricerca")
            .len(),
        1,
        "`reindex_all` rimette le cose a posto"
    );
}

#[test]
fn la_riga_memorizzata_non_e_piegata() {
    // La piegatura sta nell'indice, non nei dati: l'export di D12 deve poter
    // mandare `Perché` con l'accento, perché il materiale è italiano e
    // l'accento è il materiale.
    let mut s = School::new();
    let id = indicizza(&mut s, 1, "Continuità perché", "Senso compiuto", contratto());
    let title: String = s
        .store
        .conn()
        .query_row(
            "SELECT title FROM artifact_chunks WHERE argument_id = ?1",
            [&id.0],
            |r| r.get(0),
        )
        .expect("titolo");
    assert_eq!(title, "Continuità perché");

    // E `embedding` resta NULL dopo l'indicizzazione: è ciò che la migrazione
    // promette, e la promessa è verificabile solo se qualcuno la verifica.
    let embedding: Option<f64> = s
        .store
        .conn()
        .query_row(
            "SELECT embedding FROM artifact_chunks WHERE argument_id = ?1",
            [&id.0],
            |r| r.get(0),
        )
        .expect("embedding");
    assert!(
        embedding.is_none(),
        "nessun percorso di questo crate può scrivere un embedding (D3)"
    );
}

#[test]
fn il_risultato_e_ordinato_per_bm25_e_il_segno_e_negativo() {
    let mut s = School::new();
    indicizza(&mut s, 1, "Continuità", "una sola volta la parola rara", contratto());
    s.draft(3);
    let terzo = School::argument(
        &s.course,
        &s.teacher,
        4,
        kbs_core::PublicationState::Bozza,
    );
    s.store.upsert_argument(&terzo).expect("bozza");
    s.store
        .ratify(&terzo.id, &s.teacher, "ok")
        .expect("ratifica");
    s.store.publish(&terzo.id).expect("pubblicazione");
    s.store
        .index_chunk(&ArtifactChunk {
            argument: terzo.id.clone(),
            course: s.course.clone(),
            kind: ChunkKind::Argument,
            ord: 0,
            rel_path: "corsi/analisi-1/altra.html".to_string(),
            title: "Continuità".to_string(),
            body: "continuità".to_string(),
            headings: String::new(),
            code: String::new(),
            prompt: String::new(),
            contract: String::new(),
            updated_at: Millis(1_700_000_000_000),
        })
        .expect("indicizzazione");
    s.store
        .index_chunk(&ArtifactChunk {
            argument: terzo.id.clone(),
            course: s.course.clone(),
            kind: ChunkKind::Section,
            ord: 1,
            rel_path: "corsi/analisi-1/altra.html#1".to_string(),
            title: String::new(),
            body: "continuità".to_string(),
            headings: String::new(),
            code: String::new(),
            prompt: String::new(),
            contract: String::new(),
            updated_at: Millis(1_700_000_000_000),
        })
        .expect("secondo chunk");

    let trovati = s
        .store
        .search(&s.teacher, "continuità", 10)
        .expect("ricerca");
    assert!(trovati.len() >= 2);
    // Un argomento con più chunk non compare due volte, e i `rank` crescono.
    assert!(trovati.windows(2).all(|w| w[0].rank <= w[1].rank));
    assert!(trovati.iter().all(|h| h.rank < 0.0), "bm25 è negativo");
}
