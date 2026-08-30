//! Dual-core task affinity for ESP32.
//!
//! ESP32 has two cores: Core 0 (Protocol Core, handles WiFi/BLE radio) and
//! Core 1 (Application Core, runs application logic). The split ensures that
//! real-time WiFi beacon injection and BLE advertising are never blocked by
//! the scheduler, web server, or NVS writes.
//!
//! # Core Assignment
//!
//! | Core | Task | Rationale |
//! |------|------|-----------|
//! | 0 | WiFi beacon/NAN TX, BLE radio | ESP-IDF WiFi task runs here by default; radio timing is critical |
//! | 1 | Scheduler loop, web server, LED, NVS, CLI | Application logic with no timing constraints |
//!
//! ESP-IDF already routes WiFi to Core 0 internally. This module provides
//! helpers to explicitly pin FreeRTOS tasks when the default placement is
//! insufficient (e.g. a high-priority scheduler task that must not be
//! preempted by web server handlers).

use alloc::boxed::Box;
use core::ffi::c_void;

/// Core 0: Protocol/radio (WiFi TX, BLE controller).
pub const CORE_WIFI: u8 = 0;

/// Core 1: Application (scheduler, web server, LED, NVS).
pub const CORE_APP: u8 = 1;

/// Default stack size for pinned tasks (in bytes).
const TASK_STACK_SIZE: u32 = 4096;

/// Scheduler task priority (high — above web server, below radio).
const SCHEDULER_PRIORITY: u32 = 5;

/// Spawn a FreeRTOS task pinned to the application core (Core 1).
///
/// This is the recommended way to run the main scheduler loop: it guarantees
/// that WiFi/BLE radio callbacks on Core 0 never block on scheduler work,
/// and that the scheduler is never preempted by web server handlers.
///
/// The task starts immediately on the specified core. The closure must be
/// `Send + 'static` because it runs on a different thread.
pub fn spawn_pinned_task<F>(name: &[u8], core: u8, priority: u32, f: F)
where
    F: FnOnce() + Send + 'static,
{
    extern "C" fn trampoline<F: FnOnce() + Send + 'static>(arg: *mut core::ffi::c_void) {
        unsafe {
            let boxed = Box::from_raw(arg as *mut F);
            boxed();
        }
    }

    let boxed = Box::new(f);
    let arg = Box::into_raw(boxed) as *mut c_void;

    let mut handle: esp_idf_sys::TaskHandle_t = core::ptr::null_mut();
    // Issue #26: `xTaskCreatePinnedToCore` returns `pdPASS` (1) on success;
    // on failure (e.g. not enough heap for the stack) it returns
    // `pdFAIL`/`errCOULD_NOT_ALLOCATE_REQUIRED_MEMORY`. A task that fails to
    // spawn leaves the system unusable (missing scheduler/web/CLI task), so we
    // fail fast rather than silently running partially initialised.
    let ret = unsafe {
        esp_idf_sys::xTaskCreatePinnedToCore(
            Some(trampoline::<F>),
            name.as_ptr() as *const _,
            TASK_STACK_SIZE,
            arg,
            priority as u32,
            &mut handle,
            core as i32,
        )
    };
    if ret != 1 {
        let name_str = core::str::from_utf8(name).unwrap_or("<unnamed>");
        panic!("xTaskCreatePinnedToCore failed for {name_str} (ret={ret})");
    }
    debug_assert!(!handle.is_null());
}

/// Spawn the scheduler loop task pinned to Core 1.
pub fn spawn_scheduler<F: FnOnce() + Send + 'static>(f: F) {
    spawn_pinned_task(b"rid_sched\0", CORE_APP, SCHEDULER_PRIORITY, f);
}

/// Spawn a lower-priority background task pinned to a specific core.
pub fn spawn_background<F: FnOnce() + Send + 'static>(name: &[u8], core: u8, f: F) {
    spawn_pinned_task(name, core, 2, f);
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn core_constants_are_correct() {
        assert_eq!(CORE_WIFI, 0);
        assert_eq!(CORE_APP, 1);
    }

    #[test]
    fn scheduler_priority_is_above_default() {
        // Priority 5 is above FreeRTOS default (0) and below radio tasks.
        assert!(SCHEDULER_PRIORITY > 0);
        assert!(SCHEDULER_PRIORITY < 25);
    }

    #[test]
    fn task_stack_is_reasonable() {
        // 4096 bytes is enough for the scheduler loop (no deep recursion).
        assert!(TASK_STACK_SIZE >= 2048);
        assert!(TASK_STACK_SIZE <= 16384);
    }
}
