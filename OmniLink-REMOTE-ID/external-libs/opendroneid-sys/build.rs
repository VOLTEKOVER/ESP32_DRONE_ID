//! Builds the vendored Open Drone ID C library (official Intel code, see
//! `vendor/opendroneid.c` header) plus the layout probe used by the tests.
//!
//! The upstream source is pinned in `vendor/` (identical to
//! `ESP32_DRONE_REMOTE_ID_Firmware/components/esp_remote_id/src/opendroneid.c`
//! + `include/opendroneid.h`); no network fetch is required.
//!
//! Upstream: https://github.com/opendroneid/OpenDroneID
//! Auto-fetch would be:
//!   cc::Build::new().file("opendroneid.c").include(dir).compile("opendroneid");
//! here the crate stays self-contained instead.

use std::path::PathBuf;

fn main() {
    let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor");

    let mut build = cc::Build::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // The upstream header is GCC-only (__attribute__((packed)), C11):
        // force MinGW gcc instead of the MSVC toolchain cc-rs auto-picks.
        build.compiler("gcc");
    }
    build
        .file(vendor.join("opendroneid.c"))
        .file(vendor.join("layout_probe.c"))
        .include(&vendor)
        // Drop the debug print functions so no stdio is pulled in.
        .define("ODID_DISABLE_PRINTF", None)
        .warnings(false)
        .compile("opendroneid");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        // opendroneid.c uses round()/roundf() from libm on ELF targets.
        println!("cargo:rustc-link-lib=m");
    }

    println!("cargo:rerun-if-changed=vendor");
}
