//! La tabola dei quaranta item, in un solo posto.
//!
////! Il numero 40 non è scelto per bellezza: è il più piccolo numero che copre
//! le dodici famiglie del catalogo mantenendo almeno un item per ciascuna e
//! senza che il banco diventi un corpus. Le dodici famiglie vengono dalla
//! ricerca (`kb-scuola-2026-09/03`), non da una scelta di comodo.
//!
//! La tabella è statica e non ha funzioni: un banco le cui fixture cambiano a
//! ogni esecuzione non può servire a individuare una regressione.

use crate::spec::Spec;

/// I quattro blocchi in cui la tabola è scritta. Sono file separati perché un
/// file solo sarebbe illeggibile, e un file illeggibile viene corretto male.
const BLOCCHI: [&[Spec]; 4] = [
    crate::table_1::V,
    crate::table_2::V,
    crate::table_3::V,
    crate::table_4::V,
];

/// Quanti item il banco contiene. È una costante e non un `len()`: se il banco
/// cresce, il numero va scritto qui, e il crescere diventa una decisione.
pub const DIMENSIONE: usize = 40;

/// I quaranta item, in ordine di percorso relativo.
pub fn voci() -> Vec<Spec> {
    let mut out = Vec::with_capacity(DIMENSIONE);
    for t in BLOCCHI {
        out.extend(t.iter().copied());
    }
    out.sort_by_key(|s| s.rel);
    out
}

/// Una voce per percorso. `None` se il percorso non è nel banco: il banco
/// non indovina, e un percorso che non c'è è un errore di chi lo cerca.
pub fn voce(rel: &str) -> Option<Spec> {
    voci().into_iter().find(|s| s.rel == rel)
}
