//! The shared vocabulary of the ABI: element descriptors, the function-pointer
//! types codegen supplies, and the enum discriminants.
//!
//! See the crate documentation, §5 and §6, for the rules these types encode.

/// Destructor for one value of some Science type.
///
/// The pointer is to a live, initialised value of that type. The function
/// releases whatever the value owns and leaves the slot logically
/// uninitialised; the caller never reads it again.
///
/// A `None` in a [`ScienceTypeInfo`] means the type owns nothing and needs no
/// destructor. Codegen emits `None` for `Copy` types, for primitives, and for
/// any aggregate transitively made of them.
///
/// # Safety
///
/// The pointer must be non-null, aligned for the type, and point to a value of
/// that exact type which has not already been destroyed.
pub type ScienceDropFn = unsafe extern "C" fn(value: *mut u8);

/// Hash one key, for [`ScienceMap`](crate::ScienceMap).
///
/// Codegen derives this from the key type. The only requirement the runtime
/// imposes is consistency with `eq_fn`: **two keys that compare equal must
/// hash equal.** Quality beyond that affects speed, never correctness; the
/// runtime mixes the result before using it, so a hash whose entropy sits in
/// the low bits, the high bits, or nowhere at all still yields a correct table.
///
/// # Safety
///
/// The pointer must be non-null, aligned for the key type, and point to a live
/// key. The function must not mutate it.
pub type ScienceHashFn = unsafe extern "C" fn(key: *const u8) -> u64;

/// Compare two keys for equality, for [`ScienceMap`](crate::ScienceMap).
///
/// Must be an equivalence relation, and must agree with `hash_fn` as described
/// there.
///
/// # Safety
///
/// Both pointers must be non-null, aligned for the key type, and point to live
/// keys. The function must not mutate either.
pub type ScienceEqFn = unsafe extern "C" fn(a: *const u8, b: *const u8) -> bool;

/// Everything the runtime needs to know about a Science type it is storing.
///
/// Codegen emits one `static ScienceTypeInfo` per monomorphised element type and
/// passes a pointer to it on every container call. See the crate
/// documentation, §6, for the convention in full and for the precondition that
/// a container must always be given the same descriptor.
///
/// Layout: three words, `{ size, align, drop_fn }`, in that order.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ScienceTypeInfo {
    /// Size of one value, in bytes. May be zero: Science's `()` is zero-sized and
    /// `Array[()]` is legal. This is the stride as well as the size; Science, like
    /// Rust and unlike C++, has no tail padding that an array may reuse.
    pub size: usize,
    /// Alignment of one value, in bytes. Must be a power of two, and at least
    /// one.
    pub align: usize,
    /// Destructor, or `None` when the type owns nothing.
    pub drop_fn: Option<ScienceDropFn>,
}

/// Everything the runtime needs to know about a `Map[K, V]` instantiation.
///
/// Layout: eight words, `{ key, value, hash_fn, eq_fn }`, where the two
/// descriptors are three words each.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ScienceMapInfo {
    /// Descriptor for the key type.
    pub key: ScienceTypeInfo,
    /// Descriptor for the value type.
    pub value: ScienceTypeInfo,
    /// Hash function for the key type.
    pub hash_fn: ScienceHashFn,
    /// Equality for the key type.
    pub eq_fn: ScienceEqFn,
}

/// Discriminant of `Result::Ok`, the first variant declared in §8.
pub const LINK_RESULT_OK: u8 = 0;

/// Discriminant of `Result::Err`, the second variant declared in §8.
pub const LINK_RESULT_ERR: u8 = 1;

/// Discriminant of `Option::Some` in the tagged layout, the first variant
/// declared in §8.
///
/// Unused when the niche rule of §5.2 applies, because then no discriminant is
/// materialised.
pub const LINK_OPTION_SOME: u8 = 0;

/// Discriminant of `Option::None` in the tagged layout, the second variant
/// declared in §8.
pub const LINK_OPTION_NONE: u8 = 1;

impl ScienceTypeInfo {
    /// The stride between consecutive elements: equal to [`Self::size`].
    #[inline]
    pub(crate) fn stride(&self) -> usize {
        self.size
    }

    /// Byte offset of element `index`.
    ///
    /// Overflow here would mean a container holding more than `isize::MAX`
    /// bytes, which the allocator refused long ago, so a wrapping multiply is
    /// unreachable rather than merely unlikely.
    #[inline]
    pub(crate) fn offset_of(&self, index: usize) -> usize {
        index.wrapping_mul(self.stride())
    }
}
