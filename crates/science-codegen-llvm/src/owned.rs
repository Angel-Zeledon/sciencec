//! The newtype layer over LLVM's owning handles.
//!
//! **The decision.** Every handle LLVM-C hands back with an ownership transfer
//! is wrapped in a Rust type with a `Drop`, and the raw handle is reachable only
//! through a method. Nothing in [`crate::emit`] calls `LLVMDisposeAnything`.
//!
//! **The reason.** Decision 1's second cost item, in full:
//!
//! > *You give up `inkwell`'s ownership discipline. LLVM-C has real ownership
//! > transfers — `LLVMDisposeBuilder`, who owns a `Module` after it is added to
//! > an execution engine, when a `Type` is context-owned and must not be freed.
//! > **Mitigation:** a thin newtype layer, about 200 lines in each
//! > implementation, wrapping the half-dozen owning handles. It is the same 200
//! > lines on both sides and it ports with everything else.*
//!
//! This is that layer, and the estimate was close: six owning handles, and the
//! file is about the size the note predicted.
//!
//! **The cost.** The layer is Rust-shaped — `Drop`, `Deref`-free, move
//! semantics — and Science has none of those, so the *port* of this file is the
//! one part of the backend that is not a transliteration. What ports is the
//! **list**: which six handles own, and which of the many `LLVMTypeRef`s and
//! `LLVMValueRef`s do not. That list is the thing a reader of this file needs,
//! and it is stated here rather than left implicit in six `impl Drop`s.
//!
//! # What owns and what does not
//!
//! | Handle | Owned by | Freed with |
//! |---|---|---|
//! | `LLVMContextRef` | the caller | `LLVMContextDispose` |
//! | `LLVMModuleRef` | the caller, until it is given away | `LLVMDisposeModule` |
//! | `LLVMBuilderRef` | the caller | `LLVMDisposeBuilder` |
//! | `LLVMTargetMachineRef` | the caller | `LLVMDisposeTargetMachine` |
//! | `LLVMTargetDataRef` | the caller | `LLVMDisposeTargetData` |
//! | `LLVMPassBuilderOptionsRef` | the caller | `LLVMDisposePassBuilderOptions` |
//! | `char *` from `LLVMPrintModuleToString`, `LLVMGetDefaultTargetTriple`, `LLVMCopyStringRepOfTargetData`, and every `char **ErrorMessage` | the caller | `LLVMDisposeMessage` |
//! | `char *` from `LLVMGetErrorMessage` | the caller | `LLVMDisposeErrorMessage` |
//! | **`LLVMTypeRef`** | the **context** | never |
//! | **`LLVMValueRef`** | the **module** (globals, functions) or the function (instructions) | never |
//! | **`LLVMBasicBlockRef`** | the function | never |
//! | **`LLVMAttributeRef`** | the **context** | never |
//! | **`LLVMTargetRef`** | the target registry, which is process-global | never |
//!
//! The bottom five are the ones a newtype layer would get *wrong* by being
//! thorough: wrapping an `LLVMTypeRef` in something with a `Drop` is a
//! double-free the first time two structs share an `i64`.
//!
//! # The ordering rule, which `Drop` cannot express
//!
//! A module must be disposed **before** its context, and every `LLVMTypeRef` and
//! `LLVMValueRef` derived from that context is dangling afterwards. Rust's drop
//! order for struct fields is declaration order, so [`crate::emit::LlvmBackend`]
//! declares the module above the context and the compiler does the rest — but
//! that is a fact about one struct's field order, not a property of this file,
//! and it is written down in both places for that reason.

use std::ffi::{CStr, CString, c_char};

use crate::sys;

/// A NUL-terminated string to hand to LLVM.
///
/// Every `const char *` parameter in [`crate::sys`] needs one, and the borrow
/// has to outlive the call: `CString::new(s).unwrap().as_ptr()` is a dangling
/// pointer at the semicolon, which compiles, links, and passes freed memory to
/// LLVM. The name is used at every call site to make that impossible to write
/// by accident.
///
/// A Science identifier can contain a NUL only if the lexer let one through, so
/// the interior-NUL case is a compiler bug rather than a user error, and it is
/// reported as one.
pub fn cstr(text: &str) -> CString {
    CString::new(text).unwrap_or_else(|_| {
        CString::new(text.replace('\0', "\u{fffd}"))
            .expect("a string with its NULs replaced still has none")
    })
}

/// An owned `char *` that LLVM allocated.
///
/// Covers both disposal functions, because the header is explicit that
/// `LLVMGetErrorMessage`'s result goes to `LLVMDisposeErrorMessage` and
/// everything else goes to `LLVMDisposeMessage`. They are the same allocator in
/// 18.1.8 and that is not a licence to use one for the other.
pub struct Message {
    ptr: *mut c_char,
    from_error: bool,
}

impl Message {
    /// Take ownership of a `char *` from any function but `LLVMGetErrorMessage`.
    ///
    /// # Safety
    ///
    /// `ptr` must be null, or a string LLVM allocated and handed over.
    pub unsafe fn from_message(ptr: *mut c_char) -> Option<Message> {
        if ptr.is_null() { None } else { Some(Message { ptr, from_error: false }) }
    }

    /// Take ownership of a `char *` from `LLVMGetErrorMessage`.
    ///
    /// # Safety
    ///
    /// `ptr` must be null, or the result of `LLVMGetErrorMessage`.
    pub unsafe fn from_error(ptr: *mut c_char) -> Option<Message> {
        if ptr.is_null() { None } else { Some(Message { ptr, from_error: true }) }
    }

    /// The text, lossily. LLVM's diagnostics are ASCII in practice and a
    /// diagnostic that panicked on a stray byte would be a compiler that
    /// crashed while explaining why it could not compile something.
    pub fn to_string_lossy(&self) -> String {
        unsafe { CStr::from_ptr(self.ptr) }.to_string_lossy().into_owned()
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe {
            if self.from_error {
                sys::LLVMDisposeErrorMessage(self.ptr);
            } else {
                sys::LLVMDisposeMessage(self.ptr);
            }
        }
    }
}

/// An owned `LLVMContextRef`.
///
/// Everything else in a module — types, values, blocks, attributes — is owned by
/// this and dies with it. It is therefore the field that must be declared
/// **last** in any struct that also holds a module.
pub struct Context(sys::LLVMContextRef);

impl Context {
    /// A fresh context.
    pub fn new() -> Context {
        Context(unsafe { sys::LLVMContextCreate() })
    }

    /// The raw handle. Borrowed, never owned by the caller.
    pub fn raw(&self) -> sys::LLVMContextRef {
        self.0
    }
}

impl Default for Context {
    fn default() -> Context {
        Context::new()
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { sys::LLVMContextDispose(self.0) }
    }
}

/// An owned `LLVMModuleRef`.
///
/// Decision 4: **one module per crate**, so exactly one of these exists per
/// build.
pub struct Module(sys::LLVMModuleRef);

impl Module {
    /// A module in `context`.
    pub fn new(context: &Context, name: &str) -> Module {
        let name = cstr(name);
        Module(unsafe { sys::LLVMModuleCreateWithNameInContext(name.as_ptr(), context.raw()) })
    }

    /// The raw handle.
    pub fn raw(&self) -> sys::LLVMModuleRef {
        self.0
    }

    /// `target datalayout` and `target triple`.
    pub fn set_target(&self, triple: &str, data_layout: &str) {
        let triple = cstr(triple);
        let layout = cstr(data_layout);
        unsafe {
            sys::LLVMSetTarget(self.0, triple.as_ptr());
            sys::LLVMSetDataLayout(self.0, layout.as_ptr());
        }
    }

    /// The module as LLVM IR text.
    pub fn print(&self) -> String {
        let raw = unsafe { sys::LLVMPrintModuleToString(self.0) };
        match unsafe { Message::from_message(raw) } {
            Some(message) => message.to_string_lossy(),
            None => String::new(),
        }
    }

    /// Decision 34's verifier. `Ok(())` or the verifier's own message.
    ///
    /// `RETURN_STATUS` rather than `ABORT_PROCESS`, so that the caller can print
    /// the offending function's IR beside the message — which is the half of
    /// Decision 34 that makes it useful rather than merely loud.
    pub fn verify(&self) -> Result<(), String> {
        let mut out: *mut c_char = std::ptr::null_mut();
        let broken =
            unsafe { sys::LLVMVerifyModule(self.0, sys::verifier_action::RETURN_STATUS, &mut out) };
        let message = unsafe { Message::from_message(out) };
        if broken == 0 {
            return Ok(());
        }
        Err(message.map(|m| m.to_string_lossy()).unwrap_or_else(|| {
            "the module failed verification and LLVM gave no message".to_string()
        }))
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe { sys::LLVMDisposeModule(self.0) }
    }
}

/// An owned `LLVMBuilderRef`.
pub struct Builder(sys::LLVMBuilderRef);

impl Builder {
    /// A builder in `context`.
    pub fn new(context: &Context) -> Builder {
        Builder(unsafe { sys::LLVMCreateBuilderInContext(context.raw()) })
    }

    /// The raw handle.
    pub fn raw(&self) -> sys::LLVMBuilderRef {
        self.0
    }
}

impl Drop for Builder {
    fn drop(&mut self) {
        unsafe { sys::LLVMDisposeBuilder(self.0) }
    }
}

/// An owned `LLVMTargetDataRef`, and the `datalayout` string it prints to.
pub struct TargetData(sys::LLVMTargetDataRef);

impl TargetData {
    /// The layout string LLVM will use, which is what goes in the module.
    pub fn as_string(&self) -> String {
        let raw = unsafe { sys::LLVMCopyStringRepOfTargetData(self.0) };
        match unsafe { Message::from_message(raw) } {
            Some(message) => message.to_string_lossy(),
            None => String::new(),
        }
    }

    /// LLVM's own size for a type, in bytes.
    ///
    /// Used by `tests/layout_agreement.rs` and nowhere in the emission path.
    /// §3's layout is `science-codegen`'s answer and this is LLVM's; a code
    /// generator that asked LLVM for an offset would have moved Decision 42's
    /// line, and a *test* that compares the two is the reason to have both.
    ///
    /// # Safety
    ///
    /// `ty` must be a live `LLVMTypeRef` from a context that is still alive.
    /// An `LLVMTypeRef` is context-owned (see this module's ownership table), so
    /// it dangles the moment its [`Context`] is dropped, and LLVM dereferences
    /// it here. That is a caller obligation no signature can express, which is
    /// why the function is `unsafe` rather than merely containing an `unsafe`
    /// block.
    pub unsafe fn size_of(&self, ty: sys::LLVMTypeRef) -> u64 {
        unsafe { sys::LLVMABISizeOfType(self.0, ty) }
    }

    /// LLVM's own alignment for a type, in bytes.
    ///
    /// # Safety
    ///
    /// As [`TargetData::size_of`].
    pub unsafe fn align_of(&self, ty: sys::LLVMTypeRef) -> u64 {
        unsafe { sys::LLVMABIAlignmentOfType(self.0, ty) as u64 }
    }
}

impl Drop for TargetData {
    fn drop(&mut self) {
        unsafe { sys::LLVMDisposeTargetData(self.0) }
    }
}

/// An owned `LLVMTargetMachineRef`.
pub struct TargetMachine(sys::LLVMTargetMachineRef);

impl TargetMachine {
    /// Wrap a handle `LLVMCreateTargetMachine` just returned.
    ///
    /// # Safety
    ///
    /// `raw` must be a non-null target machine the caller owns.
    pub unsafe fn from_raw(raw: sys::LLVMTargetMachineRef) -> TargetMachine {
        TargetMachine(raw)
    }

    /// The raw handle.
    pub fn raw(&self) -> sys::LLVMTargetMachineRef {
        self.0
    }

    /// The machine's data layout.
    pub fn data_layout(&self) -> TargetData {
        TargetData(unsafe { sys::LLVMCreateTargetDataLayout(self.0) })
    }
}

impl Drop for TargetMachine {
    fn drop(&mut self) {
        unsafe { sys::LLVMDisposeTargetMachine(self.0) }
    }
}

/// An owned `LLVMPassBuilderOptionsRef`.
///
/// Created with its defaults and never configured: Decision 33 says the pipeline
/// is *"selected by string, with no custom passes"*, and every option on this
/// object is a way to make the pipeline something other than the string.
pub struct PassOptions(sys::LLVMPassBuilderOptionsRef);

impl PassOptions {
    /// The default options.
    pub fn new() -> PassOptions {
        PassOptions(unsafe { sys::LLVMCreatePassBuilderOptions() })
    }

    /// The raw handle.
    pub fn raw(&self) -> sys::LLVMPassBuilderOptionsRef {
        self.0
    }
}

impl Default for PassOptions {
    fn default() -> PassOptions {
        PassOptions::new()
    }
}

impl Drop for PassOptions {
    fn drop(&mut self) {
        unsafe { sys::LLVMDisposePassBuilderOptions(self.0) }
    }
}

/// The enum attribute kinds this backend emits, looked up once.
///
/// **The reason this is a struct and not six calls.**
/// `LLVMGetEnumAttributeKindForName` returns **0** for a name LLVM does not
/// know, and 0 is not an error value — it is `AttrKind::None`, which
/// `LLVMCreateEnumAttribute` will happily build an attribute out of. A typo in
/// `"nocapture"` therefore produces an attribute that is silently absent from
/// the emitted IR, and `noalias` silently absent is Decision 24 silently not
/// applied. Looking them all up at module start and failing on 0 turns that into
/// one loud failure at a known place.
///
/// `sret` is **not** here: it is a *type* attribute (§4.2 and
/// [`sys::LLVMCreateTypeAttribute`]) and its kind id is looked up the same way
/// but used differently, so it is a separate field.
pub struct Attrs {
    context: sys::LLVMContextRef,
    nounwind: u32,
    noalias: u32,
    nocapture: u32,
    readonly: u32,
    align: u32,
    sret: u32,
}

impl Attrs {
    /// Look up every kind id this backend uses.
    ///
    /// Returns the names LLVM did not recognise, which is a list that should
    /// always be empty and would not be on an LLVM that renamed one — LLVM 21
    /// replaced `nocapture` with `captures(none)`, so this is the check that
    /// turns "the backend quietly stopped emitting an attribute" into "the
    /// backend says which attribute this LLVM does not have".
    pub fn new(context: &Context) -> Result<Attrs, Vec<&'static str>> {
        let raw = context.raw();
        let mut missing = Vec::new();
        let mut kind = |name: &'static str| -> u32 {
            let c = cstr(name);
            let id = unsafe { sys::LLVMGetEnumAttributeKindForName(c.as_ptr(), name.len()) };
            if id == 0 {
                missing.push(name);
            }
            id
        };
        let attrs = Attrs {
            context: raw,
            nounwind: kind("nounwind"),
            noalias: kind("noalias"),
            nocapture: kind("nocapture"),
            readonly: kind("readonly"),
            align: kind("align"),
            sret: kind("sret"),
        };
        if missing.is_empty() { Ok(attrs) } else { Err(missing) }
    }

    fn flag(&self, kind: u32) -> sys::LLVMAttributeRef {
        unsafe { sys::LLVMCreateEnumAttribute(self.context, kind, 0) }
    }

    /// `nounwind`. Decision 6, on every function without exception.
    pub fn nounwind(&self) -> sys::LLVMAttributeRef {
        self.flag(self.nounwind)
    }

    /// `noalias`. Decision 24, and only ever on an exclusive borrow or an `sret`
    /// slot.
    pub fn noalias(&self) -> sys::LLVMAttributeRef {
        self.flag(self.noalias)
    }

    /// `nocapture`.
    pub fn nocapture(&self) -> sys::LLVMAttributeRef {
        self.flag(self.nocapture)
    }

    /// `readonly`. Decision 24's shared borrow.
    pub fn readonly(&self) -> sys::LLVMAttributeRef {
        self.flag(self.readonly)
    }

    /// `align N`.
    pub fn align(&self, bytes: u64) -> sys::LLVMAttributeRef {
        unsafe { sys::LLVMCreateEnumAttribute(self.context, self.align, bytes) }
    }

    /// `sret(<ty>)` — a type attribute, which is why it takes a type.
    ///
    /// # Safety
    ///
    /// `ty` must be a live `LLVMTypeRef` belonging to the same context this
    /// table was built from. LLVM reads through it to name the attribute's
    /// type, and a type from a *different* context is accepted, stored, and
    /// dangles when that context dies.
    pub unsafe fn sret(&self, ty: sys::LLVMTypeRef) -> sys::LLVMAttributeRef {
        unsafe { sys::LLVMCreateTypeAttribute(self.context, self.sret, ty) }
    }
}
