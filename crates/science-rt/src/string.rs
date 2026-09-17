//! `String`: owned, UTF-8, growable. §8's method set in full.

use crate::mem::{capacity_overflow, dangling, science_dealloc, science_realloc};
use crate::panic::science_panic_bytes;

/// Science's `String`.
///
/// Layout: three words, `{ ptr, len, cap }`, exactly like an `Array[U8]` whose
/// contents are guaranteed to be valid UTF-8.
///
/// - `ptr` is **never null**. An empty `String` holds a dangling but aligned
///   pointer and has allocated nothing. Codegen may rely on this; it is what
///   keeps the null niche of §5.2 unambiguous.
/// - `len` is the number of **bytes** in use, which is what §8's `len` returns.
/// - `cap` is the number of bytes allocated at `ptr`, and is always at least
///   `len`.
/// - The `len` bytes at `ptr` are always valid UTF-8. Everything between `len`
///   and `cap` is uninitialised and must not be read.
///
/// The type implements neither `Drop` nor any reference count. A `String` is
/// moved by copying these three words and nothing else; it is destroyed by
/// [`science_string_free`], which codegen calls from drop glue when the ownership
/// rules say the value dies.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScienceString {
    /// Pointer to the bytes. Never null; dangling when `cap == 0`.
    pub ptr: *mut u8,
    /// Bytes in use.
    pub len: usize,
    /// Bytes allocated.
    pub cap: usize,
}

/// `String::chars()`, an iterator over Unicode code points.
///
/// This is the `Chars` of §8, which `implements Iterate[Char]`. It **borrows**
/// the string it walks: it owns nothing and is destroyed by forgetting it. The
/// region engine keeps the borrow alive for as long as the iterator is,
/// exactly as it does for any other `&String`.
///
/// Layout: three words, `{ ptr, len, offset }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScienceChars {
    /// Start of the borrowed bytes.
    pub ptr: *const u8,
    /// Total length of the borrowed bytes.
    pub len: usize,
    /// Byte offset of the next code point. Always on a character boundary.
    pub offset: usize,
}

impl ScienceString {
    #[inline]
    pub(crate) fn empty() -> Self {
        ScienceString {
            ptr: dangling(1),
            len: 0,
            cap: 0,
        }
    }

    /// # Safety
    ///
    /// `self` must be a live `ScienceString`.
    #[inline]
    pub(crate) unsafe fn bytes(&self) -> &[u8] {
        // SAFETY: a live `ScienceString` owns `len` initialised bytes at a
        // non-null, aligned `ptr`.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// # Safety
    ///
    /// `self` must be a live `ScienceString`, whose bytes are UTF-8 by invariant.
    #[inline]
    pub(crate) unsafe fn as_str(&self) -> &str {
        // SAFETY: the UTF-8 invariant is upheld by every constructor here.
        unsafe { std::str::from_utf8_unchecked(self.bytes()) }
    }

    /// Copy `len` bytes into a fresh `String`.
    ///
    /// # Safety
    ///
    /// `src` must point to `len` readable bytes that are valid UTF-8.
    pub(crate) unsafe fn from_raw_utf8(src: *const u8, len: usize) -> ScienceString {
        if len == 0 {
            return ScienceString::empty();
        }
        // SAFETY: `len` is non-zero, so this allocates `len` bytes at align 1.
        let ptr = unsafe { crate::mem::science_alloc(len, 1) };
        // SAFETY: `src` has `len` readable bytes and the fresh allocation has
        // `len` writable ones; two distinct allocations cannot overlap.
        unsafe { std::ptr::copy_nonoverlapping(src, ptr, len) };
        ScienceString { ptr, len, cap: len }
    }

    /// Append `bytes` to this string.
    ///
    /// The caller owns the UTF-8 invariant: `bytes` must be a whole number of
    /// well-formed sequences, which is why every caller in this crate has a
    /// `&str` in hand. UTF-8 is self-synchronising, so appending one valid
    /// sequence to another yields a valid one.
    ///
    /// # Safety
    ///
    /// `self` must be a live `ScienceString`, and `bytes` must be valid UTF-8
    /// that does not alias this string's buffer.
    pub(crate) unsafe fn append(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        // SAFETY: `self` is live, as the caller guarantees.
        unsafe { self.reserve(bytes.len()) };
        // SAFETY: the reservation just made `bytes.len()` writable bytes at
        // `len`, and the source does not alias the destination.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr.add(self.len), bytes.len()) };
        self.len += bytes.len();
    }

    /// Make room for `additional` more bytes.
    ///
    /// # Safety
    ///
    /// `self` must be a live `ScienceString`.
    unsafe fn reserve(&mut self, additional: usize) {
        let Some(needed) = self.len.checked_add(additional) else {
            capacity_overflow()
        };
        if needed <= self.cap {
            return;
        }
        // Amortised doubling, with a floor so that a string built one small
        // piece at a time does not reallocate on every piece.
        let new_cap = needed.max(self.cap.saturating_mul(2)).max(8);
        // SAFETY: `ptr`/`cap` describe this string's current block, and
        // alignment 1 is a power of two.
        self.ptr = unsafe { science_realloc(self.ptr, self.cap, new_cap, 1) };
        self.cap = new_cap;
    }
}

/// `String::new()`.
///
/// Allocates nothing. The returned pointer is dangling but non-null.
#[no_mangle]
pub extern "C" fn science_string_new() -> ScienceString {
    ScienceString::empty()
}

/// Build a `String` from raw UTF-8 bytes, copying them.
///
/// **Codegen support.** This is how a string literal (§4.1) becomes a value:
/// codegen emits the bytes into the binary's read-only data and calls this
/// with a pointer and a length.
///
/// The bytes are validated. Invalid UTF-8 is a panic, not a silently
/// mis-encoded `String`, because every other function here treats the UTF-8
/// invariant as given — [`science_string_truncate`] in particular would then be
/// able to cut mid-character, which is the one thing §8 says it must never do.
///
/// # Safety
///
/// `ptr` must point to `len` readable, initialised bytes.
#[no_mangle]
pub unsafe extern "C" fn science_string_from_bytes(ptr: *const u8, len: usize) -> ScienceString {
    // SAFETY: the caller guarantees `len` readable bytes at `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if std::str::from_utf8(bytes).is_err() {
        let message = b"science-rt: String constructed from bytes that are not valid UTF-8";
        // SAFETY: a static byte string is a valid buffer of that length.
        unsafe { science_panic_bytes(message.as_ptr(), message.len()) }
    }
    // SAFETY: just validated.
    unsafe { ScienceString::from_raw_utf8(ptr, len) }
}

/// Release a `String`'s buffer.
///
/// **Codegen support:** this is `String`'s drop glue, called when the ownership
/// rules say the value dies (§6.1 rule 6).
///
/// The receiver is left as a valid empty `String` rather than as a dangling
/// one. That is a convenience for codegen — a slot that has been moved out of
/// can be freed again without harm — not a safety net: nothing here detects a
/// second free of the *same buffer*, and nothing is meant to.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`] whose
/// buffer has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn science_string_free(value: *mut ScienceString) {
    // SAFETY: the caller guarantees a live, uniquely borrowed `ScienceString`.
    let value = unsafe { &mut *value };
    // SAFETY: `ptr`/`cap` describe the block this string allocated at align 1.
    unsafe { science_dealloc(value.ptr, value.cap, 1) };
    *value = ScienceString::empty();
}

/// `Clone::clone` for `String`: a fresh, independent copy.
///
/// **Codegen support** for the `Clone` trait of §5.4.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_clone(value: *const ScienceString) -> ScienceString {
    // SAFETY: the caller guarantees a live `ScienceString`.
    let value = unsafe { &*value };
    // SAFETY: a live string's bytes are readable and valid UTF-8.
    unsafe { ScienceString::from_raw_utf8(value.ptr, value.len) }
}

/// `String::len(&self) -> Int`, **in bytes**, as §8 specifies.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_len(value: *const ScienceString) -> i64 {
    // SAFETY: the caller guarantees a live `ScienceString`.
    unsafe { (*value).len as i64 }
}

/// `String::is_empty(&self) -> Bool`.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_is_empty(value: *const ScienceString) -> bool {
    // SAFETY: the caller guarantees a live `ScienceString`.
    unsafe { (*value).len == 0 }
}

/// Raw pointer to a `String`'s bytes.
///
/// **Codegen support**, for passing a Science string to foreign code and for
/// inlining byte-level operations. The pointer is valid for `len` bytes and is
/// invalidated by any mutation of the string.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_as_ptr(value: *const ScienceString) -> *const u8 {
    // SAFETY: the caller guarantees a live `ScienceString`.
    unsafe { (*value).ptr }
}

/// `String::push_str(&mut self, other: &String)`.
///
/// # Safety
///
/// Both pointers must be non-null, aligned and point to live [`ScienceString`]s,
/// and **they must not be the same string**. Appending a string to itself would
/// read through a buffer the reallocation has already freed. §6.1 rule 4
/// forbids holding `&mut self` and `&self` to one value at once, so a program
/// the borrow checker accepted cannot reach that call; this function does not
/// check for it, on purpose.
#[no_mangle]
pub unsafe extern "C" fn science_string_push_str(value: *mut ScienceString, other: *const ScienceString) {
    // SAFETY: the caller guarantees two live, non-aliasing strings.
    let (value, other) = unsafe { (&mut *value, &*other) };
    if other.len == 0 {
        return;
    }
    // SAFETY: `value` is live.
    unsafe { value.reserve(other.len) };
    // SAFETY: `other` has `other.len` readable bytes; `value` now has at least
    // that much spare capacity at `len`; the two blocks are distinct, as the
    // caller guarantees they are different strings.
    unsafe { std::ptr::copy_nonoverlapping(other.ptr, value.ptr.add(value.len), other.len) };
    value.len += other.len;
    // The result is valid UTF-8 because it is the concatenation of two valid
    // UTF-8 sequences, and UTF-8 is self-synchronising: no character's encoding
    // is a suffix of another's followed by a prefix of a third.
}

/// `String::truncate(&self, limit: Int) -> String` — **by characters, never
/// mid-character**, as §8 requires.
///
/// Takes `&self` and returns a **new** `String`: the receiver is untouched.
/// That is what §8's signature says, and what the `Summarize::preview` default
/// method in §4.3 relies on.
///
/// `limit` counts Unicode code points, not bytes. A limit at or beyond the
/// string's character count copies the whole string; a negative limit yields
/// the empty string.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_truncate(value: *const ScienceString, limit: i64) -> ScienceString {
    if limit <= 0 {
        return ScienceString::empty();
    }
    // SAFETY: the caller guarantees a live `ScienceString`.
    let value = unsafe { &*value };
    // SAFETY: as above; the UTF-8 invariant holds.
    let text = unsafe { value.as_str() };

    // The byte index just past the `limit`-th character, or the whole string
    // when there are fewer than `limit` characters. `char_indices` only ever
    // yields character boundaries, so the cut cannot land inside one.
    let end = match text.char_indices().nth(limit as usize) {
        Some((index, _)) => index,
        None => value.len,
    };

    // SAFETY: `end` is a character boundary within the string's bytes.
    unsafe { ScienceString::from_raw_utf8(value.ptr, end) }
}

/// `String::starts_with(&self, prefix: &String) -> Bool`.
///
/// A byte comparison, which is also a character comparison: a valid UTF-8
/// sequence can only be a byte prefix of another at a character boundary.
///
/// # Safety
///
/// Both pointers must be non-null, aligned and point to live [`ScienceString`]s.
/// They may be the same string.
#[no_mangle]
pub unsafe extern "C" fn science_string_starts_with(
    value: *const ScienceString,
    prefix: *const ScienceString,
) -> bool {
    // SAFETY: the caller guarantees two live strings.
    let (value, prefix) = unsafe { ((*value).bytes(), (*prefix).bytes()) };
    value.starts_with(prefix)
}

/// `String::chars(&self) -> Chars`.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`], and it
/// must stay alive and unmodified for as long as the returned [`ScienceChars`] is
/// used. The region engine guarantees that; this function does not check it.
#[no_mangle]
pub unsafe extern "C" fn science_string_chars(value: *const ScienceString) -> ScienceChars {
    // SAFETY: the caller guarantees a live `ScienceString`.
    let value = unsafe { &*value };
    ScienceChars {
        ptr: value.ptr,
        len: value.len,
        offset: 0,
    }
}

/// Advance a [`ScienceChars`], yielding `Char?`.
///
/// This is `Iterate[Char]::next` for `Chars`, and `collections-and-chains.md`
/// gives that method the signature `next(mutable self) -> Self.Item?`. This is
/// the runtime's half of it: the owned-`T?` convention of the crate
/// documentation, §5.3. Returns `true` after writing the next code point to
/// `out` (a `Char`, one `u32` scalar value), or `false` when the iterator is
/// exhausted, in which case `out` is **not written** and the answer is `null`.
/// An exhausted iterator stays exhausted.
///
/// # Safety
///
/// `iter` must be a non-null, aligned pointer to a live [`ScienceChars`] whose
/// string is still alive; `out` must be non-null, aligned for `u32`, and
/// writable.
#[no_mangle]
pub unsafe extern "C" fn science_chars_next(iter: *mut ScienceChars, out: *mut u32) -> bool {
    // SAFETY: the caller guarantees a live, uniquely borrowed `ScienceChars`.
    let iter = unsafe { &mut *iter };
    if iter.offset >= iter.len {
        return false;
    }
    // SAFETY: the iterator borrows a live string, and `offset` is within it and
    // on a character boundary, so the remainder is itself valid UTF-8.
    let rest = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            iter.ptr.add(iter.offset),
            iter.len - iter.offset,
        ))
    };
    // The slice is non-empty, so it has a first character.
    let Some(character) = rest.chars().next() else {
        return false;
    };
    iter.offset += character.len_utf8();
    // SAFETY: the caller guarantees `out` is writable and aligned for `u32`.
    unsafe { *out = character as u32 };
    true
}

/// `Eq::eq` for `String`.
///
/// **Codegen support:** the natural `eq_fn` for a `Map` keyed by `String`, and
/// the implementation of the `==` operator on strings.
///
/// # Safety
///
/// Both pointers must be non-null, aligned and point to live [`ScienceString`]s.
#[no_mangle]
pub unsafe extern "C" fn science_string_eq(a: *const ScienceString, b: *const ScienceString) -> bool {
    // SAFETY: the caller guarantees two live strings.
    let (a, b) = unsafe { ((*a).bytes(), (*b).bytes()) };
    a == b
}

/// `Ord::cmp` for `String`: negative, zero or positive as `a` sorts before,
/// with, or after `b`.
///
/// **Codegen support** for the `Ord` trait of §5.4. The order is
/// lexicographic over bytes, which for UTF-8 is also lexicographic over code
/// points.
///
/// # Safety
///
/// Both pointers must be non-null, aligned and point to live [`ScienceString`]s.
#[no_mangle]
pub unsafe extern "C" fn science_string_cmp(a: *const ScienceString, b: *const ScienceString) -> i32 {
    // SAFETY: the caller guarantees two live strings.
    let (a, b) = unsafe { ((*a).bytes(), (*b).bytes()) };
    match a.cmp(b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Hash a `String`.
///
/// **Codegen support:** the natural `hash_fn` for a `Map` keyed by `String`.
/// FNV-1a — chosen because it is a dozen instructions and has no state to
/// initialise. It is neither cryptographic nor resistant to an adversary
/// choosing keys; when Science grows a story for hostile input, that story
/// replaces this function and nothing else.
///
/// Equal strings hash equal, which is the one property [`ScienceMapInfo`] requires.
///
/// [`ScienceMapInfo`]: crate::ScienceMapInfo
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_hash(value: *const ScienceString) -> u64 {
    // SAFETY: the caller guarantees a live string.
    let bytes = unsafe { (*value).bytes() };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
