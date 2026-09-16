//! Allocation, over the system allocator.
//!
//! Generated code knows every size and alignment statically, so it passes both
//! in rather than making the runtime store a header alongside each block. The
//! system allocator is used unadorned: no arena, no free list, no pooling. If a
//! Science program ever wants those, they belong in the language, not hidden here
//! where the ownership rules cannot see them.
//!
//! # Zero-sized allocations
//!
//! `size == 0` is legal everywhere in this module and never reaches the system
//! allocator. [`science_alloc`] answers with a *dangling* pointer — the integer
//! value of the requested alignment — which is non-null and correctly aligned,
//! so zero-length reads and writes through it are well defined, and
//! [`science_dealloc`] with `size == 0` does nothing. That makes the contract
//! total: every container in this crate can start life empty, hand out a
//! pointer, and be freed, without ever branching on emptiness.

use core::alloc::Layout;

use crate::panic::science_panic_bytes;

/// A dangling but correctly aligned, non-null pointer.
#[inline]
pub(crate) fn dangling(align: usize) -> *mut u8 {
    if !align.is_power_of_two() {
        bad_align();
    }
    core::ptr::without_provenance_mut(align)
}

#[cold]
#[inline(never)]
fn bad_align() -> ! {
    let message = b"science-rt: allocation alignment is not a power of two";
    // SAFETY: a static byte string is a valid, live buffer of that length.
    unsafe { science_panic_bytes(message.as_ptr(), message.len()) }
}

#[cold]
#[inline(never)]
fn bad_size() -> ! {
    let message = b"science-rt: allocation size exceeds the address space";
    // SAFETY: as above.
    unsafe { science_panic_bytes(message.as_ptr(), message.len()) }
}

#[cold]
#[inline(never)]
pub(crate) fn allocation_failed() -> ! {
    let message = b"science-rt: out of memory";
    // SAFETY: as above.
    unsafe { science_panic_bytes(message.as_ptr(), message.len()) }
}

/// Capacity arithmetic overflowed. Reachable only from a container asked to
/// hold more elements than the address space can describe.
#[cold]
#[inline(never)]
pub(crate) fn capacity_overflow() -> ! {
    let message = b"science-rt: capacity overflow";
    // SAFETY: as above.
    unsafe { science_panic_bytes(message.as_ptr(), message.len()) }
}

#[inline]
fn layout_of(size: usize, align: usize) -> Layout {
    match Layout::from_size_align(size, align) {
        Ok(layout) => layout,
        Err(_) => {
            if align.is_power_of_two() {
                bad_size()
            } else {
                bad_align()
            }
        }
    }
}

/// Allocate `size` bytes aligned to `align`.
///
/// Returns a pointer that is never null and always aligned. On exhaustion the
/// process aborts with a message rather than returning null: F0 gives a Science
/// program no way to observe or recover from allocation failure (§8 lists no
/// fallible allocation), so a null return would only be dereferenced.
///
/// `size == 0` returns a dangling aligned pointer and does not call the system
/// allocator; see the module documentation.
///
/// # Safety
///
/// `align` must be a power of two, and `size` rounded up to `align` must not
/// exceed `isize::MAX`. Violating either aborts rather than misbehaving, but
/// the caller is expected not to.
#[no_mangle]
pub unsafe extern "C" fn science_alloc(size: usize, align: usize) -> *mut u8 {
    if size == 0 {
        return dangling(align);
    }
    let layout = layout_of(size, align);
    // SAFETY: `layout` has a non-zero size, checked just above.
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        allocation_failed();
    }
    ptr
}

/// Resize a block from `old_size` to `new_size`, keeping `align`.
///
/// The contents common to both sizes are preserved; growth leaves the new tail
/// uninitialised. The returned pointer may differ from `ptr`, and when it does,
/// `ptr` has been freed and must not be used again.
///
/// Both zero cases are handled: `old_size == 0` allocates afresh, and
/// `new_size == 0` frees and answers with a dangling pointer.
///
/// # Safety
///
/// `ptr` must come from [`science_alloc`] or [`science_realloc`] with exactly
/// `old_size` and `align`, and must not have been freed. `align` must be a
/// power of two.
#[no_mangle]
pub unsafe extern "C" fn science_realloc(
    ptr: *mut u8,
    old_size: usize,
    new_size: usize,
    align: usize,
) -> *mut u8 {
    if new_size == 0 {
        // SAFETY: the caller's obligations for `ptr`/`old_size`/`align` are
        // exactly `science_dealloc`'s.
        unsafe { science_dealloc(ptr, old_size, align) };
        return dangling(align);
    }
    if old_size == 0 {
        // Nothing was allocated, so there is nothing to copy or release.
        // SAFETY: `align` is the caller's, and `new_size` is non-zero.
        return unsafe { science_alloc(new_size, align) };
    }
    let old_layout = layout_of(old_size, align);
    // Validate the destination size before handing it to the allocator, which
    // requires it to satisfy the same constraints.
    let _ = layout_of(new_size, align);
    // SAFETY: `ptr` came from this allocator with `old_layout`, and `new_size`
    // is non-zero and describes a valid layout at this alignment.
    let new_ptr = unsafe { std::alloc::realloc(ptr, old_layout, new_size) };
    if new_ptr.is_null() {
        allocation_failed();
    }
    new_ptr
}

/// Release a block of `size` bytes aligned to `align`.
///
/// `size == 0` is a no-op, matching [`science_alloc`].
///
/// # Safety
///
/// `ptr` must come from [`science_alloc`] or [`science_realloc`] with exactly `size`
/// and `align`, and must not already have been freed.
#[no_mangle]
pub unsafe extern "C" fn science_dealloc(ptr: *mut u8, size: usize, align: usize) {
    if size == 0 {
        return;
    }
    let layout = layout_of(size, align);
    // SAFETY: the caller guarantees `ptr` came from this allocator with this
    // exact layout and is still live.
    unsafe { std::alloc::dealloc(ptr, layout) }
}
