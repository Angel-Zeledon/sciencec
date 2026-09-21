//! `String`: the §8 method set plus the codegen support entry points.

mod common;

use common::{as_str, free, s};
use science_rt::*;

#[test]
fn new_is_empty() {
    unsafe {
        let mut v = science_string_new();
        assert_eq!(science_string_len(&v), 0);
        assert!(science_string_is_empty(&v));
        assert!(
            !v.ptr.is_null(),
            "an empty String still has a dangling, non-null pointer"
        );
        science_string_free(&mut v);
        assert_eq!(v.len, 0);
        assert_eq!(v.cap, 0);
    }
}

#[test]
fn from_bytes_round_trips() {
    unsafe {
        let v = s("hello");
        assert_eq!(science_string_len(&v), 5);
        assert!(!science_string_is_empty(&v));
        assert_eq!(as_str(&v), "hello");
        free(v);
    }
}

#[test]
fn len_is_in_bytes_not_characters() {
    unsafe {
        // Five characters, ten bytes.
        let v = s("áéíóú");
        assert_eq!(science_string_len(&v), 10);
        free(v);
    }
}

#[test]
fn push_str_appends() {
    unsafe {
        let mut v = science_string_new();
        let w = s("world");
        let h = s("hello, ");
        science_string_push_str(&mut v, &h);
        science_string_push_str(&mut v, &w);
        assert_eq!(as_str(&v), "hello, world");
        free(h);
        free(w);
        free(v);
    }
}

#[test]
fn push_str_of_empty_into_empty_stays_empty() {
    unsafe {
        let mut v = science_string_new();
        let e = science_string_new();
        science_string_push_str(&mut v, &e);
        assert!(science_string_is_empty(&v));
        assert_eq!(as_str(&v), "");
        free(e);
        free(v);
    }
}

#[test]
fn push_str_grows_across_many_reallocations() {
    unsafe {
        let mut v = science_string_new();
        let chunk = s("0123456789");
        for _ in 0..500 {
            science_string_push_str(&mut v, &chunk);
        }
        assert_eq!(science_string_len(&v), 5000);
        assert!(v.cap >= 5000);
        let text = as_str(&v);
        assert!(text.starts_with("0123456789"));
        assert!(text.ends_with("0123456789"));
        assert_eq!(text.len(), 5000);
        assert!(text.bytes().all(|b| b.is_ascii_digit()));
        free(chunk);
        free(v);
    }
}

#[test]
fn push_str_from_itself_is_not_required_but_aliasing_two_reads_is_safe() {
    unsafe {
        let a = s("ab");
        let mut b = science_string_new();
        science_string_push_str(&mut b, &a);
        science_string_push_str(&mut b, &a);
        assert_eq!(as_str(&b), "abab");
        free(a);
        free(b);
    }
}

// --- `truncate`, which used to do something else ----------------------------
//
// **Every test in this section was rewritten, and the old ones were not
// weakened but wrong.** They pinned a `truncate` that took `&self`, returned a
// **new** `String`, and counted **characters**. `stdlib-core.md` §6.9 writes
// the signature as `def truncate(mutable self, bytes: Int)` and calls it *"the
// one `String` mutator"*, and §6.5's decision is in bytes: *"shortens to at
// most `bytes` bytes, stopping at the last character boundary at or below that
// offset"*, with the cost *"`truncate(200)` may leave 197 bytes"* spelled out.
//
// The old implementation and these old tests agreed with each other and with
// nothing else. They survived because `science-resolve`'s `builtins.rs` never
// declared the name, so no Science program could reach this entry point and
// no test above the runtime could see what it did.

/// The rule in §6.5, at the boundary it exists to protect.
///
/// `"áéíóú"` is five two-byte characters. A limit of 3 bytes lands inside the
/// second one, so the cut backs off to 2 — the last boundary at or below —
/// leaving `"á"`. A byte count that ignored boundaries would leave invalid
/// UTF-8, which is the one thing §6.5 says it must never do.
#[test]
fn truncate_backs_off_to_the_last_character_boundary() {
    unsafe {
        let mut v = s("áéíóú");
        science_string_truncate(&mut v, 3);
        assert_eq!(as_str(&v), "á");
        assert_eq!(science_string_len(&v), 2, "the boundary below three");
        free(v);
    }
}

/// A four-byte character, which is the widest back-off there is: a limit one
/// byte into it must drop all four.
#[test]
fn truncate_handles_four_byte_characters() {
    unsafe {
        let mut v = s("a\u{1F600}b");
        assert_eq!(science_string_len(&v), 6);
        science_string_truncate(&mut v, 2);
        assert_eq!(as_str(&v), "a", "two bytes lands inside the emoji");
        science_string_truncate(&mut v, 1);
        assert_eq!(as_str(&v), "a");
        free(v);
    }
}

/// **Totality, over every limit.** §6.5's whole argument is that `truncate`
/// *"cannot fail, cannot panic, and cannot corrupt"*, so the property is
/// checked at every byte offset from zero past the end rather than at a few
/// chosen ones. `as_str` would be undefined behaviour on invalid UTF-8, so
/// reading it back at each step is the assertion.
#[test]
fn truncate_never_splits_a_character_at_any_limit() {
    let text = "aá€\u{1F600}b\u{4E2D}";
    for limit in 0..=(text.len() as i64 + 3) {
        unsafe {
            let mut v = s(text);
            science_string_truncate(&mut v, limit);
            let cut = as_str(&v);
            assert!(text.starts_with(cut), "limit {limit}: {cut:?} is not a prefix");
            assert!(cut.len() <= limit.max(0) as usize, "limit {limit}: kept too much");
            // The next byte, if there is one, must be where a character
            // starts — otherwise the cut landed inside one.
            assert!(text.is_char_boundary(cut.len()), "limit {limit}: not a boundary");
            free(v);
        }
    }
}

/// A limit past the end leaves the string alone, and does **not** reallocate.
#[test]
fn truncate_beyond_the_end_changes_nothing() {
    unsafe {
        let mut v = s("hello");
        let before = v.ptr;
        science_string_truncate(&mut v, 1000);
        assert_eq!(as_str(&v), "hello");
        assert_eq!(v.ptr, before, "truncate does not reallocate");
        free(v);
    }
}

/// Zero and a negative limit both empty the string, which is the only total
/// answer for a negative one.
#[test]
fn truncate_to_zero_or_below_is_empty() {
    unsafe {
        let mut v = s("hello");
        science_string_truncate(&mut v, 0);
        assert!(science_string_is_empty(&v));
        free(v);

        let mut w = s("hello");
        science_string_truncate(&mut w, -7);
        assert!(science_string_is_empty(&w));
        free(w);
    }
}

/// **The capacity survives**, which is what makes truncating in place free.
///
/// `truncate` shortens `len` and leaves `cap`, so the bytes past the new end
/// are still owned by the same buffer and are released with it. A `truncate`
/// that shrank the allocation would have to copy, and one that forgot the
/// bytes it dropped would leak them.
#[test]
fn truncate_keeps_the_buffer_it_had() {
    unsafe {
        let mut v = s("hello");
        let before = (v.ptr, v.cap);
        science_string_truncate(&mut v, 2);
        assert_eq!(as_str(&v), "he");
        assert_eq!((v.ptr, v.cap), before, "the allocation is untouched");
        free(v);
    }
}

#[test]
fn truncate_of_empty_is_empty() {
    unsafe {
        let mut v = science_string_new();
        science_string_truncate(&mut v, 5);
        assert!(science_string_is_empty(&v));
        free(v);
    }
}

#[test]
fn starts_with() {
    unsafe {
        let v = s("hello, world");
        let cases: [(&str, bool); 6] = [
            ("", true),
            ("h", true),
            ("hello", true),
            ("hello, world", true),
            ("hello, world!", false),
            ("world", false),
        ];
        for (prefix, expected) in cases {
            let p = s(prefix);
            assert_eq!(
                science_string_starts_with(&v, &p),
                expected,
                "prefix {prefix:?}"
            );
            free(p);
        }
        free(v);
    }
}

#[test]
fn empty_starts_with_only_empty() {
    unsafe {
        let v = science_string_new();
        let e = science_string_new();
        let a = s("a");
        assert!(science_string_starts_with(&v, &e));
        assert!(!science_string_starts_with(&v, &a));
        free(a);
        free(e);
        free(v);
    }
}

#[test]
fn chars_iterates_code_points() {
    unsafe {
        let v = s("aá€\u{1F600}");
        let mut it = science_string_chars(&v);
        let mut out: u32 = 0;
        let mut seen = Vec::new();
        while science_chars_next(&mut it, &mut out) {
            seen.push(char::from_u32(out).expect("a valid code point"));
        }
        assert_eq!(seen, vec!['a', 'á', '€', '\u{1F600}']);
        free(v);
    }
}

#[test]
fn chars_of_empty_yields_nothing() {
    unsafe {
        let v = science_string_new();
        let mut it = science_string_chars(&v);
        let mut out: u32 = 0;
        assert!(!science_chars_next(&mut it, &mut out));
        free(v);
    }
}

#[test]
fn chars_of_one_character() {
    unsafe {
        let v = s("\u{1F600}");
        let mut it = science_string_chars(&v);
        let mut out: u32 = 0;
        assert!(science_chars_next(&mut it, &mut out));
        assert_eq!(out, 0x1F600);
        assert!(!science_chars_next(&mut it, &mut out));
        assert!(
            !science_chars_next(&mut it, &mut out),
            "exhausted stays exhausted"
        );
        free(v);
    }
}

#[test]
fn clone_is_independent() {
    unsafe {
        let a = s("hello");
        let mut b = science_string_clone(&a);
        assert_eq!(as_str(&b), "hello");
        assert_ne!(a.ptr, b.ptr);
        let more = s("!!");
        science_string_push_str(&mut b, &more);
        assert_eq!(as_str(&a), "hello", "the original is untouched");
        assert_eq!(as_str(&b), "hello!!");
        free(more);
        free(b);
        free(a);
    }
}

#[test]
fn clone_of_empty() {
    unsafe {
        let a = science_string_new();
        let b = science_string_clone(&a);
        assert!(science_string_is_empty(&b));
        free(b);
        free(a);
    }
}

#[test]
fn equality_and_ordering() {
    unsafe {
        let a = s("apple");
        let b = s("apple");
        let c = s("banana");
        let d = s("app");
        assert!(science_string_eq(&a, &b));
        assert!(!science_string_eq(&a, &c));
        assert!(!science_string_eq(&a, &d));
        assert_eq!(science_string_cmp(&a, &b), 0);
        assert!(science_string_cmp(&a, &c) < 0);
        assert!(science_string_cmp(&c, &a) > 0);
        assert!(science_string_cmp(&d, &a) < 0, "a prefix sorts first");
        free(d);
        free(c);
        free(b);
        free(a);
    }
}

#[test]
fn equal_strings_hash_equal() {
    unsafe {
        let a = s("a moderately long key, long enough to span several words");
        let b = science_string_clone(&a);
        let c = s("a moderately long key, long enough to span several wordt");
        assert_eq!(science_string_hash(&a), science_string_hash(&b));
        assert_ne!(
            science_string_hash(&a),
            science_string_hash(&c),
            "not required, but a hash that cannot see the last byte is useless"
        );
        free(c);
        free(b);
        free(a);
    }
}

#[test]
fn empty_strings_hash_equal() {
    unsafe {
        let a = science_string_new();
        let b = s("");
        assert_eq!(science_string_hash(&a), science_string_hash(&b));
        free(b);
        free(a);
    }
}

#[test]
fn as_ptr_points_at_the_bytes() {
    unsafe {
        let v = s("xyz");
        let p = science_string_as_ptr(&v);
        assert_eq!(*p, b'x');
        assert_eq!(*p.add(2), b'z');
        free(v);
    }
}

#[test]
fn free_is_idempotent_on_the_emptied_value() {
    unsafe {
        // Drop glue runs exactly once per value in real Science code; this only
        // asserts that `free` leaves behind a valid empty String rather than a
        // dangling one, which is what makes the second call harmless.
        let mut v = s("hello");
        science_string_free(&mut v);
        assert_eq!(v.len, 0);
        assert_eq!(v.cap, 0);
        science_string_free(&mut v);
        assert_eq!(v.len, 0);
    }
}
