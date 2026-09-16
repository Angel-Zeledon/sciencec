//! `science-rt` — the Science runtime.
//!
//! This crate is statically linked into every binary `sciencec` produces. It is
//! the whole of what runs beneath a Science program: allocation over the system
//! allocator, `panic` with a message and abort, and the memory representation
//! of every type in the F0 standard library (§8 of the F0 design).
//!
//! Everything below is the contract between this crate and `science-codegen`. It
//! is written so that codegen can be emitted from this page alone, without
//! reading a single function body.
//!
//! # 1. There is no garbage collector and no reference counting
//!
//! Ownership is verified at compile time (§6.1), so the runtime never decides
//! when to free anything. It frees when told, and only then. No value in this
//! crate implements Rust's `Drop`; [`ScienceString`], [`ScienceArray`] and
//! [`ScienceMap`] are plain `Copy` records of pointers and lengths, and moving one
//! is a bitwise copy with no bookkeeping whatsoever. That is exactly Science's
//! move semantics (§6.1 rule 2), expressed in the representation.
//!
//! The consequence, stated plainly so nobody later mistakes it for an
//! oversight: **calling a `_free` entry point twice, or not at all, is a bug
//! this crate will not catch.** There is deliberately no drop flag, no
//! canary, no poisoning of freed handles. The borrow checker exists to prevent
//! precisely those bugs, and a runtime safety net would hide the very failures
//! that prove it is working.
//!
//! # 2. Every symbol is `extern "C"` and every type is `#[repr(C)]`
//!
//! No Rust layout assumption appears anywhere in this interface. Field order is
//! declaration order, padding is the C rule, and every type's size, alignment
//! and field offsets are pinned by `tests/layout.rs`.
//!
//! **Aggregate returns follow the platform C ABI, not LLVM's structural
//! return.** Several entry points return a struct by value —
//! [`science_string_new`], [`science_string_clone`], [`science_string_truncate`],
//! [`science_array_new`], [`science_array_with_capacity`], [`science_map_new`],
//! [`science_string_chars`],
//! [`science_read_file`]. Every one of those is three words or more, which both
//! the System V x86-64 and the Windows x64 conventions classify as MEMORY:
//! the caller passes a hidden pointer to the return slot and the callee writes
//! through it. Codegen must emit those calls with an `sret` parameter rather
//! than an LLVM `ret { ptr, i64, i64 }`, or the two sides will disagree about
//! where the value lives. [`ScienceNullableIoError`] is the exception: at two
//! bytes it comes back in a register on both conventions.
//!
//! `science_array_with_capacity` was missing from that list until
//! `codegen-and-linking.md` was written against this page and checked the
//! result against the signatures. It returns `ScienceArray` by value — three
//! words — exactly like `science_array_new` beside it, and a generator that
//! trusted the list would have emitted a structural return for it. That is
//! silent memory corruption rather than a compile or link failure, which is the
//! failure mode this whole section exists to prevent, and it is the reason the
//! list is now checked by a test rather than maintained by hand.
//!
//! # 3. Pointer conventions
//!
//! | Form in Science | Form in the ABI |
//! |---|---|
//! | `&T` | `*const T`, never null, always valid and aligned |
//! | `&mut T` | `*mut T`, never null, always valid and aligned, no aliasing |
//! | `T` passed by move into the runtime | `*const u8` to a slot the caller has initialised; after the call the caller must treat that slot as moved-from and must not drop it |
//! | `T` moved out of the runtime | `*mut u8` out-parameter, written only when the return value says so |
//!
//! `&mut` is never aliased, because §6.1 rule 4 forbids it. Several entry
//! points here would be unsound under aliasing — `science_string_push_str(s, s)`
//! is the clearest example — and they do not check for it. They rely on the
//! borrow checker, which is the component whose entire job that is.
//!
//! # 4. `Int` is `I64`
//!
//! §5.1 makes `Int` an alias for `I64`, so every length and index that is an
//! `Int` in Science is an `i64` here. Sizes and capacities internal to the
//! runtime, which never surface in a Science signature, are `usize`.
//!
//! # 5. The layout of `T?` and of a pair return
//!
//! This is an ABI commitment. Both the type checker and codegen depend on it,
//! and changing it later breaks every previously compiled object file.
//!
//! ## 5.1 The general rule for enums
//!
//! A Science enum is laid out as a C struct of a **discriminant** followed by a
//! **union of the variant payloads**:
//!
//! - The discriminant is the smallest unsigned integer that can hold the
//!   variant count: `u8` up to 256 variants, `u16` beyond. `T?` has two cases
//!   and `IoError` has five, so every discriminant in this crate is a `u8`.
//! - Discriminant values follow **declaration order**. `T?` is built in rather
//!   than declared, so its two values are fixed here instead:
//!   [`SCIENCE_NULLABLE_NULL`]` == 0` and [`SCIENCE_NULLABLE_PRESENT`]` == 1`.
//!   That constant's own documentation says why null is the zero.
//! - The discriminant sits at offset 0. The payload union follows at the next
//!   offset that is a multiple of the union's alignment. The enum's alignment
//!   is the maximum of the discriminant's and the union's; its size is rounded
//!   up to that alignment.
//! - An enum whose variants all carry no payload is just the discriminant.
//!   `IoError` is one such: see [`ScienceIoError`].
//! - A payload of type `()` is zero-sized and contributes nothing to the
//!   union, so a variant carrying one costs the discriminant alone.
//!
//! ## 5.2 The niche rule
//!
//! The general rule is overridden when **exactly one variant carries a
//! payload, every other variant carries none, and that payload has a niche** —
//! a bit pattern its type can never hold. Then the enum is represented as the
//! payload alone, and the payload-free variants are encoded in unused niche
//! values. No discriminant is materialised at all. `T?` is the two-case
//! instance of that shape and the one that matters most: the present case
//! carries the payload and `null` carries none.
//!
//! In F0 the niche-carrying types are the ones that are never null, and **this
//! table is closed**. Decision 6 of `type-checking-and-mir.md` §4.1 grants the
//! null niche to pointer-like types and says a `T` with no niche gets a
//! discriminant byte; nothing grants a niche to anything else, and inventing
//! one — a reserved code point in a small enum, say — would be a representation
//! no design note licenses.
//!
//! | Type | Representation | Niche |
//! |---|---|---|
//! | `Box[T]` | `*mut T` | the null pointer |
//! | `borrowed T` | `*const T` | the null pointer |
//! | `mutable borrowed T` | `*mut T` | the null pointer |
//!
//! Therefore:
//!
//! - `(Box[T])?` is one pointer, with `null` represented as the null pointer.
//!   It costs **not one bit more** than `Box[T]`.
//! - `(borrowed T)?` and `(mutable borrowed T)?` are likewise one pointer, with
//!   `null` again the null pointer.
//! - `Int?`, `String?`, `(Array[T])?`, `IoError?` and every other payload
//!   without a niche fall back to the tagged form of §5.1. There is no niche in
//!   an integer; `String`'s pointer is never null even when the string is empty
//!   (§7 below), so `String` deliberately offers none either; and `IoError` is a
//!   plain byte, which [`ScienceNullableIoError`] states at length because it is
//!   the case where the temptation to invent a niche is strongest.
//! - A **pair** return never gets the niche treatment, and never gets a
//!   discriminant either, because it is not an enum. See §5.4.
//!
//! Two payload-free variants would need two niche values; F0 has no enum of
//! that shape in the standard library, but the rule generalises: the `n`
//! payload-free variants take the first `n` values of the niche, in declaration
//! order.
//!
//! ## 5.3 How the runtime hands back a `T?`
//!
//! The runtime is not generic over Science types, so for an unknown `T` it cannot
//! assemble the tagged layout of §5.1: it does not know `T`'s alignment at the
//! point where it would have to place the payload. Two conventions follow, and
//! codegen materialises the nullable itself.
//!
//! - **Returning `(borrowed T)?` or `(mutable borrowed T)?`** — the runtime
//!   returns the pointer, possibly null. That *is* the niche representation of
//!   §5.2, so codegen uses the returned value as the `T?` directly, with no
//!   conversion at all. [`science_array_get`], [`science_array_get_mut`] and
//!   [`science_map_get`] work this way.
//! - **Returning an owned `T?`** — the runtime returns a `bool` and
//!   writes the payload through a `*mut u8` out-parameter that the caller
//!   supplies. `true` means the payload was written and is now the caller's to
//!   own; `false` means the out slot was **not touched** and the answer is
//!   `null`. [`science_array_pop`], [`science_map_insert`], [`science_map_remove`] and
//!   [`science_chars_next`] work this way. The returned `bool` is the same byte
//!   as the discriminant of §5.1, so when `T` has no niche codegen may store it
//!   straight into the tag.
//!
//! Where every type involved is concrete the runtime does build the aggregate
//! itself: [`science_read_file`] returns [`ScienceStringAndIoError`] and
//! [`science_write_file`] returns [`ScienceNullableIoError`], by value, laid out
//! exactly per §5.1 and §5.4.
//!
//! ## 5.4 Pairs, which are how a function fails
//!
//! `syntax-revision-2.md` §3 makes a fallible function return its value *and* a
//! nullable error: `-> (T, E?)`. A pair is a **plain struct, and both fields are
//! live at once**. There is no tag choosing between them, no union, and nothing
//! for a niche to disambiguate. Field order is declaration order and padding is
//! the C rule of §2, exactly as for any other aggregate.
//!
//! Two consequences codegen must get right, and neither has an analogue in what
//! this replaced:
//!
//! - **The presence test reads the second field, not a tag.** `err?` is the
//!   niche test or the discriminant load of §5.2 applied to that field alone.
//!   The first field is not consulted and has no bearing on it.
//! - **The value half is live on the failing path too, so the caller owns it
//!   there too.** A failing `read_file` still hands back a `String`, and that
//!   `String` still has to be freed. The runtime returns the empty string, which
//!   allocates nothing (§7), so the cost is a call codegen must emit rather than
//!   memory anyone must find — but omitting the call is a leak on the error
//!   path, which is the path least likely to be exercised.
//!
//! A pair whose first element is `()` degenerates to its second element alone,
//! by the zero-sized-payload rule of §5.1: [`science_write_file`] returns
//! [`ScienceNullableIoError`] and not a struct wrapping one.
//!
//! # 6. The element-descriptor convention
//!
//! This is the crux of the design, so it is worth stating at length.
//!
//! `Array[T]` and `Map[K, V]` are generic in Science, and §5.3 says generics are
//! monomorphised. The obvious consequence would be one copy of the container
//! per instantiation, emitted by codegen. This crate does the opposite: **one
//! implementation, parameterised at run time by a description of the element
//! type.** The container stores raw bytes and is told, on every call, how big
//! an element is, how it is aligned, and how to destroy one.
//!
//! The description is [`ScienceTypeInfo`]: `{ size, align, drop_fn }`.
//! `drop_fn` is a nullable function pointer — null means the type needs no
//! destructor, which is the common case and which lets the runtime skip the
//! per-element loop entirely. (Note that a nullable function pointer is itself
//! the niche rule of §5.2 at work.) For `Map`, [`ScienceMapInfo`] adds a
//! descriptor for the value type and the two function pointers the table needs:
//! `hash_fn` and `eq_fn`, both supplied by codegen from the key type's `Eq`
//! implementation.
//!
//! **Codegen emits one `static` descriptor per monomorphised instantiation and
//! passes a pointer to it on every call.** The descriptor is *not* stored in
//! the container: `Array[T]` stays three words and `Map[K, V]` six, so moving
//! one is as cheap as moving a `String`, which matters because F1 will move
//! them between actors.
//!
//! The price is a hard precondition, and it is the one place where a codegen
//! bug turns into silent memory corruption rather than a compile error:
//!
//! > **Every call touching a given container must pass the same descriptor
//! > contents that were passed when it was created.** Passing a descriptor for
//! > a different type reinterprets the container's bytes as that type. Nothing
//! > detects this.
//!
//! `size` may be zero (Science's `()` is a zero-sized type, and `Array[()]` and
//! `Map[K, ()]` are legal); the runtime handles that without allocating.
//! `align` must be a power of two.
//!
//! Elements are moved **bitwise**. Growing an `Array` or rehashing a `Map`
//! relocates every element with a `memcpy` and runs no destructor and no clone,
//! because relocating an owned value is a move (§6.1 rule 2) and a move in Science
//! has no user-visible behaviour. `drop_fn` runs in exactly three places: when
//! a container is freed while still holding elements, when `Map::insert`
//! replaces a key, and when `Map::remove` discards the key whose value it
//! yields.
//!
//! # 7. An empty container allocates nothing, and is never null
//!
//! [`science_string_new`], [`science_array_new`] and [`science_map_new`] make no call to
//! the system allocator. Their pointers are *dangling but well aligned and
//! non-null* — the integer value of the alignment, with no provenance —
//! exactly like an empty `Vec`. Codegen may therefore assume that the `ptr`
//! field of a live `String` or `Array` is never null, which is what makes
//! `(borrowed T)?`'s null niche unambiguous.
//!
//! # 8. Names
//!
//! Every symbol is `science_`-prefixed and matches its Science spelling:
//! `Array::push` is `science_array_push`. Entry points with no counterpart in §8
//! exist because codegen needs them and are marked **codegen support** in their
//! own documentation: drop glue (`science_*_free`), literal construction
//! (`science_string_from_bytes`), and the capacity hints.
//!
//! This paragraph said `link_`-prefixed until it was checked against the crate:
//! all 45 exported symbols are `science_`-prefixed and always were, and the
//! sentence contradicted its own next example. It is recorded rather than
//! quietly corrected because this page invites a code generator to be emitted
//! *from it alone*, without reading a function body — which is the one reading
//! under which the error was fatal rather than cosmetic. Every fossil of that
//! draft is now gone: the `LINK_OPTION_*` and `LINK_RESULT_*` constants went
//! with the types they described, and the map's slot bytes were renamed to
//! `SCIENCE_MAP_SLOT_*`. **There is one prefix in this crate and it is
//! `science_`**, which is the only form of this sentence a code generator can
//! be emitted from safely.
//!
//! `science_print` and `science_write` were also the wrong way round until the
//! same check: `science_print` wrote its bytes verbatim and `science_println`
//! added the newline, while revision 2 §3.5 gives `print` the newline and makes
//! `write` the form that adds nothing. A generator lowering Science's `print`
//! to a symbol of that name would have produced output with no line breaks in
//! it — a defect that compiles, links and runs. The symbols now match the
//! Science spellings they implement, and `science_println` no longer exists.
//!
//! # 8.1 What on this page was stale, and is not any more
//!
//! This crate was written against the pre-revision-2 error model, and §5 and §9
//! described it for as long as its replacement was undecided. `Option[T]` is
//! now `T?` and `Result[T, E]` is gone entirely, replaced by the pair
//! `-> (T, E?)` of `syntax-revision-2.md` §3. **Both sections are now written
//! against the new model and `io.rs` implements it.** The record of what was
//! wrong is kept because a page that invites a code generator to be emitted
//! from it alone owes its reader the history of its own errors.
//!
//! **The layout rules survived; the names and one shape did not.** `Option[&T]`
//! became `(borrowed T)?` with the same null niche, so every rule in §5.2 about
//! niches, boxes and pointer payloads held word for word and was kept word for
//! word. `Result[T, E]` became a pair, which is a *different* representation — a
//! tagged union with one live payload against a struct with two — so §5.4 is
//! new, and `io.rs`'s `ScienceIoResultString`, `ScienceIoResultStringPayload`
//! and `ScienceIoResultUnit` are replaced by [`ScienceStringAndIoError`] and
//! [`ScienceNullableIoError`].
//!
//! The one thing that was genuinely blocked was the representation of `T?` for
//! a `T` that is not pointer-like, and Decision 6 of
//! `type-checking-and-mir.md` §4.1 settled it: such a `T` gets a discriminant
//! byte. [`ScienceNullableIoError`] is that decision applied, including the
//! reasoning for **not** taking the niche that `IoError`'s 251 unused code
//! points appear to offer.
//!
//! # 9. What the language asks for that has no symbol here
//!
//! Three things a reader of §8 and of the error model might expect to find here
//! are deliberately absent. Two of them because putting them here would cost a
//! function call for something codegen emits as a handful of instructions; the
//! third because it is not a runtime concern at all.
//!
//! - **The operations on `T?`.** A presence test is the null-pointer test or
//!   the discriminant load of §5.2 — one instruction in either representation —
//!   and a null-coalescing default is that test and a select. Forcing a value
//!   out of a `T?` where the language allows it at all is the same test and a
//!   call to [`science_panic_bytes`] with a static message on the null side.
//!   Everything codegen needs is written down in §5; nothing needs to be called.
//! - **Narrowing** (`type-checking-and-mir.md` §4.2) is a fact the type checker
//!   proves, not a value anything holds. Inside `if err?:` the compiler knows
//!   `err` is non-null and reads the payload without re-testing, and no
//!   representation changes at the boundary — a narrowed `T?` is the same bytes
//!   it always was, read under a stronger fact. Nothing here participates.
//! - **`Iterate`'s method set.** `collections-and-chains.md` gives the interface
//!   one method, `function next(mutable self) -> Self.Item?`, and Decision 15 of
//!   `type-checking-and-mir.md` §6.3 leaves it unchanged, adding failure to a
//!   sibling `TryIterate` instead. [`science_chars_next`] conforms: one `next`,
//!   following the owned-`T?` convention of §5.3. `TryIterate` exists for the
//!   `python:` region and has nothing to do in F0, so it gets no symbol here.
//!
//! What used to stand in this section and no longer does: `Result`'s methods,
//! and a `?` operator that propagated a failure through an invisible `From`
//! conversion. `syntax-revision-2.md` §3 removed the type, re-spelled `?` as the
//! presence test, and made the caller write the conversion at the `return`. The
//! only implicit conversion left in the neighbourhood is a concrete error
//! coercing to `any Error` (Decision 14), which is a box and not a change of
//! value, and which codegen emits without help from here.
//!
//! # 10. There are no threads
//!
//! §8 puts concurrency in F1. Nothing here synchronises anything: no container
//! holds a lock, no counter is atomic, and no entry point is safe to call on
//! two threads at once against the same value. When F1 arrives, an actor owns
//! its own data and messages move between actors, so the containers should not
//! need to change — but that is F1's design to confirm, not an assumption this
//! crate encodes.

#![warn(missing_docs)]
#![warn(unsafe_op_in_unsafe_fn)]

mod abi;
mod array;
mod boxed;
mod io;
mod map;
mod mem;
mod panic;
mod string;

pub use abi::*;
pub use array::*;
pub use boxed::*;
pub use io::*;
pub use map::*;
pub use mem::*;
pub use panic::*;
pub use string::*;
