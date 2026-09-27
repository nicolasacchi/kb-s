//! Le dodici famiglie di media del corpus di studio.
//!
//! L'elenco **non è inventato qui**: è il catalogo dei media della ricerca
//! `kb-scuola-2026-09/03-oggetti-di-apprendimento.html`, che enumera dodici
//! famiglie e le interroga con le stesse sei domande. Il banco usa quel
//! catalogo perché un banco che copre dodici famiglie inventate non coprirebbe
//! il dominio di cui `kb-s` si occupa.
//!
//! Ogni famiglia porta con sé la **sua rotta di malfunzionamento**, che è la
//! ragione per cui il catalogo classifica i ruoli del mezzo e non i formati:
//! una figura male letta e un checker che sbaglia il segno non possono stare
//! nello stesso indicizzatore senza perdere la capacità di distinguere un
//! errore di estrazione da un errore di comprensione.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Una famiglia di media del catalogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    /// 1 · PDF, articoli, appunti del docente. Passivo come formato.
    Letture,
    /// 2 · Da sorgente vettoriale. I nodi si prendono, gli archi no.
    Mappe,
    /// 3 · Passivo per default: la parte che rende il video interessante è
    /// la relazione spaziale e il gesto, e quelle non sono testo.
    VideoAudio,
    /// 4 · Con checker deterministico. Il mezzo che porta con sé il proprio
    /// criterio di verità.
    Esercizi,
    /// 5 · Saggio, risposta aperta, tesina.
    Scritture,
    /// 6 · Prove passate: la matrice risposta-per-studente si auto-diagnostica.
    Prove,
    /// 7 · Forum, annotazioni, interrogazioni: l'ancoraggio a quattro scope.
    Dialoghi,
    /// 8 · Appunti dello studente: attivi, e diventano didattici solo dopo.
    Appunti,
    /// 9 · Notebook, simulazione, dataset.
    Laboratori,
    /// 10 · La rubrica: criteri e descrittori di livello.
    Rubrica,
    /// 11 · Mazzi di ripasso: l'attività è autoregolata, non dal sistema.
    Mazzi,
    /// 12 · Produzioni multimediali: la comprensione non è nel segnale.
    Produzioni,
}

/// Le dodici famiglie, nell'ordine del catalogo. L'ordine è parte del
/// contratto: il banco asserisce la copertura in quest'ordine, e un report
/// diversamente ordinato non è confrontabile con il precedente.
pub const ALL: [Family; 12] = [
    Family::Letture,
    Family::Mappe,
    Family::VideoAudio,
    Family::Esercizi,
    Family::Scritture,
    Family::Prove,
    Family::Dialoghi,
    Family::Appunti,
    Family::Laboratori,
    Family::Rubrica,
    Family::Mazzi,
    Family::Produzioni,
];

impl Family {
    /// Numero nel catalogo della ricerca, 1-based.
    pub fn numero(self) -> u8 {
        match self {
            Family::Letture => 1,
            Family::Mappe => 2,
            Family::VideoAudio => 3,
            Family::Esercizi => 4,
            Family::Scritture => 5,
            Family::Prove => 6,
            Family::Dialoghi => 7,
            Family::Appunti => 8,
            Family::Laboratori => 9,
            Family::Rubrica => 10,
            Family::Mazzi => 11,
            Family::Produzioni => 12,
        }
    }

    /// Etichetta nel corpus: il nome del catalogo, minuscolo e con trattini.
    pub fn slug(self) -> &'static str {
        match self {
            Family::Letture => "letture",
            Family::Mappe => "mappe",
            Family::VideoAudio => "video-audio",
            Family::Esercizi => "esercizi",
            Family::Scritture => "scritture",
            Family::Prove => "prove",
            Family::Dialoghi => "dialoghi",
            Family::Appunti => "appunti",
            Family::Laboratori => "laboratori",
            Family::Rubrica => "rubrica",
            Family::Mazzi => "mazzi",
            Family::Produzioni => "produzioni",
        }
    }

    /// Nome nel catalogo della ricerca, per il referto.
    pub fn nome(self) -> &'static str {
        match self {
            Family::Letture => "Letture e documenti",
            Family::Mappe => "Mappe concettuali",
            Family::VideoAudio => "Video e audio",
            Family::Esercizi => "Esercizi e problemi",
            Family::Scritture => "Scritture e compiti",
            Family::Prove => "Compiti d'esame e prove passate",
            Family::Dialoghi => "Dialoghi e scambi",
            Family::Appunti => "Appunti dello studente",
            Family::Laboratori => "Dati e laboratori",
            Family::Rubrica => "La rubrica",
            Family::Mazzi => "Mazzi di ripasso",
            Family::Produzioni => "Produzioni multimediali",
        }
    }

    /// Il verdetto ICAP della colonna maestra, che è un **vincolo** e non un
    /// commento: un oggetto la cui azione studente è passiva per costruzione
    /// non diventa didattico aggiungendogli un chatbot davanti.
    pub fn icap(self) -> &'static str {
        match self {
            Family::Letture => "passivo come formato",
            Family::Mappe => "passivo se generata, interattivo se lo studente la corregge",
            Family::VideoAudio => "passivo per default",
            Family::Esercizi => "interattivo senza componenti aggiuntivi",
            Family::Scritture => "costruttivo, poi interattivo",
            Family::Prove => "costruttivo, e la misura è la riga di risposta",
            Family::Dialoghi => "interattivo come effetto, non come funzione",
            Family::Appunti => "attivo, diventa didattico dopo la rielaborazione",
            Family::Laboratori => "interattivo: il costo è l'ambiente",
            Family::Rubrica => "attivo, poi interattivo",
            Family::Mazzi => "attivo ma l'autore decide quando",
            Family::Produzioni => "interattivo senza bisogno di macchina",
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nome())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le dodici famiglie sono un **catalogo**, non un'etichetta: se ne
    /// aggiunge una, il numero di copertura del banco non significa più
    /// niente. Il test fissa il conteggio e l'unicità degli slug.
    #[test]
    fn il_catalogo_ha_dodici_famiglie_e_slug_univoci() {
        assert_eq!(ALL.len(), 12);
        let mut slug: Vec<&str> = ALL.iter().map(|f| f.slug()).collect();
        slug.sort_unstable();
        let n = slug.len();
        slug.dedup();
        assert_eq!(slug.len(), n, "due famiglie hanno lo stesso slug");
        let mut numeri: Vec<u8> = ALL.iter().map(|f| f.numero()).collect();
        numeri.sort_unstable();
        assert_eq!(numeri, (1..=12).collect::<Vec<u8>>(), "i numeri non sono 1..=12");
    }

    #[test]
    fn nessuna_famiglia_e_senza_verdetto_icap() {
        for f in ALL {
            assert!(!f.icap().is_empty(), "{f} non ha verdetto ICAP");
            assert!(!f.nome().is_empty(), "{f} non ha nome");
        }
    }
}
