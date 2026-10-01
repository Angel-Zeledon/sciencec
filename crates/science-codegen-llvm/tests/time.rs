//! The bundled `time` module (`stdlib-standard.md` §3), **built, linked, run**.
//!
//! Everything here is deterministic but the two clock tests, and those assert
//! only what a clock guarantees: a monotonic reading taken after a sleep is at
//! least the sleep later, and the wall clock is past the day this file was
//! written. No upper bound — a loaded machine may oversleep by seconds.
//!
//! The calendar is checked on pinned timestamps whose civil form was computed
//! independently (Python's `datetime`, UTC): the epoch, the second before it,
//! a leap day in a year divisible by 400, a century that is not leap, and the
//! last nanosecond an I64 can hold.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("time", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `+`, `-`, `*` and unary `-` through the operator interfaces, `is` through
/// `Eq`, and `Display` choosing the largest unit with a whole part.
#[test]
fn durations_add_subtract_compare_and_display() {
    assert_eq!(
        prints(
            "duration_arithmetic",
            "use time (Duration)

def main():
    let a be Duration.from_milliseconds(1500)
    let b be Duration.from_milliseconds(250)
    print(a + b)
    print(a - b)
    print(b - a)
    print(a * 3)
    print(-a)
    print((a + b).seconds())
    print((a + b).milliseconds())
    print(a is b)
    print(a is not b)
    print(a - b is Duration.from_microseconds(1_250_000))
    print(Duration.from_seconds(90))
    print(Duration.from_nanoseconds(17))
    print(Duration.from_nanoseconds(1_250))
    print(Duration.from_nanoseconds(1_000_001))
    print(Duration.zero())
    print(Duration.from_seconds_f64(0.25))
    print(Duration.from_seconds_f64(-2.5).nanoseconds())
    print(f\"took {a + b}\")
",
        ),
        "1.75s\n1.25s\n-1.25s\n4.5s\n-1.5s\n1.75\n1750\nfalse\ntrue\ntrue\n90s\n17ns\n\
         1.25µs\n1.000001ms\n0s\n250ms\n-2500000000\ntook 1.75s\n"
    );
}

/// `elapsed` across a `sleep` is at least the sleep, read both ways: through
/// `elapsed()` and through `Monotonic`'s `-`. The wall clock is after
/// 2026-01-01T00:00:00Z.
#[test]
fn a_sleep_is_measured_by_the_monotonic_clock() {
    assert_eq!(
        prints(
            "monotonic_sleep",
            "use time (Duration, Monotonic, now, sleep)

def main():
    let start be Monotonic.now()
    sleep(Duration.from_milliseconds(20))
    let took be start.elapsed()
    print(took.nanoseconds() >= 20_000_000)
    print((Monotonic.now() - start).nanoseconds() >= took.nanoseconds())
    sleep(Duration.from_milliseconds(-5))
    print(now().unix_seconds() > 1_767_225_600)
",
        ),
        "true\ntrue\ntrue\n"
    );
}

/// Epoch seconds to UTC civil time and back, on pinned instants.
#[test]
fn instants_convert_to_utc_civil_time() {
    assert_eq!(
        prints(
            "civil",
            "use time (Civil, Duration, Instant)

def show(instant: Instant):
    let civil be instant.to_civil_utc()
    let back be civil.to_instant()
    print(f\"{civil} {back is instant}\")

def main():
    show(Instant.from_unix_seconds(0))
    show(Instant.from_unix_seconds(-1))
    show(Instant.from_unix_nanoseconds(-1))
    show(Instant.from_unix_seconds(951_825_600))
    show(Instant.from_unix_seconds(4_107_542_400))
    show(Instant.from_unix_seconds(1_709_164_800))
    show(Instant.from_unix_seconds(-5_364_662_400))
    show(Instant.from_unix_nanoseconds(9_223_372_036_854_775_807))
    show(Instant.from_unix_seconds(0) + Duration.from_milliseconds(1500))
    print(Civil.utc(2000, 2, 29, 0, 0, 0).to_instant().unix_seconds())
    print(Instant.from_unix_seconds(-1).unix_seconds())
    print(Instant.from_unix_seconds(86_399).to_civil_utc().hour)
",
        ),
        "1970-01-01T00:00:00Z true\n\
         1969-12-31T23:59:59Z true\n\
         1969-12-31T23:59:59.999999999Z true\n\
         2000-02-29T12:00:00Z true\n\
         2100-03-01T00:00:00Z true\n\
         2024-02-29T00:00:00Z true\n\
         1800-01-01T00:00:00Z true\n\
         2262-04-11T23:47:16.854775807Z true\n\
         1970-01-01T00:00:01.5Z true\n\
         951782400\n-1\n23\n"
    );
}

/// RFC 3339 in: offsets, fractions, lowercase `t` and `z`, and a refusal for
/// each malformed field, naming the byte it stopped at.
#[test]
fn rfc3339_parses_and_refuses() {
    assert_eq!(
        prints(
            "rfc3339",
            "use time (parse_rfc3339, format_rfc3339)

def show(text: &String):
    let instant, err be parse_rfc3339(text)
    if err?:
        print(err.message())
        return
    print(f\"{format_rfc3339(instant)} {instant.unix_seconds()}\")

def main():
    show(\"2000-02-29T12:00:00Z\")
    show(\"1970-01-01t00:00:00.5z\")
    show(\"2026-09-30T10:15:30.123456789123+02:00\")
    show(\"1969-12-31T23:59:59-00:30\")
    show(\"2001-02-29T00:00:00Z\")
    show(\"2000-13-01T00:00:00Z\")
    show(\"2000-01-01T00:00:60Z\")
    show(\"2000-01-01T00:00:00\")
    show(\"2000-01-01T00:00:00Zjunk\")
    show(\"2000-01-01T00:00:00.Z\")
    show(\"9999-01-01T00:00:00Z\")
    show(\"\")
",
        ),
        "2000-02-29T12:00:00Z 951825600\n\
         1970-01-01T00:00:00.5Z 0\n\
         2026-09-30T08:15:30.123456789Z 1790756130\n\
         1970-01-01T00:29:59Z 1799\n\
         expected a day that month has at byte 8\n\
         expected a month from 01 to 12 and `-` at byte 5\n\
         expected a second from 00 to 59 at byte 17\n\
         expected `Z` or an offset `±HH:MM` at byte 19\n\
         expected the end of the text at byte 20\n\
         expected a digit after `.` at byte 20\n\
         expected a time from 1677-09-21 to 2262-04-11 at byte 0\n\
         expected a four-digit year and `-` at byte 0\n"
    );
}

/// The two compiler defects the module's parser ran into, each reduced to the
/// line that found it: a `-1` tail after an `if` block was a subtraction from
/// the `if`, and `as` on a `&U8` from `Array.get` reached the backend as a
/// cast from a pointer.
#[test]
fn a_negative_tail_and_a_cast_through_a_borrow() {
    assert_eq!(
        prints(
            "negative_tail_cast",
            "def sign(x: Int) -> Int:
    if x > 0:
        return 1
    -1

def first(bytes: &Array[U8]) -> Int:
    let found be bytes.get(0)
    if found?:
        return found as Int
    -1

def main():
    print(sign(3))
    print(sign(-3))
    print(first(\"A\".bytes()))
    print(first(\"\".bytes()))
",
        ),
        "1\n-1\n65\n-1\n"
    );
}
