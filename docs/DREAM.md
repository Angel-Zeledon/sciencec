# Science — DREAM

> **Qué es este documento.** La versión de Science que queremos que exista cuando
> esté terminada: el ecosistema completo, de `core` a los agentes, con el orden en
> que construirlo. Es un documento de destino, no de estado.
>
> **Qué no es.** No es la especificación. La especificación es
> `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` y manda sobre este
> documento en todo lo que ya esté decidido allí. Cuando los dos difieren, no es
> que uno esté mal — es que este documento está pidiendo un cambio, y la
> [Parte II](#parte-ii--las-propuestas-de-sintaxis) es el único sitio donde eso
> pasa. Está aislado ahí a propósito.

---

## Cómo leer este documento

Tiene dos usos y conviene no mezclarlos.

**Como mapa de arquitectura.** Partes III a VI: qué librerías existen, de qué
dependen, en qué orden se construyen. Nada de eso contradice lo ya decidido;
es material nuevo que rellena el hueco que la especificación deja a propósito
fuera de F0.

**Como instrucción para un agente de código.** La Parte VII es el acuerdo de
trabajo, y es lo que hay que leer antes de tocar el repositorio. La Parte II
**no** es una instrucción: es un conjunto de propuestas sin aprobar, y ejecutarlas
sin una decisión explícita rompería el compilador, los veintidós ejemplos y las
cuarenta y ocho notas de diseño a la vez.

### El estado de cada parte

| Parte | Qué contiene | Estado |
|---|---|---|
| I | Visión y principios | Acordado, y compatible con la especificación |
| **II** | **Cambios de sintaxis** | **Propuesto, en conflicto — requiere decisión** |
| III | Arquitectura del ecosistema | Propuesto, sin conflicto |
| IV | Catálogo de librerías | Propuesto, sin conflicto. Es el grueso del documento |
| V | Toolchain | Propuesto; parcialmente ya decidido en `package-manager.md` |
| VI | Roadmap y prioridades | Propuesto |
| VII | Acuerdo de trabajo | Acordado |

---

## Índice

**[Parte I — La visión](#parte-i--la-visión)** · 1 Qué es Science · 2 El objetivo final · 3 Principios de arquitectura

**[Parte II — Las propuestas de sintaxis](#parte-ii--las-propuestas-de-sintaxis)** · 4 El conflicto, en una tabla · 5 Propuesta por propuesta · 6 La migración, si se aprueba

**[Parte III — Arquitectura del ecosistema](#parte-iii--arquitectura-del-ecosistema)** · 7 Organización · 8 Capas y dependencias · 9 Backends · 10 Serialización · 11 Frontera nativa

**[Parte IV — El catálogo de librerías](#parte-iv--el-catálogo-de-librerías)** · 12 Índice · 13–27 las quince familias

> **Convención de los listados de la Parte IV.** Los bloques que enumeran
> firmas dentro de un `X has:` son **catálogos de API**: métodos declarados sin
> cuerpo, para leerlos de corrido. Eso no compila tal cual — un `has:` exige
> cuerpo, a diferencia de una `interface`. Los bloques que muestran funciones
> completas sí están en sintaxis vigente, y las formas que usan
> (`let a, b, err be …`, `continue`, `if not x?:`, `each`, `nombre giving …`,
> argumentos etiquetados) se verificaron contra `sciencec check`. Lo que no se
> puede verificar todavía son los **nombres**: ninguna de estas librerías
> existe.

**[Parte V — Toolchain](#parte-v--toolchain)** · 28 Package manager · 29 Logging y observabilidad · 30 Tooling · 31 Documentación · 32 Testing · 33 Diagnósticos · 34 Formatter · 35 Compatibilidad

**[Parte VI — Roadmap](#parte-vi--roadmap)** · 35bis Dónde estamos hoy, medido · 36 Prioridades · 37 Las trece fases

**[Parte VII — Acuerdo de trabajo](#parte-vii--acuerdo-de-trabajo)** · 38 Reglas · 39 Lo que no hay que hacer · 40 Definition of Done

**[Apéndices](#apéndice-a--programas-objetivo)** · A Programas objetivo · B Qué se reorganizó

---
---

# Parte I — La visión

## 1. Qué es Science


Science debe evolucionar hacia un lenguaje compilado de propósito general con una identidad especialmente fuerte en:

- scientific computing
- numerical computing
- machine learning
- automatic differentiation
- data processing
- high-performance computing
- AI agents
- backend/API development
- sistemas y tooling

La meta no es construir simplemente "otro lenguaje con una sintaxis bonita", ni limitar Science a ser "Python más rápido".

La meta es construir un ecosistema donde el lenguaje pueda expresar de forma natural:

```science
import tensor
import autograd
import nn
import optim
import plot

let model be nn.Sequential([
    nn.Linear(784, 256),
    nn.ReLU(),
    nn.Linear(256, 10)
])

for epoch in range(10):
    let prediction be model(images)
    let loss be nn.cross_entropy(prediction, labels)

    autograd.backward(loss)
    optimizer.step()

plot.show(loss_history)
```

pero también:

```science
import http
import agents

let agent be agents.Agent(
    tools: [search, calculate]
)

let app be http.App()

app.post("/chat", (req, res):
    let response be agent.run(req.body.message)
    res.json(response)
)

app.listen(8080)
```

Science debe poder crecer sin convertirse en un framework monolítico.

---

## 2. El objetivo final


Science debe poder posicionarse conceptualmente como:

```text
                    SCIENCE
                       │
       ┌───────────────┼────────────────┐
       │               │                │
     SYSTEM         SCIENCE            AI
       │               │                │
       │          ┌────┼────┐       ┌───┼────┐
       │          │    │    │       │   │    │
      FFI       Tensor Math ML    Agents Data Tools
       │          │    │    │       │
       └──────────┴────┴────┴───────┘
                       │
                   Ecosystem
                       │
                 scargo + tooling
```

La identidad de Science debe surgir de la integración entre:

- lenguaje moderno
- rendimiento nativo
- tipos fuertes
- memoria segura/controlable
- computación científica
- tensores
- autodiff
- ML
- GPU
- interoperabilidad
- agentes
- backend/API
- tooling
- package manager

El objetivo no es lanzar todo inmediatamente.

El objetivo es que **la arquitectura permita llegar a todo esto sin tener que rediseñar el lenguaje desde cero**.

---

## 3. Principios de arquitectura


### 3.1 El compilador debe ser pequeño en responsabilidades

No implementar todas las capacidades dentro del compilador.

El compilador debe proporcionar las primitivas que hagan posible construir un ecosistema sólido:

- lexer
- parser
- AST
- name resolution
- type system
- generics
- interfaces/traits
- ownership/borrowing si aplica
- mutability
- diagnostics
- module system
- package integration
- ABI
- FFI
- async/concurrency primitives
- SIMD/intrinsics cuando sea apropiado
- code generation
- runtime primitives

Todo lo demás debería vivir preferentemente en librerías.

---
---

# Parte II — Las propuestas de sintaxis

> **Esta parte pide revertir decisiones que ya están tomadas, implementadas y
> documentadas.** No está aquí para ser ejecutada sin más: está aquí para ser
> decidida. Cada propuesta lleva lo que cuesta y el argumento que hay en contra,
> que en varios casos ya estaba escrito antes de que esta lista se redactara.

## 4. El conflicto, en una tabla

La especificación, en §4.3, tiene una tabla de dos columnas: cómo lo escribe
Science y cómo lo escribe Rust. **Lo que esta parte propone es, línea por línea,
mover la columna de Science a la columna de Rust.** Verlo así es lo que hace
comprensible la lista entera, y también lo que explica por qué cada punto tiene
un argumento en contra ya escrito: son las mismas filas, discutidas cuando se
eligió la columna.

| # | Hoy | Se propone | Choca con | Coste |
|---|---|---|---|---|
| 1 | `borrowed T` / `mutable borrowed T` | `&T` / `&mut T` | Spec §4.1, §4.3 | Medio |
| 2 | `let mutable x be v` | `let mut x be v` | Spec §4.3 | **Bajo** |
| 3 | `Array of T`, `Map of (K, V)` | `Array[T]`, `Map[K, V]` | Spec §4.3; `indexing-and-array-literals.md` | **Alto** |
| 4 | `def largest of T(…)` | `def largest[T](…)` | Igual que 3 — es la misma decisión | Alto |
| 5 | `T?` y `-> (T, Error?)` | `Option[T]` y `Result[T, E]` | `syntax-revision-2.md` §3 y §5.5 | **Muy alto** |
| 6 | `use modulo` | `import modulo` | Spec §4.4; `import` está reservada sin uso | Bajo |
| 7 | `for i in 0..n:` | `for i in range(n):` | `AGENTS.md` regla 6 | Bajo |
| 8 | `implements`, `has` | *sin cambio* | — | **Ninguno: ya coinciden** |

La fila 8 importa tanto como las otras siete. Este documento dice, sobre
`implements`, que *"forma parte de la identidad legible de Science"* — y eso es
exactamente el principio de §4.1, que es el que sostiene las filas 1 a 4. La
lista está aplicando dos criterios opuestos a la vez, y esa es la contradicción
interna que hay que resolver antes que cada punto por separado.

### La pregunta que decide las siete

> ¿Science es un lenguaje que se lee en inglés y por eso escribe `Array of T`, o
> es un lenguaje con sintaxis familiar para quien viene de Rust y Python?

No hay respuesta parcial que sea coherente. Adoptar `&T` y `[T]` y conservar
`implements` y `has` deja un lenguaje que es simbólico en los tipos y verboso en
las declaraciones, que es el peor de los dos mundos y el que ningún lector
podrá predecir.

---

## 5. Propuesta por propuesta

### 5.1 `&T` en lugar de `borrowed T`

**Coste: medio.** Lexer, parser, AST, los veintidós ejemplos, `tests/ui/`, la
gramática de VS Code, el resaltador de la web y las notas que lo citan.

**A favor.** Es más corto, y todo el mundo que llega de Rust ya lo sabe leer.

**En contra.** `&` ya es *and* bit a bit en la tabla de operadores, así que el
mismo carácter tendría dos trabajos y se distinguirían por posición. Y el
público objetivo de Science no viene de Rust: viene de Python, R y MATLAB, donde
`&` no significa préstamo sino nada en absoluto.

### 5.2 `let mut` en lugar de `let mutable`

**Coste: bajo.** Una palabra en el lexer, un diagnóstico de migración, un pase
de `sciencec fmt`.

Es la única propuesta de las siete que puede adoptarse por separado sin
incoherencia, porque no cambia de registro: sigue siendo una palabra inglesa,
sólo que abreviada. El precedente exacto existe — la revisión 3 hizo justo esto
con `function` → `def`, y `syntax-revision-3.md` documenta lo que costó y por
qué salió barato.

**Si se aprueba una sola cosa de toda la Parte II, que sea ésta.**

### 5.3 `Array[T]` en lugar de `Array of T`

**Coste: alto, y no es sólo trabajo — es una colisión de gramática.**

Los corchetes ya tienen dos trabajos, y `indexing-and-array-literals.md` es
explícita en que *"el papel del corchete lo decide la posición en el parser y
nada más"*: `a[i]` indexa y `[1, 2, 3]` construye. Añadir un tercer papel en
posición de tipo es **parseable** — Python y Scala lo hacen — pero gasta la
última holgura que le quedaba a ese símbolo, y lo hace justo en el lenguaje
donde `m[i, j]`, los rangos abiertos dentro de corchetes y los literales de
matriz ya conviven en la misma regla.

**Lo que se gana** es familiaridad. **Lo que se pierde** es que `Map of (String,
Int)` se lee en voz alta y `Map[String, Int]` no, que era el argumento de §4.1.

### 5.4 `def largest[T](…)` en lugar de `def largest of T(…)`

No es una propuesta independiente: es la 5.3 aplicada a la declaración. Se
aprueban juntas o ninguna. Aprobar una sola deja el lenguaje declarando
genéricos con una sintaxis y usándolos con otra.

### 5.5 Recuperar `Option[T]` y `Result[T, E]`

**Coste: muy alto, y es el punto más grave de la lista.**

Los dos tipos **no existen por una decisión deliberada**. `syntax-revision-2.md`
§3 los eliminó junto con `try`, y §5.5 de la especificación los reemplazó por
`T?` con `null`, y por el par `-> (T, Error?)` con el operador postfijo `?` como
prueba de presencia. Con ellos se fueron `Some`, `None`, `Ok`, `Err`,
`is_some`, `is_ok`, `unwrap` y `unwrap_or`.

Eso no es una preferencia de escritura. Es el modelo de errores del lenguaje.
El ejemplo `09_absence_and_failure.science` existe para documentarlo, el
diagnóstico `SC0140` existe para vigilarlo, y `SC0155` existe para enseñar la
migración desde `try`.

**Lo que esta propuesta pide, en la práctica, es deshacer la revisión 2.** Puede
ser lo correcto — la propia nota reconoce en §3.3 que el modelo nuevo es casi el
doble de largo de escribir — pero es una revisión de sintaxis completa, no una
línea en una lista de migración, y necesita su propia nota que la argumente
contra la que la eliminó.

### 5.6 `import` en lugar de `use`

**Coste: bajo.** `import` ya está reservada y sin usar, así que la palabra está
disponible. `use` quedaría libre.

**En contra, y es pequeño pero real:** `use text.parser (Token, lex)` ya está en
los ejemplos y en `17_modules.science`, y `import` arrastra de Python la
expectativa de que exista un `from X import Y`, que en Science se escribe de
otra forma.

### 5.7 `range(n)` en lugar de `0..n`

**Coste: bajo, y probablemente sea un error de redacción más que una propuesta.**

Los ejemplos de la Parte I de este mismo documento escriben `for epoch in
range(10)`, pero `0..n` es la forma del lenguaje y `Range of T` es un tipo que
ya está implementado — hay un commit reciente que lo introdujo. Lo más probable
es que esas líneas se escribieran por inercia de Python. **Recomendación:
corregir los ejemplos, no el lenguaje.**

### 5.8 Dos requisitos de §3 que no son cambios de grafía

La lista original mezclaba las reversiones de arriba con dos cosas que son
peticiones reales y que sobreviven se decida lo que se decida, porque no
dependen de qué símbolo se elija.

**Los genéricos deben anidar.** El original lo pide así:

```
Array[Map[String, Int]]
Map[String, Array[Float]]
Array[Array[T]]
```

**Esto ya funciona** y no hace falta decidir nada: se escribe
`Array of (Map of (String, Int))`, y la regla de que dos o más argumentos llevan
paréntesis está en `AGENTS.md` §2 regla 3. Si la Parte II se aprueba, la
propiedad debe conservarse; si se rechaza, ya está.

**Hace falta una forma de escribir el tipo de una función.** El original la
escribe `Function[T, U]`, en `def map[T, U](items: &Array[T], f: Function[T, U])`.

Esto sí es un hueco real y **ya tiene dueño**: `collections-and-chains.md` §1.2
decidió la grafía `(A) -> B`, sin palabra clave, y el README de las notas lo
lista entre las peticiones pendientes con tres clientes esperándolo. Falta la
implementación, no la decisión — y hay un detalle que nadie había presupuestado:
`parse_type_bound` tiene que dejar de ser un *path*, o `where F: (A) -> B` no
parsea.

Queda además un cabo suelto que la nota reconoce: no hay forma de escribir un
tipo de función que sea **él mismo genérico**, que es justo lo que
`Function[T, U]` quiere decir aquí. Es una pregunta abierta, no una preferencia
de escritura.

---

## 6. La migración, si se aprueba

Lo que sigue es el plan de ejecución original, conservado tal cual. **Depende
por completo de que la Parte II se decida a favor**, y no debe ejecutarse antes.
Si sólo se aprueba 5.2, de esta lista sólo aplica la segunda línea.


Realizar una migración completa hacia la nueva sintaxis.

Reemplazar:

```text
borrowed T
```

por:

```text
&T
```

Reemplazar:

```text
mutable T
```

por la representación basada en:

```text
mut
```

Reemplazar:

```text
Array of T
```

por:

```text
Array[T]
```

Reemplazar:

```text
Map of (String, Int)
```

por:

```text
Map[String, Int]
```

Reemplazar:

```text
def foo of T(...)
```

por:

```text
def foo[T](...)
```

Mantener:

```text
Doc implements Summarize:
Doc has:
```

Actualizar todo lo necesario:

- lexer
- parser
- AST
- resolver
- type checker
- diagnostics
- formatter
- examples
- fixtures
- tests
- documentation

Agregar tests negativos para la sintaxis anterior.

Buscar usos residuales de:

```text
borrowed
mutable
Array of
Map of
 of T
```

y eliminarlos cuando correspondan a la sintaxis anterior.

---
---

# Parte III — Arquitectura del ecosistema

> Nada de esta parte contradice lo ya decidido. Rellena el espacio que la
> especificación deja fuera de F0 a propósito.

## 7. Organización del repositorio


La organización objetivo es:

```text
science/
│
├── compiler/
│
├── runtime/
│
├── std/
│
├── libs/
│
├── tools/
│
└── packages/
```

Conceptualmente:

```text
Science Compiler
      │
      ├── Core language
      ├── Runtime
      ├── FFI
      └── Package system
             │
             └── Science ecosystem
                    ├── core
                    ├── math
                    ├── tensor
                    ├── ml
                    ├── data
                    ├── plot
                    ├── net
                    ├── web
                    ├── agents
                    ├── security
                    └── tools
```

No asumir que todos estos módulos deben estar implementados desde el primer release.

La arquitectura debe permitir agregarlos progresivamente sin romper el lenguaje.

---

## 8. Capas y modelo de dependencias


Definir capas.

Una organización recomendada:

```text
                 ┌───────────────┐
                 │   Compiler    │
                 └───────┬───────┘
                         │
                 ┌───────▼───────┐
                 │    Runtime    │
                 └───────┬───────┘
                         │
       ┌─────────────────┼─────────────────┐
       │                 │                 │
     core             system            ffi
       │                 │                 │
       └──────────┬──────┴─────────────────┘
                  │
          ┌───────▼────────┐
          │ math/numerical │
          └───────┬────────┘
                  │
             ┌────▼─────┐
             │   ML     │
             └────┬─────┘
                  │
       ┌──────────┼──────────┐
       │          │          │
      data      agents      plot
       │          │
       └──────┬───┴─────┐
              │         │
             web       tools
```

Evitar ciclos de dependencias.

---

## 9. Abstracción de backends


Para que Science pueda crecer en CPU/GPU, se recomienda un sistema de backends.

Ejemplo:

```text
Tensor API
   │
   └── Backend API
       ├── CPU
       ├── BLAS
       ├── CUDA
       ├── ROCm
       └── Metal
```

Debe existir una abstracción común para:

- allocation
- kernels
- copy
- synchronize
- device
- dtype
- stream/event

Esto permitirá agregar hardware sin modificar la API pública de `tensor`.

---

## 10. Arquitectura de serialización


Definir temprano un sistema de serialization.

Debe soportar:

- JSON
- binary
- protobuf
- TOML/YAML si se consideran apropiados

El sistema debe integrarse con:

- structs
- enums/choices
- collections
- tensors
- models
- HTTP

---

## 11. Frontera entre Science y código nativo


Definir desde temprano:

- ABI
- symbol naming
- calling convention
- struct layout
- ownership across FFI
- error handling across FFI
- primitive type mapping

Un objetivo importante es que librerías externas puedan exponerse de manera segura a Science.

---
---

# Parte IV — El catálogo de librerías

> Es el grueso del documento: quince familias, cada una con qué es y de qué
> responde. El índice de abajo era, en el original, una tabla que aparecía
> **dos veces seguidas y palabra por palabra**; se conserva una sola vez y se
> convierte en lo que en realidad es — el índice de las secciones que vienen
> después.

## 12. Índice de librerías

| Familia | Librerías | Sección |
|---|---|---|
| Núcleo | `core`, `prelude`, `string`, `collections`, `mem` | [13](#13-núcleo) |
| Numérico | `math`, `complex`, `random`, `stats` | [14](#14-matemático-y-numérico) |
| Tensores | `tensor`, `ndarray`, broadcasting | [15](#15-arrays-y-tensores) |
| Álgebra lineal | `linalg` | [16](#16-álgebra-lineal) |
| Performance | `simd`, BLAS/LAPACK, GPU, `parallel` | [17](#17-performance) |
| ML | `autograd`, `nn`, `optim`, `model` | [18](#18-machine-learning) |
| Datos | `dataframe`, `csv`, `parquet`, `json`, `encoding` | [19](#19-datos) |
| Visualización | `plot` | [20](#20-visualización) |
| Sistema | `os`, `fs`, `path`, `io`, `time` | [21](#21-sistema-y-entorno) |
| Red | `net`, `http`, `async`, `thread`, `sync` | [22](#22-networking-y-concurrencia) |
| Interoperabilidad | `c-ffi`, `python-ffi` | [23](#23-interoperabilidad) |
| Seguridad | `hash`, `crypto`, `tls` | [24](#24-seguridad) |
| Testing | `test`, `bench` | [25](#25-testing-y-benchmarking) |
| Agentes | `llm`, `tools`, `agent-loop`, `memory`, y diez más | [26](#26-agentes) |
| Web/API | `http-server`, `router`, `middleware`, y cuatro más | [27](#27-framework-webapi) |

**Y una cosa que no es una librería y es innegociable:** el gestor de paquetes
propio. Sin él ninguna librería de terceros puede crecer alrededor del lenguaje,
y sin ecosistema de terceros el lenguaje nunca pasa de lo que escriba su propio
autor. Está en la [Parte V](#28-package-manager--scargo).

---

## 13. Núcleo


### 13.1 `core`

Responsabilidades:

- tipos primitivos
- Option
- Result
- errores fundamentales
- traits/interfaces básicos
- iteradores
- operaciones fundamentales

`core` debe ser una dependencia mínima y estable.

---

### 13.2 `prelude`

Exportaciones de uso frecuente.

Objetivo:

```science
import prelude
```

o una inclusión automática según la evolución del lenguaje.

Debe evitar imports excesivos para operaciones fundamentales.

---

### 13.3 `string`

Responsabilidades: `String`, UTF-8, slicing, búsqueda, concatenación, formateo,
parsing textual.

> **Decisión pendiente que bloquea la mitad de esto.** Hoy no hay concatenación
> de `String`: `"a" + "b"` reporta `SC0535` honestamente porque `Add` está
> declarada sin métodos a propósito — la aritmética numérica depende de que
> siga así para saltarse el despacho por método. Declarar `Add.add` sin
> resolver eso rompe el `+` de los números. Ver `STDLIB-DECISIONS.md`.

**UTF-8 es la decisión que ordena la API.** `length` devuelve **bytes**, no
caracteres, y eso tiene que estar en el nombre o en la nota de cada método que
lo toque; una API que finge que un `String` es una secuencia de caracteres es
la que produce el bug que aparece recién cuando alguien escribe en árabe.
Indexar por byte puede caer en medio de un punto de código: por eso el slicing
devuelve `(String)?` y no `String`.

```science
String has:
    def new() -> String
    def from_bytes(bytes: &Array[U8]) -> (String, Error?)

    # Medidas. Tres, porque son tres preguntas distintas.
    def length(self) -> Int                      # bytes
    def chars_count(self) -> Int                 # puntos de código — O(n)
    def is_empty(self) -> Bool

    # Construcción.
    def push_str(mutable self, other: &String)
    def push_char(mutable self, c: Char)
    def truncate(mutable self, bytes: Int)
    def clear(mutable self)
    def concat(self, other: &String) -> String

    # Búsqueda.
    def contains(self, needle: &String) -> Bool
    def starts_with(self, prefix: &String) -> Bool
    def ends_with(self, suffix: &String) -> Bool
    def find(self, needle: &String) -> Int?      # offset en bytes
    def rfind(self, needle: &String) -> Int?

    # Slicing. `null` si un extremo cae dentro de un punto de código.
    def slice(self, from: Int, to: Int) -> (String)?

    # Transformación.
    def trim(self) -> String
    def trim_start(self) -> String
    def trim_end(self) -> String
    def to_upper(self) -> String
    def to_lower(self) -> String
    def replace(self, from: &String, to: &String) -> String
    def repeat(self, times: Int) -> String

    # Partición.
    def split(self, sep: &String) -> Array[String]
    def lines(self) -> Array[String]
    def chars(self) -> Array[Char]
    def bytes(self) -> Array[U8]

    # Parsing. Par falible: el texto del usuario no es de fiar.
    def parse_int(self) -> (Int, Error?)
    def parse_float(self) -> (F64, Error?)
    def parse_bool(self) -> (Bool, Error?)
```

`join` no es método de `String` sino de `Array[String]`, porque el receptor
natural es la colección: `partes.join(", ")`.

**En código real** — parsear un archivo de configuración `clave = valor`:

```science
def parse_config(text: &String) -> (Map[String, String], Error?):
    let mutable out be Map.new()

    for line in text.lines():
        let trimmed be line.trim()

        # Comentarios y líneas en blanco.
        if trimmed.is_empty() or trimmed.starts_with("#"):
            continue

        let at be trimmed.find("=")
        if not at?:
            return (out, ParseError("línea sin `=`"))

        let key be trimmed.slice(0, at)
        let value be trimmed.slice(at + 1, trimmed.length())
        if not key? or not value?:
            return (out, ParseError("límite de carácter inválido"))

        out.insert(key.trim(), value.trim())

    (out, null)
```

Dos cosas que este ejemplo muestra a propósito: `find` devuelve `Int?` y hay
que probarlo, y `slice` devuelve `(String)?` porque un extremo puede caer en
medio de un punto de código UTF-8. Nada de eso es ceremonia — es el error que
aparece recién cuando alguien pone un acento en la clave.

---

### 13.4 `collections`

Mínimo: `Array`, `Map`, `Set`, `Queue`, `List`, y estructuras genéricas
adicionales cuando hagan falta. API consistente con el sistema de tipos.

```science
Array[Int]
Map[String, F64]
Set[String]
```

**El acceso devuelve `T?` y no `T`.** Es la decisión que más se nota al
escribir: `get` fuera de rango no entra en pánico, devuelve `null`, y el
lenguaje ya tiene `if x?:` para eso. El índice `a[i]` sí entra en pánico,
porque es la forma que dice "yo sé que está".

```science
Array[T] has:
    def new() -> Array[T]
    def with_capacity(n: Int) -> Array[T]
    def filled(value: T, n: Int) -> Array[T] where T: Clone

    def length(self) -> Int
    def is_empty(self) -> Bool
    def capacity(self) -> Int

    def get(self, i: Int) -> (&T)?
    def get_mut(mutable self, i: Int) -> (&mut T)?
    def first(self) -> (&T)?
    def last(self) -> (&T)?

    def push(mutable self, value: T)
    def pop(mutable self) -> T?
    def insert(mutable self, i: Int, value: T) -> Error?
    def remove(mutable self, i: Int) -> T?
    def clear(mutable self)
    def truncate(mutable self, n: Int)
    def extend(mutable self, other: Array[T])

    def contains(self, value: &T) -> Bool where T: Eq
    def index_of(self, value: &T) -> Int? where T: Eq
    def sort(mutable self) where T: Ord
    def sort_by(mutable self, less: (&T, &T) -> Bool)
    def reverse(mutable self)

    def slice(self, from: Int, to: Int) -> (&Array[T])?
    def concat(self, other: &Array[T]) -> Array[T] where T: Clone

    # La puerta a las cadenas. `Array` NO lleva `map`/`filter` propios:
    # esa vocabulario vive en la cadena. Ver abajo.
    def iterate(self) -> Iterate[Item = &T]

Array[String] has:
    def join(self, sep: &String) -> String
```

#### El vocabulario de cadenas, que es cerrado

`collections-and-chains.md` §1.4 manda sobre esto y **el conjunto es cerrado a
propósito**. Los nombres no son los de Rust ni los de Python, y las diferencias
son decisiones, no descuidos:

| En vez de | Science usa | Por qué |
|---|---|---|
| `filter(p)` | `keep(p)` **y** `discard(p)` | Un predicado negado es la verruga de legibilidad más común al filtrar |
| `filter_map` | `map` + `keep_some()` | Dos palabras honestas en vez de un combinador |
| `fold` | `reduce(inicial, f)` | Siempre con valor inicial: la variante sin él devuelve un nullable que nadie maneja |
| `min`/`max` | `minimum()`/`maximum()` | Sin abreviaturas (§4.3) |
| `step_by` | `every(n)` | Nombrado por lo que el usuario hace: un frame de cada diez |
| `dedup` | `unique()` / `unique(by:)` | — |
| `len()` sobre un stream | `count()` | `length()` es O(1); contar un stream lo consume. Dos nombres mantienen la diferencia visible |
| `chain` | `followed_by(other)` | — |
| `enumerate` | `numbered()` | — |
| `flat_map` | `expand(f)` | Fusionado, porque el intermedio nunca se quiere |

**Dos closures, y ninguna lleva flecha.** `each` nombra el sujeto sin
declararlo; `nombre giving expresión` lo declara cuando `each` no alcanza:

```science
def titulares(docs: &Array[Doc]) -> Array[String]:
    docs
        .iterate()
        .discard(each.is_empty())
        .map(each.title)
        .take(5)
        .collect()

def por_longitud(docs: &Array[Doc]) -> Array[String]:
    docs
        .iterate()
        .map(doc giving doc.summarize())
        .sorted(by: line giving line.length())
        .collect()
```

**Adaptadores propios del trabajo científico**, que justifican que el conjunto
cerrado los incluya en vez de dejarlos a una librería:

```science
# Minibatching: piezas disjuntas.
let lotes be muestras.iterate().batches(32).collect()

# Ventanas solapadas: medias móviles, lags, frames de espectrograma.
let medias be serie
    .iterate()
    .windows(20)
    .map(w giving w.iterate().sum() / 20.0)
    .collect()

# Top-k.
let mejores be candidatos
    .iterate()
    .sorted(by: each.score)
    .reverse()
    .take(10)
    .collect()

# Features junto a labels, y numeradas para reportar progreso.
let pares be x.iterate().zip(y.iterate()).numbered().collect()
```

**Las tres políticas de error tienen nombre en el sitio de la llamada**, que es
donde se decide qué hacer con una fila mala:

```science
let buenas be filas.iterate().keep_ok().collect()            # descartar
let todas, err be filas.iterate().collect_or_error()         # parar en la 1ª
let resultado be filas.iterate().partition_results()         # las dos mitades
```

**Laziness:** los adaptadores no hacen trabajo; los terminales corren la
cadena. `sorted()` y `reverse()` son barreras que bufferean, y están
documentadas como tales.

```science
Map[K, V] has:
    def new() -> Map[K, V]

    def length(self) -> Int
    def is_empty(self) -> Bool

    def get(self, key: &K) -> (&V)?
    def get_mut(mutable self, key: &K) -> (&mut V)?
    def contains_key(self, key: &K) -> Bool

    def insert(mutable self, key: K, value: V) -> V?   # el anterior, si había
    def remove(mutable self, key: &K) -> V?
    def clear(mutable self)

    # `or_insert` es el que evita el doble lookup del patrón
    # "si no está, ponelo". Sin él, todo el mundo escribe dos búsquedas.
    def or_insert(mutable self, key: K, default: V) -> &mut V

    def keys(self) -> Array[&K]
    def values(self) -> Array[&V]
    def entries(self) -> Array[(&K, &V)]

Set[T] has:
    def new() -> Set[T]
    def length(self) -> Int
    def is_empty(self) -> Bool
    def contains(self, value: &T) -> Bool
    def insert(mutable self, value: T) -> Bool     # true si no estaba
    def remove(mutable self, value: &T) -> Bool
    def union(self, other: &Set[T]) -> Set[T]
    def intersection(self, other: &Set[T]) -> Set[T]
    def difference(self, other: &Set[T]) -> Set[T]
    def is_subset_of(self, other: &Set[T]) -> Bool

Queue[T] has:
    def new() -> Queue[T]
    def length(self) -> Int
    def is_empty(self) -> Bool
    def push_back(mutable self, value: T)
    def push_front(mutable self, value: T)
    def pop_front(mutable self) -> T?
    def pop_back(mutable self) -> T?
    def peek_front(self) -> (&T)?
    def peek_back(self) -> (&T)?
```

**`Map` y `Set` necesitan `Hash` antes que nada de esto compile**, y `Hash` es
una de las trece interfaces del preludio que hoy son nombres sin métodos.

---

### 13.5 `mem`

Responsabilidades:

- alocación
- desaloación
- memoria de bajo nivel
- punteros/handles según el modelo definitivo
- operaciones explícitas de memoria cuando sean necesarias

No exponer primitivas peligrosas si pueden mantenerse encapsuladas.

---

## 14. Matemático y numérico


### 14.1 `math`

```science
math has:
    # Trigonometría.
    def sin(x: F64) -> F64
    def cos(x: F64) -> F64
    def tan(x: F64) -> F64
    def asin(x: F64) -> F64
    def acos(x: F64) -> F64
    def atan(x: F64) -> F64
    def atan2(y: F64, x: F64) -> F64
    def sinh(x: F64) -> F64
    def cosh(x: F64) -> F64
    def tanh(x: F64) -> F64

    # Exponencial y logaritmo.
    def exp(x: F64) -> F64
    def exp2(x: F64) -> F64
    def expm1(x: F64) -> F64            # exp(x)-1 preciso cerca de 0
    def log(x: F64) -> F64
    def log2(x: F64) -> F64
    def log10(x: F64) -> F64
    def log1p(x: F64) -> F64            # log(1+x) preciso cerca de 0
    def pow(x: F64, y: F64) -> F64
    def sqrt(x: F64) -> F64
    def cbrt(x: F64) -> F64
    def hypot(x: F64, y: F64) -> F64    # sin overflow intermedio

    # Redondeo y signo.
    def abs(x: F64) -> F64
    def floor(x: F64) -> F64
    def ceil(x: F64) -> F64
    def round(x: F64) -> F64
    def trunc(x: F64) -> F64
    def sign(x: F64) -> F64
    def copysign(x: F64, y: F64) -> F64

    # Clasificación. Un lenguaje científico las necesita explícitas.
    def is_nan(x: F64) -> Bool
    def is_infinite(x: F64) -> Bool
    def is_finite(x: F64) -> Bool

    # Especiales.
    def gamma(x: F64) -> F64
    def lgamma(x: F64) -> F64
    def erf(x: F64) -> F64
    def erfc(x: F64) -> F64
    def bessel_j(n: Int, x: F64) -> F64
    def bessel_y(n: Int, x: F64) -> F64
```

Constantes: `PI`, `TAU`, `E`, `SQRT_2`, `LN_2`, `LN_10`, `INFINITY`, `NAN`,
`EPSILON`.

**Las cuatro que la gente olvida y son la razón de tener una `math` propia.**
`expm1`, `log1p`, `hypot` y `cbrt` no son adornos: son las versiones
numéricamente estables de cosas que uno escribiría a mano y mal. `log(1+x)`
con `x = 1e-16` da `0`; `log1p(1e-16)` da `1e-16`. Un lenguaje que se presenta
como científico y obliga a su usuario a saber eso ya perdió el argumento.

**`atan2` es la primera prueba de la Decisión 2.3 sobre préstamos** —
`STDLIB-DECISIONS.md` la cita como el único precedente existente sobre si
`other` va prestado.

**En código real** — log-verosimilitud gaussiana, que es donde se ven las
cuatro funciones estables de arriba:

```science
def log_likelihood(x: &Array[F64], mu: F64, sigma: F64) -> F64:
    let n be x.length()
    let term be -0.5 * math.log(2.0 * math.PI) - math.log(sigma)

    let mutable total be 0.0
    for value in x:
        let z be (value - mu) / sigma
        total be total + term - 0.5 * z * z

    total

# Estable cerca de cero, que es donde importa.
def log_sum_exp(values: &Array[F64]) -> F64:
    let peak be values.iterate().maximum()
    if not peak?:
        return math.NEG_INFINITY

    let mutable acc be 0.0
    for v in values:
        acc be acc + math.exp(v - peak)

    peak + math.log(acc)
```

`log_sum_exp` es el patrón que justifica la librería entera: escrito
ingenuamente como `log(sum(exp(v)))` desborda con valores perfectamente
normales, y todo el mundo lo escribe ingenuamente la primera vez.

---

### 14.2 `complex`

Tipo complejo y operaciones:

```science
Complex
real
imag
abs
arg
conj
```

Debe integrarse con `math`, `tensor` y `linalg`.

---

### 14.3 `random`

Generadores aleatorios con API reproducible.

Debe soportar:

- uniforme
- normal
- poisson
- binomial
- otras distribuciones importantes

Ejemplo conceptual:

```science
let rng be random.Rng(seed: 42)

let x be rng.normal(
    mean: 0.0,
    std: 1.0,
    shape: [1000]
)
```

La reproducibilidad debe ser una propiedad explícita.

---

### 14.4 `stats`

Debe cubrir:

- media
- varianza
- desviación estándar
- covarianza
- correlación
- distribuciones
- intervalos de confianza
- t-test
- chi-cuadrado
- pruebas de hipótesis
- métricas estadísticas

No asumir que todo algoritmo estadístico debe formar parte de `core`.

---

## 15. Arrays y tensores


### 15.1 `tensor`

Una de las piezas centrales del ecosistema, y **la que bloquea la
interoperabilidad con Python** (ver [23.2](#232-python-ffi)).

> **Dos requisitos no negociables**, de `python-interop.md` §2. Son baratos si
> se respetan desde el primer commit y carísimos de retrofitear:
>
> 1. La cabecera de runtime de un tensor es **campo por campo** un `DLTensor`:
>    puntero a datos, tipo e id de dispositivo, rango, dtype, forma, strides,
>    offset en bytes.
> 2. Las variables de forma que no sean constantes de compilación son
>    **recuperables en runtime** desde esa cabecera.
>
> Diseñar el tensor sin mirar esto es garantizar que el día que se quiera
> hablar con NumPy haya que copiar cada array.

```science
Tensor[T, S] has:
    # Construcción.
    def zeros(shape: &Array[Int]) -> Tensor[T, S]
    def ones(shape: &Array[Int]) -> Tensor[T, S]
    def full(shape: &Array[Int], value: T) -> Tensor[T, S]
    def empty() -> Tensor[T, S]
    def from_array(data: Array[T], shape: &Array[Int]) -> (Tensor[T, S], Error?)
    def arange(start: T, stop: T, step: T) -> Tensor[T, S]
    def linspace(start: T, stop: T, n: Int) -> Tensor[T, S]
    def eye(n: Int) -> Tensor[T, S]

    # Metadatos — los que la membrana de Python lee de la cabecera.
    def shape(self) -> Array[Int]
    def strides(self) -> Array[Int]
    def rank(self) -> Int
    def size(self) -> Int
    def dtype(self) -> DType
    def device(self) -> Device
    def is_contiguous(self) -> Bool

    # Forma. Vistas donde se puede, copia sólo donde hay que.
    def reshape(self, shape: &Array[Int]) -> (Tensor[T, S], Error?)
    def transpose(self, axes: &Array[Int]) -> (Tensor[T, S], Error?)
    def permute(self, axes: &Array[Int]) -> (Tensor[T, S], Error?)
    def squeeze(self, axis: Int?) -> Tensor[T, S]
    def unsqueeze(self, axis: Int) -> Tensor[T, S]
    def broadcast_to(self, shape: &Array[Int]) -> (Tensor[T, S], Error?)
    def contiguous(self) -> Tensor[T, S]

    # Indexado y selección.
    def get(self, index: &Array[Int]) -> T?
    def slice(self, ranges: &Array[Range]) -> (Tensor[T, S], Error?)
    def gather(self, axis: Int, index: &Tensor[I64, S]) -> (Tensor[T, S], Error?)
    def scatter(mutable self, axis: Int, index: &Tensor[I64, S],
                src: &Tensor[T, S]) -> Error?
    def masked_select(self, mask: &Tensor[Bool, S]) -> Tensor[T, S]

    # Combinación.
    def concat(parts: &Array[Tensor[T, S]], axis: Int) -> (Tensor[T, S], Error?)
    def stack(parts: &Array[Tensor[T, S]], axis: Int) -> (Tensor[T, S], Error?)
    def split(self, sizes: &Array[Int], axis: Int) -> (Array[Tensor[T, S]], Error?)

    # Reducciones. `axis: null` reduce todo.
    def sum(self, axis: Int?, keep_dims: Bool) -> Tensor[T, S]
    def mean(self, axis: Int?, keep_dims: Bool) -> Tensor[T, S]
    def prod(self, axis: Int?, keep_dims: Bool) -> Tensor[T, S]
    def min(self, axis: Int?, keep_dims: Bool) -> Tensor[T, S]
    def max(self, axis: Int?, keep_dims: Bool) -> Tensor[T, S]
    def argmin(self, axis: Int?) -> Tensor[I64, S]
    def argmax(self, axis: Int?) -> Tensor[I64, S]
    def cumsum(self, axis: Int) -> Tensor[T, S]

    # Elemento a elemento.
    def map(self, f: (T) -> T) -> Tensor[T, S]
    def clamp(self, low: T, high: T) -> Tensor[T, S]
    def where(cond: &Tensor[Bool, S], a: &Tensor[T, S],
              b: &Tensor[T, S]) -> (Tensor[T, S], Error?)

    # Dispositivo.
    def to_device(self, device: Device) -> (Tensor[T, S], Error?)
    def to_dtype[U](self) -> Tensor[U, S]

    # El borde con Python — §4 de `python-interop.md`.
    def to_dlpack(self) -> PyObject
    def from_dlpack(capsule: &PyObject) -> (Tensor[T, S], Error?)
```

```science
let x be Tensor.ones([32, 784])
let y be Tensor.randn([784, 256])
let z be x @ y
```

**`@` es el producto matricial y es una interfaz de operador** (`MatMul`), no
una función. Hoy el backend lo rechaza por nombre: *"es una operación sobre
arrays enteros y la Decisión 5 la vuelve una llamada de runtime que no
existe"*.

**Por qué las reducciones llevan `keep_dims` y no dos métodos.** Es el
parámetro que decide si el resultado sigue siendo difundible contra el
original. NumPy lo tiene por la misma razón, y las librerías que lo omitieron
terminaron agregándolo con otro nombre.

**En código real** — normalización por columna y una pasada hacia adelante:

```science
use tensor (Tensor)

# Media y desvío por característica. `keep_dims: true` es lo que deja el
# resultado difundible contra `x` en la línea siguiente.
def standardize(x: &Tensor[F32, (n, d)]) -> (Tensor[F32, (n, d)], Error?):
    let mu be x.mean(axis: 0, keep_dims: true)
    let centered, err be x.sub(mu)
    if err?:
        return (Tensor.empty(), err)

    let var be centered.mul(centered).mean(axis: 0, keep_dims: true)
    let sd be var.add_scalar(1e-8).sqrt()

    centered.div(sd)

def forward(x: &Tensor[F32, (n, 784)],
            w1: &Tensor[F32, (784, 256)],
            b1: &Tensor[F32, (256)]) -> (Tensor[F32, (n, 256)], Error?):
    let z, err be linalg.matmul(x, w1)
    if err?:
        return (Tensor.empty(), err)

    # `b1` es (256,) y `z` es (n, 256): el broadcasting lo resuelve.
    let biased, err be z.add(b1)
    if err?:
        return (Tensor.empty(), err)

    (biased.clamp(0.0, math.INFINITY), null)   # ReLU
```

**Lo que muestra el ejemplo y no se ve en las firmas:** las formas están en el
tipo — `Tensor[F32, (n, 784)]` — así que `matmul` contra un `(784, 256)` es un
error de **compilación**, no un `ValueError` a los veinte minutos de
entrenamiento. Ese es el argumento entero del lenguaje aplicado a una capa
densa, y es lo que la membrana de Python convierte en **un** chequeo de
dimensión en la puerta.

---

### 15.2 `ndarray`

Puede existir como abstracción o compatibilidad cuando la separación sea útil.

Debe cubrir operaciones generales de arrays multidimensionales.

No duplicar innecesariamente funcionalidades de `tensor`.

La arquitectura debe poder permitir:

```text
ndarray
   │
   └── tensor
```

o fusionarlos si el diseño final lo justifica.

---

### 15.3 Broadcasting

Debe definirse claramente:

- reglas
- errores
- shape inference
- compatibilidad con autodiff
- backend CPU/GPU

Ejemplo:

```science
let x be tensor.ones([100, 3])
let bias be tensor.ones([3])

let y be x + bias
```

---

## 16. Álgebra lineal


### 16.1 `linalg`

No reimplementar desde cero algoritmos que puedan delegarse a BLAS/LAPACK u
otros backends maduros.

**Toda descomposición devuelve un par falible.** No es ceremonia: una matriz
singular, una no cuadrada o una que no converge son entradas *normales* en
trabajo científico, y una API que entra en pánico obliga a validar a mano antes
de cada llamada — que es exactamente el chequeo que la librería ya hizo.

```science
linalg has:
    # Productos.
    def matmul(a: &Tensor[T, S], b: &Tensor[T, S]) -> (Tensor[T, S], Error?)
    def dot(a: &Tensor[T, S], b: &Tensor[T, S]) -> (T, Error?)
    def outer(a: &Tensor[T, S], b: &Tensor[T, S]) -> Tensor[T, S]
    def cross(a: &Tensor[T, S], b: &Tensor[T, S]) -> (Tensor[T, S], Error?)
    def kron(a: &Tensor[T, S], b: &Tensor[T, S]) -> Tensor[T, S]

    # Normas y medidas.
    def norm(a: &Tensor[T, S], ord: Norm) -> F64
    def trace(a: &Tensor[T, S]) -> (T, Error?)
    def det(a: &Tensor[T, S]) -> (T, Error?)
    def slogdet(a: &Tensor[T, S]) -> (T, T, Error?)   # signo y log|det|
    def rank(a: &Tensor[T, S], tol: F64?) -> (Int, Error?)
    def cond(a: &Tensor[T, S]) -> (F64, Error?)

    # Resolución. `solve` antes que `inv`, y a propósito.
    def solve(a: &Tensor[T, S], b: &Tensor[T, S]) -> (Tensor[T, S], Error?)
    def lstsq(a: &Tensor[T, S], b: &Tensor[T, S]) -> (Tensor[T, S], Error?)
    def inv(a: &Tensor[T, S]) -> (Tensor[T, S], Error?)
    def pinv(a: &Tensor[T, S]) -> (Tensor[T, S], Error?)

    # Descomposiciones.
    def qr(a: &Tensor[T, S]) -> (Tensor[T, S], Tensor[T, S], Error?)
    def svd(a: &Tensor[T, S]) -> (Tensor[T, S], Tensor[T, S], Tensor[T, S], Error?)
    def lu(a: &Tensor[T, S]) -> (Tensor[T, S], Tensor[T, S], Tensor[T, S], Error?)
    def cholesky(a: &Tensor[T, S]) -> (Tensor[T, S], Error?)
    def eig(a: &Tensor[T, S]) -> (Tensor[T, S], Tensor[T, S], Error?)
    def eigh(a: &Tensor[T, S]) -> (Tensor[T, S], Tensor[T, S], Error?)

choice Norm:
    Frobenius
    Nuclear
    L1
    L2
    LInf
    P(F64)
```

```science
let U, S, Vt, err be linalg.svd(A)
if err?:
    return (Tensor.empty(), err)
```

**`solve` existe para que nadie escriba `inv(A) @ b`.** Es más lento, es menos
preciso, y es lo primero que hace todo el que viene de la notación matemática.
Que `solve` esté antes en la lista y que `inv` lleve una nota diciéndolo es
diseño de API, no documentación.

**`slogdet` acompaña a `det` por la misma razón que `log1p` acompaña a `log`:**
el determinante de una matriz grande desborda un `F64` mucho antes de que su
logaritmo moleste a nadie, y la verosimilitud gaussiana —que es donde aparece—
siempre quiere el logaritmo.

**`eigh` no es una optimización de `eig`.** Para matrices simétricas garantiza
autovalores reales y ordenados; `eig` sobre la misma matriz devuelve complejos
con error de redondeo y sin orden. Elegir mal es un bug silencioso.

**En código real** — PCA, que usa tres de las decisiones de arriba a la vez:

```science
use linalg
use tensor (Tensor)

def pca(x: &Tensor[F32, (n, d)], k: Int) -> (Tensor[F32, (n, k)], Error?):
    let centered, err be standardize(x)
    if err?:
        return (Tensor.empty(), err)

    # `eigh` y no `eig`: la covarianza es simétrica por construcción, y
    # sólo `eigh` garantiza autovalores reales y ordenados.
    let cov, err be linalg.matmul(centered.transpose([1, 0]), centered)
    if err?:
        return (Tensor.empty(), err)

    let values, vectors, err be linalg.eigh(cov.div_scalar(n - 1))
    if err?:
        return (Tensor.empty(), err)

    let top, err be vectors.slice([Range.all(), Range.to(k)])
    if err?:
        return (Tensor.empty(), err)

    linalg.matmul(centered, top)

# Ajuste por mínimos cuadrados. `lstsq`, no `inv`.
def fit(a: &Tensor[F64, (n, d)], b: &Tensor[F64, (n)]) -> (Tensor[F64, (d)], Error?):
    linalg.lstsq(a, b)
```

**Lo que este ejemplo evita, y es el punto:** nadie escribió `inv(A) @ b`, y
nadie llamó a `eig` sobre una matriz simétrica. Las dos son las trampas que
todo el que llega desde la notación matemática pisa la primera vez, y las dos
se evitan porque la API puso el nombre correcto más a mano que el incorrecto.
Eso es diseño de API, no documentación.

---

## 17. Performance


### 17.1 `simd`

Exponer operaciones SIMD de forma segura cuando sea apropiado.

Debe existir una abstracción suficientemente estable para que librerías como `tensor` puedan aprovechar SIMD sin duplicar APIs.

---

### 17.2 BLAS/LAPACK

Crear bindings sólidos para:

- BLAS
- LAPACK

La implementación de `linalg` debe poder elegir estos backends.

Objetivo:

```text
linalg
   │
   ├── native
   ├── BLAS
   └── LAPACK
```

---

### 17.3 GPU

Diseñar una capa de backends:

```text
tensor
  │
  └── backend
       ├── cpu
       ├── cuda
       ├── rocm
       └── metal
```

No acoplar `tensor` directamente a CUDA.

Ejemplo conceptual:

```science
let x be tensor.randn(
    [1024, 1024],
    device: gpu
)
```

El backend debe decidir el mecanismo concreto.

---

### 17.4 `parallel`

Debe permitir paralelización de operaciones y loops sin requerir manejo manual de threads para casos comunes.

Ejemplo conceptual:

```science
parallel for item in items:
    process(item)
```

Debe integrarse con el modelo de memoria y concurrencia de Science.

---

## 18. Machine learning


### 18.1 `autograd`

Autodiferenciación como capacidad central de ML.

Debe permitir:

```science
let loss be f(weights)
let grad be autograd.grad(loss, weights)
```

Objetivos:

- reverse-mode autodiff
- forward-mode cuando tenga sentido
- graph tracking
- gradient accumulation
- detach/no-grad
- integración con tensor
- compatibilidad CPU/GPU

---

### 18.2 `nn`

Capas y componentes:

- Linear
- Conv1D
- Conv2D
- Conv3D
- activations
- normalization
- dropout
- pooling
- recurrent modules
- attention
- transformer building blocks
- Sequential
- Module
- parameter management

Ejemplo:

```science
let model be nn.Sequential([
    nn.Linear(784, 256),
    nn.ReLU(),
    nn.Linear(256, 10)
])
```

---

### 18.3 `optim`

Debe incluir:

- SGD
- Adam
- AdamW
- RMSprop
- momentum
- learning-rate schedulers

Ejemplo:

```science
let optimizer be optim.Adam(
    model.parameters(),
    lr: 0.001
)
```

---

### 18.4 `model`

Capa de serialización y gestión de modelos.

Debe poder almacenar:

- arquitectura
- parámetros
- dtype
- device
- versión
- metadata
- información de entrenamiento cuando corresponda

Ejemplo:

```science
model.save(model, "model.sci")
let loaded be model.load("model.sci")
```

---

## 19. Datos


### 19.1 `dataframe`

Abstracción tabular.

Debe soportar:

- columns
- rows
- filter
- select
- sort
- group
- aggregation
- joins
- missing values
- conversiones con tensor

Ejemplo:

```science
let df be dataframe.read_csv("train.csv")

let clean be df
    .filter(col("age") > 18)
    .select(["age", "income", "label"])
```

---

### 19.2 `csv`

Lectura/escritura robusta.

---

### 19.3 `parquet`

Soporte eficiente para datasets científicos.

---

### 19.4 `json`

Parseo y serialización.

Debe integrarse con estructuras de datos de Science.

---

### 19.5 `encoding`

Debe incluir:

- Base64
- Hex
- UTF-8
- UTF-16
- conversiones binarias

---

## 20. Visualización


### `plot`

Debe cubrir como mínimo:

- line plots
- scatter
- histogram
- bar
- heatmap
- axis
- labels
- legends
- exportación de imágenes

API objetivo:

```science
plot.line(x, y)
plot.scatter(x, y)
plot.histogram(data)

plot.save("result.png")
plot.show()
```

Debe existir una separación clara entre plotting, rendering y datos.

---

## 21. Sistema y entorno


### `os`

Debe incluir:

- environment variables
- arguments
- process information
- process spawning
- signals donde sea apropiado

---

### `fs`

Debe incluir:

- archivos
- directorios
- lectura
- escritura
- metadata
- permisos cuando sea seguro y necesario

---

### `path`

API cross-platform.

Debe evitar concatenaciones manuales de paths.

---

### `io`

Debe incluir:

- stdin
- stdout
- stderr
- streams
- buffers
- readers
- writers

---

### `time` / `datetime`

Debe cubrir:

- timestamp
- duration
- timezone
- parsing
- formatting
- monotonic clocks
- measurement de performance

---

## 22. Networking y concurrencia


### `net`

Debe cubrir:

- sockets
- TCP
- UDP

---

### `http`

Cliente HTTP.

Debe soportar:

- GET
- POST
- headers
- body
- JSON
- streaming
- TLS integration

---

### `async`

Modelo de async/await.

Debe integrarse con el runtime.

---

### `thread`

Debe permitir creación y coordinación de threads.

---

### `sync`

Debe incluir:

- mutex/locks
- atomics
- semaphores
- synchronization primitives
- channels según diseño

---

## 23. Interoperabilidad


### 23.1 `c-ffi`

Esto es esencial.

Debe permitir usar:

- C
- librerías nativas
- BLAS
- LAPACK
- CUDA y otras APIs cuando sea viable

Objetivo:

```text
Science
  ↓
C ABI
  ↓
Native ecosystem
```

No reimplementar desde cero librerías maduras si pueden ser consumidas mediante FFI.

---

### 23.2 `python-ffi`

> **Estado.** Diseñado en detalle y **no empezado**. Las notas
> `python-interop.md` (1453 líneas) y `python-from-science.md` (1405) deciden
> esto entero; el compilador hoy no tiene `SC0455`, no reserva la palabra
> `python`, y no existe `Tensor` ni nada de DLPack. Lo que sigue resume esas
> notas y **manda sobre este documento** cuando difieran.

#### Por qué esta librería decide la adopción

La tesis está en `python-interop.md` §1 y conviene no suavizarla:

> *Nadie adopta un lenguaje científico entero. Lo adoptan una función caliente
> por vez, desde adentro del programa Python que ya tienen.*

De ahí sale el criterio de éxito, que es medible: **una persona con un programa
NumPy que funciona reemplaza una función por una de Science en una tarde, deja
todo lo demás igual, y obtiene la velocidad sin una copia.**

Eso implica que la dirección principal **no** es la que primero se le ocurre a
uno. Son dos, y tienen prioridades distintas:

| | Dirección | Para qué | Prioridad |
|---|---|---|---|
| **A** | Science llamado **desde** Python | Adopción. Reemplazar una función caliente | **Primera** |
| **B** | Python llamado **desde** Science | Alcance. Llegar a pandas, sklearn, astropy | Segunda |

La arquitectura debe evitar convertir Science en un wrapper de Python: la
interoperabilidad es una puerta de entrada, no el núcleo del diseño.

#### El principio que gobierna el borde

> **El límite es donde cada garantía estática se convierte en exactamente una
> verificación dinámica.**

Adentro de la función Science no se re-verifica nada que el compilador ya
probó; afuera rigen las reglas de Python. La membrana es fina, es generada, y
el usuario no la escribe.

| Garantía del lado Science | En qué se convierte en el borde |
|---|---|
| Tipos estáticos en la firma | Una conversión por argumento; `TypeError` al fallar |
| Formas estáticas | Un chequeo de dimensión por variable de forma; `ValueError` |
| `Error?` final en retorno múltiple | Devuelve `T`, o lanza una excepción generada |
| `T?` | Devuelve `T`, o `None` |
| Propiedad de un buffer | Un *deleter* DLPack que libera por el dueño correcto |
| Región de un préstamo | Una referencia fuerte al objeto Python que la enraíza |
| `&mut T` | Un contador de exportación verificado en la membrana |
| "Esta función no toca Python" | Suelta el GIL durante la llamada |
| `panic` | Desenrolla hasta el shim y lanza `SciencePanic` |

#### Dirección A — Science desde Python

Se marca una función como exportada y el build produce un módulo de extensión
de CPython importable. Los arrays cruzan **sin copia**, por DLPack y el
protocolo de buffers, en las dos direcciones.

```python
import mimodulo, numpy as np

puntos = np.random.rand(100_000, 128).astype(np.float32)
salida = mimodulo.reducir(puntos)     # sin copia; el GIL se suelta adentro
```

#### Dirección B — Python desde Science

Es la que alcanza las librerías **sin API de C**, que son casi todo el Python
científico por encima de la capa de arrays: pandas, scikit-learn, astropy,
matplotlib.

```science
use python "numpy" as numpy
use python "sklearn.decomposition" (PCA)

def reducir(puntos: &Tensor[F32, (n, d)]) -> (Tensor[F32, (n, 8)], Error?):
    let modelo, err be PCA(n_components: 8)
    if err?:
        return (Tensor.empty(), err)

    let ajustado, err be modelo.fit_transform(puntos)
    if err?:
        return (Tensor.empty(), err)

    return (ajustado.into_tensor(), null)
```

El diseño es deliberadamente mínimo, y cada punto es una decisión:

- **Un solo tipo opaco, `PyObject`**, con las interfaces `FromPython` y
  `ToPython` para convertir. **No se intenta tipar Python.**
- **Toda llamada falible devuelve un par** `-> (PyObject, PyError?)`, y quien
  llama prueba el error donde ocurre.
- **`PyError implements Error`.** Una función declarada `-> (T, Error?)` acepta
  una falla de Python sin conversión escrita en el retorno. Una función que
  puede fallar en Python y en Science tiene **un solo tipo de error y un solo
  chequeo**.
- El acceso a atributos y las llamadas van por el protocolo dinámico.

**Métodos del tipo `PyObject`:**

```science
PyObject has:
    def attr(self, name: &String) -> (PyObject, PyError?)
    def set_attr(mutable self, name: &String, value: &PyObject) -> PyError?
    def call(self, args: &Array[PyObject]) -> (PyObject, PyError?)
    def call_kw(self, args: &Array[PyObject],
                kw: &Map[String, PyObject]) -> (PyObject, PyError?)
    def index(self, key: &PyObject) -> (PyObject, PyError?)
    def len(self) -> (Int, PyError?)
    def iter(self) -> (PyIter, PyError?)
    def is_none(self) -> Bool
    def type_name(self) -> String

interface FromPython:
    def from_python(value: &PyObject) -> (Self, PyError?)

interface ToPython:
    def to_python(self) -> (PyObject, PyError?)
```

#### El fallo de esta dirección, dicho antes que sus virtudes

`python-interop.md` §6.1 los enumera y ninguno es menor:

- **Un binario Science que embebe Python deja de ser autocontenido.** Necesita
  un intérprete, de versión compatible, con los paquetes correctos,
  encontrable en tiempo de ejecución. *La historia de despliegue pasa a ser la
  de Python, que es justo de lo que los usuarios vienen huyendo.*
- **Todo lo que viene de Python es dinámico.** Un `PyObject` es `any`. Cada
  llamada a Python es un agujero en la historia de verificación, que es
  exactamente la apuesta del lenguaje.
- **El GIL se sostiene durante toda la llamada.** Una llamada a Python en un
  bucle caliente serializa el programa.
- **Los errores también se vuelven dinámicos**: un `PyError` con nombre de tipo
  y mensaje, no un `choice` contra el que el compilador pueda verificar un
  `match`.

#### Hosted y standalone, que no reciben la misma respuesta

- **Hosted** — el código Science se compiló a un módulo de extensión y corre
  dentro de un proceso Python que ya existe. Todos los costos de arriba salvo
  los dinámicos **se evaporan**: el intérprete ya está inicializado, los
  paquetes ya están instalados, el despliegue ya es el de Python. Llamar a
  Python acá es ordinario y está permitido.
- **Standalone** — un binario AOT que arranca su propio intérprete. Permitido
  **sólo con opt-in explícito**.

> **Decisión 10** (`python-from-science.md` §4.5). `use python` se permite en
> un build standalone detrás de un opt-in por programa, y el binario declara
> que no es autocontenido. Es propiedad del paquete **raíz** únicamente: una
> dependencia no puede meter a un programa en embeber un intérprete.
>
> **`SC0455` se redefinió**: ya no significa *"`use python` en standalone"*,
> una prohibición, sino *"`use python` en standalone sin tabla `[python]`"*,
> una declaración faltante.

```toml
[python]
embed    = true
version  = ">= 3.11, < 3.14"
isolated = true

[python.requires]
numpy        = ">= 1.26"
scikit-learn = ">= 1.4, < 2"
```

El veto original se revisó por un argumento que vale la pena conservar: tres de
sus cuatro razones las terminó pagando otra nota para otro propósito, y *una
prohibición cuyas razones declaradas ya fueron pagadas por otro debería
revisarse*.

**Los bloques `python { … }` en línea son esta misma decisión con otra grafía**
y se responden en `syntax-revision-2.md` §8.3–§8.5, que además objeta algo que
esta sección no: código extranjero dentro de un `.science` es invisible para
`black`, `mypy` y `pytest`, y obliga al formateador y al servidor de lenguaje
de Science a entender tres lenguajes o a rendirse dentro de esas regiones. La
conclusión de allá es un directorio `foreign/*.py`, con la forma en línea como
azúcar si alguna vez se quiere.

#### La cadena de dependencias — qué tiene que existir antes

Esto es lo que convierte la sección en un plan y no en un deseo. `use python
"numpy"` no se puede empezar hasta que exista lo de abajo, **en este orden**:

| # | Pieza | Por qué la bloquea | Estado |
|---|---|---|---|
| 1 | Backend completo de F0 | Hoy no se puede ni imprimir un tipo de usuario | 15/20 ejemplos |
| 2 | `Formatter` / `Display` | Sin esto no hay mensaje de error legible en la membrana | Decidido, sin implementar |
| 3 | ABI de C exportable | **El shim de Python se escribe contra él**, no contra el backend | `extern "C"` anda |
| 4 | `Tensor` con cabecera DLPack | Requisito 1 de `python-interop.md` §2 | No existe |
| 5 | Variables de forma recuperables en runtime | Requisito 2: la membrana chequea la forma una vez | No existe |
| 6 | Sistema de efectos | Decide **dónde se suelta el GIL** (§5.2) | No existe |
| 7 | Gestor de paquetes | `[python.requires]` no significa nada sin él | Diseñado |
| 8 | Dirección A — módulo de extensión | Es la que justifica todo el resto | — |
| 9 | Dirección B — `use python` hosted | Casi gratis una vez que A existe | — |
| 10 | Dirección B — standalone + Decisión 10 | Descubrimiento de intérprete, venvs, empaquetado | — |

Los dos requisitos que la nota le pone al tipo `Tensor` son exactos y conviene
tenerlos a la vista al diseñarlo, porque son baratos si se respetan desde el
principio y carísimos de retrofitear:

1. La cabecera de runtime de un tensor es **campo por campo** un `DLTensor`:
   puntero a datos, tipo y id de dispositivo, rango, dtype, forma, strides,
   offset en bytes.
2. Las variables de forma que no sean constantes de compilación son
   **recuperables en runtime** desde esa cabecera.

#### Las dos direcciones, en un programa completo

Este es el caso de adopción entero: un programa NumPy que existe, una función
caliente reemplazada, y una llamada de vuelta a scikit-learn desde adentro.

**El lado Science** (`acelerado.science`):

```science
use python "sklearn.decomposition" (PCA)
use tensor (Tensor)
use linalg

# `export` marca lo que sale al módulo de extensión.
export def distancias(puntos: &Tensor[F32, (n, d)],
                      centro: &Tensor[F32, (d)]) -> (Tensor[F32, (n)], Error?):
    let delta, err be puntos.sub(centro)
    if err?:
        return (Tensor.empty(), err)

    (delta.mul(delta).sum(axis: 1, keep_dims: false).sqrt(), null)

# Esta llama a Python desde Science: sólo legal en hosted, o con
# `[python] embed = true` en `science.toml`.
export def reducir(puntos: &Tensor[F32, (n, d)]) -> (Tensor[F32, (n, 8)], Error?):
    let modelo, err be PCA(n_components: 8)
    if err?:
        return (Tensor.empty(), err)

    let ajustado, err be modelo.fit_transform(puntos)
    if err?:
        return (Tensor.empty(), err)

    ajustado.into_tensor()
```

**El lado Python**, que es el programa que ya existía:

```python
import numpy as np
import acelerado                      # el módulo compilado desde .science

puntos = np.random.rand(1_000_000, 128).astype(np.float32)
centro = puntos.mean(axis=0)

d = acelerado.distancias(puntos, centro)   # sin copia; suelta el GIL
print(d.shape, d.dtype)                    # (1000000,) float32

try:
    z = acelerado.reducir(puntos)
except ValueError as e:                    # la forma no cuadró en la puerta
    print("forma inválida:", e)
```

**Qué pasó en cada línea del borde, que es todo el diseño en un párrafo.**
`puntos` entró por DLPack sin copiarse; su forma `(1_000_000, 128)` se
verificó **una vez** contra `(n, d)` y ligó `n` y `d`; adentro de `distancias`
no se verificó nada más porque el compilador ya lo probó; el GIL se soltó
porque esa función no toca Python; el `Error?` del retorno se convirtió en la
excepción que el `except` atrapa; y el array que volvió comparte el buffer, con
un *deleter* que libera por el dueño correcto.

`reducir` es distinta en una sola cosa: lleva el efecto `python`, así que **no**
suelta el GIL y no puede ir en una región paralela.

---

**Fuera de alcance, y dicho para que nadie lo asuma:** R, Julia, MATLAB y la
JVM. Y el tipo tensor en sí, el sistema de efectos, los targets de GPU y el
gestor de paquetes son de notas hermanas; esta sección declara lo que necesita
de ellas y lo marca como dependencia en vez de diseñarlo.

---

## 24. Seguridad


### `hash`

Debe cubrir hashes usados para:

- integridad
- identificación
- verificación de datasets
- cachés
- artefactos

---

### `crypto`

Primitivas criptográficas maduras y bien auditadas cuando sea posible.

No inventar algoritmos criptográficos propios.

---

### `tls`

Integración con TLS para HTTP y networking.

---

## 25. Testing y benchmarking


### `test`

Debe estar integrado al ecosistema.

Ejemplo:

```science
test "tensor addition":
    let a be tensor.ones([2, 2])
    let b be tensor.ones([2, 2])

    assert a + b == tensor.full([2, 2], 2.0)
```

Debe soportar:

- assertions
- fixtures
- setup/teardown
- parametrización
- tests de error
- integración

---

### `bench`

Debe permitir medir performance.

Ejemplo conceptual:

```science
bench "matrix multiplication":
    linalg.matmul(A, B)
```

Debe producir métricas útiles.

---

## 26. Agentes


### `agents`

Debe ser una librería independiente de HTTP.

Un agente puede ejecutarse como:

- CLI
- proceso batch
- worker
- servicio
- componente embebido
- API HTTP
- WebSocket
- gRPC

No acoplar agentes al framework web.

---

### 26.1 `llm`

Cliente unificado para modelos.

Debe abstraer proveedores locales y remotos.

Debe contemplar:

- prompts
- responses
- streaming
- model configuration
- provider abstraction

---

### 26.2 `tools`

Sistema de herramientas tipadas.

Una herramienta debe tener:

- nombre
- descripción
- schema de argumentos
- función ejecutable
- resultado tipado cuando sea posible

---

### 26.3 `agent-loop`

Motor de ejecución:

```text
observe
   ↓
reason
   ↓
act
   ↓
observe
   ↓
...
```

Debe permitir estrategias como:

- ReAct
- plan/execute
- reflection
- custom orchestration

La arquitectura no debe obligar a una única estrategia.

---

### 26.4 `memory`

Memoria de:

- corto plazo
- largo plazo
- sesión
- almacenamiento persistente

---

### 26.5 `prompt`

Debe incluir:

- templates
- variables
- few-shot examples
- versioning
- composición

---

### 26.6 `embeddings`

Debe permitir:

- generar embeddings
- comparar vectores
- almacenar embeddings
- batch processing

---

### 26.7 `vector-db`

Cliente/abstracción para índices vectoriales.

Debe permitir:

- insert
- search
- filter
- metadata
- delete

---

### 26.8 `chunking`

División de documentos en fragmentos.

Debe contemplar:

- size
- overlap
- semantic chunking
- metadata preservation

---

### 26.9 `tokenizer`

Debe permitir:

- tokenize
- detokenize cuando sea posible
- token counts
- limits
- truncation

---

### 26.10 `multi-agent`

Coordinación entre agentes.

Debe soportar roles sin imponer una arquitectura única.

---

### 26.11 `mcp` / `protocol`

Soporte para protocolos estándar de conexión entre agentes y herramientas.

No acoplarlo al módulo HTTP.

---

### 26.12 `sandbox`

Aislamiento para ejecución de acciones o código generado.

Debe tratarse como componente de seguridad crítica.

---

### 26.13 `trace` / `eval`

Observabilidad de agentes:

- traces
- events
- tool calls
- latency
- token usage
- evaluation metrics

---

### 26.14 `guardrails`

Validación de:

- outputs
- tool arguments
- tool results
- schemas
- límites
- políticas definidas por la aplicación

---

## 27. Framework Web/API


Debe ser independiente del sistema de agentes.

Modelo:

```text
agents
    │
    └──── puede ser usado por ────> http
```

No:

```text
http
   └── obliga a usar agents
```

---

### 27.1 `http-server`

Servidor HTTP base.

---

### 27.2 `router`

Routing:

```text
GET /users/:id
POST /chat
DELETE /items/:id
```

Debe mantener el diseño simple y componible.

---

### 27.3 `middleware`

Capas como:

- auth
- logging
- CORS
- rate limiting
- tracing
- compression

Debe ser componible.

---

### 27.4 `serde`-like

Serialización/deserialización:

```text
struct <-> JSON
struct <-> binary
struct <-> protobuf
```

Debe integrarse profundamente con el type system.

---

### 27.5 `orm` / `db-driver`

Conectores a bases de datos.

No convertir esto inmediatamente en un ORM gigante.

Primero una interfaz común y drivers sólidos.

---

### 27.6 `websocket`

Comunicación bidireccional.

Importante para:

- streaming LLM
- dashboards
- realtime apps

---

### 27.7 `grpc` / `protobuf`

RPC binario tipado.

Especialmente útil para:

- microservicios
- inference servers
- sistemas distribuidos

### 27.1 Por qué `agents` y el framework web son dos piezas y no una

Esta discusión estaba suelta al principio del documento, antes del plan maestro,
y pertenece aquí: es la decisión de diseño que explica por qué las secciones 26
y 27 están separadas en lugar de ser una sola.


### Librería `agents` — qué y para qué

| Módulo | Qué es | Para qué sirve |
|---|---|---|
| `llm` | Cliente unificado para hablar con modelos (locales o por API) | Enviar prompts, recibir respuestas/streaming, abstraer distintos proveedores detrás de una sola interfaz |
| `tools` | Sistema para declarar funciones invocables por el modelo, con schema tipado de argumentos | Es lo que convierte un chatbot en agente: le da la capacidad de *actuar*, no solo responder texto |
| `agent-loop` | Motor de orquestación (ReAct, plan-execute, reflexión) | Implementa el ciclo pensar → actuar → observar → repetir sin que el usuario lo programe a mano cada vez |
| `memory` | Almacenamiento de contexto de corto/largo plazo | Que el agente recuerde interacciones previas dentro o entre sesiones |
| `prompt` | Templates, formateo, few-shot examples | Construir prompts de forma programática, versionable y reusable |
| `embeddings` | Generación y manejo de vectores semánticos | Base para búsqueda semántica y RAG |
| `vector-db` | Cliente o implementación de índice vectorial | Guardar/buscar embeddings — le da "memoria de conocimiento" al agente |
| `chunking` | División de documentos en fragmentos manejables | Preparar texto largo para embeddings sin perder coherencia semántica |
| `tokenizer` | Conteo y división de tokens | Controlar cuánto contexto ocupa un prompt, evitar cortes o costos excesivos |
| `multi-agent` | Coordinación entre agentes con roles distintos | Sistemas tipo "uno planea, otro ejecuta, otro revisa" |
| `mcp`/`protocol` | Soporte a protocolos estándar de conexión agente-herramienta | Interoperar con herramientas externas sin reinventar cada integración |
| `sandbox` | Ejecución aislada de código/acciones generadas por el agente | Seguridad: que el agente no pueda dañar el sistema al ejecutar algo propio |
| `trace`/`eval` | Logging estructurado + evaluación de calidad de respuestas | Debuggear decisiones del agente, medir si está funcionando bien |
| `guardrails` | Validación de outputs, límites de comportamiento | Confiabilidad antes de dejar actuar al agente en el mundo real |

### Librería REST/API — qué y para qué

| Módulo | Qué es | Para qué sirve |
|---|---|---|
| `http-server` | Capa sobre tu `net`/`http` de bajo nivel: routing, request/response, listen | El punto de entrada: levantar un servidor sin manejar sockets a mano |
| `router` | Sistema de rutas (`GET /users/:id`) | Mapear URLs y métodos HTTP a funciones handler |
| `middleware` | Auth, logging, CORS, rate-limiting como capas componibles | Reusar lógica transversal sin repetirla en cada endpoint |
| `serde`-like | Serialización/deserialización struct ↔ JSON (o protobuf) | Que un endpoint reciba/devuelva datos tipados sin parseo manual |
| `orm`/`db-driver` | Conexión a bases de datos | Persistencia — casi todo backend real la necesita |
| `websocket` | Comunicación bidireccional en tiempo real | Streaming de respuestas del agente/modelo, dashboards en vivo |
| `grpc`/`protobuf` | RPC binario tipado | Comunicación rápida entre microservicios, muy usado sirviendo modelos |

### Ahora tu pregunta de diseño: ¿`agents` como librería propia, y la API como framework tipo Express?

**Sí a las dos, y es la separación correcta.** Te explico el razonamiento:

**`agents` como librería independiente (no como parte del framework HTTP):**
Un agente no siempre corre detrás de una API REST — puede correr como CLI, como script batch, como worker que consume una cola, embebido en otro programa. Si acoplás `agents` al framework HTTP, obligás a todo el que quiera un agente simple a cargar con routing, middlewares, etc. que no necesita. Mejor que `agents` sea agnóstico de transporte, y que el framework HTTP simplemente *use* `agents` cuando alguien quiere exponerlo como API.

**El framework HTTP, estilo Express (no un framework "todo incluido" tipo Rails/Django):**
Para tu caso tiene más sentido un enfoque minimalista y componible (Express, Flask, Actix-web) que uno "baterías incluidas" (Rails, Django). Razones concretas:

- Tu audiencia inicial (gente de ML/ciencia de datos) no quiere aprender convenciones pesadas de un framework monolítico — quiere levantar un endpoint para servir un modelo o un agente en pocas líneas.
- Un estilo Express te deja crecer orgánicamente: empezás con `http-server` + `router` mínimo, y la gente va sumando `middleware`, `orm`, `websocket` solo si los necesita.
- Los frameworks "todo incluido" tardan mucho más en madurar bien (Rails/Django tienen décadas de trabajo), y con pocos recursos es más realista clavar bien una capa delgada y sólida.

**Cómo se conectan las dos piezas en la práctica**, algo como:

```
import agents
import http

let agent = agents.Agent(tools: [buscar_clima, calcular])

let app = http.App()
app.post("/chat", (req, res) => {
    let response = agent.run(req.body.message)
    res.json(response)
})

app.listen(3000)
```

`agents` no sabe nada de HTTP. `http` no sabe nada de agentes. Se combinan en el código del usuario. Esa independencia es la que te va a permitir, más adelante, ofrecer también `agents` corriendo sobre WebSocket, sobre gRPC, o como binario standalone, sin tocar una línea de la librería de agentes.

---
---

# Parte V — Toolchain

## 28. Package manager — `scargo`

> **Nota de contraste.** `package-manager.md` ya decidió esta pieza para el
> proyecto real: el manifiesto es `science.toml`, la resolución es por selección
> mínima de versiones, el lockfile es direccionable por contenido, no hay scripts
> de build y los diagnósticos van al espacio `SP`. El nombre `scargo` y lo que
> sigue son de este documento; donde los dos hablen de lo mismo, manda la nota.


Esto debe existir como pieza central del ecosistema.

Comandos objetivo:

```bash
scargo new my_project
scargo add tensor
scargo add agents
scargo add http
scargo build
scargo run
scargo test
scargo bench
```

Debe resolver:

- dependencias
- versiones
- lockfile
- compilación
- tests
- benchmarks
- metadata
- publicación futura

Formato conceptual:

```text
package.sc
```

o el formato que finalmente se defina.

También debe existir un lockfile para builds reproducibles.

---

## 29. Logging y observabilidad


Agregar una librería `logging`.

Debe permitir:

```science
log.info("training started")
log.warn("learning rate is high")
log.error("dataset failed")
```

También logging estructurado:

```science
log.info(
    "epoch completed",
    epoch: epoch,
    loss: loss,
    accuracy: accuracy
)
```

Debe integrarse con:

- agents
- HTTP
- benchmarks
- CLI
- training

Crear una base reutilizable para:

- logs
- metrics
- traces
- profiling

Los agentes y servidores deberían poder enviar información estructurada sin duplicar infraestructura.

---

## 30. Developer tooling


El ecosistema debe terminar incluyendo:

```text
sciencec
scargo
sci-fmt
sci-test
sci-bench
sci-doc
```

y eventualmente:

```text
sci-lsp
```

con soporte para:

- completion
- diagnostics
- go-to-definition
- rename
- formatting
- hover
- code actions

---

## 31. Sistema de documentación


El lenguaje debe poder generar documentación desde el código.

Objetivo conceptual:

```science
/// Compute the norm of a vector.
def norm[T](x: &Array[T]) -> Float:
    ...
```

Y:

```bash
scargo doc
```

debe producir documentación navegable.

---

## 32. Estrategia de testing


No implementar nuevas features sin tests.

Cada nuevo constructo debe tener:

1. lexer tests
2. parser tests
3. semantic/type tests
4. diagnostics tests
5. runtime tests
6. integration tests cuando corresponda

Para cambios sintácticos:

- agregar tests positivos
- agregar tests negativos
- actualizar fixtures
- actualizar formatter
- actualizar documentación

---

## 33. Diagnósticos


Los diagnósticos son una parte importante de la experiencia.

Debe mantenerse un sistema de errores estable y útil.

Objetivo:

```text
SC0016
SC0017
...
```

Los tests deben verificar no solo que algo falla, sino que falle con el diagnóstico correcto.

---

## 34. Formatter


El formatter debe ser una herramienta oficial.

Ejemplo:

```bash
sci-fmt .
```

Debe producir output determinista.

La sintaxis del formatter debe mantenerse alineada con la gramática.

No permitir que documentación o tests contengan sintaxis antigua.

---

## 35. Política de compatibilidad


Durante el desarrollo inicial se puede cambiar la sintaxis si mejora el diseño.

Sin embargo, una vez establecida una versión de lenguaje:

- documentar breaking changes
- usar versiones del lenguaje
- proveer migraciones cuando sea razonable
- evitar deprecaciones interminables

---
---

# Parte VI — Roadmap

> El original tenía las prioridades (§34) y las fases (§33) como dos secciones
> separadas que decían lo mismo en dos formatos. Aquí la lista de prioridades va
> primero, como resumen, y las fases después, como el detalle de esa misma lista.

## 35bis. Dónde estamos hoy, medido

> **Por qué esta sección está en un documento de destino.** Todo lo de arriba
> es catálogo: métodos que ninguna máquina ha ejecutado. Esta sección es lo
> único aquí que se midió corriendo el compilador, y existe para que nadie
> lea la Parte IV y crea que falta poco. Fecha de la medición: **2026-09-22**.

### El número honesto

**17 de 20 ejemplos** compilan, enlazan, corren y tienen su salida fijada byte
por byte. El denominador es 20 y no 22 porque dos nunca van a construir, y eso
está decidido: `17_modules` importa módulos que la biblioteca F0 no tiene, y
`20_extern` es una biblioteca sin `main` a propósito, que `SC0403` existe para
rechazar.

Suite: **2229 tests en verde**. Correrla con paralelismo acotado
(`-- --test-threads=4`): a carga completa el directorio de scratch compartido
compite consigo mismo y falla un puñado distinto de tests en cada corrida, con
conjuntos disjuntos. Se lee igual que una regresión y no lo es.

### Los tres que faltan, con el bloqueo real

Cada uno está bisecado hasta la línea. **Los mensajes de error del compilador
no son de fiar para esto**, y esa es la lección más cara de la sesión: de cinco
bloqueos investigados, **cinco** culpaban a la fase equivocada o describían un
agujero ya cerrado.

| Ejemplo | Bloqueo real | Dónde |
|---|---|---|
| `03_structs` | const generics sustituidos en el cuerpo — el front end ya sale limpio | mono |
| `06_traits` | `print` de un tipo de usuario: necesita `Display`/`Formatter` | 4 crates |
| `00_kitchen_sink` | cierres con capturas, interfaces de operador, boxes | varios |

### Lo que resultó no estar roto

Tres cosas que este documento, o un mensaje del compilador, daban por
pendientes y **ya funcionaban**. Se verificaron corriendo programas, no
leyendo código:

- **La monomorfización.** Su mensaje decía *"nada ha monomorfizado: la
  Decisión 42 pone el walk por encima de este crate y ninguna fase lo corre"*.
  Funcionaba desde commits anteriores. La misma función genérica bajo el mismo
  bound compila y corre con `T := String` y fallaba sólo con `T := Int` —
  porque clonar un escalar `Copy` es una copia de valor y eso no estaba
  codificado en ninguna parte.
- **`for` sobre un `Iterate` propio.** Su mensaje decía *"todo `for` del
  lenguaje se detiene acá, incluido `for i in 0..10:"`*. Cerrado en `757aa5d`.
  Y la frase es doblemente falsa: un rango nunca pasa por ese camino, tiene su
  propio lowering aritmético.
- **`Formatter`.** Un comentario en `builtins.rs` afirmaba que ninguna nota lo
  especifica. `strings-formatting-and-docs.md` §3.1 lo especifica entero, con
  una `Decision` y un conjunto cerrado de cinco métodos.

**El patrón, porque cuesta horas cada vez:** un mensaje de rechazo envejece
peor que el código que lo rodea. Nadie lo vuelve a leer cuando arregla la cosa
que describe, y el siguiente lector lo trata como diagnóstico actual. Antes de
perseguir cualquiera de los tres que quedan, **reproducí el bloqueo en un
programa mínimo** y creele a lo que imprime, no a lo que dice el mensaje.

### Las decisiones que bloquean, y que no son código

Ninguna de estas se resuelve programando. Están en `STDLIB-DECISIONS.md`.

1. **`Display`.** El spec decidió `display(self, into: mutable &Formatter)`, con
   `Formatter` completamente especificado en `strings-formatting-and-docs.md`
   §3.1. El corpus escribe `def display(self) -> String`. Uno de los dos está
   mal. Bloquea `06`, y bloquea cualquier mensaje de error legible en la
   membrana de Python.
2. **`Eq`/`Ord`.** Si `other` va prestado. Aquí **no** hay contradicción de
   spec — ninguna nota lo especifica —, es el corpus peleándose consigo mismo.
3. **`Add` y las cadenas.** `"a" + "b"` no existe porque `Add` está sin métodos
   a propósito: la aritmética numérica depende de eso para saltarse el despacho.

### El orden que yo tomaría

1. **Const generics a través de la mono → 18/20.** El front end ya sale
   limpio, así que es sólo sustitución: reemplazar `ROWS` y `COLS` por sus
   argumentos const en el cuerpo instanciado. Acotado y sin decisiones.
2. **`Display`/`Formatter` → 19/20.** La decisión está tomada: gana el spec.
   Hay que aterrizar `Formatter`, `FormatSpec` y cuatro `choice` en
   `builtins.rs`, los puntos de entrada del runtime, y una vtable que el
   backend hoy no emite. Cuatro crates, y el corpus escribe la firma vieja en
   dos ejemplos.
   **Riesgo real:** declarar métodos en una interfaz del preludio que hasta
   ahora no tenía ya rompió este corpus una vez, con `Clone.clone`. Si pasa,
   revertir la declaración y decir por qué — no arreglar el ejemplo.
3. **`00_kitchen_sink` → 20/20.** Cierres con capturas, interfaces de operador
   y boxes. Es el último por construcción, no por descuido.

Y una de infraestructura que no mueve el contador y ahorra horas: **el scratch
compartido de los tests**. Cuesta dos corridas y un rato descartar que un fallo
sea propio, y va a engañar a la próxima persona que toque el repo.

**Cómo repartirlos.** No por ejemplo, por *feature*: `00` no es una tarea, es
la unión de tres. Y el reparto tiene que ser por crate, porque varias de estas
convergen en `science-codegen-llvm` y dos agentes editando ese archivo a la vez
se pisan.

### La distancia hasta la Parte IV

El catálogo de librerías empieza a ser escribible cuando el backend cierre F0.
Hoy el compilador **no puede imprimir un tipo de usuario**, así que `tensor`,
`linalg` y `python-ffi` están a varias capas. El orden de dependencias para
Python está en [23.2](#232-python-ffi); el resumen es que la pieza que hay que
diseñar bien *desde el principio* es la cabecera DLPack del tensor, porque es
la única de la lista que es barata ahora y carísima después.

---

## 36. Prioridades


No intentar implementar todas las librerías al mismo tiempo.

Prioridad arquitectónica:

```text
1. compiler
2. runtime
3. standard library
4. package manager
5. tensor
6. linalg
7. autograd
8. nn
9. optim
10. data
11. plot
12. networking
13. agents
14. web
15. advanced GPU/FFI
```

Pero la arquitectura debe dejar preparadas las interfaces para todas.

---

## 37. Las trece fases


### Phase 0 — Language foundation

Prioridad absoluta:

- lexer
- parser
- AST
- resolver
- types
- generics
- interfaces
- references
- mutability
- diagnostics
- formatter
- test infrastructure
- runtime básico

---

### Phase 1 — Standard library

Implementar:

```text
core
prelude
string
collections
mem
io
fs
path
os
time
datetime
math
complex
random
```

---

### Phase 2 — Package manager

Implementar:

```text
scargo new
scargo add
scargo build
scargo run
scargo test
scargo bench
```

y dependency resolution básica.

---

### Phase 3 — Numerical foundation

Implementar:

```text
tensor
ndarray
linalg
stats
```

con CPU primero.

---

### Phase 4 — Native acceleration

Agregar:

```text
simd
BLAS
LAPACK
parallel
```

---

### Phase 5 — GPU architecture

Diseñar e implementar:

```text
backend API
CPU backend
CUDA backend
```

y posteriormente:

```text
ROCm
Metal
```

según disponibilidad y prioridades.

---

### Phase 6 — ML

Implementar:

```text
autograd
nn
optim
model
```

---

### Phase 7 — Data

Implementar:

```text
dataframe
csv
parquet
json
encoding
```

---

### Phase 8 — Plotting

Implementar:

```text
plot
```

con exportación a imágenes.

---

### Phase 9 — Networking

Implementar:

```text
net
http
async
thread
sync
```

---

### Phase 10 — Web framework

Implementar:

```text
http-server
router
middleware
serde
db-driver
websocket
grpc/protobuf
```

---

### Phase 11 — Agents

Implementar progresivamente:

```text
llm
tools
prompt
memory
embeddings
vector-db
chunking
tokenizer
agent-loop
trace
eval
guardrails
sandbox
multi-agent
mcp
```

---

### Phase 12 — Interoperability

Implementar:

```text
c-ffi
python-ffi
```

con ejemplos reales.

---
---

# Parte VII — Acuerdo de trabajo

> Esto es lo que hay que leer antes de tocar el repositorio. El original lo
> tenía repartido en cuatro sitios — las diez reglas (§35), lo que no hay que
> hacer (§39), la Definition of Done (§40) y la instrucción final (§42) — que se
> solapaban entre sí. Aquí están juntos y en el orden en que se usan.

## 38. Reglas para trabajar en este repositorio


Cuando trabajes en este repositorio:

### Regla 1

Primero inspecciona la arquitectura existente.

No inventes una arquitectura paralela si ya existe un módulo apropiado.

---

### Regla 2

Prioriza consistencia.

Si una feature nueva requiere una excepción sintáctica, evalúa primero si puede expresarse usando una construcción existente.

---

### Regla 3

No implementes "fake functionality".

No crear APIs que parezcan funcionar pero no tengan implementación real.

Cuando algo sea un stub, marcarlo explícitamente.

---

### Regla 4

No romper comportamiento existente sin causa.

Si un cambio es breaking:

- documentarlo
- actualizar tests
- actualizar ejemplos
- actualizar documentación

---

### Regla 5

Cada feature debe tener tests.

---

### Regla 6

No duplicar abstracciones.

Ejemplo:

No crear cinco tipos distintos de tensor si uno puede tener backends correctamente diseñados.

---

### Regla 7

Mantener las dependencias acíclicas.

---

### Regla 8

Usar interfaces/traits para extensibilidad.

---

### Regla 9

Separar API pública de detalles internos.

---

### Regla 10

No introducir dependencias externas grandes sin justificar su necesidad y aislamiento.

### Cómo abordar un cambio grande


When implementing this vision, work incrementally.

Before every major change:

1. inspect the existing implementation
2. identify the smallest compatible change
3. implement it
4. add tests
5. update diagnostics
6. update documentation
7. run the relevant test suite
8. run the full suite when practical

Do not attempt to implement every planned library in a single change.

Build the foundations so each subsequent library becomes easier to add.

The long-term target is the complete Science ecosystem described in this document.

---

## 39. Lo que NO hay que hacer


No convertir Science en:

```text
un wrapper gigante de Python
```

No convertir Science en:

```text
un framework web monolítico
```

No meter todas las funcionalidades directamente en `core`.

No acoplar tensors a un único fabricante de GPU.

No hacer que `agents` dependa de `http`.

No obligar a todos los programas a cargar todo el ecosistema.

No crear una API distinta para cada backend cuando una abstracción común sea posible.

No sacrificar seguridad del runtime por velocidad de implementación.

No sacrificar claridad de la sintaxis por copiar otro lenguaje.

---

## 40. Definition of Done


Una feature se considera terminada únicamente cuando:

- compila
- tiene tests
- tiene diagnostics apropiados
- está documentada
- tiene ejemplos si aplica
- está integrada con formatter
- no genera regresiones
- respeta la arquitectura del ecosistema
- tiene API pública definida
- tiene límites conocidos documentados

---
---

# Apéndice A — Programas objetivo

> Los dos programas que el ecosistema terminado debe permitir escribir. Son la
> prueba de aceptación de todo lo anterior.
>
> **Aviso de sintaxis:** están escritos con la sintaxis que propone la
> [Parte II](#parte-ii--las-propuestas-de-sintaxis), no con la del lenguaje
> actual — usan `import`, `range(…)` y corchetes para genéricos. Si la Parte II
> se rechaza, estos dos apéndices se reescriben; no son código válido hoy.

## A.1 Proyecto científico


El ecosistema final debería poder permitir algo parecido a:

```science
import tensor
import nn
import optim
import autograd
import dataframe
import plot

let data be dataframe.read_csv("train.csv")

let x be data.features().to_tensor()
let y be data.labels().to_tensor()

let model be nn.Sequential([
    nn.Linear(784, 256),
    nn.ReLU(),
    nn.Linear(256, 10)
])

let optimizer be optim.Adam(
    model.parameters(),
    lr: 0.001
)

for epoch in range(10):
    let prediction be model(x)
    let loss be nn.cross_entropy(prediction, y)

    optimizer.zero_grad()
    autograd.backward(loss)
    optimizer.step()

    print("epoch:", epoch, "loss:", loss)

model.save(model, "model.sci")

plot.line(loss_history)
plot.save("training.png")
```

---

## A.2 Proyecto de agente + API


```science
import agents
import http

def search(query: String) -> String:
    ...

def calculate(expression: String) -> String:
    ...

let agent be agents.Agent(
    tools: [
        search,
        calculate
    ]
)

let app be http.App()

app.post("/chat", (req, res):
    let response be agent.run(
        req.body.message
    )

    res.json(response)
)

app.listen(8080)
```

Principio:

```text
agents != HTTP
```

El sistema de agentes debe ser agnóstico del transporte.

---
---

# Apéndice B — Qué se reorganizó

Este documento nació como una lluvia de ideas y se ordenó el 2026-09-20. No se
borró ninguna propuesta.

El original se descartó una vez comprobada la reorganización. Conviene hacer
`git add docs/DREAM.md` cuanto antes: este fichero nunca llegó a commitearse, así
que hasta que se haga no existe ninguna versión anterior a la que volver.

Lo que cambió:

**Duplicación eliminada.** La tabla de librerías aparecía dos veces seguidas,
palabra por palabra (97.9% idéntica; la segunda copia además estaba truncada a
media tabla). Se conserva una sola vez, en la sección 12, convertida en índice —
que es la función que ya cumplía, porque las secciones 13 a 27 desarrollan cada
una de sus filas con mucho más detalle.

**Cuarenta y dos secciones planas agrupadas en siete partes.** La numeración
corrida de 1 a 42 no distinguía una decisión de visión de un detalle de
formatter. Las partes separan lo acordado de lo propuesto y de lo conflictivo.

**Cuatro solapamientos fusionados:**

| Se fusionó | En |
|---|---|
| §33 Roadmap + §34 Prioridades | Parte VI, prioridades como resumen y fases como detalle |
| §35 Reglas + §42 Instrucción final | Sección 38 |
| §24 Logging + §25 Observabilidad | Sección 29 |
| §3 Sintaxis + §36 Migración | Parte II, porque la migración era sólo la ejecución de §3 |

**La discusión sobre `agents` y el framework web**, que estaba suelta antes del
plan maestro, se movió a la sección 27.1, donde explica por qué 26 y 27 son
secciones distintas.

**Se añadió la Parte II.** Es el único material realmente nuevo. La lista de
cambios de sintaxis estaba escrita como una tarea de migración pendiente, y no
lo es: cada punto revierte una decisión que tiene detrás una nota de diseño, un
diagnóstico y código. Presentarla como trabajo por hacer habría llevado a un
agente de código a romper el compilador siguiendo el documento al pie de la
letra. Ahora está presentada como lo que es — propuestas sin aprobar — con el
coste y el contraargumento de cada una.

**Lo que no se tocó.** El contenido de las secciones 13 a 35 es el original,
verbatim. Son la parte del documento que no entra en conflicto con nada y que no
necesitaba más que un sitio ordenado donde vivir.

## Lo que queda por decidir

1. **La Parte II, entera.** Es la única que bloquea trabajo real.
2. **El nombre del package manager**: `scargo` aquí, `science.toml` y sin nombre
   propio en `package-manager.md`.
3. **`Option`/`Result`**: si vuelven, hace falta una nota que argumente contra
   `syntax-revision-2.md` §3, no una línea en una lista.
4. **Los dos apéndices** se reescriben en cuanto la Parte II se decida, en un
   sentido o en el otro.
