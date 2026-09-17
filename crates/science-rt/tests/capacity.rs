//! §1.7's *"the common case is one allocation"*, **counted**.
//!
//! # Why this file exists
//!
//! `strings-formatting-and-docs.md` §1.7 says an `f"…"` *"lowers to a builder
//! over the fragments, with the capacity pre-computed from the literal
//! fragments plus a per-type estimate for each hole, so the common case is one
//! allocation"*. That is a *performance* claim, and a performance claim that
//! nobody counts is a comment. Every other test in this crate asserts a value
//! or a layout; this one asserts a **number of calls into the system
//! allocator**, because that is the only thing §1.7 actually says.
//!
//! # How it counts
//!
//! A `#[global_allocator]` that forwards to [`System`] and increments a
//! **thread-local** counter on `alloc`, `alloc_zeroed` and `realloc`. Per
//! thread and not global, because the test harness runs each `#[test]` on its
//! own thread and allocates on others: a global counter would measure whatever
//! `libtest` happened to be doing while the measurement ran, which is a number
//! that changes between runs and is therefore worse than none.
//!
//! A `realloc` counts as one allocation event. That is the right unit for the
//! claim: `ScienceString::reserve` grows by `science_realloc`, so the thing
//! §1.7 is promising to avoid is a *growth*, and a growth that the system
//! allocator happens to satisfy in place is still a call it had to make a
//! decision in.
//!
//! The counter is off until [`allocations`] turns it on, so nothing the test
//! harness does before or after the measured region is in the number.
//!
//! # What the measured sequence is
//!
//! Exactly the call sequence `science-mir`'s `Builder::lower_fstring` emits for
//! `f"n es {n} y x es {x}"` — `tests/interpolation.rs`'s acceptance case — with
//! `n` being `42` and `x` being `0.5`. One constructor, then one
//! `science_string_push_bytes` per text run and one `science_string_push_*` per
//! hole. **It is written out here rather than driven from MIR** because this
//! crate cannot depend on the compiler, and the two halves are joined by two
//! other tests: `science-mir`'s `tests/fstring.rs` asserts the estimate this
//! file's `ESTIMATE` is, and `science-codegen-llvm`'s `tests/interpolation.rs`
//! asserts the emitted IR passes it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use science_rt::*;

thread_local! {
    /// How many allocation events this thread has made while counting.
    static COUNT: Cell<usize> = const { Cell::new(0) };
    /// Whether this thread is counting.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

/// [`System`], with a thread-local tally.
struct Counting;

/// One allocation event, if this thread asked for them to be counted.
///
/// `try_with` rather than `with` throughout: a thread-local may be accessed
/// during its own destruction, and a panic from inside the global allocator is
/// an abort with no message. Both cells are `const`-initialised, so reading one
/// allocates nothing and cannot recurse into this function.
fn tally() {
    let on = COUNTING.try_with(Cell::get).unwrap_or(false);
    if on {
        let _ = COUNT.try_with(|count| count.set(count.get() + 1));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        tally();
        // SAFETY: the caller's obligations are `System::alloc`'s, unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        tally();
        // SAFETY: as above.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above. A free is not an allocation event and is not
        // counted: §1.7 is a claim about how often the string grows.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        tally();
        // SAFETY: as above.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Run `body` and answer how many times it reached the system allocator.
fn allocations(body: impl FnOnce()) -> usize {
    COUNT.with(|count| count.set(0));
    COUNTING.with(|on| on.set(true));
    body();
    COUNTING.with(|on| on.set(false));
    COUNT.with(Cell::get)
}

/// The capacity `science-mir` computes for `f"n es {n} y x es {x}"`.
///
/// `"n es "` is five bytes and `" y x es "` is eight, an `Int` hole estimates
/// twenty and an `F64` hole twenty-four: 13 + 44. `science-mir`'s
/// `tests/fstring.rs` asserts the compiler agrees, which is the half of this
/// number that lives on the other side of the boundary.
const ESTIMATE: usize = 57;

/// Append the acceptance case's fragments to `s`, in the order MIR emits them.
///
/// # Safety
///
/// `s` must point to a live [`ScienceString`].
unsafe fn build_the_acceptance_case(s: *mut ScienceString) {
    let first = b"n es ";
    let second = b" y x es ";
    // SAFETY: the caller guarantees a live string, and both byte strings are
    // static ASCII, so they are valid UTF-8 and cannot alias its buffer.
    unsafe {
        science_string_push_bytes(s, first.as_ptr(), first.len());
        science_string_push_i64(s, 42);
        science_string_push_bytes(s, second.as_ptr(), second.len());
        science_string_push_f64(s, 0.5);
    }
}

/// The rendered text, so that the two measurements below are known to be
/// measuring a string that came out right.
const RENDERED: &str = "n es 42 y x es 0.5";

/// A `String`'s bytes, through its public fields.
///
/// `ScienceString::bytes` is `pub(crate)`, and a test is a separate crate — so
/// this is the same two lines `tests/common/mod.rs` already has, written out
/// rather than pulled in, because that module also carries a drop-counting
/// element type and a `Mutex` this file has no use for.
fn as_str(value: &ScienceString) -> &str {
    // SAFETY: a live `ScienceString` owns `len` initialised, valid UTF-8 bytes
    // at a non-null, aligned `ptr`.
    unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(value.ptr, value.len))
            .expect("a `String`'s bytes are valid UTF-8 by invariant")
    }
}

/// **The measurement, both ways, in one test** so a reader sees the pair.
///
/// The "before" number is the one §1.7's own text calls the failure —
/// *"the common case is one allocation per growth"*, which is
/// `science-mir`'s §7 item 9 — and it is measured rather than predicted,
/// because the growth schedule is `ScienceString::reserve`'s amortised
/// doubling and the number of `write_str` calls `core::fmt` makes for an `f64`
/// is not something to work out on paper.
#[test]
fn the_acceptance_case_goes_from_three_allocations_to_one() {
    let mut without = science_string_new();
    let before = allocations(|| unsafe { build_the_acceptance_case(&mut without) });
    assert_eq!(as_str(&without), RENDERED);
    // SAFETY: live, and freed exactly once.
    unsafe { science_string_free(&mut without) };

    let mut with = science_string_with_capacity(ESTIMATE);
    let after = allocations(|| unsafe { build_the_acceptance_case(&mut with) });
    assert_eq!(as_str(&with), RENDERED);
    // SAFETY: live, and freed exactly once.
    unsafe { science_string_free(&mut with) };

    assert_eq!(
        before, 3,
        "an empty accumulator grows once per fragment that does not fit; if this number moved, \
         `ScienceString::reserve`'s schedule or `core::fmt`'s chunking did"
    );
    assert_eq!(
        after, 0,
        "§1.7's \"one allocation\" — the constructor's, which is outside the measured region \
         because it is the one allocation the claim allows"
    );
}

/// The constructor's own allocation, counted, so that "one" in the test above
/// is a claim about the **whole** builder and not only about its pushes.
#[test]
fn the_whole_builder_is_one_allocation_and_the_empty_one_is_none() {
    let mut string = science_string_new();
    let total = allocations(|| {
        string = science_string_with_capacity(ESTIMATE);
        // SAFETY: `string` was just constructed by this test.
        unsafe { build_the_acceptance_case(&mut string) };
    });
    assert_eq!(as_str(&string), RENDERED);
    // SAFETY: live, freed once.
    unsafe { science_string_free(&mut string) };
    assert_eq!(total, 1, "§1.7: \"so the common case is one allocation\"");

    // And the same builder started the old way is four: the constructor's zero
    // plus three growths.
    let mut empty = science_string_new();
    let total = allocations(|| {
        empty = science_string_new();
        // SAFETY: as above.
        unsafe { build_the_acceptance_case(&mut empty) };
    });
    // SAFETY: as above.
    unsafe { science_string_free(&mut empty) };
    assert_eq!(total, 3, "`science_string_new` allocates nothing, so this is the three growths");
}

/// A capacity of zero allocates nothing, which is what makes `f""` cost what it
/// did before this entry point existed.
#[test]
fn a_zero_capacity_is_science_string_new_exactly() {
    let mut string = science_string_new();
    let total = allocations(|| string = science_string_with_capacity(0));
    assert_eq!(total, 0);
    assert_eq!(string.len, 0);
    assert_eq!(string.cap, 0);
    assert!(!string.ptr.is_null(), "the never-null invariant holds for the empty case too");
    // SAFETY: a live, empty string.
    unsafe { science_string_free(&mut string) };
}

/// The capacity is honoured exactly, and the string is still empty.
///
/// **Exactly, and not rounded up to `reserve`'s floor of eight.** A caller that
/// computed an estimate gets the estimate; the amortisation floor exists to
/// stop a string built one small piece at a time from reallocating on every
/// piece, and a caller that has already said how big the string will be is
/// exactly the caller that does not need it.
#[test]
fn the_capacity_is_the_number_asked_for() {
    for cap in [1usize, 3, 8, 57, 1024] {
        let mut string = science_string_with_capacity(cap);
        assert_eq!(string.cap, cap, "capacity {cap} was not honoured exactly");
        assert_eq!(string.len, 0);
        // SAFETY: live, freed once.
        unsafe { science_string_free(&mut string) };
    }
}

/// An estimate that is too small still produces the right string; it only costs
/// the growth it was meant to avoid.
///
/// This is the half of §1.7 that the word *"estimate"* is doing: the capacity
/// is a hint and never a bound, and a hole that renders longer than its
/// per-type estimate must not truncate, panic, or corrupt anything.
#[test]
fn an_underestimate_grows_and_is_still_correct() {
    let mut string = science_string_with_capacity(1);
    let count = allocations(|| unsafe { build_the_acceptance_case(&mut string) });
    assert_eq!(as_str(&string), RENDERED);
    // SAFETY: live, freed once.
    unsafe { science_string_free(&mut string) };
    assert!(count > 0, "a one-byte capacity cannot hold eighteen bytes without growing");
}
