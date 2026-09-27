//! `kbs-serve` — il binario che è il server.
//!
//! Il nome è dichiarato in `Cargo.toml` e non dedotto da questo file, per la
//! ragione che è scritta in `kbs-intake/Cargo.toml`: un binario inferito da
//! `src/main.rs` prende il nome del pacchetto (`kbs-server`) e non quello che
//! l'operatore digita. Qui la differenza è un nome che esiste già — `kbs` è il
//! binario di `kbs-intake`, e `kbs-fixtures` lo cerca in `target/debug`.
//!
//! Il corpo è questo file intero: il nome del processo, la riga di comando, e
//! un `exit`. Tutto il resto — che cosa si apre, dove si ascolta, la guardia
//! dell'epoch, la chiusura pulita — sta in [`kbs_server::daemon`], che è
//! libreria per una ragione precisa: è ciò che un test può avviare davvero, su
//! una porta davvero, senza `fork`.

use std::process::ExitCode;

use kbs_server::daemon::{self, Esito, Richiesta};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--help` e `--version` sono richieste, non opzioni: rispondono senza
    // aprire il database, perché l'istanza di cui si vuole la versione
    // potrebbe essere proprio quella che non si apre.
    let opzioni = match daemon::analizza(&args) {
        Ok(Richiesta::Testo(testo)) => {
            print!("{testo}");
            return ExitCode::from(Esito::Ok.codice());
        }
        Ok(Richiesta::Avvio(opzioni)) => opzioni,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(Esito::da_errore(&e));
        }
    };

    // Il runtime si costruisce qui e non dentro `daemon`: `esegui` è una
    // `async fn`, e un test che monta il server da solo ha già il proprio
    // runtime e non deve pagarne un secondo. Qui il runtime è una cosa del
    // processo, e `main` è l'unico posto in cui un processo ha bisogno di uno.
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: il runtime non si costruisce: {e}", daemon::NOME);
            return ExitCode::from(Esito::Sistema.codice());
        }
    };

    // `block_on` e non `main` asincrono: un `#[tokio::main]` chiama `exit` e
    // lascia in giro i task di `tokio::spawn` — qui quello che ascolta
    // `SIGINT`/`SIGTERM`. Uccidere il runtime senza averlo chiuso è
    // esattamente la chiusura non pulita che il daemon promette di evitare.
    runtime.block_on(daemon::esegui(opzioni)).into()
}
