//! The token contract between the lexer and the parser.
//!
//! This module is deliberately data-only: changing it breaks both sides at
//! once, so change it deliberately.

use link_diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
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
    Fn,
    Let,
    Mut,
    If,
    Else,
    Match,
    For,
    In,
    While,
    Loop,
    Return,
    Break,
    Continue,
    Struct,
    Enum,
    Trait,
    Impl,
    Use,
    Mod,
    Pub,
    True,
    False,
    SelfValue, // self
    SelfType,  // Self
    As,
    Dyn,
    Where,
    And,
    Or,
    Not,

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
    Arrow,    // ->
    FatArrow, // =>
    Question, // ?
    Underscore,
    At,   // @
    Hash, // # — only if it ever stops meaning a comment; not emitted today

    // --- Operators --------------------------------------------------------
    Plus,
    Minus,
    Star,
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
}

impl TokenKind {
    /// Maps a word to its keyword token, or `None` if it is an identifier.
    pub fn from_word(word: &str) -> Option<TokenKind> {
        use ReservedWord::*;
        use TokenKind::*;
        Some(match word {
            "fn" => Fn,
            "let" => Let,
            "mut" => Mut,
            "if" => If,
            "else" => Else,
            "match" => Match,
            "for" => For,
            "in" => In,
            "while" => While,
            "loop" => Loop,
            "return" => Return,
            "break" => Break,
            "continue" => Continue,
            "struct" => Struct,
            "enum" => Enum,
            "trait" => Trait,
            "impl" => Impl,
            "use" => Use,
            "mod" => Mod,
            "pub" => Pub,
            "true" => True,
            "false" => False,
            "self" => SelfValue,
            "Self" => SelfType,
            "as" => As,
            "dyn" => Dyn,
            "where" => Where,
            "and" => And,
            "or" => Or,
            "not" => Not,

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
            "shape" => Reserved(Shape),
            "model" => Reserved(Model),

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
