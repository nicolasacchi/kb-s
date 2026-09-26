//! Il contratto del crate, provato dall'esterno.
//!
//! I test dentro `src/` provano i pezzi. Questi provano **la promessa**, e la
//! promettono dall'unico punto di vista che un utente ha: l'API pubblica. Ogni
//! test qui sotto è scritto perché fallisca se la regola che dichiara è
//! rimossa, e non perché continui a passare se la regola è sostituita da un
//!'altra.

use kbs_core::{
    ArgumentId, CohortId, CourseId, Evidence, GraderKind, Instance, Millis, Observation, PersonId,
    SeqInSession,
};
use kbs_verify::{
    CanonicalError, Chain, ChainExport, ChainVerdict, Coherence, ConsistencyProof, ExportRow,
    GeneratorKey, Hash, InstanceGenerator, LimitId, Limits, ReplayError, ReplayField, ReplayKey,
    ReplayOutcome, ReplayRecord, SegmentPlan, SessionId, Verified, VerifyRequest, Witness,
    WitnessEntry, WitnessError, canonicalize_str, leaf_of_value, replay, verify,
};
use serde_json::json;

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
            instance: format!("inst-{tag}-{seq}"),
            correct: seq % 2 == 0,
        },
        judged_by: Some(GraderKind::Deterministic),
        at: Millis(1_700_000_000_000 + seq as i64),
    }
}

/// `n` righe della stessa sessione, con le righe marcate da `tag`.
fn rows(n: u64, tag: &str) -> Vec<Observation> {
    (0..n).map(|i| obs(i, tag)).collect()
}

fn chain(session: &str, r: &[Observation]) -> Chain {
    Chain::build(&SessionId::new(session), r, SegmentPlan::every(4)).unwrap()
}

// ── D6 · la catena di hash ──────────────────────────────────────────────────

#[test]
fn due_sessioni_diverse_con_le_stesse_righe_hanno_la_stessa_testa() {
    let r = rows(11, "a");
    let a = chain("sessione-di-martedì", &r);
    let b = chain("sessione-di-giovedì", &r);
    assert_eq!(a.head(), b.head());
    assert_ne!(a.session(), b.session());
}

#[test]
fn cambiare_una_riga_qualunque_cambia_la_testa() {
    let base = rows(11, "a");
    let head = chain("s", &base).head();
    for at in [0usize, 1, 5, 10] {
        let mut changed = base.clone();
        changed[at].at = Millis(1);
        assert_ne!(
            chain("s", &changed).head(),
            head,
            "cambiare la riga {at} deve cambiare la testa"
        );
    }
}

#[test]
fn l_ordine_e_impegnato_dalla_catena() {
    let base = rows(11, "a");
    let head = chain("s", &base).head();
    let mut swapped = base.clone();
    swapped.swap(4, 5);
    // le seq seguono le righe: l'ordine che la catena impegna è quello delle
    // posizioni, non quello in cui le righe sono state scritte
    swapped[4].seq = SeqInSession(4);
    swapped[5].seq = SeqInSession(5);
    assert_ne!(chain("s", &swapped).head(), head);
}

#[test]
fn la_prova_di_coerenza_verifica_da_sola() {
    let c = chain("s", &rows(11, "a"));
    for p in c.consistency_proofs().unwrap() {
        assert_eq!(p.check().unwrap(), c.head());
        assert_eq!(c.check(&p).unwrap(), c.head());
    }
}

// ── il canonico ─────────────────────────────────────────────────────────────

#[test]
fn lo_stesso_documento_con_chiavi_in_ordine_diverso_ha_la_stessa_foglia() {
    // Due testi diversi che descrivono lo stesso documento. La forma canonica
    // deve coincidere: se non coincidesse, la stessa riga avrebbe due foglie
    // e la catena non saprebbe quale delle due è vera.
    let uno = r#"{"argument":"arg_x","at":0,"id":"obs-1","judged_by":null}"#;
    let due = r#"{"judged_by":null,"id":"obs-1","at":0,"argument":"arg_x"}"#;
    assert_eq!(canonicalize_str(uno).unwrap(), canonicalize_str(due).unwrap());
    assert_eq!(
        leaf_of_value(&json!({"id": "obs-1", "at": 0, "judged_by": null})).unwrap(),
        leaf_of_value(&json!({"judged_by": null, "at": 0, "id": "obs-1"})).unwrap()
    );
    // e l'ordine finale è quello dichiarato dalla regola, non quello della
    // scrittura
    assert_eq!(
        canonicalize_str(uno).unwrap(),
        r#"{"argument":"arg_x","at":0,"id":"obs-1","judged_by":null}"#
    );
}

#[test]
fn la_foglia_di_una_riga_e_fissa_e_non_dipende_dall_ordine_delle_campi() {
    // La stessa riga di dominio, serializzata in due modi diversi, produce la
    // stessa foglia: `canonicalize_str` mette in ordine, e la catena firma
    // quello che ne esce.
    let r = obs(0, "a");
    let testo = serde_json::to_string(&r).unwrap();
    let r2: Observation = serde_json::from_str(&testo).unwrap();
    assert_eq!(r, r2);
    assert_eq!(
        chain("s", std::slice::from_ref(&r)).head(),
        chain("s", std::slice::from_ref(&r2)).head()
    );
}

#[test]
fn il_canonico_distingue_null_dall_assente() {
    assert_ne!(canonicalize_str(r#"{"a":null}"#).unwrap(), canonicalize_str("{}").unwrap());
    assert_ne!(
        leaf_of_value(&json!({"judged_by": serde_json::Value::Null})).unwrap(),
        leaf_of_value(&json!({})).unwrap()
    );
    // due righe di dominio che differiscono solo per un `null` esplicito
    let senza = obs(0, "a");
    let mut con = obs(0, "a");
    con.evidence = Evidence::None;
    assert_ne!(chain("s", &[senza]).head(), chain("s", &[con]).head());
}

#[test]
fn il_canonico_rifiuta_un_documento_troppo_profondo_e_un_json_rotto() {
    let mut profondo = json!(1);
    for _ in 0..200 {
        profondo = json!([profondo]);
    }
    assert!(matches!(
        leaf_of_value(&profondo),
        Err(CanonicalError::TooDeep { .. })
    ));
    assert!(matches!(
        canonicalize_str("{"),
        Err(CanonicalError::Json(_))
    ));
}

// ── i tre limiti, nel valore restituito ──────────────────────────────────────

/// Il test che fallisce se qualcuno toglie un limite dal valore che `verify`
/// restituisce. Non è un test che passa: è una guardia.
#[test]
fn il_valore_restituito_da_verify_porta_i_tre_limiti() {
    let r = rows(6, "a");
    let sessione = SessionId::new("s");
    let v = verify(VerifyRequest::new(&sessione, &r)).unwrap();

    let limits = v.limits();
    assert_eq!(limits.len(), 3);
    let ids: Vec<LimitId> = limits.iter().map(|l| l.id).collect();
    assert_eq!(
        ids,
        vec![LimitId::Rewrite, LimitId::Rollback, LimitId::Truth],
        "i tre limiti devono essere dichiarati, nell'ordine della dichiarazione"
    );
    for l in limits.iter() {
        assert!(!l.statement.is_empty(), "il limite {} non dichiara niente", l.id);
    }
    // il testo stampato contiene i tre enunciati: stampare il verdetto stampa
    // anche quello che non è garantito
    let testo = v.to_string();
    for l in limits.iter() {
        assert!(
            testo.contains(l.statement),
            "il limite {} non viaggia con il verdetto",
            l.id
        );
    }
    // e non si può prendere il verdetto senza
    let (verdetto, limiti) = v.into_parts();
    let _: ChainVerdict = verdetto;
    assert_eq!(limiti, Limits::ALL);
}

/// Il primo limite, provato: una catena riscritta da capo verifica come
/// coerente, e l'API lo dice.
#[test]
fn una_catena_riscritta_da_capo_verifica_coerente_e_l_api_lo_dice() {
    let onesta = rows(6, "a");
    let riscritta = rows(6, "b");
    let sessione = SessionId::new("s");

    let v = verify(VerifyRequest::new(&sessione, &riscritta)).unwrap();
    assert_eq!(v.verdict().coherence, Coherence::Coherent { anchored: false });
    assert!(v.verdict().coherence.is_coherent());
    assert!(!v.verdict().coherence.is_anchored());

    // l'API non dice «valido»: dice «coerente», e dice che nessuno di
    // indipendente l'ha confermata
    let testo = format!("{}", v.verdict().coherence);
    assert!(testo.contains("coerente"), "{testo}");
    assert!(testo.contains("non ancorata"), "{testo}");
    assert!(!testo.contains("valid"), "l'API promette più di quanto verifica: {testo}");

    // e la stessa catena, davanti a una copia indipendente, non è più
    // coerente: è il limite che ha un rimedio
    let mut testimone = Witness::new();
    testimone
        .observe(&chain("s", &onesta))
        .expect("il testimone registra la testa che ha visto");
    let con_testimone = verify(
        VerifyRequest::new(&sessione, &riscritta).with_witness(&testimone),
    )
    .unwrap();
    assert!(con_testimone.verdict().coherence.is_contradiction());
    assert_eq!(
        con_testimone.verdict().coherence,
        Coherence::ContradictsWitness {
            presented: chain("s", &riscritta).head(),
            presented_rows: 6,
            witnessed: chain("s", &onesta).head(),
            witnessed_rows: 6,
        }
    );
}

/// Il secondo limite, provato: il rollback si vede solo se c'è la copia
/// indipendente che cresce.
#[test]
fn un_rollback_si_vede_solo_con_una_copia_indipendente() {
    let sessione = SessionId::new("s");
    let corta = chain("s", &rows(3, "a"));
    let lunga = chain("s", &rows(9, "a"));
    let mut testimone = Witness::new();
    testimone.observe(&corta).unwrap();
    testimone.observe(&lunga).unwrap();

    // senza testimone, il registro più corto è coerente come qualunque altro
    let senza = verify(VerifyRequest::new(&sessione, &rows(3, "a"))).unwrap();
    assert_eq!(senza.verdict().coherence, Coherence::Coherent { anchored: false });

    // con il testimone, è un rollback, e lo dice con i numeri
    let con = verify(
        VerifyRequest::new(&sessione, &rows(3, "a")).with_witness(&testimone),
    )
    .unwrap();
    assert_eq!(
        con.verdict().coherence,
        Coherence::Rollback {
            presented: corta.head(),
            presented_rows: 3,
            witnessed: corta.head(),
            witnessed_rows: 3,
        }
    );
    // il testimone non torna indietro: è la regola che rende il rollback
    // distinguibile dalla riscrittura
    assert!(matches!(
        testimone.observe(&corta),
        Err(WitnessError::NotMonotonic { .. })
    ));
}

#[test]
fn la_testa_dichiarata_ha_la_precedenza_sul_testimone() {
    let sessione = SessionId::new("s");
    let r = rows(6, "a");
    let giusta = chain("s", &r).head();
    let falsa = chain("s", &rows(6, "b")).head();

    // la testa dichiarata che le righe non ricostruiscono vince su tutto
    let mut testimone = Witness::new();
    testimone.observe(&chain("s", &r)).unwrap();
    let v = verify(
        VerifyRequest::new(&sessione, &r)
            .with_declared_head(&falsa)
            .with_witness(&testimone),
    )
    .unwrap();
    assert_eq!(
        v.verdict().coherence,
        Coherence::ContradictsDeclaredHead {
            declared: falsa,
            recomputed: giusta,
        }
    );

    // e una testa dichiarata che le righe ricostruiscono **non** zittisce un
    // testimone che la contraddice: le due cose si dicono entrambe
    let v = verify(
        VerifyRequest::new(&sessione, &r)
            .with_declared_head(&giusta)
            .with_witness(&testimone),
    )
    .unwrap();
    assert_eq!(
        v.verdict().coherence,
        Coherence::Coherent { anchored: true }
    );

    let mut altro = Witness::new();
    altro.observe(&chain("s", &rows(6, "b"))).unwrap();
    let v = verify(
        VerifyRequest::new(&sessione, &r)
            .with_declared_head(&giusta)
            .with_witness(&altro),
    )
    .unwrap();
    assert!(matches!(
        v.verdict().coherence,
        Coherence::ContradictsWitness { .. }
    ));
}

#[test]
fn una_testa_dichiarata_che_non_ottiene_le_righe_è_una_contraddizione() {
    let sessione = SessionId::new("s");
    let altra = chain("s", &rows(6, "b")).head();
    let v = verify(
        VerifyRequest::new(&sessione, &rows(6, "a")).with_declared_head(&altra),
    )
    .unwrap();
    assert_eq!(
        v.verdict().coherence,
        Coherence::ContradictsDeclaredHead {
            declared: altra,
            recomputed: chain("s", &rows(6, "a")).head(),
        }
    );
}

#[test]
fn un_testimone_incoerente_è_un_verdetto_e_non_un_panico() {
    let sessione = SessionId::new("s");
    let r = rows(6, "a");
    let mut testimone = Witness::new();
    testimone.observe(&chain("s", &r)).unwrap();
    // l'attacco è sul file, non sull'API: si cambia una voce e si rilegge
    let testo = serde_json::to_string(&testimone).unwrap();
    let manomesso = testo.replace("\"rows\":6", "\"rows\":99");
    assert_ne!(testo, manomesso);
    let rotto: Witness = serde_json::from_str(&manomesso).unwrap();
    assert!(matches!(rotto.check(), Err(WitnessError::Broken { .. })));
    let v = verify(
        VerifyRequest::new(&sessione, &r).with_witness(&rotto),
    )
    .unwrap();
    assert_eq!(v.verdict().coherence, Coherence::BrokenWitness { declared: rotto.head() });
}

#[test]
fn righe_incoerenti_sono_un_errore_e_non_un_verdetto() {
    let sessione = SessionId::new("s");
    // nessuna riga: non c'è una testa da attestare
    assert!(verify(VerifyRequest::new(&sessione, &[])).is_err());
    // seq non contigue
    let mut r = rows(4, "a");
    r[2].seq = SeqInSession(9);
    assert!(verify(VerifyRequest::new(&sessione, &r)).is_err());
    // segmentazione vuota
    let ok = rows(4, "a");
    assert!(
        verify(VerifyRequest::new(&sessione, &ok).with_plan(SegmentPlan::every(0))).is_err()
    );
}

// ── D11 · il replay ─────────────────────────────────────────────────────────

/// Un generatore finto, dichiarato qui come lo dichiarerebbe `kbs-exercise`:
/// versione, e una risposta all'incarico.
struct Fake {
    version: String,
    answer: i64,
}

impl InstanceGenerator for Fake {
    fn version(&self) -> &str {
        &self.version
    }

    fn generate(&self, key: &GeneratorKey) -> Result<Instance, ReplayError> {
        if key.exercise != "ex-1" {
            return Err(ReplayError::Generator {
                exercise: key.exercise.clone(),
                reason: "questo generatore non conosce l'esercizio".into(),
            });
        }
        Ok(Instance {
            exercise: key.exercise.clone(),
            seed: key.seed.clone(),
            rendered_prompt: "Calcola 2·3".into(),
            expected: self.answer.to_string(),
            params: json!({ "a": 2, "b": 3 }),
        })
    }
}

fn record() -> ReplayRecord {
    let key = ReplayKey::new("corpus-hash-1", "ex-1", "gen-1", "s1");
    let instance = Fake {
        version: "gen-1".into(),
        answer: 7,
    }
    .generate(&GeneratorKey {
        corpus_hash: key.corpus_hash.clone(),
        exercise: key.exercise.clone(),
        seed: key.seed.clone(),
    })
    .unwrap();
    ReplayRecord::new(key, instance, Millis(1_700_000_000_000), PersonId("person_docente".into()))
        .unwrap()
}

#[test]
fn il_replay_riproduce_la_risposta_attesa() {
    let v = replay(&record(), "corpus-hash-1", &Fake { version: "gen-1".into(), answer: 7 })
        .unwrap();
    assert_eq!(*v.verdict(), ReplayOutcome::Match);
    // e porta con sé i limiti come ogni altro verdetto
    assert_eq!(v.limits(), Limits::ALL);
    assert_eq!(v.verdict().to_string(), "replay fedele: l'istanza rigenerata è quella registrata");
}

#[test]
fn un_replay_che_non_torna_e_un_verdetto_con_due_valori() {
    let v = replay(&record(), "corpus-hash-1", &Fake { version: "gen-1".into(), answer: 8 })
        .unwrap();
    assert!(!v.verdict().is_match());
    let (field, recorded, regenerated) = v.verdict().mismatch().unwrap();
    assert_eq!(field.to_string(), "expected");
    assert_eq!(recorded, "7");
    assert_eq!(regenerated, "8");
    // non è un Err: la funzione è tornata con Ok
}

#[test]
fn un_replay_su_un_corpus_diverso_non_e_un_replay() {
    let v = replay(&record(), "corpus-hash-2", &Fake { version: "gen-1".into(), answer: 7 })
        .unwrap();
    assert_eq!(
        v.verdict().mismatch(),
        Some((
            ReplayField::CorpusHash,
            "corpus-hash-1",
            "corpus-hash-2"
        ))
    );
}

#[test]
fn un_replay_su_una_versione_diversa_non_e_un_replay() {
    let v = replay(&record(), "corpus-hash-1", &Fake { version: "gen-2".into(), answer: 7 })
        .unwrap();
    assert_eq!(
        v.verdict().mismatch(),
        Some((
            ReplayField::GeneratorVersion,
            "gen-1",
            "gen-2"
        ))
    );
}

#[test]
fn un_generatore_che_non_puo_generare_è_un_errore() {
    let sconosciuto = ReplayRecord::new(
        ReplayKey::new("corpus-hash-1", "ex-9", "gen-1", "s1"),
        Instance {
            exercise: "ex-9".into(),
            seed: "s1".into(),
            rendered_prompt: String::new(),
            expected: "0".into(),
            params: json!({}),
        },
        Millis(0),
        PersonId("p".into()),
    )
    .unwrap();
    let err = replay(
        &sconosciuto,
        "corpus-hash-1",
        &Fake { version: "gen-1".into(), answer: 7 },
    )
    .unwrap_err();
    assert!(matches!(err, ReplayError::Generator { .. }));
}

#[test]
fn un_record_che_non_descrive_la_sua_istanza_non_esiste() {
    let key = ReplayKey::new("corpus-hash-1", "ex-1", "gen-1", "s2");
    let instance = Instance {
        exercise: "ex-1".into(),
        seed: "s1".into(),
        rendered_prompt: "Calcola 2·3".into(),
        expected: "7".into(),
        params: json!({ "a": 2, "b": 3 }),
    };
    assert!(matches!(
        ReplayRecord::new(key, instance, Millis(0), PersonId("p".into())),
        Err(ReplayError::RecordKeyMismatch { .. })
    ));
}

// ── D12 · l'export a colonne fisse ───────────────────────────────────────────

#[test]
fn una_terza_persona_verifica_le_prove_dall_export_solo() {
    let onesta = chain("s", &rows(11, "a"));
    let mut testimone = Witness::new();
    testimone.observe(&onesta).unwrap();

    let testo = ChainExport::of(&[onesta.clone()], Some(&testimone))
        .unwrap()
        .to_text();
    let rilevato = ChainExport::from_text(&testo).unwrap();

    // la terza persona ha solo il file: nessuna riga, nessun server
    let mut teste: Vec<(String, Hash)> = Vec::new();
    for row in rilevato.rows() {
        match row {
            ExportRow::Head { session, head, .. } => teste.push((session.clone(), *head)),
            _ => {}
        }
    }
    assert_eq!(teste.len(), 1);
    let (_sessione, testa) = teste[0].clone();

    let mut segmenti = 0;
    for row in rilevato.rows() {
        if let ExportRow::Segment {
            session,
            index,
            from,
            to,
            rows,
            root,
            proof,
        } = row
        {
            let p = ConsistencyProof {
                session: SessionId::new(session.clone()),
                index: *index,
                from: *from,
                to: *to,
                rows: *rows,
                segment_root: *root,
                steps: ConsistencyProof::from_text(proof).unwrap(),
                head: testa,
            };
            assert_eq!(p.check().unwrap(), testa, "segmento {index} non torna");
            segmenti += 1;
        }
    }
    assert_eq!(segmenti, 3);
    assert_eq!(testa, onesta.head());
}

#[test]
fn lesportazione_dichiara_anche_i_limiti() {
    let c = chain("s", &rows(6, "a"));
    let testo = ChainExport::of(&[c], None).unwrap().to_text();
    for l in Limits::ALL.iter() {
        assert!(testo.contains(l.statement), "manca il limite {}", l.id);
    }
    // e un export i cui limiti sono stati addolciti non si riapre
    let addolcito = testo.replace("non verità", "e verità");
    assert!(addolcito.contains("e verità"));
    assert!(ChainExport::from_text(&addolcito).is_err());
}

#[test]
fn il_testimone_e_una_copia_indipendente_verificabile() {
    let c = chain("s", &rows(6, "a"));
    let mut w = Witness::new();
    w.observe(&c).unwrap();
    assert!(w.check().is_ok());
    assert_eq!(w.entries().len(), 1);
    let entry: &WitnessEntry = w.entries().first().unwrap();
    assert_eq!(entry.head, c.head());
    assert_eq!(entry.rows, 6);
    assert_eq!(entry.last_seq(), Some(5));
    // e la sua testa è ricalcolabile
    assert_eq!(w.recompute(), w.head());
}

#[test]
fn il_verdetto_senza_testimone_dice_che_non_e_ancorato() {
    let sessione = SessionId::new("s");
    let v: Verified<ChainVerdict> =
        verify(VerifyRequest::new(&sessione, &rows(6, "a"))).unwrap();
    assert!(v.to_string().contains("limiti dichiarati"));
    assert!(v.to_string().contains("riscrive"));
}
