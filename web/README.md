# The Science website

Static HTML. No build step, no package manager, no `node_modules`.

```
web/
  index.html         the landing page
  tutorial.html      learn the language from nothing, in order
  guide.html         six features, one chapter each, in prose
  reference.html     the language reference
  syntax.html        the inventory of everything that can be written
  rationale.html     the arguments, including the ones that lost
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

**That day has arrived and the trigger should be honoured.** Adding
`guide.html` meant editing the same `<nav>` in six files, which is the exact
condition written above, and it was done with a script rather than by hand —
which is the tell. The next page should not be added until the nav is generated
from one list. This paragraph is here so that the decision is not quietly
relitigated by whoever adds the seventh page.

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

**Code samples are real, on four of the five pages.** Every Science block on
`index`, `tutorial`, `reference` and `syntax` is taken from `examples/` or
checked with `sciencec` before being pasted. A documentation site for a compiler
is the last place to print syntax the compiler would reject, and the corpus
exists precisely so that nobody has to invent any.

**`guide.html` is the exception, and it is a deliberate one that needs stating
in full.** That page teaches six features that are *designed and not yet
implemented* — `property`, `requires`/`ensures`/`invariant`, the missing-data
reduction policies, named axes, `durable`, and the policy clauses of `with` —
and it teaches them in the present tense, as though the compiler accepted them.
Each chapter compresses one note in `docs/superpowers/design/`, which is the
authority for what it says.

This is a real departure from the rule above and the reason is that a design
corpus nobody can read is a corpus nobody reviews: the six notes are forty
thousand words of argument aimed at compiler authors, and the guide is the
version a scientist can be handed. The cost is that a reader who types a
chapter's example into `sciencec` today gets a parse error.

**So the honest thing is not to hide the exception but to make it cheap to
retire.** Two obligations follow, and whoever implements one of these features
owes both:

1. When a feature lands, its chapter moves under the same checking the other
   four pages get, and its examples go into `examples/`.
2. Until then, `guide.html` is the *only* page allowed to print unimplemented
   syntax, and it says so in its own footer and in its "where to go next"
   section, which points at the notes as the primary sources.

A site that prints a future syntax without saying so is the failure this
project's own `reference.html` was careful to avoid with its `changing` markers.
A site that refuses to describe its design until the compiler catches up is a
different failure, and a slower one.

**With one exception, and the exception is labelled.** The error model of syntax
revision 2 §3 — `-> (Config, Error?)`, `T?`, postfix `?`, `null` — now parses:
those blocks go through `check` like every other. What is not implemented is
their *meaning* — flow narrowing and the unchecked-error diagnostic — which
needs a type checker that does not exist. So the `changing` markers stay, and
what they mark has changed underneath them: it used to be "this does not
compile", and it is now "this compiles and nothing verifies it".

That distinction is the whole promise: a reader is never shown syntax that does
not work without being told. It was never that the compiler is always ahead of
the site — and for one afternoon the compiler was ahead, and this paragraph was
the last thing to know.

**Removed syntax is struck through.** A block marked `<pre data-science
data-legacy>` is kept for comparison only. It gets a grey rail instead of the
neon one, no glow, and a line through every word in `REMOVED` in
`assets/highlight.js`. Three markings, because a reader skimming will miss any
one of them, and mistaking a removed form for a live one is the expensive
mistake this site can make. There are five such blocks today — three in
`rationale.html`, one each in `syntax.html` and `reference.html` — up from one
when this paragraph was first written, because the bracket-and-`&` migration
gave the site a second thing worth showing a removed spelling of; the
attribute is what keeps the strike from hitting `Result` or `Some` if a future
sample uses either as an ordinary name.

**The highlighter is the keyword list.** `assets/highlight.js` holds the
keywords and the reserved words, and they must move when
`crates/science-lexer/src/token.rs` moves. It is under three hundred lines
because no off-the-shelf highlighter knows this language and teaching one
costs more than writing this did.

It also feeds the three word banks at the bottom of `reference.html`, through a
`data-words` attribute — `keywords`, `reserved`, `never`. A bank without that
attribute renders empty, silently.

**The lists are now ahead of the lexer by design, in one direction.** Six words
left `RESERVED` when the notes above landed — `durable`, `checkpoint`, `resume`,
`with`, `assert` and `pure` — and thirteen joined `DECLARE` and `CONTROL`:
`test`, `property`, `axis`, `requires`, `ensures`, `invariant`, `keyed`, `by`,
`every`, `ignoring`, and the policy names `seed`, `device` and `precision`. Per
the rule in the paragraph below, this is recorded as a decision rather than left
to be found as a bug: **the list is ahead of `token.rs`, deliberately, because
`guide.html` needs those words coloured.** Every one of the thirteen is
*contextual* in its note's grammar, so none of them is a reservation and
`let axis be 0` must keep compiling when the lexer catches up.

`axis` and `device` also left the `never` bank, and that bank's promise is
unchanged: both words are still free as identifiers. The bank now lists the
words that are free *and* uninvolved in any construct.

The lists and `token.rs` otherwise agree word for word today. Two entries were knowingly
**ahead** of the lexer for a while — `try` left `CONTROL` and `null` joined it,
per revision 2 §3 and §7, before `from_word` had either — and the lexer has
since caught up with both, as it has with revision 3's rename of `function` to
`def`. Where the two ever diverge again, say so in `highlight.js`'s own comment
and say which way round: a list that is ahead of the compiler is a decision, and
a list that is behind it is a bug.

## Adding a page

Copy the `<head>` of `reference.html`, keep the two stylesheet links and the
script at the end, and write the body inside `<div class="wrap">`. Use
`<pre data-science>` for Science, `<pre class="shell">` for a terminal and
`<pre class="err">` for a diagnostic.

**On `<pre class="err">`, and how much of it to use.** The reference and the
tutorial lean on diagnostics heavily and correctly — teaching a language through
its error messages is this project's stated principle, and those two pages exist
to be precise. A page written to be *read*, rather than consulted, needs the
opposite ratio: `guide.html` has one error block in six chapters, placed in the
one spot where reading the message is itself the lesson. Alternating a code
block and an error box teaches the reader what the compiler rejects before they
have understood what it accepts. Prefer prose that explains the rule, and spend
an error box only where the message says something the prose cannot.
