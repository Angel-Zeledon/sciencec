//! The 55 entry points, classified — and the two properties a hand-maintained
//! list cannot have.
//!
//! **What this file is for.** §9.2's finding was a list that fell out of step
//! with the signatures it described, and §9.4's repair note says the list *"is
//! now checked by a test rather than maintained by hand"*. This is the codegen
//! side of that check: it classifies every entry point from its signature and
//! asserts properties of the result, so that adding an entry point with the
//! wrong return shape fails here rather than in a corrupted register.
//!
//! The two properties are: **the set is derived**, so it cannot be stale; and
//! **it is the same on all three targets**, so a developer on one platform
//! cannot leave the other two broken.

use science_codegen::abi::ReturnClass;
use science_codegen::layout::{CAbi, Triple};
use science_codegen::runtime::{
    EMITTED_FN_SIGNATURES, EXIT_CONTRACT, RUNTIME, RtParam, RtRet, runtime_fn,
};

const ABIS: [CAbi; 3] = [CAbi::SystemVAmd64, CAbi::Aapcs64, CAbi::Win64];

/// The two entry points whose return sits in the sixteen-byte band, where the
/// three conventions disagree about `sret`.
///
/// **This used to be the empty set, asserted.** The test below was
/// `the_sret_set_is_the_same_on_every_target`, and its comment said what would
/// make it fire: *"the day somebody adds an entry point returning a `{ f64,
/// f64 }`, which would be register-passed on System V and AAPCS64 and indirect
/// on Windows — and would need a per-target call site for the first time."*
/// `stdlib-core.md` §6.9's `parse_int` and `parse_float` are that day: their
/// pairs are `{ i64, TextError? }` and `{ f64, TextError? }`, sixteen bytes.
///
/// **The per-target call site already existed**, which is why the property
/// could be narrowed rather than defended: `science-codegen-llvm` builds every
/// runtime declaration from `runtime_signature(target, …)`, which classifies
/// per target, and emits `sret` or a register return from that answer alone.
/// What did not exist was the register return being spoken in C's registers
/// rather than LLVM's per-member split; `emit.rs`'s `c_return_ty` is that, and
/// its note records that `science_write_file` had been missing it too.
///
/// So the assertion is now exact rather than empty: these two, and only these
/// two, may differ by convention — and they differ in exactly one way.
const SIXTEEN_BYTE_RETURNS: [&str; 2] = ["science_string_parse_int", "science_string_parse_float"];

#[test]
fn the_sret_set_differs_between_targets_by_the_two_parse_pairs_on_windows_alone() {
    let sret = |abi: CAbi| -> Vec<&str> {
        RUNTIME.iter().filter(|f| f.needs_sret(abi)).map(|f| f.symbol).collect()
    };
    // System V and AAPCS64 both return up to sixteen bytes in two registers,
    // so they still agree on every entry point.
    assert_eq!(sret(CAbi::SystemVAmd64), sret(CAbi::Aapcs64));
    // Windows x64 returns in a register only at 1, 2, 4 or 8 bytes, so the two
    // sixteen-byte pairs are `sret` there and nowhere else.
    let mut windows: Vec<&str> = sret(CAbi::Win64);
    windows.retain(|symbol| !SIXTEEN_BYTE_RETURNS.contains(symbol));
    assert_eq!(windows, sret(CAbi::SystemVAmd64));
    for symbol in SIXTEEN_BYTE_RETURNS {
        let f = runtime_fn(symbol).unwrap();
        assert!(f.needs_sret(CAbi::Win64), "{symbol} on Windows x64");
        assert!(!f.needs_sret(CAbi::SystemVAmd64), "{symbol} on System V");
        assert!(!f.needs_sret(CAbi::Aapcs64), "{symbol} on AAPCS64");
    }
}

#[test]
fn every_aggregate_return_is_three_words_two_bytes_or_one_of_the_named_pairs() {
    // The reason the previous test's two sets agree everywhere else, stated as
    // the property rather than as the consequence. A new entry point in the
    // band has to be named above, which is the deliberate act it should be.
    for f in RUNTIME {
        let RtRet::Aggregate(aggregate) = f.ret else { continue };
        let size = aggregate.layout(Triple::X86_64LinuxGnu).size;
        assert!(
            size == 2 || size >= 24 || (size == 16 && SIXTEEN_BYTE_RETURNS.contains(&f.symbol)),
            "{}: {} is {size} bytes, which is in the band where the three conventions disagree",
            f.symbol,
            aggregate.name()
        );
    }
}

#[test]
fn every_entry_point_that_takes_a_descriptor_takes_exactly_one() {
    // Two descriptors in one call would be ambiguous at the call site, and
    // `science_map_*` takes a `ScienceMapInfo` that *contains* two rather than
    // two parameters — which is a design choice worth not undoing by accident.
    for f in RUNTIME {
        let count = f.params.iter().filter(|p| **p == RtParam::Descriptor).count();
        assert!(count <= 1, "{} takes {count} descriptors", f.symbol);
    }
}

#[test]
fn the_container_entry_points_all_take_a_descriptor_and_the_scalar_ones_do_not() {
    // §6: a container call is parameterised at run time by a description of the
    // element type. An `Array` or `Map` entry point that did not take one would
    // be one that had guessed.
    for f in RUNTIME {
        let touches_elements = matches!(
            f.symbol,
            "science_array_new"
                | "science_array_with_capacity"
                | "science_array_reserve"
                | "science_array_free"
                | "science_array_push"
                | "science_array_pop"
                | "science_array_get"
                | "science_array_get_mut"
                // The chain's one barrier. It moves whole elements around
                // inside the buffer, so it needs their width and alignment
                // exactly as `push` and `pop` do — the keys beside them are
                // `Int`s and need no describing.
                | "science_array_sort_by_int_key"
                | "science_map_new"
                | "science_map_free"
                | "science_map_insert"
                | "science_map_get"
                | "science_map_contains"
                | "science_map_remove"
                // `Set of T`'s own two: a set is a map whose value is `()`,
                // and hashing the element needs the map descriptor's `hash_fn`.
                | "science_set_insert"
                | "science_set_remove"
                // The positional accessor behind `for entry in counts:`. It
                // computes where entry `n` starts from the key and value
                // sizes, which only the descriptor knows.
                | "science_map_entry_at"
                | "science_box_new"
                | "science_box_free"
        );
        assert_eq!(
            f.descriptor_index().is_some(),
            touches_elements,
            "{} disagrees about whether it needs a descriptor",
            f.symbol
        );
    }
    // And the ones that only read a header do not: a length is a length
    // whatever the elements are.
    assert!(runtime_fn("science_array_len").unwrap().descriptor_index().is_none());
    assert!(runtime_fn("science_array_is_empty").unwrap().descriptor_index().is_none());
    assert!(runtime_fn("science_array_as_ptr").unwrap().descriptor_index().is_none());
    assert!(runtime_fn("science_map_len").unwrap().descriptor_index().is_none());
    // `String`'s four searches take none either, and are named so that the
    // loop above is not the only thing saying so: a `ScienceString` is always
    // bytes, so there is no element type to describe. `find`'s third pointer
    // is its `Int?` out-parameter, not a slot the runtime sizes from a
    // descriptor — `science_chars_next`'s shape and not `science_map_remove`'s.
    for symbol in [
        "science_string_ends_with",
        "science_string_contains",
        "science_string_find",
        "science_string_replace",
    ] {
        let f = runtime_fn(symbol).unwrap_or_else(|| panic!("{symbol} is not in RUNTIME"));
        assert!(f.descriptor_index().is_none(), "{symbol} takes a descriptor");
        assert!(!f.params.contains(&RtParam::Slot), "{symbol} takes a slot");
    }
    // A loop's extent is a count of positions, which is a header field too.
    assert!(runtime_fn("science_map_extent").unwrap().descriptor_index().is_none());
}

#[test]
fn a_diverging_entry_point_is_never_called_for_its_value() {
    // Decision 6: a panic edge lowers to `call science_panic_bytes(...)`
    // followed by `unreachable`. An `RtRet::Never` with a register class would
    // be a code generator expecting a value back from a function that aborts.
    for f in RUNTIME {
        if f.ret == RtRet::Never {
            for abi in ABIS {
                assert_eq!(f.return_class(abi), ReturnClass::Void, "{}", f.symbol);
            }
        }
    }
}

#[test]
fn the_eleven_sret_entry_points_are_named_so_a_reader_can_check_them_by_hand() {
    // The derived set, printed as the assertion rather than computed into one,
    // so that a reader comparing this file against `science-rt` §2 sees the
    // difference immediately. §2 listed eight; this was nine, and
    // `science_string_with_capacity` made it ten — read off its signature, not
    // added to a list.
    //
    // **Eleven now**, and the two that joined are §3.1's two `FormatSpec`
    // returns: `science_formatter_spec` and `science_format_spec_default`.
    // Neither was added to any list — a six-word record is MEMORY on every
    // convention, so the classifier said so. The other five `Formatter` entry
    // points return `()`, and `science_formatter_init` does so by taking an
    // out-pointer rather than by luck.
    //
    // **Fourteen.** `science_io_error_message` and `science_string_lines`
    // had already made it thirteen without this test's name following, and
    // `science_string_replace` is the fourteenth: `String.replace` returns a
    // fresh `String`, three words, MEMORY everywhere. Its three siblings —
    // `ends_with`, `contains`, `find` — return a `Bool` and are not here;
    // `find`'s `Int?` travels through an out-pointer for §5.3's reason, not
    // this list's.
    //
    // `science_text_error_message` joins as `science_io_error_message` did: a
    // `String` back, three words. The two parse pairs beside it do **not**
    // join this list, which is System V's — sixteen bytes is two registers
    // there; see `the_sret_set_differs_between_targets_…` for Windows.
    //
    // `science_string_from_utf8` joins as `science_read_file` did: its
    // `(String, TextError?)` is thirty-two bytes, MEMORY everywhere.
    //
    // **Nineteen.** §6.9's last three each return an aggregate:
    // `science_string_trim` an owned `String`, `science_string_slice` the
    // same four-word `(String, TextError?)` as `from_utf8`, and
    // `science_string_split` a nine-word `Split`. `science_split_next` returns
    // a `Bool` and `science_split_free` nothing.
    //
    // **Twenty.** `science_read_line` returns `(String?, IoError?)`, forty
    // bytes — `science_read_file`'s pair with a tagged `String?` in front —
    // MEMORY everywhere.
    let mut derived: Vec<&str> =
        RUNTIME.iter().filter(|f| f.needs_sret(CAbi::SystemVAmd64)).map(|f| f.symbol).collect();
    derived.sort_unstable();
    assert_eq!(
        derived,
        [
            "science_array_new",
            "science_array_with_capacity",
            "science_format_spec_default",
            "science_formatter_spec",
            "science_io_error_message",
            "science_map_new",
            "science_read_file",
            "science_read_line",
            "science_string_chars",
            "science_string_clone",
            "science_string_from_bytes",
            "science_string_from_utf8",
            "science_string_lines",
            "science_string_new",
            "science_string_replace",
            "science_string_slice",
            "science_string_split",
            "science_string_trim",
            "science_string_with_capacity",
            "science_text_error_message",
        ]
    );
}

#[test]
fn the_emitted_function_pointer_signatures_are_available_to_a_code_generator() {
    // §9.3's finding 2: the three signatures codegen must *emit* are in
    // `abi.rs`'s item docs and not on the contract page, and the note calls
    // that "the most consequential gap because the three functions are
    // codegen's own output and a wrong signature is a wrong calling
    // convention".
    assert_eq!(EMITTED_FN_SIGNATURES.len(), 3);
    let by_name: std::collections::BTreeMap<_, _> = EMITTED_FN_SIGNATURES.iter().copied().collect();
    assert_eq!(by_name["drop_fn"], "void (*)(uint8_t *value)");
    assert_eq!(by_name["hash_fn"], "uint64_t (*)(const uint8_t *key)");
    assert_eq!(by_name["eq_fn"], "bool (*)(const uint8_t *a, const uint8_t *b)");
}

#[test]
fn the_exit_contract_is_satisfiable_and_still_cannot_render_the_error() {
    // §9.3's finding 5, in the state it is now in. The mechanical half is
    // closed: `science-rt` has a stderr writer that returns and a symbol that
    // exits with a chosen status, so a `main` that returns an error can be
    // lowered and `script-mode.md` §2.3's fourth row is reachable.
    assert!(EXIT_CONTRACT.is_satisfiable());
    assert_eq!(EXIT_CONTRACT.eprint_symbol, Some("science_write_error_bytes"));
    assert_eq!(EXIT_CONTRACT.exit_symbol, Some("science_exit"));
    for symbol in [EXIT_CONTRACT.eprint_symbol, EXIT_CONTRACT.exit_symbol] {
        let symbol = symbol.expect("both are named");
        assert!(runtime_fn(symbol).is_some(), "{symbol} is named and not in the table");
    }
    // And the half that is open, as the tripwire the old version of this test
    // was: it fails on the day `Display` grows a method, which is the day the
    // placeholder in `science-codegen-llvm`'s `lower::ERROR_MESSAGE` should
    // become the error itself.
    assert!(!EXIT_CONTRACT.renders_the_error());
    // `science_main` is still absent, and deliberately: finding 5's *"there is
    // no program entry point"* is answered by codegen emitting `main`, not by
    // the runtime growing one.
    assert!(runtime_fn("science_main").is_none());
}

#[test]
fn nothing_outside_the_table_is_callable() {
    // Decision 14: "Those symbols are the only runtime calls F0 emits.
    // Everything else is inline. No entry point is added to `science-rt` to
    // make codegen simpler."
    //
    // Forty-five until finding 5 was discharged; forty-seven after it, fifty-four
    // once `format.rs` could render a number, and fifty-five after that. The two
    // that joined first are the two `codegen-and-linking.md` §13 asks for by
    // name, and they are not a convenience for codegen — without them §2.3's
    // fourth row has nothing to lower to at all. The fifty-fifth is
    // `science_string_with_capacity`, and the same test applies to it:
    // `strings-formatting-and-docs.md` §1.7 prescribes *"one allocation"* and
    // fifty-four entry points had no way to express a capacity, so this is a
    // note's requirement arriving rather than codegen being made simpler.
    //
    // **Fifty-nine now**, and the four that joined pass Decision 14's test for
    // the same reason the others did.
    //
    // `science_int_hash` and `science_int_eq` are the `hash_fn`/`eq_fn` pair
    // for an eight-byte `Map` key. They are not *called* by emitted code at
    // all — their addresses are stored into a `ScienceMapInfo` global — and
    // codegen could have emitted the two bodies itself, since they are a load
    // and a compare. `runtime::map_key_support` records why it does not:
    // `String`'s pair has always been a runtime symbol
    // (`science_string_hash`, `science_string_eq`), and a key type whose hash
    // and equality live in two different crates depending on the key is a key
    // type whose hash and equality disagree in two different crates. So this
    // is one mechanism covering both, not a convenience.
    //
    // `science_libm_pow` and `science_ipow_i64` are `**`'s codegen. §7.3's
    // Decision 37 forbids `llvm.pow`, and no instruction computes an integer
    // power at all, so neither stands in for an instruction sequence this
    // crate could have emitted instead — which is the one door this rule
    // leaves open.
    //
    // **Sixty-six now**, and the seven that joined are §3.1's `Formatter`:
    // `init`, `text`, `raw`, `number`, `integer`, `spec` and
    // `science_format_spec_default`. They pass Decision 14's test the same way
    // `**`'s two did — none of them stands in for an instruction sequence this
    // crate could have emitted instead. Padding to a width with a fill
    // character, grouping digits in threes and choosing between fixed and
    // exponential form at a significant-figure count are a library, and §3.1
    // asks for them by name and by signature.
    //
    // **Sixty-nine now.** `science_string_lines` and `science_lines_next` are
    // `stdlib-core.md` §4.5's Level 1 `Lines`, `Chars`' pair one type over:
    // finding each line end, dropping a `\r` before it and allocating the line
    // is a loop over a buffer, not an instruction sequence.
    //
    // **Seventy-one.** `science_set_insert` and `science_set_remove` are
    // `Set of T`'s, over a `ScienceMap` whose value is `()`: *new* is the
    // negation of *displaced*, which is an instruction, but the value slot a
    // map's insert and remove need an address for is not — so both are here
    // rather than threading a zero-sized operand through two call sites.
    //
    // **Seventy-two.** `science_io_error_message` is `IoError`'s `message`,
    // five constant sentences picked by a byte — a table in the binary, which
    // a runtime function holds once instead of every call site holding five.
    //
    // **Seventy-six.** `science_string_ends_with`, `science_string_contains`,
    // `science_string_find` and `science_string_replace` are `stdlib-core.md`
    // §6.9's searches beside `starts_with`, declared in the prelude and
    // refused by the backend until now for want of a body. A substring
    // search is a loop over the haystack, and `replace` is that loop building
    // a new buffer, so none stands in for an instruction sequence — the same
    // door `science_string_lines` went through, and not a convenience.
    //
    // **Seventy-nine.** `science_write_error`, `science_print_error` and
    // `science_flush` are §8's `write_error`, `print_error` and `flush`, which
    // `strings-formatting-and-docs.md` §4.3 added to the free functions. A
    // write to a file descriptor and a flush of a buffer that lives inside
    // `std` are not instruction sequences this crate could have emitted, and
    // none of the three is on the tempting list below.
    //
    // **Eighty-one.** `science_map_extent` and `science_map_entry_at` are
    // what `for entry in counts:` and `for x in set:` compile to, in
    // `collections-and-chains.md` §5.2's insertion order. The extent is a
    // field read, and on its own it would be the kind of addition this rule
    // forbids; it is here because the accessor beside it cannot be inline —
    // it computes an entry's address from a stride only the descriptor's two
    // sizes determine and tests a liveness byte the runtime's own layout puts
    // there — and a loop that read one header field inline and called the
    // runtime for the rest would be two places that know `ScienceMap`'s
    // layout instead of one.
    //
    // **Eighty-four.** `science_string_parse_int` and
    // `science_string_parse_float` are `stdlib-core.md` §6.9's two parses, and
    // `science_text_error_message` is the `message` of the `TextError` they
    // return — `science_io_error_message`'s reason exactly. Correctly rounded
    // decimal-to-binary conversion is Eisel–Lemire with a big-number fallback,
    // which is a library and not an instruction sequence, so Decision 14's
    // door is the one `**`'s two came through.
    //
    // **Eighty-six.** `science_string_push_io_error` and
    // `science_string_push_text_error` are `IoError implements Display` and
    // `TextError implements Display`: the f-string builder's pushes for the
    // two error types, which append the sentence each `message` returns. The
    // sentence is picked by a byte from a table `science-rt` holds once, for
    // `science_io_error_message`'s reason; emitting the table here instead
    // would be a second copy that `print(err)` and `print(err.message())` could
    // disagree through.
    //
    // **Eighty-seven.** `science_string_from_utf8` is §6.9's
    // `String.from_bytes`. UTF-8 validation is a loop over the bytes, not an
    // instruction sequence, and it is the only way a `String` is built from
    // bytes a program did not write as a literal — the bundled `os` module's
    // way back from the runtime.
    //
    // **One hundred and four.** `stdlib-core.md` §8.1's Level 1 math added
    // seventeen. Sixteen are `science_libm_*` — `cbrt`, `hypot`, `exp`, `ln`,
    // `log2`, `log10`, the six circular and three hyperbolic functions and
    // `atan2` — and they come through `science_libm_pow`'s door exactly: a
    // libm call is not an instruction sequence codegen could emit, and
    // Decision 37 forbids the `llvm.sin` that would pretend to be one. The
    // seventeenth, `science_rem_euclid_i64`, is `science_ipow_i64`'s case:
    // `Int.rem_euclid(0)` panics, a panic is a branch, and a method call is one
    // block. What did *not* join is the point of the rule: `abs`, `floor`,
    // `min` and the rest of the whitelist are intrinsics, and `sign`, `clamp`,
    // `fract`, the predicates and five of `Int`'s seven methods are a few
    // inline compares and selects — the tempting list below grows by the
    // names that were kept out.
    //
    // **One hundred and nine.** `science_string_trim`, `science_string_slice`,
    // `science_string_split`, `science_split_next` and `science_split_free`:
    // §6.9's `trim`, `slice` and `split`, and the iterator `split` returns. A
    // scan for whitespace, for a character boundary and for a separator, each
    // building a fresh buffer — `science_string_replace`'s door. The `free` is
    // there because `Split` owns copies of its two operands, which its
    // `science-rt` doc comment argues; releasing two buffers is two calls to a
    // function this table already has, made where the layout is known.
    //
    // **One hundred and ten.** `science_read_line`, `stdlib-core.md` §4.4's
    // `read_line()`: a read of standard input up to a newline through the
    // process's one buffer, then a UTF-8 check — a system call and a loop,
    // `science_read_file`'s door. `io`'s `File.open`, `File.read` and
    // `Stdin.read` are Science calling `science-rt` through an `extern` block,
    // so they are not in this table, as `File.create` is not.
    assert_eq!(RUNTIME.len(), 110);
    // The tempting additions, named so that adding one is a deliberate act:
    // §2.6 puts every one of these in the inline column.
    for tempting in [
        "science_nullable_is_present",
        "science_nullable_unwrap",
        "science_bounds_check",
        "science_int_add",
        "science_string_format",
        "science_libm_sqrt",
        "science_libm_abs",
        "science_libm_floor",
        "science_libm_min",
        "science_float_sign",
        "science_float_is_nan",
        "science_int_abs",
    ] {
        assert!(runtime_fn(tempting).is_none(), "{tempting} is inline, per §2.6");
    }
}
