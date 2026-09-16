//! `String`: the §8 method set plus the codegen support entry points.

mod common;

use common::{as_str, free, s};
use link_rt::*;

#[test]
fn new_is_empty() {
    unsafe {
        let mut v = link_string_new();
        assert_eq!(link_string_len(&v), 0);
        assert!(link_string_is_empty(&v));
        assert!(!v.ptr.is_null(), "an empty String still has a dangling, non-null pointer");
        link_string_free(&mut v);
        assert_eq!(v.len, 0);
        assert_eq!(v.cap, 0);
    }
}

#[test]
fn from_bytes_round_trips() {
    unsafe {
        let v = s("hello");
        assert_eq!(link_string_len(&v), 5);
        assert!(!link_string_is_empty(&v));
        assert_eq!(as_str(&v), "hello");
        free(v);
    }
}

#[test]
fn len_is_in_bytes_not_characters() {
    unsafe {
        // Five characters, ten bytes.
        let v = s("áéíóú");
        assert_eq!(link_string_len(&v), 10);
        free(v);
    }
}

#[test]
fn push_str_appends() {
    unsafe {
        let mut v = link_string_new();
        let w = s("world");
        let h = s("hello, ");
        link_string_push_str(&mut v, &h);
        link_string_push_str(&mut v, &w);
        assert_eq!(as_str(&v), "hello, world");
        free(h);
        free(w);
        free(v);
    }
}

#[test]
fn push_str_of_empty_into_empty_stays_empty() {
    unsafe {
        let mut v = link_string_new();
        let e = link_string_new();
        link_string_push_str(&mut v, &e);
        assert!(link_string_is_empty(&v));
        assert_eq!(as_str(&v), "");
        free(e);
        free(v);
    }
}

#[test]
fn push_str_grows_across_many_reallocations() {
    unsafe {
        let mut v = link_string_new();
        let chunk = s("0123456789");
        for _ in 0..500 {
            link_string_push_str(&mut v, &chunk);
        }
        assert_eq!(link_string_len(&v), 5000);
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
        let mut b = link_string_new();
        link_string_push_str(&mut b, &a);
        link_string_push_str(&mut b, &a);
        assert_eq!(as_str(&b), "abab");
        free(a);
        free(b);
    }
}

#[test]
fn truncate_cuts_on_character_boundaries() {
    unsafe {
        // Each of these characters is two bytes.
        let v = s("áéíóú");
        let t = link_string_truncate(&v, 3);
        assert_eq!(as_str(&t), "áéí");
        assert_eq!(link_string_len(&t), 6, "three two-byte characters");
        free(t);
        free(v);
    }
}

#[test]
fn truncate_handles_four_byte_characters() {
    unsafe {
        let v = s("a\u{1F600}b\u{1F601}");
        assert_eq!(link_string_len(&v), 10);
        let t = link_string_truncate(&v, 2);
        assert_eq!(as_str(&t), "a\u{1F600}");
        assert_eq!(link_string_len(&t), 5);
        free(t);
        free(v);
    }
}

#[test]
fn truncate_never_splits_a_character_at_any_limit() {
    unsafe {
        let text = "aá€\u{1F600}b\u{4E2D}";
        let v = s(text);
        let count = text.chars().count();
        for limit in 0..=(count as i64 + 3) {
            let t = link_string_truncate(&v, limit);
            let expected: String = text.chars().take(limit as usize).collect();
            assert_eq!(as_str(&t), expected, "limit {limit}");
            free(t);
        }
        free(v);
    }
}

#[test]
fn truncate_beyond_the_end_copies_everything() {
    unsafe {
        let v = s("hello");
        let t = link_string_truncate(&v, 1000);
        assert_eq!(as_str(&t), "hello");
        free(t);
        free(v);
    }
}

#[test]
fn truncate_to_zero_is_empty() {
    unsafe {
        let v = s("hello");
        let t = link_string_truncate(&v, 0);
        assert!(link_string_is_empty(&t));
        free(t);
        free(v);
    }
}

#[test]
fn truncate_of_a_negative_limit_is_empty() {
    unsafe {
        let v = s("hello");
        let t = link_string_truncate(&v, -7);
        assert!(link_string_is_empty(&t));
        free(t);
        free(v);
    }
}

#[test]
fn truncate_returns_an_independent_copy() {
    unsafe {
        let v = s("hello");
        let mut t = link_string_truncate(&v, 5);
        assert_ne!(t.ptr, v.ptr, "truncate returns a fresh allocation");
        link_string_free(&mut t);
        // The receiver is untouched: `truncate` takes `&self`.
        assert_eq!(as_str(&v), "hello");
        free(v);
    }
}

#[test]
fn truncate_of_empty_is_empty() {
    unsafe {
        let v = link_string_new();
        let t = link_string_truncate(&v, 5);
        assert!(link_string_is_empty(&t));
        free(t);
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
            assert_eq!(link_string_starts_with(&v, &p), expected, "prefix {prefix:?}");
            free(p);
        }
        free(v);
    }
}

#[test]
fn empty_starts_with_only_empty() {
    unsafe {
        let v = link_string_new();
        let e = link_string_new();
        let a = s("a");
        assert!(link_string_starts_with(&v, &e));
        assert!(!link_string_starts_with(&v, &a));
        free(a);
        free(e);
        free(v);
    }
}

#[test]
fn chars_iterates_code_points() {
    unsafe {
        let v = s("aá€\u{1F600}");
        let mut it = link_string_chars(&v);
        let mut out: u32 = 0;
        let mut seen = Vec::new();
        while link_chars_next(&mut it, &mut out) {
            seen.push(char::from_u32(out).expect("a valid code point"));
        }
        assert_eq!(seen, vec!['a', 'á', '€', '\u{1F600}']);
        free(v);
    }
}

#[test]
fn chars_of_empty_yields_nothing() {
    unsafe {
        let v = link_string_new();
        let mut it = link_string_chars(&v);
        let mut out: u32 = 0;
        assert!(!link_chars_next(&mut it, &mut out));
        free(v);
    }
}

#[test]
fn chars_of_one_character() {
    unsafe {
        let v = s("\u{1F600}");
        let mut it = link_string_chars(&v);
        let mut out: u32 = 0;
        assert!(link_chars_next(&mut it, &mut out));
        assert_eq!(out, 0x1F600);
        assert!(!link_chars_next(&mut it, &mut out));
        assert!(!link_chars_next(&mut it, &mut out), "exhausted stays exhausted");
        free(v);
    }
}

#[test]
fn clone_is_independent() {
    unsafe {
        let a = s("hello");
        let mut b = link_string_clone(&a);
        assert_eq!(as_str(&b), "hello");
        assert_ne!(a.ptr, b.ptr);
        let more = s("!!");
        link_string_push_str(&mut b, &more);
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
        let a = link_string_new();
        let b = link_string_clone(&a);
        assert!(link_string_is_empty(&b));
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
        assert!(link_string_eq(&a, &b));
        assert!(!link_string_eq(&a, &c));
        assert!(!link_string_eq(&a, &d));
        assert_eq!(link_string_cmp(&a, &b), 0);
        assert!(link_string_cmp(&a, &c) < 0);
        assert!(link_string_cmp(&c, &a) > 0);
        assert!(link_string_cmp(&d, &a) < 0, "a prefix sorts first");
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
        let b = link_string_clone(&a);
        let c = s("a moderately long key, long enough to span several wordt");
        assert_eq!(link_string_hash(&a), link_string_hash(&b));
        assert_ne!(
            link_string_hash(&a),
            link_string_hash(&c),
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
        let a = link_string_new();
        let b = s("");
        assert_eq!(link_string_hash(&a), link_string_hash(&b));
        free(b);
        free(a);
    }
}

#[test]
fn as_ptr_points_at_the_bytes() {
    unsafe {
        let v = s("xyz");
        let p = link_string_as_ptr(&v);
        assert_eq!(*p, b'x');
        assert_eq!(*p.add(2), b'z');
        free(v);
    }
}

#[test]
fn free_is_idempotent_on_the_emptied_value() {
    unsafe {
        // Drop glue runs exactly once per value in real Link code; this only
        // asserts that `free` leaves behind a valid empty String rather than a
        // dangling one, which is what makes the second call harmless.
        let mut v = s("hello");
        link_string_free(&mut v);
        assert_eq!(v.len, 0);
        assert_eq!(v.cap, 0);
        link_string_free(&mut v);
        assert_eq!(v.len, 0);
    }
}
