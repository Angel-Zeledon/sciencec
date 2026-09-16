//! `Box[T]`: one heap allocation, represented as the bare pointer to it.

mod common;

use common::*;
use link_rt::*;

#[test]
fn box_new_moves_the_value_onto_the_heap() {
    unsafe {
        let info = i64_info();
        let value = 1234i64;
        let b = link_box_new(&info, (&raw const value).cast::<u8>());
        assert!(
            !b.is_null(),
            "Box is never null: that is what makes it a niche"
        );
        assert_eq!(b as usize % info.align, 0);
        assert_eq!(*b.cast::<i64>(), 1234);
        *b.cast::<i64>() = 5678;
        assert_eq!(*b.cast::<i64>(), 5678);
        link_box_free(&info, b);
    }
}

#[test]
fn box_free_runs_drop_glue_exactly_once() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = counted_info();
        let value = Counted { id: 1 };
        let b = link_box_new(&info, (&raw const value).cast::<u8>());
        assert_eq!(drops(), 0, "boxing moves, it does not drop");
        link_box_free(&info, b);
        assert_eq!(drops(), 1);
    }
}

#[test]
fn box_of_an_over_aligned_type() {
    unsafe {
        let info = over_info();
        let value = Over { id: 99 };
        let b = link_box_new(&info, (&raw const value).cast::<u8>());
        assert_eq!(b as usize % 32, 0);
        assert_eq!(*b.cast::<Over>(), Over { id: 99 });
        link_box_free(&info, b);
    }
}

#[test]
fn box_of_a_zero_sized_type_allocates_nothing_but_is_still_non_null() {
    unsafe {
        let info = unit_info();
        let unit: [u8; 0] = [];
        let b = link_box_new(&info, unit.as_ptr());
        assert!(!b.is_null());
        link_box_free(&info, b);
    }
}

#[test]
fn box_of_a_string_owns_the_string() {
    unsafe {
        let info = string_info();
        let value = s("boxed");
        let b = link_box_new(&info, (&raw const value).cast::<u8>());
        assert_eq!(as_str(&*b.cast::<LinkString>()), "boxed");
        // Frees both the box and the String buffer it owns.
        link_box_free(&info, b);
    }
}

#[test]
fn many_boxes_are_distinct_allocations() {
    unsafe {
        let info = i64_info();
        let mut boxes = Vec::new();
        for i in 0..256i64 {
            boxes.push(link_box_new(&info, (&raw const i).cast::<u8>()));
        }
        for (i, &b) in boxes.iter().enumerate() {
            assert_eq!(*b.cast::<i64>(), i as i64);
        }
        let mut sorted = boxes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), boxes.len(), "two boxes shared an address");
        for b in boxes {
            link_box_free(&info, b);
        }
    }
}
