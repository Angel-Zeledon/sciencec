//! `Box[T]`: a single heap allocation.
//!
//! # Representation
//!
//! A `Box[T]` **is** a `*mut T`. There is no header, no length, no reference
//! count and no wrapper struct: one word, pointing at one allocation of exactly
//! `size` bytes at `align`, which the box owns.
//!
//! The pointer is **never null**, including for a zero-sized `T`, where it is
//! dangling but aligned. That is what gives `Box[T]` the niche described in the
//! crate documentation, §5.2, and hence what makes `Option[Box[T]]` cost the
//! same as `Box[T]`: `None` is the null pointer. Codegen may rely on it
//! everywhere.
//!
//! `Box[dyn Trait]` (§4.3) is the one case that is wider: a pointer to the
//! value and a pointer to the vtable, in that order. The data pointer carries
//! the niche, so `Option[Box[dyn Trait]]` is still two words. The vtable itself
//! is codegen's to emit and lay out; the runtime stores nothing about it and
//! offers no entry point for it, because the allocation underneath is made and
//! released by the same two functions below with the concrete type's
//! descriptor.

use crate::abi::ScienceTypeInfo;
use crate::mem::{science_alloc, science_dealloc};

/// `Box::new(value: T) -> Box[T]`.
///
/// Allocates one `T` and **moves** the value at `value` into it: after the call
/// the caller's slot is logically uninitialised and must not be dropped. The
/// returned pointer is the `Box[T]`, and is never null.
///
/// # Safety
///
/// `info` must be a non-null, aligned pointer to a valid [`ScienceTypeInfo`];
/// `value` must be non-null, aligned for the type, and point to an initialised
/// value of it.
#[no_mangle]
pub unsafe extern "C" fn science_box_new(info: *const ScienceTypeInfo, value: *const u8) -> *mut u8 {
    // SAFETY: the caller guarantees a valid descriptor.
    let info = unsafe { &*info };
    // SAFETY: `info.align` is a power of two by the descriptor contract.
    let ptr = unsafe { science_alloc(info.size, info.align) };
    // SAFETY: the fresh allocation is `size` writable bytes, `value` is `size`
    // readable ones, and two distinct allocations cannot overlap.
    unsafe { std::ptr::copy_nonoverlapping(value, ptr, info.size) };
    ptr
}

/// Release a `Box[T]`, running the destructor over the value it holds.
///
/// **Codegen support:** this is `Box`'s drop glue (§6.1 rule 6).
///
/// # Safety
///
/// `info` must be the descriptor the box was created with; `ptr` must come from
/// [`science_box_new`] with that same descriptor and must not already have been
/// freed. It must still hold an initialised value: if the value was moved out
/// of the box, codegen must release the allocation with
/// [`science_dealloc`](crate::science_dealloc) instead, so no destructor runs.
#[no_mangle]
pub unsafe extern "C" fn science_box_free(info: *const ScienceTypeInfo, ptr: *mut u8) {
    // SAFETY: the caller guarantees a valid descriptor.
    let info = unsafe { &*info };
    if let Some(drop_fn) = info.drop_fn {
        // SAFETY: the caller guarantees the box still holds a live value.
        unsafe { drop_fn(ptr) }
    }
    // SAFETY: `ptr` came from `science_alloc` with exactly this size and
    // alignment.
    unsafe { science_dealloc(ptr, info.size, info.align) }
}
