# Changelog

All notable changes to the Science extension are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- **`def` replaces `function`** in `keyword.declaration.science`. Syntax
  revision 3 renamed the declaration, and `TokenKind::from_word` now maps
  `def`; `function` is an ordinary identifier, so the grammar no longer
  colours it. Writing it where a declaration belongs is `SC0156`, which
  carries `def` as a machine-applicable fix.
- **`try` leaves `keyword.control.science` and `null` joins it.** Revision 2
  §3 removed `try` with the `Result` type and §7 added the `null` literal;
  the lexer has since caught up with both, and this grammar was still listing
  the pre-revision split. `try` is now an ordinary identifier, and using it as
  the old prefix is `SC0155`.

## [0.1.0] — 2026-09-16

First release. Everything below is new.

### Added

- **The `science` language**, on the `.science` extension, with `#` as its line
  comment. There is no block comment form, because the lexer has none.
- **A TextMate grammar** (`source.science`) covering:
  - `#` comments to end of line;
  - string and character literals with the escape set the lexer accepts
    (`\n`, `\t`, `\r`, `\\`, `\"`, `\'`, `\0`, `\u{…}`), and
    `invalid.illegal.unknown-escape.science` for anything else after a
    backslash;
  - every numeric form — decimal, `0x`, `0o` and `0b`, `_` separators,
    exponents, and the `i8`…`u64` and `f32`/`f64` type suffixes;
  - the keyword list of `TokenKind::from_word`, split into a declaration group
    and a control group;
  - the `ReservedWord` list in its own scope, underlined as well as coloured;
  - `extern` blocks, held open by indentation, with their `library`, `via
    pkg-config`, `kind`, `when available`, `symbol`, `size` and `align`
    clauses, and with `static` and `union` read as items rather than as
    reserved words inside one;
  - types by capitalisation, calls by the `(` that follows, labels and fields
    by the `:` that follows;
  - operators including `->`, `**`, `..`, `..=`, `>=`, `<=`, `<<`, `>>`, `@`
    and `?`;
  - `==`, `!=` and `!` as `invalid.illegal` — syntax revision 2 §1 removed
    them and the lexer reports each as an error;
  - the unit literal `9.8<m/s^2>`, anchored tightly enough that `a < b`,
    `1 << 4` and `0 <= n` are untouched.
- **`language-configuration.json`**: bracket pairs, auto-closing and
  surrounding pairs including both quote characters, off-side folding, an
  `increaseIndentPattern` on a line ending in `:`, a `decreaseIndentPattern`
  on `else`, an `onEnterRule` that outdents after `return`, `break` and
  `continue`, and one that continues a `#` comment block.
- **Science Neon** and **Science Light**, both complete themes: editor,
  widgets, tabs, sidebar, status bar, git decorations and terminal ANSI.

### Notes

- The palette is **Science Neon**: Dracula's hue family at full saturation on a
  blue-black ground. It is shared with `web/assets/science.css`, which is the
  source of truth, and with `web/assets/highlight.js`.
- The chrome is blue-black rather than the project's teal-tinted neutrals.
  Teal and cyan are neighbours, and with cyan on types the furniture stopped
  separating from the code.
- Every foreground in both themes clears 4.5:1 on its own ground, computed.
  The two tightest are the comments, at 4.84:1 dark and 4.55:1 light, which is
  deliberate.
- The light theme is a derivation and is not neon. No neon hue survives a white
  ground; the hues are held and the luminance is moved until each is legible.
- `type Doc:` colours `Doc` as a type in both the editor and the website.
  `highlight.js` previously checked the label rule first and was changed to
  match this grammar; no known difference between the two remains.
