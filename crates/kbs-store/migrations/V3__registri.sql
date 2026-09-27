-- I tre registri di D6, e la sessione che dà significato a `SeqInSession`.
--
-- La regola che governa tutta questa migrazione: **una riga di registro non si
-- riscrive e non si cancella**. Non è una convenzione dell'applicazione, sono
-- trigger: valgono per chi scrive SQL a mano e per ogni percorso di questo
-- crate, e nessuno dei due può aggirarli.
--
-- Il limite, dichiarato perché è il primo che si cerca: questi trigger
-- proteggono da una riscrittura *per iscritto*. Non proteggono da un ripristino
-- da backup, da un `DROP TABLE` ricreato, né da un file copiato sopra: quelli
-- producono un database conforme, e sono indistinguibili da una riscrittura
-- senza una copia indipendente. È il secondo dei tre limiti di D6, ed è il
-- motivo per cui esiste `kbs-verify`.
--
-- L'unica eccezione alle regole di questo file è lo stato della contestazione,
-- che è un fatto *nuovo* e non una riscrittura del giudizio.

-- La sessione che dà significato a `SeqInSession`.
--
-- `SeqInSession` è un ordinale *dentro una sessione*, non un id di riga: due
-- osservazioni con seq 1 in due sessioni diverse non si contraddicono, e senza
-- questa tabella quel numero non significherebbe niente. Un registro senza
-- sessione è un diario.
CREATE TABLE write_sessions (
    id        TEXT NOT NULL PRIMARY KEY,
    register  TEXT NOT NULL CHECK (register IN ('observations', 'gradings')),
    opened_at INTEGER NOT NULL,
    sealed_at INTEGER,
    note      TEXT NOT NULL,
    CHECK (sealed_at IS NULL OR sealed_at >= opened_at)
) STRICT;

-- Un voto senza rubric non è riproducibile (D6): `grading.grade` è una stringa
-- e una stringa senza la scala che le dà significato è un numero senza unità.
--
-- La scala sta qui e non nel giudizio, così due versioni non possono condividere
-- una scala senza perdere il significato.
CREATE TABLE rubrics (
    id         TEXT NOT NULL PRIMARY KEY,
    course_id  TEXT NOT NULL REFERENCES sources (id),
    title      TEXT NOT NULL CHECK (length(trim(title)) > 0),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE rubric_versions (
    id         TEXT NOT NULL PRIMARY KEY,
    rubric_id  TEXT NOT NULL REFERENCES rubrics (id),
    version    TEXT NOT NULL CHECK (length(trim(version)) > 0),
    -- JSON: la scala è una lista di livelli, e una lista non è una colonna.
    scale      TEXT NOT NULL,
    defined_at INTEGER NOT NULL,
    note       TEXT NOT NULL,
    UNIQUE (rubric_id, version)
) STRICT;

-- ── claims: «questa frase è vera?» ──────────────────────────────────────────
CREATE TABLE claims (
    id          TEXT NOT NULL PRIMARY KEY,
    course_id   TEXT NOT NULL REFERENCES sources (id),
    argument_id TEXT NOT NULL REFERENCES arguments (id),
    text        TEXT NOT NULL CHECK (length(trim(text)) > 0),
    -- Lo span che la sostiene. `span_text` non è un extra: senza il testo
    -- effettivo `span_anchor` è un indirizzo che il lettore non può verificare,
    -- e l'affermazione torna a essere una dichiarazione con un indirizzo.
    span_anchor TEXT,
    span_text   TEXT,
    status      TEXT NOT NULL CHECK (status IN ('supported', 'contradicted', 'unciteable', 'retracted')),
    status_reason TEXT,
    emitted_at  INTEGER NOT NULL,
    emitter     TEXT NOT NULL CHECK (emitter IN ('teacher', 'content', 'from-work')),
    emitter_by  TEXT REFERENCES people (id),
    emitter_argument TEXT REFERENCES arguments (id),
    emitter_observation TEXT,

    -- Lo span e il suo testo vanno e vengono insieme.
    CHECK (span_anchor IS NULL OR span_text IS NOT NULL),
    -- `retracted` è l'unico stato che porta una ragione, e la ragione non è
    -- opzionale: un ritratto senza motivo è una cancellazione travestita.
    CHECK ((status = 'retracted') = (status_reason IS NOT NULL)),
    CHECK (
        (emitter = 'teacher' AND emitter_by IS NOT NULL
            AND emitter_argument IS NULL AND emitter_observation IS NULL)
     OR (emitter = 'content' AND emitter_by IS NULL
            AND emitter_argument IS NOT NULL AND emitter_observation IS NULL)
     OR (emitter = 'from-work' AND emitter_by IS NULL
            AND emitter_argument IS NULL AND emitter_observation IS NOT NULL)
    )
) STRICT;

CREATE INDEX claims_by_argument ON claims (argument_id, status);

-- Una claim non si riscrive: cambia solo il suo stato. Il resto della riga è
-- congelato, perché «l'affermazione» è la cosa che è stata affermata.
CREATE TRIGGER claims_status_is_the_only_change
BEFORE UPDATE ON claims
FOR EACH ROW WHEN
       NEW.id IS NOT OLD.id
    OR NEW.course_id IS NOT OLD.course_id
    OR NEW.argument_id IS NOT OLD.argument_id
    OR NEW.text IS NOT OLD.text
    OR NEW.span_anchor IS NOT OLD.span_anchor
    OR NEW.span_text IS NOT OLD.span_text
    OR NEW.emitted_at IS NOT OLD.emitted_at
    OR NEW.emitter IS NOT OLD.emitter
    OR NEW.emitter_by IS NOT OLD.emitter_by
    OR NEW.emitter_argument IS NOT OLD.emitter_argument
    OR NEW.emitter_observation IS NOT OLD.emitter_observation
BEGIN
    SELECT RAISE(ABORT, 'una claim non si riscrive: dell''affermazione cambia solo lo stato');
END;

CREATE TRIGGER claims_no_delete
BEFORE DELETE ON claims
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'una claim non si cancella: un registro che cancella i propri errori non è un registro');
END;

-- ── observations: «chi ha dimostrato che cosa?» ──────────────────────────────
CREATE TABLE observations (
    id          TEXT NOT NULL PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES write_sessions (id),
    -- La quantità che la catena di hash di D6 ordina.
    seq         INTEGER NOT NULL CHECK (seq >= 1),
    student     TEXT NOT NULL REFERENCES people (id),
    course_id   TEXT NOT NULL REFERENCES sources (id),
    -- Una classe in un anno. È un'etichetta della scuola, non un oggetto di
    -- kb-s: nessuna tabella, nessun ruolo, nessun permesso.
    cohort      TEXT NOT NULL CHECK (length(trim(cohort)) > 0),
    argument_id TEXT NOT NULL REFERENCES arguments (id),
    -- La prova è un enum con obblighi diversi: `checked` è riproducibile, `oral`
    -- non lo è, e il registro deve poter dire quale delle due è.
    evidence     TEXT NOT NULL CHECK (evidence IN ('checked', 'oral', 'written', 'none')),
    evidence_payload TEXT,
    judged_by    TEXT CHECK (judged_by IN ('deterministic', 'peer', 'human', 'teacher')),
    at           INTEGER NOT NULL,
    UNIQUE (session_id, seq),
    CHECK ((evidence = 'none') = (evidence_payload IS NULL))
) STRICT;

CREATE INDEX observations_by_argument ON observations (argument_id, student);
CREATE INDEX observations_by_student ON observations (student, argument_id);

CREATE TRIGGER observations_no_update
BEFORE UPDATE ON observations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'observations è append-only: una dimostrazione non si riscrive');
END;

CREATE TRIGGER observations_no_delete
BEFORE DELETE ON observations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'observations è append-only: una dimostrazione non si cancella');
END;

-- ── gradings: «chi ha deciso, con che cosa, e qualcuno ha contestato?» ───────
CREATE TABLE gradings (
    id          TEXT NOT NULL PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES write_sessions (id),
    seq         INTEGER NOT NULL CHECK (seq >= 1),
    student     TEXT NOT NULL REFERENCES people (id),
    course_id   TEXT NOT NULL REFERENCES sources (id),
    argument_id TEXT NOT NULL REFERENCES arguments (id),
    kind        TEXT NOT NULL CHECK (kind IN ('deterministic', 'peer', 'human', 'teacher')),
    graded_by   TEXT NOT NULL REFERENCES people (id),
    -- Foreign key, non una convenzione: un giudizio con una rubric che non esiste
    -- non entra nel database.
    rubric_version TEXT NOT NULL REFERENCES rubric_versions (id),
    grade      TEXT NOT NULL CHECK (length(trim(grade)) > 0),
    at         INTEGER NOT NULL,

    -- La contestazione sta *dentro* la riga e non cancella la riga: la difesa
    -- procedurale primaria è annotare, non sostituire. È una sola riga, e la si
    -- legge con la stessa SELECT del giudizio: due query che possono disaccordarsi
    -- sono due fonti di verità.
    contested_by     TEXT REFERENCES people (id),
    contested_at     INTEGER,
    contested_reason TEXT,
    contested_outcome TEXT CHECK (contested_outcome IN ('upheld', 'rejected', 'under-review')),

    UNIQUE (session_id, seq),
    -- `outcome` può mancare: una contestazione aperta non ha ancora esito.
    -- Identità e motivo invece no.
    CHECK (
        (contested_at IS NULL) = (contested_by IS NULL)
    AND (contested_at IS NULL) = (contested_reason IS NULL)
    )
) STRICT;

CREATE INDEX gradings_by_argument ON gradings (argument_id, student);
CREATE INDEX gradings_by_student ON gradings (student, argument_id);
CREATE INDEX gradings_contested ON gradings (argument_id) WHERE contested_at IS NOT NULL;

-- Il giudizio è congelato; la contestazione è viva. Sono due fatti di età
-- diverse, e il trigger è il posto in cui la loro età è scritta.
CREATE TRIGGER gradings_judgement_is_frozen
BEFORE UPDATE ON gradings
FOR EACH ROW WHEN
       NEW.id IS NOT OLD.id
    OR NEW.session_id IS NOT OLD.session_id
    OR NEW.seq IS NOT OLD.seq
    OR NEW.student IS NOT OLD.student
    OR NEW.course_id IS NOT OLD.course_id
    OR NEW.argument_id IS NOT OLD.argument_id
    OR NEW.kind IS NOT OLD.kind
    OR NEW.graded_by IS NOT OLD.graded_by
    OR NEW.rubric_version IS NOT OLD.rubric_version
    OR NEW.grade IS NOT OLD.grade
    OR NEW.at IS NOT OLD.at
BEGIN
    SELECT RAISE(ABORT, 'il giudizio è congelato: si annota la contestazione, non si riscrive il voto');
END;

CREATE TRIGGER gradings_no_delete
BEFORE DELETE ON gradings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'gradings è append-only: un giudizio non si cancella, si contesta');
END;

UPDATE schema_epoch SET epoch = MAX(epoch, 3);
