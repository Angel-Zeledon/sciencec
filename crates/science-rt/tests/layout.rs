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
    // The same niche rule the language uses for `Option[Box[T]]`: an absent
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
fn result_string_io_error_is_a_tag_then_a_union() {
    // tag: u8 at 0; payload aligned to 8, hence at 8; String is 3 words.
    assert_eq!(align_of::<ScienceIoResultString>(), align_of::<usize>());
    assert_eq!(size_of::<ScienceIoResultString>(), 4 * WORD);
    assert_eq!(offset_of!(ScienceIoResultString, tag), 0);
    assert_eq!(offset_of!(ScienceIoResultString, payload), WORD);
}

#[test]
fn result_unit_io_error_degenerates_to_two_bytes() {
    // The `Ok` payload is `()`, which is zero-sized, so the union is just the
    // error byte and the whole enum needs no padding at all.
    assert_eq!(size_of::<ScienceIoResultUnit>(), 2);
    assert_eq!(align_of::<ScienceIoResultUnit>(), 1);
    assert_eq!(offset_of!(ScienceIoResultUnit, tag), 0);
    assert_eq!(offset_of!(ScienceIoResultUnit, err), 1);
}

#[test]
fn discriminants_follow_declaration_order() {
    // `enum Result[T, E]: Ok(T) / Err(E)` and `enum Option[T]: Some(T) / None`.
    assert_eq!(LINK_RESULT_OK, 0);
    assert_eq!(LINK_RESULT_ERR, 1);
    assert_eq!(LINK_OPTION_SOME, 0);
    assert_eq!(LINK_OPTION_NONE, 1);
}

#[test]
fn slot_states_are_the_documented_bytes() {
    assert_eq!(LINK_MAP_SLOT_EMPTY, 0);
    assert_eq!(LINK_MAP_SLOT_OCCUPIED, 1);
    assert_eq!(LINK_MAP_SLOT_TOMBSTONE, 2);
}

/// The niche rule spelled out as a runnable check: a Science `Box[T]` is a
/// non-null `*mut T`, so `Option[Box[T]]` is the same pointer with `None`
/// encoded as null, and costs not one bit more.
#[test]
fn option_of_box_costs_nothing_extra() {
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
            "None is the null pointer"
        );
        let value = 1i64;
        science_array_push(&mut a, &info, (&raw const value).cast::<u8>());
        assert!(
            !science_array_get(&a, &info, 0).is_null(),
            "Some(&T) is the pointer itself"
        );
        science_array_free(&mut a, &info);
    }
}
