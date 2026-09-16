//! Snapshot tests for the AST dump.
//!
//! These build nodes by hand rather than through the parser, on purpose. A
//! dump built from parsed source can only show trees the parser happens to
//! produce, so it can never catch a node the dump renders wrongly but the
//! parser never builds. Writing the nodes out also pins the shape of the types
//! themselves, which the other crates read as a contract.
//!
//! Between them these six dumps reach every variant of every enum in
//! `ast.rs`: a printer that is never called is a printer nothing is checking.
//! The comment above each tree is the Science source it stands for; the spans
//! are laid out to match that source's shape — parents covering children,
//! siblings in order — rather than being byte-exact offsets into a file that
//! is never actually lexed.

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

/// `text.parser.Token` — several segments, laid out end to end with a dot
/// between them.
fn dotted_path(names: &[&str], start: u32) -> Path {
    let mut at = start;
    let mut segments = Vec::new();
    for name in names {
        let name = ident(name, at);
        let span = name.span;
        at = span.end + 1; // the `.`
        segments.push(PathSegment { name, generics: Vec::new(), span });
    }
    Path { segments, span: sp(start, at - 1) }
}

fn ty(name: &str, start: u32) -> Type {
    let p = path(name, start);
    let span = p.span;
    Type { kind: TypeKind::Path(p), span }
}

/// `Array of T`, `Map of (String, Int)` — a path type carrying arguments.
fn generic_ty(name: &str, start: u32, end: u32, generics: Vec<Type>) -> Type {
    let span = sp(start, end);
    let segment = PathSegment { name: ident(name, start), generics, span };
    Type { kind: TypeKind::Path(Path { segments: vec![segment], span }), span }
}

/// A const generic argument: the `4` of `Window of (Int, 4)`.
fn const_arg(literal: Literal, start: u32, end: u32) -> Type {
    let span = sp(start, end);
    Type { kind: TypeKind::Const(ConstExpr { kind: ConstExprKind::Lit(literal), span }), span }
}

/// A negated const generic argument: the `-1` of a dimension vector. `start`
/// is the `-`, so the literal's own span begins one byte later.
fn neg_const_arg(magnitude: u128, start: u32, end: u32) -> Type {
    let literal = Literal::Int { value: magnitude, base: IntBase::Dec, suffix: None };
    let operand = ConstExpr { kind: ConstExprKind::Lit(literal), span: sp(start + 1, end) };
    let span = sp(start, end);
    let value = ConstExpr { kind: ConstExprKind::Neg(Box::new(operand)), span };
    Type { kind: TypeKind::Const(value), span }
}

fn borrowed_ty(mutable: bool, inner: Type, start: u32, end: u32) -> Type {
    Type { kind: TypeKind::Borrowed { mutable, inner: Box::new(inner) }, span: sp(start, end) }
}

fn bound(name: &str, start: u32) -> TypeBound {
    let p = path(name, start);
    let span = p.span;
    TypeBound { path: p, span }
}

/// `T` or `T: Ord` in an `of` list.
fn type_param(name: &str, start: u32, end: u32, bounds: Vec<TypeBound>) -> GenericParam {
    GenericParam {
        name: ident(name, start),
        kind: GenericParamKind::Type { bounds },
        span: sp(start, end),
    }
}

fn expr(kind: ExprKind, start: u32, end: u32) -> Expr {
    Expr { kind, span: sp(start, end) }
}

fn var(name: &str, start: u32) -> Expr {
    let p = path(name, start);
    let span = p.span;
    Expr { kind: ExprKind::Path(p), span }
}

fn int(value: u128, start: u32, end: u32) -> Expr {
    expr(ExprKind::Literal(Literal::Int { value, base: IntBase::Dec, suffix: None }), start, end)
}

fn pat(kind: PatternKind, start: u32, end: u32) -> Pattern {
    Pattern { kind, span: sp(start, end) }
}

fn block(stmts: Vec<Stmt>, tail: Option<Expr>, start: u32, end: u32) -> Block {
    Block { stmts, tail: tail.map(Box::new), span: sp(start, end) }
}

fn stmt(kind: StmtKind, start: u32, end: u32) -> Stmt {
    Stmt { kind, span: sp(start, end) }
}

fn item(kind: ItemKind, start: u32, end: u32) -> Item {
    Item { kind, span: sp(start, end) }
}

// --- items ---------------------------------------------------------------

/// Every `ItemKind`, so each declaration's printer is exercised once, and with
/// them the two `GenericParamKind`s and all three forms of `use`.
#[test]
fn dump_items() {
    // use text.parser
    let use_module = item(
        ItemKind::Use(UseDecl {
            path: dotted_path(&["text", "parser"], 4),
            imports: None,
            span: sp(0, 15),
        }),
        0,
        15,
    );

    // use text.parser (Token, lex)
    let use_names = item(
        ItemKind::Use(UseDecl {
            path: dotted_path(&["text", "parser"], 20),
            imports: Some(vec![ident("Token", 33), ident("lex", 40)]),
            span: sp(16, 44),
        }),
        16,
        44,
    );

    // use text ()  — the degenerate empty list, which the dump names rather
    // than printing as an absent one.
    let use_nothing = item(
        ItemKind::Use(UseDecl {
            path: dotted_path(&["text"], 49),
            imports: Some(Vec::new()),
            span: sp(45, 56),
        }),
        45,
        56,
    );

    // public def largest of T: Ord(items: borrowed Array of T)
    //         -> borrowed T where T: Clone:
    //     items.first()
    let largest = item(
        ItemKind::Fn(FnDecl {
            is_pub: true,
            name: ident("largest", 76),
            generics: vec![type_param("T", 87, 93, vec![bound("Ord", 90)])],
            self_param: None,
            params: vec![Param {
                name: ident("items", 95),
                ty: borrowed_ty(
                    false,
                    generic_ty("Array", 111, 126, vec![ty("T", 125)]),
                    102,
                    126,
                ),
                span: sp(95, 126),
            }],
            ret: Some(borrowed_ty(false, ty("T", 145), 136, 146)),
            where_clause: vec![WherePredicate {
                ty: ty("T", 159),
                bounds: vec![bound("Clone", 162)],
                span: sp(159, 167),
            }],
            body: Some(block(
                Vec::new(),
                Some(expr(
                    ExprKind::MethodCall {
                        receiver: Box::new(var("items", 175)),
                        method: ident("first", 181),
                        generics: Vec::new(),
                        args: Vec::new(),
                    },
                    175,
                    188,
                )),
                175,
                188,
            )),
            span: sp(60, 188),
        }),
        60,
        188,
    );

    // public type Grid of (T, const ROWS: Int):
    //     public cells: Array of T
    //     stride: Int
    let grid = item(
        ItemKind::Record(RecordDecl {
            is_pub: true,
            name: ident("Grid", 222),
            generics: vec![
                type_param("T", 231, 232, Vec::new()),
                GenericParam {
                    name: ident("ROWS", 240),
                    kind: GenericParamKind::Const { ty: ty("Int", 246) },
                    span: sp(234, 249),
                },
            ],
            where_clause: Vec::new(),
            fields: vec![
                FieldDef {
                    is_pub: true,
                    name: ident("cells", 263),
                    ty: generic_ty("Array", 270, 280, vec![ty("T", 279)]),
                    span: sp(256, 280),
                },
                FieldDef {
                    is_pub: false,
                    name: ident("stride", 285),
                    ty: ty("Int", 293),
                    span: sp(285, 296),
                },
            ],
            span: sp(210, 296),
        }),
        210,
        296,
    );

    // choice Format of T where T: Clone:
    //     Plain
    //     Rich(String, T)
    let format = item(
        ItemKind::Choice(ChoiceDecl {
            is_pub: false,
            name: ident("Format", 317),
            generics: vec![type_param("T", 327, 328, Vec::new())],
            where_clause: vec![WherePredicate {
                ty: ty("T", 335),
                bounds: vec![bound("Clone", 338)],
                span: sp(335, 343),
            }],
            variants: vec![
                VariantDef { name: ident("Plain", 350), payload: Vec::new(), span: sp(350, 355) },
                VariantDef {
                    name: ident("Rich", 360),
                    payload: vec![ty("String", 365), ty("T", 373)],
                    span: sp(360, 375),
                },
            ],
            span: sp(310, 375),
        }),
        310,
        375,
    );

    // public type Embedding is Array of F32
    let embedding = item(
        ItemKind::Alias(AliasDecl {
            is_pub: true,
            name: ident("Embedding", 402),
            generics: Vec::new(),
            ty: generic_ty("Array", 415, 427, vec![ty("F32", 424)]),
            span: sp(390, 427),
        }),
        390,
        427,
    );

    // type Handle of T is Box of T
    let handle = item(
        ItemKind::Alias(AliasDecl {
            is_pub: false,
            name: ident("Handle", 435),
            generics: vec![type_param("T", 445, 446, Vec::new())],
            ty: generic_ty("Box", 450, 458, vec![ty("T", 457)]),
            span: sp(430, 458),
        }),
        430,
        458,
    );

    // public const WIDTH: Int be 768
    let width = item(
        ItemKind::Const(ConstDecl {
            is_pub: true,
            name: ident("WIDTH", 473),
            ty: Some(ty("Int", 480)),
            value: int(768, 487, 490),
            span: sp(460, 490),
        }),
        460,
        490,
    );

    // const GREETING be "hello"
    let greeting = item(
        ItemKind::Const(ConstDecl {
            is_pub: false,
            name: ident("GREETING", 500),
            ty: None,
            value: expr(ExprKind::Literal(Literal::Str("hello".into())), 512, 519),
            span: sp(494, 519),
        }),
        494,
        519,
    );

    let module = Module {
        items: vec![
            use_module,
            use_names,
            use_nothing,
            largest,
            grid,
            format,
            embedding,
            handle,
            width,
            greeting,
        ],
        span: sp(0, 519),
    };
    insta::assert_snapshot!(module.dump());
}

// --- interfaces and implementations --------------------------------------

/// An interface with an associated type, a required method and a default
/// body, all three receivers, and every shape of implementation head:
/// `implements` with a body, a generic one, a marker on one line, and `has`.
#[test]
fn dump_interfaces_and_implementations() {
    // public interface Iterate: Clone:
    //     type Item
    //     def next(mutable self) -> Option of Self.Item
    //     def count(self: Self) -> Int
    //     def describe(self) -> String:
    //         "an iterator"
    let next = FnDecl {
        is_pub: false,
        name: ident("next", 57),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Mutable, span: sp(62, 74) }),
        params: Vec::new(),
        ret: Some(generic_ty(
            "Option",
            84,
            103,
            vec![Type { kind: TypeKind::SelfAssoc(ident("Item", 99)), span: sp(94, 103) }],
        )),
        where_clause: Vec::new(),
        body: None,
        span: sp(48, 103),
    };
    let count = FnDecl {
        is_pub: false,
        name: ident("count", 119),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Value, span: sp(125, 135) }),
        params: Vec::new(),
        ret: Some(ty("Int", 146)),
        where_clause: Vec::new(),
        body: None,
        span: sp(110, 149),
    };
    let describe = FnDecl {
        is_pub: false,
        name: ident("describe", 164),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Shared, span: sp(173, 177) }),
        params: Vec::new(),
        ret: Some(ty("String", 186)),
        where_clause: Vec::new(),
        body: Some(block(
            Vec::new(),
            Some(expr(ExprKind::Literal(Literal::Str("an iterator".into())), 200, 213)),
            200,
            213,
        )),
        span: sp(155, 213),
    };
    let interface_decl = item(
        ItemKind::Interface(InterfaceDecl {
            is_pub: true,
            name: ident("Iterate", 13),
            generics: Vec::new(),
            supers: vec![bound("Clone", 22)],
            where_clause: Vec::new(),
            assoc_types: vec![AssocTypeDecl { name: ident("Item", 38), span: sp(33, 42) }],
            methods: vec![next, count, describe],
            span: sp(0, 213),
        }),
        0,
        213,
    );

    // Counter implements Iterate:
    //     type Item is Int
    //     def next(mutable self) -> Option of Self.Item:
    //         Some(self.value)
    let impl_next = FnDecl {
        is_pub: false,
        name: ident("next", 304),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Mutable, span: sp(309, 321) }),
        params: Vec::new(),
        ret: Some(generic_ty(
            "Option",
            331,
            350,
            vec![Type { kind: TypeKind::SelfAssoc(ident("Item", 346)), span: sp(341, 350) }],
        )),
        where_clause: Vec::new(),
        body: Some(block(
            Vec::new(),
            Some(expr(
                ExprKind::Call {
                    callee: Box::new(var("Some", 360)),
                    args: vec![Arg {
                        name: None,
                        value: expr(
                            ExprKind::Field {
                                base: Box::new(expr(ExprKind::SelfValue, 365, 369)),
                                name: ident("value", 370),
                            },
                            365,
                            375,
                        ),
                        span: sp(365, 375),
                    }],
                },
                360,
                376,
            )),
            360,
            376,
        )),
        span: sp(295, 376),
    };
    let impl_iterate = item(
        ItemKind::Impl(ImplBlock {
            generics: Vec::new(),
            interface: Some(bound("Iterate", 256)),
            self_ty: ty("Counter", 240),
            where_clause: Vec::new(),
            assoc_types: vec![AssocTypeBinding {
                name: ident("Item", 274),
                ty: ty("Int", 282),
                span: sp(269, 285),
            }],
            methods: vec![impl_next],
            span: sp(240, 376),
        }),
        240,
        376,
    );

    // Pair of (A, B) implements Swap where A: Clone:
    //     def swapped(self) -> Pair of (B, A)
    let swapped = FnDecl {
        is_pub: false,
        name: ident("swapped", 499),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Shared, span: sp(507, 511) }),
        params: Vec::new(),
        ret: Some(generic_ty("Pair", 520, 535, vec![ty("B", 528), ty("A", 531)])),
        where_clause: Vec::new(),
        body: None,
        span: sp(490, 535),
    };
    let impl_swap = item(
        ItemKind::Impl(ImplBlock {
            generics: vec![
                type_param("A", 438, 439, Vec::new()),
                type_param("B", 441, 442, Vec::new()),
            ],
            interface: Some(bound("Swap", 456)),
            self_ty: generic_ty("Pair", 430, 444, vec![ty("A", 438), ty("B", 441)]),
            where_clause: vec![WherePredicate {
                ty: ty("A", 467),
                bounds: vec![bound("Clone", 470)],
                span: sp(467, 475),
            }],
            assoc_types: Vec::new(),
            methods: vec![swapped],
            span: sp(430, 535),
        }),
        430,
        535,
    );

    // Position implements Copy   (a marker, all on one line)
    let impl_marker = item(
        ItemKind::Impl(ImplBlock {
            generics: Vec::new(),
            interface: Some(bound("Copy", 565)),
            self_ty: ty("Position", 545),
            where_clause: Vec::new(),
            assoc_types: Vec::new(),
            methods: Vec::new(),
            span: sp(545, 569),
        }),
        545,
        569,
    );

    // (Array of Doc) has:
    //     def first(self) -> borrowed Doc:
    //         self[0]
    let first = FnDecl {
        is_pub: false,
        name: ident("first", 614),
        generics: Vec::new(),
        self_param: Some(SelfParam { kind: SelfKind::Shared, span: sp(620, 624) }),
        params: Vec::new(),
        ret: Some(borrowed_ty(false, ty("Doc", 643), 634, 646)),
        where_clause: Vec::new(),
        body: Some(block(
            Vec::new(),
            Some(expr(
                ExprKind::Index {
                    base: Box::new(expr(ExprKind::SelfValue, 655, 659)),
                    index: Box::new(int(0, 660, 661)),
                },
                655,
                662,
            )),
            655,
            662,
        )),
        span: sp(605, 662),
    };
    let impl_inherent = item(
        ItemKind::Impl(ImplBlock {
            generics: Vec::new(),
            interface: None,
            // A parenthesised type is just the type: the parens group and
            // leave no node, so the span starts inside them.
            self_ty: generic_ty("Array", 581, 593, vec![ty("Doc", 590)]),
            where_clause: Vec::new(),
            assoc_types: Vec::new(),
            methods: vec![first],
            span: sp(580, 662),
        }),
        580,
        662,
    );

    let module = Module {
        items: vec![interface_decl, impl_iterate, impl_swap, impl_marker, impl_inherent],
        span: sp(0, 662),
    };
    insta::assert_snapshot!(module.dump());
}

// --- statements and expressions -----------------------------------------

/// Every binary operator, in the order `BinaryOp` declares them, as `a OP b`.
///
/// The dump prints each operator's internal name. For identity that name is
/// still `==` and `!=`, although §1 leaves `is` and `is not` as the only way
/// to *write* them — which is the point of the pair further down.
fn every_binary_op(start: u32) -> Expr {
    use BinaryOp::*;
    let ops = [
        Add, Sub, Mul, Div, Rem, Pow, MatMul, Shl, Shr, BitAnd, BitXor, BitOr, Eq, Ne, Lt, Gt, Le,
        Ge, And, Or,
    ];
    let elems: Vec<Expr> = ops
        .iter()
        .enumerate()
        .map(|(i, op)| {
            let at = start + i as u32 * 8;
            expr(
                ExprKind::Binary {
                    op: *op,
                    lhs: Box::new(var("a", at)),
                    rhs: Box::new(var("b", at + 4)),
                },
                at,
                at + 5,
            )
        })
        .collect();
    expr(ExprKind::Tuple(elems), start, start + ops.len() as u32 * 8)
}

/// Every base and every suffix a number can carry. The dump prints them back
/// from two little tables, which are exactly the kind of thing that rots
/// unseen if nothing ever asks for the entries at the bottom.
fn every_literal_form(start: u32) -> Expr {
    let mut elems = Vec::new();
    let mut at = start;
    for base in [IntBase::Dec, IntBase::Hex, IntBase::Oct, IntBase::Bin] {
        elems.push(expr(
            ExprKind::Literal(Literal::Int { value: 10, base, suffix: None }),
            at,
            at + 6,
        ));
        at += 8;
    }
    for suffix in [
        NumSuffix::I8,
        NumSuffix::I16,
        NumSuffix::I32,
        NumSuffix::I64,
        NumSuffix::U8,
        NumSuffix::U16,
        NumSuffix::U32,
        NumSuffix::U64,
    ] {
        elems.push(expr(
            ExprKind::Literal(Literal::Int { value: 1, base: IntBase::Dec, suffix: Some(suffix) }),
            at,
            at + 4,
        ));
        at += 6;
    }
    for suffix in [NumSuffix::F32, NumSuffix::F64] {
        elems.push(expr(
            ExprKind::Literal(Literal::Float { value: 1.5, suffix: Some(suffix) }),
            at,
            at + 6,
        ));
        at += 8;
    }
    expr(ExprKind::Tuple(elems), start, at)
}

/// One block holding every statement kind and a broad sweep of expressions.
#[test]
fn dump_statements_and_expressions() {
    let stmts = vec![
        // let mutable count: Int be 0
        stmt(
            StmtKind::Let(LetStmt {
                mutable: true,
                names: vec![LetName {
                    name: ident("count", 11),
                    ty: Some(ty("Int", 18)),
                    span: sp(11, 21),
                }],
                value: int(0, 25, 26),
                span: sp(0, 26),
            }),
            0,
            26,
        ),
        // let d be Doc(title: "a", body: "b")
        stmt(
            StmtKind::Let(LetStmt {
                mutable: false,
                names: vec![LetName {
                    name: ident("d", 34),
                    ty: None,
                    span: sp(0, 0),
                }],
                value: expr(
                    ExprKind::StructLit {
                        path: path("Doc", 39),
                        fields: vec![
                            FieldInit {
                                name: ident("title", 43),
                                value: expr(ExprKind::Literal(Literal::Str("a".into())), 50, 53),
                                span: sp(43, 53),
                            },
                            FieldInit {
                                name: ident("body", 55),
                                value: expr(ExprKind::Literal(Literal::Str("b".into())), 61, 64),
                                span: sp(55, 64),
                            },
                        ],
                    },
                    39,
                    64,
                ),
                span: sp(30, 64),
            }),
            30,
            64,
        ),
        // count be count + 0x01u8 ** 2.5
        stmt(
            StmtKind::Assign {
                target: var("count", 70),
                value: expr(
                    ExprKind::Binary {
                        op: BinaryOp::Add,
                        lhs: Box::new(var("count", 79)),
                        rhs: Box::new(expr(
                            ExprKind::Binary {
                                op: BinaryOp::Pow,
                                lhs: Box::new(expr(
                                    ExprKind::Literal(Literal::Int {
                                        value: 1,
                                        base: IntBase::Hex,
                                        suffix: Some(NumSuffix::U8),
                                    }),
                                    87,
                                    93,
                                )),
                                rhs: Box::new(expr(
                                    ExprKind::Literal(Literal::Float {
                                        value: 2.5,
                                        suffix: None,
                                    }),
                                    96,
                                    99,
                                )),
                            },
                            87,
                            99,
                        )),
                    },
                    79,
                    99,
                ),
            },
            70,
            99,
        ),
        // items.get of Int(0).value? as Int
        //
        // `?` is postfix and sits on the same rung as the field access, so it
        // applies to the whole chain to its left; `as` is above that rung, so
        // the cast is outermost. The tree is the same shape the prefix `try`
        // produced and it is reached from the other direction.
        stmt(
            StmtKind::Expr(expr(
                ExprKind::Cast {
                    expr: Box::new(expr(
                        ExprKind::Present(Box::new(expr(
                            ExprKind::Field {
                                base: Box::new(expr(
                                    ExprKind::MethodCall {
                                        receiver: Box::new(var("items", 109)),
                                        method: ident("get", 115),
                                        generics: vec![ty("Int", 122)],
                                        args: vec![Arg {
                                            name: None,
                                            value: int(0, 131, 132),
                                            span: sp(131, 132),
                                        }],
                                    },
                                    109,
                                    133,
                                )),
                                name: ident("value", 134),
                            },
                            109,
                            139,
                        ))),
                        105,
                        139,
                    )),
                    ty: ty("Int", 144),
                },
                105,
                147,
            )),
            105,
            147,
        ),
        // grid[i](mutable borrowed buf, not flag, -1)
        stmt(
            StmtKind::Expr(expr(
                ExprKind::Call {
                    callee: Box::new(expr(
                        ExprKind::Index {
                            base: Box::new(var("grid", 155)),
                            index: Box::new(var("i", 160)),
                        },
                        155,
                        163,
                    )),
                    args: vec![
                        Arg {
                            name: None,
                            value: expr(
                                ExprKind::Borrowed {
                                    mutable: true,
                                    expr: Box::new(var("buf", 181)),
                                },
                                164,
                                184,
                            ),
                            span: sp(164, 184),
                        },
                        Arg {
                            name: None,
                            value: expr(
                                ExprKind::Unary {
                                    op: UnaryOp::Not,
                                    operand: Box::new(var("flag", 190)),
                                },
                                186,
                                194,
                            ),
                            span: sp(186, 194),
                        },
                        Arg {
                            name: None,
                            value: expr(
                                ExprKind::Unary {
                                    op: UnaryOp::Neg,
                                    operand: Box::new(int(1, 197, 198)),
                                },
                                196,
                                198,
                            ),
                            span: sp(196, 198),
                        },
                    ],
                },
                155,
                199,
            )),
            155,
            199,
        ),
        // docs.sort(by: doc giving doc.title)  — a named argument holding the
        // explicit-subject closure form.
        stmt(
            StmtKind::Expr(expr(
                ExprKind::MethodCall {
                    receiver: Box::new(var("docs", 205)),
                    method: ident("sort", 210),
                    generics: Vec::new(),
                    args: vec![Arg {
                        name: Some(ident("by", 215)),
                        value: expr(
                            ExprKind::Closure {
                                param: Some(ident("doc", 219)),
                                body: Box::new(expr(
                                    ExprKind::Field {
                                        base: Box::new(var("doc", 229)),
                                        name: ident("title", 233),
                                    },
                                    229,
                                    238,
                                )),
                            },
                            219,
                            238,
                        ),
                        span: sp(215, 238),
                    }],
                },
                205,
                239,
            )),
            205,
            239,
        ),
        // names.map(each.title)  — the implicit-subject closure, whose body is
        // what mentions `each`.
        stmt(
            StmtKind::Expr(expr(
                ExprKind::MethodCall {
                    receiver: Box::new(var("names", 245)),
                    method: ident("map", 251),
                    generics: Vec::new(),
                    args: vec![Arg {
                        name: None,
                        value: expr(
                            ExprKind::Closure {
                                param: None,
                                body: Box::new(expr(
                                    ExprKind::Field {
                                        base: Box::new(expr(ExprKind::Each, 255, 259)),
                                        name: ident("title", 260),
                                    },
                                    255,
                                    265,
                                )),
                            },
                            255,
                            265,
                        ),
                        span: sp(255, 265),
                    }],
                },
                245,
                266,
            )),
            245,
            266,
        ),
        // let window be 0..=n
        stmt(
            StmtKind::Let(LetStmt {
                mutable: false,
                names: vec![LetName {
                    name: ident("window", 279),
                    ty: None,
                    span: sp(0, 0),
                }],
                value: expr(
                    ExprKind::Range {
                        start: Box::new(int(0, 289, 290)),
                        end: Box::new(var("n", 294)),
                        inclusive: true,
                    },
                    289,
                    295,
                ),
                span: sp(275, 295),
            }),
            275,
            295,
        ),
        // if a: b else: c
        stmt(
            StmtKind::Expr(expr(
                ExprKind::If(IfExpr {
                    cond: Box::new(var("a", 303)),
                    then_branch: block(Vec::new(), Some(var("b", 306)), 306, 307),
                    else_branch: Some(Box::new(expr(
                        ExprKind::Block(block(Vec::new(), Some(var("c", 325)), 325, 326)),
                        325,
                        326,
                    ))),
                    span: sp(300, 326),
                }),
                300,
                326,
            )),
            300,
            326,
        ),
        // loop:
        //     loop:
        //         for x in 0..n:
        //             continue
        //             break x
        //             break
        //             return count
        //             return
        stmt(
            StmtKind::Expr(expr(
                ExprKind::Loop {
                    body: block(
                        vec![stmt(
                            StmtKind::Expr(expr(
                                ExprKind::Loop {
                                    body: block(
                                        vec![stmt(
                                            StmtKind::Expr(expr(
                                                ExprKind::For {
                                                    pattern: pat(
                                                        PatternKind::Binding {
                                                            mutable: false,
                                                            name: ident("x", 360),
                                                        },
                                                        360,
                                                        361,
                                                    ),
                                                    iter: Box::new(expr(
                                                        ExprKind::Range {
                                                            start: Box::new(int(0, 365, 366)),
                                                            end: Box::new(var("n", 368)),
                                                            inclusive: false,
                                                        },
                                                        365,
                                                        369,
                                                    )),
                                                    body: block(
                                                        vec![
                                                            stmt(StmtKind::Continue, 370, 378),
                                                            stmt(
                                                                StmtKind::Break(Some(var(
                                                                    "x", 385,
                                                                ))),
                                                                380,
                                                                386,
                                                            ),
                                                            stmt(StmtKind::Break(None), 390, 395),
                                                            stmt(
                                                                StmtKind::Return(Some(var(
                                                                    "count", 405,
                                                                ))),
                                                                398,
                                                                410,
                                                            ),
                                                            stmt(StmtKind::Return(None), 415, 421),
                                                        ],
                                                        None,
                                                        370,
                                                        421,
                                                    ),
                                                },
                                                351,
                                                421,
                                            )),
                                            351,
                                            421,
                                        )],
                                        None,
                                        351,
                                        421,
                                    ),
                                },
                                345,
                                421,
                            )),
                            345,
                            421,
                        )],
                        None,
                        345,
                        421,
                    ),
                },
                335,
                421,
            )),
            335,
            421,
        ),
        // borrowed doc   — the shared form, which auto-borrow usually makes
        // unnecessary and which stays legal where it clarifies.
        stmt(
            StmtKind::Expr(expr(
                ExprKind::Borrowed { mutable: false, expr: Box::new(var("doc", 439)) },
                430,
                442,
            )),
            430,
            442,
        ),
        // (a + b, a - b, .. ) — one of each binary operator.
        stmt(StmtKind::Expr(every_binary_op(450)), 450, 610),
        // (count is 0, count is not 0)
        //
        // The two forms of `Eq`, which are the only comparisons written as
        // words. The dump prints them under their internal names, `==` and
        // `!=`, which is the whole claim §5.4 makes about the word forms:
        // the spelling is English, the node is the operator.
        stmt(
            StmtKind::Expr(expr(
                ExprKind::Tuple(vec![
                    expr(
                        ExprKind::Binary {
                            op: BinaryOp::Eq,
                            lhs: Box::new(var("count", 620)),
                            rhs: Box::new(int(0, 629, 630)),
                        },
                        620,
                        630,
                    ),
                    expr(
                        ExprKind::Binary {
                            op: BinaryOp::Ne,
                            lhs: Box::new(var("count", 635)),
                            rhs: Box::new(int(0, 646, 647)),
                        },
                        635,
                        647,
                    ),
                ]),
                619,
                648,
            )),
            619,
            648,
        ),
        // A statement that failed to parse, and an expression that did: the
        // parser keeps both so later phases still see a tree.
        stmt(StmtKind::Error, 655, 665),
        stmt(StmtKind::Expr(expr(ExprKind::Error, 670, 680)), 670, 680),
        // (10, 0xa, 0o12, 0b1010, 1i8, .., 1.5f64)
        stmt(StmtKind::Expr(every_literal_form(700)), 700, 800),
    ];

    // (true, 'a', (), 1.5f64)
    let body = block(
        stmts,
        Some(expr(
            ExprKind::Tuple(vec![
                expr(ExprKind::Literal(Literal::Bool(true)), 811, 815),
                expr(ExprKind::Literal(Literal::Char('a')), 817, 820),
                expr(ExprKind::Unit, 822, 824),
                expr(
                    ExprKind::Literal(Literal::Float { value: 1.5, suffix: None }),
                    826,
                    829,
                ),
            ]),
            810,
            830,
        )),
        0,
        830,
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
                    // `None` is a binding until resolution says otherwise; a
                    // written `None()` is the empty variant.
                    pat(
                        PatternKind::Variant { path: path("None", 82), elems: Vec::new() },
                        82,
                        88,
                    ),
                    pat(PatternKind::Binding { mutable: false, name: ident("other", 91) }, 91, 96),
                ]),
                70,
                96,
            ),
            body: expr(ExprKind::Unit, 98, 100),
            span: sp(70, 100),
        },
        // A pattern that failed to parse keeps its arm in the tree.
        MatchArm {
            pattern: pat(PatternKind::Error, 104, 105),
            body: expr(ExprKind::Unit, 107, 109),
            span: sp(104, 109),
        },
    ];

    let match_expr = expr(
        ExprKind::Match(MatchExpr {
            scrutinee: Box::new(var("result", 6)),
            arms,
            span: sp(0, 109),
        }),
        0,
        109,
    );

    insta::assert_snapshot!(match_expr.dump());
}

// --- types ---------------------------------------------------------------

/// Types have their own corner of the tree; dump them on their own so a change
/// there is visible without a whole function around it. Every `TypeKind` is
/// here, including the const argument, which is a value in a list of types.
#[test]
fn dump_types() {
    let types = vec![
        // borrowed Doc
        borrowed_ty(false, ty("Doc", 9), 0, 12),
        // mutable borrowed Array of Doc
        borrowed_ty(true, generic_ty("Array", 31, 43, vec![ty("Doc", 40)]), 14, 43),
        // any Summarize
        Type { kind: TypeKind::Any(bound("Summarize", 49)), span: sp(45, 58) },
        // Map of (String, Array of (Map of (String, Int)))
        generic_ty(
            "Map",
            60,
            107,
            vec![
                ty("String", 68),
                generic_ty(
                    "Array",
                    76,
                    106,
                    vec![generic_ty("Map", 86, 105, vec![ty("String", 94), ty("Int", 102)])],
                ),
            ],
        ),
        // (Int, Bool)
        Type { kind: TypeKind::Tuple(vec![ty("Int", 111), ty("Bool", 116)]), span: sp(110, 121) },
        // (Int) — parentheses only group, so the type is the inner one and the
        // span starts after the `(`.
        ty("Int", 124),
        // ()
        Type { kind: TypeKind::Unit, span: sp(130, 132) },
        // Self
        Type { kind: TypeKind::SelfType, span: sp(134, 138) },
        // Self.Item
        Type { kind: TypeKind::SelfAssoc(ident("Item", 145)), span: sp(140, 149) },
        // Window of (Int, 4) — the `4` is a const generic argument.
        generic_ty(
            "Window",
            152,
            170,
            vec![
                ty("Int", 163),
                const_arg(
                    Literal::Int { value: 4, base: IntBase::Dec, suffix: None },
                    168,
                    169,
                ),
            ],
        ),
        // Quantity of (T, -1) — a negated const generic argument, the shape
        // every dimension vector in `scientific-libraries.md` §12.3 is made
        // of. The `-` and the digits carry separate spans.
        generic_ty(
            "Quantity",
            172,
            191,
            vec![ty("T", 185), neg_const_arg(1, 188, 190)],
        ),
        // Window of (Int, "a") — a const argument that is not an integer at
        // all. The parser accepts it and a later phase reports the kind
        // (`const-expression-arithmetic.md` §2.3), so the dump has to render
        // it; only the *negation* is restricted to integers.
        generic_ty(
            "Window",
            193,
            214,
            vec![ty("Int", 204), const_arg(Literal::Str("a".to_string()), 209, 212)],
        ),
        // text.parser.Token
        {
            let p = dotted_path(&["text", "parser", "Token"], 216);
            let span = p.span;
            Type { kind: TypeKind::Path(p), span }
        },
        // A type that failed to parse.
        Type { kind: TypeKind::Error, span: sp(236, 241) },
    ];

    let mut out = String::new();
    for t in &types {
        out.push_str(&t.dump());
    }
    insta::assert_snapshot!(out);
}

// --- extern blocks -------------------------------------------------------

/// Every `ExternItemKind`, the whole of `LibraryClause`, and the three states
/// the dump has to distinguish from a present value: a block with no library,
/// a union with no size, and an item that failed to parse.
///
/// Built by hand for the reason the module comment gives: the parser produces
/// `ExternItemKind::Error` only on a path a test would have to contrive, and a
/// printer nothing calls is a printer nothing is checking.
#[test]
fn dump_extern_blocks() {
    // unsafe extern "C" library "openblas" via pkg-config "openblas"
    //         kind static when available:
    let openblas = item(
        ItemKind::Extern(ExternBlock {
            is_unsafe: true,
            abi: StrLit { value: "C".to_string(), span: sp(14, 17) },
            library: Some(LibraryClause {
                name: StrLit { value: "openblas".to_string(), span: sp(26, 36) },
                pkg_config: Some(StrLit { value: "openblas".to_string(), span: sp(52, 62) }),
                static_link: Some(sp(63, 74)),
                when_available: Some(sp(75, 89)),
                span: sp(18, 89),
            }),
            items: vec![
                //     type BlasInt is I32
                ExternItem {
                    kind: ExternItemKind::Alias(ExternAlias {
                        name: ident("BlasInt", 100),
                        ty: ty("I32", 111),
                        span: sp(95, 114),
                    }),
                    span: sp(95, 114),
                },
                //     const CBLAS_ROW_MAJOR be 101 as CblasLayout
                ExternItem {
                    kind: ExternItemKind::Const(ExternConst {
                        name: ident("CBLAS_ROW_MAJOR", 125),
                        negative: false,
                        value: Literal::Int { value: 101, base: IntBase::Dec, suffix: None },
                        ty: ty("CblasLayout", 151),
                        span: sp(119, 162),
                    }),
                    span: sp(119, 162),
                },
                //     const H5I_BADID be -1 as Hid — a negative enumerator,
                //     which C has and a literal alone cannot spell.
                ExternItem {
                    kind: ExternItemKind::Const(ExternConst {
                        name: ident("H5I_BADID", 173),
                        negative: true,
                        value: Literal::Int { value: 1, base: IntBase::Dec, suffix: None },
                        ty: ty("Hid", 193),
                        span: sp(167, 196),
                    }),
                    span: sp(167, 196),
                },
                //     def dgemm(m: BlasInt) -> Herr symbol "dgemm_"
                ExternItem {
                    kind: ExternItemKind::Fn(ExternFn {
                        name: ident("dgemm", 210),
                        params: vec![Param {
                            name: ident("m", 216),
                            ty: ty("BlasInt", 219),
                            span: sp(216, 226),
                        }],
                        ret: Some(ty("Herr", 231)),
                        symbol: Some(StrLit {
                            value: "dgemm_".to_string(),
                            span: sp(243, 251),
                        }),
                        variadic: None,
                        span: sp(201, 251),
                    }),
                    span: sp(201, 251),
                },
                //     static H5T_NATIVE_DOUBLE_g: Hid
                ExternItem {
                    kind: ExternItemKind::Static(ExternStatic {
                        name: ident("H5T_NATIVE_DOUBLE_g", 263),
                        ty: ty("Hid", 284),
                        span: sp(256, 287),
                    }),
                    span: sp(256, 287),
                },
                //     union H5R_ref_t: size 64 align 8
                ExternItem {
                    kind: ExternItemKind::Union(ExternUnion {
                        name: ident("H5R_ref_t", 298),
                        size: Some(64),
                        align: Some(8),
                        span: sp(292, 324),
                    }),
                    span: sp(292, 324),
                },
            ],
            span: sp(0, 324),
        }),
        0,
        324,
    );

    // extern "C": — no `unsafe`, no library, and a body of two items the
    // parser could not read.
    let broken = item(
        ItemKind::Extern(ExternBlock {
            is_unsafe: false,
            abi: StrLit { value: "Fortran".to_string(), span: sp(339, 348) },
            library: None,
            items: vec![
                ExternItem { kind: ExternItemKind::Error, span: sp(355, 361) },
                //     union H5L_info2_t:  — neither number written.
                ExternItem {
                    kind: ExternItemKind::Union(ExternUnion {
                        name: ident("H5L_info2_t", 372),
                        size: None,
                        align: None,
                        span: sp(366, 384),
                    }),
                    span: sp(366, 384),
                },
            ],
            span: sp(332, 384),
        }),
        332,
        384,
    );

    // def call():
    //     unsafe:
    //         H5open()
    let call = item(
        ItemKind::Fn(FnDecl {
            is_pub: false,
            name: ident("call", 395),
            generics: Vec::new(),
            self_param: None,
            params: Vec::new(),
            ret: None,
            where_clause: Vec::new(),
            body: Some(block(
                Vec::new(),
                Some(expr(
                    ExprKind::Unsafe(block(
                        Vec::new(),
                        Some(expr(
                            ExprKind::Call {
                                callee: Box::new(var("H5open", 429)),
                                args: Vec::new(),
                            },
                            429,
                            437,
                        )),
                        429,
                        437,
                    )),
                    412,
                    437,
                )),
                412,
                437,
            )),
            span: sp(386, 437),
        }),
        386,
        437,
    );

    let module = Module { items: vec![openblas, broken, call], span: sp(0, 437) };
    insta::assert_snapshot!(module.dump());
}
