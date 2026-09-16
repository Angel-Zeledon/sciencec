//! A recursive-descent parser over the lexer's token stream.
//!
//! # What is here, and what is not
//!
//! This is the first batch. It covers the parser's skeleton, top-level `fn`,
//! `struct`, `enum` and `use` declarations, type expressions, and block
//! structure. Statements, expressions, patterns, `trait` and `impl` are marked
//! `todo!()` and land in the next batch, together with the precedence-climbing
//! loop for §4.4's operator table.
//!
//! Because statements are not parsed yet, a block's contents are *skipped*:
//! the block node comes out with an accurate span and no statements. That is
//! deliberate, so declarations can be tested now.
//!
//! # Error recovery
//!
//! The parser never stops at the first error. On a failure it reports a
//! diagnostic and synchronises: it drops tokens up to the end of the logical
//! line or the end of the enclosing indented block, and carries on. One pass
//! therefore reports several problems, which is the whole point of the
//! `Diagnostics` accumulator.
//!
//! # Where the spec and the grammar pull against each other
//!
//! §4.4 lists `=` in the precedence table, alongside the operators. It is
//! non-associative, and Link has no place where the value of an assignment is
//! useful, so assignment is parsed as a *statement*: parse an expression, and
//! if a `=` follows, it was an assignment target. That accepts the same
//! programs and keeps `let x = if c: a = b else: c` from being grammatical.

use link_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span};
use link_lexer::{ReservedWord, Token, TokenKind};

use crate::ast::*;

/// Syntax diagnostics, which §9 of the spec assigns the range LK0100-LK0199.
pub mod codes {
    use link_diagnostics::Code;

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

    /// The span of the token just consumed, which is what closes a node.
    fn prev_span(&self) -> Span {
        if self.pos == 0 {
            self.span()
        } else {
            self.tokens[self.pos - 1].span
        }
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

    /// A logical line has to end where the parser thinks it does.
    fn expect_line_end(&mut self) {
        match self.peek() {
            TokenKind::Newline => {
                self.advance();
            }
            TokenKind::Dedent | TokenKind::Eof => {}
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

        Some(Item { kind, span: start.merge(self.prev_span()) })
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
            span: start.merge(self.prev_span()),
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
                Some(SelfParam { kind: SelfKind::Ref, span: start.merge(self.prev_span()) })
            }
            (TokenKind::Mut, TokenKind::SelfValue) => {
                self.advance();
                self.advance();
                self.advance();
                Some(SelfParam { kind: SelfKind::RefMut, span: start.merge(self.prev_span()) })
            }
            _ => None,
        }
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.span();
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type();
        Some(Param { name, ty, span: start.merge(self.prev_span()) })
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
            span: start.merge(self.prev_span()),
        })
    }

    fn parse_field(&mut self) -> Option<FieldDef> {
        let start = self.span();
        let is_pub = self.eat(&TokenKind::Pub).is_some();
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type();
        Some(FieldDef { is_pub, name, ty, span: start.merge(self.prev_span()) })
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
            span: start.merge(self.prev_span()),
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
        Some(VariantDef { name, payload, span: start.merge(self.prev_span()) })
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
        Some(UseDecl { path, imports, span: start.merge(self.prev_span()) })
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
            params.push(GenericParam { name, bounds, span: start.merge(self.prev_span()) });
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
        loop {
            match self.parse_type_bound() {
                Some(bound) => bounds.push(bound),
                None => break,
            }
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
            predicates.push(WherePredicate { ty, bounds, span: start.merge(self.prev_span()) });
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
            span = span.merge(self.prev_span());
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
                    span: start.merge(self.prev_span()),
                }
            }
            TokenKind::Dyn => {
                self.advance();
                match self.parse_type_bound() {
                    Some(bound) => Type {
                        kind: TypeKind::Dyn(bound),
                        span: start.merge(self.prev_span()),
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
            return Type { kind: TypeKind::Unit, span: start.merge(self.prev_span()) };
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
        Type { kind: TypeKind::Tuple(elems), span: start.merge(self.prev_span()) }
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
    /// - inline: `:` followed by a single expression, on the same line
    ///
    /// The returned span covers the block's contents, not the `:`.
    ///
    /// TODO(batch 2): the contents are skipped rather than parsed. Replace
    /// `skip_indented_block` and `skip_inline_block` with the statement parser.
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
            let span = self.skip_indented_block();
            self.eat(&TokenKind::Dedent);
            return Block { stmts: Vec::new(), tail: None, span };
        }

        if matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
            let span = self.span();
            self.error(
                codes::EXPECTED_BLOCK,
                "expected an indented block or a single expression after `:`",
                span,
            );
            return empty_block(span);
        }

        let span = self.skip_inline_block();
        Block { stmts: Vec::new(), tail: None, span }
    }

    /// Consumes an indented block's contents and returns the span they cover.
    /// Nested blocks are consumed with it, so a nested `Dedent` never closes
    /// the outer block early.
    fn skip_indented_block(&mut self) -> Span {
        let start = self.span();
        let mut last = start;
        let mut depth = 0usize;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Dedent if depth == 0 => break,
                TokenKind::Dedent => {
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Indent => {
                    depth += 1;
                    self.advance();
                }
                _ => {
                    let token = self.advance();
                    // Newlines carry no text, so they must not stretch the
                    // span past the last thing actually written.
                    if !token.span.is_empty() {
                        last = token.span;
                    }
                }
            }
        }
        start.merge(last)
    }

    /// Consumes an inline block's single expression, up to the end of the line.
    ///
    /// TODO(batch 2): once expressions parse, the end of an inline block is
    /// wherever the expression ends, not the end of the line. That matters for
    /// `if c: a else: b`, where the `then` block stops at `else`.
    fn skip_inline_block(&mut self) -> Span {
        let start = self.span();
        let mut last = start;
        loop {
            match self.peek() {
                TokenKind::Eof | TokenKind::Dedent => break,
                TokenKind::Newline => {
                    self.advance();
                    break;
                }
                _ => {
                    let token = self.advance();
                    if !token.span.is_empty() {
                        last = token.span;
                    }
                }
            }
        }
        start.merge(last)
    }

    // --- next batch ------------------------------------------------------

    /// TODO(batch 2): `trait` declarations. The body is an indented list of
    /// `fn` declarations, with and without bodies, which `parse_fn` and
    /// `parse_indented_body` already handle between them.
    fn parse_trait(&mut self, _is_pub: bool, _start: Span) -> Option<TraitDecl> {
        todo!("trait declarations arrive in the next batch")
    }

    /// TODO(batch 2): `impl Trait for Type` and `impl Type`.
    fn parse_impl(&mut self, _start: Span) -> Option<ImplBlock> {
        todo!("impl blocks arrive in the next batch")
    }

    /// TODO(batch 2): statements. `let`, assignment, `return`, `break`,
    /// `continue`, and expression statements.
    #[allow(dead_code)]
    fn parse_stmt(&mut self) -> Option<Stmt> {
        todo!("statements arrive in the next batch")
    }

    /// TODO(batch 2): expressions, by precedence climbing over §4.4's table.
    #[allow(dead_code)]
    fn parse_expr(&mut self) -> Expr {
        todo!("expressions arrive in the next batch")
    }

    /// TODO(batch 2): patterns, including alternatives with `|`.
    #[allow(dead_code)]
    fn parse_pattern(&mut self) -> Pattern {
        todo!("patterns arrive in the next batch")
    }
}

fn empty_block(span: Span) -> Block {
    Block { stmts: Vec::new(), tail: None, span: Span::at(span.file, span.start) }
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

    /// §9 hands syntax the range LK0100-LK0199, and nothing may drift out of it.
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
        ];
        for code in all {
            assert!(
                (100..=199).contains(&code.0),
                "{code} is outside the syntax range LK0100-LK0199"
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
        ];
        for kinds in garbage {
            let tokens = stream(kinds);
            let (_module, _diagnostics) = parse_module(&tokens, FileId(0));
        }
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
