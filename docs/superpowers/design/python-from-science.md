# Science — Design: Python called from Science

Date: 2026-09-16
Status: draft for review
Owns: the reverse direction of the Python boundary — `use python`, the dynamic
protocol, the ergonomics of calling a fallible dynamic language under the Go
error model, standalone embedding, and what `.pyi` stubs may and may not be used
for.
Parent: `python-interop.md`. That note owns the direction that matters for
adoption — **Science called from Python** — and §6 of it is a four-paragraph
answer to the question this note spends a whole file on. Where the two disagree,
§9 here names the section and the reason.
Depends on: `native-dependencies.md` (Decisions 1, 7, 9, 10, 11, 12, 15 — its
provider machinery is the instrument this note reaches for, and its redefinition
of "self-contained" is the thing this note has to argue with);
`syntax-revision-2.md` §3 (the error model, which is what makes §3 here
necessary) and §8 (the inline-block ruling, which this note narrows the *reason*
for without touching the ruling); `ffi-c-boundary.md` §5.1 and §5.2 (library
search, `when available`, `dlopen`); `rust-interop.md` §2.3 (the zero-percent
result that §1 here is measured against); `package-manager.md` §4.2 and §4.5
(the manifest this note adds a table to).
Syntax: revisions 2 and 3 throughout — `def`, `->`, `of`, `borrowed`, `is` / `is
not`, `> < >= <=`, `interface`, `Type has:`, `T?`, `-> (T, Error?)` with `?` as
the presence test, `for x in xs`, `loop:` / `break`, `print`. The parent note is
written in the `Result` / `try` model; every quotation from it is translated on
the way in, and §3.1 states the translation once.
Diagnostics: `SC0180`–`SC0189` (syntax) and `SC0459` (Python codegen), per the
README's allocation table. `SC0455` is **redefined** rather than claimed, in
§4.4.

---

## 0. The position, in one paragraph

The project's owner has asked that all Python code be usable from Science. The
honest answer is that **Science can call all of Python and cannot be Python**,
and that the gap is not where anyone expects it. It is not the type system — a
dynamic protocol reaches ninety-nine percent of scikit-learn by name, because
there are no static types to lose at the boundary. It is not the ABI. It is not
the packages. It is two specific things: Python code that *defines* a class
Python then introspects, and Python code that expects to be the process. What
stands between the design as `python-interop.md` §6 leaves it and that ceiling
is not one large problem but eight small ones — iteration, subscripting,
operators, context managers, container conversion, callbacks, subclassing,
keyword names — plus one large one that revision 2 created and the parent note
never had to face: under `-> (T, Error?)`, a five-line Python snippet becomes
fifteen lines of error checks, and a feature that costs three times its Python
equivalent in lines is a feature people read about rather than use.

---

## 1. Why Python is the middle case, and why that is worth a section

Science now has three foreign-ecosystem notes, and the three outcomes are so
different that the difference is itself a design fact.

**C** has a stable ABI and headers. The types in a C signature *are* ABI types;
nothing is lost crossing the boundary because there was never anything on the
other side of it. Binding is mechanical, `ffi-c-boundary.md` §7 generates it,
and essentially the whole of a C library's public surface is reachable. The unit
of work is **a header**, and there is one per library.

**Rust** has rich static types that do not survive the ABI, and
`rust-interop.md` §2.3 reports the result without softening it:

> **Zero.**
>
> Not "a small fraction". Zero, for essentially every crate in the registry […]
> There is no signature in `regex`'s public API — not one — that is
> `extern "C"`-callable.

Every function must be hand-re-expressed in a shim; that note budgets 215 lines
of Rust for one deliberately simple crate. The unit of work is **a crate**, and
there is one shim per crate, forever.

**Python has no static types to lose.** Every value is a `PyObject`. The
protocol for reaching into one — `PyObject_GetAttr`, `PyObject_Call`,
`PyObject_GetItem`, `PyObject_GetIter`, `PyNumber_Add` — is uniform, total, and
closed. Write it once and it covers every package that exists, every package
that will ever exist, and every object those packages will ever construct. The
unit of work is **nothing**. There is no per-library binding at all.

| | C | Rust | Python |
|---|---|---|---|
| Unit of binding work | a header | a crate | **nothing** |
| Per-library hand-written code | generated declarations | ~215 lines of Rust | none |
| Fraction of a library's named API reachable | ~100% | **0%** unmodified | **~99%** |
| What is lost crossing | nothing — they were ABI types | every static type | nothing — there were none |
| Reaches a library published after Science ships | needs its header | needs a new shim | **automatically** |
| What must exist on the machine at run time | a `.so`, or nothing | nothing | **an interpreter and `site-packages`** |
| What the compiler can check about a call | the declared signature | the shim's signature | **nothing, ever** |

Read the last two rows against the first four and the shape of the problem
appears. **The property that makes Python the easiest of the three to reach is
the same property that makes it the hardest to ship.** Python is uniform at the
boundary because a runtime is doing all the work that C and Rust do at compile
time — attribute lookup, dispatch, arity checking, coercion. That is why no
per-library binding is needed. It is also, exactly and unavoidably, why the
runtime has to be present when the program runs. The binding cost and the
deployment cost are not two facts about Python. They are one fact seen from two
sides, and any design that takes the first without paying the second is lying
about one of them.

The consequence for this note's structure: §2 is short on mechanism and long on
enumeration, because reaching Python is a matter of covering protocols rather
than of solving a hard problem; §4 is where the difficulty actually lives.

---

## 2. Coverage in hosted mode

### 2.1 The protocol as `python-interop.md` §6.3 leaves it

Translated into revision 2, §6.3 specifies exactly four things:

- one opaque type, `PyObject`, with `FromPython` and `ToPython` for conversion;
- attribute access and calls through the dynamic protocol;
- keyword arguments via Science's named-argument syntax, which lines up exactly;
- tensors crossing by DLPack, in both directions, without copying.

The two interfaces, in revision-2 spelling:

```science
interface ToPython:
    def to_python(borrowed self) -> (PyObject, PyError?)

interface FromPython:
    def from_python(value: borrowed PyObject) -> (Self, PyError?)
```

That is a real design and it is not a small one: it reaches every *function* and
every *method* in the scientific Python ecosystem. The question this section
answers is what fraction of a real program consists of function and method
calls, and the answer is: less than half.

### 2.2 Two programs, operation by operation

The parent note names `scikit-learn` and `astropy` as the motivation, on the
grounds that they have no C API and so are unreachable by any other route. Here
they are, as a scientist writes them, decomposed into elementary Python
operations. The middle column is §6.3 as written; the right-hand column is §6.3
plus the eight decisions in §2.3.

| # | Operation | Kind | §6.3 | +§2.3 |
|---|---|---|---|---|
| 1 | `fits.open(path, memmap=False)` | module attribute, call, keyword | ✓ | ✓ |
| 2 | `with fits.open(…) as hdul:` | context manager | ✗ | ✓ D3 |
| 3 | `hdul[0]` | subscript | ✗ | ✓ D2 |
| 4 | `hdu.data` | attribute | ✓ | ✓ |
| 5 | `hdu.header` | attribute | ✓ | ✓ |
| 6 | `WCS(header)` | construction | ✓ | ✓ |
| 7 | `w.pixel_to_world(x, y)` | method, positional | ✓ | ✓ |
| 8 | `coord.ra.deg` | attribute chain | ✓ | ✓ |
| 9 | `float(coord.ra.deg)` → `F64` | `FromPython` | ✓ | ✓ |
| 10 | `for hdu in hdul:` | iteration | ✗ | ✓ D1 |
| 11 | `hdu.name == "SCI"` | conversion then compare | ✓ | ✓ |
| 12 | `image - np.median(image)` | operator | ✗ | ✓ D2 |
| 13 | `np.asarray(image)` → `Tensor` | DLPack | ✓ | ✓ |
| 14 | `q.to(u.km / u.s)` | operator on a module attribute | ✗ | ✓ D2 |
| 15 | `make_pipeline(StandardScaler(), PCA(n_components=8))` | nested calls | ✓ | ✓ |
| 16 | `{"pca__n_components": [4, 8, 16]}` | `dict` and `list` construction | ✗ | ✓ D4 |
| 17 | `GridSearchCV(pipe, grid, cv=KFold(5, shuffle=True))` | keywords, nested | ✓ | ✓ |
| 18 | `search.fit(X, y)` | method, tensors in | ✓ | ✓ |
| 19 | `search.best_params_["pca__n_components"]` | attribute then subscript | ✗ | ✓ D2 |
| 20 | `est.named_steps["pca"].explained_variance_ratio_` | chain through a subscript | ✗ | ✓ D2 |
| 21 | `for tr, te in KFold(5).split(X):` | generator, tuple unpacking | ✗ | ✓ D1 |
| 22 | `cross_val_score(pipe, X, y, scoring=my_scorer)` | Science callback passed in | ✗ | ✓ D5 |
| 23 | `class MyScaler(BaseEstimator, TransformerMixin):` | subclassing a Python class | ✗ | ✓ D6 |
| 24 | `GridSearchCV(MyScaler(), …)` → `clone()` → `inspect.signature` | introspection of a Science-defined type | ✗ | **✗** |
| 25 | `joblib.dump(search, …)` where a step is Science-defined | pickling a Science object | ✗ | **✗** |
| 26 | `np.zeros(shape=(4, 4))` | keyword whose name is a Science keyword | ✗ | ✓ D8 |

**Twelve of twenty-six** reachable as §6.3 stands: **46%**.

The number that matters more is the program-level one, and it is worse than 46%
suggests, because the thirteen misses are not exotic. Rows 3, 10, 12, 16, 19, 21
and 26 are *subscripting, iteration, arithmetic, list and dict literals, and a
keyword named `shape`*. A Python program that uses none of those is a program
that calls functions and reads attributes and does nothing else. Searching for
one in a scientific codebase is not a useful exercise.

> **So the honest statement of §6.3's coverage is: ~46% of elementary
> operations and approximately 0% of real programs.** It reaches the whole API
> and almost none of the language, and an API you cannot index, iterate or add
> is not usable.

That is not a criticism of §6.3, which is four paragraphs inside a note about
the other direction and never claimed to be a specification. It is the reason
this section exists.

### 2.3 The eight things it does not reach, and what to do about each

None of these is hard. All of them are decisions.

> **Decision 1 — `PyObject` implements `Iterate`.** `for x in obj` lowers to
> `PyObject_GetIter` once and `PyIter_Next` per step, and tuple destructuring in
> the loop binding lowers to `PySequence_Fast` on the yielded value. A raised
> exception during iteration sets the region's error binding (§3) and terminates
> the loop; `StopIteration` terminates it normally.

**Rejected alternative: an explicit `obj.iterate()` returning an
`(PyObject?, PyError?)` pair the user steps by hand in a `loop:`.** It is
mechanically honest and it is four lines per iteration, which is the whole
problem §3 is about. Rejected on ergonomics, and because `KFold.split` returns a
generator — a sizeable fraction of scikit-learn cannot be used at all without
iterating, so making iteration the awkward case makes the library the awkward
case.

**Cost.** `Iterate` in `collections-and-chains.md` does not model a `next()`
that can fail, and this is the first implementor that needs it. §10 asks that
note to confirm the shape rather than inventing one here.

> **Decision 2 — subscripting and the arithmetic operators are available on
> `PyObject`, and only inside a `python:` region (§3).** `a[k]` lowers to
> `PyObject_GetItem`, `a[k] be v` to `PyObject_SetItem`, `a + b` to
> `PyNumber_Add`, and so on for the operator set. Outside a region, the existing
> "no implementation of `Add` for `PyObject`" type error fires, with a help line
> naming the region and naming the explicit method form (`a.item(k)`,
> `a.add(b)`), which is always available.

This is the decision that makes the region load-bearing rather than cosmetic,
and it deserves its reason stated plainly: **under the revision-2 error model an
operator cannot be fallible.** `a + b` is an expression that produces a value;
there is no syntax for it to produce `(PyObject, PyError?)`, and inventing one
would be inventing a second error model. Inside a region, where the failure edge
is the region's own exit, an operator *can* produce a plain value — so the
region is not sugar over something the language could otherwise say. It is the
only construct in which these operations can be spelled at all.

**Rejected alternative: operators on `PyObject` panic on a Python exception.**
Rejected because `python-interop.md` §7.2 is explicit that a panic is
unrecoverable within Science, and a `KeyError` from `d[k]` is an ordinary,
expected, recoverable condition that every Python program handles. Turning the
single most common recoverable Python failure into an unrecoverable one is the
wrong trade in both directions at once.

**Rejected alternative: operators always available, returning `PyObject`, with
the error deferred to the next checked operation** (the errno model, or NumPy's
`np.errstate`). Rejected: it gives the language a hidden mutable error slot, it
makes the point of failure and the point of report different, and the parent
note's §7.3 already pays to capture a traceback *eagerly* for exactly this
reason.

> **Decision 3 — a Python context manager is a Science value whose `Drop` calls
> `__exit__`.** `obj.entered()` calls `__enter__`, returns a `PyScope` holding
> both the manager and the bound value, and its drop glue calls
> `__exit__(None, None, None)` at end of scope.

This is the one place where Science is straightforwardly better than Python at
Python's own job. `with` exists in Python because Python has no deterministic
destruction; Science has it, so the construct needs no statement form. The
lifetime is the binding's lifetime, which is visible in the source, and it
composes with early return and with the region's failure exit because drop glue
runs on those edges already.

**The hole, stated.** `__exit__` can *suppress* an exception by returning true,
and it receives the exception triple. A `Drop`-based mapping passes `None, None,
None` and discards the return value, so **exception-suppressing context managers
do not work**: `contextlib.suppress`, `pytest.raises`, and any transaction
manager that rolls back and swallows. Managers that only *release* —
`fits.open`, `h5py.File`, `torch.no_grad`, `plt.style.context`, a lock, a
temporary directory — which is the overwhelming majority in scientific code,
work exactly right. The diagnostic for a known-suppressing manager is not
available (we cannot know), so this is documentation, and it is the kind of
documentation people do not read. It is in §11.

> **Decision 4 — Science containers convert to Python containers and back,
> through `ToPython` and `FromPython`, by copying.** `Array of T` ↔ `list`,
> a Science map ↔ `dict`, tuples ↔ `tuple`, `String` ↔ `str`, `T?` ↔ `T`-or-
> `None`. `python.list()` and `python.dict()` construct empty ones.

Copying, and the parent note's §3.4 warning applies unchanged and in the same
words: a Python `list` is a vector of pointers to boxed objects and there is no
shared representation. The compiler emits the same warning (`SC0457`) for a
large numeric `Array` crossing here as it does for one crossing the other way,
naming `Tensor` as the fix. Nothing numeric should ever travel as a `list`; a
hyperparameter grid should, and that is the case this decision exists for.

> **Decision 5 — a Science closure can be passed into Python as a callable.**
> `python.callable(f)` wraps a Science closure of type `def(PyObject) ->
> (PyObject, PyError?)` in a generated heap type with `tp_call`. The wrapper
> owns the closure. On a returned non-null error the wrapper raises; when the
> error is null it returns the value.

The lifetime rule is the same one `python-interop.md` §4.4 already states for
buffers, applied to code: the Python object owns the closure, so the closure
must be owned rather than borrowed, and a closure capturing a borrow is rejected
by the existing ownership checker with the existing diagnostic. `python-interop`
§5.4 already decided the GIL consequence — a function with a Python callable in
its parameters carries the `python` effect and the GIL is held for its duration
— and that falls out here for free, in the other direction.

**What this buys:** `scoring=`, `key=`, `callback=`, `func=` in `apply`,
`minimize(fun=…)` in SciPy, every `matplotlib` event handler. It is the pattern
that makes SciPy's optimisers reachable, which is a larger prize than it looks.

> **Decision 6 — a Science type may subclass a Python class, through an explicit
> runtime construction, and the result is not fully introspectable.**
> `python.subclass(bases, name, methods)` builds a heap type with
> `PyType_FromModuleAndSpec` whose methods dispatch to Science closures.

This is the one that scikit-learn users want most and it is the one that does
not fully work. `sklearn.base.clone` — which `GridSearchCV`, `cross_val_score`,
every ensemble meta-estimator and `Pipeline` itself all call — reconstructs an
estimator by reading `get_params()`, and the default `get_params()` obtains the
parameter names from `inspect.signature(cls.__init__)`. A Science-defined
`__init__` is a C function pointer. `inspect.signature` raises `ValueError: no
signature found for builtin`, and the failure surfaces three frames inside
scikit-learn with a message about a builtin, to a user who wrote no builtin.

Two mitigations, and neither is complete:

- **Emit `__text_signature__`** on the generated `__init__` from the Science
  signature. This is the Argument Clinic convention CPython uses for its own
  builtins, `inspect.signature` reads it, and it is a string the compiler
  already has all the information to write. It fixes `clone` for estimators
  whose parameters are all simple. It does not fix anything that reads
  `__init__.__code__`, and it does not fix `__init__` parameters whose defaults
  have no `repr`.
- **Generate a thin Python source shim** — an actual `class X(Base):` whose
  `__init__` is Python and whose body delegates — which fixes introspection
  completely and reintroduces a Python file into the build, which is the thing
  `syntax-revision-2.md` §8.4 argues against on tooling grounds. It is the right
  answer for the subclassing case specifically and it is not the default.

**Decision, narrowly:** ship `python.subclass` with `__text_signature__`, and
say in the diagnostic and the documentation that a Science-defined Python class
is **callable, not introspectable**. §6 prices what that costs.

> **Decision 7 — `use python` imports lazily, at first use, never at module
> initialisation.** `use python "sklearn.decomposition" (PCA)` binds `PCA` to a
> cell; the first read of it performs `PyImport_ImportModule` and then
> `PyObject_GetAttr`, both of which are ordinary fallible Python operations.

This is a narrowing of the parent note's §3.3, made explicitly because `use
python` looks like it breaks that section's best decision:

> the module **imports nothing** at init, and has no Python dependencies at all
> […] This is the single most important packaging decision in this note.

A `use python "sklearn"` that imported at `Py_mod_exec` would make a Science
extension module hard-depend on scikit-learn at import time, which is exactly
what §3.3 forbids, and would make an `ImportError` during someone else's
`import` the first thing a user of the module ever saw. Lazy import preserves
§3.3 intact. **It also produces the standalone case's biggest question** — a
standalone binary that imports lazily discovers a missing package after the
queue wait — and §4.8 decides that one the other way, for a reason that only
applies when there is no host to be a good neighbour to.

> **Decision 8 — the label rule: a word immediately followed by `:` in argument
> position is an argument label, whether or not it is a reserved word.**

`np.zeros(shape: (4, 4))`, `pytest.raises(ValueError, match: "…")`,
`argparse.add_argument("--n", type: int)`. `shape` is a Science keyword (core
spec §13), `match` is a Science keyword, and `type` is a Science keyword; all
three are ordinary and frequent Python parameter names, and with no rule here
they are simply unreachable — a keyword argument you cannot spell is an API you
cannot call.

This is the exact sibling of `reserved-words.md` §0.1's **dot rule** ("any word
after `.` is a member name"), it is the same five lines of parser, and it is
unambiguous for the same reason: in argument position, a word followed by `:` is
never an expression. It should be adopted for *all* calls, not only Python ones,
because the parser cannot know which is which and because Science's own named
arguments benefit identically. §10 asks `reserved-words.md` to take it.

For the residue — a keyword name computed at run time, or `*args` / `**kwargs`
splatting — there is one explicit escape and no sugar:
`python.call(f, args, kwargs)`, taking an `Array of PyObject` and a map. Sugar
over a mechanism that works is cheap; a mechanism whose only form is sugar is
not, which is `syntax-revision-2.md` §8.5's ordering applied again.

### 2.4 What the eight decisions add up to

Twenty-four of twenty-six operations: **92%**. The two that remain are rows 24
and 25, and they are the same thing twice — **Python introspecting or
serialising a type that Science defined**. Not calling it. Introspecting it.

At the program level, with the caveat that the denominator is two programs and
not a corpus (§11): **roughly 80% of real scientific Python programs could be
transliterated and run unmodified.** The 20% is dominated by one pattern — the
custom estimator, the custom `Fittable1DModel`, the custom `nn.Module`, the
custom `Dataset` — which is not exotic in machine-learning code and is close to
universal in it.

A phrase worth fixing, because it is the ceiling and §6 restates it: **Science
can call all of Python; it cannot be Python.**

---

## 3. Ergonomics, which decides whether anyone uses it

### 3.1 The problem revision 2 created

`python-interop.md` §6.3's example is three lines:

```
let model be try PCA(n_components: 8)
let fitted be try model.fit_transform(points)
Ok(try fitted.into_tensor())
```

`syntax-revision-2.md` §3.3 already saw what its own decision does to this, and
underestimated it:

> **Interaction with `python-interop.md`.** §6.3 of that note writes every
> Python call as `Result of (PyObject, PyError)` and uses `try` for the
> ergonomics. That section needs rewriting against this model; the mapping is
> mechanical (`-> (PyObject, PyError?)`) and **the note's substance does not
> change.**

The mapping is mechanical. The substance does change, and here is the same
function, written out:

```science
def reduce(points: borrowed Tensor of (F32, (n, d)))
        -> (Tensor of (F32, (n, 8)), Error?):
    let model, err be PCA(n_components: 8)
    if err?:
        return (Tensor.empty(), err)

    let fitted, err be model.fit_transform(points)
    if err?:
        return (Tensor.empty(), err)

    let out, err be fitted.into_tensor()
    if err?:
        return (Tensor.empty(), err)

    return (out, null)
```

Three lines became eleven. That is revision 2's stated tax — "three lines per
fallible call instead of one" — and for Science-to-Science code it is a tax the
note argued for and this note does not reopen.

**For Python it is not the same tax, for a reason §3.3 did not see.** In Science
code, the unit that fails is a *function call*, and a function call is a line.
In Python code, the unit that fails is a *sub-expression*: `hdul[0].data` is
three fallible operations, `search.best_params_["pca__n_components"]` is two,
`q.to(u.km / u.s)` is three. The expansion is not 3× per statement. It is 3× per
*dot*. The astropy program in §2.2 is fourteen lines of Python and, written
flat, is fifty-one lines of Science, of which thirty-four are `if err?:` and its
return.

The consequence is not that the feature is unpleasant. It is that nobody uses
it: a scientist who wants the fourteen lines writes them in a `.py` file, calls
it with `subprocess`, and parses stdout, and that is a worse outcome for
everyone than any amount of design work here.

**The other half of the translation**, stated once so the rest of the note can
use it. `PyError` implements the `Error` interface of `syntax-revision-2.md`
§3.4:

```science
PyError implements Error:
    def message(self) -> String:
        f"{self.type_name}: {self.text}"
```

So `-> (T, Error?)` accepts a Python failure with no conversion at all, which is
the boxed-interface form that note's §3.4 says composes across library
boundaries. A function that can fail in Python and in Science has one error type
and one check. That is a genuinely good property of the new model and it should
be said before the criticism.

### 3.2 The options

**A. Accept the verbosity.** The argument for it is consistency: one error
model, no exceptions, no special case. It is rejected because the premise of
calling Python at all is that you are reusing an idiom you already have, and an
idiom that costs 3.6× its original length is not reused. This is not the same
situation as Science-to-Science code, where the verbosity buys readability in
sequence for code that is *written once and read many times*; glue code is
written to be got through.

**B. Reintroduce `try`, for Python only.** Rejected outright.
`syntax-revision-2.md` §1 is built on the principle that there is exactly one
spelling for each operation, and §3.2 removed `try` deliberately. A `try` that
works on `PyError` and nothing else gives the language two error models, and
gives `sciencec fmt` back the canonicalisation problem §1.1 exists to have
deleted.

**C. Panic on a Python error.** Rejected, in §2.3 under Decision 2, and for the
same reason: a Python exception is recoverable and a panic is not.

**D. A block in which one error check covers a sequence.** This is the
recommendation, and the rest of §3 is it.

### 3.3 The region

> **Decision 9 — the `python:` region.** A block headed `python <name>:`
> declares `<name>: PyError?` in the enclosing scope. Inside the region, every
> Python operation that would produce `(PyObject, PyError?)` produces a
> `PyObject`. On a failure, `<name>` is bound to the error, the remaining
> statements of the region do not run, drop glue runs for everything the region
> has constructed, and control resumes at the statement after the region.

```science
def reduce(points: borrowed Tensor of (F32, (n, d)))
        -> (Tensor of (F32, (n, 8)), Error?):
    python err:
        let model be PCA(n_components: 8)
        let fitted be model.fit_transform(points)
        let out be fitted.into_tensor()

    if err?:
        return (Tensor.empty(), err)

    return (out, null)
```

Seven lines against eleven, and against three in Python. The astropy program is
twenty-two lines rather than fifty-one.

**The rules, exactly.**

1. Bindings introduced in the region are in scope after it, and are **definitely
   assigned only on the success edge**. Reading one without first handling the
   failure edge is the compiler's existing use-before-assignment error, not a
   new analysis.
2. Because of (1), the statement after a region must handle `err?` with a
   branch that diverges — returns, breaks, or panics — before any binding is
   read. **The error cannot be dropped on the floor.**
3. The region covers *Python* operations. A fallible Science call inside a
   region still binds a pair and is still checked by hand. §3.5 says why.
4. The region is not a new scope for ownership and not a closure. It lowers to
   exactly the flat form; `sciencec fmt --expand` prints it, and the flat form is
   always legal and is the primitive.
5. A region carries the `python` effect wholesale, so `python-interop.md` §5.2's
   GIL policy and §5.4's parallel-region rejection (`SC0454`) apply to the region
   as a unit rather than to each operation.
6. Regions do not nest (`SC0183`), and a region may not contain `return`,
   `break`, `loop:` or a nested `def` (`SC0180`) — the failure exit and a
   second control-flow exit in the same block is a semantics nobody should have
   to reason about, and closing the region first costs one line.

**Three things this buys beyond line count**, and the second and third are the
reasons it is right rather than merely shorter.

- **It is the only place operators and subscripting can exist** (Decision 2).
  Without the region, `a[k]` and `a + b` on `PyObject` are inexpressible under
  the revision-2 error model, and thirteen of the twenty-six operations in §2.2
  stay unreachable no matter how much anyone is willing to type.
- **It makes the error impossible to ignore.** `syntax-revision-2.md` §3.3 names
  this as the model's own weakness — *"a returned `Error?` can be dropped on the
  floor"* — and mitigates it with `SC0140`. Rule (2) above gets the stronger
  property structurally: a region's bindings are unreadable until the error is
  handled with a diverging branch. **The sugar is safer than the thing it is
  sugar for**, which is unusual enough to be worth checking, and it holds because
  the region has exactly one failure edge and the compiler knows where it goes.
- **It needs no new type-checker feature.** Definite assignment across two edges
  is what `if`/`else` bindings already require. No narrowing correlation between
  two variables, no flow-sensitive typing beyond what revision 2 §3.1 already
  asks for.

**It is not an inline `python { … }` block.** `syntax-revision-2.md` §8 rejected
inline foreign source and this note does not reopen it. A `python:` region
contains **Science** statements, in Science syntax, checked by the Science
parser, with Science spans; the only thing that changes inside it is where the
failure edge of a Python operation goes. Every one of §8.4's objections —
rustfmt cannot see it, mypy cannot see it, the language server must understand
three languages, a foreign syntax error surfaces as a Science diagnostic —
applies to inline source and none of them applies here, because there is no
foreign source.

### 3.4 What the region costs

**It is a partial retreat from `syntax-revision-2.md` §3.2, and it should be
recorded as one.** That section's argument for the Go model is:

> **It is readable in sequence.** The Go model's virtue is that the error path is
> written where it happens, in the order it happens.

Inside a region, the error path is not written where it happens. You cannot see,
looking at line three of a region, that line three can jump to the end. That is
precisely the criticism of exceptions, and this note is taking a bounded dose of
it deliberately.

What bounds it: the jump target is the end of the region, which is on screen and
usually four lines away; the region is marked, so a reader knows they are in one;
the error binding is named in the header, so the reader knows what is being
accumulated; and the region cannot contain a `return`, so the only two ways out
are the bottom and the failure edge. The claim is not that this is free. It is
that a sequence of Python calls is genuinely *one transaction* — if the `PCA`
constructor failed you were never going to fit it — and stating the failure once
per transaction rather than once per call is the correct granularity for this
one case and not for the general one.

**Second cost.** `sciencec fmt` must render both forms and the desugaring must
carry the user's spans into every diagnostic produced inside a region, or the
region becomes a place where error messages point at generated code. That is the
same discipline `llm-ergonomics.md` requires everywhere and it is not free.

**Third cost.** One more construct to teach, in a language whose pitch includes
that there are few of them.

### 3.5 The generalisation this note does not take

The obvious next question is why the region is Python-only, when a general
`check err:` block over *any* fallible call would remove revision 2's verbosity
tax everywhere.

The answer is jurisdiction, not merit. That is a language-wide change to the
error model, it belongs to whoever owns the next revision, and this note should
not decide it as a side effect of a Python feature. What this note can do is
make it cheap to take later: **`python err:` is source-compatible with a future
general `check err:`**, since every program written with the Python-only form
remains correct under a form that covers more. If the general version is adopted,
`python:` becomes a deprecated spelling and nothing written today breaks. §10
puts that in front of the error model's owner rather than answering it here.

---

## 4. Standalone, reopened

### 4.1 What §6.2 decided, and what has changed since

> **Yes in hosted mode, no in standalone mode, in the first version.**
>
> **Standalone**: an AOT binary that starts an interpreter of its own. Rejected
> for the first version (`SC0455` if a `use python` appears in a standalone
> build). Interpreter discovery, virtual-environment resolution, version
> compatibility and packaging are a project of their own, and doing it badly is
> worse than not doing it.

Underneath the four reasons is a fifth, which is the one the section actually
argues:

> A Science binary that embeds Python is not self-contained. […] The deployment
> story becomes Python's deployment story, which is the thing users are running
> toward Science to escape.

Two things have happened since. `native-dependencies.md` **redefined
self-contained**, and it built **provider-and-lockfile machinery** that is a
general answer to "find a native dependency at a version, reproducibly" — which
is, word for word, what interpreter discovery is. Both have to be tested against
§6.2 rather than assumed to help.

### 4.2 The redefinition does not help. It sharpens the objection.

> **Decision 12** [`native-dependencies.md` §7.1]**.** "Self-contained" means:
> **no runtime to install, no interpreter, no VM, no garbage collector, no
> `site-packages`, and no Science-level dependency resolved at run time.**

It is worth being exact about what that does, because the intuitive reading is
backwards. That decision *weakened* the promise: it stopped meaning "statically
linked", so a binary that dynamically links `libopenblas.so` is still
self-contained. The instinct is that a weaker promise is easier for an embedded
interpreter to satisfy.

It is not, because the new definition **names the interpreter and
`site-packages` explicitly**. Decision 12 did not weaken the promise in a
direction that helps here; it moved it onto exactly the axis that an embedded
CPython violates, and it did so deliberately — the comparison it draws is with
Python, because Python's deployment story is the baseline the promise is
measured against. A Science binary embedding CPython is not self-contained under
the old reading *or* the new one, and under the new one it is not self-contained
in the specific words the definition uses.

**So `python-interop.md` §6.2's central argument survives intact and is
stronger than when it was written.** That is the opposite of what this note set
out to find, and it is where the section has to start.

What the redefinition *does* supply is a vocabulary. Decision 12 makes
self-containment a **property of a program**, not of the language — a program
that links OpenBLAS has traded some of it for speed, deliberately, and the
provenance record (Decision 9) is where the trade is written down. That reframes
the question from *may a Science binary embed Python* to *may a Science program
trade self-containment away, and if so, how does the binary say so*. A language
that already ships a mechanism for recording exactly this trade has no principled
reason to forbid the largest instance of it. It has a reason to insist the trade
be declared, checked, and legible, which is what §4.4 does.

### 4.3 The four reasons, one by one

**Interpreter discovery — solved, and not approximately.** This maps onto
`native-dependencies.md` Decision 1 with nothing left over. CPython ships
`python-3.12-embed.pc`, so the `pkg-config` path works; `libpython3.12.so`,
`python312.dll` and `Python.framework` are ordinary shared objects under
`ffi-c-boundary.md` §5.1's search; `python3.dll` on Windows is the stable-ABI
forwarder the parent note already links against in §3.2. The provider list, the
`--library-path` override, `SCIENCE_LIBRARY_PATH`, `sciencec doctor` reporting
what was found — all of it applies unchanged. A cluster with
`module load python/3.12` is served by the `system` provider for precisely the
reason §3.1 of that note gives for BLAS: the site administrator has already made
the choice.

**Version compatibility — solved, and better than §6.2 assumed.**
`native-dependencies.md` Decision 7 requires a probe that reports the library's
own configuration, verified before `main`, aborting on a mismatch. CPython's is
`Py_GetVersion()`, which returns the full version string, and
`Py_GetCompiler()`. The GIL/free-threaded split is an ABI variant in exactly that
note's sense — `variant = "3.12-gil"` versus `"3.13-freethreaded"` — and it is a
variant that must not be got wrong for the same class of reason ILP64 must not
be: two builds with the same `SONAME` shape and different concurrency semantics.
The parent note's §5.5 already enumerates what changes between them.

**Packaging — solved by declining to do it**, following
`native-dependencies.md` Decision 15's shape for CUDA. CPython's licence is
permissive, so unlike cuDNN we legally *could* redistribute it; Decision 15's
other two reasons still apply, and the operative one is that becoming a
distributor of someone else's runtime is a permanent operational commitment with
no relationship to designing a language. Find it; never ship it.

**Virtual-environment resolution — not solved. This is the one.**

### 4.4 What is genuinely special about Python

A virtual environment is not a library. It is a *state*, and three properties
separate it from everything the provider machinery was built for.

1. **Its contents are discovered by `import` at run time, not by the linker at
   build time.** `libopenblas.so` is named in a manifest, keyed by the link name,
   resolved once, hashed, and recorded. The fourteen distributions that
   `import sklearn` transitively pulls in are named nowhere, resolved by walking
   `sys.path` while the program runs, and may be different tomorrow because the
   user ran `pip install`. There is no link name to key on.
   `native-dependencies.md` §5.1's Tier C — *"nothing at build time […] knowable
   only at run time"* — is the honest classification, and Tier C was designed for
   a handful of `dlopen`ed GPU libraries, not for the majority of a program's
   dependencies.

2. **A Python wheel can carry its own native libraries.** This is
   `native-dependencies.md` §2.1's own observation used against it: NumPy and
   SciPy wheels each bundle an OpenBLAS with a mangled `SONAME`, so a Science
   program that links `[native.openblas]` *and* embeds a Python that imports
   NumPy has two BLAS implementations and two thread pools in one address space.
   That note's §11 names this risk and says plainly that neither it nor
   `python-interop.md` currently owns it. Embedding is what makes it common
   rather than hypothetical, so this note claims it (§4.7).

3. **The ambient environment silently changes the answer.** `PYTHONPATH`,
   `PYTHONHOME`, an activated venv, a `.pth` file, `sitecustomize` — any of them
   can change what a binary imports without changing the binary. A build input
   that is not in the lockfile and is read from the environment at run time is
   precisely the thing every reproducibility mechanism in this project exists to
   prevent.

So: the machinery's **shape** transfers exactly, and its **guarantee** does not.
That is the finding, and it decides the form of the answer — not "no", and not
"yes", but "yes, declared and reported".

### 4.5 The decision

> **Decision 10 — `use python` is permitted in a standalone build, behind an
> explicit per-program opt-in, and the resulting binary declares that it is not
> self-contained.**
>
> The opt-in is a `[python]` table in `science.toml`. It is a property of the
> **root** package only; a dependency cannot opt a program into embedding an
> interpreter, for the same reason `package-manager.md` §4.3 refuses to let a
> dependency set a `[profile]`.
>
> **`SC0455` is redefined.** It no longer means *"`use python` in a standalone
> build"*, a prohibition. It means *"`use python` in a standalone build with no
> `[python]` table"*, a missing declaration, and its text names the three lines
> to add and states the consequence in the same words Decision 12 uses.

```toml
[python]
embed    = true
version  = ">= 3.11, < 3.14"
variant  = "3.12-gil"
env      = ["system", "prebuilt"]     # §4.6
isolated = true                       # §4.6

[python.requires]                     # §4.8
numpy        = ">= 1.26"
scikit-learn = ">= 1.4, < 2"
astropy      = ">= 6.0"
```

**Rejected alternative: leave §6.2 as it stands.** The reason given for the
prohibition was that interpreter discovery, version compatibility and packaging
"are a project of their own". Three of the four are now free, delivered by a
sibling note for another purpose. A prohibition whose stated reasons have been
paid for by someone else should be revisited, and leaving it in place would mean
the project ships a mechanism and forbids its largest application without saying
why.

**Rejected alternative: permit it with no opt-in**, treating the interpreter as
an ordinary native dependency. Rejected because it is not ordinary: it is the one
dependency that changes what the headline promise in core spec §3 means for this
program. A trade that large should appear in the manifest where a reviewer looks,
not be inferred from the presence of a `use` line in some file.

**Rejected alternative: permit it only under a build flag, with no manifest
entry.** Rejected because a flag is not archived with the source, and a
two-year-old result's binary should be able to say what it needed. `--locked`,
`sciencec archive` and the provenance record all key on the manifest.

**Cost, stated plainly.** Science now has programs that are not self-contained,
and some of them will be shipped by people who did not read this section. The
mitigations are §4.7 and §4.9 and they are mitigations, not a fix. The
reputational risk is in §11 and it is real: "Science needs Python installed" is a
sentence that, said once in the wrong place, is very hard to unsay.

### 4.6 The environment is a provider list

> **Decision 11 — the Python environment is resolved by the same ordered
> provider vocabulary as a native dependency, and it is Tier B at best.**
>
> | Provider | What it is | Tier |
> |---|---|---|
> | `system` | the environment of the discovered interpreter: its `sys.prefix`, or `VIRTUAL_ENV` if set at *build* time and recorded | B |
> | `prebuilt` | an environment materialised into `SCIENCE_CACHE` by `sciencec fetch` from a locked requirement set | A for the tree's hash, B for what the wheels link |
> | `vendored` | an environment copied into the project tree by `sciencec vendor` | A for the tree's hash, B for what the wheels link |
>
> **`sciencec fetch` is the only command that may run `pip`**, which is
> `native-dependencies.md` Decision 10 applied without amendment: `sciencec
> build` makes no network request, ever, and a build that needs an environment
> which is not in the cache fails with `SC0472` printing the `fetch` command.

That last line is the part that makes this worth doing rather than merely
possible. The failure mode §6.2 was protecting against is a program that
discovers its Python problem on a compute node after a two-day queue wait.
Decision 10 of the sibling note moves every network-dependent failure onto the
login node by construction, and it does so for Python for free.

> **Decision 12 — the embedded interpreter is initialised isolated, by
> default.** `PyConfig.isolated = 1`, `PyConfig.use_environment = 0`, with
> `prefix`, `exec_prefix` and `module_search_paths` set from the resolved
> environment. `PYTHONPATH`, `PYTHONHOME` and an activated venv in the calling
> shell have **no effect** on what a Science binary imports.
> `--python-env=inherit` opts out, and the fact that it was used is recorded in
> the provenance section.

Without this, a Science binary's behaviour is a function of the shell that
launched it, which is a build input outside the lockfile and outside the
provenance record. With it, the environment is a resolved, recorded, Tier B
artefact, which is the strongest thing available. This is `native-dependencies.md`
§5.2's honest-guarantee sentence extended to the Python half, and it is stated in
the same deliberately weak words: **for the Python environment, Science
guarantees only that what was used is recorded.**

### 4.7 The second native dependency graph

> **Decision 13 — in an embedding build, `sciencec` walks the resolved
> environment's compiled extensions and records the native libraries they carry
> or require, in the provenance section, as Tier C. `sciencec doctor` reports
> any capability that appears twice.**

Mechanically this is `auditwheel` run in reverse and it is cheap: enumerate
`*.so` / `*.pyd` under the environment's `site-packages`, read `SONAME` and
`DT_NEEDED` (or the PE import table, or the Mach-O load commands), and match
against the capabilities the program itself declares. `numpy.libs/libscipy_openblas-*.so`
is a BLAS; so is `[native.openblas]`; the report says so.

This is the one place this note contributes something the parent design did not
have, and it is a direct answer to `native-dependencies.md` §11's last risk,
which that note names and explicitly does not claim:

> If a Science program links OpenBLAS and also loads a Python extension that
> bundled its own, there are two BLAS libraries and two thread pools in one
> address space, and neither this note nor `python-interop.md` currently owns
> that.

**This note owns the Python half of it.** Not solves — owns. Detection and
reporting is what is affordable; thread-pool coordination between two BLAS
implementations is a runtime problem that belongs to whoever owns the parallel
runtime, and §10 hands it there with the detection already built. What detection
buys is that the oversubscription becomes a line in `doctor`'s output and a field
in the provenance record instead of an unexplained factor of three that a
scientist attributes to the language.

**Cost.** A platform-specific binary traversal, in three flavours, that
`native-dependencies.md` §13 question 4 was still undecided about doing at all.
This note needs it for the embedding case, which is an argument for answering
that question yes.

### 4.8 The startup probe, which is where this note contradicts its own Decision 7

> **Decision 14 — in an embedding build, every module named in a `use python`
> item must be covered by a `[python.requires]` entry (`SC0459`), and the
> runtime imports each one and checks its version against the declared range
> before `main`.** A failure aborts with `SC0475`, naming the distribution, the
> version found, the range wanted, and the environment's path.

Decision 7 above makes `use python` lazy, to preserve the parent note's §3.3.
This decision makes it eager, in one mode, and the two must be reconciled rather
than left to collide.

They are reconciled by what the two modes are for. In hosted mode the Science
module is a guest in someone else's process; importing scikit-learn at init to
check its version would impose a two-second cost and a hard dependency on a user
who may never call the function that needs it, and §3.3's "imports nothing" is
the right rule. In standalone mode there is no host. The program *is* the
process, it will import scikit-learn within the second, and the only question is
whether a missing or wrong-versioned package is discovered now or three hours
into a job.

**Rejected alternative: lazy in both modes**, following
`ffi-c-boundary.md` §5.2's `when available` laziness and
`native-dependencies.md` Decision 16, which makes the GPU probe fire at first
table initialisation for exactly this reason. Rejected because the GPU case has a
property Python does not: a CPU-only run never touches the table, so laziness
avoids work that would otherwise be pure waste. A standalone program with
`use python "sklearn"` is going to import sklearn. Deferring the check defers
nothing except the bad news, and `native-dependencies.md` §6.1's argument — the
place you discover you forgot is inside a batch job — applies with full force.

**Cost, stated.** Importing scikit-learn is on the order of a second and
importing `torch` is several. A standalone Science binary with a broad
`[python.requires]` has a startup cost measured in seconds, which for a program
invoked in a loop by a shell script is intolerable. `--python-probe=lazy` exists
for that person, it is recorded in the provenance section, and it is not the
default — the same shape as `--no-native-probe` in the sibling note, for the same
reason.

### 4.9 What a standalone binary honestly promises

> **Decision 15 — the provenance record gains a `self_contained` boolean, and an
> embedding build sets it false.**

`native-dependencies.md` Decision 9 already specifies a record that is readable
without running the binary, which matters here more than anywhere: a reviewer
asking "does this need Python?" can answer it with `readelf` on a laptop rather
than by reproducing a cluster. The field is one byte and it turns the headline
promise in core spec §3 from a claim about the language into a fact about an
artefact, which is what Decision 12 made it. Programs that embed nothing are
unaffected and say `true`.

The record's Python section carries: the interpreter's path, `SONAME`, version
string and variant; the environment's provider, tier and, for Tier A, the tree
hash; every `[python.requires]` entry with the version actually found; every
native library found under `site-packages` (§4.7); and whether `isolated` and the
startup probe were on.

---

## 5. Types, revisited

### 5.1 Testing the first reason

`python-interop.md` §6.4 refuses `.pyi` consumption in two sentences, and the
first reason is that it is *"a second type system, one that does not agree with
ours"*.

**That is true, and it is more true than the sentence claims.** The
disagreements, concretely:

- **`Any` is not a type.** Gradual typing's relation is *consistency*, and
  consistency is deliberately **not transitive**: `int` is consistent with `Any`
  and `Any` is consistent with `str`, and `int` is not consistent with `str`.
  Science's subtyping is transitive because every type system that supports
  inference is. There is no sound embedding of one into the other; there is only
  a conversion that is wrong at the edges, and `Any` is not an edge case in
  typeshed, it is everywhere.
- **`Union[A, B]` is untagged.** Science's `choice` is tagged, which is what
  makes `match` exhaustive — core spec §1's reason for having it. A `str | bytes`
  return has no Science type; the honest translation is a `choice` with
  constructors that do not exist at run time.
- **`Protocol` is structural; `interface` is nominal** (`syntax-revision-2.md`
  §6). A Python function declared over `SupportsFloat` accepts anything with
  `__float__`; a Science interface accepts what declared `implements`.
- **`@overload`, `TypeVar` variance, `ParamSpec`, `Literal`, `TypedDict`,
  `Self`, `Concatenate`, `TypeGuard`.** Science has none of these and should not
  acquire them to read a stub file.

And one that is specific to this language and is decisive:

- **The stubs are blank exactly where Science needs them and detailed exactly
  where it does not.** `numpy.typing.NDArray[np.float32]` carries a dtype and
  **no shape**. Static shapes are the whole payoff of `python-interop.md` §3.4 —
  "one check at the door, zero checks inside the loop" — and the one thing a
  scientific program most wants checked is the one thing typeshed cannot say. The
  parts of typeshed that are rich are `str`, `dict`, `pathlib` and `datetime`,
  which are precisely the parts where Science's own types are already fine.

So reason one holds, with a better argument than the one given.

### 5.2 Testing the second reason

The second reason is that consuming stubs *"would build the verification story on
a foundation Python itself does not enforce"*.

**True about soundness, and it overshoots.** As written it would equally forbid
`.pyi` files from being useful to Python programmers, and they demonstrably are:
typeshed now covers most of the standard library, `pandas-stubs` and
`scipy-stubs` exist, and scikit-learn and astropy ship inline annotations under
`py.typed`. Millions of real bugs are caught every day by a mechanism with no
runtime enforcement whatever.

The correct, narrower statement is:

> A stub can support a **diagnostic** claim and cannot support a **soundness**
> claim. A diagnostic that is sometimes wrong is worth more than no diagnostic;
> a guarantee that is sometimes wrong is worth less than no guarantee.

Science already has a category for exactly this and has already shipped the
vocabulary for it. `native-dependencies.md` §4.3 permits `probe = "none"` and
labels it in four words — *"honest, not safe"* — and §5.1's Tier B records what
was found while saying in terms that *"every field there is true and none of it
is a constraint."* A project that knows how to ship an unenforceable record and
label it can ship this one.

### 5.3 What the refusal costs

§6.4 did not price its refusal, and with §2's coverage number established the
price is now visible. `python-interop.md` §6.1 already names it:

> **Everything is dynamic.** A `PyObject` is `any`. […] Every Python call is a
> hole in the verification story.

If hosted coverage is 92% of operations, and the reason to have this feature at
all is that people call a lot of Python, then the fraction of a Science program
that is untyped is proportional to how successful this feature is. The better
§2 works, the worse §6.1 gets. That is an argument for reconsidering, and it is
not an argument for reconsidering *soundness*.

There is also a plain usability cost that has nothing to do with types: in an
editor, `numpy.` offers nothing. No completion, no signature, no docstring. For a
dynamic FFI that is the single most-reported complaint, ahead of error handling,
and it is entirely a tooling problem rather than a type-system one.

### 5.4 The decision

> **Decision 16 — stubs come back, for three things, and never as a type.**
>
> `sciencec` and the language server may read `.pyi` files and inline
> annotations for:
>
> 1. **completion and hover** in the editor;
> 2. **documentation** rendered next to a `use python` item;
> 3. **one warning-level diagnostic**, covering the three checks that are
>    decidable from a stub without a type system — *this module or object has no
>    such attribute*, *this call has no such keyword parameter*, and *this arity
>    is impossible*.
>
> And for nothing else. In particular: no inference, no narrowing, no
> unification, no effect on codegen, no effect on overload selection, and no
> effect on what compiles.

> **Decision 17 — the invariant. No stub may ever change whether a program
> compiles.** Adding a stub, removing a stub, or replacing a stub with a wrong
> one changes the set of warnings and nothing else.

Decision 17 is the whole safety of Decision 16 and it is one sentence, which is
why it is a separate decision rather than a clause. It has three consequences
that are worth spelling out because each one closes a door that would otherwise
be pushed on:

- **Stubs are not a build input**, so they are not in the lockfile, so a stale
  stub cannot make a reproducible build irreproducible. `package-manager.md` §2
  is untouched.
- **A wrong stub cannot stop a right program.** `getattr`, `__getattr__`, plugin
  registries, pandas accessors, astropy's lazily-loaded submodules and every
  monkeypatch in the ecosystem produce attributes no stub knows about. All of
  them warn and all of them run. `#[allow]`-style suppression per call site and a
  manifest switch per package.
- **There is a wall against scope creep.** The first request after this ships
  will be "just let `Optional[T]` narrow to `T?`, it agrees exactly". It does
  agree exactly, and it is the first step to a second type system, and Decision
  17 is the sentence that refuses it without having to re-argue §5.1 each time.

The three checks in Decision 16 are chosen because they are **name-level, not
type-level**. `PCA(n_componets=8)` — a transposition — is the mistake people
actually make, it is decidable from a parameter list without knowing a single
type, and today it becomes a `TypeError` at run time, possibly after an hour of
data loading. Catching that at build time is most of the practical value of
static types here, at none of the cost.

**Configuration**, in the manifest rather than as a flag, so it is archived:

```toml
[python.stubs]
source = "environment"      # inline py.typed and bundled .pyi from the resolved env
extra  = ["stubs/"]         # a local directory, for a package with none
```

**Rejected alternative: full `.pyi` consumption with `Any` as a Science top
type.** Rejected on §5.1 — consistency is not transitive, and a type system with
a non-transitive relation bolted onto a transitive one does not have a
well-defined subtyping judgement.

**Rejected alternative: stubs as types, with every result widened to `PyObject`
and the stub used only to check arguments.** Genuinely tempting, since it is
unidirectional and would catch real errors at *error* level. Rejected because
argument checking needs the argument's Science type to relate to a Python type,
which is the embedding §5.1 says does not exist; and because it makes stubs a
build input, which violates Decision 17 and drags them into the lockfile.

**Rejected alternative: generate Science declarations from stubs, ahead of
time**, the way `ffi-c-boundary.md` §7 generates from C headers. Rejected because
a C header is *the ABI* — it is normative, the compiler enforced it, and the
linker will catch a lie. A `.pyi` file is a comment. Generating from the first is
transcription; generating from the second is fabrication.

**Cost, stated.** A parser for a subset of PEP 484 syntax — names, parameter
lists, defaults, `*`/`**` markers — which is a few hundred lines and must ignore
the type expressions rather than understand them. A stub cache keyed on the
resolved environment. And a documentation burden: users will believe this is type
checking, and the documentation must use `native-dependencies.md`'s own four
words, **honest, not safe.**

---

## 6. The honest ceiling

Three denominators, because a single number here would be a lie whichever one it
was.

| Measure | §6.3 as written | With this note |
|---|---|---|
| scikit-learn's named public API reachable | ~99% | ~99% |
| Elementary operations in §2.2's two programs | 12 / 26 = **46%** | 24 / 26 = **92%** |
| Real programs that run unmodified | ≈ **0%** | ≈ **80%** |

**Hosted mode: ~80% of programs.** The 20% is one pattern — a Python class
*defined* in Science, introspected or serialised by Python. Custom scikit-learn
estimators under `clone`, `torch.nn.Module` subclasses, astropy
`Fittable1DModel`s, anything pickled across `multiprocessing` with the spawn
start method. `__text_signature__` recovers part of it; nothing recovers all of
it short of generating a Python source shim, which is a decision this note
declines (§2.3, Decision 6).

**Standalone mode: the same ~80%, multiplied by whether the machine has the
interpreter and the distributions.** The language-level coverage is *identical* —
same protocol, same region, same conversions, same everything. What differs is
entirely deployment, and the honest form of the number is a product:

> ceiling(standalone) = 0.80 × P(the declared interpreter and the declared
> distributions resolve on this machine)

On a laptop with a venv, the second factor is 1. On a login node that ran
`sciencec fetch`, it is 1. On a compute node with a `module load python`, it is
1. On a stranger's machine with nothing, it is 0 — and this note's whole
contribution to the standalone case is that the 0 is **discovered at build time
where possible, announced before `main` where not, and written in the binary
where a reviewer can read it without running it.**

**Against the stated goal — "all Python code usable from Science" — the answer
is no**, and the gap is not where anyone expects. It is not types: there were
none to lose, which is §1's whole point. It is not the ABI, and it is not the
packages. It is:

1. **Python code that defines a class Python then introspects** — because
   introspection reads Python-level structure that a Science-generated type does
   not have; and
2. **Python code that expects to be the process** — `multiprocessing` forking a
   Science host, `sys.exit`, signal handling, anything that assumes it owns
   `__main__`.

**Science can call all of Python. It cannot be Python.** Those two clauses are
the ceiling, and both of them are properties of Python's reflective and process
models rather than of anything Science chose.

---

## 7. Diagnostics

The README allocates `SC0180`–`SC0189` (syntax) and `SC0459` (the last free code
in `python-interop.md`'s codegen block, `SC0458` having gone to
`script-mode.md`).

| Code | Meaning |
|---|---|
| `SC0180` | A `python:` region contains a statement it may not — `return`, `break`, `loop:`, or a nested `def`. Help: close the region first. |
| `SC0181` | A `python:` region header is malformed: no error binding named. |
| `SC0182` | An argument label admitted by the label rule (§2.3, Decision 8) is not a valid Python identifier. |
| `SC0183` | A `python:` region nested inside another `python:` region. |
| `SC0184`–`SC0189` | Reserved. |
| `SC0459` | A `use python` module in an embedding build is not covered by any `[python.requires]` entry (§4.8). Prints the manifest line to add. |

**Redefined, not claimed:**

- **`SC0455`** — was *"`use python` in a standalone (non-hosted) build"*. Is now
  *"`use python` in a standalone build with no `[python]` table"*. It remains a
  mode-dependent diagnostic, which `script-mode.md` §5.1 uses as its example of
  the category and which §11 of that note asks be kept to a small number; the
  count does not change.

**Amended, not claimed:**

- **`SC0475`** (`native-dependencies.md`, the startup probe) gains the
  interpreter probe and the `[python.requires]` version probes.
- **`SC0477`** (no provider succeeded) covers "no CPython of the declared
  version and variant was found", with the provider list and the reason each
  failed.
- **`SC0472`** (artefact not in the cache, build makes no network request)
  covers a `prebuilt` or `vendored` Python environment, printing the
  `sciencec fetch` command.
- **`SC0454`** (Python-effecting code in a parallel region) applies to a
  `python:` region as a unit.
- **`SC0457`** (`Array of T` with numeric `T` at the boundary, warning) applies
  to Decision 4's outbound conversions.
- The existing type error for a missing operator implementation gains a help
  line naming the `python:` region, rather than this note claiming a code for it
  (§2.3, Decision 2).

---

## 8. Two things the parent note's §6.1 got right, one it got wrong, one to add

§6.1 lists four costs of the reverse direction. Checked:

- *"A Science binary that embeds Python is not self-contained."* **Right, and
  more right than when written** (§4.2). Kept, and made a recorded property of
  the artefact rather than a prohibition.
- *"Everything is dynamic. […] Every Python call is a hole in the verification
  story."* **Right.** §5 does not close it; it adds a lint beside it and is
  explicit that a lint is not a closure.
- *"Errors become dynamic too — a `PyError` carrying a type name and a message,
  not a `choice` the compiler can check a `match` against."* **Right**, and
  slightly better under revision 2 than under `Result`: `PyError implements
  Error` (§3.1), so a Python failure composes with every other Science failure
  with no conversion, which is more than `Result` offered without `From`
  widening.
- *"The GIL is held for the whole time. […] One Python call in a hot loop
  serializes the program."* **No longer unconditionally true.** On a
  free-threaded build (3.13+, PEP 703) there is no GIL to hold, and the parent
  note's own §5.5 enumerates what that changes. The cost is now
  version-dependent, and the sentence should say so rather than stating a
  universal. It remains true on every GIL build, which in 2026 is still most of
  them.

**One cost §6.1 does not name, and should**: embedding or importing drags a
**second native dependency graph** into the process, which Science's resolver
cannot see and cannot deduplicate (§4.4, §4.7). It is the most likely source of
an unexplained performance regression in a program that is otherwise correct.

---

## 9. What this does to `python-interop.md`

Named section by section, per the README's convention.

| Section | What changes | Why |
|---|---|---|
| **§3.3** | **Narrowed, and preserved.** "The module imports nothing at init" is confirmed to survive `use python`, because Decision 7 makes the import lazy. Worth a sentence in that section, because the reader who meets §6 will assume it broke. | A `use python` importing at `Py_mod_exec` would make a Science module hard-depend on scikit-learn and could fail at `import`. |
| **§3.4, §3.5** | **Translated.** The `Result of (T, E)` row becomes a trailing `Error?` in a multiple return: return `T` when the error is null, raise when it is not. `Option of T` becomes `T?`. `SC0451`'s subject (`Option of (Option of T)`) becomes `T??`, which revision 2 may not admit at all — if it does not, that code may be retired. `SC0453` (`Result` in argument position) becomes: a parameter of type `Error?` is rejected. | Revision 2 removed `Result` and `try`. The mapping is mechanical; the codes and reasons survive. |
| **§5.4** | **Extended.** The callable wrapper of Decision 5 is the concrete design that section's rule was written against. No rule changes. | The section states the GIL consequence of a Python callable and leaves the mechanism unspecified. |
| **§6.1** | **Extended and one item narrowed.** See §8 above. | Free-threaded builds; the second native graph. |
| **§6.2** | **Overturned in part.** Standalone is permitted behind `[python] embed = true`; `SC0455` is redefined from a prohibition to a missing declaration. The *argument* of §6.2 is upheld and restated (§4.2): the binary is genuinely not self-contained, and it now says so in a field a tool can read. | Three of the four stated reasons are paid for by `native-dependencies.md`'s provider machinery. The fourth — the environment — is not solved and is handled by declaration and report rather than by prohibition. |
| **§6.3** | **Extended substantially, and rewritten into revision 2.** This discharges `syntax-revision-2.md` §9 ask 4. Adds iteration, subscripting, operators, context managers, container conversion, callbacks, subclassing, lazy import, the label rule, and the `python:` region. | §6.3 reaches 46% of elementary operations and approximately 0% of real programs (§2.2). |
| **§6.4** | **Partially overturned.** Stubs return for completion, documentation and one warning-level name check, under Decision 17's invariant that no stub may change whether a program compiles. Both of §6.4's reasons are upheld *as reasons against stubs-as-types* and neither survives as a reason against stubs-as-documentation. | §5. |
| **§7.3, §7.4** | **Restated, unchanged in substance.** `PyError` gains `implements Error`. The fatal-exception check on the way out (`KeyboardInterrupt`, `SystemExit`, `MemoryError`) must additionally cover a region that short-circuited and whose error was handled without re-raising — the same belt-and-braces check, one more path. | Revision 2's error model; the region is a new way to drop an error. |
| **§9** | Table gains `SC0459`; `SC0455`'s text is replaced. | §7 here. |
| **§10** | **"Standalone embedding of Python"** and **"Typed Python stubs consumed by Science"** both leave the deferred list. The first becomes Decisions 10–15; the second becomes Decision 16, in a strictly smaller form than the deferred item described. | §4, §5. |
| **§12** | Gains the risks in §11 here. | |

And one knock-on outside the parent note. **`syntax-revision-2.md` §8.3 keeps
its ruling and loses one of its reasons.** That section rejects an inline
`python { … }` block by citing §6.2's standalone prohibition and quoting the
deployment-story sentence. The prohibition is now a declaration requirement, so
that leg of the argument is gone. §8.4's argument — that foreign source inside a
`.science` file is invisible to every tool in both ecosystems, and that a foreign
syntax error surfaces as a Science diagnostic Science did not produce — is
untouched and is sufficient on its own. §8.3 should be re-grounded on §8.4. The
`python:` region introduced here is not an inline block and does not bear on it
(§3.3).

---

## 10. What this note asks of others

**Of `python-interop.md`:** the twelve rows of §9.

**Of `native-dependencies.md`:**

1. A **`self_contained` boolean** in the provenance record (Decision 15). One
   byte, and it turns core spec §3's promise into a checkable property of an
   artefact, which is what that note's own Decision 12 made it.
2. `capability = "cpython"` in the variant vocabulary, with variants for the
   version family and for the GIL / free-threaded split — which is an ABI variant
   in exactly that note's sense, and a pointed example for its §13 question 2 on
   who arbitrates the vocabulary.
3. Acceptance that the **provider vocabulary extends to a Python environment**
   (Decision 11), at Tier B for `system` and Tier A-for-the-tree-only for
   `prebuilt` and `vendored`. This is the least comfortable ask in the note and
   it should be argued with.
4. Its **§13 question 4** — should the provenance record walk the transitive
   native closure — answered **yes**, because Decision 13 needs the traversal
   anyway and the embedding case is where the closure matters most.
5. Its **§11 last risk** and **§13 question 6** (two providers of one capability
   in one process; who owns thread-pool coordination): this note claims the
   *detection and reporting* of the Python half and hands the coordination on,
   with the detection built.

**Of `package-manager.md`:** a `[python]` table and a `[python.requires]` table
in `science.toml`, root-package-only, on the same footing as `[native.<name>]`
and for the same reason its §4.3 gives for refusing a dependency-set
`[profile]`; a `[python.stubs]` table that is explicitly **not** a lock input
(Decision 17); a Python section in the lockfile carrying the environment's tier;
and confirmation that `sciencec fetch` is the only command that may run `pip`,
which is its §6.2 and `native-dependencies.md` Decision 10 with no amendment.

**Of `reserved-words.md`:** the **label rule** — any word immediately followed
by `:` in argument position is an argument label, reserved or not. It is the dot
rule's sibling, it is the same five lines of parser, and it removes a whole class
of collision that is otherwise unreachable rather than merely awkward: `shape:`,
`match:` and `type:` are all common Python parameter names and all Science
keywords. It should be adopted for every call, not only Python ones.

**Of `collections-and-chains.md`:** the shape of `Iterate` when `next()` can
fail (Decision 1). Python is the first implementor that needs it; there will be
others (a `Rows` reader over a file, per `data-io.md`), so it should be that
note's shape and not one invented here.

**Of `indexing-and-array-literals.md`:** confirmation that `a[i]` and `a[i] be v`
lower to something a non-builtin type can implement, which Decision 2 assumes.

**Of whoever owns the error model** (`syntax-revision-2.md` §3, or its
successor): the question in §3.5 — whether the `python:` region should be
generalised to a `check err:` region over any fallible call. This note does not
answer it, ships the Python-only form, and observes that the two are
source-compatible in the direction that matters.

**Of `models-and-inference.md`:** that note loads trained models, and the
practical route to `transformers`, `safetensors` tooling and most tokenizer
implementations is through Python. Whether it depends on this feature — and
therefore on a program not being self-contained — is a decision it should make
explicitly rather than inherit.

**Of `README.md`:** rows for this note in the index and in the allocation table,
claiming `SC0180`–`SC0189` and `SC0459`, and a note that `SC0455` is redefined
here rather than reallocated. The convention is that a note adds its row before
writing; this one could not, because its brief forbade editing any file but its
own.

---

## 11. Risks

**The region is a partial retreat from `syntax-revision-2.md` §3.2**, and if the
general `check err:` form is later adopted, the retreat becomes language-wide.
That is the correct place for the argument to happen and this note should not be
read as having pre-decided it. If the general form is *rejected*, Science has one
construct with a hidden failure edge and it is the Python one, which is a
slightly awkward asymmetry to defend.

**Reopening standalone will be read as "Science needs Python."** One sentence in
one conference talk and the perception is permanent, and it is the exact
perception the language exists to escape. `self_contained = false` in the
provenance record is a mitigation for a reviewer and no mitigation at all for a
rumour. The documentation's framing matters more than usual here, and the default
— no `[python]` table, `SC0455`, a binary that embeds nothing — has to stay the
default forever.

**The second native dependency graph is detected, not fixed.** Decision 13
reports that there are two BLAS libraries in the process. Nothing stops them
oversubscribing the node, and a scientist who reads `doctor`'s output and does
not know what to do with it is only marginally better off. The value is real but
it is diagnostic value.

**The subclassing hole is invisible until it is not.** `python.subclass` works,
the class instantiates, methods dispatch, and then `GridSearchCV` calls `clone`
and the program fails three frames inside scikit-learn with a message about a
builtin. That is the worst shape a limitation can have: it passes the first test
and fails the second, in someone else's code, with someone else's error message.
`__text_signature__` covers the common case and the failure of the uncommon case
is not detectable in advance.

**The `Drop`-based context manager silently mis-handles suppression.**
`contextlib.suppress` and `pytest.raises` are the two that will be met. There is
no diagnostic available, because whether `__exit__` suppresses is a run-time
property of an arbitrary Python object.

**Stubs will be mistaken for type checking.** Decision 17 is the wall and it is
one sentence in a file nobody reads. The first feature request will be
`Optional[T]` narrowing and it will be reasonable, and the second will be
overload selection and it will not be. The wall holds only if it is restated in
the documentation and in the diagnostics themselves.

**The coverage numbers come from two programs, not a corpus.** 46% and 92% are
counts over twenty-six operations chosen to be representative by one author. The
API-level figure (~99%) is robust because it follows from the protocol being
total; the operation-level and program-level figures are estimates with the shape
of the work behind them, in the same sense as the parent note's §4.2 and §5.3
performance figures, and they should be replaced by counts over a real corpus
before anyone quotes them. **The direction of the error is knowable even if the
magnitude is not: 46% is an overestimate of §6.3's program-level usefulness, and
80% is an optimistic reading of the subclassing pattern's frequency in
machine-learning code.**

**`n_jobs=-1` inside a hosted extension module.** joblib defaults to
process-based parallelism, which on Linux means `fork` in a process that has a
Science runtime, possibly a thread pool, and the GIL. Forking a threaded process
is a known hazard independent of Science, and a Science host makes it more likely
rather than less. Nothing in this note addresses it and something should.

---

## 12. Summary of decisions

| # | Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|---|
| 1 | `PyObject` implements `Iterate` | An explicit hand-stepped `loop:` | `KFold.split` returns a generator; a large part of scikit-learn is unusable without iteration | `Iterate` must model a failing `next()` — an ask on a sibling |
| 2 | Subscripting and operators on `PyObject`, inside a region only | Panic on error; a deferred errno-style error slot | An operator cannot be fallible under revision 2; a `KeyError` is recoverable and a panic is not | Thirteen of twenty-six operations depend on a construct existing |
| 3 | A context manager is a value whose `Drop` calls `__exit__` | A `with` statement | Science has deterministic destruction, which is why Python needed `with` in the first place | Exception-suppressing managers are silently wrong |
| 4 | Containers convert by copying, both ways | Nothing; or a shared representation | There is no shared representation; hyperparameter grids must cross | `SC0457`'s warning applies; nothing numeric may travel as a `list` |
| 5 | A Science closure crosses as a callable | Nothing | `scoring=`, `key=`, `callback=`, SciPy's `minimize(fun=…)` | The closure must be owned; the GIL is held (already §5.4's rule) |
| 6 | `python.subclass` with `__text_signature__`; callable, not introspectable | A generated Python source shim | The shim fixes introspection completely and reintroduces a `.py` file into the build | `clone` and `GridSearchCV` over a Science-defined estimator still fail |
| 7 | `use python` imports lazily, at first use | Import at `Py_mod_exec` | `python-interop.md` §3.3's best decision; an extension module must not fail at `import` | Standalone needs the opposite — Decision 14 |
| 8 | The label rule: a word before `:` in argument position is a label | A `kwargs:` map for every collision | `shape:`, `match:`, `type:` are common Python parameters and Science keywords; unreachable otherwise | Five lines of parser, and a sibling note must adopt it |
| 9 | The `python err:` region | Accept the verbosity; a Python-only `try`; panic | 3.6× expansion per *sub-expression*, not per call; and operators cannot exist without it | A partial retreat from `syntax-revision-2.md` §3.2; one more construct |
| 10 | Standalone embedding permitted behind `[python] embed = true`; `SC0455` redefined | Keep the prohibition; permit with no opt-in; a build flag | Three of §6.2's four reasons are paid for by a sibling; the fourth is handled by declaration | Science now has programs that are not self-contained |
| 11 | The Python environment is a provider list, Tier B at best | Treat a venv as a pinned artefact | A venv is a state discovered by `import` at run time, not a file with a link name | The lockfile admits it is not a lock, again |
| 12 | The embedded interpreter is initialised isolated by default | Inherit the ambient environment | A build input read from the shell at run time is outside the lockfile and the provenance record | `--python-env=inherit` exists and is recorded |
| 13 | `sciencec` walks `site-packages` for bundled native libraries and reports duplicates | Ignore them | `native-dependencies.md` §11 names this risk and claims no owner; embedding makes it common | A platform-specific traversal in three flavours |
| 14 | In an embedding build, imports and version probes run before `main` | Lazy, as in hosted mode and as for GPU tables | The place you discover a missing package must not be a queued batch job | Seconds of startup; `--python-probe=lazy` exists and is recorded |
| 15 | The provenance record gains `self_contained` | Say nothing | Decision 12 made self-containment a property of a program; a property should be readable | One byte |
| 16 | Stubs return for completion, documentation, and one warning-level name check | Full `.pyi` consumption; stubs for arguments only; generating declarations from stubs | Consistency is not transitive; a C header is normative and a `.pyi` is a comment; but a wrong diagnostic beats no diagnostic | A PEP 484 subset parser; users will believe it is type checking |
| 17 | **No stub may ever change whether a program compiles** | A little narrowing, just for `Optional` | It is the only wall between Decision 16 and a second type system | Some genuinely useful checks are refused on principle |
