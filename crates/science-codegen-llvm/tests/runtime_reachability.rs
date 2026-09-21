//! The ratchet for a defect this compiler has now shipped **six** separate
//! times: a component fully written, documented and unit-tested, with no
//! caller connecting it to the rest of the pipeline.
//!
//! `diagnostics::no_entry_point` (`SC0403`) was written, unit-tested and never
//! called for its real case. `science_codegen::mono` was 1753 lines and forty
//! tests with zero callers. `science-rt`'s file I/O was complete, with ten
//! tests and a `RUNTIME` declaration, and the gap was three hunks in this
//! crate's backend. `AbiRefusal::HalfPrecision` (`SC0431`) was a rendered
//! diagnostic on a path nothing reaches. `String.starts_with` was implemented
//! in `science-rt`, declared in [`science_codegen::runtime::RUNTIME`], and
//! missing one row in [`crate::lower::Lowerer::prelude_method`]'s table, while
//! that table's own comment claimed the prelude's `String` methods worked.
//! `DescriptorTable::intern_map` had zero callers anywhere, so `Map` was
//! unusable. Every one of the six sits on the same seam: `RUNTIME` declares a
//! symbol, and nothing between it and an emitted program calls it. The
//! `science-rt` → `RUNTIME` boundary has `tests/symbols.rs`, which pins that
//! seam exhaustively and has never caught a defect. The boundary one layer up
//! — `RUNTIME` → this crate's own lowering — had no such test, and every one
//! of the six defects above lived there. This file is that test.
//!
//! # What "reachable" means, and why it is measured rather than read
//!
//! A textual grep of `src/lower.rs` for `"science_…"` is the wrong instrument.
//! `crate::lower::Lowerer::intern_map_descriptor` reaches `science_string_hash`
//! and `science_int_hash` **by value**, through
//! [`science_codegen::runtime::map_key_support`]; the literal never appears in
//! `lower.rs` at all. `crate::lower::Lowerer::declare` is the single choke
//! point every one of those routes passes through before a call can be
//! emitted — a `prelude_method` row, a hard-coded `Callee::Runtime("…")`, a
//! `direct_release`, the `f"…"` builder's per-fragment `science_string_push_*`
//! (declared in `science-mir`, reached here as `mir::Callee::Runtime`), and a
//! descriptor's `hash_fn`/`eq_fn` all end at the same `self.declare(symbol)` —
//! and [`Lowered::declarations`] is that choke point's own record, already
//! `pub` for exactly this reason (`tests/exit_code.rs` and
//! `tests/formatting_boundary.rs` already read it).
//!
//! So **reachable** here means: compiling a real Science program through the
//! same front end `sciencec build` runs — lex, parse, resolve, check, MIR,
//! then this crate's [`crate::lower::Lowerer::lower_crate`] — and finding the
//! symbol in the resulting [`Lowered::declarations`]. That is not a claim
//! about what the language *could* express; it is a claim about what this
//! compiler, today, actually emits a `call` to. It needs no change to
//! `lower.rs` at all: [`Lowerer::new`], [`Lowerer::with_externs`],
//! [`Lowerer::lower_crate`] and [`Lowered::declarations`] were already `pub`.
//!
//! # The corpus, and why it is small
//!
//! [`CORPUS`] is not a stress test; it is the smallest set of programs whose
//! union of declared symbols is the reachable set. Each program either is, or
//! is a direct reduction of, a program an existing execution test in this
//! directory already builds and runs (`arrays.rs`, `maps.rs`, `methods.rs`,
//! `interpolation.rs`) — nothing here is a claim this crate does not already
//! make elsewhere, only a place that reads the *declarations* rather than the
//! *stdout*.
//!
//! # The allowlist, and what disqualifies an entry from it
//!
//! [`ALLOWLIST`] is not "everything the corpus above did not happen to call."
//! Every entry was checked by hand, against one of three kinds of evidence,
//! before it was allowed on the list:
//!
//! 1. **The front end refuses the call before codegen sees it.**
//!    `science-resolve`'s `builtins.rs` either declares no method of that
//!    name (`Array.as_ptr`, confirmed empirically: `SC0532`, "`Array of I64`
//!    has no method `as_ptr`") or lists the name in its own `UNWRITTEN` table
//!    with the reason a note has not settled its signature (`Array.pop`,
//!    `Array.reserve`, `String.clone`, `String.truncate`).
//! 2. **The symbol is runtime-internal**, called by `science-rt`'s own Rust
//!    from one entry point to another and never emitted by any backend:
//!    `science_alloc`, `science_realloc`, `science_dealloc`, `science_abort`,
//!    `science_array_as_ptr`, `science_string_as_ptr` — see
//!    `crates/science-codegen/src/descriptor.rs`'s note on
//!    `science_string_free` calling `science_dealloc`, and
//!    `crates/science-rt/src/{array,string}.rs` for the two `as_ptr`s, which
//!    no codegen crate ever names.
//! 3. **The method is declared and the call reaches this crate, and this
//!    crate refuses it by name, for a reason `lower.rs` or a neighbouring
//!    test already states.** This category's own worked example closed while
//!    the test was being written, which is the ratchet doing its job in the
//!    only direction that matters: `String.chars()` was declared and
//!    `science_string_chars` existed, and `Lowerer::cg_ty_at` had no arm for
//!    the prelude's `Chars` — so `science_chars_next` was out of reach too,
//!    and `for c in text.chars():` did not build. Both symbols are in the
//!    corpus now, and the two allowlist entries that named them are gone.
//!    **`science_box_free` is the same shape a third time.** It read *"`Box`'s
//!    only release path is a vtable slot this backend does not emit"*, which
//!    was true of Decision 14's boxed error — `tests/methods.rs`'s
//!    `a_boxed_error_is_reached_through_its_vtable_and_read_back` still
//!    documents that gap and still returns the box rather than dropping it —
//!    and false of §2.6's plain `Box[T]`, which was never behind a vtable at
//!    all: `Lowerer::direct_release`'s new arm frees it through
//!    `Lowerer::intern_element_descriptor`, the same descriptor builder
//!    `Array`'s and `Map`'s elements already used. `boxed_value_owning_a_string`
//!    is the corpus program and the allowlist entry is gone with it.
//!    `String`'s ordering is refused by name
//!    (`tests/methods.rs`'s `ordering_two_strings_is_refused` asserts the
//!    refusal names `science_string_cmp`). **`science_write` was the eighth
//!    instance, and it is closed the same way `panic` was.** `science_write`
//!    sat in `RUNTIME` with `science_print`'s own signature and no caller:
//!    `Lowerer::lower_call` special-cased `name == "print"` by literal string
//!    rather than by the condition *"one of §4.1's two unary output
//!    functions"*, so `write(x)` was `SC0400` even though the symbol it needed
//!    was already declared. `Lowerer::lower_print` now reads `function` —
//!    `"print"` or `"write"` — off the def it is lowering and picks
//!    `science_print`/`science_write` from that, so `write_reaches_stdout`
//!    below is the program that closes it and no allowlist entry survives.
//!    **`Array.pop` is the entry worth reading closely, because half
//!    its old reason has already stopped being true.** `§5.3`'s
//!    bool-plus-out-parameter convention it needs now exists —
//!    `Map.insert`/`Map.remove` are built on it, and both are reachable below
//!    — so the only thing still standing between a Science program and
//!    `science_array_pop` is that `Array.pop` itself is not declared.
//!    `Lowerer::owned_nullable_method`'s own doc comment says so in as many
//!    words: *"a third — `Array.pop`… is not one of them: `science-resolve`'s
//!    `builtins.rs` leaves `pop` undeclared on purpose… That reasoning is read
//!    and still stands."* `builtins.rs`'s `UNWRITTEN` table still lists it,
//!    against the same undecided signature. This entry stays on the list for
//!    that reason and not the old one.
//!
//! No entry is on the list because a program for it was merely not written
//! yet. If that were sufficient, the list would hide exactly the defect this
//! file exists to catch.
//!
//! # The reverse direction: a prelude method with no runtime symbol wired
//!
//! `String.starts_with`'s defect was RUNTIME symbol present, prelude method
//! declared, no `prelude_method` row joining them. Every symbol
//! [`ALLOWLIST`] class 3 names above was probed for exactly that shape by
//! hand — `chars`, `ends_with`, `contains`, `find`, `replace`, `trim`, `pop`,
//! `as_ptr`, `reserve`, ordering, `clone`/`owned` — and every one of them
//! either has no `RUNTIME` entry point at all (`ends_with`, `contains`,
//! `find`, `replace`, `trim` — `science-rt` exports none of them) or is
//! blocked one layer below a missing table row (`chars` by `cg_ty`, `pop` by
//! the prelude declaration). `Map.insert`/`Map.remove` were exactly this
//! shape until the commit that added [`Lowerer::owned_nullable_method`] and
//! the out-parameter machinery it needs — the table row and the convention
//! landed together, so they were never observed sitting apart with a symbol
//! waiting.
//!
//! **`panic` was the seventh instance, and it is closed.** It was declared —
//! `builtins.rs`'s free function, `recv: None` — with both `science_panic`
//! and `science_panic_bytes` already in `RUNTIME` and already reachable
//! through `assert`'s failing branch, and never special-cased in
//! `lower_call`: `panic("boom")` was `SC0400` even though the symbols it
//! needed were already declared elsewhere. The gap was the same shape as
//! `starts_with`'s — present on both sides, joined on neither — one level up:
//! a free builtin has no `Self` for [`Lowerer::prelude_method`]'s table to
//! key on, so it needed its own arm rather than a row, chosen the way
//! `science-mir`'s own `lower_assert` chooses between the same two symbols —
//! from the argument's shape, a literal or not. [`CORPUS`]'s
//! `panic_literal_message`/`panic_dynamic_message` are the program that
//! closed it; neither adds an `ALLOWLIST` entry, because the symbols were
//! never the missing half.

#![cfg(feature = "llvm")]

mod harness;

use harness::lower;
use science_codegen::layout::Triple;
use science_codegen::runtime::{RUNTIME, runtime_fn};
use science_codegen_llvm::extern_blocks;
use science_codegen_llvm::lower::Lowerer;

/// One program, and the `RUNTIME` symbols this crate's own lowering declared
/// for it.
///
/// Deliberately stops at [`Lowerer::lower_crate`] rather than going on to
/// `emit_and_link`: [`Lowered::declarations`] is filled before a single LLVM
/// call is built, so this needs no LLVM installation beyond the one that lets
/// this crate compile at all, no `science_rt.lib`, and no linker — the
/// question is only ever "did this crate's own dispatch reach a `declare`",
/// which is a fact about this crate and not about the toolchain.
fn declared_runtime_symbols(source: &str) -> Vec<&'static str> {
    let program = lower(source);
    let triple = Triple::host().expect("a supported host");
    let externs = extern_blocks(&program.krate);
    let mut lowerer = Lowerer::with_externs(
        triple,
        &program.krate.defs,
        &program.types,
        &program.decls,
        &externs,
    );
    let lowered = lowerer.lower_crate(&program.bodies, &program.instances, &program.mono).unwrap_or_else(|e| {
        panic!("a fixture in this file's own corpus failed to lower: {}", e.construct)
    });
    lowered
        .declarations
        .iter()
        .filter_map(|signature| runtime_fn(&signature.symbol))
        .map(|entry| entry.symbol)
        .collect()
}

/// The smallest set of programs whose declared symbols, unioned, are this
/// compiler's reachable set. See the module doc for why each is here and what
/// execution test it is a reduction of.
const CORPUS: &[(&str, &str)] = &[
    // `loops.rs`'s `for`, which is what makes `Chars` reachable at all:
    // `science_string_chars` builds the iterator and `science_chars_next`
    // advances it through §5.3's bool-plus-out-parameter convention. Both were
    // on the allowlist until the day `for` ran, and this is the program that
    // took them off it.
    ("a `for` over a string's characters", "for c in \"abc\".chars():\n    print(f\"[{c}]\")\n"),
    // `arrays.rs`'s literal, `get`, `push` and `get_mutably` (the row
    // `tests/arrays.rs` itself does not exercise; `get_mutably` has an
    // execution test nowhere in this crate today, so this is that call's
    // only witness): `science_array_with_capacity`, `science_array_push`,
    // `science_array_get`, `science_array_get_mut`, `science_array_len`,
    // `science_array_is_empty`, and — the literal going out of scope unused —
    // `science_array_free`.
    (
        "array_literal_and_ops",
        "def main():\n\
         \x20   let mutable xs be [10, 20, 30]\n\
         \x20   xs.push(40)\n\
         \x20   let a be xs.get(0)\n\
         \x20   let b be xs.get_mutably(1)\n\
         \x20   if a? and b?:\n\
         \x20       print(f\"{xs.length()} {xs.is_empty()} {a} {b}\")\n",
    ),
    // `Array.new()`, which the literal above does not call, and a string
    // literal, which is `science_string_from_bytes` — Decision 15's temporary,
    // and the first runtime call any `print("…")` in this language makes.
    (
        "array_new_and_string_literal",
        "def main():\n\
         \x20   let ys be Array[Int].new()\n\
         \x20   let s be \"hola\"\n\
         \x20   print(f\"{ys.length()} {s}\")\n",
    ),
    // `maps.rs`'s two keyed variants, both run to completion and both dropped
    // unused: `science_map_new`, `science_map_get`, `science_map_contains`,
    // `science_map_len`, `science_map_free`, and — through
    // `map_key_support` — `science_string_hash`/`science_string_eq` for the
    // first and `science_int_hash`/`science_int_eq` for the second.
    (
        "map_string_keyed",
        "def main():\n\
         \x20   let m be Map[String, Int].new()\n\
         \x20   let probe be \"uno\"\n\
         \x20   let found be m.get(probe)\n\
         \x20   print(f\"{m.length()} {m.contains(probe)} {found?}\")\n",
    ),
    (
        "map_int_keyed",
        "def main():\n\
         \x20   let m be Map[Int, Int].new()\n\
         \x20   print(f\"{m.length()} {m.contains(7)}\")\n",
    ),
    // `maps.rs`'s `a_map_insert_overwrite_read_and_remove_round_trip`,
    // verbatim: `Map.insert` and `Map.remove` reach `science_map_insert` and
    // `science_map_remove` through `Lowerer::owned_nullable_method`'s
    // out-parameter convention rather than through `prelude_method`'s table,
    // which is exactly why they need their own program here rather than
    // riding along with `map_string_keyed` above.
    (
        "map_insert_remove_round_trip",
        "def main():\n\
         \x20   let mutable m be Map[String, Int].new()\n\
         \x20   let first be m.insert(\"uno\", 1)\n\
         \x20   let second be m.insert(\"dos\", 2)\n\
         \x20   print(f\"n={m.length()} first={first?} second={second?}\")\n\
         \x20   let old be m.insert(\"uno\", 11)\n\
         \x20   if old?:\n\
         \x20       print(f\"overwrote, previous={old}\")\n\
         \x20   let k be \"uno\"\n\
         \x20   let got be m.get(k)\n\
         \x20   if got?:\n\
         \x20       print(f\"read={got} n={m.length()}\")\n\
         \x20   let gone be m.remove(k)\n\
         \x20   if gone?:\n\
         \x20       print(f\"removed={gone} n={m.length()}\")\n",
    ),
    // `methods.rs`'s `String.new()`/`push_str` program, plus `starts_with` —
    // the fifth prelude-method row, fixed the same day `Map` was wired.
    (
        "string_methods",
        "def main():\n\
         \x20   let mutable out be String.new()\n\
         \x20   out.push_str(\"mar\")\n\
         \x20   out.push_str(\"cado\")\n\
         \x20   let prefix be \"mar\"\n\
         \x20   print(f\"{out.starts_with(prefix)} {out.length()} {out.is_empty()}\")\n",
    ),
    // `interpolation.rs`'s `every_entry_point_the_runtime_has`, verbatim: the
    // seven `science_string_push_*` entry points, one call each, from a real
    // `f\"…\"`.
    (
        "every_push_entry_point",
        "let i be 42\n\
         let u be 18446744073709551615u64\n\
         let d be 2.5\n\
         let f be 0.5f32\n\
         let b be true\n\
         let c be '!'\n\
         let s be \"texto\"\n\
         print(f\"i={i} u={u} d={d} f={f} b={b} c={c} s={s}\")\n",
    ),
    // `assert`, at both message shapes: a literal lowers to
    // `science_panic_bytes` (the same entry point an out-of-bounds index
    // panics through) and a `String` that is not a literal lowers to
    // `science_panic` — `science-mir`'s own `lower_assert` note says so, and
    // this is the only program anywhere in this crate's tests that reaches
    // the second one.
    ("assert_literal_message", "def main():\n    assert(false, \"boom\")\n"),
    (
        "assert_dynamic_message",
        "def main():\n\
         \x20   let m be \"boom\"\n\
         \x20   assert(false, m)\n",
    ),
    // `methods.rs`'s boxed-error program: a concrete error built, moved to the
    // heap (`science_box_new`), and returned from `main` — script-mode's
    // failing edge, `science_write_error_bytes` then `science_exit`.
    (
        "boxed_error_failing_main",
        "type Boom:\n\
         \x20   code: I64\n\
         \n\
         Boom implements Error:\n\
         \x20   def message(self) -> String:\n\
         \x20       f\"boom {self.code}\"\n\
         \n\
         def fail() -> Error?:\n\
         \x20   Boom(code: 409)\n\
         \n\
         def main() -> Error?:\n\
         \x20   fail()\n",
    ),
    // §2.6's plain `Box[T]`, not Decision 14's boxed error above: `Box.new`
    // (`science_box_new`) and the binding going out of scope unused
    // (`science_box_free`). `Holder` owns a `String`, so the free also runs
    // the descriptor's `drop_fn` and reaches `science_string_free` — the same
    // shape `tests/methods.rs`'s
    // `a_boxed_value_owning_a_string_is_built_and_freed_ten_thousand_times`
    // runs, kept here in its reduced form because this file reads
    // declarations and not stdout.
    (
        "boxed_value_owning_a_string",
        "type Holder:\n\
         \x20   label: String\n\
         \n\
         def main():\n\
         \x20   let h be Box.new(Holder(label: \"owned\"))\n\
         \x20   print(\"built\")\n",
    ),
    // `**` in both of its forms, which are two different entry points and
    // not one generic. §7.3's Decision 37 forbids `llvm.pow` — LLVM
    // constant-folds a recognised `pow` against the *build host's* libm,
    // which is a reproducibility hazard — so a float power is
    // `science_libm_pow`, a plain symbol the optimiser does not recognise.
    // An integer power has no LLVM instruction at all and Decision 8 forbids
    // the inlined loop, so it is `science_ipow_i64`.
    //
    // Both in one program because a corpus entry per symbol would say the
    // same thing twice, and `**` is one operator.
    (
        "both_powers",
        "def main():\n\
         \x20   print(2 ** 10)\n\
         \x20   print(f\"{2.0 ** 0.5}\")\n",
    ),
    // `read_file`/`write_file`, in the one syntax the corpus uses for a
    // pair-returning call — `let text, io_err be read_file(path)` —
    // `examples/09_absence_and_failure.science`'s own spelling, which sidesteps
    // a drop of the raw `(String, IoError?)` pair this crate cannot build glue
    // for.
    (
        "read_and_write_file",
        "def main():\n\
         \x20   let e be write_file(\"/tmp/science_runtime_reachability_probe.txt\", \"hi\")\n\
         \x20   let text, io_err be read_file(\"/tmp/science_runtime_reachability_probe.txt\")\n\
         \x20   print(f\"{e?} {io_err?} {text}\")\n",
    ),
    // `panic`, called directly rather than reached through `assert`'s failing
    // branch: the module doc above used to read *"`panic("boom")` is
    // `SC0400` today although `assert`'s `science_panic`/`science_panic_bytes`
    // both run"* — true when it was written, and false since `lower_call`
    // gained an arm for the free builtin itself. Both symbols were already
    // reachable through `assert_literal_message`/`assert_dynamic_message`
    // above, so this pair does not close an `ALLOWLIST` entry — `panic` was
    // never on the list, because the symbols it needs were never missing,
    // only the caller was. What this closes is the doc's own claim.
    ("panic_literal_message", "def main():\n    panic(\"boom\")\n"),
    (
        "panic_dynamic_message",
        "def main():\n\
         \x20   let m be \"boom\"\n\
         \x20   panic(m)\n",
    ),
    // `write`, `science_write`'s only caller anywhere in this crate's tests
    // before this program: a literal (built the same way `print("…")` builds
    // one) and a rendered integer, so both of `lower_print`'s non-`String`
    // and literal arms run for `function == "write"` and not only for
    // `"print"`.
    (
        "write_reaches_stdout",
        "def main():\n\
         \x20   write(\"no newline here: \")\n\
         \x20   write(42)\n",
    ),
];

/// Every symbol in [`RUNTIME`] this compiler's own lowering, run over
/// [`CORPUS`], declares a call to.
fn reachable() -> std::collections::BTreeSet<&'static str> {
    CORPUS.iter().flat_map(|(_, source)| declared_runtime_symbols(source)).collect()
}

/// The symbols nothing in [`CORPUS`] reaches, each with the reason it is not
/// a defect. See the module doc's three classes of evidence.
const ALLOWLIST: &[(&str, &str)] = &[
    (
        "science_array_reserve",
        "`Array.reserve` is not declared: `builtins.rs`'s `UNWRITTEN` table lists it against \
         `collections-and-chains.md` §5.3, whose signature nothing has transcribed yet",
    ),
    (
        "science_array_as_ptr",
        "no method of this name is declared on `Array` anywhere; confirmed empirically \
         (`SC0532`, \"`Array[I64]` has no method `as_ptr`\"). Runtime-internal, defined in \
         `science-rt/src/array.rs` and named by no codegen crate",
    ),
    (
        "science_array_pop",
        "`Array.pop` is not declared: `builtins.rs`'s `UNWRITTEN` table holds it back because \
         `stdlib-core.md` §3.2 names it with no signature, and the two candidate signatures \
         disagree about whether it returns the element. The bool-plus-out-parameter convention \
         it also needed no longer blocks it — `Map.insert`/`Map.remove` are built on that \
         convention and are reachable below — so the prelude declaration is the only thing left; \
         `Lowerer::owned_nullable_method`'s own doc comment says the reasoning was re-read after \
         that convention landed and still stands",
    ),
    ("science_alloc", "runtime-internal: called from `science-rt`'s own Rust, never by codegen"),
    ("science_realloc", "runtime-internal, the same as `science_alloc`"),
    (
        "science_dealloc",
        "runtime-internal: `crates/science-codegen/src/descriptor.rs` documents \
         `science_string_free` calling it, from inside `science-rt`, never from a backend",
    ),
    (
        "science_abort",
        "runtime-internal: `science-rt`'s own allocation-failure path, never emitted by codegen",
    ),
    (
        "science_string_clone",
        "`String.clone`/`.owned()` are not declared: `Clone` is one of the prelude's \
         methodless interfaces (`builtins.rs`'s `IMPLEMENTS` documents the decision), so the \
         method name is not written down anywhere a call could resolve to",
    ),
    (
        "science_string_as_ptr",
        "no method of this name is declared on `String`. Runtime-internal, defined in \
         `science-rt/src/string.rs` and named by no codegen crate",
    ),
    (
        "science_string_truncate",
        "`String.truncate` is not declared: `builtins.rs`'s `UNWRITTEN` table holds it back \
         because §6.9's signature contradicts eighteen corpus call sites that read it as \
         `-> String`",
    ),
    (
        "science_string_cmp",
        "`String`'s ordering is refused by name rather than lowered: `tests/methods.rs`'s \
         `ordering_two_strings_is_refused` asserts the refusal's message names this exact \
         symbol and cites the missing `Ord` implementation",
    ),
];

/// **The ratchet.** Every symbol `RUNTIME` declares is either reached by a
/// program in [`CORPUS`], or named in [`ALLOWLIST`] with a reason — and never
/// both.
///
/// The three assertions are the three ways this can go wrong, each one a
/// version of the defect the module doc opens with:
///
/// - A symbol in neither set is the defect itself, caught: something `RUNTIME`
///   declares that nothing in this compiler's own tests has ever shown reaches
///   a call.
/// - A symbol in both sets is a stale allowlist entry: someone wired it up —
///   which is progress — and left the excuse in place, which would make the
///   next unwired symbol invisible beside it.
/// - An allowlist entry naming something that is not in `RUNTIME` at all is a
///   typo that would otherwise silently cover for whichever real symbol was
///   meant.
#[test]
fn every_runtime_symbol_is_reachable_or_named_on_the_allowlist() {
    let declared: Vec<&str> = RUNTIME.iter().map(|f| f.symbol).collect();

    let mut allow_names: Vec<&str> = ALLOWLIST.iter().map(|(symbol, _)| *symbol).collect();
    let allow_count = allow_names.len();
    allow_names.sort_unstable();
    allow_names.dedup();
    assert_eq!(allow_names.len(), allow_count, "a symbol is on `ALLOWLIST` twice");

    for symbol in &allow_names {
        assert!(
            declared.contains(symbol),
            "`{symbol}` is on `ALLOWLIST` and is not in `RUNTIME`: a stale or misspelled entry \
             that is hiding nothing, because there is nothing by that name to hide"
        );
    }

    let reached = reachable();

    let stale: Vec<&str> =
        allow_names.iter().copied().filter(|symbol| reached.contains(symbol)).collect();
    assert!(
        stale.is_empty(),
        "{} symbol(s) are both reachable and on `ALLOWLIST`: {stale:?}\n\
         Someone wired this up. Remove the entry — leaving it in place is exactly what would \
         hide the next symbol that stops being called.",
        stale.len()
    );

    let uncovered: Vec<&str> = declared
        .iter()
        .copied()
        .filter(|symbol| !reached.contains(symbol) && !allow_names.contains(symbol))
        .collect();
    assert!(
        uncovered.is_empty(),
        "{} `RUNTIME` symbol(s) are neither reached by any program in `CORPUS` nor named on \
         `ALLOWLIST` with a reason: {uncovered:?}\n\
         This is the shape of every one of the six defects this file's module doc opens with — \
         a component `RUNTIME` declares with nothing between it and an emitted program calling \
         it. Either a program that reaches it belongs in `CORPUS`, or it belongs on \
         `ALLOWLIST` with a reason as specific as the ones already there.",
        uncovered.len()
    );
}

/// [`RUNTIME`]'s own count, read here rather than assumed, so a failure in the
/// test above names a number that still means something if the table grows
/// between now and whenever this is read.
#[test]
fn the_partition_accounts_for_every_symbol_runtime_declares() {
    let declared: std::collections::BTreeSet<&str> = RUNTIME.iter().map(|f| f.symbol).collect();
    let reached = reachable();
    let allowed: std::collections::BTreeSet<&str> =
        ALLOWLIST.iter().map(|(symbol, _)| *symbol).collect();
    let union: std::collections::BTreeSet<&str> = reached.union(&allowed).copied().collect();
    let declared_sorted: Vec<&str> = declared.into_iter().collect();
    let union_sorted: Vec<&str> = union.into_iter().collect();
    assert_eq!(
        declared_sorted, union_sorted,
        "`RUNTIME`'s symbols and (reachable ∪ allowlisted) disagree, which the test above \
         should already have failed on with a more specific message"
    );
}
