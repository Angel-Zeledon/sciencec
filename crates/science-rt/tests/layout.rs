//! The ABI is the product. These tests pin every layout decision codegen has to
//! agree with, so that changing one is a deliberate, visible act rather than an
//! accident discovered in a miscompiled binary.

use std::mem::{align_of, offset_of, size_of};

use science_rt::*;

const WORD: usize = size_of::<usize>();

#[test]
fn string_is_pointer_length_capacity() {
    assert_eq!(size_of::<ScienceString>(), 3 * WORD);
    assert_eq!(align_of::<ScienceString>(), align_of::<usize>());
    assert_eq!(offset_of!(ScienceString, ptr), 0);
    assert_eq!(offset_of!(ScienceString, len), WORD);
    assert_eq!(offset_of!(ScienceString, cap), 2 * WORD);
}

#[test]
fn array_is_pointer_length_capacity() {
    assert_eq!(size_of::<ScienceArray>(), 3 * WORD);
    assert_eq!(align_of::<ScienceArray>(), align_of::<usize>());
    assert_eq!(offset_of!(ScienceArray, ptr), 0);
    assert_eq!(offset_of!(ScienceArray, len), WORD);
    assert_eq!(offset_of!(ScienceArray, cap), 2 * WORD);
}

#[test]
fn map_is_six_words() {
    assert_eq!(size_of::<ScienceMap>(), 6 * WORD);
    assert_eq!(offset_of!(ScienceMap, states), 0);
    assert_eq!(offset_of!(ScienceMap, keys), WORD);
    assert_eq!(offset_of!(ScienceMap, values), 2 * WORD);
    assert_eq!(offset_of!(ScienceMap, len), 3 * WORD);
    assert_eq!(offset_of!(ScienceMap, tombstones), 4 * WORD);
    assert_eq!(offset_of!(ScienceMap, cap), 5 * WORD);
}

#[test]
fn chars_is_pointer_length_offset() {
    assert_eq!(size_of::<ScienceChars>(), 3 * WORD);
    assert_eq!(offset_of!(ScienceChars, ptr), 0);
    assert_eq!(offset_of!(ScienceChars, len), WORD);
    assert_eq!(offset_of!(ScienceChars, offset), 2 * WORD);
}

#[test]
fn type_info_is_size_align_drop() {
    assert_eq!(size_of::<ScienceTypeInfo>(), 3 * WORD);
    assert_eq!(offset_of!(ScienceTypeInfo, size), 0);
    assert_eq!(offset_of!(ScienceTypeInfo, align), WORD);
    assert_eq!(offset_of!(ScienceTypeInfo, drop_fn), 2 * WORD);
}

#[test]
fn a_nullable_drop_function_is_exactly_one_pointer() {
    // The same niche rule the language uses for `(Box[T])?`: an absent
    // function pointer is the null pointer, and costs nothing.
    assert_eq!(size_of::<Option<ScienceDropFn>>(), WORD);
    let absent: Option<ScienceDropFn> = None;
    let bits = unsafe { std::mem::transmute::<Option<ScienceDropFn>, usize>(absent) };
    assert_eq!(bits, 0, "the absent drop function is the null pointer");
}

#[test]
fn map_info_is_two_descriptors_and_two_function_pointers() {
    assert_eq!(size_of::<ScienceMapInfo>(), 8 * WORD);
    assert_eq!(offset_of!(ScienceMapInfo, key), 0);
    assert_eq!(offset_of!(ScienceMapInfo, value), 3 * WORD);
    assert_eq!(offset_of!(ScienceMapInfo, hash_fn), 6 * WORD);
    assert_eq!(offset_of!(ScienceMapInfo, eq_fn), 7 * WORD);
}

#[test]
fn io_error_is_one_byte() {
    assert_eq!(size_of::<ScienceIoError>(), 1);
    assert_eq!(align_of::<ScienceIoError>(), 1);
    assert_eq!(ScienceIoError::NOT_FOUND.0, 0);
    assert_eq!(ScienceIoError::PERMISSION_DENIED.0, 1);
    assert_eq!(ScienceIoError::ALREADY_EXISTS.0, 2);
    assert_eq!(ScienceIoError::INVALID_DATA.0, 3);
    assert_eq!(ScienceIoError::OTHER.0, 4);
}

#[test]
fn nullable_io_error_is_a_discriminant_byte_then_the_error() {
    // `IoError` is not pointer-like, so Decision 6 of `type-checking-and-mir.md`
    // §4.1 gives `IoError?` a discriminant byte. Both bytes are alignment 1, so
    // the payload follows the discriminant immediately and there is no padding.
    assert_eq!(size_of::<ScienceNullableIoError>(), 2);
    assert_eq!(align_of::<ScienceNullableIoError>(), 1);
    assert_eq!(offset_of!(ScienceNullableIoError, present), 0);
    assert_eq!(offset_of!(ScienceNullableIoError, error), 1);
}

#[test]
fn a_null_io_error_is_two_zero_bytes_and_a_present_one_is_not() {
    // `null` is the all-zero pattern in the tagged representation just as it is
    // in the niche one, so codegen may emit it as a two-byte zero store. The
    // presence test is still `present` and only `present`; this pins the bytes,
    // not a second way to ask the question.
    let null = ScienceNullableIoError {
        present: SCIENCE_NULLABLE_NULL,
        error: ScienceIoError(0),
    };
    assert_eq!(
        unsafe { std::mem::transmute::<ScienceNullableIoError, [u8; 2]>(null) },
        [0, 0]
    );

    let present = ScienceNullableIoError {
        present: SCIENCE_NULLABLE_PRESENT,
        error: ScienceIoError::INVALID_DATA,
    };
    assert_eq!(
        unsafe { std::mem::transmute::<ScienceNullableIoError, [u8; 2]>(present) },
        [1, 3]
    );
}

#[test]
fn string_and_io_error_is_a_pair_with_both_fields_live() {
    // A pair is a plain struct: `String` at 0 for three words, `IoError?` at 24
    // for two bytes, six bytes of tail padding for the struct's alignment.
    assert_eq!(align_of::<ScienceStringAndIoError>(), align_of::<usize>());
    assert_eq!(size_of::<ScienceStringAndIoError>(), 4 * WORD);
    assert_eq!(offset_of!(ScienceStringAndIoError, value), 0);
    assert_eq!(offset_of!(ScienceStringAndIoError, error), 3 * WORD);
}

#[test]
fn the_nullable_discriminant_is_the_bool_that_the_presence_test_yields() {
    // Null is zero in both representations of §5.2 — the null pointer and the
    // null discriminant agree — and `present` is bit-for-bit the `Bool` of `?`.
    assert_eq!(SCIENCE_NULLABLE_NULL, 0);
    assert_eq!(SCIENCE_NULLABLE_PRESENT, 1);
    assert_eq!(SCIENCE_NULLABLE_NULL, u8::from(false));
    assert_eq!(SCIENCE_NULLABLE_PRESENT, u8::from(true));
}

#[test]
fn slot_states_are_the_documented_bytes() {
    assert_eq!(SCIENCE_MAP_SLOT_EMPTY, 0);
    assert_eq!(SCIENCE_MAP_SLOT_OCCUPIED, 1);
    assert_eq!(SCIENCE_MAP_SLOT_TOMBSTONE, 2);
}

/// The niche rule spelled out as a runnable check: a Science `Box[T]` is a
/// non-null `*mut T`, so `(Box[T])?` is the same pointer with `null`
/// encoded as the null pointer, and costs not one bit more.
#[test]
fn a_nullable_box_costs_nothing_extra() {
    assert_eq!(size_of::<*mut u8>(), WORD);
    assert_eq!(size_of::<Option<std::ptr::NonNull<u8>>>(), WORD);

    // And the runtime relies on exactly that: `get` hands back the niche form
    // directly, as a possibly-null pointer.
    unsafe {
        let info = ScienceTypeInfo {
            size: 8,
            align: 8,
            drop_fn: None,
        };
        let mut a = science_array_new(&info);
        assert!(
            science_array_get(&a, &info, 0).is_null(),
            "null is the null pointer"
        );
        let value = 1i64;
        science_array_push(&mut a, &info, (&raw const value).cast::<u8>());
        assert!(
            !science_array_get(&a, &info, 0).is_null(),
            "a present borrow is the pointer itself"
        );
        science_array_free(&mut a, &info);
    }
}

/// Every entry point returning an aggregate by value is named in §2's `sret`
/// list, and nothing else is.
///
/// The list was maintained by hand and `science_array_with_capacity` was
/// missing from it — found by writing a code generator's design against the
/// page and then checking it against the signatures. Omission there is silent
/// memory corruption, not a build failure, so the list is pinned here.
///
/// This asserts sizes rather than parsing signatures: a return of three words
/// or more is MEMORY on every target the project supports, so "returns an
/// aggregate this big" and "needs `sret`" are the same question.
#[test]
fn every_aggregate_return_is_three_words_or_more() {
    let word = std::mem::size_of::<usize>();
    for (name, size) in [
        ("science_string_new / clone / truncate", std::mem::size_of::<ScienceString>()),
        ("science_array_new / with_capacity", std::mem::size_of::<ScienceArray>()),
        ("science_map_new", std::mem::size_of::<ScienceMap>()),
        ("science_read_file", std::mem::size_of::<ScienceStringAndIoError>()),
    ] {
        assert!(
            size >= 3 * word,
            "{name} returns {size} bytes, which is under three words: \
             it is no longer a MEMORY return and §2's sret list is wrong about it"
        );
    }
    // The stated exception: two bytes, returned in a register.
    assert_eq!(std::mem::size_of::<ScienceNullableIoError>(), 2);
}
