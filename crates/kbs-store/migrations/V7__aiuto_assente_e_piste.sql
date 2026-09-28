-- V7: la direzione che mancava fra «aiuto assente» e «quante piste».
--
-- `V6` vieta una delle due incoerenze fra `unaided` e `n_hints` e non vieta
-- l'altra, e il motivo per cui la seconda è passata è che la prima aveva già
-- riempito il modulo: sapere che una guardia fra due colonne esiste fa credere
-- che copra la coppia.
--
-- I due `CHECK` di `V6` sono di colonna e non possono parlare fra loro, e il
-- trigger che copre una direzione è un trigger solo. Quindi la combinazione
-- speculare — dichiarare l'aiuto assente **e** contare le piste — non è vietata
-- dal tipo, né dai `CHECK`, né dal trigger: `ObservationDraft` ha i due campi
-- `pub` indipendenti e `append_observation` li passa indipendenti.
--
-- **Perché è un difetto e non una semantica.** Una riga che dichiara «nessun
-- aiuto disponibile» mentre conta tre piste è la claim del progetto — *la quota
-- di argomenti che passano da non dimostrato a dimostrato* — che si mette da
-- sola nel registro, e il registro è append-only (`V3`): la riga cattiva, una
-- volta dentro, non si riscrive. `kbs-verify` non la intercetta, perché la
-- catena impegna la riga com'è e non sa che le due colonne si contraddicono.
--
-- Il perimetro onesto: i due scrittori di prodotto non la producono
-- (`pratica.rs` deriva `unaided` da `n_hints`, `diagnosis.rs` scrive `None` e
-- `None`) e non esiste una rotta HTTP che la scriva. Chi la può scrivere è chi
-- scrive SQL a mano — ed è il caso per cui `V6:78-79` dice che i `CHECK` di
-- colonna ci sono: la guardia serve quando la dichiarazione non passa da
-- `append_observation`.
--
-- ── Perché `V7` e non `V6` riscritta ────────────────────────────────────────
--
-- La terza guardia di apertura di `schema.rs` dichiara che «le migrazioni
-- applicate non si riscrivono», e la fa rispettare il checksum di
-- `refinery_schema_history`: riscrivere `V6` fa smettere di aprire ogni
-- database che l'ha già applicata, e la correzione di un invariante non è un
-- motivo per cambiare il numero sotto una guardia che ne ha uno. La guardia si
-- alza quindi a 7, e l'unico posto in cui quel numero è scritto a mano è il
-- test che lo confronta — che è il motivo per cui quel test esiste.
--
-- ── La forma: un trigger di più, e non uno riscritto ────────────────────────
--
-- Il trigger di `V6` resta com'è: ha la sua direzione e il suo messaggio, e
-- un messaggio che descrive due rifiuti diversi non è più la frase che un
-- revisore legge. Qui sotto c'è quindi **un secondo trigger**, con la stessa
-- forma e la stessa promessa, e i due si leggono insieme.
--
-- La terza combinazione resta libera, e va detto perché: `unaided = 1` con
-- `n_hints IS NULL` è un docente che dichiara che non c'era aiuto e non ha
-- contato niente, che è un fatto e non una contraddizione — è la riga che
-- `la_riga_precedente_alla_colonna_non_diventa_una_misura` scrive. E
-- `unaided = 0` con `n_hints >= 1` è la riga che il corpus scrive: aiuto
-- dichiarato disponibile e piste contate, che è la direzione in cui le due
-- colonne si dicono la stessa cosa.
CREATE TRIGGER observations_unaided_never_counts_hints
BEFORE INSERT ON observations
WHEN NEW.unaided = 1 AND NEW.n_hints >= 1
BEGIN
    SELECT RAISE(ABORT, 'unaided dichiara l''aiuto assente e n_hints conta le piste: chi non ha avuto aiuto non può avere avuto piste');
END;

-- `MAX` e non assegnazione, per la ragione che `V6` dichiara: l'ordine con cui
-- refinery applica le migrazioni non è il loro numero.
UPDATE schema_epoch SET epoch = MAX(epoch, 7);
