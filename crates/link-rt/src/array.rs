//! `Array[T]`: growable and contiguous. §8's method set in full, over the
//! element-descriptor convention of the crate documentation, §6.

use crate::abi::LinkTypeInfo;
use crate::mem::{capacity_overflow, dangling, link_dealloc, link_realloc};

/// Link's `Array[T]`.
///
/// Layout: three words, `{ ptr, len, cap }`.
///
/// - `ptr` is **never null**; it is dangling but aligned to the element type
///   when nothing has been allocated.
/// - `len` is the number of **elements**, not bytes, and is what §8's `len`
///   returns.
/// - `cap` is the number of elements that fit at `ptr`. For a zero-sized
///   element type it is `usize::MAX` and no allocation ever happens.
/// - The first `len` elements are initialised; everything from `len` to `cap`
///   is not and must not be read.
///
/// The element type appears nowhere in this struct. Every operation takes a
/// [`LinkTypeInfo`] describing it, and **it must be the same descriptor every
/// time**; see the crate documentation, §6, for why the descriptor is kept
/// outside the value and what happens if that rule is broken.
///
/// Like every type here, `Array` implements no `Drop`: it is moved by copying
/// three words and destroyed by [`link_array_free`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LinkArray {
    /// Pointer to the elements. Never null; dangling when nothing is allocated.
    pub ptr: *mut u8,
    /// Number of initialised elements.
    pub len: usize,
    /// Number of elements the allocation can hold.
    pub cap: usize,
}

impl LinkArray {
    #[inline]
    fn empty(info: &LinkTypeInfo) -> Self {
        LinkArray {
            ptr: dangling(info.align),
            // A zero-sized element type needs no storage, so capacity is
            // unbounded by definition and `push` never has to grow.
            cap: if info.size == 0 { usize::MAX } else { 0 },
            len: 0,
        }
    }

    /// Make room for `additional` more elements.
    ///
    /// # Safety
    ///
    /// `self` must be a live array described by `info`.
    unsafe fn reserve(&mut self, info: &LinkTypeInfo, additional: usize) {
        let Some(needed) = self.len.checked_add(additional) else {
            capacity_overflow()
        };
        if needed <= self.cap {
            return;
        }
        // Unreachable for a zero-sized element type, whose capacity is already
        // `usize::MAX`, so the arithmetic below never has to handle it.
        let new_cap = needed.max(self.cap.saturating_mul(2)).max(4);
        let Some(new_bytes) = new_cap.checked_mul(info.size) else {
            capacity_overflow()
        };
        let old_bytes = info.offset_of(self.cap);
        // SAFETY: `ptr`/`old_bytes`/`align` describe this array's current
        // block, and `info.align` is a power of two by the descriptor contract.
        self.ptr = unsafe { link_realloc(self.ptr, old_bytes, new_bytes, info.align) };
        self.cap = new_cap;
    }

    /// Address of element `index`, which need not be initialised.
    ///
    /// # Safety
    ///
    /// `index` must be at most `cap`, and `info` must describe this array.
    #[inline]
    unsafe fn slot(&self, info: &LinkTypeInfo, index: usize) -> *mut u8 {
        // SAFETY: `index <= cap`, so the offset stays within the allocation (or
        // one past its end, which is a valid address to compute).
        unsafe { self.ptr.add(info.offset_of(index)) }
    }
}

/// `Array::new()`.
///
/// Allocates nothing. The returned pointer is dangling but aligned and
/// non-null.
///
/// # Safety
///
/// `info` must be a non-null, aligned pointer to a valid [`LinkTypeInfo`].
#[no_mangle]
pub unsafe extern "C" fn link_array_new(info: *const LinkTypeInfo) -> LinkArray {
    // SAFETY: the caller guarantees a valid descriptor.
    LinkArray::empty(unsafe { &*info })
}

/// Create an `Array` with room for `capacity` elements already allocated.
///
/// **Codegen support.** Not in §8, but an array literal `[a, b, c]` knows its
/// length up front and should not reallocate its way there.
///
/// # Safety
///
/// `info` must be a non-null, aligned pointer to a valid [`LinkTypeInfo`].
#[no_mangle]
pub unsafe extern "C" fn link_array_with_capacity(
    info: *const LinkTypeInfo,
    capacity: usize,
) -> LinkArray {
    // SAFETY: the caller guarantees a valid descriptor.
    let info = unsafe { &*info };
    let mut array = LinkArray::empty(info);
    // SAFETY: `array` is live and described by `info`.
    unsafe { array.reserve(info, capacity) };
    array
}

/// Ensure room for `additional` more elements without reallocating.
///
/// **Codegen support**, as [`link_array_with_capacity`].
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn link_array_reserve(
    array: *mut LinkArray,
    info: *const LinkTypeInfo,
    additional: usize,
) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    unsafe { (*array).reserve(&*info, additional) }
}

/// Release an `Array`, running the element destructor over everything it still
/// holds.
///
/// **Codegen support:** this is `Array`'s drop glue (§6.1 rule 6). Elements
/// still owned by the array are destroyed, in index order, before the buffer is
/// released; elements that were moved out by [`link_array_pop`] are not, since
/// they are no longer the array's.
///
/// The receiver is left as a valid empty array.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`] that has
/// not already been freed, and `info` must be the descriptor it was created
/// with.
#[no_mangle]
pub unsafe extern "C" fn link_array_free(array: *mut LinkArray, info: *const LinkTypeInfo) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    if let Some(drop_fn) = info.drop_fn {
        for index in 0..array.len {
            // SAFETY: elements below `len` are initialised, and the descriptor
            // is the one they were stored under.
            unsafe { drop_fn(array.slot(info, index)) }
        }
    }
    // SAFETY: `ptr` and `cap * size` describe the block this array allocated.
    unsafe { link_dealloc(array.ptr, info.offset_of(array.cap), info.align) };
    *array = LinkArray::empty(info);
}

/// `Array::len(&self) -> Int`, in elements.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`].
#[no_mangle]
pub unsafe extern "C" fn link_array_len(array: *const LinkArray) -> i64 {
    // SAFETY: the caller guarantees a live array.
    unsafe { (*array).len as i64 }
}

/// `Array::is_empty(&self) -> Bool`.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`].
#[no_mangle]
pub unsafe extern "C" fn link_array_is_empty(array: *const LinkArray) -> bool {
    // SAFETY: the caller guarantees a live array.
    unsafe { (*array).len == 0 }
}

/// Raw pointer to the first element.
///
/// **Codegen support**, for lowering `for x in array` to a pointer walk rather
/// than a call per element. Valid for `len` elements, and invalidated by any
/// mutation of the array.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`].
#[no_mangle]
pub unsafe extern "C" fn link_array_as_ptr(array: *const LinkArray) -> *const u8 {
    // SAFETY: the caller guarantees a live array.
    unsafe { (*array).ptr }
}

/// `Array::push(&mut self, value: T)`.
///
/// `value` points to a slot the caller has initialised with a `T`. The `size`
/// bytes there are **moved** into the array: after the call the caller's slot
/// is logically uninitialised and must not be dropped. No destructor runs.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`]; `info`
/// must be the descriptor it was created with; `value` must be non-null,
/// aligned for the element type, point to an initialised element, and not
/// alias the array's buffer.
#[no_mangle]
pub unsafe extern "C" fn link_array_push(
    array: *mut LinkArray,
    info: *const LinkTypeInfo,
    value: *const u8,
) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    // SAFETY: `array` is live and described by `info`.
    unsafe { array.reserve(info, 1) };
    // SAFETY: after `reserve`, slot `len` is allocated and uninitialised;
    // `value` holds one initialised element and does not alias the buffer.
    unsafe { std::ptr::copy_nonoverlapping(value, array.slot(info, array.len), info.size) };
    array.len += 1;
}

/// `Array::pop(&mut self) -> Option[T]`.
///
/// The owned-`Option` convention of the crate documentation, §5.3: returns
/// `true` after **moving** the last element into `out`, which the caller now
/// owns and must eventually destroy, or `false` on an empty array, in which
/// case `out` is **not written** at all.
///
/// Capacity is not released: an array that has been emptied keeps its buffer.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`]; `info`
/// must be the descriptor it was created with; `out` must be non-null, aligned
/// for the element type, and writable for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn link_array_pop(
    array: *mut LinkArray,
    info: *const LinkTypeInfo,
    out: *mut u8,
) -> bool {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    if array.len == 0 {
        return false;
    }
    array.len -= 1;
    // SAFETY: slot `len` (after the decrement) holds an initialised element,
    // and `out` is writable for `size` bytes in a distinct allocation.
    unsafe { std::ptr::copy_nonoverlapping(array.slot(info, array.len), out, info.size) };
    true
}

/// `Array::get(&self, index: Int) -> Option[&T]`.
///
/// The niche convention of the crate documentation, §5.3: the return value
/// **is** the `Option[&T]`. A null pointer is `None`; anything else is
/// `Some(&T)` and needs no conversion.
///
/// `None` is the answer for `index >= len` and for any negative index, so §8's
/// promise that indexing cannot panic holds for every `Int` a program can
/// produce.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn link_array_get(
    array: *const LinkArray,
    info: *const LinkTypeInfo,
    index: i64,
) -> *const u8 {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&*array, &*info) };
    let Some(index) = in_bounds(array.len, index) else {
        return std::ptr::null();
    };
    // SAFETY: `index < len`, so the slot holds an initialised element.
    unsafe { array.slot(info, index) }
}

/// `Array::get_mut(&mut self, index: Int) -> Option[&mut T]`.
///
/// As [`link_array_get`], with an exclusive borrow. The return value **is** the
/// `Option[&mut T]`: null for `None`.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`LinkArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn link_array_get_mut(
    array: *mut LinkArray,
    info: *const LinkTypeInfo,
    index: i64,
) -> *mut u8 {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    let Some(index) = in_bounds(array.len, index) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `index < len`, so the slot holds an initialised element.
    unsafe { array.slot(info, index) }
}

/// Convert a Link `Int` index to a `usize`, or `None` if it is out of range.
#[inline]
fn in_bounds(len: usize, index: i64) -> Option<usize> {
    if index < 0 {
        return None;
    }
    let index = index as u64;
    // On a 32-bit target an `Int` can name an index no `usize` can hold; that
    // index is out of bounds for the same reason every other one is.
    if index > usize::MAX as u64 {
        return None;
    }
    let index = index as usize;
    if index < len {
        Some(index)
    } else {
        None
    }
}
