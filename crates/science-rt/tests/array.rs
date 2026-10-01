//! `Array[T]`: the §8 method set over the element-descriptor convention.

mod common;

use common::*;
use science_rt::*;

/// Push an `i64` by value, the way codegen would: materialise the element in a
/// slot it owns, then hand the runtime a pointer to it.
unsafe fn push_i64(array: &mut ScienceArray, info: &ScienceTypeInfo, value: i64) {
    science_array_push(array, info, (&raw const value).cast::<u8>());
}

unsafe fn get_i64(array: &ScienceArray, info: &ScienceTypeInfo, index: i64) -> Option<i64> {
    let p = science_array_get(array, info, index);
    if p.is_null() {
        None
    } else {
        Some(*p.cast::<i64>())
    }
}

#[test]
fn new_is_empty() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        assert_eq!(science_array_len(&a), 0);
        assert!(science_array_is_empty(&a));
        assert_eq!(a.cap, 0, "a new Array allocates nothing");
        assert!(!a.ptr.is_null(), "but its pointer is dangling, not null");
        assert_eq!(a.ptr as usize % info.align, 0);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn push_then_get() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        push_i64(&mut a, &info, 42);
        assert_eq!(science_array_len(&a), 1);
        assert!(!science_array_is_empty(&a));
        assert_eq!(get_i64(&a, &info, 0), Some(42));
        science_array_free(&mut a, &info);
    }
}

#[test]
fn get_out_of_range_is_null() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        assert!(science_array_get(&a, &info, 0).is_null(), "empty array");
        push_i64(&mut a, &info, 1);
        assert!(!science_array_get(&a, &info, 0).is_null());
        assert!(science_array_get(&a, &info, 1).is_null(), "one past the end");
        assert!(science_array_get(&a, &info, -1).is_null(), "negative index");
        assert!(science_array_get(&a, &info, i64::MAX).is_null());
        assert!(science_array_get(&a, &info, i64::MIN).is_null());
        science_array_free(&mut a, &info);
    }
}

#[test]
fn get_mut_writes_through() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        push_i64(&mut a, &info, 1);
        push_i64(&mut a, &info, 2);
        let p = science_array_get_mut(&mut a, &info, 1);
        assert!(!p.is_null());
        *p.cast::<i64>() = 99;
        assert_eq!(get_i64(&a, &info, 1), Some(99));
        assert_eq!(get_i64(&a, &info, 0), Some(1));
        assert!(science_array_get_mut(&mut a, &info, 2).is_null());
        assert!(science_array_get_mut(&mut a, &info, -1).is_null());
        science_array_free(&mut a, &info);
    }
}

#[test]
fn growth_reallocates_and_preserves_every_element() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        let mut caps = Vec::new();
        for i in 0..1000i64 {
            push_i64(&mut a, &info, i);
            caps.push(a.cap);
        }
        assert_eq!(science_array_len(&a), 1000);
        assert!(a.cap >= 1000);
        for i in 0..1000i64 {
            assert_eq!(get_i64(&a, &info, i), Some(i), "element {i} lost");
        }
        // Amortised doubling: far fewer reallocations than pushes.
        let reallocs = caps.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(reallocs < 20, "{reallocs} reallocations for 1000 pushes");
        science_array_free(&mut a, &info);
    }
}

#[test]
fn growth_preserves_over_alignment() {
    unsafe {
        let info = over_info();
        let mut a = science_array_new(&info);
        for i in 0..200u64 {
            let v = Over { id: i };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
            assert_eq!(a.ptr as usize % 32, 0, "alignment lost at push {i}");
        }
        for i in 0..200u64 {
            let p = science_array_get(&a, &info, i as i64);
            assert_eq!(*p.cast::<Over>(), Over { id: i });
        }
        science_array_free(&mut a, &info);
    }
}

#[test]
fn reserve_allocates_up_front_without_changing_len() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        science_array_reserve(&mut a, &info, 500);
        assert!(a.cap >= 500);
        assert_eq!(science_array_len(&a), 0);
        let cap = a.cap;
        for i in 0..500i64 {
            push_i64(&mut a, &info, i);
        }
        assert_eq!(a.cap, cap, "no reallocation within the reserved capacity");
        science_array_free(&mut a, &info);
    }
}

#[test]
fn with_capacity_allocates_up_front() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_with_capacity(&info, 64);
        assert!(a.cap >= 64);
        assert_eq!(science_array_len(&a), 0);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn pop_returns_elements_last_in_first_out() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        for i in 0..5i64 {
            push_i64(&mut a, &info, i);
        }
        let mut out: i64 = 0;
        for expected in (0..5i64).rev() {
            assert!(science_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
            assert_eq!(out, expected);
        }
        assert_eq!(science_array_len(&a), 0);
        assert!(science_array_is_empty(&a));
        science_array_free(&mut a, &info);
    }
}

#[test]
fn pop_of_empty_is_none() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        let mut out: i64 = -1;
        assert!(!science_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
        assert_eq!(
            out, -1,
            "the out slot is untouched when there is nothing to pop"
        );
        science_array_free(&mut a, &info);
    }
}

#[test]
fn single_element_round_trip() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        push_i64(&mut a, &info, 7);
        let mut out: i64 = 0;
        assert!(science_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
        assert_eq!(out, 7);
        assert!(science_array_is_empty(&a));
        assert!(science_array_get(&a, &info, 0).is_null());
        assert!(a.cap > 0, "popping does not release capacity");
        science_array_free(&mut a, &info);
    }
}

#[test]
fn free_drops_every_remaining_element_exactly_once() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = science_array_new(&info);
        for id in 0..100u64 {
            let v = Counted { id };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        assert_eq!(drops(), 0, "pushing moves, it does not drop");
        science_array_free(&mut a, &info);
        assert_eq!(drops(), 100);
        assert_eq!(a.len, 0);
        assert_eq!(a.cap, 0);
    }
}

#[test]
fn pop_moves_out_without_dropping() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = science_array_new(&info);
        for id in 0..3u64 {
            let v = Counted { id };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        let mut out = Counted { id: u64::MAX };
        assert!(science_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
        assert_eq!(out, Counted { id: 2 });
        assert_eq!(drops(), 0, "pop transfers ownership to the caller");
        science_array_free(&mut a, &info);
        assert_eq!(drops(), 2, "only the two elements still owned by the array");
    }
}

#[test]
fn free_of_an_empty_array_drops_nothing() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = science_array_new(&info);
        science_array_free(&mut a, &info);
        assert_eq!(drops(), 0);
    }
}

#[test]
fn growth_moves_elements_bitwise_without_dropping_them() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = science_array_new(&info);
        for id in 0..500u64 {
            let v = Counted { id };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        assert_eq!(drops(), 0, "reallocation is a move, never a drop");
        for id in 0..500u64 {
            let p = science_array_get(&a, &info, id as i64);
            assert_eq!(*p.cast::<Counted>(), Counted { id });
        }
        science_array_free(&mut a, &info);
        assert_eq!(drops(), 500);
    }
}

#[test]
fn zero_sized_elements_need_no_allocation() {
    unsafe {
        let info = unit_info();
        let mut a = science_array_new(&info);
        let unit: [u8; 0] = [];
        for _ in 0..1000 {
            science_array_push(&mut a, &info, unit.as_ptr());
        }
        assert_eq!(science_array_len(&a), 1000);
        assert!(!science_array_get(&a, &info, 999).is_null());
        assert!(science_array_get(&a, &info, 1000).is_null());
        let mut out: [u8; 0] = [];
        for _ in 0..1000 {
            assert!(science_array_pop(&mut a, &info, out.as_mut_ptr()));
        }
        assert!(!science_array_pop(&mut a, &info, out.as_mut_ptr()));
        science_array_free(&mut a, &info);
    }
}

#[test]
fn as_ptr_walks_the_elements_in_order() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        for i in 0..16i64 {
            push_i64(&mut a, &info, i * 3);
        }
        let base = science_array_as_ptr(&a).cast::<i64>();
        for i in 0..16isize {
            assert_eq!(*base.offset(i), i as i64 * 3);
        }
        science_array_free(&mut a, &info);
    }
}

#[test]
fn an_array_of_strings_drops_its_strings() {
    unsafe {
        let info = string_info();
        let mut a = science_array_new(&info);
        for i in 0..50 {
            let v = s(&format!("element number {i}"));
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        let p = science_array_get(&a, &info, 7);
        assert_eq!(as_str(&*p.cast::<ScienceString>()), "element number 7");
        // Every one of the 50 heap buffers is released here; a leak shows up
        // under Miri's leak check.
        science_array_free(&mut a, &info);
    }
}

// --- The rest of Level 1's surface ----------------------------------------------
//
// Every test below reads the elements back after the operation, because an
// operation over opaque bytes that moved the wrong `size` would still report
// the right length. The panicking paths — an index out of range for `insert`,
// `swap` and `replace` — abort the process, so they are measured end to end in
// `science-codegen-llvm`'s `tests/arrays.rs`, where the program is a child.

/// Every element, in order, read through `science_array_get`.
unsafe fn all_i64(array: &ScienceArray, info: &ScienceTypeInfo) -> Vec<i64> {
    (0..science_array_len(array)).map(|i| get_i64(array, info, i).unwrap()).collect()
}

unsafe fn array_of(info: &ScienceTypeInfo, values: &[i64]) -> ScienceArray {
    let mut a = science_array_new(info);
    for &v in values {
        push_i64(&mut a, info, v);
    }
    a
}

/// The `(Int, ())` map descriptor `contains` and `index_of` take: an `Int`
/// key with its equality, and a zero-sized value nothing reads.
fn int_eq_info() -> ScienceMapInfo {
    ScienceMapInfo { key: i64_info(), value: unit_info(), hash_fn: hash_u64, eq_fn: eq_u64 }
}

#[test]
fn capacity_reports_the_buffer_and_not_the_length() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_with_capacity(&info, 10);
        assert_eq!(science_array_capacity(&a), 10);
        push_i64(&mut a, &info, 1);
        assert_eq!(science_array_capacity(&a), 10);
        science_array_free(&mut a, &info);
        let unit = unit_info();
        let z = science_array_new(&unit);
        assert_eq!(science_array_capacity(&z), i64::MAX, "unbounded, clamped to Int.MAX");
    }
}

#[test]
fn first_and_last_are_the_ends_or_null() {
    unsafe {
        let info = i64_info();
        let mut a = science_array_new(&info);
        assert!(science_array_first(&a, &info).is_null());
        assert!(science_array_last(&a, &info).is_null());
        for v in [4, 5, 6] {
            push_i64(&mut a, &info, v);
        }
        assert_eq!(*science_array_first(&a, &info).cast::<i64>(), 4);
        assert_eq!(*science_array_last(&a, &info).cast::<i64>(), 6);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn insert_shifts_the_tail_up_at_every_position() {
    unsafe {
        let info = i64_info();
        let mut a = array_of(&info, &[1, 2, 3]);
        let v = 10i64;
        science_array_insert(&mut a, &info, 0, (&raw const v).cast());
        let v = 20i64;
        science_array_insert(&mut a, &info, 2, (&raw const v).cast());
        let v = 30i64;
        science_array_insert(&mut a, &info, 5, (&raw const v).cast());
        assert_eq!(all_i64(&a, &info), [10, 1, 20, 2, 3, 30], "front, middle, and at `len`");
        science_array_free(&mut a, &info);
    }
}

#[test]
fn remove_closes_the_gap_and_out_of_range_writes_nothing() {
    unsafe {
        let info = i64_info();
        let mut a = array_of(&info, &[1, 2, 3, 4]);
        let mut out = -1i64;
        assert!(science_array_remove(&mut a, &info, 1, (&raw mut out).cast()));
        assert_eq!(out, 2);
        assert_eq!(all_i64(&a, &info), [1, 3, 4]);
        assert!(science_array_remove(&mut a, &info, 2, (&raw mut out).cast()), "the last");
        assert_eq!(out, 4);
        let mut untouched = -7i64;
        for bad in [3, -1, i64::MAX] {
            assert!(!science_array_remove(&mut a, &info, bad, (&raw mut untouched).cast()));
        }
        assert_eq!(untouched, -7, "`false` writes nothing");
        assert_eq!(all_i64(&a, &info), [1, 3]);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn swap_exchanges_and_swapping_with_itself_is_nothing() {
    unsafe {
        let info = i64_info();
        let mut a = array_of(&info, &[1, 2, 3]);
        science_array_swap(&mut a, &info, 0, 2);
        science_array_swap(&mut a, &info, 1, 1);
        assert_eq!(all_i64(&a, &info), [3, 2, 1]);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn replace_hands_back_the_old_element_and_stores_the_new() {
    unsafe {
        let info = i64_info();
        let mut a = array_of(&info, &[1, 2, 3]);
        let value = 99i64;
        let mut out = 0i64;
        science_array_replace(&mut a, &info, 1, (&raw const value).cast(), (&raw mut out).cast());
        assert_eq!(out, 2);
        assert_eq!(all_i64(&a, &info), [1, 99, 3]);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn reverse_reverses_odd_and_even_lengths() {
    unsafe {
        let info = i64_info();
        for values in [&[][..], &[1], &[1, 2], &[1, 2, 3], &[1, 2, 3, 4, 5, 6]] {
            let mut a = array_of(&info, values);
            science_array_reverse(&mut a, &info);
            let mut expected = values.to_vec();
            expected.reverse();
            assert_eq!(all_i64(&a, &info), expected);
            science_array_free(&mut a, &info);
        }
    }
}

#[test]
fn truncate_and_clear_drop_exactly_what_they_cut() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = science_array_new(&info);
        for id in 0..10u64 {
            let v = Counted { id };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        let cap = a.cap;
        science_array_truncate(&mut a, &info, 20);
        assert_eq!((a.len, drops()), (10, 0), "past the end changes nothing");
        science_array_truncate(&mut a, &info, 7);
        assert_eq!((a.len, drops()), (7, 3));
        science_array_truncate(&mut a, &info, -5);
        assert_eq!((a.len, drops()), (0, 10), "negative empties, as `String.truncate`");
        assert_eq!(a.cap, cap, "the buffer is kept");
        for id in 0..4u64 {
            let v = Counted { id };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        science_array_clear(&mut a, &info);
        assert_eq!((a.len, drops()), (0, 14));
        science_array_free(&mut a, &info);
        assert_eq!(drops(), 14, "nothing is dropped twice");
    }
}

#[test]
fn extend_moves_every_element_and_drops_none() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = science_array_new(&info);
        let mut b = science_array_new(&info);
        for id in 0..3u64 {
            let v = Counted { id };
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
            let w = Counted { id: id + 10 };
            science_array_push(&mut b, &info, (&raw const w).cast::<u8>());
        }
        science_array_extend(&mut a, &info, &mut b);
        assert_eq!(drops(), 0, "moved, not dropped");
        assert_eq!((b.len, b.cap), (0, 0), "the source is left empty and released");
        let ids: Vec<u64> = (0..6)
            .map(|i| (*science_array_get(&a, &info, i).cast::<Counted>()).id)
            .collect();
        assert_eq!(ids, [0, 1, 2, 10, 11, 12]);
        science_array_free(&mut b, &info);
        science_array_free(&mut a, &info);
        assert_eq!(drops(), 6, "each once, by the array that owns it now");
    }
}

#[test]
fn contains_and_index_of_use_the_descriptors_equality() {
    unsafe {
        let info = i64_info();
        let eq = int_eq_info();
        let mut a = array_of(&info, &[5, 7, 5, 9]);
        let (seven, eight) = (7i64, 8i64);
        assert!(science_array_contains(&a, &eq, (&raw const seven).cast()));
        assert!(!science_array_contains(&a, &eq, (&raw const eight).cast()));
        let five = 5i64;
        let mut at = -1i64;
        assert!(science_array_index_of(&a, &eq, (&raw const five).cast(), &mut at));
        assert_eq!(at, 0, "the first match");
        let mut untouched = -3i64;
        assert!(!science_array_index_of(&a, &eq, (&raw const eight).cast(), &mut untouched));
        assert_eq!(untouched, -3);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn contains_compares_strings_by_content() {
    unsafe {
        let info = string_info();
        let eq = ScienceMapInfo {
            key: string_info(),
            value: unit_info(),
            hash_fn: hash_link_string,
            eq_fn: eq_link_string,
        };
        let mut a = science_array_new(&info);
        for word in ["alpha", "beta"] {
            let v = s(word);
            science_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        let probe = s("beta");
        let mut at = -1i64;
        assert!(science_array_index_of(&a, &eq, (&raw const probe).cast(), &mut at));
        assert_eq!(at, 1);
        free(probe);
        science_array_free(&mut a, &info);
    }
}

#[test]
fn sort_orders_ints_and_strings_ascending() {
    unsafe {
        let info = i64_info();
        let mut a = array_of(&info, &[3, -1, 2, i64::MIN, 0]);
        science_array_sort_i64(&mut a);
        assert_eq!(all_i64(&a, &info), [i64::MIN, -1, 0, 2, 3], "signed order");
        science_array_free(&mut a, &info);

        let info = string_info();
        let mut b = science_array_new(&info);
        for word in ["pear", "Apple", "apple", "ä", "banana"] {
            let v = s(word);
            science_array_push(&mut b, &info, (&raw const v).cast::<u8>());
        }
        science_array_sort_string(&mut b);
        let words: Vec<&str> = (0..5)
            .map(|i| as_str(&*science_array_get(&b, &info, i).cast::<ScienceString>()))
            .collect();
        assert_eq!(words, ["Apple", "apple", "banana", "pear", "ä"], "byte order, §6.8");
        science_array_free(&mut b, &info);
    }
}
