//! Gli artifact del banco sono leggibili da `kbs-doc`, che è il lettore del
//! formato.
//!
//! Una fixture che scrive claim in una convenzione che nessuno legge è una
//! fixture che non prova niente: il banco può contare le sue claim, e la
//! pipeline non ne vede nessuna. Il difetto è invisibile a ogni controllo che
//! confronta la tabella con la resa, perché la tabella e la resa sono d'accordo
//! fra loro e sbagliano entrambe.
//!
//! Qui la domanda non è «la resa contiene queste stringhe?» ma «il lettore del
//! formato, applicato alla resa, restituisce le claim che la tabella
//! dichiara?». La risposta viene dal parser, e il parser non è di questo crate:
//! se domani la convenzione, la risposta cambia e questo test la dice.

use kbs_fixtures::spec::{ClaimStatusKind, EmittenteSpec, Spec};
use kbs_fixtures::{render, voci};

/// Le claim che il banco dichiara, per item: id, testo del fatto, ancora dello
/// span, stato e emittente.
fn dichiarate(s: &Spec) -> Vec<(&str, &str, Option<&str>, ClaimStatusKind)> {
    s.claims
        .iter()
        .map(|c| (c.id, c.testo, c.ancora, c.stato))
        .collect()
}

/// L'id che il lettore restituisce è **l'id dichiarato**.
///
/// È la prova che distingue «una claim con un id» da «una claim che il lettore
/// riconosce»: senza, il parser inventa `clm_N`, il registro contiene un'id che
/// nessuno ha dichiarato, e la riga è attribuita all'affermazione sbagliata.
#[test]
fn il_legge_torna_con_gli_id_dichiarati() {
    for s in voci() {
        let artefatto = render::artifact(&s);
        let letto = kbs_doc::validate(&artefatto);
        let letti: Vec<&str> = letto.parsed.claims.iter().map(|c| c.id.as_str()).collect();
        let attesi: Vec<&str> = dichiarate(&s).into_iter().map(|(id, ..)| id).collect();
        assert_eq!(
            letti, attesi,
            "{}: le claim che il lettore riconosce non sono quelle dichiarate",
            s.rel
        );
    }
}

/// Il testo che il lettore restituisce è il testo del fatto, non il suo id.
///
/// Le due cose si somigliano ed è per questo che la confusione sopravvive:
/// `data-claim` porta il fatto e `data-claim-id` porta l'identità. Scambiarle
/// produce un registro in cui ogni affermazione è il nome di un'etichetta.
#[test]
fn il_legge_torna_con_i_testi_dei_fatti() {
    for s in voci() {
        let artefatto = render::artifact(&s);
        let letto = kbs_doc::validate(&artefatto);
        for (id, testo, _, _) in dichiarate(&s) {
            let riga = riga_di(&letto, id, s.rel);
            assert_eq!(
                riga.text, testo,
                "{}: la claim {id} porta nel registro un testo diverso dal fatto dichiarato",
                s.rel
            );
        }
    }
}

/// Lo span che il lettore risolve è lo span dichiarato.
///
/// Una claim **senza** span non è `supported`: è `unciteable`, ed è la riga
/// che D6 vuole nel registro. Se il lettore non la vede, la non citabilità non
/// viene registrata — che è cancellare l'errore, il contrario di quello che D6
/// chiede.
#[test]
fn gli_span_dichiarati_sono_quelli_che_il_legge_risolve() {
    for s in voci() {
        let artefatto = render::artifact(&s);
        let letto = kbs_doc::validate(&artefatto);
        for (id, _, ancora, _) in dichiarate(&s) {
            let riga = riga_di(&letto, id, s.rel);
            match ancora {
                Some(a) => match &riga.span {
                    kbs_doc::SpanBinding::Bound { anchor, .. } => assert_eq!(
                        anchor, a,
                        "{}: la claim {id} non porta lo span dichiarato",
                        s.rel
                    ),
                    altro => panic!(
                        "{}: la claim {id} dichiara lo span {a} e il lettore dà {altro:?}: \
                         un anchor che il documento non risolve non sostiene niente",
                        s.rel
                    ),
                },
                None => {
                    assert!(
                        !riga.span.is_bound(),
                        "{}: la claim {id} non dichiara span e il lettore ne ha trovato uno: \
                         un'affermazione che nessuno può verificare è nata verificabile",
                        s.rel
                    );
                    assert_eq!(
                        riga.core_status(),
                        kbs_core::ClaimStatus::Unciteable,
                        "{}: la claim {id} non ha span e il lettore non la dà per non citabile",
                        s.rel
                    );
                }
            }
        }
    }
}

/// Lo stato che il lettore ne ricava dipende dallo span, non dall'autore.
///
/// `kbs-doc` non sa che una claim è contraddetta: un lettore di formato non
/// giudica. Quel che può sapere guardando il testo è se c'è uno span che
/// sostiene la claim, e nulla più.
#[test]
fn lo_stato_che_il_legge_ricava_dipende_dallo_span() {
    for s in voci() {
        let artefatto = render::artifact(&s);
        let letto = kbs_doc::validate(&artefatto);
        for (id, _, ancora, _) in dichiarate(&s) {
            let riga = riga_di(&letto, id, s.rel);
            let atteso = if ancora.is_some() {
                kbs_core::ClaimStatus::Supported
            } else {
                kbs_core::ClaimStatus::Unciteable
            };
            assert_eq!(
                riga.core_status(),
                atteso,
                "{}: la claim {id} dichiara lo span {:?} e il lettore dice {:?}",
                s.rel,
                ancora,
                riga.core_status()
            );
        }
    }
}

/// Lo stato che l'autore dichiara è scritto nel file, dove lo legge
/// `kbs_intake`.
///
/// Senza questo, la contraddizione che la tabella dichiara arriva in registro
/// come una claim supportata: l'errore più caro che un registro possa fare,
/// perché si presenta come una verità. È la riga che rende il file leggibile
/// anche da chi non è il banco.
#[test]
fn lo_stato_dichiarato_dalla_tabella_e_scritto_nel_file() {
    for s in voci() {
        let artefatto = render::artifact(&s);
        for (id, _, ancora, stato) in dichiarate(&s) {
            // La regola di `kbs_intake` è che lo span tiene lo stato: nessuno
            // span significa che nessuno sostiene la claim, e dichiararla
            // `supported` è una dichiarazione senza evidenza. Il banco non può
            // produrre una forma che quella regola rende irraggiungibile, e
            // quindi la tabella non ne dichiara.
            if ancora.is_none() {
                assert_eq!(
                    stato,
                    ClaimStatusKind::Unciteable,
                    "{}: la claim {id} non ha span e non è dichiarata `unciteable`: \
                     è una forma che `kbs_intake` corregge nel file e che il banco non sa produrre",
                    s.rel
                );
            }
            let atteso = match stato {
                ClaimStatusKind::Supported => "supported",
                ClaimStatusKind::Contradicted => "contradicted",
                ClaimStatusKind::Unciteable => "unciteable",
                ClaimStatusKind::Retracted(_) => "retracted",
            };
            assert!(
                artefatto.contains(&format!("data-stato=\"{atteso}\"")),
                "{}: la claim {id} non dichiara `data-stato=\"{atteso}\"`",
                s.rel
            );
        }
    }
}

/// L'emittente dichiarato arriva al registro.
///
/// D3 chiude la catena dei giudicanti e l'unico posto in cui un modello compare
/// è l'origine. Un emittente che l'artifact non dichiara diventa
/// `Emitter::Content` per difetto, e un'affermazione emessa da un lavoro non si
/// distingue più da un'affermazione scritta nel testo: il registro perde
/// l'informazione che rende la ripetizione possibile (D11).
#[test]
fn gli_emittenti_dichiarati_arrivano_al_registro() {
    for s in voci() {
        let artefatto = render::artifact(&s);
        for c in s.claims {
            // Le tre etichette sono quelle che `kbs_intake::route` distingue:
            // `teacher:<persona>`, `work:<osservazione>`, e `content` senza
            // prefisso. Un'etichetta che l'intake non riconosce cade nel
            // `Content` di difetto, e l'errore è invisibile.
            let dichiarato = match c.emittente {
                EmittenteSpec::Docente { by } => format!("teacher:person_{by:04}"),
                EmittenteSpec::Contenuto => "content".to_string(),
                EmittenteSpec::DaLavoro { osservazione } => format!("work:{osservazione}"),
            };
            assert!(
                artefatto.contains(&format!("data-claim-emitter=\"{dichiarato}\"")),
                "{}: la claim {} non dichiara l'emittente «{dichiarato}»",
                s.rel,
                c.id
            );
        }
    }
}

fn riga_di<'a>(
    letto: &'a kbs_doc::ArtifactReport,
    id: &str,
    rel: &str,
) -> &'a kbs_doc::ParsedClaim {
    letto.parsed
        .claims
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("{rel}: la claim {id} non è nel registro che il lettore ne ricava"))
}
