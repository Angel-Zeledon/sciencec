//! The whole pipeline, down to regions.
//!
//! `science-mir`'s harness says why this goes through the real lexer, parser,
//! resolver and checker rather than building a [`Body`] by hand: *"the thing
//! under test is precisely the shape the checker happens to produce — where
//! `Coerce` nodes land, where `auto_borrow` inserted a `Borrow`, whether a
//! method resolved"*. That argument is stronger one level on, because what this
//! crate concludes about a signature depends on exactly those things.

#![allow(dead_code)]

use science_diagnostics::{Diagnostics, FileId};
use science_mir::mir::Body;
use science_mir::{lower_crate, CallGraph};
use science_regions::{analyse_crate, Analysis, BodyAnalysis, Context};
use science_resolve::hir::{self, DefId, DefKind};
use science_types::items::Declarations;
use science_types::{check_crate, Aliases, AtomOrder, Types};

/// One program, lexed, parsed, resolved, checked, lowered and region-checked.
pub struct Checked {
    pub krate: hir::Crate,
    pub types: Types,
    pub decls: Declarations,
    pub bodies: Vec<Body>,
    pub analysis: Analysis,
    /// What region inference reported, and nothing else.
    pub regions: Diagnostics,
    /// What the type checker reported, kept separate so that a test can assert
    /// on the borrow check without the container holes drowning it.
    pub checking: Diagnostics,
}

/// Runs everything over a program that must lex, parse and resolve cleanly.
///
/// The *checker* is not required to be silent, for `science-mir`'s harness's
/// reason: the holes `check`'s §6 names — method calls, `for`, operators on
/// user types — produce diagnostics on ordinary programs, and a harness that
/// demanded silence could only test the fragment those holes do not touch.
pub fn check(source: &str) -> Checked {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    assert!(!lexed.has_errors(), "the fixture must lex: {:?}", codes(&lexed));
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    assert!(!parsed.has_errors(), "the fixture must parse: {:?}", codes(&parsed));
    let (krate, resolution) = science_resolve::resolve_module(file, "regions.science", &ast);
    assert!(!resolution.has_errors(), "the fixture must resolve: {:?}", codes(&resolution));

    let order = AtomOrder::of(&krate.defs);
    let mut types = Types::new();
    let mut checking = Diagnostics::new();
    let mut aliases = Aliases::of(&krate, &mut types, &order, &mut checking);
    let decls = Declarations::of(&krate, &mut types, &order, &mut checking);
    let thir = check_crate(&krate, &decls, &mut types, &mut aliases, &order, &mut checking);

    let bodies = {
        let mut context = science_mir::Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        lower_crate(&mut context, &thir)
    };

    let graph = CallGraph::of(&bodies);
    let mut regions = Diagnostics::new();
    let analysis = {
        let mut context = Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        analyse_crate(&mut context, &bodies, &graph, &mut regions)
    };

    Checked { krate, types, decls, bodies, analysis, regions, checking }
}

/// `examples/21_compiler_shapes.science`, the acceptance case.
pub fn acceptance() -> Checked {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/21_compiler_shapes.science");
    let source = std::fs::read_to_string(path).expect("the acceptance example");
    check(&source)
}

pub fn codes(diagnostics: &Diagnostics) -> Vec<(u16, String)> {
    diagnostics.iter().map(|d| (d.code.0, d.message.clone())).collect()
}

impl Checked {
    pub fn body(&self, name: &str) -> &Body {
        self.bodies
            .iter()
            .find(|body| self.krate.defs.get(body.def()).name == name)
            .unwrap_or_else(|| panic!("the fixture lowers no body for `{name}`"))
    }

    /// The named function's body, chosen by parameter count when the name is
    /// ambiguous — which it is for the three `new`s of the acceptance case.
    pub fn body_with(&self, name: &str, params: usize) -> &Body {
        self.bodies
            .iter()
            .filter(|body| self.krate.defs.get(body.def()).name == name)
            .find(|body| body.params().count() == params)
            .unwrap_or_else(|| panic!("no `{name}` with {params} parameters"))
    }

    pub fn analysis_of(&self, name: &str) -> &BodyAnalysis {
        self.analysis.body(self.body(name).def()).expect("the body was analysed")
    }

    pub fn dump(&self, name: &str) -> String {
        let body = self.body(name);
        science_regions::dump::analysis(
            &self.krate.defs,
            body,
            self.analysis.body(body.def()).expect("analysed"),
        )
    }

    pub fn def(&self, name: &str, kind: DefKind) -> DefId {
        self.krate
            .defs
            .iter()
            .find(|def| def.name == name && def.kind == kind)
            .unwrap_or_else(|| panic!("the fixture declares no {kind:?} named `{name}`"))
            .id
    }

    /// The codes region inference reported, in order.
    pub fn reported(&self) -> Vec<u16> {
        self.regions.iter().map(|d| d.code.0).collect()
    }
}

/// Every [`science_mir::BorrowData`] in `body` whose referent is the local a
/// named binding or parameter lives in.
///
/// **Why a filter rather than `borrows()[0]`.** Several fixtures here are about
/// *the* loan in a body and used to say so by counting. `science-mir`'s §7
/// item 17 made `print(n)` of a non-`String` render through
/// `strings-formatting-and-docs.md` §1.7's builder, and the builder takes a
/// `mutable borrowed String` of an accumulator the fixture has no name for — a
/// real loan, of a temporary, with nothing to do with what the test asserts.
/// Counting it would make those tests fail for a reason that is not their
/// subject; dropping the count would let the loan they *are* about disappear
/// unnoticed. Selecting by name keeps both halves.
pub fn borrows_of<'a>(
    checked: &Checked,
    body: &'a Body,
    name: &str,
) -> Vec<&'a science_mir::BorrowData> {
    let local = body
        .locals()
        .find(|(_, decl)| match decl.kind {
            science_mir::LocalKind::Binding(def) | science_mir::LocalKind::Param(def) => {
                checked.krate.defs.get(def).name == name
            }
            _ => false,
        })
        .map(|(local, _)| local)
        .unwrap_or_else(|| panic!("the fixture has no binding called `{name}`"));
    body.borrows().iter().filter(|data| data.place.local == local).collect()
}
