//! I test di accettazione, uno per ogni requisito.
//!
//! Sono in un file di integrazione e non dentro i moduli perché sono la
//! **specifica** del crate: se uno di questi fallisce, il crate non fa quello
//! che `ARCHITECTURE.md` dice che fa, e il resto dei test non conta.

use kbs_doc::build::{self, BuildOptions, Gate, Resolver};
use kbs_doc::contract::{self, ContractError, HARD_CAP, SECTIONS};
use kbs_doc::offbox::{self, Origin};
use kbs_doc::parser::{self, SpanBinding};
use kbs_doc::testing::{artifact_buono, contratto_buono};
use kbs_doc::validate::{self, IssueCode};
use kbs_doc::{markdown, DocError};

/// Un artifact con il contratto indicato.
fn artifact_con_contratto(contratto: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html lang="it"><head><meta charset="utf-8">
<title>Continuità e continuità uniforme</title>
<meta name="kb-course" content="mat-1">
</head><body>
<h1>Continuità</h1>
<h2 id="equivoci">Equivoci</h2>
<p>Le due nozioni non sono interscambiabili.</p>
<template id="kb-kbprompt">
{contratto}
</template>
</body></html>"#
    )
}

// ── 1. Le otto sezioni, in ordine, entro budget → eseguibile ──────────────────

#[test]
fn un_contratto_con_le_otto_sezioni_in_ordine_e_entro_budget_e_eseguibile() {
    let r = contract::inspect(&contratto_buono());
    assert!(r.executable(), "errori: {:?}", r.errors);
    assert!(!r.truncated);
    let nomi: Vec<&str> = r.sections.iter().map(|s| s.name.as_str()).collect();
    let attesi: Vec<&str> = SECTIONS.iter().map(|s| s.name).collect();
    assert_eq!(nomi, attesi);
    for s in &r.sections {
        assert!(s.body_bytes > 0, "{} è vuota", s.name);
        assert!(!s.over_budget(), "{} sfora il suo budget", s.name);
    }
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert!(r.require().is_ok());
}

// ── 2. Manca `LIMITE` → rifiutato, con il nome della sezione ────────────────

#[test]
fn lo_stesso_contratto_senza_limite_e_rifiutato_con_il_nome_della_sezione() {
    let testo: String = contratto_buono()
        .lines()
        .filter(|l| !l.starts_with("## LIMITE") && !l.starts_with("Il grafico serve"))
        .collect::<Vec<_>>()
        .join("\n");

    let r = contract::inspect(&testo);
    assert!(!r.executable());
    match r.first_error() {
        Some(ContractError::MissingSection { name, .. }) => assert_eq!(name, "LIMITE"),
        altro => panic!("attesa la sezione mancante, trovato {altro:?}"),
    }
    // E l'artifact che lo contiene non si pubblica.
    let report = validate::inspect(&artifact_con_contratto(&testo));
    assert!(!report.can_publish());
    assert!(report.blocking().any(|i| matches!(
        i.code,
        IssueCode::Contract(ContractError::MissingSection { .. })
    )));
}

// ── 3. `GUARDIAN` oltre i 640 byte → rifiutato ───────────────────────────────

#[test]
fn lo_stesso_contratto_con_guardian_oltre_i_640_byte_e_rifiutato() {
    // 700 byte di preambolo: `GUARDIAN` comincia dopo il mark.
    let testo = format!("{}\n{}", "p".repeat(700), contratto_buono());
    let r = contract::inspect(&testo);
    assert!(!r.executable());
    match r.first_error() {
        Some(ContractError::GuardianTooLate { offset, limit }) => {
            assert!(*offset >= 640, "offset {offset}");
            assert_eq!(*limit, 640);
        }
        altro => panic!("atteso GuardianTooLate, trovato {altro:?}"),
    }
}

// ── 4 e 5. Troppo lungo: troncato e non eseguibile, anche se la testa è buona ─

#[test]
fn un_contratto_oltre_8192_byte_e_troncato_e_non_eseguibile() {
    let testo = format!("{}\n{}", contratto_buono(), "z".repeat(HARD_CAP));
    let r = contract::inspect(&testo);
    assert!(r.truncated, "contracts.truncated = 1");
    assert!(!r.executable(), "un contratto troncato non è eseguibile");
    assert!(matches!(
        r.first_error(),
        Some(ContractError::OverCap { bytes, cap }) if *bytes > HARD_CAP && *cap == HARD_CAP
    ));
    assert!(r.stored_text.len() <= HARD_CAP);
}

#[test]
fn la_testa_con_tutte_le_otto_sezioni_non_rende_eseguibile_un_contratto_troncato() {
    // Il caso che un validatore ingenuo passa: tronca, guarda la testa, trova
    // otto intestazioni, dice che va bene. La coda che il docente aveva
    // scritto è sparita, e il sistema ha dichiarato integro un contratto che
    // ha perso un pezzo.
    let testo = format!("{}{}", contratto_buono(), "z".repeat(HARD_CAP));
    let r = contract::inspect(&testo);

    // La precondizione: il testo **memorizzato** — quello che l'agente legge —
    // contiene davvero tutte e otto. È guardando quello che un validatore
    // ingenuo dice che va bene.
    for s in SECTIONS {
        assert!(
            r.stored_text.contains(&format!("## {}", s.name)),
            "la testa deve contenere {}",
            s.name
        );
    }
    assert_eq!(r.sections.len(), 8, "la testa presenta tutte e otto le sezioni");
    assert!(r.total_bytes > HARD_CAP);
    assert!(r.truncated, "e nonostante ciò è troncato");
    assert!(!r.executable());
    assert!(!validate::inspect(&artifact_con_contratto(&testo)).can_publish());
}

// ── 6 e 7. D15: un CDN non si pubblica, il runtime vendorizzato sì ───────────

#[test]
fn un_artifact_con_uno_script_da_cdn_e_rifiutato() {
    let src = artifact_buono().replace(
        "</body>",
        "<script src=\"https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.js\"></script></body>",
    );
    let fuori = offbox::external(&src);
    assert_eq!(fuori.len(), 1);
    assert_eq!(fuori[0].origin, Origin::External);
    assert!(fuori[0].line > 0);

    let report = validate::inspect(&src);
    assert!(!report.can_publish());
    let issue = report
        .first_blocking()
        .expect("un problema bloccante");
    assert!(matches!(issue.code, IssueCode::ExternalReference { .. }));
    assert!(issue.message.contains("D15"));
    assert!(issue.line.is_some(), "l'errore dice dove");

    // E la build non lo produce: un gate che lascia passare il file è
    // decorativo, e D4 dice che è peggio di nessun gate.
    struct Vuoto;
    impl Resolver for Vuoto {
        fn read(&self, _path: &str) -> Result<Vec<u8>, String> {
            Err("nessun file".to_string())
        }
    }
    assert!(matches!(
        build::build(&src, &BuildOptions::default(), &Vuoto, Gate::Strict),
        Err(DocError::Refused { .. })
    ));
}

#[test]
fn un_artifact_che_punta_al_runtime_vendorizzato_e_accettato() {
    // Il caso che passa: senza di esso la regola sarebbe soddisfatta vietando
    // tutto, e una regola che vieta tutto non è una regola, è un blackout.
    let src = artifact_buono().replace(
        "</body>",
        &format!("<script type=\"module\" src=\"{}\"></script></body>", offbox::THREE_RUNTIME),
    );
    assert!(offbox::external(&src).is_empty(), "{:?}", offbox::references(&src));
    assert_eq!(offbox::classify(offbox::THREE_RUNTIME), Origin::SameOrigin);

    let report = validate::inspect(&src);
    assert!(report.can_publish(), "{:?}", report.blocking().collect::<Vec<_>>());

    // Il percorso vendorizzato è quello del file minimizzato, e il file
    // minimizzato è nel repository.
    assert_eq!(offbox::THREE_RUNTIME, "/three/three.module.min.js");
    let vendor = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/three/three.module.min.js");
    assert!(vendor.exists(), "manca {}", vendor.display());
}

// ── 8. Collisione di id fra intestazioni ────────────────────────────────────

#[test]
fn lo_stesso_testo_di_intestazione_due_volte_e_una_collisione_riferita() {
    let src = artifact_buono().replace(
        "<h2 id=\"equivoci\">Equivoci</h2>",
        "<h2>Equivoci</h2>\n<h2>Equivoci</h2>",
    );
    let a = parser::parse(&src);
    assert_eq!(a.headings.len(), 3);
    assert_eq!(a.heading_collisions.len(), 1);
    let c = &a.heading_collisions[0];
    assert_eq!(c.id, "equivoci");
    assert_eq!(c.first_text, "Equivoci");
    assert_eq!(c.second_text, "Equivoci");

    // I due id derivati sono identici: è il problema, non la sua soluzione.
    let derivati: Vec<&str> = a
        .headings
        .iter()
        .filter(|h| h.derived_id)
        .map(|h| h.id.as_str())
        .collect();
    assert_eq!(
        derivati,
        vec!["continuita-e-continuita-uniforme", "equivoci", "equivoci"]
    );

    let report = validate::inspect(&src);
    assert!(!report.can_publish());
    assert!(report.blocking().any(|i| matches!(
        i.code,
        IssueCode::HeadingCollision { ref id, occurrences: 2 } if id == "equivoci"
    )));
}

// ── 9. Una claim il cui span è un'ancora vuota non è sostenuta ───────────────

#[test]
fn una_claim_che_punta_a_un_ancora_senza_testo_non_e_sostenuta() {
    let src = artifact_buono().replace(
        "<p>Le due nozioni non sono interscambiabili.</p>",
        r#"<p id="vuoto"></p>
        <span data-claim="la continuità non implica l'uniformità" data-claim-id="clm_2" data-claim-span="vuoto"></span>"#,
    );
    let a = parser::parse(&src);
    let claim = a
        .claims
        .iter()
        .find(|c| c.id == "clm_2")
        .expect("la claim dichiarata");
    assert!(!claim.is_supported());
    assert_eq!(claim.span, SpanBinding::AnchorOnly { anchor: "vuoto".into() });
    assert_eq!(claim.span.text(), None);
    assert_eq!(
        claim.core_status(),
        kbs_core::ClaimStatus::Unciteable,
        "senza testo lo span non sostiene niente"
    );

    // E l'artifact non è bloccato per questo: un registro di claim non
    // verificabili è un registro, non un artifact rotto.
    let report = validate::inspect(&src);
    assert!(report.can_publish());
    assert!(report.warnings().any(|i| matches!(
        i.code,
        IssueCode::UnverifiableClaim { ref claim_id, .. } if claim_id == "clm_2"
    )));

    // La claim dichiarata con uno span che c'è, invece, è sostenuta.
    assert!(a.claims.iter().find(|c| c.id == "clm_1").expect("clm_1").is_supported());
}

// ── 10. Il markdown in ingresso dichiara la sua provenienza ──────────────────

#[test]
fn il_markdown_ingresso_produce_un_artifact_che_dichiara_di_essere_stato_convertito() {
    let md = format!(
        "---\ntitle: Continuità\n---\n\n# Continuità\n\nProse.\n\n```kb-kbprompt\n{}\n```\n",
        contratto_buono()
    );
    let c = markdown::convert(&md);
    assert!(c.has_contract);
    assert!(c.html.contains("<meta name=\"kb-provenance\" content=\"converted-from-markdown\">"));

    let a = parser::parse(&c.html);
    assert!(a.converted_from_markdown());
    assert_eq!(a.title.as_deref(), Some("Continuità"));

    let report = validate::inspect(&c.html);
    assert!(report.can_publish(), "{:?}", report.blocking().collect::<Vec<_>>());
    assert!(
        report.warnings().any(|i| matches!(i.code, IssueCode::ConvertedFromMarkdown)),
        "il lettore deve poter sapere che è un artifact convertito"
    );

    // E un artifact scritto in HTML non porta quel meta: il flag dichiara
    // una differenza, non un formato.
    assert!(!parser::parse(&artifact_buono()).converted_from_markdown());
}

// ── Il budget: il runtime è una voce, non una nota ──────────────────────────

#[test]
fn il_peso_del_runtime_vendorizzato_e_una_voce_del_budget() {
    struct Vuoto;
    impl Resolver for Vuoto {
        fn read(&self, _path: &str) -> Result<Vec<u8>, String> {
            Err("nessun file".to_string())
        }
    }
    // `vendor_dir` è la directory che **contiene** `three/`, non `three/`
    // stessa: il percorso servito è `/three/…` e sotto la radice ci deve
    // corrispondere `three/…`.
    let vendor = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor");
    let src = artifact_buono().replace(
        "</body>",
        &format!(
            "<script type=\"module\" src=\"{}\"></script></body>",
            offbox::THREE_RUNTIME
        ),
    );
    let out = build::build(&src, &BuildOptions::for_repo(&vendor), &Vuoto, Gate::Strict)
        .expect("build");

    // Il runtime non è nel testo dell'artifact: è dichiarato a parte.
    let riga = out
        .budget
        .lines
        .iter()
        .find(|l| l.label == offbox::THREE_RUNTIME)
        .expect("il runtime è una riga di budget");
    assert_eq!(riga.counted_in, kbs_doc::build::CountedIn::DeclaredRuntime);
    assert!(riga.measured, "il peso è misurato sui byte del repository");

    // I numeri sono quelli del file, non una stima.
    let grezzo = std::fs::metadata(vendor.join("three/three.module.min.js"))
        .expect("three vendorizzato")
        .len() as usize;
    let gz = std::fs::metadata(vendor.join("three/three.module.min.js.gz"))
        .expect("three compresso")
        .len() as usize;
    assert_eq!(riga.raw_bytes, grezzo);
    assert_eq!(riga.gzip_bytes, gz);
    assert!(gz < grezzo);

    // I due conti non si sommano per errore: il testo è il testo.
    assert_eq!(out.budget.artifact_text_bytes(), out.html.len());
    assert_eq!(out.budget.declared_runtime_bytes(), gz);
    assert_eq!(
        out.budget.transferred_bytes(),
        out.html.len() + gz,
        "un artifact da 40 KB che carica 171 KB di runtime non è un artifact da 40 KB"
    );
    // Il testo non contiene il runtime: lo referenzia per percorso.
    assert!(out.html.contains(&format!("src=\"{}\"", offbox::THREE_RUNTIME)));
    assert!(
        out.html.len() < grezzo,
        "il runtime non è inline: il file è più piccolo del runtime"
    );

    // Il tetto dichiarato esiste, e il runtime vendorizzato ci sta.
    assert!(out.budget.over_runtime_budget().is_empty());
    assert!(gz < kbs_doc::build::DEFAULT_RUNTIME_BUDGET_GZIP);
    // Con un tetto più piccolo, la riga che sfora viene detta.
    let stretto = BuildOptions {
        vendor_dir: Some(vendor.clone()),
        runtime_budget_gzip: 1024,
    };
    let out2 = build::build(&src, &stretto, &Vuoto, Gate::Strict).expect("build");
    assert_eq!(out2.budget.over_runtime_budget().len(), 1);
    assert!(out2.budget.render().contains("oltre il tetto"));
}

// ── La build produce un file self-contained ─────────────────────────────────

#[test]
fn la_build_inline_il_foglio_di_stile_e_produce_un_file_unico() {
    let src = artifact_buono()
        .replace("</head>", "<link rel=\"stylesheet\" href=\"style.css\">\n</head>")
        .replace("</body>", "<img src=\"fig.png\" alt=\"f\"></body>");
    struct Fig;
    impl Resolver for Fig {
        fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            match path {
                "style.css" => Ok(b"h1{color:#222}".to_vec()),
                "fig.png" => Ok(b"\x89PNG".to_vec()),
                other => Err(format!("{other}: non c'è")),
            }
        }
    }
    let out = build::build(&src, &BuildOptions::default(), &Fig, Gate::Strict).expect("build");

    assert!(out.html.contains("<style data-inlined-from=\"style.css\">"));
    assert!(out.html.contains("h1{color:#222}"));
    assert!(out.html.contains("src=\"data:image/png;base64,"));
    assert!(!out.html.contains("href=\"style.css\""));
    assert!(!out.html.contains("src=\"fig.png\""));

    // Ogni peso inline è nel conto del testo, e il conto torna.
    let testo: usize = out
        .budget
        .lines
        .iter()
        .filter(|l| l.counted_in == kbs_doc::build::CountedIn::ArtifactText)
        .map(|l| l.raw_bytes)
        .sum();
    assert!(testo >= out.html.len(), "il testo conta tutto ciò che contiene");
    assert!(out.budget.unresolved().is_empty());
    assert!(out.report.can_publish());
}

// ── L'anagrafe del materiale: sei elementi, zero costi, zero blocchi ─────────

/// Un artifact con i `<meta>` dell'anagrafe indicati, sul documento buono.
fn artifact_con_anagrafe(meta: &str) -> String {
    artifact_buono().replace("</head>", &format!("{meta}</head>"))
}

#[test]
fn un_artifact_senza_un_solo_meta_ha_un_archivio_completo_e_silenzioso() {
    // Il caso da novanta volte su cento: il docente non ha scritto niente. Il
    // sistema non chiede niente, non deduce niente di inventato, e non dice
    // una parola: tre elementi arrivano dal documento, tre restano assenti, e
    // l'assenza è uno stato e non un avviso.
    let src = artifact_con_anagrafe("<meta name=\"kb-argument\" content=\"letture/01-x.html\">");
    let report = validate::inspect(&src);
    let a = &report.anagrafe;

    assert_eq!(a.value("dc.title"), Some("Continuità e continuità uniforme"));
    assert_eq!(a.value("dc.language"), Some("it"));
    assert_eq!(a.value("dc.identifier"), Some("letture/01-x.html"));
    for nome in ["dc.creator", "dc.date", "dc.rights"] {
        let c = a.get(nome).expect("l'elemento è nel registro anche se manca");
        assert_eq!(c.origine, kbs_doc::Origine::Assente, "{nome} non è dedotto e non c'è");
    }
    assert!(a.dichiarati().next().is_none());
    assert!(!report
        .warnings()
        .any(|i| matches!(i.code, IssueCode::Anagrafe(_))));
    assert!(report.can_publish());
}

#[test]
fn l_anagrafe_dichiara_chi_sono_quando_il_docente_lo_dice_e_il_documento_lo_conferma() {
    let src = artifact_con_anagrafe(concat!(
        r#"<meta name="kb-argument" content="letture/01-x.html">"#,
        r#"<meta name="dc.creator" content="Anna Rossi">"#,
        r#"<meta name="dc.date" content="2026-09-26">"#,
        r#"<meta name="dc.rights" content="Uso didattico in classe, CC BY-SA 4.0">"#,
    ));
    let report = validate::inspect(&src);
    let a = &report.anagrafe;

    assert_eq!(a.value("dc.creator"), Some("Anna Rossi"));
    assert_eq!(a.value("dc.date"), Some("2026-09-26"));
    assert!(a.value("dc.rights").unwrap().contains("CC BY-SA"));
    assert_eq!(a.dichiarati().count(), 3, "tre dichiarati, tre dedotti");
    assert!(!report
        .warnings()
        .any(|i| matches!(i.code, IssueCode::Anagrafe(_))));
}

#[test]
fn un_elemento_dichiarato_e_sbagliato_e_un_avviso_e_l_artifact_si_pubblica_lo_stesso() {
    let src = artifact_con_anagrafe(concat!(
        r#"<meta name="dc.title" content="Continuità uniforme">"#, // ≠ <title>
        r#"<meta name="dc.date" content="26 settembre 2026">"#,
        r#"<meta name="dc.subject" content="funzioni">"#, // fuori registro
    ));
    let report = validate::inspect(&src);
    let avvisi: Vec<String> = report
        .warnings()
        .filter_map(|i| match &i.code {
            IssueCode::Anagrafe(w) => Some(w.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(avvisi.len(), 3, "{avvisi:?}");
    assert!(report.can_publish(), "l'anagrafe non blocca niente");
    assert!(
        !report.blocking().any(|i| matches!(i.code, IssueCode::Anagrafe(_))),
        "nessun avviso dell'anagrafe è bloccante"
    );
}

#[test]
fn il_record_di_un_artifact_torna_da_se_stesso_passando_dai_meta() {
    // Il round-trip è la metà della specifica: l'anagrafe esiste perché il
    // materiale del docente sia recuperabile, e un record che si sa leggere ma
    // non si sa riscrivere è decorazione. Il confronto passa dal parser vero.
    let src = artifact_con_anagrafe(concat!(
        r#"<meta name="kb-argument" content="letture/01-x.html">"#,
        r#"<meta name="dc.creator" content="Anna Rossi">"#,
        r#"<meta name="dc.date" content="2026-09-26">"#,
    ));
    let prima = validate::inspect(&src).anagrafe;
    let meta = prima
        .to_metas()
        .iter()
        .map(|(n, v)| format!("<meta name=\"{n}\" content=\"{v}\">"))
        .collect::<String>();
    // Il percorso resta nel documento: `dc.identifier` è **dedotto** da
    // `kb-argument`, e riscrivere il documento senza di esso cambierebbe
    // l'ambiente in cui il record viene letto. Il round-trip che si verifica
    // è quello del record, non quello dell'ambiente.
    let riscritto = artifact_con_anagrafe(&format!(
        r#"<meta name="kb-argument" content="letture/01-x.html">{meta}"#
    ));
    let dopo = validate::inspect(&riscritto).anagrafe;
    assert_eq!(prima, dopo, "il record che esce torna identico a quello che entra");
}

#[test]
fn il_record_si_scrive_anche_in_front_matter_e_torna_da_li() {
    // La stessa chiave, nella forma in cui il docente la scrive: il
    // front-matter è il nome del meta com'è, senza un secondo vocabolario in
    // mezzo.
    let a = validate::inspect(&artifact_con_anagrafe(r#"<meta name="dc.creator" content="Anna Rossi">"#))
        .anagrafe;
    let md = format!(
        "---\ntitle: Continuità e continuità uniforme\n{}---\n\ntesto\n",
        a.to_front_matter()
    );
    let r = validate::inspect(&markdown::convert(&md).html);
    assert_eq!(r.anagrafe.value("dc.creator"), Some("Anna Rossi"));
    assert_eq!(r.anagrafe.value("dc.title"), Some("Continuità e continuità uniforme"));
}
