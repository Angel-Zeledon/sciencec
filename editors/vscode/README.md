# Science for Visual Studio Code

Syntax highlighting, editor configuration and two colour themes for the
[Science](../../) programming language.

Science is indentation-delimited, so the most load-bearing thing here is not
the grammar — it is `increaseIndentPattern` in `language-configuration.json`,
which is what makes a line ending in `:` open a block when you press Enter.
Without it the editor is unusable; with it the rest is colour.

## Installing it unpacked

There is no marketplace listing. Copy or link this directory into your VS Code
extensions folder and restart:

| Platform | Extensions folder |
|---|---|
| Linux, macOS | `~/.vscode/extensions/` |
| Windows | `%USERPROFILE%\.vscode\extensions\` |

```sh
# From the repository root, on Linux or macOS:
ln -s "$PWD/editors/vscode" ~/.vscode/extensions/science-lang

# On Windows, in an elevated PowerShell:
New-Item -ItemType SymbolicLink `
  -Path "$env:USERPROFILE\.vscode\extensions\science-lang" `
  -Target "$PWD\editors\vscode"
```

A symlink means the extension tracks the repository; a copy does not. Either
way, restart VS Code, open any `examples/*.science` file, and pick a theme with
**Preferences: Color Theme** → *Science Neon* or *Science Light*.

To package it as a `.vsix` instead:

```sh
npx --yes @vscode/vsce package
```

## What is in here

| File | What it does |
|---|---|
| `package.json` | Contributes the `science` language, the grammar and both themes. |
| `language-configuration.json` | Comments, brackets, auto-closing pairs, indentation rules. |
| `syntaxes/science.tmLanguage.json` | The TextMate grammar. |
| `themes/science-dark.json` | Science Neon — the flagship. |
| `themes/science-light.json` | Science Light. |

## The palette

**Science Neon** — Dracula's hue family pushed to full saturation on a deep
blue-black ground, with the chrome moved off teal to match it. The palette is
built for the dark theme; the light column is a derivation, and it is not neon,
because no neon hue survives a white ground.

The values are shared with the website: `web/assets/science.css` holds the same
palette as `--c-*` custom properties, and `web/assets/highlight.js` reproduces
the same token classes, so a sample on the site and the same file in an editor
are coloured the same way. **`science.css` is the source of truth.** If the two
ever disagree, the stylesheet is right.

| Role | Dark | Light |
|---|---|---|
| comment *(italic)* | `#7080a4` | `#66708a` |
| keyword, operator | `#e85aa8` | `#c0117f` |
| type | `#4de8ff` | `#0a6c85` |
| function | `#3dfca0` | `#0d7a4e` |
| string | `#f6ff6b` | `#6d6a00` |
| number, escape | `#c77dff` | `#7b3fd4` |
| parameter, label *(italic)* | `#ffa23c` | `#9e5800` |
| reserved for a later phase *(underlined)* | `#ff6e6e` | `#b23a3a` |
| punctuation | `#8fa0c4` | `#5b6579` |
| ground / surface | `#0b0f1a` / `#121a2e` | `#f4f6f5` / `#ffffff` |

Every value in both columns clears 4.5:1 against the ground it is used on; the
numbers were computed, not estimated. The two tightest are the dark comment at
4.84:1 and the light comment at 4.55:1, and both are tight on purpose — a
comment that competes with the code is a comment in the wrong colour.

The saturation is the design. A hue this strong on a ground this dark is what
makes the theme read as emissive rather than merely coloured, and it is why the
chrome moved from the teal-tinted neutrals to blue-black: teal and cyan are
neighbours, and the cyan on types stopped separating from the furniture.

## How the grammar decides

The grammar has no name resolution, so it decides the same way
`web/assets/highlight.js` does — by what is next to a word:

| Rule | Example | Scope |
|---|---|---|
| a word in the declaration list | `function`, `type`, `let`, `be` | `keyword.declaration.science` |
| a word in the control list | `if`, `match`, `is`, `try` | `keyword.control.science` |
| a word reserved for F1–F4 | `agent`, `tensor`, `yield` | `invalid.deprecated.reserved.science` |
| a name followed by `(` | `truncate(80)` | `entity.name.function.science` |
| a capitalised word | `Doc`, `Option` | `entity.name.type.science` |
| a lowercase name followed by `:` | `title:`, `limit:` | `variable.parameter.science` |

The keyword lists come from `TokenKind::from_word` in
`crates/science-lexer/src/token.rs`, and the reserved list from `ReservedWord`
in the same file. When that file changes, this grammar and `highlight.js` change
with it.

Reserved words are marked structurally as well as by hue — underlined here, a
dotted underline on the site. No spare hue reads as "not yet", and a reader who meets
`tensor` in a sample has to be able to see that it is not an ordinary name.
VS Code's theme `fontStyle` has no dotted variant, so the editor gets a solid
underline where the site gets a dotted one.

## What the grammar cannot express

TextMate grammars are regular expressions over single lines. Four things
follow from that, and each costs a reader something:

1. **A capitalised name is a type, always.** `Doc` is teal whether it is a type
   or a constant. Science capitalises types by convention (§4.2), so this is
   right nearly always — but a capitalised binding would be miscoloured.

2. **A name before `(` is a call, always.** So is a variant constructor
   (`Some(x)`), and so is a record construction (`Doc(title: …)`). All three
   are green. The site does the same thing, so at least they agree.

3. **Indentation is not a scope.** The grammar cannot tell a method body from a
   free function, or a `match` arm from a statement. `extern` blocks are the
   one exception — a `begin`/`while` rule holds the block open for every line
   indented past its header, which is what lets `static` and `union` mean
   *item* inside a block and *reserved word* outside one.

4. **`is not` is two words.** The grammar colours `is` and `not` as two
   keywords rather than one operator. They are the same colour, so it does not
   show.

One thing the grammar *does* express that may surprise you: `==`, `!=` and `!`
are scoped `invalid.illegal.operator.science` and appear underlined. They are
not operators in Science — equality is `is`, inequality is `is not`, negation
is `not` — and the lexer reports each as an error (`SC0016`, `SC0017`,
`SC0001`). Colouring them as errors is the point.

### The website now follows this grammar

`highlight.js` used to check "is it followed by `:`?" before "is it
capitalised?", which coloured the `Doc` in `type Doc:` as a label rather than as
a type. The grammar was right and the stylesheet was changed to match: both now
test capitalisation first, so `type Doc:`, `choice Format:` and
`Countdown implements Iterate:` colour their names as types. No divergence
remains.

### The unit literal

`9.8<m/s^2>` is in. It is scoped only when a `<…>` follows a digit *directly*,
contains no whitespace, and holds nothing but letters, digits and `*`, `/`, `^`:

```
9.8<m/s^2>      →  number + type
7660<m/s>       →  number + type
a < b           →  operator, untouched
1 << 4          →  operator, untouched
count<10        →  operator, untouched — `<` is not preceded by a digit
0 <= n          →  operator, untouched
```

The narrow anchoring is what makes it safe: Science writes generics with `of`,
not with angle brackets, so `<…>` after a digit has no other meaning. Note that
the unit literal is a *staged* feature (`docs/superpowers/design/unit-literals.md`);
the F0 lexer does not accept it yet.
