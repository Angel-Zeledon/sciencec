# The Science website

Static HTML. No build step, no package manager, no `node_modules`.

```
web/
  index.html         the landing page
  reference.html     the language reference
  assets/
    science.css      every page's styles
    highlight.js     syntax highlighting for `<pre data-science>` blocks
```

## Why no framework

Astro, Eleventy or Hugo would each give template inheritance and Markdown
rendering, and each would put a Node or Go toolchain between a contributor and a
one-line copy fix. This repository declined to add an argument-parsing crate for
the compiler's own driver on the grounds that one third-party dependency was
already enough; the same reasoning applies here with less at stake.

The cost is real and worth naming: the header and footer are duplicated per
page, so a change to either is a change in every file. That is affordable at two
pages. **It stops being affordable at about six**, and the honest trigger for
adopting a generator is the day someone edits the same nav three times in one
sitting — not a page count decided in advance.

## Running it

Any static server. There is nothing to compile.

```sh
python3 -m http.server -d web 8000
# then open http://localhost:8000
```

Opening `web/index.html` from the filesystem also works, since every asset is
referenced relatively.

## Deploying

The directory is the site. Point GitHub Pages at `web/` on the default branch,
or copy it anywhere that serves files.

Two things come from the network and both degrade gracefully: the fonts are
loaded from Google Fonts and fall back to the system stack, and nothing else is
fetched at all. There is no analytics, no tracking and no third-party script.

## Conventions

**Colours are tokens.** They are declared once on `:root` and redefined for dark
mode under both `prefers-color-scheme` and an explicit `data-theme`, because a
reader is in one of three states — light, dark, or a system default that stamps
no attribute — and a colour defined only inside a media query renders one
theme's text on the other theme's background.

**Every code surface is dark, in both themes.** The syntax palette — "Science
Neon" — is declared once on `:root` and never redefined, because it never has
to render on paper. That is the decision that lets it stay saturated: a neon
hue survives on `#0b0f1a` and dies on white, and a palette asked to do both
ends up doing neither. A reader in light mode gets a light page and a dark
listing, which is also what their editor gives them.

**Code samples are real.** Every Science block on these pages is taken from
`examples/` or checked with `sciencec` before being pasted. A documentation site
for a compiler is the last place to print syntax the compiler would reject, and
the corpus exists precisely so that nobody has to invent any.

**With one exception, and the exception is labelled.** The error model of syntax
revision 2 §3 — `-> (Config, Error?)`, `T?`, postfix `?`, `null` — is decided and
not yet implemented, so blocks using it are checked against the revision note by
reading and **cannot** be run through `sciencec`, which would reject them. They
carry `<span class="cite wip">changing</span>`, the reference page says so at the
top and again in the Errors section, and the landing page says so beside the
install command. When the lexer and parser land, drop the markers and run the
blocks through `check` like everything else. The promise above is that a reader
is never shown syntax that does not work without being told; it is not that the
compiler is always ahead of the site.

**Removed syntax is struck through.** A block marked `<pre data-science
data-legacy>` is kept for comparison only. It gets a grey rail instead of the
neon one, no glow, and a line through every word in `REMOVED` in
`assets/highlight.js`. Three markings, because a reader skimming will miss any
one of them, and mistaking a removed form for a live one is the expensive
mistake this site can make. There is exactly one such block today, in the Errors
section; the attribute is what keeps the strike from hitting `Result` or `Some`
if a future sample uses either as an ordinary name.

**The highlighter is the keyword list.** `assets/highlight.js` holds the
keywords and the reserved words, and they must move when
`crates/science-lexer/src/token.rs` moves. It is forty lines because no
off-the-shelf highlighter knows this language and teaching one costs more than
writing this did.

It also feeds the three word banks at the bottom of `reference.html`, through a
`data-words` attribute — `keywords`, `reserved`, `never`. A bank without that
attribute renders empty, silently.

Two entries are knowingly **ahead** of the lexer: `try` has left `CONTROL` and
`null` has joined it, per revision 2 §3 and §7, while `token.rs` still has
`"try" => Try` and no `null`. That divergence is the same one the `changing`
markers describe and it closes the day the error model lands.

## Adding a page

Copy the `<head>` of `reference.html`, keep the two stylesheet links and the
script at the end, and write the body inside `<div class="wrap">`. Use
`<pre data-science>` for Science, `<pre class="shell">` for a terminal and
`<pre class="err">` for a diagnostic.
