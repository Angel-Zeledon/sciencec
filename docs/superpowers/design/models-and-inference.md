# Science — Design: models and inference

Date: 2026-09-16
Status: draft for review
Area: working with trained models — runtime bindings, the type of a loaded model,
the inference call, device memory, weight formats, tokenizers, and the training
question.
Depends on: the F0 core design (`docs/superpowers/specs/2026-09-16-science-f0-core-design.md`),
F1 tensors and shapes, F2 GPU targets.

---

## 0. The claim this note is built on

Science's first job is not to train models. It is to **load a model somebody else
trained and run it**, and to do that better than the alternatives.

"Better than the alternatives" is a specific, testable claim, and the alternatives
are specific too:

| Alternative | What it gets right | What it gets wrong |
|---|---|---|
| Python + `onnxruntime` | Everything is one `pip install` away | Ships an interpreter and a virtualenv to production; every shape and name error is found at request time, in production, by a customer |
| C++ + ONNX Runtime | Fast, small, no runtime | Two days to get a build working; preprocessing written by hand each time; nobody on the team wants to maintain it |
| Rust + `ort` | Fast, safe, small | Tensor names and shapes are strings and `Vec<i64>`; the borrow checker helps with memory and not at all with the model |
| Go + `onnxruntime_go` | Deploys beautifully | Same: the model's interface is stringly typed |

The gap every one of them leaves is the same one: **the model's interface is
checked late or not at all.** Python finds a wrong tensor name at request time.
Rust finds it at request time. C++ finds it at request time and then segfaults.
The thing Science can do that none of them do is *move that check earlier* —
mostly to load time, and for the parts that can be pinned down, to compile time.

Everything below follows from that. Where a design choice moves a class of error
from run time to load time or compile time, take it. Where it does not, prefer
the boring option that gets the vendor library working.

The second claim, which is less flattering and needs stating once: **the quality
of this area is roughly the quality of Science's C FFI.** None of what follows
exists without `extern`, and `extern` is a reserved word in F0, not a feature.
See §9.

### 0.1 Syntax this note assumes but F0 does not yet specify

Three small things. Each is flagged here rather than smuggled into an example.

1. **Let-ascription**: `let net: Session of (Cuda, In, Out), err be onnx.open(...)`.
   §5.2 says signatures are annotated and bodies inferred; a loaded model is the
   one case where the annotation *is* the specification, so the binding must be
   able to carry a type. Without it, every load needs generic arguments spelled
   out at the call site and the examples become unreadable.
2. **Array literals**: `["cat", "dog"]`. Assumed throughout.
3. **Generic parameter defaults**: `Tensor of (F32, DIMS, D = Cpu)`, so that host
   tensors are written with two arguments and device tensors with three. Without
   defaults, every host tensor in every scientific program carries a redundant
   `Cpu`, and that is a tax on the 95% case to serve the 5%. If the core team
   rejects defaults, the fallback is a `HostTensor` / `DeviceTensor` split, which
   doubles the surface of every generic function over tensors. Defaults are worth
   the small complication in the resolver.

### 0.2 The reserved-word collision, resolved

`model` and `tensor` are reserved globally (§13). That is a real cost in this
area and it is worth saying exactly where it lands.

- `let model be onnx.open("resnet.onnx")` does not compile. This is the single
  most likely first line a new user writes in this area, so the diagnostic must
  be excellent: `SC0101: 'model' is a reserved word`, with the note *"reserved
  for the `model` construct in a later version"* and a suggestion of `net`,
  `classifier`, or `predictor` — machine-applicable, so the fix is one keystroke
  in an editor.
- Case is significant, and the F0 spec already relies on it: `tensor` is reserved
  while `Tensor` is the type name in §2. This note therefore **does not name any
  type `Model`**, even though it would be legal. A language whose whole premise
  is readability should not contain a keyword `model` and a type `Model`
  distinguished only by a capital letter. The nouns used here are:

| Noun | Meaning |
|---|---|
| `Session` | A loaded, device-placed, executable thing. Owns the runtime handle, the weights and the arena. |
| `Signature` | The names, dtypes and shapes of a session's inputs and outputs, as read from the file. |
| `Bindable` | A record type whose fields correspond to named tensors. |
| `Weights` | A name-to-tensor map read from a weight file, with no runtime attached. |

`Session` is also the honest word: ONNX Runtime calls it a session, and what you
hold is not an abstract model but one instantiation of it on one device with one
set of allocators.

---

## 1. Which runtimes to bind, and in what order

Ranked by **audience unblocked per unit of work**. The unit of work is
engineer-weeks to a binding that is correct, tested, and installable by someone
who is not us.

### Rank 1 — ONNX Runtime. First, and not close.

**Audience unblocked:** scikit-learn, XGBoost, LightGBM, CatBoost (via
`onnxmltools` / `skl2onnx`), most PyTorch vision and embedding models (via
`torch.onnx.export`), TensorFlow and Keras (via `tf2onnx`), and essentially
everything on Hugging Face that `optimum` can export. In one binding: classical
tabular ML, image classification and detection, embedding models, speech models,
and small-to-mid transformers.

**Cost:** low, and lower than it looks. ONNX Runtime ships a **stable C API**
(`onnxruntime_c_api.h`) reached through a single `OrtApiBase` struct of function
pointers, versioned explicitly. That struct-of-pointers shape is a gift to a new
language: you bind one symbol, `OrtGetApiBase`, and reach everything else through
fields. Roughly 40 functions cover the whole of load, bind, run, and introspect.
Estimate: 3–4 engineer-weeks to a good binding, plus 2 for signature
introspection and error mapping.

**It also brings hardware for free.** Execution providers (CUDA, TensorRT,
CoreML, DirectML, ROCm, OpenVINO, XNNPACK) are selected at session-creation time
through the same C API. Binding ONNX Runtime binds five accelerators.

**What it costs to be honest about:** the ONNX ecosystem has export gaps. Dynamic
control flow, custom ops, and anything written the week before last does not
export. When a user's model does not export, Science's answer is "it does not
export", and that is not an answer they will like.

### Rank 2 — llama.cpp / GGUF. Second, because of what it unblocks, not how clean it is.

**Audience unblocked:** local LLM inference — the highest-demand single use of a
trained model today, by a wide margin, and the one people will actually try
Science for. A GGUF file plus llama.cpp gives you quantized weights, a working
tokenizer (the vocab is embedded in the file), a KV cache, and Metal/CUDA/Vulkan
backends, all in one dependency.

**Cost:** medium-high, and mostly not technical. `llama.h` is a clean C header,
but it **churns**: functions are renamed and deprecated across releases on a
timescale of weeks. Binding it means pinning an exact upstream commit, vendoring
or documenting it precisely, and budgeting recurring maintenance forever. The
build is also large and wants CMake and a GPU toolchain.

**Decision:** bind it, pin the version in the manifest, and treat version bumps
as a scheduled chore rather than a surprise. Expose a narrow surface — load,
tokenize, evaluate, sample, KV-cache reuse — and refuse to chase llama.cpp's full
API. A narrow surface is what survives churn.

### Rank 3 — safetensors. Third by audience, but do it first because it is nearly free.

**Cost:** very low. The format is a `u64` little-endian header length, a JSON
header mapping tensor name to `{dtype, shape, data_offsets}`, and a data blob.
Reading it natively is a few hundred lines plus a dtype table; see §5.

**Audience unblocked:** anyone who wants to inspect, convert, slice, or diff
weights without loading a runtime — and, later, anyone writing model code in
Science itself. It also unblocks Science's own tests: a test fixture that is a
`.safetensors` file needs no C dependency and works on every platform on day one.

Do it first because it is the only item on this list that has no external
dependency at all, which makes it the only one that can land before the FFI story
is finished.

### Rank 4 — LibTorch. Fourth, and deferrable.

**Audience unblocked:** TorchScript modules people did not export to ONNX, and —
in principle — training and autograd.

**Cost:** high. There is no stable C ABI. The public interface is C++ with
templates and `at::Tensor` by value; every language that binds it (`tch-rs`,
the Java bindings, and the rest) does so through a **generated C shim** driven by
PyTorch's `Declarations.yaml`, which means the binding is a code generator that
must be re-run and re-validated per PyTorch release, over a surface of roughly
two thousand operators. The distribution is multi-gigabyte and version-matched to
a CUDA toolkit.

**Decision:** defer past F2. Most of its audience is reachable through ONNX
export at a fraction of the cost. Revisit only if a concrete user says "I have a
TorchScript module I cannot export", more than once.

### Rank 5 — TensorRT. Do not bind directly. Reach it through ONNX Runtime.

**Audience unblocked by a direct binding, over and above the ONNX Runtime
TensorRT execution provider:** a latency improvement on NVIDIA hardware for users
who are already tuning, plus explicit control of engine building, calibration and
plan caching.

**Cost:** high and permanent. The API is C++ with virtual interface classes and a
logger callback, so the shim is nontrivial; engines are **not portable** across
GPU architectures or TensorRT versions, so the binding owns a build-and-cache
lifecycle with its own failure modes; and NVIDIA revises the API across major
versions.

**Decision:** leave out. Get TensorRT through ONNX Runtime's execution provider,
which handles engine caching already. If profiling later shows the EP leaves real
latency on the table for a user who cares, revisit then, with a measurement.

### Ranking, condensed

| Rank | Runtime | Effort | Unblocks | Phase |
|---|---|---|---|---|
| 1 | ONNX Runtime (C API) | ~5 weeks | classical ML, vision, embeddings, speech, small transformers, 5 accelerators | F1 |
| 2 | safetensors (native) | ~1 week | weight inspection and conversion, test fixtures, Science-native model code | F1, land first |
| 3 | llama.cpp / GGUF | ~6 weeks + upkeep | local LLMs, the demo people want | F1.5 |
| 4 | LibTorch | ~12 weeks + upkeep | TorchScript, autograd | not before F3 |
| 5 | TensorRT (direct) | ~8 weeks + upkeep | NVIDIA latency tuning | not planned |

### What is deliberately left out, and why

- **TensorFlow / TFLite / SavedModel.** The C API exists and TFLite's is small.
  Left out because `tf2onnx` reaches the same models through rank 1, and because
  the TensorFlow audience is shrinking rather than growing. Reconsider TFLite
  specifically if an embedded user appears.
- **JAX / XLA via PJRT.** Intellectually the closest neighbour to Science, and
  the PJRT C API is designed for exactly this. Left out because the audience that
  would use it is small and already happy, and because a PJRT binding is a
  compiler-integration project, not a library project.
- **OpenVINO, DirectML, CoreML, ROCm, MLX.** All reachable as ONNX Runtime
  execution providers, except MLX. MLX left out; Apple-silicon users are served
  by ONNX Runtime's CoreML provider and by llama.cpp's Metal backend.
- **TVM, Triton (the compiler), IREE.** These are compiler stacks. Binding one is
  a strategic commitment, not a library.
- **Triton Inference Server, TorchServe, vLLM, Ollama, OpenAI-compatible HTTP
  endpoints.** These are not runtimes to bind; they are *servers to call*, and
  calling them is an HTTP client and a JSON codec. That is worth having and it
  belongs to whoever owns networking and serialization, not here. The boundary is
  worth naming explicitly, because for a large fraction of users "run a model"
  means "POST to an endpoint", and that path must not be made to look
  second-class by living in a different module. Recommend: `inference.remote`
  presents the same `Session`-shaped interface over HTTP, so switching a local
  model for a hosted one changes the `open` line and nothing else.
- **Pickle-based PyTorch checkpoints.** Refused on security grounds,
  permanently. See §5.4.

---

## 2. What a loaded model looks like in the type system

### 2.1 The problem, stated precisely

A model has inputs and outputs. Each has a **name** (`"input_ids"`), a **dtype**
(`I64`), and a **shape** that is partly static (`3, 224, 224`) and partly
symbolic (`batch`, `sequence`). None of this is known to the compiler, because it
lives in a file that the compiler has never opened.

Science is statically typed. So there are exactly three honest positions, and the
design uses all three, at different tiers.

### 2.2 Tier 0 — dynamic. Always available, never the recommendation.

Every session can be used without declaring anything:

```science
use inference.onnx
use inference (Bundle, Device)

def main() -> ((), Error?):
    let net, err be onnx.open("resnet50.onnx", device: Device.cpu())
    if err?:
        return ((), err)

    let mutable inputs be Bundle.new()
    inputs.set("pixels", pixels)                 # pixels: AnyTensor

    let outputs, err be net.run_dynamic(inputs)
    if err?:
        return ((), err)

    let logits, err be outputs.get("logits")     # an error, not a panic
    if err?:
        return ((), err)

    print(logits.dims())
    return ((), null)
```

`Bundle` is a name-to-`AnyTensor` map. `AnyTensor` is the erased tensor: dtype
and rank are runtime fields, not type parameters. Everything is checked at run
time and every accessor returns its value with an `Error?` beside it.

(`dims()`, not `.shape` — `shape` is reserved, and `dim`/`dims` are deliberately
not. That is the reserved-word decision paying for itself: the accessor everyone
reaches for has a legal spelling already waiting.)

This tier exists because it must: a script that walks a directory of unknown
`.onnx` files and prints their signatures cannot be statically typed, and telling
that user to go away would be absurd. It is also the tier the interactive tier of
F1 will use most, because at a REPL you have not written a signature yet.

What it is not, is the thing Science is for. It is Python with a compiler.

### 2.3 Tier 1 — declared signature. The recommended tier, and the design's centre.

The user writes the model's interface down as two record types. The compiler
checks every *use* against the declaration; the loader checks the *file* against
the declaration once.

```science
use tensors (Tensor, Dyn)
use inference (Bindable, Binding)

type ResnetInputs of D:
    pixels: Tensor of (F32, (Dyn, 3, 224, 224), D)

type ResnetOutputs of D:
    logits: Tensor of (F32, (Dyn, 1000), D)

ResnetInputs of D implements Bindable:
    def bindings() -> Array of Binding:
        [Binding(field: "pixels", wire: "input.1")]

ResnetOutputs of D implements Bindable:
    def bindings() -> Array of Binding:
        [Binding(field: "logits", wire: "output")]
```

Three things are happening here and each is deliberate.

**Field names are tensor names.** A record type is exactly the right shape for
"a set of named, typed things", and using one means the compiler's existing
field-resolution machinery does all the work. `inputs.pixels` is a field access;
a typo is `SC0203`, unresolved field, with the same "did you mean" the rest of the
language already has.

**`Bindable` handles the names that are not identifiers.** ONNX tensor names are
frequently not valid identifiers — `/model/layer.0/Add_output_0` is an ordinary
name in an exported graph. The `bindings()` method maps field to wire name. It is
tedious to write by hand, which is why the generator of §2.5 writes it. When the
wire name *is* a valid identifier, which is the common case for hand-authored
models, `bindings()` has a default implementation that uses the field names
directly and the impl is one line: `ResnetInputs of D implements Bindable`.

**`D` is the device.** See §4.2. It is a marker type parameter, so a host tensor
handed to a CUDA session is a type error rather than a silent transfer.

The session is generic over the device and the two record types:

```science
type Session of (D, I, O) where I: Bindable, O: Bindable
```

and loading annotates:

```science
let net: Session of (Cuda, ResnetInputs of Cuda, ResnetOutputs of Cuda), err be
    onnx.open("resnet50.onnx", device: Device.cuda(0))
```

That line is long. It is long once per model per program, and in exchange every
subsequent line about that model is checked. A type alias shortens it, and the
generator emits one:

```science
type Resnet is Session of (Cuda, ResnetInputs of Cuda, ResnetOutputs of Cuda)

let net: Resnet, err be onnx.open("resnet50.onnx", device: Device.cuda(0))
```

### 2.4 What is checked when

This table is the core of the design. It is what a user needs to be able to
predict without reading the implementation.

| Check | When | Failure |
|---|---|---|
| The field you named exists on the record | compile | `SC0203` unresolved field |
| Arity of `run` | compile | `SC0251` |
| dtype of every bound tensor | compile | `SC0263` |
| Rank of every bound tensor | compile | `SC0264` |
| Every **static** dimension | compile | `SC0264`, with both shapes rendered |
| Device of every bound tensor vs. the session's device | compile | `SC0262`, suggestion `.moved_to(device)` |
| Outputs do not outlive the session | compile | `SC0312` (ownership) |
| File's tensor names vs. declared wire names | **load**, once | `LoadError.MissingTensor` |
| File's dtypes vs. declared dtypes | **load**, once | `LoadError.DtypeMismatch` |
| File's static dims vs. declared static dims | **load**, once | `LoadError.ShapeMismatch` |
| Opset / file version supported by the runtime | **load** | `LoadError.Unsupported` |
| Requested execution provider is available | **load** | `LoadError.ProviderUnavailable` — and CPU fallback is *not* taken silently; see §4.6 |
| **Dynamic** dimensions (`Dyn`): the actual value | run | `RunError.ShapeMismatch` |
| Dynamic axes sharing a symbol agree across inputs | run | `RunError.InconsistentAxis` |

The distribution is the point. Everything about *names, types, ranks and static
shapes* is compile time. Everything about *this particular file* is load time,
once, at startup, not per request. Only the genuinely dynamic axes are per-call,
and those are two integer comparisons.

Compare against the alternatives: in Python and in Rust, every row of that table
happens in the last three lines.

Error codes above are proposals in the ranges §9 of the core spec assigns:
`SC025x`–`SC026x` for types, `SC031x` for ownership.

### 2.5 Can a model's interface be known at compile time?

This is the interesting question, so it gets a real answer rather than a
preference.

**Technically, yes.** An ONNX file is a protobuf. Its `graph.input` and
`graph.output` carry `ValueInfoProto` entries with a name, an element type, and a
shape whose dimensions are either an integer or a symbolic string. A compiler
could open the file during compilation and synthesize `ResnetInputs` and
`ResnetOutputs` from it. Concretely, doing that in-language would need:

1. **A compile-time facility that reads a file.** F0 has no macros (reserved, not
   implemented) and no compile-time evaluation over I/O. So this is a new
   language feature: either compile-time evaluation with a file-reading
   intrinsic, or a type-producing macro. Either is a large feature landing for
   one use case.
2. **A protobuf reader inside `sciencec`.** The compiler would carry a parser for
   a subset of `onnx.proto`. That subset is small — the graph header, not the
   nodes — but the dependency direction is bad: the *language compiler* now
   tracks a *vendor file format's* schema, on the vendor's release cadence.
3. **Salsa integration.** This part is genuinely attractive and nearly free. The
   `.onnx` file becomes a compiler input; its hash participates in the dependency
   graph; touching the model recompiles exactly the code that used it. §7.3 of
   the core spec already has the machinery.

**And it still would not work, for three reasons that no amount of engineering
fixes:**

- **The shapes are symbolic anyway.** A real exported model has `batch` and
  `sequence` as strings in the shape, and frequently entire shapes are absent
  because the exporter did not run shape inference. Reading the file gives a
  partially-dynamic answer. You do the whole feature and get `Dyn` back for the
  axes you most wanted.
- **The build stops being hermetic.** Model weights are routinely gigabytes and
  routinely downloaded at deploy time from object storage. If compiling requires
  the file, CI must fetch gigabytes to typecheck, and a colleague cannot build
  the repository without credentials to the model bucket. §12 of the core spec
  names environment reproducibility as the thing scientists trust results on;
  making compilation depend on a large mutable binary blob is the opposite of
  that.
- **It moves runtime failures into the compiler.** A model re-exported by a newer
  `torch.onnx` with a proto field `sciencec` does not know becomes a *compile
  error in the language*, reported by the language's diagnostics, blamed on the
  language. That is a support burden with no ceiling.

**The recommendation: an external generator, not a language feature.**

```
$ sciencec gen-signature resnet50.onnx --module vision.resnet > src/vision/resnet.science
```

It emits exactly the code in §2.3 — the two record types, the two `Bindable`
impls with the real wire names, the type alias, and a header comment recording
the file's SHA-256, the producer string, and the opset version. The generated
file is **checked into the repository**.

What that buys, which the language feature does not:

- The signature is reviewable in a pull request. When somebody swaps the model,
  the diff shows `1000` becoming `1001` and a reviewer sees it. That is worth
  more than any compile-time check.
- The build stays hermetic. No weights needed to compile.
- Nothing is added to the language, the parser, or the compiler's dependencies.
- The load-time check of §2.4 verifies the generated declaration still matches
  the file actually loaded, and the recorded SHA-256 lets the runtime say *"this
  file is not the one the signature was generated from"* — which is a better
  error than any of the mismatches it would otherwise produce.

Cost: a few hundred lines of protobuf reading in a tool, plus GGUF and
safetensors readers reusing §5's code. It should ship in F1 with the ONNX
binding, because without it §2.3's `Bindable` impls are hand-written misery and
users will fall back to tier 0 forever.

**Verdict: reading a `.onnx` signature during compilation is possible, is about
two months of compiler work, delivers a partially-dynamic answer, breaks hermetic
builds, and is strictly worse than a 300-line code generator. Do not do it.**

### 2.6 What a mismatch looks like

Compile-time, static dimension:

```
error[SC0264]: dimension mismatch in tensor bound to `pixels`
  --> classify.science:41:31
   |
41 |     let inputs be ResnetInputs(pixels: crops)
   |                                        ^^^^^ this is Tensor of (F32, (Dyn, 3, 256, 256), Cuda)
   |
   = note: field `pixels` is declared Tensor of (F32, (Dyn, 3, 224, 224), Cuda)
   = note: axis 2 differs: 256 vs 224
   = note: axis 3 differs: 256 vs 224
  --> vision/resnet.science:4:5
   |
 4 |     pixels: Tensor of (F32, (Dyn, 3, 224, 224), D)
   |     --------------------------------------------- declared here
   |     (generated from resnet50.onnx, sha256 4f9a2b…, opset 17)
help: resize before binding
   |
41 |     let inputs be ResnetInputs(pixels: crops.resized(224, 224))
```

Load-time, file disagrees with the declaration:

```
LoadError.ShapeMismatch:
  file:      resnet50.onnx
  tensor:    "input.1"
  declared:  F32 [Dyn, 3, 224, 224]   (vision/resnet.science:4)
  in file:   F32 [Dyn, 3, 299, 299]
  note: the declaration was generated from a file with sha256 4f9a2b…,
        this file has sha256 c018de…
  help: regenerate with `sciencec gen-signature resnet50.onnx`
```

The second is an error *value*, not a diagnostic code, because it happens in a
running program. That distinction should be maintained rigidly: `SCxxxx` codes
are the compiler's, and runtime failures are `choice` types the user matches on.
Reusing compiler codes for runtime errors is a small convenience now and a
permanent confusion later.

---

## 3. The inference call

### 3.1 The whole shape of it

```science
use inference.onnx
use inference (Device)
use vision.resnet (Resnet, ResnetInputs, ResnetOutputs)

def classify(path: borrowed String) -> (Array of I32, Error?):
    let net: Resnet, err be onnx.open(path, device: Device.cuda(0))
    if err?:
        return (Array.new(), err)

    let pixels, err be load_batch("images/", batch_size: 32)    # host tensor
    if err?:
        return (Array.new(), err)

    let inputs be ResnetInputs(pixels: pixels.moved_to(Device.cuda(0)))

    let outputs, err be net.run(inputs)
    if err?:
        return (Array.new(), err)

    return (outputs.logits.argmax(axis: 1).moved_to(Device.cpu()).to_array(), null)
```

Four moves — open, bind, run, read — each a separate expression with its own
failure. Nothing is hidden: the host-to-device transfer is written, the run's
error is tested where it happens, and the read back to host is written.

(Parameter labels avoid `on`, `with`, `at` and `in`, all of which are reserved.
`device:` is used throughout, and `device` is on the core spec's deliberately-not-
reserved list. That list was chosen well.)

### 3.2 The three run forms

```science
Session of (D, I, O) has:
    # Allocates outputs. The common case.
    def run(borrowed self, inputs: borrowed I) -> (O, RunError?)

    # Writes into caller-owned outputs. No allocation per call.
    def run_into(borrowed self, inputs: borrowed I,
                      outputs: mutable borrowed O) -> ((), RunError?)

    # Erased. For scripts and the REPL.
    def run_dynamic(borrowed self, inputs: Bundle) -> (Bundle, RunError?)
```

`run` borrows both the session and the inputs. Borrowing the session shared
rather than exclusively is a deliberate and load-bearing choice: it says a
session is safe to call concurrently, which is true of ONNX Runtime and is what
anyone writing a server needs. A stateful session — an LLM with a KV cache — is
a *different type* whose step method takes `mutable self`, and that difference is
visible in the signature rather than in documentation. See §3.5.

`run_into` exists because the steady state of a real service is a loop at a fixed
batch size, and allocating and freeing device memory on every request is how a
service acquires a p99 latency problem. Preallocate once, run into the buffers,
read them out.

### 3.3 Batching, and the last batch

Batch is the leading dimension and nothing more. The interesting part is the part
everyone forgets:

**The last batch is smaller.** 1000 images at batch 32 is 31 full batches and one
batch of 8. Three ways to handle it:

| Approach | Cost |
|---|---|
| Declare the batch axis `Dyn` and let the runtime handle it | Correct; some runtimes re-plan per new batch size, costing latency on the odd batch. This is the default. |
| Pad the last batch to full and slice the outputs | One wasted partial batch of compute; constant shapes, which TensorRT and CoreML strongly prefer. |
| Load two sessions, one at 32 and one at 8 | Twice the weights in device memory. Almost never right. |

The library provides the helper so nobody writes it wrong:

```science
for chunk in images.batched(32, pad: Pad.repeat_last()):
    let outputs, err be net.run(ResnetInputs(pixels: chunk.data))
    if err?:
        return (Array.new(), err)

    results.extend(outputs.logits.slice(axis: 0, count: chunk.valid))
```

`chunk.valid` is the number of real rows; `chunk.data` is always full width. The
padding policy is explicit — `Pad.repeat_last()`, `Pad.zeros()`, `Pad.none()` —
because with `Pad.zeros()` a model containing batch normalization in training
mode gives different answers, and a library that picks silently will eventually
be wrong in a paper.

**A dependency on the shapes design.** All of the above requires the F1 shape
language to have a **dynamic dimension marker**. If every dimension must be a
compile-time constant, then a model with a dynamic batch axis or a dynamic
sequence length cannot be given a type at all, and this entire tier collapses
into tier 0. This is not a preference; it is a hard requirement from this area on
that one. The minimum is:

- a `Dyn` dimension that type-checks against any value and is verified once per
  call;
- named dynamic dimensions (`Dyn of "batch"`) so that two tensors that must agree
  can be *stated* to agree and checked together rather than separately;
- a rule that `Dyn` is contagious: any operation on a `Dyn` axis produces `Dyn`,
  so inference never silently invents a constant.

### 3.4 Several named inputs

The record type carries them, and named-argument construction means the call site
reads as a list of names:

```science
type EncoderInputs of D:
    input_ids:      Tensor of (I64, (Dyn of "b", Dyn of "s"), D)
    attention_mask: Tensor of (I64, (Dyn of "b", Dyn of "s"), D)
    token_type_ids: Tensor of (I64, (Dyn of "b", Dyn of "s"), D)

type EncoderOutputs of D:
    last_hidden_state: Tensor of (F32, (Dyn of "b", Dyn of "s", 768), D)
    pooler_output:     Tensor of (F32, (Dyn of "b", 768), D)

let encoded be tok.encode_batch(texts, max_length: 128)

let outputs, err be encoder.run(EncoderInputs(
    input_ids:      encoded.ids.moved_to(dev),
    attention_mask: encoded.mask.moved_to(dev),
    token_type_ids: encoded.types.moved_to(dev),
))

let sentence_vectors be outputs.pooler_output
```

Three properties fall out of using a record, all free:

- **Order does not matter**, because construction is by name (§4.4 of the core
  spec). The commonest ONNX bug in Python — inputs supplied in the wrong order
  because the feed dict was built from a list — cannot be written.
- **A missing input is a compile error**, because a record cannot be partly
  constructed. In Python a missing key is a runtime error deep inside the
  runtime, and the message names an internal node.
- **The shared `Dyn of "b"` and `Dyn of "s"`** say all three inputs have the same
  batch and sequence. The runtime check is then one comparison per symbol rather
  than one per tensor, and the error says *"sequence length differs between
  `input_ids` (128) and `attention_mask` (127)"*, which is the actual bug, rather
  than a rank-4 shape dump from the runtime.

Optional inputs — models where `token_type_ids` may be omitted — use
`Tensor?`, and the binder skips the null fields. This is exactly the §5.5
argument: absence has a type.

### 3.5 Generation is a different shape, and should look different

An LLM is not a function from inputs to outputs. It is a stateful object with a
KV cache, consumed one token at a time. Modelling it as `run` would be a lie and
would produce a bad API.

```science
use inference.llama

let weights, err be llama.open("qwen3-8b-q4_k_m.gguf", device: Device.metal())
if err?:
    return ((), err)

let mutable chat be weights.session(context: 8192, seed: 42)

let prompt be chat.template([
    Message(role: Role.System, text: "You are terse."),
    Message(role: Role.User, text: "What is a tensor?"),
])

let _, err be chat.feed(prompt)
if err?:
    return ((), err)

for token in chat.generate(Sampling(temperature: 0.7, top_p: 0.9, max_tokens: 256)):
    let text, err be token.text()
    if err?:
        return ((), err)

    print(text)
```

What this deliberately does:

- `weights` and `chat` are **separate values**. The weights are loaded once and
  are shareable; a session owns a KV cache, which is per-conversation and is the
  memory that actually matters — an 8k-context KV cache for an 8B model is on the
  order of a gigabyte. Multiple sessions borrow one weights object, and the
  region engine ensures no session outlives it. This is exactly the structure
  llama.cpp has (`llama_model` / `llama_context`), so the binding stays thin.
- `chat` is `mutable`, and `generate` takes `mutable self`. Feeding two prompts
  concurrently into one session is a compile error, not a corrupted cache.
- `generate` returns an iterator, so `take`, `discard` and the rest of §4.6's
  chain vocabulary work on a token stream, and the user can stop early by
  dropping it.
- The `seed` is on the session, and the generator it creates is neither `Copy`
  nor `Clone` (§6.5 of the core spec). Two runs with the same seed and the same
  prompt give the same text, and a user cannot accidentally reuse the key.

---

## 4. Device placement and memory

§6.5 of the core spec argues that device memory is where ownership earns its
place. This section is that argument made concrete.

### 4.1 What a session owns

A `Session` owns, and is the sole owner of:

- the vendor runtime's opaque handle (`OrtSession*`, `llama_model*`);
- the weights, wherever they live — host or device;
- the runtime's memory arena or workspace for intermediates;
- the execution-provider state (CUDA streams, cuDNN handles, a TensorRT engine).

`Session` is **not `Copy` and not `Clone`**. There is no way to have two of them
by accident. Passing one to a function moves it unless the parameter is declared
`borrowed`, and since `run` takes `borrowed self` (§3.2), ordinary use never
moves it.

`Session` implements `Drop`. Dropping it releases the handle, which releases the
weights and the arena, **at a program point the compiler can name**. That is the
whole claim of §6.5 and it is worth spelling out why it matters here rather than
in the abstract: a GPU with 24 GB holds three or four sessions of a serious model.
Under a garbage collector, dropping the last reference to a session does not free
the device memory — it makes the session eligible for collection, and the
collector's decision to run is driven by *host* memory pressure, which is fine.
The program then fails to allocate on the device while the host has 60 GB free.
Julia's CUDA stack has lived with this for a decade. Under ownership, the free is
at the end of the scope, deterministically, every time.

### 4.2 Device in the type

A tensor's device is a **type parameter**, not a runtime field:

```science
choice DeviceKind:          # marker types, used only as type arguments
    Cpu
    Cuda
    Metal
    Rocm

type Tensor of (T, DIMS, D = Cpu)
```

The reason is the thesis of the language. With the device in the type:

```science
let net: Resnet, err be onnx.open("resnet50.onnx", device: Device.cuda(0))
let pixels, err be load_image("cat.jpg")           # Tensor of (F32, …, Cpu)
let outputs, err be net.run(ResnetInputs(pixels: pixels))
```

```
error[SC0262]: device mismatch
  --> classify.science:12:48
   |
12 |     let outputs, err be net.run(ResnetInputs(pixels: pixels))
   |                                                      ^^^^^^ this tensor is on Cpu
   |
   = note: `net` is a Session on Cuda, so `pixels` must be Tensor of (F32, …, Cuda)
help: move it to the device — this consumes `pixels`
   |
12 |     let outputs, err be net.run(ResnetInputs(pixels: pixels.moved_to(net.device())))
   |                                                            +++++++++++++++++++++++
```

In every alternative — Python, Rust with `ort`, C++ — that line either silently
copies to the device on every call or raises at run time. Science refuses to
compile it and tells you the fix. **This is the single most concrete
demonstration available of what the language is for**, it costs one type
parameter, and it should be on the first page of the tutorial.

The cost, paid honestly: every function generic over tensors gains a device
parameter, and functions that are genuinely host-only must say `Cpu`. The
generic-parameter default of §0.1 keeps that off the page in the common case.

The rejected alternative is device as a runtime field on the tensor, checked at
call time. It is simpler, it is what everyone else does, and it gives up the one
error message above. Not worth it.

### 4.3 Moving without copying

Two methods, named so the difference is unmissable:

```science
Tensor of (T, DIMS, D) has:
    # Consumes self. The source allocation is freed after the transfer.
    def moved_to of D2(self, device: Device of D2) -> Tensor of (T, DIMS, D2)

    # Borrows self. Both tensors exist afterwards. Costs a copy, always.
    def copied_to of D2(borrowed self, device: Device of D2) -> Tensor of (T, DIMS, D2)
```

`moved_to` consumes, so after it the host tensor is gone and using it is
`SC0301`, use after move. The user cannot hold a stale host copy of a 2 GB tensor
and wonder where the memory went. `copied_to` exists for the case where you
genuinely need both, and the reader of the code can see that you meant it.

(`move` is a reserved word; `moved_to` and `copied_to` are ordinary identifiers.
The past-participle naming is not a workaround, it is the clearer name anyway:
the tensor *has been* moved.)

**There is no implicit transfer, anywhere.** Not on call, not on assignment, not
on comparison. Every byte that crosses the PCIe bus is a method call the author
wrote. For a language whose users publish performance numbers, an invisible
transfer is a worse bug than a wrong answer, because a wrong answer gets noticed.

`moved_to` to the device a tensor is already on is a no-op returning `self`, and
the compiler eliminates it because the types are equal — so writing it
defensively inside a generic function costs nothing.

**Pinned host memory** for asynchronous transfer is the obvious next step and is
deliberately *not* in F1. With pinned memory, `moved_to` returns before the copy
has finished and the first use of the result synchronizes — a hidden
synchronization point, which this section has just spent two pages arguing
against. The resolution: **F1 is synchronous and pinned memory is not offered.**
Explicit streams arrive with the rest of GPU support in F2, and then `moved_to`
takes an optional `stream:` argument and the synchronization is written down like
everything else. Shipping asynchrony before there is a way to talk about it is a
mistake that is hard to take back.

**Zero-copy from outside.** §8 of the core spec commits the tensor layout to
DLPack, which is what makes interchange with NumPy, PyTorch, JAX, CuPy and MLX a
cast rather than a copy. Importing a DLPack capsule produces a `Tensor` that
**owns the capsule's deleter**, not one that borrows — because region inference
cannot see the foreign runtime's garbage collector, and a borrow whose referent
is managed by CPython's refcount is a borrow the compiler cannot check. Owning
the deleter is cheap, correct, and keeps the guarantee honest.

### 4.4 Who owns the outputs

This is the subtle one, and getting it wrong produces use-after-free in a
language that promised not to have any.

A vendor runtime can return outputs in two ways: as independently-owned
allocations the caller releases, or as pointers into the session's arena, valid
only until the next run. ONNX Runtime does both, depending on whether the output
was bound to a caller-supplied buffer.

Science's rule: **an output tensor carries a borrow of the session that produced
it**, in a field, and region inference does the rest.

```science
type Outputs of (D, O):
    values: O
    source: borrowed Session of (D, _, O)     # the region is inferred
```

Consequences, all of them free because the region engine already exists:

- An output cannot outlive its session. Returning `outputs.logits` from a
  function that owns the session is `SC0312`, with the chain of borrows §6.2
  requires.
- A second `run` on the same session while an output from the first is still live
  is allowed for `run` (shared borrow) and rejected for anything taking
  `mutable borrowed self`, which is the correct rule for an arena the runtime may
  reuse.
- To escape, the user writes `outputs.logits.to_host()` or `.to_owned()`, which
  copies into a Science-owned allocation and severs the borrow. Explicit,
  visible, and the diagnostic for `SC0312` suggests it.

This is precisely the "a type holding a borrow in a field" case that §11 of the
core spec already lists in the definition of done. It is good that the hardest
ownership case in the language's own acceptance test is also the one this area
needs most.

### 4.5 Arenas and steady state

```science
let net: Resnet, err be onnx.open("models/resnet50.onnx",
    device: Device.cuda(0),
    arena: Arena.reuse(),              # keep intermediates between runs
)
if err?:
    return ((), err)

let mutable outputs be ResnetOutputs.allocate(device: Device.cuda(0), batch: 32)

loop:
    let request, err be queue.take()
    if err?:
        return ((), err)

    let _, err be net.run_into(ResnetInputs(pixels: request.pixels), outputs)
    if err?:
        return ((), err)

    let _, err be respond(request, outputs.logits)
    if err?:
        return ((), err)
```

Nothing allocates inside the loop. Both the intermediates (the runtime's arena)
and the outputs (the user's buffers) are allocated once. For a service that is
the difference between a flat latency distribution and a ragged one, and it is
expressible because ownership makes "this buffer is mine and lives across
iterations" a statement the compiler understands.

### 4.6 Device selection, and not falling back silently

```science
let net: Resnet, err be onnx.open("models/resnet50.onnx", device: Device.cuda(0))
```

If CUDA is unavailable this returns `LoadError.ProviderUnavailable` in the error
slot. It does **not** fall back to CPU.

This is a deliberate reversal of what most libraries do, and the reason is the
audience. A benchmark that silently ran on the CPU is a published number wrong by
two orders of magnitude, and nobody notices until a reviewer does. A program that
refuses to start is noticed in ten seconds. Fallback is available and must be
asked for:

```science
let net: Resnet, err be onnx.open("models/resnet50.onnx",
    device: Device.prefer([Device.cuda(0), Device.cpu()]))
print("running on " + net.device().name())
```

Note the type consequence: `Device.prefer` cannot produce a statically-known
device, so a session opened that way is generic over `D` and the program must
either be written generically or branch on the outcome. That is the honest cost
of device-in-the-type, and it is the right cost: code that wants to run anywhere
must be written to run anywhere.

---

## 5. Weight formats: what Science reads, and what it hands over

### 5.1 safetensors — read and write natively

The format, in full: a `u64` little-endian header length; that many bytes of JSON
mapping tensor name to `{"dtype", "shape", "data_offsets": [begin, end]}`, plus an
optional `__metadata__` string map; then the tensor data, contiguous, in the
order the offsets give.

That is the whole specification. No compression, no quantization, no pickling, no
code execution, and no versioning problem. Reading it natively is a JSON parser
Science needs anyway, a dtype table, and an `mmap`.

```science
use weights.safetensors

let w, err be safetensors.open("model.safetensors")
if err?:
    return ((), err)

print(w.names().len())                                    # how many tensors
print(w.info("layers.0.attn.q_proj.weight"))              # F16 [4096, 4096]

let q: Tensor of (F16, (4096, 4096)), err be
    w.get("layers.0.attn.q_proj.weight")
```

Two properties worth calling out.

**Zero copy, checked.** `get` returns a tensor that borrows the memory mapping,
so reading a 16 GB file costs no copies and no resident memory beyond what is
touched. Because the borrow is in the type, a tensor that outlives the mapping is
`SC0312` rather than a segfault — the exact bug every mmap-based loader in every
language has shipped at least once. `.to_owned()` copies out when the user
genuinely needs to outlive the file.

**Write it too.** `safetensors.write(path, tensors, metadata)` makes Science able
to *produce* weights other tools consume, which is what makes it a participant in
an ecosystem rather than a consumer of one. A conversion utility written in
Science that reads a `.gguf`, dequantizes, and writes a `.safetensors` is about
forty lines, and it is a genuinely useful thing to be able to show.

Effort: one week including the dtype table. The dtype table is the only fiddly
part, because it must cover `BF16`, `F8_E4M3`, `F8_E5M2` and the integer types,
and dtypes that F0's primitives lack (§5.1 has `F16` and `BF16`, but no FP8) must
be surfaced as opaque byte tensors rather than silently reinterpreted.

### 5.2 GGUF — parse the metadata natively, hand the weights to llama.cpp

GGUF's container is also documented and also easy: a magic number, a version, a
tensor count, a key-value metadata section with a typed value union, then tensor
descriptors, then aligned data. Parsing that much natively is about the same work
as safetensors and is **worth doing**, because it powers the thing users want
first:

```
$ science inspect qwen3-8b-q4_k_m.gguf
architecture      qwen3
parameters        8.19 B
quantization      Q4_K_M
context length    32768
vocab size        151936
rope freq base    1000000
file size         4.68 GiB
estimated VRAM    5.4 GiB  (at context 4096)
tensors           291
```

Everything on that screen comes from metadata. No dequantization is involved, and
"will this fit on my card" is the first question every user has.

**The values are a different matter and Science must not touch them.** GGUF
carries on the order of thirty quantization schemes — `Q4_K_M`, `Q5_K_S`, `Q6_K`,
`IQ2_XXS`, `IQ4_NL` and the rest. Each is a bit-packed block format with
per-block scales, sometimes per-super-block scales, sometimes a codebook. They
are specified *by llama.cpp's implementation and nothing else*: there is no
document, the reference is the C++ source, and new schemes appear while old ones
change. A native dequantizer would be a permanent commitment to bit-exact
tracking of a moving target, and its failure mode is not a crash — it is slightly
wrong numbers and slightly wrong text, which nobody notices for a month.

So the split is: **Science reads GGUF's header, llama.cpp reads GGUF's body.**
That line is defensible, explainable, and stable.

### 5.3 ONNX — parse enough for the signature, nothing more

An `.onnx` file is a protobuf `ModelProto` with the graph and the initializers
(the weights) inline. Science parses the small subset needed for §2.5's
generator — `graph.input`, `graph.output`, `opset_import`, `producer_name` — and
never parses the nodes or the initializers. Executing the graph is ONNX Runtime's
job and there is no version of this project in which it should be Science's.

One operational trap that must be handled in the loader rather than discovered by
users: protobuf has a 2 GB limit, so models above it store weights in **external
data files** referenced by relative path (`model.onnx` plus `model.onnx_data`).
Copying only the `.onnx` file to a server is a very common mistake and produces
an unhelpful runtime error. Science's loader should detect external-data
references, check the files exist before handing off, and fail with
`LoadError.MissingExternalData` naming the expected path.

### 5.4 PyTorch `.pt` / `.pth` / `.bin` — refused, permanently

A `.pt` file is a ZIP archive containing a **Python pickle**. Unpickling is, by
construction, arbitrary code execution: the format contains opcodes that import
modules and call functions, and "safe unpickling" is a blocklist, which is to say
it is a thing that gets bypassed.

**Science will never implement a pickle interpreter.** Not a restricted one. A
memory-safe language that ships a remote-code-execution primitive in its model
loader has given up more than it gained.

The error says so and says what to do:

```
LoadError.PickleFormat:
  file: pytorch_model.bin
  This is a Python pickle. Science does not execute pickles, because
  loading one runs arbitrary code.
  Convert it once, with Python:

      import torch
      from safetensors.torch import save_file
      save_file(torch.load("pytorch_model.bin", map_location="cpu"),
                "model.safetensors")

  Or export to ONNX with torch.onnx.export.
```

Hugging Face has already moved the ecosystem's default to safetensors for exactly
this reason, so this position costs less every year.

### 5.5 Summary

| Format | Science's role | Reason |
|---|---|---|
| safetensors | **read and write natively**, zero-copy | Trivial, dependency-free, ubiquitous |
| GGUF | **read metadata natively**; weights via llama.cpp | Container is simple; quantization is a moving C++ target |
| ONNX | **read the signature**; execution via ONNX Runtime | Protobuf subset is small; the graph is not our business |
| `.npy` / `.npz` | read and write natively | Trivial, and scientific users have data in them (coordinate with the data-IO area) |
| TensorRT plan | opaque blob, cached, never parsed | Not portable across GPUs or versions |
| `.pt` / `.pth` / `.bin` | **refused** | Pickle is arbitrary code execution |

---

## 6. Tokenizers and preprocessing

This is where real inference pipelines spend their frustration, and it is the
part a language usually leaves to somebody else. It should not be left out here,
because it is where the "better than the alternatives" claim is won or lost. A
user who can load a model in three lines and then spends two days on why their
embeddings do not match Python's has not been helped.

The governing observation: **preprocessing failures are silent.** A wrong tensor
name is an error. A wrong resize filter is a model that is two points less
accurate, and a wrong tokenizer normalizer is an embedding subtly in the wrong
place. Nothing crashes. The design must therefore be organized around *matching a
reference exactly* and *proving that it does*.

### 6.1 Tokenizers: bind, do not reimplement

A `tokenizer.json` is a declarative pipeline — normalizer, pre-tokenizer, model
(BPE / WordPiece / Unigram), post-processor, decoder — and implementing it
natively is genuinely feasible: a few thousand lines, no exotic algorithms.

**Do not.** The reason is the silence. An NFKC normalizer that handles one
codepoint class differently, a byte-level pre-tokenizer with a different
space-prefix rule, a BPE merge tie broken the other way — each produces token ids
that are *valid* and *wrong*, and the model produces plausible output. You do not
get a bug report; you get a quiet accuracy regression in someone's paper.

So: **bind Hugging Face `tokenizers`** through a C shim, and bind
**SentencePiece** for the models that predate `tokenizer.json`. GGUF's embedded
vocabulary comes free with the llama.cpp binding. Three bindings, and the vast
majority of models are covered by ids that are correct by construction because
they came from the reference implementation.

```science
use text.tokenizers

let tok, err be tokenizers.open("tokenizer.json")

let encoded be tok.encode_batch(
    ["the cat sat", "on the mat"],
    max_length: 128,
    padding: Padding.longest(),
    truncation: Truncation.longest_first(),
)

# encoded.ids   : Tensor of (I64, (Dyn of "b", Dyn of "s"))
# encoded.mask  : Tensor of (I64, (Dyn of "b", Dyn of "s"))
# encoded.spans : Array of Array of Span     # offsets back into the input text
```

`encode_batch` returns tensors with the `Dyn of "b"` / `Dyn of "s"` symbols of
§3.4 already attached, so the tokenizer's output binds to an encoder's inputs
with the batch and sequence agreement *checked*, not assumed. That handoff is the
whole reason to put tokenizers in the same design note as inference.

`spans` is not optional. Every extractive task — NER, question answering,
highlighting — needs to map a token back to a character range in the original
string, and a tokenizer that drops offsets forces users into fragile realignment
code. It is the single most common thing missing from lightweight tokenizer
bindings.

**The conformance suite is the deliverable, not the binding.** For a fixed list
of models and a corpus of adversarial strings — emoji, combining marks, CJK,
zero-width joiners, mixed scripts, leading and trailing whitespace, very long
words — assert that Science's ids equal the reference ids exactly. Run it in CI.
It is the only mechanism that keeps this honest, and it must exist before the
binding is announced.

### 6.2 Images: the resize filter is the bug

```science
use vision.preprocess

let pipeline be Preprocess.imagenet_torchvision()    # exactly torchvision's transform
let pixels, err be pipeline.apply_batch(paths)  # Tensor of (F32, (Dyn, 3, 224, 224))
```

What the library owns:

- **Decoding**: JPEG, PNG, WebP, TIFF. Bound, not written — `libjpeg-turbo`,
  `libpng`, or the single-header `stb_image` for a dependency-free default with a
  documented quality caveat.
- **Resize**, with the filter named and no default that is not named. `bilinear`,
  `bicubic`, `lanczos`, `nearest`, and crucially the `antialias` flag. PIL's
  bilinear and OpenCV's bilinear are *different functions*, and a model trained
  with one and served with the other loses accuracy. This is the most common
  silent inference bug in computer vision and the library's job is to make it
  impossible to be vague about.
- **Crop** (center; resize-shortest-side-then-center-crop), **normalize** with
  explicit mean and std, **layout** HWC↔CHW, **dtype** conversion with the scale
  factor written down — `u8` in `[0,255]` to `f32` in `[0,1]` is a division by
  255 that somebody will forget.
- **Named presets that are exact reproductions** of the well-known pipelines:
  `imagenet_torchvision()`, `imagenet_tensorflow()`, `clip()`, `siglip()`,
  `yolo_letterbox()`. Each carries a test asserting its output matches the
  reference implementation's within a tolerance, on fixed images.

### 6.3 Audio

Resampling (with the filter named, same argument as images), mono downmix, frame
slicing, STFT, mel filterbanks, log-mel. And `Preprocess.whisper()` as an exact
preset, because Whisper's expected input — 30-second padded windows, 80 mel bins,
a specific hop and window — is fiddly, universally needed, and universally gotten
wrong the first time.

### 6.4 Tabular, and a piece of good news

For scikit-learn models exported through `skl2onnx`, the scaler, the imputer and
the one-hot encoder are **baked into the ONNX graph**. The user feeds raw columns
and the graph does the preprocessing, with the fitted parameters, exactly as
fitted. Say this loudly in the documentation, because users arrive expecting to
have to reimplement `StandardScaler` and they do not.

What Science must supply is the column-to-tensor step: a dataframe or column-store
to `Tensor` conversion that checks column names and order against the model's
input names. Column order is the tabular equivalent of the wrong-tensor-name bug,
and it deserves the same compile-time treatment — the generator can emit a record
type whose fields are the column names.

### 6.5 Output post-processing

Belongs in the library, because every user writes it and half write it wrong:

- `softmax`, `log_softmax`, `sigmoid`, `argmax`, `topk`;
- **label maps** loaded from the artifacts that accompany models — `labels.txt`,
  or `id2label` out of a `config.json` — so outputs are strings rather than
  indices, with an off-by-one in the label file caught by comparing the map's
  size to the output dimension at load;
- non-maximum suppression and box decoding for detection, with the coordinate
  convention named (`xyxy`, `xywh`, `cxcywh`) because it is the resize filter of
  object detection;
- sampling for generation: greedy, temperature, top-k, top-p, min-p, repetition
  penalty — all driven by an explicitly-seeded generator that is neither `Copy`
  nor `Clone` (§6.5 of the core spec), so a reproducible run is reproducible by
  construction;
- detokenization, which is not `join(" ")` and never was.

### 6.6 The principle

**Preprocessing must be declared, versioned, and printable.** A `Preprocess`
value implements `Display` — the same trait `print` requires (§5.4 of the core
spec) — and prints the exact pipeline:

```
imagenet_torchvision:
  decode(rgb) -> resize(shortest=256, bilinear, antialias=true)
  -> center_crop(224) -> to_f32(scale=1/255)
  -> normalize(mean=[0.485,0.456,0.406], std=[0.229,0.224,0.225])
  -> to_chw
```

A published result can then state what it did, and a discrepancy between two
machines is diagnosed by diffing two strings instead of by reading two codebases.
For an audience that publishes, that printout is worth as much as the speed.

---

## 7. Training

**Science should not attempt training in its first versions. Not in F1, not in
F2, and the documentation should say so on the front page rather than in a
footnote.**

### 7.1 Why

Training is not a feature. It is a stack, and every layer of it is a multi-year
project:

- Reverse-mode AD over the *whole library*, not over a demo. F2 brings AD, and AD
  over twenty operators is not AD over what a real model needs.
- Optimizers with correct state handling, plus learning-rate schedules.
- Mixed precision with loss scaling, and the numerics work to make it stable.
- Checkpointing, including optimizer state, including resuming mid-epoch.
- Data loading with prefetch, shuffling, sharding and augmentation, overlapped
  with compute — which is where most training frameworks' actual engineering
  went.
- Distributed collectives: NCCL, gradient bucketing, ZeRO-style sharding.
- And underneath all of it, **kernels**: fused attention, fused optimizers, fused
  normalization. The user's bar is not "it trains"; it is "as fast as PyTorch",
  and PyTorch's speed is ten years of cuBLAS, cuDNN, CUTLASS, Triton and
  hand-written CUDA. That is not a language-design problem, and no amount of type
  system closes it.

A training story that is 3× slower than PyTorch is not a training story. It is a
reason not to use the language.

### 7.2 What the honest answer costs

It should be said plainly, because it is not free:

- **The headline goes.** "Train your models in Science" is the sentence people
  expect a new ML language to say, and Mojo says it. Not saying it means Science
  introduces itself as a *deployment and scientific-computing* language, which is
  a smaller story on a conference slide.
- **The examples never show a training loop.** The training loop is the canonical
  ML code sample. A language whose documentation has none reads as unambitious to
  exactly the people who write about languages.
- **Some users leave at the first question.** "Can I train in it?" is asked
  early, and "no" ends some conversations that a vaguer answer would have
  continued. That is the cost of the honesty and it is worth paying, because the
  alternative — a training API that technically exists and is 3× slow — loses the
  same users three months later, angrier, and after they have written code.

What it buys is the thing this note opened with: a small surface that is actually
better than the alternatives, shipped while it still matters, instead of a large
surface that is worse than PyTorch at the one thing it gets compared on.

### 7.3 What to do instead, which is not nothing

Two things are cheap and serve most of the *scientific* audience, as opposed to
the deep-learning audience:

**Fine-tuning by delegation.** ONNX Runtime has an on-device training API;
LibTorch has autograd. Where a user needs to fine-tune, bind the vendor's
training API rather than building one. The result is limited and honest: you can
fine-tune what the runtime supports, at the runtime's speed, and Science is the
glue. That is consistent with the frame — the heavy computation is the vendor's.

**Fitting that is not deep learning.** This is the larger and more neglected
half. A physicist fitting a curve, a chemist fitting rate constants, an
epidemiologist fitting a compartment model — none of them want a training loop.
They want least squares, maximum likelihood, Levenberg–Marquardt, L-BFGS,
gradient boosting, and confidence intervals. That is:

- bindings to LAPACK, and to XGBoost and LightGBM, whose C APIs are small;
- optimizers over a scalar objective, which F2's AD serves **exactly**, because
  the objective is a function the user wrote in Science over a few thousand
  parameters and there are no fused attention kernels involved;
- and it is *model fitting*, which for a language called Science is arguably the
  more on-brand capability.

Framed that way, "Science does not train neural networks but does fit models" is
not a retreat. It is a clearer positioning than "Science trains models, slowly".

### 7.4 The tripwire

Revisit deep-learning training only when all four are true:

1. Typed shapes have shipped and are used in anger by someone outside the team.
2. AD has shipped and is correct over the full tensor library.
3. There is a GPU kernel story — a way to write or generate a fused kernel — that
   does not mean "call cuBLAS".
4. At least one nontrivial model *runs* in Science, end to end, faster than the
   Python equivalent, measured, in public.

Until all four hold the answer is no, and the answer should be given quickly so
people can plan around it.

---

## 8. A complete worked example

Load an ONNX image classifier, run a directory of images through it in batches on
a GPU, and write the predictions to a CSV.

### 8.1 The generated signature

Produced by `sciencec gen-signature resnet50.onnx --module vision.resnet` and
checked into the repository.

```science
# src/vision/resnet.science
# Generated by sciencec gen-signature. Do not edit by hand.
#   source:   resnet50.onnx
#   sha256:   4f9a2b8c1d0e5a37f2b9c4d8e1a06b3f7c2d9e4a1b8c5d2e9f6a3b0c7d4e1a58
#   producer: pytorch 2.4.0
#   opset:    17

use tensors (Tensor, Dyn)
use inference (Bindable, Binding, Session, Cuda)

public type ResnetInputs of D:
    pixels: Tensor of (F32, (Dyn of "batch", 3, 224, 224), D)

public type ResnetOutputs of D:
    logits: Tensor of (F32, (Dyn of "batch", 1000), D)

ResnetInputs of D implements Bindable:
    def bindings() -> Array of Binding:
        [Binding(field: "pixels", wire: "input.1")]

ResnetOutputs of D implements Bindable:
    def bindings() -> Array of Binding:
        [Binding(field: "logits", wire: "495")]

public type Resnet is Session of (Cuda, ResnetInputs of Cuda, ResnetOutputs of Cuda)
```

Note `"495"`, the wire name of the output: an exported PyTorch graph names its
output after an internal node number. That is why `Bindable` exists, and why it
is generated rather than written.

### 8.2 The program

```science
# src/classify.science

use inference.onnx
use inference (Device, Pad, LoadError, RunError)
use vision.preprocess (Preprocess)
use vision.resnet (Resnet, ResnetInputs)
use text.labels

const BATCH be 32
const TOP_K be 5

type Prediction:
    path:  String
    label: String
    score: F32

def main() -> ((), Error?):
    let dev be Device.cuda(0)

    # ---- load the model -------------------------------------------------
    # Names, dtypes, ranks and static dims were checked when this file compiled.
    # This call checks the file on disk against that declaration, once.
    let net: Resnet, err be onnx.open("models/resnet50.onnx", device: dev)
    if err?:
        return ((), err)

    print("loaded " + net.describe())

    let classes, err be labels.read_lines("models/imagenet_classes.txt")
    if err?:
        return ((), err)

    if classes.len() is not 1000:
        return ((), Error.new("label file has " + classes.len() as String
                              + " entries, the model outputs 1000"))

    # ---- preprocessing, declared and printable --------------------------
    let pipeline be Preprocess.imagenet_torchvision()
    print(pipeline)

    let paths, err be list_images("images/")
    if err?:
        return ((), err)

    print(paths.len() as String + " images, batch " + BATCH as String)

    # ---- run ------------------------------------------------------------
    let mutable predictions be Array of Prediction .new()

    for chunk in paths.batched(BATCH, pad: Pad.repeat_last()):
        # host tensor: Tensor of (F32, (32, 3, 224, 224), Cpu)
        let staged, err be pipeline.apply_batch(chunk.items)
        if err?:
            return ((), err)

        # `moved_to` consumes `staged`; the host buffer is freed here.
        # Omitting it is SC0262 at compile time, with the fix suggested.
        let inputs be ResnetInputs(pixels: staged.moved_to(dev))

        # `outputs` borrows `net`. It cannot escape this loop body,
        # and the compiler knows it.
        let outputs, err be net.run(inputs)
        if err?:
            return ((), err)

        let scores be outputs.logits.softmax(axis: 1)
        let best   be scores.topk(TOP_K, axis: 1).moved_to(Device.cpu())

        for row in 0..chunk.valid:
            for rank in 0..TOP_K:
                predictions.push(Prediction(
                    path:  chunk.items.get(row).clone(),
                    label: classes.get(best.indices.at(row, rank) as Int).clone(),
                    score: best.values.at(row, rank),
                ))
        # `outputs` drops here; the borrow of `net` ends; the arena is reusable.

    # ---- write ----------------------------------------------------------
    let mutable csv be String.new()
    csv.push_str("path,label,score\n")
    for p in predictions:
        csv.push_str(p.path + "," + quote(p.label) + "," + p.score as String + "\n")

    let _, err be write_file("predictions.csv", csv)
    if err?:
        return ((), err)

    print("wrote " + predictions.len() as String + " rows to predictions.csv")
    return ((), null)
    # `net` drops here. The CUDA context, the weights and the arena are
    # freed at this point, named by the compiler, not when a collector
    # decides host memory is tight.
```

### 8.3 What that example demonstrates, line by line

| Line | What it shows |
|---|---|
| `let net: Resnet, err be onnx.open(...)` | Compile-time signature from a generated, reviewed, checked-in declaration; load-time verification against the actual file; the error tested on the next line |
| `if classes.len() is not 1000` | The label-map-versus-output-dimension check of §6.5, written once, in English words |
| `print(pipeline)` | Preprocessing is declared, versioned and printable (§6.6) |
| `paths.batched(BATCH, pad: ...)` | Batching with the last-batch problem handled explicitly, not silently (§3.3) |
| `staged.moved_to(dev)` | Explicit host-to-device transfer that consumes the host tensor; the omission is a compile error with a machine-applicable fix (§4.3) |
| `let outputs, err be net.run(inputs)` | Shared borrow of the session, so it is concurrency-safe by signature; outputs borrow the session and cannot escape (§4.4) |
| `for row in 0..chunk.valid` | Padded rows discarded by construction |
| the final `return ((), null)` | Device memory freed at a program point the compiler names (§6.5 of the core spec) |

None of the eight rows is checkable at all in the Python version of this program,
and only the last is checkable in the Rust version. That gap is the product.

---

## 9. Dependencies, risks, and what must be true elsewhere

### 9.1 The FFI is the whole project

Everything in §1 except safetensors is an FFI binding, and F0 has no FFI. The
minimum this area needs, stated as a requirement on whoever owns it:

- `extern` declarations with C calling convention and C struct layout;
- opaque pointer types that are not `Copy`, so a handle cannot be duplicated;
- function pointers, for the logger and allocator callbacks every runtime wants;
- null-terminated string conversion in both directions, with the allocation
  ownership stated;
- a linking story: `sciencec` must be told which libraries to link and where to
  find them, on three platforms;
- a safety boundary: the FFI surface is unsafe, and the whole design above is the
  *safe* wrapper over it. `unsafe` is a reserved word in F0 and needs to become a
  feature.

**If the FFI is mediocre, this area is mediocre, and no amount of type system
design in this note compensates.** It is the highest-priority dependency and
should be scheduled before the ONNX binding, not alongside it.

### 9.2 Getting the vendor library onto the user's machine

ONNX Runtime with CUDA is a few hundred megabytes and version-matched to a CUDA
toolkit. Two options:

| Option | Cost |
|---|---|
| **Static link** at build time | `sciencec` builds now need a GPU toolchain; one binary per accelerator; a CPU-only machine cannot build the program at all |
| **Dynamic load at run time** (`dlopen` / `LoadLibrary`) | One binary runs anywhere; the same program uses CUDA where present and CPU where not; errors move from link time to run time |

**Recommend dynamic loading**, with a documented search path and a genuinely good
"not found" message that names the paths searched and the expected version. The
cost — a missing library becomes a run-time failure — is real but small, because
the failure is at `open`, which is the first thing the program does, and the
message can be exact.

### 9.3 Risks, named

- **llama.cpp churn.** Mitigated by a pinned commit and a narrow surface; it is
  still recurring work, forever. Budget it rather than being surprised by it.
- **Tokenizer drift.** A binding upgrade changes ids. Mitigated only by the
  conformance suite of §6.1, which is therefore not optional.
- **ONNX export gaps.** Some users' models will not export, and Science's answer
  is "no". Document the limitation up front rather than letting people discover
  it after writing code.
- **The `Dyn` dependency.** If the shapes design does not provide a dynamic
  dimension, tier 1 of §2 does not exist and this area degrades to a nicer
  Python. This must be settled early, between the two designs, not discovered
  during implementation.
- **Generic-parameter defaults.** If rejected, §4.2's device-in-the-type gets
  noticeably uglier. Still worth doing; the ergonomic fallback is a `HostTensor`
  alias.
- **`run`'s shared borrow is a concurrency promise F0 cannot check.** F0 has no
  threads, so nothing verifies it. It must not be quietly un-promised when F3
  arrives.

### 9.4 Proposed phasing

| Phase | Ships |
|---|---|
| F0.5 | C FFI: `extern`, opaque handles, callbacks, linking. Prerequisite for everything else here. |
| F1 | safetensors read/write; GGUF metadata reader and `science inspect`; ONNX Runtime binding on CPU; `Session`, `Bindable`, tiers 0 and 1; `sciencec gen-signature`; tokenizers binding plus conformance suite; image preprocessing with exact presets |
| F1.5 | llama.cpp / GGUF execution; generation API with sampling and KV cache; audio preprocessing and the Whisper preset; `inference.remote` over HTTP |
| F2 | CUDA and Metal execution providers; device-in-the-type enforced end to end; explicit streams and asynchronous transfer; `run_into` and arena reuse; XGBoost / LightGBM / LAPACK for the fitting story of §7.3 |
| later | LibTorch, if and only if asked for twice. TensorRT direct, if and only if measured. |

---

## 10. Open questions

1. **Does the shape language have `Dyn`, and does it have *named* dynamic
   dimensions?** Hard dependency (§3.3). Settle with the shapes design first;
   everything in tier 1 rests on it.
2. **Generic parameter defaults**: in or out? Determines whether
   device-in-the-type is pleasant or merely correct (§0.1, §4.2).
3. **Does `Session` need a `Send`/`Sync`-shaped guarantee before F3's actors?**
   `run` takes a shared borrow, which promises concurrency safety that nothing
   currently checks (§9.3).
4. **Where does `inference.remote` live?** It presents this area's interface over
   somebody else's transport. Recommend the interface is defined here and the
   transport imported.
5. **Should `sciencec gen-signature` be a compiler subcommand or a separate
   tool?** A subcommand ties the ONNX protobuf subset to the compiler's release
   cadence; a separate tool does not. Leaning separate, despite the worse
   ergonomics.
