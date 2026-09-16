//! The layout engine against the only fixed ABI in the project.
//!
//! **What this file is.** Every one of `science-rt`'s eight `#[repr(C)]`
//! aggregates, laid out twice: once by [`science_codegen::layout::layout_of`]
//! from the rules of §3, and once by `rustc` from the real Rust type. The two
//! answers must agree.
//!
//! **Why it is worth more than the unit tests in `layout.rs`.** Those check the
//! engine against its own author's reading of the note. This one checks it
//! against a fixed C ABI nobody in this crate controls, computed by a different
//! compiler, over types that `science-rt`'s own `tests/layout.rs` already pins.
//! §4.1's failure mode is *"not a compile error, not a link error, a `dgemm`
//! that returns numbers"*, and the only defence against it that does not depend
//! on the defender being right is a differential test.
//!
//! **What it does not prove.** It compares against **Rust's** `#[repr(C)]`,
//! which is Rust's implementation of the C rule, not a C compiler's. Decision
//! 23 asks for the real thing — *"a generated corpus compiled by `clang -S
//! -emit-llvm` and by `sciencec`, with the parameter and return attributes
//! compared"* — and that needs `clang`, which needs LLVM, which is not
//! installed. This is the strongest test available without it, and it is
//! strictly weaker than the one Decision 23 requires.

use std::mem::{align_of, offset_of, size_of};

use science_codegen::layout::{Repr, Triple, layout_of};
use science_codegen::runtime::RtAggregate;
use science_rt::{
    SCIENCE_NULLABLE_NULL, SCIENCE_NULLABLE_PRESENT, ScienceArray, ScienceChars, ScienceMap,
    ScienceMapInfo, ScienceNullableIoError, ScienceString, ScienceStringAndIoError, ScienceTypeInfo,
};

/// The host triple. These comparisons are only meaningful against the target
/// `rustc` compiled the runtime for, which is the host.
fn host() -> Triple {
    Triple::host().expect("this test needs a triple F0 supports; see `Triple::host`")
}

macro_rules! agrees {
    ($aggregate:expr, $rust:ty) => {{
        let computed = $aggregate.layout(host());
        assert_eq!(
            computed.size,
            size_of::<$rust>() as u64,
            "{}: size disagrees with the Rust type",
            $aggregate.name()
        );
        assert_eq!(
            computed.align,
            align_of::<$rust>() as u64,
            "{}: alignment disagrees with the Rust type",
            $aggregate.name()
        );
        computed
    }};
}

#[test]
fn every_runtime_aggregate_has_the_size_and_alignment_rustc_gives_it() {
    agrees!(RtAggregate::String, ScienceString);
    agrees!(RtAggregate::Chars, ScienceChars);
    agrees!(RtAggregate::Array, ScienceArray);
    agrees!(RtAggregate::Map, ScienceMap);
    agrees!(RtAggregate::TypeInfo, ScienceTypeInfo);
    agrees!(RtAggregate::MapInfo, ScienceMapInfo);
    agrees!(RtAggregate::NullableIoError, ScienceNullableIoError);
    agrees!(RtAggregate::StringAndIoError, ScienceStringAndIoError);
}

#[test]
fn every_field_is_where_decision_17_puts_it() {
    // Sizes agreeing is necessary and not sufficient: two layouts with the same
    // total can put a field in different places, and a field offset is what a
    // `getelementptr` uses. `{ u8, u64, u8 }` would pass the size check under
    // Rust's reordering rule and fail this one.
    let string = layout_of(host(), &RtAggregate::String.cg_ty());
    assert_eq!(string.field_offset(0), Some(offset_of!(ScienceString, ptr) as u64));
    assert_eq!(string.field_offset(1), Some(offset_of!(ScienceString, len) as u64));
    assert_eq!(string.field_offset(2), Some(offset_of!(ScienceString, cap) as u64));

    let chars = layout_of(host(), &RtAggregate::Chars.cg_ty());
    assert_eq!(chars.field_offset(0), Some(offset_of!(ScienceChars, ptr) as u64));
    assert_eq!(chars.field_offset(1), Some(offset_of!(ScienceChars, len) as u64));
    assert_eq!(chars.field_offset(2), Some(offset_of!(ScienceChars, offset) as u64));

    let array = layout_of(host(), &RtAggregate::Array.cg_ty());
    assert_eq!(array.field_offset(0), Some(offset_of!(ScienceArray, ptr) as u64));
    assert_eq!(array.field_offset(1), Some(offset_of!(ScienceArray, len) as u64));
    assert_eq!(array.field_offset(2), Some(offset_of!(ScienceArray, cap) as u64));

    let map = layout_of(host(), &RtAggregate::Map.cg_ty());
    assert_eq!(map.field_offset(0), Some(offset_of!(ScienceMap, states) as u64));
    assert_eq!(map.field_offset(1), Some(offset_of!(ScienceMap, keys) as u64));
    assert_eq!(map.field_offset(2), Some(offset_of!(ScienceMap, values) as u64));
    assert_eq!(map.field_offset(3), Some(offset_of!(ScienceMap, len) as u64));
    assert_eq!(map.field_offset(4), Some(offset_of!(ScienceMap, tombstones) as u64));
    assert_eq!(map.field_offset(5), Some(offset_of!(ScienceMap, cap) as u64));

    let info = layout_of(host(), &RtAggregate::TypeInfo.cg_ty());
    assert_eq!(info.field_offset(0), Some(offset_of!(ScienceTypeInfo, size) as u64));
    assert_eq!(info.field_offset(1), Some(offset_of!(ScienceTypeInfo, align) as u64));
    assert_eq!(info.field_offset(2), Some(offset_of!(ScienceTypeInfo, drop_fn) as u64));

    let map_info = layout_of(host(), &RtAggregate::MapInfo.cg_ty());
    assert_eq!(map_info.field_offset(0), Some(offset_of!(ScienceMapInfo, key) as u64));
    assert_eq!(map_info.field_offset(1), Some(offset_of!(ScienceMapInfo, value) as u64));
    assert_eq!(map_info.field_offset(2), Some(offset_of!(ScienceMapInfo, hash_fn) as u64));
    assert_eq!(map_info.field_offset(3), Some(offset_of!(ScienceMapInfo, eq_fn) as u64));

    // §5.4's pair: a plain struct, both fields live at once, no tag.
    let pair = layout_of(host(), &RtAggregate::StringAndIoError.cg_ty());
    assert_eq!(pair.field_offset(0), Some(offset_of!(ScienceStringAndIoError, value) as u64));
    assert_eq!(pair.field_offset(1), Some(offset_of!(ScienceStringAndIoError, error) as u64));
}

#[test]
fn a_nullable_function_pointer_costs_one_word_and_not_two() {
    // `ScienceTypeInfo.drop_fn` is `Option<ScienceDropFn>` in Rust and
    // `(fn ptr)?` in the model. §5.2's niche rule is what makes them the same
    // size, and the runtime page points at it: "note that a nullable function
    // pointer is itself the niche rule of §5.2 at work". A layout engine that
    // tagged it would make `ScienceTypeInfo` 32 bytes and every descriptor
    // wrong.
    assert_eq!(size_of::<ScienceTypeInfo>(), 24);
    let info = layout_of(host(), &RtAggregate::TypeInfo.cg_ty());
    assert_eq!(info.size, 24);
    let Repr::Aggregate { fields } = &info.repr else { panic!("not an aggregate") };
    assert!(matches!(fields[2].layout.repr, Repr::Niched { .. }));
    assert_eq!(fields[2].layout.size, 8);
}

#[test]
fn nullable_io_error_is_two_bytes_and_takes_the_discriminant_not_a_niche() {
    // The runtime's §8.1 gives the reasoning, and Decision 19 agrees: a niche
    // exists only where the *type* guarantees the bit pattern is unreachable,
    // and an integer newtype with five named constants guarantees nothing of
    // the sort, because the next version of the enum has a sixth.
    let layout = layout_of(host(), &RtAggregate::NullableIoError.cg_ty());
    assert_eq!(layout.size, size_of::<ScienceNullableIoError>() as u64);
    assert_eq!(layout.size, 2);
    match &layout.repr {
        Repr::Tagged { tag, payload_offset, variants } => {
            assert_eq!(tag.width(host()), 1);
            assert_eq!(*payload_offset, offset_of!(ScienceNullableIoError, error) as u64);
            // Discriminant values follow declaration order from zero, and the
            // runtime fixes which is which.
            assert_eq!(variants[0].discriminant, u64::from(SCIENCE_NULLABLE_NULL));
            assert_eq!(variants[1].discriminant, u64::from(SCIENCE_NULLABLE_PRESENT));
        }
        other => panic!("expected the tagged form, got {other:?}"),
    }
}

#[test]
fn the_nullable_discriminants_are_the_runtimes_own_constants() {
    // `CgTy::Nullable` desugars to a two-variant choice with `null` first. If
    // anyone reverses that, every presence test in every emitted program is
    // exactly backwards — and, as `abi.rs` says at length, a rename would have
    // invited precisely that.
    assert_eq!(SCIENCE_NULLABLE_NULL, 0);
    assert_eq!(SCIENCE_NULLABLE_PRESENT, 1);
}

#[test]
fn the_pair_absorbs_its_error_byte_into_padding_the_alignment_forced_anyway() {
    // The runtime's `ScienceNullableIoError` documentation claims this in
    // passing: "the second byte is absorbed by the tail padding the pair's
    // eight-byte alignment forces anyway, so the niched form would have
    // produced a pair of exactly the same size". Checked, because it is the
    // justification for not taking the niche.
    let pair = layout_of(host(), &RtAggregate::StringAndIoError.cg_ty());
    let string = layout_of(host(), &RtAggregate::String.cg_ty());
    assert_eq!(pair.size, string.size + 8);
    assert_eq!(pair.align, 8);
    // A one-byte error would have produced the same 32 bytes.
    assert_eq!(pair.size, 32);
}

#[test]
fn a_zero_sized_element_is_legal_and_allocates_nothing() {
    // §6: "`size` may be zero (Science's `()` is a zero-sized type, and
    // `Array[()]` and `Map[K, ()]` are legal); the runtime handles that without
    // allocating." The layout side of that promise.
    let info = science_codegen::descriptor::type_info(
        host(),
        &science_codegen::layout::CgTy::Unit,
        None,
    );
    assert_eq!(info.size, 0);
    assert_eq!(info.align, 1, "§6: `align` must be a power of two, and at least one");
    assert!(info.align.is_power_of_two());
}

#[test]
fn every_descriptor_alignment_the_engine_can_produce_is_a_power_of_two() {
    // §6 states it as a precondition on the descriptor, and a descriptor is
    // codegen's own output, so it is codegen's obligation.
    use science_codegen::layout::{CgTy, FloatTy, IntTy, PtrKind};
    let types = [
        CgTy::Unit,
        CgTy::Bool,
        CgTy::Char,
        CgTy::Int(IntTy::I8),
        CgTy::Int(IntTy::I64),
        CgTy::Int(IntTy::Usize),
        CgTy::Float(FloatTy::F32),
        CgTy::Float(FloatTy::F64),
        CgTy::Ptr(PtrKind::Box),
        CgTy::Interface,
        CgTy::nullable(CgTy::Int(IntTy::I64)),
        CgTy::nullable(CgTy::Ptr(PtrKind::Borrow)),
        RtAggregate::String.cg_ty(),
        RtAggregate::Map.cg_ty(),
        RtAggregate::StringAndIoError.cg_ty(),
    ];
    for triple in Triple::ALL {
        for ty in &types {
            let layout = layout_of(triple, ty);
            assert!(layout.align.is_power_of_two(), "{ty:?} on {triple:?}");
            assert!(layout.align >= 1);
            assert_eq!(layout.size % layout.align, 0, "size must be a multiple of alignment");
        }
    }
}

#[test]
fn the_layout_is_the_same_on_all_three_targets_for_every_runtime_type() {
    // Not a law of the language, but a fact about F0's three targets: all are
    // 64-bit little-endian with the same scalar alignments, so the runtime's
    // types have one layout. Asserting it is what will fail loudly on the day
    // somebody adds a 32-bit target, which is the day `usize` stops being
    // `i64` and §9.3's finding 3 stops being theoretical.
    for aggregate in RtAggregate::ALL {
        let layouts: Vec<_> = Triple::ALL.iter().map(|t| aggregate.layout(*t)).collect();
        assert!(
            layouts.windows(2).all(|w| w[0] == w[1]),
            "{} differs between F0's targets",
            aggregate.name()
        );
    }
}
