//! Work-queue configurations. Mirrors
//! `platforms/common/include/px4_platform_common/px4_work_queue/WorkQueueManager.hpp`.

use core::ffi::CStr;

use px4_sys::px4_rs_wq_config;

/// Wrapper around PX4's `wq_config_t`. Layout-compatible via `#[repr(C)]`
/// and verified at compile time by `wrapper.cpp`.
#[derive(Copy, Clone)]
#[repr(C)]
pub struct WqConfig {
    inner: px4_rs_wq_config,
}

impl WqConfig {
    pub const fn new(name: &'static CStr, stacksize: u16, relative_priority: i8) -> Self {
        Self {
            inner: px4_rs_wq_config {
                name: name.as_ptr(),
                stacksize,
                relative_priority,
            },
        }
    }

    #[doc(hidden)]
    pub const fn as_ffi(&self) -> *const px4_rs_wq_config {
        &self.inner as *const _
    }
}

// SAFETY: WqConfig holds a static C string pointer and POD integers.
// PX4's WorkQueueManager reads these fields from arbitrary threads.
unsafe impl Send for WqConfig {}
unsafe impl Sync for WqConfig {}

/// Canonical PX4 work queues. Values transcribed from
/// `WorkQueueManager.hpp` for PX4 v1.16.2 — the list has been
/// append-only for years, so this hand-authored copy is low-risk.
///
/// Naming: constants use PX4's original C++ identifier (`rate_ctrl`,
/// `SPI0`, `hp_default`, …). Casing is therefore not `SCREAMING_SNAKE_CASE`;
/// clippy is silenced at the module level.
#[allow(non_upper_case_globals)]
pub mod wq_configurations {
    use super::WqConfig;

    pub const rate_ctrl: WqConfig = WqConfig::new(c"wq:rate_ctrl", 3150, 0);

    pub const SPI0: WqConfig = WqConfig::new(c"wq:SPI0", 2392, -1);
    pub const SPI1: WqConfig = WqConfig::new(c"wq:SPI1", 2392, -2);
    pub const SPI2: WqConfig = WqConfig::new(c"wq:SPI2", 2392, -3);
    pub const SPI3: WqConfig = WqConfig::new(c"wq:SPI3", 2392, -4);
    pub const SPI4: WqConfig = WqConfig::new(c"wq:SPI4", 2392, -5);
    pub const SPI5: WqConfig = WqConfig::new(c"wq:SPI5", 2392, -6);
    pub const SPI6: WqConfig = WqConfig::new(c"wq:SPI6", 2392, -7);

    pub const I2C0: WqConfig = WqConfig::new(c"wq:I2C0", 2336, -8);
    pub const I2C1: WqConfig = WqConfig::new(c"wq:I2C1", 2336, -9);
    pub const I2C2: WqConfig = WqConfig::new(c"wq:I2C2", 2336, -10);
    pub const I2C3: WqConfig = WqConfig::new(c"wq:I2C3", 2336, -11);
    pub const I2C4: WqConfig = WqConfig::new(c"wq:I2C4", 2336, -12);

    pub const nav_and_controllers: WqConfig = WqConfig::new(c"wq:nav_and_controllers", 2240, -13);

    pub const INS0: WqConfig = WqConfig::new(c"wq:INS0", 6000, -14);
    pub const INS1: WqConfig = WqConfig::new(c"wq:INS1", 6000, -15);
    pub const INS2: WqConfig = WqConfig::new(c"wq:INS2", 6000, -16);
    pub const INS3: WqConfig = WqConfig::new(c"wq:INS3", 6000, -17);

    pub const hp_default: WqConfig = WqConfig::new(c"wq:hp_default", 2800, -18);

    pub const uavcan: WqConfig = WqConfig::new(c"wq:uavcan", 3624, -19);

    pub const lp_default: WqConfig = WqConfig::new(c"wq:lp_default", 3500, -50);

    pub const test1: WqConfig = WqConfig::new(c"wq:test1", 2000, 0);
    pub const test2: WqConfig = WqConfig::new(c"wq:test2", 2000, 0);
}
