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
//! # The places the grammar is decided rather than discovered
//!
//! **Assignment is a statement, not an operator.** §4.6's precedence table has
//! no assignment in it at all, on purpose: listing it would make
//! `let x be if c: a be b else: c` grammatical. So `parse_stmt` parses an
//! expression and, on finding a following `be`, treats the whole thing as an
//! assignment.
//!
//! **An item may begin with a type rather than a keyword.** `Doc implements
//! Summarize:` and `Doc has:` (§4.4) put the type first, so `parse_item`
//! cannot dispatch on a leading keyword alone: it parses a path and then looks
//! for `implements` or `has`. A line that is neither is
//! still reported as "not a declaration", because a statement at the top level
//! of a module has to be told so in those terms.
//!
//! **An inline body ends where the expression ends, not at the newline.**
//! §4.5 is explicit: in `if c: a else: b` the `then` body stops mid-line, at
//! `else`, because `else` cannot continue an expression. Recursive descent
//! gives that for free — the expression parser simply stops — and the same
//! mechanism gives the dangling `else` its innermost binding.
//!
//! **`is` resolves to a symbol, not to an operator of its own.** Revision 2
//! §1 leaves exactly one spelling for each comparison: `is` and `is not` for
//! identity, and the symbols for order. `peek_binary_op` maps `is` to the
//! *token* `==` and `is not` to `!=`, which are then read by the single
//! operator table every symbol goes through, so a word and a symbol can never
//! become different operators. The phrases `is at least`, `is at most`, `is
//! above` and `is below` are recognised in the same place for one purpose
//! only: to report that they were removed.
//!
//! **`[...]` introduces arguments in a type and parameters in a declaration
//! head.** `Array[Doc]` passes an argument; `def largest[T](..)` and
//! `Grid[T, const ROWS: Int] has:` declare parameters. Nothing in the token
//! stream distinguishes them, and nothing has to: the two are read by
//! different functions (`parse_generic_args` and `parse_generic_params`),
//! reached from different places in the grammar — `parse_path_segments` is
//! read instead of `parse_path` at every declaration head for exactly this
//! reason.
//!
//! **`of` survives in one place brackets could not reach.** §4.3's old
//! spelling is refused everywhere else by name, but the associated-call
//! escape valve `(Array of Doc).new()` still reads `of`, because a `[` there
//! would be indistinguishable from indexing — `parse_postfix`'s `[` already
//! means "index the thing on the left," and `Array[Doc]` and `xs[i]` are the
//! same three tokens with no later one to tell them apart. See
//! `parse_instantiation`'s doc comment for the full argument.
//!
//! **Two ambiguities are left for name resolution**, because §4.4 says they
//! cannot be settled by syntax: `Doc()` parses as a call, not as a record with
//! no fields, and a bare name in a pattern parses as a binding, not as a unit
//! variant. The same rule decides `Doc(title: "a")` against `f(1)`: named
//! arguments mean a record, positional ones mean a call or a variant. It is
//! also what decides `docs.sort(by: f)`, which is written exactly like the
//! qualified construction `text.Doc(title: "a")` and parses as one — only
//! resolution knows whether `docs` names a module or a value.
//!
//! A third ambiguity follows from `.` serving as both the path separator and
//! the field-access operator: `Option.Some(x)` (§4.7's qualified variant) is
//! written exactly like a method call, and `Doc.new("a")` (§4.4's associated
//! function) exactly like one too. Both parse as `MethodCall`; resolution
//! reclassifies them. A path in expression position is therefore one segment
//! long, and every `.` after it belongs to the postfix chain.

use science_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span, Suggestion};
use science_lexer::{DocComment, ReservedWord, Token, TokenKind};

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
    /// shaped the way §4.5 requires.
    pub const EXPECTED_BLOCK: Code = Code(103);
    /// A type expression was required here.
    pub const EXPECTED_TYPE: Code = Code(104);
    /// An expression was required here.
    pub const EXPECTED_EXPR: Code = Code(105);
    /// A pattern was required here.
    pub const EXPECTED_PATTERN: Code = Code(106);
    /// `public` in front of something that cannot be public.
    pub const MISPLACED_PUBLIC: Code = Code(107);
    /// A `self` receiver somewhere other than first in the parameter list.
    pub const MISPLACED_RECEIVER: Code = Code(108);
    /// A statement where §4.5 requires an inline block's single expression.
    pub const STATEMENT_IN_INLINE_BLOCK: Code = Code(109);
    /// Named arguments in front of something that is not a record's name.
    pub const MISPLACED_NAMED_ARGUMENT: Code = Code(110);
    /// `<something that is not a path> implements ..`.
    pub const EXPECTED_TRAIT: Code = Code(111);
    /// Something in an `interface` or implementation body that is not a
    /// member.
    pub const EXPECTED_MEMBER: Code = Code(112);
    /// A nested `each`, which §4.6 rejects rather than giving it a rule.
    pub const NESTED_EACH: Code = Code(115);
    /// `Array of Doc.new()`, whose `.new()` §4.3 refuses to attach by guessing.
    pub const AMBIGUOUS_GENERIC_CALL: Code = Code(116);
    /// A file with top-level statements that also declares `main`.
    ///
    /// `script-mode.md` §2.2 allocates it, and `docs/superpowers/design/
    /// README.md` records it as that note's one claim on the syntax range.
    /// The top-level statements *are* a `main`, so a second one is two
    /// answers to "what runs first" and the note refuses to pick between them
    /// on the reader's behalf.
    pub const SCRIPT_AND_MAIN: Code = Code(117);
    /// The word `returns` where §4.4 now writes `->`.
    pub const RETURNS_WORD: Code = Code(118);

    /// A bound that opens with `(` and then does not continue with `->`.
    ///
    /// `collections-and-chains.md` §1.2 spells a closure type `(A) -> B` and
    /// notes that the parser may need one code from the syntax block for the
    /// case where a parenthesised list is followed by the wrong thing; it
    /// recorded the need without taking a number. This takes it, and takes it
    /// from the free pool `docs/superpowers/design/README.md` lists rather
    /// than from the next number after `SC0118`.
    ///
    /// It fires in **bound** position only. In type position `(A, B)` with no
    /// arrow after it is a tuple and needs no diagnostic at all — that is the
    /// whole of §1.2's one-token test. In bound position there is no second
    /// reading: no interface name begins with `(`, so the missing arrow is a
    /// refusal rather than a fork, and saying so here is what keeps it one
    /// diagnostic instead of the three that follow from a parser left standing
    /// on a parameter list it cannot use.
    pub const EXPECTED_BOUND_ARROW: Code = Code(119);

    // The migration codes of syntax revision 2. Each names a word the
    // revision removed and says what replaced it, because a reader arriving
    // with pre-revision code gets a keyword that is now an ordinary word, and
    // the error that falls out of the grammar names the symptom instead.
    //
    // `SC0140` is skipped: `syntax-revision-2.md` §3.3 already claims it for
    // the unchecked-error analysis.

    /// `for each x in xs`, where revision 2 §2.1 writes `for x in xs`.
    pub const EACH_AFTER_FOR: Code = Code(138);
    /// The word `trait`, which revision 2 §6 renamed to `interface`.
    pub const TRAIT_WORD: Code = Code(139);
    /// `Type has methods:`, where revision 2 §5 writes `Type has:`.
    pub const METHODS_AFTER_HAS: Code = Code(141);
    /// The word `while`, which revision 2 §2.2 removed in favour of `loop`.
    pub const WHILE_WORD: Code = Code(142);
    /// `is above`, `is below`, `is at least`, `is at most` — the comparison
    /// phrases revision 2 §1 replaced with `>`, `<`, `>=` and `<=`.
    pub const COMPARISON_PHRASE: Code = Code(143);
    /// The word `println`, which revision 2 §3.5 renamed to `print`.
    pub const PRINTLN_WORD: Code = Code(144);

    /// The word `try`, which revision 2 §3 removed with the `Result` it
    /// unwrapped. `SC0145`–`SC0149` belong to `rust-interop.md`, so this
    /// takes the next block the allocation map records as free.
    pub const TRY_WORD: Code = Code(155);

    // `indexing-and-array-literals.md` §7.1's block, `SC0150`-`SC0154`, which
    // `docs/superpowers/design/README.md` records as that note's claim on the
    // syntax range. Four of the five are here. `SC0150` — an open-ended range
    // outside an index bracket — is not, because nothing in this phase can yet
    // produce an open-ended range to be outside one: see
    // `Parser::parse_range`'s note on what F0 does and does not spell.
    //
    // All four are decided on the AST, before types, which is §7.1's own
    // requirement for two of them. It matters most for `SC0153`: a
    // comprehension type-checks as nothing at all, so a phase that let it
    // through would report the absence of a chain three times and the
    // comprehension never.

    /// An empty index bracket: `a[]` (§7.1).
    pub const EMPTY_INDEX: Code = Code(151);
    /// A literal negative index: `a[-1]` (§4.5, §7.1).
    pub const NEGATIVE_INDEX: Code = Code(152);
    /// A comprehension: `[` … `for` … `]` (§5.3, §7.1).
    pub const COMPREHENSION: Code = Code(153);
    /// A range with literal bounds whose start exceeds its end: `a[5..1]`
    /// (§7.1).
    pub const REVERSED_RANGE: Code = Code(154);

    /// `a * b` where neither side is an integer literal.
    ///
    /// `const-expression-arithmetic.md` §2.1 keeps const arithmetic linear
    /// by the shape of its productions rather than by a check, so this is
    /// the one place the shape has to be defended.
    pub const CONST_FACTOR: Code = Code(157);

    /// The word `function`, which revision 3 replaced with `def`.
    ///
    /// This is the migration every model will need, because a model's priors
    /// are Python's and `def-and-lambda.md` §3.2 predicted the traffic in the
    /// opposite direction. It gets a machine-applicable fix for the same
    /// reason every other migration code does.
    ///
    /// `def-and-lambda.md` §9.3 had pre-allocated `SC0136` for exactly this
    /// contingency — *"`function` starting an item, if §3.4 is overruled and
    /// `def` is adopted"* — and this code was allocated without reading that
    /// far. The number stays here anyway, and not because the compiler is the
    /// record: **the migration codes cluster.** `SC0138`–`SC0144` are
    /// revision 2's, `SC0155` is its `try`, and this is revision 3's, beside
    /// them. `SC0136` sits in that note's design block, which is about what
    /// `def` and `lambda` should *be*, not about migrating to them.
    pub const FUNCTION_WORD: Code = Code(156);

    // The `tool` declaration's own block, `SC0190`-`SC0199`, allocated by
    // `mcp-servers.md` §16.1. Every one of them is checkable here, with no
    // types at all, which is what makes §14.2's stage 0 a stage rather than a
    // wish. `SC0199` is held unallocated against the dynamic registration form
    // of that note's §15 and must not be spent on anything else.
    //
    // Not one of them names a protocol. That is §2.7's own test of whether the
    // layering is real: *"if a future reviewer finds a diagnostic in the `SC`
    // namespace that names an MCP concept, the layering has leaked and
    // Decision 1 has stopped being true."*

    /// A `tool` with no `##` run immediately above it.
    pub const TOOL_WITHOUT_DESCRIPTION: Code = Code(190);
    /// `tool f of T(..)`. A tool is one entry with one schema (§2.4 item 2).
    pub const GENERIC_TOOL: Code = Code(191);
    /// A `tool` parameter declared `borrowed` or `mutable borrowed`.
    pub const BORROWED_TOOL_PARAMETER: Code = Code(192);
    /// A `tool` with a `self` or `mutable self` receiver.
    pub const TOOL_WITH_RECEIVER: Code = Code(193);
    /// A `##` run before a parameter of something that is not a `tool`.
    pub const PARAMETER_DOC_OUTSIDE_TOOL: Code = Code(194);
    /// A `tool` whose description opens with a blank `##` line, leaving no
    /// summary for the `title` of §5.2 to be.
    pub const TOOL_SUMMARY_BLANK: Code = Code(195);
    /// `prompt` or `agent` in declaration position — Decision 2, which spends
    /// `tool` and only `tool`.
    pub const RESERVED_DECLARATION_WORD: Code = Code(196);
    /// A `tool` anywhere but the top level of a module.
    pub const TOOL_NOT_AT_MODULE_LEVEL: Code = Code(197);
    /// A `tool` with no body, in the shape of an interface's required method.
    pub const TOOL_WITHOUT_BODY: Code = Code(198);
}

/// The `extern` block's own diagnostics.
///
/// `ffi-c-boundary.md` §8 owns `SC0410`-`SC0449` and `docs/superpowers/design/README.md`
/// has the authoritative partition, so these are the one place the parser
/// steps outside §9's syntax range. It is allowed to, and only here: an
/// `extern` block is a separate grammar with its own vocabulary, and a
/// diagnostic about the FFI type table is not a diagnostic about syntax.
///
/// Codes `SC0410`, `SC0421`, `SC0424`, `SC0426`, `SC0429`, `SC0431` and
/// `SC0433` are named by §8 of that note; the ones it names and this phase can
/// check are below under the meaning §8 gives them. `SC0434` is the code
/// `c-binding-coverage.md` §7 asks for by number. The rest are allocated here,
/// inside the range and nowhere near a neighbour's.
pub mod ffi_codes {
    use science_diagnostics::Code;

    /// A line inside an `extern` block that is none of the five item forms.
    pub const UNKNOWN_EXTERN_ITEM: Code = Code(411);
    /// An `extern` block with no `library` clause (§5.1).
    pub const MISSING_LIBRARY: Code = Code(412);
    /// An ABI string other than `"C"` (§1.1).
    pub const UNKNOWN_ABI: Code = Code(413);
    /// An `extern` block not marked `unsafe` (§1.1).
    pub const EXTERN_NOT_UNSAFE: Code = Code(414);
    /// A `union` item with no size, or no alignment
    /// (`c-binding-coverage.md` §3.4).
    pub const UNION_WITHOUT_LAYOUT: Code = Code(417);
    /// A type §1.3's closed vocabulary cannot represent at the boundary.
    pub const NOT_FFI_REPRESENTABLE: Code = Code(420);
    /// `Array[T]` in an extern signature; names `ffi.Span[T]` as the fix
    /// (§8, `SC0421`).
    pub const ARRAY_IN_SIGNATURE: Code = Code(421);
    /// `F16` or `BF16` passed by value across the boundary (§8, `SC0431`).
    pub const HALF_PRECISION_BY_VALUE: Code = Code(431);
    /// A variadic function in a hand-written `extern` block. Asked for by
    /// number in `c-binding-coverage.md` §7.
    pub const VARIADIC_FUNCTION: Code = Code(434);
}

/// Where a bound list stands, and therefore whether `(A) -> B` is one of the
/// things that may appear in it.
///
/// `collections-and-chains.md` §1.2 needs the closure bound in the two
/// positions that *constrain* a parameter, and asks for it nowhere else. The
/// other three positions *name* an interface — `implements`, `any`, and an
/// interface's own super list — and a structural type in any of them would be
/// a thing no later phase could make sense of.
///
/// Saying so in the grammar rather than checking it afterwards is what keeps
/// this addition purely additive: in an [`Interface`](Self::Interface)
/// position the parser does exactly what it did before the closure type
/// existed, down to the diagnostic and its span. `implements Ord -> Bool` is
/// still `SC0100` on the `->`, and `any (A) -> B` is still `SC0102` on the
/// `(`, because neither ever reaches the fork.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundPosition {
    /// `[T: ..]` and `where T: ..` — what a parameter must satisfy, which a
    /// closure type is one of the ways of saying.
    Constraint,
    /// `interface Foo: ..`, `implements ..`, `any ..` — positions that hold an
    /// interface's name and can hold nothing else.
    Interface,
}

/// Which of two nested constructions a `->` after a generic argument belongs
/// to.
///
/// It is the one place `collections-and-chains.md` §1.2's one-token rule had
/// to be told something, rather than simply reading the token: the arrow is
/// unambiguous, but *whose* it is depends on whether the argument list was
/// parenthesised, and that is a fact the argument itself cannot see.
///
/// **Only the `of`-style escape valve still has an `Enclosing` reading.**
/// `[...]` closes its own list the way `of (..)` always did, so
/// `parse_generic_args` passes `Argument` unconditionally; `Enclosing` exists
/// for `parse_of_style_generic_args`'s bare form, `Array of Int -> Bool`,
/// which is the one generic-argument position left where a list has no
/// delimiter of its own to close it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrowAfter {
    /// Inside `[..]` or the `of`-style escape valve's own `of (..)`: an
    /// explicit delimiter closes the list, so an arrow written inside it is
    /// the argument's own.
    Argument,
    /// After the `of`-style escape valve's bare `of T`, which has no
    /// delimiter of its own: the arrow belongs to whatever the path is part
    /// of, so `Array of Int -> Bool` takes an array and gives a `Bool`.
    Enclosing,
}

/// The parameter list a closure type's left-hand side denotes.
///
/// The mapping is total, which is the point: `parse_type` never has to know in
/// advance that it is parsing a parameter list, so there is no backtracking
/// and no second grammar for parameter lists. `()` is no parameters, a tuple
/// is its elements, and anything else is one parameter.
///
/// It is lossless in one direction only, and §1.2 states the loss: `(T)`
/// collapses to `T` with no node, so a one-parameter closure whose parameter
/// is a tuple cannot be spelled — `((A, B)) -> C` arrives here as a `Tuple`
/// and comes out as two parameters. §1.3 closes that hole by ruling that
/// pairs are records and never tuples; the same argument covers the `()` case
/// below it, where a lone unit parameter is unspellable and worth nothing.
fn closure_params(ty: Type) -> Vec<Type> {
    match ty.kind {
        TypeKind::Unit => Vec::new(),
        TypeKind::Tuple(elems) => elems,
        _ => vec![ty],
    }
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
    /// One entry per enclosing call argument, `true` once an `each` inside
    /// that argument has claimed it as a closure (§4.6). See `parse_call_args`.
    each_scopes: Vec<bool>,
    diagnostics: Diagnostics,
}

impl<'t> Parser<'t> {
    pub fn new(tokens: &'t [Token], file: FileId) -> Self {
        let end = tokens.last().map(|t| t.span.end).unwrap_or(0);
        Parser {
            tokens,
            pos: 0,
            eof: Token::new(TokenKind::Eof, Span::at(file, end)),
            each_scopes: Vec::new(),
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

    /// The `##` run carried by the token about to be consumed, text and span.
    ///
    /// Cloned rather than taken: the token stream is borrowed, and a doc run
    /// is a handful of lines per declaration, so the copy is not worth an
    /// ownership mechanism to avoid.
    ///
    /// The span comes back with it because two diagnostics — `SC0194` and
    /// `SC0195` — are about the `##` lines rather than about what they
    /// document, and the tree keeps only the text: `ast::Item` and
    /// `ast::Param` hold an `Option<String>`, because every later phase reads
    /// a doc comment as prose and none of them points at one.
    fn peek_doc(&self) -> Option<DocComment> {
        self.token_at(0).doc.clone()
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

    /// Whether the last token consumed was the `Dedent` closing an indented
    /// block.
    ///
    /// Everywhere else a line ends with a `Newline`, and that token is what
    /// stops the postfix row from running on into the next line. An indented
    /// block does not get one: `parse_indented_block` eats its `Dedent` and
    /// leaves the cursor on the first token of the following line, so the row
    /// sees `(` or `[` with nothing in between and reads a call or an index.
    ///
    /// `if c:` / `return e` / `(x, y)` parsed as a **call on the `if`** with
    /// `x` and `y` for arguments, silently. It cost nothing to write and it
    /// reached six functions in `examples/`; the type checker found it, since
    /// a call to an `if` is the first thing that fails to type.
    ///
    /// Asking about the `Dedent` rather than about which primary was parsed is
    /// what keeps this true for a form nobody has written yet: whatever ends
    /// by closing a block ends its line by doing so.
    ///
    /// A chain broken over lines is untouched. §4.6 makes `.foo()` on the next
    /// line one logical line, and the lexer implements that by emitting no
    /// layout token at all — so there is no `Dedent` before it to find.
    fn just_closed_an_indented_block(&self) -> bool {
        self.pos > 0 && matches!(self.tokens[self.pos - 1].kind, TokenKind::Dedent)
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

    /// Whether the token `offset` ahead is the identifier `word`.
    ///
    /// Every word syntax revision 2 removed is an ordinary identifier now, so
    /// recognising one to report it is a string comparison rather than a
    /// token test. Keeping that in one place is what stops the migration
    /// diagnostics from each inventing their own spelling of it.
    fn word_at(&self, offset: usize, word: &str) -> bool {
        matches!(self.peek_ahead(offset), TokenKind::Ident(name) if name == word)
    }

    /// Whether `word` stands at `offset` in *declaration* position: the word
    /// itself, and a name after it.
    ///
    /// The two words this is asked about — `function` and `trait` — are
    /// ordinary identifiers to the lexer, so they arrive at the top level
    /// looking exactly like the first token of a statement. The test lives in
    /// one place rather than three so that the migration diagnostics and
    /// [`at_top_level_statement`](Self::at_top_level_statement) cannot come to
    /// disagree about what a declaration is; if they did, a file could be read
    /// as a script by one and as a module by the other.
    fn at_word_declaration(&self, offset: usize, word: &str) -> bool {
        self.word_at(offset, word) && matches!(self.peek_ahead(offset + 1), TokenKind::Ident(_))
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
    /// Two things end one. A `Newline`, obviously — a bodiless `function` in a
    /// nested list consumes its own, and the enclosing one has nothing left to
    /// take. And a `Dedent`: a statement whose last token closed an indented
    /// block has had its newline emitted *inside* that block, before the
    /// `Dedent`, so `if c:` with an indented body is one logical line with
    /// nothing trailing it.
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

    /// Drops tokens up to and including this index bracket's own `]`.
    ///
    /// **Why not [`Self::recover_in_brackets`]**, which is the recovery every
    /// other bracketed list uses: that one stops at a `,`, because inside a
    /// list the next element follows one. An index bracket has no list in it
    /// that F0 reads. §2.5 specifies `m[i, j]` as one index position of arity
    /// two and leaves it to F1, so in F0 the comma is part of the mistake; a
    /// recovery that stopped on it would leave the `]` to be reported a second
    /// time when the line failed to end, and one mistake would print twice.
    ///
    /// **The cost** is that everything between the `,` and the `]` is dropped
    /// unread, so a second error inside a rank-2 index is not reported until
    /// the first is gone. That is the ordinary price of recovering past a
    /// construct the grammar does not have, and it buys the count.
    fn recover_to_index_close(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                TokenKind::Eof | TokenKind::Newline | TokenKind::Dedent => return,
                // Not this bracket's: it belongs to whatever encloses it.
                TokenKind::RParen | TokenKind::RBrace if depth == 0 => return,
                TokenKind::RBracket if depth == 0 => {
                    self.advance();
                    return;
                }
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
                _ => {}
            }
            self.advance();
        }
    }

    // --- module ----------------------------------------------------------

    /// A whole file: declarations and statements, in source order.
    ///
    /// **The decision.** The top level of a `.science` file is a sequence of
    /// *items and statements* (`script-mode.md` §1.1), and the statements are
    /// desugared **here**, into the body of a generated `def main() -> Error?`
    /// appended to the module's items. No new statement form, no new keyword,
    /// no new node: `parse_stmt` is the one the function bodies already use, and what comes out is a [`Module`] of [`Item`]s
    /// exactly as before.
    ///
    /// **What puts a file in script mode is that it contains a top-level
    /// statement**, and nothing else. Not a flag, not an extension, not the
    /// name of the file, not a shebang — §9.3 of that note rejects each of
    /// those on the ground that the same text would then parse two ways
    /// depending on something the file does not record. A reader decides by
    /// looking at the left margin, which is where the statement is.
    ///
    /// **The reason for desugaring rather than carrying a script through the
    /// compiler** is that the alternative is two grammars. Every phase after
    /// this one — the resolver, `science-fmt`'s tree check, `sciencec tools
    /// --json`, and every phase not written yet — walks items; a second
    /// top-level shape would have to be taught to all of them, and §9.2 of the
    /// note works through why the second shape is never smaller than the
    /// first. Here it is one function that builds one [`FnDecl`], and nothing
    /// downstream learns the word "script".
    ///
    /// **The cost, in three parts, none of which is hypothetical.**
    ///
    /// 1. **Generated nodes have no source text.** The `main`, its return
    ///    type and the `null` that ends its body are given zero-width spans —
    ///    the name at the start of the first statement, the rest at the end of
    ///    the last — so a diagnostic that reaches one points at the edge of
    ///    the script rather than at a token the author never typed.
    ///    `script-mode.md` §6.3 asks the renderer to call that point "the end
    ///    of the script"; until it does, a message about it reads as a message
    ///    about nothing.
    /// 2. **A message can name a function the author did not write.** Anything
    ///    that says "in function `main`" about a script is naming a word that
    ///    is not in the file. §13 of the note lists this as the failure mode
    ///    that survives testing, because it is confusing rather than wrong.
    /// 3. **A hand-written `main` collides with the generated one.** That
    ///    collision is `SC0117` rather than a duplicate definition; see
    ///    `report_script_and_main` below.
    pub fn parse_module(&mut self) -> Module {
        let span = match (self.tokens.first(), self.tokens.last()) {
            (Some(first), Some(last)) => first.span.merge(last.span),
            _ => self.eof.span,
        };

        let mut items = Vec::new();
        let mut script: Vec<Stmt> = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&TokenKind::Eof) {
                break;
            }
            if self.at(&TokenKind::Indent) {
                let span = self.span();
                self.error(
                    codes::UNEXPECTED_TOKEN,
                    "unexpected indentation at the top level of a module",
                    span,
                );
                self.skip_indented_region();
                continue;
            }
            if self.at(&TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if self.at_top_level_statement() {
                self.parse_top_level_statement(&mut script);
            } else {
                match self.parse_item() {
                    Some(item) => items.push(item),
                    None => self.synchronize(),
                }
            }
        }

        if !script.is_empty() {
            match explicit_main(&items) {
                Some(declared) => self.report_script_and_main(declared, &script),
                None => items.push(script_body(script, span)),
            }
        }

        Module { items, span }
    }

    /// One top-level statement, read with the same [`parse_stmt`] the body of
    /// every function is read with.
    ///
    /// The loop here is `parse_indented_block`'s, minus the block: report,
    /// recover, and guarantee forward progress, because a recovery that
    /// consumes nothing at the top level of a file is a hang.
    ///
    /// [`parse_stmt`]: Self::parse_stmt
    fn parse_top_level_statement(&mut self, script: &mut Vec<Stmt>) {
        // `public` exports a *declaration*. A statement is not a declaration
        // and has nothing to export, so the word is reported — `SC0107`, the
        // code the parser already spends on a misplaced `public`, reused per
        // §8 of the note rather than given a number of its own — and then
        // stepped over, so that the statement behind it still parses and one
        // stray word costs one diagnostic.
        if self.at(&TokenKind::Public) {
            let word = self.span();
            self.advance();
            // The deletion covers the space after the word as well. Deleting
            // `public` alone would leave the statement one column in from the
            // left margin, and in a language that delimits blocks by
            // indentation that is not cosmetic: the "fix" would trade one
            // diagnostic for `SC0004`.
            let span = Span::new(word.file, word.start, self.span().start.max(word.end));
            self.diagnostics.push(
                Diagnostic::error(
                    codes::MISPLACED_PUBLIC,
                    "`public` has no meaning on a statement",
                )
                .with_label(Label::primary(word, "a statement declares no name to export"))
                .with_suggestion(Suggestion {
                    span,
                    replacement: String::new(),
                    message: "the statement runs either way".to_string(),
                }),
            );
        }

        let before = self.pos;
        match self.parse_stmt() {
            Some(stmt) => {
                script.push(stmt);
                self.expect_line_end();
            }
            None => self.synchronize(),
        }
        if self.pos == before {
            self.advance();
        }
    }

    /// Whether the logical line the cursor is on is a statement rather than a
    /// declaration.
    ///
    /// **The decision** is `script-mode.md` §8.2's, restated: every
    /// declaration but an implementation is settled by its first token, and an
    /// implementation begins with the type being implemented — so a name at
    /// the left margin could begin either one. The rule that separates them is
    /// **exact, not a heuristic**: the line is an implementation header if and
    /// only if the word `implements` or the word `has` occurs in it at bracket
    /// depth zero, before its `:` or its end.
    ///
    /// **The reason it is exact** is that both are reserved words, so neither
    /// can occur in an expression, a path, a type or an assignment target. One
    /// linear scan of one logical line decides it: no backtracking, no
    /// speculative parse, no diagnostic held back and thrown away.
    ///
    /// **The cost** is an obligation on every future syntax change: the day
    /// either word can stand in expression position, this stops being exact
    /// and starts being a guess, silently. What keeps that honest is
    /// `implements_and_has_cannot_stand_in_expression_position`, in
    /// `crates/science-parser/tests/parse_script.rs`: it asserts the premise
    /// rather than the conclusion, so the rule here cannot decay into a
    /// heuristic without a test going red.
    fn at_top_level_statement(&self) -> bool {
        // `public` belongs to the declaration behind it, so the question is
        // asked about that declaration; the misplaced word is reported by
        // `parse_top_level_statement` if the answer comes back "statement".
        let from = usize::from(self.at(&TokenKind::Public));
        match self.peek_ahead(from) {
            // Nothing in the language declares with one of these.
            TokenKind::Let
            | TokenKind::Return
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Assert => true,
            // §8.2's case, and the only one that needs the scan.
            TokenKind::Ident(_) => {
                !self.at_word_declaration(from, "function")
                    && !self.at_word_declaration(from, "trait")
                    && !self.impl_header_at(from)
            }
            // `unsafe extern` opens a foreign block; `unsafe` alone opens a
            // block expression, which is a statement like any other.
            TokenKind::Unsafe => !self.at_ahead(from + 1, &TokenKind::Extern),
            // Every word a declaration can begin with. `Self` is here because
            // it names a type — `Self implements ..` is the one thing it can
            // begin at the top level — and `Reserved` because a word held for
            // a later revision should be reported as the declaration it was
            // meant to be, not parsed as an expression.
            TokenKind::Function
            | TokenKind::Tool
            | TokenKind::Type
            | TokenKind::Choice
            | TokenKind::Interface
            | TokenKind::Const
            | TokenKind::Use
            | TokenKind::Public
            | TokenKind::Extern
            | TokenKind::SelfType
            | TokenKind::Reserved(_) => false,
            // Anything else is a statement exactly when it could be one. A
            // token that begins neither falls through to `parse_item`, whose
            // `SC0101` names both halves of the top level.
            kind => self.starts_expr(kind),
        }
    }

    /// Whether `implements` or `has` occurs at bracket depth zero in the
    /// logical line beginning at `from`, before its `:` or its end.
    ///
    /// The depth is what keeps `Grid[T, const ROWS: Int] has:` readable:
    /// the `:` inside the parameter list is not the header's.
    fn impl_header_at(&self, from: usize) -> bool {
        let mut depth = 0usize;
        let mut offset = from;
        loop {
            match self.peek_ahead(offset) {
                TokenKind::Eof
                | TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent => return false,
                TokenKind::Implements | TokenKind::Has if depth == 0 => return true,
                TokenKind::Colon if depth == 0 => return false,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth = depth.saturating_sub(1)
                }
                _ => {}
            }
            offset += 1;
        }
    }

    /// `SC0117`: the file has top-level statements *and* declares `main`.
    ///
    /// **The decision** is §2.2's: this is an error, not a precedence rule.
    /// The three alternatives each need the reader to know a rule they will
    /// never look up — statements then `main`, `main` and the statements are
    /// module initialisation, or `main` is silently dead — and one coherent
    /// answer beats two.
    ///
    /// **No applicable fix**, deliberately. There are two reasonable repairs —
    /// move the statements into `main`, or delete `main` and let the
    /// statements be the script — and the compiler cannot choose between
    /// them, so it names both and applies neither.
    ///
    /// **The script body is then not generated at all.** Generating it anyway
    /// would declare `main` twice and buy the reader a duplicate-definition
    /// error about a function only one of whose declarations is in the file:
    /// one mistake, two diagnostics, and the second one unanswerable.
    fn report_script_and_main(&mut self, declared: Span, script: &[Stmt]) {
        let first = script[0].span;
        self.diagnostics.push(
            Diagnostic::error(
                codes::SCRIPT_AND_MAIN,
                "this file has top-level statements and also declares `main`",
            )
            .with_label(Label::primary(declared, "`main` is declared here"))
            .with_label(Label::secondary(
                first,
                "the top-level statements start here, and are already a `main`",
            ))
            .with_note(
                "a file whose top level holds statements is a script: the statements are the \
                 program, and the compiler writes the `main` that runs them",
            )
            .with_note(
                "move the statements into `main`, or delete `main` and let the statements be \
                 the script",
            ),
        );
    }

    /// One declaration.
    ///
    /// Everything but an implementation begins with a keyword. An
    /// implementation begins with the type being implemented (§4.4), so a name
    /// has to be parsed before the parser can know what it is looking at — and
    /// a top-level statement, which also begins with a name, is diagnosed from
    /// the same place.
    fn parse_item(&mut self) -> Option<Item> {
        let start = self.span();
        // Read before anything is consumed: the run rides on the item's first
        // token, and `public` is that token when it is present.
        let doc = self.peek_doc();
        let public = self.eat(&TokenKind::Public).map(|t| t.span);
        let is_pub = public.is_some();

        // Dispatching with `at` rather than a `match` on `peek` keeps the
        // borrow of the token from overlapping the parse call in each arm.
        let kind = if self.at(&TokenKind::Function) {
            ItemKind::Fn(self.parse_fn(FnForm::Def, is_pub, start)?)
        } else if self.at_function_word() {
            self.report_function_word();
            ItemKind::Fn(self.parse_fn(FnForm::Def, is_pub, start)?)
        } else if self.at(&TokenKind::Tool) {
            // Checked before the declaration is read, so that the span in
            // hand is the word `tool` rather than whatever the signature ended
            // on: `SC0190` puts its caret there, and `SC0195` — which points
            // at the `##` run instead — uses it for the label that says which
            // declaration the run belongs to.
            self.check_tool_description(doc.as_ref());
            ItemKind::Fn(self.parse_fn(FnForm::Tool, is_pub, start)?)
        } else if self.at_reserved_declaration_word() {
            self.report_reserved_declaration_word();
            return None;
        } else if self.at(&TokenKind::Type) {
            self.parse_type_item(is_pub, start)?
        } else if self.at(&TokenKind::Choice) {
            ItemKind::Choice(self.parse_choice(is_pub, start)?)
        } else if self.at(&TokenKind::Interface) {
            ItemKind::Interface(self.parse_interface(is_pub, start)?)
        } else if self.at_trait_word() {
            self.report_trait_word();
            ItemKind::Interface(self.parse_interface_body(is_pub, start)?)
        } else if self.at(&TokenKind::Const) {
            ItemKind::Const(self.parse_const(is_pub, start)?)
        } else if self.at(&TokenKind::Use) {
            self.reject_public(public, "a `use` declaration");
            ItemKind::Use(self.parse_use(start)?)
        } else if self.at_extern_block() {
            self.reject_public(public, "an `extern` block");
            ItemKind::Extern(self.parse_extern_block(start)?)
        } else if matches!(self.peek(), TokenKind::Ident(_) | TokenKind::SelfType) {
            self.reject_public(public, "an implementation");
            ItemKind::Impl(self.parse_impl(start)?)
        } else {
            let found = describe(self.peek());
            let span = self.span();
            let message = format!("{EXPECTED_DECLARATION}, found {found}");
            self.error(codes::EXPECTED_ITEM, message, span);
            return None;
        };

        Some(Item { kind, span: start.merge(self.last_text_span()), doc: doc.map(|d| d.text) })
    }

    fn reject_public(&mut self, public: Option<Span>, what: &str) {
        if let Some(span) = public {
            self.error(
                codes::MISPLACED_PUBLIC,
                format!("`public` has no meaning on {what}"),
                span,
            );
        }
    }

    // --- functions -------------------------------------------------------

    /// `def name of T(params) -> T where ..:`, and the same for `tool`.
    ///
    /// The two forms share every production. What `form` decides is which of
    /// `mcp-servers.md` §2.4's obligations are checked on the way past —
    /// generics (`SC0191`), a receiver (`SC0193`), a borrowed parameter
    /// (`SC0192`) and a missing body (`SC0198`) — and where a `##` run before
    /// a parameter is Decision 7's description rather than `SC0194`.
    fn parse_fn(&mut self, form: FnForm, is_pub: bool, start: Span) -> Option<FnDecl> {
        self.advance(); // `def` or `tool`
        let name = self.expect_ident()?;
        // Taken before the list is read so that the fix can delete the `[`
        // along with what follows it; afterwards the bracket is gone from
        // view. A tool written with the refused `of T` spelling is not
        // caught here — `parse_generic_params` already reported that, and a
        // second diagnostic about the same bracket would be the cascade
        // `check_tool_is_concrete` exists to avoid elsewhere.
        let generics_span = self.at(&TokenKind::LBracket).then(|| self.span());
        let generics = self.parse_generic_params();
        if form == FnForm::Tool {
            self.check_tool_is_concrete(generics_span, &generics);
        }
        let (self_param, params) = self.parse_params(form);
        // The receiver is dropped rather than kept, so the tree holds the tool
        // the author meant and no later phase has to decide what a module-level
        // function with a receiver is. `report_while_word` recovers the same
        // way and for the same reason.
        let self_param = match (form, self_param) {
            (FnForm::Tool, Some(receiver)) => {
                self.report_tool_receiver(receiver.span);
                None
            }
            (_, self_param) => self_param,
        };
        if form == FnForm::Tool {
            for param in &params {
                self.check_tool_parameter_is_owned(&param.ty);
            }
        }
        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type())
        } else if self.at_returns_word() {
            self.report_returns_word();
            Some(self.parse_type())
        } else {
            None
        };
        let where_clause = self.parse_where_clause();

        // No `:` means no body, which is exactly an interface's required
        // method.
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
            // Only on this branch. The one above has already reported a
            // missing `:` over the same declaration, and saying twice that a
            // body is absent is the cascade this block exists to avoid.
            if form == FnForm::Tool {
                self.report_tool_without_body(&name);
            }
            None
        };

        Some(FnDecl {
            form,
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

    /// Whether the parameter list is followed by the word `returns`, which
    /// §4.4 used to spell the return type before `->` replaced it.
    fn at_returns_word(&self) -> bool {
        matches!(self.peek(), TokenKind::Ident(name) if name == "returns")
    }

    /// Reports the old `returns` spelling and steps over the word.
    ///
    /// `returns` is an ordinary identifier now, so without this the parser
    /// would end the signature at the parameter list and complain that a name
    /// is not the end of a line — the symptom, not the mistake. Recovering as
    /// if `->` had been written keeps the return type in the tree, so the rest
    /// of the file parses as the programmer meant it.
    fn report_returns_word(&mut self) {
        let span = self.span();
        self.diagnostics.push(
            Diagnostic::error(codes::RETURNS_WORD, "the return type is written `->`")
                .with_label(Label::primary(span, "`returns` is not a keyword in Science"))
                .with_suggestion(Suggestion {
                    span,
                    replacement: "->".to_string(),
                    message: "write the return type with".to_string(),
                }),
        );
        self.advance();
    }

    // --- syntax revision 2 migration -------------------------------------
    //
    // Every word below was a keyword before the revision and is an ordinary
    // identifier after it. Left to the grammar each one produces a message
    // about a name where a colon belongs, or a call to something undefined —
    // the symptom, never the mistake. Each of these reports the mistake, and
    // each recovers by parsing the construct the programmer meant, so one
    // stale keyword costs one diagnostic instead of a cascade.

    /// Whether the cursor is on `trait Name`, the pre-revision spelling of
    /// `interface Name` (§6).
    fn at_trait_word(&self) -> bool {
        self.at_word_declaration(0, "trait")
    }

    /// Reports the old `trait` spelling and steps over the word.
    fn report_trait_word(&mut self) {
        let span = self.span();
        self.diagnostics.push(
            Diagnostic::error(codes::TRAIT_WORD, "the declaration is written `interface`")
                .with_label(Label::primary(span, "`trait` is not a keyword in Science"))
                .with_suggestion(Suggestion {
                    span,
                    replacement: "interface".to_string(),
                    message: "declare the interface with".to_string(),
                }),
        );
        self.advance();
    }

    /// Reports `for each x in xs` and steps over the `each` (§2.1).
    ///
    /// `each` is still a keyword — it is the implicit closure subject in
    /// `docs.map(each.title)` — so this is the one migration here whose stale
    /// word is still a token. Only the loop dropped it.
    ///
    /// **The decision.** The primary label covers `for each`, which is the
    /// span the suggestion replaces, and not the word `each` alone. The same
    /// is done in `report_methods_after_has` and `report_while_word`, the
    /// other two reporters in this block whose fix is wider than one word.
    ///
    /// **The reason.** The renderer prints a caret and a `= help:` line and
    /// names the span of neither, so the reader has only the caret to tell
    /// them what the replacement replaces. Under a caret on `each` alone, the
    /// help line *"write the loop with: `for`"* reads as an instruction to
    /// write a word that is already there — and taken literally it yields
    /// `for for row in rows`. Widening the caret to the replaced text makes
    /// the two halves of the diagnostic describe one edit.
    ///
    /// **The cost.** The caret now covers `for`, which the reader did not get
    /// wrong, so the label has to keep naming the word that is wrong. The
    /// alternative — teaching the renderer to draw the suggestion's span
    /// whenever it differs from the label's — fixes the class rather than the
    /// three instances, and is a change to every diagnostic in the compiler
    /// rather than to these.
    fn report_each_after_for(&mut self, for_span: Span) {
        let span = self.span();
        let head = for_span.merge(span);
        self.diagnostics.push(
            Diagnostic::error(codes::EACH_AFTER_FOR, "the loop is written `for x in xs`")
                .with_label(Label::primary(head, "`each` is not part of the loop"))
                .with_suggestion(Suggestion {
                    span: head,
                    replacement: "for".to_string(),
                    message: "write the loop with".to_string(),
                })
                .with_note(
                    "`each` names the subject of a call, as in `docs.map(each.title)`, \
                     and nothing else",
                ),
        );
        self.advance();
    }

    /// Reports `Type has methods:` and steps over the `methods` (§5).
    ///
    /// The primary label covers `has methods`, the span the fix replaces, for
    /// the reason written out over `report_each_after_for`: this was the
    /// clearest of the three, because a caret under `methods` beside *"open
    /// the block with: `has`"* told the reader to write the word they had
    /// already written, and applying it by hand gave `Doc has has:`.
    fn report_methods_after_has(&mut self, has_span: Span) {
        let span = self.span();
        let clause = has_span.merge(span);
        self.diagnostics.push(
            Diagnostic::error(
                codes::METHODS_AFTER_HAS,
                "the inherent block is written `Type has:`",
            )
            .with_label(Label::primary(clause, "`methods` is not a keyword in Science"))
            .with_suggestion(Suggestion {
                span: clause,
                replacement: "has".to_string(),
                message: "open the block with".to_string(),
            }),
        );
        self.advance();
    }

    /// Whether the cursor is on `while <expression>`, the loop §2.2 removed.
    fn at_while_word(&self) -> bool {
        self.word_at(0, "while") && self.starts_expr(self.peek_ahead(1))
    }

    /// Reports `while cond:` and steps over `while` and its condition.
    ///
    /// The fix replaces the whole head with `loop`, which is the part a tool
    /// can apply: what is left is a well-formed loop over the same body. The
    /// note carries the half a span cannot — that the condition comes back as
    /// the body's first statement, or that a count belongs in a range.
    /// Recovery drops the condition for the same reason, so the tree matches
    /// what applying the fix would produce.
    ///
    /// The primary label covers that whole head, for `report_each_after_for`'s
    /// reason and for one more of its own: the condition really is discarded,
    /// by the fix and by the recovery alike, and a caret on `while` alone said
    /// that only the word was wrong. The label says why the condition is
    /// underlined, because a reader who sees `n > 0` marked and is told only
    /// that `while` is not a keyword has been shown two facts and given one.
    fn report_while_word(&mut self, start: Span) -> Expr {
        self.advance(); // `while`
        // Read the condition and drop it. Keeping it would mean inventing a
        // node the language no longer has, and the fix below discards the same
        // text, so the tree and the fix agree.
        self.parse_expr();
        let head = start.merge(self.last_text_span());
        self.diagnostics.push(
            Diagnostic::error(codes::WHILE_WORD, "the unbounded loop is written `loop`")
                .with_label(Label::primary(
                    head,
                    "`while` is not a keyword in Science, and `loop` takes no condition",
                ))
                .with_suggestion(Suggestion {
                    span: head,
                    replacement: "loop".to_string(),
                    message: "open the loop with".to_string(),
                })
                .with_note(
                    "end it with the condition negated: make `if not …: break` the first \
                     statement of the body, or write a count as a range, `for i in 0..n:`",
                ),
        );
        let body = self.parse_block();
        let span = start.merge(self.last_text_span());
        Expr { kind: ExprKind::Loop { body }, span }
    }

    /// Whether the cursor is on `function <name>`, the declaration revision 3
    /// renamed to `def`.
    ///
    /// The lookahead is an identifier, which is what separates the keyword
    /// that was from a variable called `function` — now an ordinary name.
    fn at_function_word(&self) -> bool {
        self.at_word_declaration(0, "function")
    }

    /// Reports `function` and steps over it, so `parse_fn` sees what it
    /// expects and the rest of the declaration parses normally.
    ///
    /// Unlike `try`, this one *can* offer a machine-applicable fix, because
    /// the replacement is a word for a word and nothing around it moves. It is
    /// the migration that will be needed most: a model's priors are Python's,
    /// and `def-and-lambda.md` §3.2 expected the traffic to run the other way.
    fn report_function_word(&mut self) {
        // Reports without consuming. `parse_fn` opens by advancing over the
        // declaration keyword, so advancing here too ate the function's
        // *name* and produced a second, nonsense diagnostic: "expected an
        // identifier, found `(`" about a name the author spelled correctly.
        // `report_trait_word` beside this one was already factored this way
        // and is why it never cascaded.
        let span = self.span();
        self.diagnostics.push(
            Diagnostic::error(codes::FUNCTION_WORD, "the declaration is written `def`")
                .with_label(Label::primary(span, "`function` is not a keyword in Science"))
                .with_suggestion(Suggestion {
                    span,
                    replacement: "def".to_string(),
                    message: "write the declaration as".to_string(),
                })
                .with_note("`def` declares every function, method and interface member"),
        );
    }

    // --- the `tool` declaration ------------------------------------------
    //
    // `mcp-servers.md` §2.4 is the whole case for the keyword: a `tool`
    // carries five obligations an ordinary `def` does not, and a marker that
    // carries none is an attribute rather than a declaration form. Four of the
    // five are checkable here with no types at all, and they are checked here.
    // The fifth — that every parameter type survives a round trip through JSON
    // Schema — is `SC0504`-`SC0518` and waits for the type checker.
    //
    // Each of these reports once and leaves the parser standing where an
    // ordinary `def` would have left it, so one mistake costs one diagnostic.
    // `report_function_word` above is the cautionary tale: it advanced over a
    // word the parser then advanced over again, and the second diagnostic was
    // nonsense about a name the author had spelled correctly.

    /// `SC0190` and `SC0195`: a `tool` is declared with a description.
    ///
    /// Decision 5 makes this the only construct in Science for which
    /// documentation is mandatory, and the justification has to be narrow or
    /// it becomes "document your code", which a compiler has no business
    /// enforcing. It is narrow: everywhere else a doc comment is read by
    /// someone who has already decided to call the function, and here it is
    /// read by the caller *in order to* decide. The compiler knows this
    /// because the author wrote `tool`.
    ///
    /// **The decision.** `SC0195` puts its caret on the `##` run and names the
    /// `tool` with a second label. `SC0190` keeps its caret on the word
    /// `tool`.
    ///
    /// **The reason.** The two are about different things. `SC0195` is about
    /// the run: it exists, and its first line is blank, and that first line is
    /// the text the reader has to write. `SC0190` is about a run that is not
    /// there, and an absent run has no span, so the declaration that needed
    /// one is the only honest place to point.
    ///
    /// **The cost.** `SC0195` now points somewhere the word `tool` is not, so
    /// it carries a secondary label to say which declaration is meant —
    /// without it the snippet would show the run alone and the reader would
    /// have to count lines to find what it belongs to.
    fn check_tool_description(&mut self, doc: Option<&DocComment>) {
        let span = self.span();
        let Some(doc) = doc else {
            self.diagnostics.push(
                Diagnostic::error(
                    codes::TOOL_WITHOUT_DESCRIPTION,
                    "a `tool` is declared with a description",
                )
                .with_label(Label::primary(span, "this `tool` has no `##` comment above it"))
                .with_note(
                    "the description is what a model reads in order to decide whether to \
                     call the tool, so it is program data and not documentation",
                )
                .with_note(
                    "an undescribed tool does not fail: it is simply never chosen, and no \
                     test catches that",
                ),
            );
            return;
        };
        // §5.2 splits the run where `strings-formatting-and-docs.md` §5.4
        // already splits one: the first line is the summary, and the short
        // title a caller displays is that summary. A run that opens with a
        // blank `##` has no first line to be it.
        if doc.text.lines().next().is_none_or(str::is_empty) {
            self.diagnostics.push(
                Diagnostic::error(
                    codes::TOOL_SUMMARY_BLANK,
                    "a `tool`'s description opens with its summary",
                )
                .with_label(Label::primary(doc.span, "this run opens with a blank `##`"))
                .with_label(Label::secondary(span, "the `tool` it describes"))
                .with_note(
                    "the first line, up to the first blank `##`, is the tool's title; the \
                     whole comment is its description, summary included",
                ),
            );
        }
    }

    /// `SC0191`: a `tool` is not generic.
    ///
    /// The list a model is given is flat and concrete, with one parameter
    /// schema per entry, so there is nothing for a type parameter to be
    /// instantiated at (§2.4 item 2).
    ///
    /// The parameters are **kept** in the tree after the report. Dropping them
    /// would leave every mention of `T` in the signature unresolved, and one
    /// stale word would cost a diagnostic plus a name-resolution failure per
    /// use — which is the cascade, arriving from a later phase.
    fn check_tool_is_concrete(&mut self, generics_span: Option<Span>, generics: &[GenericParam]) {
        let (Some(generics_span), Some(last)) = (generics_span, generics.last()) else { return };
        let clause = generics_span.merge(last.span);
        self.diagnostics.push(
            Diagnostic::error(codes::GENERIC_TOOL, "a `tool` is not generic")
                .with_label(Label::primary(clause, "a type parameter has no single schema"))
                .with_note(
                    "one tool is one entry in the list its caller is given, with one \
                     parameter schema; write one tool per concrete type",
                )
                .with_suggestion(Suggestion {
                    span: clause,
                    replacement: String::new(),
                    message: "make the declaration concrete".to_string(),
                }),
        );
    }

    /// `SC0192`: a `tool` owns its arguments.
    ///
    /// There is no caller to borrow from. Every argument arrived from outside
    /// the program a moment ago and belongs to the tool (§2.4 item 3).
    fn check_tool_parameter_is_owned(&mut self, ty: &Type) {
        if !matches!(ty.kind, TypeKind::Borrowed { .. }) {
            return;
        }
        let words = self.borrow_words(ty);
        self.diagnostics.push(
            Diagnostic::error(codes::BORROWED_TOOL_PARAMETER, "a `tool` owns its arguments")
                .with_label(Label::primary(words, "there is nothing here to borrow from"))
                .with_note(
                    "a tool's arguments are built from what the caller sent, so the tool is \
                     the only owner there is",
                )
                .with_suggestion(Suggestion {
                    span: words,
                    replacement: String::new(),
                    message: "take the value".to_string(),
                }),
        );
    }

    /// The span of the `&` or `&mut` that opens a type.
    ///
    /// A [`Type`] records *that* it is borrowed and not where the sigil sits,
    /// because until now nothing needed to point at it. It is recovered from
    /// the token stream instead of being added to every type in the tree for
    /// one diagnostic's sake: a borrow prefix is one or two tokens, it begins
    /// where the type begins, and the scan stops on the third.
    ///
    /// **Matches `Amp`/`Mut`, not `Borrowed`/`Mutable`.** The two keywords the
    /// old spelling used are still lexable — kept only so `borrowed T` can be
    /// refused by name (§4.3) — but a well-formed borrow never contains them
    /// once the parser gets here, so scanning for them would leave this
    /// function returning `ty.span`'s fallback on every call and the
    /// diagnostic's `help: take the value: delete …` pointing at the whole
    /// type, generic arguments and all, rather than the one or two tokens
    /// that actually need to go.
    fn borrow_words(&self, ty: &Type) -> Span {
        let first = self.tokens.partition_point(|t| t.span.start < ty.span.start);
        let mut words = None;
        for token in &self.tokens[first..] {
            match token.kind {
                TokenKind::Amp | TokenKind::Mut => {
                    words = Some(words.map_or(token.span, |s: Span| s.merge(token.span)));
                }
                _ => break,
            }
        }
        words.unwrap_or(ty.span)
    }

    /// `SC0193`: a `tool` is not a method.
    fn report_tool_receiver(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(codes::TOOL_WITH_RECEIVER, "a `tool` is not a method")
                .with_label(Label::primary(span, "a tool has no receiver"))
                .with_note(
                    "a tool is called by its name alone, and a caller outside the program \
                     has no value to bind `self` to; declare it at module level and take \
                     what it needs as a parameter",
                ),
        );
    }

    /// `SC0194`: only a `tool` documents its parameters one by one.
    ///
    /// Decision 7 is deliberately scoped to `tool`, so that the general
    /// question of documenting a `def`'s parameters stays open and belongs to
    /// `strings-formatting-and-docs.md`. There is no applicable fix: moving
    /// prose from one comment into another is an edit, not a substitution.
    ///
    /// **The decision.** The caret goes on the `##` run and the parameter's
    /// name takes a secondary label.
    ///
    /// **The reason.** The run is the text that has to move. Pointing at the
    /// name put the caret on the line *below* the mistake and described
    /// something the reader had written correctly.
    ///
    /// **The cost.** Two labels where there was one, and the run may be
    /// several lines, in which case the renderer marks the first and says
    /// where the span ends rather than drawing a bar down the gutter.
    fn report_parameter_doc(&mut self, doc: Span, name: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                codes::PARAMETER_DOC_OUTSIDE_TOOL,
                "only a `tool` documents its parameters one by one",
            )
            .with_label(Label::primary(doc, "this `##` comment documents a parameter"))
            .with_label(Label::secondary(name, "the parameter it is attached to"))
            .with_note(
                "move the text into the `##` comment above the declaration; a `tool` \
                 documents a parameter here because each one becomes a described field of \
                 the schema its caller reads",
            ),
        );
    }

    /// `SC0197`: a `tool` is declared at the top level of a module.
    ///
    /// Reports **without consuming**, for `report_function_word`'s reason in
    /// reverse: both callers hand a `None` back to a loop that synchronises,
    /// and synchronising is what drops the declaration and the block under it
    /// in one step. Advancing here as well would leave the signature's tokens
    /// to be read as something else.
    fn report_tool_out_of_place(&mut self) {
        let span = self.span();
        self.diagnostics.push(
            Diagnostic::error(
                codes::TOOL_NOT_AT_MODULE_LEVEL,
                "a `tool` is declared at the top level of a module",
            )
            .with_label(Label::primary(span, "this one is not"))
            .with_note(
                "a tool is offered to its caller by name alone: a method would need a \
                 receiver the caller cannot supply, and a declaration inside a body is not \
                 visible outside it",
            ),
        );
    }

    /// `SC0198`: a `tool` has a body.
    fn report_tool_without_body(&mut self, name: &Ident) {
        self.diagnostics.push(
            Diagnostic::error(codes::TOOL_WITHOUT_BODY, "a `tool` is declared with a body")
                .with_label(Label::primary(name.span, "this one is a signature and nothing else"))
                .with_note(
                    "there is no abstract tool: a signature without a body is an \
                     interface's required method, and that is declared with `def`",
                ),
        );
    }

    /// Whether the cursor is on `prompt <name>` or `agent <name>`.
    ///
    /// The lookahead is what makes it *declaration position* rather than any
    /// use of the word, so the two are told apart the same way `trait` and
    /// `function` are.
    fn at_reserved_declaration_word(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Reserved(ReservedWord::Prompt | ReservedWord::Agent)
        ) && matches!(self.peek_ahead(1), TokenKind::Ident(_))
    }

    /// `SC0196`: `prompt` and `agent` are reserved and declare nothing.
    ///
    /// Decision 2 spends `tool` and only `tool`, and this is the diagnostic
    /// that says so to someone who reasonably expected all three. Without it
    /// the word falls through to `SC0101`, which describes it as *"reserved
    /// for a later phase"* — a phrase that reads, to anyone who knows what a
    /// compiler phase is, as though a later pass will accept it.
    ///
    /// Reports without consuming: `parse_item` returns `None` and the module
    /// loop synchronises over the declaration and its block.
    fn report_reserved_declaration_word(&mut self) {
        let span = self.span();
        let (word, note) = match self.peek() {
            TokenKind::Reserved(ReservedWord::Prompt) => (
                "prompt",
                "a prompt is an ordinary function returning `Array[Message]`; its \
                 arguments carry no schema, so there is nothing for a declaration form to \
                 derive and nothing for it to check",
            ),
            _ => (
                "agent",
                "`agent` is the other direction — a program calling a model, rather than a \
                 model calling a program — and the language has nothing for it yet",
            ),
        };
        self.diagnostics.push(
            Diagnostic::error(
                codes::RESERVED_DECLARATION_WORD,
                format!("`{word}` is reserved and declares nothing"),
            )
            .with_label(Label::primary(span, format!("a declaration cannot begin with `{word}`")))
            .with_note(note)
            .with_note(
                "`tool` is the one word of the three that was spent, because it is the one \
                 whose obligations a compiler can check",
            ),
        );
    }

    /// Whether the cursor is on `try <expression>`, the prefix §3 removed.
    ///
    /// The lookahead is what separates the keyword that was from a variable
    /// named `try`, which is now a perfectly ordinary name: `try f()` is the
    /// old syntax, `try be 3` is a binding, and only the first has an
    /// expression after the word.
    fn at_try_word(&self) -> bool {
        self.word_at(0, "try") && self.starts_expr(self.peek_ahead(1))
    }

    /// Reports `try e` and returns the expression without it.
    ///
    /// There is no suggestion here, and that is deliberate. Every other
    /// migration code in this block renames a word: `while` becomes `loop`,
    /// `println` becomes `print`, and the fix is a span and a replacement a
    /// tool can apply. `try` has no replacement — the new model turns one
    /// expression into a binding, a test and a return, and which value the
    /// function should return on the error path is not something this phase
    /// can know. Emitting a machine-applicable fix that dropped the error
    /// would be worse than emitting none.
    ///
    /// Recovery keeps the operand, so the rest of the statement still parses
    /// and the reader gets the errors after this one.
    fn report_try_word(&mut self) -> Expr {
        let word = self.advance().span; // `try`
        self.diagnostics.push(
            Diagnostic::error(codes::TRY_WORD, "`try` was removed with the `Result` type")
                .with_label(Label::primary(word, "there is no `try` in Science"))
                .with_note(
                    "a function that can fail returns its value and an error: write `let value, err be f()`, then `if err?:` and return",
                ),
        );
        self.parse_postfix()
    }

    /// Reports `println(..)` and returns the `print` it stands for (§3.5).
    fn report_println_word(&mut self) -> Expr {
        let span = self.advance().span;
        self.diagnostics.push(
            Diagnostic::error(codes::PRINTLN_WORD, "the free function is `print`")
                .with_label(Label::primary(span, "there is no `println` in Science"))
                .with_suggestion(Suggestion {
                    span,
                    replacement: "print".to_string(),
                    message: "write the call as".to_string(),
                })
                .with_note("`print` writes a newline; `write` is the form that does not"),
        );
        let name = Ident::new("print".to_string(), span);
        let segment = PathSegment { name, generics: Vec::new(), span };
        Expr { kind: ExprKind::Path(Path { segments: vec![segment], span }), span }
    }

    /// `(a: T, b: U)`, optionally opening with a `self` receiver.
    fn parse_params(&mut self, form: FnForm) -> (Option<SelfParam>, Vec<Param>) {
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
                match self.parse_param(form) {
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

    /// The three receivers of §4.4: `self`, `mutable self`, and the by-value
    /// `self: Self`, which is written as an ordinary annotated parameter and
    /// is the only one that takes the value away from the caller.
    fn try_parse_self_param(&mut self) -> Option<SelfParam> {
        let start = self.span();
        if self.at(&TokenKind::Mutable) && matches!(self.peek_ahead(1), TokenKind::SelfValue) {
            self.advance();
            self.advance();
            return Some(SelfParam {
                kind: SelfKind::Mutable,
                span: start.merge(self.last_text_span()),
            });
        }
        if !self.at(&TokenKind::SelfValue) {
            return None;
        }
        self.advance();
        if self.eat(&TokenKind::Colon).is_none() {
            return Some(SelfParam { kind: SelfKind::Shared, span: start });
        }

        // The annotation is dropped because it carries nothing: a receiver's
        // type is the implementing type, whatever it is written as. It is
        // still checked, so `self: Int` cannot pass for a by-value receiver.
        let ty = self.parse_type();
        if !matches!(ty.kind, TypeKind::SelfType | TypeKind::Error) {
            self.error(
                codes::EXPECTED_TYPE,
                "a `self` parameter can only be annotated `Self`, which is what takes the \
                 receiver by value",
                ty.span,
            );
        }
        Some(SelfParam { kind: SelfKind::Value, span: start.merge(self.last_text_span()) })
    }

    fn parse_param(&mut self, form: FnForm) -> Option<Param> {
        let start = self.span();
        // A parameter's first token is its name, and the lexer hangs a `##`
        // run on whatever token comes next whether that is a declaration or
        // not — which is what makes Decision 7 free here and `SC0194`
        // necessary everywhere else.
        let doc = self.peek_doc();
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type();
        let doc = match (form, doc) {
            (FnForm::Tool, doc) => doc.map(|d| d.text),
            (FnForm::Def, Some(found)) => {
                self.report_parameter_doc(found.span, name.span);
                None
            }
            (FnForm::Def, None) => None,
        };
        Some(Param { name, ty, doc, span: start.merge(self.last_text_span()) })
    }

    // --- records, choices, aliases and constants --------------------------

    /// `type` introduces two declarations, told apart by what follows the
    /// name: `is` makes it an alias (§4.4), a block makes it a record. The
    /// generic parameters come first either way, so `type Handle[T] is
    /// Box[T]` needs no lookahead beyond the one token.
    fn parse_type_item(&mut self, is_pub: bool, start: Span) -> Option<ItemKind> {
        self.advance(); // `type`
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();

        if self.eat(&TokenKind::Is).is_some() {
            let ty = self.parse_type();
            self.expect_line_end();
            return Some(ItemKind::Alias(AliasDecl {
                is_pub,
                name,
                generics,
                ty,
                span: start.merge(self.last_text_span()),
            }));
        }

        let where_clause = self.parse_where_clause();
        if !self.at(&TokenKind::Colon) {
            let found = describe(self.peek());
            let span = self.span();
            self.error(
                codes::UNEXPECTED_TOKEN,
                format!(
                    "expected `:` to open the type's fields or `is` to alias it, found {found}"
                ),
                span,
            );
            return None;
        }

        let fields = self.parse_indented_body(Self::parse_field);
        Some(ItemKind::Record(RecordDecl {
            is_pub,
            name,
            generics,
            where_clause,
            fields,
            span: start.merge(self.last_text_span()),
        }))
    }

    fn parse_field(&mut self) -> Option<FieldDef> {
        let start = self.span();
        let is_pub = self.eat(&TokenKind::Public).is_some();
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type();
        Some(FieldDef { is_pub, name, ty, span: start.merge(self.last_text_span()) })
    }

    fn parse_choice(&mut self, is_pub: bool, start: Span) -> Option<ChoiceDecl> {
        self.advance(); // `choice`
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();
        let where_clause = self.parse_where_clause();
        let variants = self.parse_indented_body(Self::parse_variant);
        Some(ChoiceDecl {
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

    /// `const WIDTH be 768`, with the same optional annotation a `let` takes.
    fn parse_const(&mut self, is_pub: bool, start: Span) -> Option<ConstDecl> {
        self.advance(); // `const`
        let name = self.expect_ident()?;
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect(&TokenKind::Be, "`be`")?;
        let value = self.parse_expr();
        self.expect_line_end();
        Some(ConstDecl { is_pub, name, ty, value, span: start.merge(self.last_text_span()) })
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

    // --- extern blocks ---------------------------------------------------
    //
    // §9 of `ffi-c-boundary.md` calls this "a small separate grammar with
    // contextual keywords", and that is what the code below is: nothing here
    // reaches back into the declaration grammar except `parse_type` and
    // `parse_param`, and nothing outside this section reaches in.
    //
    // `library`, `via`, `pkg-config`, `kind`, `when`, `available`, `symbol`,
    // `size` and `align` are ordinary identifiers everywhere in the language,
    // including inside the block wherever they are not in one of the two or
    // three positions below. `reserved-words.md` argues for keeping the
    // reserved list short and the FFI note took the same view; the price is
    // that every one of them is recognised here by `word_at` rather than by a
    // token test, and the price is worth paying for words as ordinary as
    // `kind` and `size`.
    //
    // `static` and `union` are the exceptions, and they are not exceptions to
    // the principle: both are already in §13's reserved list, so the block's
    // grammar spends words that were frozen long ago rather than freezing two
    // more.

    /// Whether the cursor is on an `extern` block, with or without its
    /// `unsafe`.
    fn at_extern_block(&self) -> bool {
        self.at(&TokenKind::Extern)
            || (self.at(&TokenKind::Unsafe) && self.at_ahead(1, &TokenKind::Extern))
    }

    /// `unsafe extern "C" library "openblas" via pkg-config "openblas":`
    ///
    /// Every clause after the ABI is optional to the *parser* — a block that
    /// has lost one is reported and then parsed to the end, because the items
    /// inside it are what the rest of the file refers to and losing them would
    /// turn one missing word into an error per foreign call.
    fn parse_extern_block(&mut self, start: Span) -> Option<ExternBlock> {
        let is_unsafe = self.eat(&TokenKind::Unsafe).is_some();
        let extern_span = self.advance().span; // `extern`
        if !is_unsafe {
            self.report_extern_without_unsafe(extern_span);
        }

        let abi = self.expect_str("the ABI as a string, `\"C\"`")?;
        if abi.value != "C" {
            self.report_unknown_abi(&abi);
        }

        let library = self.parse_library_clause();
        if library.is_none() {
            self.report_missing_library(start.merge(abi.span));
        }

        let items = self.parse_indented_body(Self::parse_extern_item);
        Some(ExternBlock {
            is_unsafe,
            abi,
            library,
            items,
            span: start.merge(self.last_text_span()),
        })
    }

    /// A string literal, or a report that one belonged here.
    fn expect_str(&mut self, what: &str) -> Option<StrLit> {
        if let TokenKind::Str(value) = self.peek() {
            let value = value.clone();
            let span = self.advance().span;
            return Some(StrLit { value, span });
        }
        let found = describe(self.peek());
        let span = self.span();
        self.error(codes::UNEXPECTED_TOKEN, format!("expected {what}, found {found}"), span);
        None
    }

    /// §1.1: the `unsafe` is on the block because writing the declaration is
    /// itself the unverifiable act. The fix is exact, so it is a suggestion.
    fn report_extern_without_unsafe(&mut self, extern_span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::EXTERN_NOT_UNSAFE,
                "an `extern` block is written `unsafe extern`",
            )
            .with_label(Label::primary(extern_span, "this block declares what cannot be checked"))
            .with_suggestion(Suggestion {
                span: extern_span,
                replacement: "unsafe extern".to_string(),
                message: "mark the block with".to_string(),
            })
            .with_note(
                "the declaration claims a symbol of this name exists, takes these arguments \
                 at these widths, and neither retains nor frees the pointers it is given — \
                 none of which the compiler can see",
            ),
        );
    }

    /// §1.1: `"C"` is the only ABI in this phase, and Fortran is one of them.
    ///
    /// No suggestion: replacing the string would silently produce a binding
    /// that links and is wrong, which is the failure mode the whole note is
    /// written against. The note says instead where the difference really
    /// lives.
    fn report_unknown_abi(&mut self, abi: &StrLit) {
        self.diagnostics.push(
            Diagnostic::error(ffi_codes::UNKNOWN_ABI, "the only ABI in this phase is `\"C\"`")
                .with_label(Label::primary(
                    abi.span,
                    format!("`{}` names no calling convention Science emits", abi.value),
                ))
                .with_note(
                    "Fortran BLAS and LAPACK are `\"C\"` too: their difference is not the \
                     calling convention but the argument passing, which is expressed in the \
                     types",
                ),
        );
    }

    /// §5.1: the library name is the link target, and `via pkg-config` does
    /// not replace it — it says how to find the flags for it.
    fn report_missing_library(&mut self, head: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::MISSING_LIBRARY,
                "an `extern` block names the library it comes from",
            )
            .with_label(Label::primary(head, "no `library` clause on this block"))
            .with_note(
                "write `library \"openblas\"` after the ABI; a block discovering its flags \
                 with `via pkg-config` still declares the plain name as the fallback, \
                 because pkg-config is not present on every platform",
            ),
        );
    }

    /// `library "openblas" via pkg-config "openblas" kind static when available`.
    ///
    /// The three trailing clauses are read in a loop rather than in a fixed
    /// order: each modifies how this one library is found or linked, none of
    /// them interacts with another, and an order would be a rule for a reader
    /// to remember for no gain.
    ///
    /// The clauses are read whether or not the `library` name is there. A
    /// block that opens `extern "C" via pkg-config "zlib":` has one mistake in
    /// it, and consuming the clause it *did* write is what keeps that one
    /// mistake from turning the `:` into a second error and the body into a
    /// third.
    fn parse_library_clause(&mut self) -> Option<LibraryClause> {
        let start = self.span();
        let name = if self.word_at(0, "library") {
            self.advance();
            self.expect_str("the library's name as a string")
        } else {
            None
        };
        let mut pkg_config = None;
        let mut static_link = None;
        let mut when_available = None;

        loop {
            if self.word_at(0, "via") {
                self.advance();
                if !self.expect_pkg_config() {
                    self.recover_to_block_colon();
                    break;
                }
                pkg_config = self.expect_str("the pkg-config module name as a string");
                if pkg_config.is_none() {
                    self.recover_to_block_colon();
                    break;
                }
            } else if self.word_at(0, "kind") {
                let kind_span = self.advance().span;
                match self.eat(&TokenKind::Reserved(ReservedWord::Static)) {
                    Some(token) => static_link = Some(kind_span.merge(token.span)),
                    None => {
                        let found = describe(self.peek());
                        let span = self.span();
                        self.error(
                            codes::UNEXPECTED_TOKEN,
                            format!("expected `static` after `kind`, found {found}"),
                            span,
                        );
                        self.recover_to_block_colon();
                        break;
                    }
                }
            } else if self.word_at(0, "when") && self.word_at(1, "available") {
                let when_span = self.advance().span;
                let available_span = self.advance().span;
                when_available = Some(when_span.merge(available_span));
            } else {
                break;
            }
        }

        Some(LibraryClause {
            name: name?,
            pkg_config,
            static_link,
            when_available,
            span: start.merge(self.last_text_span()),
        })
    }

    /// Drops the rest of a broken block header, stopping in front of the `:`
    /// that opens the body.
    ///
    /// The header is one line and the body is the part the rest of the file
    /// refers to, so recovery here throws away the line and keeps the block:
    /// `synchronize` would take the indented region with it and turn a
    /// misspelled clause into an unresolved name per foreign call.
    fn recover_to_block_colon(&mut self) {
        loop {
            match self.peek() {
                TokenKind::Colon
                | TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::Eof => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// `pkg-config`, which is three tokens and one word.
    ///
    /// The lexer has no reason to know the name of a build tool, so it reads
    /// `pkg-config` as `pkg`, `-`, `config` like any other subtraction. Here
    /// there is no expression for a subtraction to be part of, and the three
    /// tokens are only accepted when they are written with nothing between
    /// them — `pkg - config` is not the name of anything.
    fn expect_pkg_config(&mut self) -> bool {
        let joined = self.word_at(0, "pkg")
            && self.at_ahead(1, &TokenKind::Minus)
            && self.word_at(2, "config")
            && self.token_at(0).span.end == self.token_at(1).span.start
            && self.token_at(1).span.end == self.token_at(2).span.start;
        if joined {
            self.advance();
            self.advance();
            self.advance();
            return true;
        }
        let found = describe(self.peek());
        let span = self.span();
        self.error(
            codes::UNEXPECTED_TOKEN,
            format!("expected `pkg-config` after `via`, found {found}"),
            span,
        );
        false
    }

    /// One line of a block's body: §1.2's three forms, plus the two
    /// `c-binding-coverage.md` adds.
    fn parse_extern_item(&mut self) -> Option<ExternItem> {
        let start = self.span();
        let kind = if self.at(&TokenKind::Function) {
            ExternItemKind::Fn(self.parse_extern_fn()?)
        } else if self.at(&TokenKind::Type) {
            ExternItemKind::Alias(self.parse_extern_alias()?)
        } else if self.at(&TokenKind::Const) {
            ExternItemKind::Const(self.parse_extern_const()?)
        } else if self.at(&TokenKind::Reserved(ReservedWord::Static)) {
            ExternItemKind::Static(self.parse_extern_static()?)
        } else if self.at(&TokenKind::Reserved(ReservedWord::Union)) {
            ExternItemKind::Union(self.parse_extern_union()?)
        } else {
            self.report_unknown_extern_item();
            return None;
        };
        Some(ExternItem { kind, span: start.merge(self.last_text_span()) })
    }

    /// §1.2 closed the list at three and `c-binding-coverage.md` reopened it
    /// once, so the message enumerates rather than describing: the list is
    /// short, and a reader who guessed wrong needs to see what is on it.
    fn report_unknown_extern_item(&mut self) {
        let found = describe(self.peek());
        let span = self.span();
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::UNKNOWN_EXTERN_ITEM,
                "this is not something an `extern` block may contain",
            )
            .with_label(Label::primary(span, format!("found {found}")))
            .with_note(
                "a block contains `def name(..) -> T`, `type Name is T`, \
                 `const NAME be literal as T`, `static NAME: T` and \
                 `union Name: size N align M`, and nothing else",
            )
            .with_note(
                "a record or a handle type is an ordinary `type` declaration outside the \
                 block, marked `implements ffi.CLayout`, because a handle needs a `Drop` \
                 the declaring module must own",
            ),
        );
    }

    /// `def cblas_dgemm(layout: CblasLayout, ..) -> Herr symbol "dgemm_"`.
    fn parse_extern_fn(&mut self) -> Option<ExternFn> {
        let start = self.advance().span; // `function`
        let name = self.expect_ident()?;
        let (params, variadic) = self.parse_extern_params();
        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type())
        } else if self.at_returns_word() {
            self.report_returns_word();
            Some(self.parse_type())
        } else {
            None
        };
        // §1.6: `symbol` decouples the Science name from the linker name, and
        // is what turns an ILP64/LP64 mismatch into an undefined symbol
        // instead of a wrong answer.
        let symbol = if self.word_at(0, "symbol") {
            self.advance();
            self.expect_str("the linker name as a string")
        } else {
            None
        };

        for param in &params {
            self.check_ffi_type(&param.ty, true);
        }
        if let Some(ret) = &ret {
            self.check_ffi_type(ret, true);
        }
        if let Some(span) = variadic {
            self.report_variadic_function(&name, span);
        }

        Some(ExternFn {
            name,
            params,
            ret,
            symbol,
            variadic,
            span: start.merge(self.last_text_span()),
        })
    }

    /// `(a: T, b: U)`, optionally ending in `...`.
    ///
    /// A separate reader from `parse_params`: an extern function has no `self`
    /// receiver to look for, and it is the only place in the language where a
    /// parameter list can end in something that is not a parameter.
    fn parse_extern_params(&mut self) -> (Vec<Param>, Option<Span>) {
        let mut params = Vec::new();
        let mut variadic = None;
        if self.expect(&TokenKind::LParen, "`(`").is_none() {
            return (params, variadic);
        }

        while !self.at(&TokenKind::RParen) {
            if let Some(span) = self.eat_ellipsis() {
                variadic = Some(span);
                self.eat(&TokenKind::Comma);
                break;
            }
            // `FnForm::Def`: an `extern` function is not a tool, so a `##`
            // run before one of its parameters is `SC0194` like any other.
            match self.parse_param(FnForm::Def) {
                Some(param) => params.push(param),
                None => self.recover_in_brackets(),
            }
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`");
        (params, variadic)
    }

    /// `...`, which is `..` followed by `.` and nothing between them.
    ///
    /// No `Ellipsis` token exists and none is added: three dots mean something
    /// in exactly one position in the language, and a token for it would put a
    /// C-only concept in the contract every other phase reads. Requiring the
    /// two spans to touch is what keeps a range followed by a field access
    /// from being read as one.
    fn eat_ellipsis(&mut self) -> Option<Span> {
        if !(self.at(&TokenKind::DotDot)
            && self.at_ahead(1, &TokenKind::Dot)
            && self.token_at(0).span.end == self.token_at(1).span.start)
        {
            return None;
        }
        let first = self.advance().span;
        let second = self.advance().span;
        Some(first.merge(second))
    }

    /// §1.4 and `c-binding-coverage.md` Decision 3: variadics stay
    /// unreachable, and the audit measured the cost at 0.84% of a surface
    /// whose non-variadic half always exists.
    ///
    /// No suggestion. Dropping the `...` would leave a declaration that links
    /// and passes its arguments under the wrong convention, which is worse
    /// than the error; the only fix is a shim, and a shim is not an edit to
    /// this line.
    fn report_variadic_function(&mut self, name: &Ident, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::VARIADIC_FUNCTION,
                format!("`{}` is variadic, and a variadic function cannot be declared", name.name),
            )
            .with_label(Label::primary(span, "C's variadic convention has no Science spelling"))
            .with_note(
                "the convention is per-platform and depends on the types of the arguments \
                 actually passed, so no fixed signature describes it",
            )
            .with_note(
                "where the library ships a `va_list` sibling — `PyErr_FormatV` beside \
                 `PyErr_Format`, `vfprintf` beside `fprintf` — bind that instead, behind a \
                 three-line C shim",
            ),
        );
    }

    /// `type BlasInt is I32` — a spelling for a C typedef (§1.2).
    fn parse_extern_alias(&mut self) -> Option<ExternAlias> {
        let start = self.advance().span; // `type`
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Is, "`is`")?;
        let ty = self.parse_type();
        self.check_ffi_type(&ty, true);
        Some(ExternAlias { name, ty, span: start.merge(self.last_text_span()) })
    }

    /// `const CBLAS_ROW_MAJOR be 101 as CblasLayout` (§1.2).
    ///
    /// The value is a literal and not an expression. §1.2 says the item
    /// transcribes a `#define` or an enumerator, and Decision 7 of
    /// `c-binding-coverage.md` turns on the distinction: a constant is a
    /// compile-time value, a `static` is a link-time address, and a grammar
    /// that let an expression in here would blur the two. A leading `-` is
    /// admitted because C enumerators are routinely negative.
    fn parse_extern_const(&mut self) -> Option<ExternConst> {
        let start = self.advance().span; // `const`
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Be, "`be`")?;
        let negative = self.eat(&TokenKind::Minus).is_some();
        let value = match literal_of(self.peek()) {
            Some(literal) => {
                self.advance();
                literal
            }
            None => {
                let found = describe(self.peek());
                let span = self.span();
                self.error(
                    codes::EXPECTED_EXPR,
                    format!("expected the constant's literal value, found {found}"),
                    span,
                );
                return None;
            }
        };
        self.expect(&TokenKind::As, "`as`, and the constant's type")?;
        let ty = self.parse_type();
        self.check_ffi_type(&ty, true);
        Some(ExternConst {
            name,
            negative,
            value,
            ty,
            span: start.merge(self.last_text_span()),
        })
    }

    /// `static H5T_NATIVE_DOUBLE_g: Hid` — `c-binding-coverage.md` Decision 7.
    ///
    /// The fourth item form, and the one the audit measured as the largest
    /// gap: `H5T_NATIVE_DOUBLE` expands to `(H5open(), H5T_NATIVE_DOUBLE_g)`,
    /// so without a way to name the global half HDF5 is 90% function-bindable
    /// and 0% usable.
    fn parse_extern_static(&mut self) -> Option<ExternStatic> {
        let start = self.advance().span; // `static`
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`, and the global's type")?;
        let ty = self.parse_type();
        // A global is reached through its address, never passed by value, so
        // the half-precision rule of §1.3 does not reach it.
        self.check_ffi_type(&ty, false);
        Some(ExternStatic { name, ty, span: start.merge(self.last_text_span()) })
    }

    /// `union H5R_ref_t: size 64 align 8` — `c-binding-coverage.md` §3.4.
    ///
    /// A C union has no Science layout and is not going to get one: the audit
    /// found two in HDF5 and one in CPython, and the remedy §1.4 already named
    /// — an opaque byte array of the right size and alignment — is the whole
    /// of the import. The arms are not written down because nothing in
    /// Science may read them; the library's own accessors (`H5Rget_type`,
    /// `Py_REFCNT`) are ordinary `function` items beside this one.
    fn parse_extern_union(&mut self) -> Option<ExternUnion> {
        let start = self.advance().span; // `union`
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon, "`:`, and the union's size and alignment")?;
        let size = self.eat_layout_number("size");
        let align = self.eat_layout_number("align");
        if size.is_none() || align.is_none() {
            let span = start.merge(self.last_text_span());
            self.report_union_without_layout(&name, span);
        }
        Some(ExternUnion { name, size, align, span: start.merge(self.last_text_span()) })
    }

    /// `size 64` or `align 8`: a contextual word and the byte count after it.
    fn eat_layout_number(&mut self, word: &str) -> Option<u128> {
        if !self.word_at(0, word) {
            return None;
        }
        self.advance();
        match self.peek() {
            TokenKind::Int { value, .. } => {
                let value = *value;
                self.advance();
                Some(value)
            }
            _ => {
                let found = describe(self.peek());
                let span = self.span();
                self.error(
                    codes::EXPECTED_EXPR,
                    format!("expected a byte count after `{word}`, found {found}"),
                    span,
                );
                None
            }
        }
    }

    /// Neither number can be inferred and neither can be guessed, so there is
    /// nothing to suggest — only the two places they can honestly come from.
    fn report_union_without_layout(&mut self, name: &Ident, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::UNION_WITHOUT_LAYOUT,
                format!("`{}` needs its size and its alignment", name.name),
            )
            .with_label(Label::primary(span, "written `union Name: size N align M`"))
            .with_note(
                "a C union is imported as an opaque blob of the right size and alignment, \
                 because its layout is the C ABI's and nothing in Science may read its arms",
            )
            .with_note(
                "take both numbers from `sizeof` and `alignof` on the target, or from what \
                 `sciencec bindgen` recorded; the library's own accessors bind as ordinary \
                 `function` items",
            ),
        );
    }

    // --- the FFI type vocabulary -----------------------------------------

    /// The half of §1.3's closed vocabulary that syntax can decide.
    ///
    /// The parser knows two things about a type: what it is written as, and
    /// which names the F0 library already owns. That is enough to reject every
    /// Science layout by name — a `String` is a length and UTF-8 bytes, an
    /// `Array` is a `{ptr, len, cap}` header, an `Option` is a niche — and it
    /// is not enough to *accept* anything: whether `DLTensor` implements
    /// `ffi.CLayout`, and whether every one of its fields transitively does
    /// (`SC0424`), depends on definitions this phase has not seen.
    ///
    /// So the rule is: report what is certainly wrong, stay silent about
    /// everything else, and leave the positive half to the type checker. A
    /// parser that guessed would produce the one kind of FFI diagnostic worse
    /// than none, the false one.
    ///
    /// `by_value` is false under a pointer or a borrow, which is the only
    /// thing the half-precision rule turns on.
    fn check_ffi_type(&mut self, ty: &Type, by_value: bool) {
        match &ty.kind {
            TypeKind::Borrowed { mutable, inner } => {
                if let Some(element) = array_element(inner) {
                    self.report_array_in_signature(ty, Some(*mutable), &element);
                    return;
                }
                self.check_ffi_type(inner, false);
            }
            TypeKind::Path(path) => {
                if let Some(element) = array_element(ty) {
                    self.report_array_in_signature(ty, None, &element);
                    return;
                }
                let name = path.dotted();
                if let Some(why) = science_layout_note(&name) {
                    let what = format!("`{name}`");
                    self.report_not_ffi_representable(ty, &what, why);
                    return;
                }
                if by_value && matches!(name.as_str(), "F16" | "BF16") {
                    self.report_half_precision_by_value(ty, &name);
                    return;
                }
                // The arguments of `ffi.Span[T]`, `ffi.Pointer[T]` and the
                // rest name a pointee, never a value the ABI passes.
                for argument in path.segments.iter().flat_map(|s| s.generics.iter()) {
                    self.check_ffi_type(argument, false);
                }
            }
            TypeKind::Any(bound) => {
                let what = format!("`any {}`", bound.describe());
                self.report_not_ffi_representable(
                    ty,
                    &what,
                    "a dynamically dispatched value is a Science pair of pointers, and C \
                     knows neither half",
                );
            }
            TypeKind::Tuple(_) => self.report_not_ffi_representable(
                ty,
                "a tuple",
                "C has no tuple; declare a `type` marked `implements ffi.CLayout` and pass \
                 a pointer to it",
            ),
            // A closure is a code pointer *and* its captures, which is the
            // same two-pointer shape as `any Trait` and no more representable.
            // A bare C function pointer is a different type and has a spelling
            // of its own: `ffi-c-boundary.md` §1.2's `ffi.FunctionPointer`.
            TypeKind::Closure { .. } => self.report_not_ffi_representable(
                ty,
                "a closure type",
                "a closure carries its captures, and C has nowhere to put them; a plain C \
                 callback is `ffi.FunctionPointer`",
            ),
            // `()` is how a function that returns nothing is written, `Self`
            // cannot occur in a block with no implementation around it, a
            // const argument is a number, and an `Error` has been reported
            // already.
            // `T?` has no C spelling in general. A nullable *pointer* would be
            // exactly C's own convention, but this phase cannot tell a pointer
            // from anything else — `Doc?` and `(ffi.Ptr[Doc])?` are the same
            // shape here — so it rejects the form and leaves the narrower
            // positive case to whoever knows the type.
            TypeKind::Nullable(_) => self.report_not_ffi_representable(
                ty,
                "a nullable type",
                "C has no `T?`; pass a pointer and let the null pointer carry the absence, or return the error separately",
            ),
            TypeKind::Unit
            | TypeKind::SelfType
            | TypeKind::SelfAssoc(_)
            | TypeKind::Const(_)
            | TypeKind::Error => {}
        }
    }

    /// §1.3, and `SC0421` by the name §8 gives it.
    ///
    /// The suggestion is exact and the call site does not move: §1.3 coerces
    /// `&Array[T]` to `ffi.Span[T]` at an extern call site for precisely this
    /// reason, so rewriting the declaration is the whole fix.
    fn report_array_in_signature(&mut self, ty: &Type, mutable: Option<bool>, element: &str) {
        let span = ty.span;
        let replacement = match mutable {
            Some(true) => format!("ffi.MutableSpan[{element}]"),
            _ => format!("ffi.Span[{element}]"),
        };
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::ARRAY_IN_SIGNATURE,
                "`Array[T]` has no C representation",
            )
            .with_label(Label::primary(
                span,
                "C wants the elements; an `Array` is a `{ptr, len, cap}` header",
            ))
            .with_suggestion(Suggestion {
                span,
                replacement,
                message: "declare the parameter as".to_string(),
            })
            .with_note(
                "the length does not cross the ABI — it exists on the Science side so that \
                 the wrapper's bounds check is an expression the compiler checks",
            )
            .with_note(
                "a call site still passes the array: `&Array[T]` coerces to \
                 `ffi.Span[T]` there, and `.span()` names the conversion where it has to \
                 be written out",
            ),
        );
    }

    /// `what` arrives spelled the way it should read in the message, quoted
    /// when it is a name and bare when it is a description: "`String`" and
    /// "a tuple" are both things a sentence can say, and "`a tuple`" is not.
    fn report_not_ffi_representable(&mut self, ty: &Type, what: &str, why: &str) {
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::NOT_FFI_REPRESENTABLE,
                format!("{what} cannot cross the C boundary"),
            )
            .with_label(Label::primary(ty.span, "not in the `ffi` type vocabulary"))
            .with_note(why.to_string()),
        );
    }

    /// §1.3, and `SC0431` by the name §8 gives it.
    fn report_half_precision_by_value(&mut self, ty: &Type, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(
                ffi_codes::HALF_PRECISION_BY_VALUE,
                format!("`{name}` cannot be passed by value across the C boundary"),
            )
            .with_label(Label::primary(ty.span, "half precision has no agreed C ABI by value"))
            .with_note(
                "`_Float16` is passed differently by GCC, Clang and MSVC, and CUDA's \
                 `__half` is a struct around an `unsigned short`",
            )
            .with_note(
                "pass it by reference — `&F16`, or `ffi.Span[F16]` — or as the \
                 `U16` bit pattern the callee reinterprets",
            ),
        );
    }

    // --- generics and bounds ---------------------------------------------

    /// `[T]`, `[T: Ord + Clone]`, `[A, B]`, `[T, const WIDTH: Int]`, or
    /// nothing at all.
    ///
    /// The bracket is its own delimiter, so — unlike the `of` spelling this
    /// replaced — there is no separate bare form to keep straight from the
    /// parenthesised one: `[T]` and `[A, B]` are written the same way whether
    /// there is one parameter or several. §4.3 used to need the parentheses
    /// because a *bare* `of A, B` could not be told from `of A` in front of a
    /// parameter list starting `B(..)`; a closing `]` ends the list on its
    /// own and asks that question of nothing.
    ///
    /// **The old spelling is refused by name.** `of` is still lexable — kept
    /// only so `def foo of T(..)` reports *"`of T` is now written `[T]`"*
    /// rather than *"expected `(`, found `of`"* — and it is kept for exactly
    /// as long as it takes readers to stop writing it, the same trade §4.3
    /// already made for `borrowed T` against `&T`.
    fn parse_generic_params(&mut self) -> Vec<GenericParam> {
        let mut params = Vec::new();

        if self.at(&TokenKind::Of) {
            let start = self.span();
            self.advance();
            self.error(codes::UNEXPECTED_TOKEN, "`of T` is now written `[T]`", start);
            // Recovered with the grammar `of` used to have, so a stray `of`
            // costs this one diagnostic and not a cascade of "undefined
            // parameter" errors through the rest of the declaration.
            if self.eat(&TokenKind::LParen).is_none() {
                if let Some(param) = self.parse_generic_param() {
                    params.push(param);
                }
                return params;
            }
            while !self.at(&TokenKind::RParen) {
                match self.parse_generic_param() {
                    Some(param) => params.push(param),
                    None => self.recover_in_brackets(),
                }
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "`)`");
            return params;
        }

        if self.eat(&TokenKind::LBracket).is_none() {
            return params;
        }
        while !self.at(&TokenKind::RBracket) {
            match self.parse_generic_param() {
                Some(param) => params.push(param),
                None => self.recover_in_brackets(),
            }
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "`]`");
        params
    }

    fn parse_generic_param(&mut self) -> Option<GenericParam> {
        let start = self.span();

        // A const parameter (§5.3) is annotated, and a type parameter's `:`
        // introduces bounds instead. The leading `const` is what tells them
        // apart, which is why it is written even though the annotation alone
        // would be enough to infer it: `[T, WIDTH: Int]` would read as a
        // bound on `WIDTH`.
        if self.eat(&TokenKind::Const).is_some() {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon, "`:`")?;
            let ty = self.parse_type();
            return Some(GenericParam {
                name,
                kind: GenericParamKind::Const { ty },
                span: start.merge(self.last_text_span()),
            });
        }

        let name = self.expect_ident()?;

        // `[T: Ord]` bounds the parameter; the interface head's own two
        // colons (`Doc implements Summarize:` and the `:` of `type
        // Wrapper[T]:` that opens the body) are a different question this
        // function never sees, because the block's colon now always falls
        // *after* the closing `]` and this one is always still inside it.
        // Brackets closed that gap: the `of T:` spelling this replaced had no
        // delimiter of its own, so a *bare* single parameter's bound and a
        // bare single parameter's block-opening colon were one lookahead
        // token apart, and this function used to have to tell them apart by
        // whether a `Newline` followed. A colon can only mean one thing here
        // now.
        let bounds = if self.at(&TokenKind::Colon) {
            self.advance();
            self.parse_bounds(BoundPosition::Constraint)
        } else {
            Vec::new()
        };
        Some(GenericParam {
            name,
            kind: GenericParamKind::Type { bounds },
            span: start.merge(self.last_text_span()),
        })
    }

    /// `A + B + C`. The list ends at anything that is not a `+`, which is what
    /// lets a `where` clause end on the `:` that opens the block and an inline
    /// bound end on the `(` that opens the parameter list.
    fn parse_bounds(&mut self, position: BoundPosition) -> Vec<TypeBound> {
        let mut bounds = Vec::new();
        while let Some(bound) = self.parse_type_bound(position) {
            bounds.push(bound);
            if self.eat(&TokenKind::Plus).is_none() {
                break;
            }
        }
        bounds
    }

    /// One bound: an interface named by path, or — where `position` admits one
    /// — the closure type `(A) -> B`.
    ///
    /// The fork is the same single token [`Self::parse_type`] uses, moved one
    /// production up, and it is made *before* the list rather than after it:
    /// no interface name can begin with `(`, so a `(` here settles the
    /// question with no lookahead at all.
    fn parse_type_bound(&mut self, position: BoundPosition) -> Option<TypeBound> {
        let start = self.span();
        if position == BoundPosition::Constraint && self.at(&TokenKind::LParen) {
            let params = self.parse_paren_type(start);
            return Some(self.closure_bound(params, start));
        }
        let path = self.parse_path()?;
        let span = path.span;
        // `where F: Int -> Bool`. Admitted for the same reason `parse_type`
        // has to admit it: `(T)` collapses with no node, so by the time the
        // arrow is read nothing remembers whether a paren was written, and a
        // grammar that accepted one and refused the other would be describing
        // a distinction the tree cannot hold.
        if position == BoundPosition::Constraint && self.at(&TokenKind::Arrow) {
            let params = Type { kind: TypeKind::Path(path), span };
            return Some(self.closure_bound(params, start));
        }
        Some(TypeBound { kind: TypeBoundKind::Interface(path), span })
    }

    /// The rest of a closure bound, with its parameter list already parsed.
    ///
    /// The `->` is *required* here, where [`Self::parse_type`] merely peeks
    /// for it: a parenthesised type has a second reading when no arrow
    /// follows — it is a tuple — and a parenthesised bound has none.
    fn closure_bound(&mut self, params: Type, start: Span) -> TypeBound {
        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            self.parse_type()
        } else {
            self.report_bound_arrow(params.span);
            // Nothing is skipped. The parameter list is already consumed and
            // the cursor sits on whatever followed the `)`, which is the `+`,
            // `,` or `:` the enclosing list is waiting for — so the refusal
            // costs one diagnostic and the rest of the declaration still
            // parses.
            Type { kind: TypeKind::Error, span: params.span }
        };
        TypeBound {
            kind: TypeBoundKind::Closure {
                params: closure_params(params),
                ret: Box::new(ret),
            },
            span: start.merge(self.last_text_span()),
        }
    }

    /// `SC0119`, the one code `collections-and-chains.md` §1.2 left as a need.
    fn report_bound_arrow(&mut self, params: Span) {
        let found = describe(self.peek());
        self.diagnostics.push(
            Diagnostic::error(
                codes::EXPECTED_BOUND_ARROW,
                format!("expected `->` after a closure type's parameter list, found {found}"),
            )
            .with_label(Label::primary(params, "this is a parameter list, not an interface"))
            .with_note(
                "`collections-and-chains.md` §1.2 spells a closure type `(A) -> B`; a bound \
                 that opens with `(` has no other reading, so the arrow is not optional here",
            ),
        );
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
            let bounds = self.parse_bounds(BoundPosition::Constraint);
            predicates.push(WherePredicate { ty, bounds, span: start.merge(self.last_text_span()) });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        predicates
    }

    // --- paths and types -------------------------------------------------

    /// A dotted name and nothing else.
    ///
    /// Split out from `parse_path` for the one caller that must not read a
    /// `[...]`: an implementation head, where the bracket declares parameters
    /// rather than passing arguments, and `Grid[T, const ROWS: Int] has:`
    /// would be nonsense read as a type — `parse_impl` reads it with
    /// `parse_generic_params` instead, right after this returns.
    fn parse_path_segments(&mut self) -> Option<Path> {
        let first = self.expect_ident()?;
        let mut span = first.span;
        let mut segments = vec![PathSegment { name: first, generics: Vec::new(), span }];

        // A `.` only continues the path when a name follows; otherwise it is
        // field access and belongs to whoever called us.
        while self.at(&TokenKind::Dot) && matches!(self.peek_ahead(1), TokenKind::Ident(_)) {
            self.advance();
            let name = self.expect_ident()?;
            span = span.merge(name.span);
            let segment_span = name.span;
            segments.push(PathSegment { name, generics: Vec::new(), span: segment_span });
        }

        Some(Path { segments, span })
    }

    /// A dotted name, with `[...]` arguments when it has any.
    ///
    /// The arguments bind to the whole name rather than to a segment —
    /// §4.3 writes `text.parser.Token[Doc]`, never `text.Token[Doc].parser`
    /// — so they land on the last segment, which is the one they name.
    ///
    /// **The old spelling is refused by name.** `of` is still lexable — kept
    /// only so `Array of T` reports *"`Array of T` is now written
    /// `Array[T]`"* rather than leaving the `of` to be reported by whatever
    /// reads the token after it, which would not name the type at all. It is
    /// kept for exactly as long as it takes readers to stop writing it.
    fn parse_path(&mut self) -> Option<Path> {
        let mut path = self.parse_path_segments()?;
        if self.at(&TokenKind::LBracket) {
            let generics = self.parse_generic_args();
            let end = self.last_text_span();
            if let Some(last) = path.segments.last_mut() {
                last.generics = generics;
                last.span = last.span.merge(end);
            }
            path.span = path.span.merge(end);
        } else if self.at(&TokenKind::Of) {
            let of_start = self.span();
            self.advance();
            self.error(
                codes::UNEXPECTED_TOKEN,
                format!("`{} of T` is now written `{}[T]`", path.dotted(), path.dotted()),
                of_start,
            );
            let generics = self.parse_of_style_generic_args();
            let end = self.last_text_span();
            if let Some(last) = path.segments.last_mut() {
                last.generics = generics;
                last.span = last.span.merge(end);
            }
            path.span = path.span.merge(end);
        }
        Some(path)
    }

    /// The arguments inside `[...]`, with the `[` not yet consumed.
    ///
    /// Always bracket-delimited: one argument and several are written the
    /// same way, `[T]` and `[String, Int]`, because the bracket is its own
    /// closing delimiter and never needs a second one the way the `of`
    /// spelling needed parentheses to tell `Array of A, B` from `Array of A`
    /// in front of whatever followed.
    /// Whether the brackets at the cursor hold a comma-separated list rather
    /// than a single subscript.
    ///
    /// A scan and not a speculative parse: the question is only whether a
    /// comma appears before the matching `]`, and nesting is tracked so that
    /// `xs[f(a, b)]` and `xs[m[i, j]]` are still one subscript. Cheap, and it
    /// cannot consume tokens the way a backtracking parse would.
    fn brackets_hold_a_list(&self) -> bool {
        let mut depth = 0usize;
        let mut at = 0usize;
        loop {
            match self.peek_ahead(at) {
                TokenKind::LBracket | TokenKind::LParen | TokenKind::LBrace => depth += 1,
                TokenKind::RParen | TokenKind::RBrace => depth = depth.saturating_sub(1),
                TokenKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokenKind::Comma if depth == 1 => return true,
                // **`any` cannot begin an expression**, so brackets holding
                // one hold type arguments. `Array[Box[any Summarize]].new()`
                // has no comma at depth 1 and is still not an index, and this
                // is the one token that says so without guessing.
                TokenKind::Any => return true,
                TokenKind::Eof => return false,
                _ => {}
            }
            at += 1;
        }
    }

    fn parse_generic_args(&mut self) -> Vec<Type> {
        self.advance(); // `[`
        let mut args = Vec::new();
        while !self.at(&TokenKind::RBracket) {
            args.push(self.parse_generic_arg(ArrowAfter::Argument));
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "`]`");
        args
    }

    /// The arguments after `of`, with the `of` already consumed.
    ///
    /// **Kept for the one place `of` still spells generic arguments**: the
    /// associated-call escape valve [`Self::parse_instantiation`] reads,
    /// where `[` is already claimed by indexing and cannot replace it — see
    /// that function's doc comment for the ambiguity this avoids. Everywhere
    /// else, [`Self::parse_generic_args`] reads `[...]` instead.
    ///
    /// One argument may be written bare (`Array of Doc`); two or more take
    /// parentheses, and so may one (§4.3, as it read before `[...]` existed).
    fn parse_of_style_generic_args(&mut self) -> Vec<Type> {
        if self.eat(&TokenKind::LParen).is_none() {
            // The bare form stops before a `->`: see [`Self::parse_type_no_arrow`]
            // for why the two spellings of a one-argument list have to agree
            // about which side of the list an arrow falls on.
            return vec![self.parse_generic_arg(ArrowAfter::Enclosing)];
        }
        let mut args = Vec::new();
        while !self.at(&TokenKind::RParen) {
            args.push(self.parse_generic_arg(ArrowAfter::Argument));
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`");
        args
    }

    /// One argument of a generic argument list, bracketed or (in the
    /// `of`-style escape valve) not: a type, or a const generic argument.
    ///
    /// This is `parse_type` plus exactly one thing — a const generic argument
    /// here may be negated, and only here. `Quantity[T, 1, 0, -1, 0, 0,
    /// 0, 0]` is `scientific-libraries.md` §12.3's velocity, and roughly half
    /// of every SI dimension vector in that note is negative, so this is not
    /// an edge case; before this existed, none of its nine unit aliases
    /// parsed.
    ///
    /// The `-` is recognised here rather than at the head of `parse_type`
    /// because a negative number is not a type. `def f(x: -1)` stays the
    /// error it has always been: an argument list is the one position that
    /// holds values beside types (§5.3), and it is the only position where a
    /// leading `-` has a value to belong to.
    ///
    /// Only an **integer** literal may follow the `-`. A negative float,
    /// string, character or boolean const argument is not a thing:
    /// `const-expression-arithmetic.md` §2.3 admits the const-parameter kinds
    /// `Int` and (in F1) `Shape` and nothing else, so there is no kind in
    /// which negating one of those is a phrase. Rather than grow a second
    /// rejection path that could drift, those fall through to `parse_type`,
    /// which reports the `-` as "expected a type" exactly as it does for
    /// every other token that cannot start one.
    ///
    /// **`- 1`, with a space, parses, and means `-1`.** The lexer's rule that
    /// `->` is one token only when its two characters touch does not reach
    /// here and should not: that rule is about a *lexeme* that two characters
    /// spell, whereas this is a parser production over two tokens —
    /// `const-expression-arithmetic.md` §2.1 writes `ConstAtom := '-'
    /// ConstAtom`, whose operand grows to `-(N + 1)`, where there is no pair
    /// of characters for adjacency to be measured between. `- 1` is already
    /// legal unary negation in expression position, and making whitespace
    /// significant in one of the two positions and not the other would be a
    /// rule with exactly one instance in the language.
    fn parse_generic_arg(&mut self, arrow: ArrowAfter) -> Type {
        let start = self.span();
        if self.at(&TokenKind::Minus) && matches!(self.peek_ahead(1), TokenKind::Int { .. }) {
            let minus = self.advance().span;
            let literal = literal_of(self.peek())
                .expect("the token after the `-` was just matched as an integer literal");
            let operand_span = self.advance().span;
            let span = minus.merge(operand_span);
            let operand = ConstExpr { kind: ConstExprKind::Lit(literal), span: operand_span };
            let atom = ConstExpr { kind: ConstExprKind::Neg(Box::new(operand)), span };
            return self.const_expr_from(atom, start);
        }
        let ty = match arrow {
            ArrowAfter::Argument => self.parse_type(),
            ArrowAfter::Enclosing => self.parse_type_no_arrow(),
        };
        // A type followed by a const operator was never a type.
        //
        // The parser cannot tell `N` the type parameter from `N` the const
        // parameter — that is a question about scopes and resolution answers
        // it — so an argument is parsed as a type and *promoted* the moment
        // an operator proves it was arithmetic. This costs no backtracking
        // and no second grammar: `4`, `N` and `N + 1` each take the one path
        // that fits them.
        if self.at_const_operator() {
            if let Some(atom) = const_atom_of(&ty) {
                return self.const_expr_from(atom, start);
            }
        }
        ty
    }

    /// Whether the cursor is on an operator of §2.1's five.
    fn at_const_operator(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash
        )
    }

    /// The rest of §2.1's grammar, once the first atom is in hand.
    ///
    /// `+` and `-` are left-associative and bind loosest; `*` and `/` bind
    /// tighter and **require a literal** on the operator's right, which is
    /// the production that makes the whole normaliser linear. `k * e` with
    /// the literal on the left is the same node with the operands read the
    /// other way round.
    fn const_expr_from(&mut self, first: ConstExpr, start: Span) -> Type {
        let mut lhs = self.const_term_from(first);
        loop {
            let add = if self.eat(&TokenKind::Plus).is_some() {
                true
            } else if self.at(&TokenKind::Minus) {
                self.advance();
                false
            } else {
                break;
            };
            let Some(rhs_atom) = self.parse_const_atom() else { break };
            let rhs = self.const_term_from(rhs_atom);
            let span = start.merge(self.last_text_span());
            let kind = if add {
                ConstExprKind::Add(Box::new(lhs), Box::new(rhs))
            } else {
                ConstExprKind::Sub(Box::new(lhs), Box::new(rhs))
            };
            lhs = ConstExpr { kind, span };
        }
        let span = start.merge(self.last_text_span());
        Type { kind: TypeKind::Const(lhs), span }
    }

    /// `*` and `/` runs over one atom. The right operand must be a literal.
    fn const_term_from(&mut self, first: ConstExpr) -> ConstExpr {
        let start_of_term = first.span;
        let mut term = first;
        loop {
            let times = if self.at(&TokenKind::Star) {
                true
            } else if self.at(&TokenKind::Slash) {
                false
            } else {
                break;
            };
            let op = self.advance().span;
            // §2.1 has **two** multiplication productions — `ConstTerm '*'
            // IntLiteral` and `IntLiteral '*' ConstTerm` — so `2 * N` is as
            // legal as `N * 2`. Only the first was implemented, which rejected
            // a legal program with a message telling its author to do the
            // thing they had done. Division has one production and keeps the
            // literal on the right, which is why the two are not symmetric.
            let left_literal = const_literal_of(&term);
            let kind = match (literal_of(self.peek()), left_literal, times) {
                // `e * k` and `e / k`.
                (Some(literal), _, _) => {
                    let literal_span = self.advance().span;
                    let operand = Box::new(term);
                    if times {
                        ConstExprKind::Mul { operand, factor: literal, factor_span: literal_span }
                    } else {
                        ConstExprKind::Div {
                            operand,
                            divisor: literal,
                            divisor_span: literal_span,
                        }
                    }
                }
                // `k * e`, the mirrored production. The node is the same one:
                // multiplication is commutative and the normaliser has one
                // shape to handle rather than two.
                (None, Some((literal, literal_span)), true) => {
                    let Some(operand) = self.parse_const_atom() else {
                        self.report_const_factor(op, true);
                        self.skip_const_operand();
                        break;
                    };
                    ConstExprKind::Mul {
                        operand: Box::new(operand),
                        factor: literal,
                        factor_span: literal_span,
                    }
                }
                // Neither side is a literal, or it is `k / e`, which §2.1
                // has no production for.
                _ => {
                    self.report_const_factor(op, times);
                    // Step over what stood where the literal should have been,
                    // so the argument list closes and the reader gets one
                    // error rather than one plus the three that follow from
                    // the parser still standing on a name it cannot use.
                    self.skip_const_operand();
                    break;
                }
            };
            let span = start_of_term.merge(self.last_text_span());
            term = ConstExpr { kind, span };
        }
        term
    }

    /// `IntLiteral`, a parameter name, `-atom`, or `( ConstExpr )`.
    fn parse_const_atom(&mut self) -> Option<ConstExpr> {
        let start = self.span();
        if self.at(&TokenKind::Minus) {
            self.advance();
            let operand = self.parse_const_atom()?;
            let span = start.merge(operand.span);
            return Some(ConstExpr { kind: ConstExprKind::Neg(Box::new(operand)), span });
        }
        if self.eat(&TokenKind::LParen).is_some() {
            let inner = self.parse_const_atom()?;
            let ty = self.const_expr_from(inner, start);
            self.expect(&TokenKind::RParen, "`)`");
            let TypeKind::Const(value) = ty.kind else { return None };
            return Some(value);
        }
        if let Some(literal) = literal_of(self.peek()) {
            let span = self.advance().span;
            return Some(ConstExpr { kind: ConstExprKind::Lit(literal), span });
        }
        let name = self.expect_ident()?;
        let span = name.span;
        Some(ConstExpr { kind: ConstExprKind::Param(name), span })
    }

    /// Consumes the rest of one const argument after a refusal.
    ///
    /// Stops at the comma or the bracket that ends the argument, and counts
    /// nesting so that an inner `(` does not end an outer argument. This is
    /// the repository's no-cascade rule applied to a position that had no
    /// recovery before, because before this it had no way to fail.
    fn skip_const_operand(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                TokenKind::Eof | TokenKind::Newline => return,
                TokenKind::Comma if depth == 0 => return,
                TokenKind::RParen | TokenKind::RBracket if depth == 0 => return,
                TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket => depth -= 1,
                _ => {}
            }
            self.advance();
        }
    }

    /// §2.1's central refusal, reported rather than parsed around.
    fn report_const_factor(&mut self, op: Span, times: bool) {
        // The two operators are not symmetric and the message must not
        // pretend they are: §2.1 admits a literal on *either* side of `*` and
        // only on the right of `/`. Saying "on one side" for division told a
        // reader that `2 / N` was a spelling problem when it is a grammar
        // one, and saying "multiplies" for `/` was simply the wrong word.
        let (what, where_) = if times {
            ("multiplies only by a literal", "one side of this needs an integer literal")
        } else {
            ("divides only by a literal", "the right of this needs an integer literal")
        };
        self.diagnostics.push(
            Diagnostic::error(codes::CONST_FACTOR, format!("a const expression {what}"))
                .with_label(Label::primary(op, where_))
                .with_note("`const-expression-arithmetic.md` §2.1 keeps const arithmetic linear by grammar: two parameters may be added or subtracted, never multiplied or divided by each other"),
        );
    }

    /// A type expression: `&T`, `&mut T`, `any Trait`,
    /// `Array[T]`, `(A, B)`, `()`, `Self`, `Self.Item`, a const argument,
    /// and dotted paths.
    ///
    /// Always returns a node. A failure becomes `TypeKind::Error`, so the tree
    /// keeps its shape and the phases after this one still have something to
    /// walk.
    /// A type, with any `?` suffixes applied.
    ///
    /// `T?` is revision 2 §3.1's nullable type. The suffix is a loop rather
    /// than a single test so that `T??` produces one node per `?` and is
    /// rejected by the phase that can say *why* — the parser refusing it
    /// here would have to explain a type system it cannot see.
    ///
    /// The recursive calls inside [`Self::parse_type_atom`] come back through
    /// this function, so `&T?` is `&(T?)`: the suffix binds to
    /// the type it follows, not to the whole construction.
    /// `(A) -> B` is read by **one token of lookahead past the closing paren,
    /// with no backtracking.**
    ///
    /// [`Self::parse_paren_type`] already parses `()`, `(T)` and `(A, B)` to
    /// completion, and a closure's parameter list has the identical inner
    /// grammar — a comma-separated list of types — so the same parse serves
    /// both readings and only the *reduction* differs. After it returns, one
    /// peek decides: `Arrow` means the thing just parsed was a parameter list,
    /// anything else means it is what it has always been.
    ///
    /// `-> (Config, Error?)`, the error model's return shape on every fallible
    /// function in the language, is settled by that peek: what follows a
    /// return type is `where` or `:` and never `->`, so no existing signature
    /// changes meaning.
    ///
    /// The return type is a recursive call, which buys both of §1.2's stated
    /// associativity rules for nothing: `->` comes out right-associative, so
    /// `(A) -> (B) -> C` is `(A) -> ((B) -> C)`; and the `?` loop above runs
    /// *before* the arrow is read and again inside the recursion, so `?` binds
    /// tighter and `(A) -> B?` is `(A) -> (B?)`.
    fn parse_type(&mut self) -> Type {
        let start = self.span();
        let ty = self.parse_type_no_arrow();
        if self.eat(&TokenKind::Arrow).is_some() {
            let ret = self.parse_type();
            return Type {
                kind: TypeKind::Closure {
                    params: closure_params(ty),
                    ret: Box::new(ret),
                },
                span: start.merge(self.last_text_span()),
            };
        }
        ty
    }

    /// A type up to but not including a `->`, which is left for the caller.
    ///
    /// [`Self::parse_type`] calls this for its own, general reason — a `->`
    /// is right-associative and read by recursing on the return type, so the
    /// left side of one has to stop before it — but it has a second caller
    /// left over from before `[...]` existed: `parse_generic_arg`'s
    /// `ArrowAfter::Enclosing`, for the **bare** single argument of the
    /// `of`-style escape valve, `Array of T`. §4.3 said one argument could be
    /// written with parentheses or without, so `Array of T` and `Array of
    /// (T)` were the same type — but in `Array of (T) -> B` the parens close
    /// the *argument list*, so the arrow is outside it and the closure takes
    /// the array. If the bare form parsed its argument greedily the same
    /// source with two parens removed would be an `Array` of closures
    /// instead, and two spellings §4.3 called equal would name different
    /// types. So the arrow fell outside in both, which is also the
    /// ML-family reading `collections-and-chains.md` §1.2 appeals to:
    /// application binds tighter than the arrow. A closure *as* a generic
    /// argument wrote the parens it needed — `Array of ((Int) -> Bool)` —
    /// exactly as it does in every other position where two readings meet.
    ///
    /// `[...]` has no bare form and needs none of this: `Array[T]`'s `]`
    /// closes the argument list on its own, so `parse_generic_args` always
    /// passes `ArrowAfter::Argument` and a closure argument's own parens are
    /// the only parens `Array[(Int) -> Bool]` needs.
    fn parse_type_no_arrow(&mut self) -> Type {
        let start = self.span();
        let mut ty = self.parse_type_atom();
        while self.at(&TokenKind::Question) {
            self.advance();
            ty = Type {
                kind: TypeKind::Nullable(Box::new(ty)),
                span: start.merge(self.last_text_span()),
            };
        }
        ty
    }

    fn parse_type_atom(&mut self) -> Type {
        let start = self.span();

        // A const generic argument (§5.3) is a value where a type is expected:
        // the `4` of `Window[Int, 4]`. Nothing else in a type position can
        // be a literal, so there is no ambiguity to resolve.
        //
        // `null` is the exception, and it is excluded rather than accepted and
        // rejected later: it is an inhabitant of a type, never an argument to
        // one, and letting it build a `Const` node would put a value nobody
        // can evaluate into the one position that must stay evaluable.
        if let Some(literal) = literal_of(self.peek()).filter(|_| !self.at(&TokenKind::Null)) {
            self.advance();
            let value = ConstExpr { kind: ConstExprKind::Lit(literal), span: start };
            return Type { kind: TypeKind::Const(value), span: start };
        }

        match self.peek() {
            // `&T` and `&mut T`. **The sigil replaced the word**, and the one
            // thing it does not replace is `mutable` on a binding: `let mutable
            // total be 0` is unchanged, because a binding says what it *is*
            // where a borrow says what it *grants*.
            TokenKind::Amp => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut).is_some();
                let inner = self.parse_type();
                Type {
                    kind: TypeKind::Borrowed { mutable, inner: Box::new(inner) },
                    span: start.merge(self.last_text_span()),
                }
            }
            // **The old spelling is refused by name rather than by accident.**
            // `borrowed` is still a keyword, so `borrowed Int` would otherwise
            // fail as *"expected a type, found `borrowed`"* — true and useless.
            // The word is kept lexable for exactly as long as it takes readers
            // to stop writing it.
            TokenKind::Borrowed | TokenKind::Mutable
                if matches!(self.peek(), TokenKind::Borrowed)
                    || self.peek_ahead(1) == &TokenKind::Borrowed =>
            {
                let mutable = self.at(&TokenKind::Mutable);
                self.advance();
                if mutable {
                    self.advance();
                }
                self.error(
                    codes::UNEXPECTED_TOKEN,
                    if mutable {
                        "`mutable borrowed T` is now written `&mut T`"
                    } else {
                        "`borrowed T` is now written `&T`"
                    },
                    start,
                );
                let inner = self.parse_type();
                Type {
                    kind: TypeKind::Borrowed { mutable, inner: Box::new(inner) },
                    span: start.merge(self.last_text_span()),
                }
            }
            TokenKind::Any => {
                self.advance();
                match self.parse_type_bound(BoundPosition::Interface) {
                    Some(bound) => Type {
                        kind: TypeKind::Any(bound),
                        span: start.merge(self.last_text_span()),
                    },
                    None => Type { kind: TypeKind::Error, span: start },
                }
            }
            TokenKind::SelfType => {
                self.advance();
                // `Self.Item` names an associated type (§5.4); `Self` alone is
                // the implementing type.
                if self.at(&TokenKind::Dot) && matches!(self.peek_ahead(1), TokenKind::Ident(_)) {
                    self.advance();
                    return match self.expect_ident() {
                        Some(name) => Type {
                            kind: TypeKind::SelfAssoc(name),
                            span: start.merge(self.last_text_span()),
                        },
                        None => Type { kind: TypeKind::Error, span: start },
                    };
                }
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
                | TokenKind::Be
                | TokenKind::Arrow
                | TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::Eof
        )
    }

    // --- blocks ----------------------------------------------------------

    /// The indented body of a declaration whose contents are one item per
    /// line: a record's fields, a choice's variants, an interface's members.
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

    /// A block of statements, in either of §4.5's two forms:
    ///
    /// - indented: `:` NEWLINE INDENT statements DEDENT
    /// - inline: `:` followed by a single expression
    ///
    /// The inline form ends where the *expression* ends, not at the end of the
    /// line. That is the whole of §4.5's rule, and the reason `if c: a else: b`
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
    /// §4.5 makes a statement here an error and gives `let` as the example,
    /// which is what `SC0109` reports.
    ///
    /// Assignment, `return`, `break` and `continue` are still accepted, and the
    /// spec writes all four inline itself: `if item > best: best be item` in
    /// §4.4, and `if n < 0: return -1` and `loop: break` in the corpus. Each of
    /// them is an expression of type `Never` (§4.7) in every respect except
    /// where the AST happens to file it — `StmtKind` rather than `ExprKind` —
    /// and a filing decision is not what §4.5 is ruling on. A `let` has no such
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

    /// The body of a `match` arm, which §4.5 makes an expression rather than a
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
                let mutable = self.eat(&TokenKind::Mutable).is_some();
                // One name, or several separated by commas. `mutable` is read
                // once and applies to all of them: the list receives one
                // tuple, and a binding list where half the names are mutable
                // would need a second `mutable` in a position nothing else in
                // the language puts one.
                let mut names = Vec::new();
                loop {
                    let name_start = self.span();
                    let name = self.expect_ident()?;
                    let ty = if self.eat(&TokenKind::Colon).is_some() {
                        Some(self.parse_type())
                    } else {
                        None
                    };
                    let span = name_start.merge(self.last_text_span());
                    names.push(LetName { name, ty, span });
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                // F0 has no `let` without an initialiser: `LetStmt::value` is
                // not an `Option`, and inference is local (§5.2), so a
                // binding with no value has nothing to infer from.
                self.expect(&TokenKind::Be, "`be`")?;
                let value = self.parse_expr();
                let span = start.merge(self.last_text_span());
                Some(Stmt { kind: StmtKind::Let(LetStmt { mutable, names, value, span }), span })
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
            // `assert(cond)` / `assert(cond, message)`. Spelled like a call
            // and parsed like one, but it is a statement (`TokenKind::Assert`'s
            // decision): nothing here reaches name resolution, so an
            // `assert` with the wrong arity is a parse error and not a call
            // that failed to resolve.
            TokenKind::Assert => {
                self.advance();
                self.expect(&TokenKind::LParen, "`(` after `assert`")?;
                let cond = self.parse_expr();
                let message =
                    if self.eat(&TokenKind::Comma).is_some() { Some(self.parse_expr()) } else { None };
                self.expect(&TokenKind::RParen, "`)` to close `assert`")?;
                let span = start.merge(self.last_text_span());
                Some(Stmt { kind: StmtKind::Assert { cond, message }, span })
            }
            // A `tool` inside a body is `SC0197`. Nothing is consumed: the
            // caller synchronises on `None`, which drops the declaration and
            // the block under it together.
            TokenKind::Tool => {
                self.report_tool_out_of_place();
                None
            }
            _ => {
                let expr = self.parse_expr();
                // `=` is not an operator in Science and not assignment either
                // (§4.3): it survives in the lexer only as the tail of `==`,
                // `<=` and `..=`. After an expression it can only be a missed
                // `be`, and saying so beats "expected end of line", which
                // names the symptom rather than the mistake. Recovering as an
                // assignment keeps the tree the programmer meant.
                if self.at(&TokenKind::Eq) {
                    let span = self.span();
                    self.diagnostics.push(
                        Diagnostic::error(codes::UNEXPECTED_TOKEN, "assignment is written `be`")
                            .with_label(Label::primary(span, "`=` does not assign in Science"))
                            .with_suggestion(Suggestion {
                                span,
                                replacement: "be".to_string(),
                                message: "assign with".to_string(),
                            }),
                    );
                    self.advance();
                    let value = self.parse_expr();
                    let span = start.merge(self.last_text_span());
                    return Some(Stmt { kind: StmtKind::Assign { target: expr, value }, span });
                }
                // `place be value` (§4.3): the same word that binds a name is
                // what assigns to one that already exists.
                if self.eat(&TokenKind::Be).is_some() {
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
            TokenKind::Let
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Assert
        ) || self.at_expr_start()
    }

    /// Whether the current token can begin an expression.
    ///
    /// Used where an expression is optional — after `return` and `break` — and
    /// to tell an empty inline body from a real one.
    fn at_expr_start(&self) -> bool {
        self.starts_expr(self.peek())
    }

    /// Whether `kind` can begin an expression.
    ///
    /// Taken as an argument rather than read off the cursor because the
    /// migration diagnostics look one token ahead: `while` is an ordinary
    /// identifier now, and what tells `while n > 0:` from a variable named
    /// `while` is whether an expression follows it.
    ///
    /// # `[` is in this list, and it moves six other decisions
    ///
    /// **The decision** is that `[` goes in here with no exception anywhere,
    /// because this predicate answers a question about the grammar — *can an
    /// expression begin with this token* — and after
    /// `indexing-and-array-literals.md` §3.1 the answer is yes. An exception
    /// in one caller would make the predicate say something false in order to
    /// make one caller say something true, and it would say it in the last
    /// place a reader of that caller would look.
    ///
    /// Six callers read it, and each was checked rather than assumed:
    ///
    /// * [`Self::at_expr_start`] decides whether `return` and `break` carry a
    ///   value. `return [1, 2]` must return the array, and before this change
    ///   it returned nothing and then failed on the `[`. **Right, and it is
    ///   half the bug this change exists to fix.**
    /// * [`Self::at_stmt_start`] decides whether `:` is followed by an inline
    ///   body. `if ready: return [0]` now has one. **Right**, and a bare
    ///   `if c: [1, 2]` is an inline body whose value is an array, which is
    ///   useless and not ill-formed — the same standing `if c: 1` has.
    /// * [`Self::at_top_level_statement`] decides whether a top-level line is
    ///   a statement (`script-mode.md` §8.2). Nothing in the language
    ///   *declares* with a `[`, so the arm this reaches is the one for "a
    ///   statement exactly when it could be one". **Right**, and it is what
    ///   makes `print([1, 2])` legal in a script.
    /// * [`Self::at_while_word`] and [`Self::at_try_word`] tell `while n > 0:`
    ///   and `try f()` — the forms revisions 2 and 3 removed — from ordinary
    ///   variables called `while` and `try`. Adding `[` means `while[0]`,
    ///   which is an index into a variable named `while`, now reports
    ///   `SC0142` instead. **This is a real change and it is accepted**, for
    ///   two reasons. `LParen` has had exactly this property since the
    ///   migration codes were written — `while(0)` is already read as the
    ///   stale loop — so the behaviour is consistent rather than newly
    ///   surprising; and the traffic runs one way, because no pre-revision
    ///   program can have a `while` whose condition begins with `[`. Array
    ///   literals did not exist in the language until this commit, so the
    ///   reading that is lost is one nobody has written and the reading that
    ///   is gained covers every file the migration is for.
    /// * [`Self::stale_comparison_phrase`] is the same shape one rung up:
    ///   `n is above [0]` was a comparison against element zero of an array
    ///   named `above`, and is now `SC0143` offering `n > [0]`. **Accepted for
    ///   the same reason**, and with the same `LParen` precedent —
    ///   `n is above(0)` already reports the phrase.
    ///
    /// **The cost**, stated once: a program that names a variable `while`,
    /// `try`, `above`, `below`, `least` or `most` and then indexes it gets a
    /// migration diagnostic instead of an index. There is no fix for that
    /// which is not a lookahead for the `:` that a block header ends with, and
    /// a lookahead that scans to the end of a line to decide what its first
    /// word meant is the thing §4.6 refuses everywhere else.
    fn starts_expr(&self, kind: &TokenKind) -> bool {
        use TokenKind::*;
        matches!(
            kind,
            Int { .. }
                | Float { .. }
                | Str(_)
                | FStrStart
                | Char(_)
                | True
                | False
                | Ident(_)
                | SelfValue
                | SelfType
                | LParen
                | LBracket
                | Minus
                | Not
                | Null
                // `&x` and `&mut x`. **`&` can begin an expression and can
                // also continue one**, which is the whole of what makes the
                // sigil work: this predicate is asked where a *term* is
                // expected, and `BitAnd`'s row is consulted where an operator
                // is. `return &found.inner` is the case that found it — the
                // token list said `return` carried no value, and the `&` was
                // then reported as an unexpected end of line.
                | Amp
                | Borrowed
                | Mutable
                | Each
                | If
                | Match
                | Loop
                | For
                | Unsafe
        )
    }

    // --- expressions -----------------------------------------------------

    /// An expression, by precedence climbing over §4.6's table.
    ///
    /// Always returns a node: a failure becomes `ExprKind::Error`, so the tree
    /// keeps its shape and the phases after this one still have something to
    /// walk. Assignment is not in the table; see the note at the top of this
    /// module.
    ///
    /// The named closure form is read here, above everything else, because
    /// `doc giving doc.title.length()` gives its whole right-hand side to the
    /// closure: there is no operator `giving` could be an operand of.
    fn parse_expr(&mut self) -> Expr {
        if matches!(self.peek(), TokenKind::Ident(_))
            && matches!(self.peek_ahead(1), TokenKind::Giving)
        {
            let start = self.span();
            let param = self.expect_ident();
            self.advance(); // `giving`
            let body = self.parse_expr();
            let span = start.merge(self.last_text_span());
            return Expr {
                kind: ExprKind::Closure { param, body: Box::new(body) },
                span,
            };
        }
        self.parse_range()
    }

    /// `a..b` and `a..=b` (§4.5).
    ///
    /// §4.6's table does not place `..`, so it is placed here: looser than
    /// every operator, which makes `0..n - 1` count to `n - 1` rather than
    /// subtracting from a range. Both ends are required. An open end would
    /// have to be told from a `..` that ends a line, and F0 has no method that
    /// takes one — §8 gives `Array` no slicing.
    fn parse_range(&mut self) -> Expr {
        let start = self.span();
        let lhs = self.parse_binary(1);
        let inclusive = match self.peek() {
            TokenKind::DotDot => false,
            TokenKind::DotDotEq => true,
            _ => return lhs,
        };
        self.advance();
        let end = self.parse_binary(1);
        let span = start.merge(self.last_text_span());
        Expr {
            kind: ExprKind::Range { start: Box::new(lhs), end: Box::new(end), inclusive },
            span,
        }
    }

    /// One rung of the table and everything above it.
    ///
    /// Every row is left-associative but `**`, which is why the recursive call
    /// usually asks for `prec + 1`: a second operator of the same row cannot be
    /// absorbed by the right-hand side and is left for this loop. `**` asks for
    /// `prec`, so `2 ** 3 ** 2` is `2 ** (3 ** 2)` as §4.6 requires.
    fn parse_binary(&mut self, min_prec: u8) -> Expr {
        let start = self.span();
        let mut lhs = self.parse_cast();
        while let Some((op, prec, width)) = self.peek_binary_op() {
            if prec < min_prec {
                break;
            }
            // A pre-revision comparison phrase is reported here rather than
            // where it is recognised. `peek_binary_op` runs once per rung of
            // the climb and the rung that gives up on a comparison gives up
            // *after* the look, so reporting there would fire the diagnostic
            // again for every rung above the one that takes the operator.
            // Here is past the only `break`, so it fires exactly once.
            if let Some((symbol, phrase_width)) = self.stale_comparison_phrase() {
                self.report_comparison_phrase(symbol, phrase_width);
            }
            for _ in 0..width {
                self.advance();
            }
            let next = if op == BinaryOp::Pow { prec } else { prec + 1 };
            let rhs = self.parse_binary(next);
            let span = start.merge(self.last_text_span());
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                span,
            };
        }
        lhs
    }

    /// The operator at the cursor, its rung, and how many tokens it is written
    /// with.
    ///
    /// The width survives although revision 2 §1 deleted the comparison
    /// phrases, because `is not` is two words. It is the only operator that
    /// is, and the pre-revision phrases are recognised here only to report
    /// them.
    /// Side-effect free, because the climb looks at every rung: see the note
    /// in [`Self::parse_binary`] on where a stale phrase is reported.
    fn peek_binary_op(&self) -> Option<(BinaryOp, u8, usize)> {
        if !self.at(&TokenKind::Is) {
            let (op, prec) = binary_op(self.peek())?;
            return Some((op, prec, 1));
        }
        let (symbol, width) = match self.stale_comparison_phrase() {
            Some(phrase) => phrase,
            // `is` is equality and `is not` is inequality. Nothing else
            // follows `is`.
            None if self.at_ahead(1, &TokenKind::Not) => (TokenKind::NotEq, 2),
            None => (TokenKind::EqEq, 1),
        };
        let (op, prec) = binary_op(&symbol)?;
        Some((op, prec, width))
    }

    /// The symbol a pre-revision comparison phrase stood for, and how many
    /// words it took, when the cursor is on `is`.
    ///
    /// `at`, `above`, `below`, `most` and `least` are ordinary identifiers
    /// now, so a phrase is recognised by spelling rather than by token. The
    /// phrase must be followed by something that starts an expression, which
    /// is what keeps `n is above` — equality against a variable named
    /// `above` — out of the net.
    fn stale_comparison_phrase(&self) -> Option<(TokenKind, usize)> {
        if self.word_at(1, "at") && self.starts_expr(self.peek_ahead(3)) {
            if self.word_at(2, "least") {
                return Some((TokenKind::GtEq, 3));
            }
            if self.word_at(2, "most") {
                return Some((TokenKind::LtEq, 3));
            }
        }
        if self.starts_expr(self.peek_ahead(2)) {
            if self.word_at(1, "above") {
                return Some((TokenKind::Gt, 2));
            }
            if self.word_at(1, "below") {
                return Some((TokenKind::Lt, 2));
            }
        }
        None
    }

    /// Reports a pre-revision comparison phrase, naming the symbol §1 put in
    /// its place.
    ///
    /// Recovery is that symbol: the caller reads the phrase with the table the
    /// symbol uses, so the expression keeps the shape the programmer meant and
    /// nothing after the parser sees the difference.
    fn report_comparison_phrase(&mut self, symbol: TokenKind, width: usize) {
        let span = self.span().merge(self.token_at(width - 1).span);
        let words: Vec<String> =
            (0..width).map(|i| describe_word(self.peek_ahead(i))).collect();
        let phrase = words.join(" ");
        let replacement = fixed_text(&symbol).to_string();
        self.diagnostics.push(
            Diagnostic::error(
                codes::COMPARISON_PHRASE,
                format!("the comparison is written `{replacement}`"),
            )
            .with_label(Label::primary(span, format!("`{phrase}` is not an operator in Science")))
            .with_suggestion(Suggestion {
                span,
                replacement,
                message: "compare with".to_string(),
            })
            .with_note("`is` and `is not` are the only comparisons written as words"),
        );
    }

    /// Whether the token `offset` ahead is `kind`.
    fn at_ahead(&self, offset: usize, kind: &TokenKind) -> bool {
        self.peek_ahead(offset) == kind
    }

    /// `as`, which sits between the unary operators and `**`.
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

    /// `-`, `not`, `borrowed` and `mutable borrowed`, all of them prefix.
    ///
    /// Auto-borrow (§6.3) means a call site rarely writes a borrow at all; the
    /// forms stay for the places where writing it clarifies, and for storing
    /// one in a record field.
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
            // `&x` and `&mut x`. **Prefix only, which is what keeps it apart
            // from `a & b`.** `&` is also `BitAnd`, and the two never collide:
            // a borrow is only ever written where a *term* is expected and an
            // infix `&` only ever where an operator is, so the same token is
            // read by two rows of the grammar that cannot both be looking.
            TokenKind::Amp => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut).is_some();
                let inner = self.parse_unary();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Borrowed { mutable, expr: Box::new(inner) }, span }
            }
            TokenKind::Borrowed => {
                self.advance();
                self.error(codes::UNEXPECTED_TOKEN, "`borrowed x` is now written `&x`", start);
                let inner = self.parse_unary();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Borrowed { mutable: false, expr: Box::new(inner) }, span }
            }
            TokenKind::Mutable if self.peek_ahead(1) == &TokenKind::Borrowed => {
                self.advance();
                self.advance();
                self.error(
                    codes::UNEXPECTED_TOKEN,
                    "`mutable borrowed x` is now written `&mut x`",
                    start,
                );
                let inner = self.parse_unary();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Borrowed { mutable: true, expr: Box::new(inner) }, span }
            }
            _ => self.parse_postfix(),
        }
    }

    /// The tightest row: call, index, field access and `try`, applied left to
    /// right to whatever precedes them.
    ///
    /// `?` is postfix and sits on this row with call, index and field access,
    /// so `f().x?` is `(f().x)?`. It replaced the prefix `try`, which covered
    /// the whole chain that *followed* it; the two read in opposite
    /// directions, which is why this is a rewrite of the row and not a rename.
    fn parse_postfix(&mut self) -> Expr {
        let start = self.span();
        let mut expr = self.parse_primary();

        // The block this primary closed was the end of its line. See
        // `just_closed_an_indented_block`.
        if self.just_closed_an_indented_block() {
            return expr;
        }

        loop {
            match self.peek() {
                // `e?`. Chaining it is accepted here and rejected later:
                // `err??` is a `Bool` tested for presence, which is never
                // what anyone means, but saying so needs the type and this
                // phase does not have it.
                TokenKind::Question => {
                    self.advance();
                    let span = expr.span.merge(self.last_text_span());
                    expr = Expr { kind: ExprKind::Present(Box::new(expr)), span };
                }
                TokenKind::Dot => {
                    self.advance();
                    let Some(name) = self.expect_ident() else {
                        return error_expr(start.merge(self.last_text_span()));
                    };
                    expr = if self.at(&TokenKind::LParen) {
                        // Named arguments construct a record, and only a path
                        // can name one. `docs.sort(by: f)` is written exactly
                        // like `text.Doc(title: "a")` and parses the same way;
                        // resolution tells them apart, because only it knows
                        // whether `docs` is a module.
                        if self.at_named_args() && matches!(expr.kind, ExprKind::Path(_)) {
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
                // §6.2: `[` in *postfix* position — immediately after a
                // complete primary — is an index, and binds with call and
                // field access so that `a[i].f[j]` is `((a[i]).f)[j]`.
                //
                // This is also why the `Of` arm below is still `Of` and not
                // `LBracket`: `Array[Int]` and `xs[i]` are the same tokens,
                // and this match already commits to the index reading before
                // `instantiable_path` could be asked. See
                // `Self::parse_instantiation`'s doc comment for the full
                // argument.
                // **Two or more arguments settle themselves; one cannot.**
                //
                // The decision. `Map[String, Int].new()` is read here as a
                // generic instantiation, because an index takes exactly one
                // subscript and a comma at this depth means the brackets are
                // not one. `Array[Int].new()` is left as an
                // `ExprKind::Index` and `science-resolve` decides, because
                // deciding it here is not possible.
                //
                // The reason. `Array[Int]` and `xs[i]` are the same tokens:
                // a path, a bracket, a path, a bracket. Nothing in the token
                // stream separates a type applied to an argument from a value
                // subscripted by a name — the difference is what `Array` *is*,
                // and a parser does not know. A rule of thumb would work:
                // types are `UpperCamel` in every line of this corpus. It is
                // still a rule of thumb, and making the grammar depend on
                // capitalisation is a language change smuggled in as a parsing
                // convenience.
                //
                // So the split is by where the information lives. The comma is
                // a fact about the tokens and is decided here; *"is `Array` a
                // type"* is a fact about the program's names and is decided by
                // the phase that has them.
                //
                // The cost. An `ExprKind::Index` in the tree no longer always
                // means an index, and a reader of the AST has to know that
                // `science-resolve` may turn one into a path. That is the same
                // shape `ExprKind::Field` already has, two arms up: a field
                // access whose base is a module is a path, and this arm's
                // neighbour already rewrites it.
                TokenKind::LBracket if instantiable_path(&expr) && self.brackets_hold_a_list() => {
                    let args = self.parse_generic_args();
                    let end = self.last_text_span();
                    if let ExprKind::Path(path) = &mut expr.kind {
                        if let Some(segment) = path.segments.last_mut() {
                            segment.generics = args;
                            segment.span = segment.span.merge(end);
                        }
                        path.span = path.span.merge(end);
                    }
                    expr.span = start.merge(end);
                }
                TokenKind::LBracket => {
                    let open = self.span();
                    self.advance();
                    let index = match self.eat(&TokenKind::RBracket) {
                        Some(close) => self.report_empty_index(open.merge(close.span)),
                        None => {
                            let index = self.parse_expr();
                            if self.expect(&TokenKind::RBracket, "`]`").is_none() {
                                self.recover_to_index_close();
                            }
                            self.check_index_position(&index, open.merge(self.last_text_span()));
                            index
                        }
                    };
                    let span = start.merge(self.last_text_span());
                    expr = Expr {
                        kind: ExprKind::Index { base: Box::new(expr), index: Box::new(index) },
                        span,
                    };
                }
                TokenKind::Of if instantiable_path(&expr) => {
                    expr = self.parse_instantiation(expr, start);
                }
                _ => break,
            }
        }

        expr
    }

    /// `Array of Doc` in expression position, which §4.4 reaches an associated
    /// function through: `(Array of Doc).new()`.
    ///
    /// The parenthesised form is unambiguous and is all the corpus writes. The
    /// bare `Array of Doc.new()` is the form §4.3 rules on: `.new()` could
    /// attach to `Doc` or to the whole type, and rather than make the space in
    /// `Array of Doc .new()` load-bearing, Science reports the ambiguity
    /// (`SC0116`), says which reading it took, and offers the parentheses as a
    /// fix.
    ///
    /// **This is the one place the bracket migration left `of` in place, and
    /// the reason is a genuine grammar conflict rather than an oversight.**
    /// [`Self::parse_postfix`]'s loop already gives `[` a meaning in exactly
    /// this position — immediately after a complete primary — and that
    /// meaning is indexing. The decision the type grammar made everywhere
    /// else, "a bracket after a bare, not-yet-instantiated path is generic
    /// arguments," cannot be made here too: `Array[Int]` and `xs[i]` are the
    /// same three tokens, `instantiable_path` is true of `xs` for exactly the
    /// same reason it is true of `Array` (neither has taken arguments yet),
    /// and nothing else in the token stream tells them apart. Reading further
    /// does not help either — `Array[Int].new()` and `xs[i].new()` (a
    /// perfectly ordinary indexed method call) diverge only in whether `Array`
    /// or `xs` names a type, which is a fact name resolution has and the
    /// parser by design does not (`array_element`'s comment says the same
    /// thing about a different function: *"the parser has no definitions"*).
    /// Rust faces this exact conflict and resolves it with a second syntax,
    /// `::<>`, rather than by deciding it; Science's answer is to keep the one
    /// syntax that was never ambiguous, `of`, for exactly this position, and
    /// let `[...]` own every position where indexing does not compete with
    /// it: types, `def`/`type`/`interface`/`tool` headers, and `has`/
    /// `implements` heads. The cost is a second spelling of "generic
    /// arguments" that survives nowhere else, and a reader who reaches this
    /// function after reading every other one in this file and expects `[`
    /// has to be told why it is not here.
    fn parse_instantiation(&mut self, mut expr: Expr, start: Span) -> Expr {
        self.advance(); // `of`

        let parenthesised = self.at(&TokenKind::LParen);
        let mut args = self.parse_of_style_generic_args();

        // Only the bare single-argument form can be ambiguous: a parenthesised
        // list ends at its own `)`, so any `.` after it has just one reading.
        let split = if parenthesised { None } else { split_trailing_call(&mut args, self.peek()) };

        let end = self.last_text_span();
        if let ExprKind::Path(path) = &mut expr.kind {
            if let Some(segment) = path.segments.last_mut() {
                segment.generics = args;
                segment.span = segment.span.merge(end);
            }
            path.span = path.span.merge(end);
        }
        expr.span = start.merge(end);

        let Some(method) = split else { return expr };

        // The head and its one argument, read back out of the path the
        // arguments were just attached to, so the message quotes the type as
        // it now stands rather than as it was written.
        let (head, argument) = match &expr.kind {
            ExprKind::Path(path) => {
                let argument = path
                    .segments
                    .last()
                    .and_then(|segment| segment.generics.first())
                    .map(type_text_of_style)
                    .unwrap_or_default();
                (path.dotted(), argument)
            }
            _ => (String::new(), String::new()),
        };
        let call = format!("`.{}()`", method.name);
        let parenthesised_form = format!("({head} of {argument})");
        self.diagnostics.push(
            Diagnostic::error(
                codes::AMBIGUOUS_GENERIC_CALL,
                format!("{call} could belong to `{argument}` or to `{head} of {argument}`"),
            )
            .with_label(Label::primary(
                expr.span.merge(method.span),
                "this associated function call is ambiguous",
            ))
            .with_note(format!(
                "Science reads it as `{parenthesised_form}.{}()`, the whole generic type",
                method.name
            ))
            .with_suggestion(Suggestion {
                span: expr.span,
                replacement: parenthesised_form,
                message: "write the parentheses to say so".to_string(),
            }),
        );

        let args = self.parse_call_args();
        let span = start.merge(self.last_text_span());
        Expr {
            kind: ExprKind::MethodCall {
                receiver: Box::new(expr),
                method,
                generics: Vec::new(),
                args,
            },
            span,
        }
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
                "named arguments construct a record, so they need a record's name in front of them",
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
    /// §4.4 makes the one and only sign of a record construction.
    fn at_named_args(&self) -> bool {
        self.at(&TokenKind::LParen)
            && matches!(self.peek_ahead(1), TokenKind::Ident(_))
            && matches!(self.peek_ahead(2), TokenKind::Colon)
    }

    /// The arguments of a call, and the place §4.6's implicit closures are
    /// formed.
    ///
    /// `each` names the subject of the enclosing call without declaring it, so
    /// the argument that mentions one *is* the closure: `docs.map(each.title)`
    /// passes `each giving each.title` with the binder left out. Each argument
    /// therefore opens a scope, and an `each` inside it claims that scope; the
    /// argument is wrapped when it does.
    fn parse_call_args(&mut self) -> Vec<Arg> {
        let mut args = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return args;
        }
        while !self.at(&TokenKind::RParen) {
            let start = self.span();
            // `docs.sort(by: f)`: a name in front of an argument. Where the
            // callee is a path this list is a record construction instead and
            // never reaches here.
            let name = if matches!(self.peek(), TokenKind::Ident(_))
                && matches!(self.peek_ahead(1), TokenKind::Colon)
            {
                let name = self.expect_ident();
                self.advance(); // `:`
                name
            } else {
                None
            };

            self.each_scopes.push(false);
            let value = self.parse_expr();
            let claimed = self.each_scopes.pop().unwrap_or(false);
            let span = start.merge(self.last_text_span());
            let value = if claimed {
                Expr { kind: ExprKind::Closure { param: None, body: Box::new(value) }, span }
            } else {
                value
            };

            args.push(Arg { name, value, span });
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

    /// `f"..."`, from the five token kinds the lexer emits for one.
    ///
    /// **The decision.** The parser trusts the shape. `FStrStart` is always
    /// matched by an `FStrEnd`, every `InterpStart` by an `InterpEnd`, and the
    /// loop below therefore has no recovery of its own: it reads parts until
    /// the end token and stops.
    ///
    /// **The reason.** The lexer emits the delimiters on every path, including
    /// the four that report, so there is no token stream in which the shape is
    /// broken. A second layer of recovery here would be recovery from a state
    /// that cannot arise, and every diagnostic it produced would be a second
    /// complaint about a mistake `SC0170`-`SC0177` has already named — which is
    /// §1.5's *"fifty cascading diagnostics"* arriving one phase later.
    ///
    /// **The cost.** An empty hole is an `ExprKind::Error` with **no
    /// diagnostic from here**, because `SC0174` is the diagnostic and saying it
    /// twice helps nobody. A reader of this function has to know that to see
    /// why the silent arm is right.
    fn parse_fstring(&mut self, start: Span) -> Expr {
        self.advance(); // `f"`
        let mut parts = Vec::new();
        loop {
            match self.peek() {
                TokenKind::FStrText(text) => {
                    let text = text.clone();
                    self.advance();
                    parts.push(FStringPart::Text(text));
                }
                TokenKind::InterpStart => {
                    let brace = self.span();
                    self.advance();
                    // The lexer has already reported the empty hole; it
                    // reaches here as a hole with nothing in it, and the tree
                    // keeps its shape with an error node.
                    let expr = if self.at(&TokenKind::InterpEnd) {
                        error_expr(brace)
                    } else {
                        self.parse_expr()
                    };
                    self.expect(&TokenKind::InterpEnd, "the `}` of an interpolation");
                    parts.push(FStringPart::Hole(expr));
                }
                TokenKind::FStrEnd => {
                    self.advance();
                    break;
                }
                // Unreachable on any stream this lexer produces: it emits
                // `FStrEnd` before end of file on every path. Written rather
                // than unwrapped, because a parser that panics on a malformed
                // file is the one failure mode the whole crate is built to
                // avoid.
                _ => break,
            }
        }
        Expr { kind: ExprKind::FString(parts), span: start.merge(self.last_text_span()) }
    }

    /// A literal, a name, `self`, `each`, a parenthesised expression, or one of
    /// the control-flow forms that §4.5 makes expressions.
    fn parse_primary(&mut self) -> Expr {
        let start = self.span();

        if let Some(literal) = literal_of(self.peek()) {
            self.advance();
            return Expr { kind: ExprKind::Literal(literal), span: start };
        }

        match self.peek() {
            TokenKind::FStrStart => self.parse_fstring(start),
            TokenKind::SelfValue => {
                self.advance();
                Expr { kind: ExprKind::SelfValue, span: start }
            }
            TokenKind::Each => {
                self.advance();
                self.claim_each(start);
                Expr { kind: ExprKind::Each, span: start }
            }
            // `while` and `println` are ordinary identifiers now, so they
            // reach the name arm below unless they are caught first.
            _ if self.at_while_word() => self.report_while_word(start),
            _ if self.at_try_word() => self.report_try_word(),
            _ if self.word_at(0, "println") && matches!(self.peek_ahead(1), TokenKind::LParen) => {
                self.report_println_word()
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
            // §6.2: `[` in *prefix* position opens a literal. There is no
            // rule to apply and no lookahead to do — this is the null
            // denotation and [`Self::parse_postfix`]'s arm is the left one,
            // exactly as `(` is grouping here and a call there.
            TokenKind::LBracket => self.parse_array_lit(start),
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Loop => {
                self.advance();
                let body = self.parse_block();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Loop { body }, span }
            }
            // §3 of the FFI note: `unsafe:` with a body, in either of §4.5's
            // two block forms. It is an expression, so `let h be unsafe:
            // cudnnCreate(..)` needs no statement form of its own.
            TokenKind::Unsafe => {
                self.advance();
                let body = self.parse_block();
                let span = start.merge(self.last_text_span());
                Expr { kind: ExprKind::Unsafe(body), span }
            }
            TokenKind::For => {
                self.advance();
                // `for x in xs` (revision 2 §2.1). `each` used to stand
                // between the two and is still a keyword, so a stale loop
                // arrives here as a token rather than as a pattern.
                if self.at(&TokenKind::Each) {
                    self.report_each_after_for(start);
                }
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

    /// Marks the argument this `each` belongs to, and rejects a nested one.
    ///
    /// §4.6 gives exactly one case where `each` cannot work: in
    /// `outer.map(each.inner.map(each.x))` the two refer to different subjects
    /// and the inner shadows the outer irrecoverably. Science rejects that
    /// rather than picking a rule, and the error names the `giving` form,
    /// which is what the case exists for.
    ///
    /// Two `each` in the *same* argument are fine — they are the same subject —
    /// so only an enclosing claim is an error.
    fn claim_each(&mut self, span: Span) {
        let depth = self.each_scopes.len();
        if depth == 0 {
            // Outside any call argument there is no subject to name. Leaving
            // the bare `Each` node for resolution keeps the parser out of a
            // judgement it has no scope information to make.
            return;
        }
        if self.each_scopes[..depth - 1].iter().any(|claimed| *claimed) {
            self.diagnostics.push(
                Diagnostic::error(
                    codes::NESTED_EACH,
                    "a nested `each` would name a different subject than the `each` around it",
                )
                .with_label(Label::primary(span, "this `each` is inside another `each`"))
                .with_note(
                    "name the parameter instead, with the `giving` form: \
                     `outer.map(item giving item.inner.map(each.x))`",
                ),
            );
            return;
        }
        self.each_scopes[depth - 1] = true;
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

    /// `[a, b, c]` and `[]` — the array literal of
    /// `indexing-and-array-literals.md` §3.1.
    ///
    /// **The decision** is that this is an ordinary comma-separated list with
    /// no rule of its own: the loop below is `parse_call_args`'s loop with the
    /// delimiters changed and nothing added. §4.7 already allows a trailing
    /// comma "in every bracketed and parenthesized list", and a bracketed list
    /// is what this is, so the trailing comma needs no rule either.
    ///
    /// **The reason** that is worth saying out loud is the formatter. Its
    /// magic trailing comma reads the *comma*, not the node — `science-fmt`
    /// never looks at the tree — so a literal whose trailing comma were
    /// special here would lay out differently from every other list, for a
    /// reason no reader could see from the text.
    ///
    /// **The cost** is that `[]` and a literal whose elements all failed to
    /// parse arrive at the checker as the same zero-length node; see
    /// [`ExprKind::ArrayLit`].
    fn parse_array_lit(&mut self, start: Span) -> Expr {
        if let Some(close) = self.comprehension_bracket() {
            return self.report_comprehension(start, close);
        }
        self.advance(); // `[`

        let mut elements = Vec::new();
        while !self.at(&TokenKind::RBracket) {
            elements.push(self.parse_expr());
            // `[1 2]`: the element ended and what follows is neither a
            // separator nor the close. Reported here, where the list is in
            // scope and the message can name both tokens, and then recovered
            // to the next landmark — leaving it to the `expect` below would
            // report a second time when the line failed to end, which is the
            // two-diagnostics-for-one-mistake this change exists to remove.
            //
            // A list boundary is the other half of that count. `[1, 2` with
            // no `]` has one mistake and it is the missing bracket, so the
            // loop stops without a word and the `expect` below is the only
            // place that speaks. This is also why recovery may never eat one:
            // see `at_list_boundary`.
            if !self.at(&TokenKind::Comma) && !self.at(&TokenKind::RBracket) {
                if self.at_list_boundary() {
                    break;
                }
                self.expect(&TokenKind::RBracket, "`,` or `]`");
                self.recover_in_brackets();
            }
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "`]`");

        let span = start.merge(self.last_text_span());
        Expr { kind: ExprKind::ArrayLit(elements), span }
    }

    /// The offset of this bracket's `]`, when what stands in it is Python's
    /// comprehension rather than a list of elements (§5.3).
    ///
    /// The mark is a `for` at the bracket's own depth that is not the first
    /// token inside it, and both halves earn their place. The **depth** leaves
    /// `[[y for y in ys]]`'s inner comprehension to the inner bracket, so one
    /// mistake is reported by one bracket. The **position** keeps
    /// `[for x in xs: f(x)]` out of the net: §4.5 makes `for` an expression,
    /// so an array holding one is a legal if pointless thing to write, and a
    /// comprehension always has its output expression in front of the `for`
    /// because that is the whole of what the form is for.
    ///
    /// The scan is bounded by the bracket and never crosses a layout token —
    /// which inside an unclosed bracket the lexer does not emit anyway. The
    /// arms for them are what makes this total on a file that did not lex.
    fn comprehension_bracket(&self) -> Option<usize> {
        let mut depth = 0usize;
        let mut saw_for = false;
        let mut offset = 1;
        loop {
            match self.peek_ahead(offset) {
                TokenKind::Eof | TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent => {
                    return None
                }
                TokenKind::RParen | TokenKind::RBrace if depth == 0 => return None,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                TokenKind::RBracket if depth == 0 => return saw_for.then_some(offset),
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
                TokenKind::For if depth == 0 && offset > 1 => saw_for = true,
                _ => {}
            }
            offset += 1;
        }
    }

    /// `SC0153`: a comprehension, reported on the whole bracket (§5.3).
    ///
    /// **The decision** is that this is caught here and not left to fall out
    /// of the grammar, which is §7.1's own requirement: a comprehension
    /// type-checks as nothing at all, so what falls out is a complaint about a
    /// missing `,` followed by a complaint about the end of the line, and
    /// nothing at all about the construct. This is `llm-ergonomics.md`'s play
    /// — catch the habit the other language taught and name the local
    ///   form — against the strongest habit Python has.
    ///
    /// **There is no machine-applicable fix, and that is deliberate.** The
    /// rewrite §5.3 prints is three sub-expressions reordered with the loop
    /// variable rewritten to `each`, and it is *source text*. This phase holds
    /// tokens and spans and no text at all, so the fix it could build would be
    /// a guess at three spellings. [`Self::report_try_word`] settled what to
    /// do about that: a fix that might be wrong is worse than a note that is
    /// right. The note carries the shape, which is what the reader has to
    /// learn anyway.
    ///
    /// **The cost** is that the bracket is dropped whole — recovery consumes
    /// it and leaves an error node — so a name misspelled inside the
    /// comprehension is not reported until the comprehension is gone. That is
    /// the right order, because the reader fixes the construct first, and it
    /// is what keeps this one diagnostic instead of one per element.
    fn report_comprehension(&mut self, start: Span, close: usize) -> Expr {
        let span = start.merge(self.token_at(close).span);
        self.diagnostics.push(
            Diagnostic::error(
                codes::COMPREHENSION,
                "Science has no comprehensions; write the chain",
            )
            .with_label(Label::primary(
                span,
                "this bracket holds a comprehension, not a list",
            ))
            .with_note(
                "the chain does the same thing, lazily and in reading order: \
                 `source.iterate().keep(<condition>).map(<expression>).collect()`",
            )
            .with_note(
                "`keep` comes before `map`, and `each` names the element, so the chain \
                 declares no loop variable",
            ),
        );
        // Everything up to and including the `]`. The offset is exact, so
        // nothing after the bracket is eaten and the next statement parses.
        for _ in 0..=close {
            self.advance();
        }
        error_expr(span)
    }

    /// `SC0151`: `a[]`, an index bracket with no index in it.
    ///
    /// **The decision** is a code of its own rather than the `SC0105` that
    /// falls out — *expected an expression, found `]`* — because the two name
    /// different things. `SC0105` says a token stands where one may not; this
    /// says the bracket is missing its index, which is what the reader did.
    ///
    /// **§7.1 offers two fixes and this offers one.** Its first is `a[..]`,
    /// the whole thing as a view, and F0 does not spell a bare `..` — see
    /// [`Self::parse_range`]. A fix that does not parse is worse than one
    /// fix, so the open-ended form waits for the grammar that carries it.
    ///
    /// **The cost** is a line: the tree keeps an `Index` whose index is an
    /// error node, rather than becoming an error node whole. That is what
    /// leaves the base to be resolved, so a misspelling in `frame[]`'s `frame`
    /// is still reported.
    fn report_empty_index(&mut self, span: Span) -> Expr {
        self.diagnostics.push(
            Diagnostic::error(codes::EMPTY_INDEX, "this index bracket has no index in it")
                .with_label(Label::primary(span, "`[]` indexes nothing"))
                .with_note(
                    "an index bracket holds one index position: `a[0]`, `a[i]`, or a \
                     range, `a[1..5]`",
                ),
        );
        error_expr(span)
    }

    /// The two things §7.1 can decide about an index position with no types.
    ///
    /// **Both are literal-only, and the line is drawn there on purpose.**
    /// `a[-1]` is a mistake in the text; `a[-n]` is a program whose `n` might
    /// be anything, and saying something about it needs the range analysis
    /// §1.3 hands to `SC0286` in `science-types`. Asking only about what is
    /// written is what keeps these two free of false positives, and a
    /// diagnostic with false positives in the highest-traffic expression in
    /// the language would be turned off in a week.
    ///
    /// **The cost** is that the rule does not compose: `a[-1..2]` has a
    /// negative bound and is not reported, because the index position is a
    /// range and not a negative literal. Widening it means deciding what
    /// `a[-1..-1]` should say, and §7.1 gives one code for one shape.
    fn check_index_position(&mut self, index: &Expr, bracket: Span) {
        match &index.kind {
            ExprKind::Unary { op: UnaryOp::Neg, operand } => {
                if let ExprKind::Literal(Literal::Int { value, .. }) = &operand.kind {
                    self.report_negative_index(index.span, bracket, *value);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let (Some(low), Some(high)) = (int_literal(start), int_literal(end)) {
                    if low > high {
                        self.report_reversed_range(index.span, low, high);
                    }
                }
            }
            _ => {}
        }
    }

    /// `SC0152`: `a[-1]`, which §4.5 refuses to read as the last element.
    ///
    /// **The decision to give this its own code** is §4.5's: the Python muscle
    /// memory is strong enough that the mistake will be common, and the fix is
    /// mechanical. Left to the type checker it would arrive as `SC0286` — an
    /// index that could not be proved in range — which is true and is not what
    /// happened.
    ///
    /// **The fix is the bracket, not the expression.** `a.last()` needs the
    /// text of `a`, which this phase does not have; replacing `[-1]` with
    /// `.last()` needs only the bracket's span and produces the same text. It
    /// is offered for `-1` alone, because §4.5's second replacement,
    /// `a[a.length() - n]`, needs the base twice and cannot be built from
    /// spans at all. That one is a note.
    fn report_negative_index(&mut self, index: Span, bracket: Span, value: u128) {
        let mut diagnostic = Diagnostic::error(
            codes::NEGATIVE_INDEX,
            "a negative index does not count from the end in Science",
        )
        .with_label(Label::primary(index, "an index runs from `0` up to the extent"))
        .with_note(
            "Python reads `a[-1]` as the last element, which is what makes `a[i - 1]` \
             in a loop read the last element at an `i` of zero instead of failing",
        );
        if value == 1 {
            diagnostic = diagnostic.with_suggestion(Suggestion {
                span: bracket,
                replacement: ".last()".to_string(),
                message: "read the last element with".to_string(),
            });
        } else {
            diagnostic = diagnostic.with_note(format!(
                "count from the end in the text instead: `xs[xs.length() - {value}]`"
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    /// `SC0154`: `a[5..1]`, a range with literal bounds that holds nothing.
    ///
    /// **No fix is offered and §7.1 says why**: which of the two bounds is the
    /// wrong one is not something the compiler can know. Saying that in the
    /// message is better than guessing and better than saying nothing — a
    /// reader told only that the range is empty will re-read the bound they
    /// already believe.
    fn report_reversed_range(&mut self, span: Span, low: u128, high: u128) {
        self.diagnostics.push(
            Diagnostic::error(
                codes::REVERSED_RANGE,
                format!("this range starts at {low} and ends at {high}, so it holds nothing"),
            )
            .with_label(Label::primary(span, "the start is past the end"))
            .with_note(
                "no fix is offered because which of the two bounds is wrong is not \
                 something the compiler can know",
            ),
        );
    }

    /// `if cond: ..` with an optional `else`.
    ///
    /// The dangling `else` binds to the innermost `if` (§4.5) because this
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

    /// One arm. §4.5: the pattern is parsed by the pattern grammar, and
    /// whatever follows it is the separator — there is no way to find the `:`
    /// by scanning, because `:` also appears inside a record pattern.
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

    /// One alternative: a literal, `_`, a binding, a variant, a record or a
    /// tuple.
    ///
    /// Unlike an expression, a pattern's path may span several segments: there
    /// is no field access in a pattern, so a `.` can only be §4.7's qualified
    /// variant.
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
            TokenKind::Mutable => {
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
                    // Named fields mean a record, positional ones a variant —
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

    /// The named fields of a record pattern: `Doc(title: t, body: _)`.
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

    // --- interfaces and implementations -----------------------------------

    /// `interface Name of T: Super + Other where ..:` followed by an indented
    /// list of members.
    fn parse_interface(&mut self, is_pub: bool, start: Span) -> Option<InterfaceDecl> {
        self.advance(); // `interface`
        self.parse_interface_body(is_pub, start)
    }

    /// Everything after the `interface` keyword.
    ///
    /// Split out so that [`Self::report_trait_word`], which has already
    /// stepped over the old spelling, can share it.
    fn parse_interface_body(&mut self, is_pub: bool, start: Span) -> Option<InterfaceDecl> {
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params();

        // Two colons can follow, and only one of them is ever present at a
        // time in the corpus: the one that introduces the required interfaces
        // is followed by a name, the one that opens the body by the end of
        // the line.
        let supers = if self.at(&TokenKind::Colon)
            && !matches!(self.peek_ahead(1), TokenKind::Newline)
        {
            self.advance();
            self.parse_bounds(BoundPosition::Interface)
        } else {
            Vec::new()
        };

        let where_clause = self.parse_where_clause();
        let members = self.parse_indented_body(Self::parse_trait_member);

        let mut assoc_types = Vec::new();
        let mut methods = Vec::new();
        for member in members {
            match member {
                Member::AssocDecl(decl) => assoc_types.push(decl),
                Member::Method(decl) => methods.push(decl),
                Member::AssocBinding(binding) => self.error(
                    codes::EXPECTED_MEMBER,
                    "an interface declares an associated type with `type Item` and leaves \
                     the type to the implementation",
                    binding.span,
                ),
            }
        }

        Some(InterfaceDecl {
            is_pub,
            name,
            generics,
            supers,
            where_clause,
            assoc_types,
            methods,
            span: start.merge(self.last_text_span()),
        })
    }

    /// `Doc implements Summarize:`, `Doc has:`, and the block-less marker
    /// form `Position implements Copy` (§4.4).
    ///
    /// The type comes first in all three, so this is also where a line at the
    /// top level of a module that is not a declaration at all is diagnosed:
    /// by the time the head has been read, the next word says whether it was
    /// ever meant to be one.
    fn parse_impl(&mut self, start: Span) -> Option<ImplBlock> {
        let head = self.parse_path_segments()?;
        let generics = self.parse_generic_params();
        let self_ty = self_type_of(&head, &generics);

        let interface = if self.eat(&TokenKind::Implements).is_some() {
            Some(self.parse_impl_trait()?)
        } else if let Some(has) = self.eat(&TokenKind::Has) {
            // `has methods` was two words before revision 2 §5; `methods` is
            // an ordinary identifier now, so a stale block is caught by name.
            if self.word_at(0, "methods") {
                self.report_methods_after_has(has.span);
            }
            None
        } else {
            let found = describe(self.peek());
            let message = format!(
                "expected `implements` or `has` after `{}`, found {found}",
                head.dotted()
            );
            self.diagnostics.push(
                Diagnostic::error(codes::EXPECTED_ITEM, message.clone())
                    .with_label(Label::primary(self.span(), message))
                    .with_label(Label::secondary(head.span, "this begins an implementation"))
                    // The note that stood here said *"a module holds
                    // declarations only; a statement belongs in a function
                    // body"*. `script-mode.md` §8.1 predicted the day that
                    // clause stopped being true, and this is it: a top-level
                    // line beginning with a name is now a statement unless
                    // `implements` or `has` is in it. So the line that reaches
                    // this point is one that *does* contain one of those words
                    // and still is not an implementation — and the note's job
                    // is to say which word made the parser read it this way.
                    .with_note(
                        "a top-level line beginning with a name is an implementation when \
                         `implements` or `has` follows the type, and a statement when neither \
                         does",
                    ),
            );
            return None;
        };

        let where_clause = self.parse_where_clause();

        let members = if self.at(&TokenKind::Colon) {
            self.parse_indented_body(Self::parse_impl_member)
        } else {
            if self.at(&TokenKind::Newline) && matches!(self.peek_ahead(1), TokenKind::Indent) {
                // An indented body with nothing introducing it is a missing
                // `:`, and saying so here points at the header rather than at
                // the first method.
                let span = self.span();
                self.error(
                    codes::EXPECTED_BLOCK,
                    "expected `:` before the body of the implementation",
                    span,
                );
                self.advance();
                self.skip_indented_region();
            } else {
                // The marker form: one line, no block, no members.
                self.expect_line_end();
            }
            Vec::new()
        };

        let mut assoc_types = Vec::new();
        let mut methods = Vec::new();
        for member in members {
            match member {
                Member::AssocBinding(binding) => assoc_types.push(binding),
                Member::Method(decl) => methods.push(decl),
                Member::AssocDecl(decl) => self.error(
                    codes::EXPECTED_MEMBER,
                    "an implementation has to give the associated type a type, as in \
                     `type Item is Int`",
                    decl.span,
                ),
            }
        }

        Some(ImplBlock {
            generics,
            interface,
            self_ty,
            where_clause,
            assoc_types,
            methods,
            span: start.merge(self.last_text_span()),
        })
    }

    /// The interface after `implements`. It is named by a path — possibly with
    /// arguments, as in `From of ParseError` — so anything else is an error
    /// rather than something a later phase could make sense of.
    fn parse_impl_trait(&mut self) -> Option<TypeBound> {
        if !matches!(self.peek(), TokenKind::Ident(_)) {
            let found = describe(self.peek());
            let span = self.span();
            self.error(
                codes::EXPECTED_TRAIT,
                format!("expected an interface name after `implements`, found {found}"),
                span,
            );
            return None;
        }
        self.parse_type_bound(BoundPosition::Interface)
    }

    /// One member of an `interface` body: a function, or `type Item`.
    fn parse_trait_member(&mut self) -> Option<Member> {
        self.parse_member()
    }

    /// One member of an implementation body: a function, or `type Item is Int`.
    fn parse_impl_member(&mut self) -> Option<Member> {
        self.parse_member()
    }

    /// The shared shape of both bodies.
    ///
    /// An associated type is read in whichever of §5.4's two forms it was
    /// written in, and the caller rejects the one that does not belong to it.
    /// Refusing the wrong form here instead would be cheaper by one branch and
    /// strictly worse to read: the parser would report `is` as an unexpected
    /// token, when what the author needs to be told is that an interface declares
    /// `type Item` and only the implementation says what it stands for.
    fn parse_member(&mut self) -> Option<Member> {
        let start = self.span();

        if self.at(&TokenKind::Type) {
            self.advance();
            let name = self.expect_ident()?;
            if self.eat(&TokenKind::Is).is_some() {
                let ty = self.parse_type();
                return Some(Member::AssocBinding(AssocTypeBinding {
                    name,
                    ty,
                    span: start.merge(self.last_text_span()),
                }));
            }
            return Some(Member::AssocDecl(AssocTypeDecl {
                name,
                span: start.merge(self.last_text_span()),
            }));
        }

        let is_pub = self.eat(&TokenKind::Public).is_some();
        // An `interface` body and an implementation body share this function,
        // and a `tool` belongs in neither. Reported here rather than left to
        // the message below, which would name `tool` as a word that is not
        // `function` and say nothing about why.
        if self.at(&TokenKind::Tool) {
            self.report_tool_out_of_place();
            return None;
        }
        if !self.at(&TokenKind::Function) {
            let found = describe(self.peek());
            self.error(
                codes::EXPECTED_MEMBER,
                format!(
                    "expected a `function` or an associated type declared with `type`, found \
                     {found}"
                ),
                start,
            );
            return None;
        }
        Some(Member::Method(self.parse_fn(FnForm::Def, is_pub, start)?))
    }
}

/// What one line of an `interface` or implementation body turned out to be.
///
/// The two bodies differ in one member only, and parsing them apart would mean
/// two copies of the function grammar's entry point.
enum Member {
    /// `type Item`, in an interface.
    AssocDecl(AssocTypeDecl),
    /// `type Item is Int`, in an implementation.
    AssocBinding(AssocTypeBinding),
    Method(FnDecl),
}

/// What may stand at the top level of a file, as a diagnostic says it.
///
/// Written once because it appears in two messages that must not drift apart:
/// a line that starts with the wrong keyword, and a line that starts with a
/// name but never reaches `implements`.
///
/// **"or a statement" is the half `script-mode.md` §8.1 required.** Until that
/// note landed this message ended at the closing parenthesis and a companion
/// note said *"a module holds declarations only; a statement belongs in a
/// function body"* — a sentence that was a grammar commitment, was never
/// argued for, and stopped being true the day the top level admitted
/// statements. What is left for `SC0101` to report is a token that can begin
/// neither half, which is why the message now names both.
const EXPECTED_DECLARATION: &str = "expected a declaration (`def`, `tool`, `type`, `choice`, \
                                    `interface`, `const`, `use`, or a type followed by \
                                    `implements` or `has`) or a statement";

/// The `main` a file declares for itself, if it declares one.
///
/// A `tool main` counts. The collision `SC0117` is about is over the *name*,
/// and which word declared it changes nothing about there being two.
fn explicit_main(items: &[Item]) -> Option<Span> {
    items.iter().find_map(|item| match &item.kind {
        ItemKind::Fn(decl) if decl.name.name == "main" => Some(decl.name.span),
        _ => None,
    })
}

/// A file's top-level statements, as the `def main() -> Error?` the author did
/// not type.
///
/// **The signature is `-> Error?`, and it is not sugar for `-> ((), Error?)`.**
/// `script-mode.md` §2.3 chose the type and flagged that nothing in the core
/// spec said a lone `E?` return was legal. It is: the corpus has had one since
/// before this note — `examples/09_absence_and_failure.science` declares both
/// `def save_config(..) -> Error?` and `def start(..) -> Error?`, and says in
/// as many words why the lone form is not the pair — *"a def with no value to
/// hand back returns the error alone. The pair would have `()` in its first
/// slot and a name bound to it could do nothing."* So the question the note
/// raised is settled by what the compiler already accepts, and this function
/// needed no license it did not have.
///
/// **The type is `-> Error?` rather than nothing** because revision 2 makes
/// `if err?: return err` the idiom for every fallible call, and a script is
/// mostly IO. A top level on which the language's own error model does not
/// typecheck would need a workaround on the first line of most programs.
///
/// **The body always ends in `null`, and never in the last statement.**
/// §2.3's "falling off the end is an implicit `return null`" is spelled here:
/// the tail expression is a generated `null`, and a trailing expression
/// statement stays a statement instead of being promoted to the tail the way
/// [`finish_block`] would promote it. Promoting it would make the value of
/// `print("hello")` the script's return value, and `()` is not an `Error?`.
///
/// Every generated node gets a zero-width span — the name where the script
/// starts, the return type and the `null` where it ends — because a span that
/// covered real tokens would underline code the author wrote to explain a
/// function they did not.
fn script_body(stmts: Vec<Stmt>, module: Span) -> Item {
    let first = stmts.first().map_or(module, |stmt| stmt.span);
    let last = stmts.last().map_or(module, |stmt| stmt.span);
    let span = first.merge(last);
    let opens = Span::at(span.file, span.start);
    let ends = Span::at(span.file, span.end);

    let error = Path {
        segments: vec![PathSegment {
            name: Ident::new("Error", ends),
            generics: Vec::new(),
            span: ends,
        }],
        span: ends,
    };
    let ret = Type {
        kind: TypeKind::Nullable(Box::new(Type {
            kind: TypeKind::Path(error),
            span: ends,
        })),
        span: ends,
    };
    let tail = Expr { kind: ExprKind::Literal(Literal::Null), span: ends };
    let body = Block { stmts, tail: Some(Box::new(tail)), span };

    let decl = FnDecl {
        form: FnForm::Def,
        is_pub: false,
        name: Ident::new("main", opens),
        generics: Vec::new(),
        self_param: None,
        params: Vec::new(),
        ret: Some(ret),
        where_clause: Vec::new(),
        body: Some(body),
        span,
    };
    Item { kind: ItemKind::Fn(decl), span, doc: None }
}

fn empty_block(span: Span) -> Block {
    Block { stmts: Vec::new(), tail: None, span: Span::at(span.file, span.start) }
}

fn error_expr(span: Span) -> Expr {
    Expr { kind: ExprKind::Error, span }
}

/// The value of an integer literal written with no sign, or `None`.
///
/// The suffix is ignored: `a[5u32..1u32]` is the same mistake as `a[5..1]`,
/// and how wide a bound is has nothing to do with whether it is past the other
/// one. The base is ignored for the same reason — `a[0x5..0x1]` counts.
fn int_literal(expr: &Expr) -> Option<u128> {
    match &expr.kind {
        ExprKind::Literal(Literal::Int { value, .. }) => Some(*value),
        _ => None,
    }
}

/// The type an implementation head names, rebuilt from the head path and the
/// parameters declared on it.
///
/// `Grid[T, const ROWS: Int] has:` declares two parameters and
/// implements `Grid[T, ROWS]`. Echoing the names back as arguments keeps
/// `self_ty` an ordinary type — a const *parameter* has an annotation and no
/// type has room for one — while `generics` keeps what was declared.
fn self_type_of(head: &Path, generics: &[GenericParam]) -> Type {
    let mut path = head.clone();
    if !generics.is_empty() {
        let args = generics
            .iter()
            .map(|param| {
                let span = param.name.span;
                let segment =
                    PathSegment { name: param.name.clone(), generics: Vec::new(), span };
                Type { kind: TypeKind::Path(Path { segments: vec![segment], span }), span }
            })
            .collect();
        if let Some(last) = path.segments.last_mut() {
            last.generics = args;
        }
    }
    let span = path.span;
    Type { kind: TypeKind::Path(path), span }
}

/// Takes the `.new` off `Array of Doc.new(`, returning the method name.
///
/// This is the ambiguity §4.3 rules on, and it exists because a type's path
/// and a method call are both written with a `.`. The reading taken is the one
/// the parentheses would have given — the call belongs to the whole generic
/// type — and the caller reports it either way, so nothing is decided here in
/// silence.
fn split_trailing_call(args: &mut [Type], next: &TokenKind) -> Option<Ident> {
    if !matches!(next, TokenKind::LParen) {
        return None;
    }
    let [Type { kind: TypeKind::Path(path), span }] = args else { return None };
    if path.segments.len() < 2 {
        return None;
    }
    let method = path.segments.pop()?;
    let end = path.segments.last().map(|s| s.span).unwrap_or(method.span);
    path.span = Span::new(path.span.file, path.span.start, end.end);
    *span = path.span;
    Some(method.name)
}

/// The element type of `Array[T]`, written back out, when `ty` is one.
///
/// Matching on the name is the whole of it: the parser has no definitions, and
/// `Array` is the F0 library's, not any user's.
fn array_element(ty: &Type) -> Option<String> {
    let TypeKind::Path(path) = &ty.kind else { return None };
    let [segment] = path.segments.as_slice() else { return None };
    if segment.name.name != "Array" {
        return None;
    }
    match segment.generics.as_slice() {
        [element] => Some(type_text(element)),
        _ => Some("T".to_string()),
    }
}

/// Why a named F0 type cannot cross the boundary, when it cannot.
///
/// The list is the negative half of §1.3 and §1.4: every one of these is a
/// Science layout the C side does not know, and every one of them is a name
/// the F0 library owns, so no user type can collide with an entry. Anything
/// not listed is left to the type checker, which is the phase that knows
/// whether it implements `ffi.CLayout`.
fn science_layout_note(name: &str) -> Option<&'static str> {
    Some(match name {
        "String" => {
            "a C string is NUL-terminated and a Science `String` is UTF-8 bytes with a \
             length; write `ffi.CStr` for one that is borrowed and `ffi.CString` for one \
             Science owns, and expect the copy both of them make"
        }
        "Map" => "a `Map` is a Science hash table, and the C side does not know its layout",
        "Box" => {
            "a `Box` owns what it points at under Science's allocator; pass \
             `ffi.Pointer[T]` and say in the binding who frees it"
        }
        "Option" => {
            "an `Option` is a niche, and which niche depends on the payload; the C \
             spelling of absence is a null `ffi.Pointer`"
        }
        "Result" => {
            "a `Result` is a Science choice; a C function reports failure with a status \
             the safe wrapper converts"
        }
        "Chars" => "an iterator is a Science value with no C representation",
        _ => return None,
    })
}

/// A type written back out in the `of` spelling, for the fix `SC0116` offers.
///
/// **Deliberately not bracket syntax.** `SC0116` fires only inside
/// [`Parser::parse_instantiation`], the one escape valve the bracket
/// migration left spelled `of` because `[` there collides with indexing (see
/// that function's doc comment); the fix it offers wraps the *user's own*
/// `of` in parentheses, `Array of Doc` to `(Array of Doc)`, and a suggestion
/// that handed back `Array[Doc]` would be correcting a spelling the user was
/// never wrong to use. [`type_text`] is the bracket-spelling counterpart,
/// used everywhere a type is read back out of type position rather than of
/// this one expression-only position.
///
/// It handles only what can appear in the ambiguous position — a path, with or
/// without arguments of its own — because that is the only place it is used,
/// and a fix that guesses at the text it is replacing is worse than no fix.
fn type_text_of_style(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(path) => {
            let head = path.dotted();
            let args: Vec<String> = path
                .segments
                .last()
                .map(|s| s.generics.iter().map(type_text_of_style).collect())
                .unwrap_or_default();
            match args.len() {
                0 => head,
                1 => format!("{head} of {}", args[0]),
                _ => format!("{head} of ({})", args.join(", ")),
            }
        }
        TypeKind::SelfType => "Self".to_string(),
        TypeKind::SelfAssoc(name) => format!("Self.{}", name.name),
        _ => String::new(),
    }
}

/// A type written back out as source, in the bracket spelling every type
/// position but the `of`-style escape valve uses (see
/// [`type_text_of_style`]'s doc comment for that one).
///
/// It handles only what can appear where it is called from — a path, with or
/// without arguments of its own — because a fix or a message that guesses at
/// text it does not fully understand is worse than none.
fn type_text(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(path) => {
            let head = path.dotted();
            let args: Vec<String> = path
                .segments
                .last()
                .map(|s| s.generics.iter().map(type_text).collect())
                .unwrap_or_default();
            if args.is_empty() { head } else { format!("{head}[{}]", args.join(", ")) }
        }
        TypeKind::SelfType => "Self".to_string(),
        TypeKind::SelfAssoc(name) => format!("Self.{}", name.name),
        _ => String::new(),
    }
}

/// Whether `expr` is a name that could still take generic arguments.
///
/// `Array` can; `Array of Int` already has them, and a call, a field or a
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
/// §4.4: "a function's value is its last expression". Splitting that
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
/// Shared by expressions, patterns and const generic arguments, which take
/// literals from exactly the same set: they would otherwise drift apart
/// silently.
/// The const atom a parsed type turns out to have been.
///
/// Only two shapes can be promoted: a bare one-segment path, which is a
/// parameter name, and a const argument already recognised as one. A
/// `&T` or an `Array[T]` followed by `+` is not arithmetic that was
/// mis-parsed; it is an error, and returning `None` leaves it to be reported
/// as the syntax error it is.
/// The literal a const term turns out to be, for §2.1's mirrored production.
///
/// Only a bare literal counts. `(2) * N` is not `k * e` — the parenthesised
/// form is a `ConstExpr` whose kind is `Lit`, which this does match, and that
/// is deliberate: the parentheses are not part of the grammar's shape.
fn const_literal_of(term: &ConstExpr) -> Option<(Literal, Span)> {
    match &term.kind {
        ConstExprKind::Lit(literal) => Some((literal.clone(), term.span)),
        _ => None,
    }
}

fn const_atom_of(ty: &Type) -> Option<ConstExpr> {
    match &ty.kind {
        TypeKind::Const(value) => Some(value.clone()),
        TypeKind::Path(path) if path.segments.len() == 1 && path.segments[0].generics.is_empty() => {
            let name = path.segments[0].name.clone();
            let span = name.span;
            Some(ConstExpr { kind: ConstExprKind::Param(name), span })
        }
        _ => None,
    }
}

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
        TokenKind::Null => Literal::Null,
        _ => return None,
    })
}

/// §4.6's precedence table, as an operator and the rung it sits on.
///
/// Higher binds tighter. The rows above `**` — unary, `as`, and the postfix
/// chain, which now carries `?` where it used to carry `try` — are not here:
/// they are not infix, so they belong to the descent rather than to the climb. Assignment is not here either, and
/// §4.6 says why: it is a statement, not an operator.
///
/// The comparison phrases reach this table through the symbol they stand for,
/// so there is exactly one place a comparison's precedence is written down.
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
        T::AtSign => (BinaryOp::MatMul, 9),

        T::StarStar => (BinaryOp::Pow, 10),

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
        FStrStart | FStrText(_) | FStrEnd => "an interpolating string literal".to_string(),
        InterpStart => "the `{` of an interpolation".to_string(),
        InterpEnd => "the `}` of an interpolation".to_string(),
        Char(_) => "a character literal".to_string(),
        Ident(name) => format!("`{name}`"),
        Unknown(c) => format!("`{c}`"),
        Reserved(word) => {
            format!("`{}`, which is reserved for a later phase", reserved_text(*word))
        }
        other => format!("`{}`", fixed_text(other)),
    }
}

/// The source text of a word, keyword or not.
///
/// A pre-revision comparison phrase is `is` — a keyword — followed by words
/// that are ordinary identifiers now, so quoting the phrase back at the
/// programmer needs both halves.
fn describe_word(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(name) => name.clone(),
        other => fixed_text(other).to_string(),
    }
}

/// The source text of every token whose spelling is fixed.
fn fixed_text(kind: &TokenKind) -> &'static str {
    use TokenKind::*;
    match kind {
        Function => "def",
        Tool => "tool",
        Let => "let",
        Be => "be",
        Mutable => "mutable",
        If => "if",
        Else => "else",
        Match => "match",
        For => "for",
        Each => "each",
        In => "in",
        Loop => "loop",
        Return => "return",
        Break => "break",
        Continue => "continue",
        Assert => "assert",
        Type => "type",
        Choice => "choice",
        Interface => "interface",
        Implements => "implements",
        Has => "has",
        Of => "of",
        Borrowed => "borrowed",
        Any => "any",
        Use => "use",
        Public => "public",
        Const => "const",
        Giving => "giving",
        Null => "null",
        True => "true",
        False => "false",
        SelfValue => "self",
        SelfType => "Self",
        As => "as",
        Where => "where",
        Extern => "extern",
        Unsafe => "unsafe",
        And => "and",
        Or => "or",
        Not => "not",

        Is => "is",

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
        DotDot => "..",
        DotDotEq => "..=",
        Arrow => "->",
        FatArrow => "=>",
        Underscore => "_",
        AtSign => "@",
        Hash => "#",
        Question => "?",

        Plus => "+",
        Minus => "-",
        Star => "*",
        StarStar => "**",
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
        Equation => "equation",
        Mod => "mod",
        Pure => "pure",
        Parallel => "parallel",
        On => "on",
        With => "with",
        Yield => "yield",
        Move => "move",
        Static => "static",
        Macro => "macro",
        Union => "union",
        Kernel => "kernel",
        Import => "import",
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
            codes::MISPLACED_PUBLIC,
            codes::MISPLACED_RECEIVER,
            codes::STATEMENT_IN_INLINE_BLOCK,
            codes::MISPLACED_NAMED_ARGUMENT,
            codes::EXPECTED_TRAIT,
            codes::EXPECTED_MEMBER,
            codes::NESTED_EACH,
            codes::AMBIGUOUS_GENERIC_CALL,
            // `script-mode.md`'s one claim on this range.
            codes::SCRIPT_AND_MAIN,
            // `mcp-servers.md` §16.1's block. It is the one block in this
            // range that a sibling note allocated rather than the parser, so
            // it is the one most able to drift: the check that it has not is
            // that all nine are here and that `SC0199` is not.
            codes::TOOL_WITHOUT_DESCRIPTION,
            codes::GENERIC_TOOL,
            codes::BORROWED_TOOL_PARAMETER,
            codes::TOOL_WITH_RECEIVER,
            codes::PARAMETER_DOC_OUTSIDE_TOOL,
            codes::TOOL_SUMMARY_BLANK,
            codes::RESERVED_DECLARATION_WORD,
            codes::TOOL_NOT_AT_MODULE_LEVEL,
            codes::TOOL_WITHOUT_BODY,
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

    /// The `extern` block's codes live in the range `ffi-c-boundary.md` §8
    /// owns and `README.md` partitions, and nowhere else.
    ///
    /// The two tables also have to stay disjoint from each other: this file is
    /// the only one in the compiler that allocates out of two ranges, and the
    /// day someone copies a number from one list into the other is the day a
    /// UI test starts asserting the wrong phase.
    #[test]
    fn every_ffi_code_is_in_the_extern_range() {
        let all = [
            ffi_codes::UNKNOWN_EXTERN_ITEM,
            ffi_codes::MISSING_LIBRARY,
            ffi_codes::UNKNOWN_ABI,
            ffi_codes::EXTERN_NOT_UNSAFE,
            ffi_codes::UNION_WITHOUT_LAYOUT,
            ffi_codes::NOT_FFI_REPRESENTABLE,
            ffi_codes::ARRAY_IN_SIGNATURE,
            ffi_codes::HALF_PRECISION_BY_VALUE,
            ffi_codes::VARIADIC_FUNCTION,
        ];
        for code in all {
            assert!(
                (410..=449).contains(&code.0),
                "{code} is outside SC0410-SC0449, which ffi-c-boundary.md owns"
            );
        }
        let mut seen: Vec<u16> = all.iter().map(|c| c.0).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "two FFI diagnostics share a code");

        // Nothing in the syntax range may be reused here, and nothing here
        // may drift into the syntax range.
        for code in all {
            assert!(!(100..=199).contains(&code.0), "{code} is a syntax code");
        }
    }

    /// The three codes §8 of the FFI note names by number keep the meanings it
    /// gave them. A code is a promise to whoever reads the note.
    #[test]
    fn the_codes_the_note_named_keep_their_numbers() {
        assert_eq!(ffi_codes::ARRAY_IN_SIGNATURE.0, 421);
        assert_eq!(ffi_codes::HALF_PRECISION_BY_VALUE.0, 431);
        // `c-binding-coverage.md` §7 asks for this one by number rather than
        // taking it from its own block, so that it sits with the grammar it
        // belongs to.
        assert_eq!(ffi_codes::VARIADIC_FUNCTION.0, 434);
    }

    /// Recovery has to terminate on anything, including streams no lexer would
    /// ever produce. A parser that loops here hangs the whole compiler.
    #[test]
    fn parsing_terminates_on_garbage() {
        let garbage = vec![
            vec![TokenKind::Indent, TokenKind::Dedent, TokenKind::Dedent],
            vec![TokenKind::Colon, TokenKind::Colon, TokenKind::Colon],
            vec![TokenKind::Function, TokenKind::LParen, TokenKind::LParen],
            vec![TokenKind::Type, TokenKind::Indent, TokenKind::Indent],
            vec![TokenKind::Choice, TokenKind::Ident("E".into()), TokenKind::Colon],
            vec![TokenKind::Use, TokenKind::Comma, TokenKind::RParen],
            vec![TokenKind::Unknown('§'), TokenKind::Unknown('¿')],
            Vec::new(),
            // The heads of the two implementation forms, each cut off.
            vec![TokenKind::Ident("Doc".into()), TokenKind::Implements],
            vec![TokenKind::Ident("Doc".into()), TokenKind::Has],
            vec![TokenKind::Ident("Doc".into()), TokenKind::Of],
            vec![TokenKind::Ident("Doc".into()), TokenKind::Plus, TokenKind::Ident("x".into())],
            // Statements, expressions and patterns, each cut off mid-grammar.
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Let],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("a".into()),
                 TokenKind::Plus],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("a".into()),
                 TokenKind::Is, TokenKind::Ident("at".into())],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("a".into()),
                 TokenKind::DotDot],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Null],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Each, TokenKind::Giving],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Match],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Match,
                 TokenKind::Ident("x".into()), TokenKind::Colon, TokenKind::Newline,
                 TokenKind::Indent, TokenKind::Pipe, TokenKind::Pipe],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("f".into()),
                 TokenKind::LParen, TokenKind::Comma, TokenKind::Comma],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Newline,
                 TokenKind::Indent, TokenKind::Colon, TokenKind::Colon],
            vec![TokenKind::Interface, TokenKind::Ident("T".into()), TokenKind::Colon],
            vec![TokenKind::Interface, TokenKind::Ident("T".into()), TokenKind::Colon,
                 TokenKind::Newline, TokenKind::Indent, TokenKind::Type],
            // The pre-revision spellings, which recover by parsing on.
            vec![TokenKind::Ident("trait".into()), TokenKind::Ident("T".into()),
                 TokenKind::Colon],
            vec![TokenKind::Ident("Doc".into()), TokenKind::Has,
                 TokenKind::Ident("methods".into())],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::Ident("while".into()),
                 TokenKind::True],
            vec![TokenKind::Function, TokenKind::Ident("f".into()), TokenKind::LParen,
                 TokenKind::RParen, TokenKind::Colon, TokenKind::For, TokenKind::Each],
        ];
        for kinds in garbage {
            let tokens = stream(kinds);
            let (_module, _diagnostics) = parse_module(&tokens, FileId(0));
        }
    }

    /// §4.6's table, read back out of `binary_op`. A row that loses an operator
    /// to a typo would otherwise show up only as a wrong parse somewhere far
    /// away.
    #[test]
    fn the_precedence_table_matches_the_spec() {
        let rows: [&[TokenKind]; 10] = [
            &[TokenKind::Or],
            &[TokenKind::And],
            &[TokenKind::EqEq, TokenKind::NotEq, TokenKind::Lt, TokenKind::Gt,
              TokenKind::LtEq, TokenKind::GtEq],
            &[TokenKind::Pipe],
            &[TokenKind::Caret],
            &[TokenKind::Amp],
            &[TokenKind::Shl, TokenKind::Shr],
            &[TokenKind::Plus, TokenKind::Minus],
            &[TokenKind::Star, TokenKind::Slash, TokenKind::Percent, TokenKind::AtSign],
            &[TokenKind::StarStar],
        ];
        for (index, row) in rows.iter().enumerate() {
            let expected = index as u8 + 1;
            for kind in row.iter() {
                let (_, prec) = binary_op(kind).expect("every operator in the table has a rung");
                assert_eq!(prec, expected, "{} sits on the wrong rung", fixed_text(kind));
            }
        }
        // Assignment is a statement, not an operator, and `be` must never gain
        // a rung.
        assert!(binary_op(&TokenKind::Be).is_none(), "`be` is not in the precedence table");
    }

    /// §5.4: a word and its symbol are the same operator "by construction,
    /// not by a parser rule that could drift". They are, because `is`
    /// resolves to the symbol's own token before the operator table sees it.
    ///
    /// Revision 2 §1 leaves two rows here where §4.6 had six. The four it
    /// deleted are in `stale_comparison_phrase`, which reports rather than
    /// resolves, and the test below holds them to the same mapping so a fix
    /// cannot suggest the wrong symbol.
    #[test]
    fn is_and_is_not_are_their_symbols() {
        use TokenKind::*;
        let words: [(&[TokenKind], TokenKind); 2] = [(&[Is], EqEq), (&[Is, Not], NotEq)];
        for (spelling, symbol) in words {
            let tokens = stream(spelling.to_vec());
            let parser = Parser::new(&tokens, FileId(0));
            let (op, _, width) = parser.peek_binary_op().expect("`is` is always an operator");
            assert_eq!(width, spelling.len(), "{spelling:?} is the wrong number of words");
            assert_eq!(
                Some(op),
                binary_op(&symbol).map(|(op, _)| op),
                "{spelling:?} must be the operator `{}` is",
                fixed_text(&symbol)
            );
        }
    }

    /// The four phrases revision 2 §1 deleted, each still recognised for the
    /// one purpose of reporting the symbol that replaced it.
    #[test]
    fn every_stale_comparison_phrase_maps_to_its_symbol() {
        use TokenKind::*;
        let word = |w: &str| Ident(w.to_string());
        let phrases: [(Vec<TokenKind>, TokenKind); 4] = [
            (vec![Is, word("at"), word("least"), True], GtEq),
            (vec![Is, word("at"), word("most"), True], LtEq),
            (vec![Is, word("above"), True], Gt),
            (vec![Is, word("below"), True], Lt),
        ];
        for (spelling, symbol) in phrases {
            let tokens = stream(spelling.clone());
            let parser = Parser::new(&tokens, FileId(0));
            let (resolved, width) =
                parser.stale_comparison_phrase().expect("the phrase begins with `is`");
            assert_eq!(resolved, symbol, "{spelling:?} resolves to the wrong symbol");
            // The trailing operand is not part of the phrase.
            assert_eq!(width, spelling.len() - 1, "{spelling:?} is the wrong number of words");
        }
        // `n is above` with nothing after it is equality against a variable.
        let tokens = stream(vec![Is, word("above")]);
        let parser = Parser::new(&tokens, FileId(0));
        assert!(parser.stale_comparison_phrase().is_none(), "`is above <end>` is not a phrase");
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
