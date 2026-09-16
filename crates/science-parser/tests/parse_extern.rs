//! Snapshot tests for `extern` blocks: `ffi-c-boundary.md` §1 and §5, and the
//! four additions `c-binding-coverage.md` asks for.
//!
//! These go through the real lexer rather than a hand-built token stream. The
//! block's grammar is contextual almost throughout — `library`, `via`,
//! `pkg-config`, `kind`, `when`, `available`, `symbol`, `size` and `align` are
//! ordinary identifiers, and `pkg-config` is three tokens that have to touch —
//! so a test written as a token list would be asserting the thing most likely
//! to be wrong about it, namely the token list.

mod common;
use common::{parse_source, parse_source_allowing_errors};

/// The fixes a source offers, as `(code, replaced text, replacement)`.
///
/// The snapshot format does not render suggestions, so a fix pointing at the
/// wrong span would otherwise be invisible — `migration.rs` makes the same
/// argument and this file borrows it. What is asserted here is the *applicable*
/// half of the FFI diagnostics: the two that have an exact fix have one, and
/// the ones whose fix is a C shim or a number nobody can guess offer none.
#[track_caller]
fn fixes(source: &str) -> Vec<(String, String, String)> {
    let (tokens, _lexical) = science_lexer::lex(common::FILE, source);
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    diagnostics
        .iter()
        .flat_map(|d| {
            d.suggestions.iter().map(|fix| {
                (
                    d.code.to_string(),
                    source[fix.span.start as usize..fix.span.end as usize].to_string(),
                    fix.replacement.clone(),
                )
            })
        })
        .collect()
}

// --- the block, as §1.1 writes it ---------------------------------------

#[test]
fn the_block_of_section_1_1() {
    insta::assert_snapshot!(parse_source(
        r#"unsafe extern "C" library "openblas" via pkg-config "openblas":
    type CblasLayout is CInt
    const CBLAS_ROW_MAJOR be 101 as CblasLayout
    const CBLAS_COL_MAJOR be 102 as CblasLayout

    type CblasTranspose is CInt
    const CBLAS_NO_TRANS be 111 as CblasTranspose
    const CBLAS_TRANS be 112 as CblasTranspose

    def cblas_dgemm(
        layout: CblasLayout,
        transpose_a: CblasTranspose,
        transpose_b: CblasTranspose,
        m: BlasInt, n: BlasInt, k: BlasInt,
        alpha: F64,
        a: ffi.Span of F64, lda: BlasInt,
        b: ffi.Span of F64, ldb: BlasInt,
        beta: F64,
        c: ffi.MutableSpan of F64, ldc: BlasInt,
    )
"#
    ));
}

/// §5.1 and §5.2: the three clauses that hang off the library name, all at
/// once, in the order a reader is least likely to have memorised.
#[test]
fn every_library_clause_at_once() {
    insta::assert_snapshot!(parse_source(
        r#"unsafe extern "C" library "cudnn" kind static when available via pkg-config "cudnn":
    def cudnnGetVersion() -> CSizeT
"#
    ));
}

/// §1.6: the two builds differ in one alias and one linker name, and nothing
/// else. Getting the pair wrong is the worst failure mode in numerical
/// computing, and `symbol` is what turns it into a link error.
#[test]
fn both_integer_widths_of_section_1_6() {
    insta::assert_snapshot!(parse_source(
        r#"unsafe extern "C" library "openblas":
    type BlasInt is I32
    def dgemm(m: BlasInt, n: BlasInt) symbol "dgemm_"

unsafe extern "C" library "openblas64_":
    type BlasInt64 is I64
    def dgemm64(m: BlasInt64, n: BlasInt64) symbol "dgemm_64_"
"#
    ));
}

// --- the four additions of `c-binding-coverage.md` -----------------------

/// Decision 7, the largest gap in the audit: `H5T_NATIVE_DOUBLE` is
/// `(H5open(), H5T_NATIVE_DOUBLE_g)`, and this is the global half.
#[test]
fn exported_globals() {
    insta::assert_snapshot!(parse_source(
        r#"unsafe extern "C" library "hdf5":
    type Hid is I64
    type Herr is I32
    def H5open() -> Herr
    static H5T_NATIVE_DOUBLE_g: Hid
    static H5T_NATIVE_INT_g: Hid
    static PyExc_TypeError: ffi.Pointer of PyObject
"#
    ));
}

/// §4.1(1) of the audit: half of LAPACK is behind this, and
/// `ffi-c-boundary.md` §2.4 already uses the name its own §1.3 never defined.
#[test]
fn the_complex_types_are_ordinary_ffi_types() {
    insta::assert_snapshot!(parse_source(
        r#"unsafe extern "C" library "openblas":
    type BlasInt is I32
    def cblas_zgemv(
        m: BlasInt,
        alpha: borrowed ffi.Complex64,
        a: ffi.Span of ffi.Complex64,
        y: ffi.MutableSpan of ffi.Complex64,
    ) symbol "cblas_zgemv"
    def cblas_cdotu(x: ffi.Span of ffi.Complex32) -> ffi.Complex32
"#
    ));
}

/// §3.4 of the audit: HDF5 and CPython both have one in the public interface,
/// so §1.4's "no library in the target set needs a union" is wrong. The import
/// is the opaque blob §1.4 already described, and the arms are unreadable by
/// construction.
#[test]
fn unions_are_opaque_blobs_of_a_size_and_an_alignment() {
    insta::assert_snapshot!(parse_source(
        r#"unsafe extern "C" library "hdf5":
    type Hid is I64
    union H5L_info2_t: size 32 align 8
    union H5R_ref_t: size 64 align 8
    def H5Rget_type(reference: borrowed H5R_ref_t) -> I32
"#
    ));
}

/// §3.3 of the audit and §1.4 of the note: the declaration is recognised so
/// that it can be refused, which is the only way the message can name the
/// function and point at the `...`.
#[test]
fn a_variadic_function_is_recognised_and_refused() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "python3":
    def PyErr_Format(kind: ffi.Pointer of PyObject, format: ffi.CStr, ...) -> ffi.Pointer of PyObject
    def PyErr_SetString(kind: ffi.Pointer of PyObject, message: ffi.CStr)
"#
    ));
}

/// The refusal must not cost the declarations around it: the block still
/// yields its other items, which is what keeps one `printf` from becoming an
/// error per call site.
#[test]
fn a_variadic_function_does_not_derail_the_block() {
    let report = parse_source_allowing_errors(
        r#"unsafe extern "C" library "z":
    def gzprintf(file: ffi.OpaqueHandle, format: ffi.CStr, ...) -> CInt
    def gzclose(file: ffi.OpaqueHandle) -> CInt
"#,
    );
    assert!(report.contains("ExternFn `gzclose`"), "the next declaration was lost:\n{report}");
    assert_eq!(report.matches("SC0434").count(), 1, "one `...`, one diagnostic:\n{report}");
}

// --- what the vocabulary refuses ----------------------------------------

/// §1.3, `SC0421`: C wants the elements and an `Array` is a header. The fix is
/// exact because §1.3 coerces at the call site, so the call does not move.
#[test]
fn an_array_in_a_signature_names_the_span_that_replaces_it() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "openblas":
    def takes(a: borrowed Array of F64, b: mutable borrowed Array of F64, c: Array of I32)
"#
    ));
}

/// §1.3 and §1.4: every one of these is a Science layout, and the note for
/// each says which C spelling the binding wanted instead.
#[test]
fn the_science_layouts_are_refused_by_name() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "z":
    def a(text: borrowed String)
    def b(table: Map of (String, I32))
    def c(maybe: Option of I32) -> Result of (I32, I32)
    def d(pair: (I32, I32))
    def e(value: any Summarize)
    type Bad is Array of F64
    static worse: String
    const WORST be 1 as Box of I32
"#
    ));
}

/// §1.3, `SC0431`: `_Float16` is passed differently by GCC, Clang and MSVC, so
/// half precision crosses by reference only. Under a borrow it is fine, which
/// is what the second parameter checks.
#[test]
fn half_precision_is_refused_by_value_and_admitted_by_reference() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "cudnn":
    def scale(alpha: F16, out: mutable borrowed F16, buffer: ffi.Span of BF16) -> BF16
"#
    ));
}

// --- the block's own mistakes -------------------------------------------

/// §1.1: the keyword belongs where the unverifiable act is, and the act is
/// writing the declaration.
#[test]
fn a_block_without_unsafe_is_reported_and_still_parsed() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"extern "C" library "z":
    def crc32(crc: CULong) -> CULong
"#
    ));
}

/// §5.1: the name is the link target, and `via pkg-config` is how its flags
/// are found rather than a replacement for it.
#[test]
fn a_block_without_a_library_is_reported_and_still_parsed() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C":
    def crc32(crc: CULong) -> CULong

unsafe extern "C" via pkg-config "zlib":
    def crc32_z(crc: CULong) -> CULong
"#
    ));
}

/// §1.1: `"C"` is the platform's C convention, and Fortran BLAS is `"C"` too.
#[test]
fn an_abi_other_than_c_is_reported() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "Fortran" library "openblas":
    def dgemm(m: I32)
"#
    ));
}

/// §1.2 closed the list, and the message enumerates it rather than describing
/// it: a reader who wrote a record inside the block needs to see the five
/// forms and where a record goes instead.
#[test]
fn an_item_form_the_block_does_not_have() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "hdf5":
    type Hid is I64
    choice Status:
        Ok
        Failed
    interface Readable:
        def read(self)
    def H5open() -> Hid
"#
    ));
}

/// A union whose layout was left off. Neither number can be guessed, so the
/// diagnostic says where they come from instead of offering a fix.
#[test]
fn a_union_without_its_layout() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "hdf5":
    union H5R_ref_t:
    union H5L_info2_t: size 32
"#
    ));
}

/// `public` names nothing on a block: the block itself is not a name, and the
/// items in it are module-level names on their own account.
#[test]
fn public_on_a_block_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"public unsafe extern "C" library "z":
    def crc32(crc: CULong) -> CULong
"#
    ));
}

/// `via pkg - config` is a subtraction anywhere else in the language and is
/// not the name of a build tool here either.
#[test]
fn pkg_config_has_to_be_written_as_one_word() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"unsafe extern "C" library "z" via pkg - config "zlib":
    def crc32(crc: CULong) -> CULong
"#
    ));
}

/// The contextual words are contextual: outside the block's clause positions
/// every one of them is an ordinary name, and a program that uses them as
/// names keeps working.
#[test]
fn the_contextual_words_are_still_names_everywhere_else() {
    insta::assert_snapshot!(parse_source(
        r#"def shelve(library: I32, kind: I32, size: I32, align: I32) -> I32:
    let via be library + kind
    let symbol be size + align
    let available be via + symbol
    available
"#
    ));
}

// --- `unsafe` as a block ------------------------------------------------

/// §3: the block is an expression, in both of §4.5's forms, and it is what
/// delimits the source a reviewer reads against the C documentation.
#[test]
fn the_unsafe_block_in_both_forms() {
    insta::assert_snapshot!(parse_source(
        r#"def call():
    unsafe:
        H5open()
        H5close()
    let version be unsafe: cudnnGetVersion()
    version
"#
    ));
}

// --- §1.7, the acceptance criterion -------------------------------------

// --- the fixes, which the snapshot format does not render ----------------

/// `SC0421` and `SC0414` each have an exact fix, and a tool may apply either
/// without changing what the program means: §1.3 coerces `borrowed Array of T`
/// to a span at the call site, and `unsafe` adds a word to a declaration whose
/// items do not move.
#[test]
fn the_two_exact_fixes_replace_exactly_what_they_should() {
    assert_eq!(
        fixes(
            r#"unsafe extern "C" library "openblas":
    def takes(a: borrowed Array of F64, b: mutable borrowed Array of F64)
"#
        ),
        vec![
            ("SC0421".to_string(), "borrowed Array of F64".to_string(), "ffi.Span of F64".to_string()),
            (
                "SC0421".to_string(),
                "mutable borrowed Array of F64".to_string(),
                "ffi.MutableSpan of F64".to_string()
            ),
        ]
    );

    assert_eq!(
        fixes(
            r#"extern "C" library "z":
    def crc32(crc: CULong) -> CULong
"#
        ),
        vec![("SC0414".to_string(), "extern".to_string(), "unsafe extern".to_string())]
    );
}

/// The three diagnostics whose fix is not an edit to the line offer none.
///
/// A suggestion here would be worse than silence: dropping a `...` leaves a
/// declaration that links and passes its arguments under the wrong convention,
/// rewriting an ABI string leaves one that links and is wrong, and a union's
/// size is a number no tool in this phase knows.
#[test]
fn the_diagnostics_with_no_mechanical_fix_offer_none() {
    for source in [
        r#"unsafe extern "C" library "z":
    def gzprintf(file: ffi.OpaqueHandle, format: ffi.CStr, ...) -> CInt
"#,
        r#"unsafe extern "Fortran" library "openblas":
    def dgemm(m: I32)
"#,
        r#"unsafe extern "C" library "hdf5":
    union H5R_ref_t: size 64
"#,
        r#"unsafe extern "C" library "z":
    def a(text: borrowed String)
"#,
    ] {
        assert!(fixes(source).is_empty(), "this diagnostic should not offer a fix:\n{source}");
    }
}

/// The `dgemm` binding of §1.7, in revision-2 syntax: the declaration, the
/// safe wrapper whose signature contains no `ffi` type at all, and the check
/// that discharges the obligation the borrow checker cannot.
///
/// The named arguments at the call site are §1.7's one exception to §4.6, and
/// they need nothing from this parser: a call with named arguments on a path
/// is already §4.4's ambiguity, parsed as a construction and reclassified by
/// name resolution.
#[test]
fn the_dgemm_binding_of_section_1_7_parses_clean() {
    let source = include_str!("fixtures/dgemm.science");
    insta::assert_snapshot!(parse_source(source));
}

// --- the corpus file -----------------------------------------------------

/// `examples/20_extern.science`, dumped.
///
/// The corpus test next door already asserts that every example parses with no
/// diagnostics; this one pins the *tree*, so that a change to the block's
/// grammar which still parses but parses differently is visible in a diff
/// rather than in whatever breaks three phases later.
#[test]
fn the_example_corpus_file() {
    let source = include_str!("../../../examples/20_extern.science");
    insta::assert_snapshot!(parse_source(source));
}
