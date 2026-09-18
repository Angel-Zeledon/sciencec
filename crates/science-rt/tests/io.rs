//! The free functions: `print`, `println`, `read_file`, `write_file`.
//!
//! `read_file` returns the pair `(String, IoError?)` and `write_file` returns
//! `IoError?`, so every test here asks the error half through `present` and
//! frees the value half on **both** paths.
//!
//! The file tests touch the real filesystem, so they are skipped under Miri,
//! which runs with host isolation on.

mod common;

use common::{as_str, free, s};
use science_rt::*;

#[cfg(not(miri))]
fn scratch(name: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push("science-rt-tests");
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir.push(format!("{}-{}", name, std::process::id()));
    dir
}

#[test]
fn print_and_println_accept_a_string() {
    // Output is asserted from a child process in `tests/process.rs`; here the
    // point is only that the pointer handling is sound, which is what Miri
    // checks.
    unsafe {
        let a = s("");
        let b = s("science-rt: print smoke test\n");
        science_write(&a);
        science_write(&b);
        science_print(&a);
        free(b);
        free(a);
    }
}

#[cfg(not(miri))]
#[test]
fn write_then_read_round_trips() {
    unsafe {
        let path = scratch("round-trip.txt");
        let p = s(path.to_str().unwrap());
        let contents = s("hello\nsecond line\ná€\u{1F600}\n");

        let written = science_write_file(&p, &contents);
        assert_eq!(written.present, SCIENCE_NULLABLE_NULL);

        let read = science_read_file(&p);
        assert_eq!(read.error.present, SCIENCE_NULLABLE_NULL);
        let text = read.value;
        assert_eq!(as_str(&text), "hello\nsecond line\ná€\u{1F600}\n");

        free(text);
        free(contents);
        free(p);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(not(miri))]
#[test]
fn write_file_overwrites() {
    unsafe {
        let path = scratch("overwrite.txt");
        let p = s(path.to_str().unwrap());
        let long = s("a very long first version of the file");
        let short = s("short");

        assert_eq!(science_write_file(&p, &long).present, SCIENCE_NULLABLE_NULL);
        assert_eq!(science_write_file(&p, &short).present, SCIENCE_NULLABLE_NULL);

        let read = science_read_file(&p);
        assert_eq!(read.error.present, SCIENCE_NULLABLE_NULL);
        let text = read.value;
        assert_eq!(as_str(&text), "short");

        free(text);
        free(short);
        free(long);
        free(p);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(not(miri))]
#[test]
fn write_then_read_an_empty_file() {
    unsafe {
        let path = scratch("empty.txt");
        let p = s(path.to_str().unwrap());
        let empty = science_string_new();
        assert_eq!(science_write_file(&p, &empty).present, SCIENCE_NULLABLE_NULL);

        let read = science_read_file(&p);
        assert_eq!(read.error.present, SCIENCE_NULLABLE_NULL);
        let text = read.value;
        assert_eq!(science_string_len(&text), 0);

        free(text);
        free(empty);
        free(p);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(not(miri))]
#[test]
fn reading_a_missing_file_is_not_found() {
    unsafe {
        let path = scratch("definitely-absent.txt");
        let _ = std::fs::remove_file(&path);
        let p = s(path.to_str().unwrap());
        let read = science_read_file(&p);
        assert_eq!(read.error.present, SCIENCE_NULLABLE_PRESENT);
        assert_eq!(read.error.error, ScienceIoError::NOT_FOUND);
        // Both halves of the pair are live, so the value half is the caller's
        // to free even though the read failed.
        free(read.value);
        free(p);
    }
}

#[cfg(not(miri))]
#[test]
fn reading_a_non_utf8_file_is_invalid_data() {
    unsafe {
        let path = scratch("not-utf8.bin");
        std::fs::write(&path, [0xFFu8, 0xFE, 0x00, 0x80]).expect("write the fixture");
        let p = s(path.to_str().unwrap());
        let read = science_read_file(&p);
        assert_eq!(read.error.present, SCIENCE_NULLABLE_PRESENT);
        assert_eq!(read.error.error, ScienceIoError::INVALID_DATA);
        free(read.value);
        free(p);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(not(miri))]
#[test]
fn writing_into_a_missing_directory_fails() {
    unsafe {
        let mut path = scratch("nowhere");
        path.push("no-such-directory");
        path.push("file.txt");
        let p = s(path.to_str().unwrap());
        let contents = s("x");
        let written = science_write_file(&p, &contents);
        assert_eq!(written.present, SCIENCE_NULLABLE_PRESENT);
        assert_eq!(written.error, ScienceIoError::NOT_FOUND);
        free(contents);
        free(p);
    }
}

#[cfg(not(miri))]
#[test]
fn reading_a_directory_is_an_error_not_a_panic() {
    unsafe {
        let dir = std::env::temp_dir();
        let p = s(dir.to_str().unwrap());
        let read = science_read_file(&p);
        assert_eq!(
            read.error.present,
            SCIENCE_NULLABLE_PRESENT,
            "a directory is not readable text"
        );
        free(read.value);
        free(p);
    }
}

#[cfg(not(miri))]
#[test]
fn a_failed_read_still_hands_back_a_usable_empty_string() {
    // The pair's value half is live on the failing path, so it must be a
    // well-formed `String` and not a hole the caller has to know to avoid.
    unsafe {
        let path = scratch("failed-read-value-half.txt");
        let _ = std::fs::remove_file(&path);
        let p = s(path.to_str().unwrap());

        let read = science_read_file(&p);
        assert_eq!(read.error.present, SCIENCE_NULLABLE_PRESENT);

        let text = read.value;
        assert_eq!(science_string_len(&text), 0);
        assert!(!text.ptr.is_null(), "an empty String is dangling, never null");
        assert_eq!(text.cap, 0, "the failing path allocates nothing");
        assert_eq!(as_str(&text), "");

        // And freeing it is legal, which is what codegen will emit.
        free(text);
        free(p);
    }
}

#[cfg(not(miri))]
#[test]
fn the_error_half_a_caller_receives_is_the_byte_image_codegen_branches_on() {
    // `tests/layout.rs` pins the byte image of an `IoError?` this file *builds*.
    // This pins the byte image of the one `science_write_file` actually
    // **returns**, on both paths, and the two are not the same claim: the
    // layout test would still pass if this entry point returned a correct
    // struct through a convention that dropped its second byte, or if some
    // future rewrite set `present` to `1` rather than to
    // `SCIENCE_NULLABLE_PRESENT`.
    //
    // It is worth a test of its own because of what codegen does with it.
    // §5.2's `present` byte *is* the `Bool` that Science's `err?` yields, so
    // the emitted code for `if err?:` is a one-byte load at offset 0 and a
    // branch — not a comparison against a constant. A `present` byte that were
    // any other non-zero value would still be "present" to every assertion in
    // this file and would still branch correctly, but it would stop being a
    // `Bool` the moment anything printed it.
    unsafe {
        let path = scratch("error-half-byte-image.txt");
        let p = s(path.to_str().unwrap());
        let contents = s("x");

        let ok = science_write_file(&p, &contents);
        assert_eq!(
            std::mem::transmute::<ScienceNullableIoError, [u8; 2]>(ok),
            [SCIENCE_NULLABLE_NULL, 0],
            "success is two zero bytes: the tag, then the payload the runtime \
             zeroes so a memory dump reads cleanly"
        );

        let mut missing = scratch("error-half-byte-image-dir");
        missing.push("no-such-directory");
        missing.push("file.txt");
        let q = s(missing.to_str().unwrap());
        let failed = science_write_file(&q, &contents);
        assert_eq!(
            std::mem::transmute::<ScienceNullableIoError, [u8; 2]>(failed),
            [SCIENCE_NULLABLE_PRESENT, ScienceIoError::NOT_FOUND.0],
            "failure is the tag then the discriminant, in that order"
        );
        assert_eq!(SCIENCE_NULLABLE_PRESENT, 1, "`present` is a `Bool`, not merely non-zero");

        free(q);
        free(contents);
        free(p);
        let _ = std::fs::remove_file(&path);
    }
}
