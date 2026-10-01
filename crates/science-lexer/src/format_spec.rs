//! §2.1's format mini-language: what follows the `:` in `f"{x:>10.3f}"`.
//!
//! # The decision
//!
//! **The spec is parsed here, in the lexer, into a [`FormatSpec`] carried by
//! one token, and checked against the hole's type in `science-types`.** The
//! grammar is §2.1's, character for character:
//!
//! ```text
//! spec      := [[fill] align] [sign] ['#'] ['0'] [width] [grouping] ['.' precision] [code]
//! align     := '<' | '>' | '^'
//! sign      := '+' | '-' | ' '
//! grouping  := ',' | '_'
//! code      := 'f' | 'e' | 'E' | 'g' | 'G' | '%' | 'd' | 'x' | 'X' | 'o' | 'b' | 's'
//! ```
//!
//! # The reason
//!
//! §2.4 splits the check in two and the split falls on a phase boundary. A
//! spec that is not a sentence of the grammar — `.` with no digits, `F` for
//! `f`, `>` after the width — is wrong whatever it is applied to, and that is
//! `SC0173`, which §7 puts in the syntax band. A spec that *is* a sentence
//! but asks an `Int` for three decimal places is wrong only because of the
//! hole's type, which is `SC0274` and cannot be known before type checking.
//! The lexer already finds the end of the spec — it has to, to find the `}`
//! — so reading what it found costs one function; doing it later would mean
//! carrying the raw text through three phases to parse it in the fourth.
//!
//! # The cost
//!
//! **§2.5's dynamic width and precision are not built.** `{x:>{w}}` is
//! refused with `SC0173` naming the construct, rather than parsed: the
//! nested hole is an expression, and an expression inside a spec inside a
//! hole is a second level of the mode stack §1.5 already calls the largest
//! piece of work in the note. The static spec is the common case by far,
//! and the refusal says what is missing.
//!
//! **The `!i` conversion is refused for the same reason one layer down:**
//! `Inspect` (§3.2) is not declared, so there is nothing for it to select.
//! `!s` names the default and is accepted.

/// `<`, `>` or `^` — §2.1's `align` production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// `+`, `-` or `' '` — §2.1's `sign` production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sign {
    Plus,
    Minus,
    Space,
}

/// `,` or `_` — §2.1's `grouping` production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grouping {
    Comma,
    Underscore,
}

/// §2.1's `code` production, in the grammar's own order — which is also the
/// order the prelude's `Code` choice declares its variants in and the order
/// `science-rt`'s `ScienceCode` numbers them, so [`FormatCode::index`] is the
/// runtime's tag with nothing in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatCode {
    Fixed,
    Exp,
    ExpUpper,
    General,
    GeneralUpper,
    Percent,
    Decimal,
    Hex,
    HexUpper,
    Octal,
    Binary,
    Str,
}

impl FormatCode {
    /// Every code, in §2.1's order.
    pub const ALL: [FormatCode; 12] = [
        FormatCode::Fixed,
        FormatCode::Exp,
        FormatCode::ExpUpper,
        FormatCode::General,
        FormatCode::GeneralUpper,
        FormatCode::Percent,
        FormatCode::Decimal,
        FormatCode::Hex,
        FormatCode::HexUpper,
        FormatCode::Octal,
        FormatCode::Binary,
        FormatCode::Str,
    ];

    /// The character the code is written as.
    pub fn letter(self) -> char {
        match self {
            FormatCode::Fixed => 'f',
            FormatCode::Exp => 'e',
            FormatCode::ExpUpper => 'E',
            FormatCode::General => 'g',
            FormatCode::GeneralUpper => 'G',
            FormatCode::Percent => '%',
            FormatCode::Decimal => 'd',
            FormatCode::Hex => 'x',
            FormatCode::HexUpper => 'X',
            FormatCode::Octal => 'o',
            FormatCode::Binary => 'b',
            FormatCode::Str => 's',
        }
    }

    fn from_letter(c: char) -> Option<FormatCode> {
        FormatCode::ALL.into_iter().find(|code| code.letter() == c)
    }

    /// The position in §2.1's list, which is `science-rt`'s `ScienceCode`.
    pub fn index(self) -> u8 {
        FormatCode::ALL.iter().position(|&c| c == self).expect("every code is in ALL") as u8
    }

    /// `f e E g G %` — §2.4's first row: a float, or a `DisplayNumber`.
    pub fn is_float(self) -> bool {
        matches!(
            self,
            FormatCode::Fixed
                | FormatCode::Exp
                | FormatCode::ExpUpper
                | FormatCode::General
                | FormatCode::GeneralUpper
                | FormatCode::Percent
        )
    }

    /// `d x X o b` — §2.4's second row: an integer.
    pub fn is_integer(self) -> bool {
        matches!(
            self,
            FormatCode::Decimal
                | FormatCode::Hex
                | FormatCode::HexUpper
                | FormatCode::Octal
                | FormatCode::Binary
        )
    }

    /// §2.4's `#` row: *"the code is `x X o b f e g`"*. The upper-case
    /// spellings of `e` and `g` are the same rendering in another case and
    /// are read as included; `%` and `d` have no alternate form to select.
    pub fn takes_alternate(self) -> bool {
        matches!(
            self,
            FormatCode::Hex
                | FormatCode::HexUpper
                | FormatCode::Octal
                | FormatCode::Binary
                | FormatCode::Fixed
                | FormatCode::Exp
                | FormatCode::ExpUpper
                | FormatCode::General
                | FormatCode::GeneralUpper
        )
    }
}

/// One parsed spec. Every field is what the author wrote and nothing more:
/// §2.3's defaults are applied by the renderer, not filled in here, so a
/// consumer can still tell `{x}` from `{x:<}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatSpec {
    /// The fill character, when one was written before an alignment.
    pub fill: Option<char>,
    pub align: Option<Align>,
    pub sign: Option<Sign>,
    /// `#`.
    pub alternate: bool,
    /// `0` before the width: §2.3's *"zero-fill"*. Kept apart from `fill`
    /// because it is sign-aware — `{-7:04}` is `-007`, where `{-7:0>4}` is
    /// `00-7` — which is Python's behaviour and the one a column of signed
    /// numbers needs.
    pub zero: bool,
    pub width: Option<u32>,
    pub grouping: Option<Grouping>,
    pub precision: Option<u32>,
    pub code: Option<FormatCode>,
    /// The spec as written, after the `:` — for diagnostics.
    pub text: String,
}

impl FormatSpec {
    /// Whether nothing at all was written after the `:` — `f"{x:}"`, which
    /// is legal and means what `f"{x}"` means.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A spec that is not a sentence of §2.1's grammar. Offsets are bytes into
/// the text the parser was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError {
    pub start: usize,
    pub end: usize,
    pub message: String,
    pub label: String,
    pub note: Option<String>,
    /// A replacement for `start..end`, and what it does.
    pub fix: Option<(String, String)>,
}

const CODES: &str = "`f` `e` `E` `g` `G` `%` `d` `x` `X` `o` `b` `s`";
const ORDER: &str =
    "the order is [[fill]align][sign][#][0][width][grouping][.precision][code], e.g. `*^12,.3f`";

fn is_align(c: char) -> Option<Align> {
    match c {
        '<' => Some(Align::Left),
        '>' => Some(Align::Right),
        '^' => Some(Align::Center),
        _ => None,
    }
}

/// Parses the text after a hole's `:` — not including the `:` itself.
pub fn parse(text: &str) -> Result<FormatSpec, SpecError> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = |i: usize| chars.get(i).map(|&(_, c)| c);
    let offset = |i: usize| chars.get(i).map_or(text.len(), |&(o, _)| o);
    let end_of = |i: usize| chars.get(i).map_or(text.len(), |&(o, c)| o + c.len_utf8());
    let mut spec = FormatSpec {
        fill: None,
        align: None,
        sign: None,
        alternate: false,
        zero: false,
        width: None,
        grouping: None,
        precision: None,
        code: None,
        text: text.to_string(),
    };
    let mut i = 0;

    if let Some(align) = at(1).and_then(is_align) {
        let fill = at(0).expect("index 1 exists, so index 0 does");
        if fill == '{' {
            return Err(dynamic(offset(0), text, "fill"));
        }
        spec.fill = Some(fill);
        spec.align = Some(align);
        i = 2;
    } else if let Some(align) = at(0).and_then(is_align) {
        spec.align = Some(align);
        i = 1;
    }
    spec.sign = match at(i) {
        Some('+') => Some(Sign::Plus),
        Some('-') => Some(Sign::Minus),
        Some(' ') => Some(Sign::Space),
        _ => None,
    };
    if spec.sign.is_some() {
        i += 1;
    }
    if at(i) == Some('#') {
        spec.alternate = true;
        i += 1;
    }
    if at(i) == Some('0') {
        spec.zero = true;
        i += 1;
    }
    let (width, next) = digits(&chars, i, text, "width")?;
    spec.width = width;
    i = next;
    spec.grouping = match at(i) {
        Some(',') => Some(Grouping::Comma),
        Some('_') => Some(Grouping::Underscore),
        _ => None,
    };
    if spec.grouping.is_some() {
        i += 1;
    }
    if at(i) == Some('.') {
        let dot = i;
        let (precision, next) = digits(&chars, i + 1, text, "precision")?;
        if precision.is_none() {
            return Err(SpecError {
                start: offset(dot),
                end: end_of(dot),
                message: "a `.` with no precision after it".to_string(),
                label: "the digits after this `.` are missing".to_string(),
                note: Some(ORDER.to_string()),
                fix: None,
            });
        }
        spec.precision = precision;
        i = next;
    }
    if let Some(c) = at(i) {
        if let Some(code) = FormatCode::from_letter(c) {
            spec.code = Some(code);
            i += 1;
        } else if c.is_alphabetic() {
            return Err(unknown_code(c, offset(i), end_of(i)));
        }
    }
    if let Some(c) = at(i) {
        let start = offset(i);
        // `%%` is the one doubled code anybody writes, and it is C's: §7's
        // own example of an edit-distance-one fix.
        if c == '%' && spec.code == Some(FormatCode::Percent) {
            return Err(SpecError {
                start: offset(i - 1),
                end: end_of(i),
                message: "`%%` is not a format code".to_string(),
                label: "a percentage is one `%`".to_string(),
                note: None,
                fix: Some(("%".to_string(), "write one `%`".to_string())),
            });
        }
        if c == '{' {
            return Err(dynamic(start, text, "width or precision"));
        }
        let message = if spec.code.is_some() {
            format!("`{}` after the format code", c.escape_debug())
        } else {
            format!("`{}` cannot appear here in a format specification", c.escape_debug())
        };
        return Err(SpecError {
            start,
            end: end_of(i),
            message,
            label: "nothing of the spec can come here".to_string(),
            note: Some(ORDER.to_string()),
            fix: None,
        });
    }
    Ok(spec)
}

/// A run of decimal digits at `i`, as a `u32`, and the index after it.
fn digits(
    chars: &[(usize, char)],
    mut i: usize,
    text: &str,
    what: &str,
) -> Result<(Option<u32>, usize), SpecError> {
    let start = i;
    if chars.get(i).map(|&(_, c)| c) == Some('{') {
        return Err(dynamic(chars[i].0, text, what));
    }
    let mut value: u64 = 0;
    while let Some(&(_, c)) = chars.get(i) {
        let Some(d) = c.to_digit(10) else { break };
        value = value.saturating_mul(10).saturating_add(u64::from(d));
        i += 1;
    }
    if i == start {
        return Ok((None, i));
    }
    // A width past what a `u16` holds is a typo or a denial of service, not a
    // column; the line is drawn where the runtime's `i64` is in no danger
    // and well past any table.
    if value > 10_000 {
        let s = chars[start].0;
        let e = chars.get(i).map_or(text.len(), |&(o, _)| o);
        return Err(SpecError {
            start: s,
            end: e,
            message: format!("a {what} of {value} characters"),
            label: format!("a {what} is at most 10000"),
            note: None,
            fix: None,
        });
    }
    Ok((Some(value as u32), i))
}

fn dynamic(start: usize, text: &str, what: &str) -> SpecError {
    SpecError {
        start,
        end: text.len(),
        message: format!("a {what} computed at run time, which this compiler does not implement"),
        label: "§2.5's nested `{…}` is not built".to_string(),
        note: Some(
            "only a literal width and precision are read: write the number, or build the \
             padding with an `if`"
                .to_string(),
        ),
        fix: None,
    }
}

fn unknown_code(c: char, start: usize, end: usize) -> SpecError {
    // §7: *"Nearest legal code, when the edit distance is one (`F` → `f`)"*.
    // A one-character code at edit distance one from a legal code is the
    // other case of the same letter, when that case is a code.
    let flipped: Vec<char> =
        if c.is_uppercase() { c.to_lowercase().collect() } else { c.to_uppercase().collect() };
    let near = match flipped.as_slice() {
        [one] => FormatCode::from_letter(*one),
        _ => None,
    };
    let note = if c == 'n' {
        "`n`, the locale-aware code, is deliberately absent (§2.6): the same program would print \
         `3.14` on one machine and `3,14` on another"
            .to_string()
    } else {
        format!("the format codes are {CODES}")
    };
    SpecError {
        start,
        end,
        message: format!("`{c}` is not a format code"),
        label: "unknown format code".to_string(),
        note: Some(note),
        fix: near.map(|code| (code.letter().to_string(), "the nearest code is".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_in_order() {
        let spec = parse("*^+#012,.3f").unwrap();
        assert_eq!(spec.fill, Some('*'));
        assert_eq!(spec.align, Some(Align::Center));
        assert_eq!(spec.sign, Some(Sign::Plus));
        assert!(spec.alternate);
        assert!(spec.zero);
        assert_eq!(spec.width, Some(12));
        assert_eq!(spec.grouping, Some(Grouping::Comma));
        assert_eq!(spec.precision, Some(3));
        assert_eq!(spec.code, Some(FormatCode::Fixed));
    }

    #[test]
    fn a_fill_may_be_any_character_including_an_align_one() {
        assert_eq!(parse("<<8").unwrap().fill, Some('<'));
        assert_eq!(parse("·>8").unwrap().fill, Some('·'));
        assert_eq!(parse("0>8").unwrap().fill, Some('0'));
    }

    #[test]
    fn the_codes_index_in_the_grammars_order() {
        let letters: String = FormatCode::ALL.iter().map(|c| c.letter()).collect();
        assert_eq!(letters, "feEgG%dxXobs");
        assert_eq!(FormatCode::Str.index(), 11);
    }

    #[test]
    fn errors() {
        assert!(parse(".").unwrap_err().message.contains("no precision"));
        assert_eq!(parse("F").unwrap_err().fix, Some(("f".to_string(), "the nearest code is".to_string())));
        assert!(parse(".2>").is_err());
        assert!(parse("%%").unwrap_err().fix.is_some());
        assert!(parse(">{w}").unwrap_err().message.contains("run time"));
        assert!(parse("99999").is_err());
    }
}
