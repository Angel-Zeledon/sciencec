//! Helpers shared by the integration tests.
//!
//! Every helper goes through the `extern "C"` surface, never around it: the C
//! ABI is the only interface generated code will ever see, so it is the only
//! interface worth testing.

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use link_rt::*;

/// Build a `String` from a Rust `&str`, the way codegen builds a string literal.
pub fn s(text: &str) -> LinkString {
    unsafe { link_string_from_bytes(text.as_ptr(), text.len()) }
}

/// Borrow a `String`'s bytes as a Rust `&str`.
pub fn as_str(value: &LinkString) -> &str {
    unsafe {
        let bytes = std::slice::from_raw_parts(value.ptr, value.len);
        std::str::from_utf8(bytes).expect("String contents must be valid UTF-8")
    }
}

/// Release a `String`, the way drop glue would.
pub fn free(mut value: LinkString) {
    unsafe { link_string_free(&mut value) }
}

// ---------------------------------------------------------------------------
// A drop-counting element type, for proving that ownership transfer is exact.
// ---------------------------------------------------------------------------

/// Serialises the tests that observe [`DROPS`], which is process-global.
pub static DROP_LOCK: Mutex<()> = Mutex::new(());

/// Number of times [`drop_counted`] has run.
pub static DROPS: AtomicUsize = AtomicUsize::new(0);

/// An 8-byte element carrying an identity, so a test can tell elements apart.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Counted {
    pub id: u64,
}

/// Drop glue for [`Counted`]: reads the value (so Miri checks the pointer is
/// live and initialised) and bumps the counter.
pub extern "C" fn drop_counted(value: *mut u8) {
    let id = unsafe { (*value.cast::<Counted>()).id };
    // Keep the read from being optimised away.
    assert!(id != u64::MAX, "sentinel id must never reach drop glue");
    DROPS.fetch_add(1, Ordering::SeqCst);
}

pub fn counted_info() -> LinkTypeInfo {
    LinkTypeInfo {
        size: std::mem::size_of::<Counted>(),
        align: std::mem::align_of::<Counted>(),
        drop_fn: Some(drop_counted),
    }
}

pub fn reset_drops() {
    DROPS.store(0, Ordering::SeqCst);
}

pub fn drops() -> usize {
    DROPS.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// Plain, drop-free element types.
// ---------------------------------------------------------------------------

pub fn i64_info() -> LinkTypeInfo {
    LinkTypeInfo {
        size: 8,
        align: 8,
        drop_fn: None,
    }
}

/// A zero-sized element type, the representation of Link's `()`.
pub fn unit_info() -> LinkTypeInfo {
    LinkTypeInfo {
        size: 0,
        align: 1,
        drop_fn: None,
    }
}

/// An over-aligned element type, to prove alignment is honoured on every
/// reallocation rather than only on the first.
#[repr(C, align(32))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Over {
    pub id: u64,
}

pub fn over_info() -> LinkTypeInfo {
    LinkTypeInfo {
        size: std::mem::size_of::<Over>(),
        align: std::mem::align_of::<Over>(),
        drop_fn: None,
    }
}

// ---------------------------------------------------------------------------
// Hash and equality function pointers, the way codegen supplies them for `Map`.
// ---------------------------------------------------------------------------

pub extern "C" fn hash_u64(key: *const u8) -> u64 {
    unsafe { *key.cast::<u64>() }
}

pub extern "C" fn eq_u64(a: *const u8, b: *const u8) -> bool {
    unsafe { *a.cast::<u64>() == *b.cast::<u64>() }
}

/// A deliberately terrible hash: every key lands in the same bucket, so the
/// collision path is the only path exercised.
pub extern "C" fn hash_always_zero(_key: *const u8) -> u64 {
    0
}

pub extern "C" fn hash_link_string(key: *const u8) -> u64 {
    unsafe { link_string_hash(key.cast::<LinkString>()) }
}

pub extern "C" fn eq_link_string(a: *const u8, b: *const u8) -> bool {
    unsafe { link_string_eq(a.cast::<LinkString>(), b.cast::<LinkString>()) }
}

pub extern "C" fn drop_link_string(value: *mut u8) {
    unsafe { link_string_free(value.cast::<LinkString>()) }
}

pub fn string_info() -> LinkTypeInfo {
    LinkTypeInfo {
        size: std::mem::size_of::<LinkString>(),
        align: std::mem::align_of::<LinkString>(),
        drop_fn: Some(drop_link_string),
    }
}

pub fn u64_map_info() -> LinkMapInfo {
    LinkMapInfo {
        key: i64_info(),
        value: i64_info(),
        hash_fn: hash_u64,
        eq_fn: eq_u64,
    }
}

pub fn colliding_map_info() -> LinkMapInfo {
    LinkMapInfo {
        key: i64_info(),
        value: i64_info(),
        hash_fn: hash_always_zero,
        eq_fn: eq_u64,
    }
}
