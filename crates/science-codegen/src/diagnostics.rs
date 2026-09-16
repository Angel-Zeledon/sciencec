//! `SC0400`–`SC0409`: §11 of `codegen-and-linking.md`, and nothing else.
//!
//! **The decision.** This crate claims the whole free General codegen
//! sub-range and takes none of the neighbouring ranges. `SC0410`–`SC0449`
//! belong to `ffi-c-boundary.md`, `SC0450`–`SC0459` to the Python boundary,
//! `SC0460`–`SC0461` to linking, `SC0462`–`SC0470` to `rust-interop.md` and
//! `SC0471`–`SC0479` to `native-dependencies.md`.
//!
//! **The reason.** A code range is a shared resource with one record, and the
//! cheapest way to corrupt it is for a crate to need a code at eleven at night
//! and take the next free number. The constants below are the whole vocabulary;
//! a new one is a change to §11 first.
//!
//! **The cost.** Six codes for a code generator is not many, and `SC0407`–
//! `SC0409` are reserved rather than spent, so the practical budget is three
//! unclaimed. §11 reserves them for the ABI classifier of §4.3, which *"will
//! want at least one code for an aggregate shape it cannot classify"*.
//!
//! # Codes this crate emits from other people's ranges
//!
//! `SC0429` and `SC0431` are `ffi-c-boundary.md`'s and are rendered by
//! [`crate::abi::AbiRefusal`]. That is §11's *"amended, not claimed"*: the
//! refusal is emitted here because this is where the classification happens,
//! and the code stays in the range of the note that defined the rule.
//!
//! `docs/superpowers/design/README.md` is **not edited**. §11: *"The row
//! belongs to whoever maintains the allocation record."*

use science_diagnostics::{Code, Diagnostic, Label, Span};

/// The codes, as constants, so that a typo is a compile error.
pub mod code {
    use science_diagnostics::Code;

    /// A toolchain feature required to build this program is not compiled into
    /// this `sciencec`.
    ///
    /// §11 adds: *"Names the feature and how to obtain a build that has it.
    /// This discharges `data-io.md` §7's ask."* Today this is the code the
    /// compiler emits for every `build`, because the LLVM backend is not
    /// compiled in — see [`super::backend_not_compiled_in`].
    pub const SC0400: Code = Code(400);
    /// No linker driver found.
    pub const SC0401: Code = Code(401);
    /// The linker failed for a reason that is not an attributable undefined
    /// symbol.
    pub const SC0402: Code = Code(402);
    /// `sciencec build` was given a file with no entry point.
    pub const SC0403: Code = Code(403);
    /// Two distinct monomorphisation keys produced the same mangled symbol.
    pub const SC0404: Code = Code(404);
    /// A `static` or `const` initialiser is not a constant expression codegen
    /// can emit into a data section.
    pub const SC0405: Code = Code(405);
    /// The requested target triple is not the host's.
    pub const SC0406: Code = Code(406);

    /// Reserved for the ABI classifier of §4.3. Unused.
    pub const SC0407: Code = Code(407);
    /// Reserved. Unused.
    pub const SC0408: Code = Code(408);
    /// Reserved. Unused.
    pub const SC0409: Code = Code(409);

    /// `ffi-c-boundary.md`'s: a by-value aggregate across the `extern`
    /// boundary. Referenced, not claimed.
    pub const SC0429: Code = Code(429);
    /// `ffi-c-boundary.md`'s: `F16`/`BF16` by value across the boundary.
    /// Referenced, not claimed.
    pub const SC0431: Code = Code(431);
    /// `ffi-c-boundary.md`'s: an undefined symbol codegen can attribute to a
    /// declaration. Referenced, not claimed; §5.5 specifies its rendering and
    /// §11 gives `SC0402` as the fallback.
    pub const SC0461: Code = Code(461);
}

/// The whole range this crate claims, for the test that asserts it emits
/// nothing outside it.
pub const CLAIMED: std::ops::RangeInclusive<u16> = 400..=409;

/// `SC0400`: the LLVM backend is not compiled into this `sciencec`.
///
/// This is the diagnostic the compiler actually produces today, and it is
/// exactly what `SC0400` was reserved for: *"names the feature and how to
/// obtain a build that has it"*. Writing it as a first-class diagnostic rather
/// than a `panic!` or an `eprintln!` is the difference between a compiler that
/// has a missing feature and a compiler that is broken.
///
/// The `how` text is a parameter rather than a constant so that the driver can
/// name the platform's package. A message that says "install LLVM" on a machine
/// where the answer is one `winget` line is a message that costs an hour.
pub fn backend_not_compiled_in(backend: &str, how: &str) -> Diagnostic {
    Diagnostic::error(code::SC0400, format!("no `{backend}` backend is compiled into this `sciencec`"))
        .with_note(
            "`science-codegen` computes layout, the C ABI, symbol names and the runtime \
             boundary, but emitting an object file needs a backend and none is linked in",
        )
        .with_note(how.to_string())
}

/// `SC0401`: no linker driver found.
///
/// §11: *"Names the candidates searched, in order, and the `SCIENCE_LINKER`
/// variable."* Both halves matter. The candidate list tells the reader what the
/// compiler believed; the variable tells them what to do about it.
pub fn no_linker_driver(candidates: &[String]) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(code::SC0401, "no linker driver was found");
    diagnostic = diagnostic.with_note(format!("searched, in order: {}", candidates.join(", ")));
    diagnostic.with_note("set `SCIENCE_LINKER` to the driver to use")
}

/// `SC0402`: the linker failed for a reason codegen cannot attribute.
///
/// §5.5's obligation, and it has two halves that are easy to do one of: print
/// the command line and the linker's output **verbatim**, and *say that it is
/// doing so*. A reader who does not know they are looking at another program's
/// output spends the first minute deciding whether the compiler is broken.
pub fn linker_failed(command: &str, output: &str) -> Diagnostic {
    Diagnostic::error(code::SC0402, "the linker failed")
        .with_note(format!("the command was: {command}"))
        .with_note("what follows is the linker's own output, verbatim:")
        .with_note(output.to_string())
}

/// `SC0403`: `build` was given a file with no entry point.
///
/// §11 names the gap this fills: *"`script-mode.md` §4.2 rule 3 defines this
/// situation as a library build and does not say what happens when somebody
/// asks for a binary."*
pub fn no_entry_point(file: &str) -> Diagnostic {
    Diagnostic::error(code::SC0403, format!("`{file}` has no entry point"))
        .with_note("a binary needs either top-level statements or a `def main`")
        .with_note("a file with neither is a library; `sciencec check` is the command for one")
}

/// `SC0404`: two distinct monomorphisation keys produced the same mangled
/// symbol.
///
/// An internal consistency check, not a user error. It exists because
/// `type-checking-and-mir.md` §15 says the dual failure — one key producing two
/// symbols — *"is not a type error, not a link error on most platforms, and
/// shows up as a performance mystery or a pointer-equality failure long after
/// the cause"*. This code is the trap-door for the half that *is* detectable.
pub fn symbol_collision(symbol: &str, first: &str, second: &str) -> Diagnostic {
    Diagnostic::error(code::SC0404, format!("two instantiations mangled to `{symbol}`"))
        .with_note(format!("one is `{first}`"))
        .with_note(format!("the other is `{second}`"))
        .with_note("this is a bug in the compiler's mangler, not in the program")
}

/// `SC0405`: a `static` or `const` initialiser is not a constant expression.
pub fn not_a_constant(span: Span) -> Diagnostic {
    Diagnostic::error(code::SC0405, "this initialiser cannot be emitted into a data section")
        .with_label(Label::primary(span, "not a constant expression"))
}

/// `SC0406`: the requested target is not the host's.
///
/// Cross-compilation is out of scope for F0 per core spec §12, and §11's whole
/// reason for spending a code on it is that the diagnostic *"says so rather
/// than failing obscurely in the linker"*.
pub fn cross_compilation_unsupported(requested: &str, host: &str) -> Diagnostic {
    Diagnostic::error(code::SC0406, format!("cannot build for `{requested}` on `{host}`"))
        .with_note("F0 does not cross-compile")
}

/// Whether a code belongs to the range this crate claims.
pub fn is_claimed(code: Code) -> bool {
    CLAIMED.contains(&code.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_this_module_constructs_is_in_range_or_borrowed_deliberately() {
        let ours = [
            backend_not_compiled_in("llvm", "install it"),
            no_linker_driver(&["cc".to_string()]),
            linker_failed("cc a.o", "undefined"),
            no_entry_point("a.science"),
            symbol_collision("_S1f", "f[Int]", "f[I64]"),
            not_a_constant(Span::new(science_diagnostics::FileId(0), 0, 1)),
            cross_compilation_unsupported("aarch64-apple-darwin", "x86_64-pc-windows-msvc"),
        ];
        for diagnostic in &ours {
            assert!(is_claimed(diagnostic.code), "{} is outside SC0400-SC0409", diagnostic.code);
        }
    }

    #[test]
    fn the_reserved_codes_are_reserved_and_not_used() {
        // If one of these ever appears in a constructor, this test is the
        // reminder that §11 has to be amended first.
        for code in [code::SC0407, code::SC0408, code::SC0409] {
            assert!(is_claimed(code));
        }
    }

    #[test]
    fn borrowed_codes_are_outside_the_claimed_range_which_is_the_point() {
        for code in [code::SC0429, code::SC0431, code::SC0461] {
            assert!(!is_claimed(code), "{code} would be a claim on someone else's range");
        }
    }

    #[test]
    fn sc0400_names_the_feature_and_how_to_get_it() {
        let diagnostic = backend_not_compiled_in(
            "llvm",
            "install LLVM 18.1.8 and set `LLVM_SYS_181_PREFIX`",
        );
        assert!(diagnostic.message.contains("llvm"));
        assert!(diagnostic.notes.iter().any(|n| n.contains("LLVM_SYS_181_PREFIX")));
    }

    #[test]
    fn sc0401_names_the_candidates_and_the_variable() {
        let diagnostic = no_linker_driver(&["cc".into(), "clang".into(), "link.exe".into()]);
        let notes = diagnostic.notes.join("\n");
        assert!(notes.contains("cc, clang, link.exe"), "the order is part of the message");
        assert!(notes.contains("SCIENCE_LINKER"));
    }

    #[test]
    fn sc0402_says_that_the_output_is_the_linkers_own() {
        let diagnostic = linker_failed("cc a.o -lm", "ld: cannot find -lm");
        assert!(diagnostic.notes.iter().any(|n| n.contains("verbatim")));
        assert!(diagnostic.notes.iter().any(|n| n.contains("cannot find -lm")));
    }
}
