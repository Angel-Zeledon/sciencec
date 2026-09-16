//! The token contract between the lexer and the parser.
//!
//! This module is deliberately data-only: changing it breaks both sides at
//! once, so change it deliberately.

use science_diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// The `##` run immediately above this token, if there was one.
    ///
    /// Doc comments are trivia, not tokens: making them tokens would put them
    /// in every `match` over `TokenKind` in the parser, for a thing the
    /// grammar never mentions. They are carried on the token they document
    /// instead, which is where the parser wants them and nowhere else.
    ///
    /// `strings-formatting-and-docs.md` §5.3 requires this in F0 and says why
    /// it cannot wait: *"once the lexer discards them every tool downstream is
    /// built assuming they are gone."* They were discarded until now.
    ///
    /// The text is the run with its `##` markers and one following space
    /// removed, and its lines joined by line feeds. Nothing else is done to
    /// it: deciding what a doc comment *means* — a summary line, a body,
    /// Markdown — is §5.4's job and is not done here.
    pub doc: Option<String>,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span, doc: None }
    }

    /// The same token carrying a doc run.
    pub fn with_doc(kind: TokenKind, span: Span, doc: Option<String>) -> Self {
        Token { kind, span, doc }
    }
}

/// Integer and float types of explicit width, as they appear in a literal's
/// suffix (`42i32`, `2.5f32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumSuffix {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntBase {
    Dec,
    Hex,
    Oct,
    Bin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // --- Block structure --------------------------------------------------
    /// End of a logical line. Not emitted inside unclosed parentheses or
    /// brackets, where line continuation is implicit.
    Newline,
    /// Opens an indented block.
    Indent,
    /// Closes an indented block. One is emitted per level closed.
    Dedent,
    /// End of file, preceded by any outstanding `Dedent`s.
    Eof,

    // --- Literals ---------------------------------------------------------
    /// The value arrives with `_` separators and the base prefix already
    /// stripped.
    Int { value: u128, base: IntBase, suffix: Option<NumSuffix> },
    Float { value: f64, suffix: Option<NumSuffix> },
    /// Contents with escapes already resolved.
    Str(String),
    Char(char),

    Ident(String),

    // --- Keywords ---------------------------------------------------------
    Function,
    Let,
    Be,
    Mutable,
    If,
    Else,
    Match,
    For,
    Each,
    In,
    Loop,
    Return,
    Break,
    Continue,
    Type,
    Choice,
    Interface,
    Implements,
    Has,
    Of,
    Borrowed,
    Any,
    Use,
    Public,
    Const,
    Giving,
    True,
    False,

    /// `null`, the absence of a value in a nullable type (revision 2 §3.1).
    /// A literal and not a prelude value: `T?` is a type the compiler knows,
    /// so the thing that inhabits it has to be a token the lexer knows.
    Null,
    SelfValue, // self
    SelfType,  // Self
    As,
    Where,

    /// `extern "C"` — the foreign declaration block of the FFI note's §1.1.
    /// A real keyword rather than a reservation since the block exists.
    Extern,
    /// `unsafe`, on an `extern` block and on a block expression (§3 of the
    /// FFI note). Also a real keyword now, for the same reason.
    Unsafe,

    And,
    Or,
    Not,

    /// Identity. `is` alone is equality and `is not` is inequality; ordering
    /// is written with the symbols. Revision 2 §1 deleted the comparison
    /// phrases, so `is` is followed by at most one more word.
    Is,

    /// Words reserved for F1-F4. Using one today is an error, but it is never
    /// an identifier. Reserving them now avoids a breaking change later.
    Reserved(ReservedWord),

    // --- Punctuation ------------------------------------------------------
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semi,
    Dot,
    DotDot,   // .. — the half-open range of §4.5
    DotDotEq, // ..= — the inclusive one
    Arrow,    // -> — the return type of §4.4
    FatArrow, // =>
    Underscore,
    AtSign, // @
    Hash, // # — only if it ever stops meaning a comment; not emitted today
    /// `?` — the postfix presence test of revision 2 §3.1. `err?` is a `Bool`.
    /// The original design gave `?` to error propagation and then removed it,
    /// so the character carries one meaning and no history.
    Question,

    // --- Operators --------------------------------------------------------
    Plus,
    Minus,
    Star,
    StarStar, // ** — the power operator of §4.6, right-associative
    Slash,
    Percent,
    Amp,   // &
    Pipe,  // |
    Caret, // ^
    Shl,   // <<
    Shr,   // >>
    Eq,    // =
    EqEq,  // ==
    NotEq, // !=
    Lt,    // <
    Gt,    // >
    LtEq,  // <=
    GtEq,  // >=

    /// Text the lexer could not recognize. Emitted instead of aborting so the
    /// parser can carry on and report more than one error per pass.
    Unknown(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReservedWord {
    Agent,
    Tool,
    Prompt,
    Spawn,
    Send,
    Receive,
    Durable,
    Checkpoint,
    Resume,
    Supervise,
    Async,
    Await,
    Tensor,
    Shape,
    Model,
    /// An equation as a construct: dimensionally checked at compile time and
    /// rendered to the paper from the same definition that ran. Reserved now
    /// because reserving costs nothing today and is impossible later; see
    /// `docs/superpowers/design/equations.md`. `formula` was rejected for the
    /// name because `chem` already spells a chemical formula that way.
    Equation,
    Mod,
    Pure,
    Parallel,
    On,
    With,
    Yield,
    Assert,
    Move,
    Static,
    Macro,
    Union,
    Kernel,
    Import,
}

impl TokenKind {
    /// Maps a word to its keyword token, or `None` if it is an identifier.
    pub fn from_word(word: &str) -> Option<TokenKind> {
        use ReservedWord::*;
        use TokenKind::*;
        Some(match word {
            "function" => Function,
            "let" => Let,
            "be" => Be,
            "mutable" => Mutable,
            "if" => If,
            "else" => Else,
            "match" => Match,
            "for" => For,
            "each" => Each,
            "in" => In,
            "loop" => Loop,
            "return" => Return,
            "break" => Break,
            "continue" => Continue,
            "type" => Type,
            "choice" => Choice,
            "interface" => Interface,
            "implements" => Implements,
            "has" => Has,
            "of" => Of,
            "borrowed" => Borrowed,
            "any" => Any,
            "use" => Use,
            "public" => Public,
            "const" => Const,
            "giving" => Giving,
            "null" => Null,
            "true" => True,
            "false" => False,
            "self" => SelfValue,
            "Self" => SelfType,
            "as" => As,
            "where" => Where,

            "extern" => Extern,
            "unsafe" => Unsafe,

            "and" => And,
            "or" => Or,
            "not" => Not,

            "is" => Is,

            "agent" => Reserved(Agent),
            "tool" => Reserved(Tool),
            "prompt" => Reserved(Prompt),
            "spawn" => Reserved(Spawn),
            "send" => Reserved(Send),
            "receive" => Reserved(Receive),
            "durable" => Reserved(Durable),
            "checkpoint" => Reserved(Checkpoint),
            "resume" => Reserved(Resume),
            "supervise" => Reserved(Supervise),
            "async" => Reserved(Async),
            "await" => Reserved(Await),
            "tensor" => Reserved(Tensor),
            "equation" => Reserved(Equation),
            "shape" => Reserved(Shape),
            "model" => Reserved(Model),
            "mod" => Reserved(Mod),
            "pure" => Reserved(Pure),
            "parallel" => Reserved(Parallel),
            "on" => Reserved(On),
            "with" => Reserved(With),
            "yield" => Reserved(Yield),
            "assert" => Reserved(Assert),
            "move" => Reserved(Move),
            "static" => Reserved(Static),
            "macro" => Reserved(Macro),
            "union" => Reserved(Union),
            "kernel" => Reserved(Kernel),
            "import" => Reserved(Import),

            _ => return None,
        })
    }
}

impl IntBase {
    /// The numeric radix this base denotes.
    pub fn radix(self) -> u32 {
        match self {
            IntBase::Dec => 10,
            IntBase::Hex => 16,
            IntBase::Oct => 8,
            IntBase::Bin => 2,
        }
    }

    /// The base's name preceded by its article, for diagnostic messages.
    ///
    /// Carried here rather than assembled at the call site because "a octal"
    /// is the kind of wrong that survives a hundred code reviews.
    pub fn article_name(self) -> &'static str {
        match self {
            IntBase::Dec => "a decimal",
            IntBase::Hex => "a hexadecimal",
            IntBase::Oct => "an octal",
            IntBase::Bin => "a binary",
        }
    }

    /// The base's name, for diagnostic messages.
    pub fn name(self) -> &'static str {
        match self {
            IntBase::Dec => "decimal",
            IntBase::Hex => "hexadecimal",
            IntBase::Oct => "octal",
            IntBase::Bin => "binary",
        }
    }
}

impl NumSuffix {
    /// Maps a literal's suffix text to its type, or `None` if unrecognized.
    ///
    /// The counterpart to `TokenKind::from_word`: decoding a suffix is a
    /// property of the enum, not of whoever happens to be scanning.
    pub fn from_word(word: &str) -> Option<NumSuffix> {
        Some(match word {
            "i8" => NumSuffix::I8,
            "i16" => NumSuffix::I16,
            "i32" => NumSuffix::I32,
            "i64" => NumSuffix::I64,
            "u8" => NumSuffix::U8,
            "u16" => NumSuffix::U16,
            "u32" => NumSuffix::U32,
            "u64" => NumSuffix::U64,
            "f32" => NumSuffix::F32,
            "f64" => NumSuffix::F64,
            _ => return None,
        })
    }

    pub fn is_float(self) -> bool {
        matches!(self, NumSuffix::F32 | NumSuffix::F64)
    }
}
