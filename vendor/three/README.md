# three.js — vendorizzato (D15)

three r170, build `three.module.min.js`, scaricato il 2026-09-26 e committato nel
repository perche una scelta con garanzia di rete non puo dipendere da un CDN.

| file | byte | byte gzip |
|---|---:|---:|
| `three.module.min.js` | 691648 | 170759 |

Licenza three.js: MIT — il testo e` in `LICENSE-three.txt`.
Serving: `/three/three.module.min.js`, servito da `kbs-server` con `Cache-Control`
lungo ed `ETag`. Nessun artifact puo` referenziare un CDN esterno (D15).
