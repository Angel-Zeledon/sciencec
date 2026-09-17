//! `science-codegen` — everything a code generator decides before it names a
//! backend.
//!
//! # 0. Read this first: LLVM is not installed, and that shaped the crate
//!
//! `codegen-and-linking.md` Decision 1 makes this crate's backend call LLVM's C
//! API through `llvm-sys`, and Decision 2 pins LLVM to 18.1.8. Decision 1's
//! cost item 5 names the consequence: *"`llvm-config` becomes a build-time
//! requirement for building `sciencec`. On Windows it means an LLVM
//! installation and an `LLVM_SYS_181_PREFIX` environment variable."*
//!
//! On the machine this crate was written on, that requirement is not met.
//! There is no `llvm-config` on `PATH`, no `LLVM_SYS_181_PREFIX`, and no LLVM
//! installation anywhere the search looked. `rustc` reports `LLVM version:
//! 21.1.2`, but that is LLVM linked into `rustc`, not an SDK: it ships no
//! `llvm-config`, no headers and no importable libraries, and it is the wrong
//! major version besides.
//!
//! So this crate is **the half of the code generator that does not need LLVM**,
//! and it is that half on purpose rather than by accident. Decision 42 already
//! required the split:
//!
//! > *`MIR -> backend` is an interface, and LLVM is its first implementation.
//! > `science-codegen` splits in two: a target-independent half owning layout,
//! > the ABI classification of §4, mangling, the monomorphisation walk and
//! > drop-glue construction; and a backend half owning nothing but the
//! > translation of an already-decided lowering into a particular IR.
//! > Everything two backends would have to agree on lives above the line.*
//!
//! **The decision this crate makes:** build the above-the-line half first,
//! completely, with a stub backend ([`stub::TextBackend`]) standing in for
//! LLVM, rather than build nothing until LLVM is installed.
//!
//! **The reason:** the above-the-line half is where the expensive mistakes are.
//! §4.1's failure mode — *"not a compile error, not a link error, a `dgemm`
//! that returns numbers"* — lives in layout and ABI classification, both of
//! which are target-independent and neither of which needs LLVM to be written
//! or tested. Decision 42 says that putting them below the line is *"§4's
//! silent-corruption failure mode with two more places for it to happen"*, and
//! a machine with no LLVM is the one machine on which it is impossible to make
//! that mistake by accident.
//!
//! **The cost, and it is the whole cost:** no Science program has been
//! compiled. Nothing here has emitted an object file, driven a linker or run.
//! §10's stage 0 is *not* complete, stage 1 is not started, and the claim
//! "hello world works" is not made anywhere in this crate. What is complete is
//! the set of answers a backend would have had to get right, written down in a
//! form that a test can check and a second backend can be held to.
//!
//! **What is missing, exactly.** An LLVM 18.1 development installation — the
//! `llvm-config` binary, the `llvm-c` headers and the link archives. On
//! Windows: `winget install LLVM.LLVM --version 18.1.8`, or
//! `choco install llvm --version=18.1.8`, then set `LLVM_SYS_181_PREFIX` to the
//! install root. On Debian, `apt install llvm-18-dev`; on macOS,
//! `brew install llvm@18`. Nothing here vendors LLVM, fakes it, or provides a
//! fallback code generator, because each of those is a worse answer than saying
//! which package is absent.
//!
//! # 1. What is above the line, and why each item is there
//!
//! Decision 42's table, item by item, with the module that owns it:
//!
//! | Above the line | Module |
//! |---|---|
//! | Layout: sizes, alignments, offsets, niches (§3) | [`layout`] |
//! | ABI classification: `sret`, by-pointer arguments, parameter attributes (§4) | [`abi`] |
//! | Mangling and the symbol table (§2.7, Decision 4, `SC0404`) | [`mangle`] |
//! | The monomorphisation walk (§2.7, Decision 4) | [`mono`] |
//! | Descriptor contents and the drop-glue rule (§2.5, §3.5) | [`descriptor`] |
//! | Which operations are runtime calls (§2.6) | [`runtime`] |
//! | The float policy's obligations (§7.3) | [`target`] |
//! | The interface itself | [`backend`] |
//!
//! Below the line there is one implementation, [`stub::TextBackend`], which
//! emits a deterministic text transcript instead of machine code. It exists to
//! make the above-the-line decisions **observable**: a test can read the
//! transcript and assert that no fast-math flag was emitted, that `a * b + c`
//! became an `fmul` and an `fadd` and not an `fma`, and that a call returning
//! `ScienceString` carried an `sret` parameter.
//!
//! # 2. This is not a lowering of the language, and one module is the exception
//!
//! **This section used to open with *"there is no type checker in this
//! workspace and no MIR"*, and that is no longer true.** `science-types` and
//! `science-mir` are built, `sciencec check` runs the whole front half, and the
//! thing that was missing when this crate was written — the input a
//! monomorphisation walk consumes — is here. [`mono`] is the walk, and it is
//! the one module in this crate that names the front end.
//!
//! The seam the rest of the crate keeps is unchanged, and the argument for it
//! is unchanged with it:
//!
//! - **Layout, ABI classification, descriptors, the runtime table, mangling and
//!   the target configuration are defined over [`layout::CgTy`]**, a small type
//!   model that MIR lowers *into*. That is the correct seam and not a
//!   workaround: Decision 42 puts layout and ABI above the line precisely so
//!   that they answer questions about *types*, not about syntax, and a layout
//!   engine that reaches into the HIR is a layout engine a second backend
//!   cannot reuse. `tests/independence.rs` is that claim as a test, module by
//!   module, because a seam enforced by a reviewer is §15's eroding interface.
//! - **[`mono`] is the exception, and it is one by definition rather than by
//!   convenience.** Decision 42's table puts *"the monomorphisation walk"*
//!   above the line, and the question it answers — *which instantiations does
//!   this program's MIR reach* — is spelled in `science-mir`'s bodies, the
//!   checker's `Ty` and the resolver's `DefTable`. `Cargo.toml` is the full
//!   argument and `mono`'s §4 is why the answer cannot be routed through
//!   [`layout::CgTy`] on the way.
//! - **There is still no `lower_hir` function and no drop elaboration here.**
//!   Drop elaboration is `science-mir`'s, by Decision 13. A `Ty -> CgTy`
//!   lowering does not exist, which is what keeps the drop glue of Decision 12
//!   and the descriptors of Decision 20 out of [`mono`]; that module's §10 says
//!   so and says what it would need.
//! - The narrow program the note's stage 1 names — `print("hello, world")` — is
//!   present as a **lowering shape** in [`backend`]'s instruction set and in
//!   `tests/stage_one.rs`, not as a compiled binary. The instruction set is
//!   exactly as large as stages 0 to 2 need and no larger, which is the note's
//!   own discipline applied to an interface instead of to a backend.
//!
//! # 3. What `science-rt`'s "emittable from this page alone" claim looked like
//!    from here
//!
//! §9 of the note tested that claim by writing §§2–4 from the runtime's module
//! documentation and then checking against the signatures, and found six
//! defects. This crate was written from the same page with §9's findings in
//! hand, so it is not an independent test of the claim — it is a test of
//! whether §9's repairs are *enough*, which is a different and more useful
//! question. Three of the six are now facts a test checks rather than sentences
//! a reader must notice, and they are checked in [`runtime`]:
//!
//! - **Finding 1** (`science_array_with_capacity` missing from §2's `sret`
//!   list) is discharged by [`runtime::RUNTIME`] carrying a return type per
//!   entry point and `tests/runtime_abi.rs` classifying all 45 rather than
//!   copying a list. A list maintained by hand is what failed; a list derived
//!   from signatures cannot fail the same way.
//!
//!   **And deriving it found the same defect again, on a second symbol.** The
//!   derived set is nine and §2's repaired list is eight: the ninth is
//!   `science_string_from_bytes`, which returns `ScienceString` by value, is
//!   named in §8 as *"literal construction"* and nowhere in §2. It is worse
//!   than the original because it is the call a string literal lowers to under
//!   Decision 15, so it is the **first runtime call hello world makes** — and
//!   because §10's stage 1 of `codegen-and-linking.md` already says the `sret`
//!   convention is needed *"immediately, because `science_string_from_bytes`
//!   returns `ScienceString` by value"*. The fact is written down in the note
//!   and missing from the list the note tells a code generator to read. See
//!   [`runtime`] for the full account; this crate does not edit `science-rt`.
//! - **Finding 2** (the three function-pointer signatures are not on the page)
//!   is discharged by [`runtime::EMITTED_FN_SIGNATURES`], which is the one
//!   place in this workspace where the signatures codegen must *emit* — not
//!   call — are written down as a codegen input.
//! - **Finding 4** (the descriptor's parameter position is unstated and
//!   inconsistent) is discharged by [`runtime::RuntimeFn::descriptor_index`],
//!   which reads the position out of the parameter list instead of out of a
//!   convention. `science_box_new` and `science_box_free` put the descriptor
//!   first; every container entry point puts it second; and a test asserts it,
//!   so nobody has to remember.
//!
//! **Finding 3** (§4's `usize` rule is false of the interface it describes) is
//! answered by making it impossible to write: [`layout::IntTy::Usize`] resolves
//! against the target's pointer width and is a different type from
//! [`layout::IntTy::I64`], and [`runtime::RtParam`] distinguishes `Usize` from
//! `Int`. On the three targets F0 supports they are the same width, which is
//! exactly why a codegen that conflated them would be correct by accident; the
//! type model refuses the accident.
//!
//! **Findings 5 and 6 are not fixed and cannot be fixed here.** Finding 5 is
//! that the runtime has no program entry point, no `science_exit`, and no
//! symbol that writes to stderr without aborting — so `script-mode.md` §2.3's
//! contract (print `error: `, exit **1**) has nothing to lower to. §9.4 is
//! right that a hello world can be emitted from the page and a `main` that
//! returns an error cannot, and this crate ran into the same wall from the
//! other side: [`runtime::EXIT_CONTRACT`] is the record of what is missing and
//! it names no symbol, because there is none. Finding 6 is the
//! `cap == 0 => len == 0` invariant, and this crate's response is Decision 15,
//! implemented as a rule with no escape hatch: [`descriptor::StringLiteral`]
//! can only produce the `science_string_from_bytes` form. There is no
//! constructor for a static `ScienceString` at all, because the unsound
//! representation is the one an optimising author reaches for on purpose.
//!
//! **A seventh, which §9 did not find and this crate ran into.** §5.3 says an
//! owned `T?` comes back as a `bool` plus a `*mut u8` out-parameter, and adds
//! that *"the returned `bool` is the same byte as the discriminant of §5.1, so
//! when `T` has no niche codegen may store it straight into the tag"*. That is
//! true, and it is a trap when `T` **does** have a niche: for `(Box of T)?` the
//! answer is the niche form, there is no tag to store the byte into, and
//! codegen must instead branch on the `bool` and materialise a null pointer on
//! the false edge — while the out-parameter slot is documented as *untouched*,
//! so reading it unconditionally reads uninitialised memory.
//! `science_array_pop` on an `Array of (Box of T)` is that case and it is not
//! exotic. The page's sentence is a licence for one of the two cases and reads
//! like a licence for both. [`runtime::OwnedNullableReturn`] states both.
//!
//! # 4. The float policy, which is the one obligation LLVM defaults against
//!
//! `reproducibility.md` Decision 3 is unconditional and §7.3 names three
//! obligations that implement it. All three live in [`target`], and the second
//! is the one that bites:
//!
//! 1. Never set a fast-math flag. [`backend::FloatOp`] has no way to carry one,
//!    which is stronger than a convention.
//! 2. `AllowFPOpFusion = Strict` on every target machine. LLVM's
//!    `TargetOptions` defaults it to `Standard`, so a backend that does not
//!    touch the field contracts `a * b + c` into an `fma` at every optimisation
//!    level on every FMA-capable machine. [`target::TargetConfig`] has no
//!    constructor that produces anything but [`target::FpContract::Strict`].
//! 3. No LLVM intrinsic for a transcendental, because LLVM constant-folds
//!    `llvm.sin` using the build host's libm. [`target::Intrinsic`] is the
//!    whitelist of eleven IEEE-exact operations and nothing else can be spelled.
//!
//! What could be tested here and what could not is in `tests/float_policy.rs`,
//! and the file says so at the top: the decision is testable, the
//! `TargetOptions` field is not, because setting it requires LLVM.
//!
//! # 5. Diagnostics
//!
//! `SC0400`–`SC0409`, the whole of the free General codegen sub-range, and
//! nothing else. See [`diagnostics`]. `SC0400` is the code this crate uses
//! most, because "a toolchain feature required to build this program is not
//! compiled into this `sciencec`" is the literal state of the compiler.
//!
//! Two changes came with [`mono`] and both are recorded at the constant rather
//! than only here. `SC0407` — one of §11's three codes *"reserved for the ABI
//! classifier of §4.3"* — is **spent** on a monomorphisation walk that does not
//! terminate, leaving two reserved where the sentence that reserved them asks
//! for one; §11's table should say two and this crate cannot amend it. And
//! `SC0522` — `type-checking-and-mir.md`'s Decision 18 — is **referenced, not
//! claimed**, from the phase that `science-types` says is the one able to see
//! the condition.

#![warn(missing_docs)]

pub mod abi;
pub mod backend;
pub mod descriptor;
pub mod diagnostics;
pub mod driver;
pub mod layout;
pub mod mangle;
pub mod mono;
pub mod runtime;
pub mod stub;
pub mod target;

pub use layout::{CgTy, Layout, Triple};
pub use target::TargetConfig;
