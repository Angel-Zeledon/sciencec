//! `Map[K, V]`: the §8 method set over the element-descriptor convention, with
//! hashing and equality supplied by the compiler as function pointers.

mod common;

use common::*;
use science_rt::*;

unsafe fn insert(map: &mut ScienceMap, info: &ScienceMapInfo, key: i64, value: i64) -> Option<i64> {
    let mut old: i64 = 0;
    let replaced = science_map_insert(
        map,
        info,
        (&raw const key).cast::<u8>(),
        (&raw const value).cast::<u8>(),
        (&raw mut old).cast::<u8>(),
    );
    if replaced {
        Some(old)
    } else {
        None
    }
}

unsafe fn get(map: &ScienceMap, info: &ScienceMapInfo, key: i64) -> Option<i64> {
    let p = science_map_get(map, info, (&raw const key).cast::<u8>());
    if p.is_null() {
        None
    } else {
        Some(*p.cast::<i64>())
    }
}

unsafe fn remove(map: &mut ScienceMap, info: &ScienceMapInfo, key: i64) -> Option<i64> {
    let mut out: i64 = 0;
    let found = science_map_remove(
        map,
        info,
        (&raw const key).cast::<u8>(),
        (&raw mut out).cast::<u8>(),
    );
    if found {
        Some(out)
    } else {
        None
    }
}

unsafe fn contains(map: &ScienceMap, info: &ScienceMapInfo, key: i64) -> bool {
    science_map_contains(map, info, (&raw const key).cast::<u8>())
}

#[test]
fn new_is_empty_and_allocates_nothing() {
    unsafe {
        let info = u64_map_info();
        let mut m = science_map_new(&info);
        assert_eq!(science_map_len(&m), 0);
        assert_eq!(m.cap, 0);
        science_map_free(&mut m, &info);
    }
}

#[test]
fn lookups_on_an_empty_map_find_nothing() {
    unsafe {
        let info = u64_map_info();
        let mut m = science_map_new(&info);
        assert_eq!(get(&m, &info, 1), None);
        assert!(!contains(&m, &info, 1));
        assert_eq!(remove(&mut m, &info, 1), None);
        assert_eq!(science_map_len(&m), 0);
        science_map_free(&mut m, &info);
    }
}

#[test]
fn single_entry_round_trip() {
    unsafe {
        let info = u64_map_info();
        let mut m = science_map_new(&info);
        assert_eq!(insert(&mut m, &info, 10, 100), None);
        assert_eq!(science_map_len(&m), 1);
        assert!(contains(&m, &info, 10));
        assert_eq!(get(&m, &info, 10), Some(100));
        assert_eq!(get(&m, &info, 11), None);
        assert_eq!(remove(&mut m, &info, 10), Some(100));
        assert_eq!(science_map_len(&m), 0);
        assert!(!contains(&m, &info, 10));
        assert_eq!(get(&m, &info, 10), None);
        science_map_free(&mut m, &info);
    }
}

#[test]
fn insert_over_an_existing_key_returns_the_old_value() {
    unsafe {
        let info = u64_map_info();
        let mut m = science_map_new(&info);
        assert_eq!(insert(&mut m, &info, 1, 10), None);
        assert_eq!(insert(&mut m, &info, 1, 20), Some(10));
        assert_eq!(science_map_len(&m), 1, "replacing does not grow the map");
        assert_eq!(get(&m, &info, 1), Some(20));
        science_map_free(&mut m, &info);
    }
}

#[test]
fn many_entries_are_all_retrievable_after_repeated_growth() {
    unsafe {
        let info = u64_map_info();
        let mut m = science_map_new(&info);
        for i in 0..2000i64 {
            assert_eq!(insert(&mut m, &info, i, i * 7), None);
        }
        assert_eq!(science_map_len(&m), 2000);
        for i in 0..2000i64 {
            assert_eq!(get(&m, &info, i), Some(i * 7), "key {i} lost across rehash");
        }
        for i in 2000..2100i64 {
            assert_eq!(get(&m, &info, i), None);
        }
        science_map_free(&mut m, &info);
    }
}

#[test]
fn every_key_colliding_still_behaves() {
    unsafe {
        // Every key hashes to zero: the probe sequence is the only thing
        // separating entries.
        let info = colliding_map_info();
        let mut m = science_map_new(&info);
        for i in 0..200i64 {
            assert_eq!(insert(&mut m, &info, i, i + 1000), None);
        }
        assert_eq!(science_map_len(&m), 200);
        for i in 0..200i64 {
            assert_eq!(get(&m, &info, i), Some(i + 1000));
        }
        assert_eq!(
            get(&m, &info, 200),
            None,
            "absent key, full collision chain"
        );
        // Remove every other entry, then check the survivors are still found
        // past the tombstones the removals left behind.
        for i in (0..200i64).step_by(2) {
            assert_eq!(remove(&mut m, &info, i), Some(i + 1000));
        }
        assert_eq!(science_map_len(&m), 100);
        for i in 0..200i64 {
            let expected = if i % 2 == 0 { None } else { Some(i + 1000) };
            assert_eq!(get(&m, &info, i), expected, "key {i} after tombstoning");
        }
        science_map_free(&mut m, &info);
    }
}

#[test]
fn removal_then_reinsertion_reuses_the_slot() {
    unsafe {
        let info = colliding_map_info();
        let mut m = science_map_new(&info);
        for round in 0..200i64 {
            assert_eq!(insert(&mut m, &info, 1, round), None);
            assert_eq!(science_map_len(&m), 1);
            assert_eq!(remove(&mut m, &info, 1), Some(round));
            assert_eq!(science_map_len(&m), 0);
        }
        // Tombstones must not accumulate without bound: 200 insert/remove
        // cycles on a one-entry map must not leave a huge table behind.
        assert!(m.cap <= 64, "tombstones grew the table to {}", m.cap);
        science_map_free(&mut m, &info);
    }
}

#[test]
fn interleaved_insert_and_remove_keeps_len_exact() {
    unsafe {
        let info = u64_map_info();
        let mut m = science_map_new(&info);
        let mut shadow = std::collections::BTreeMap::new();
        // A fixed, reproducible pseudo-random schedule.
        let mut state: u64 = 0x243F_6A88_85A3_08D3;
        for _ in 0..8000 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let key = ((state >> 33) % 300) as i64;
            if state & 1 == 0 {
                let value = (state >> 20) as i64;
                assert_eq!(insert(&mut m, &info, key, value), shadow.insert(key, value));
            } else {
                assert_eq!(remove(&mut m, &info, key), shadow.remove(&key));
            }
            assert_eq!(science_map_len(&m) as usize, shadow.len());
        }
        for (&key, &value) in &shadow {
            assert_eq!(get(&m, &info, key), Some(value));
        }
        science_map_free(&mut m, &info);
    }
}

#[test]
fn free_drops_every_remaining_key_and_value() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = ScienceMapInfo {
            key: i64_info(),
            value: counted_info(),
            hash_fn: hash_u64,
            eq_fn: eq_u64,
        };
        let mut m = science_map_new(&info);
        for id in 0..100u64 {
            let key = id as i64;
            let value = Counted { id };
            let mut old = Counted { id: 0 };
            assert!(!science_map_insert(
                &mut m,
                &info,
                (&raw const key).cast::<u8>(),
                (&raw const value).cast::<u8>(),
                (&raw mut old).cast::<u8>(),
            ));
        }
        assert_eq!(drops(), 0);
        science_map_free(&mut m, &info);
        assert_eq!(drops(), 100);
        assert_eq!(m.len, 0);
        assert_eq!(m.cap, 0);
    }
}

#[test]
fn remove_moves_the_value_out_without_dropping_it() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = ScienceMapInfo {
            key: i64_info(),
            value: counted_info(),
            hash_fn: hash_u64,
            eq_fn: eq_u64,
        };
        let mut m = science_map_new(&info);
        let key = 1i64;
        let value = Counted { id: 5 };
        let mut old = Counted { id: 0 };
        science_map_insert(
            &mut m,
            &info,
            (&raw const key).cast::<u8>(),
            (&raw const value).cast::<u8>(),
            (&raw mut old).cast::<u8>(),
        );
        let mut out = Counted { id: 0 };
        assert!(science_map_remove(
            &mut m,
            &info,
            (&raw const key).cast::<u8>(),
            (&raw mut out).cast::<u8>(),
        ));
        assert_eq!(out, Counted { id: 5 });
        assert_eq!(drops(), 0, "remove transfers the value to the caller");
        science_map_free(&mut m, &info);
        assert_eq!(drops(), 0);
    }
}

#[test]
fn replacing_a_value_drops_the_key_the_map_already_held() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = ScienceMapInfo {
            key: counted_info(),
            value: i64_info(),
            hash_fn: hash_counted_id,
            eq_fn: eq_counted_id,
        };
        let mut m = science_map_new(&info);
        let k1 = Counted { id: 1 };
        let v1 = 10i64;
        let mut old = 0i64;
        assert!(!science_map_insert(
            &mut m,
            &info,
            (&raw const k1).cast::<u8>(),
            (&raw const v1).cast::<u8>(),
            (&raw mut old).cast::<u8>(),
        ));
        assert_eq!(drops(), 0);

        // The same key again. The map takes ownership of both keys it has been
        // given, so exactly one of them must be destroyed here or it leaks. It
        // keeps the new one and drops the one it was already holding; see
        // `science_map_insert` for why that direction and not the other.
        let k2 = Counted { id: 1 };
        let v2 = 20i64;
        assert!(science_map_insert(
            &mut m,
            &info,
            (&raw const k2).cast::<u8>(),
            (&raw const v2).cast::<u8>(),
            (&raw mut old).cast::<u8>(),
        ));
        assert_eq!(old, 10);
        assert_eq!(drops(), 1, "the previously stored key is dropped");

        science_map_free(&mut m, &info);
        assert_eq!(drops(), 2, "and the surviving key on teardown");
    }
}

#[test]
fn remove_drops_the_key_but_yields_the_value() {
    let _guard = DROP_LOCK.lock().unwrap();
    unsafe {
        reset_drops();
        let info = ScienceMapInfo {
            key: counted_info(),
            value: i64_info(),
            hash_fn: hash_counted_id,
            eq_fn: eq_counted_id,
        };
        let mut m = science_map_new(&info);
        let k = Counted { id: 3 };
        let v = 30i64;
        let mut old = 0i64;
        science_map_insert(
            &mut m,
            &info,
            (&raw const k).cast::<u8>(),
            (&raw const v).cast::<u8>(),
            (&raw mut old).cast::<u8>(),
        );
        let mut out = 0i64;
        assert!(science_map_remove(
            &mut m,
            &info,
            (&raw const k).cast::<u8>(),
            (&raw mut out).cast::<u8>(),
        ));
        assert_eq!(out, 30);
        assert_eq!(drops(), 1, "the map owned the key, so removal drops it");
        science_map_free(&mut m, &info);
        assert_eq!(drops(), 1);
    }
}

#[test]
fn string_keys_work_with_the_supplied_hash_and_equality() {
    unsafe {
        let info = ScienceMapInfo {
            key: string_info(),
            value: i64_info(),
            hash_fn: hash_link_string,
            eq_fn: eq_link_string,
        };
        let mut m = science_map_new(&info);
        for i in 0..200i64 {
            let key = s(&format!("key-{i}"));
            let mut old = 0i64;
            assert!(!science_map_insert(
                &mut m,
                &info,
                (&raw const key).cast::<u8>(),
                (&raw const i).cast::<u8>(),
                (&raw mut old).cast::<u8>(),
            ));
        }
        assert_eq!(science_map_len(&m), 200);
        for i in 0..200i64 {
            let probe = s(&format!("key-{i}"));
            let p = science_map_get(&m, &info, (&raw const probe).cast::<u8>());
            assert!(!p.is_null(), "key-{i} not found");
            assert_eq!(*p.cast::<i64>(), i);
            free(probe);
        }
        let absent = s("key-999");
        assert!(science_map_get(&m, &info, (&raw const absent).cast::<u8>()).is_null());
        free(absent);

        // Removing yields the value and drops the owned key.
        let probe = s("key-7");
        let mut out = 0i64;
        assert!(science_map_remove(
            &mut m,
            &info,
            (&raw const probe).cast::<u8>(),
            (&raw mut out).cast::<u8>(),
        ));
        assert_eq!(out, 7);
        free(probe);

        science_map_free(&mut m, &info);
    }
}

#[test]
fn a_map_to_zero_sized_values_is_a_set() {
    unsafe {
        let info = ScienceMapInfo {
            key: i64_info(),
            value: unit_info(),
            hash_fn: hash_u64,
            eq_fn: eq_u64,
        };
        let mut m = science_map_new(&info);
        let unit: [u8; 0] = [];
        let mut out: [u8; 0] = [];
        for i in 0..100i64 {
            assert!(!science_map_insert(
                &mut m,
                &info,
                (&raw const i).cast::<u8>(),
                unit.as_ptr(),
                out.as_mut_ptr(),
            ));
        }
        assert_eq!(science_map_len(&m), 100);
        for i in 0..100i64 {
            assert!(science_map_contains(&m, &info, (&raw const i).cast::<u8>()));
        }
        let absent = 100i64;
        assert!(!science_map_contains(
            &m,
            &info,
            (&raw const absent).cast::<u8>()
        ));
        science_map_free(&mut m, &info);
    }
}

extern "C" fn hash_counted_id(key: *const u8) -> u64 {
    unsafe { (*key.cast::<Counted>()).id }
}

extern "C" fn eq_counted_id(a: *const u8, b: *const u8) -> bool {
    unsafe { (*a.cast::<Counted>()).id == (*b.cast::<Counted>()).id }
}

// ---------------------------------------------------------------------------
// The key-support pair the compiler can actually name.
//
// Every test above this line supplies `hash_u64`/`eq_u64` from
// `tests/common/mod.rs` — hand-written in the test crate, agreeing with nothing
// the compiler could emit a reference to. That is how `Map` stayed fully tested
// and entirely unreachable at the same time: the table was exercised through
// function pointers that exist only here, so a complete table and a compiler
// with nothing to put in `hash_fn` looked identical from this file.
//
// `science_codegen::runtime::map_key_support` now answers "which two symbols go
// into this `ScienceMapInfo`" for `String` and for eight-byte integer keys. The
// tests below drive a real `ScienceMap` through *those* symbols and nothing
// else, so that the pair the compiler will name is a pair that has been run.
// ---------------------------------------------------------------------------

/// `science_int_hash`/`science_int_eq` drive a real table, across the rehashes
/// that a hash function only affects the distribution of.
#[test]
fn the_int_key_pair_drives_a_real_table() {
    unsafe {
        let info = ScienceMapInfo {
            key: i64_info(),
            value: i64_info(),
            hash_fn: science_int_hash,
            eq_fn: science_int_eq,
        };
        let mut m = science_map_new(&info);

        for i in 0..2000i64 {
            assert_eq!(insert(&mut m, &info, i, i * 3), None);
        }
        assert_eq!(science_map_len(&m), 2000);
        for i in 0..2000i64 {
            assert_eq!(get(&m, &info, i), Some(i * 3), "key {i} lost across rehash");
            assert!(contains(&m, &info, i));
        }
        assert_eq!(get(&m, &info, 2000), None);
        assert!(!contains(&m, &info, -1));

        // Replacement and removal both go through `eq_fn`, so they are the part
        // a merely-plausible equality gets wrong.
        assert_eq!(insert(&mut m, &info, 7, 70), Some(21));
        assert_eq!(get(&m, &info, 7), Some(70));
        assert_eq!(remove(&mut m, &info, 7), Some(70));
        assert_eq!(get(&m, &info, 7), None);
        assert_eq!(science_map_len(&m), 1999);

        science_map_free(&mut m, &info);
    }
}

/// The extreme bit patterns, pinned separately from the loop above because they
/// are what a sign-extending or truncating integer hash collapses onto each
/// other, and a collapse is invisible in a test whose keys are all small and
/// positive.
#[test]
fn the_int_key_pair_handles_negative_and_extreme_keys() {
    unsafe {
        let info = ScienceMapInfo {
            key: i64_info(),
            value: i64_info(),
            hash_fn: science_int_hash,
            eq_fn: science_int_eq,
        };
        let mut m = science_map_new(&info);
        for i in -50..50i64 {
            assert_eq!(insert(&mut m, &info, i, i), None);
        }
        assert_eq!(science_map_len(&m), 100);
        for i in -50..50i64 {
            assert_eq!(get(&m, &info, i), Some(i));
        }
        assert_eq!(insert(&mut m, &info, i64::MAX, 1), None);
        assert_eq!(insert(&mut m, &info, i64::MIN, 2), None);
        assert_eq!(get(&m, &info, i64::MAX), Some(1));
        assert_eq!(get(&m, &info, i64::MIN), Some(2));
        assert_eq!(get(&m, &info, -1), Some(-1));
        assert_eq!(science_map_len(&m), 102);
        science_map_free(&mut m, &info);
    }
}

/// The one contract `ScienceMapInfo` imposes, asserted directly on the pair that
/// will be named for `Int`: **equal keys hash equal**.
///
/// Directly, and not through the table, because the table cannot detect a
/// violation — a pair that disagreed would make the tests above lose entries
/// occasionally, as a function of capacity, which is a flake and not a failure.
#[test]
fn the_int_key_pair_agrees_with_itself() {
    unsafe {
        for value in [0i64, 1, -1, 42, i64::MIN, i64::MAX, 1 << 32] {
            let (a, b) = (value, value);
            let (pa, pb) = ((&raw const a).cast::<u8>(), (&raw const b).cast::<u8>());
            assert!(science_int_eq(pa, pb), "{value} is not equal to itself");
            assert_eq!(
                science_int_hash(pa),
                science_int_hash(pb),
                "{value} does not hash equal to itself"
            );
        }
        let (x, y) = (1i64, 2i64);
        assert!(!science_int_eq(
            (&raw const x).cast::<u8>(),
            (&raw const y).cast::<u8>()
        ));
    }
}

/// `science_string_hash` and `science_string_eq` fill a `ScienceMapInfo`'s two
/// slots **with no adaptor in between**, which is the claim `map_key_support`
/// rests on for `String` keys.
///
/// Their Rust signatures say `*const ScienceString` where `ScienceHashFn` and
/// `ScienceEqFn` say `*const u8`. In the C ABI those are one pointer, so codegen
/// will take the address of `science_string_hash` and store it straight into a
/// `hash_fn` slot. The `transmute` below *is* that store, written in Rust: if
/// the two were not ABI-compatible, this test is what would crash, rather than
/// an emitted program nobody could reduce.
///
/// `tests/common/mod.rs`'s `hash_link_string`/`eq_link_string` stay what the
/// other string tests use; they are a Rust type-checking convenience, and this
/// test is the record that they are only that.
#[test]
fn the_string_key_pair_needs_no_shim() {
    unsafe {
        let info = ScienceMapInfo {
            key: string_info(),
            value: i64_info(),
            // SAFETY: `*const ScienceString` and `*const u8` are the same
            // pointer in the C ABI, which is the sentence this test runs.
            hash_fn: std::mem::transmute::<
                unsafe extern "C" fn(*const ScienceString) -> u64,
                ScienceHashFn,
            >(science_string_hash),
            eq_fn: std::mem::transmute::<
                unsafe extern "C" fn(*const ScienceString, *const ScienceString) -> bool,
                ScienceEqFn,
            >(science_string_eq),
        };
        let mut m = science_map_new(&info);

        for i in 0..200i64 {
            let key = s(&format!("key-{i}"));
            let mut old = 0i64;
            assert!(!science_map_insert(
                &mut m,
                &info,
                (&raw const key).cast::<u8>(),
                (&raw const i).cast::<u8>(),
                (&raw mut old).cast::<u8>(),
            ));
        }
        assert_eq!(science_map_len(&m), 200);

        // A freshly built key with the same bytes must find the entry: hashing
        // the contents and not the buffer is the whole point of the pair.
        for i in 0..200i64 {
            let probe = s(&format!("key-{i}"));
            let found = science_map_get(&m, &info, (&raw const probe).cast::<u8>());
            assert!(!found.is_null(), "key-{i} not found through the real pair");
            assert_eq!(*found.cast::<i64>(), i);
            free(probe);
        }

        let absent = s("key-200");
        assert!(science_map_get(&m, &info, (&raw const absent).cast::<u8>()).is_null());
        free(absent);

        science_map_free(&mut m, &info);
    }
}
