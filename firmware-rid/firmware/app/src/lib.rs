//! Application assembly (Fase 5), port of the remaining glue of
//! `esp_remote_id.c` (`esp_rid_init`/`esp_rid_set_config`/
//! `esp_rid_factory_reset`) on top of the already-ported core (`rid-core`
//! scheduler/hub/readiness) and BSP-facing layer (`rid-app`).
//!
//! The `Controller` keeps the full BSP config, derives the hub-facing
//! `rid_interface::Config` and drives the scheduler. The hardware (parsers as
//! `GpsSource`, radios as `Transmitter`, NVS, web server, LEDC/RMT) is
//! injected by the BSP, so everything here is host-testable.
//!
//! Also provides the `/api/capabilities` build descriptor for the adaptive
//! web UI (Fase 5) and the JSON payloads of the three API endpoints
//! (`/api/config`, `/api/status`, `/api/capabilities`).
#![no_std]

extern crate alloc;

pub mod capabilities;
pub mod controller;

pub use capabilities::{Capabilities, capabilities_json};
pub use controller::{
    Controller, SetConfigOutcome, core_config, derive_ids_from_mac, is_placeholder_id,
    mavlink_tx_enabled,
};
