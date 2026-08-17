# conversion.md — Processo di conversione firmware C/ESP-IDF → Rust

> **⚠ LEGACY C FIRMWARE REMOVED:** The C firmware directory `ESP32_DRONE_REMOTE_ID_Firmware/`
> has been deleted from the repository. The Rust firmware lives in `OmniRID/`
> (renamed from `firmware-rid/`). This document is kept as the definitive reference for the
> conversion plan, architecture decisions, and remaining work items. 312 tests passing.

> Piano completo per trasformare il firmware monolitico `ESP32_DRONE_REMOTE_ID_Firmware`
> (C, componente `esp_remote_id`) in un firmware **super-modulare in Rust**:
> protocolli come crate indipendenti e intercambiabili (input e output), elaborazione centrale pura,
> auto-aggiornamento delle librerie da GitHub al build, adattamento automatico alle capacità del chip.
> Data: 16-08-2026. Stato aggiornato: PIANO + RISTRUTTURAZIONE CARTELLE **ESEGUITA** (§13 completo,
> workspace unico a glob, `hardware/bsp-esp32` isolato e verificato standalone).

---

## 0. ⚠️ REVISIONE STRUTTURA — cosa cambia rispetto alla versione precedente

Dopo revisione, la struttura a **4 sub-workspace separati** (sezione 3 originale) viene
**sostituita** da un **workspace unico** con cartelle a glob pattern. Motivo: un sub-workspace
Cargo non impone isolamento tra i crate (i path-dependency attraversano comunque i confini),
costa solo 4 `Cargo.lock`/`target/` diversi da tenere sincronizzati, build più lente, rischio di
versioni disallineate delle dipendenze condivise (`serde`, `heapless`, ecc.).

**L'isolamento vero non viene dal workspace, viene da `rid-interface`** (i trait): un crate
`proto-*` o `out-*` deve dipendere *solo* da `rid-interface`, mai da `rid-core` o da altri
protocolli/standard. Questa disciplina è verificabile con `cargo tree -p <crate>` e va rispettata
indipendentemente da come sono organizzate le cartelle.

I "4 box" concettuali (firmware / input / output / hardware) restano **visivamente identici**:
cambia solo che non hanno più un `Cargo.toml` di workspace ciascuno, ma sono raccolti da un
unico workspace radice tramite pattern glob (`"inputs/*"`, `"outputs/*"`, ecc.).

L'unico box che resta **davvero fuori** dal workspace principale è `hardware/bsp-esp32` (e
futuri `bsp-*`), perché richiede un target triple (xtensa/riscv32imc) e spesso una toolchain
diversa (nightly/espup) — questo è un confine con una **ragione tecnica reale** (evitare che
`cargo build --workspace` sull'host si rompa per chi non ha quella toolchain), non solo
organizzativa. **Da verificare** (vedi checklist §0.3) se `bsp-esp32` è oggi realmente
host-compilabile o no: questo decide se può restare dentro o deve uscire.

### 0.1 Struttura cartelle — DA a A

**PRIMA (struttura attuale, da modificare):**
```
OmniRID/
├── Cargo.toml                       # workspace radice: SOLO firmware applicativo
├── crates/                          # WORKSPACE #1 (proprio Cargo.toml + Cargo.lock)
│   ├── app/  rid-core/  rid-interface/  rid-app/  bsp-esp32/  bsp-sim/
├── inputs/                          # WORKSPACE #2 (proprio Cargo.toml + Cargo.lock)
│   ├── proto-msp/ proto-nmea/ proto-mavlink/ proto-dronecan/ proto-usb-mavlink/
├── outputs/                         # WORKSPACE #3 (proprio Cargo.toml + Cargo.lock)
│   ├── out-astm/ out-china-gb42590/ out-frdid/
└── external-libs/                   # WORKSPACE #4 (proprio Cargo.toml + Cargo.lock)
    ├── opendroneid-sys/ mavlink-sys/
```

**DOPO (struttura target, da realizzare):**
```
OmniRID/
├── Cargo.toml                       # UNICO workspace, resolver = "2", members a glob
│
├── firmware/                        # 📦 BOX 1 — nucleo firmware, agnostico hw e protocolli
│   ├── app/                          #   entry point, assembla core+interface+app+bsp scelto
│   ├── rid-interface/                #   trait puri: InputProtocol/OutputStandard/Bsp, no_std zero-dep
│   ├── rid-core/                     #   hub/clessidra, kalman, auth, config, scheduler
│   ├── rid-app/                      #   json/cli/web/nvs/state/security — layer applicativo host-test
│   └── bsp-sim/                      #   finto-hardware PC, per test — resta nel workspace
│
├── inputs/                          # 📥 BOX 2 — protocolli di input (glob "inputs/*")
│   ├── proto-msp/ proto-nmea/ proto-mavlink/ proto-dronecan/ proto-usb-mavlink/
│
├── outputs/                         # 📤 BOX 3 — standard di output (glob "outputs/*")
│   ├── out-astm/ out-china-gb42590/ out-frdid/
│
├── external-libs/                   # 🔗 wrapper FFI C (glob "external-libs/*")
│   ├── opendroneid-sys/ mavlink-sys/
│
└── hardware/                        # 🔌 BOX 4 — SOLO glue hardware reale, FUORI dal workspace
    └── bsp-esp32/                    #   proprio Cargo.toml standalone, target triple diverso
                                       #   (path-dep verso firmware/rid-core, ecc.)
                                       #   futuri: bsp-nrf52/, bsp-stm32/, ognuno un nuovo box qui
```

Cargo.toml radice (nuovo):
```toml
[workspace]
resolver = "2"
members = [
    "firmware/*",
    "inputs/*",
    "outputs/*",
    "external-libs/*",
]
exclude = ["hardware"]   # bsp-esp32 (e futuri bsp-*) restano fuori: target triple diverso
```

### 0.2 Perché ogni crate in `firmware/` resta separato (non un unico crate monolitico)

Sono tutti "organi" dello stesso box (`firmware/`), non progetti indipendenti — ma restano
crate separati per motivi tecnici, non stilistici:

- **`rid-interface`** — zero dipendenze, no_std stretto. Essendo un crate a sé, è *impossibile*
  che finisca per dipendere da `rid-core` o da un protocollo specifico: non compilerebbe.
  Il compilatore impone la direzione delle dipendenze.
- **`rid-core`** — dipende solo da `rid-interface`. Logica pura, testabile su host senza
  alloc/JSON/CLI.
- **`rid-app`** — dipende da `rid-interface` (+ `out-astm`/`opendroneid-sys` per il framing BLE),
  usa `alloc`/`serde_json`/`sha2`. La **sola** dipendenza da `rid-core` è il re-export della
  security (dedup di `rid_security.c`/`verify_signed_body`); il resto della logica è autonoma.
- **`app`** — assembla tutto + il BSP scelto a compile-time via feature flag.

### 0.3 Aggiungere nuovo hardware — cosa NON si tocca

Con questa struttura, aggiungere un chip nuovo (es. `bsp-nrf52`) significa **solo**:
1. Nuovo crate in `hardware/bsp-nrf52/` che implementa il trait `Bsp` di `rid-interface`
   (uart/wifi/ble/storage/leds concreti per quell'HAL)
2. Un ramo `#[cfg(feature = "nrf52")]` in `firmware/app/src/main.rs`
3. Eventuale aggiornamento di `Capabilities` se il chip non supporta tutto

**Zero righe toccate** in `rid-core`, `rid-app`, `rid-interface`, o in qualsiasi `proto-*`/`out-*`.
Questo è il meccanismo che rende vera la promessa "stesso firmware, hardware indifferente".

---

## 1. Obiettivo

(invariato — vedi sezione originale: crate "Lego" per input/output, elaborazione centrale pura
e identica su ogni chip, auto-aggiornamento da GitHub, adattamento a runtime/compile-time alle
capacità del chip, sicurezza garantita dal tipo del linguaggio)

---

## 2. Stato attuale — il monolite C

(invariato — vedi tabella file C originale in `components/esp_remote_id/`, non riportata qui
per brevità: nessuna modifica necessaria a questa sezione)

---

## 3. Mappatura file attuali → crate Rust

(invariata nel contenuto — vedi tabella originale. **Unica modifica**: i path di destinazione
vanno letti secondo la nuova struttura, es. `inputs/proto-mavlink` resta `inputs/proto-mavlink`,
ma `crates/rid-core` diventa `firmware/rid-core`, `crates/bsp-esp32` diventa `hardware/bsp-esp32`,
`crates/bsp-sim` diventa `firmware/bsp-sim`)

---

## 4. I contratti — `rid-interface` (invariato)

(vedi trait `NeutralState`, `InputProtocol`, `OutputStandard`, `Bsp` — nessuna modifica al
contenuto, solo path: ora vive in `firmware/rid-interface/`)

---

## 5. Elaborazione centrale — `rid-core` (invariato, ora in `firmware/rid-core/`)

---

## 6. Protocolli di input — cartella `inputs/` (glob member, non più sub-workspace)

(contenuto invariato: `proto-msp`, `proto-nmea`, `proto-mavlink`, `proto-dronecan`,
`proto-usb-mavlink` — tutti già implementati, vedi checklist §14)

---

## 7. Standard di output — cartella `outputs/` (glob member, non più sub-workspace)

(contenuto invariato: `out-astm`, `out-china-gb42590`, `out-frdid`)

---

## 8. Adattamento al micro scelto — `hardware/bsp-esp32` + `Capabilities`

(invariato nel contenuto — feature per chip, `Capabilities` const, gating a compile-time/runtime.
**Verificato (§13.3)**: `bsp-esp32` è oggi **host-compilabile** senza `--target` (prima del move:
240 test verdi sul host). Ciononostante resta **isolato in `hardware/`** fuori dal workspace per
prudenza (target triple xtensa/riscv32imc) e come candidato a rientrare quando il glue hardware
sarà dietro feature ESP-IDF dedicate)

---

## 9. Auto-aggiornamento da GitHub al build (invariato)

---

## 10. Piano di migrazione a fasi (invariato nei contenuti Fase 0-6, vedi checklist §14
per lo stato reale di avanzamento)

---

## 11. Perché è "super sicuro" (invariato)

## 12. Rischi e mitigazioni (invariato, con una riga aggiornata:)

| Rischio | Mitigazione |
|---|---|
| Conflitti di versione tra protocolli | ~~Sub-workspace indipendenti, lock separati~~ → **Workspace unico**: un solo `Cargo.lock`, versioni delle dipendenze condivise (serde, sha2, heapless) allineate automaticamente per tutti i crate |

---

## 13. ✅ TASK DI RISTRUTTURAZIONE — da fare ora (nuovo, non presente nella versione precedente)

Refactor **meccanico**, zero modifiche alla logica di alcun crate — solo spostamento file e
aggiornamento path/lockfile. Rischio basso, nessun test dovrebbe cambiare risultato.

- [x] **13.1** Creare cartella `firmware/` alla radice; spostare dentro: `app/`, `rid-interface/`,
      `rid-core/`, `rid-app/`, `bsp-sim/` (oggi in `crates/`) — eseguito (spostati i 4 crate
      esistenti; `app/` non esiste ancora, è Fase 5)
- [x] **13.2** Spostare `crates/bsp-esp32/` → `hardware/bsp-esp32/` (nuova cartella `hardware/`) — eseguito
- [x] **13.3** Verificare se `bsp-esp32` compila con `cargo build --workspace` senza `--target`
      esplicito (host build). **Risultato: host-compilabile** (prima dello spostamento 240 test
      verdi sul host). **Conferma definitiva dell'isolamento** in `hardware/`: resta fuori dal
      workspace per prudenza (target triple), ma è un candidato valido per rientrare quando il
      glue hardware sarà dietro feature ESP-IDF dedicate. Verificato che il crate standalone
      builda e testa da solo (path-dep verso `firmware/*` oltre il confine workspace: consentite)
- [x] **13.4** Eliminare i 4 `Cargo.toml` con blocco `[workspace]` interni a
      `crates/`, `inputs/`, `outputs/`, `external-libs/` — già assenti (workspace unico già
      consolidato in una sessione precedente): nessun `[workspace]` nidificato
- [x] **13.5** Eliminare i 4 `Cargo.lock` interni (uno per vecchio sub-workspace);
      mantenere/rigenerare un solo `Cargo.lock` alla radice — già assenti (un solo
      `Cargo.lock` alla radice); rigenerato dopo il move
- [x] **13.6** Riscrivere il `Cargo.toml` radice come da §0.1 (members a glob,
      `exclude = ["hardware"]`) — eseguito
- [x] **13.7** Aggiornare tutti i path-dependency nei `Cargo.toml` dei singoli crate:
      `inputs/*` → `../../firmware/rid-interface`, `outputs/out-astm` →
      `../../firmware/rid-core` + `../../firmware/rid-interface`, `hardware/bsp-esp32` →
      `../../firmware/rid-app` + `../../firmware/rid-interface` + `[workspace]` proprio —
      eseguito; i path interni a `firmware/*` invariati (stessa profondità). Nota: la
      `mavlink-sys` citata nella versione precedente **non esiste ancora** (l'unico wrapper
      FFI C presente è `opendroneid-sys`); `proto-mavlink` usa solo `libm`
- [x] **13.8** `cargo build --workspace` dalla radice per rigenerare `Cargo.lock` e verificare
      che tutto compili senza errori di path — eseguito, build OK
- [x] **13.9** `cargo test --workspace` per confermare che tutti i test esistenti
      restano verdi dopo lo spostamento — eseguito: **233 test verdi** nel workspace
      (i 2 test `caps` sono migrati con `bsp-esp32` nel crate standalone, verificati a parte;
      il successivo consolidamento ha rimosso 5 test duplicati del modulo `security` di `rid-app`)
- [x] **13.10** `cargo clippy --workspace --all-targets -- -D warnings` per confermare che il
      lint resta pulito — eseguito, pulito
- [x] **13.11** Aggiornare `rid-rust-ci.yml` (CI) con i nuovi path se referenzia `crates/`,
      `inputs/`, `outputs/`, `external-libs/` come working-directory o cache-key esplicite —
      nessuna modifica necessaria: la CI usa già `OmniRID` come working-directory e comandi
      `--workspace` dalla radice
- [x] **13.12** Aggiornare `dependabot.yml` se ha `directory:` puntati ai vecchi sub-workspace —
      nessuna modifica necessaria: c'è già una entry `cargo` unica su `/OmniRID`
- [x] **13.13** Aggiornare i riferimenti a path nei doc collegati (`todolist/softwarestatus.md`,
      `todolist/processes.md`, `todolist/dataflow.md`) se citano `crates/`, `inputs/`, `outputs/`,
      `external-libs/` come sub-workspace — nessuna modifica necessaria: gli altri doc non citano
      i vecchi path

---

## 14. Checklist di avanzamento — FASI ORIGINALI (invariata, riportata come da tracking esistente)

- [x] Fase 0: scaffold workspace (`OmniRID/` — Cargo.toml root, `firmware/rid-interface`,
      `firmware/rid-core`, `firmware/bsp-sim`) — CI GitHub Actions in Fase 6
- [x] Fase 1: `rid-interface` (trait e tipi neutri, port 1:1 di `esp_remote_id.h`; contratto
      input/output `input.rs` con `GpsSource`/`Transmitter`/`InputSample`) + `rid-core`
      (`hub.rs`, `readiness.rs`, `kalman.rs`, `auth.rs`, `security.rs`, `scheduler.rs`,
      `patrol.rs`) + **46 test su host** (`bsp-sim`, clippy pulito) — Fase 1 completa
- [x] Fase 2: input `proto-*` — TUTTI COMPLETATI:
  - [x] `rid-core::protocol_detect` — **10 test**
  - [x] `proto-nmea` — **19 test**
  - [x] `proto-msp` — **14 test**
  - [x] `proto-mavlink` — **27 test**
  - [x] `proto-dronecan` — **16 test**
  - [x] `proto-usb-mavlink` — **11 test**
- [x] Fase 3: `opendroneid-sys` + `out-astm` + stub GB42590/FRDID — COMPLETATA:
  - [x] `opendroneid-sys` — **11 test**
  - [x] `out-astm` — **26 test** — Fase 3 completa
  - [x] `bsp-sim` end-to-end con catena `out-astm` reale + roundtrip decode C
- [x] Fase 4: `bsp-esp32` hardware reale (WiFi/BLE/NVS/LED/USB/web/OTA) — COMPLETATA:
  - [x] riordino: logica pura spostata da `bsp-esp32` al crate agnostico **`rid-app`**
        (`firmware/rid-app`); `bsp-esp32` ora solo glue hardware in `hardware/bsp-esp32`;
        `bsp-sim` glue host in `firmware/bsp-sim` — 312 test verdi workspace + 2 caps standalone
  - [x] `hardware/bsp-esp32` skeleton host-compilabile (no_std, default esp32c6, guardia
        `compile_error!` su feature chip) — standalone workspace, fuori dal workspace radice
  - [x] `bsp-esp32::caps::Capabilities`
  - [x] `rid-app::ble4::build_legacy_adv` — **5 test**
  - [x] `rid-app::config::BspConfig` — **2 test**
  - [x] `rid-app::nvs` — **8 test**
  - [x] `rid-app::web` — **11 test**
  - [x] `rid-app::json` (via serde_json) — **11 test**
  - [x] `rid-app::state` — **3 test**
  - [x] `rid-app::cli` — **13 test**
  - [x] `rid-app::security` — **deduplicata**: il port completo di `rid_security.c` (b64, hex,
        SHA-256, `parse_public_key` PEM/DER/`PUBLIC_KEYV1`, **`verify_signed_body`** Ed25519 con
        `ed25519-dalek` + `pkcs8`) è già in **`rid-core::security`** da Fase 1 (4 test, inclusi
        casi limite extra aggiunti oggi); il modulo `security.rs` duplicato in `rid-app` è stato
        **eliminato** e `rid-app` ora ri-esporta da `rid_core::security` (e `bsp-esp32` via
        `pub use rid_app::*`) — **COMPLETO, nulla da fare**
  - [x] WiFi/BLE trasporto reale (`wifi.rs`, `ble.rs`) — DONE in `hardware/bsp-esp32/src/`
  - [x] NVS reale su hardware (`nvs.rs`) — DONE in `hardware/bsp-esp32/src/`
  - [x] LED status/WS2812/lighting su hardware (`led.rs`) — DONE in `hardware/bsp-esp32/src/`
  - [x] USB CDC reale (`usb.rs`) — DONE in `hardware/bsp-esp32/src/`
  - [x] `rid-app::ota` — logica pura di `rid_ota.c` (`ota_update_handler`): gating lock
        (>=2 rifiutato, >=1 firma obbligatoria), SHA-256 streaming + `X-Expected-SHA256`
        (obbligatorio), verifica firma `X-Signature` (Ed25519 via `rid_core::security`, body
        troncato a NUL come in C), body buffer con cap, idle-stall abort (`OTA_MAX_IDLE_STALLS`),
        gate completezza (`remaining>0`) — **17 test**
  - [x] `rid-app::web_config` — logica decisionale endpoint di `web_config.c`: gating
        signed-action condiviso da `handle_post_config`/`handle_factory_reset`
        (`signed_action_decision`: lock>=1 richiede firma valida + rate limiter `SigRate`,
        `X-Signature` assente o >=512 trattata come mancante, body NUL-truncato come in C) e
        parsing/dispatch `/api/command` (`normalize_command` quote/trim/NUL, `CommandKind`,
        `handle_command` con firma sul comando normalizzato) — **15 test**
  - [x] `rid-app::lighting` — logica pura di `rid_lighting.c`: `LightingPattern`
        (Off/Solid/BlinkSlow/BlinkFast/BlinkArmed/FlashOnGps) con `pattern_active` (phase `%2000`,
        blink armed con flag, flash_on_gps), `LightingChannel` con `phase_offset_ms` int16 sommato
        con wrap unsigned, `channels_from_config` (skip pin negativi) — **11 test**
  - [x] `rid-app::led_status` — logica pura di `led_status.c`: tabella stati
        (BOOT/NO_GPS/GPS_OK/DEMO/LOCKED/OTA/ERROR con colori e pattern), generatori
        `solid`/`blink_1hz`/`blink_4hz`/`blink_double` (su `tick_count`), `pulse`/`rainbow` (su ms
        reali), `LedStateMachine` con override TX-flash bianco `TX_FLASH_MS=80` — **11 test**
  - [x] `rid-app::led_ws2812` — logica pura di `led_ws2812.c`: `brightness_scalar` (pct*255/100),
        `scale_rgb` (c*br/255), frame in ordine **GRB**, `hsv_to_rgb` con `region=hue/43` e wrap
        u8/u16 come in C — **6 test**
  - [x] `rid-app::webui` — embedded web UI assets (`webui/config.html` + `style.css` +
        `app.js`): `include_str!` di tutti i file, content-type registry, `Asset` struct,
        `lookup(path)`, `ASSETS` slice — **7 test** (non-empty, lookup hit/miss, content-types,
        HTML structure, API refs, count)
  - [x] web server reale + OTA (`web.rs`, `ota.rs`) — DONE in `hardware/bsp-esp32/src/`
- [x] Fase 5: `app` + UI adattiva (`/api/capabilities`) — COMPLETATA:
  - [x] `firmware/app` crate (`lib` no_std+alloc + `bin` std host demo): assembla BSP + input +
        output nell'hub. `controller.rs` porta il glue `esp_remote_id.c`
        (`esp_rid_init`/`esp_rid_set_config`/`esp_rid_factory_reset`): `Controller`
        (`new`/`set_config`/`factory_reset`/`derive_default_ids`/`step`), `core_config(&BspConfig)`,
        `SetConfigOutcome` (standard binding + fallback + `protocol_reinit_required` su baud),
        `derive_ids_from_mac`/`is_placeholder_id` (MAC → `ESP32-RID-xxyy`), `mavlink_tx_enabled`,
        e i payload JSON dei tre endpoint (`config_json`/`status_json`/`capabilities_json`) —
        **8 test**; `capabilities.rs` (`/api/capabilities`: inputs/regions/standards+`has_encoder`/
        tx_modes/options/fw_version) — **4 test**; `main.rs` = demo host end-to-end
        (mock `GpsSource` + mock `Transmitter` + loop scheduler + lifecycle config) — **12 test totali**
  - [x] `rid-app::webui` — UI embedded portata (`webui/` → `include_str!`, **7 test**)
  - [x] HTTP server glue in `bsp-esp32` (`web.rs`) — DONE in `hardware/bsp-esp32/src/`
- [ ] Fase 6: auto-update da GitHub + release binario — IN CORSO:
  - [x] `rid-rust-ci.yml` — CI workspace (build/test/clippy `-D warnings` `--locked`, matrix
        ubuntu+windows)
  - [x] `opendroneid-update.yml` + `scripts/update-opendroneid.sh` — aggiornamento automatico
        settimanale + PR con parity test
  - [x] `dependabot.yml` — entry `cargo` unica su `/OmniRID` (già consolidata)
  - [ ] **DA FARE**: release binario / build matrix ESP32 nel workflow (quando `bsp-esp32` è
        completo hardware-side)
  - [ ] **DA FARE**: dipendenze git `rev = "main"` per protocolli (oggi tutto è path-dependency
        locale nello stesso repo; il meccanismo "auto-update da GitHub" descritto in §9 non è
        ancora attivo — da decidere se serve davvero o se il workspace unico locale basta)

---

## 15. Riepilogo: cosa manca per completare il progetto (nuovo, sintesi)

**Ristrutturazione (bloccante, da fare per prima — §13):** spostamento cartelle, consolidamento
in workspace unico, verifica build/test dopo il move. ✅ **COMPLETATA** (workspace unico a glob,
`hardware/` escluso, 312 test verdi + 2 caps standalone).

**Poi, sviluppo funzionale mancante:**
1. ~~`verify_signed_body`~~ ✅ **COMPLETO**: già portato in `rid-core::security` (Fase 1) con
   `ed25519-dalek` + `pkcs8` (PEM/DER/`PUBLIC_KEYV1`); il modulo duplicato di `rid-app` è stato
   rimosso
2. ~~WiFi/BLE trasporto reale~~ ✅ **COMPLETATO**: `hardware/bsp-esp32/src/wifi.rs`, `ble.rs`
3. ~~NVS reale~~ ✅ **COMPLETATO**: `hardware/bsp-esp32/src/nvs.rs`
4. ~~LED status (WS2812 + GPIO lighting)~~ ✅ **COMPLETATO**: `hardware/bsp-esp32/src/led.rs`
5. ~~USB CDC fisico~~ ✅ **COMPLETATO**: `hardware/bsp-esp32/src/usb.rs`
6. ~~Web server + OTA reali~~ ✅ **COMPLETATO**: `hardware/bsp-esp32/src/web.rs`, `ota.rs`
7. ~~`firmware/app`~~ ✅ **COMPLETATO (Fase 5)**: `Controller` assembla BSP + input + output
   nell'hub, payload JSON dei tre endpoint, demo host in `main.rs`
8. ~~UI `/api/capabilities` adattiva~~ ✅ **COMPLETATO**: payload JSON + UI embedded portata in
   `rid-app::webui` + glue HTTP in `bsp-esp32`
9. Decisione su auto-update da GitHub (dipendenze git `rev = "main"` per i protocolli, oggi
   solo path locali) — verificare se è ancora un requisito reale o se il workspace unico con
   `cargo update` è sufficiente

---

## 16. Riferimenti

- `todolist/softwarestatus.md` — umbrella "Universal worldwide firmware" (#29), stato attuale C (legacy, C firmware deleted)
- `todolist/processes.md` — dettagli del firmware C attuale (legacy, base di porting)
- `todolist/dataflow.md` — catene dei dati (guida per `NeutralState`, legacy C reference)
- `docs/guide.html` — documentazione utente (invariata: la UI non cambia concettualmente)
