//! The free functions: `print`, `println`, `read_file`, `write_file`.
//!
//! The file tests touch the real filesystem, so they are skipped under Miri,
//! which runs with host isolation on.

mod common;

use common::{as_str, free, s};
use link_rt::*;

#[cfg(not(miri))]
fn scratch(name: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push("link-rt-tests");
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
        let b = s("link-rt: print smoke test\n");
        link_print(&a);
        link_print(&b);
        link_println(&a);
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

        let written = link_write_file(&p, &contents);
        assert_eq!(written.tag, LINK_RESULT_OK);

        let read = link_read_file(&p);
        assert_eq!(read.tag, LINK_RESULT_OK);
        let text = read.payload.ok;
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

        assert_eq!(link_write_file(&p, &long).tag, LINK_RESULT_OK);
        assert_eq!(link_write_file(&p, &short).tag, LINK_RESULT_OK);

        let read = link_read_file(&p);
        assert_eq!(read.tag, LINK_RESULT_OK);
        let text = read.payload.ok;
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
        let empty = link_string_new();
        assert_eq!(link_write_file(&p, &empty).tag, LINK_RESULT_OK);

        let read = link_read_file(&p);
        assert_eq!(read.tag, LINK_RESULT_OK);
        let text = read.payload.ok;
        assert_eq!(link_string_len(&text), 0);

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
        let read = link_read_file(&p);
        assert_eq!(read.tag, LINK_RESULT_ERR);
        assert_eq!(read.payload.err, LinkIoError::NOT_FOUND);
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
        let read = link_read_file(&p);
        assert_eq!(read.tag, LINK_RESULT_ERR);
        assert_eq!(read.payload.err, LinkIoError::INVALID_DATA);
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
        let written = link_write_file(&p, &contents);
        assert_eq!(written.tag, LINK_RESULT_ERR);
        assert_eq!(written.err, LinkIoError::NOT_FOUND);
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
        let read = link_read_file(&p);
        assert_eq!(read.tag, LINK_RESULT_ERR, "a directory is not readable text");
        free(p);
    }
}
