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

**[Parte V — Toolchain](#parte-v--toolchain)** · 28 Package manager · 29 Logging y observabilidad · 30 Tooling · 31 Documentación · 32 Testing · 33 Diagnósticos · 34 Formatter · 35 Compatibilidad

**[Parte VI — Roadmap](#parte-vi--roadmap)** · 36 Prioridades · 37 Las trece fases

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

Responsabilidades:

- String
- UTF-8
- slicing
- búsqueda
- concatenación
- formateo
- parsing textual

---

### 13.4 `collections`

Debe proporcionar, como mínimo:

- Array
- Map
- Set
- Queue
- List
- estructuras genéricas adicionales cuando sean necesarias

Debe soportar una API consistente con el sistema de tipos de Science.

Ejemplos:

```science
Array[Int]
Map[String, Float]
Set[String]
```

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

Debe incluir:

- sin
- cos
- tan
- asin
- acos
- atan
- atan2
- sinh
- cosh
- tanh
- exp
- log
- log10
- sqrt
- pow
- abs
- floor
- ceil
- round
- constantes matemáticas
- funciones especiales cuando sea razonable

Funciones especiales deseadas:

- gamma
- bessel
- erf
- etc.

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

Esta debe ser una de las piezas centrales del ecosistema.

Debe soportar:

- tensores N-dimensionales
- shape
- dtype
- strides
- indexing
- slicing
- reshape
- transpose
- broadcast
- reduce
- concatenate
- stack
- split
- gather/scatter cuando sea viable
- device
- CPU/GPU backends

Ejemplos:

```science
let x be tensor.ones([32, 784])
let y be tensor.randn([784, 256])

let z be x @ y
```

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

Debe incluir:

- matmul
- dot
- norm
- inverse
- solve
- determinant
- rank
- transpose
- QR
- SVD
- eigenvalues
- eigenvectors
- decomposiciones importantes

Ejemplo:

```science
let U, S, Vt be linalg.svd(A)
```

No reimplementar desde cero algoritmos que puedan delegarse a BLAS/LAPACK u otros backends maduros.

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

Objetivo estratégico:

permitir interoperar con:

- NumPy
- SciPy
- scikit-learn
- ecosistema Python
- otras librerías científicas

Esto puede acelerar mucho la adopción inicial del lenguaje.

La arquitectura debe evitar convertir Science en un wrapper de Python.

La interoperabilidad debe ser una puerta de entrada, no el núcleo del diseño.

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
