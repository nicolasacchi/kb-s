-- I chunk indicizzati e l'indice lessicale.
--
-- `kb` indicizza `title, body, headings, code, prompt`. `kb-s` indicizza quegli
-- stessi campi **più `contract`**, perché il contratto didattico non è
-- un allegato: è la parte del materiale che dice che cosa è lecito fare
-- (D7, D15.1).

CREATE TABLE artifact_chunks (
    id          INTEGER NOT NULL PRIMARY KEY,
    argument_id TEXT NOT NULL REFERENCES arguments (id),
    course_id   TEXT NOT NULL REFERENCES sources (id),
    kind        TEXT NOT NULL CHECK (kind IN ('argument', 'section', 'contract', 'exercise')),
    ord         INTEGER NOT NULL CHECK (ord >= 0),
    rel_path    TEXT NOT NULL,
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    headings    TEXT NOT NULL,
    code        TEXT NOT NULL,
    prompt      TEXT NOT NULL,
    contract    TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    -- `embedding` è la promessa di D2 resa esplicita.
    --
    -- Perché la colonna esiste: D2 dichiara che se un domani serve la ricerca
    -- semantica, si aggiunge un embedder, e allora si aggiunge. Con la colonna
    -- l'aggiunta è una migrazione e un `Provider`; senza, è un rifacimento
    -- dell'indice e di ogni lettura che lo attraversa.
    --
    -- Perché è NULL in questa release: nessun modello gira dentro il prodotto
    -- (D3), quindi in *questo* repository non esiste alcun percorso di scrittura
    -- che la riempia. Non è un campo «non ancora compilato»: è un campo che
    -- nessuno scrive, e un test verifica che resti NULL dopo ogni scrittura che
    -- questo crate espone. Riempirlo richiede un embedder *fuori* dal prodotto,
    -- che è D10, non una modifica a questo schema.
    embedding REAL NULL,
    UNIQUE (argument_id, kind, ord)
) STRICT;

CREATE INDEX artifact_chunks_by_argument ON artifact_chunks (argument_id);

-- L'indice lessicale.
--
-- `content = ''` (contentless) è una scelta, non una dimenticanza. Il tokenizzatore
-- `unicode61` piega `perché` in `perche` ma **non** piega `ł ø ß æ œ`: sono
-- caratteri di token e restano tali, quindi un documento con `Łukasz` non è
-- raggiungibile da `lukasz`. La piegatura giusta la fa `kbs_store::italian`, in
-- Rust; ma se l'indice si alimentasse dalle colonne grezze, indice e interrogazione
-- piegherebbero in modo diverso e la ricerca mancherebbe proprio dove il testo
-- è europeo. Indicizzando il testo **già piegato**, la piegatura del tokenizzatore
-- non fa nulla (l'ha già fatta `italian`) e i due lati coincidono per costruzione.
--
-- Il prezzo di questa scelta, dichiarato: l'indice non ha contenuto, quindi
-- `snippet()` e `highlight()` non sono disponibili, e la sincronizzazione è
-- codice Rust (`index_chunk`, `unindex_chunk`, `reindex_all`) e non un trigger.
-- Il vantaggio è che l'indice non può divergere dal predicato di visibilità:
-- è lo stesso codice che scrive la riga a fare la query.
--
-- `contentless_delete = 1` (SQLite ≥ 3.43) fa sì che `DELETE` sia un DELETE.
CREATE VIRTUAL TABLE artifact_fts USING fts5 (
    title,
    body,
    headings,
    code,
    prompt,
    contract,
    content = '',
    contentless_delete = 1,
    tokenize = "unicode61 remove_diacritics 2"
);

UPDATE schema_epoch SET epoch = MAX(epoch, 5);
