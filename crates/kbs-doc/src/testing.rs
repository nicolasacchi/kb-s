//! Esempi riusabili: un contratto nelle otto sezioni e un artifact completo che
//! li contiene.
//!
//! Sono qui, e non dentro un `#[cfg(test)]`, perché servono a tre cose che non
//! sono i test: i test degli altri moduli, i test di integrazione (che non
//! vedono gli item di test), e chiunque debba costruire un artifact di prova
//! senza riscrivere otto sezioni a mano. Il costo è qualche riga di codice
//! sempre compilato; il beneficio è che un solo esempio valido esiste e non
//! ne divergono tre.

/// Un contratto con le otto sezioni di D7, tutte entro budget.
///
/// Le frasi sono reali: un contratto finto non teaches niente e, soprattutto,
/// finisce dentro l'indice durante gli smoke test.
pub fn contratto_buono() -> String {
    [
        "## GUARDIAN",
        "Non confondere la continuità con la continuità uniforme: la prima è un limite, \
         la seconda una richiesta. Lo studente che le scambia risolve esercizi diversi \
         da quelli che il docente ha preparato.",
        "## PREREQUISITI",
        "Sapere leggere il grafico di una funzione in una variabile.",
        "## OBIETTIVI",
        "Distinguere i due enunciati, e scegliere quale dei due un esercizio stia \
         chiedendo.",
        "## SCALA",
        "Il concetto si usa su funzioni in una e in due variabili, e sulle successioni. \
         Fuori da quel contesto, «continuità» indica una proprietà topologica che questo \
         corso non tratta: va detto, altrimenti lo studente estende per analogia.",
        "## EQUIVOCI",
        "«Uniformemente continua» non vuol dire «derivabile», e «continua» non vuol dire \
         «uniformemente continua». Il secondo errore è più comune del primo perché il \
         grafico induce a leggerli come la stessa cosa.",
        "## ESEMPIO-LAVORATO",
        "f(x) = x² su [0,1] è uniformemente continua: per ogni ε c'è un δ che vale su \
         tutto l'intervallo. f(x) = 1/x su (0,1] è continua ma non uniformemente \
         continua, perché δ dipende dal punto. Il grafico dei due sembra diverso prima \
         della definizione e identico dopo.",
        "## VERIFICA",
        "Dare tre funzioni e chiedere di classificarle; chiedere di giustificare la \
         classificazione con la definizione, non con il grafico.",
        "## LIMITE",
        "Il grafico serve a orientarsi, non a dimostrare: la continuità si dimostra con \
         la definizione.",
    ]
    .join("\n")
}

/// Un artifact completo, valido, che contiene [`contratto_buono`].
pub fn artifact_buono() -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="it">
<head>
<meta charset="utf-8">
<title>Continuità e continuità uniforme</title>
<meta name="kb-course" content="mat-1">
<meta name="kb-tags" content="analisi,continuita">
</head>
<body>
<h1>Continuità e continuità uniforme</h1>
<h2 id="equivoci">Equivoci</h2>
<p>Le due nozioni non sono interscambiabili.</p>
<p id="span-1">Una funzione continua su tutto l'intervallo non è per questo uniformemente continua.</p>
<span data-claim="la continuità non implica l'uniforme continuità" data-claim-id="clm_1" data-claim-span="span-1"></span>
<template id="kb-kbprompt">
{}
</template>
</body>
</html>
"#,
        contratto_buono()
    )
}
