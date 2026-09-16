# Science — Design: MCP servers

Date: 2026-09-16
Status: draft for review
Area: whether a Science program can expose itself to a language model, what that
costs the language, and which of the three words §13 reserved for F4 this note
spends.
Depends on: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
(§2 the phases, §5 types, §12 what is out of scope, §13 the reserved list).
Syntax: `syntax-revision-2.md` — `->`, `interface`, `for x in xs`, `loop`/`break`,
`print`, `is`/`is not` with `> < >= <=`, and §3's error model `-> (T, Error?)`,
`T?`, postfix `?`, `null`.
Reconciles rather than re-specifies: `strings-formatting-and-docs.md` §5 (doc
comments — this note makes one of them load-bearing and contradicts §5.3 in one
sentence), `data-io.md` §4.3 (the `Record` derivation, whose third customer this
note becomes), `effects.md` §3 and Decision 5 (`pure function`, which this note
spends and does not extend), `unit-literals.md` §5.1 and §7.3 (SI coherent base
units, and the display-unit seam this note lands on), `llm-ergonomics.md` (the
opposite direction), `rust-binding-generation.md` §5.2 Decision 10 (bind the
format, not the library — the precedent this note has to answer to).
Diagnostics: `SC0190`–`SC0199` (Syntax), `SC0504`–`SC0519` (Types, second band).

---

## 0. The third direction

This project has thought about language models twice, in opposite directions,
and this note is the third.

| Direction | Note | The question |
|---|---|---|
| A model **writes** Science | `llm-ergonomics.md` | Can a model that has never seen this language produce a program that compiles? |
| Science **calls** a model | F4 — `agent`, unwritten | Can a Science program drive a model as part of a computation? |
| A model **calls** Science | **this note** | Can a researcher's Science program be something a model reaches for? |

The third is the smallest of the three and it is the only one available now.
It needs no inference runtime, no `agent` construct, no durability, and nothing
from F4 or F5. A server is a *passive callee*: it declares what it can do, it
waits, it answers. That is the whole reason this note can be written before the
type checker exists and `agent` cannot.

It is also the one with a deadline that is not the project's own. The other two
are paced by Science. This one is paced by a protocol from another vendor, and
the first job of the note is to make sure that pacing does not reach the
language.

### 0.1 The brief, and the ordering in it

The request was for MCP servers that are **easy, fast, and above all readable**,
and the ordering is the specification. Where the three conflict below,
readability wins and the note says what was traded:

- §4 trades **expressiveness** for readability: a `tool` may take only the types
  that survive a round trip through JSON Schema, which is fewer types than a
  `function` may take, and the rejections are compile errors rather than runtime
  surprises.
- §5 trades **the author's freedom** for readability: a tool with no
  documentation does not compile. This is the only place in the language where a
  comment is mandatory, and §5.1 argues that it is not a comment.
- §9 trades **interruptibility** for readability and for speed at once: a
  cancellation check is a line the author writes, because the alternative is a
  branch in the inner loop of every numerical kernel in the language.
- §11 trades the **speed** claim away entirely. It was measured and it is not the
  reason to do this.

Two sections are not about the brief's three words at all, and they are the two
that decide whether this is worth building. **§9** is there because a scientific
tool is long-running by nature, so what a general-purpose SDK treats as an edge
case is this audience's median case. **§10** is there because a result a model
reports should be checkable by somebody else, and this project has already built
every piece of that machinery for a different reason.

---

## 1. What MCP actually is, verified against the live specification

Nothing in this section is from memory. It was checked today against
`schema/2026-07-28/schema.ts` and `docs/specification/2026-07-28/*` in
`modelcontextprotocol/modelcontextprotocol` at `main`, HEAD `f56f204`, whose commit
date is **2026-09-16** — the same day as this note — cross-checked against the
rendered specification at `modelcontextprotocol.io`. §1.7 says what could not be
verified.

> **This note targets protocol revision `2026-07-28`.** `schema.ts` declares
> `LATEST_PROTOCOL_VERSION = "2026-07-28"`, and `schema/draft/schema.ts` reports
> the same value, so no newer dated revision is in flight as of today. **The spec's
> own compatibility matrix marks both Modern→Legacy and Legacy→Modern as
> "Fails"**, so a design written against `2025-06-18` — which is what most
> published material still describes — would not interoperate at all.

### 1.1 The revisions, and what each one moved

Six dated revisions exist, each with its own changelog at
`/specification/<date>/changelog`, so none of this is inferred.

| Revision | What it did |
|---|---|
| `2024-11-05` | Baseline. HTTP+SSE transport. No changelog. |
| `2025-03-26` | OAuth 2.1 authorization; **Streamable HTTP replaces HTTP+SSE**; **JSON-RPC batching added**; **tool annotations added**; audio content; `completions` capability. |
| `2025-06-18` | **JSON-RPC batching removed**; **structured tool output** (`structuredContent` + `outputSchema`); elicitation; resource links; `MCP-Protocol-Version` header required; **`title` added**, so `name` becomes a programmatic identifier. |
| `2025-11-25` | Icons; OIDC discovery; **JSON Schema 2020-12 established as the default dialect** (SEP-1613); tool-name guidance; experimental tasks; **input-validation errors clarified as tool errors, not protocol errors** (SEP-1303). |
| `2026-07-28` | **Sessions and `Mcp-Session-Id` removed. `initialize` and `notifications/initialized` removed; the protocol is stateless with per-request `_meta`.** `server/discover` added and servers MUST implement it. `subscriptions/listen` replaces the HTTP GET stream and `resources/subscribe`. `ping`, `logging/setLevel` and `notifications/roots/list_changed` removed. `resultType` now required on every result. SSE resumability removed. `ttlMs`/`cacheScope` required on list results. **Roots, sampling and logging all deprecated.** |

**Read the last two rows together, because they are §2.1's evidence rather than
trivia.** In twenty-one months this protocol added JSON-RPC batching and removed
it one revision later; replaced its transport; introduced a `title` field that
changed what `name` means; and then **deleted its own handshake**. HTTP+SSE is now
formally Deprecated with an earliest removal of three months after its removal
proposal reaches Final.

That is not a criticism of MCP — a protocol that is willing to delete `initialize`
is a protocol being maintained rather than ossified, and it is why it is worth
targeting. It is a precise statement of what a language would be welding itself to
if `tool` meant "an MCP tool". **A language cannot delete its handshake.**

### 1.2 The three primitives, exactly

**`Tool`**, from `schema.ts`:

```ts
export interface Tool extends BaseMetadata, Icons {
  description?: string;
  inputSchema: { $schema?: string; type: "object"; [key: string]: unknown };
  outputSchema?: { $schema?: string; [key: string]: unknown };
  annotations?: ToolAnnotations;
  _meta?: MetaObject;
}
```

`BaseMetadata` supplies `name` (required) and `title`; `Icons` supplies `icons`.
Display precedence is `title`, then `annotations.title`, then `name`.

**`ToolAnnotations`** has exactly four hints, and **two of them default to
`true`**: `readOnlyHint` (default `false`), `destructiveHint` (**default `true`**),
`idempotentHint` (default `false`), `openWorldHint` (**default `true`**). The
defaults are fail-safe, which §8 depends on.

**`Prompt` arguments are not schematised.** `PromptArgument extends BaseMetadata`
with only `description?` and `required?` — no type, no schema — and the wire values
are `{ [key: string]: string }`. A prompt takes a flat list of named strings.
**This is the fact Decision 2 turns on**, and it was verified rather than assumed.

**`Resource`** carries `uri`, `name`, `title`, `description`, `mimeType` and
annotations, with `uriTemplate` (RFC 6570) for templates.

### 1.3 `inputSchema`, which is the shape that decides §4

- **`type: "object"` is required at the root.** It is in the TypeScript type
  itself, not just the prose: *"Tool arguments are always JSON objects, so
  `type: "object"` is required at the root."*
- **Everything else is open.** `properties` and `required` are not mandatory; the
  index signature admits any keyword. The prose explicitly permits composition
  (`oneOf`, `anyOf`, `allOf`, `not`), conditionals (`if`/`then`/`else`) and
  references (`$ref`, `$defs`, `$anchor`).
- **The dialect is declared**: JSON Schema **2020-12** by default when no `$schema`
  is given; implementations MUST support at least 2020-12.
- `outputSchema` is **not** constrained to `type: "object"`; the prose shows an
  array-typed example.

**Two consequences for §4 that are worth stating now.** First, `$defs` and `$ref`
are permitted, so a recursive record type is expressible (§4.2, `SC0510`). Second,
**`oneOf` is permitted, so §4.7's rejection of payload-carrying `choice` types is
Science's decision and not the protocol's.** The note says so there rather than
hiding behind the spec.

There are also two security rules riding on the schema that a Science emitter must
respect by simply never doing the thing: a network-URI `$ref` MUST NOT be
automatically dereferenced, and implementations SHOULD bound composition depth as
denial-of-service protection. A generated schema has no `$ref` to a network URI and
bounded depth by construction, which is one of the quieter arguments for
generating rather than accepting schemas.

### 1.4 `CallToolResult`, and the two error channels

```ts
export interface CallToolResult extends Result {
  content: ContentBlock[];
  structuredContent?: unknown;
  isError?: boolean;
}
```

`Result` now also requires **`resultType`**, which is `"complete"` or
`"input_required"`. `ContentBlock` discriminators are exactly `"text"`, `"image"`,
`"audio"`, `"resource_link"` and `"resource"`.

`outputSchema` and `structuredContent` are bound to each other: *"Servers MUST
provide structured results that conform to this schema. Clients SHOULD validate
structured results against this schema."* And for compatibility: *"a tool that
returns structured content SHOULD also return the serialized JSON in a
`TextContent` block."* `structuredContent` is now any JSON value, not only an
object.

The `isError` rule, verbatim from the doc comment in `schema.ts` — this is the
sentence §7 is built on:

> Whether the tool call ended in an error. If not set, this is assumed to be false
> (the call was successful). Any errors that originate from the tool SHOULD be
> reported inside the result object, with `isError` set to true, _not_ as an MCP
> protocol-level error response. Otherwise, the LLM would not be able to see that
> an error occurred and self-correct. However, any errors in _finding_ the tool, an
> error indicating that the server does not support tool calls, or any other
> exceptional conditions, should be reported as an MCP error response.

And the closing rule: *"Clients MAY provide protocol errors to language models,
though these are less likely to result in successful recovery. Clients SHOULD
provide tool execution errors to language models to enable self-correction."*

**The non-obvious part, and it changes §7: input validation errors are on the
`isError` side, not the protocol side.** The prose names *"input validation errors
(e.g. date in wrong format, value out of range)"* among tool execution errors
explicitly, as a deliberate `2025-11-25` decision (SEP-1303).

### 1.5 Transports and framing

**stdio**, which is the only transport this note ships (§14.4):

- *"Messages are delimited by newlines, and MUST NOT contain embedded newlines."*
  One JSON-RPC message per line.
- The server **MUST NOT** write anything that is not an MCP message to stdout.
- The server MAY write UTF-8 to **stderr** *"for any logging purposes including
  informational, debug, and error messages"*, and the client *"SHOULD NOT assume
  stderr output indicates error conditions."*

**That third bullet is a Science-specific hazard and §8.2 turns it into a
diagnostic**: `print` writes to stdout, and a `print` reachable from a tool body
corrupts the protocol stream.

**Streamable HTTP** remains, minus sessions and minus SSE resumability. HTTP+SSE
from `2024-11-05` is Deprecated.

**On batching, stated precisely because it is easy to get wrong:** the current
specification contains zero occurrences of the word "batch". The accurate claim is
not that batching is forbidden but that **support was removed in `2025-06-18` and
the current wire formats structurally admit only single messages** —
`JSONRPCMessage` has no array variant, an HTTP POST body *"MUST be a single
JSON-RPC request or notification"*, and stdio is one message per line.

### 1.6 Capabilities, discovery and trust

Capability objects are `tools: {"listChanged": true}`,
`prompts: {"listChanged": true}`,
`resources: {"listChanged": true, "subscribe": true}` — but they now live in
`DiscoverResult.capabilities`, returned from `server/discover`, **not in an
`initialize` response, which no longer exists**. And
`notifications/tools/list_changed` is no longer freely pushed: it is delivered only
on a `subscriptions/listen` stream the client opted into, and *"the server MUST NOT
send notification types the client has not explicitly requested."*

**On trust**, cited carefully because the scope matters: the MUST-level language is
about **annotations** specifically — *"clients MUST consider tool annotations to be
untrusted unless they come from trusted servers."* The broader statement that
descriptions of tool behaviour should be considered untrusted is on the
specification's index page rather than in the tools chapter. §8.1 and §19 both
depend on this distinction and neither overstates it.

### 1.7 What was verified, what was not, and what the specification declines to say

**Verified by reading the source:** the revision list and every changelog; the
`Tool`, `ToolAnnotations`, `PromptArgument`, `Resource` and `CallToolResult`
shapes; the `inputSchema` constraint and the declared dialect; the `isError`
wording quoted above; the stdio framing rules; the capability objects; the
statelessness of `2026-07-28`.

**Read from `schema.ts`, which the specification names as the source of truth,
rather than from the generated `schema.json`.** If the two ever disagree, this
note is wrong in whichever direction the generator is.

**Fetched through the GitHub contents API** rather than `raw.githubusercontent.com`,
which was unreachable from this machine throughout. Two independent passes agreed,
and nothing 404'd.

**The Tasks extension was verified separately and in depth**, because §9 depends on
it entirely. It does not live in this repository: it has its own,
`modelcontextprotocol/ext-tasks`, whose `specification/2026-07-28/tasks.md` and
`schema/2026-07-28/schema.ts` were read end to end. Its `2026-07-28` snapshot is
marked **`Stable`** and immutable, and SEP-2663 is **Final** — the file exists at
`seps/2663-tasks-extension.md` in the main repository and is not an open pull
request. §9.1 quotes it; §19 prices the risk of designing against an extension with
its own release train.

**Not verified:** the MCP Apps and Skills extensions, beyond their overviews.
Nothing in this note depends on them and §15 says neither is shipped.

**A documentation bug found while verifying, reported here because someone will
otherwise copy it.** The `ext-tasks` landing page cites `-32003` for a missing
required client capability. The normative specification and the core schema both
use **`-32021`**; the changelog records the renumber. The landing page was not
updated.

**A published registry exists** at `registry.modelcontextprotocol.io`, under a
`/v0.1/` path prefix, with an `mcp-publisher` CLI. `.well-known/mcp.json` as a
server-metadata card is **a proposal, not the specification** — SEP-2127 is an open
pull request with no Final SEP file — and what shipped instead is the
transport-agnostic `server/discover`. §15 does not ship a publisher and §20 notes
why the registry matters anyway.

**The specification says nothing about performance, and this is an exhaustive
negative rather than a sampled one.** A grep across `docs/` and `seps/` for every
revision plus `draft`, for `performance|latency|startup|cold.?start|benchmark|
throughput|p95|p99`, returns only incidental prose: no section, no table, no
number, no target, no budget. The conformance suite tests correctness, not speed.
The timeouts section specifies **no concrete duration at all** — no default, no
ceiling, no suggested range. **So the number in §11 is this note's, and it has to be
justified here rather than cited.**

**There is an official conformance suite**, at
`modelcontextprotocol/conformance`, available since 2026-01-23, and an SDK tiering
policy (SEP-1730, Final). Ten SDKs are official, in three tiers. A Science `mcp`
package would be a community implementation and **the conformance suite is its test
target** — §17 asks for that explicitly, because it is the difference between
claiming compliance and having it checked.

---

## 2. The central question: does an MCP server deserve syntax?

§13 of the core spec reserves `agent`, `tool` and `prompt` for F4, and
`crates/science-lexer/src/token.rs` lexes all three today:

```rust
"agent"  => Reserved(Agent),
"tool"   => Reserved(Tool),
"prompt" => Reserved(Prompt),
```

The reservation makes a `tool` declaration *possible*. It does not make it
*right*, and `reserved-words.md` is the note that established the difference:
its §3.1 found that `shape`, `model` and `tensor` were reserved to protect
"a hypothesis" that its §6 then showed F1 did not hold, and concluded that a
reservation buying nothing should be released. This note inherits that test and
has to apply it to three more words. The question is not "may we write `tool`" —
we may. It is **"does `tool` protect a construct, or a hypothesis?"**

### 2.1 The precedent that points the other way, stated at its strongest

`rust-binding-generation.md` §5.2 Decision 10 is the sharpest rule this project
has produced about third-party targets:

> **When a crate has a stable interchange format, bind the format. When it does
> not, generate a binding to the crate. Never bind an API whose data type has a
> published ABI, because the ABI is smaller, more stable, and shared with every
> other consumer.**

Its rejection paragraph names the failure mode exactly: binding polars' API is
refused because it is "an enormous surface with a fast-moving pre-1.0 semver, so
the binding is a permanent treadmill against a crate that renames methods between
minor versions." Four sibling notes made the same bet — `data-io.md` §9 (the
Arrow C Data Interface), `data-io.md` §4.2 and `python-interop.md` §4.1 (DLPack),
`c-binding-coverage.md` §3.8 Decision 6 (a C ABI the library itself publishes and
versions).

MCP is exactly the shape that rule exists to refuse. It is a versioned protocol
from one vendor, and §1.1's verified revision list is not a theoretical worry: in
twenty-one months it added JSON-RPC batching and removed it one revision later,
replaced its transport, redefined what `name` means by adding `title` beside it,
and in the **current** revision **deleted `initialize` and sessions outright**. A
`tool` keyword that means *an MCP tool* welds a language construct to that, and a
language cannot issue a point release to catch up with a protocol that removes its
own handshake.

`stdlib-shape-and-packages.md` §1 supplies the second half of the same argument
from the other side: **"Nothing enters Level 1 or Level 2 whose design is
unsettled."** MCP's design is unsettled by construction — that is what a dated
revision string *means*. So whatever else is decided below, the protocol code is
Level 3, an ordinary package with an ordinary version, not in the prelude and not
in the toolchain.

### 2.2 Why the Python and TypeScript answer is not available here

The obvious move is the one every other SDK makes: a library, ordinary functions,
and a registration call.

```science
# The shape this would take. It cannot work, and §2.2 is why.
mcp.serve([fit_peaks, list_runs, d_spacing])
```

The two mainstream SDKs obtain the schema by two different mechanisms and
**Science has neither**:

- **Python** reads `inspect.signature` and the type annotations at run time. A
  decorated handler carries its parameter names, annotations and defaults as live
  objects. Science is monomorphised, compiled ahead of time, with no reflection:
  a function value at run time is a code pointer and nothing else. There is no
  signature to read.
- **TypeScript** does not introspect either. It makes the author pass a schema
  *beside* the handler — a Zod schema, or raw JSON Schema. The two are separate
  values and nothing checks that they agree. This is precisely the drift the brief
  names, and it is not a bug in that SDK; it is the only thing a language without
  reflection can do without help from its compiler.

So the choice is not "library versus syntax". It is **"which compiler feature pays
for the schema"**, and there are exactly three candidates.

### 2.3 The three real options

**Option A — a hand-written schema value beside each handler.** The TypeScript
shape. Zero language risk. Rejected: it *is* the drift, and it fails the
readability test badly — every tool is written twice, once as a signature the
compiler checks and once as a schema nothing checks, and the two copies sit next
to each other inviting a reader to believe they agree.

**Option B — a compiler-derived interface over a record type.** The real
alternative, and it has precedent in this repository. `data-io.md` §4.3 already
asks for exactly this machinery for `Record`:

```science
## Arguments to the peak fit.
type FitPeaksArgs:
    ## The run identifier, as printed in the run log.
    run: String
    ## Lower edge of the fit window.
    window_lower: Time of F64
    ## Number of peaks to fit.
    max_peaks: U8

FitPeaksArgs implements ToolArgs

function fit_peaks(args: FitPeaksArgs) -> (PeakTable, Error?):
    ...
```

This works. It needs **no new keyword at all**; it reuses a derive mechanism the
README already lists as a standing cross-note ask with two customers (`Record` in
`data-io.md` §11.1 and `Inspect` in `strings-formatting-and-docs.md`), and a third
customer strengthens that ask rather than inventing a new one. It gets
per-parameter descriptions for free, because §5.2 of the strings note already
attaches a `##` comment to a record field.

It is the option this note came closest to taking, and the reason it does not is
narrow, so it is stated before the decision rather than after: **it is two
declarations and one indirection where the thing being declared is one callable.**
A reader of `fit_peaks` has to hold `FitPeaksArgs` in their head to know what the
tool takes; the author names a type per tool whose only purpose is to be a
parameter list; and `args.run` appears in the body where `run` would. Against the
brief's first priority that is a real loss, and it is paid on every tool in every
server forever.

**Option C — a declaration form.** One declaration, parameters as parameters, doc
comment as description.

```science
## Fit peaks in a time-of-flight window.
tool fit_peaks(
        ## The run identifier, as printed in the run log.
        run: String,
        ## Lower edge of the fit window.
        window_lower: Time of F64,
        ## Number of peaks to fit.
        max_peaks: U8) -> (PeakTable, Error?):
    ...
```

### 2.4 What a `tool` has to satisfy that a `function` does not

A marker that changes nothing about a declaration is an attribute, and Science
should not spend a keyword on an attribute. The case for Option C rests entirely
on whether a `tool` carries obligations an ordinary `function` does not. It
carries five, and all five are checkable at compile time:

1. **Every parameter type must survive a round trip through JSON Schema** (§4). A
   `function` may take a closure, a trait object, a `borrowed` slice, a
   `Frame of Dynamic`; a `tool` may take none of them.
2. **It may not be generic.** `tools/list` is a flat list of concrete tools with
   one schema each; there is nothing for a type parameter to be instantiated at
   (`SC0191`).
3. **No parameter may be `borrowed`.** There is no caller to borrow from — every
   argument was deserialized from the wire a moment ago and is owned (`SC0192`).
4. **It must carry a doc comment** (§5). The description is not documentation; it
   is an input to the caller's dispatch decision.
5. **Its return type is constrained** to `Content`, to a record type that can
   produce an output schema, or to an `Error?` alone (§7).

Five obligations, none of which an ordinary `function` has, all of which a
compiler can check. That is not an attribute. That is a declaration form.

### 2.5 Decision 1

> **Decision 1. `tool` is a declaration form, and it declares *a callable exposed
> to a model* — a name, a typed parameter list, a required natural-language
> description, and a result. It does not mention MCP, it does not name a protocol,
> and nothing in the language knows what JSON-RPC is. MCP is one backend,
> implemented entirely in a Level 3 package.**

The word earns its keyword because the *abstraction* is not MCP's. A callable
with a machine-readable contract and a human-readable description, offered across
a boundary to a caller that is not a compiler, is the shape of MCP's tools, of
OpenAI's function calling, of Anthropic's tool use, of an OpenAPI operation and of
a gRPC reflection service. Every one of those is a backend for the same
declaration. Had `tool` meant "an MCP tool", `rust-binding-generation.md`'s
Decision 10 would forbid it
and this note would have landed on Option B.

**Rejected: Option A, a schema value beside the handler.** §2.3. It is the drift.

**Rejected: Option B, a record type plus a derive.** §2.3, on readability alone,
and the loss is acknowledged rather than argued away: Option B needs no keyword
and would have been free. **This note takes the one thing Option B has that Option
C cannot have for nothing** — per-parameter doc comments — and pays for it in §5.3
by asking `strings-formatting-and-docs.md` §5.2 to extend its attachment table by
one row. **If that ask is refused, Option B becomes the better answer and this
decision should be reopened**, because without per-parameter descriptions a
`tool`'s schema has a `description` on the tool and nothing on its properties,
which is the one place a model most needs prose.

**Rejected: `#[tool]`-style attributes on ordinary functions.** Science has no
attribute syntax and §12 of the core spec puts macros out of scope. Introducing an
attribute grammar in order to avoid spending a word that is *already reserved* is
a strictly larger language change than the one it avoids.

**Cost, stated.** Three, in increasing weight.

- One word leaves §13's *reserved, not yet used* list and joins the in-use list.
  That is the cheap one: the word was already gone.
- **`tool` is a second function-declaration form**, so every piece of the front
  end that walks items grows an arm — the parser, the resolver, `sciencec fmt`,
  the language server, the doc tool. `def-and-lambda.md` rejected `def` partly on
  this ground, and this note adds what that note refused. The difference it
  claims: `def` would have been a *synonym* for `function`, and `tool` is a
  declaration with five obligations `function` does not have. If that difference
  is judged insufficient, the fallback is Option B and it is a good fallback.
- **The five obligations are five new ways for a program to fail to compile**, and
  four of them fire in the type checker, which does not exist. §14 says what can
  ship before it; the answer is more than expected, and it is not all of it.

### 2.6 What would have to be true for Decision 1 to have been wrong

House rule, and this note owes it more than most, because it is spending a word.
Three falsifiers, each stated so that it can actually be checked later:

1. **If exposed callables are mostly *dynamic*.** A declaration is static by
   construction: the set of tools is fixed when the binary is linked. If real
   servers overwhelmingly register tools from data — a directory of scripts, a
   database table, a config file, a set of SQL views — then a declaration form is
   the wrong shape and a registration API was right all along. **The check:**
   across a sample of published servers, how many call a per-tool registration in
   a loop rather than once per literal tool. If it is most of them, Decision 1 was
   wrong. §15 keeps a dynamic escape hatch open precisely because this falsifier
   is live, and §20 says what would make it ship.
2. **If the type restrictions bite on the common case.** §4's claim is that one
   source of truth removes drift. If most real tools need a parameter shape §4.2
   rejects, authors reach for the `Json` escape hatch on most tools — at which
   point the schema is hand-written again, the drift is back **and** a keyword has
   been spent. **The check:** the fraction of tools in the first ten real servers
   that take a `Json` parameter. Above a third and the mapping is too narrow.
3. **If "model-facing callable" does not generalise.** Decision 1's whole defence
   is that the abstraction outlives MCP. If the second backend — say, a
   function-calling schema for a chat completions API — needs materially different
   *declarations* rather than a different emitter over the same ones, then `tool`
   was an MCP keyword in a disguise and `rust-binding-generation.md`'s Decision 10
   should have caught it. **The
   check:** write the second backend before the language locks. §20 records this
   as the open question that most deserves an early answer.

### 2.7 The exit, if MCP dies

Stated concretely, because a design whose exit is "we would figure something out"
has no exit.

| Layer | Where it lives | If MCP is replaced |
|---|---|---|
| The `tool` declaration, the five obligations, the type mapping | The language | **Unchanged.** Nothing in it names MCP. |
| The JSON Schema emitter | The compiler | **Unchanged.** JSON Schema outlives MCP; every successor that describes a callable uses it or something isomorphic to it. |
| `tools/list`, `tools/call`, `isError`, `resultType`, the content block variants, `structuredContent`, `server/discover` and the capability objects | The `mcp` package, Level 3 | Deleted, or pinned at the package manager's granularity. |
| The transports and the JSON-RPC framing | The `mcp` package | Same. |
| The word `mcp` | `use mcp (…)` and one line of `science.toml` | Deleted from a program by editing two lines. |

The test that this layering is real, and it is checkable by a reviewer rather than
asserted here: **`sciencec` has no MCP-specific diagnostic.** Every code allocated
in §16 is about a Science type, a Science declaration or a doc comment. Not one
mentions a protocol field, a method name or a revision string. If a future
reviewer finds a diagnostic in the `SC` namespace that names an MCP concept, the
layering has leaked and Decision 1 has stopped being true.

---

## 3. Decision 2: `prompt` and `agent` stay reserved

Three words were reserved. Spending all three because they arrived in one list is
exactly the reasoning `reserved-words.md` §3.1 objected to.

> **Decision 2. This note spends `tool` and only `tool`. `prompt` stays reserved
> and unspent. `agent` stays reserved, unspent, and out of scope.**

**`agent` is the other direction and this note does not touch it.** §0's table is
the argument: MCP is the boundary where a model calls Science; `agent` is the
construct where Science calls a model. They share a vocabulary and nothing else.
An `agent` needs an inference client, conversation state, and — per §2 of the core
spec — F5's lowering of its body to a serializable state machine. A `tool` needs a
parameter list. Deciding them together would let the harder one set the schedule
for the easier one, which is the mistake §2 of the core spec corrected once
already when it moved tensors ahead of agents.

**`prompt` fails the test `tool` passes, and it fails on the one thing that
mattered.** §2.4's case for a declaration form rests on obligations a compiler can
check, and the largest is *the schema is the signature*. An MCP prompt's arguments
are **not** schematised — they are a flat list of name, description and a required
flag, with no types at all (§1.2, verified). So a `prompt` declaration has no
schema to derive, no type mapping to check, and no drift to prevent. What is left
is a function that returns a list of messages, which is a function.

```science
# What a prompt is, without a keyword. `f"…"` is already the templating.
function fit_report(run: String) -> Array of Message:
    [Message.user(f"Summarise the peak fit for run {run}. "
                  f"Call fit_peaks first if you have not already.")]

# registered on the server with `.prompts([…])`; see §13.1
```

**Cost, stated.** The asymmetry is visible and someone will ask about it: a server
written in Science has a `tool` keyword and a `prompt` function, which looks
arbitrary until you know why. It is the honest shape, because the two primitives
are genuinely not alike, and this note would rather be asked about an asymmetry
than spend a word to hide one. If MCP ever gives prompt arguments a schema, the
decision is reopened and the word is still there.

---

## 4. The schema is the signature

This is the argument the whole note rests on, and it is the one that is true of
Science and false of Python and TypeScript for a reason that has nothing to do
with taste.

Every MCP SDK makes an author produce two artefacts: a handler, and a JSON Schema
describing what the handler takes. In TypeScript they are two values written side
by side. In Python they are one value and a derivation from it, which is better,
and which still drifts when the annotation is `dict` or `Any` or a string
forward-reference the introspector cannot resolve. **Where two artefacts exist,
they diverge**, and the failure is invisible: the schema says the parameter is a
string, the handler indexes it as a list, and the model is told something false
about a function it is choosing whether to call.

> **Decision 3. A `tool`'s input schema is generated from its parameter list.
> There is no second artefact, there is no way to write a schema by hand, and
> there is therefore nothing for the schema to drift from.**

The interesting part is not the decision. It is **which Science types can and
cannot cross**, because that is where the honest limits are and where the work is.

### 4.1 What crosses, exactly

The target is a JSON Schema object with `type: "object"`, `properties` and
`required`, which is what an `inputSchema` is (§1.4).

| Science type | JSON Schema | Notes |
|---|---|---|
| `Bool` | `{"type":"boolean"}` | Exact. |
| `I8` `I16` `I32` `I64` | `{"type":"integer","minimum":…,"maximum":…}` | **The bounds come from the width.** `I8` emits −128…127. No other SDK's `int` can say this. |
| `U8` `U16` `U32` | `{"type":"integer","minimum":0,"maximum":…}` | Same, and `minimum: 0` is the common case that matters — a count cannot be negative and the model is told so. |
| `U64` | `{"type":"integer","minimum":0}` | **No `maximum`.** 2⁶⁴−1 exceeds what a JSON number represents exactly; emitting it would be a lie. §4.3. |
| `F32` `F64` | `{"type":"number"}` | No bounds. JSON numbers are not floats; NaN and the infinities are **not representable in JSON at all** and a `tool` cannot receive one. §4.3. |
| `String` | `{"type":"string"}` | Exact. Science strings are UTF-8 and so is JSON. |
| `Array of T` | `{"type":"array","items":⟨T⟩}` | Recurses. |
| `Map of (String, V)` | `{"type":"object","additionalProperties":⟨V⟩}` | **`String` keys only.** `Map of (I64, V)` is `SC0507`: a JSON object's keys are strings and nothing else. |
| `T?` | `⟨T⟩`, and the name is absent from `required` | §4.4. Not the same thing, and the difference is stated there rather than hidden. |
| A record `type` whose fields all cross | `{"type":"object","properties":…,"required":…}` | Recurses. This is the `Record` derivation of `data-io.md` §4.3 pointed at a different consumer. |
| A `choice` all of whose variants are unit variants | `{"type":"string","enum":["Gaussian","Lorentzian","PseudoVoigt"]}` | **The largest single win in the table.** §4.5. |
| `Range of T` for integer `T` | a two-property object, `start` and `end` | A convenience the `mcp` package supplies; it is not a language rule. |
| `Quantity of (F64, …)` | `{"type":"number"}` plus a generated description | §6.2. The number on the wire is in SI coherent base units, always. |
| `Tensor of (T, (768,))` — literal shape | `{"type":"array","items":⟨T⟩,"minItems":768,"maxItems":768}` | Nests for higher rank. §6.1. |
| `Json` | `true` — the schema that accepts anything | The escape hatch. §4.6. |

### 4.2 What does not cross, and why each rejection is a compile error

| Science type | Diagnostic | Why |
|---|---|---|
| `borrowed T`, `mutable borrowed T` | `SC0192` | There is no caller to borrow from. The value was deserialized a moment ago and the tool owns it. |
| `function(T) -> U` | `SC0505` | A callable is not data. Nothing on the wire can become one. |
| `any Interface` | `SC0506` | An interface object's concrete type is unknown at the declaration, so there is no schema to emit. Note this is *not* the same rejection as §4.1's — a sealed interface with a known implementation set could in principle emit a `oneOf`, and §20 records that as deliberately not done. |
| A type parameter `T` | `SC0191` | A generic tool has no single schema. §2.4 item 2. |
| `Map of (K, V)` where `K` is not `String` | `SC0507` | JSON object keys are strings. |
| A `choice` with payload-carrying variants | `SC0508` | §4.7. |
| `Tensor` with a symbolic shape | `SC0509` | §6.1. |
| `Frame of Dynamic` | `SC0517` | `data-io.md` §4.4's escape hatch is *"for the first five minutes with a new file"* and its schema is not known before the run. A schema that is not known before the run cannot be in `tools/list`. |
| A recursive record type | `SC0510`, a **warning** | JSON Schema expresses it perfectly with `$defs` and `$ref`. The warning is about the consumer, not the schema: §20. |
| A type alias expanding to any of the above | `SC0512` | Distinct from the others so the diagnostic can carry **two** spans — the alias at the use site and its definition — which is the case where a single span teaches nothing. |

Every one of these is a check on a Science type. Not one names MCP. That is
§2.7's test, applied.

### 4.3 The three numeric holes, named rather than smoothed over

**NaN and the infinities do not exist on the wire.** JSON has no syntax for them.
A `tool` taking `F64` therefore takes a *finite* `F64`, and the schema cannot say
so, because JSON Schema has no "finite" keyword either. The compiler knows a fact
it cannot express and the model is told a type that is slightly wider than the
truth. **Mitigation: none is proposed.** A generated sentence in the description
("finite values only") would be noise on every numeric parameter in every tool.
This is recorded as a limit of JSON, not of Science.

**`U64` and `I64` exceed exact JSON numbers.** A JSON number is not an integer
type; every mainstream parser reads it into a double, which is exact to 2⁵³.
`U64`'s true maximum cannot be written and a tool taking one may silently receive
a rounded value. **Mitigation: the `mcp` package's `U64` deserializer rejects a
number outside ±2⁵³ with an error** rather than accepting a rounded one, which
turns a silent wrong answer into a tool error the model can see and correct.
Stating a `maximum` of 2⁵³ in the schema was considered and rejected: it is a
property of the transport, not of the parameter, and a model that reads it would
conclude the tool has an unusual domain.

**`F32` and `F64` emit the same schema.** The width is lost. A tool taking `F32`
receives a double and narrows it, which may round. This is the one case in the
table where the wire is *wider* than the type rather than narrower, and it is
harmless in every use this note could construct.

### 4.4 `T?`, and the state JSON has that Science does not

JSON distinguishes three things where Science's `T?` distinguishes two: a property
that is **absent**, a property present with the value **`null`**, and a property
present with a **value**. Science has `null` and a value.

> **Decision 4. `T?` means the property is omitted from `required`. A property
> that is absent and a property that is explicitly `null` both arrive as `null`.
> The distinction is not representable and the language does not pretend it is.**

**Cost, stated, and it is a real one.** A tool that wants to mean *"leave this
field unchanged"* versus *"clear this field"* cannot express it with `T?`, because
both spellings arrive identically. That tool must model the third state itself, as
a `choice`:

```science
choice HumidityUpdate:
    Unchanged
    Cleared
    Set
```

— which, being a unit-variant `choice`, becomes a clean `enum` in the schema (§4.5)
and is arguably clearer than the JSON convention it replaces. The cost is that the
author has to know to do it.

**There are no default values**, because Science has no default arguments — §4.2
of the core spec gives named arguments at construction and at call sites, and
nothing else. So `default` never appears in a generated schema. This is a loss:
JSON Schema's `default` is read by some clients and shown to models, and a Python
handler with `limit: int = 20` gets it for free. **A `tool` expresses the same
thing as `limit: U32?` and a `if not limit?:` in the body**, which is one line
longer, visible, and does not require the language to grow default arguments for
one consumer.

### 4.5 The `enum` win, which is larger than it looks

A unit-variant `choice` becomes a JSON Schema `enum`, and this is the place where
Science's ordinary type discipline buys something specifically *model-facing*.

```science
choice LineShape:
    Gaussian
    Lorentzian
    PseudoVoigt
```

```json
{"type": "string", "enum": ["Gaussian", "Lorentzian", "PseudoVoigt"]}
```

Three consequences, and the third is the one that matters:

1. The model is given the complete list of legal values and does not guess.
2. The tool body `match`es on it, and core spec §1's exhaustiveness means adding a
   fourth line shape breaks every body that handles it — **and updates the schema
   in the same edit**, because they are the same declaration.
3. In Python the equivalent is `Literal["gaussian", "lorentzian"]` or an `Enum`,
   and the author must remember to use one. The default — `shape: str` — produces
   `{"type": "string"}` and the model guesses. **In Science there is no such
   default**, because a fixed set of alternatives is spelled `choice` and always
   was. The ergonomic win comes from the language's existing habits rather than
   from anything this note adds, which is the strongest kind.

### 4.6 The escape hatch, and its boundary

`Json` is a type in the `mcp` package — not in the language — whose schema is the
permissive one and whose value is an untyped tree. It exists for the case §2.6's
second falsifier is about: a parameter whose shape is genuinely dynamic.

It is deliberately **inconvenient**: reading a field out of a `Json` returns
`(Json, Error?)`, so every access is three lines under §3's error model, and a
tool that takes three `Json` parameters is visibly worse to read than one that
takes typed ones. That is the intended pressure. `data-io.md` §4.4 made the same
move with `Frame of Dynamic` — *"it is not the default, it is not what the worked
example uses, and no library function returns it unless you ask for it by name"* —
and the boundary here is drawn in the same place and for the same reason.

### 4.7 The one rejection that will be argued about

A `choice` with payload-carrying variants — `choice Window: Channels(U32, U32)
Microseconds(F64, F64)` — is **rejected** (`SC0508`), and JSON Schema can in fact
express it, with `oneOf` and a discriminating property. Three reasons, in
increasing weight:

- There is no canonical encoding. Externally tagged, internally tagged, adjacently
  tagged and untagged are four conventions and every serialization library in
  every language argues about which is the default. Choosing one in the language
  would be choosing it forever.
- Models handle `oneOf` poorly in practice. A schema that is correct and that the
  caller cannot follow is worse than one that is narrower and obvious.
- **A payload-carrying `choice` in a tool signature is almost always two tools.**
  `fit_by_channel` and `fit_by_time` are more readable than one tool with a tagged
  union, they produce two obvious entries in `tools/list`, and the model's choice
  between them is the choice the union was encoding anyway.

The third reason is the one that makes the rejection right rather than merely
convenient, and it is the argument to make to whoever objects.

---

## 5. The description is the doc comment, and the compiler requires it

A tool's description is what a model reads in order to decide whether to call the
tool at all. It is not documentation of the implementation; it is **an input to
the caller's control flow**. A server whose tools are perfectly implemented and
badly described is a server that is never called correctly, and the failure is
silent — nothing errors, the model simply does something else.

`strings-formatting-and-docs.md` §5 already decided everything needed here, and
the decisions happen to be the right ones. They are checked against rather than
assumed:

| That note's decision | What this note needs | Fits? |
|---|---|---|
| §5.1 — a doc comment is a run of `##` lines **before** the declaration | A description attached to a `tool` | **Yes**, exactly |
| §5.3 — the lexer keeps `##` runs as trivia on the following token, the parser attaches them to the AST, they survive into HIR, **and this must happen in F0** | The description available to a front-end pass | **Yes**, and it is the reason §14 can ship something before the type checker |
| §5.4 — the first line, up to the first blank `##`, is the summary; the rest is the body | A `title` and a `description`, which are two fields (§1.2) | **Yes** — §5.2 below |
| §5.4 — Markdown, no `@param` tags | A description a model reads | **Yes.** Models read Markdown; and §5.3 below is careful not to reopen the `@param` refusal |
| §5.2 — the attachment table | A description **per parameter** | **No.** Parameters are not in that table. §5.3 is the one ask this note makes of it |

### 5.1 Decision 5: a `tool` without a doc comment does not compile

> **Decision 5. A `tool` declaration with no `##` run immediately preceding it is
> `SC0190`, an error. A `tool` whose summary line is empty is `SC0195`. This is
> the only construct in Science for which documentation is mandatory.**

The justification has to be narrow or it becomes "document your code", which is a
style rule a compiler has no business enforcing. It is narrow:

- **The string is a parameter, not prose.** Everywhere else in Science, a doc
  comment is read by a human who has already decided to call the function. Here
  it is read by the caller *in order to* decide, and the compiler knows this
  because the author wrote `tool`. Requiring an argument is not a style rule.
- **The failure it prevents is invisible.** An undescribed tool does not error, it
  is simply not chosen, and no test catches that. This is the same reasoning §5.2
  of the strings note used for `SC0176` — *"the silent failure mode of every
  doc-comment system is documentation that was written and never appears
  anywhere"* — arriving at a worse version of the same problem.
- **It is checkable with what exists.** §5.3's attachment happens in the parser.
  `SC0190` is a parser-level or resolver-level check and needs no types at all,
  which is why §14 can ship it first.

**Rejected: a lint.** A lint that everyone disables is worse than nothing, and a
lint that nobody disables is an error with extra steps. **Rejected: requiring it
only for `public` tools.** Every tool is public — it is on the wire — so the
distinction does not exist here.

**Cost, stated, and it is not zero.** Three things.

- **It makes `##` load-bearing for the first time in the language.** Up to now a
  doc comment could be deleted and the program would behave identically. For a
  `tool` it cannot.
- **It is friction on the smallest possible server.** A three-line toy tool now
  needs four lines. This is accepted: the smallest server is not the case worth
  optimising, and a toy server with undescribed tools teaches the wrong habit at
  exactly the moment a beginner is forming one.
- **It contradicts a sibling note in one sentence.** §5.5 below.

### 5.2 `title` and `description`, from the summary rule that already exists

MCP's `Tool` carries both a human-facing `title` and a model-facing `description`
(§1.2). §5.4 of the strings note already splits a doc comment in exactly that
place, for its own reasons, and this note does not invent a second rule:

> **Decision 6. `title` is the summary — the first line, up to the first blank
> `##`. `description` is the whole comment, summary included.**

```science
## Fit peaks in a time-of-flight window.
##
## Uses the run's own dark-current subtraction. Returns one row per peak with
## its centre, width and integrated intensity. Fails if the window contains
## fewer than 32 channels, because the fit is then underdetermined.
tool fit_peaks(...):
```

`title` is *"Fit peaks in a time-of-flight window."*; `description` is all five
lines. The duplication of the summary is deliberate: a model reading only
`description` must not be missing the sentence that says what the tool is for.

### 5.3 Per-parameter descriptions, and the one ask this note makes

A JSON Schema property carries its own `description`, and that is where a model
looks to find out what a parameter *means* — that `window_lower` is an edge rather
than a centre, that `run` is the identifier from the log rather than a filename.
Science has no syntax for documenting a parameter, because §5.4 of the strings
note deliberately refused `@param` tags.

**That refusal is not in the way, and the reason is worth being precise about.**
§5.4 rejected `@param` on the ground that *"the parameter names and types are in
the signature, which the compiler knows exactly, and a tag that restates them is a
second copy that goes stale."* That argument is entirely about a tag inside prose
duplicating the signature. It says nothing against a doc comment attached to a
parameter **in the signature**, which is not a second copy of anything — it is the
same move §5.2 already blesses for a record field:

```science
## A single detector reading.
type Row:
    ## Instrument identifier, as written in the run log.
    station: String
```

> **Decision 7. A `##` run may precede a parameter in a `tool` declaration's
> parameter list, and becomes that property's `description`. It is optional;
> `SC0190` requires a description for the tool and nothing requires one per
> parameter, for the same reason §5.4 refused to check that a prose list covers
> every parameter — a checker that demanded one would produce
> `## the max_peaks`.**

```science
## Fit peaks in a time-of-flight window.
tool fit_peaks(
        ## The run identifier, as printed in the run log.
        run: String,
        ## Lower edge of the fit window, in seconds from the pulse.
        window_lower: Time of F64,
        ## At most 32; the fit is underdetermined above that.
        max_peaks: U8) -> (PeakTable, Error?):
```

**This is an ask of `strings-formatting-and-docs.md` §5.2**, whose attachment
table does not list parameters, and it is the one on which §2.5's choice of Option
C over Option B depends. It is a small ask — the table gains a row reading
*"Parameter of a `tool` declaration | before the parameter line"* — and it is
deliberately scoped to `tool` and not to `function`, so that the general question
of documenting parameters stays open and belongs to that note. A `##` before an
ordinary `function`'s parameter is `SC0194`, with the fix "move it into the doc
comment above the declaration".

**Cost, stated.** A `tool`'s parameter list is now multi-line by convention, which
makes short tools longer than the same `function` would be. That is the price of
the thing the brief put first.

### 5.4 Doc tests become the examples a model reads, and they cannot be wrong

§5.5 of the strings note specifies doc tests: a ` ```science ` fence inside a `##`
run is compiled and run, and a neighbouring ` ```output ` fence is compared to
stdout byte for byte. That decision was made for documentation quality, and it
lands here as something else.

> **Decision 8. A `tool`'s doc comment, fences included, is the `description` sent
> to the model. A ` ```science ` fence in a tool description is therefore both a
> compile-verified test and a worked example the model reads.**

This is the argument in this note that is most specifically *this language's*, and
it survives the test the brief set: it is false of Python and TypeScript, and not
for a contingent reason. Both can put an example in a docstring; neither compiles
it. A docstring example that has drifted from the handler is the single most
common defect in published MCP servers, and it is worse than an absent one,
because a model follows it.

Science's version cannot drift, because §5.5 already compiles it:

````science
## Convert a time of flight to a d-spacing by Bragg's law.
##
## ```science
## let d, err be d_spacing(0.0142<s>, 40.0<m>, 1.5708<rad>)
## print(f"{d.in(ANGSTROM):.4f}")
## ```
## ```output
## 0.9931
## ```
tool d_spacing(...) -> (Length of F64, Error?):
````

**Cost, stated, and it is the sharpest cost in this note.** §5.5 of the strings
note lists doc tests as "a fifth test layer", "a real cost on every library build"
and a thing that "must be a separate target, not part of `sciencec build`". For a
`tool`, the fence is part of the shipped artefact — the description is in the
binary and goes out on the wire. So for tools specifically, **the doc test cannot
be an optional target**: a build that skips it ships an unverified example to a
model. This note asks that `sciencec build` run doc tests for `tool` declarations
only, which is a narrow exception to that note's rule and is the smallest version
of it that keeps Decision 8 true. If that is refused, Decision 8 degrades to "the
example is shipped and is verified when someone runs the doc-test target", which
is what every other SDK has, and the argument is lost.

### 5.5 The contradiction, recorded rather than smoothed over

`strings-formatting-and-docs.md` §5.3 says of the doc-comment metadata section:

> Static data, not reachable from any code path, **strippable**.

**For a `tool`, it is not strippable.** The description is the tool's behaviour as
far as any caller is concerned; a binary built with doc comments stripped exposes
tools with empty descriptions and is silently useless. The sentence needs one
clause: *strippable, except for the descriptions of `tool` declarations, which are
program data.*

There is a second, smaller edge on the same seam. That note's §5.3 has the
compiler emit doc comments "into a metadata section of the object file: a string
table plus an index from fully-qualified name to offset". A `tool`'s description
must be reachable from the `tools/list` handler, which means it is either a
reference into that same table — in which case the table is reachable and the
"not reachable from any code path" clause is also wrong for tools — or it is
duplicated into ordinary read-only data. **This note recommends the latter**: the
`tool` description is emitted as an ordinary string constant, the metadata table
stays exactly as §5.3 describes it for every other declaration, and the two
mechanisms do not have to be reconciled. The duplication is a few hundred bytes
per tool.

---

## 6. Units and shapes in the schema — the argument, demoted

The brief asked whether Science's two distinguishing type-level facts — physical
units and const-generic tensor shapes — are expressible in JSON Schema, and
whether it actually helps a model or is a party trick. Both halves of the question
have answers and they are not the same answer.

### 6.1 Shapes: literal yes, symbolic no

JSON Schema has `minItems` and `maxItems`. Setting them equal is exactly a fixed
length, and it nests.

```science
# Tensor of (F32, (768,))
{"type": "array", "items": {"type": "number"}, "minItems": 768, "maxItems": 768}

# Tensor of (F32, (3, 768))
{"type": "array", "minItems": 3, "maxItems": 3,
 "items": {"type": "array", "items": {"type": "number"},
           "minItems": 768, "maxItems": 768}}
```

**This works, it is checked by every JSON Schema validator, and it is a real
statement no other SDK's type system can make.** A Python handler annotated
`np.ndarray` produces no schema at all; annotated `list[float]` it produces one
with no length.

**A symbolic shape does not cross, and this is `SC0509`.** `Tensor of (F32,
(batch, 768))` has a shape variable, and `const-expression-arithmetic.md`'s
normal form `k + Σ cᵢ·aᵢ` has nothing to evaluate it to at the moment the schema
is emitted. There is no JSON Schema vocabulary for "the same length as the other
parameter" — the draft's `$data` proposal does that and it is not in any release
and not implemented anywhere. So a tool taking two vectors that must agree in
length checks that **inside the body** and returns an error, exactly as a
Python tool does, and the schema says only that both are arrays of numbers.

**Verdict on shapes: survives, narrowly.** A fixed shape is genuinely better than
any alternative. A dynamic shape is exactly as good as Python's, which is to say
not at all. Since the interesting scientific tools mostly take *dynamic* arrays —
a spectrum is however many channels the instrument has — the win applies to the
minority case. It is real and it is small, and the note would rather say so than
sell it.

### 6.2 Units: the schema gains nothing, and the author gains a lot

`unit-literals.md` §5.1 is the fact that decides this:

> Every `Quantity` value is stored in SI coherent base units.

And its §3.3 gives the type spelling: `Length of T` is
`Quantity of (T, 1, 0, 0, 0, 0, 0, 0)` — **the dimension is in the type and the
unit is not**. A `Quantity of (F64, …)` is a one-field wrapper over `F64`, so its
schema is `{"type": "number"}` and there is nothing else JSON Schema can say. Its
`description` can carry the coherent unit name, and the compiler writes that
sentence rather than the author, so it cannot be wrong.

An `x-unit` annotation was considered. JSON Schema ignores unknown keywords, so
`{"type":"number","x-unit":"kg"}` is legal and harmless — and it is also read by
nothing, so it would be a keyword invented for an audience of zero. **Rejected.**
The unit reaches the model through the description or it does not reach it.

**So does it help the model?** Marginally. A model told *"mass, in kilograms"*
instead of *"mass"* is better off, and models mostly infer the unit from the
parameter name anyway. As a model-facing feature this is close to a party trick
and the brief was right to suspect it.

**The win is on the other side of the boundary, and it is not a trick.** The value
arrives as a bare number and is a `Quantity` from the first line of the body, so
every dimensional mistake inside the tool is a compile error under
`unit-literals.md`'s Stage B — a tool that multiplies a time by a length and
assigns it to a length does not build. In Python the parameter is a `float` and
stays one. **The unit is declared to the model and checked against the
implementation, and only the second half is worth anything.**

> **Decision 9. A `Quantity` parameter's schema is `{"type":"number"}` and its
> generated description names the SI coherent unit. The wire value is always in
> SI coherent base units. No unit keyword is invented.**

**The cost is the one that will annoy people, and it lands on `unit-literals.md`
§7.3's open seam.** The type knows the dimension, not the unit. So a tool taking a
time of flight takes it in **seconds**, not microseconds, because seconds is the
coherent unit for the time dimension — and a neutron time of flight is a number
like `0.0142`, which is not how anybody at the instrument writes it. The author's
choices are to accept seconds, or to take a bare `F64` named `microseconds` and
lose the dimensional check inside the body. Neither is good.

§7.3 of that note calls printing in a non-stored unit "the one open seam" and
defers it. **This note is a second caller for it**, and the ask is concrete: a way
to say, at the declaration, which unit a `Quantity` parameter is *written* in, so
that the description says "microseconds", the conversion happens at the boundary,
and the body still holds a `Quantity`. Until that exists, §13's worked example
takes seconds and says so in the description, which is honest and slightly
unpleasant.

### 6.3 What is rejected from this area

**Rejected: putting the unit in the schema as a keyword.** Nothing reads it.

**Rejected: emitting a `pattern` for a unit-bearing string parameter** — that is,
accepting `"14.2 us"` as a string and parsing it. It would let the model write the
unit it is thinking in, which is genuinely attractive, and it is refused because
it moves the parse from the schema into the body, turns a type error into a
runtime error, and invents a unit-string grammar on the wire that the language
would then own. The `Quantity` literal grammar of `unit-literals.md` §3.1 is a
*source* grammar and making it a *wire* grammar would be a much larger commitment
than it looks.

---

## 7. The error model, checked against `isError`

The brief asked whether `-> (Content, Error?)` maps onto `isError` exactly or only
nearly. The answer is **nearly, and the gap is in a more interesting place than
expected**: the direction that matters maps exactly, and what is lossy is the
*shape* of the error rather than its classification.

### 7.1 Two channels, and they are two places in the program

§1.4's rule sorts failures into protocol errors and tool execution errors. Mapping
that onto Science is unusually clean, because the two categories correspond to two
**distinct places in the program**, and the tool author has access to only one of
them:

| MCP category | Where it happens in a Science server | Reaches the model as |
|---|---|---|
| Errors in *finding* the tool; unsupported method; malformed JSON-RPC | The `mcp` package's dispatcher, before any tool body runs | A JSON-RPC error |
| **Input validation** — a parameter absent, of the wrong type, out of range | The `mcp` package's deserializer, before the body runs | **`isError: true`** (§1.4, SEP-1303) |
| Anything the tool body returns as a non-null `Error?` | The tool body | **`isError: true`** |

> **Decision 10. Every non-null `Error?` returned by a `tool` body becomes
> `isError: true` with the error's `message()` as a single `TextContent` block.
> A `tool` body cannot produce a protocol error, because a protocol error is by
> construction something that happened before it was called.**

That is a clean map and it is clean for a reason worth stating: Science's error
model is **one channel**, and the protocol's two channels are separated by *where*
rather than by *what*. A Go-style `(T, Error?)` has exactly one place to put a
failure, and that place is precisely the place the protocol calls a tool execution
error. A language with exceptions would have had to decide which throws are which,
which is the choice every other SDK's middleware has to make and get wrong for
some caller.

**The input-validation row is the one that surprised.** It would be natural to
assume a schema violation is a protocol error — the request is malformed against
the contract. The specification says the opposite, deliberately (SEP-1303): a
model can fix a wrong date format and cannot fix a missing method, so validation
failures go back through the channel the model can see. **So the `mcp` package's
deserializer must produce `isError: true` and a message the model can act on**, and
this is the one place where the quality of the library's generated prose matters as
much as the author's.

### 7.2 Where the map is lossy, and the decision not to fix it

`syntax-revision-2.md` §3.4 makes `Error` an interface with exactly one method:

```science
interface Error:
    function message(self) -> String
```

An MCP tool error carries **`content: ContentBlock[]`** — it can be an image, a
resource link, an embedded resource, several blocks. `Error` can be a string.

So a Science tool cannot return an error that includes a plot of the failed fit,
which is a genuinely useful thing for a scientific server to do.

**Rejected: extending `Error`, or defining a richer `ToolError` interface in the
language.** This is the one place in the note where the temptation to reach into
the language for a protocol feature is strongest, and refusing it is the note's own
Decision 1 applied to itself. A richer error interface would be a language-level
change made for a protocol-level want — the exact move §2 spent its length
arguing against — and it would put MCP's content-block model into Science's error
type, where every non-MCP user of `Error` would then meet it.

**The workaround, and it is honest rather than good.** A tool that wants a rich
failure returns it as a *successful* `Content` that describes the failure, with
`err` null and `isError` therefore false. The model sees the images; the client
does not see `isError`. That is a real loss of signal and it is recorded rather
than papered over. If it turns out to matter, the right fix is in the `mcp`
package — a `ToolError` type the package defines, which implements `Error` for
`message()` and carries content blocks besides, so the language's `Error` is
unchanged and the package's serializer checks for its own type. That is available
whenever somebody needs it and is not built now.

### 7.3 `outputSchema`, `structuredContent`, and the return half of Decision 3

§4 derives the *input* schema from the parameter list. The same derivation runs on
the return type.

> **Decision 11. A `tool` returning a record type or an `Array` of one emits an
> `outputSchema` and serializes into `structuredContent`. A `tool` returning
> `Content` emits no `outputSchema`. Both forms also emit the serialized JSON in a
> `TextContent` block, because §1.4 says a server SHOULD.**

`outputSchema` is not constrained to `type: "object"` (§1.3) and `structuredContent`
is any JSON value (§1.4), so `Array of Peak` emits an array-typed schema directly
with no wrapper object. This is a `2026-07-28` loosening; a note written against an
earlier revision would have had to invent a wrapper, and §13.2 shows the
unwrapped form because that is what the current revision admits.

**The duplication is the specification's, not this note's.** Emitting the same data
twice — once structured, once as serialized text — is what `SHOULD` asks for, and
it roughly doubles a result's size. The `mcp` package does it by default and offers
a way off. That is a library decision recorded here so that whoever implements it
does not think they have found a bug.

**`resultType` is always `"complete"`.** The other value, `"input_required"`,
belongs to elicitation, which §15 does not ship. A Science tool body is a
synchronous function that returns a value; there is no point in it at which it can
ask the user a question. When elicitation is shipped, this is the field that
changes and the tool signature that would have to change with it, which is worth
knowing before somebody promises it.

### 7.4 The `SC0140` interaction, which is a small piece of luck

`syntax-revision-2.md` §3.3 makes an unchecked error a compile error rather than a
lint: *"if a binding of a nullable error type is never tested with `?` before the
function returns, that is `SC0140`."*

In a server, that check is worth more than it is anywhere else. A dropped error in
a batch script produces a wrong number that somebody eventually notices. A dropped
error in a tool body produces a **result with `isError` unset**, which the
specification says *"is assumed to be false (the call was successful)"* — so the
model is told the call succeeded, and acts on whatever was in the value slot.
`SC0140` makes that specific failure impossible, and it is not a feature this note
asked for or paid for.

---

## 8. Tool annotations, and the only SDK that can check one

MCP tools carry `annotations` — `readOnlyHint`, `destructiveHint`, `idempotentHint`
and `openWorldHint` (§1.2). In every SDK in existence these are literals the author
types, and the specification says clients **MUST** consider them untrusted unless
they come from a trusted server (§1.6). They are unverified assertions about
behaviour.

Two of the four default to `true` — `destructiveHint` and `openWorldHint` — which
is the fail-safe direction and which is what makes the decision below possible
(§1.2).

`effects.md` gives Science the machinery to check two of the four. Its Decision 2
attaches three inferred bits to every function — `python`, `ambient`, `external` —
and its Decision 5 gives exactly one place to write an assertion about them:

> A function may be declared `pure function`, which is a compiler-checked
> assertion that its inferred effect set is empty.

> **Decision 12. `pure tool` is the same assertion on a tool. A tool whose
> inferred effect set is empty emits `readOnlyHint: true`, `openWorldHint: false`
> and `idempotentHint: true`. A tool that is not `pure` emits none of the four.
> No new keyword is spent and no new diagnostic is allocated: a `pure tool` whose
> effect set is non-empty is `effects.md`'s existing `SC0214`, with its witness
> chain.**

```science
## Convert a time of flight to a d-spacing by Bragg's law.
pure tool d_spacing(
        ## Time of flight, in seconds from the pulse.
        time_of_flight: Time of F64,
        ## Total flight path, in metres.
        flight_path: Length of F64,
        ## Scattering angle, in radians.
        two_theta: Angle of F64) -> (Length of F64, Error?):
    ...
```

`readOnlyHint: true` on that tool is not a claim. It is a consequence of the fact
that the body cannot reach the filesystem, the network, the clock, the
environment, the entropy source or any `extern` function that has not itself been
declared `pure` — because `effects.md` §2.1 closed every ambient channel through
which it could, and §3.5 propagates the bits over the resolved call graph
conservatively.

### 8.1 The honest limits, of which there are three

**One: the negative direction only.** The three bits say *whether* a function
touches the world, not *how*. A tool carrying `external` might read a file or
delete one, so `destructiveHint` cannot be computed at all, and `readOnlyHint` can
only be emitted as `true` — never as `false`, which would be a claim the compiler
cannot support. **This is why Decision 12 emits nothing rather than `false` for an
impure tool**, and it is safe to do so for a reason that had to be checked rather
than assumed: §1.2 confirms `destructiveHint` and `openWorldHint` **default to
`true`**. Omitting an annotation therefore lands on the conservative reading, not
on a permissive one. Had the defaults gone the other way, Decision 12 would have
had to emit something for every tool and would have been worth much less.

**Two: the client is told not to trust it.** §1.6's MUST is about annotations
specifically. So the value of a *checked* annotation does not accrue to the model
at the far end of the wire — a client that distrusts a hint distrusts a true one
identically. It accrues to **the author**, who cannot mark a tool read-only and
then add a `write_file` to it, and to **a reviewer**, who reads `pure tool` and
knows it was checked rather than typed. That is a smaller win than it first appears
and it is still a win no other SDK offers.

**Three: it needs the effect query, which does not exist.** `effects.md` §3.5
specifies `effects(DefId) -> EffectSet` as a salsa query over the resolved call
graph. The resolver exists; the query does not. §14 places this.

**`idempotentHint: true` from purity deserves one sentence of scepticism.** A pure
function is idempotent in the sense the annotation means — calling it twice with
the same arguments has the same effect as calling it once, because it has no
effect. That is sound, and it is also the least interesting of the four
annotations, since the tools people worry about being called twice are exactly the
impure ones.

### 8.2 The same machinery catches a bug that eats an afternoon

This one was not on the brief's list and it is the most immediately useful thing
in the section.

§1.5's stdio rule: a server **MUST NOT** write anything that is not an MCP message
to stdout, and messages **MUST NOT** contain embedded newlines. `print` writes a
line to stdout. So **one `print` left in a tool body corrupts the protocol
stream**, and the symptom is a client that reports a parse error with no reference
to the line that caused it.

Every MCP author in every language has had this bug. In Python it is a stray
`print` in a library three levels down; the standard advice is "never print to
stdout in an MCP server", which is advice, not a mechanism.

**Science can see it.** `effects.md` §3.1 seeds the `external` bit from a closed
list of primitives and `print` is on it by name. So the question *"is `print`
reachable from this tool body?"* is a query over the same call graph, with the same
witness chain, that §8 already needs — it is not new machinery, it is one more
seeded primitive read out of an existing answer.

> **Decision 13. `SC0519`, a warning: `print` or `write` is reachable from a `tool`
> body in a program that links the stdio transport. The message carries the witness
> chain from the tool to the call, and the fix is `logging`, which writes to
> stderr — which the specification explicitly permits for *"any logging purposes
> including informational, debug, and error messages"*, and which clients are told
> not to read as an error.**

**Three caveats, because the check is not airtight.**

- It is a **warning**, not an error, because "links the stdio transport" is known
  at the link step and a library crate compiled on its own cannot know it.
- It is conservative in the direction that produces false positives:
  `effects.md` §3.5 item 5 contributes the full effect set at any call through
  `any Interface`, so a tool that dispatches dynamically will be warned about even
  when it never prints. That is the right direction for a warning and the wrong one
  for an error, which is a second reason it is not one.
- It does not catch a `print` from inside a linked C library, which
  `ffi-c-boundary.md` cannot see into either.

Even with all three, it turns the single most common structural bug in MCP servers
from an afternoon of confusion into a line of build output, and it costs one
diagnostic and no new analysis.

---

## 9. Long-running tools, which for this audience is the median case

Every other MCP SDK treats a long operation as unusual. A file read returns in
milliseconds; a database query returns in tens; the rare slow tool is a special
case with a special mechanism bolted beside it.

**For a scientific server that ordering is inverted.** A peak fit over a 40 GB run
takes minutes. A molecular dynamics step takes longer. A structure search, a
Bayesian sampler, a docking run, a genome alignment — the median tool in the
audience's world is slow, and "slow" is measured in the units a coffee break is
measured in. A design that treats this as an edge case has treated the whole
audience as an edge case.

This section was an open question in an earlier draft of this note, recorded as
the strongest live argument for rejecting `tool` as a declaration. It is answered
here, and the answer turned out to be shorter than the objection, for a reason
that had to be verified rather than guessed.

### 9.1 Decision 14: a long tool is a task, and `tool` gains nothing to say so

The Tasks extension was read the way §1 read everything else, and three facts
decide this section.

**One: Tasks is not in the specification repository.** It has its own repository,
`modelcontextprotocol/ext-tasks`, its own schema and its own release train. The
main repository carries a stub that says only that the extension repository *"contains
the full specification and documentation for MCP Tasks."*

**Two: it is marked `Stable`, not experimental.** The extension's README carries a
version table whose `2026-07-28` row reads **`Stable`**, and *"Released schema
directories are immutable snapshots with version-specific JSON Schema
identifiers."* SEP-2663 is **Final** — the file exists at `seps/2663-tasks-extension.md`,
it is not an open pull request — and it states the intent plainly:

> Tasks will become a foundational building block of MCP and are expected to be
> supported in future protocol versions. […] Once the extension has stabilized and
> achieved broad adoption, it is intended to be promoted into the core protocol.

There is **no** "experimental", "draft" or "subject to change" banner anywhere on
the specification, the schema or the rendered page. The only hedges are that
extensions are *"always disabled by default and require explicit opt-in"* and that
they *"evolve independently of the core protocol"*. So it is safe to design
against, with the caveat §19 records.

**Three, and this is the one that decides the section: task creation is purely
server-directed, and there is no field on `Tool`.** Verbatim:

> Task creation is server-directed: the client signals support by including the
> extension in its per-request capabilities, and the server decides on a
> per-request basis whether to materialize a task.

> A server that has negotiated this extension **MAY** return `CreateTaskResult` in
> lieu of a standard result (e.g. `CallToolResult`) in response to any supported
> request at its own discretion and on a per-request basis. The server is the sole
> decider; clients do not signal task preference on the request itself.

And the overview page, in as many words: *"**No per-tool warmup or per-request
flag.**"*

**This is a deliberate removal, not an omission.** `2025-11-25` had
`Tool.execution.taskSupport?: "forbidden" | "optional" | "required"`, and
SEP-2663's motivation is that it was the wrong shape: *"The handshake is fragile.
[…] A client that wants to opt into tasks must therefore prime its state with a
`tools/list` call before issuing any task-augmented request. […] This is
confusing, implicit, and easy to get wrong."*

> **Decision 14. A long-running tool returns a task, and the `tool` declaration
> says nothing about it. There is no `long` modifier, no `task` keyword and no new
> field in the schema, because the protocol deliberately removed the per-tool
> field that would have been the thing to generate. Whether a given call becomes a
> task is a **dispatch policy** in the `mcp` package.**

This is the best possible outcome for §4 and it is worth being explicit about why:
**the schema-is-the-signature decision survives Tasks completely untouched.**
`Tool` gains no field, `inputSchema` is unchanged, `tools/list` is unchanged, and
`sciencec tools --json` emits exactly what it emitted before. The single largest
open question against Decision 1 closed without costing Decision 3 anything, and
it closed because a protocol committee independently reached the conclusion that a
per-tool declaration was the wrong place for this.

**Had Tasks kept `execution.taskSupport`, this section would read very
differently**, and it is worth saying so rather than claiming foresight: a per-tool
protocol field would have been a field on the `Tool` object that no Science type
produces, which is precisely the *"one exception to the derivation"* §9.2 rejects
in another form two pages from here.

#### The dispatch policy, since it now has to exist

The protocol made it the server's decision, so somebody has to make it. Two shapes
were considered.

**Rejected: an author-declared list.** `Server.new(…).tools([…]).as_task([fit_peaks])`
is honest and it is a worse version of the field the protocol just deleted — it
requires the author to predict which calls are slow, and `fit_peaks` over 300
channels is fast while `fit_peaks` over 40 GB is not. The predicate is about the
*call*, not the *tool*, which is exactly what "per-request basis" means.

> **Decision 14a. The `mcp` package promotes a call to a task on a time
> threshold. The body runs on a worker thread; if it has not returned within
> `task_after`, the dispatcher returns a `CreateTaskResult` and the work continues.
> The default is a few seconds and it is declared once per server.**
>
> ```toml
> [server]
> task_after = "5s"           # 0 disables promotion; a call then always blocks
> ```

This is legal because the decision point is *when the server responds*, not when it
starts work, and the specification's only hard requirement is durability before the
handle is issued: *"A server **MUST NOT** return `CreateTaskResult` until the task
is durably created — that is, until a `tasks/get` for the returned `taskId` would
resolve."* A dispatcher that registers the task before responding satisfies that
trivially.

It also means **a fast call is never a task** — `d_spacing` returns in microseconds
and the model gets its answer in one round trip, with no polling, no `taskId` and
no extra vocabulary — while the same code path handles a fit that runs for an
hour. The author writes one tool.

#### What the extension actually is, in three lines

Three methods, and two that existed in `2025-11-25` are gone: `tasks/get`,
`tasks/update`, `tasks/cancel`. `tasks/list` and `tasks/result` were **removed** —
the first because *"`tasks/list` scoping cannot be defined"* and, on the security
side, because *"a server cannot inadvertently leak the existence of one caller's
tasks to another"*; the second because it was *"a blocking trap"*.

Status is one of exactly five strings — `working`, `input_required`, `completed`,
`failed`, `cancelled` — with the last three terminal. `ttlMs` is **required and
nullable**; `pollIntervalMs` is optional. Polling `tasks/get` is the default and
only guaranteed mechanism; push exists as `notifications/tasks` over
`subscriptions/listen` and is opt-in on both sides.

**Three consequences for a Science server, each of which is a small decision:**

- **`ttlMs` must be honest, and for a stdio server it is short.** A stdio server is
  a subprocess; its tasks die when the client stops it. There is no `tasks/list`,
  so a lost `taskId` is unrecoverable by design and the specification tells clients
  to *"persist task IDs to durable storage"*. A server that cannot honour that
  should not imply it can: the `mcp` package sets `ttlMs` to the process's own
  expectation rather than `null`, and `null` — meaning unlimited — is available
  only for a server that actually persists tasks, which §15 does not ship.
- **`failed` is not for tool errors, and this is the third time the same split
  appears.** Verbatim: *"The `failed` status **MUST NOT** be used to represent
  non-JSON-RPC errors, such as a tool result that completed with `isError: true`.
  […] the task uses `completed` status and the `result` field **MUST** contain that
  result."* So §7.1's Decision 10 passes through the task layer unchanged — a
  Science `Error?` becomes `isError: true` inside a `completed` task. **Science's
  one-channel error model lands on the same side of the split at both layers**,
  and it does so without a rule, because a tool body still cannot produce a
  JSON-RPC error.
- **`input_required` is not ours.** A task's `inputRequests` carry elicitation,
  sampling and `roots/list` requests, and §15 ships none of them. A Science task
  goes `working → completed | failed | cancelled` and never enters
  `input_required`.

#### The finding that costs the most: tasks have no progress

This is the one that hurts, it is normative, and it would have been easy to get
wrong from memory:

> `notifications/progress` and `notifications/message` notifications **MUST NOT**
> be sent on the `subscriptions/listen` stream for a task, and **are not supported
> on tasks in general in this specification.**

A task's only progress channel is `statusMessage`, a free-text field on the `Task`
object that a caller sees when it polls. There is no fraction, no total, and no
push.

So the two paths a Science tool body's progress can take are **not equivalent**,
and §9.3's API has to be designed for the weaker one:

| | Inline call, client supplied `_meta.progressToken` | Promoted to a task |
|---|---|---|
| `notifications/progress` with `progress` / `total` | **Yes** | **Forbidden** |
| A human-readable message | Yes, the `message` field | Yes, `statusMessage` |
| Push to the client | Yes | No — the client polls `tasks/get` |

> **Decision 14b. `progress.report` takes a fraction **and** a message, because
> the message is the half that survives both paths. On an inline call both are
> sent; on a task the message becomes `statusMessage` and the fraction is
> formatted into it by the library rather than dropped silently.**

A fraction rendered into text — `"[ 43% ] fitting peak 3 of 7"` — is worse than a
structured one and it is not nothing, and it is the most the protocol permits. The
alternative, dropping the fraction on the task path, would make the same tool body
behave differently depending on a dispatch decision the author did not make.

**Two smaller normative points that the library must absorb so the author never
meets them.** Progress values *"**MUST** increase with each notification"*, so the
`mcp` package clamps a non-monotonic report rather than letting a loop written
backwards produce a protocol violation. And progress tokens are client-supplied and
optional — *"The receiver is not obligated to provide these notifications"* — so
`progress.report` on a call with no token is a no-op, which is the same no-op
§9.3's testability argument depends on.

### 9.2 The sink problem, and the three shapes it could take

A tool body is a function. Progress reporting needs it to reach something that is
not one of its arguments and not its return value. The three ways to arrange that
are not equally priced.

**Option A — a distinguished first parameter, excluded from the schema by rule.**
What most SDKs do. It costs exactly one thing and the cost is larger than it
looks: **the input schema stops being "the parameter list" and becomes "the
parameter list, minus a parameter you have to know about".** §4's Decision 3 is a
derivation, and this makes it a derivation with an exception. Two concrete
consequences:

- A reader of a `tool` declaration can no longer compute the schema by reading the
  parameters; they must also know the exception and recognise the type that
  triggers it. That is precisely the readability the brief put first.
- §14.3's `sciencec tools --json` — the thing that ships first and proves the whole
  thesis — needs a type-level special case in the one pass that is supposed to be a
  mechanical walk. The first exception is cheap; §19 says what exceptions do to a
  source-of-truth rule, and the request after this one is always `context`, then
  `session`, then `client`.

**Rejected**, and the amount it costs is that sentence: the derivation would stop
being total, and a total derivation is the only reason §2 spent a keyword.

**Option B — a fourth effect bit.** This is right about the *shape* — progress is a
property of a body and not of its signature, which is exactly where effects
already live in this language, and it is the same reasoning §8 used to get
`readOnlyHint` out of the effect lattice. It is wrong about the *mechanism*, for a
reason already settled: `effects.md` Decision 2 closes the set at three bits and
calls the closure
*"the most load-bearing sentence in the note"*, and `reproducibility.md` §4.1
already proposed a fourth bit and **withdrew it** for that reason. This note is not
going to be the accretion that note refused to be.

It is also unnecessary, because an effect bit answers *"does this body reach X"* and
does not *supply* X. The bit is not the hard part. The sink is.

**Option C — an ambient sink, under an exception this project already made.**

### 9.3 Decision 15: progress is ambient, and the logger already paid for it

`effects.md` §2.1's inventory lists every ambient channel Science closed, and
exactly one row reads *"Exists, deliberately"* — the process-global logger. The
justification, from `stdlib-standard.md` §8.2, is a single test:

> **Ambient state is acceptable exactly when it cannot change the program's
> computed result.**
>
> A global RNG changes the answer. A global logger changes what is printed.

**Progress reporting is on the logger's side of that test and it is not a close
call.** A progress notification cannot change what a tool computes; it is, almost
literally, a log line with a number on it. §1.5 even puts them in the same physical
place: the built-in logging sink writes *"a line per record to standard error"*,
and stderr is where the stdio transport tells a server to put everything that is
not a protocol message.

> **Decision 15. Progress is reported through a process-global sink the transport
> owns, in the same position and under the same justification as
> `stdlib-standard.md` §8.2's logger. It is not a parameter, it is not in the
> schema, and it costs `effects.md` no new bit. `progress.report` is seeded into
> the existing `external` bit, beside `logging`.**

```science
## Fit Bragg peaks in a window of flight time.
tool fit_peaks(…) -> (Array of Peak, Error?):
    …
    for i in 0..max_peaks:
        if not progress.report(i as F64 / max_peaks as F64, f"peak {i + 1}"):
            return ([], ServerError.Cancelled)
        …
```

**Rejected: a `Progress` value threaded through the call chain by hand.** Correct,
explicit, and it means every helper function a tool body calls grows a parameter it
does not use, which is the colouring problem `stdlib-shape-and-packages.md` §3
rejected `async` to avoid. The argument that killed `async` kills this.

**Two guards, borrowed wholesale from §8.2 because they are the right ones.** The
sink is configured once, by the transport, before any tool runs; and a
`progress.report` in a program with no transport linked is a no-op rather than an
error, so a tool body is testable outside a server. That second property is what
makes the ambient form better than the parameter form for the thing authors
actually do, which is call the function from a unit test.

### 9.4 The cost: a `pure tool` cannot report progress

`progress.report` carries `external`, so it cannot appear in a body the compiler
must prove effect-free. Stated as the trade it is:

| | `pure tool` | reports progress |
|---|---|---|
| Gets | `readOnlyHint: true`, `openWorldHint: false`, `idempotentHint: true`, checked (§8) | a progress stream and a cancellation point (§9.5) |
| Cannot have | progress, cancellation from inside | the three checked annotations |

The diagnostic is **`effects.md`'s `SC0214` with its witness chain**, unchanged and
unclaimed — the same amendment §8 already makes, arriving from the other side.

**This is a real loss and it is worth naming the case where it bites.** A long
optimisation over data passed in as arguments — no file, no clock, no network — is
genuinely pure, genuinely slow, and genuinely wants a progress bar. It has to
choose.

**The mitigation that costs nothing:** a `pure tool` is by definition free of side
effects, so **the transport can bracket it** — report `started` when dispatch
begins and `complete` when it returns — without the body doing anything at all. So
a pure tool is not silent; it just cannot say *where it is*. That is the whole
difference, and for a fit whose duration is predictable it is most of what a caller
wanted.

**Recorded as an ask rather than a demand** (§17): if `effects.md` ever wants a
narrowing for "cannot change the computed result", in the shape of its own
Decision 12's `pure` on an `extern`, then progress reporting is its **second
caller** after the logger, and both callers pass the same test. This note does not
ask for it, because one note asking for a narrowing on its own behalf is how closed
sets stop being closed.

### 9.5 Decision 16: cancellation is cooperative, explicit, and the same call

`stdlib-shape-and-packages.md` Decision 3 gives Science OS threads and no `async`,
so there is no future to drop and no task to abort. Cancellation is a flag, and the
only question is **who reads it and where**.

**Before pricing the options: the specification's own model is cooperative, and
this was verified rather than hoped for.** Of task cancellation, verbatim:

> Cancellation is **cooperative**: The request signals intent, and the server
> decides whether and when to honor it. **A server is not obligated to actually
> stop the work**; it is only obligated to acknowledge the request. Eventual
> transition to `cancelled` is not guaranteed.

And of request cancellation, every obligation is `SHOULD` or `MAY`: a server
*"**SHOULD** stop processing the cancelled request"* and *"**MAY** ignore
cancellation notifications if […] the request cannot be cancelled."* So the design
below is not a concession forced by the absence of `async`. It is the model the
protocol specifies, and a language that could kill a thread would still be
implementing this shape.

**Two channels arrive, and the server must fold them into one flag.** This is easy
to get wrong: `notifications/cancelled` carries a `requestId` and is what an inline
call is cancelled with — on stdio the client **MUST** send it, because there is no
stream to close — while a promoted task is cancelled by the `tasks/cancel`
*request*, keyed on `taskId`, and *"the `notifications/cancelled` notification
**MUST NOT** be used for task cancellation."* A tool body must not care which one
happened, so the dispatcher maps both onto the same per-call flag.

Three answers to who reads it, and two are disqualified by things this project has
already decided.

**Killing the thread — rejected, and not on style grounds.** A thread stopped at an
arbitrary point holds borrows the region engine proved live, and may hold a lock
inside OpenBLAS or polars. Core spec §6 is the whole reason this is not available.

**A compiler-inserted check at loop back-edges — rejected, and this one deserves
emphasis.** It is the ergonomic answer: every loop becomes interruptible and no
author writes anything. It puts **a load and a branch in the inner loop of every
numerical kernel in the language**, in a language whose §1 bet is that it beats
Julia and Mojo at exactly that. A scientific language does not get to put a memory
read in the hot loop to improve a protocol's user experience. It is also
unpredictable: which loops get the check becomes a performance question the author
cannot see.

**Cooperative, at a point the author wrote — accepted.** And it composes with §9.3
rather than adding a second mechanism:

> **Decision 16. `progress.report(fraction, message)` returns a `Bool`, `true`
> meaning "keep going". A tool that reports progress gets cancellation at the same
> points, for free and for no extra syntax. `mcp.cancelled()` exists for a body
> that wants the check without reporting anything.**

The shape is the one the language already uses everywhere: a thing that can go
wrong returns something you test, in an `if`, where it happens. `syntax-revision-2.md`
§3.2's argument for the Go error model — *"the error path is written where it
happens, in the order it happens"* — is the same argument, and cancellation reads
like the rest of the language instead of like a framework.

**One obligation the library absorbs so the author never meets it.** The stdio
chapter says a server *"**SHOULD** stop work on a cancelled request as soon as
practical and **MUST NOT** send any further messages for it."* A tool body that
keeps calling `progress.report` after cancellation would violate that `MUST NOT`,
so the sink is closed by the dispatcher the moment the flag is set and every later
report is discarded. The author's ignored return value becomes a silent no-op
rather than a protocol violation, which is the right direction for a mistake to
fail.

**And a race the author also never meets**: *"cancellation notifications may arrive
after request processing has completed […] Both parties **MUST** handle these race
conditions gracefully."* A body that finishes between the flag being set and the
next check simply returns; the dispatcher drops the result rather than sending a
response for a cancelled request, which is the specification's own `SHOULD`.

### 9.6 What cannot be cancelled, and the one place Python is better

A tool body that calls `dgemm` on a large matrix, or hands a 40 GB frame to polars,
is inside foreign code for minutes with no loop of its own. **Nothing in this
design cancels it.** The `notifications/cancelled` arrives, the flag is set, and
nobody reads it until the foreign call returns.

This is compliant — cancellation is best-effort by the specification's own wording
(§9.1) — and it is worse than what a Python server gets for free, which is worth
saying plainly rather than leaving for a reader to notice:

> **CPython checks for signals between bytecodes, so a pure-Python loop is
> interruptible without the author doing anything, and `KeyboardInterrupt` reaches
> a tight loop.** A compiled Science loop is not, unless the author put the check
> there.

Python's advantage evaporates the moment the work is in C — which for scientific
Python it almost always is, so `Ctrl-C` during a large NumPy call is famously
unresponsive too. But for the case where the loop *is* the program, Python wins,
and this note does not have an answer beyond §9.5's explicit check.

### 9.7 Decision 17: concurrency is 1 by default, and the reason is the thread budget

Every other SDK's server handles concurrent tool calls, because every other SDK's
tools are I/O-bound and concurrency is free. **For this audience it is not free and
it is not a win**, and the reason is a problem this repository already found and
solved for a different caller.

`rust-binding-generation.md` §5.5 and Decision 13 establish the facts: polars'
`polars_core::POOL` is a process-wide singleton built once and **cannot be
resized**; OpenBLAS and MKL keep their worker threads parked between calls; and
Science's own `.parallel()` pool is a singleton sized to the *allocation*, not the
machine (`stdlib-shape-and-packages.md` §3.6). Decision 13 partitions the budget
between Science's pool and the foreign one, **statically, at bind time** for an
`env-at-init` library.

**A server adds a third factor that Decision 13 did not have to consider: the
number of tool calls in flight.** And the arithmetic is unusually clean, because
every pool involved is a singleton:

- Two concurrent `fit_peaks` calls do **not** create two thread pools. They queue
  against the same ones.
- So concurrency does not increase throughput — the cores were already saturated
  by the first call — and it does increase the latency of both, plus the memory
  high-water mark, because two 40 GB frames are live where one was.
- And the static partition cannot be renegotiated per call. `POLARS_MAX_THREADS`
  was read at init; §5.6 is explicit that re-setting it *"would be a mechanism that
  appears to work and does nothing."*

> **Decision 17. A Science MCP server dispatches one tool call at a time by
> default. Concurrency is opt-in with an explicit bound, and the bound is declared
> **beside the thread budget** in the manifest rather than in the transport
> configuration, because it is the same budget being divided.**
>
> ```toml
> [server]
> concurrent_calls = 1          # default; a bound, never "unbounded"
> ```

**Cost, stated, and it is the real one:** a slow tool blocks the server, so a model
that asks for a peak fit cannot also ask for a run list until the fit finishes.
That is bad, and §9.1's Tasks answer is what makes it acceptable rather than
merely honest — a long call that becomes a task returns immediately and the
dispatcher stays free. **The two decisions are one design**: serial dispatch is
only tolerable because long work does not occupy the dispatcher.

**What the specification says about concurrent execution: nothing, and that
negative was checked rather than assumed.** A grep across the transport chapters,
all five pattern chapters, the tools chapter and the architecture chapter turns up
no guidance on concurrent `tools/call` handling, no per-server request budget, no
statement about whether two calls to the same tool may run at once, and **no
explicit permission or requirement for a stdio server to interleave responses** —
that is implied only by responses being *"correlated by JSON-RPC `id`"*.

**The one sentence that looks like it contradicts Decision 17, and does not.** The
statelessness chapter says:

> Servers **SHOULD** be prepared to handle requests associated with multiple tasks,
> threads, or conversations. […] clients may interleave unrelated requests on the
> same transport, and a server must not treat connection or process identity as a
> proxy for conversation or session continuity.

That is about **identity**, not scheduling. It forbids inferring state from which
connection a request arrived on; a serial dispatcher satisfies it completely,
because it infers nothing. What the passage does establish is that **interleaved
arrival is expected**, which is the real cost of Decision 17 and is why §9.1 is
load-bearing: a client is entitled to send `list_runs` while a fit is running, and
without task promotion it waits behind it.

So Decision 17 is a policy this note is choosing, not a reading of a requirement,
and a server that wants to be concurrent violates nothing.

---

## 10. Reproducible tool results, which is the strongest thing on offer

Everything in §4 through §9 makes a Science MCP server *better*. This section is
the one that makes it *different*, and it is not a new idea — it is
`reproducibility.md` pointed at a boundary that note did not anticipate.

The claim, stated first so the rest can be measured against it:

> **A model that calls a Science tool does not get a number back. It gets a
> result, with an identity, that somebody else can check.**

No other MCP SDK can offer this, and the reason is not effort. It is that the
provenance record requires knowing the compiler version, the LLVM version, the
float flags, the resolved native libraries with their ABI variants and probe
strings, the package set with digests, and the reachable-effect analysis — and in
Python or TypeScript none of those things exists, is stable, or is knowable from
inside the process.

### 10.1 What already exists, and what is left to decide

Four sibling notes have built this and none of them knew it was going to be used
here.

| Piece | Owner | State |
|---|---|---|
| A provenance record on **every** binary and **every** output — R0, unconditional: *"The artefact states what produced it"* | `reproducibility.md` §2.2 | Decided |
| **Build record** (compiler, LLVM, target, opt level, float channels, `deterministic`, `allow_ambient`, packages, native entries with probe strings) in `.science.provenance` | `native-dependencies.md` Decision 9, extended by `reproducibility.md` §5.2 | Decided |
| **Run record** — embeds the build record verbatim, adds `argv`, `env`, input digests, resolved natives, `allowed_used`, `exit` | `reproducibility.md` §5.2, Decision 11 | Decided |
| **Canonical JSON, no floats, SHA-256 identity**, quoted as twelve hex characters (`pv:9f3a1c04e7b2`) | `reproducibility.md` §5.1, Decision 10 | Decided |
| A `tables` array, written only for reference tables **actually read** | `intrinsics-chem-bio.md` Decision 4.4 | Decided |
| `--deterministic`, which **forbids** a reachable `ambient` bit, over `effects.md`'s analysis | `reproducibility.md` Decision 7 | Decided |
| An 8 KB cap with a stated degradation order; `--provenance=minimal` strips paths and environment values | `native-dependencies.md` §5.4, `reproducibility.md` §5.3 | Decided |

So the machinery is complete and **only two questions are open here**: *what is a
run, when the program is a server?* and *how does the record travel?*

### 10.2 Decision 18: a tool call is a run, and that makes the record better

`reproducibility.md`'s run record is per-process: it carries `argv`, `started` and
`exit`. A server process serves many calls and exits when the client goes away, so
a per-process record would describe the *session* and say nothing about any
individual answer, which is useless.

> **Decision 18. For a server, the unit of a run record is **one tool call**. The
> build record is per-binary and unchanged. The run record's `argv` is replaced by
> the tool name and the call's arguments; `exit` is replaced by `isError`; every
> other field means exactly what it meant.**

```json
{
  "schema": 1,
  "build_id": "9f3a1c04e7b2…",
  "build": { "…": "the build record, embedded verbatim" },
  "run": {
    "started": "2026-09-16T10:41:02Z",
    "tool": "fit_peaks",
    "arguments": { "run": "WISH00042117", "bank": 3, "window_lower": 0.0138,
                   "window_upper": 0.0146, "line_shape": "PseudoVoigt",
                   "max_peaks": 8 },
    "inputs": [ { "path": "WISH00042117.parquet", "bytes": 41237882, "sha256": "…" } ],
    "allowed_used": [],
    "deterministic": true,
    "is_error": false
  }
}
```

**The substitution is not a compromise; it is an improvement, and it is worth
being explicit about why.** `argv` is a list of strings a reviewer has to parse,
whose meaning lives in a `--help` text that may no longer exist. A tool call's
arguments are **already a typed JSON object**, and — this is the part that only
works here — **the schema that describes them is in the same artefact**, because
`build_id` identifies the binary and the binary's `tools/list` is derivable from
it. A reviewer holding `pv:9f3a1c04e7b2` can recover not only what was passed but
what it was allowed to be.

§4's Decision 3 is what makes that true. One source of truth for the schema means
the record and the interface cannot describe different functions.

**The one field that does not survive the substitution.** `env` is per-process, and
a server reads its environment once at start-up, not per call. It stays in the
record and is a property of the session rather than the call, which is accurate
and slightly odd; the alternative — omitting it — would lose `SPECTRA_RUNS`, which
is the single most important thing about which runs the server could see.

### 10.3 Decision 19: the identity travels always, the record travels on request

Three carriers were available and the choice between them is decided by §4, not by
convenience.

**`structuredContent` — rejected, and the reason is Decision 3.** §1.4 binds
`structuredContent` to `outputSchema`: *"Servers MUST provide structured results
that conform to this schema."* Putting provenance in `structuredContent` therefore
means putting it in `outputSchema`, which means the schema derived from a tool's
return type gains a field that is not in the return type. That is one exception to
the derivation, and §19 says what exceptions do to a source-of-truth rule. It
would also put a 3 KB object in the model's context on every call, described by a
schema the model must read past to find the answer.

**`_meta` — accepted for the identity.** It is the protocol's designated extension
point, it is present on results, and `2026-07-28` made per-request `_meta` the
carrier for a stateless protocol (§1.1). A client that does not care ignores it;
no schema describes it; nothing in `tools/list` grows.

**A `resource_link` content block — accepted for the full record.** `resource_link`
is one of the five content block discriminators (§1.4), and a record that most
callers never read is exactly what a link is for.

> **Decision 19. Every tool result carries `_meta["science/provenance"]` — the
> provenance id and the two bits a caller can act on without fetching anything:
>
> ```json
> "_meta": { "science/provenance":
>   { "id": "pv:9f3a1c04e7b2", "deterministic": true, "uri": "science://provenance/9f3a1c04e7b2" } }
> ```
>
> The full record is a resource at that URI. `--provenance=inline` puts the whole
> record in `_meta` instead, for a caller that will fetch it every time anyway.**

**Cost, stated, and this is the design's whole justification.** The id, the flag
and the URI are about **110 bytes** on a result. The full record is capped at 8 KB
and is fetched by the callers who want it, which is approximately none of them
interactively and all of them at publication. **That asymmetry is the argument for
two tiers rather than one**: the author cannot know at declaration time which
results will be cited, so the record must always be *made* and must not always be
*sent*.

**The tension this creates with statelessness, named rather than hidden.**
`2026-07-28` removed sessions, and a resource that exists only because an earlier
call happened is server state by any reasonable reading. Three things make it
tolerable and none of them makes it clean:

- The record is **content-addressed** — the id *is* the SHA-256 of the canonical
  bytes — so the URI is not a session handle, it is a hash, and any server holding
  the same binary and the same call can reproduce it.
- A record is at most 8 KB and a bounded cache of the last few hundred is under a
  megabyte.
- If the cache misses, the server **recomputes** the record from the build record
  and the call it still has in the log, or returns a resource-not-found, which is
  a resource-shaped failure rather than a protocol one.

It is still state, and a server that is restarted between the call and the fetch
loses the record. `--provenance=inline` is the escape for anyone who cannot accept
that, and it costs 8 KB per result.

### 10.4 What `--deterministic` means for a server

`reproducibility.md` Decision 7 makes `--deterministic` an **error** when
`effects.md`'s `ambient` bit is reachable from `main`, unless the seed is in a
recorded allow-list. For a server, `main` reaches **every tool**, so one tool that
reads the clock destroys the determinism claim of every other tool in the binary.
That is technically correct and practically useless: `list_runs` and `d_spacing`
did not become non-deterministic because somebody added a `server_time` tool.

The fix costs nothing, because the analysis is already at the right granularity.

> **Decision 20. `--deterministic` keeps `reproducibility.md`'s Decision 7 meaning
> exactly — it is a whole-binary build flag and it still forbids. Independently
> and always, the `ambient` bit is computed **per tool**, because
> `effects.md`'s `effects(DefId)` query is per-definition already, and the
> per-tool answer is what `_meta`'s `deterministic` flag reports. Nothing new is
> analysed; the coarsening to whole-binary was the added step, not the per-tool
> answer.**

So a server built without `--deterministic` still tells the truth about each tool,
result by result, and a lab that wants the stronger guarantee — *no tool in this
binary can reach the machine* — takes the flag.

### 10.5 Decision 21: `glob` needs its expansion in the record

`effects.md` §3.2 argues that `external` must not be forbidden, because *"a program
reads its input file […] two runs over the same file agree."* Input digests are what
make that true, and they work for `fetch_spectrum`, which reads one named file.

**They do not work for `list_runs`, which is a tool in this note's own worked
example**, and chasing that case turned up something in
`reproducibility.md` Decision 8 that is worth reporting carefully, because the
first version of this section got it wrong.

That decision rejects, under `--deterministic`, *"the `ambient` bit, plus `net`,
`http` and `os.run` from the `external` set"*, and adds three source-level items
including *"`list_directory`, until G7 lands and it sorts"*. So a directory listing
**is** already on the list. But:

- the reason given is **ordering** — G7 is about sorting the result — and
- the explicit not-rejected list names **`glob`**, beside `read_file`, `print` and
  `logging`, as part of *"the reproducible half of the library"*.

**Ordering is not the problem here, and `glob` has the same problem
`list_directory` does.** A sorted glob over a directory an instrument is still
writing into returns a different set on Tuesday than it did on Monday. Nothing is
ambient, nothing is unordered, every file that *was* read can be digested — and the
call is still not reproducible, because **the answer is which files existed, and
that fact is nowhere in the artefact.**

That is exactly Decision 8's own criterion for rejecting `http`: *"the bytes are not
in any artefact this design produces. A local input is at least recordable
(Decision 6 digests it); a URL fetched at run time is recordable only after the
fact."* A glob expansion is recordable — and nothing currently records it.

> **Decision 21. A `glob` or directory listing performed during a tool call has its
> **expansion** written into the run record: the pattern, and the sorted list of
> entries it matched, with a digest for each entry the call went on to read. With
> the expansion recorded, `glob` meets Decision 8's own recordability test and
> needs no exclusion. Without it, `glob` is on the wrong side of that test and the
> not-rejected list is wrong to name it.**

**Rejected: adding `glob` to Decision 8's rejected set.** It would forbid the
commonest data-loading idiom in the audience's code — `data-io.md`'s own worked
example opens with `glob("measurements/2026-09-*.csv")` — and it would forbid it in
the ordinary batch case where the directory is a fixed checkout and the glob is
perfectly reproducible. The problem is not the call; it is that its result is
currently invisible.

**Cost.** A run record for a tool that globs a large directory carries a file list,
and `reproducibility.md` §5.3 caps the record at 8 KB. A ten-thousand-run directory
does not fit. The degradation is the one that note already specifies — the
expansion truncates with a recorded count, so the record says *"matched 10,412
entries, listing truncated"*, which is less useful and is not a lie.

**What remains genuinely unclosed.** Even with the expansion recorded, a *reviewer*
cannot re-run `list_runs` and get the same answer, because the directory has moved
on. The record makes the call **auditable** — you can see what it saw — and not
**repeatable**. That distinction is `reproducibility.md` §2.1's own (*"four
different things are called reproducible, and conflating them is how claims
fail"*), and this is a case that lands on the auditable side. §17 asks that note to
take Decision 21 or reject it; either way its §4.2 should say something about
`glob`, because a discovery tool over a live data directory is close to universal
in scientific servers and this note found the hole by accident, in its own example,
while trying to demonstrate something else.

### 10.6 The sentence a methods section can write

`intrinsics-chem-bio.md` Decision 4.4 calls its version *"the sharpest thing this
note produces"*. Here is the same sentence, from the other side of a model:

> Peak positions obtained through the `spectra` MCP server
> (provenance `pv:9f3a1c04e7b2`); OpenBLAS 0.3.26 lp64; CIAAW 2021 masses.
> The conversation is deposited at … and each tool result carries its record.

And a reviewer runs `sciencec provenance --id pv:9f3a1c04e7b2 ./spectra-mcp` and
gets yes or no.

**The limit, and it is a real one that nobody else will state.** The provenance
record describes **the server**, not the model's use of it. A model that calls
`fit_peaks` correctly, receives a perfectly attested result, misreads which column
is the width and writes a wrong sentence has produced a false claim with an
impeccable record attached. Provenance makes the *computation* checkable; it says
nothing about the *inference* drawn from it, and a note that claimed otherwise
would be selling the thing this project exists to be sceptical of.

What it does do is move the boundary. Today, a number a model reports from a tool
call is unattributable — the tool version, the library, the flags and the inputs
are all gone the moment the response is rendered. After this, the number carries
twelve hex characters that recover all of it. That is a smaller claim than
"reproducible science" and it is a true one.

---

## 11. Speed, measured, and mostly given away

The brief asked for the number rather than an implication, so here is the number
and then the concession.

**The specification supplies no number to cite, and that was checked exhaustively
rather than assumed** (§1.7): no performance section, no latency target, no budget,
in any revision, and the timeouts section specifies no concrete duration at all.
So the table below is this note's measurement and stands or falls on its method.

**Method.** Windows 11, Python 3.12.10, Node v24.11.0, `rustc` 1.91.0 with `-O`.
Each row is 25 runs of a child process that reads one line of JSON-RPC from stdin
and writes one line back, timed around `subprocess.run` so the figure includes
process creation and exit. Reported as minimum and median, in milliseconds.

| Process | min | median |
|---|---|---|
| Native binary (`rustc -O`) | **9.6** | **11.7** |
| `python -c pass` — bare interpreter | 58.2 | 70.3 |
| Python, importing `json`, `anyio` and `pydantic` | 243.2 | 286.9 |
| Node, reading one line | 96.3 | 116.1 |

**What is honest about this table and what is not.** The ~10 ms floor is
`CreateProcess` and is paid by every row; the interpreter's marginal cost is the
difference. The third row is a **proxy**: the `mcp` Python package is not
installed on this machine, so `anyio` and `pydantic` — which it depends on — stand
in for it, and the real number is higher, not lower. A native Science binary would
be in the first row's neighbourhood; it is not measured, because there is no
codegen.

So the number is **roughly a quarter of a second saved per server process**.

> **Decision 22. Start-up speed is not a reason to write an MCP server in Science
> and this note does not claim it as one.**

A stdio server is spawned once per session, and 250 ms once per session is below
the threshold at which anyone notices. The second speed argument is weaker still:
a scientific tool's cost is in the work, and a Python tool's work is already in C
— NumPy, SciPy, Arrow — so a compiled server does not beat it on the hot path
either.

**What survives, and it is a different claim.** Core spec §3's first decision is
"self-contained binaries", and *that* is the real difference at this boundary. An
MCP server is a thing a researcher hands to a collaborator, or lists in a config
file, or drops on a cluster login node. A Science server is one file. A Python
server is a file plus an interpreter version plus a virtual environment plus a
resolver plus whatever `uv` or `conda` is doing this year, and every one of those
is a way for the server to fail to start on a machine that is not the author's.
**The argument is deployment, not speed**, and it is the same argument
`python-interop.md` §6.2 made when it rejected standalone Python — *"the
deployment story becomes Python's deployment story, which is the thing users are
running toward Science to escape."*

There is one place the speed argument does survive intact, and it is not an MCP
argument: a tool whose *loop* is in Science — a per-row filter, a custom kernel, a
fit written out rather than vectorised — is fast where the Python equivalent is
slow. That is the language's ordinary claim, it applies here as it applies
everywhere, and it is not evidence for anything about servers.

---

## 12. The boundary where every type-level discipline evaporates

`unit-literals.md` §13 says this about handing a `Quantity` to C:

> C has no idea what unit the value is in, and **the entire value of the type
> evaporates at the boundary**.

The MCP boundary is the same boundary and it is worse, because more of the
project's distinctive types are pointed at it. This deserves its own section
rather than a line in §4.2, because a reader will otherwise discover it one type
at a time.

| Type | What it guarantees inside Science | What survives on the wire |
|---|---|---|
| `Quantity of (F64, …)` | Dimensional correctness, checked | A number, and a sentence (§6.2) |
| `PValue` / `AdjustedPValue` (`statistical-validity.md` Decision 1) | **That an uncorrected p-value cannot be thresholded** — the flow property is the whole point | A number. The discipline is gone. |
| `Train of R` / `Holdout of R` (`data-leakage.md` Decision 1) | That a scaler cannot be fitted before the split | An object. The distinction is gone. |
| `Uncertain of T` (`uncertainty.md`) | Correlated error propagation | A number, or a pair of numbers |
| `Tensor of (F32, (batch, 768))` | A shape agreement across calls | An array of arrays (§6.1) |

Three of those are **opaque types with no public fields** — `statistical-validity.md`
Decision 1 declares `type PValue` with nothing in it, deliberately. An opaque type has no
schema, so a tool returning one is `SC0504` and the author must convert. **That is
the right behaviour and it should be read as a feature**: the compiler refuses to
serialize the value silently and makes the author write the line where the
discipline is given up.

> **Decision 23. A type whose guarantee is structural rather than numeric does not
> cross this boundary silently. `SC0504`'s message names the guarantee being lost,
> not merely the type.**

Concretely, for a tool returning a `PValue`, the diagnostic should read close to:

```
error[SC0504]: `PValue` cannot be sent to a model
  --> spectra/tools.science:41:44
   |
41 | tool peak_significance(run: String) -> (PValue, Error?):
   |                                         ^^^^^^ this type has no JSON
   |                                                representation
   |
   = note: `PValue` is opaque so that an uncorrected p-value cannot be
           thresholded (statistical-validity.md, Decision 1). Sending it as a
           number gives the caller exactly the value that type exists to
           withhold.
help: correct first, then send the corrected value
   |
41 | tool peak_significance(run: String) -> (AdjustedPValue, Error?):
   |                                         ~~~~~~~~~~~~~~
```

This is `llm-ergonomics.md` §3's rule — *name the construct, not the token* —
applied to a case that note did not anticipate, and it is the strongest argument
in this note for allocating diagnostics rather than letting a library raise a
runtime error.

**The general statement, which is uncomfortable and true:** this boundary is
where Science stops being Science. Everything the project has built in the type
system is on one side of it, and on the other side is a JSON object and a
model's judgement. The most a language can do is make the crossing explicit,
refuse the crossings that lose something important, and generate honest prose
about the rest. That is all §4, §5 and §6 are doing.

---

## 13. A worked example: a time-of-flight diffraction server

Not a calculator. This is a server a beamline scientist would actually run: it
sits on the directory of run files the instrument writes, and lets a model find
runs, pull a spectrum, fit Bragg peaks in a window, and convert a flight time to a
lattice spacing. `spectra` is the running example the rest of this repository uses
— `package-manager.md` §4.2's manifest and `strings-formatting-and-docs.md` §5.2's
768-channel rows are the same program.

### 13.1 The program

```science
## Time-of-flight diffraction runs, exposed to a model.
##
## Every tool here reads the run directory given by `SPECTRA_RUNS` and
## operates on the 768-channel detector rows `spectra` writes, with the
## dark current already subtracted.

use fs (Path, glob)
use data (Frame, Record)
use data.parquet (read_parquet)
use os.env
use physics.constants (PLANCK, NEUTRON_MASS)
use physics.units (Time, Length, Angle, ANGSTROM)
use mcp (Server, Stdio, progress)
use spectra (fit_one, remove_peak, FitOptions)

## Which instrument wrote a run.
choice Instrument:
    Wish
    Polaris
    Gem

## A line shape for the peak fit.
choice LineShape:
    Gaussian
    Lorentzian
    PseudoVoigt

## One run, as it appears in the run log.
type RunSummary:
    ## Run identifier, for example "WISH00042117".
    run: String
    ## Which instrument wrote it.
    instrument: Instrument
    ## Proposal number the run was taken under.
    proposal: U32
    ## Total integrated counts over all detector banks.
    counts: U64
    ## Title as typed by the experimenter. Often empty.
    title: String

RunSummary implements Record

## One detector row from one run.
type DetectorRow:
    ## Detector bank, 0 through 9.
    bank: U32
    ## Channel counts. Always 768 long; this is checked on load.
    channels: Array of F32

DetectorRow implements Record

## One fitted Bragg peak.
type Peak:
    ## Peak centre, in seconds of flight time.
    centre: F64
    ## Full width at half maximum, in seconds.
    width: F64
    ## Integrated intensity, in detector counts.
    intensity: F64
    ## Fit residual over the window, normalised by the window width.
    residual: F64

Peak implements Record

## Everything this server itself can fail with. A `choice`, so the bodies
## below stay exhaustive; the tool signatures still say `Error?`, because a
## tool also propagates failures from `fs`, `data.parquet` and `spectra`,
## and boxing on an error path is the trade §3.4 of the syntax revision
## describes.
choice ServerError:
    NoRunDirectory
    NoSuchRun(String)
    NoSuchBank(String, U32)
    EmptyWindow
    TooManyPeaks(U8)
    BadGeometry
    Cancelled

ServerError implements Error:
    function message(self) -> String:
        match self:
            NoRunDirectory: "SPECTRA_RUNS is not set"
            NoSuchRun(run): f"no run named {run}"
            NoSuchBank(run, bank): f"run {run} has no bank {bank}"
            EmptyWindow: "the upper window edge is not above the lower"
            TooManyPeaks(n): f"{n} peaks requested; the limit is 32"
            BadGeometry: "the flight path and angle do not describe a detector"
            Cancelled: "the fit was cancelled"

# `run_path`, `run_log` and `fit_report` are ordinary functions and are elided.

# --- The tools -------------------------------------------------------------

## List runs in the run directory, newest first.
##
## Returns at most 200 runs. A run appears here as soon as the instrument
## has closed its file, which may be before the reduction has finished; a
## run whose reduction is incomplete has `counts` of zero.
tool list_runs(
        ## Only runs from this instrument. All instruments if omitted.
        instrument: Instrument?,
        ## Only runs under this proposal number.
        proposal: U32?) -> (Array of RunSummary, Error?):
    let directory be env.get("SPECTRA_RUNS")
    if not directory?:
        return ([], ServerError.NoRunDirectory)

    let files, err be glob(Path.from(directory).join("*.parquet"))
    if err?:
        return ([], err)

    let frame, err be read_parquet of RunSummary(files)
    if err?:
        return ([], err)

    let mutable kept be frame.rows()
    if instrument?:
        kept be kept.keep(each.instrument is instrument)
    if proposal?:
        kept be kept.keep(each.proposal is proposal)

    let summaries, err be kept.take(200).collect()
    if err?:
        return ([], err)

    (summaries, null)

## Fetch one detector row from a run.
##
## The row is 768 channels of counts. Channels are ordered by increasing
## flight time and the first channel starts at the pulse.
##
## ```science,no_run
## let row, err be fetch_spectrum("WISH00042117", 3u32)
## print(row.len())
## ```
tool fetch_spectrum(
        ## Run identifier, as returned by `list_runs`.
        run: String,
        ## Detector bank, 0 through 9.
        bank: U32) -> (Array of F32, Error?):
    let path, err be run_path(run)
    if err?:
        return ([], err)

    let frame, err be read_parquet of DetectorRow([path])
    if err?:
        return ([], err)

    let row be frame.rows().keep(each.bank is bank).first()
    if not row?:
        return ([], ServerError.NoSuchBank(run, bank))

    (row.channels, null)

## Fit Bragg peaks in a window of flight time.
##
## The window is given in seconds from the pulse, which is the unit the
## instrument's own logs do *not* use — divide microseconds by 1e6. A
## window narrower than 32 channels is refused, because the fit is
## underdetermined below that and a returned answer would be noise.
tool fit_peaks(
        ## Run identifier, as returned by `list_runs`.
        run: String,
        ## Detector bank, 0 through 9.
        bank: U32,
        ## Lower edge of the window, in seconds from the pulse.
        window_lower: Time of F64,
        ## Upper edge of the window, in seconds from the pulse.
        window_upper: Time of F64,
        ## Line shape to fit. PseudoVoigt is the usual choice.
        line_shape: LineShape,
        ## Maximum number of peaks to fit. At most 32.
        max_peaks: U8) -> (Array of Peak, Error?):
    if window_upper <= window_lower:
        return ([], ServerError.EmptyWindow)
    if max_peaks > 32u8:
        return ([], ServerError.TooManyPeaks(max_peaks))

    let channels, err be fetch_spectrum(run, bank)
    if err?:
        return ([], err)

    let options be FitOptions.new()
        .shape(line_shape)
        .max_peaks(max_peaks as U32)

    # Sequential peak stripping: fit the strongest peak, subtract it, repeat.
    # `report` is the progress point and the cancellation point at once
    # (§9.5); ignoring its result is legal and means the fit runs to the end.
    let mutable residual be channels
    let mutable peaks: Array of Peak be []
    for i in 0..max_peaks:
        let fraction be i as F64 / max_peaks as F64
        if not progress.report(fraction, f"peak {i + 1} of {max_peaks}"):
            return ([], ServerError.Cancelled)

        let peak, err be fit_one(residual, window_lower, window_upper, options)
        if err?:
            return ([], err)
        if not peak?:
            break

        residual be remove_peak(residual, peak)
        peaks.push(peak)

    (peaks, null)

## Convert a time of flight to a d-spacing by Bragg's law.
##
## ```science
## let d, err be d_spacing(0.0142<s>, 40.0<m>, 1.5708<rad>)
## print(f"{d.in(ANGSTROM):.4f}")
## ```
## ```output
## 0.9931
## ```
pure tool d_spacing(
        ## Time of flight, in seconds from the pulse.
        time_of_flight: Time of F64,
        ## Total flight path, source to detector, in metres.
        flight_path: Length of F64,
        ## Scattering angle, in radians.
        two_theta: Angle of F64) -> (Length of F64, Error?):
    if flight_path <= 0.0<m>:
        return (0.0<m>, ServerError.BadGeometry)

    # h / (m_n · L) · t, divided by 2 sin(θ). Every factor carries its
    # dimension, so a wrong power here does not compile.
    let wavelength be (PLANCK / (NEUTRON_MASS * flight_path)) * time_of_flight
    let denominator be 2.0 * (two_theta / 2.0).sine()
    if denominator is 0.0:
        return (0.0<m>, ServerError.BadGeometry)

    (wavelength / denominator, null)

# --- The server ------------------------------------------------------------

function main() -> ((), Error?):
    let server be Server.new("spectra", "0.4.1")
        .tools([list_runs, fetch_spectrum, fit_peaks, d_spacing])
        .resource_template("spectra://run/{run}/log", run_log)
        .prompts([("fit_report", fit_report)])

    server.run(Stdio.new())
```

### 13.2 What `tools/list` produces for one of them

Nobody wrote this. It is the declaration of `fit_peaks` above, mechanically:

```json
{
  "name": "fit_peaks",
  "title": "Fit Bragg peaks in a window of flight time.",
  "description": "Fit Bragg peaks in a window of flight time.\n\nThe window is given in seconds from the pulse, which is the unit the\ninstrument's own logs do *not* use — divide microseconds by 1e6. A\nwindow narrower than 32 channels is refused, because the fit is\nunderdetermined below that and a returned answer would be noise.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "run": {
        "type": "string",
        "description": "Run identifier, as returned by `list_runs`."
      },
      "bank": {
        "type": "integer", "minimum": 0, "maximum": 4294967295,
        "description": "Detector bank, 0 through 9."
      },
      "window_lower": {
        "type": "number",
        "description": "Lower edge of the window, in seconds from the pulse. (second)"
      },
      "window_upper": {
        "type": "number",
        "description": "Upper edge of the window, in seconds from the pulse. (second)"
      },
      "line_shape": {
        "type": "string",
        "enum": ["Gaussian", "Lorentzian", "PseudoVoigt"],
        "description": "Line shape to fit. PseudoVoigt is the usual choice."
      },
      "max_peaks": {
        "type": "integer", "minimum": 0, "maximum": 255,
        "description": "Maximum number of peaks to fit. At most 32."
      }
    },
    "required": ["run", "bank", "window_lower", "window_upper",
                 "line_shape", "max_peaks"]
  },
  "outputSchema": {
    "type": "array",
    "items": {
      "type": "object",
      "properties": {
        "centre":    {"type": "number", "description": "Peak centre, in seconds of flight time."},
        "width":     {"type": "number", "description": "Full width at half maximum, in seconds."},
        "intensity": {"type": "number", "description": "Integrated intensity, in detector counts."},
        "residual":  {"type": "number", "description": "Fit residual over the window, normalised by the window width."}
      },
      "required": ["centre", "width", "intensity", "residual"]
    }
  }
}
```

Two details in that JSON are consequences of the revision this note targets and
would have been different a year ago. `inputSchema` carries `type: "object"` at the
root because §1.3 requires it there and only there. `outputSchema` is a bare array
because §1.3 does **not** constrain it to an object and §1.4 lets
`structuredContent` be any JSON value; against `2025-06-18` this would have needed
a wrapper property, and the wrapper would have been in the model's way forever.

### 13.3 Eleven things this example is making a case for

1. **There is no schema in the source and no schema file in the repository.**
   Every one of the sixty-odd lines of JSON above came from a declaration a human
   wrote for a human. This is the whole of Decision 3 and it is the only claim in
   the note that is worth the keyword on its own.
2. **`line_shape` is an `enum` and nobody asked for one.** The author wrote
   `choice LineShape` because that is how Science spells a fixed set of
   alternatives — they would have written it for a `function` too. §4.5.
3. **`max_peaks: U8` produced `minimum: 0, maximum: 255` for free**, and the body
   still has to check `> 32u8` itself, because 32 is a property of the fit and not
   of the type. The schema states what the type knows and the body states the
   rest, and neither restates the other.
4. **The description of `fit_peaks` says something no schema can.** *"A window
   narrower than 32 channels is refused, because the fit is underdetermined below
   that"* is the sentence that stops a model asking for a five-channel fit, and it
   is prose, and it is required by `SC0190` to exist. Requiring a description does
   not make it a *good* description — §19 says so — but a required field at least
   gets read once.
5. **The unit problem is visible in the source rather than hidden.** `window_lower`
   is in seconds because `Time of F64` stores SI coherent base units, and a
   beamline scientist writes microseconds. The description carries the conversion
   because the author had to write it by hand. §6.2 calls this a cost, and this is
   what paying it looks like.
6. **`d_spacing` is `pure tool` and the compiler checked it**, so
   `readOnlyHint: true` on that tool is a fact. Adding a `print` for debugging
   would break the build with `effects.md`'s `SC0214` and its witness chain, not
   silently falsify an annotation. §8.
7. **The doc tests are the examples the model reads, and they are of two kinds.**
   `d_spacing`'s compiles, runs, and has `0.9931` compared byte for byte.
   `fetch_spectrum`'s is `science,no_run` — `strings-formatting-and-docs.md` §5.5's
   fence for a sample that needs a file — so it is compile-verified and not run,
   which is the most that can be promised about an example that touches an
   instrument's disk. **Both are shipped to the model; only one is executed**, and
   the description does not distinguish them, which is a small honesty gap worth
   knowing about. §5.4.
8. **The error model needed nothing added.** Every tool returns
   `(T, Error?)`; every `if err?:` is the ordinary shape from
   `examples/09_absence_and_failure.science`; `fit_peaks` calls `fetch_spectrum`
   as a plain function and propagates its error unchanged. On the wire each of
   those becomes `isError: true` with `message()` as the text, and the tool body
   could not have produced a protocol error if it had tried. §7.1.
9. **`fetch_spectrum` is called twice — once by the model and once by
   `fit_peaks`.** A `tool` is an ordinary callable from inside the program, and
   nothing about the declaration makes it special to its neighbours. That is what
   keeps a server from growing a second, internal copy of every tool, which is the
   commonest structural defect in published servers.
10. **Progress and cancellation are one line and no ceremony.** `fit_peaks` is a
   minutes-long call, so it is the case §9 exists for: `progress.report` is the
   only new vocabulary, it is not a parameter, it does not appear in the schema
   above, and the same call is where the fit can be interrupted. If the call runs
   past `task_after` the dispatcher hands the model a `taskId` and the loop keeps
   going, with no change to this source (§9.1).
11. **Nothing in the example mentions provenance, and every result carries it.**
   `_meta["science/provenance"]` is added by the dispatcher, not by the author, and
   the record for a `fit_peaks` call names the run file's digest, OpenBLAS's probe
   string and the compiler's float flags. §10 is the only section of this note
   whose payoff is invisible in the source, which is the point of it.

And the thing the example is *not* making a case for, said plainly: none of this
is faster than the Python equivalent, because `fit_one` is where the time goes
and the Python version would call a C fitter to do the same work. §11.

---

## 14. What this needs from phases that do not exist, and what can ship now

The front end today is `science-lexer`, `science-parser`, `science-resolve`,
`science-diagnostics` and the salsa database; `science-rt` contains `abi.rs` and
nothing else; `sciencec` has one subcommand, `check`. There is **no type checker
and no codegen**. So the honest question is not "when can this ship" but "how much
of it is a front-end feature", and the answer is more than expected.

### 14.1 The fact that makes an early stage possible

Core spec §5.2 requires **fully annotated signatures**: *"local, signatures are
fully annotated, inside a body everything is inferred by unification."* A `tool`'s
parameter types are therefore **written in the source**, not inferred. Mapping
them to JSON Schema needs:

- the *syntax* of each parameter's type, which the parser has;
- for a record type, its field list and their types — which **resolution** has;
- for a `choice`, its variant list and whether they carry payloads — resolution
  again;
- name resolution to tell `Instrument` the user's `choice` from `Instrument` some
  other module's type — resolution again.

None of that is type *checking*. It is a walk over resolved declarations. **The
schema generator is a resolver-level pass and can be written against the compiler
that exists today.**

### 14.2 The stages

| Stage | Needs | Delivers |
|---|---|---|
| **0** | Today's front end | `tool` moves from `Reserved(Tool)` to a keyword in `token.rs`; `parse_item` gains an arm; doc comments attach (already an F0 obligation under `strings-formatting-and-docs.md` §5.3). **All nine syntax diagnostics** `SC0190`–`SC0198`, none of which needs a type. |
| **1** | Stage 0 + resolution | **`sciencec tools --json`** — reads a file, prints the `tools/list` array to stdout. No server, no transport, no codegen. §14.3. |
| **2** | The type checker | `SC0504`–`SC0518` as errors rather than best-effort. Alias expansion (`SC0512`), the nested-record case (`SC0514`), and the `Record` derivation of `data-io.md` §4.3, without which there is no `outputSchema`. |
| **3** | `effects.md`'s `effects(DefId)` query | `pure tool`, and the two computable annotations. §8. The query is over the resolved call graph, which exists, so this is closer than stage 2 in real terms and is ordered after it only because it is worth less. |
| **4** | Codegen, `data.json`, and a runtime | The `mcp` package: framing, dispatch, the stdio transport. §14.4. |

### 14.3 What ships first, and why it is worth shipping alone

**`sciencec tools --json` is the whole thesis, minus the server.**

```
$ sciencec tools --json src/tools.science
[{"name":"list_runs","title":"List runs in the run directory, newest first.", …}]
```

It is worth building before anything else for four reasons:

1. **It proves Decision 3 or refutes it.** If the generated schema for ten real
   tools is right, the note's central claim holds. If it is not, that is known
   before a keyword is spent in earnest.
2. **It is immediately useful without a Science runtime.** A Python or TypeScript
   MCP server can *consume* that JSON today — declare the tools in Science, emit
   the schema, dispatch from the existing SDK. Ugly, and it makes the first
   version usable a phase early.
3. **It is a byte-exact test target.** `tests/ui/` already compares rendered
   output byte for byte; a generated schema is the same kind of artefact, and
   `llm-ergonomics.md` §6's discipline applies to it unchanged.
4. **It costs nothing if Decision 1 is later reversed.** The same pass over Option
   B's record types produces the same JSON.

### 14.4 What stage 4 needs that nobody has written

- **`data.json`.** `data-io.md` §3 budgets two weeks and `stdlib-standard.md` §6
  decides there is no separate `json` module. It is a hard dependency and it is on
  someone else's list already.
- **Its number model.** `stdlib-standard.md` §6 files an open question about what
  `data.json` does with an integer that does not fit exactly. **This note is a
  second caller for it and §4.3 is the concrete case**: a `U64` parameter above
  2⁵³ must be rejected rather than rounded, and that has to be `data.json`'s
  behaviour, not the `mcp` package's.
- **The stdio transport needs nothing else.** Line-delimited JSON over stdin and
  stdout, with diagnostics on stderr, is `print`, `write` and a reader. That is
  why it is the first and only transport (§15).
- **The HTTP transport cannot be built, and this is not a scheduling opinion.**
  `stdlib-standard.md` §3's table lists `http` as Level 3, F2, and **"HTTP/1.1
  client only"**. Streamable HTTP needs an HTTP *server*, which no note in this
  repository plans and which would need TLS and concurrency. It no longer needs a
  session store, because `2026-07-28` removed sessions — so the gap is smaller
  than it was, and it is still an HTTP server. §15 records it as deliberately not
  shipped rather than deferred, because "deferred" implies somebody is scheduled
  to do it.
- **A test target that already exists.** `modelcontextprotocol/conformance` has
  been available since 2026-01-23 and tests protocol correctness. The `mcp`
  package should be run against it from its first commit rather than after a bug
  report, and §17 asks for that. It is the difference between claiming compliance
  and having it checked, and it is free.

---

## 15. What is deliberately not being shipped

Named, so that each is a decision rather than an omission somebody discovers.

**No HTTP transport.** §14.4. Stdio only. A Science MCP server is a subprocess, so
it cannot be a hosted, remote, multi-tenant server — which is a large part of where
MCP is going, and this note accepts being on the wrong side of it for the first
version because the alternative is writing an HTTP server first.

**No session management, and this one stopped being a gap while the note was being
written.** `2026-07-28` removed sessions and `Mcp-Session-Id` and made the protocol
stateless with per-request `_meta` (§1.1). There is nothing left to manage. It is
named here because every piece of published MCP material still describes sessions,
and a reader will otherwise look for them.

**No authorization.** Follows from the transport decision: the authorization work
attaches to Streamable HTTP. A stdio server's security boundary is the process
boundary and the user who spawned it.

**No registry publishing.** A live registry exists (§1.7) with an `mcp-publisher`
CLI and a `server.json`. Publishing is a packaging concern, it belongs beside
`package-manager.md`'s own publishing story rather than in a language note, and
`server.json` is not something `sciencec` should learn to write.

**No `prompt` keyword.** §3.

**No `agent`, and nothing about calling a model.** §3, §0.

**No MCP *client*.** A Science program that *consumes* MCP servers is a different
note and a much bigger one: it needs an async or threaded request pipeline, and
`stdlib-shape-and-packages.md` §3 chose threads over async, which is a live
constraint on how that would be built. Nothing here precludes it.

**No dynamic tool registration in the first version.** §2.6's first falsifier is
about exactly this, and the escape hatch is kept *open* rather than built:
`Server.tool_dynamic(name, description, schema, handler)` is the shape it would
take, it is Option A with all of Option A's drift, and it should exist only once
somebody produces a server that genuinely needs it. `SC0199` is held unallocated
against it.

**No `oneOf` for sealed interfaces.** §4.2 notes that an interface whose complete
implementation set is known could emit a discriminated union. It is not built, for
§4.7's reasons plus one more: `effects.md` §3.5 item 6 already depends on the
resolver knowing a complete implementation set, and making a *schema* depend on
that same optimisation would make a wire format sensitive to whether a program
happens to be closed.

**No completions and no elicitation.** Real parts of the protocol, library surface,
no language consequence — which is exactly why a *language* note omits them:
adding them changes no decision here. Elicitation is the one with a future claim on
the grammar, because it is what `resultType: "input_required"` is for and a `tool`
body has no point at which it can ask a question (§7.3).

**No sampling, roots or logging notifications, and this is the protocol's choice
rather than this note's.** All three are **deprecated** as of `2026-07-28` (§1.1).
Building them now would be building toward a removal.

**No task persistence, and therefore no `ttlMs` of `null`.** §9.1. A stdio server's
tasks die with the process. Persisting them needs a store, a reaper and an answer
about two servers sharing a directory, and it is the first thing an HTTP transport
would need anyway.

**No task push; polling only.** `notifications/tasks` over `subscriptions/listen` is
opt-in on both sides and needs a second streaming concept in the transport, and
polling `tasks/get` is the specification's default and only guaranteed path. The
`mcp` package sets `pollIntervalMs` honestly and leaves push for when somebody has
a client that wants it.

**No task `input_required`.** Its `inputRequests` carry elicitation, sampling and
`roots/list`, none of which ships. A Science task goes
`working → completed | failed | cancelled`.

**No `sciencec mcp new` scaffolding command.** Tempting, and it is how every other
ecosystem onboards. Refused for now because a scaffold is a strong statement about
the shape of a server, and this note has not yet seen ten real ones.

---

## 16. Diagnostics allocated

Per `README.md`'s allocation map, this note's blocks are **`SC0190`–`SC0199`** in
the Syntax range and **`SC0504`–`SC0519`** in the Types second band. No other code
is claimed. Two are *amended* and not claimed, in the sense the README permits:
`effects.md`'s `SC0214` (§8) and `strings-formatting-and-docs.md`'s `SC0176`
(§5.1's reasoning, unchanged).

**This note was allocated no Resolution code**, and two of the checks below are
resolution-shaped — `SC0513` is a duplicate-name check and `SC0517` needs a
resolved type. They are in the Types band because that is where the allocation is,
and this is recorded so that a later reader does not read the placement as a claim
about which phase runs them.

### 16.1 Syntax, `SC0190`–`SC0199`

Every one of these is checkable by the parser and the resolver, with no types.
That is §14.2 stage 0.

| Code | Fires on | Fix |
|---|---|---|
| `SC0190` | A `tool` declaration with no `##` run immediately before it | Write one. The message names what the description is *for* — a model reads it to decide whether to call the tool — and not "add documentation". §5.1 |
| `SC0191` | A generic `tool`: `tool f of T(…)` | Remove the type parameter, or declare one tool per instantiation. §2.4 item 2 |
| `SC0192` | A `tool` parameter declared `borrowed` or `mutable borrowed` | Drop the word. Applicable fix: delete the span. §2.4 item 3 |
| `SC0193` | A `tool` with a `self` or `mutable self` parameter | A tool is not a method; move it to module level |
| `SC0194` | A `##` run before a parameter of anything that is not a `tool` | Move the text into the declaration's own doc comment. §5.3 |
| `SC0195` | A `tool` whose doc comment's summary line is blank — the run starts with an empty `##` | Put the summary first. §5.2 needs a `title` |
| `SC0196` | `prompt` or `agent` used in declaration position | Names Decision 2 and says what to write instead: a `function` returning `Array of Message` for a prompt; nothing, yet, for an agent |
| `SC0197` | A `tool` declared anywhere but module level — inside an `interface`, a `has` block, an `implements` block or a function body | Move it out |
| `SC0198` | A `tool` with no body, in the shape of an interface method | Give it a body; there is no abstract tool |
| `SC0199` | **Unallocated.** Held against the dynamic registration form of §15, so that if it ever lands it does not need a code from a fresh block | — |

### 16.2 Types, `SC0504`–`SC0519`

| Code | Fires on | Notes |
|---|---|---|
| `SC0504` | A `tool` parameter or return type with no JSON representation — the general case | **The message names the guarantee being lost, not just the type.** §12, Decision 23. This is the one whose wording matters most |
| `SC0505` | A parameter of a closure type | §4.2 |
| `SC0506` | A parameter of type `any Interface` | §4.2. Distinct from `SC0504` because the fix is different: name a concrete type |
| `SC0507` | `Map of (K, V)` where `K` is not `String` | §4.1. Fix: `Map of (String, V)`, or an `Array` of pairs |
| `SC0508` | A `choice` with payload-carrying variants | §4.7. The message carries the *third* argument — that this is usually two tools — because that is the fix an author can act on |
| `SC0509` | A `Tensor` whose shape is not all literal | §6.1. Names which axis is symbolic |
| `SC0510` | **Warning.** A recursive record type in a `tool` signature | The schema is correct (`$defs` + `$ref`). The warning is about consumers, §20 |
| `SC0511` | A `tool` return type that is none of: `Content`, a record type, a primitive that crosses, or a bare `Error?` | §7 |
| `SC0512` | A type alias that expands to something unexpressible | **Two spans**: the alias at the use site and its definition. `llm-ergonomics.md` §3's rule 1 — a single span here teaches nothing |
| `SC0513` | Two `tool` declarations with the same name | The wire name must be unique across the whole server |
| `SC0514` | A record *field* whose type does not cross, reported at the field with the tool named as a note | Distinct from `SC0504` so the error lands at the definition rather than at the parameter three levels up |
| `SC0515` | A `tool` parameter or return of type `()` or `Never` | A tool that takes nothing takes `{}`, which is legal and is *not* this error; `()` as a named parameter is |
| `SC0516` | A record return type from which no output schema could be emitted | Separate from `SC0504` because the fix is different — return `Content` and format it yourself |
| `SC0517` | A type whose schema is not known before the run: `Frame of Dynamic`, or `Json` in *return* position with an output schema demanded | §4.2 |
| `SC0518` | **Warning.** A `Quantity` parameter whose doc comment names a unit symbol from the unit namespace that is not the coherent unit for its dimension | The compiler appends the coherent unit to the description; if the prose says `μs` and the wire is seconds, the description now contains two contradictory units. §6.2's cost, made visible. Prose-scanning, hence a warning |
| `SC0519` | **Warning.** `print` or `write` is reachable from a `tool` body in a program that links the stdio transport | §8.2, Decision 13. Carries `effects.md`'s witness chain. The one diagnostic here that is not about a type, and the one most likely to be thanked for |

**The Types block is fully spent; the Syntax block has `SC0199` left.** If §2.6's
third falsifier is checked by writing a second backend and that backend needs
diagnostics of its own, it takes a fresh block rather than borrowing from here.

**§9 and §10 claim no code at all, and that is a result rather than an omission.**
Long-running tools needed none because Decision 14 adds nothing to a declaration for
a compiler to check; the one thing that *is* checkable — a `pure tool` that reports
progress — is `effects.md`'s existing `SC0214` arriving from a new direction (§9.4).
Provenance needed none because it is a dispatcher behaviour and a build flag, and
the build flag's diagnostic is `reproducibility.md`'s `SC0217`, also unchanged. Two
substantial sections that spend no namespace is the strongest available evidence for
§2.7's claim that the protocol work is not in the language.

---

## 17. What this note asks of the others

| # | Of | Ask | Weight |
|---|---|---|---|
| 1 | `strings-formatting-and-docs.md` §5.2 | One row in the attachment table: *a parameter of a `tool` declaration, documented before the parameter line*. **§2.5's choice of Option C over Option B depends on this**; if it is refused, this note's Decision 1 should be reopened | **Blocking** |
| 2 | `strings-formatting-and-docs.md` §5.3 | Amend *"strippable"* to except `tool` descriptions, which are program data. §5.5 | Blocking |
| 3 | `strings-formatting-and-docs.md` §5.5 | A narrow exception to *"a separate target, not part of `sciencec build`"*: doc tests inside `tool` declarations run on every build, because the fence is shipped. §5.4 | Strong. Decision 8's claim is false without it |
| 4 | Core spec §13 | Move `tool` to the in-use list. Leave `prompt` and `agent` where they are | Mechanical |
| 5 | Core spec §2 | **F4's row reads "`agent`, `tool`, `prompt` as language constructs" and depends on F3.** A `tool` declaration depends on nothing in F3 — no actors, no `spawn`, no supervision. Split the row: `tool` is F0 syntax with an F1 backend, and F4 keeps `agent` | Real. It is the difference between this shipping and waiting two phases |
| 6 | `effects.md` | Confirm that `pure` may prefix `tool`, and that `SC0214` covers a `pure tool` whose effect set is non-empty. Decision 5 of that note says `pure` is *"the only word this note spends"*; this is a widening of its surface, not a new word | Small, and it should be that note's call |
| 7 | `unit-literals.md` §7.3 | A declaration-site display unit, so that a `Quantity` parameter can be written in the unit its audience uses. **Second caller** for a seam that note deferred. §6.2 | Strong. It is the ugliest thing in §13's example |
| 8 | `data-io.md` §3 / `stdlib-standard.md` §6 | `data.json`'s number model: an integer outside ±2⁵³ is an error, not a rounded value. **Second caller** for an open question already filed. §4.3 | Strong |
| 9 | `data-io.md` §4.3 | Nothing changes; the `Record` derivation gains a **third customer**, which strengthens the README's standing ask for a derive mechanism from two to three | Informational |
| 10 | `README.md` | The index row and the allocation-table row for this note. The note does not edit it | Mechanical |
| 11 | Whoever writes the `mcp` package | Run `modelcontextprotocol/conformance` in CI from the first commit, and pin **both** targeted revisions — the core `2026-07-28` and the `ext-tasks` `2026-07-28` — in `science.toml`, so that either cadence moving is a visible dependency change | Strong, and free |
| 12 | `effects.md` §3.1 | Seed `progress.report` into the **`external`** bit, beside `logging`, for the reason §8.2 of `stdlib-standard.md` gives for the logger. No new bit is requested and Decision 2's closure is not touched. §9.3 | Small |
| 13 | `effects.md` Decision 5 | **Record, do not act.** If a narrowing for *"cannot change the computed result"* is ever wanted, progress reporting is its second caller after the logger, and both pass the same test. This note deliberately does **not** ask for it. §9.4 | Informational |
| 14 | `reproducibility.md` §5.2, Decision 11 | The run record gains a **tool-call form**: `tool` and `arguments` in place of `argv`, `is_error` in place of `exit`. §10.2 | Strong |
| 15 | `reproducibility.md` §5.3, Decision 12 | The carrier table gains a row for a tool result: the id in `_meta`, the record behind a `resource_link`. It is the first carrier in that table that is a *protocol* rather than a file format. §10.3 | Strong |
| 16 | `reproducibility.md` §4.2, Decision 8 | **Take Decision 21 or reject it.** That decision's not-rejected list names `glob`, and §10.5 argues `glob` fails its own recordability test unless the expansion is in the run record. This is a disagreement, not a request. §10.5, §18 item 8 | **The sharpest one here** |
| 17 | `rust-binding-generation.md` Decision 13 | The thread budget gains a third factor. That decision partitions between Science's pool and a foreign `env-at-init` pool; a server multiplies by concurrent calls, and an `env-at-init` partition cannot be renegotiated per call. §9.7 declares the concurrency bound beside the budget for that reason | Real, and it is that note's call |
| 18 | `stdlib-standard.md` §8.2 | Confirm the progress sink sits beside the logger under the same rules — configured once, by the transport, before any tool runs. §9.3 | Small |

---

## 18. Contradictions with existing notes, recorded

House rule: name the section and give the reason, rather than smoothing it over.

**1. `strings-formatting-and-docs.md` §5.3 — "strippable".** Directly
contradicted for `tool` declarations. §5.5 above states the amendment and
recommends the version that costs that note the least: emit tool descriptions as
ordinary string constants and leave the metadata table exactly as specified.

**2. `strings-formatting-and-docs.md` §5.4 — the refusal of `@param`.** Not
contradicted, *narrowed*, and a reviewer may reasonably read the narrowing as a
contradiction. That note refused a tag inside prose that restates the signature.
Decision 7 adds a doc comment attached to a parameter **in** the signature, which
restates nothing and is the same move §5.2 already makes for a record field. The
distinction is real and it is thin, and it is stated here rather than buried so
that the owner of that note can reject it.

**3. `strings-formatting-and-docs.md` §5.5 — "must be a separate target".**
Contradicted narrowly by §5.4 above, for `tool` declarations only.

**4. Core spec §2 — F4 depends on F3.** Contradicted. `tool` needs no actor, no
`spawn` and no supervision; the dependency was inherited from `agent`, which is in
the same table row and does need them. §17 item 5.

**5. `rust-binding-generation.md` §5.2 Decision 10 — bind the format, not the
library.** Not contradicted, and §2 had to argue past it, so the argument is put
where a reviewer can attack it: `tool` survives that Decision **only because it does
not name MCP**. Every protocol-shaped thing — the methods, the result envelope,
the transports, the capability negotiation, the revision strings — is in a Level 3
package. §2.7's test (no MCP-specific diagnostic in `sciencec`) is what keeps that
claim honest and is falsifiable by inspection. A reviewer who thinks `tool` is an
MCP construct wearing a coat should read this as a contradiction and say so.

**6. `effects.md` Decision 5.** `pure function` is described there as the one
declaration the effect discipline admits. `pure tool` widens that by one form. The
mechanism is unchanged — the same query, the same witness chain, the same
`SC0214` — and it is still an extension of a decision this note does not own.

**7a. `reproducibility.md` Decision 8's not-rejected list, on `glob`.** A real
disagreement and the only one in this note that is about somebody else's rule
rather than somebody else's wording. That decision names `glob` among *"the
reproducible half of the library"* and separately rejects `list_directory` *"until
G7 lands and it sorts"*. §10.5 argues that sorting is not the problem, that a glob
over a live data directory returns a different set on a different day, and that
this is exactly the recordability test that decision used to reject `http`. The
proposed fix — record the expansion — keeps `glob` permitted and is offered as
Decision 21 rather than as an exclusion, because excluding it would forbid
`data-io.md`'s own worked example.

**7b. `effects.md` Decision 2's closed set, which this note did *not* breach.**
Recorded because the temptation was real and the refusal costs something. Progress
reporting is a property of a body rather than a signature, which is where effects
live, and a `progress` bit would have been the natural design. Decision 2 closes the
set at three and calls the closure *"the most load-bearing sentence in the note"*;
`reproducibility.md` §4.1 already proposed a fourth bit and withdrew it. §9.2
withdraws this one too, and §9.4 pays the price in full: a `pure tool` cannot report
progress.

**7c. `reproducibility.md` Decision 11's run record is per-process.** §10.2 makes it
per-call for a server. That is an extension rather than a contradiction, with one
rough edge stated in place: `env` is read once at start-up, so in a tool-call record
it describes the session rather than the call.

**8. `reserved-words.md` §3.1 and core spec §13, on `model`.** §13's worked
example wanted `model` as the parameter naming the fitted line shape, and had to
write `line_shape` because §13 still reserves the word. That note argued the
reservation protects a hypothesis F1 does not hold. **This is one more data point
on its side, from a note written for a different purpose**, and it is recorded
because that is how a collision list earns its entries.

---

## 19. Risks

**The keyword is the irreversible part, and the falsifiers have not been run.**
§2.6 names three things that would make Decision 1 wrong, and none of them can be
checked without real servers. The mitigation is the ordering in §14: build
`sciencec tools --json` first, generate schemas for ten real tools, and only then
commit the grammar. **If the grammar lands before that measurement, this note has
made the mistake it spent §2 arguing against.**

**A mandatory description is not a good description.** `SC0190` buys a non-empty
string, not a useful one, and `## Fit peaks.` satisfies it. This is exactly the
failure `strings-formatting-and-docs.md` §5.4 predicted when it refused to check
that a prose list covers every parameter — *"a checker that demanded a tag per
parameter would produce `## @param x the x`"* — and Decision 5 risks being the
thing that note refused, at one remove. The defence is narrow and should be judged
narrowly: requiring the field does not make it good, and it does remove the case
where there is no field at all, which is the common one.

**The schema generator will be asked to become a type system.** The first request
after this ships will be `format: "date-time"` on a string, then `pattern`, then
`minLength`, then a range on a number. Each is one line of emitter and each is a
refinement type entering Science through a side door, in a language whose §12 has
declined much less than that. **The rule, stated now so it can be pointed at
later: the schema says what the type says, and nothing more. A constraint that is
not in the type is checked in the body and explained in the description.** If a
constraint is worth expressing, it is worth expressing as a type, and that is a
different note.

**A tool description cannot be assembled at run time, and this is load-bearing for
more than readability.** A `tool`'s description is a `##` run in the source: a
compile-time constant, not a computed string. A server therefore cannot build a
description out of data it read from somewhere, which is the mechanism behind the
prompt-injection-through-tool-metadata class of attack the specification warns
clients about (§1.6). Science gets that property by construction — and **it gets
it only as long as §15's dynamic registration hatch stays unbuilt**, because that
hatch takes a description as a runtime `String`. If it is ever built, this
property is gone and the note should say so there.

**MCP's revision cadence outruns a language's, and the failure is total rather
than graceful.** §1.1's list is the evidence, and §1's opening is the sharp part:
the specification's own compatibility matrix marks both Modern→Legacy and
Legacy→Modern as **"Fails"**. There is no degraded mode. So a user whose client
speaks a revision the `mcp` package does not gets nothing, not less. That is a
package-manager problem with a package-manager answer
(`package-manager.md`'s minimal version selection, and §17 item 11's pinned
revision), and it is not thereby a pleasant experience. **It is also the single
best argument for Decision 1's layering**: when this happens, and it will, what
breaks is a package version, not a program's syntax.

**Designing against an extension means tracking two release trains.** Tasks is
`Stable` and SEP-2663 is Final (§1.7), so the wire format is not expected to churn —
but `ext-tasks` releases on its own cadence, and the extensions policy says a
breaking change takes **a new identifier** rather than a new version. So the
identifier is the version pin, and the `mcp` package now has two revisions to track
where §19's earlier risk assumed one. The mitigation is §17 item 11: pin both, in
the manifest, where a bump is visible.

**A promoted call changes the *shape* of the response, and the trigger is how fast
the machine is.** This is the most uncomfortable consequence of Decision 14a and it
should not be buried. A `fit_peaks` call that finishes in four seconds on a
workstation returns a `CallToolResult`; the same call on a loaded cluster node
returns a `CreateTaskResult` and a `taskId`. Nothing in the source differs. The
specification is explicit that clients must cope — *"A client that has negotiated
this extension **MUST** be prepared to handle either"* — and a client that has
negotiated the extension and then validates strictly against the **core** schema
will still break, because `CreateTaskResult` is not in core's
`CallToolResultResponse` union. `task_after = 0` disables promotion entirely and is
the escape for anyone who meets such a client.

**A cached provenance record is server state in a protocol that just removed
state.** §10.3 argues it is tolerable because the record is content-addressed and
bounded, and it is still state, and a restarted server loses it.
`--provenance=inline` is the escape and costs 8 KB per result.

**Decision 8 can quietly become false.** If ask 3 in §17 is refused, doc tests
inside tool descriptions run on a separate target, and a server built without that
target ships examples nobody verified — while this note still claims they cannot
be wrong. Whoever refuses that ask should edit §5.4.

---

## 20. Open questions

**The second backend, and it should be answered early.** §2.6's third falsifier is
the one that decides whether Decision 1 was right, and it is the only one that can
be checked *before* real servers exist: write a second emitter — a function-calling
schema for a chat completions API — over the same `tool` declarations. If it is a
different emitter over the same declarations, Decision 1 holds. If it needs
different declarations, `tool` was an MCP keyword. This is a few days of work and
it is worth more than anything else in §14.

**Progress and cancellation — answered in §9, and the record of what it looked
like before is worth keeping.** An earlier draft of this note recorded these as
unsolved and called them *"the strongest argument for the `tool`-as-record Option
B"*, on the ground that a progress sink would have to be a parameter that is not in
the schema. That turned out to be wrong twice over: a sink does not have to be a
parameter (§9.3 puts it where the logger already is), and Tasks does not want a
per-tool declaration at all (§9.1), so the mechanism that looked like it would
force an exception to Decision 3 requires no exception. **What remains open is
narrower**: whether `task_after` as a single per-server duration is the right
granularity, or whether it eventually wants to be per-tool — which would reopen
exactly the question SEP-2663 closed, and should therefore be resisted until
somebody has a server that needs it.

**Whether a task should survive the process.** §9.1 sets `ttlMs` to the stdio
process's own expectation, because a subprocess's tasks die with it and there is no
`tasks/list` to recover a lost `taskId`. A server that persisted tasks to disk could
set `null` and honour it, and would then need a store, a reaper and a story about
two servers sharing one directory. Not designed here, and it is the first thing an
HTTP transport would need.

**What `Content` is, exactly.** Text, image, audio, embedded resource and resource
link are five variants. As a `choice` in the `mcp` package that is a
payload-carrying `choice`, which §4.7 forbids in a *parameter* and which is fine in
a *return* because the library serializes it by hand. The asymmetry is defensible
and slightly awkward, and somebody should write it down properly.

**Whether the doc-test fences belong in the description.** Decision 8 says yes, on
the ground that a compiled example is the best thing a model can be given. The case
against: fences make the description several times longer, descriptions are in
every `tools/list` response, and a server with forty tools could spend a
substantial part of a model's context on examples. A `--no-examples` build flag is
the obvious compromise and it reintroduces the possibility of a description that
differs between builds. Unresolved.

**Whether a server should be an object.** §13 writes tools at module level and
composes them in `main`. A server with per-session state — an open file handle, a
cache, a database connection — has nowhere to put it, because a `tool` is not a
method (`SC0193`). The options are a `Server` value threaded through by the
library, module-level `const` state that cannot be mutable, or allowing `tool`
inside a `has` block after all. **Not decided here**, and it is the question most
likely to force a revision of the grammar, so §14's ordering matters: it should be
answered before the parser arm is written, not after.

**Whether `tools/list_changed` can ever be true for a Science server.** The tool
list is fixed at link time (§2.6, falsifier 1), so the capability is advertised as
`{"listChanged": false}` and the notification is never sent. That is the clearest
statement of what a static declaration form costs — and it is also, in
`2026-07-28`, almost free: the notification is no longer freely pushed but
delivered only on a `subscriptions/listen` stream the client opted into (§1.6), so
a client that never subscribes loses nothing at all.

**The other side of that is a small win nobody would have designed for.**
`2026-07-28` requires `ttlMs` on list results. A server whose tool list cannot
change can advertise a long cache lifetime **and be telling the truth**, where a
server that registers tools from a database has to guess. This is the only place in
the note where the static declaration form is better on the wire rather than merely
better in the source, and it is recorded because it was not anticipated.

---

## 21. Summary of decisions

| # | Decision | Rejected | Reason | Cost |
|---|---|---|---|---|
| 1 | `tool` is a declaration form and declares *a callable exposed to a model*; MCP is one backend in a Level 3 package | A hand-written schema beside the handler; a record type plus a derive; an attribute grammar | The abstraction is not MCP's, so `rust-binding-generation.md` Decision 10 does not bite. §2.5 | A second function-declaration form in every front-end pass; five new ways to fail to compile |
| 2 | `prompt` and `agent` stay reserved and unspent | Spending all three F4 words together | Prompt arguments are not schematised, so the one argument that earns `tool` a declaration is absent; `agent` is the opposite direction and needs F5 | A visible asymmetry someone will ask about |
| 3 | The input schema is generated from the parameter list; there is no way to write one by hand | Both mainstream SDK shapes | Two artefacts diverge, and the divergence is invisible and is told to a model as fact | Fewer types may cross than a `function` admits |
| 4 | `T?` means the property is omitted from `required`; absent and explicitly-`null` are the same | Modelling all three JSON states | The language has two states and should not pretend otherwise | "unchanged versus cleared" must be modelled as a `choice` |
| 5 | A `tool` with no doc comment does not compile | A lint; requiring it only for `public` | The description is an input to the caller's control flow, and its absence fails silently | `##` becomes load-bearing; contradicts a sibling note in one sentence |
| 6 | `title` is the summary line, `description` is the whole comment | Inventing a second split | §5.4 of the strings note already splits it there | The summary appears twice |
| 7 | A `##` run may precede a `tool`'s parameter and becomes that property's description | Leaving properties undescribed; `@param` tags | A parameter description is new information, not a restatement of the signature | One row asked of a sibling note's table; multi-line parameter lists |
| 8 | A `tool`'s doc test is the example the model reads | Shipping unverified examples, as every other SDK does | Science already compiles them | Doc tests must run on every build for tools |
| 9 | A `Quantity`'s schema is `{"type":"number"}` with a generated unit sentence; the wire is SI coherent base units | An `x-unit` keyword; a unit-bearing string with a `pattern` | Nothing reads a custom keyword; a wire grammar for units is a much larger commitment | The wire unit is not the unit the audience writes. §6.2 |
| 10 | Every non-null `Error?` from a `tool` body becomes `isError: true`; a tool body cannot produce a protocol error | Classifying errors by type or by an annotation | The protocol's two channels are separated by *where*, and a one-channel error model has exactly one where. §7.1 | An error is one text block, not content blocks |
| 11 | A record return emits `outputSchema` and `structuredContent`, plus the serialized JSON as text | Emitting only one of the two | §1.4 says a server SHOULD do both | Results are roughly twice the size |
| 12 | `pure tool` computes `readOnlyHint`, `openWorldHint` and `idempotentHint`; an impure tool emits none of them | Emitting `false`; letting the author assert | The effect bits can falsify, not classify, so `false` would be a claim the compiler cannot support | Only two of four annotations, and the client distrusts them anyway |
| 13 | `SC0519`, a warning: `print` reachable from a `tool` body under the stdio transport | Documentation saying "do not print" | The effect graph already answers it; §1.5 makes it a protocol violation | A warning, not an error, and conservative at dynamic dispatch |
| 14 | A long tool returns a task; the `tool` declaration says nothing about it, and promotion is a time threshold in the library | A `long` modifier; an author-declared list of slow tools | The protocol **deleted** the per-tool `execution.taskSupport` field in `2026-07-28`; slowness is a property of the call, not the tool. §9.1 | The response *shape* depends on machine speed. §19 |
| 14b | `progress.report` takes a fraction **and** a message | Fraction only | A task has **no** progress notifications — only free-text `statusMessage`, verified — so the message is the half that survives both paths. §9.1 | On the task path the fraction is rendered into text |
| 15 | Progress goes through a process-global sink, not a parameter and not a new effect bit | A distinguished parameter excluded from the schema; a fourth effect bit | `stdlib-standard.md` §8.2's logger exception already covers it: it cannot change the computed result. §9.3 | A `pure tool` cannot report progress. §9.4 |
| 16 | Cancellation is cooperative and shares `progress.report`'s call site | Killing the thread; a compiler-inserted check at loop back-edges | The protocol's own model is cooperative — *"a server is not obligated to actually stop the work"*; and a check in every loop back-edge is disqualified by §1 of the core spec. §9.5 | A body inside BLAS or polars cannot be cancelled at all. §9.6 |
| 17 | One tool call at a time by default; the bound is declared beside the thread budget | Concurrent by default, as every other SDK | The pools are singletons that cannot be re-partitioned per call, so concurrency buys latency and not throughput. §9.7 | Head-of-line blocking, which is only tolerable because of Decision 14 |
| 18 | For a server, a run record is **one tool call** | A per-process record | A per-process record would describe the session and no answer. And a tool call's arguments are already typed and already schematised. §10.2 | `env` becomes session-scoped inside a per-call record |
| 19 | The provenance id travels in `_meta` on every result; the full record is a `resource_link` | `structuredContent`; inline always | `structuredContent` is bound to `outputSchema`, so it would put a field in the schema that no return type produces. §10.3 | ~110 bytes always; a cache that is server state |
| 20 | `--deterministic` stays whole-binary; the `ambient` bit is reported **per tool**, always | Letting one ambient tool poison the binary's claim | `effects(DefId)` is per-definition already; the coarsening was the added step. §10.4 | None found |
| 21 | A `glob` writes its **expansion** into the run record | Adding `glob` to the rejected set | It would forbid the commonest data-loading idiom; the problem is that the result is invisible, not that the call is wrong. §10.5 | A large directory truncates under the 8 KB cap; the call becomes auditable, not repeatable |
| 22 | Start-up speed is not claimed as a reason | Claiming it | Measured: ~250 ms per process, once per session. §11 | The argument that survives is deployment, which is a different argument |
| 23 | A type whose guarantee is structural does not cross silently; `SC0504` names the guarantee being lost | A runtime serialization error; a silent numeric collapse | `PValue`, `Train of R` and friends exist to withhold exactly what JSON would reveal | One more diagnostic whose wording has to be good |

---

## 22. The one-paragraph version

A Science program can expose itself to a model, and the thing that makes it worth
doing is not speed and not units — it is that **the schema, the description and
the examples are all generated from the one declaration the author wrote for a
human, so none of them can be wrong about the others.** That is a property of a
compiled, fully-annotated, doc-comment-preserving language and it is not available
to an SDK that has to introspect at run time or be handed a schema. The price is a
keyword, five compile-time obligations, a narrower set of types than a `function`
admits, and a mandatory doc comment — and the whole of §2 is about making sure the
keyword buys an abstraction that outlives the protocol that suggested it.

**And one thing more, which is not about ergonomics at all.** Because every Science
binary already carries a provenance record, a tool result can carry twelve hex
characters that recover the compiler, the flags, the linked BLAS, the package
digests and the inputs — so a number a model reports stops being unattributable.
That is smaller than "reproducible science" and it is true, and it is the only claim
in this note that another SDK could not match by trying harder.
