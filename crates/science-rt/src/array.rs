//! `Array[T]`: growable and contiguous. §8's method set in full, over the
//! element-descriptor convention of the crate documentation, §6.

use crate::abi::ScienceTypeInfo;
use crate::mem::{capacity_overflow, dangling, science_dealloc, science_realloc};

/// Science's `Array[T]`.
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
/// [`ScienceTypeInfo`] describing it, and **it must be the same descriptor every
/// time**; see the crate documentation, §6, for why the descriptor is kept
/// outside the value and what happens if that rule is broken.
///
/// Like every type here, `Array` implements no `Drop`: it is moved by copying
/// three words and destroyed by [`science_array_free`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScienceArray {
    /// Pointer to the elements. Never null; dangling when nothing is allocated.
    pub ptr: *mut u8,
    /// Number of initialised elements.
    pub len: usize,
    /// Number of elements the allocation can hold.
    pub cap: usize,
}

impl ScienceArray {
    #[inline]
    fn empty(info: &ScienceTypeInfo) -> Self {
        ScienceArray {
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
    unsafe fn reserve(&mut self, info: &ScienceTypeInfo, additional: usize) {
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
        self.ptr = unsafe { science_realloc(self.ptr, old_bytes, new_bytes, info.align) };
        self.cap = new_cap;
    }

    /// Address of element `index`, which need not be initialised.
    ///
    /// # Safety
    ///
    /// `index` must be at most `cap`, and `info` must describe this array.
    #[inline]
    unsafe fn slot(&self, info: &ScienceTypeInfo, index: usize) -> *mut u8 {
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
/// `info` must be a non-null, aligned pointer to a valid [`ScienceTypeInfo`].
#[no_mangle]
pub unsafe extern "C" fn science_array_new(info: *const ScienceTypeInfo) -> ScienceArray {
    // SAFETY: the caller guarantees a valid descriptor.
    ScienceArray::empty(unsafe { &*info })
}

/// Create an `Array` with room for `capacity` elements already allocated.
///
/// **Codegen support.** Not in §8, but an array literal `[a, b, c]` knows its
/// length up front and should not reallocate its way there.
///
/// # Safety
///
/// `info` must be a non-null, aligned pointer to a valid [`ScienceTypeInfo`].
#[no_mangle]
pub unsafe extern "C" fn science_array_with_capacity(
    info: *const ScienceTypeInfo,
    capacity: usize,
) -> ScienceArray {
    // SAFETY: the caller guarantees a valid descriptor.
    let info = unsafe { &*info };
    let mut array = ScienceArray::empty(info);
    // SAFETY: `array` is live and described by `info`.
    unsafe { array.reserve(info, capacity) };
    array
}

/// Ensure room for `additional` more elements without reallocating.
///
/// **Codegen support**, as [`science_array_with_capacity`].
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_reserve(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
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
/// released; elements that were moved out by [`science_array_pop`] are not, since
/// they are no longer the array's.
///
/// The receiver is left as a valid empty array.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`] that has
/// not already been freed, and `info` must be the descriptor it was created
/// with.
#[no_mangle]
pub unsafe extern "C" fn science_array_free(array: *mut ScienceArray, info: *const ScienceTypeInfo) {
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
    unsafe { science_dealloc(array.ptr, info.offset_of(array.cap), info.align) };
    *array = ScienceArray::empty(info);
}

/// `Array::len(&self) -> Int`, in elements.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`].
#[no_mangle]
pub unsafe extern "C" fn science_array_len(array: *const ScienceArray) -> i64 {
    // SAFETY: the caller guarantees a live array.
    unsafe { (*array).len as i64 }
}

/// `Array::is_empty(&self) -> Bool`.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`].
#[no_mangle]
pub unsafe extern "C" fn science_array_is_empty(array: *const ScienceArray) -> bool {
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
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`].
#[no_mangle]
pub unsafe extern "C" fn science_array_as_ptr(array: *const ScienceArray) -> *const u8 {
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
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`]; `info`
/// must be the descriptor it was created with; `value` must be non-null,
/// aligned for the element type, point to an initialised element, and not
/// alias the array's buffer.
#[no_mangle]
pub unsafe extern "C" fn science_array_push(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
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

/// `Array::pop(&mut self) -> T?`.
///
/// The owned-`T?` convention of the crate documentation, §5.3: returns
/// `true` after **moving** the last element into `out`, which the caller now
/// owns and must eventually destroy, or `false` on an empty array, in which
/// case `out` is **not written** at all and the answer is `null`.
///
/// Capacity is not released: an array that has been emptied keeps its buffer.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`]; `info`
/// must be the descriptor it was created with; `out` must be non-null, aligned
/// for the element type, and writable for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn science_array_pop(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
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

/// `Array::get(&self, index: Int) -> (borrowed T)?`.
///
/// The niche convention of the crate documentation, §5.3: the return value
/// **is** the `(borrowed T)?`. A null pointer is `null`; anything else is the
/// borrow itself and needs no conversion.
///
/// `null` is the answer for `index >= len` and for any negative index, so §8's
/// promise that indexing cannot panic holds for every `Int` a program can
/// produce.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_get(
    array: *const ScienceArray,
    info: *const ScienceTypeInfo,
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

/// `Array::get_mut(&mut self, index: Int) -> (mutable borrowed T)?`.
///
/// As [`science_array_get`], with an exclusive borrow. The return value **is** the
/// `(mutable borrowed T)?`: the null pointer is `null`.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_get_mut(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
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

/// Convert a Science `Int` index to a `usize`, or `None` if it is out of range.
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

/// The barrier behind `chain.sorted(by: key)`.
///
/// Reorders `array` so that its elements are in non-decreasing order of the
/// `Int` key at the matching index of `keys`, and reorders `keys` with them.
/// Both arrays must have the same length; a mismatch is a compiler bug and the
/// call does nothing rather than read past either buffer.
///
/// # Why the keys are computed by the caller and passed in
///
/// `collections-and-chains.md` §1.4 classes `sorted(by:)` as a **barrier**:
/// it buffers the whole stream, sorts, and yields. The key is a *closure*, and
/// this side of the boundary has no way to call one — a Science closure is a
/// function pointer and an environment that `science-codegen-llvm` knows the
/// layout of and this crate does not. So the fused loop
/// (`science-mir`'s `Builder::lower_chain`) evaluates the key once per element
/// on its way into the buffer, exactly as a Schwartzian transform does, and
/// hands over two parallel arrays. That also gives the note's own guarantee
/// for free: the key is computed once per element, not once per comparison.
///
/// # The sort is stable, and that is a published-results decision
///
/// §5.2 of the same note makes `Map` and `Set` iterate in insertion order
/// *"because this is a language whose users publish"*, and an unstable sort
/// breaks the same promise one link along: two elements with equal keys would
/// come out in an order that depends on the algorithm's internal state. A
/// stable sort makes `sorted(by:)` a function of the input alone.
///
/// # Safety
///
/// `array` and `keys` must be non-null, aligned pointers to live
/// [`ScienceArray`]s; `info` must be the descriptor `array` was created with;
/// `keys` must hold `i64` elements. Elements are moved, never dropped, and no
/// destructor runs.
#[no_mangle]
pub unsafe extern "C" fn science_array_sort_by_int_key(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    keys: *mut ScienceArray,
) {
    // SAFETY: the caller guarantees two live arrays and the descriptor.
    let (array, info, keys) = unsafe { (&mut *array, &*info, &mut *keys) };
    if array.len != keys.len || array.len < 2 {
        return;
    }
    // SAFETY: `keys` holds `array.len` initialised `i64`s, by the contract.
    let key_at = |index: usize| -> i64 { unsafe { *(keys.ptr as *const i64).add(index) } };

    // **An index permutation, not a swap sort.** The element type is opaque
    // bytes of `info.size` and may be anything from an `i64` to a record that
    // owns a `String`, so every move has to be a `copy_nonoverlapping` of the
    // whole element. Sorting the *indices* and then materialising the
    // permutation once costs `len` element-moves total instead of one per
    // comparison, and it is the only version whose cost does not depend on
    // how wide `T` is.
    let mut order: Vec<usize> = (0..array.len).collect();
    // `sort_by_key` on the key alone would be stable only because the
    // standard library's sort is; the index tiebreak states it rather than
    // relying on it, and costs nothing.
    order.sort_by(|left, right| {
        key_at(*left).cmp(&key_at(*right)).then_with(|| left.cmp(right))
    });

    let mut sorted_elements: Vec<u8> = vec![0; info.offset_of(array.len)];
    let mut sorted_keys: Vec<i64> = Vec::with_capacity(array.len);
    for (destination, source) in order.iter().copied().enumerate() {
        // SAFETY: `source < len`, so the slot holds an initialised element;
        // the scratch buffer is `len` elements wide and is a distinct
        // allocation, so the ranges cannot overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(
                array.slot(info, source),
                sorted_elements.as_mut_ptr().add(info.offset_of(destination)),
                info.size,
            );
        }
        sorted_keys.push(key_at(source));
    }
    // SAFETY: the scratch buffer holds `len` initialised elements laid out at
    // the same stride as the array's own, and the two allocations are
    // distinct.
    unsafe {
        std::ptr::copy_nonoverlapping(
            sorted_elements.as_ptr(),
            array.ptr,
            info.offset_of(array.len),
        );
        std::ptr::copy_nonoverlapping(sorted_keys.as_ptr(), keys.ptr as *mut i64, array.len);
    }
}
