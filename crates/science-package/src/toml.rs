//! The TOML the manifest is written in, parsed with a span on everything.
//!
//! `package-manager.md` Decision 12: *"The manifest is TOML … The compiler's
//! own parser is **normative**."* This is that parser.
//!
//! # What it reads
//!
//! **Decision. The subset of TOML 1.0 a manifest uses, and an `SP0001` naming
//! the construct for everything else.** Comments; `[table]` and `[[array]]`
//! headers with dotted names; bare, basic-quoted and literal-quoted keys,
//! dotted keys included; basic strings with TOML's escapes, literal strings,
//! integers, booleans, arrays that may span lines, and inline tables.
//!
//! **Reason.** Every value §4.2's example manifest writes is one of those, and
//! a parser whose every accepted construct is one somebody needed is a parser
//! whose every accepted construct is tested. What is left out is the part of
//! TOML that is large and that no manifest key takes: floats, the four
//! date-time forms, and multi-line strings.
//!
//! **Cost, named because Decision 12 makes it a conformance question.** A
//! manifest that is valid TOML and uses one of those is refused here, with a
//! message that says which construct and that the refusal is this parser's.
//! It is never misread: a construct outside the subset is an error, never a
//! guess. The day a key needs one, it is added here, to the normative parser,
//! and `data.toml` is measured against it.
//!
//! Spans are byte offsets into the manifest, registered with the source map
//! like any `.science` file, so an `SP` diagnostic renders against
//! `science.toml` exactly as an `SC` one renders against a program — §8.3
//! rule 2, *"always point at a file and a line"*.

use science_diagnostics::{Code, Diagnostic, FileId, Label, Span};

/// A key, as written, with where it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Bool(bool),
    Array(Vec<Spanned>),
    /// An inline table, `{ path = "../x" }`. Keys are single segments here: a
    /// dotted key inside braces is refused by the parser.
    Table(Vec<Entry>),
}

impl Value {
    /// What the value is, in the words a diagnostic uses.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::String(_) => "a string",
            Value::Integer(_) => "an integer",
            Value::Bool(_) => "a boolean",
            Value::Array(_) => "an array",
            Value::Table(_) => "a table",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub value: Value,
    pub span: Span,
}

/// `key = value`, where the key may be dotted.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: Vec<Key>,
    pub value: Spanned,
}

/// A `[header]` or `[[header]]` and the entries under it.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub header: Vec<Key>,
    /// `[[header]]` rather than `[header]`.
    pub array: bool,
    pub span: Span,
    pub entries: Vec<Entry>,
}

/// A whole file. `root` is what comes before the first header.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub root: Vec<Entry>,
    pub sections: Vec<Section>,
}

/// Parses `text`, or reports the first thing that is not TOML as `SP0001`.
///
/// **One error, not a list.** A TOML file is short, and the parser cannot
/// recover past a malformed value without guessing where it ends — which is
/// how a second, invented error is produced. The manifest is fixed one line at
/// a time anyway.
pub fn parse(file: FileId, text: &str) -> Result<Document, Diagnostic> {
    let mut parser = Parser { file, text, bytes: text.as_bytes(), pos: 0 };
    let document = parser.document()?;
    check_duplicates(file, &document)?;
    Ok(document)
}

struct Parser<'a> {
    file: FileId,
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

fn invalid(file: FileId, start: usize, end: usize, message: &str, label: &str) -> Diagnostic {
    let end = end.max(start);
    Diagnostic::error(Code::sp(1), format!("`science.toml` is not valid TOML: {message}"))
        .with_label(Label::primary(Span::new(file, start as u32, end as u32), label))
}

impl<'a> Parser<'a> {
    fn error(&self, start: usize, message: &str, label: &str) -> Diagnostic {
        let end = (start + 1).min(self.bytes.len()).max(start);
        invalid(self.file, start, end, message, label)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn span(&self, start: usize) -> Span {
        Span::new(self.file, start as u32, self.pos as u32)
    }

    /// Spaces and tabs, and nothing that ends a line.
    fn blanks(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    /// Blanks, comments and newlines: everything between two statements.
    fn trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n') => self.pos += 1,
                Some(b'\r') if self.bytes.get(self.pos + 1) == Some(&b'\n') => self.pos += 2,
                Some(b'#') => self.comment(),
                _ => return,
            }
        }
    }

    fn comment(&mut self) {
        while let Some(byte) = self.peek() {
            if byte == b'\n' {
                return;
            }
            self.pos += 1;
        }
    }

    /// After a statement: blanks, an optional comment, then a newline or the
    /// end of the file. TOML puts one statement on a line.
    fn end_of_line(&mut self) -> Result<(), Diagnostic> {
        self.blanks();
        if self.peek() == Some(b'#') {
            self.comment();
        }
        match self.peek() {
            None | Some(b'\n') => Ok(()),
            Some(b'\r') if self.bytes.get(self.pos + 1) == Some(&b'\n') => Ok(()),
            _ => Err(self.error(
                self.pos,
                "expected the end of the line",
                "a statement ends here, and this continues it",
            )),
        }
    }

    fn document(&mut self) -> Result<Document, Diagnostic> {
        let mut document = Document::default();
        loop {
            self.trivia();
            let Some(byte) = self.peek() else { return Ok(document) };
            if byte == b'[' {
                let start = self.pos;
                self.pos += 1;
                let array = self.peek() == Some(b'[');
                if array {
                    self.pos += 1;
                }
                self.blanks();
                let header = self.dotted_key()?;
                self.blanks();
                let close: &[u8] = if array { b"]]" } else { b"]" };
                if !self.bytes[self.pos..].starts_with(close) {
                    let wanted = if array { "`]]`" } else { "`]`" };
                    return Err(self.error(
                        self.pos,
                        &format!("a table header is closed by {wanted}"),
                        "expected the header to end here",
                    ));
                }
                self.pos += close.len();
                let span = self.span(start);
                self.end_of_line()?;
                document.sections.push(Section { header, array, span, entries: Vec::new() });
            } else {
                let entry = self.entry()?;
                self.end_of_line()?;
                match document.sections.last_mut() {
                    Some(section) => section.entries.push(entry),
                    None => document.root.push(entry),
                }
            }
        }
    }

    fn entry(&mut self) -> Result<Entry, Diagnostic> {
        let key = self.dotted_key()?;
        self.blanks();
        if self.peek() != Some(b'=') {
            return Err(self.error(self.pos, "a key is followed by `=`", "expected `=` here"));
        }
        self.pos += 1;
        self.blanks();
        let value = self.value()?;
        Ok(Entry { key, value })
    }

    fn dotted_key(&mut self) -> Result<Vec<Key>, Diagnostic> {
        let mut keys = vec![self.simple_key()?];
        loop {
            let save = self.pos;
            self.blanks();
            if self.peek() != Some(b'.') {
                self.pos = save;
                return Ok(keys);
            }
            self.pos += 1;
            self.blanks();
            keys.push(self.simple_key()?);
        }
    }

    fn simple_key(&mut self) -> Result<Key, Diagnostic> {
        let start = self.pos;
        match self.peek() {
            Some(b'"') => {
                let name = self.basic_string()?;
                Ok(Key { name, span: self.span(start) })
            }
            Some(b'\'') => {
                let name = self.literal_string()?;
                Ok(Key { name, span: self.span(start) })
            }
            _ => {
                while matches!(self.peek(), Some(b) if b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                {
                    self.pos += 1;
                }
                if self.pos == start {
                    return Err(self.error(start, "expected a key", "a key goes here"));
                }
                Ok(Key { name: self.text[start..self.pos].to_string(), span: self.span(start) })
            }
        }
    }

    fn value(&mut self) -> Result<Spanned, Diagnostic> {
        let start = self.pos;
        let value = match self.peek() {
            Some(b'"') => {
                if self.bytes[self.pos..].starts_with(b"\"\"\"") {
                    return Err(self.unsupported(start, "a multi-line string"));
                }
                Value::String(self.basic_string()?)
            }
            Some(b'\'') => {
                if self.bytes[self.pos..].starts_with(b"'''") {
                    return Err(self.unsupported(start, "a multi-line string"));
                }
                Value::String(self.literal_string()?)
            }
            Some(b'[') => self.array()?,
            Some(b'{') => self.inline_table()?,
            Some(b't') if self.bytes[self.pos..].starts_with(b"true") => {
                self.pos += 4;
                Value::Bool(true)
            }
            Some(b'f') if self.bytes[self.pos..].starts_with(b"false") => {
                self.pos += 5;
                Value::Bool(false)
            }
            Some(b) if b.is_ascii_digit() || b == b'+' || b == b'-' => self.number()?,
            Some(b'i' | b'n') if matches!(&self.text[self.pos..], t if t.starts_with("inf") || t.starts_with("nan")) => {
                return Err(self.unsupported(start, "a floating-point value"))
            }
            _ => return Err(self.error(start, "expected a value", "a value goes here")),
        };
        Ok(Spanned { value, span: self.span(start) })
    }

    /// A TOML construct this parser deliberately does not read — see the
    /// module's §"What it reads".
    fn unsupported(&self, start: usize, what: &str) -> Diagnostic {
        self.error(
            start,
            &format!("{what} is TOML, and is not used by any manifest key, so this parser does not read it"),
            "not read by `science.toml`'s parser",
        )
    }

    fn number(&mut self) -> Result<Value, Diagnostic> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        while matches!(
            self.peek(),
            Some(b) if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-' | b'+')
        ) {
            self.pos += 1;
        }
        let written = &self.text[start..self.pos];
        let digits: String = written.chars().filter(|c| *c != '_').collect();
        if let Ok(n) = digits.parse::<i64>() {
            return Ok(Value::Integer(n));
        }
        let body = written.trim_start_matches(['+', '-']);
        let what = if written.contains(':') || body.contains('-') {
            Some("a date or time")
        } else if body.contains(['.', 'e', 'E']) || body == "inf" || body == "nan" {
            Some("a floating-point value")
        } else {
            None
        };
        match what {
            Some(what) => Err(self.unsupported(start, what)),
            None => Err(invalid(self.file, start, self.pos, "this is not an integer", "not a number TOML reads")),
        }
    }

    fn basic_string(&mut self) -> Result<String, Diagnostic> {
        let open = self.pos;
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error(open, "this string is never closed", "the string opens here"));
            };
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\n' | b'\r' => {
                    return Err(self.error(open, "this string is never closed", "the string opens here"))
                }
                b'\\' => {
                    let at = self.pos;
                    self.pos += 1;
                    let escaped = match self.peek() {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'n') => '\n',
                        Some(b't') => '\t',
                        Some(b'r') => '\r',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'u') | Some(b'U') => {
                            let width = if self.peek() == Some(b'u') { 4 } else { 8 };
                            let digits = self.text.get(self.pos + 1..self.pos + 1 + width).unwrap_or("");
                            let code = u32::from_str_radix(digits, 16).ok().and_then(char::from_u32);
                            let Some(c) = code.filter(|_| digits.len() == width) else {
                                return Err(self.error(at, "this is not a valid unicode escape", "here"));
                            };
                            self.pos += width;
                            c
                        }
                        _ => return Err(self.error(at, "this is not an escape TOML defines", "here")),
                    };
                    self.pos += 1;
                    out.push(escaped);
                }
                _ => {
                    let c = self.text[self.pos..].chars().next().expect("in bounds");
                    out.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    fn literal_string(&mut self) -> Result<String, Diagnostic> {
        let open = self.pos;
        self.pos += 1;
        let start = self.pos;
        loop {
            match self.peek() {
                None | Some(b'\n') | Some(b'\r') => {
                    return Err(self.error(open, "this string is never closed", "the string opens here"))
                }
                Some(b'\'') => {
                    let text = self.text[start..self.pos].to_string();
                    self.pos += 1;
                    return Ok(text);
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn array(&mut self) -> Result<Value, Diagnostic> {
        let open = self.pos;
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.trivia();
            match self.peek() {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                None => return Err(self.error(open, "this array is never closed", "the array opens here")),
                _ => {}
            }
            items.push(self.value()?);
            self.trivia();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {}
                _ => {
                    return Err(self.error(
                        self.pos,
                        "array elements are separated by `,`",
                        "expected `,` or `]` here",
                    ))
                }
            }
        }
    }

    fn inline_table(&mut self) -> Result<Value, Diagnostic> {
        self.pos += 1;
        let mut entries = Vec::new();
        self.blanks();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Table(entries));
        }
        loop {
            self.blanks();
            let entry = self.entry()?;
            if entry.key.len() > 1 {
                return Err(self.unsupported(entry.key[0].span.start as usize, "a dotted key inside an inline table"));
            }
            entries.push(entry);
            self.blanks();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Table(entries));
                }
                _ => {
                    return Err(self.error(
                        self.pos,
                        "an inline table's entries are separated by `,` and it closes with `}` on the same line",
                        "expected `,` or `}` here",
                    ))
                }
            }
        }
    }
}

/// TOML 1.0 forbids defining a key twice and opening a `[table]` twice. Both
/// are checked after the parse, so the error can point at both places.
fn check_duplicates(file: FileId, document: &Document) -> Result<(), Diagnostic> {
    let duplicate = |first: Span, second: Span, what: &str| {
        Diagnostic::error(Code::sp(1), format!("`science.toml` is not valid TOML: {what} is defined twice"))
            .with_label(Label::primary(second, "defined again here"))
            .with_label(Label::secondary(first, "first defined here"))
    };
    let _ = file;
    fn entries_once(entries: &[Entry]) -> Option<(Span, Span, String)> {
        for (i, entry) in entries.iter().enumerate() {
            let names: Vec<&str> = entry.key.iter().map(|k| k.name.as_str()).collect();
            for earlier in &entries[..i] {
                let other: Vec<&str> = earlier.key.iter().map(|k| k.name.as_str()).collect();
                if names == other {
                    return Some((earlier.key[0].span, entry.key[0].span, format!("the key `{}`", names.join("."))));
                }
            }
            if let Value::Table(inner) = &entry.value.value {
                if let Some(found) = entries_once(inner) {
                    return Some(found);
                }
            }
        }
        None
    }
    if let Some((first, second, what)) = entries_once(&document.root) {
        return Err(duplicate(first, second, &what));
    }
    for (i, section) in document.sections.iter().enumerate() {
        if let Some((first, second, what)) = entries_once(&section.entries) {
            return Err(duplicate(first, second, &what));
        }
        if section.array {
            continue;
        }
        let names: Vec<&str> = section.header.iter().map(|k| k.name.as_str()).collect();
        for earlier in &document.sections[..i] {
            let other: Vec<&str> = earlier.header.iter().map(|k| k.name.as_str()).collect();
            if !earlier.array && names == other {
                return Err(duplicate(earlier.span, section.span, &format!("the table `[{}]`", names.join("."))));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Document {
        parse(FileId(0), text).unwrap_or_else(|d| panic!("{text:?} must parse: {}", d.message))
    }

    fn refused(text: &str) -> String {
        match parse(FileId(0), text) {
            Ok(_) => panic!("{text:?} must be refused"),
            Err(d) => {
                assert_eq!(d.code.to_string(), "SP0001");
                d.message
            }
        }
    }

    #[test]
    fn the_design_notes_manifest_parses() {
        let doc = parsed(
            "[package]\nname        = \"spectra\"   # a comment\nversion = '0.4.1'\nsidecar = false\n\n\
             [dependencies]\nlinalg = \"0.7.0\"\nshared = { path = \"../shared\" }\n\n\
             [[binary]]\nname = \"fit\"\n\n[native.sources]\nfiles = [\"vendor/miniz.c\",\n  \"b.c\", ]\n",
        );
        assert_eq!(doc.sections.len(), 4);
        assert_eq!(doc.sections[0].entries[0].value.value, Value::String("spectra".into()));
        assert_eq!(doc.sections[0].entries[2].value.value, Value::Bool(false));
        let Value::Table(inner) = &doc.sections[1].entries[1].value.value else { panic!() };
        assert_eq!(inner[0].key[0].name, "path");
        assert!(doc.sections[2].array);
        assert_eq!(doc.sections[3].header.len(), 2);
        let Value::Array(files) = &doc.sections[3].entries[0].value.value else { panic!() };
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn escapes_and_quoted_keys_are_read() {
        let doc = parsed("\"a b\" = \"x\\ty\\u00e9\"\n");
        assert_eq!(doc.root[0].key[0].name, "a b");
        assert_eq!(doc.root[0].value.value, Value::String("x\ty\u{e9}".into()));
    }

    #[test]
    fn what_is_not_toml_is_refused_with_the_reason() {
        assert!(refused("name = \"open\n").contains("never closed"));
        assert!(refused("name \"x\"\n").contains("followed by `=`"));
        assert!(refused("a = 1 b = 2\n").contains("end of the line"));
        assert!(refused("[package\n").contains("`]`"));
        assert!(refused("a = 1\na = 2\n").contains("defined twice"));
        assert!(refused("[p]\n[p]\n").contains("defined twice"));
    }

    #[test]
    fn toml_this_parser_does_not_read_is_named_rather_than_misread() {
        assert!(refused("x = 1.5\n").contains("floating-point"));
        assert!(refused("x = 1979-05-27\n").contains("date or time"));
        assert!(refused("x = \"\"\"a\"\"\"\n").contains("multi-line string"));
    }
}
