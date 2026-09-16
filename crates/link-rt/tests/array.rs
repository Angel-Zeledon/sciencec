//! `Array[T]`: the §8 method set over the element-descriptor convention.

mod common;

use common::*;
use link_rt::*;

/// Push an `i64` by value, the way codegen would: materialise the element in a
/// slot it owns, then hand the runtime a pointer to it.
unsafe fn push_i64(array: &mut LinkArray, info: &LinkTypeInfo, value: i64) {
    link_array_push(array, info, (&raw const value).cast::<u8>());
}

unsafe fn get_i64(array: &LinkArray, info: &LinkTypeInfo, index: i64) -> Option<i64> {
    let p = link_array_get(array, info, index);
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
        let mut a = link_array_new(&info);
        assert_eq!(link_array_len(&a), 0);
        assert!(link_array_is_empty(&a));
        assert_eq!(a.cap, 0, "a new Array allocates nothing");
        assert!(!a.ptr.is_null(), "but its pointer is dangling, not null");
        assert_eq!(a.ptr as usize % info.align, 0);
        link_array_free(&mut a, &info);
    }
}

#[test]
fn push_then_get() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        push_i64(&mut a, &info, 42);
        assert_eq!(link_array_len(&a), 1);
        assert!(!link_array_is_empty(&a));
        assert_eq!(get_i64(&a, &info, 0), Some(42));
        link_array_free(&mut a, &info);
    }
}

#[test]
fn get_out_of_range_is_null() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        assert!(link_array_get(&a, &info, 0).is_null(), "empty array");
        push_i64(&mut a, &info, 1);
        assert!(!link_array_get(&a, &info, 0).is_null());
        assert!(link_array_get(&a, &info, 1).is_null(), "one past the end");
        assert!(link_array_get(&a, &info, -1).is_null(), "negative index");
        assert!(link_array_get(&a, &info, i64::MAX).is_null());
        assert!(link_array_get(&a, &info, i64::MIN).is_null());
        link_array_free(&mut a, &info);
    }
}

#[test]
fn get_mut_writes_through() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        push_i64(&mut a, &info, 1);
        push_i64(&mut a, &info, 2);
        let p = link_array_get_mut(&mut a, &info, 1);
        assert!(!p.is_null());
        *p.cast::<i64>() = 99;
        assert_eq!(get_i64(&a, &info, 1), Some(99));
        assert_eq!(get_i64(&a, &info, 0), Some(1));
        assert!(link_array_get_mut(&mut a, &info, 2).is_null());
        assert!(link_array_get_mut(&mut a, &info, -1).is_null());
        link_array_free(&mut a, &info);
    }
}

#[test]
fn growth_reallocates_and_preserves_every_element() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        let mut caps = Vec::new();
        for i in 0..1000i64 {
            push_i64(&mut a, &info, i);
            caps.push(a.cap);
        }
        assert_eq!(link_array_len(&a), 1000);
        assert!(a.cap >= 1000);
        for i in 0..1000i64 {
            assert_eq!(get_i64(&a, &info, i), Some(i), "element {i} lost");
        }
        // Amortised doubling: far fewer reallocations than pushes.
        let reallocs = caps.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(reallocs < 20, "{reallocs} reallocations for 1000 pushes");
        link_array_free(&mut a, &info);
    }
}

#[test]
fn growth_preserves_over_alignment() {
    unsafe {
        let info = over_info();
        let mut a = link_array_new(&info);
        for i in 0..200u64 {
            let v = Over { id: i };
            link_array_push(&mut a, &info, (&raw const v).cast::<u8>());
            assert_eq!(a.ptr as usize % 32, 0, "alignment lost at push {i}");
        }
        for i in 0..200u64 {
            let p = link_array_get(&a, &info, i as i64);
            assert_eq!(*p.cast::<Over>(), Over { id: i });
        }
        link_array_free(&mut a, &info);
    }
}

#[test]
fn reserve_allocates_up_front_without_changing_len() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        link_array_reserve(&mut a, &info, 500);
        assert!(a.cap >= 500);
        assert_eq!(link_array_len(&a), 0);
        let cap = a.cap;
        for i in 0..500i64 {
            push_i64(&mut a, &info, i);
        }
        assert_eq!(a.cap, cap, "no reallocation within the reserved capacity");
        link_array_free(&mut a, &info);
    }
}

#[test]
fn with_capacity_allocates_up_front() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_with_capacity(&info, 64);
        assert!(a.cap >= 64);
        assert_eq!(link_array_len(&a), 0);
        link_array_free(&mut a, &info);
    }
}

#[test]
fn pop_returns_elements_last_in_first_out() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        for i in 0..5i64 {
            push_i64(&mut a, &info, i);
        }
        let mut out: i64 = 0;
        for expected in (0..5i64).rev() {
            assert!(link_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
            assert_eq!(out, expected);
        }
        assert_eq!(link_array_len(&a), 0);
        assert!(link_array_is_empty(&a));
        link_array_free(&mut a, &info);
    }
}

#[test]
fn pop_of_empty_is_none() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        let mut out: i64 = -1;
        assert!(!link_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
        assert_eq!(
            out, -1,
            "the out slot is untouched when there is nothing to pop"
        );
        link_array_free(&mut a, &info);
    }
}

#[test]
fn single_element_round_trip() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        push_i64(&mut a, &info, 7);
        let mut out: i64 = 0;
        assert!(link_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
        assert_eq!(out, 7);
        assert!(link_array_is_empty(&a));
        assert!(link_array_get(&a, &info, 0).is_null());
        assert!(a.cap > 0, "popping does not release capacity");
        link_array_free(&mut a, &info);
    }
}

#[test]
fn free_drops_every_remaining_element_exactly_once() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = link_array_new(&info);
        for id in 0..100u64 {
            let v = Counted { id };
            link_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        assert_eq!(drops(), 0, "pushing moves, it does not drop");
        link_array_free(&mut a, &info);
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
        let mut a = link_array_new(&info);
        for id in 0..3u64 {
            let v = Counted { id };
            link_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        let mut out = Counted { id: u64::MAX };
        assert!(link_array_pop(&mut a, &info, (&raw mut out).cast::<u8>()));
        assert_eq!(out, Counted { id: 2 });
        assert_eq!(drops(), 0, "pop transfers ownership to the caller");
        link_array_free(&mut a, &info);
        assert_eq!(drops(), 2, "only the two elements still owned by the array");
    }
}

#[test]
fn free_of_an_empty_array_drops_nothing() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = link_array_new(&info);
        link_array_free(&mut a, &info);
        assert_eq!(drops(), 0);
    }
}

#[test]
fn growth_moves_elements_bitwise_without_dropping_them() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let mut a = link_array_new(&info);
        for id in 0..500u64 {
            let v = Counted { id };
            link_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        assert_eq!(drops(), 0, "reallocation is a move, never a drop");
        for id in 0..500u64 {
            let p = link_array_get(&a, &info, id as i64);
            assert_eq!(*p.cast::<Counted>(), Counted { id });
        }
        link_array_free(&mut a, &info);
        assert_eq!(drops(), 500);
    }
}

#[test]
fn zero_sized_elements_need_no_allocation() {
    unsafe {
        let info = unit_info();
        let mut a = link_array_new(&info);
        let unit: [u8; 0] = [];
        for _ in 0..1000 {
            link_array_push(&mut a, &info, unit.as_ptr());
        }
        assert_eq!(link_array_len(&a), 1000);
        assert!(!link_array_get(&a, &info, 999).is_null());
        assert!(link_array_get(&a, &info, 1000).is_null());
        let mut out: [u8; 0] = [];
        for _ in 0..1000 {
            assert!(link_array_pop(&mut a, &info, out.as_mut_ptr()));
        }
        assert!(!link_array_pop(&mut a, &info, out.as_mut_ptr()));
        link_array_free(&mut a, &info);
    }
}

#[test]
fn as_ptr_walks_the_elements_in_order() {
    unsafe {
        let info = i64_info();
        let mut a = link_array_new(&info);
        for i in 0..16i64 {
            push_i64(&mut a, &info, i * 3);
        }
        let base = link_array_as_ptr(&a).cast::<i64>();
        for i in 0..16isize {
            assert_eq!(*base.offset(i), i as i64 * 3);
        }
        link_array_free(&mut a, &info);
    }
}

#[test]
fn an_array_of_strings_drops_its_strings() {
    unsafe {
        let info = string_info();
        let mut a = link_array_new(&info);
        for i in 0..50 {
            let v = s(&format!("element number {i}"));
            link_array_push(&mut a, &info, (&raw const v).cast::<u8>());
        }
        let p = link_array_get(&a, &info, 7);
        assert_eq!(as_str(&*p.cast::<LinkString>()), "element number 7");
        // Every one of the 50 heap buffers is released here; a leak shows up
        // under Miri's leak check.
        link_array_free(&mut a, &info);
    }
}
