//! The one entry point the bundled `random` module's `Stream.from_entropy()`
//! is written over.
//!
//! # Not codegen support, and not in `RUNTIME`
//!
//! As with `file.rs`'s three: the caller is **Science source** — `random`'s
//! `unsafe extern "C" library "science-rt":` block — and never emitted code,
//! so `science-codegen`'s `RUNTIME` does not list it and
//! `science-codegen-llvm`'s `tests/symbols.rs` names it in its `LIBRARY`
//! exemption instead.
//!
//! # Where the bits come from
//!
//! **Decision. `std`'s `RandomState`, hashed over nothing.** `RandomState::new`
//! keys SipHash from the operating system's random source on every platform
//! `std` supports (`getrandom`, `SecRandomCopyBytes`, `BCryptGenRandom`), and
//! later calls in one thread step that key, so two calls answer differently.
//!
//! **Reason.** It is the operating system's entropy with no dependency and no
//! per-platform code here. `science-rt` has no dependencies at all, and a
//! seed for a non-cryptographic generator does not justify the first one.
//!
//! **Cost.** Sixty-four bits, through SipHash, are not a cryptographic key and
//! are not offered as one: `random` says so in its header, and secrets are
//! `crypto`'s (`stdlib-standard.md` §13.4).

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

/// Sixty-four bits of operating-system entropy, for `Stream.from_entropy()`.
#[no_mangle]
pub extern "C" fn science_random_entropy() -> u64 {
    RandomState::new().hash_one(())
}
