-- Esercizi, istanze, e gli eventi di generazione.
--
-- D8: un esercizio è un generatore con seed e un checker, non un giudizio. Il
-- `UNIQUE (exercise, seed)` è la costruzione che rende la copia inefficace: due
-- istanze della stessa famiglia hanno lo stesso ragionamento e risposte diverse.

CREATE TABLE exercises (
    id                TEXT NOT NULL PRIMARY KEY,
    course_id         TEXT NOT NULL REFERENCES sources (id),
    argument_id       TEXT NOT NULL REFERENCES arguments (id),
    family            TEXT NOT NULL CHECK (length(trim(family)) > 0),
    generator_version TEXT NOT NULL CHECK (length(trim(generator_version)) > 0),
    prompt            TEXT NOT NULL CHECK (length(trim(prompt)) > 0),
    -- Il checker è un enum con quattro forme e quattro payload diversi. Il `kind`
    -- è una colonna per poterlo interrogare; il payload è JSON perché un insieme
    -- di elementi non è una colonna. Codificare `Set { elements }` in colonne
    -- nullable sarebbe una bugia sulla forma.
    checker         TEXT NOT NULL CHECK (checker IN ('numeric', 'set', 'multiple-choice', 'equivalence')),
    checker_payload TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    created_by   TEXT NOT NULL REFERENCES people (id)
) STRICT;

CREATE INDEX exercises_by_argument ON exercises (argument_id, family);

CREATE TABLE instances (
    id              TEXT NOT NULL PRIMARY KEY,
    exercise        TEXT NOT NULL REFERENCES exercises (id),
    -- Con `generator_version` dell'esercizio, il seed riproduce la risposta (D11).
    seed            TEXT NOT NULL CHECK (length(trim(seed)) > 0),
    rendered_prompt TEXT NOT NULL,
    expected        TEXT NOT NULL,
    params          TEXT NOT NULL,
    UNIQUE (exercise, seed)
) STRICT;

-- Un evento di generazione per volta (D10). La tabella dice *quando* e *per
-- quale argomento* un modello esterno ha prodotto materiale; il lock dice *con
-- che cosa*. Sono due domande diverse e quindi due tabelle: accorparle
-- cancellerebbe la possibilità di generare lo stesso contenuto due volte.
CREATE TABLE generations (
    id          INTEGER NOT NULL PRIMARY KEY,
    lock_id     TEXT NOT NULL REFERENCES model_locks (id),
    argument_id TEXT NOT NULL REFERENCES arguments (id),
    requester   TEXT NOT NULL REFERENCES people (id),
    at          INTEGER NOT NULL
) STRICT;

CREATE INDEX generations_by_argument ON generations (argument_id, at);

CREATE TRIGGER generations_no_update
BEFORE UPDATE ON generations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'un evento di generazione non si riscrive');
END;

CREATE TRIGGER generations_no_delete
BEFORE DELETE ON generations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'un evento di generazione non si cancella: senza il registro delle generazioni non c''è D10');
END;

-- D9: il cohorte invisibile.
--
-- La soglia è scritta qui, in un CHECK, e non solo nel codice: sotto questa
-- soglia il segnale di coorte non è aggregabile, e la riga non entra nel
-- database nemmeno se qualcuno scrive SQL a mano. `5` è
-- `kbs_core::COHORT_MIN_K`, e un test lo confronta: se la costante di dominio
-- cambiasse, la correzione è una migrazione nuova, perché una migrazione
-- applicata non si riscrive.
--
-- La soglia è un accesso, non una cancellazione: le osservazioni individuali
-- restano dove sono, e sono loro che si contano.
CREATE TABLE cohort_signals (
    course_id   TEXT NOT NULL REFERENCES sources (id),
    cohort      TEXT NOT NULL CHECK (length(trim(cohort)) > 0),
    argument_id TEXT NOT NULL REFERENCES arguments (id),
    failing     INTEGER NOT NULL CHECK (failing >= 0),
    total       INTEGER NOT NULL CHECK (total >= 0),
    at          INTEGER NOT NULL,
    PRIMARY KEY (course_id, cohort, argument_id),
    CHECK (failing <= total),
    CHECK (failing >= 5)
) STRICT;

CREATE INDEX cohort_signals_by_argument ON cohort_signals (argument_id);

UPDATE schema_epoch SET epoch = MAX(epoch, 4);
