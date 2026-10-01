//! `Array[T]`: growable and contiguous. §8's method set in full, over the
//! element-descriptor convention of the crate documentation, §6.

use crate::abi::{ScienceMapInfo, ScienceTypeInfo};
use crate::mem::{capacity_overflow, dangling, science_dealloc, science_realloc};
use crate::panic::science_panic_bytes;
use crate::string::ScienceString;

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

/// `Array::clone(&self) -> Array[T]` for an element that owns nothing.
///
/// Allocates a buffer of `src.len` slots and copies the elements' bytes. That
/// is a duplicate only where the type has no destructor, so a descriptor with a
/// `drop_fn` is refused here as a backstop: `science-codegen-llvm` refuses it
/// first, by name, and never emits this call for one.
///
/// # Safety
///
/// `src` must be a non-null, aligned pointer to a live [`ScienceArray`] and
/// `info` the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_clone(
    src: *const ScienceArray,
    info: *const ScienceTypeInfo,
) -> ScienceArray {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (src, info) = unsafe { (&*src, &*info) };
    assert!(
        info.drop_fn.is_none(),
        "Array.clone of an element type that owns memory is not supported"
    );
    let mut out = ScienceArray::empty(info);
    // SAFETY: `out` is live and described by `info`.
    unsafe { out.reserve(info, src.len) };
    // SAFETY: after `reserve`, `out` has room for `src.len` elements; `src`'s
    // first `len` slots are initialised and live in a different allocation.
    unsafe {
        std::ptr::copy_nonoverlapping(src.ptr, out.ptr, info.offset_of(src.len));
    }
    out.len = src.len;
    out
}

/// `Array::clone(&self) -> Array[String]`: every string is cloned into its own
/// buffer.
///
/// # Safety
///
/// `src` must be a non-null, aligned pointer to a live `Array[String]` and
/// `info` the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_clone_strings(
    src: *const ScienceArray,
    info: *const ScienceTypeInfo,
) -> ScienceArray {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (src, info) = unsafe { (&*src, &*info) };
    let mut out = ScienceArray::empty(info);
    // SAFETY: `out` is live and described by `info`.
    unsafe { out.reserve(info, src.len) };
    for index in 0..src.len {
        // SAFETY: both slots are within their buffers, `src`'s is a live
        // `ScienceString`, and `out`'s is uninitialised and written once.
        unsafe {
            let from = src.slot(info, index) as *const crate::string::ScienceString;
            let to = out.slot(info, index) as *mut crate::string::ScienceString;
            to.write(crate::string::science_string_clone(from));
        }
        // Counted as it is written, so a panic part-way leaves a valid array.
        out.len = index + 1;
    }
    out
}

/// `Array::clone(&self) -> Array[T]` for an element that owns something other
/// than a `String`: `clone_fn` writes a clone of the element at its first
/// argument into the uninitialised slot at its second.
///
/// `ScienceTypeInfo` carries no clone function, and growing it would change the
/// layout every descriptor in every compiled program shares; the function
/// travels beside the descriptor instead, as a third argument, and only this
/// one call takes it. Codegen emits one `clone_fn` per element type.
///
/// # Safety
///
/// `src` must be a non-null, aligned pointer to a live [`ScienceArray`], `info`
/// the descriptor it was created with, and `clone_fn` a function that, given a
/// live element and room for one, initialises that room with an independent
/// copy.
#[no_mangle]
pub unsafe extern "C" fn science_array_clone_with(
    src: *const ScienceArray,
    info: *const ScienceTypeInfo,
    clone_fn: unsafe extern "C" fn(*const u8, *mut u8),
) -> ScienceArray {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (src, info) = unsafe { (&*src, &*info) };
    let mut out = ScienceArray::empty(info);
    // SAFETY: `out` is live and described by `info`.
    unsafe { out.reserve(info, src.len) };
    for index in 0..src.len {
        // SAFETY: both slots are within their buffers, `src`'s is a live
        // element, and `out`'s is uninitialised and written once.
        unsafe { clone_fn(src.slot(info, index), out.slot(info, index)) };
        // Counted as it is written, so a panic part-way leaves a valid array.
        out.len = index + 1;
    }
    out
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

// --- The rest of Level 1's surface -------------------------------------------
//
// `stdlib-core.md` §3 gives `Array` five signatures and `docs/DREAM.md` §13.4
// the rest of the catalogue; `science-resolve`'s `builtins.rs` is where each
// name is argued. Everything below is element-agnostic in the way the entry
// points above are: an element is `size` opaque bytes, a move is a `copy` or
// `copy_nonoverlapping` of them, and the only thing that ever runs *on* an
// element is `drop_fn`, exactly when the array stops owning one it did not
// hand out.
//
// The three that need to know something about an element — equality for
// `contains` and `index_of`, order for `sort` — take it from somewhere other
// than `ScienceTypeInfo`, which carries neither: equality from a
// `ScienceMapInfo` over `(T, ())`, the descriptor `Set of T` is already
// represented by, and order from the entry point's own name.

/// Panic for an index that names no element a checked mutator could act on.
///
/// **The message names the method, the index and the length**, which the
/// bracket form's *"index out of bounds"* does not, and the difference is
/// where each is built: `xs[i]` panics from a block `science-mir` emits with
/// a constant, because formatting on the failing path would be MIR it has to
/// build, while this is Rust on a path that is about to abort anyway.
#[cold]
#[inline(never)]
fn index_out_of_bounds(method: &str, index: i64, len: usize) -> ! {
    let message = format!("Array.{method}: index {index} out of bounds for length {len}");
    // SAFETY: `message` is `len` initialised bytes for the duration of the
    // call, and the call does not return.
    unsafe { science_panic_bytes(message.as_ptr(), message.len()) }
}

/// `index` as a position strictly below `len`, or a panic naming `method`.
#[inline]
fn checked(method: &str, len: usize, index: i64) -> usize {
    match in_bounds(len, index) {
        Some(index) => index,
        None => index_out_of_bounds(method, index, len),
    }
}

/// `Array::capacity(&self) -> Int`, in elements.
///
/// A zero-sized element type's capacity is unbounded (`usize::MAX`), which no
/// `Int` holds; it is reported as `Int.MAX`, the largest answer that is still
/// true of every array a program can build.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`].
#[no_mangle]
pub unsafe extern "C" fn science_array_capacity(array: *const ScienceArray) -> i64 {
    // SAFETY: the caller guarantees a live array.
    let cap = unsafe { (*array).cap };
    i64::try_from(cap).unwrap_or(i64::MAX)
}

/// `Array::first(&self) -> (borrowed T)?`: [`science_array_get`] at `0`.
///
/// # Safety
///
/// As [`science_array_get`].
#[no_mangle]
pub unsafe extern "C" fn science_array_first(
    array: *const ScienceArray,
    info: *const ScienceTypeInfo,
) -> *const u8 {
    // SAFETY: forwarded unchanged.
    unsafe { science_array_get(array, info, 0) }
}

/// `Array::last(&self) -> (borrowed T)?`: the element at `len - 1`, or the
/// null pointer on an empty array.
///
/// # Safety
///
/// As [`science_array_get`].
#[no_mangle]
pub unsafe extern "C" fn science_array_last(
    array: *const ScienceArray,
    info: *const ScienceTypeInfo,
) -> *const u8 {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&*array, &*info) };
    if array.len == 0 {
        return std::ptr::null();
    }
    // SAFETY: `len - 1 < len`, so the slot holds an initialised element.
    unsafe { array.slot(info, array.len - 1) }
}

/// `Array::insert(&mut self, index: Int, value: T)`.
///
/// Moves `value` in at `index` and every element from `index` on one place
/// up. `index` may be `len` — an insert at the end is a `push` — and anything
/// outside `0..=len` panics, naming the index and the length.
///
/// # Safety
///
/// As [`science_array_push`].
#[no_mangle]
pub unsafe extern "C" fn science_array_insert(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    index: i64,
    value: *const u8,
) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    // `0..=len`, which is `in_bounds` against `len + 1`. Saturating, because a
    // `len` of `usize::MAX` is a zero-sized array, whose `reserve` below
    // refuses one more anyway.
    let Some(index) = in_bounds(array.len.saturating_add(1), index) else {
        index_out_of_bounds("insert", index, array.len)
    };
    // SAFETY: `array` is live and described by `info`.
    unsafe { array.reserve(info, 1) };
    // SAFETY: after `reserve`, slots `0..=len` are allocated. The tail
    // `index..len` moves up one slot — an overlapping copy, hence `copy` — and
    // slot `index` is then written from `value`, which does not alias the
    // buffer by contract.
    unsafe {
        let at = array.slot(info, index);
        std::ptr::copy(at, array.slot(info, index + 1), info.offset_of(array.len - index));
        std::ptr::copy_nonoverlapping(value, at, info.size);
    }
    array.len += 1;
}

/// `Array::remove(&mut self, index: Int) -> T?`.
///
/// The owned-`T?` convention of [`science_array_pop`]: on an index in range,
/// moves that element into `out`, closes the gap by moving every later element
/// one place down, and returns `true`; on any other index returns `false` and
/// writes nothing. Nothing is dropped either way.
///
/// # Safety
///
/// As [`science_array_pop`].
#[no_mangle]
pub unsafe extern "C" fn science_array_remove(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    index: i64,
    out: *mut u8,
) -> bool {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    let Some(index) = in_bounds(array.len, index) else {
        return false;
    };
    // SAFETY: `index < len`, so the slot holds an initialised element; `out`
    // is a distinct allocation. The tail `index + 1..len` then moves down one
    // slot over the element just moved out — overlapping, hence `copy`.
    unsafe {
        let at = array.slot(info, index);
        std::ptr::copy_nonoverlapping(at, out, info.size);
        std::ptr::copy(array.slot(info, index + 1), at, info.offset_of(array.len - index - 1));
    }
    array.len -= 1;
    true
}

/// `Array::swap(&mut self, first: Int, second: Int)`.
///
/// Exchanges two elements. Either index out of range panics, naming it.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_swap(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    first: i64,
    second: i64,
) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    let first = checked("swap", array.len, first);
    let second = checked("swap", array.len, second);
    if first != second {
        // SAFETY: both are initialised slots below `len`, distinct, and each
        // `size` bytes wide at a stride of `size`, so they do not overlap.
        unsafe {
            std::ptr::swap_nonoverlapping(
                array.slot(info, first),
                array.slot(info, second),
                info.size,
            )
        }
    }
}

/// `Array::replace(&mut self, index: Int, value: T) -> T`.
///
/// **The move-out primitive.** `xs[i]` is a borrow and `pop` reaches only the
/// last element, so without this nothing could take ownership of an element
/// in the middle of an array without shifting the rest — which is what a ring
/// buffer over `Array of T?` needs. The old element is moved into `out` and
/// `value` into its slot; nothing is dropped, and the caller owns what `out`
/// now holds. An index out of range panics, naming it, and writes neither.
///
/// # Safety
///
/// As [`science_array_push`] for `value`, and as [`science_array_pop`] for
/// `out`; `value` and `out` must not overlap each other.
#[no_mangle]
pub unsafe extern "C" fn science_array_replace(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    index: i64,
    value: *const u8,
    out: *mut u8,
) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    let index = checked("replace", array.len, index);
    // SAFETY: `index < len`, so the slot is initialised; `value` and `out` are
    // distinct from the buffer and from each other by contract.
    unsafe {
        let at = array.slot(info, index);
        std::ptr::copy_nonoverlapping(at, out, info.size);
        std::ptr::copy_nonoverlapping(value, at, info.size);
    }
}

/// `Array::clear(&mut self)`: drop every element, in index order, and keep
/// the buffer.
///
/// # Safety
///
/// As [`science_array_free`], except that the array stays usable.
#[no_mangle]
pub unsafe extern "C" fn science_array_clear(array: *mut ScienceArray, info: *const ScienceTypeInfo) {
    // SAFETY: forwarded unchanged.
    unsafe { science_array_truncate(array, info, 0) }
}

/// `Array::truncate(&mut self, length: Int)`: drop every element from
/// `length` on, in index order, and keep the buffer.
///
/// **Total, as `String.truncate` is** (`stdlib-core.md` §6.5): a `length` at
/// or past the end changes nothing, and a negative one empties the array.
///
/// # Safety
///
/// As [`science_array_free`], except that the array stays usable.
#[no_mangle]
pub unsafe extern "C" fn science_array_truncate(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    length: i64,
) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    let keep = usize::try_from(length.max(0)).unwrap_or(usize::MAX);
    if keep >= array.len {
        return;
    }
    let old = array.len;
    // **`len` first, then the drops**, so that the array never claims an
    // element whose destructor has already run.
    array.len = keep;
    if let Some(drop_fn) = info.drop_fn {
        for index in keep..old {
            // SAFETY: slots `keep..old` were initialised and are no longer the
            // array's; each is dropped exactly once.
            unsafe { drop_fn(array.slot(info, index)) }
        }
    }
}

/// `Array::extend(&mut self, other: Array[T])`.
///
/// Moves every element of `other` onto the end of `array`, in order, and then
/// releases `other`'s buffer **without** running `drop_fn` — its elements are
/// `array`'s now. `other` is consumed: it is left a valid empty array, and
/// the caller does not free it again, because Science moved it into the call.
///
/// # Safety
///
/// `array` and `other` must be non-null, aligned pointers to two distinct live
/// [`ScienceArray`]s, both created with `info`.
#[no_mangle]
pub unsafe extern "C" fn science_array_extend(
    array: *mut ScienceArray,
    info: *const ScienceTypeInfo,
    other: *mut ScienceArray,
) {
    // SAFETY: the caller guarantees two distinct live arrays and the descriptor.
    let (array, info, other) = unsafe { (&mut *array, &*info, &mut *other) };
    // SAFETY: `array` is live and described by `info`.
    unsafe { array.reserve(info, other.len) };
    // SAFETY: after `reserve`, slots `len..len + other.len` are allocated and
    // uninitialised; `other`'s first `len` slots are initialised and live in a
    // different allocation.
    unsafe {
        std::ptr::copy_nonoverlapping(
            other.ptr,
            array.slot(info, array.len),
            info.offset_of(other.len),
        );
    }
    array.len += other.len;
    // SAFETY: `ptr` and `cap * size` describe the block `other` allocated; its
    // elements were moved out above, so nothing is dropped.
    unsafe { science_dealloc(other.ptr, info.offset_of(other.cap), info.align) };
    *other = ScienceArray::empty(info);
}

/// `Array::reverse(&mut self)`: reverse the elements in place.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_array_reverse(array: *mut ScienceArray, info: *const ScienceTypeInfo) {
    // SAFETY: the caller guarantees a live array and its descriptor.
    let (array, info) = unsafe { (&mut *array, &*info) };
    let len = array.len;
    for low in 0..len / 2 {
        // SAFETY: `low < len - 1 - low`, both below `len`: two distinct
        // initialised slots that cannot overlap.
        unsafe {
            std::ptr::swap_nonoverlapping(
                array.slot(info, low),
                array.slot(info, len - 1 - low),
                info.size,
            )
        }
    }
}

/// The position of the first element equal to `value`, by `info.eq_fn`.
///
/// # Safety
///
/// As [`science_array_contains`].
unsafe fn position(array: &ScienceArray, info: &ScienceMapInfo, value: *const u8) -> Option<usize> {
    (0..array.len).find(|&index| {
        // SAFETY: `index < len`, so the slot holds an initialised element of
        // the key type `info.key` describes, which is `eq_fn`'s operand type.
        unsafe { (info.eq_fn)(array.slot(&info.key, index), value) }
    })
}

/// `Array::contains(&self, value: &T) -> Bool`.
///
/// **The descriptor is a `ScienceMapInfo` over `(T, ())`, not a
/// `ScienceTypeInfo`.** Equality is not in an element descriptor and has no
/// business being there for every array that never compares; it is in a map
/// descriptor, whose `key` is `T`'s own descriptor and whose `eq_fn` is the
/// one `Set of T` — represented as exactly this `(T, ())` map — already uses
/// for its own `contains`. So `xs.contains(x)` and `set.contains(x)` agree on
/// what equal means by construction.
///
/// # Safety
///
/// `array` must be a live [`ScienceArray`] whose elements `info.key`
/// describes; `value` must point to a live `T`.
#[no_mangle]
pub unsafe extern "C" fn science_array_contains(
    array: *const ScienceArray,
    info: *const ScienceMapInfo,
    value: *const u8,
) -> bool {
    // SAFETY: the caller guarantees a live array and its descriptor.
    unsafe { position(&*array, &*info, value).is_some() }
}

/// `Array::index_of(&self, value: &T) -> Int?`.
///
/// The owned-`T?` convention of [`science_array_pop`] over an `Int` payload:
/// the index of the first element equal to `value`, written to `out`, and
/// `true`; or `false` with `out` untouched. Equality as
/// [`science_array_contains`].
///
/// # Safety
///
/// As [`science_array_contains`]; `out` must be writable and aligned for an
/// `i64`.
#[no_mangle]
pub unsafe extern "C" fn science_array_index_of(
    array: *const ScienceArray,
    info: *const ScienceMapInfo,
    value: *const u8,
    out: *mut i64,
) -> bool {
    // SAFETY: the caller guarantees a live array and its descriptor.
    match unsafe { position(&*array, &*info, value) } {
        Some(index) => {
            // SAFETY: `out` is writable by contract. An index below `len` is
            // below `isize::MAX`, so it fits an `i64` on every F0 target.
            unsafe { out.write(index as i64) };
            true
        }
        None => false,
    }
}

/// `Array[Int]::sort(&mut self)`: ascending, stable.
///
/// **One entry point per element type, named for it**, because `sort()`'s
/// bound is `T: Ord` and `Ord` declares no method yet: there is no compare
/// function for a descriptor to carry. Codegen picks this symbol off the
/// element type at the call, and refuses by name an element type that has
/// none.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`] of
/// `i64`.
#[no_mangle]
pub unsafe extern "C" fn science_array_sort_i64(array: *mut ScienceArray) {
    // SAFETY: the caller guarantees a live array of `len` initialised `i64`s.
    let elements =
        unsafe { std::slice::from_raw_parts_mut((*array).ptr.cast::<i64>(), (*array).len) };
    elements.sort();
}

/// `Array[String]::sort(&mut self)`: ascending in byte order — `String: Ord`
/// as `stdlib-core.md` §6.8 defines it, and `science_string_cmp`'s relation —
/// and stable, so equal strings keep their order.
///
/// # Safety
///
/// `array` must be a non-null, aligned pointer to a live [`ScienceArray`] of
/// [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_array_sort_string(array: *mut ScienceArray) {
    // SAFETY: the caller guarantees a live array of `len` initialised strings.
    // `ScienceString` has no `Drop`, so the sort's bitwise moves neither run a
    // destructor nor duplicate an owner.
    let elements = unsafe {
        std::slice::from_raw_parts_mut((*array).ptr.cast::<ScienceString>(), (*array).len)
    };
    // SAFETY: each element is a live string.
    elements.sort_by(|a, b| unsafe { a.bytes().cmp(b.bytes()) });
}
