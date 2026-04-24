//! `UorbTopic` trait + `OrbMetadata` Sync wrapper.

use px4_sys::orb_metadata;

/// Sync wrapper around PX4's `orb_metadata`. PX4's struct contains a
/// `*const c_char` field, which makes the raw type `!Sync`. In practice
/// the values are read-only static data, so we mark the wrapper Sync
/// and hand out `&'static orb_metadata` references via the inner field.
#[repr(transparent)]
pub struct OrbMetadata(pub orb_metadata);

// SAFETY: `o_name` points at a `'static CStr`, the integer fields are
// POD. PX4 reads the metadata read-only across threads.
unsafe impl Sync for OrbMetadata {}

impl OrbMetadata {
    pub const fn new(meta: orb_metadata) -> Self {
        Self(meta)
    }

    pub const fn get(&'static self) -> &'static orb_metadata {
        &self.0
    }
}

/// Bridge between a generated topic ZST (e.g. `sensor_gyro`) and its
/// payload struct (`SensorGyro`) plus PX4's `orb_metadata`.
///
/// Implementations are produced by the `#[px4_message(...)]` macro —
/// one impl per entry in the message's `# TOPICS` directive.
pub trait UorbTopic: 'static {
    /// Plain-old-data payload struct — the `#[repr(C)]` type the macro
    /// emitted from the `.msg` file.
    type Msg: Copy + 'static;

    /// PX4-side metadata pointer used by `orb_*` calls.
    fn metadata() -> &'static orb_metadata;
}
