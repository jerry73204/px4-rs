//! Optional `#[panic_handler]` for pure-Rust PX4 modules.
//!
//! A PX4 module's Rust staticlib needs a panic handler because `core`
//! doesn't ship one. Mixed C++/Rust binaries can take the C++ side's
//! handler instead — that's why this lives behind the `panic-handler`
//! feature and is off by default.

use core::ffi::CStr;
use core::panic::PanicInfo;

use crate::{__log_impl, Level};

unsafe extern "C" {
    /// libc `abort(3)`. NuttX and POSIX both provide it.
    fn abort() -> !;
}

#[panic_handler]
fn on_panic(info: &PanicInfo<'_>) -> ! {
    // Log at PANIC level. Rendering goes through the same stack buffer
    // as the regular macros; `PanicInfo` implements `Display`.
    let module: &CStr = c"panic";
    __log_impl(Level::Panic, module, format_args!("{info}"));

    // SAFETY: `abort` never returns. Calling it after a panic is the
    // correct termination.
    unsafe { abort() }
}
