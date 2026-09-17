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

use crate::token::{DocComment, IntBase, NumSuffix, Token, TokenKind};

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

// --- The `f"…"` block (SC0170-SC0177) ------------------------------------
//
// These six are in the **syntax** band and are emitted here, in the lexer.
// `docs/superpowers/design/README.md` allocates `SC0170`-`SC0177` to
// `strings-formatting-and-docs.md` §7, and its crate table says in as many
// words that *"a band is a topic, not a crate"* — `ffi-c-boundary.md`'s
// `SC0411`-`SC0434` are already emitted from the parser on that rule. The
// topic here is the shape of an interpolating literal, and the only phase that
// can see that shape is the one reading the characters: by the time the parser
// has a token stream, an unterminated interpolation has already become a token
// stream that stops.
//
// The two codes in the block that are **not** here are the two that need a
// scope. `SC0172` — a plain `"…"` whose every name resolves — cannot be
// decided before name resolution, and `SC0176` — a `##` run attached to
// nothing — is the parser's, since only the parser knows what a declaration
// is. Neither is implemented, and neither can be implemented here.

/// An interpolation with no closing `}` before the literal ends. §1.4.
const E_UNTERMINATED_INTERP: Code = Code(170);
/// A `}` inside an `f"…"` with no opener. §1.3.
const E_UNPAIRED_BRACE: Code = Code(171);
/// A format specification, which §2 specifies and this compiler does not
/// implement. See [`Lexer::interpolation`].
const E_FORMAT_SPEC: Code = Code(173);
/// `f"{}"` — an interpolation with nothing in it. §2.6.
const E_EMPTY_INTERP: Code = Code(174);
/// A `#` comment inside an interpolation. §1.4 restriction 3.
const E_STATEMENT_IN_INTERP: Code = Code(175);
/// `rf"…"`, or any other combination of literal prefixes. §1.3.
const E_PREFIX_COMBINATION: Code = Code(177);

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
    /// The `##` lines seen since the last token with text of its own.
    ///
    /// A run accumulates here and is handed to the next such token. The
    /// synthetic tokens — `Newline`, `Indent`, `Dedent` — pass over it without
    /// consuming it, which is what lets a doc comment survive the line break
    /// and the indent that always sit between it and the thing it documents.
    doc_run: Vec<String>,
    /// Where the lines in `doc_run` were written, merged as each arrives.
    ///
    /// Accumulated beside the text rather than recovered from it later: every
    /// line has had its marker and one space stripped, so by the time the run
    /// is handed over the source it came from is no longer in it.
    doc_span: Option<Span>,
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
            doc_run: Vec::new(),
            doc_span: None,
        }
    }

    fn run(&mut self) {
        // The first line takes part in the indentation computation like any
        // other, so it goes through the same entry point.
        self.start_of_line();

        while let Some(c) = self.peek() {
            if c == '\n' {
                let start = self.pos;
                self.bump();
                if self.depth == 0 {
                    // §4.6: a chain broken by a leading `.` is still one
                    // logical line, so it gets neither a `Newline` nor the
                    // indentation treatment. Asking `pending_newline` first is
                    // what stops `.foo()` from continuing a line that produced
                    // no token to continue.
                    if !(self.pending_newline && self.eat_line_continuation()) {
                        if self.pending_newline {
                            self.emit(TokenKind::Newline, start, self.pos);
                        }
                        self.start_of_line();
                    }
                }
                continue;
            }
            self.scan(c);
        }

        self.finish();
    }

    /// One token — everything except the line break.
    ///
    /// **Split out of [`Lexer::run`]** so that the interior of an
    /// interpolation is scanned by the same code that scans everything else,
    /// which is the whole of what `strings-formatting-and-docs.md` §1.5 asks
    /// for when it says the interior must be *tokenized*. The line break stays
    /// in `run`, because a line break is the one thing an interpolation cannot
    /// contain (§1.4 restriction 2) and the indentation machine has no
    /// business inside one.
    fn scan(&mut self, c: char) {
        match c {
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
    /// Consumes a comment to end of line, keeping it if it documents.
    ///
    /// `##` is documentation and `#` is an ordinary comment; a third `#`
    /// is still documentation, because `###` heads a Markdown section and
    /// refusing it would make the marker fight the language inside it.
    fn skip_comment(&mut self) {
        let start = self.pos;
        let is_doc = self.peek_at(1) == Some('#');
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
        if is_doc {
            // `##`, and the single space that conventionally follows it.
            // Any further indentation is the author's and is kept: a doc
            // comment holds code samples and §5.5 makes them compile.
            let line = &self.src[start + 2..self.pos];
            let line = line.strip_prefix(' ').unwrap_or(line);
            self.doc_run.push(line.trim_end().to_string());
            // The run's span grows by this line. It ends at the last character
            // the line actually shows, so that a caret under the run is as
            // wide as the run reads; the text above drops the same trailing
            // whitespace for the same reason.
            let end = start + self.src[start..self.pos].trim_end().len();
            let line_span = self.span(start, end);
            self.doc_span = Some(self.doc_span.map_or(line_span, |run| run.merge(line_span)));
        }
    }

    // --- Words -----------------------------------------------------------

    fn word(&mut self) {
        let start = self.pos;
        self.eat_ident();
        let src = self.src;
        let text = &src[start..self.pos];

        // §1.3's two literal prefixes, recognised here because a prefix is
        // word-shaped and the word is what has just been read.
        //
        // **The decision.** A word is a prefix only when it is spelled out of
        // `f` and `r`, is at most two characters, and a `"` follows it with no
        // space. Everything else stays an identifier followed by a string,
        // exactly as it was.
        //
        // **The reason.** §1.3 checks that `f` and `r` do not become reserved
        // words — calls are always parenthesised and there is no juxtaposition
        // operator, so `f"x"` can only be a prefixed literal. That argument
        // covers `f` and `r` and no other word: `extern"C"` is a real, if
        // unusual, spelling of a real declaration, and a rule that read any
        // word before a quote as a prefix would turn it into `SC0177`. The
        // two-character combinations are matched *in order to be refused*,
        // which is what `SC0177` is for and what keeps `rf"…"` from lexing as
        // an identifier `rf` beside a string.
        if self.peek() == Some('"') && is_prefix_word(text) {
            if text == "f" {
                self.fstring(start);
            } else if text == "r" {
                self.raw_string(start);
            } else {
                self.report_prefix_combination(start, text);
                // Recovery: read it as an ordinary literal, which is the one
                // of the three readings that always exists.
                self.string_at(start);
            }
            return;
        }

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
        self.string_at(start);
    }

    /// A `"…"`, whose token span begins at `start` — which is the quote for a
    /// plain literal and the prefix for a rejected one.
    fn string_at(&mut self, start: usize) {
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

    /// `r"..."` -- §1.3's raw literal: no escape processing, no interpolation.
    ///
    /// **The decision.** Every byte between the quotes is text, and the first
    /// `"` ends it.
    ///
    /// **The reason.** §1.3 buys this form for LaTeX, regular expressions and
    /// Windows paths -- the three places where `\` is data and doubling it is
    /// what people get wrong. It is also §1.2's suppression for `SC0172`: a
    /// raw literal is never inspected for a forgotten `f`.
    ///
    /// **The cost**, which §1.3 states rather than hides: a raw literal has no
    /// way to contain a `"`. `r#"..."#` is the known extension, it is
    /// source-compatible to add, and it is deferred until a caller needs it. A
    /// raw literal carries no marker into the token stream, because nothing
    /// after the lexer needs to know how a `String` was spelled.
    fn raw_string(&mut self, start: usize) {
        self.bump(); // opening quote
        let text_start = self.pos;
        loop {
            match self.peek() {
                None | Some('\n') => {
                    self.diags.push(
                        Diagnostic::error(E_UNTERMINATED_STRING, "unterminated string literal")
                            .with_label(Label::primary(
                                self.span(start, self.pos),
                                "this raw string has no closing `\"`",
                            ))
                            .with_note(
                                "a raw string is never continued onto the next line, and it \
                                 cannot contain a `\"` at all",
                            ),
                    );
                    let text = self.src[text_start..self.pos].to_string();
                    self.emit(TokenKind::Str(text), start, self.pos);
                    return;
                }
                Some('"') => {
                    let end = self.pos;
                    self.bump();
                    let text = self.src[text_start..end].to_string();
                    self.emit(TokenKind::Str(text), start, self.pos);
                    return;
                }
                Some(_) => {
                    self.bump();
                }
            }
        }
    }

    /// `f"..."` -- §1.3's interpolating literal, as the five tokens of
    /// [`TokenKind::FStrStart`] and its neighbours.
    ///
    /// **The decision.** The literal becomes `FStrStart`, then an alternation
    /// of `FStrText` runs and `InterpStart`-`InterpEnd` pairs whose interiors
    /// are ordinary tokens, then `FStrEnd`. The delimiters are always emitted,
    /// **including on every error path**, so the shape the parser matches on is
    /// the same whatever the text did.
    ///
    /// **The reason.** §1.4 admits an arbitrary expression in a hole and
    /// §1.5 says what that costs: the interior has to be tokenized, because
    /// `f"{m["a"]}"` has a `"` that does not end the literal and `f"{f"{x}"}"`
    /// has braces that are not the match. Emitting those tokens into the one
    /// stream -- rather than into a payload the parser re-lexes -- means the
    /// parser calls `parse_expr` on a hole exactly as it does everywhere else,
    /// and a caret inside a hole is a span in the file with no arithmetic
    /// between it and the source.
    ///
    /// **The cost.** `{` and `}` inside an `f"..."` are `InterpStart` and
    /// `InterpEnd`, not `LBrace` and `RBrace`, so a consumer matching on brace
    /// tokens does not see them; and §1.3's escaping rule -- a literal brace is
    /// `{{` -- exists here and in no other literal, which is exactly what
    /// §1.2 bought by marking the form.
    fn fstring(&mut self, start: usize) {
        self.bump(); // opening quote
        self.emit(TokenKind::FStrStart, start, self.pos);

        let mut text = String::new();
        let mut text_start = self.pos;
        let mut terminated = false;

        loop {
            match self.peek() {
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
                    terminated = true;
                    break;
                }
                Some('\\') => self.escape(&mut text),
                // §1.3: a literal brace inside `f"..."` is doubled. The pair is
                // consumed as one character of text, so `f"{{a}}"` is the four
                // characters `{a}` and holds no interpolation at all.
                Some('{') if self.peek_at(1) == Some('{') => {
                    text.push('{');
                    self.bump();
                    self.bump();
                }
                Some('}') if self.peek_at(1) == Some('}') => {
                    text.push('}');
                    self.bump();
                    self.bump();
                }
                Some('{') => {
                    self.flush_fragment(&mut text, text_start);
                    let closed = self.interpolation();
                    text_start = self.pos;
                    if !closed {
                        // The interpolation has reported. When what stopped it
                        // was this literal's own closing quote, consume it, so
                        // that the rest of the line lexes as the code it is
                        // rather than as the inside of a string.
                        if self.peek() == Some('"') {
                            terminated = true;
                        }
                        break;
                    }
                }
                Some('}') => {
                    let at = self.pos;
                    self.bump();
                    self.diags.push(
                        Diagnostic::error(
                            E_UNPAIRED_BRACE,
                            "a `}` with nothing it closes",
                        )
                        .with_label(Label::primary(
                            self.span(at, self.pos),
                            "this `}` closes no interpolation",
                        ))
                        .with_note("inside an `f\"…\"` a literal brace is written `}}`")
                        .with_suggestion(Suggestion {
                            span: self.span(at, self.pos),
                            replacement: "}}".to_string(),
                            message: "for a literal brace, write".to_string(),
                        }),
                    );
                    // Recovery: the brace was almost certainly meant as text.
                    text.push('}');
                }
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }

        self.flush_fragment(&mut text, text_start);
        let end_start = self.pos;
        if terminated {
            self.bump(); // closing quote
        }
        self.emit(TokenKind::FStrEnd, end_start, self.pos);
    }

    /// Emits the pending run of literal text, if there is any.
    fn flush_fragment(&mut self, text: &mut String, from: usize) {
        if text.is_empty() {
            return;
        }
        let value = std::mem::take(text);
        self.emit(TokenKind::FStrText(value), from, self.pos);
    }

    /// One `{...}`, with the cursor on the `{`.
    ///
    /// Returns whether the interpolation was closed. A `false` means the
    /// literal ended inside it and [`Lexer::fstring`] must stop without
    /// reporting the same mistake a second time: `SC0170` already names both
    /// the `{` and the place the literal ran out.
    ///
    /// **The format specification is refused, not parsed.** §2 specifies a
    /// mini-language after a `:` -- fill, alignment, sign, width, grouping,
    /// precision and a type code -- and §2.4 makes it checkable against the
    /// argument's static type. None of it is implemented. A `:` or a `!` at the
    /// hole's own bracket depth is therefore `SC0173`, which says so and names
    /// the one form that does work. Accepting a spec and ignoring it would
    /// print a number to seventeen digits where the author asked for three,
    /// which is §0's whole complaint; parsing the grammar and ignoring it
    /// would be worse, because it would look supported.
    ///
    /// **What the `:` costs, stated.** A `:` at the hole's depth ends the
    /// expression, so `f"{if flag: "yes" else: "no"}"` reads as the expression
    /// `if flag` with a specification after it. That is §2.1's grammar working
    /// as written rather than a shortcut taken here -- Python has the same seam
    /// -- and the answer is parentheses.
    fn interpolation(&mut self) -> bool {
        let brace = self.pos;
        self.bump(); // `{`
        self.emit(TokenKind::InterpStart, brace, self.pos);

        let outer = self.depth;
        // A hole suspends the line structure exactly as an unclosed bracket
        // does; §1.5 asks for this and the mechanism already existed.
        self.depth = outer + 1;

        let before = self.tokens.len();
        let mut closed = false;
        let mut spec: Option<usize> = None;

        loop {
            match self.peek() {
                None | Some('\n') => {
                    let at = self.pos;
                    self.diags.push(
                        Diagnostic::error(
                            E_UNTERMINATED_INTERP,
                            "this interpolation has no closing `}`",
                        )
                        .with_label(Label::primary(
                            self.span(brace, brace + 1),
                            "this `{` opens an interpolation",
                        ))
                        .with_label(Label::secondary(self.span(at, at), "the literal ends here"))
                        .with_note("inside an `f\"…\"` a literal brace is written `{{`"),
                    );
                    break;
                }
                Some('}') if self.depth == outer + 1 => {
                    closed = true;
                    break;
                }
                // A `"` inside a hole normally opens a nested literal, which
                // is what makes `f"{m[\"a\"]}"` work at all. When nothing on
                // the rest of the line closes it, it is not a nested literal:
                // it is this literal's own closing quote, met early because
                // the `}` is missing.
                //
                // **The reason this branch exists at all** is §1.5's second
                // named cost, *"a single missing `}` produces fifty cascading
                // diagnostics"*. Without it, `f"{n"` reports an unterminated
                // *string* from the interior scan and then an unterminated
                // *interpolation* from here, in that order — two diagnostics
                // for one missing character, the first of which names a string
                // the author never wrote. One line of lookahead buys exactly
                // one diagnostic, and it is `SC0170`.
                Some('"') if !self.quote_pairs_on_this_line() => {
                    let at = self.pos;
                    self.diags.push(
                        Diagnostic::error(
                            E_UNTERMINATED_INTERP,
                            "this interpolation has no closing `}`",
                        )
                        .with_label(Label::primary(
                            self.span(brace, brace + 1),
                            "this `{` opens an interpolation",
                        ))
                        .with_label(Label::secondary(self.span(at, at + 1), "the literal ends here"))
                        .with_note("inside an `f\"…\"` a literal brace is written `{{`"),
                    );
                    break;
                }
                // §1.4 restriction 3. A `#` would swallow the closing brace and
                // the quote, so it is reported here rather than allowed to turn
                // the rest of the line into a comment.
                Some('#') if self.depth == outer + 1 && spec.is_none() => {
                    let at = self.pos;
                    self.diags.push(
                        Diagnostic::error(
                            E_STATEMENT_IN_INTERP,
                            "a `#` comment inside an interpolation",
                        )
                        .with_label(Label::primary(
                            self.span(at, at + 1),
                            "this would comment out the rest of the literal",
                        ))
                        .with_note(
                            "an interpolation holds one expression: no `let`, no statement, no \
                             comment and no line break",
                        ),
                    );
                    break;
                }
                Some(':' | '!') if self.depth == outer + 1 && spec.is_none() => {
                    spec = Some(self.pos);
                    self.bump();
                }
                Some(c) => {
                    if spec.is_some() {
                        // Inside a specification nothing is tokenized: it is
                        // characters until the brace that ends the hole.
                        self.bump();
                    } else {
                        self.scan(c);
                    }
                }
            }
        }

        let interior = self.tokens.len() > before;
        let end = self.pos;
        if closed {
            self.bump(); // `}`
        }
        self.depth = outer;

        if let Some(at) = spec {
            self.diags.push(
                Diagnostic::error(
                    E_FORMAT_SPEC,
                    "a format specification, which this compiler does not implement",
                )
                .with_label(Label::primary(
                    self.span(at, end),
                    "no part of the format mini-language is built",
                ))
                .with_note(
                    "`f\"{value}\"` is the whole of what this compiler reads: an expression, \
                     rendered by its `Display`. The fill, alignment, sign, width, grouping, \
                     precision and type codes, and the `!i` conversion, are specified and unbuilt",
                ),
            );
        } else if !interior {
            self.diags.push(
                Diagnostic::error(E_EMPTY_INTERP, "an interpolation with nothing in it")
                    .with_label(Label::primary(self.span(brace, self.pos), "name the value"))
                    .with_note(
                        "there are no positional fields: `print` takes one value, so `{}` has no \
                         argument list to refer to",
                    ),
            );
        }

        self.emit(TokenKind::InterpEnd, end, self.pos);
        closed
    }

    /// Whether a `"` at the cursor has a partner before the line ends.
    ///
    /// Escapes are honoured, because a nested literal may itself contain a
    /// quote and then the partner is the third one, not the second.
    fn quote_pairs_on_this_line(&self) -> bool {
        let mut chars = self.src[self.pos..].chars();
        chars.next(); // the quote under the cursor
        while let Some(c) = chars.next() {
            match c {
                '\n' => return false,
                '\\' => {
                    chars.next();
                }
                '"' => return true,
                _ => {}
            }
        }
        false
    }

    /// §1.3: the prefixes do not combine.
    fn report_prefix_combination(&mut self, start: usize, text: &str) {
        let span = self.span(start, start + text.len());
        self.diags.push(
            Diagnostic::error(
                E_PREFIX_COMBINATION,
                format!("`{text}\"…\"` is not a string literal"),
            )
            .with_label(Label::primary(span, "the literal prefixes do not combine"))
            .with_note(
                "there are exactly three literal kinds: `\"…\"` is text, `f\"…\"` \
                 interpolates, and `r\"…\"` processes no escapes. A raw interpolating string \
                 is a fourth set of rules for a case nobody has demonstrated",
            ),
        );
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
            // One character, one meaning. `?` is postfix in expressions
            // (`err?`) and suffix in types (`Error?`), and the two never
            // collide because a type and an expression are never both legal
            // in the same position. There is no `??`, no `?.` and no `?:`.
            '?' => TokenKind::Question,

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
        let has_text = !matches!(
            kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
        );
        self.pending_newline = has_text;
        let span = self.span(start, end);
        // A run reaches the next token with text of its own; the synthetic
        // tokens between a comment and the declaration leave it alone.
        let doc = if has_text && !self.doc_run.is_empty() {
            // The run's own span travels with it, not this token's: `SC0194`
            // and `SC0195` are about the `##` lines, and the declaration
            // underneath is one line below the text they ask the reader to
            // edit. The fallback is unreachable — `doc_span` is set by the
            // same push that fills `doc_run` — and is written rather than
            // unwrapped because a panic in the lexer is never the right answer
            // to a malformed file.
            let run = self.doc_span.take().unwrap_or(span);
            let text = std::mem::take(&mut self.doc_run).join("\n");
            Some(DocComment { text, span: run })
        } else {
            None
        };
        self.tokens.push(Token::with_doc(kind, span, doc));
    }
}

/// What a broken character literal turns into, so the stream keeps its shape.
const REPLACEMENT: char = '\u{FFFD}';

/// Identifiers may start with any Unicode letter or `_`: keywords are English,
/// names are written in the author's language.
/// Whether a word that a `"` immediately follows is one of §1.3's prefixes, or
/// a combination of them that exists in order to be refused.
///
/// The set is closed at `f`, `r` and the four two-letter words over them. It is
/// deliberately not "any short word": `extern"C"` must keep lexing as a keyword
/// beside a string.
fn is_prefix_word(text: &str) -> bool {
    !text.is_empty() && text.len() <= 2 && text.bytes().all(|b| b == b'f' || b == b'r')
}

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
