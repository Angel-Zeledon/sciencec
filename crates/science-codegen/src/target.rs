//! Target configuration, and the three float-policy obligations LLVM defaults
//! against.
//!
//! **The decision.** A [`TargetConfig`] is the only way to describe a target to
//! a backend, it can only be built by [`TargetConfig::new`], and that
//! constructor sets [`FpContract::Strict`] unconditionally. There is no setter,
//! no builder method and no public field.
//!
//! **The reason.** §7.3 obligation 2 is the trap in the whole note:
//!
//! > *LLVM's `TargetOptions` defaults FP contraction to `Standard`, not
//! > `Strict` — meaning the backend may fuse an `fmul` feeding an `fadd` into
//! > an `fma` even when neither instruction carries the `contract` flag. A code
//! > generator that creates a target machine and does not touch that field gets
//! > contraction, at every optimisation level including `-O0`, on any target
//! > with an FMA unit — which is every AArch64 chip and every x86 since
//! > Haswell.*
//!
//! That is a line which is correct *by never being written*, which §0 of the
//! note calls the worst kind of obligation. The mitigation is to make the line
//! impossible to omit: a backend cannot obtain a target description that does
//! not carry `Strict`, so "did anyone remember to set it" is not a question
//! anybody has to ask.
//!
//! **The cost.** `AllowFPOpFusion` becomes unconfigurable, including for the
//! debugging case where somebody wants to see what contraction would have done.
//! That is deliberate: `reproducibility.md` Decision 3 is unconditional, and a
//! flag that turns off a language property is a flag that ends up in somebody's
//! build script. `math.fma(a, b, c)` remains available and is the supported way
//! to ask for the fused operation — *"faster and more deterministic than letting
//! the backend decide per call site"*.
//!
//! # What this module can and cannot test
//!
//! It can test that [`TargetConfig`] carries `Strict`, that
//! [`FloatOp`](crate::backend::FloatOp) cannot
//! spell a fused multiply-add, that [`Intrinsic`] is the eleven-entry whitelist,
//! and that [`contains_fast_math_flag`] finds a flag in emitted text. It
//! **cannot** test that LLVM's `TargetOptions.AllowFPOpFusion` was actually set,
//! because setting it requires LLVM. `tests/float_policy.rs` says which half is
//! which, and names the test to add the day `science-codegen-llvm` exists.

use crate::layout::Triple;

/// LLVM's `FPOpFusion` enumeration, mirrored.
///
/// Mirrored rather than reduced to a boolean because the names are the ones a
/// reader will find in LLVM's `TargetOptions.h`, and a codegen that spelled it
/// `allow_fma: bool` would be one rename away from meaning the opposite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpContract {
    /// Never fuse. The only value Science uses.
    Strict,
    /// Fuse within a single expression, as C permits. **LLVM's default**, and
    /// the reason this enum exists rather than a comment.
    Standard,
    /// Fuse anywhere.
    Fast,
}

impl FpContract {
    /// The name as it appears in LLVM's C API (`LLVMCodeGenOpt`-adjacent
    /// spelling), for a backend to map.
    pub fn llvm_name(self) -> &'static str {
        match self {
            FpContract::Strict => "Strict",
            FpContract::Standard => "Standard",
            FpContract::Fast => "Fast",
        }
    }
}

/// Optimisation level (Decision 33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
}

impl Default for OptLevel {
    /// `-O2`.
    ///
    /// §7.3's cost note for Decision 8 gives the reason: *"`-O0` exists for
    /// stepping through a program, and §7 makes `-O2` the default so that
    /// nobody encounters `-O0` by accident."* `-O0` output is alloca-heavy and
    /// round-trips every local through memory.
    fn default() -> OptLevel {
        OptLevel::O2
    }
}

impl OptLevel {
    /// LLVM's new-pass-manager pipeline string.
    pub fn pipeline(self) -> &'static str {
        match self {
            OptLevel::O0 => "default<O0>",
            OptLevel::O1 => "default<O1>",
            OptLevel::O2 => "default<O2>",
            OptLevel::O3 => "default<O3>",
        }
    }

    /// The flag as a user spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            OptLevel::O0 => "-O0",
            OptLevel::O1 => "-O1",
            OptLevel::O2 => "-O2",
            OptLevel::O3 => "-O3",
        }
    }
}

/// Which CPU to target (Decision 38).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetCpu(String);

impl TargetCpu {
    /// The triple's baseline. Never `native`.
    pub fn baseline(triple: Triple) -> TargetCpu {
        TargetCpu(triple.baseline_cpu().to_string())
    }

    /// A named CPU, from `--target-cpu=NAME`.
    ///
    /// `native` is accepted and **resolved to a concrete name here**, which is
    /// Decision 38's small idea: *"a build record that says `"cpu": "native"`
    /// is a record of nothing, and one that says `"cpu": "znver4"` is a record
    /// somebody can rebuild from."* Resolution needs the host's feature set,
    /// which needs the backend, so this function takes the resolved name as a
    /// parameter rather than pretending it can compute it; a caller with no
    /// backend gets the baseline and a note saying so.
    pub fn named(name: &str) -> TargetCpu {
        TargetCpu(name.to_string())
    }

    /// The concrete CPU name that goes in the `[build]` table.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the unresolved `native` spelling, which must never reach
    /// a build record.
    pub fn is_unresolved_native(&self) -> bool {
        self.0 == "native"
    }
}

/// Everything a backend is told about the target.
///
/// The fields are private and there is one constructor. See the module
/// documentation for why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetConfig {
    triple: Triple,
    cpu: TargetCpu,
    opt: OptLevel,
    fp_contract: FpContract,
    no_noalias: bool,
}

impl TargetConfig {
    /// A target configuration. `fp_contract` is always [`FpContract::Strict`].
    pub fn new(triple: Triple, opt: OptLevel) -> TargetConfig {
        TargetConfig {
            triple,
            cpu: TargetCpu::baseline(triple),
            opt,
            // Decision 36. Not a default, not a parameter, not a setter.
            fp_contract: FpContract::Strict,
            no_noalias: false,
        }
    }

    /// The default configuration for a host build: the host triple at `-O2`.
    pub fn for_host() -> Option<TargetConfig> {
        Triple::host().map(|triple| TargetConfig::new(triple, OptLevel::default()))
    }

    /// Select a CPU other than the baseline.
    pub fn with_cpu(mut self, cpu: TargetCpu) -> TargetConfig {
        self.cpu = cpu;
        self
    }

    /// Suppress the `noalias` attribute on exclusive borrows.
    ///
    /// §4.4's unsupported debugging flag: *"so that 'is this a region bug or a
    /// codegen bug' is one recompile rather than a week"*. It is not a
    /// supported build setting and it changes no answer a correct program
    /// computes; a program whose answer changes when it is set has a region
    /// bug, which is exactly what it is for.
    pub fn with_no_noalias(mut self, no_noalias: bool) -> TargetConfig {
        self.no_noalias = no_noalias;
        self
    }

    /// The target triple.
    pub fn triple(&self) -> Triple {
        self.triple
    }

    /// The target CPU.
    pub fn cpu(&self) -> &TargetCpu {
        &self.cpu
    }

    /// The optimisation level.
    pub fn opt(&self) -> OptLevel {
        self.opt
    }

    /// The FP contraction setting. Always [`FpContract::Strict`].
    pub fn fp_contract(&self) -> FpContract {
        self.fp_contract
    }

    /// Whether `noalias` is suppressed.
    pub fn no_noalias(&self) -> bool {
        self.no_noalias
    }
}

/// The seven per-instruction fast-math flags LLVM gates its value-changing
/// float transforms on.
///
/// §7.3's *"good news"*: the optimisation level does not set them, so an `fadd`
/// with no flags is not reassociated at `-O3` and the loop vectoriser refuses a
/// float reduction. Science never sets one, which makes `-O3` exactly as
/// bit-reproducible as `-O0`.
pub const FAST_MATH_FLAGS: [&str; 7] =
    ["nnan", "ninf", "nsz", "arcp", "contract", "afn", "reassoc"];

/// Decision 35's grep, as a function so that the test is three lines.
///
/// > *A test greps the emitted IR of the entire corpus at `-O3` for the
/// > fast-math flag tokens and fails on any hit. Three lines of test for the
/// > language's headline property.*
///
/// Token-matched rather than substring-matched, because `contract` is a
/// substring of nothing in LLVM IR but `nsz` is a substring of a symbol name
/// somebody will eventually write, and a test that fails on a function called
/// `transform_nsz_data` is a test that gets deleted.
pub fn contains_fast_math_flag(ir: &str) -> bool {
    first_fast_math_flag(ir).is_some()
}

/// The first fast-math flag in `ir`, with the line it is on.
///
/// Returning the location rather than a boolean is the difference between "the
/// corpus has a fast-math flag somewhere" and a failure somebody can act on.
pub fn first_fast_math_flag(ir: &str) -> Option<(usize, &'static str)> {
    for (number, line) in ir.lines().enumerate() {
        // Only instruction lines can carry a flag; a comment mentioning one is
        // not a violation, and neither is this crate's own documentation.
        let line = line.split(';').next().unwrap_or(line);
        for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if let Some(flag) = FAST_MATH_FLAGS.iter().find(|f| **f == token) {
                return Some((number + 1, flag));
            }
        }
    }
    None
}

/// The intrinsics Decision 37 whitelists, and the complete set.
///
/// > *A transcendental function lowers to a direct call to `science-libm`'s
/// > symbol, never to an LLVM intrinsic, precisely so that LLVM cannot
/// > constant-fold it against the build host's libm. The exception is the
/// > whitelist.*
///
/// Every entry is marked "IEEE-754 exact" by `intrinsics-math-physics.md` §3.1,
/// which is exactly the property that makes constant-folding them bit-identical
/// on any host. That note wrote the table to decide tier 1 from libcall; this
/// note reads it as a **constant-folding safety whitelist**, which is a use it
/// did not anticipate.
///
/// The failure this prevents is subtle and worth restating: LLVM constant-folds
/// `llvm.sin.f64` of a constant argument *by calling the host's `sin()` at
/// compile time*, so `let x be sin(0.5)` compiled on macOS and on Linux can
/// differ in the last bit, from the same source, at the same optimisation
/// level, with no fast-math flag anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum Intrinsic {
    Sqrt,
    Fabs,
    Copysign,
    Fma,
    Floor,
    Ceil,
    Trunc,
    RoundEven,
    Round,
    Minimum,
    Maximum,
}

impl Intrinsic {
    /// All eleven, in the order `intrinsics-math-physics.md` §3.1 lists them.
    pub const ALL: [Intrinsic; 11] = [
        Intrinsic::Sqrt,
        Intrinsic::Fabs,
        Intrinsic::Copysign,
        Intrinsic::Fma,
        Intrinsic::Floor,
        Intrinsic::Ceil,
        Intrinsic::Trunc,
        Intrinsic::RoundEven,
        Intrinsic::Round,
        Intrinsic::Minimum,
        Intrinsic::Maximum,
    ];

    /// The LLVM intrinsic's base name, without the type suffix.
    pub fn llvm_name(self) -> &'static str {
        match self {
            Intrinsic::Sqrt => "llvm.sqrt",
            Intrinsic::Fabs => "llvm.fabs",
            Intrinsic::Copysign => "llvm.copysign",
            Intrinsic::Fma => "llvm.fma",
            Intrinsic::Floor => "llvm.floor",
            Intrinsic::Ceil => "llvm.ceil",
            Intrinsic::Trunc => "llvm.trunc",
            Intrinsic::RoundEven => "llvm.roundeven",
            Intrinsic::Round => "llvm.round",
            Intrinsic::Minimum => "llvm.minimum",
            Intrinsic::Maximum => "llvm.maximum",
        }
    }

    /// The Science-level name that lowers to this intrinsic.
    pub fn science_name(self) -> &'static str {
        match self {
            Intrinsic::Sqrt => "sqrt",
            Intrinsic::Fabs => "abs",
            Intrinsic::Copysign => "copysign",
            Intrinsic::Fma => "fma",
            Intrinsic::Floor => "floor",
            Intrinsic::Ceil => "ceil",
            Intrinsic::Trunc => "trunc",
            Intrinsic::RoundEven => "round_even",
            Intrinsic::Round => "round",
            Intrinsic::Minimum => "min",
            Intrinsic::Maximum => "max",
        }
    }
}

/// How a `math` function lowers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathLowering {
    /// A whitelisted LLVM intrinsic.
    Intrinsic(Intrinsic),
    /// A direct call to a `science-libm` symbol. Everything else.
    LibmCall(String),
}

/// Decide how a `math` function lowers (Decision 37).
///
/// The default is the libcall, and that is the decision: a name this function
/// does not recognise as whitelisted becomes a call, never an intrinsic. A
/// classifier written the other way round — intrinsic unless known dangerous —
/// would emit `llvm.sin` the first time anyone added `sinh` to the library.
pub fn lower_math_call(name: &str) -> MathLowering {
    match Intrinsic::ALL.iter().find(|i| i.science_name() == name) {
        Some(intrinsic) => MathLowering::Intrinsic(*intrinsic),
        None => MathLowering::LibmCall(format!("science_libm_{name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_no_way_to_build_a_config_that_contracts() {
        for triple in Triple::ALL {
            for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2, OptLevel::O3] {
                let config = TargetConfig::new(triple, opt);
                assert_eq!(
                    config.fp_contract(),
                    FpContract::Strict,
                    "{triple:?} at {opt:?} must be Strict, not LLVM's Standard default"
                );
                // The builder methods must not be able to change it either.
                let config = config
                    .with_cpu(TargetCpu::named("znver4"))
                    .with_no_noalias(true);
                assert_eq!(config.fp_contract(), FpContract::Strict);
            }
        }
    }

    #[test]
    fn the_default_optimisation_level_is_o2() {
        assert_eq!(OptLevel::default(), OptLevel::O2);
    }

    #[test]
    fn the_default_cpu_is_the_baseline_and_never_native() {
        for triple in Triple::ALL {
            let config = TargetConfig::new(triple, OptLevel::O2);
            assert_eq!(config.cpu().as_str(), triple.baseline_cpu());
            assert!(!config.cpu().is_unresolved_native());
        }
        assert_eq!(TargetCpu::baseline(Triple::X86_64LinuxGnu).as_str(), "x86-64");
        assert_eq!(TargetCpu::baseline(Triple::Aarch64AppleDarwin).as_str(), "apple-m1");
    }

    #[test]
    fn unresolved_native_is_detectable_so_it_never_reaches_a_build_record() {
        assert!(TargetCpu::named("native").is_unresolved_native());
        assert!(!TargetCpu::named("znver4").is_unresolved_native());
    }

    #[test]
    fn the_fast_math_grep_finds_a_flag_on_an_instruction() {
        assert!(contains_fast_math_flag("  %x = fadd reassoc double %a, %b"));
        assert!(contains_fast_math_flag("  %x = fmul contract double %a, %b"));
        assert_eq!(
            first_fast_math_flag("  %a = fadd double 1.0, 2.0\n  %b = fmul nnan double %a, %a"),
            Some((2, "nnan"))
        );
    }

    #[test]
    fn the_fast_math_grep_does_not_fire_on_a_comment_or_a_name() {
        assert!(!contains_fast_math_flag("  %x = fadd double %a, %b ; not reassoc"));
        assert!(!contains_fast_math_flag("  call void @transform_nsz_data()"));
        assert!(!contains_fast_math_flag("define void @contract_test()"));
    }

    #[test]
    fn the_whitelist_is_eleven_and_closed() {
        assert_eq!(Intrinsic::ALL.len(), 11);
        let mut names: Vec<&str> = Intrinsic::ALL.iter().map(|i| i.llvm_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 11, "two intrinsics share an LLVM name");
    }

    #[test]
    fn a_transcendental_is_a_libm_call_and_never_an_intrinsic() {
        // The whole of Decision 37 in one assertion. If any of these ever
        // becomes an intrinsic, the same source produces different numbers on
        // two build hosts.
        for name in ["sin", "cos", "tan", "exp", "log", "log2", "log10", "pow", "asin", "cbrt"] {
            match lower_math_call(name) {
                MathLowering::LibmCall(symbol) => assert!(symbol.starts_with("science_libm_")),
                MathLowering::Intrinsic(i) => {
                    panic!("{name} lowered to {}, which LLVM constant-folds against the build host's libm", i.llvm_name())
                }
            }
        }
    }

    #[test]
    fn the_whitelisted_names_do_lower_to_intrinsics() {
        assert_eq!(lower_math_call("sqrt"), MathLowering::Intrinsic(Intrinsic::Sqrt));
        assert_eq!(lower_math_call("fma"), MathLowering::Intrinsic(Intrinsic::Fma));
        assert_eq!(lower_math_call("round_even"), MathLowering::Intrinsic(Intrinsic::RoundEven));
    }

    #[test]
    fn llvms_own_default_is_recorded_so_nobody_has_to_look_it_up() {
        // Not a test of behaviour: a test that the note's claim is written down
        // where a reader of this crate will find it. Deleting the `Standard`
        // variant would delete the reason `Strict` is set explicitly.
        assert_eq!(FpContract::Standard.llvm_name(), "Standard");
    }
}
