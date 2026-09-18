//! Past §10's stage 3: a `choice` and a `match`, a second function with
//! parameters, and a record — each as a program that is built, **run**, and
//! asked what it printed and what status it exited with.
//!
//! # Why every test here runs the program
//!
//! §10's discipline, and this crate's §3 is the standing evidence for it: of
//! the thirteen things reading could not have established, three were **silent
//! miscompiles that verified, linked and ran** — a `main` that never wrote its
//! return value, a null test that loaded a fat pointer, and a store eight bytes
//! wide into a one-byte slot. Every construct this file adds has the same
//! shape available to it. A record whose second field is written at the first
//! field's offset is a program that verifies. A `switch` whose case values come
//! from the wrong `choice` is a program that verifies. A parameter read from
//! the wrong position is a program that verifies. So no test here stops at the
//! IR: the IR assertions below pin *shape* — that a discriminant is one byte,
//! that a sixteen-byte record comes back through `sret` on Windows — and every
//! one of them sits beside a test that ran the program and read its bytes.
//!
//! # How a program shows its answer, which is still one byte at a time
//!
//! There is no string interpolation in this language and no integer-to-string
//! entry point in the runtime, so a computed value is shown by **branching on
//! it** and writing a byte with `putchar`. `tests/stage_two_and_three.rs`
//! established the shape and its module note is the argument; this file reuses
//! it, and where a test has several answers to distinguish it writes several
//! bytes and asserts the whole string. A byte per fact makes a partial failure
//! legible: `"YNY"` says which of the three was wrong.
//!
//! # What this file found
//!
//! Recorded at the tests that found them, and summarised in the crate's §3:
//!
//! - **A `match` arm is tested one at a time and `otherwise` chains**, so a
//!   two-variant `choice` is two `switch`es and not one. `science-mir` says so
//!   — *"arms are tested in order, no decision tree"* — and the consequence for
//!   a backend is that the *second* switch's discriminant is a **second** read
//!   of the same local, into a **second** untyped temporary. A backend that
//!   remembered one discriminant slot per body would branch the second arm on
//!   the first arm's value.
//! - **MIR's discriminant temporary is typed `Ty::ERROR` on purpose**, so the
//!   slot for it cannot be reserved from the local's declaration and has to be
//!   invented when the read is lowered. See [`crate::lower`]'s
//!   `lower_discriminant`.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// An `extern` block declaring `putchar`, which every program that has to show
/// a value uses. Identical to `tests/stage_two_and_three.rs`'s, and duplicated
/// rather than shared because the two files are two stages and a constant
/// moved into `harness` would be a third place to look.
const PUTCHAR: &str = "unsafe extern \"C\" library \"c\":\n    def putchar(c: I32) -> I32\n\n";

/// Build and run one program, and give back what it printed.
fn output(name: &str, source: &str) -> harness::Ran {
    let dir = scratch("past3", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// A program whose answer is a string of bytes, written by the C library, and
/// which must exit 0 with nothing on stderr.
fn bytes(name: &str, source: &str) -> String {
    let ran = output(name, source);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The IR of a program that built, for the assertions that pin shape.
fn ir(name: &str, source: &str) -> String {
    let dir = scratch("past3", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

/// Why a program was refused, as `SC0400`'s text.
fn refusal(name: &str, source: &str) -> String {
    let dir = scratch("past3", name);
    let output = executable(&dir, name);
    let diagnostics = lower(source)
        .try_build(&output, OptLevel::O0)
        .err()
        .unwrap_or_else(|| panic!("`{name}` built, and this test exists because it must not"));
    assert!(!output.is_file(), "a refused build left an executable behind");
    let _ = std::fs::remove_dir_all(&dir);
    diagnostics
        .iter()
        .map(|diagnostic| {
            let notes: String = diagnostic.notes.join(" ");
            format!("{} {notes}", diagnostic.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `putchar` of one byte, as a statement.
fn put(byte: u8) -> String {
    format!("    let r{byte} be unsafe: putchar({byte})\n")
}

// --- a `choice` and a `match` ---------------------------------------------

/// **The program that could not be built.** A `match` over a two-variant
/// payload-free `choice`, in both directions.
///
/// This is the sentence the last pass left: *"the smallest program that cannot
/// be built is a `match` over a two-variant payload-free `choice` — `cg_ty` has
/// no arm for a choice, so §3.3's tagged layout is never computed and the
/// switch terminator is refused before it is reached."* Both halves are now
/// there and this is what proves it: the arm taken decides the byte, so a
/// discriminant written at the wrong value, read at the wrong width, or matched
/// against the wrong case constant prints the other letter.
///
/// **Both directions, because one is half a test.** A `switch` whose arms are
/// all wired to the first block gives `AA`, and a program that only ever
/// constructs `Red` cannot tell that from a correct one.
#[test]
fn a_match_over_a_payload_free_choice_takes_the_arm_the_value_selects() {
    let source = |variant: &str| {
        format!(
            "{PUTCHAR}choice Colour:\n    Red\n    Green\n\n\
             def letter(c: Colour) -> I32:\n    match c:\n        Red: 82\n        Green: 71\n\n\
             let r be unsafe: putchar(letter({variant}))\n"
        )
    };
    assert_eq!(bytes("choice-red", &source("Red")), "R");
    assert_eq!(bytes("choice-green", &source("Green")), "G");
}

/// Every arm of a four-variant `choice`, in one program, in one run.
///
/// **Four rather than two, because two cannot distinguish a `switch` from an
/// `if`.** A two-variant `match` lowers to something a single comparison would
/// also get right; the third and fourth arms are what make the case values
/// load-bearing, and a discriminant numbered from one instead of from zero
/// prints `BCD?` instead of `ABCD`.
#[test]
fn every_arm_of_a_four_variant_choice_is_reachable_and_distinct() {
    let source = format!(
        "{PUTCHAR}choice Suit:\n    Clubs\n    Diamonds\n    Hearts\n    Spades\n\n\
         def letter(s: Suit) -> I32:\n    match s:\n        Clubs: 65\n        Diamonds: 66\n\
         \x20       Hearts: 67\n        Spades: 68\n\n\
         let a be unsafe: putchar(letter(Clubs))\n\
         let b be unsafe: putchar(letter(Diamonds))\n\
         let c be unsafe: putchar(letter(Hearts))\n\
         let d be unsafe: putchar(letter(Spades))\n"
    );
    assert_eq!(bytes("choice-four", &source), "ABCD");
}

/// Decision 18's discriminant is a `u8` up to 256 variants, and that is what is
/// in the IR.
///
/// **A shape assertion, and it stands beside a behaviour one rather than in
/// place of it.** The width matters because `LLVMConstInt` builds each case
/// value against the switched value's type: an `i64` case constant on an `i8`
/// switch is a verifier failure, and an `i8` discriminant loaded as an `i64`
/// reads seven bytes that belong to whatever is next on the stack — which
/// verifies, and which at `-O0` on a slot of its own would usually still answer
/// correctly. That is the kind of thing that comes back on a different machine.
#[test]
fn a_small_choices_discriminant_is_one_byte_wide() {
    let text = ir(
        "choice-width",
        &format!(
            "{PUTCHAR}choice Colour:\n    Red\n    Green\n\n\
             def letter(c: Colour) -> I32:\n    match c:\n        Red: 82\n        Green: 71\n\n\
             let r be unsafe: putchar(letter(Red))\n"
        ),
    );
    assert!(
        text.contains("switch i8 "),
        "the discriminant is not switched on as an `i8`:\n{text}"
    );
    assert!(
        !text.contains("switch i64 "),
        "a discriminant widened to `i64`, which reads seven bytes it does not own:\n{text}"
    );
}

/// A `choice` whose variants carry payloads: the payload reads back what was
/// constructed, and only on the arm that selected it.
///
/// **The two shapes `Lowerer::choice_ty` makes are both here.** `Circle(I32)`
/// has a one-element payload, which that function deliberately does **not**
/// wrap in a struct — so MIR's `TupleField { index: 0 }` off the downcast is
/// the identity — and `Rect(I32, I32)` has a two-element one, which is a
/// struct, so the same projection is a field lookup at an offset. A backend
/// that handled only one of the two would compile one of these and misread the
/// other.
#[test]
fn a_variants_payload_reads_back_what_was_constructed() {
    let source = format!(
        "{PUTCHAR}choice Shape:\n    Dot\n    Circle(I32)\n    Rect(I32, I32)\n\n\
         def first(s: Shape) -> I32:\n    match s:\n        Dot: 46\n        Circle(r): r\n\
         \x20       Rect(w, h): w\n\n\
         def second(s: Shape) -> I32:\n    match s:\n        Dot: 46\n        Circle(r): r\n\
         \x20       Rect(w, h): h\n\n\
         let a be unsafe: putchar(first(Dot))\n\
         let b be unsafe: putchar(first(Circle(67)))\n\
         let c be unsafe: putchar(first(Rect(88, 89)))\n\
         let d be unsafe: putchar(second(Rect(88, 89)))\n"
    );
    assert_eq!(bytes("choice-payload", &source), ".CXY");
}

/// A payload wide enough that the union's alignment moves the payload offset,
/// read back through the projection that has to know it.
///
/// **§3.3's `payload_offset` is the number under test.** *"The payload union
/// begins at the next offset that is a multiple of the union's alignment"* — so
/// a `u8` discriminant in front of an `F64` payload puts the payload at 8, not
/// at 1. A backend that used 1 would read a value straddling the tag, which is
/// a different number and not a crash.
#[test]
fn a_payload_starts_at_the_offset_its_alignment_requires() {
    let source = format!(
        "{PUTCHAR}choice Measure:\n    Unknown\n    Exact(F64)\n\n\
         def is_one(m: Measure) -> Bool:\n    match m:\n        Unknown: false\n\
         \x20       Exact(v): v is 1.0\n\n\
         if is_one(Exact(1.0)):\n{}else:\n{}\
         if is_one(Exact(2.0)):\n{}else:\n{}\
         if is_one(Unknown):\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("choice-align", &source), "YNN");
}

// --- a second function, with parameters -----------------------------------

/// **The second thing the last pass measured.** A function that takes arguments
/// is called, and its answer decides the byte.
///
/// `Operand::Param` is the repair and it is above the line now:
/// `science_codegen::backend::Operand` grew the variant, and `emit`'s
/// `BodyState::params` — populated and `#[allow(dead_code)]` since stage 1 — is
/// what it reads.
#[test]
fn a_function_with_parameters_is_called_and_its_answer_decides_the_byte() {
    let source = format!(
        "{PUTCHAR}def add(a: Int, b: Int) -> Int:\n    a + b\n\n\
         if add(2, 3) is 5:\n{}else:\n{}\
         if add(-1, 1) is 0:\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("params-add", &source), "YY");
}

/// **Argument order, which is the one thing a two-argument test of a
/// commutative operator cannot check.**
///
/// `add(a, b)` gives the same answer whichever way round the parameters are
/// read, so a backend that reversed them would pass the test above. Subtraction
/// does not, and three parameters of three different widths make the position
/// of each one observable: a parameter read from the wrong ABI position is a
/// value of the wrong width as well as the wrong number.
#[test]
fn parameters_arrive_in_order_and_at_their_own_widths() {
    let source = format!(
        "{PUTCHAR}def pick(a: I32, b: I8, c: Int) -> Bool:\n    a is 1000 and b is -5i8 and c is 77\n\n\
         if pick(1000, -5i8, 77):\n{}else:\n{}\
         if pick(77, -5i8, 1000):\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("params-order", &source), "YN");
}

/// More parameters than either calling convention has argument registers.
///
/// **Windows x64 has four and System V has six**, so a six-argument call puts
/// two on the stack on one target and none on the other, and an eight-argument
/// call spills on both. LLVM assigns the registers, which is exactly why this
/// is worth running rather than reading: the thing being tested is that the
/// *positions* this crate hands LLVM are the source order, and a shift by one
/// shows up only once the shift crosses into the stack.
#[test]
fn eight_parameters_arrive_in_order_across_the_register_boundary() {
    let params: Vec<String> = (0..8).map(|i| format!("p{i}: Int")).collect();
    let body: Vec<String> = (0..8).map(|i| format!("p{i} is {i}")).collect();
    let call: Vec<String> = (0..8).map(|i| i.to_string()).collect();
    let shuffled: Vec<String> =
        (0..8).map(|i| if i == 7 { "6".to_string() } else { i.to_string() }).collect();
    let source = format!(
        "{PUTCHAR}def ordered({}) -> Bool:\n    {}\n\n\
         if ordered({}):\n{}else:\n{}\
         if ordered({}):\n{}else:\n{}",
        params.join(", "),
        body.join(" and "),
        call.join(", "),
        put(89),
        put(78),
        shuffled.join(", "),
        put(89),
        put(78),
    );
    assert_eq!(bytes("params-eight", &source), "YN");
}

/// A `Bool` parameter, which §3.1 gives two forms of.
///
/// *"`Bool` is `i1` in registers and `i8` in memory"*, and a parameter is the
/// one place both meet: LLVM passes the declared type, this crate declares the
/// memory form, and the entry block stores it into an `i8` slot. A backend that
/// declared `i1` and stored into `i8` would need a widen that is not there, and
/// one that declared `i8` and stored `i1` is the store-width bug this crate has
/// already found once.
#[test]
fn a_bool_parameter_survives_the_round_trip_through_its_slot() {
    let source = format!(
        "{PUTCHAR}def flip(b: Bool) -> Bool:\n    not b\n\n\
         if flip(false):\n{}else:\n{}\
         if flip(true):\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("params-bool", &source), "YN");
}

/// A float parameter and a float return, which travel in a different register
/// file from everything above.
#[test]
fn a_float_parameter_and_return_use_the_other_register_file() {
    let source = format!(
        "{PUTCHAR}def half(x: F64) -> F64:\n    x * 0.5\n\n\
         if half(3.0) is 1.5:\n{}else:\n{}",
        put(89),
        put(78),
    );
    assert_eq!(bytes("params-float", &source), "Y");
}

/// A function that calls itself: the call graph has a cycle and the emission
/// order is Decision 4's.
#[test]
fn a_recursive_function_terminates_with_the_right_answer() {
    let source = format!(
        "{PUTCHAR}def factorial(n: Int) -> Int:\n    if n is 0:\n        return 1\n    \
         n * factorial(n - 1)\n\n\
         if factorial(5) is 120:\n{}else:\n{}",
        put(89),
        put(78),
    );
    assert_eq!(bytes("params-recursive", &source), "Y");
}

/// A function returning `()`, whose call has a destination with no slot.
#[test]
fn a_unit_returning_function_is_called_for_its_effect() {
    let source =
        format!("{PUTCHAR}def shout():\n{}\nshout()\nshout()\n", put(33));
    assert_eq!(bytes("params-unit", &source), "!!");
}

/// **A function the program declares and never calls is not emitted.**
///
/// [`Lowerer::lower_crate`]'s reachability walk, as a fact rather than a
/// sentence. The generic function below is one this backend cannot lower —
/// nothing has monomorphised it, so its parameter's type is a `TyKind::Param`
/// with no layout — and the program builds anyway, because `main` does not
/// reach it. The cost is in that function's own note: an error in an uncalled
/// function is not reported by `sciencec build`.
#[test]
fn an_uncalled_function_this_backend_cannot_lower_does_not_stop_the_build() {
    let source = format!(
        "{PUTCHAR}def identity of T(value: T) -> T:\n    value\n\n\
         def reached(n: I32) -> I32:\n    n\n\n\
         let r be unsafe: putchar(reached(75))\n"
    );
    assert_eq!(bytes("params-unreached", &source), "K");
}

/// And calling it is refused, by name.
#[test]
fn calling_a_generic_function_is_refused_and_says_what_is_missing() {
    let text = refusal(
        "params-generic",
        &format!(
            "{PUTCHAR}def identity of T(value: T) -> T:\n    value\n\n\
             let r be unsafe: putchar(identity(75))\n"
        ),
    );
    assert!(
        text.contains("monomorphis"),
        "the refusal does not name the missing phase:\n{text}"
    );
}

// --- a record -------------------------------------------------------------

/// **The third thing the last pass measured.** A record's fields are written
/// and read back, each one distinct.
///
/// Three fields rather than one, because one field is at offset 0 and a backend
/// that ignored the offset entirely would pass. The values are three different
/// bytes, so a field read from the wrong offset prints the wrong letter rather
/// than failing.
#[test]
fn a_records_fields_read_back_what_was_written() {
    let source = format!(
        "{PUTCHAR}type Triple:\n    a: I32\n    b: I32\n    c: I32\n\n\
         let t be Triple(a: 88, b: 89, c: 90)\n\
         let p be unsafe: putchar(t.a)\n\
         let q be unsafe: putchar(t.b)\n\
         let s be unsafe: putchar(t.c)\n"
    );
    assert_eq!(bytes("record-fields", &source), "XYZ");
}

/// **Decision 17's padding, which is the offset a backend is most likely to get
/// wrong.**
///
/// §3.2's own worked example is `{ u8, u64, u8 }`: 24 bytes under the C rule
/// and 16 under Rust's, with the second field at **8** and the third at
/// **16**. A backend that packed the fields would put them at 1 and 9 — which
/// verifies, links, runs, and reads the second field as seven bytes of the
/// first and one of its own. The `U8` fields carry values that are visible as
/// bytes so that a shifted read is a wrong letter rather than a crash.
#[test]
fn a_padded_record_puts_its_fields_where_decision_17_says() {
    let source = format!(
        "{PUTCHAR}type Mixed:\n    a: U8\n    b: U64\n    c: U8\n\n\
         let m be Mixed(a: 80u8, b: 1152921504606846976u64, c: 81u8)\n\
         if m.a is 80u8:\n{}else:\n{}\
         if m.b is 1152921504606846976u64:\n{}else:\n{}\
         if m.c is 81u8:\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("record-padding", &source), "YYY");
}

/// **A record literal's fields are stored where the *declaration* puts them,
/// whatever order the literal named them in.**
///
/// `examples/03_structs.science` builds the same `Doc` two ways — `Doc(title:
/// …, body: …)` and `Doc(body: …, title: …)` — and calls them the same value.
/// MIR carries the fields in the order they were written, so a backend that
/// took MIR's order would lay the two out differently: two layouts for one
/// type, which is a wrong answer the moment one is passed to the other's
/// reader. Here the two spellings are read by the same accessor.
#[test]
fn a_record_literal_is_laid_out_by_declaration_order_and_not_by_the_literals() {
    let source = format!(
        "{PUTCHAR}type Pair:\n    first: I32\n    second: I32\n\n\
         def first_of(p: Pair) -> I32:\n    p.first\n\n\
         let forwards be Pair(first: 70, second: 83)\n\
         let backwards be Pair(second: 83, first: 70)\n\
         let a be unsafe: putchar(first_of(forwards))\n\
         let b be unsafe: putchar(first_of(backwards))\n"
    );
    assert_eq!(bytes("record-order", &source), "FF");
}

/// A record nested inside a record, read through two projections.
#[test]
fn a_nested_records_field_is_reached_through_two_offsets() {
    let source = format!(
        "{PUTCHAR}type Inner:\n    lo: I32\n    hi: I32\n\n\
         type Outer:\n    tag: I32\n    inner: Inner\n\n\
         let o be Outer(tag: 84, inner: Inner(lo: 76, hi: 77))\n\
         let a be unsafe: putchar(o.tag)\n\
         let b be unsafe: putchar(o.inner.lo)\n\
         let c be unsafe: putchar(o.inner.hi)\n"
    );
    assert_eq!(bytes("record-nested", &source), "TLM");
}

/// **A record parameter is MEMORY rather than a register, and that is a
/// different binding.**
///
/// Decision 22: *"every aggregate argument is passed by pointer to a
/// caller-owned slot"*, so the callee's local for it **is** the caller's slot.
/// `ExtInst::ParamSlot` is what binds it, and Decision 8's `alloca` would
/// instead make a private copy that the caller's value never reaches — which is
/// `ExtInst::ReturnSlot`'s bug on the argument side, and which would read
/// whatever the stack held.
///
/// The record is twelve bytes, which is over Windows x64's 1/2/4/8 and under
/// System V's sixteen — a size the three conventions disagree about, which is
/// why the argument rule being Science's own and not the platform's matters.
#[test]
fn a_record_passed_by_value_arrives_with_its_fields_intact() {
    let source = format!(
        "{PUTCHAR}type Three:\n    a: I32\n    b: I32\n    c: I32\n\n\
         def middle(t: Three) -> I32:\n    t.b\n\n\
         def last(t: Three) -> I32:\n    t.c\n\n\
         let a be unsafe: putchar(middle(Three(a: 65, b: 66, c: 67)))\n\
         let b be unsafe: putchar(last(Three(a: 65, b: 66, c: 67)))\n"
    );
    assert_eq!(bytes("record-param", &source), "BC");
}

/// **A record returned by value, at the size where the three conventions
/// disagree.**
///
/// §4.1's worked example is sixteen bytes: register-passed on System V and
/// AAPCS64, **by hidden reference on Windows**, because sixteen is not one of
/// 1, 2, 4 or 8. `science_codegen::abi` decides which, and this is the program
/// that runs the answer. A four-byte record is in registers everywhere and is
/// here as the other side of the same test.
#[test]
fn a_record_returned_by_value_survives_whichever_class_it_got() {
    let source = format!(
        "{PUTCHAR}type Wide:\n    a: F64\n    b: F64\n\n\
         type Narrow:\n    n: I32\n\n\
         def wide() -> Wide:\n    Wide(a: 1.0, b: 2.0)\n\n\
         def narrow() -> Narrow:\n    Narrow(n: 90)\n\n\
         let w be wide()\n\
         if w.a is 1.0 and w.b is 2.0:\n{}else:\n{}\
         let n be narrow()\n\
         let z be unsafe: putchar(n.n)\n",
        put(89),
        put(78),
    );
    assert_eq!(bytes("record-return", &source), "YZ");
}

/// The sixteen-byte record really does come back through `sret` on this
/// machine's convention, and the four-byte one does not.
///
/// **A shape assertion whose value is that it names the convention.** The test
/// above would pass either way — a correct `sret` and a correct register return
/// both produce the right bytes — so this is what says *which* one was chosen,
/// and it is written per-target for `abi.rs`'s reason: a loop over the three
/// conventions asserting one answer was that module's first version and it was
/// wrong.
#[test]
fn the_return_class_in_the_ir_is_the_one_this_target_calls_for() {
    let text = ir(
        "record-sret",
        &format!(
            "{PUTCHAR}type Wide:\n    a: F64\n    b: F64\n\n\
             def wide() -> Wide:\n    Wide(a: 1.0, b: 2.0)\n\n\
             let w be wide()\n\
             if w.a is 1.0:\n{}else:\n{}",
            put(89),
            put(78),
        ),
    );
    let wide_is_indirect = matches!(
        science_codegen::layout::Triple::host().map(|triple| triple.c_abi()),
        Some(science_codegen::layout::CAbi::Win64)
    );
    if wide_is_indirect {
        assert!(
            text.contains("sret"),
            "a sixteen-byte record does not come back by hidden pointer on Windows x64, where \
             sixteen is not one of 1, 2, 4 or 8:\n{text}"
        );
    } else {
        assert!(
            !text.contains("define void @_S4wide(ptr"),
            "a sixteen-byte record came back by hidden pointer where the convention returns it \
             in registers:\n{text}"
        );
    }
}

// --- drops, and the two answers a drop can have ---------------------------

/// **A drop that has to run nothing is a `br`.**
///
/// MIR emits a `Drop` terminator for the scrutinee of every `match` in the
/// language, because `science-mir`'s `needs_drop` answers `true` for **every**
/// `choice` — *"§4's true where it cannot tell"*. So this was never only about
/// `cg_ty` having an arm: a `match` over a payload-free `choice` needed the
/// `Drop` terminator lowered before any of it could run.
///
/// The IR assertion is the negative one, and it is the one worth making: a
/// backend that answered a drop with a call would put a `science_string_free`
/// in a program that has no string in it, which frees a stack slot.
#[test]
fn a_drop_of_something_that_owns_nothing_emits_no_call() {
    let text = ir(
        "drop-inert",
        &format!(
            "{PUTCHAR}choice Colour:\n    Red\n    Green\n\n\
             type P:\n    x: I32\n\n\
             def letter(c: Colour) -> I32:\n    match c:\n        Red: 82\n        Green: 71\n\n\
             let p be P(x: 1)\n\
             let r be unsafe: putchar(letter(Red))\n"
        ),
    );
    assert!(
        !text.contains("science_string_free"),
        "a value that owns nothing was released by a call:\n{text}"
    );
    assert!(
        !text.contains("science_array_free") && !text.contains("science_map_free"),
        "a value that owns nothing was released by a call:\n{text}"
    );
}

/// **A drop that has to run something runs `science_string_free`.**
///
/// Decision 12's `DropGlue::RuntimeCall`, and `lower`'s §1 already stated the
/// rule for the temporary a `print` makes: *"a `String` does not need [glue] —
/// the temporary is freed by a direct `science_string_free` at the site that
/// made it"*. A bound `String` is that same call at the site that drops it, and
/// the sentence in that note — *"a bound string literal leaks until
/// `TerminatorKind::Drop` does"* — is what this deletes.
///
/// **The run is what says it is not a double free** and the IR assertion is
/// what says it happened at all, because a leak is invisible to both a
/// terminal and a harness. Neither alone is the test.
#[test]
fn a_bound_string_is_freed_where_mir_drops_it() {
    let source = format!(
        "{PUTCHAR}let held be \"a string this program binds and never prints\"\n\
         let r be unsafe: putchar(75)\n"
    );
    assert_eq!(bytes("drop-string", &source), "K");
    let text = ir("drop-string-ir", &source);
    assert!(
        text.contains("@science_string_free("),
        "the bound string is never released:\n{text}"
    );
}

// --- Decision 6's nullable, both representations --------------------------

/// §5.5's `?`, on a tagged `T?`, in both directions.
///
/// **`256` is the adversarial value and the reason is the tag's position.**
/// Decision 18 puts the discriminant at offset 0, so a widening that wrote the
/// payload there instead would make the tag the value's low byte — and for any
/// small number that byte is non-zero, which reads back as *present* and gives
/// the right answer for the wrong reason. `256`'s low byte is zero, so the same
/// mistake reads back as `null`. `42` is here beside it as the ordinary case.
#[test]
fn a_tagged_nullable_answers_its_presence_test_both_ways() {
    let source = format!(
        "{PUTCHAR}let a: Int? be 256\nlet b: Int? be null\nlet c: Int? be 42\n\
         if a?:\n{}else:\n{}if b?:\n{}else:\n{}if c?:\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("nullable-int", &source), "YNY");
}

/// The same value made present after it was null, which is the only shape that
/// exercises `null` and `Widen` into **one** slot.
///
/// A widening that wrote the payload and left the tag alone would answer `Y`
/// here too, because the tag was `null`'s zero and the payload is at an offset;
/// what catches it is that the tag must *change*. A `null` that did not write
/// the tag is caught by the first half.
#[test]
fn a_nullable_that_is_assigned_becomes_present() {
    let source = format!(
        "{PUTCHAR}let mutable n: Int? be null\nif n?:\n{}else:\n{}n be 7\nif n?:\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("nullable-mut", &source), "NY");
}

/// `Bool?` is two bytes and a record's nullable is the record plus a tag, and
/// both are the tagged representation at a different alignment.
///
/// **§3.4's "where niches stop", as programs.** *"`Bool?` is two bytes"* — a
/// `Bool` is `i8` in memory with 254 spare values and F0 declines to use them —
/// so the payload is at offset 1, which is the only offset in the language that
/// is not a multiple of the tag's own width. And `false` is the adversarial
/// payload for the same reason `256` is above: a payload written over the tag
/// would read back as `null`.
#[test]
fn a_nullable_at_each_alignment_answers_correctly() {
    let source = format!(
        "{PUTCHAR}type P:\n    x: I32\n    y: I32\n\n\
         let a: Bool? be false\nlet b: Bool? be null\n\
         let c: P? be P(x: 1, y: 2)\nlet d: P? be null\n\
         if a?:\n{}else:\n{}if b?:\n{}else:\n{}if c?:\n{}else:\n{}if d?:\n{}else:\n{}",
        put(89),
        put(78),
        put(89),
        put(78),
        put(89),
        put(78),
        put(89),
        put(78),
    );
    assert_eq!(bytes("nullable-align", &source), "YNYN");
}

// --- what is still refused ------------------------------------------------

/// The refusals that remain, each naming its own construct.
///
/// **Not a list of everything unimplemented — a list of what a program can
/// reach.** Each of these is one line of Science that this backend meets and
/// declines, and the assertion is on the *word* the message uses, because
/// §11's contract for `SC0400` is that it *"names the feature"* and a refusal
/// that says "internal error" is the failure this discipline exists to avoid.
#[test]
fn what_is_refused_names_itself() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "refuse-index",
            "let xs be [1, 2, 3]\nlet a be xs[0]\nprint(\"x\")\n",
            "",
        ),
        (
            // A `choice` whose payload owns something, which is the half of
            // Decision 12's glue a record no longer needs: dropping one means
            // switching on the discriminant and releasing only the active
            // variant, which is one block per arm where `intern_drop_glue`
            // builds one block. A record that owns a `String` is not in this
            // list any more — `tests/methods.rs` runs one.
            "refuse-owning-drop",
            "choice C:\n    A\n    B(String)\n\nlet c be A\nprint(\"x\")\n",
            "is not a record",
        ),
        // **A tuple and a cast used to be here** and are not: both build now,
        // and `tests/casts.rs` and the tuple section above are the programs
        // that run them. What is left of a cast's refusal is the pairs §5.1
        // does not define, which is that file's `what_no_cast_may_do`.
        // **`refuse-method` used to be here and is not, and it is the largest
        // thing this list has lost.** `Doc has: def get(self) -> Int` builds,
        // links and runs; `tests/methods.rs` is twenty-four programs of it, and
        // crate §3 finding 24 is why the refusal it replaced was false about
        // every program that reached it. What is left of a method's refusal —
        // an `interface`'s default body, and a call through `any I` — lives in
        // that file, because it is about methods rather than about this file's
        // boundary.
        (
            "refuse-closure",
            "def sink(f: (Int) -> Int) -> Int:\n    1\n\n\
             let n be sink(item giving item)\nprint(\"x\")\n",
            "closure",
        ),
    ];
    for (name, source, word) in cases {
        let text = refusal(name, source);
        assert!(
            text.contains("SC0400") || !text.is_empty(),
            "`{name}` was refused with nothing to read:\n{text}"
        );
        if !word.is_empty() {
            assert!(
                text.contains(word),
                "`{name}`'s refusal does not contain `{word}`:\n{text}"
            );
        }
    }
}

/// Integer `/` and `%` build now, and the guard that let them is the whole of
/// why.
///
/// **This test asserted the refusal and its own sentence is what changed.** It
/// read *"still refused, and still for the reason that is not effort … no
/// caller guards it, so a bare `sdiv` is undefined at zero rather than a
/// trap"*. `IntOp::SDiv`'s note — *"division by zero is a panic the caller has
/// already guarded, not a trap the backend inserts"* — is unchanged and is now
/// satisfied: `science-mir`'s `division_check` is the caller, emitting the
/// comparison and the diverging `science_panic_bytes` in front of the division,
/// where basic blocks are made. Decision 5 is why it is emitted there and not
/// here.
///
/// **Both failing inputs are exercised, because there are two.** `Int.min / -1`
/// is immediate UB for `sdiv` and `srem` exactly as the zero is, and it is
/// `#DE`/`SIGFPE` on x86-64 — a guard that caught only the zero would leave a
/// program that traps at `-O0` and has its branch deleted at `-O2`.
///
/// **The divisor is computed and not written.** `6 / 0` is a constant the
/// optimiser folds, so a fixture that wrote the zero would prove the guard fires
/// at compile time and nothing about the emitted branch. `a - a` reaches `-O2`
/// as a value LLVM has to test.
#[test]
fn integer_division_is_still_refused_for_the_reason_that_is_not_effort() {
    assert_eq!(bytes("div-ok", "print(f\"{7 / 2} {7 % 2} {-9 / 2}\")\n"), "3 1 -4\n");

    let zero = output("div-zero", "let a be 7\nlet b be a - a\nprint(f\"{a / b}\")\n");
    assert_ne!(zero.status, Some(0), "a division by zero exited cleanly");
    assert!(
        zero.stderr.contains("divide by zero"),
        "the panic does not say which guard fired: {}",
        zero.stderr
    );

    let overflow = output(
        "div-overflow",
        "let a be -9223372036854775807 - 1\nlet b be 0 - 1\nprint(f\"{a / b}\")\n",
    );
    assert_ne!(overflow.status, Some(0), "`Int.min / -1` exited cleanly");
    assert!(
        overflow.stderr.contains("overflow"),
        "the panic does not say which guard fired: {}",
        overflow.stderr
    );
}

// --- a tuple --------------------------------------------------------------
//
// **The construct the last pass measured as the boundary**, and reaching it
// turned up two facts about the phases above this one that no note records.
// They are stated here rather than at each test, because both of them shape
// how every fixture below is written:
//
// 1. **`t.0` does not parse.** There is no tuple-index expression in the
//    grammar at all — `expected an identifier, found a number literal` — so
//    `mir::Projection::TupleField` is reachable from a `match` pattern and from
//    a `choice` payload and from nothing the author writes with a dot. Every
//    fixture here therefore reads its tuple by destructuring it, which is the
//    only spelling the language has.
// 2. **An unannotated tuple literal's elements are `TyKind::Error`.**
//    `let t be (1, 2)` checks clean and has type `(<error>, <error>)`, because
//    `science-types`' `ExprKind::Tuple` arm resolves each element's type as it
//    synthesises the node and an integer literal's is still an inference
//    variable then. `let t: (Int, Int) be (1, 2)` and `let t be (1i64, 2i64)`
//    are both fine. [`the_unannotated_tuple_literal_is_a_front_end_hole`] is
//    that, as a refusal.

/// **The program that could not be built, once it is given a type.**
///
/// The last pass measured the smallest unbuildable program as `let t be (1, 2)`
/// and located it at `cg_ty`'s missing `TyKind::Tuple` arm. The arm was
/// necessary and — unlike the `choice` before it, which needed the arm *and* a
/// `Drop` of its scrutinee — it was nearly sufficient: a tuple of scalars owns
/// nothing, so `drop_runs_something` already answered `false` and MIR's drop of
/// the binding was already elaborated away. What stood behind it was not a
/// codegen hole at all but the typing gap in the note above.
#[test]
fn a_tuple_is_built_and_its_elements_read_back() {
    let source = "let t: (Int, Int) be (1, 2)\nmatch t:\n\x20   (a, b):\n\x20       print(f\"{a} {b}\")\n";
    assert_eq!(bytes("tuple", source), "1 2\n");
}

/// **Mixed widths, which is where an offset computed by counting rather than by
/// asking would go wrong.**
///
/// `(I8, F64, Bool, I32)` has padding in it on every target: a byte, seven
/// bytes of padding, eight of `F64`, a byte of `Bool`, three more of padding
/// and four of `I32`. A tuple laid out as though its elements were adjacent
/// reads the `F64` out of the middle of nothing, and the values are chosen so
/// that it cannot come back looking right — `-7` is not a byte `2.5` shares.
///
/// `science-codegen`'s `layout_of` computes those offsets and was not changed:
/// this asserts that the tuple arm hands it the elements in the order the
/// author wrote them, which is Decision 17's *"no field reordering, ever"*
/// applied to a type whose fields have numbers instead of names.
#[test]
fn a_tuple_of_mixed_widths_is_laid_out_by_the_c_rule() {
    let source = "let t: (I8, F64, Bool, I32) be (-7i8, 2.5, true, 1000i32)\n\
                  match t:\n\x20   (a, b, c, d):\n\x20       print(f\"{a} {b} {c} {d}\")\n";
    assert_eq!(bytes("tuple-mixed", source), "-7 2.5 true 1000\n");
}

/// **A tuple with a `String` in it is refused, and the refusal is the drop and
/// not the layout.**
///
/// `drop_runs_something` walks a tuple element by element, so a tuple holding a
/// `String` owns something and its `TerminatorKind::Drop` needs Decision 12's
/// emitted glue. **A record now gets one** — `Lowerer::intern_drop_glue` emits
/// it, and `tests/methods.rs` runs a program that owns a `String` and drops it
/// — and a tuple still does not, for a reason the message now names: glue is
/// interned by the symbol its type's *definition* mangles to, and a tuple has
/// no definition to mangle. The *layout* is fine, as the test above this one
/// shows; it is the destructor that is missing, and the message names the type
/// rather than saying "a tuple", which is the difference between a refusal a
/// reader can act on and one that sends them to the wrong file.
#[test]
fn a_tuple_that_owns_something_is_refused_for_the_drop_and_not_the_layout() {
    let text = refusal("tuple-owning", "let s be \"hola\"\nlet t be (s, s)\nprint(\"x\")\n");
    assert!(
        text.contains("drop glue") && text.contains("(String, String)"),
        "a tuple holding a `String` should be refused for its drop, by name:\n{text}"
    );
}

/// **A tuple nested in a record and a record nested in a tuple**, so the arm is
/// reached from both directions of `cg_ty`'s recursion.
///
/// The nesting is what would break if the tuple arm produced a `CgTy` the
/// layout engine treated differently from a record's: a `Struct` inside a
/// `Struct` is one alignment computation and the answer has to be the same
/// whichever of the two is outermost.
#[test]
fn a_tuple_nests_with_a_record_in_both_directions() {
    let source = "type P:\n\x20   x: Int\n\x20   y: Int\n\n\
                  let a: (P, Int) be (P(x: 1, y: 2), 3)\n\
                  match a:\n\x20   (p, n):\n\x20       print(f\"{p.x} {p.y} {n}\")\n";
    assert_eq!(bytes("tuple-nested", source), "1 2 3\n");
}

/// **Five elements, and the last one is the one an off-by-one drops.**
///
/// A tuple of five is enough that a lowering which walked the layout's field
/// list and MIR's operand list out of step would produce a wrong number rather
/// than a crash, and the digits are distinct so the printed string says which
/// position moved.
#[test]
fn a_tuple_of_five_writes_every_element_at_its_own_offset() {
    let source = "let t: (Int, Int, Int, Int, Int) be (1, 2, 3, 4, 5)\n\
                  match t:\n\x20   (a, b, c, d, e):\n\x20       print(f\"{a}{b}{c}{d}{e}\")\n";
    assert_eq!(bytes("tuple-five", source), "12345\n");
}

/// **`let t be (1, 2)` still does not build, and the reason is no longer this
/// crate's.**
///
/// The program the last pass named as the boundary checks clean, has type
/// `(<error>, <error>)`, and is refused here — by the only phase that ever asks
/// what a tuple element's type is. The refusal names the front end rather than
/// the tuple, because a message saying *"a tuple"* would send a reader to
/// `cg_ty`, which has the arm.
///
/// **Both spellings that do work are asserted beside it**, so that this test
/// fails if the front-end gap is ever closed, rather than quietly continuing to
/// describe a fixed bug.
#[test]
fn the_unannotated_tuple_literal_is_a_front_end_hole() {
    let text = refusal("tuple-untyped", "let t be (1, 2)\nprint(\"x\")\n");
    assert!(
        text.contains("`TyKind::Error`") && text.contains("ExprKind::Tuple"),
        "the refusal should name the phase that left the hole:\n{text}"
    );
    // And the two spellings that give the elements a type both build and run.
    let annotated =
        "let t: (Int, Int) be (1, 2)\nmatch t:\n\x20   (a, b):\n\x20       print(f\"{a}{b}\")\n";
    assert_eq!(bytes("tuple-annotated", annotated), "12\n");
    let suffixed =
        "let t be (1i64, 2i64)\nmatch t:\n\x20   (a, b):\n\x20       print(f\"{a}{b}\")\n";
    assert_eq!(bytes("tuple-suffixed", suffixed), "12\n");
}
