//! The ABI is the product. These tests pin every layout decision codegen has to
//! agree with, so that changing one is a deliberate, visible act rather than an
//! accident discovered in a miscompiled binary.

use std::mem::{align_of, offset_of, size_of};

use link_rt::*;

const WORD: usize = size_of::<usize>();

#[test]
fn string_is_pointer_length_capacity() {
    assert_eq!(size_of::<LinkString>(), 3 * WORD);
    assert_eq!(align_of::<LinkString>(), align_of::<usize>());
    assert_eq!(offset_of!(LinkString, ptr), 0);
    assert_eq!(offset_of!(LinkString, len), WORD);
    assert_eq!(offset_of!(LinkString, cap), 2 * WORD);
}

#[test]
fn array_is_pointer_length_capacity() {
    assert_eq!(size_of::<LinkArray>(), 3 * WORD);
    assert_eq!(align_of::<LinkArray>(), align_of::<usize>());
    assert_eq!(offset_of!(LinkArray, ptr), 0);
    assert_eq!(offset_of!(LinkArray, len), WORD);
    assert_eq!(offset_of!(LinkArray, cap), 2 * WORD);
}

#[test]
fn map_is_six_words() {
    assert_eq!(size_of::<LinkMap>(), 6 * WORD);
    assert_eq!(offset_of!(LinkMap, states), 0);
    assert_eq!(offset_of!(LinkMap, keys), WORD);
    assert_eq!(offset_of!(LinkMap, values), 2 * WORD);
    assert_eq!(offset_of!(LinkMap, len), 3 * WORD);
    assert_eq!(offset_of!(LinkMap, tombstones), 4 * WORD);
    assert_eq!(offset_of!(LinkMap, cap), 5 * WORD);
}

#[test]
fn chars_is_pointer_length_offset() {
    assert_eq!(size_of::<LinkChars>(), 3 * WORD);
    assert_eq!(offset_of!(LinkChars, ptr), 0);
    assert_eq!(offset_of!(LinkChars, len), WORD);
    assert_eq!(offset_of!(LinkChars, offset), 2 * WORD);
}

#[test]
fn type_info_is_size_align_drop() {
    assert_eq!(size_of::<LinkTypeInfo>(), 3 * WORD);
    assert_eq!(offset_of!(LinkTypeInfo, size), 0);
    assert_eq!(offset_of!(LinkTypeInfo, align), WORD);
    assert_eq!(offset_of!(LinkTypeInfo, drop_fn), 2 * WORD);
}

#[test]
fn a_nullable_drop_function_is_exactly_one_pointer() {
    // The same niche rule the language uses for `Option[Box[T]]`: an absent
    // function pointer is the null pointer, and costs nothing.
    assert_eq!(size_of::<Option<LinkDropFn>>(), WORD);
    let absent: Option<LinkDropFn> = None;
    let bits = unsafe { std::mem::transmute::<Option<LinkDropFn>, usize>(absent) };
    assert_eq!(bits, 0, "the absent drop function is the null pointer");
}

#[test]
fn map_info_is_two_descriptors_and_two_function_pointers() {
    assert_eq!(size_of::<LinkMapInfo>(), 8 * WORD);
    assert_eq!(offset_of!(LinkMapInfo, key), 0);
    assert_eq!(offset_of!(LinkMapInfo, value), 3 * WORD);
    assert_eq!(offset_of!(LinkMapInfo, hash_fn), 6 * WORD);
    assert_eq!(offset_of!(LinkMapInfo, eq_fn), 7 * WORD);
}

#[test]
fn io_error_is_one_byte() {
    assert_eq!(size_of::<LinkIoError>(), 1);
    assert_eq!(align_of::<LinkIoError>(), 1);
    assert_eq!(LinkIoError::NOT_FOUND.0, 0);
    assert_eq!(LinkIoError::PERMISSION_DENIED.0, 1);
    assert_eq!(LinkIoError::ALREADY_EXISTS.0, 2);
    assert_eq!(LinkIoError::INVALID_DATA.0, 3);
    assert_eq!(LinkIoError::OTHER.0, 4);
}

#[test]
fn result_string_io_error_is_a_tag_then_a_union() {
    // tag: u8 at 0; payload aligned to 8, hence at 8; String is 3 words.
    assert_eq!(align_of::<LinkIoResultString>(), align_of::<usize>());
    assert_eq!(size_of::<LinkIoResultString>(), 4 * WORD);
    assert_eq!(offset_of!(LinkIoResultString, tag), 0);
    assert_eq!(offset_of!(LinkIoResultString, payload), WORD);
}

#[test]
fn result_unit_io_error_degenerates_to_two_bytes() {
    // The `Ok` payload is `()`, which is zero-sized, so the union is just the
    // error byte and the whole enum needs no padding at all.
    assert_eq!(size_of::<LinkIoResultUnit>(), 2);
    assert_eq!(align_of::<LinkIoResultUnit>(), 1);
    assert_eq!(offset_of!(LinkIoResultUnit, tag), 0);
    assert_eq!(offset_of!(LinkIoResultUnit, err), 1);
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

/// The niche rule spelled out as a runnable check: a Link `Box[T]` is a
/// non-null `*mut T`, so `Option[Box[T]]` is the same pointer with `None`
/// encoded as null, and costs not one bit more.
#[test]
fn option_of_box_costs_nothing_extra() {
    assert_eq!(size_of::<*mut u8>(), WORD);
    assert_eq!(size_of::<Option<std::ptr::NonNull<u8>>>(), WORD);

    // And the runtime relies on exactly that: `get` hands back the niche form
    // directly, as a possibly-null pointer.
    unsafe {
        let info = LinkTypeInfo {
            size: 8,
            align: 8,
            drop_fn: None,
        };
        let mut a = link_array_new(&info);
        assert!(
            link_array_get(&a, &info, 0).is_null(),
            "None is the null pointer"
        );
        let value = 1i64;
        link_array_push(&mut a, &info, (&raw const value).cast::<u8>());
        assert!(
            !link_array_get(&a, &info, 0).is_null(),
            "Some(&T) is the pointer itself"
        );
        link_array_free(&mut a, &info);
    }
}
