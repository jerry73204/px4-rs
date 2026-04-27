//! Safe iteration over a PX4 module's `(argc, argv)` entry-point pair.
//!
//! PX4's shell calls each module's `<name>_main(int argc, char *argv[])`
//! C entry. The C signature is non-negotiable, but the body shouldn't
//! have to touch raw pointers — `Args` gives module authors a `&CStr`
//! iterator with a fast `subcommand()` shortcut for the ubiquitous
//! `start | stop | status` dispatch.
//!
//! `Args` is `Copy` and zero-sized beyond the (argc, argv) pair, so
//! it's free to pass around and re-iterate.

use core::ffi::{CStr, c_char, c_int};
use core::marker::PhantomData;

/// View of a PX4 module entry point's `argv`.
///
/// Holds the raw `(argc, argv)` pair plus a phantom lifetime tying
/// the `&CStr`s it hands out to the entry function's stack frame.
/// Construct with [`Args::from_raw`] inside an `unsafe { }` block —
/// PX4's shell upholds the C argv contract; the rest of the module
/// stays safe.
#[derive(Copy, Clone)]
pub struct Args<'a> {
    argc: c_int,
    argv: *mut *mut c_char,
    _life: PhantomData<&'a CStr>,
}

impl<'a> Args<'a> {
    /// Wrap a C-style `(argc, argv)` pair.
    ///
    /// # Safety
    ///
    /// `argv` must point to an array of at least `argc` C-string
    /// pointers, each either NULL or a NUL-terminated `c_char` array.
    /// PX4's shell satisfies this contract; in handwritten tests, the
    /// caller does. The returned `Args` borrows for `'a`, which the
    /// caller pins to the entry function's stack scope.
    pub unsafe fn from_raw(argc: c_int, argv: *mut *mut c_char) -> Self {
        Self {
            argc,
            argv,
            _life: PhantomData,
        }
    }

    /// Number of arguments (PX4 shell's `argc`, including `argv[0]`).
    pub fn len(&self) -> usize {
        if self.argc <= 0 || self.argv.is_null() {
            0
        } else {
            self.argc as usize
        }
    }

    /// True when the entry was called with no arguments at all (or
    /// with a malformed argv).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the `n`-th argument as a `&CStr`. Returns `None` if `n`
    /// is out of bounds or the slot is NULL.
    pub fn get(&self, n: usize) -> Option<&'a CStr> {
        if n >= self.len() {
            return None;
        }
        // SAFETY: from_raw's contract: argv has at least argc valid
        // pointer slots, each either NULL or a NUL-terminated string.
        unsafe {
            let p = *self.argv.add(n);
            if p.is_null() {
                None
            } else {
                Some(CStr::from_ptr(p))
            }
        }
    }

    /// Iterator over all arguments as `&CStr`. NULL slots are
    /// skipped — they're always programmer error and silently
    /// dropping them keeps the call sites simpler.
    pub fn iter(&self) -> Iter<'a> {
        Iter {
            args: *self,
            idx: 0,
        }
    }

    /// The PX4 subcommand — `argv[1]` as raw bytes (no NUL).
    /// Returns `None` if the entry was called with no subcommand.
    /// Designed for the typical `match args.subcommand() { Some(b"start") => … }`
    /// dispatch.
    pub fn subcommand(&self) -> Option<&'a [u8]> {
        self.get(1).map(|s| s.to_bytes())
    }
}

impl<'a> IntoIterator for Args<'a> {
    type Item = &'a CStr;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &Args<'a> {
    type Item = &'a CStr;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

/// Iterator over the C strings in an [`Args`].
pub struct Iter<'a> {
    args: Args<'a>,
    idx: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a CStr;

    fn next(&mut self) -> Option<&'a CStr> {
        loop {
            if self.idx >= self.args.len() {
                return None;
            }
            let i = self.idx;
            self.idx += 1;
            if let Some(s) = self.args.get(i) {
                return Some(s);
            }
            // NULL slot — keep going.
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn argv_from(strs: &[&str]) -> (Vec<CString>, Vec<*mut c_char>) {
        let owned: Vec<CString> = strs.iter().map(|s| CString::new(*s).unwrap()).collect();
        let ptrs: Vec<*mut c_char> = owned
            .iter()
            .map(|s: &CString| s.as_ptr() as *mut c_char)
            .collect();
        (owned, ptrs)
    }

    #[test]
    fn subcommand_returns_argv_1_bytes() {
        let (_owned, mut ptrs) = argv_from(&["hello_module", "start"]);
        let args = unsafe { Args::from_raw(ptrs.len() as c_int, ptrs.as_mut_ptr()) };
        assert_eq!(args.subcommand(), Some(&b"start"[..]));
    }

    #[test]
    fn empty_argv_is_empty() {
        let args = unsafe { Args::from_raw(0, core::ptr::null_mut()) };
        assert!(args.is_empty());
        assert_eq!(args.subcommand(), None);
    }

    #[test]
    fn missing_subcommand_returns_none() {
        let (_owned, mut ptrs) = argv_from(&["hello_module"]);
        let args = unsafe { Args::from_raw(ptrs.len() as c_int, ptrs.as_mut_ptr()) };
        assert_eq!(args.subcommand(), None);
    }

    #[test]
    fn iter_skips_null_slots() {
        let (owned, mut ptrs) = argv_from(&["a", "b", "c"]);
        ptrs[1] = core::ptr::null_mut();
        let args = unsafe { Args::from_raw(ptrs.len() as c_int, ptrs.as_mut_ptr()) };
        let collected: Vec<&[u8]> = args.iter().map(|s| s.to_bytes()).collect();
        assert_eq!(collected, vec![&b"a"[..], &b"c"[..]]);
        drop(owned);
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let (_owned, mut ptrs) = argv_from(&["x"]);
        let args = unsafe { Args::from_raw(ptrs.len() as c_int, ptrs.as_mut_ptr()) };
        assert!(args.get(0).is_some());
        assert!(args.get(1).is_none());
        assert!(args.get(99).is_none());
    }
}
