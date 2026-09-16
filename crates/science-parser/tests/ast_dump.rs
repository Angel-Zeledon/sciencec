//! Snapshot tests for the AST dump.
//!
//! These build nodes by hand rather than through the parser, on purpose. A
//! dump built from parsed source can only show trees the parser happens to
//! produce, so it can never catch a node the dump renders wrongly but the
//! parser never builds. Writing the nodes out also pins the shape of the types
//! themselves, which the other crates read as a contract.

use science_diagnostics::{FileId, Span};
use science_lexer::{IntBase, NumSuffix};
use science_parser::ast::*;
use science_parser::Dump;

// --- tiny builders, so the trees below stay readable ---------------------

fn sp(start: u32, end: u32) -> Span {
    Span::new(FileId(0), start, end)
}

fn ident(name: &str, start: u32) -> Ident {
    Ident { name: name.to_string(), span: sp(start, start + name.len() as u32) }
}

fn path(name: &str, start: u32) -> Path {
    let name = ident(name, start);
    let span = name.span;
    Path { segments: vec![PathSegment { name, generics: Vec::new(), span }], span }
}

fn ty(name: &str, start: u32) -> Type {
    let p = path(name, start);
    let span = p.span;
    Type { kind: TypeKind::Path(p), span }
}

fn bound(name: &str, start: u32) -> TypeBound {
    let p = path(name, start);
    let span = p.span;
    TypeBound { path: p, span }
}

fn expr(kind: ExprKind, start: u32, end: u32) -> Expr {
    Expr { kind, span: sp(start, end) }
}

fn var(name: &str, start: u32) -> Expr {
    let p = path(name, start);
    let span = p.span;
    Expr { kind: ExprKind::Path(p), span }
}

fn pat(kind: PatternKind, start: u32, end: u32) -> Pattern {
    Pattern { kind, span: sp(start, end) }
}

fn block(stmts: Vec<Stmt>, tail: Option<Expr>, start: u32, end: u32) -> Block {
    Block { stmts, tail: tail.map(Box::new), span: sp(start, end) }
}

// --- traits and impls ----------------------------------------------------

/// A trait with one required method and one with a default body, plus both
/// forms of `impl`.
#[test]
fn dump_trait_and_impls() {
    // trait Summarize:
    //     fn summarize(&self) -> String
    //     fn preview(&self) -> String:
    //         self.summarize()
    let summarize = FnDecl {
        is_pub: false,
        name: ident("summarize", 20),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Ref, span: sp(30, 35) }),
        params: Vec::new(),
        ret: Some(ty("String", 40)),
        where_clause: Vec::new(),
        body: None,
        span: sp(17, 46),
    };
    let preview = FnDecl {
        is_pub: false,
        name: ident("preview", 50),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Ref, span: sp(58, 63) }),
        params: Vec::new(),
        ret: Some(ty("String", 68)),
        where_clause: Vec::new(),
        body: Some(block(
            Vec::new(),
            Some(expr(
                ExprKind::MethodCall {
                    receiver: Box::new(expr(ExprKind::SelfValue, 80, 84)),
                    method: ident("summarize", 85),
                    generics: Vec::new(),
                    args: Vec::new(),
                },
                80,
                96,
            )),
            80,
            96,
        )),
        span: sp(47, 96),
    };
    let trait_decl = Item {
        kind: ItemKind::Trait(TraitDecl {
            is_pub: true,
            name: ident("Summarize", 6),
            generics: Vec::new(),
            supertraits: vec![bound("Clone", 16)],
            where_clause: Vec::new(),
            methods: vec![summarize, preview],
            span: sp(0, 96),
        }),
        span: sp(0, 96),
    };

    // impl Summarize for Doc: ...
    let impl_trait = Item {
        kind: ItemKind::Impl(ImplBlock {
            generics: vec![GenericParam {
                name: ident("T", 105),
                bounds: vec![bound("Clone", 108)],
                span: sp(105, 113),
            }],
            trait_: Some(bound("Summarize", 116)),
            self_ty: ty("Doc", 130),
            where_clause: vec![WherePredicate {
                ty: ty("T", 140),
                bounds: vec![bound("Eq", 143), bound("Ord", 148)],
                span: sp(140, 151),
            }],
            methods: Vec::new(),
            span: sp(100, 160),
        }),
        span: sp(100, 160),
    };

    // impl Doc: ...  (inherent)
    let impl_inherent = Item {
        kind: ItemKind::Impl(ImplBlock {
            generics: Vec::new(),
            trait_: None,
            self_ty: Type {
                kind: TypeKind::Ref { mutable: true, inner: Box::new(ty("Doc", 175)) },
                span: sp(170, 178),
            },
            where_clause: Vec::new(),
            methods: Vec::new(),
            span: sp(165, 185),
        }),
        span: sp(165, 185),
    };

    let module = Module { items: vec![trait_decl, impl_trait, impl_inherent], span: sp(0, 185) };
    insta::assert_snapshot!(module.dump());
}

// --- statements and expressions -----------------------------------------

/// One block holding every statement kind and a broad sweep of expressions.
#[test]
fn dump_statements_and_expressions() {
    let stmts = vec![
        // let mut count: I64 = 0
        Stmt {
            kind: StmtKind::Let(LetStmt {
                mutable: true,
                name: ident("count", 8),
                ty: Some(ty("I64", 15)),
                value: expr(
                    ExprKind::Literal(Literal::Int {
                        value: 0,
                        base: IntBase::Dec,
                        suffix: None,
                    }),
                    21,
                    22,
                ),
                span: sp(0, 22),
            }),
            span: sp(0, 22),
        },
        // let d = Doc(title: "a", body: "b")
        Stmt {
            kind: StmtKind::Let(LetStmt {
                mutable: false,
                name: ident("d", 27),
                ty: None,
                value: expr(
                    ExprKind::StructLit {
                        path: path("Doc", 31),
                        fields: vec![
                            FieldInit {
                                name: ident("title", 35),
                                value: expr(
                                    ExprKind::Literal(Literal::Str("a".into())),
                                    42,
                                    45,
                                ),
                                span: sp(35, 45),
                            },
                            FieldInit {
                                name: ident("body", 47),
                                value: expr(
                                    ExprKind::Literal(Literal::Str("b".into())),
                                    53,
                                    56,
                                ),
                                span: sp(47, 56),
                            },
                        ],
                    },
                    31,
                    57,
                ),
                span: sp(23, 57),
            }),
            span: sp(23, 57),
        },
        // count = count + 1 * 2
        Stmt {
            kind: StmtKind::Assign {
                target: var("count", 60),
                value: expr(
                    ExprKind::Binary {
                        op: BinaryOp::Add,
                        lhs: Box::new(var("count", 68)),
                        rhs: Box::new(expr(
                            ExprKind::Binary {
                                op: BinaryOp::Mul,
                                lhs: Box::new(expr(
                                    ExprKind::Literal(Literal::Int {
                                        value: 1,
                                        base: IntBase::Hex,
                                        suffix: Some(NumSuffix::U8),
                                    }),
                                    76,
                                    82,
                                )),
                                rhs: Box::new(expr(
                                    ExprKind::Literal(Literal::Float {
                                        value: 2.5,
                                        suffix: None,
                                    }),
                                    85,
                                    88,
                                )),
                            },
                            76,
                            88,
                        )),
                    },
                    68,
                    88,
                ),
            },
            span: sp(60, 88),
        },
        // items.get(0)?.value as I32
        Stmt {
            kind: StmtKind::Expr(expr(
                ExprKind::Cast {
                    expr: Box::new(expr(
                        ExprKind::Field {
                            base: Box::new(expr(
                                ExprKind::Try(Box::new(expr(
                                    ExprKind::MethodCall {
                                        receiver: Box::new(var("items", 90)),
                                        method: ident("get", 96),
                                        generics: vec![ty("I32", 100)],
                                        args: vec![expr(
                                            ExprKind::Literal(Literal::Int {
                                                value: 0,
                                                base: IntBase::Dec,
                                                suffix: None,
                                            }),
                                            105,
                                            106,
                                        )],
                                    },
                                    90,
                                    107,
                                ))),
                                90,
                                108,
                            )),
                            name: ident("value", 109),
                        },
                        90,
                        114,
                    )),
                    ty: ty("I32", 118),
                },
                90,
                121,
            )),
            span: sp(90, 121),
        },
        // grid[i](&mut buf, not flag, -1)
        Stmt {
            kind: StmtKind::Expr(expr(
                ExprKind::Call {
                    callee: Box::new(expr(
                        ExprKind::Index {
                            base: Box::new(var("grid", 123)),
                            index: Box::new(var("i", 128)),
                        },
                        123,
                        130,
                    )),
                    args: vec![
                        expr(
                            ExprKind::Ref { mutable: true, expr: Box::new(var("buf", 136)) },
                            131,
                            139,
                        ),
                        expr(
                            ExprKind::Unary {
                                op: UnaryOp::Not,
                                operand: Box::new(var("flag", 145)),
                            },
                            141,
                            149,
                        ),
                        expr(
                            ExprKind::Unary {
                                op: UnaryOp::Neg,
                                operand: Box::new(expr(
                                    ExprKind::Literal(Literal::Int {
                                        value: 1,
                                        base: IntBase::Dec,
                                        suffix: None,
                                    }),
                                    152,
                                    153,
                                )),
                            },
                            151,
                            153,
                        ),
                    ],
                },
                123,
                154,
            )),
            span: sp(123, 154),
        },
        // if a: b else: c   (as a statement)
        Stmt {
            kind: StmtKind::Expr(expr(
                ExprKind::If(IfExpr {
                    cond: Box::new(var("a", 159)),
                    then_branch: block(Vec::new(), Some(var("b", 162)), 162, 163),
                    else_branch: Some(Box::new(expr(
                        ExprKind::Block(block(Vec::new(), Some(var("c", 170)), 170, 171)),
                        170,
                        171,
                    ))),
                    span: sp(156, 171),
                }),
                156,
                171,
            )),
            span: sp(156, 171),
        },
        // while a: loop: for x in xs: ()
        Stmt {
            kind: StmtKind::Expr(expr(
                ExprKind::While {
                    cond: Box::new(var("a", 179)),
                    body: block(
                        vec![Stmt {
                            kind: StmtKind::Expr(expr(
                                ExprKind::Loop {
                                    body: block(
                                        vec![Stmt {
                                            kind: StmtKind::Expr(expr(
                                                ExprKind::For {
                                                    pattern: pat(
                                                        PatternKind::Binding {
                                                            mutable: false,
                                                            name: ident("x", 200),
                                                        },
                                                        200,
                                                        201,
                                                    ),
                                                    iter: Box::new(var("xs", 205)),
                                                    body: block(
                                                        vec![
                                                            Stmt {
                                                                kind: StmtKind::Continue,
                                                                span: sp(209, 217),
                                                            },
                                                            Stmt {
                                                                kind: StmtKind::Break(Some(
                                                                    var("x", 224),
                                                                )),
                                                                span: sp(218, 225),
                                                            },
                                                            Stmt {
                                                                kind: StmtKind::Return(None),
                                                                span: sp(226, 232),
                                                            },
                                                        ],
                                                        None,
                                                        209,
                                                        232,
                                                    ),
                                                },
                                                196,
                                                232,
                                            )),
                                            span: sp(196, 232),
                                        }],
                                        None,
                                        196,
                                        232,
                                    ),
                                },
                                190,
                                232,
                            )),
                            span: sp(190, 232),
                        }],
                        None,
                        190,
                        232,
                    ),
                },
                173,
                232,
            )),
            span: sp(173, 232),
        },
    ];

    let body = block(
        stmts,
        Some(expr(
            ExprKind::Tuple(vec![
                expr(ExprKind::Literal(Literal::Bool(true)), 235, 239),
                expr(ExprKind::Literal(Literal::Char('a')), 241, 244),
                expr(ExprKind::Unit, 246, 248),
            ]),
            234,
            249,
        )),
        0,
        249,
    );

    insta::assert_snapshot!(body.dump());
}

// --- patterns ------------------------------------------------------------

/// Every pattern kind, inside the match arms that would produce them.
#[test]
fn dump_patterns() {
    let arms = vec![
        MatchArm {
            pattern: pat(
                PatternKind::Variant {
                    path: path("Ok", 10),
                    elems: vec![pat(
                        PatternKind::Binding { mutable: false, name: ident("value", 13) },
                        13,
                        18,
                    )],
                },
                10,
                19,
            ),
            body: var("value", 21),
            span: sp(10, 26),
        },
        MatchArm {
            pattern: pat(
                PatternKind::Tuple(vec![
                    pat(
                        PatternKind::Literal(Literal::Int {
                            value: 0,
                            base: IntBase::Dec,
                            suffix: None,
                        }),
                        31,
                        32,
                    ),
                    pat(PatternKind::Wildcard, 34, 35),
                ]),
                30,
                36,
            ),
            body: expr(ExprKind::Literal(Literal::Str("x axis".into())), 38, 46),
            span: sp(30, 46),
        },
        MatchArm {
            pattern: pat(
                PatternKind::Struct {
                    path: path("Doc", 50),
                    fields: vec![FieldPattern {
                        name: ident("title", 54),
                        pattern: pat(
                            PatternKind::Binding { mutable: true, name: ident("t", 61) },
                            61,
                            62,
                        ),
                        span: sp(54, 62),
                    }],
                },
                50,
                63,
            ),
            body: var("t", 65),
            span: sp(50, 66),
        },
        MatchArm {
            pattern: pat(
                PatternKind::Or(vec![
                    pat(PatternKind::Literal(Literal::Bool(true)), 70, 74),
                    pat(PatternKind::Unit, 77, 79),
                    pat(
                        PatternKind::Variant { path: path("None", 82), elems: Vec::new() },
                        82,
                        86,
                    ),
                ]),
                70,
                86,
            ),
            body: expr(ExprKind::Unit, 88, 90),
            span: sp(70, 90),
        },
    ];

    let match_expr = expr(
        ExprKind::Match(MatchExpr {
            scrutinee: Box::new(var("result", 6)),
            arms,
            span: sp(0, 90),
        }),
        0,
        90,
    );

    insta::assert_snapshot!(match_expr.dump());
}

/// Types have their own corner of the tree; dump them on their own so a change
/// there is visible without a whole function around it.
#[test]
fn dump_types() {
    let types = vec![
        // &mut Array[T]
        Type {
            kind: TypeKind::Ref {
                mutable: true,
                inner: Box::new(Type {
                    kind: TypeKind::Path(Path {
                        segments: vec![PathSegment {
                            name: ident("Array", 5),
                            generics: vec![ty("T", 11)],
                            span: sp(5, 13),
                        }],
                        span: sp(5, 13),
                    }),
                    span: sp(5, 13),
                }),
            },
            span: sp(0, 13),
        },
        // &dyn Summarize
        Type {
            kind: TypeKind::Ref {
                mutable: false,
                inner: Box::new(Type {
                    kind: TypeKind::Dyn(bound("Summarize", 20)),
                    span: sp(16, 29),
                }),
            },
            span: sp(15, 29),
        },
        // (I32, Bool)
        Type {
            kind: TypeKind::Tuple(vec![ty("I32", 32), ty("Bool", 37)]),
            span: sp(31, 42),
        },
        // ()
        Type { kind: TypeKind::Unit, span: sp(44, 46) },
        // Self
        Type { kind: TypeKind::SelfType, span: sp(48, 52) },
        // text.parser.Token
        Type {
            kind: TypeKind::Path(Path {
                segments: vec![
                    PathSegment {
                        name: ident("text", 54),
                        generics: Vec::new(),
                        span: sp(54, 58),
                    },
                    PathSegment {
                        name: ident("parser", 59),
                        generics: Vec::new(),
                        span: sp(59, 65),
                    },
                    PathSegment {
                        name: ident("Token", 66),
                        generics: Vec::new(),
                        span: sp(66, 71),
                    },
                ],
                span: sp(54, 71),
            }),
            span: sp(54, 71),
        },
    ];

    let mut out = String::new();
    for t in &types {
        out.push_str(&t.dump());
    }
    insta::assert_snapshot!(out);
}
