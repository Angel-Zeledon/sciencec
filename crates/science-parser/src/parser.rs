//! A recursive-descent parser over the lexer's token stream.
//!
//! # Error recovery
//!
//! The parser never stops at the first error. On a failure it reports a
//! diagnostic and synchronises: it drops tokens up to the end of the logical
//! line or the end of the enclosing indented block, and carries on. One pass
//! therefore reports several problems, which is the whole point of the
//! `Diagnostics` accumulator.
//!
//! # The four places the grammar is decided rather than discovered
//!
//! **Assignment is a statement, not an operator.** §4.4 keeps `=` out of the
//! precedence table on purpose: listing it would make
//! `let x = if c: a = b else: c` grammatical. So `parse_stmt` parses an
//! expression and, on finding a following `=`, treats the whole thing as an
//! assignment.
//!
//! **An inline body ends where the expression ends, not at the newline.**
//! §4.2 is explicit: in `if c: a else: b` the `then` body stops mid-line, at
//! `else`, because `else` cannot continue an expression. Recursive descent
//! gives that for free — the expression parser simply stops — and the same
//! mechanism gives the dangling `else` its innermost binding.
//!
//! **`&` is resolved by position.** In front of an operand it is a reference
//! (`&x`, `&mut x`); between two operands it is bitwise and. `parse_unary` is
//! only ever called where an operand is expected and the binary loop only ever
//! looks for an operator after one, so neither can see the other's `&`.
//!
//! **Two ambiguities are left for name resolution**, because §4.4 says they
//! cannot be settled by syntax: `Doc()` parses as a call, not as a struct with
//! no fields, and a bare name in a pattern parses as a binding, not as a unit
//! variant. The same rule decides `Doc(title: "a")` against `f(1)`: named
//! arguments mean a struct, positional ones mean a call or a variant.
//!
//! A third ambiguity follows from `.` serving as both the path separator and
//! the field-access operator: `Option.Some(x)` (§4.5's qualified variant) is
//! written exactly like a method call, and `Doc.new("a")` (§4.3's associated
//! function) exactly like one too. Both parse as `MethodCall`; resolution
//! reclassifies them. A path in expression position is therefore one segment
//! long, and every `.` after it belongs to the postfix chain.

use science_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span};
use science_lexer::{ReservedWord, Token, TokenKind};

use crate::ast::*;

/// Syntax diagnostics, which §9 of the spec assigns the range SC0100-SC0199.
pub mod codes {
    use science_diagnostics::Code;

    /// A token that can neither start nor continue what was being parsed.
    pub const UNEXPECTED_TOKEN: Code = Code(100);
    /// Something at the top level of a module that is not a declaration.
    pub const EXPECTED_ITEM: Code = Code(101);
    /// A name was required here.
    pub const EXPECTED_IDENT: Code = Code(102);
    /// A `:` did not introduce a block, or the block that followed was not
    /// shaped the way §4.2 requires.
    pub const EXPECTED_BLOCK: Code = Code(103);
    /// A type expression was required here.
    pub const EXPECTED_TYPE: Code = Code(104);
    /// An expression was required here.
    pub const EXPECTED_EXPR: Code = Code(105);
    /// A pattern was required here.
    pub const EXPECTED_PATTERN: Code = Code(106);
    /// `pub` in front of something that cannot be public.
    pub const MISPLACED_PUB: Code = Code(107);
    /// A `self` receiver somewhere other than first in the parameter list.
    pub const MISPLACED_RECEIVER: Code = Code(108);
    /// A statement where §4.2 requires an inline block's single expression.
    pub const STATEMENT_IN_INLINE_BLOCK: Code = Code(109);
    /// Named arguments in front of something that is not a struct's name.
    pub const MISPLACED_NAMED_ARGUMENT: Code = Code(110);
    /// `impl <something that is not a path> for ..`.
    pub const EXPECTED_TRAIT: Code = Code(111);
    /// Something in a `trait` or `impl` body that is not a method.
    pub const EXPECTED_METHOD: Code = Code(112);
}

/// Parses a token stream into a module, along with everything that went wrong.
///
/// `file` is needed separately because an empty stream still has to produce
/// spans, and a span without a file cannot be rendered.
pub fn parse_module(tokens: &[Token], file: FileId) -> (Module, Diagnostics) {
    let mut parser = Parser::new(tokens, file);
    let module = parser.parse_module();
    (module, parser.into_diagnostics())
}

pub struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
    /// Returned by `peek` once the stream runs out, so no lookahead needs a
    /// bounds check.
    eof: Token,
    diagnostics: Diagnostics,
}

impl<'t> Parser<'t> {
    pub fn new(tokens: &'t [Token], file: FileId) -> Self {
        let end = tokens.last().map(|t| t.span.end).unwrap_or(0);
        Parser {
            tokens,
            pos: 0,
            eof: Token::new(TokenKind::Eof, Span::at(file, end)),
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn into_diagnostics(self) -> Diagnostics {
        self.diagnostics
    }

    // --- cursor ----------------------------------------------------------

    fn token_at(&self, offset: usize) -> &Token {
        self.tokens.get(self.pos + offset).unwrap_or(&self.eof)
    }

    fn peek(&self) -> &TokenKind {
        &self.token_at(0).kind
    }

    fn peek_ahead(&self, offset: usize) -> &TokenKind {
        &self.token_at(offset).kind
    }

    /// The span of the token about to be consumed.
    fn span(&self) -> Span {
        self.token_at(0).span
    }

    /// The span of the token just consumed.
    fn prev_span(&self) -> Span {
        if self.pos == 0 {
            self.span()
        } else {
            self.tokens[self.pos - 1].span
        }
    }

    /// The span of the last consumed token that stands for written text, which
    /// is what closes a node.
    ///
    /// `Newline`, `Indent` and `Dedent` are markers for a change in line
    /// structure rather than for any text, and the lexer places them at the
    /// start of the following line. Merging one into a node's span would
    /// stretch that node past the last thing the programmer actually wrote,
    /// and every diagnostic that points at the node would then underline a
    /// line that has nothing to do with it.
    fn last_text_span(&self) -> Span {
        for token in self.tokens[..self.pos].iter().rev() {
            if !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                return token.span;
            }
        }
        self.prev_span()
    }

    fn advance(&mut self) -> Token {
        let token = self.token_at(0).clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.at(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consumes `kind`, or reports that it was missing and consumes nothing.
    ///
    /// `what` names the expected token the way it reads in a message, e.g.
    /// ``"`:`"`` or `"an indented block"`.
    fn expect(&mut self, kind: &TokenKind, what: &str) -> Option<Token> {
        if self.at(kind) {
            return Some(self.advance());
        }
        let found = describe(self.peek());
        let span = self.span();
        self.error(codes::UNEXPECTED_TOKEN, format!("expected {what}, found {found}"), span);
        None
    }

    fn expect_ident(&mut self) -> Option<Ident> {
        if let TokenKind::Ident(name) = self.peek() {
            let name = name.clone();
            let span = self.advance().span;
            return Some(Ident::new(name, span));
        }
        let found = describe(self.peek());
        let span = self.span();
        self.error(codes::EXPECTED_IDENT, format!("expected an identifier, found {found}"), span);
        None
    }

    fn skip_newlines(&mut self) {
        while self.at(&TokenKind::Newline) {
            self.advance();
        }
    }

    /// Whether the logical line has already ended at the token just consumed.
    ///
    /// Two things end one. A `Newline`, obviously — a bodiless `fn` in a trait
    /// consumes its own, and the enclosing list then has nothing left to take.
    /// And a `Dedent`: a statement whose last token closed an indented block
    /// has had its newline emitted *inside* that block, before the `Dedent`, so
    /// `if c:` with an indented body is one logical line with nothing trailing
    /// it.
    fn prev_ends_line(&self) -> bool {
        self.pos > 0
            && matches!(self.tokens[self.pos - 1].kind, TokenKind::Dedent | TokenKind::Newline)
    }

    /// A logical line has to end where the parser thinks it does.
    fn expect_line_end(&mut self) {
        match self.peek() {
            TokenKind::Newline => {
                self.advance();
            }
            TokenKind::Dedent | TokenKind::Eof => {}
            _ if self.prev_ends_line() => {}
            _ => {
                let found = describe(self.peek());
                let span = self.span();
                self.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("expected end of line, found {found}"),
                    span,
                );
                self.synchronize();
            }
        }
    }

    // --- diagnostics -----------------------------------------------------

    fn error(&mut self, code: Code, message: impl Into<String>, span: Span) {
        let message = message.into();
        self.diagnostics
            .push(Diagnostic::error(code, message.clone()).with_label(Label::primary(span, message)));
    }

    // --- error recovery --------------------------------------------------

    /// Drops tokens up to the end of the logical line, or to the end of the
    /// enclosing block, so parsing can resume at the next thing that could
    /// plausibly start a declaration or a statement.
    ///
    /// A `Dedent` is left in place: it belongs to whoever opened the block.
    /// An indented region that follows the broken line is dropped with it,
    /// since it was that line's body.
    fn synchronize(&mut self) {
        loop {
            match self.peek() {
                TokenKind::Eof | TokenKind::Dedent => return,
                TokenKind::Newline => {
                    self.advance();
                    if self.at(&TokenKind::Indent) {
                        self.skip_indented_region();
                    }
                    return;
                }
                TokenKind::Indent => self.skip_indented_region(),
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Consumes a whole `Indent .. Dedent` region, nested regions included.
    fn skip_indented_region(&mut self) {
        if self.eat(&TokenKind::Indent).is_none() {
            return;
        }
        let mut depth = 1usize;
        loop {
            match self.peek() {
                TokenKind::Eof => return,
                TokenKind::Indent => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Dedent => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 {
                        return;
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Recovery inside a bracketed list, where the lexer emits no `Newline`
    /// and the only useful landmarks are the separators.
    fn recover_in_brackets(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                TokenKind::Eof | TokenKind::Newline | TokenKind::Dedent => return,
                TokenKind::Comma if depth == 0 => return,
                TokenKind::RParen | TokenKind::RBracket if depth == 0 => return,
                TokenKind::LParen | TokenKind::LBracket => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RParen | TokenKind::RBracket => {
                    depth -= 1;
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // --- module ----------------------------------------------------------

    pub fn parse_module(&mut self) -> Module {
        let span = match (self.tokens.first(), self.tokens.last()) {
            (Some(first), Some(last)) => first.span.merge(last.span),
            _ => self.eof.span,
        };

        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Indent => {
                    let span = self.span();
                    self.error(
                        codes::UNEXPECTED_TOKEN,
                        "unexpected indentation at the top level of a module",
                        span,
                    );
                    self.skip_indented_region();
                }
                TokenKind::Dedent => {
                    self.advance();
                }
                _ => match self.parse_item() {
                    Some(item) => items.push(item),
                    None => self.synchronize(),
                },
            }
        }

        Module { items, span }
    }

    fn parse_item(&mut self) -> Option<Item> {
        let start = self.span();
        let pub_span = self.eat(&TokenKind::Pub).map(|t| t.span);
        let is_pub = pub_span.is_some();

        // Dispatching with `at` rather than a `match` on `peek` keeps the
        // borrow of the token from overlapping the parse call in each arm.
        let kind = if self.at(&TokenKind::Fn) {
            ItemKind::Fn(self.parse_fn(is_pub, start)?)
        } else if self.at(&TokenKind::Struct) {
            ItemKind::Struct(self.parse_struct(is_pub, start)?)
        } else if self.at(&TokenKind::Enum) {
            ItemKind::Enum(self.parse_enum(is_pub, start)?)
        } else if self.at(&TokenKind::Trait) {
            ItemKind::Trait(self.parse_trait(is_pub, start)?)
        } else if self.at(&TokenKind::Impl) {
            self.reject_pub(pub_span, "an `impl` block");
            ItemKind::Impl(self.parse_impl(start)?)
        } else if self.at(&TokenKind::Use) {
            self.reject_pub(pub_span, "a `use` declaration");
            ItemKind::Use(self.parse_use(start)?)
        } else {
            let found = describe(self.peek());
            let span = self.span();
            self.error(
                codes::EXPECTED_ITEM,
                format!(
                    "expected a declaration (`fn`, `struct`, `enum`, `trait`, `impl` or `use`), \
                     found {found}"
                ),
                span,
            );
            return None;
        };

        Some(Item { kind, span: start.merge(self.last_text_span()) })
    }

    fn reject_pub(&mut self, pub_span: Option<Span>, what: &str) {
        if let Some(span) = pub_span {
            self.error(codes::MISPLACED_PUB, format!("`pub` has no meaning on {what}"), span);
        }
    }

    // --- functions -------------------------------------------------------

    fn parse_fn(&mut self, is_pub: bool, start: Span) -> Option<FnDecl> {
        self.advance(); // `fn`
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();
        let (self_param, params) = self.parse_params();
        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        let where_clause = self.parse_where_clause();

        // No `:` means no body, which is exactly a trait's required method.
        let body = if self.at(&TokenKind::Colon) {
            Some(self.parse_block())
        } else if self.at(&TokenKind::Newline)
            && matches!(self.peek_ahead(1), TokenKind::Indent)
        {
            // An indented body with nothing introducing it is a missing `:`.
            // Saying so here is worth the special case: the alternative is
            // complaining about stray indentation a line later, which points
            // at the body instead of at the signature.
            let span = self.span();
            self.error(codes::EXPECTED_BLOCK, "expected `:` before the function body", span);
            self.advance();
            self.skip_indented_region();
            None
        } else {
            self.expect_line_end();
            None
        };

        Some(FnDecl {
            is_pub,
            name,
            generics,
            self_param,
            params,
            ret,
            where_clause,
            body,
            span: start.merge(self.last_text_span()),
        })
    }

    /// `(a: T, b: U)`, optionally opening with a `self` receiver.
    fn parse_params(&mut self) -> (Option<SelfParam>, Vec<Param>) {
        let mut receiver = None;
        let mut params = Vec::new();

        if self.expect(&TokenKind::LParen, "`(`").is_none() {
            return (receiver, params);
        }

        while !self.at(&TokenKind::RParen) {
            if let Some(found) = self.try_parse_self_param() {
                if receiver.is_none() && params.is_empty() {
                    receiver = Some(found);
                } else {
                    self.error(
                        codes::MISPLACED_RECEIVER,
                        "a `self` receiver must be the first parameter",
                        found.span,
                    );
                }
            } else {
                match self.parse_param() {
                    Some(param) => params.push(param),
                    None => self.recover_in_brackets(),
                }
            }

            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }

        self.expect(&TokenKind::RParen, "`)`");
        (receiver, params)
    }

    fn try_parse_self_param(&mut self) -> Option<SelfParam> {
        let start = self.span();
        if self.at(&TokenKind::SelfValue) {
            self.advance();
            return Some(SelfParam { kind: SelfKind::Value, span: start });
        }
        if !self.at(&TokenKind::Amp) {
            return None;
        }
        match (self.peek_ahead(1), self.peek_ahead(2)) {
            (TokenKind::SelfValue, _) => {
                self.advance();
                self.advance();
                Some(SelfParam { kind: SelfKind::Ref, span: start.merge(self.last_text_span()) })
            }
            (TokenKind::Mut, TokenKind::SelfValue) => {
                self.advance();
                self.advance();
                self.advance();
                Some(SelfParam { kind: SelfKind::RefMut, span: start.merge(self.last_text_span()) })
            }
            _ => None,
        }
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.span();
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type();
        Some(Param { name, ty, span: start.merge(self.last_text_span()) })
    }

    // --- structs and enums -----------------------------------------------

    fn parse_struct(&mut self, is_pub: bool, start: Span) -> Option<StructDecl> {
        self.advance(); // `struct`
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();
        let where_clause = self.parse_where_clause();
        let fields = self.parse_indented_body(Self::parse_field);
        Some(StructDecl {
            is_pub,
            name,
            generics,
            where_clause,
            fields,
            span: start.merge(self.last_text_span()),
        })
    }

    fn parse_field(&mut self) -> Option<FieldDef> {
        let start = self.span();
        let is_pub = self.eat(&TokenKind::Pub).is_some();
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type();
        Some(FieldDef { is_pub, name, ty, span: start.merge(self.last_text_span()) })
    }

    fn parse_enum(&mut self, is_pub: bool, start: Span) -> Option<EnumDecl> {
        self.advance(); // `enum`
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();
        let where_clause = self.parse_where_clause();
        let variants = self.parse_indented_body(Self::parse_variant);
        Some(EnumDecl {
            is_pub,
            name,
            generics,
            where_clause,
            variants,
            span: start.merge(self.last_text_span()),
        })
    }

    /// A variant with a positional payload; F0 has no named-field variants.
    fn parse_variant(&mut self) -> Option<VariantDef> {
        let start = self.span();
        let name = self.expect_ident()?;
        let mut payload = Vec::new();
        if self.eat(&TokenKind::LParen).is_some() {
            while !self.at(&TokenKind::RParen) {
                payload.push(self.parse_type());
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`");
        }
        Some(VariantDef { name, payload, span: start.merge(self.last_text_span()) })
    }

    // --- use -------------------------------------------------------------

    fn parse_use(&mut self, start: Span) -> Option<UseDecl> {
        self.advance(); // `use`
        let path = self.parse_path()?;
        let imports = if self.eat(&TokenKind::LParen).is_some() {
            let mut names = Vec::new();
            while !self.at(&TokenKind::RParen) {
                match self.expect_ident() {
                    Some(name) => names.push(name),
                    None => self.recover_in_brackets(),
                }
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`");
            Some(names)
        } else {
            None
        };
        self.expect_line_end();
        Some(UseDecl { path, imports, span: start.merge(self.last_text_span()) })
    }

    // --- generics and bounds ---------------------------------------------

    /// `[T, U: Ord + Clone]`, or nothing at all.
    fn parse_generic_params(&mut self) -> Vec<GenericParam> {
        let mut params = Vec::new();
        if self.eat(&TokenKind::LBracket).is_none() {
            return params;
        }
        while !self.at(&TokenKind::RBracket) {
            let start = self.span();
            let Some(name) = self.expect_ident() else {
                self.recover_in_brackets();
                if self.eat(&TokenKind::Comma).is_some() {
                    continue;
                }
                break;
            };
            let bounds = if self.eat(&TokenKind::Colon).is_some() {
                self.parse_bounds()
            } else {
                Vec::new()
            };
            params.push(GenericParam { name, bounds, span: start.merge(self.last_text_span()) });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "`]`");
        params
    }

    /// `A + B + C`. The list ends at anything that is not a `+`, which is what
    /// lets a `where` clause end on the `:` that opens the block.
    fn parse_bounds(&mut self) -> Vec<TypeBound> {
        let mut bounds = Vec::new();
        while let Some(bound) = self.parse_type_bound() {
            bounds.push(bound);
            if self.eat(&TokenKind::Plus).is_none() {
                break;
            }
        }
        bounds
    }

    fn parse_type_bound(&mut self) -> Option<TypeBound> {
        let path = self.parse_path()?;
        let span = path.span;
        Some(TypeBound { path, span })
    }

    /// `where T: A + B, U: C`.
    ///
    /// A predicate's bound list ends at a `,` (another predicate follows) or at
    /// anything else, which in a declaration is the `:` that opens the body.
    /// There is no other way to tell those two colons apart.
    fn parse_where_clause(&mut self) -> Vec<WherePredicate> {
        let mut predicates = Vec::new();
        if self.eat(&TokenKind::Where).is_none() {
            return predicates;
        }
        loop {
            let start = self.span();
            let ty = self.parse_type();
            if self.expect(&TokenKind::Colon, "`:`").is_none() {
                break;
            }
            let bounds = self.parse_bounds();
            predicates.push(WherePredicate { ty, bounds, span: start.merge(self.last_text_span()) });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        predicates
    }

    // --- paths and types -------------------------------------------------

    fn parse_path(&mut self) -> Option<Path> {
        let first = self.parse_path_segment()?;
        let mut span = first.span;
        let mut segments = vec![first];
        // A `.` only continues the path when a name follows; otherwise it is
        // field access and belongs to whoever called us.
        while self.at(&TokenKind::Dot) && matches!(self.peek_ahead(1), TokenKind::Ident(_)) {
            self.advance();
            let segment = self.parse_path_segment()?;
            span = span.merge(segment.span);
            segments.push(segment);
        }
        Some(Path { segments, span })
    }

    fn parse_path_segment(&mut self) -> Option<PathSegment> {
        let name = self.expect_ident()?;
        let mut span = name.span;
        let mut generics = Vec::new();
        if self.at(&TokenKind::LBracket) {
            generics = self.parse_generic_args();
            span = span.merge(self.last_text_span());
        }
        Some(PathSegment { name, generics, span })
    }

    /// `[A, B]` in a type or a path.
    fn parse_generic_args(&mut self) -> Vec<Type> {
        let mut args = Vec::new();
        if self.eat(&TokenKind::LBracket).is_none() {
            return args;
        }
        while !self.at(&TokenKind::RBracket) {
            args.push(self.parse_type());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "`]`");
        args
    }

    /// A type expression: `&T`, `&mut T`, `dyn Trait`, `Array[T]`, `(A, B)`,
    /// `()`, `Self`, and dotted paths.
    ///
    /// Always returns a node. A failure becomes `TypeKind::Error`, so the tree
    /// keeps its shape and the phases after this one still have something to
    /// walk.
    fn parse_type(&mut self) -> Type {
        let start = self.span();
        match self.peek() {
            TokenKind::Amp => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut).is_some();
                let inner = self.parse_type();
                Type {
                    kind: TypeKind::Ref { mutable, inner: Box::new(inner) },
                    span: start.merge(self.last_text_span()),
                }
            }
            TokenKind::Dyn => {
                self.advance();
                match self.parse_type_bound() {
                    Some(bound) => Type {
                        kind: TypeKind::Dyn(bound),
                        span: start.merge(self.last_text_span()),
                    },
                    None => Type { kind: TypeKind::Error, span: start },
                }
            }
            TokenKind::SelfType => {
                self.advance();
                Type { kind: TypeKind::SelfType, span: start }
            }
            TokenKind::LParen => self.parse_paren_type(start),
            TokenKind::Ident(_) => match self.parse_path() {
                Some(path) => {
                    let span = path.span;
                    Type { kind: TypeKind::Path(path), span }
                }
                None => Type { kind: TypeKind::Error, span: start },
            },
            _ => {
                let found = describe(self.peek());
                self.error(
                    codes::EXPECTED_TYPE,
                    format!("expected a type, found {found}"),
                    start,
                );
                // Drop the offending token so the enclosing list can carry on,
                // unless it is the very delimiter that list is waiting for.
                if !self.at_list_boundary() {
                    self.advance();
                }
                Type { kind: TypeKind::Error, span: start }
            }
        }
    }

    /// `()`, `(T)` and `(A, B)` all start the same way.
    fn parse_paren_type(&mut self, start: Span) -> Type {
        self.advance(); // `(`
        if self.eat(&TokenKind::RParen).is_some() {
            return Type { kind: TypeKind::Unit, span: start.merge(self.last_text_span()) };
        }

        let first = self.parse_type();
        if !self.at(&TokenKind::Comma) {
            // `(T)` is just `T`: parentheses only group, so no node of their
            // own. The inner type keeps its own span, which is what a
            // diagnostic should point at.
            self.expect(&TokenKind::RParen, "`)`");
            return first;
        }

        let mut elems = vec![first];
        while self.eat(&TokenKind::Comma).is_some() {
            if self.at(&TokenKind::RParen) {
                break;
            }
            elems.push(self.parse_type());
        }
        self.expect(&TokenKind::RParen, "`)`");
        Type { kind: TypeKind::Tuple(elems), span: start.merge(self.last_text_span()) }
    }

    /// Tokens that close or separate a list, which recovery must never eat.
    fn at_list_boundary(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::Arrow
                | TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::Eof
        )
    }

    // --- blocks ----------------------------------------------------------

    /// The indented body of a declaration whose contents are one item per
    /// line: a struct's fields, an enum's variants, a trait's methods.
    ///
    /// This is not a `Block`: those lines are declarations, not statements.
    fn parse_indented_body<T>(&mut self, parse_one: fn(&mut Self) -> Option<T>) -> Vec<T> {
        let mut items = Vec::new();
        if self.expect(&TokenKind::Colon, "`:`").is_none() {
            return items;
        }
        if self.expect(&TokenKind::Newline, "end of line").is_none() {
            return items;
        }
        if self.expect(&TokenKind::Indent, "an indented block").is_none() {
            return items;
        }

        loop {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }
            match parse_one(self) {
                Some(item) => {
                    items.push(item);
                    self.expect_line_end();
                }
                None => self.synchronize(),
            }
        }

        self.eat(&TokenKind::Dedent);
        items
    }

    /// A block of statements, in either of §4.2's two forms:
    ///
    /// - indented: `:` NEWLINE INDENT statements DEDENT
    /// - inline: `:` followed by a single expression
    ///
    /// The inline form ends where the *expression* ends, not at the end of the
    /// line. That is the whole of §4.2's rule, and the reason `if c: a else: b`
    /// works: `else` cannot continue an expression, so the `then` body stops in
    /// front of it. "Fits on one line" is a consequence, never a test.
    ///
    /// The returned span covers the block's contents, not the `:`.
    fn parse_block(&mut self) -> Block {
        let colon = self.span();
        if self.expect(&TokenKind::Colon, "`:`").is_none() {
            return empty_block(colon);
        }

        if self.eat(&TokenKind::Newline).is_some() {
            // `expect` consumes the `Indent` itself.
            if self.expect(&TokenKind::Indent, "an indented block after `:`").is_none() {
                return empty_block(self.span());
            }
            return self.parse_indented_block();
        }

        self.parse_inline_block()
    }

    /// The statements between an `Indent` and its `Dedent`, both already
    /// located by the caller.
    fn parse_indented_block(&mut self) -> Block {
        let start = self.span();
        let mut stmts = Vec::new();

        loop {
            self.skip_newlines();
            match self.peek() {
                TokenKind::Dedent | TokenKind::Eof => break,
                TokenKind::Indent => {
                    let span = self.span();
                    self.error(codes::UNEXPECTED_TOKEN, "unexpected indentation", span);
                    self.skip_indented_region();
                    continue;
                }
                _ => {}
            }

            let before = self.pos;
            match self.parse_stmt() {
                Some(stmt) => {
                    stmts.push(stmt);
                    self.expect_line_end();
                }
                None => self.synchronize(),
            }
            // Recovery is supposed to consume something; if a future edit ever
            // makes it stop doing so, a hang is a far worse failure than a
            // dropped token.
            if self.pos == before {
                self.advance();
            }
        }

        self.eat(&TokenKind::Dedent);
        finish_block(stmts, start)
    }

    /// The single expression of an inline block.
    ///
    /// §4.2 makes a statement here an error and gives `fn f(): let x = 1` as
    /// the example, which is what `SC0109` reports.
    ///
    /// Assignment, `return`, `break` and `continue` are still accepted, and the
    /// spec writes all four inline itself: `if item > best: best = item` in
    /// §4.3, and `if n < 0: return -1` and `loop: break` in the corpus. Each of
    /// them is an expression of type `Never` (§4.5) in every respect except
    /// where the AST happens to file it — `StmtKind` rather than `ExprKind` —
    /// and a filing decision is not what §4.2 is ruling on. A `let` has no such
    /// reading: it binds a name in a scope, and an inline body has none.
    fn parse_inline_block(&mut self) -> Block {
        let start = self.span();
        if !self.at_stmt_start() {
            self.error(
                codes::EXPECTED_BLOCK,
                "expected an indented block or a single expression after `:`",
                start,
            );
            return empty_block(start);
        }

        if self.at(&TokenKind::Let) {
            self.error(
                codes::STATEMENT_IN_INLINE_BLOCK,
                "the body of an inline block must be an expression, and a `let` binding is a \
                 statement; write the body as an indented block instead",
                start,
            );
        }

        let stmts = match self.parse_stmt() {
            Some(stmt) => vec![stmt],
            None => {
                self.synchronize();
                Vec::new()
            }
        };
        finish_block(stmts, start)
    }

    /// The body of a `match` arm, which §4.4 makes an expression rather than a
    /// block so that an inline arm carries the expression itself.
    fn parse_arm_body(&mut self) -> Expr {
        let colon = self.span();
        if self.expect(&TokenKind::Colon, "`:`").is_none() {
            return error_expr(colon);
        }

        if self.eat(&TokenKind::Newline).is_some() {
            if self.expect(&TokenKind::Indent, "an indented block after `:`").is_none() {
                return error_expr(self.span());
            }
            let block = self.parse_indented_block();
            let span = block.span;
            return Expr { kind: ExprKind::Block(block), span };
        }

        let block = self.parse_inline_block();
        // An inline arm is its expression; wrapping it in a block would add a
        // node that stands for nothing written.
        if block.stmts.is_empty() {
            if let Some(tail) = block.tail {
                return *tail;
            }
        }
        let span = block.span;
        Expr { kind: ExprKind::Block(block), span }
    }

    // --- statements ------------------------------------------------------

    /// One statement: `let`, an assignment, `return`, `break`, `continue`, or
    /// an expression evaluated for its effect.
    ///
    /// The line terminator is the caller's, because a statement ending in an
    /// indented block has already consumed its own.
    fn parse_stmt(&mut self) -> Option<Stmt> {
        let start = self.span();
        match self.peek() {
            TokenKind::Let => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut).is_some();
                let name = self.expect_ident()?;
                let ty = if self.eat(&TokenKind::Colon).is_some() {
                    Some(self.parse_type())
                } else {
                    None
                };
                // F0 has no `let` without an initialiser: `LetStmt::value` is
                // not an `Option`, and inference is local (§5.2), so a
                // binding with no value has nothing to infer from.
                self.expect(&TokenKind::Eq, "`=`")?;
                let value = self.parse_expr();
                let span = start.merge(self.last_text_span());
                Some(Stmt { kind: StmtKind::Let(LetStmt { mutable, name, ty, value, span }), span })
            }
            TokenKind::Return => {
                self.advance();
                let value = if self.at_expr_start() { Some(self.parse_expr()) } else { None };
                let span = start.merge(self.last_text_span());
                Some(Stmt { kind: StmtKind::Return(value), span })
            }
            TokenKind::Break => {
                self.advance();
                let value = if self.at_expr_start() { Some(self.parse_expr()) } else { None };
                let span = start.merge(self.last_text_span());
                Some(Stmt { kind: StmtKind::Break(value), span })
            }
            TokenKind::Continue => {
                self.advance();
                Some(Stmt { kind: StmtKind::Continue, span: start })
            }
            _ => {
                let expr = self.parse_expr();
                if self.eat(&TokenKind::Eq).is_some() {
                    let value = self.parse_expr();
                    let span = start.merge(self.last_text_span());
                    return Some(Stmt { kind: StmtKind::Assign { target: expr, value }, span });
                }
                let span = expr.span;
                Some(Stmt { kind: StmtKind::Expr(expr), span })
            }
        }
    }

    /// Whether the current token can begin a statement.
    fn at_stmt_start(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Let | TokenKind::Return | TokenKind::Break | TokenKind::Continue
        ) || self.at_expr_start()
    }

    /// Whether the current token can begin an expression.
    ///
    /// Used where an expression is optional — after `return` and `break` — and
    /// to tell an empty inline body from a real one.
    fn at_expr_start(&self) -> bool {
        use TokenKind::*;
        matches!(
            self.peek(),
            Int { .. }
                | Float { .. }
                | Str(_)
                | Char(_)
                | True
                | False
                | Ident(_)
                | SelfValue
                | LParen
                | Minus
                | Not
                | Amp
                | If
                | Match
                | While
                | Loop
                | For
        )
    }

    // --- expressions -----------------------------------------------------

    /// An expression, by precedence climbing over §4.4's table.
    ///
    /// Always returns a node: a failure becomes `ExprKind::Error`, so the tree
    /// keeps its shape and the phases after this one still have something to
    /// walk. `=` is not in the table; see the note at the top of this module.
    fn parse_expr(&mut self) -> Expr {
        self.parse_binary(1)
    }

    /// One rung of the table and everything above it.
    ///
    /// Every row is left-associative, which is why the recursive call asks for
    /// `prec + 1`: a second operator of the same row cannot be absorbed by the
    /// right-hand side and is left for this loop.
    fn parse_binary(&mut self, min_prec: u8) -> Expr {
        let start = self.span();
        let mut lhs = self.parse_cast();
        while let Some((op, prec)) = binary_op(self.peek()) {
            if prec < min_prec {
                break;
            }
            self.advance();
            let rhs = self.parse_binary(prec + 1);
            let span = start.merge(self.last_text_span());
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                span,
            };
        }
        lhs
    }

    /// `as`, which sits between the unary operators and `*`.
    fn parse_cast(&mut self) -> Expr {
        let start = self.span();
        let mut expr = self.parse_unary();
        while self.eat(&TokenKind::As).is_some() {
            let ty = self.parse_type();
            let span = start.merge(self.last_text_span());
            expr = Expr { kind: ExprKind::Cast { expr: Box::new(expr), ty }, span };
        }
        expr
    }

    /// `-`, `not`, `&` and `&mut`, all of them prefix.
    ///
    /// This is the only place a `&` is read as a reference, and it is only ever
    /// reached where an operand is expected — which is the whole of the rule
    /// that tells `&x` from `a & b`.
    fn parse_unary(&mut self) -> Expr {
        let start = self.span();
        match self.peek() {
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Unary { op: UnaryOp::Neg, operand: Box::new(operand) }, span }
            }
            TokenKind::Not => {
                self.advance();
                let operand = self.parse_unary();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(operand) }, span }
            }
            TokenKind::Amp => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut).is_some();
                let inner = self.parse_unary();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Ref { mutable, expr: Box::new(inner) }, span }
            }
            _ => self.parse_postfix(),
        }
    }

    /// The tightest row: call, index, field access and `?`, applied left to
    /// right to whatever precedes them.
    fn parse_postfix(&mut self) -> Expr {
        let start = self.span();
        let mut expr = self.parse_primary();

        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.advance();
                    let Some(name) = self.expect_ident() else {
                        return error_expr(start.merge(self.last_text_span()));
                    };
                    expr = if self.at(&TokenKind::LParen) {
                        if self.at_named_args() {
                            self.struct_lit(expr, Some(name), start)
                        } else {
                            let args = self.parse_call_args();
                            let span = start.merge(self.last_text_span());
                            Expr {
                                kind: ExprKind::MethodCall {
                                    receiver: Box::new(expr),
                                    method: name,
                                    generics: Vec::new(),
                                    args,
                                },
                                span,
                            }
                        }
                    } else {
                        let span = start.merge(self.last_text_span());
                        Expr { kind: ExprKind::Field { base: Box::new(expr), name }, span }
                    };
                }
                TokenKind::LParen => {
                    expr = if self.at_named_args() {
                        self.struct_lit(expr, None, start)
                    } else {
                        let args = self.parse_call_args();
                        let span = start.merge(self.last_text_span());
                        Expr { kind: ExprKind::Call { callee: Box::new(expr), args }, span }
                    };
                }
                TokenKind::LBracket => {
                    // `[` after a name is a generic instantiation —
                    // `Array[Int].new()` (§4.3) — and after anything else it is
                    // an index. Nothing in the token stream separates
                    // `Array[Int]` from `items[0]`, so position has to: §8
                    // gives `Array` and `Map` no indexing operator at all, and
                    // the instantiation form is the only one that ever follows
                    // a bare name. The index row of §4.4's table is kept for
                    // the case a type does define one, where the receiver is
                    // the result of a call or a field rather than a name.
                    if instantiable_path(&expr) {
                        let generics = self.parse_generic_args();
                        let end = self.last_text_span();
                        if let ExprKind::Path(path) = &mut expr.kind {
                            if let Some(segment) = path.segments.last_mut() {
                                segment.generics = generics;
                                segment.span = segment.span.merge(end);
                            }
                            path.span = path.span.merge(end);
                        }
                        expr.span = start.merge(end);
                    } else {
                        self.advance();
                        let index = self.parse_expr();
                        self.expect(&TokenKind::RBracket, "`]`");
                        let span = start.merge(self.last_text_span());
                        expr = Expr {
                            kind: ExprKind::Index { base: Box::new(expr), index: Box::new(index) },
                            span,
                        };
                    }
                }
                TokenKind::Question => {
                    self.advance();
                    let span = start.merge(self.last_text_span());
                    expr = Expr { kind: ExprKind::Try(Box::new(expr)), span };
                }
                _ => break,
            }
        }

        expr
    }

    /// `Doc(title: "a")`: the argument list is named, so this is construction.
    ///
    /// `extra` is the segment of a qualified name, as in `text.Doc(title: "a")`,
    /// where the postfix loop has already taken the `.` and the name.
    fn struct_lit(&mut self, base: Expr, extra: Option<Ident>, start: Span) -> Expr {
        let ExprKind::Path(mut path) = base.kind else {
            let span = self.span();
            self.error(
                codes::MISPLACED_NAMED_ARGUMENT,
                "named arguments construct a struct, so they need a struct's name in front of them",
                span,
            );
            self.parse_field_inits();
            return error_expr(start.merge(self.last_text_span()));
        };
        if let Some(name) = extra {
            let segment_span = name.span;
            path.segments.push(PathSegment { name, generics: Vec::new(), span: segment_span });
            path.span = path.span.merge(segment_span);
        }
        let fields = self.parse_field_inits();
        let span = start.merge(self.last_text_span());
        Expr { kind: ExprKind::StructLit { path, fields }, span }
    }

    /// Whether the `(` about to be read opens a *named* argument list, which
    /// §4.4 makes the one and only sign of a struct construction.
    fn at_named_args(&self) -> bool {
        self.at(&TokenKind::LParen)
            && matches!(self.peek_ahead(1), TokenKind::Ident(_))
            && matches!(self.peek_ahead(2), TokenKind::Colon)
    }

    fn parse_call_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return args;
        }
        while !self.at(&TokenKind::RParen) {
            args.push(self.parse_expr());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`");
        args
    }

    fn parse_field_inits(&mut self) -> Vec<FieldInit> {
        let mut fields = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return fields;
        }
        while !self.at(&TokenKind::RParen) {
            let start = self.span();
            let parsed = self
                .expect_ident()
                .filter(|_| self.expect(&TokenKind::Colon, "`:`").is_some())
                .map(|name| {
                    let value = self.parse_expr();
                    FieldInit { name, value, span: start.merge(self.last_text_span()) }
                });
            match parsed {
                Some(field) => fields.push(field),
                None => self.recover_in_brackets(),
            }
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`");
        fields
    }

    /// A literal, a name, `self`, a parenthesised expression, or one of the
    /// control-flow forms that §4.4 makes expressions.
    fn parse_primary(&mut self) -> Expr {
        let start = self.span();

        if let Some(literal) = literal_of(self.peek()) {
            self.advance();
            return Expr { kind: ExprKind::Literal(literal), span: start };
        }

        match self.peek() {
            TokenKind::SelfValue => {
                self.advance();
                Expr { kind: ExprKind::SelfValue, span: start }
            }
            // One segment only: every `.` after it is field access or a method
            // call, and every `[` after it is an index.
            TokenKind::Ident(_) => match self.expect_ident() {
                Some(name) => {
                    let span = name.span;
                    let segment = PathSegment { name, generics: Vec::new(), span };
                    Expr { kind: ExprKind::Path(Path { segments: vec![segment], span }), span }
                }
                None => error_expr(start),
            },
            TokenKind::LParen => self.parse_paren_expr(start),
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::While => {
                self.advance();
                let cond = self.parse_expr();
                let body = self.parse_block();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::While { cond: Box::new(cond), body }, span }
            }
            TokenKind::Loop => {
                self.advance();
                let body = self.parse_block();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Loop { body }, span }
            }
            TokenKind::For => {
                self.advance();
                let pattern = self.parse_pattern();
                self.expect(&TokenKind::In, "`in`");
                let iter = self.parse_expr();
                let body = self.parse_block();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::For { pattern, iter: Box::new(iter), body }, span }
            }
            _ => {
                let found = describe(self.peek());
                self.error(
                    codes::EXPECTED_EXPR,
                    format!("expected an expression, found {found}"),
                    start,
                );
                // Drop the offending token so the enclosing list or block can
                // carry on, unless it is the delimiter that list is waiting
                // for.
                if !self.at_list_boundary() {
                    self.advance();
                }
                error_expr(start)
            }
        }
    }

    /// `()`, `(e)` and `(a, b)` all start the same way.
    fn parse_paren_expr(&mut self, start: Span) -> Expr {
        self.advance(); // `(`
        if self.eat(&TokenKind::RParen).is_some() {
            return Expr { kind: ExprKind::Unit, span: start.merge(self.last_text_span()) };
        }

        let first = self.parse_expr();
        if !self.at(&TokenKind::Comma) {
            // `(e)` is just `e`: parentheses group and nothing more, so they
            // leave no node and `e` keeps the span a diagnostic should point at.
            self.expect(&TokenKind::RParen, "`)`");
            return first;
        }

        let mut elems = vec![first];
        while self.eat(&TokenKind::Comma).is_some() {
            if self.at(&TokenKind::RParen) {
                break;
            }
            elems.push(self.parse_expr());
        }
        self.expect(&TokenKind::RParen, "`)`");
        Expr { kind: ExprKind::Tuple(elems), span: start.merge(self.last_text_span()) }
    }

    /// `if cond: ..` with an optional `else`.
    ///
    /// The dangling `else` binds to the innermost `if` (§4.2) because this
    /// function takes the `else` it finds before returning to its caller.
    fn parse_if_expr(&mut self) -> Expr {
        let start = self.span();
        self.advance(); // `if`
        let cond = self.parse_expr();
        let then_branch = self.parse_block();

        let else_branch = if self.eat(&TokenKind::Else).is_some() {
            // `else if` with no `:` of its own chains into a nested `if`;
            // `else:` gives a block, which is what the corpus writes.
            let expr = if self.at(&TokenKind::If) {
                self.parse_if_expr()
            } else {
                let block = self.parse_block();
                let span = block.span;
                Expr { kind: ExprKind::Block(block), span }
            };
            Some(Box::new(expr))
        } else {
            None
        };

        let span = start.merge(self.last_text_span());
        Expr {
            kind: ExprKind::If(IfExpr { cond: Box::new(cond), then_branch, else_branch, span }),
            span,
        }
    }

    fn parse_match_expr(&mut self) -> Expr {
        let start = self.span();
        self.advance(); // `match`
        let scrutinee = self.parse_expr();
        let arms = self.parse_indented_body(Self::parse_match_arm);
        let span = start.merge(self.last_text_span());
        Expr {
            kind: ExprKind::Match(MatchExpr { scrutinee: Box::new(scrutinee), arms, span }),
            span,
        }
    }

    /// One arm. §4.4: the pattern is parsed by the pattern grammar, and
    /// whatever follows it is the separator — there is no way to find the `:`
    /// by scanning, because `:` also appears inside a struct pattern.
    fn parse_match_arm(&mut self) -> Option<MatchArm> {
        let start = self.span();
        let pattern = self.parse_pattern();
        let body = self.parse_arm_body();
        Some(MatchArm { pattern, body, span: start.merge(self.last_text_span()) })
    }

    // --- patterns --------------------------------------------------------

    /// A pattern, alternatives included.
    fn parse_pattern(&mut self) -> Pattern {
        let start = self.span();
        let first = self.parse_pattern_primary();
        if !self.at(&TokenKind::Pipe) {
            return first;
        }
        let mut alts = vec![first];
        while self.eat(&TokenKind::Pipe).is_some() {
            alts.push(self.parse_pattern_primary());
        }
        Pattern { kind: PatternKind::Or(alts), span: start.merge(self.last_text_span()) }
    }

    /// One alternative: a literal, `_`, a binding, a variant, a struct or a
    /// tuple.
    ///
    /// Unlike an expression, a pattern's path may span several segments: there
    /// is no field access in a pattern, so a `.` can only be §4.5's qualified
    /// variant, and a `[` can only be generic arguments.
    fn parse_pattern_primary(&mut self) -> Pattern {
        let start = self.span();

        if let Some(literal) = literal_of(self.peek()) {
            self.advance();
            return Pattern { kind: PatternKind::Literal(literal), span: start };
        }

        match self.peek() {
            TokenKind::Underscore => {
                self.advance();
                Pattern { kind: PatternKind::Wildcard, span: start }
            }
            TokenKind::Mut => {
                self.advance();
                match self.expect_ident() {
                    Some(name) => Pattern {
                        kind: PatternKind::Binding { mutable: true, name },
                        span: start.merge(self.last_text_span()),
                    },
                    None => Pattern { kind: PatternKind::Error, span: start },
                }
            }
            TokenKind::LParen => self.parse_paren_pattern(start),
            TokenKind::Ident(_) => {
                let Some(path) = self.parse_path() else {
                    return Pattern { kind: PatternKind::Error, span: start };
                };
                if self.at(&TokenKind::LParen) {
                    // Named fields mean a struct, positional ones a variant —
                    // the same rule that decides the expression form (§4.4).
                    // An empty list takes the variant form, as §4.4 requires.
                    let kind = if self.at_named_args() {
                        PatternKind::Struct { path, fields: self.parse_field_patterns() }
                    } else {
                        PatternKind::Variant { path, elems: self.parse_pattern_list() }
                    };
                    return Pattern { kind, span: start.merge(self.last_text_span()) };
                }
                // A bare name is a binding until resolution says otherwise
                // (§4.4). A qualified or generic one cannot be a binding, so
                // it can only be a unit variant.
                let bare = path.segments.len() == 1 && path.segments[0].generics.is_empty();
                let kind = if bare {
                    PatternKind::Binding { mutable: false, name: path.segments[0].name.clone() }
                } else {
                    PatternKind::Variant { path, elems: Vec::new() }
                };
                Pattern { kind, span: start.merge(self.last_text_span()) }
            }
            _ => {
                let found = describe(self.peek());
                self.error(
                    codes::EXPECTED_PATTERN,
                    format!("expected a pattern, found {found}"),
                    start,
                );
                if !self.at_list_boundary() {
                    self.advance();
                }
                Pattern { kind: PatternKind::Error, span: start }
            }
        }
    }

    /// `()`, `(p)` and `(a, b)`.
    fn parse_paren_pattern(&mut self, start: Span) -> Pattern {
        self.advance(); // `(`
        if self.eat(&TokenKind::RParen).is_some() {
            return Pattern { kind: PatternKind::Unit, span: start.merge(self.last_text_span()) };
        }

        let first = self.parse_pattern();
        if !self.at(&TokenKind::Comma) {
            self.expect(&TokenKind::RParen, "`)`");
            return first;
        }

        let mut elems = vec![first];
        while self.eat(&TokenKind::Comma).is_some() {
            if self.at(&TokenKind::RParen) {
                break;
            }
            elems.push(self.parse_pattern());
        }
        self.expect(&TokenKind::RParen, "`)`");
        Pattern { kind: PatternKind::Tuple(elems), span: start.merge(self.last_text_span()) }
    }

    /// The positional payload of a variant pattern.
    fn parse_pattern_list(&mut self) -> Vec<Pattern> {
        let mut elems = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return elems;
        }
        while !self.at(&TokenKind::RParen) {
            elems.push(self.parse_pattern());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`");
        elems
    }

    /// The named fields of a struct pattern: `Doc(title: t, body: _)`.
    fn parse_field_patterns(&mut self) -> Vec<FieldPattern> {
        let mut fields = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return fields;
        }
        while !self.at(&TokenKind::RParen) {
            let start = self.span();
            let parsed = self
                .expect_ident()
                .filter(|_| self.expect(&TokenKind::Colon, "`:`").is_some())
                .map(|name| {
                    let pattern = self.parse_pattern();
                    FieldPattern { name, pattern, span: start.merge(self.last_text_span()) }
                });
            match parsed {
                Some(field) => fields.push(field),
                None => self.recover_in_brackets(),
            }
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`");
        fields
    }

    // --- traits and impls ------------------------------------------------

    /// `trait Name[T]: Super + Other where ..:` followed by an indented list
    /// of methods, with or without bodies.
    fn parse_trait(&mut self, is_pub: bool, start: Span) -> Option<TraitDecl> {
        self.advance(); // `trait`
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();

        // Two colons can follow, and only one of them is ever present at a
        // time in the corpus: the one that introduces supertraits is followed
        // by a name, the one that opens the body by the end of the line.
        let supertraits = if self.at(&TokenKind::Colon)
            && !matches!(self.peek_ahead(1), TokenKind::Newline)
        {
            self.advance();
            self.parse_bounds()
        } else {
            Vec::new()
        };

        let where_clause = self.parse_where_clause();
        let methods = self.parse_indented_body(Self::parse_method);

        Some(TraitDecl {
            is_pub,
            name,
            generics,
            supertraits,
            where_clause,
            methods,
            span: start.merge(self.last_text_span()),
        })
    }

    /// `impl Trait for Type:`, `impl Type:` and the block-less marker form.
    ///
    /// §4.3 gives all three: an inherent impl has no trait and its bodiless
    /// functions are associated functions, and `impl Copy for Point` on one
    /// line with no `:` is how a trait with no methods is implemented, "since
    /// there is nothing to indent".
    fn parse_impl(&mut self, start: Span) -> Option<ImplBlock> {
        self.advance(); // `impl`
        let generics = self.parse_generic_params();

        // The first type is the trait when a `for` follows and the self type
        // otherwise, and nothing before the `for` says which it will be.
        let first = self.parse_type();
        let (trait_, self_ty) = if self.eat(&TokenKind::For).is_some() {
            (Some(self.type_as_bound(first)?), self.parse_type())
        } else {
            (None, first)
        };

        let where_clause = self.parse_where_clause();

        let methods = if self.at(&TokenKind::Colon) {
            self.parse_indented_body(Self::parse_method)
        } else {
            if self.at(&TokenKind::Newline) && matches!(self.peek_ahead(1), TokenKind::Indent) {
                // An indented body with nothing introducing it is a missing
                // `:`, and saying so here points at the header rather than at
                // the first method.
                let span = self.span();
                self.error(codes::EXPECTED_BLOCK, "expected `:` before the `impl` body", span);
                self.advance();
                self.skip_indented_region();
            } else {
                // The marker form: one line, no block, no methods.
                self.expect_line_end();
            }
            Vec::new()
        };

        Some(ImplBlock {
            generics,
            trait_,
            self_ty,
            where_clause,
            methods,
            span: start.merge(self.last_text_span()),
        })
    }

    /// A trait is named by a path, so anything else in front of `for` is an
    /// error rather than something a later phase could make sense of.
    fn type_as_bound(&mut self, ty: Type) -> Option<TypeBound> {
        match ty.kind {
            TypeKind::Path(path) => Some(TypeBound { path, span: ty.span }),
            TypeKind::Error => None,
            _ => {
                self.error(
                    codes::EXPECTED_TRAIT,
                    "expected a trait name before `for`",
                    ty.span,
                );
                None
            }
        }
    }

    /// One method of a `trait` or `impl` body.
    fn parse_method(&mut self) -> Option<FnDecl> {
        let start = self.span();
        let is_pub = self.eat(&TokenKind::Pub).is_some();
        if !self.at(&TokenKind::Fn) {
            let found = describe(self.peek());
            self.error(
                codes::EXPECTED_METHOD,
                format!("expected a method declaration beginning with `fn`, found {found}"),
                start,
            );
            return None;
        }
        self.parse_fn(is_pub, start)
    }
}

fn empty_block(span: Span) -> Block {
    Block { stmts: Vec::new(), tail: None, span: Span::at(span.file, span.start) }
}

fn error_expr(span: Span) -> Expr {
    Expr { kind: ExprKind::Error, span }
}

/// Whether `expr` is a name that could still take generic arguments.
///
/// `Array` can; `Array[Int]` already has them, and a call, a field or a
/// literal never could.
fn instantiable_path(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Path(path) => {
            path.segments.last().is_some_and(|segment| segment.generics.is_empty())
        }
        _ => false,
    }
}

/// Turns a finished list of statements into a block, splitting off the tail.
///
/// §4.3: "a function's value is its last expression". Splitting that
/// expression out here means no later phase has to re-derive which statement
/// the block evaluates to, and a statement that is *not* an expression simply
/// leaves the block with no tail.
///
/// `start` is where the block's contents begin; it is only used when there are
/// no statements at all, since a span has to point somewhere.
fn finish_block(mut stmts: Vec<Stmt>, start: Span) -> Block {
    let tail = match stmts.last() {
        Some(Stmt { kind: StmtKind::Expr(_), .. }) => match stmts.pop() {
            Some(Stmt { kind: StmtKind::Expr(expr), .. }) => Some(Box::new(expr)),
            _ => unreachable!("just matched an expression statement"),
        },
        _ => None,
    };

    let mut span = None;
    for stmt in &stmts {
        span = Some(span.map_or(stmt.span, |s: Span| s.merge(stmt.span)));
    }
    if let Some(tail) = &tail {
        span = Some(span.map_or(tail.span, |s: Span| s.merge(tail.span)));
    }

    Block {
        stmts,
        tail,
        span: span.unwrap_or_else(|| Span::at(start.file, start.start)),
    }
}

/// The literal a token stands for, or `None` when it is not one.
///
/// Shared by expressions and patterns, which take literals from exactly the
/// same set: the two would otherwise drift apart silently.
fn literal_of(kind: &TokenKind) -> Option<Literal> {
    Some(match kind {
        TokenKind::Int { value, base, suffix } => {
            Literal::Int { value: *value, base: *base, suffix: *suffix }
        }
        TokenKind::Float { value, suffix } => Literal::Float { value: *value, suffix: *suffix },
        TokenKind::Str(value) => Literal::Str(value.clone()),
        TokenKind::Char(value) => Literal::Char(*value),
        TokenKind::True => Literal::Bool(true),
        TokenKind::False => Literal::Bool(false),
        _ => return None,
    })
}

/// §4.4's precedence table, as an operator and the rung it sits on.
///
/// Higher binds tighter. The rows above `*` — unary, `as`, and the postfix
/// chain — are not here: they are not infix, so they belong to the descent
/// rather than to the climb. `=` is not here either, and §4.4 says why: it is
/// a statement, not an operator.
fn binary_op(kind: &TokenKind) -> Option<(BinaryOp, u8)> {
    use TokenKind as T;
    Some(match kind {
        T::Or => (BinaryOp::Or, 1),
        T::And => (BinaryOp::And, 2),

        T::EqEq => (BinaryOp::Eq, 3),
        T::NotEq => (BinaryOp::Ne, 3),
        T::Lt => (BinaryOp::Lt, 3),
        T::Gt => (BinaryOp::Gt, 3),
        T::LtEq => (BinaryOp::Le, 3),
        T::GtEq => (BinaryOp::Ge, 3),

        T::Pipe => (BinaryOp::BitOr, 4),
        T::Caret => (BinaryOp::BitXor, 5),
        T::Amp => (BinaryOp::BitAnd, 6),

        T::Shl => (BinaryOp::Shl, 7),
        T::Shr => (BinaryOp::Shr, 7),

        T::Plus => (BinaryOp::Add, 8),
        T::Minus => (BinaryOp::Sub, 8),

        T::Star => (BinaryOp::Mul, 9),
        T::Slash => (BinaryOp::Div, 9),
        T::Percent => (BinaryOp::Rem, 9),

        _ => return None,
    })
}

// --- naming tokens in messages -------------------------------------------

/// How a token is named in a diagnostic.
///
/// Concrete tokens are quoted as they are written; token classes get a phrase,
/// because ``expected `:`, found `Newline` `` reads like an implementation
/// detail leaking out.
fn describe(kind: &TokenKind) -> String {
    use TokenKind::*;
    match kind {
        Newline => "end of line".to_string(),
        Indent => "an indented block".to_string(),
        Dedent => "the end of a block".to_string(),
        Eof => "end of file".to_string(),
        Int { .. } | Float { .. } => "a number literal".to_string(),
        Str(_) => "a string literal".to_string(),
        Char(_) => "a character literal".to_string(),
        Ident(name) => format!("`{name}`"),
        Unknown(c) => format!("`{c}`"),
        Reserved(word) => {
            format!("`{}`, which is reserved for a later phase", reserved_text(*word))
        }
        other => format!("`{}`", fixed_text(other)),
    }
}

/// The source text of every token whose spelling is fixed.
fn fixed_text(kind: &TokenKind) -> &'static str {
    use TokenKind::*;
    match kind {
        Fn => "fn",
        Let => "let",
        Mut => "mut",
        If => "if",
        Else => "else",
        Match => "match",
        For => "for",
        In => "in",
        While => "while",
        Loop => "loop",
        Return => "return",
        Break => "break",
        Continue => "continue",
        Struct => "struct",
        Enum => "enum",
        Trait => "trait",
        Impl => "impl",
        Use => "use",
        Mod => "mod",
        Pub => "pub",
        True => "true",
        False => "false",
        SelfValue => "self",
        SelfType => "Self",
        As => "as",
        Dyn => "dyn",
        Where => "where",
        And => "and",
        Or => "or",
        Not => "not",

        LParen => "(",
        RParen => ")",
        LBracket => "[",
        RBracket => "]",
        LBrace => "{",
        RBrace => "}",
        Comma => ",",
        Colon => ":",
        Semi => ";",
        Dot => ".",
        Arrow => "->",
        FatArrow => "=>",
        Question => "?",
        Underscore => "_",
        At => "@",
        Hash => "#",

        Plus => "+",
        Minus => "-",
        Star => "*",
        Slash => "/",
        Percent => "%",
        Amp => "&",
        Pipe => "|",
        Caret => "^",
        Shl => "<<",
        Shr => ">>",
        Eq => "=",
        EqEq => "==",
        NotEq => "!=",
        Lt => "<",
        Gt => ">",
        LtEq => "<=",
        GtEq => ">=",

        // The variants above cover every fixed spelling; the rest are handled
        // by `describe` before it gets here.
        _ => "?",
    }
}

fn reserved_text(word: ReservedWord) -> &'static str {
    use ReservedWord::*;
    match word {
        Agent => "agent",
        Tool => "tool",
        Prompt => "prompt",
        Spawn => "spawn",
        Send => "send",
        Receive => "receive",
        Durable => "durable",
        Checkpoint => "checkpoint",
        Resume => "resume",
        Supervise => "supervise",
        Async => "async",
        Await => "await",
        Tensor => "tensor",
        Shape => "shape",
        Model => "model",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    fn stream(kinds: Vec<TokenKind>) -> Vec<Token> {
        kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Token::new(kind, span(i as u32, i as u32 + 1)))
            .collect()
    }

    /// §9 hands syntax the range SC0100-SC0199, and nothing may drift out of it.
    #[test]
    fn every_code_is_a_syntax_code() {
        let all = [
            codes::UNEXPECTED_TOKEN,
            codes::EXPECTED_ITEM,
            codes::EXPECTED_IDENT,
            codes::EXPECTED_BLOCK,
            codes::EXPECTED_TYPE,
            codes::EXPECTED_EXPR,
            codes::EXPECTED_PATTERN,
            codes::MISPLACED_PUB,
            codes::MISPLACED_RECEIVER,
            codes::STATEMENT_IN_INLINE_BLOCK,
            codes::MISPLACED_NAMED_ARGUMENT,
            codes::EXPECTED_TRAIT,
            codes::EXPECTED_METHOD,
        ];
        for code in all {
            assert!(
                (100..=199).contains(&code.0),
                "{code} is outside the syntax range SC0100-SC0199"
            );
        }
        // Codes have to stay distinct, or a UI test cannot tell two errors
        // apart.
        let mut seen: Vec<u16> = all.iter().map(|c| c.0).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "two diagnostics share a code");
    }

    /// Recovery has to terminate on anything, including streams no lexer would
    /// ever produce. A parser that loops here hangs the whole compiler.
    #[test]
    fn parsing_terminates_on_garbage() {
        let garbage = vec![
            vec![TokenKind::Indent, TokenKind::Dedent, TokenKind::Dedent],
            vec![TokenKind::Colon, TokenKind::Colon, TokenKind::Colon],
            vec![TokenKind::Fn, TokenKind::LParen, TokenKind::LParen],
            vec![TokenKind::Struct, TokenKind::Indent, TokenKind::Indent],
            vec![TokenKind::Enum, TokenKind::Ident("E".into()), TokenKind::Colon],
            vec![TokenKind::Use, TokenKind::Comma, TokenKind::RParen],
            vec![TokenKind::Unknown('§'), TokenKind::Unknown('¿')],
            Vec::new(),
            // Statements, expressions and patterns, each cut off mid-grammar.
            vec![TokenKind::Fn, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Let],
            vec![TokenKind::Fn, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("a".into()),
                 TokenKind::Plus],
            vec![TokenKind::Fn, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Match],
            vec![TokenKind::Fn, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Match,
                 TokenKind::Ident("x".into()), TokenKind::Colon, TokenKind::Newline,
                 TokenKind::Indent, TokenKind::Pipe, TokenKind::Pipe],
            vec![TokenKind::Fn, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("f".into()),
                 TokenKind::LParen, TokenKind::Comma, TokenKind::Comma],
            vec![TokenKind::Fn, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Newline,
                 TokenKind::Indent, TokenKind::Colon, TokenKind::Colon],
            vec![TokenKind::Trait, TokenKind::Ident("T".into()), TokenKind::Colon],
            vec![TokenKind::Impl, TokenKind::For],
            vec![TokenKind::Impl, TokenKind::Ident("T".into()), TokenKind::For],
        ];
        for kinds in garbage {
            let tokens = stream(kinds);
            let (_module, _diagnostics) = parse_module(&tokens, FileId(0));
        }
    }

    /// §4.4's table, read back out of `binary_op`. A row that loses an operator
    /// to a typo would otherwise show up only as a wrong parse somewhere far
    /// away.
    #[test]
    fn the_precedence_table_matches_the_spec() {
        let rows: [&[TokenKind]; 9] = [
            &[TokenKind::Or],
            &[TokenKind::And],
            &[TokenKind::EqEq, TokenKind::NotEq, TokenKind::Lt, TokenKind::Gt,
              TokenKind::LtEq, TokenKind::GtEq],
            &[TokenKind::Pipe],
            &[TokenKind::Caret],
            &[TokenKind::Amp],
            &[TokenKind::Shl, TokenKind::Shr],
            &[TokenKind::Plus, TokenKind::Minus],
            &[TokenKind::Star, TokenKind::Slash, TokenKind::Percent],
        ];
        for (index, row) in rows.iter().enumerate() {
            let expected = index as u8 + 1;
            for kind in row.iter() {
                let (_, prec) = binary_op(kind).expect("every operator in the table has a rung");
                assert_eq!(prec, expected, "{} sits on the wrong rung", fixed_text(kind));
            }
        }
        // `=` is a statement, not an operator, and must never gain a rung.
        assert!(binary_op(&TokenKind::Eq).is_none(), "`=` is not in the precedence table");
    }

    /// A block's value is its last expression; a block ending in anything else
    /// has none.
    #[test]
    fn a_block_takes_its_tail_from_the_last_expression() {
        let file = FileId(0);
        let expr = Stmt { kind: StmtKind::Expr(error_expr(span(0, 1))), span: span(0, 1) };
        let jump = Stmt { kind: StmtKind::Continue, span: span(2, 3) };

        let block = finish_block(vec![jump.clone(), expr.clone()], span(0, 0));
        assert!(block.tail.is_some());
        assert_eq!(block.stmts.len(), 1);
        assert_eq!(block.span, Span::new(file, 0, 3));

        let block = finish_block(vec![expr, jump], span(0, 0));
        assert!(block.tail.is_none());
        assert_eq!(block.stmts.len(), 2);

        let empty = finish_block(Vec::new(), span(7, 9));
        assert!(empty.tail.is_none());
        assert_eq!(empty.span, Span::at(file, 7));
    }

    /// `synchronize` must always consume something when it is not already at a
    /// boundary, otherwise the caller's loop cannot make progress.
    #[test]
    fn synchronize_makes_progress() {
        let tokens = stream(vec![
            TokenKind::Ident("junk".into()),
            TokenKind::Ident("more".into()),
            TokenKind::Newline,
            TokenKind::Ident("next".into()),
        ]);
        let mut parser = Parser::new(&tokens, FileId(0));
        parser.synchronize();
        assert!(matches!(parser.peek(), TokenKind::Ident(name) if name == "next"));
    }

    /// A `Dedent` belongs to whoever opened the block, so recovery stops in
    /// front of it rather than swallowing it.
    #[test]
    fn synchronize_leaves_the_dedent_for_the_caller() {
        let tokens = stream(vec![TokenKind::Ident("junk".into()), TokenKind::Dedent]);
        let mut parser = Parser::new(&tokens, FileId(0));
        parser.synchronize();
        assert!(matches!(parser.peek(), TokenKind::Dedent));
    }
}
