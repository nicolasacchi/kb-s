-- V6: la colonna che distingue il sistema da una bottiglia con i fantasmi.
--
-- `unaided` è la distinzione fra «aiuto» e «apprendimento», resa colonna. Senza
-- di essa la tabella `observations` misura **interazione**: ogni riga che
-- esiste è una riga in cui lo studente ha lavorato, e «ha lavorato» non è
-- «sa». La claim del prodotto — *la quota di argomenti che passano da non
-- dimostrato a dimostrato* — con questa colonna è misurabile; senza, non lo è,
-- perché il numeratore conterrebbe la coda di practice.
--
-- ── Perché `unaided` è NULL e non `NOT NULL DEFAULT 1` ───────────────────────
--
-- La colonna è **senza `NOT NULL` e senza `DEFAULT`**, e questa è una scelta,
-- non una dimenticanza. Il motivo è che SQLite *non ammette*
-- `ALTER TABLE … ADD COLUMN x NOT NULL` se non si dichiara un `DEFAULT` diverso
-- da `NULL`: su una tabella con righe, `NOT NULL` **obbliga** a dichiarare in
-- anticipo il giudizio su tutte le righe già scritte. E per le righe già
-- scritte quel giudizio non esiste.
--
-- * `DEFAULT 1` dice che quegli studenti hanno risposto senza aiuto: una
--   padronanza che il sistema non ha mai misurato, dichiarata retroattivamente
--   da una migrazione. Il numero che ne esce è più alto per costruzione, e la
--   colonna che dovrebbe impedire di esagerare la padronanza la esagerebbe.
-- * `DEFAULT 0` dice il contrario — che ogni osservazione passata era assistita
--   — ed è una dichiarazione ugualmente infondata, solo meno comoda.
--
-- Il terzo modo è **ignoto**, ed è l'unico che non mente. `NULL` su `unaided`
-- vuol dire *non registrato*, non *no*: le righe precedenti a questa migrazione
-- sono state scritte da un sistema che non aveva la colonna e che non chiedeva
-- nulla in proposito, e nessun lettore può distinguerle da una misura.
--
-- **Il costo, dichiarato perché è reale.** Una riga con `unaided IS NULL`
--   * non compare nella vista `unaided_observations`, e quindi non la vede lo
--     studente né entra nel conteggio di coorte: il `NULL` esce fuori dal
--     predicato `unaided = 1` per la logica a tre valori, senza che nessun
--     codice lo decida esplicitamente — che è il comportamento che si vuole;
--   * non è contato in nessuna «quota di padronanza», e va detto al docente
--     che il numero che vede copre un sottoinsieme delle osservazioni. Il
--     `COUNT(*)` senza predicato le conterebbe, ed è per questo che nessuna
--     query di questo crate conta le osservazioni senza passare dalla vista;
--   * resta perfettamente valida per la catena di hash e per l'export: è una
--     riga, e la sua foglia cambia perché la forma canonica della riga è
--     cambiata. È il costo numero due qui sotto.
--
-- ── I due costi che questa migrazione dichiara da sé ─────────────────────────
--
-- 1. **La foglia di ogni riga precedente cambia.** `leaf_of` impegna il JSON
--    canonico della riga intera, quindi aggiungere due campi a
--    `Observation` cambia l'hash delle righe già scritte. Una voce del
--    testimone registrata **prima** di questa migrazione non torna più con la
--    testa ricalcolata: `kbs-verify` lo dice e non lo nasconde, il che è il
--    comportamento giusto per un limite dichiarato, ma una scuola che avesse un
--    testimone firmato deve saperlo prima di migrare. Non c'è modo di evitarlo
--    senza escludere i campi nuovi dalla foglia, e un campo escluso dalla
--    foglia è un campo che un riscrittore può cambiare senza pagarne: peggio
--    di una migrazione che cambia gli hash.
-- 2. **`n_hints` è un conteggio, e un conteggio che non è stato fatto non è
--    zero.** Per la stessa ragione di `unaided` la colonna è nullable, e il
--    trigger qui sotto vieta la combinazione ambigua: non si può dichiarare
--    quante piste erano disponibili per un'osservazione di cui non si sa se
--    qualche pista era disponibile. `n_hints = 0` significa «nessuna pista
--    disponibile», che è una misura; `n_hints = NULL` significa «non contato».
--
-- ── Perché il trigger e non un `CHECK` di tabella ────────────────────────────
--
-- Il divieto di cui sopra è un `CHECK` **di tabella**, perché riguarda due
-- colonne. SQLite non sa aggiungere un `CHECK` di tabella a una tabella che
-- esiste: l'unico modo è ricrearla (nuova tabella, copia, `DROP`, rinominare) —
-- e cioè **riscrivere il registro delle dimostrazioni**, che è la sola
-- operazione che D6 vieta in un modo che i trigger di `V3` non possono
-- fermare. Fra le due cose che si possono fare senza toccare le righe, il
-- `CHECK` per colonna dentro `ADD COLUMN` e il trigger `BEFORE INSERT`, c'è
-- il trigger, perché è il meccanismo che questa migrazione di famiglia usa già
-- per ogni invariante che non sta in una colonna, e perché il suo messaggio
-- può essere la frase che un revisore legge.

-- La colonna, e il suo `CHECK`: `unaided` è un enum di due valori e nient'altro.
-- `2` non è «meglio di `1»», è una terza cosa che nessuno sa cosa significhi,
-- e il `CHECK` la rifiuta per chi scrive SQL a mano come per `append_observation`.
ALTER TABLE observations ADD COLUMN unaided INTEGER CHECK (unaided IN (0,1));

-- Anche `n_hints` è un `CHECK` di colonna, e non un enum: «quante piste erano
-- disponibili» è un intero, e l'unico valore che non è una misura è il negativo.
-- Le due colonne stanno **in coda** alla riga, e quindi nell'`INSERT` di
-- `append_observation` stanno dopo `at`.
ALTER TABLE observations ADD COLUMN n_hints INTEGER CHECK (n_hints IS NULL OR n_hints >= 0);

-- L'aiuto ignoto non può avere un conteggio di indizi: sapere «quante piste
-- c'erano» presuppone sapere «se ce n'era qualcuna».
CREATE TRIGGER observations_unknown_aid_counts_no_hints
BEFORE INSERT ON observations
WHEN NEW.unaided IS NULL AND NEW.n_hints IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'unaided ignoto e n_hints contato: non si sa se gli indizi erano disponibili, e non si sa quanti erano');
END;

-- La regola di D5 per il registro delle dimostrazioni, nello schema e non in una
-- rotta: *lo studente vede solo le osservazioni non assistite, mai la coda di
-- practice; il resto è del docente*. Qui è una **vista**, e una vista viaggia
-- col file del database: resta in un dump e in un backup, e non può marcire
-- perché qualcuno ha dimenticato un `WHERE` in una rotta.
--
-- Non è un secondo meccanismo di visibilità: «chi può leggere» resta
-- `register_scope`, che è relazione (D5). Questa vista dice **quali righe
-- compongono la superficie non assistita**, e le due domande non si confondono.
-- Le colonne sono elencate una per una e non con `*`: la forma di una vista è
-- congelata alla creazione, ed elencarle è il modo perché una migrazione futura
-- che aggiunge una colonna non cambi di nascosto ciò che questa vista consegna.
CREATE VIEW unaided_observations AS
    SELECT id, session_id, seq, student, course_id, cohort, argument_id,
           evidence, evidence_payload, judged_by, at, unaided, n_hints
      FROM observations
     WHERE unaided = 1;

-- L'indice parziale del corpus: la superficie non assistita è quella che si
-- interroga di più, ed è piccola dentro un registro che non lo è. Un indice
-- parziale su `unaided = 1` indicizza le righe che la vista contiene e non
-- indizza le altre, e la sua condizione è il predicato della vista: le due
-- frasi sono la stessa, e qui sono scritte una volta sola anche loro.
CREATE INDEX observations_unaided_by_argument
    ON observations (argument_id, student) WHERE unaided = 1;

-- `MAX` e non assegnazione, per la ragione che `V3` dichiara: l'ordine con cui
-- refinery applica le migrazioni non è il loro numero.
UPDATE schema_epoch SET epoch = MAX(epoch, 6);
