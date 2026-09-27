//! Le claim dichiarate in un artifact e gli span che le sostengono.
//!
//! Il modulo contiene i test della parte di [`crate::parser`] che riguarda le
//! claim, perché è lì che sta la regola e i test devono stare dove la
//! dichiarano: *una claim il cui span è un'ancora senza testo è un indirizzo, e
//! un indirizzo non si verifica*.
//!
//! `SpanBinding` ([`crate::parser::SpanBinding`]) rappresenta i tre stati
//! invece di fingere di aver verificato. `core_status` fa la Traduzione in
//! `kbs-core`: `Supported` solo se lo span è stato trovato e porta testo, altrimenti
//! `Unciteable` — che in D6 non cancella la riga, ne sopprime l'output.

use kbs_core::ClaimStatus;

use crate::parser::{self, ParsedClaim, SpanBinding};

/// Il testo HTML con cui si dichiarano claim e span.
///
/// Lo span è un elemento con `id`; la claim è un elemento che la nomina. Sono
/// due elementi distinti perché il testo che sostiene la claim deve poter
/// cambiare senza che la claim venga riscritta, e — più importante — perché è
/// la forma in cui il registro delle affermazioni (D6) può mostrare a un
/// lettore **che cosa** ha valutato.
const DOC: &str = r#"<html><body>
  <p id="span-1">Un grafico che sale non dimostra la continuità della funzione.</p>
  <p id="vuoto"></p>
  <span data-claim="la continuità non segue dal grafico" data-claim-id="clm_1" data-claim-span="span-1"></span>
  <span data-claim="la continuità segue dal grafico" data-claim-id="clm_2" data-claim-span="vuoto"></span>
  <span data-claim="l'argomento funziona" data-claim-id="clm_3" data-claim-span="inesistente"></span>
  <span data-claim="senza span" data-claim-id="clm_4"></span>
</body></html>"#;

fn claim<'a>(a: &'a parser::ParsedArtifact, id: &str) -> &'a ParsedClaim {
    a.claims
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("claim {id} non trovata fra {:?}", ids(a)))
}

fn ids(a: &parser::ParsedArtifact) -> Vec<&str> {
    a.claims.iter().map(|c| c.id.as_str()).collect()
}

#[test]
fn uno_span_con_testo_sostiene_la_claim() {
    let a = parser::parse(DOC);
    let c = claim(&a, "clm_1");
    assert!(c.is_supported());
    assert_eq!(c.core_status(), ClaimStatus::Supported);
    assert!(matches!(&c.span, SpanBinding::Bound { anchor, .. } if anchor == "span-1"));
    assert_eq!(
        c.span.text(),
        Some("Un grafico che sale non dimostra la continuità della funzione.")
    );
}

#[test]
fn un_ancora_senza_testo_non_sostiene_nessuna_claim() {
    // La regola: un indirizzo non è uno span. La claim non è `supported`, e il
    // parser **lo dice** invece di restituire un binding che sembri buono.
    let a = parser::parse(DOC);
    let c = claim(&a, "clm_2");
    assert!(!c.is_supported());
    assert_eq!(c.core_status(), ClaimStatus::Unciteable);
    assert!(matches!(&c.span, SpanBinding::AnchorOnly { anchor } if anchor == "vuoto"));
    assert_eq!(c.span.text(), None);
    assert!(c.span.reason().contains("indirizzo"));
}

#[test]
fn un_ancora_inesistente_non_e_una_claim_sostenuta() {
    let a = parser::parse(DOC);
    let c = claim(&a, "clm_3");
    assert!(!c.is_supported());
    assert_eq!(c.core_status(), ClaimStatus::Unciteable);
    assert!(matches!(&c.span, SpanBinding::Missing { anchor } if anchor == "inesistente"));
    assert!(c.span.reason().contains("non esiste"));
}

#[test]
fn una_claim_senza_span_dichiara_che_non_ha_span() {
    let a = parser::parse(DOC);
    let c = claim(&a, "clm_4");
    assert_eq!(c.span, SpanBinding::Undeclared);
    assert!(!c.is_supported());
    assert_eq!(c.core_status(), ClaimStatus::Unciteable);
}

#[test]
fn solo_una_claim_su_quattro_e_sostenuta() {
    let a = parser::parse(DOC);
    assert_eq!(a.claims.len(), 4);
    let non_verificabili: Vec<&str> = a.unverified_claims().map(|c| c.id.as_str()).collect();
    assert_eq!(non_verificabili, vec!["clm_2", "clm_3", "clm_4"]);
}

#[test]
fn lo_span_si_risolve_anche_se_viene_dopo_la_claim() {
    let doc = r#"<html><body>
      <span data-claim="fatto" data-claim-id="c" data-claim-span="dopo"></span>
      <p id="dopo">testo dello span</p>
    </body></html>"#;
    let a = parser::parse(doc);
    assert!(a.claims[0].is_supported());
}

#[test]
fn il_testo_del_contratto_non_e_uno_span() {
    // Lo slot del contratto ha un `id`, ma il suo testo non è prosa del
    // documento: non è sufficiente a sostenere una claim.
    let doc = r#"<html><body>
      <template id="kb-kbprompt">## GUARDIAN
un vincolo</template>
      <span data-claim="esiste un vincolo" data-claim-id="c" data-claim-span="kb-kbprompt"></span>
    </body></html>"#;
    let a = parser::parse(doc);
    assert!(!a.claims[0].is_supported(), "il contratto non è uno span");
}

#[test]
fn la_claim_senza_id_ottiene_un_identificatore_stabile() {
    let doc = r#"<html><body><span data-claim="fatto"></span><span data-claim="altro"></span></body></html>"#;
    let a = parser::parse(doc);
    assert_eq!(ids(&a), vec!["clm_1", "clm_2"]);
    assert_eq!(ids(&parser::parse(doc)), ids(&a));
}
