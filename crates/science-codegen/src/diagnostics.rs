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

    /// A generic function instantiates itself at a larger type without end.
    ///
    /// **This spends one of §11's three reserved codes, and the spend is a
    /// decision rather than an accident.** §11 holds `SC0407`-`SC0409` for
    /// *"the ABI classifier of §4.3, which will want at least one code for an
    /// aggregate shape it cannot classify"*, and reserving three was called
    /// *"cheaper than reopening the partition"*. Taking the first of the three
    /// leaves two, which is twice what the sentence that reserved them asks
    /// for.
    ///
    /// **The reason it is taken from here and not from a neighbour.** The
    /// README's partition gives every other number in `SC0400`-`SC0499` to
    /// another note, and the codegen band's free list is empty. The condition
    /// is general codegen — a monomorphisation walk that does not terminate is
    /// not an FFI question, not a Python question and not a linking question —
    /// so the alternatives were a code from somebody else's topic or no code at
    /// all, and an `unimplemented!` is not a diagnostic.
    ///
    /// **The cost, stated because §11 is the authority and this crate cannot
    /// amend it:** §11's table still says three reserved and should say two,
    /// and `docs/` is not edited here for the reason the module note gives.
    /// Whoever maintains that row owes it one line.
    pub const SC0407: Code = Code(407);
    /// Reserved for the ABI classifier of §4.3. Unused.
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

    /// `type-checking-and-mir.md`'s: a generic function across the C boundary.
    /// Referenced, not claimed.
    ///
    /// Decision 18: *"a generic Science function may be **called** from an FFI
    /// wrapper, but a function declared in an `extern` block is monomorphic,
    /// and a generic function may not be passed as a C callback without an
    /// explicit instantiation. The error is `SC0522` and it names the
    /// instantiation to write."*
    ///
    /// **It is emitted from here because this is the phase that can see the
    /// condition, and `science-types` says so itself.** That crate's `codes`
    /// module reserves `SC0522` and declines to define it: *"`SC0522` is a
    /// declaration check — a generic function reached through an `extern` block
    /// — and needs the monomorphiser's view of which instantiations cross."*
    /// This is the monomorphiser. That crate's own test that `SC0521` and
    /// `SC0522` are absent from *its* list still passes; §11's *"amended, not
    /// claimed"* rule and this crate's existing `SC0429`, `SC0431` and `SC0461`
    /// are the precedent, and the README's own words are *"a band is a topic,
    /// not a crate"*.
    pub const SC0522: Code = Code(522);
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

/// `SC0407`: a generic function instantiates itself at a larger type forever.
///
/// **The diagnostic names the chain, not a depth.** `f of T` calling
/// `f of (Array of T)` produces `f[Int]`, `f[Array of Int]`,
/// `f[Array of (Array of Int)]` and so on, and a message that says *"recursion
/// limit reached (128)"* tells the reader the compiler gave up without telling
/// them which call to look at. The chain is the evidence: every link is a call
/// site the walk actually took, and the reader can see the growth by reading
/// down the list.
///
/// `growth` is the index of the link inside which the walk found an earlier
/// link's arguments — the *cause* — or `None` when the chain was cut by the
/// depth backstop instead, in which case the chain is still printed and the
/// note says which of the two rules fired. [`crate::mono`]'s §6 is why there
/// are two rules and what the second one is for.
pub fn cyclic_instantiation(chain: &[String], growth: Option<usize>, span: Span) -> Diagnostic {
    let last = chain.last().map(String::as_str).unwrap_or("this function");
    let mut diagnostic = Diagnostic::error(
        code::SC0407,
        format!("`{last}` instantiates itself at a larger type without end"),
    )
    .with_label(Label::primary(span, "instantiated from here"))
    .with_note("the instantiation chain, outermost first:");
    for (at, link) in chain.iter().enumerate() {
        // The chain is the message, and a chain of sixty-five links is not a
        // message. The head and the tail are where the answer is — the head
        // says where the growth started and the tail says what it grew into —
        // and the middle is the same shape repeated, so it is counted rather
        // than printed. The link `growth` names is always shown: it is the
        // cause, and eliding the cause would leave a message that says less
        // than a depth number would have.
        if let Some(skipped) = elided(chain.len(), at, growth) {
            if skipped > 0 {
                diagnostic = diagnostic.with_note(format!("  ... {skipped} more ..."));
            }
            continue;
        }
        let marker = match growth {
            Some(index) if index == at => "  <- this one contains an earlier one",
            _ => "",
        };
        diagnostic = diagnostic.with_note(format!("  {at}. {link}{marker}"));
    }
    diagnostic = match growth {
        Some(_) => diagnostic.with_note(
            "each instantiation is strictly larger than the one it came from, so the set of \
             copies to emit is infinite",
        ),
        None => diagnostic.with_note(
            "the chain stopped growing only because the walk did; the containment rule did not \
             fire, so the links above are the whole of what it can say",
        ),
    };
    diagnostic.with_note("give the recursive call a concrete type argument, or make it a loop")
}

/// Whether a chain link is elided, and how many links this one stands for.
///
/// `Some(0)` means *"elided, and an earlier note already said how many"*;
/// `Some(n)` means *"elided, and this is where the count goes"*. Splitting it
/// that way keeps the caller a single pass over the chain.
fn elided(length: usize, at: usize, growth: Option<usize>) -> Option<usize> {
    const HEAD: usize = 4;
    const TAIL: usize = 4;
    if length <= HEAD + TAIL + 1 {
        return None;
    }
    if at < HEAD || at + TAIL >= length || growth == Some(at) {
        return None;
    }
    if at == HEAD {
        return Some(length - HEAD - TAIL);
    }
    Some(0)
}

/// `SC0522`: a generic function passed across the C boundary.
///
/// `type-checking-and-mir.md` Decision 18 requires that the message *"names the
/// instantiation to write"*, and this is the half of the obligation that cannot
/// be met today: **Science has no syntax for an explicit type argument at a use
/// site.** The README lists *"explicit type arguments at call sites"* as a
/// standing cross-note ask with two claimants and no decision, so there is no
/// spelling to name. The note says so rather than inventing one, because a
/// suggestion the compiler would then reject is worse than no suggestion.
///
/// [`code::SC0522`] says why the code is emitted from this crate.
pub fn generic_across_c_boundary(function: &str, extern_fn: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        code::SC0522,
        format!("`{function}` is generic and cannot be passed to `{extern_fn}` as a callback"),
    )
    .with_label(Label::primary(
        span,
        "a C function pointer has one address; a generic has one per instantiation",
    ))
    .with_note(format!(
        "`{extern_fn}` is declared in an `extern` block, so its parameters are C types"
    ))
    .with_note(
        "this needs an explicit instantiation, and Science has no syntax for one yet — write a \
         non-generic wrapper that calls it at the type you want, and pass the wrapper",
    )
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
            cyclic_instantiation(
                &["f[Int]".to_string(), "f[Array of Int]".to_string()],
                Some(1),
                Span::new(science_diagnostics::FileId(0), 0, 1),
            ),
        ];
        for diagnostic in &ours {
            assert!(is_claimed(diagnostic.code), "{} is outside SC0400-SC0409", diagnostic.code);
        }
    }

    #[test]
    fn the_two_still_reserved_codes_are_reserved_and_not_used() {
        // `SC0407` is spent — see its own documentation for the argument and
        // the amendment it owes §11. If either of the other two ever appears
        // in a constructor, this test is the reminder that §11 has to be
        // amended first.
        for code in [code::SC0408, code::SC0409] {
            assert!(is_claimed(code));
        }
    }

    #[test]
    fn sc0407_names_the_chain_and_not_a_depth() {
        let chain = vec![
            "reduce[Int]".to_string(),
            "reduce[Array of Int]".to_string(),
            "reduce[Array of (Array of Int)]".to_string(),
        ];
        let span = Span::new(science_diagnostics::FileId(0), 0, 1);
        let diagnostic = cyclic_instantiation(&chain, Some(2), span);
        let notes = diagnostic.notes.join("\n");
        for link in &chain {
            assert!(notes.contains(link.as_str()), "the chain is the message: {notes}");
        }
        assert!(
            !notes.contains("128"),
            "a depth number is what the chain replaces: {notes}"
        );
    }

    #[test]
    fn a_long_chain_keeps_its_head_its_tail_and_its_cause() {
        // A sixty-five-link chain is not a message. What has to survive is the
        // start, the end, the count of what was dropped, and the link that
        // caused it.
        let chain: Vec<String> = (0..40).map(|n| format!("step[{n}]")).collect();
        let span = Span::new(science_diagnostics::FileId(0), 0, 1);
        let diagnostic = cyclic_instantiation(&chain, Some(20), span);
        let notes = diagnostic.notes.join("\n");
        assert!(notes.contains("step[0]"), "{notes}");
        assert!(notes.contains("step[39]"), "{notes}");
        assert!(notes.contains("step[20]"), "the cause is never elided: {notes}");
        assert!(notes.contains("more ..."), "{notes}");
        assert!(!notes.contains("step[15]"), "the middle is counted, not printed: {notes}");
        assert!(diagnostic.notes.len() < 20, "{} notes", diagnostic.notes.len());
    }

    #[test]
    fn a_short_chain_is_printed_whole() {
        let chain: Vec<String> = (0..5).map(|n| format!("f[{n}]")).collect();
        let span = Span::new(science_diagnostics::FileId(0), 0, 1);
        let diagnostic = cyclic_instantiation(&chain, Some(4), span);
        let notes = diagnostic.notes.join("\n");
        for link in &chain {
            assert!(notes.contains(link.as_str()), "{notes}");
        }
        assert!(!notes.contains("more ..."), "{notes}");
    }

    #[test]
    fn sc0522_is_borrowed_and_says_what_cannot_be_written() {
        let span = Span::new(science_diagnostics::FileId(0), 0, 1);
        let diagnostic = generic_across_c_boundary("identity", "qsort", span);
        assert_eq!(diagnostic.code, code::SC0522);
        assert!(!is_claimed(diagnostic.code), "SC0522 is `science-types`' band, referenced only");
        let notes = diagnostic.notes.join("\n");
        assert!(notes.contains("no syntax for one yet"), "{notes}");
        assert!(notes.contains("wrapper"), "the message has to name a way out: {notes}");
    }

    #[test]
    fn borrowed_codes_are_outside_the_claimed_range_which_is_the_point() {
        for code in [code::SC0429, code::SC0431, code::SC0461, code::SC0522] {
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
