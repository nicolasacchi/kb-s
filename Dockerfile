# kb-s — il daemon del corpus scolastico, in un'immagine.
#
# Perché un Dockerfile e non un binario sulla host: `~/traefik` parla solo
# con container della sua rete, e ogni servizio in quel compose è
# `build: {context: <path>}`. Questo è il modo in cui quella infrastruttura
# pubblica una cosa.
#
# Il binario non è portabile per costruzione, e per un motivo dichiarato:
# `web_dir` e `vendor_dir` hanno un default **compilato dentro** da
# `env!("CARGO_MANIFEST_DIR")`, che nel checkout vale ma in un'immagine
# punta a `/build/...`, una directory che qui non esiste. Per questo il
# comando di avvio passa `--web` e `--vendor` esplicitamente: è la ragione
# per cui quei due flag esistono, e se un giorno li togli questa immagine
# si rompe in silenzio. Fallo notare.

# ── costruzione ───────────────────────────────────────────────────────────────
FROM rust:1-bookworm AS costruisci

WORKDIR /build

# Le dipendenze prima del sorgente: il layer delle dipendenze si riusa finché
# Cargo.toml e Cargo.lock non cambiano, e il codice di questo repository
# cambia molto più spesso. Senza questo, ogni modifica a una riga di un
# crate ricompila otto crate.
COPY Cargo.toml Cargo.lock ./
COPY crates/kbs-core/Cargo.toml    crates/kbs-core/
COPY crates/kbs-store/Cargo.toml    crates/kbs-store/
COPY crates/kbs-fixtures/Cargo.toml crates/kbs-fixtures/
COPY crates/kbs-doc/Cargo.toml      crates/kbs-doc/
COPY crates/kbs-verify/Cargo.toml   crates/kbs-verify/
COPY crates/kbs-exercise/Cargo.toml crates/kbs-exercise/
COPY crates/kbs-intake/Cargo.toml   crates/kbs-intake/
COPY crates/kbs-server/Cargo.toml   crates/kbs-server/
# Le dichiarazioni bastano a risolvere il grafo: si usa il fatto che un
# workspace senza sorgente non compila, per costruire solo le dipendenze.
RUN mkdir -p crates/kbs-core/src crates/kbs-store/src crates/kbs-fixtures/src \
             crates/kbs-doc/src crates/kbs-verify/src crates/kbs-exercise/src \
             crates/kbs-intake/src crates/kbs-server/src \
 && echo 'fn main() {}' > crates/kbs-core/src/lib.rs \
 && for c in kbs-store kbs-fixtures kbs-doc kbs-verify kbs-exercise kbs-intake; do \
      echo 'pub fn __riserva() {}' > crates/$c/src/lib.rs; \
    done \
 && echo 'fn main() {}' > crates/kbs-server/src/main.rs \
 && cargo build --release --bin kbs-serve \
 && rm -rf crates/*/src

COPY crates/ crates/
COPY vendor/ vendor/
# Il touch evita che cargo consideri i crate già compilati più recenti dei
# sorgenti, che è il modo in cui si perdono ore di build a un binario
# svuotato.
RUN find crates -name '*.rs' -exec touch {} + \
 && cargo build --release --bin kbs-serve --bin kbs \
 && strip target/release/kbs-serve target/release/kbs

# ── esecuzione ────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# `ca-certificates` serve alle firme TLS; `curl` serve a un healthcheck
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

# Un utente non privilegiato. Il daemon scrive in due posti — il database e
# nient'altro — e nessuno dei due è il corpus, che è montato in sola
# lettura: vedi D12, il corpus è un albero di file e l'uscita è `rm -rf`.
RUN useradd --system --create-home --uid 10001 kbs
WORKDIR /app

COPY --from=costruisci /build/target/release/kbs-serve /app/kbs-serve

# La CLI di intake viaggia con l'immagine per la stessa ragione del daemon:
# un operatore che ha materiale da caricare deve poterlo fare senza
# raggiungere la macchina da fuori. `kbs-serve` da solo non basta — il
# corpus è un albero di file e il registro è nel database, e senza il
# verbo che li unisce il corpus resta muto.
COPY --from=costruisci /build/target/release/kbs /app/kbs
# L'interfaccia e il runtime three.js viaggiano con l'immagine, ma i flag
# li dichiarano comunque: vedi la nota in testa al Dockerfile.
COPY --from=costruisci /build/crates/kbs-server/web /app/web
COPY --from=costruisci /build/vendor /app/vendor

# Il database FUORI dal corpus, perche' il daemon lo rifiuta dentro e ha
# ragione: un SQLite con i suoi -wal e -shm dentro un albero versionato è
# rumore di diff, e un registro che sparisce coi file che descrive.
RUN mkdir -p /var/lib/kbs && chown kbs:kbs /var/lib/kbs
VOLUME ["/var/lib/kbs"]

USER kbs
EXPOSE 8787

# Il corpus non viene creato: il daemon lo rifiuta se manca, e crearlo qui
# produrrebbe un server sano che serve il nulla con una causa invisibile.
ENTRYPOINT ["/app/kbs-serve"]
CMD ["--corpus", "/srv/corpus", "--db", "/var/lib/kbs/kb.sqlite3", \
     "--web", "/app/web", "--vendor", "/app/vendor", \
     "--listen", "0.0.0.0:8787"]
