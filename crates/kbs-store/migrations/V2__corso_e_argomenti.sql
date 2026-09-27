-- Il perimetro: un corso, le persone, e l'argomento come soggetto insegnabile.
--
-- Tutte le tabelle sono `STRICT`: SQLite rifiuta un valore di tipo sbagliato
-- invece di fare una conversione silenziosa. In un registro append-only la
-- coerenza dei tipi non è una questione di stile.

-- Il model lock (D10) sta per primo perché `arguments.origin_lock_id` lo
-- referenzia. È append-only: un lock applicato non si riscrive, e non si cancella.
CREATE TABLE model_locks (
    id                TEXT NOT NULL PRIMARY KEY,
    model_id          TEXT NOT NULL CHECK (length(trim(model_id)) > 0),
    prompt_hash       TEXT NOT NULL,
    corpus_hash       TEXT NOT NULL,
    generator_version TEXT NOT NULL,
    at                INTEGER NOT NULL,
    -- Due lock diversi non possono descrivere la stessa generazione: l'id è
    -- derivato da queste quattro colonne, e la UNIQUE rende impossibile che la
    -- derivazione produca due righe.
    UNIQUE (model_id, prompt_hash, corpus_hash, generator_version)
) STRICT;

CREATE TRIGGER model_locks_no_update
BEFORE UPDATE ON model_locks
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'un model lock non si riscrive: D10 richiede che la generazione resti riproducibile');
END;

CREATE TRIGGER model_locks_no_delete
BEFORE DELETE ON model_locks
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'un model lock non si cancella: senza lock non c''è un registro, c''è un diario');
END;

-- `sources` è l'albero del corso su disco: il prodotto possiede i file (D12),
-- questa tabella dice solo dove sono e a che stato è.
CREATE TABLE sources (
    id           TEXT NOT NULL PRIMARY KEY,
    slug         TEXT NOT NULL UNIQUE CHECK (length(trim(slug)) > 0),
    rel_path     TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('active', 'archived')),
    registered_at INTEGER NOT NULL,
    last_scan_at  INTEGER,
    corpus_hash   TEXT,
    CHECK (rel_path NOT LIKE '/%' AND instr(rel_path, '..') = 0 AND instr(rel_path, '\') = 0)
) STRICT;

-- Una persona non ha ruoli (D5). `display_name` è per l'operatore; nessuna
-- decisione di visibilità legge questa tabella.
CREATE TABLE people (
    id           TEXT NOT NULL PRIMARY KEY,
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    created_at   INTEGER NOT NULL
) STRICT;

-- Le relazioni di corso, e solo quelle.
--
-- `author_of` e `ratified` non stanno qui: sono fatti *sull'argomento* e hanno
-- già una colonna (`origin_by`, `ratified_by`). Copiarli in una tabella
-- creerebbe una seconda fonte per un fatto che ne ha già una, e le due
-- divergerebbero. `speculative_for` è percorso per-studente: non è condiviso e
-- non entra in `arguments`.
--
-- La PK comprende `since` perché una relazione si può riprendere: iscritto,
-- fuori, iscritto di nuovo sono tre fatti, non uno. "Ora è iscritto" è
-- `until IS NULL`.
CREATE TABLE relations (
    person_id TEXT NOT NULL REFERENCES people (id),
    course_id TEXT NOT NULL REFERENCES sources (id),
    relation  TEXT NOT NULL CHECK (relation IN ('enrolled_in', 'teaches')),
    since     INTEGER NOT NULL,
    until     INTEGER,
    PRIMARY KEY (person_id, course_id, relation, since),
    CHECK (until IS NULL OR until >= since)
) STRICT;

CREATE INDEX relations_by_course ON relations (course_id, relation, until);

-- `kbs_core::Argument` denormalizzato.
--
-- `origin` è una variante, e una variante si scrive come la variante: `kind`
-- dice quale, e il CHECK dice che le colonne appartenenti alle altre varianti
-- sono NULL. Una riga che mente sul proprio kind non può esistere.
CREATE TABLE arguments (
    id           TEXT NOT NULL PRIMARY KEY,
    course_id    TEXT NOT NULL REFERENCES sources (id),
    title        TEXT NOT NULL CHECK (length(trim(title)) > 0),
    summary      TEXT NOT NULL,
    state        TEXT NOT NULL CHECK (state IN ('bozza', 'del-docente', 'in-corso', 'archiviato')),
    rel_path     TEXT,
    content_hash TEXT NOT NULL,

    origin_kind   TEXT NOT NULL CHECK (origin_kind IN ('human', 'generated', 'derived')),
    origin_by     TEXT REFERENCES people (id),
    origin_at     INTEGER,
    origin_lock   TEXT REFERENCES model_locks (id),
    origin_from   TEXT REFERENCES arguments (id),

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    ratified_by           TEXT REFERENCES people (id),
    ratified_at           INTEGER,
    ratified_contract_hash TEXT,
    ratified_note         TEXT,

    CHECK (
        (origin_kind = 'human'
            AND origin_by IS NOT NULL AND origin_at IS NOT NULL
            AND origin_lock IS NULL AND origin_from IS NULL)
     OR (origin_kind = 'generated'
            AND origin_by IS NOT NULL AND origin_at IS NOT NULL
            AND origin_lock IS NOT NULL AND origin_from IS NULL)
     OR (origin_kind = 'derived'
            AND origin_by IS NULL AND origin_at IS NOT NULL
            AND origin_lock IS NULL AND origin_from IS NOT NULL)
    ),
    -- La ratifica è un blocco: o c'è tutto, o non c'è niente. Una ratifica a
    -- metà non è una ratifica provvisoria, è una riga malformata.
    CHECK (
        (ratified_by IS NULL AND ratified_at IS NULL
            AND ratified_contract_hash IS NULL AND ratified_note IS NULL)
     OR (ratified_by IS NOT NULL AND ratified_at IS NOT NULL
            AND ratified_contract_hash IS NOT NULL)
    ),
    CHECK (updated_at >= created_at),
    -- Un percorso relativo: niente assoluti, niente risalita. L'id dell'argomento
    -- deriva da questo percorso, e un percorso che esce dall'albero non può
    -- derivarne un id.
    CHECK (rel_path IS NULL OR (
        rel_path NOT LIKE '/%'
        AND instr(rel_path, '..') = 0
        AND instr(rel_path, '\') = 0
    )),
    -- Derivarsi da sé stessi non ha senso e romperebbe il cammino di provenienza.
    CHECK (origin_kind <> 'derived' OR origin_from <> id)
) STRICT;

CREATE INDEX arguments_by_state ON arguments (course_id, state);
CREATE INDEX arguments_by_content_hash ON arguments (content_hash);
CREATE INDEX arguments_by_origin_author ON arguments (origin_by);

-- Il grafo dei prerequisiti. Un arco è (argomento, prerequisito).
--
-- L'aciclicità non è qui: SQLite non ha la ricorsione in un CHECK, e una
-- ricorsione fatta a mano sarebbe una seconda implementazione. Il ciclo si
-- rifiuta in `kbs-store` con `kbs_core::Invariant::PrerequisiteCycle`, che è il
-- posto dove la regola è scritta una volta sola.
--
-- L'auto-loop invece è qui, perché è gratis e non ha eccezioni: è un ciclo.
CREATE TABLE argument_prerequisites (
    argument_id    TEXT NOT NULL REFERENCES arguments (id),
    prerequisite_id TEXT NOT NULL REFERENCES arguments (id),
    PRIMARY KEY (argument_id, prerequisite_id),
    CHECK (argument_id <> prerequisite_id)
) STRICT;

CREATE INDEX prerequisites_reverse ON argument_prerequisites (prerequisite_id);

UPDATE schema_epoch SET epoch = MAX(epoch, 2);
