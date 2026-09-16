//! `link-rt` — the Link runtime.
//!
//! This crate is statically linked into every binary `linkc` produces. It is
//! the whole of what runs beneath a Link program: allocation over the system
//! allocator, `panic` with a message and abort, and the memory representation
//! of every type in the F0 standard library (§8 of the F0 design).
//!
//! Everything below is the contract between this crate and `link-codegen`. It
//! is written so that codegen can be emitted from this page alone, without
//! reading a single function body.
//!
//! # 1. There is no garbage collector and no reference counting
//!
//! Ownership is verified at compile time (§6.1), so the runtime never decides
//! when to free anything. It frees when told, and only then. No value in this
//! crate implements Rust's `Drop`; [`LinkString`], [`LinkArray`] and
//! [`LinkMap`] are plain `Copy` records of pointers and lengths, and moving one
//! is a bitwise copy with no bookkeeping whatsoever. That is exactly Link's
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
//! [`link_string_new`], [`link_string_clone`], [`link_string_truncate`],
//! [`link_array_new`], [`link_map_new`], [`link_string_chars`],
//! [`link_read_file`]. Every one of those is three words or more, which both
//! the System V x86-64 and the Windows x64 conventions classify as MEMORY:
//! the caller passes a hidden pointer to the return slot and the callee writes
//! through it. Codegen must emit those calls with an `sret` parameter rather
//! than an LLVM `ret { ptr, i64, i64 }`, or the two sides will disagree about
//! where the value lives. [`LinkIoResultUnit`] is the exception: at two bytes
//! it comes back in a register on both conventions.
//!
//! # 3. Pointer conventions
//!
//! | Form in Link | Form in the ABI |
//! |---|---|
//! | `&T` | `*const T`, never null, always valid and aligned |
//! | `&mut T` | `*mut T`, never null, always valid and aligned, no aliasing |
//! | `T` passed by move into the runtime | `*const u8` to a slot the caller has initialised; after the call the caller must treat that slot as moved-from and must not drop it |
//! | `T` moved out of the runtime | `*mut u8` out-parameter, written only when the return value says so |
//!
//! `&mut` is never aliased, because §6.1 rule 4 forbids it. Several entry
//! points here would be unsound under aliasing — `link_string_push_str(s, s)`
//! is the clearest example — and they do not check for it. They rely on the
//! borrow checker, which is the component whose entire job that is.
//!
//! # 4. `Int` is `I64`
//!
//! §5.1 makes `Int` an alias for `I64`, so every length and index that is an
//! `Int` in Link is an `i64` here. Sizes and capacities internal to the
//! runtime, which never surface in a Link signature, are `usize`.
//!
//! # 5. The layout of `Option[T]` and `Result[T, E]`
//!
//! This is an ABI commitment. Both the type checker and codegen depend on it,
//! and changing it later breaks every previously compiled object file.
//!
//! ## 5.1 The general rule for enums
//!
//! A Link enum is laid out as a C struct of a **discriminant** followed by a
//! **union of the variant payloads**:
//!
//! - The discriminant is the smallest unsigned integer that can hold the
//!   variant count: `u8` up to 256 variants, `u16` beyond. Every enum in the
//!   F0 standard library has two variants, so every discriminant is a `u8`.
//! - Discriminant values follow **declaration order**. From §8 that gives
//!   [`LINK_OPTION_SOME`]` == 0`, [`LINK_OPTION_NONE`]` == 1`,
//!   [`LINK_RESULT_OK`]` == 0` and [`LINK_RESULT_ERR`]` == 1`.
//! - The discriminant sits at offset 0. The payload union follows at the next
//!   offset that is a multiple of the union's alignment. The enum's alignment
//!   is the maximum of the discriminant's and the union's; its size is rounded
//!   up to that alignment.
//! - An enum whose variants all carry no payload is just the discriminant.
//!   `IoError` is one such: see [`LinkIoError`].
//! - A payload of type `()` is zero-sized and contributes nothing to the
//!   union, so `Result[(), IoError]` is two bytes with alignment 1. See
//!   [`LinkIoResultUnit`].
//!
//! ## 5.2 The niche rule
//!
//! The general rule is overridden when **exactly one variant carries a
//! payload, every other variant carries none, and that payload has a niche** —
//! a bit pattern its type can never hold. Then the enum is represented as the
//! payload alone, and the payload-free variants are encoded in unused niche
//! values. No discriminant is materialised at all.
//!
//! In F0 the niche-carrying types are the ones that are never null:
//!
//! | Type | Representation | Niche |
//! |---|---|---|
//! | `Box[T]` | `*mut T` | the null pointer |
//! | `&T` | `*const T` | the null pointer |
//! | `&mut T` | `*mut T` | the null pointer |
//!
//! Therefore:
//!
//! - `Option[Box[T]]` is one pointer, with `None` represented as null. It costs
//!   **not one bit more** than `Box[T]`.
//! - `Option[&T]` and `Option[&mut T]` are likewise one pointer, `None` null.
//! - `Option[Int]`, `Option[String]`, `Option[Array[T]]` and every other
//!   payload without a niche fall back to the tagged form of §5.1. There is no
//!   niche in an integer, and `String`'s pointer is never null even when the
//!   string is empty (§7 below), so `String` deliberately offers none either.
//! - `Result[T, E]` **never** gets the niche treatment, because both of its
//!   variants carry a payload. It is always tagged.
//!
//! Two payload-free variants would need two niche values; F0 has no enum of
//! that shape in the standard library, but the rule generalises: the `n`
//! payload-free variants take the first `n` values of the niche, in declaration
//! order.
//!
//! ## 5.3 How the runtime hands back an `Option`
//!
//! The runtime is not generic over Link types, so for an unknown `T` it cannot
//! assemble the tagged layout of §5.1: it does not know `T`'s alignment at the
//! point where it would have to place the payload. Two conventions follow, and
//! codegen materialises the enum itself.
//!
//! - **Returning `Option[&T]` or `Option[&mut T]`** — the runtime returns the
//!   pointer, possibly null. That *is* the niche representation of §5.2, so
//!   codegen uses the returned value as the `Option` directly, with no
//!   conversion at all. [`link_array_get`], [`link_array_get_mut`] and
//!   [`link_map_get`] work this way.
//! - **Returning an owned `Option[T]`** — the runtime returns a `bool` and
//!   writes the payload through a `*mut u8` out-parameter that the caller
//!   supplies. `true` means the payload was written and is now the caller's to
//!   own; `false` means the out slot was **not touched** and the answer is
//!   `None`. [`link_array_pop`], [`link_map_insert`], [`link_map_remove`] and
//!   [`link_chars_next`] work this way.
//!
//! Where both `T` and `E` are concrete the runtime does build the enum itself:
//! [`link_read_file`] and [`link_write_file`] return [`LinkIoResultString`] and
//! [`LinkIoResultUnit`] by value, laid out exactly per §5.1.
//!
//! # 6. The element-descriptor convention
//!
//! This is the crux of the design, so it is worth stating at length.
//!
//! `Array[T]` and `Map[K, V]` are generic in Link, and §5.3 says generics are
//! monomorphised. The obvious consequence would be one copy of the container
//! per instantiation, emitted by codegen. This crate does the opposite: **one
//! implementation, parameterised at run time by a description of the element
//! type.** The container stores raw bytes and is told, on every call, how big
//! an element is, how it is aligned, and how to destroy one.
//!
//! The description is [`LinkTypeInfo`]: `{ size, align, drop_fn }`.
//! `drop_fn` is a nullable function pointer — null means the type needs no
//! destructor, which is the common case and which lets the runtime skip the
//! per-element loop entirely. (Note that a nullable function pointer is itself
//! the niche rule of §5.2 at work.) For `Map`, [`LinkMapInfo`] adds a
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
//! `size` may be zero (Link's `()` is a zero-sized type, and `Array[()]` and
//! `Map[K, ()]` are legal); the runtime handles that without allocating.
//! `align` must be a power of two.
//!
//! Elements are moved **bitwise**. Growing an `Array` or rehashing a `Map`
//! relocates every element with a `memcpy` and runs no destructor and no clone,
//! because relocating an owned value is a move (§6.1 rule 2) and a move in Link
//! has no user-visible behaviour. `drop_fn` runs in exactly three places: when
//! a container is freed while still holding elements, when `Map::insert`
//! replaces a key, and when `Map::remove` discards the key whose value it
//! yields.
//!
//! # 7. An empty container allocates nothing, and is never null
//!
//! [`link_string_new`], [`link_array_new`] and [`link_map_new`] make no call to
//! the system allocator. Their pointers are *dangling but well aligned and
//! non-null* — the integer value of the alignment, with no provenance —
//! exactly like an empty `Vec`. Codegen may therefore assume that the `ptr`
//! field of a live `String` or `Array` is never null, which is what makes
//! `Option[&T]`'s null niche unambiguous.
//!
//! # 8. Names
//!
//! Every symbol is `link_`-prefixed and matches its Link spelling:
//! `Array::push` is `link_array_push`. Entry points with no counterpart in §8
//! exist because codegen needs them and are marked **codegen support** in their
//! own documentation: drop glue (`link_*_free`), literal construction
//! (`link_string_from_bytes`), and the capacity hints.
//!
//! # 9. What §8 asks for that has no symbol here
//!
//! Three parts of §8's library are deliberately absent from this crate, because
//! putting them here would cost a function call for something codegen can emit
//! as a handful of instructions.
//!
//! - **`Option`'s and `Result`'s methods.** `is_some`, `is_ok`, `unwrap` and
//!   `unwrap_or` are each a discriminant test over the layout of §5, and
//!   `unwrap` on the wrong variant is a call to [`link_panic_bytes`] with a
//!   static message. Everything codegen needs to emit them is written down in
//!   §5; nothing needs to be called.
//! - **The `?` operator** (§4.4) is the same discriminant test plus an early
//!   return, and a `From` conversion on the error path. Also codegen's.
//! - **`Iterate`'s method set.** §8 names the trait and says `Chars` implements
//!   `Iterate[Char]`, but never spells out the trait's methods, so there was
//!   nothing to conform to. [`link_chars_next`] is this crate's proposal: one
//!   method, `next`, following the owned-`Option` convention of §5.3. If the
//!   type checker settles on a different shape for `Iterate`, that function is
//!   the only thing that has to move.
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
