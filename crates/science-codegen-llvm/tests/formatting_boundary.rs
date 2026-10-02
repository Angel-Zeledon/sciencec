//! The seven entry points that make a number printable, **run** through the
//! signatures this crate declares for them.
//!
//! # Why this file is still hand-built, now that source can reach the same
//! calls
//!
//! This note used to end *"that half arrives with `science-mir`'s
//! `ExprKind::FString` arm, and the test to write then is a source-level one
//! that replaces this file's first test"*. It arrived; the source-level test is
//! `tests/interpolation.rs`; and what it replaced is the **first test's
//! purpose** rather than this file. `lower_runtime_call` does now pick the
//! arguments for a real program, and `tests/interpolation.rs` asserts that by
//! running one.
//!
//! What no source-level program can do is **choose the values**. This file
//! passes `2^53 + 1`, `u64::MAX`, `2.5`, `0.5f32`, `true` and `'!'` through the
//! seven entry points in one call sequence, and every one of them is chosen so
//! that a width error or a wrong register file *changes the output*. An
//! ordinary program prints `42` the same way whether its argument went through
//! `i64` or `f64`. So the two files are the shape and the extremes, and this
//! one keeps the extremes.
//!
//! It does that the way `tests/exit_code.rs` does §2.3's failing row: it writes
//! a function by hand, pairs it with the real [`Lowerer::lower_c_main`], and
//! runs the result through the same emitter, the same verifier, the same
//! Decision 33 pipeline and the same linker that `build` uses.
//!
//! **The cost is stated rather than hidden**: what is hand-built here is the
//! *sequence*, so a mistake in `science-mir`'s choice of entry point is
//! invisible to this file by construction. `science-mir`'s `tests/fstring.rs`
//! is where the choice is asserted.
//!
//! # What would be silently wrong without it
//!
//! A `RUNTIME` row is seven words of transcription and every one of them is a
//! register. `science_string_push_f64(ptr, f64)` declared with an `Int` second
//! parameter passes the double's **bit pattern in an integer register**, which
//! links, verifies, runs, and prints a number in the billions instead of `2.5`.
//! `push_bool` declared as an `Int` reads seven bytes it does not own.
//! `push_u64` declared as `Int` is right on every value below `i64::MAX` and
//! wrong above it — which no small test would find. Each of those is a row in
//! the table this file exercises by value, and the values are chosen to be the
//! ones a width error changes.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::abi::AbiSignature;
use science_codegen::backend::{BlockId, Callee, Inst, LocalId, Operand, Terminator};
use science_codegen::layout::{CgTy, Triple, layout_of};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::{RUNTIME, RtAggregate, RtParam, RtRet, runtime_fn};
use science_codegen::target::{OptLevel, TargetConfig};
use science_codegen_llvm::emit::{ExtBlock, ExtBody, ExtInst};
use science_codegen_llvm::emit_and_link;
use science_codegen_llvm::lower::{Lowerer, runtime_signature};

/// The seven, by symbol, in `RUNTIME`'s order.
const PUSHES: [&str; 7] = [
    "science_string_push_bytes",
    "science_string_push_i64",
    "science_string_push_u64",
    "science_string_push_f64",
    "science_string_push_f32",
    "science_string_push_bool",
    "science_string_push_char",
];

/// Every one of them is declared, takes a `*mut ScienceString` first, and
/// returns nothing.
///
/// **A table assertion, and the cheap half of the test below.** `RUNTIME` is
/// §2.6's *"whole list"* and `tests/symbols.rs` already checks that it matches
/// `science-rt`'s `#[no_mangle]` definitions **by name**. Names are not
/// signatures: that test would pass with every one of these declared
/// `(ptr, ptr)`. This is the shape, and the test after it is the behaviour.
#[test]
fn the_seven_are_declared_with_an_accumulator_first_and_no_return() {
    for symbol in PUSHES {
        let entry = runtime_fn(symbol)
            .unwrap_or_else(|| panic!("`{symbol}` is not in `RUNTIME`, and `science-rt` exports it"));
        assert_eq!(entry.ret, RtRet::Void, "`{symbol}` returns something");
        assert_eq!(
            entry.params.first(),
            Some(&RtParam::Pointer),
            "`{symbol}` does not take the accumulator first"
        );
    }
    // And the second parameter of each is the width its name says, which is the
    // transcription a reader has to check by eye and a test can check by hand.
    let second = |symbol: &str| runtime_fn(symbol).expect("declared").params[1];
    assert_eq!(second("science_string_push_i64"), RtParam::Int);
    assert_eq!(second("science_string_push_u64"), RtParam::U64);
    assert_eq!(second("science_string_push_f64"), RtParam::F64);
    assert_eq!(second("science_string_push_f32"), RtParam::F32);
    assert_eq!(second("science_string_push_bool"), RtParam::Bool);
    assert_eq!(second("science_string_push_char"), RtParam::Char);
    assert_eq!(runtime_fn("science_string_push_bytes").expect("declared").params.len(), 3);
}

/// **The derived `sret` count, from the other side of Decision 42's line.**
///
/// `tests/runtime_abi.rs` in `science-codegen` names the members; this asserts
/// the *count* here, because the count is what changes silently when a row is
/// added carelessly. §9.2's finding was one entry point missing from that set;
/// the same mistake in the other direction — a new row that lands in it by
/// accident — is a call site that passes a return slot nobody wants.
///
/// **It read nine and it reads ten**, and the two changes that produced those
/// numbers are worth keeping side by side. Seven `format.rs` entry points were
/// added and the count did **not** move, because all seven return `()`.
/// `science_string_with_capacity` was added and it **did**, because it returns
/// `ScienceString` — three words, MEMORY on every target. Neither was decided:
/// both are what `runtime_signature` said about a signature somebody wrote.
///
/// **And it reads nine again**, because `science_string_truncate` left the
/// set: `stdlib-core.md` §6.9 makes it `(mutable self, bytes: Int)` with no
/// return, and the runtime had been returning a fresh `String` against an
/// earlier revision. Nothing caught it because `builtins.rs` never declared
/// the name. The count following is the derivation working, not a
/// re-baselining.
///
/// **And it reads eleven**, because §3.1's `Formatter` added seven entry
/// points of which two return a `ScienceFormatSpec` by value —
/// `science_formatter_spec` and `science_format_spec_default`. That record is
/// six words, so it is MEMORY on every target and the classifier said so; the
/// other five return `()`, and `science_formatter_init` returns `()` because
/// it takes an out-pointer rather than because it happened to. Same
/// derivation, third movement.
///
/// **And twelve**: `science_string_lines` returns a `ScienceLines`, three
/// words, exactly as `science_string_chars` beside it does. The four entry
/// points Gate C1 added are the two `Lines` ones and `Set`'s two; only this
/// one returns an aggregate.
///
/// **And thirteen**: `science_io_error_message` returns a `String`.
///
/// **And fourteen**: `String`'s four searches added four entry points and
/// one of them lands here. `science_string_replace` returns a fresh `String`
/// and is MEMORY for the reason every `String` return is; `ends_with`,
/// `contains` and `find` return a `Bool` — `find`'s `Int?` goes out through
/// a pointer, §5.3's convention — and the count did not move for them.
/// **Still fourteen, with the table at seventy-nine.** `science_write_error`,
/// `science_print_error` and `science_flush` return nothing, so three entries
/// joined `RUNTIME` after `String`'s four and none joined the derived set — the second assertion
/// moved and the first did not, which is the two numbers measuring different
/// things as they should.
///
/// **Still fourteen, with eighty-one rows**: `science_map_extent` and
/// `science_map_entry_at`, the two entry points `for entry in counts:` compiles
/// to, return an `Int` and a pointer. The table grew and this set did not,
/// which is the seven `format.rs` rows' lesson again: the count moves only for
/// a return that is MEMORY, and the derivation — not anybody here — decides
/// which.
///
/// **And fifteen — or seventeen, and which one depends on the host for the
/// first time.** `science_text_error_message` returns a `String`, as its
/// `IoError` twin does, and joins everywhere. `science_string_parse_int` and
/// `science_string_parse_float` return sixteen-byte pairs, which System V and
/// AArch64 hand back in two registers and Windows x64, whose register return
/// stops at eight bytes, through a hidden pointer. This test asks the host, so
/// the expected count has to as well; `science-codegen`'s `runtime_abi.rs`
/// pins the per-convention membership by name.
///
/// **Still fifteen and seventeen, with eighty-six rows**:
/// `science_string_push_io_error` and `science_string_push_text_error`, the
/// builder's pushes for the two error types, return `()` like every other
/// `science_string_push_*`. The table grew and the derived set did not.
///
/// **Still sixteen and eighteen, with ninety rows.** §2's format spec added
/// `science_formatter_init_spec`, `science_formatter_unsigned` and
/// `science_formatter_number32`, and all three return `()`: the spec goes in
/// as four scalars and comes back out only through `science_formatter_spec`,
/// which was already counted.
///
/// **And sixteen — or eighteen, with eighty-seven rows.** `science_string_from_utf8`, §6.9's
/// `String.from_bytes`, returns `(String, TextError?)`: thirty-two bytes,
/// `science_read_file`'s case, a hidden pointer on every convention.
///
/// **Still sixteen — or eighteen — with one hundred and four rows.**
/// `stdlib-core.md` §8.1's seventeen math entry points each return an `f64`
/// or an `Int`, in a register on every convention, so the table grew by
/// seventeen and this set by none.
///
/// **And nineteen — twenty-one on Windows — with one hundred and nine rows.**
/// `stdlib-core.md` §6.9's last three `String` methods added five entry
/// points and three of them return an aggregate: `science_string_trim` an
/// owned `String`, `science_string_slice` `from_bytes`'s four-word `(String,
/// TextError?)`, `science_string_split` a nine-word `Split`. All three are
/// MEMORY everywhere, so the host does not matter for them;
/// `science_split_next` returns a `Bool` and `science_split_free` nothing, and
/// the count did not move for those two.
///
/// **And twenty — twenty-two on Windows — with one hundred and ten rows.**
/// `science_read_line`, `stdlib-core.md` §4.4's `read_line()`, returns
/// `(String?, IoError?)`: `science_read_file`'s pair with a tagged `String?`
/// in front, forty bytes, a hidden pointer on every convention. `io`'s
/// `File.open`, `File.read` and `Stdin.read` are reached through an `extern`
/// block and are not rows at all.
///
/// **Still twenty — twenty-two — with one hundred and twenty-eight rows.**
/// The rest of `Array`'s Level 1 surface added fifteen `science_array_*`
/// entry points and none returns an aggregate: `replace`, the one that hands
/// an element back, writes it through a slot because its size is the
/// descriptor's and not the signature's, and the rest return `()`, a `Bool`,
/// an `Int` or `get`'s niche pointer.
///
/// **Twenty-two — twenty-four on Windows — with one hundred and thirty rows.**
/// `science_array_clone` and `science_array_clone_strings` return the cloned
/// `Array` header, three words, through a hidden pointer as `array_new` does.
///
/// **Twenty-three — twenty-five on Windows — with one hundred and thirty-one
/// rows.** `science_array_clone_with` is `science_array_clone` with a
/// per-element clone thunk beside the descriptor, and it returns the same
/// three-word header the same way.
///
/// **Still twenty-three — twenty-five — with one hundred and forty-six rows.**
/// The fifteen `science_display_*` entry points a generic `T: Display` hole
/// reaches for a prelude type each return `()`: they write into the
/// `Formatter` they are handed.
#[test]
fn the_derived_sret_set_is_twenty_three_and_twenty_five_on_windows_and_has_moved_eleven_times() {
    let triple = Triple::host().expect("a supported host");
    let indirect = RUNTIME
        .iter()
        .map(|entry| runtime_signature(triple, entry))
        .filter(|sig| sig.ret.is_sret())
        .count();
    let expected = if triple == Triple::X86_64WindowsMsvc { 25 } else { 23 };
    assert_eq!(
        indirect, expected,
        "the derived `sret` set is not the {expected} `science-codegen`'s `runtime.rs` names \
         for {triple:?}"
    );
    assert_eq!(RUNTIME.len(), 146, "§2.6: \"they are the whole list\"");
}

/// **The whole of an `f"…"` lowering, run.** A `String` is built, each of the
/// seven appends to it, and the result is printed.
///
/// This is the sequence `science-mir`'s `ExprKind::FString` arm emits, with one
/// deliberate difference: it opens with `science_string_from_bytes` on the
/// first literal chunk where the lowering opens with `science_string_new` and
/// pushes that chunk. Both build the same `String` and the second is the one a
/// program produces; this keeps the first because it is the *other* way to
/// start an accumulator and nothing else runs it.
///
/// Written by hand because no source can choose these values, and written
/// through [`runtime_signature`] and [`emit_and_link`] so that what it
/// exercises is the compiler and not a copy of it.
///
/// **Every value is chosen so that a width error changes it.**
///
/// - `9007199254740993` is `2^53 + 1`: exact as an `i64` and **not**
///   representable as an `f64`, so a signed integer that went through the float
///   entry point comes back as `9007199254740992`.
/// - `18446744073709551615` is `u64::MAX`: `-1` if the parameter is signed.
/// - `2.5` and `0.5f32` are exact at both widths, so what they catch is not
///   rounding but the **register file** — a double declared as an integer
///   parameter arrives as `4612811918334230528`.
/// - `true` catches a one-byte `_Bool` declared as anything wider only if the
///   surrounding bytes are non-zero, so the `Char` that follows it is `'!'`,
///   whose low byte is not zero.
#[test]
fn the_seven_entry_points_render_what_they_are_given() {
    let scaffold = lower("return null\n");
    let triple = Triple::host().expect("a supported host");
    let mut lowerer = Lowerer::new(triple, &scaffold.krate.defs, &scaffold.types);

    let string_layout = layout_of(triple, &RtAggregate::String.cg_ty());
    let ret_layout = layout_of(triple, &CgTy::nullable(CgTy::Interface));
    let science_main = AbiSignature::science(
        triple,
        mangle(&MonoKey::plain(&["main"])),
        ret_layout.clone(),
        vec![],
    );

    // One call, classified through the same function `build` classifies it
    // through. A `ReturnClass` invented here would be a second copy of §4.2's
    // rules with nothing comparing the two, which is §9.2's finding exactly.
    let call = |symbol: &'static str, args: Vec<Operand>, sret_slot: Option<LocalId>| ExtInst::Above(
        Inst::Call {
            dest: None,
            callee: Callee::Runtime(symbol),
            args,
            ret: runtime_signature(triple, runtime_fn(symbol).expect("an entry point")).ret,
            sret_slot,
        },
    );

    let opening = lowerer.intern_literal("n=");
    let middle = lowerer.intern_literal(" u=");

    let accumulator = LocalId(1);
    // The accumulator's address, which is every push's first argument.
    let address = science_codegen::backend::ValueId(0);
    // `_0`'s bytes are written the same way regardless of the ABI: a `null`
    // `Error?`, since this test is not about the return edge. Only *how it
    // leaves the function* differs — through the hidden pointer `ReturnSlot`
    // binds to, or as a value this function has to load and return itself —
    // and `science_main.ret.is_sret()` is the same question `lower_c_main`
    // asks about the exact same type, for the exact same reason.
    let mut insts = if science_main.ret.is_sret() {
        vec![
            ExtInst::ReturnSlot { local: LocalId(0), layout: ret_layout.clone() },
            ExtInst::Above(Inst::Store { local: LocalId(0), value: Operand::Null }),
        ]
    } else {
        vec![
            ExtInst::Above(Inst::Alloca { local: LocalId(0), layout: ret_layout.clone() }),
            ExtInst::Above(Inst::Store { local: LocalId(0), value: Operand::Null }),
        ]
    };
    insts.extend([
        ExtInst::Above(Inst::Alloca { local: accumulator, layout: string_layout }),
        // The first chunk builds the accumulator; every piece after it appends.
        call(
            "science_string_from_bytes",
            vec![
                Operand::GlobalAddr(opening.bytes_symbol.clone()),
                Operand::ConstInt(opening.len() as i128),
            ],
            Some(accumulator),
        ),
        ExtInst::LocalAddr { dest: address, local: accumulator },
        call(
            "science_string_push_i64",
            vec![Operand::Value(address), Operand::ConstInt(9_007_199_254_740_993)],
            None,
        ),
        call(
            "science_string_push_bytes",
            vec![
                Operand::Value(address),
                Operand::GlobalAddr(middle.bytes_symbol.clone()),
                Operand::ConstInt(middle.len() as i128),
            ],
            None,
        ),
        // `u64::MAX` as the `i128` `Operand::ConstInt` carries: the bit pattern
        // is what reaches `LLVMConstInt`, and reading it back as signed is the
        // error this value exists to catch.
        call(
            "science_string_push_u64",
            vec![Operand::Value(address), Operand::ConstInt(18_446_744_073_709_551_615i128)],
            None,
        ),
        call(
            "science_string_push_f64",
            vec![Operand::Value(address), Operand::ConstFloat(2.5)],
            None,
        ),
        call(
            "science_string_push_f32",
            vec![Operand::Value(address), Operand::ConstFloat(0.5)],
            None,
        ),
        call("science_string_push_bool", vec![Operand::Value(address), Operand::ConstInt(1)], None),
        call(
            "science_string_push_char",
            vec![Operand::Value(address), Operand::ConstInt(u32::from('!') as i128)],
            None,
        ),
        call("science_print", vec![Operand::Value(address)], None),
        // The free is not optional: `lower_print`'s own note is that the call
        // site owns the temporary and nothing else releases it.
        call("science_string_free", vec![Operand::Value(address)], None),
    ]);

    // On the `sret` convention the write above already left the bytes where
    // the caller reads them, and the return is `void`. On the other two, the
    // bytes have to come back as a value — read from the same local they were
    // written to, the same way `exit_code.rs`'s hand-built `main` reads its
    // own `Error?` back.
    let terminator = if science_main.ret.is_sret() {
        Terminator::Return(None)
    } else {
        let return_value = science_codegen::backend::ValueId(1);
        insts.push(ExtInst::Above(Inst::Load { dest: return_value, local: LocalId(0) }));
        Terminator::Return(Some(Operand::Value(return_value)))
    };

    let body = ExtBody {
        blocks: vec![ExtBlock { id: BlockId(0), label: "entry".to_string(), insts, terminator }],
    };

    let c_main = lowerer.lower_c_main(&science_main).expect("the emitted `main`");
    let mut module = lowerer.finish(vec![(science_main, body), c_main]);
    // **The declarations this hand-built body needs, added the way `finish`
    // would have.** `Lowerer::declare` interns one as a side effect of
    // *lowering* a call, and nothing here lowered anything — so the module came
    // back declaring only what `lower_c_main` called. The emitter catches the
    // gap rather than emitting a call to an undeclared symbol, which is how
    // this was found; what it must not do is invent a signature, so the
    // signature added here is [`runtime_signature`]'s, which is the one `build`
    // would have used.
    for symbol in ["science_string_from_bytes", "science_print", "science_string_free"]
        .into_iter()
        .chain(PUSHES)
    {
        if module.declarations.iter().any(|sig| sig.symbol == symbol) {
            continue;
        }
        module
            .declarations
            .push(runtime_signature(triple, runtime_fn(symbol).expect("an entry point")));
    }

    let dir = scratch("format", "pushes");
    let output = executable(&dir, "pushes");
    require_runtime();
    let config = TargetConfig::new(triple, OptLevel::O0);
    let built = match emit_and_link(&module, "format.science", &config, &output) {
        Ok(built) => built,
        Err(diagnostics) => panic!(
            "the build failed:\n{}",
            diagnostics
                .iter()
                .map(|d| format!("{}: {} {}", d.code, d.message, d.notes.join(" | ")))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    };
    // The float arguments are in the float register file, which is the shape a
    // wrong `RtParam` would change and which the output would also change — so
    // this is a second reading of the same fact, and the cheaper one to debug.
    assert!(
        built.ir.contains("@science_string_push_f64(ptr %0, double")
            || built.ir.contains("double 2.5"),
        "the `f64` argument is not a double:\n{}",
        built.ir
    );
    assert!(
        built.ir.contains("i1 true") || built.ir.contains("i8 1"),
        "the `bool` argument is not one byte:\n{}",
        built.ir
    );

    let ran = run(&built);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(
        ran.stdout, "n=9007199254740993 u=184467440737095516152.50.5true!\n",
        "one of the seven rendered its argument at the wrong width or in the wrong register"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
