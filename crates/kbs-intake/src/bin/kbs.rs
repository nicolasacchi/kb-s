//! `kbs` — il binario che è l'interfaccia.
//!
//! Dodici righe, e sono dodici rigole [`kbs_intake::cli::esegui`]. Tutta la
//! logica sta nella libreria, e questa è la ragione per cui è così: un binario
//! con la logica dentro non è riutilizzabile dall'interfaccia HTTP di
//! `kbs-server`, e due interfacce che non chiamano lo stesso codice divergono
//! alla prima regola che una delle due implementa diversamente.
//!
//! Il nome è `kbs` e non `kbs-intake` perché `kbs-fixtures::adapter::ProcessPipeline`
//! cerca `target/debug/kbs`: è il banco che chiama questo processo, ed è D10.2
//! («la CLI come protocollo») che vuole esattamente questo.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    let mut stdin = std::io::stdin().lock();
    let esito = kbs_intake::cli::esegui(&args, &mut out, &mut err, &mut stdin);
    // `ExitCode` si porta dietro il codice numerico: `esito.into()` è
    // l'implementazione di `From<Uscita>` in `cli`, ed è l'unico posto in cui
    // un codice di uscita diventa un intero.
    esito.into()
}
