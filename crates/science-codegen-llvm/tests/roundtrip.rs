//! Check 2 of `sys.rs` §1, end to end: the module this backend builds is fed
//! back to **LLVM 18's own parser**, through `clang -x ir`.
//!
//! **The decision.** The IR text is written to a file and assembled by `clang`
//! into an object. Nothing about the text is matched; the assertion is that LLVM
//! accepts it.
//!
//! **The reason.** `LLVMVerifyModule` and `LLVMPrintModuleToString` both run
//! inside the same process, against the same in-memory module, through the same
//! `extern` declarations. If a declaration's arity were wrong, the module those
//! two agree about could still be one the *parser* rejects — the printer writes
//! what the in-memory IR says, and the in-memory IR is what the possibly-wrong
//! call built. Handing the text to a second process closes that: `clang` was
//! built from the headers this machine does not have, so it is the only reader
//! on the machine that knows what the C API's callers were supposed to produce.
//!
//! **The cost.** It needs `clang` on the machine as well as the DLL, and it is
//! slow by this crate's standards — two processes and a file. It is worth both:
//! this is the check `sys.rs` §1 names as *"the one that catches ABI mistakes,
//! because it compares behaviour and not spelling"*, and until this file existed
//! it was a plan rather than a check.

#![cfg(feature = "llvm")]

use std::process::Command;

use science_codegen::backend::Backend;
use science_codegen::descriptor::StringLiteral;
use science_codegen::layout::Triple;
use science_codegen::target::{OptLevel, TargetConfig};
use science_codegen_llvm::emit::LlvmBackend;
use science_codegen_llvm::lower::runtime_signatures;

/// Every runtime entry point declared, plus a string literal, in one module.
///
/// **All 45, not the four `hello` calls.** A declaration is only checked by the
/// calls that build it, and this is the cheapest way to exercise every shape
/// `AbiSignature` can take on this target: nine `sret` returns, the `bool`
/// returns, the pointer returns, the `Never` of `science_panic_bytes`, and the
/// two-byte aggregate of `science_write_file` that must **not** be `sret`.
fn declare_everything() -> LlvmBackend {
    let triple = Triple::host().expect("a supported host");
    let config = TargetConfig::new(triple, OptLevel::O0);
    let mut backend = LlvmBackend::new();
    backend.begin_module("roundtrip", &config).expect("a module");
    backend
        .define_string_bytes(&StringLiteral::new(0, "hello, world"))
        .expect("a string literal");
    for signature in runtime_signatures(triple) {
        backend.declare_function(&signature).expect("a runtime declaration");
    }
    backend.verify().expect("the module verifies");
    backend
}

fn clang() -> Option<std::path::PathBuf> {
    for prefix in science_codegen_llvm::llvm_prefixes() {
        let path =
            prefix.join("bin").join(if cfg!(windows) { "clang.exe" } else { "clang" });
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

#[test]
fn llvms_own_parser_accepts_every_declaration_this_backend_writes() {
    let Some(clang) = clang() else {
        eprintln!("skipped: no `clang` beside an LLVM installation");
        return;
    };
    let backend = declare_everything();
    let ir = backend.ir();
    assert!(ir.contains("declare"), "the module has no declarations in it:\n{ir}");

    let dir = std::env::temp_dir().join(format!("science-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let source = dir.join("module.ll");
    let object = dir.join("module.o");
    std::fs::write(&source, &ir).expect("the IR is writable");

    let output = Command::new(&clang)
        .arg("-x")
        .arg("ir")
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("`clang` runs");
    assert!(
        output.status.success(),
        "LLVM 18's own parser rejected the IR this backend printed. That is a wrong `extern` \
         declaration, not a wrong program.\n--- clang ---\n{}{}\n--- the module ---\n{ir}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(object.is_file(), "`clang` reported success and wrote no object");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The nine `sret` entry points carry the attribute, and the tenth aggregate
/// does not.
///
/// **§9.2's finding, from the side that can be checked.**
/// `science_codegen::runtime::RuntimeFn::needs_sret` derives the set rather than
/// listing it, which is that module's whole argument; this asserts that the
/// derivation survives the trip through [`LlvmBackend::declare_function`] and
/// into the IR. `science_write_file` is the control: it returns a two-byte
/// aggregate, which comes back in a register on every convention, and an `sret`
/// on it would be exactly as wrong as a missing one on the nine.
#[test]
fn the_sret_set_reaches_the_ir_and_write_file_is_not_in_it() {
    let backend = declare_everything();
    let ir = backend.ir();
    let sret_symbols = [
        "science_string_new",
        "science_string_from_bytes",
        "science_string_clone",
        "science_string_truncate",
        "science_string_chars",
        "science_array_new",
        "science_array_with_capacity",
        "science_map_new",
        "science_read_file",
    ];
    for symbol in sret_symbols {
        let line = ir
            .lines()
            .find(|line| line.contains(&format!("@{symbol}(")))
            .unwrap_or_else(|| panic!("`{symbol}` is not declared:\n{ir}"));
        assert!(
            line.contains("sret("),
            "`{symbol}` returns an aggregate by value and its declaration has no `sret`. \
             §9.2's failure mode is exactly this and it corrupts a register:\n{line}"
        );
    }
    let write_file = ir
        .lines()
        .find(|line| line.contains("@science_write_file("))
        .expect("`science_write_file` is declared");
    assert!(
        !write_file.contains("sret("),
        "`science_write_file` returns two bytes, which come back in a register on every \
         convention; an `sret` here is as wrong as a missing one above:\n{write_file}"
    );
}

/// Decision 6, on every declaration without exception.
#[test]
fn every_runtime_declaration_is_nounwind() {
    let backend = declare_everything();
    let ir = backend.ir();
    let declarations: Vec<&str> =
        ir.lines().filter(|line| line.starts_with("declare ")).collect();
    assert_eq!(
        declarations.len(),
        science_codegen::runtime::RUNTIME.len(),
        "the module does not declare all {} runtime entry points",
        science_codegen::runtime::RUNTIME.len()
    );
    for line in declarations {
        assert!(
            line.contains('#'),
            "a declaration carries no attribute group, so Decision 6's `nounwind` is not on \
             it:\n{line}"
        );
    }
    assert!(ir.contains("attributes #0 = { nounwind }"), "{ir}");
}
