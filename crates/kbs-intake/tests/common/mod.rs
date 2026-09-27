//! Gli artifact di prova, costruiti e non scritti a mano.
//!
#![allow(dead_code)]
//!
//! Un test che copia un artifact da `kbs-fixtures/corpus` e lo modifica è un
//! test che si rompe quando qualcuno sistema una fixture, e non sa perché. Qui
//! l'artifact è una **funzione** dei suoi parametri: ciò che si vuole provare
//! è l'unica cosa che cambia.

use kbs_core::{CourseId, Millis, PersonId, Relation};
use kbs_store::{CourseRelation, Person, Source, SourceStatus, Store};

pub const CORSO: &str = "matematica-terza";
pub const REL: &str = "letture/prova.html";
pub const REL_ALTRO: &str = "letture/altro.html";

/// Un artifact valido: titolo, corso, stato, otto sezioni entro budget.
///
/// `guaradian_lungo` spinge il GUARDIAN oltre i suoi 640 byte (D7: deve stare
/// entro i primi, perché il troncamento taglia la coda), che è il difetto che
/// un validatore che tronca e poi valida dà per buono.
pub fn artifact(guaradian_lungo: bool) -> String {
    let guardian = if guaradian_lungo {
        format!("Prima di leggere: {}.", "spiega il vincolo ".repeat(120))
    } else {
        "Prima di leggere: la forma ridotta è unica, e unifierla significa moltiplicare numeratore e denominatore per lo stesso fattore primo. Se i due fattori non sono primi, l'unificazione non è stata fatta bene.".to_string()
    };
    format!(
        r#"<!doctype html>
<html lang="it">
<head>
<meta charset="utf-8">
<title>La frazione irriducibile</title>
<meta name="kb-argument" content="{rel}">
<meta name="kb-course" content="{corso}">
<meta name="kb-state" content="{stato}">
</head>
<body>
<main>
<h1 id="titolo">La frazione irriducibile</h1>
<p class="riassunto">Unita' che porta il concetto di forma ridotta e il suo criterio.</p>
{rel_prereq}
<p id="span-1">La scomposizione in fattori primi di un intero e' unica, e quindi la forma ridotta di una frazione e' unica.</p>
<p class="claim" data-claim="La scomposizione in fattori primi di un intero e' unica, e quindi la forma ridotta di una frazione e' unica." data-claim-id="cl_1" data-claim-span="span-1" data-stato="{stato_claim}">La scomposizione in fattori primi di un intero e' unica, e quindi la forma ridotta di una frazione e' unica.</p>
<script type="module" src="{script}"></script>
</main>
<template id="kb-kbprompt">
## GUARDIAN
{guardian}

## PREREQUISITI
01 numero razionale

## OBIETTIVI
Riconoscere la forma ridotta e saper dire perche' esiste.

## SCALA
Livello 4: riduce e spiega. Livello 3: riduce. Livello 2: sa cos'e'. Livello 1: procede a tentativi.

## EQUIVOCI
Non confondere «ridurre» con «semplificare»: la parola cambia, la operazione no.

## ESEMPIO-LAVORATO
6/8 = 3/4 perche' 2 e 4 hanno fattore primo 2; 6/8 non e' ridotta.

## VERIFICA
Riduci 12/18 e indica il fattore primo comune.

## LIMITE
Vale per interi positivi: con il segno e la riduzione ai minimi termini il criterio non basta da solo.
</template>
</body>
</html>
"#,
        rel = REL,
        corso = CORSO,
        stato = "bozza",
        rel_prereq = "",
        stato_claim = "supported",
        script = "/three/three.module.js",
        guardian = guardian,
    )
}

/// Lo stesso artifact, ma che referenzia una CDN (D15).
pub fn artifact_con_cdn() -> String {
    artifact(false).replace(
        "/three/three.module.js",
        "https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js",
    )
}

/// Lo stesso artifact, che dichiara `in-corso` senza portare ratifica.
pub fn artifact_in_corso() -> String {
    artifact(false).replace(
        r#"<meta name="kb-state" content="bozza">"#,
        r#"<meta name="kb-state" content="in-corso">"#,
    )
}

/// L'artifact che richiede sé stesso: il ciclo minimo.
pub fn artifact_ciclico() -> String {
    artifact(false).replace(
        r#"<h1 id="titolo">"#,
        "<ul class=\"prerequisiti\"><li data-prereq=\"letture/prova.html\">letture/prova.html</li></ul>\n<h1 id=\"titolo\">",
    )
}

/// Lo stesso artifact, con una claim che si dichiara `contradicted` pur
/// avendo uno span che la sostiene: D6vuole che la contraddetta resti nel
/// registro e non nell'output.
pub fn artifact_claim_contraddetta() -> String {
    artifact(false).replace(
        r#"data-stato="supported""#,
        r#"data-stato="contradicted""#,
    )
}

/// Un artifact che dichiara l'origine `generated` **senza** lock (D10).
pub fn artifact_generato_senza_lock() -> String {
    artifact(false).replace(
        r#"<meta name="kb-course" content="matematica-terza">"#,
        r#"<meta name="kb-course" content="matematica-terza">
<meta name="kb-origin" content="generated">"#,
    )
}

/// Il corso di [`artifact`].
pub fn corso() -> CourseId {
    CourseId(CORSO.to_string())
}

/// Il docente.
pub fn docente() -> PersonId {
    PersonId::fixture(1)
}

/// Un negozio con il corso registrato e il docente che lo insegna.
///
/// Il corso va registrato **prima** della relazione: `relations.course_id`
/// referenzia `sources`, e un corso che nessuno ha registrato non e' un corso.
/// E' la stessa riga che `scan::indexa` scrive per la strada del file, e qui
/// e' scritta a mano perche' il test non sta provando la scansione.
pub fn store_con_corso() -> Store {
    let mut s = Store::open_in_memory().expect("store in memoria");
    s.upsert_person(&Person {
        id: docente(),
        display_name: "docente".into(),
        created_at: Millis(0),
    })
    .unwrap();
    s.register_source(&Source {
        id: corso(),
        slug: CORSO.to_string(),
        rel_path: ".".into(),
        status: SourceStatus::Active,
        registered_at: Millis(0),
        last_scan_at: None,
        corpus_hash: None,
    })
    .unwrap();
    s.add_relation(&CourseRelation {
        person: docente(),
        course: corso(),
        relation: Relation::Teaches,
        since: Millis(0),
        until: None,
    })
    .unwrap();
    s
}
