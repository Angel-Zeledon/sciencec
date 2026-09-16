//! The lexer: source text in, token stream plus diagnostics out.
//!
//! Two things make this lexer more than a scanner:
//!
//! * **Indentation.** Blocks are delimited by indentation, so the lexer keeps a
//!   stack of open levels and synthesises `Indent` and `Dedent` tokens. Blank
//!   lines and comment-only lines do not take part in that computation.
//! * **Implicit line continuation.** Inside unclosed brackets, line breaks and
//!   indentation are ignored entirely, so a call can be spread over as many
//!   lines as it likes. A line beginning with `.` or with `where` continues
//!   the line above it too: the first is how a method chain is broken across
//!   lines (§4.6), the second how a long signature is (§4.4).
//!
//! The lexer never aborts. Anything it cannot make sense of becomes a
//! diagnostic plus whatever token keeps the stream usable, so a single pass can
//! report many problems instead of one.

use science_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span, Suggestion};

use crate::token::{IntBase, NumSuffix, Token, TokenKind};

// --- Lexical error codes (SC0001-SC0099) ---------------------------------

/// A character that is not part of the language.
const E_UNKNOWN_CHAR: Code = Code(1);
/// A tab in the indentation of a line.
const E_TAB_INDENT: Code = Code(3);
/// An indentation width matching no open level.
const E_INCONSISTENT_INDENT: Code = Code(4);
/// An integer literal that does not fit in `u128`.
const E_INT_OVERFLOW: Code = Code(5);
/// An escape sequence the lexer does not know.
const E_BAD_ESCAPE: Code = Code(6);
/// A string literal with no closing quote on its line.
const E_UNTERMINATED_STRING: Code = Code(7);
/// A character literal that is unterminated, empty or too long.
const E_UNTERMINATED_CHAR: Code = Code(8);
/// A numeric literal followed by something that is not a valid type suffix.
const E_BAD_SUFFIX: Code = Code(9);
/// A numeric literal whose digits do not fit its base.
const E_MALFORMED_NUMBER: Code = Code(10);
/// A float literal too large or too small to be represented.
const E_FLOAT_RANGE: Code = Code(11);
/// `==`, the spelling `syntax-revision-2.md` §1 removed in favour of `is`.
const E_EQ_SYMBOL: Code = Code(16);
/// `!=`, the spelling `syntax-revision-2.md` §1 removed in favour of `is not`.
const E_NOT_EQ_SYMBOL: Code = Code(17);

/// Turns `source` into a token stream.
///
/// The stream always ends with `Eof`, preceded by any pending `Dedent`s. The
/// returned `Diagnostics` may be non-empty even when the stream looks sane:
/// errors are recovered from, not propagated.
pub fn lex(file: FileId, source: &str) -> (Vec<Token>, Diagnostics) {
    let mut lexer = Lexer::new(file, source);
    lexer.run();
    (lexer.tokens, lexer.diags)
}

struct Lexer<'a> {
    file: FileId,
    src: &'a str,
    /// Byte offset of the cursor. Spans are byte offsets, so this is the only
    /// position the lexer tracks; line and column are the `SourceMap`'s job.
    pos: usize,
    tokens: Vec<Token>,
    diags: Diagnostics,
    /// Open indentation levels, in columns. Always starts with `0` and the
    /// bottom is never popped.
    indents: Vec<u32>,
    /// How many `(`, `[` or `{` are open. While this is greater than zero the
    /// line structure is suppressed.
    depth: u32,
    /// Whether the current logical line has produced a token that still needs
    /// a closing `Newline`.
    pending_newline: bool,
}

impl<'a> Lexer<'a> {
    fn new(file: FileId, src: &'a str) -> Self {
        Lexer {
            file,
            src,
            pos: 0,
            tokens: Vec::new(),
            diags: Diagnostics::new(),
            indents: vec![0],
            depth: 0,
            pending_newline: false,
        }
    }

    fn run(&mut self) {
        // The first line takes part in the indentation computation like any
        // other, so it goes through the same entry point.
        self.start_of_line();

        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                '\n' => {
                    self.bump();
                    if self.depth == 0 {
                        // §4.6: a chain broken by a leading `.` is still one
                        // logical line, so it gets neither a `Newline` nor the
                        // indentation treatment. Asking `pending_newline` first
                        // is what stops `.foo()` from continuing a line that
                        // produced no token to continue.
                        if !(self.pending_newline && self.eat_line_continuation()) {
                            if self.pending_newline {
                                self.emit(TokenKind::Newline, start, self.pos);
                            }
                            self.start_of_line();
                        }
                    }
                }
                ' ' | '\t' | '\r' => {
                    // Whitespace between tokens. Tabs are only rejected in the
                    // indentation, which `start_of_line` has already handled.
                    self.bump();
                }
                '#' => self.skip_comment(),
                '"' => self.string(),
                '\'' => self.character(),
                c if c.is_ascii_digit() => self.number(),
                c if is_ident_start(c) => self.word(),
                _ => self.operator(),
            }
        }

        self.finish();
    }

    /// Closes the last logical line and every open block, then emits `Eof`.
    fn finish(&mut self) {
        let end = self.src.len();
        if self.pending_newline {
            // A file that does not end in a newline still ends its last line,
            // so the parser sees one shape and not two.
            self.emit(TokenKind::Newline, end, end);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.emit(TokenKind::Dedent, end, end);
        }
        self.emit(TokenKind::Eof, end, end);
    }

    // --- Line structure --------------------------------------------------

    /// Handles the start of a physical line: measures its indentation, skips
    /// the lines that do not take part, and emits `Indent` / `Dedent`.
    ///
    /// On return the cursor sits on the first meaningful character of a line
    /// with content, or at the end of the file.
    fn start_of_line(&mut self) {
        loop {
            let line_start = self.pos;
            let mut width: u32 = 0;
            let mut tab: Option<usize> = None;

            while let Some(c) = self.peek() {
                match c {
                    ' ' => {
                        width += 1;
                        self.bump();
                    }
                    '\t' => {
                        // Remember the first tab; the rest of the line is
                        // measured anyway so recovery has something to work on.
                        if tab.is_none() {
                            tab = Some(self.pos);
                        }
                        self.bump();
                    }
                    // A lone `\r` before the line break carries no width.
                    '\r' => {
                        self.bump();
                    }
                    _ => break,
                }
            }

            match self.peek() {
                // Trailing whitespace at the end of the file: nothing to close
                // here, `finish` does the rest.
                None => return,
                // A blank line does not take part in the computation.
                Some('\n') => {
                    self.bump();
                }
                // Neither does a line holding nothing but a comment.
                Some('#') => {
                    self.skip_comment();
                    self.bump(); // the newline, if there is one
                }
                // A line with content: this is the one that sets the level.
                _ => {
                    self.apply_indent(line_start, width, tab);
                    return;
                }
            }
        }
    }

    fn apply_indent(&mut self, line_start: usize, width: u32, tab: Option<usize>) {
        let content = self.pos;

        if let Some(tab_pos) = tab {
            self.diags.push(
                Diagnostic::error(E_TAB_INDENT, "tabs are not allowed in indentation")
                    .with_label(Label::primary(
                        self.span(tab_pos, tab_pos + 1),
                        "tab character in the indentation",
                    ))
                    .with_note(
                        "Science does not interpret tabs: their width depends on how the file is \
                         displayed, so a block would mean different things to different readers. \
                         Indent with spaces only.",
                    ),
            );
            // Recovery: the line inherits the level currently on top of the
            // stack. Guessing a width would turn one mistake into a cascade of
            // SC0004 on every line that follows.
            return;
        }

        let top = *self.indents.last().expect("the stack always holds level 0");

        if width > top {
            self.indents.push(width);
            self.emit(TokenKind::Indent, line_start, content);
        } else if width < top {
            while *self.indents.last().expect("level 0 is never popped") > width {
                self.indents.pop();
                self.emit(TokenKind::Dedent, content, content);
            }
            if *self.indents.last().expect("level 0 is never popped") != width {
                self.diags.push(
                    Diagnostic::error(
                        E_INCONSISTENT_INDENT,
                        "this indentation matches no open level",
                    )
                    .with_label(Label::primary(
                        self.span(line_start, content),
                        format!("indented {width} columns"),
                    ))
                    .with_note(format!(
                        "the open levels are {}",
                        self.indents
                            .iter()
                            .map(|n| n.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                );
                // Recovery: take the width as a new level so the stream stays
                // balanced and the rest of the file lexes against something
                // sensible. This `Indent` is empty rather than covering the
                // indentation, because the `Dedent`s just emitted already sit
                // at `content` and token spans must never go backwards.
                self.indents.push(width);
                self.emit(TokenKind::Indent, content, content);
            }
        }
    }

    /// Whether the next line continues this one rather than starting a new one.
    ///
    /// Two things continue a line, and both are written at its start:
    ///
    /// * `.`, which breaks a method chain across lines (§4.6);
    /// * `where`, which breaks a long signature before its bounds (§4.4).
    ///
    /// Called with the cursor just past a line break. When the next line with
    /// content begins with one of them, the intervening blank and comment-only
    /// lines are consumed along with the indentation, the cursor is left on the
    /// continuing token, and the caller suppresses the line structure: no
    /// `Newline`, no `Indent`, no `Dedent`. Otherwise nothing is consumed and
    /// the caller carries on as usual.
    ///
    /// `..` is excluded deliberately: a range is not a chain link, and no line
    /// may begin with one. The indentation is not measured either — like the
    /// inside of a bracket, the leading whitespace of a continuation line means
    /// nothing, so a tab in it is not the SC0003 of `start_of_line`.
    fn eat_line_continuation(&mut self) -> bool {
        // Bytes, not chars: every character scanned for here is ASCII, and a
        // UTF-8 continuation byte can never be mistaken for one.
        let bytes = self.src.as_bytes();
        let mut probe = self.pos;
        loop {
            while matches!(bytes.get(probe), Some(b' ' | b'\t' | b'\r')) {
                probe += 1;
            }
            match bytes.get(probe) {
                // A blank line takes no part, exactly as it takes no part in
                // the indentation computation.
                Some(b'\n') => probe += 1,
                // Neither does a line holding nothing but a comment.
                Some(b'#') => {
                    while !matches!(bytes.get(probe), None | Some(b'\n')) {
                        probe += 1;
                    }
                }
                Some(b'.') => {
                    // `probe` is on a `.`, so `probe + 1` is a char boundary.
                    let continues =
                        self.src[probe + 1..].chars().next().is_some_and(is_ident_start);
                    if continues {
                        self.pos = probe;
                    }
                    return continues;
                }
                // `where` is the only word that continues a line. No statement
                // may begin with it, so recognising it here costs nothing.
                Some(b'w') if self.src[probe..].starts_with("where") => {
                    let after = self.src[probe + "where".len()..].chars().next();
                    let continues = !after.is_some_and(is_ident_continue);
                    if continues {
                        self.pos = probe;
                    }
                    return continues;
                }
                _ => return false,
            }
        }
    }

    /// Consumes `#` and the rest of the line, stopping before the line break.
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    // --- Words -----------------------------------------------------------

    fn word(&mut self) {
        let start = self.pos;
        self.eat_ident();
        let src = self.src;
        let text = &src[start..self.pos];
        let kind = if text == "_" {
            TokenKind::Underscore
        } else {
            TokenKind::from_word(text).unwrap_or_else(|| TokenKind::Ident(text.to_string()))
        };
        self.emit(kind, start, self.pos);
    }

    /// Consumes an identifier, assuming the cursor is on a valid start.
    fn eat_ident(&mut self) {
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                self.bump();
            } else {
                break;
            }
        }
    }

    // --- Numbers ---------------------------------------------------------

    fn number(&mut self) {
        let start = self.pos;

        // A base prefix, if any. `0` on its own stays decimal.
        let base = match (self.peek(), self.peek_at(1)) {
            (Some('0'), Some('x')) => IntBase::Hex,
            (Some('0'), Some('o')) => IntBase::Oct,
            (Some('0'), Some('b')) => IntBase::Bin,
            _ => IntBase::Dec,
        };

        if base == IntBase::Dec {
            self.decimal_number(start);
        } else {
            self.bump(); // 0
            self.bump(); // x / o / b
            self.prefixed_number(start, base);
        }
    }

    /// `0x`, `0o` and `0b` literals. Never floats.
    fn prefixed_number(&mut self, start: usize, base: IntBase) {
        let radix = base.radix();
        let mut digits = String::new();
        let mut bad_digit: Option<(usize, char)> = None;

        // Everything word-shaped is consumed so a stray digit does not get
        // split off into a second literal.
        while let Some(c) = self.peek() {
            if c == '_' {
                self.bump();
            } else if c.is_digit(radix) {
                digits.push(c);
                self.bump();
            } else if c.is_ascii_digit() {
                // A digit outside the base: keep it in the literal so the span
                // covers what was written.
                if bad_digit.is_none() {
                    bad_digit = Some((self.pos, c));
                }
                self.bump();
            } else {
                break;
            }
        }

        if let Some((pos, c)) = bad_digit {
            self.diags.push(
                Diagnostic::error(
                    E_MALFORMED_NUMBER,
                    format!("{} is not a valid digit in {} literal", quoted(c), base.article_name()),
                )
                .with_label(Label::primary(self.span(pos, pos + c.len_utf8()), "invalid digit")),
            );
        } else if digits.is_empty() {
            self.diags.push(
                Diagnostic::error(
                    E_MALFORMED_NUMBER,
                    format!("this {} literal has no digits", base.name()),
                )
                .with_label(Label::primary(
                    self.span(start, self.pos),
                    format!("expected at least one {} digit after the prefix", base.name()),
                )),
            );
        }

        let suffix = self.numeric_suffix(false);
        let value = self.digits_to_u128(&digits, radix, start);
        self.emit(TokenKind::Int { value, base, suffix }, start, self.pos);
    }

    /// Decimal literals, which may turn out to be floats.
    fn decimal_number(&mut self, start: usize) {
        let mut digits = String::new();
        self.eat_digits(&mut digits);

        let mut is_float = false;
        let mut literal = digits.clone();

        // A dot only makes a float if a digit follows it. `1.foo()` is field
        // access on an integer, and `1.` is an integer followed by a dot.
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.bump();
            literal.push('.');
            self.eat_digits(&mut literal);
        }

        // Likewise an exponent needs digits, otherwise the `e` is a suffix.
        if matches!(self.peek(), Some('e') | Some('E')) {
            let sign = matches!(self.peek_at(1), Some('+') | Some('-'));
            let digit_at = if sign { 2 } else { 1 };
            if self.peek_at(digit_at).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                literal.push('e');
                self.bump();
                if sign {
                    let s = self.bump().expect("checked above");
                    literal.push(s);
                }
                self.eat_digits(&mut literal);
            }
        }

        let suffix = self.numeric_suffix(is_float);
        // `1f32` is a float even though it has neither dot nor exponent.
        let is_float = is_float || suffix.is_some_and(NumSuffix::is_float);

        if is_float {
            let value = literal.parse::<f64>().unwrap_or(f64::NAN);
            // `str::parse` saturates to infinity instead of failing, and a
            // literal can never legitimately be infinite, so an infinite value
            // here always means the source asked for something unrepresentable.
            // Left undiagnosed it would silently become `inf` at runtime.
            //
            // There is no underflow branch: a minus sign lexes as a separate
            // unary operator, and a genuinely tiny literal like `1e-400`
            // parses to `0.0`, not to infinity. Nothing can reach it.
            if value.is_infinite() {
                self.diags.push(
                    Diagnostic::error(E_FLOAT_RANGE, "this float literal is out of range")
                        .with_label(Label::primary(self.span(start, self.pos), "value overflows"))
                        // Printed in exponent form: `f64::MAX` in decimal is 309
                        // digits, which tells the reader nothing.
                        .with_note(format!(
                            "the largest representable value is {:e}",
                            f64::MAX
                        )),
                );
            }
            self.emit(TokenKind::Float { value, suffix }, start, self.pos);
        } else {
            let value = self.digits_to_u128(&digits, 10, start);
            self.emit(TokenKind::Int { value, base: IntBase::Dec, suffix }, start, self.pos);
        }
    }

    /// Consumes decimal digits and `_` separators, pushing only the digits.
    fn eat_digits(&mut self, out: &mut String) {
        while let Some(c) = self.peek() {
            if c == '_' {
                self.bump();
            } else if c.is_ascii_digit() {
                out.push(c);
                self.bump();
            } else {
                break;
            }
        }
    }

    /// Reads the type suffix of a numeric literal, if there is one.
    ///
    /// Anything word-shaped that is not a valid suffix is consumed and
    /// reported, which keeps `42foo` a single broken literal rather than an
    /// integer next to an identifier.
    fn numeric_suffix(&mut self, is_float: bool) -> Option<NumSuffix> {
        let start = self.pos;
        if !self.peek().is_some_and(is_ident_start) {
            return None;
        }
        self.eat_ident();
        let src = self.src;
        let text = &src[start..self.pos];

        match NumSuffix::from_word(text) {
            Some(suffix) if is_float && !suffix.is_float() => {
                self.diags.push(
                    Diagnostic::error(
                        E_BAD_SUFFIX,
                        format!("`{text}` is not a valid suffix for a float literal"),
                    )
                    .with_label(Label::primary(self.span(start, self.pos), "invalid suffix"))
                    .with_note("float literals only take `f32` or `f64`"),
                );
                None
            }
            Some(suffix) if !is_float && suffix.is_float() => {
                // `1f32` is a float written without a dot; accept it.
                Some(suffix)
            }
            Some(suffix) => Some(suffix),
            None => {
                self.diags.push(
                    Diagnostic::error(
                        E_BAD_SUFFIX,
                        format!("`{text}` is not a valid numeric literal suffix"),
                    )
                    .with_label(Label::primary(self.span(start, self.pos), "unknown suffix"))
                    .with_note(
                        "the suffixes are `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, \
                         `f32` and `f64`",
                    ),
                );
                None
            }
        }
    }

    fn digits_to_u128(&mut self, digits: &str, radix: u32, start: usize) -> u128 {
        let mut value: u128 = 0;
        for c in digits.chars() {
            let d = c.to_digit(radix).expect("only valid digits reach here") as u128;
            match value.checked_mul(radix as u128).and_then(|v| v.checked_add(d)) {
                Some(v) => value = v,
                None => {
                    self.diags.push(
                        // The accumulator is a u128, but quoting its limit is
                        // unhelpful: §5.1 makes U64 the widest integer type, so
                        // that is the bound the programmer actually has.
                        Diagnostic::error(E_INT_OVERFLOW, "this integer literal is too large")
                        .with_label(Label::primary(
                            self.span(start, self.pos),
                            "value out of range",
                        ))
                        .with_note(format!(
                            "the widest integer type is `U64`, whose largest value is {}",
                            u64::MAX
                        )),
                    );
                    return 0;
                }
            }
        }
        value
    }

    // --- Strings and characters ------------------------------------------

    fn string(&mut self) {
        let start = self.pos;
        self.bump(); // opening quote
        let mut out = String::new();

        loop {
            match self.peek() {
                // A string never spans a line break: reporting it here keeps
                // the error local instead of swallowing the rest of the file.
                None | Some('\n') => {
                    self.diags.push(
                        Diagnostic::error(E_UNTERMINATED_STRING, "unterminated string literal")
                            .with_label(Label::primary(
                                self.span(start, self.pos),
                                "this string has no closing `\"`",
                            )),
                    );
                    break;
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => self.escape(&mut out),
                Some(c) => {
                    out.push(c);
                    self.bump();
                }
            }
        }

        self.emit(TokenKind::Str(out), start, self.pos);
    }

    fn character(&mut self) {
        let start = self.pos;
        self.bump(); // opening quote
        let mut buf = String::new();
        let mut terminated = false;

        loop {
            match self.peek() {
                None | Some('\n') => break,
                Some('\'') => {
                    self.bump();
                    terminated = true;
                    break;
                }
                Some('\\') => self.escape(&mut buf),
                Some(c) => {
                    buf.push(c);
                    self.bump();
                }
            }
        }

        let count = buf.chars().count();
        let value = if !terminated {
            self.char_error(start, "unterminated character literal", "this `'` is never closed");
            buf.chars().next().unwrap_or(REPLACEMENT)
        } else if count == 0 {
            self.char_error(start, "empty character literal", "expected a character here");
            REPLACEMENT
        } else if count > 1 {
            self.char_error(
                start,
                "a character literal holds exactly one character",
                format!("{count} characters here"),
            );
            buf.chars().next().expect("count > 1")
        } else {
            buf.chars().next().expect("count == 1")
        };

        self.emit(TokenKind::Char(value), start, self.pos);
    }

    fn char_error(&mut self, start: usize, message: &str, label: impl Into<String>) {
        let span = self.span(start, self.pos);
        self.diags.push(
            Diagnostic::error(E_UNTERMINATED_CHAR, message)
                .with_label(Label::primary(span, label)),
        );
    }

    /// Resolves one escape sequence into `out`.
    ///
    /// `\'` is accepted on top of the escapes the spec lists, because it is the
    /// only way to write a quote inside a character literal.
    fn escape(&mut self, out: &mut String) {
        let start = self.pos;
        self.bump(); // backslash

        let c = match self.peek() {
            // The backslash was the last thing on the line. The caller reports
            // the unterminated literal, which is the more useful message.
            None | Some('\n') => return,
            Some(c) => c,
        };

        match c {
            'n' => {
                out.push('\n');
                self.bump();
            }
            't' => {
                out.push('\t');
                self.bump();
            }
            'r' => {
                out.push('\r');
                self.bump();
            }
            '0' => {
                out.push('\0');
                self.bump();
            }
            '\\' => {
                out.push('\\');
                self.bump();
            }
            '"' => {
                out.push('"');
                self.bump();
            }
            '\'' => {
                out.push('\'');
                self.bump();
            }
            'u' => {
                self.bump();
                self.unicode_escape(start, out);
            }
            _ => {
                self.bump();
                self.diags.push(
                    Diagnostic::error(E_BAD_ESCAPE, format!("unknown escape sequence `\\{c}`"))
                        .with_label(Label::primary(self.span(start, self.pos), "unknown escape"))
                        .with_note(
                            "the escapes are `\\n`, `\\t`, `\\r`, `\\\\`, `\\\"`, `\\'`, `\\0` \
                             and `\\u{...}`",
                        ),
                );
                // Recovery: keep the character so the rest of the literal is
                // still worth something.
                out.push(c);
            }
        }
    }

    /// The `{...}` part of a `\u` escape, with the `\u` already consumed.
    fn unicode_escape(&mut self, start: usize, out: &mut String) {
        if self.peek() != Some('{') {
            self.bad_escape(start, "a `\\u` escape needs braces, as in `\\u{1F600}`");
            return;
        }
        self.bump();

        let mut hex = String::new();
        while let Some(c) = self.peek() {
            // Stop at anything that clearly ends the literal so a missing brace
            // does not eat the rest of the line.
            if c == '}' || c == '\n' || c == '"' || c == '\'' {
                break;
            }
            hex.push(c);
            self.bump();
        }

        if self.peek() != Some('}') {
            self.bad_escape(start, "this `\\u{` escape has no closing `}`");
            return;
        }
        self.bump();

        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
            Some(c) => out.push(c),
            None => self.bad_escape(
                start,
                "a `\\u` escape needs one to six hex digits naming a Unicode scalar value",
            ),
        }
    }

    fn bad_escape(&mut self, start: usize, note: &str) {
        let span = self.span(start, self.pos);
        self.diags.push(
            Diagnostic::error(E_BAD_ESCAPE, "malformed `\\u` escape")
                .with_label(Label::primary(span, "malformed escape"))
                .with_note(note.to_string()),
        );
    }

    // --- Operators and punctuation ---------------------------------------

    fn operator(&mut self) {
        let start = self.pos;
        let c = self.bump().expect("the caller checked there is a character");

        // Longest match wins: `<<` before `<`, `==` before `=`, `!=` before
        // `!`, `->` before `-`, and `..=` before `..` before `.`. The
        // characters that can begin a longer operator are `<`, `>`, `=`, `!`,
        // `-` and `.`; each one needs a lookahead arm before its plain arm.
        // `==` and `!=` are matched here even though §1 removed them from the
        // language: they are recognised in order to be *reported*, which is
        // what keeps a stale one from costing a second diagnostic.
        let kind = match c {
            '(' => self.open(TokenKind::LParen),
            '[' => self.open(TokenKind::LBracket),
            '{' => self.open(TokenKind::LBrace),
            ')' => self.close(TokenKind::RParen),
            ']' => self.close(TokenKind::RBracket),
            '}' => self.close(TokenKind::RBrace),

            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semi,
            // §4.5 writes ranges with dots. A range never follows a float,
            // because `number` stops at a `.` that no digit follows, so the
            // two dots always arrive here together.
            '.' if self.eat('.') => {
                if self.eat('=') {
                    TokenKind::DotDotEq
                } else {
                    TokenKind::DotDot
                }
            }
            '.' => TokenKind::Dot,
            '@' => TokenKind::AtSign,

            '+' => TokenKind::Plus,
            // `**` is one operator (§4.6: power, right-associative), not two
            // stars: Science has no prefix `*`, so nothing else could follow.
            '*' if self.eat('*') => TokenKind::StarStar,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '&' => TokenKind::Amp,
            '|' => TokenKind::Pipe,
            '^' => TokenKind::Caret,

            // `->` introduces a return type (§4.4). It has to be tried first:
            // otherwise `-` wins and the arrow lexes as subtraction followed
            // by a greater-than.
            '-' if self.eat('>') => TokenKind::Arrow,
            '-' => TokenKind::Minus,

            // `==` and `!=` were removed by `syntax-revision-2.md` §1: equality
            // is `is` and inequality is `is not`, and there is exactly one
            // spelling for each. The token is still emitted, because the
            // author's intent is never in doubt — reporting it and carrying on
            // means a stale `==` costs one diagnostic and no cascade, and the
            // tree is the one applying the fix would have produced.
            '=' if self.eat('=') => {
                let span = self.span(start, self.pos);
                self.diags.push(
                    Diagnostic::error(E_EQ_SYMBOL, "equality is written `is`")
                        .with_label(Label::primary(span, "`==` is not an operator in Science"))
                        .with_note(
                            "there is one spelling for each comparison: `is` and `is not` for \
                             identity, and `>`, `<`, `>=`, `<=` for order",
                        )
                        .with_suggestion(Suggestion {
                            span,
                            replacement: "is".to_string(),
                            message: "write the word instead".to_string(),
                        }),
                );
                TokenKind::EqEq
            }
            '=' if self.eat('>') => TokenKind::FatArrow,
            '=' => TokenKind::Eq,

            '<' if self.eat('<') => TokenKind::Shl,
            '<' if self.eat('=') => TokenKind::LtEq,
            '<' => TokenKind::Lt,

            '>' if self.eat('>') => TokenKind::Shr,
            '>' if self.eat('=') => TokenKind::GtEq,
            '>' => TokenKind::Gt,

            // `!=` is still one token, reported and recovered exactly as `==`
            // is above. It has to be matched before the lone `!` so that a
            // stale `!=` costs the one diagnostic about its own spelling and
            // not that one plus a complaint about its first character.
            '!' if self.eat('=') => {
                let span = self.span(start, self.pos);
                self.diags.push(
                    Diagnostic::error(E_NOT_EQ_SYMBOL, "inequality is written `is not`")
                        .with_label(Label::primary(span, "`!=` is not an operator in Science"))
                        .with_note(
                            "there is one spelling for each comparison: `is` and `is not` for \
                             identity, and `>`, `<`, `>=`, `<=` for order",
                        )
                        .with_suggestion(Suggestion {
                            span,
                            replacement: "is not".to_string(),
                            message: "write the words instead".to_string(),
                        }),
                );
                TokenKind::NotEq
            }

            // A lone `!` is the most likely typo from anyone arriving from C,
            // Rust or Python, so it gets its own message rather than being
            // told it is unrecognised — `!` begins nothing in Science, since
            // §1 removed `!=` as well.
            '!' => {
                self.diags.push(
                    Diagnostic::error(E_UNKNOWN_CHAR, "`!` is not an operator in Science")
                        .with_label(Label::primary(
                            self.span(start, self.pos),
                            "negation is spelled `not`",
                        ))
                        .with_note("`!` begins no operator in Science: inequality is `is not`")
                        .with_suggestion(Suggestion {
                            span: self.span(start, self.pos),
                            replacement: "not ".to_string(),
                            message: "if you meant to negate, write".to_string(),
                        }),
                );
                TokenKind::Unknown(c)
            }

            // `?` used to be error propagation and is now spelled `try`, which
            // goes *before* the expression. Anyone arriving from Rust or Swift
            // writes the postfix form first, so it gets the same treatment as
            // `!`: a message about what to write instead of "unrecognised".
            '?' => {
                self.diags.push(
                    Diagnostic::error(E_UNKNOWN_CHAR, "`?` is not an operator in Science")
                        .with_label(Label::primary(
                            self.span(start, self.pos),
                            "error propagation is not written with `?`",
                        ))
                        .with_note(
                            "error propagation is spelled `try`, and it goes before the \
                             expression: write `try f(x)` rather than `f(x)?`",
                        ),
                );
                TokenKind::Unknown(c)
            }

            _ => {
                self.diags.push(
                    Diagnostic::error(
                        E_UNKNOWN_CHAR,
                        format!("{} is not a character Science recognises", quoted(c)),
                    )
                    .with_label(Label::primary(
                        self.span(start, self.pos),
                        "unexpected character",
                    )),
                );
                TokenKind::Unknown(c)
            }
        };

        self.emit(kind, start, self.pos);
    }

    fn open(&mut self, kind: TokenKind) -> TokenKind {
        self.depth += 1;
        kind
    }

    fn close(&mut self, kind: TokenKind) -> TokenKind {
        // Saturating: an unmatched closer is the parser's problem to report,
        // and going negative here would break the line structure of the rest of
        // the file.
        self.depth = self.depth.saturating_sub(1);
        kind
    }

    // --- Cursor ----------------------------------------------------------

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Consumes `expected` if it is next, reporting whether it was.
    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.file, start as u32, end as u32)
    }

    fn emit(&mut self, kind: TokenKind, start: usize, end: usize) {
        // Only tokens with text of their own leave a line open; the synthetic
        // ones are what closes it.
        self.pending_newline = !matches!(
            kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
        );
        let span = self.span(start, end);
        self.tokens.push(Token::new(kind, span));
    }
}

/// What a broken character literal turns into, so the stream keeps its shape.
const REPLACEMENT: char = '\u{FFFD}';

/// Identifiers may start with any Unicode letter or `_`: keywords are English,
/// names are written in the author's language.
fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Wraps a character in backticks for a diagnostic message.
///
/// A backtick inside backticks reads as an empty pair, so it is quoted with
/// doubled delimiters instead. Rare, but it is exactly the character someone
/// mistypes when reaching for a quote.
fn quoted(c: char) -> String {
    if c == '`' {
        "`` ` ``".to_string()
    } else {
        format!("`{c}`")
    }
}
