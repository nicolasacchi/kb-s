-- L'epoch dello schema, e la guardia che lo usa.
--
-- Due log distinti, e non è una ridondanza:
--   * `refinery_schema_history` (creata da refinery) è il *log* delle migrazioni,
--     con nome e checksum: è ciò che rende impossibile riscrivere una migrazione
--     già applicata, perché il checksum non combacia più e refinery si rifiuta.
--   * `schema_epoch` è la *guardia* di `kbs-s`: un numero che il binario conosce
--     e confronta prima di fare qualsiasi cosa. Se il database è più nuovo del
--     binario, il binario non apre: un binario vecchio che scrive sopra uno schema
--     nuovo non è un rollback, è una corruzione silenziosa.
--
-- `id = 1` con CHECK: la tabella ha una riga sola, per costruzione e non per
-- convenzione dell'applicazione.

CREATE TABLE schema_epoch (
    id    INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    epoch INTEGER NOT NULL CHECK (epoch >= 1)
) STRICT;

INSERT INTO schema_epoch (id, epoch) VALUES (1, 1);
