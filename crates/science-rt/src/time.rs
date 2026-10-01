//! The three entry points the bundled `time` module is written over.
//!
//! # Not codegen support, and not in `RUNTIME`
//!
//! Like [`crate::file`]'s three, these are called by **Science source** — the
//! bundled `time` module declares them in an `unsafe extern "C"` block — and
//! never by code the compiler emits, so they are not `science-codegen`'s
//! `RUNTIME` rows. They cross the boundary as `i64` nanoseconds and nothing
//! else; every type, every unit and every calendar is the Science module's.
//!
//! # Why the standard library's clocks and not `clock_gettime`
//!
//! `stdlib-standard.md` §3.5 names `clock_gettime` with `CLOCK_REALTIME` and
//! `CLOCK_MONOTONIC` on Unix, and `GetSystemTimePreciseAsFileTime` and
//! `QueryPerformanceCounter` on Windows. `std::time::SystemTime` and
//! `std::time::Instant` *are* those calls on those platforms, and this crate
//! has no dependencies — taking `libc` for two `extern` declarations would be
//! the first. The one thing `std::time::Instant` does not give is a raw
//! reading, so the monotonic clock is reported as nanoseconds since the first
//! reading this process took: §3.3's `Monotonic` "has no epoch", and this one
//! is as good as the kernel's.

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The process's first monotonic reading, which every later one is measured
/// from.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

fn nanos_of(duration: Duration) -> i64 {
    // Saturates in the year 2262, where an `i64` of nanoseconds ends.
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}

/// A reading of the monotonic clock in nanoseconds. Never decreases within a
/// process; means nothing across two.
#[no_mangle]
pub extern "C" fn science_time_monotonic_nanos() -> i64 {
    let origin = *ORIGIN.get_or_init(Instant::now);
    nanos_of(Instant::now().duration_since(origin))
}

/// The wall clock, in nanoseconds since 1970-01-01T00:00:00Z, ignoring leap
/// seconds as POSIX does. Negative when the system clock is set before the
/// epoch.
#[no_mangle]
pub extern "C" fn science_time_unix_nanos() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(after) => nanos_of(after),
        Err(before) => -nanos_of(before.duration()),
    }
}

/// Suspend the calling thread for at least `nanos` nanoseconds. A zero or
/// negative duration returns at once.
#[no_mangle]
pub extern "C" fn science_time_sleep_nanos(nanos: i64) {
    if nanos > 0 {
        std::thread::sleep(Duration::from_nanos(nanos as u64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_monotonic_clock_does_not_go_backwards_across_a_sleep() {
        let before = science_time_monotonic_nanos();
        science_time_sleep_nanos(2_000_000);
        let after = science_time_monotonic_nanos();
        assert!(after - before >= 2_000_000, "{before} then {after}");
    }

    #[test]
    fn the_wall_clock_is_after_this_file_was_written() {
        // 2026-01-01T00:00:00Z.
        assert!(science_time_unix_nanos() > 1_767_225_600 * 1_000_000_000);
    }
}
