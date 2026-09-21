jAcá tenés la lista completa combinada (base general + especialización en ciencias/ML), cada una con qué es y para qué sirve:

## Núcleo del lenguaje

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `core`/`prelude` | Tipos primitivos, Option/Result, traits básicos, iteradores | Es la base sin la cual no hay lenguaje: manejo de errores, tipos genéricos, iteración |
| `string` | Manipulación de texto, UTF-8, formateo | Construir mensajes, parsear input, mostrar resultados legibles |
| `collections` | Arrays dinámicos, hashmaps, sets, colas, listas | Estructuras de datos genéricas que usa cualquier programa |
| `mem` | Manejo de memoria, punteros, alocación | Control fino de memoria si tu lenguaje lo expone (importante si es de bajo nivel) |

## Numérico (columna vertebral para ciencias/ML)

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `ndarray`/`tensor` | Arrays N-dimensionales con broadcasting, slicing, reshape | Es LA estructura de datos de ML/ciencia: sin esto no representás matrices, tensores ni datasets |
| `linalg` | Álgebra lineal: multiplicación de matrices, SVD, eigenvalues, inversión | Base de casi todo algoritmo de ML clásico y simulación científica |
| `math` | Trig, logaritmos, funciones especiales (gamma, bessel, erf), constantes | Cálculos matemáticos que aparecen en cualquier modelo estadístico o físico |
| `complex` | Números complejos | Necesarios en procesamiento de señales, FFT, física cuántica, ciertos algoritmos de optimización |
| `random` | Generadores aleatorios con distribuciones (normal, uniforme, poisson) | Inicialización de pesos en ML, simulaciones Monte Carlo, muestreo estadístico |

## Performance

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `simd` | Acceso a instrucciones vectorizadas del CPU | Acelerar operaciones numéricas masivas (sumas, multiplicaciones en batch) |
| Bindings a `BLAS`/`LAPACK` | Interfaz a librerías de álgebra lineal optimizadas y probadas por décadas | No reinventar la rueda: usar código ya optimizado en vez de implementar multiplicación de matrices lenta |
| Soporte GPU (CUDA/ROCm/Metal) | Bindings para correr cómputo en la placa de video | Entrenar modelos de ML modernos, que son inviables en CPU por el volumen de cálculo |
| `parallel` | Paralelización de loops y operaciones (tipo rayon) | Usar múltiples cores del CPU sin que el usuario tenga que manejar threads a mano |

## ML y autodiff (tu diferenciador de nicho)

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `autograd` | Diferenciación automática (cálculo de gradientes) | Es el corazón de cómo se entrenan redes neuronales (backpropagation); sin esto no hay deep learning nativo |
| `nn` | Capas básicas de redes neuronales (lineal, convolucional, activaciones) | Armar arquitecturas de modelos sin reimplementar cada capa desde cero |
| `optim` | Optimizadores (SGD, Adam, RMSprop) | Algoritmos que ajustan los pesos del modelo durante el entrenamiento |
| `stats` | Testing estadístico, distribuciones, hipótesis (t-test, chi-cuadrado) | Análisis estadístico riguroso, validación de resultados científicos |

## Datos

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `dataframe` | Estructuras tabulares tipo pandas/polars | Cargar, limpiar y transformar datasets antes de meterlos a un modelo |
| `csv` / `parquet` | Lectores/escritores de formatos de datos tabulares | Formatos estándar en los que vive el 90% de los datasets del mundo real |
| `json` | Parseo y serialización JSON | Comunicación con APIs, configuración, intercambio de datos |
| `encoding` | Base64, hex, conversión UTF-8/16 | Codificar/decodificar datos binarios o texto en distintos formatos |

## Visualización

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `plot` | Generación de gráficos (líneas, dispersión, histogramas) | Inspeccionar resultados, debuggear modelos, comunicar hallazgos — subestimado pero esencial en ciencia |

## Sistema y entorno

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `os` | Variables de entorno, argumentos CLI, procesos, señales | Interactuar con el sistema operativo, escribir scripts y herramientas |
| `fs` | Lectura/escritura de archivos y directorios | Cargar datasets, guardar modelos entrenados, logs |
| `path` | Manipulación de rutas cross-platform | Evitar bugs de rutas entre Windows/Linux/Mac |
| `io` | Streams genéricos, buffers, stdin/stdout/stderr | Entrada/salida de datos de forma eficiente y uniforme |
| `time`/`datetime` | Timestamps, duraciones, timezones | Medir performance, timestamping de datos, series temporales |

## Red y concurrencia

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `net`/`http` | Sockets, TCP/UDP, cliente/servidor HTTP | Descargar datasets, servir modelos como API (inference en producción) |
| `async`/`thread` | Concurrencia: threads, async/await, canales | Procesar datos o requests sin bloquear el programa |
| `sync` | Locks, atomics, semáforos | Coordinar acceso a memoria compartida entre threads |

## Interoperabilidad (posible killer feature)

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `python-ffi` | Interfaz para llamar librerías de Python | Reusar todo el ecosistema de numpy/scipy/sklearn sin reescribirlo; acelera muchísimo la adopción |
| `c-ffi` | Interfaz para llamar código C | Usar BLAS/LAPACK/CUDA existentes en vez de reimplementarlos desde cero |

## Seguridad

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `crypto`/`hash` | Hashing (SHA, MD5), primitivas criptográficas | Verificar integridad de datasets/modelos, seguridad básica |
| `tls` | Cifrado de conexiones | Necesario si tu `http` va a manejar tráfico real en producción |

## Testing y herramientas

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `test` | Framework de testing integrado | Da confianza a la comunidad para escribir código serio; sin esto nadie confía en tu ecosistema |
| `bench` | Benchmarking | Medir y comparar performance de implementaciones, crítico en un lenguaje numérico |

## No es librería, pero es innegociable

**Gestor de paquetes propio** (tipo cargo/pip) — sin esto, ninguna librería de terceros puede crecer alrededor de tu lenguaje, y sin ecosistema de terceros, tu lenguaje nunca escala más allá de lo que vos mismo escribas.
Acá tenés la lista completa combinada (base general + especialización en ciencias/ML), cada una con qué es y para qué sirve:

## Núcleo del lenguaje

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `core`/`prelude` | Tipos primitivos, Option/Result, traits básicos, iteradores | Es la base sin la cual no hay lenguaje: manejo de errores, tipos genéricos, iteración |
| `string` | Manipulación de texto, UTF-8, formateo | Construir mensajes, parsear input, mostrar resultados legibles |
| `collections` | Arrays dinámicos, hashmaps, sets, colas, listas | Estructuras de datos genéricas que usa cualquier programa |
| `mem` | Manejo de memoria, punteros, alocación | Control fino de memoria si tu lenguaje lo expone (importante si es de bajo nivel) |

## Numérico (columna vertebral para ciencias/ML)

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `ndarray`/`tensor` | Arrays N-dimensionales con broadcasting, slicing, reshape | Es LA estructura de datos de ML/ciencia: sin esto no representás matrices, tensores ni datasets |
| `linalg` | Álgebra lineal: multiplicación de matrices, SVD, eigenvalues, inversión | Base de casi todo algoritmo de ML clásico y simulación científica |
| `math` | Trig, logaritmos, funciones especiales (gamma, bessel, erf), constantes | Cálculos matemáticos que aparecen en cualquier modelo estadístico o físico |
| `complex` | Números complejos | Necesarios en procesamiento de señales, FFT, física cuántica, ciertos algoritmos de optimización |
| `random` | Generadores aleatorios con distribuciones (normal, uniforme, poisson) | Inicialización de pesos en ML, simulaciones Monte Carlo, muestreo estadístico |

## Performance

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `simd` | Acceso a instrucciones vectorizadas del CPU | Acelerar operaciones numéricas masivas (sumas, multiplicaciones en batch) |
| Bindings a `BLAS`/`LAPACK` | Interfaz a librerías de álgebra lineal optimizadas y probadas por décadas | No reinventar la rueda: usar código ya optimizado en vez de implementar multiplicación de matrices lenta |
| Soporte GPU (CUDA/ROCm/Metal) | Bindings para correr cómputo en la placa de video | Entrenar modelos de ML modernos, que son inviables en CPU por el volumen de cálculo |
| `parallel` | Paralelización de loops y operaciones (tipo rayon) | Usar múltiples cores del CPU sin que el usuario tenga que manejar threads a mano |

## ML y autodiff (tu diferenciador de nicho)

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `autograd` | Diferenciación automática (cálculo de gradientes) | Es el corazón de cómo se entrenan redes neuronales (backpropagation); sin esto no hay deep learning nativo |
| `nn` | Capas básicas de redes neuronales (lineal, convolucional, activaciones) | Armar arquitecturas de modelos sin reimplementar cada capa desde cero |
| `optim` | Optimizadores (SGD, Adam, RMSprop) | Algoritmos que ajustan los pesos del modelo durante el entrenamiento |
| `stats` | Testing estadístico, distribuciones, hipótesis (t-test, chi-cuadrado) | Análisis estadístico riguroso, validación de resultados científicos |

## Datos

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `dataframe` | Estructuras tabulares tipo pandas/polars | Cargar, limpiar y transformar datasets antes de meterlos a un modelo |
| `csv` / `parquet` | Lectores/escritores de formatos de datos tabulares | Formatos estándar en los que vive el 90% de los datasets del mundo real |
| `json` | Parseo y serialización JSON | Comunicación con APIs, configuración, intercambio de datos |
| `encoding` | Base64, hex, conversión UTF-8/16 | Codificar/decodificar datos binarios o texto en distintos formatos |

## Visualización

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `plot` | Generación de gráficos (líneas, dispersión, histogramas) | Inspeccionar resultados, debuggear modelos, comunicar hallazgos — subestimado pero esencial en ciencia |

## Sistema y entorno

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `os` | Variables de entorno, argumentos CLI, procesos, señales | Interactuar con el sistema operativo, escribir scripts y herramientas |
| `fs` | Lectura/escritura de archivos y directorios | Cargar datasets, guardar modelos entrenados, logs |
| `path` | Manipulación de rutas cross-platform | Evitar bugs de rutas entre Windows/Linux/Mac |
| `io` | Streams genéricos, buffers, stdin/stdout/stderr | Entrada/salida de datos de forma eficiente y uniforme |
| `time`/`datetime` | Timestamps, duraciones, timezones | Medir performance, timestamping de datos, series temporales |

## Red y concurrencia

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `net`/`http` | Sockets, TCP/UDP, cliente/servidor HTTP | Descargar datasets, servir modelos como API (inference en producción) |
| `async`/`thread` | Concurrencia: threads, async/await, canales | Procesar datos o requests sin bloquear el programa |
| `sync` | Locks, atomics, semáforos | Coordinar acceso a memoria compartida entre threads |

## Interoperabilidad (posible killer feature)

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `python-ffi` | Interfaz para llamar librerías de Python | Reusar todo el ecosistema de numpy/scipy/sklearn sin reescribirlo; acelera muchísimo la adopción |
| `c-ffi` | Interfaz para llamar código C | Usar BLAS/LAPACK/CUDA existentes en vez de reimplementarlos desde cero |

## Seguridad

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `crypto`/`hash` | Hashing (SHA, MD5), primitivas criptográficas | Verificar integridad de datasets/modelos, seguridad básica |
| `tls` | Cifrado de conexiones | Necesario si tu `http` va a manejar tráfico real en producción |

## Testing y herramientas

| Librería | Qué es | Para qué sirve |
|---|---|---|
| `test` | Framework de testing integrado | Da confianza a la comunidad para escribir código serio; sin esto nadie confía en tu ecosistema |
| `bench` | Benchmarking | Medir y comparar performance de implementaciones, crítico en un lenguaje numérico |

## Librería `agents` — qué y para qué

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

## Librería REST/API — qué y para qué

| Módulo | Qué es | Para qué sirve |
|---|---|---|
| `http-server` | Capa sobre tu `net`/`http` de bajo nivel: routing, request/response, listen | El punto de entrada: levantar un servidor sin manejar sockets a mano |
| `router` | Sistema de rutas (`GET /users/:id`) | Mapear URLs y métodos HTTP a funciones handler |
| `middleware` | Auth, logging, CORS, rate-limiting como capas componibles | Reusar lógica transversal sin repetirla en cada endpoint |
| `serde`-like | Serialización/deserialización struct ↔ JSON (o protobuf) | Que un endpoint reciba/devuelva datos tipados sin parseo manual |
| `orm`/`db-driver` | Conexión a bases de datos | Persistencia — casi todo backend real la necesita |
| `websocket` | Comunicación bidireccional en tiempo real | Streaming de respuestas del agente/modelo, dashboards en vivo |
| `grpc`/`protobuf` | RPC binario tipado | Comunicación rápida entre microservicios, muy usado sirviendo modelos |

## Ahora tu pregunta de diseño: ¿`agents` como librería propia, y la API como framework tipo Express?

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

# Science — Ecosystem Master Plan & Implementation Prompt

> Documento maestro para definir la visión del lenguaje **Science**, su librería estándar, ecosistema científico/ML, agentes, web/backend, interoperabilidad, tooling y roadmap de implementación.
>
> Este documento está pensado para poder usarse de dos maneras:
>
> 1. Como **especificación/roadmap de arquitectura** del proyecto.
> 2. Como **prompt maestro para un agente de código** que trabaje sobre el repositorio.

---

# 1. Vision

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

# 2. Principios de arquitectura

## 2.1 El compilador debe ser pequeño en responsabilidades

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

# 3. Sintaxis base objetivo

La sintaxis debe seguir una filosofía de minimalismo, legibilidad y consistencia.

## 3.1 Referencias

Usar:

```science
&T
&mut T
```

No mantener como sintaxis principal:

```science
borrowed T
mutable borrowed T
```

La migración debe eliminar usos de la sintaxis antigua donde corresponda.

---

## 3.2 Mutabilidad

Usar:

```science
let mut x be value
```

o la forma equivalente definida por la gramática actual.

No usar:

```science
let mutable x be value
```

---

## 3.3 Genéricos

Usar corchetes:

```science
Array[T]
Map[K, V]
Result[T, E]
Option[T]
Pair[A, B]
```

No usar:

```science
Array of T
Map of (K, V)
```

Los genéricos deben soportar anidamiento:

```science
Array[Map[String, Int]]
Map[String, Array[Float]]
Array[Array[T]]
```

---

## 3.4 Funciones genéricas

Usar:

```science
def largest[T](items: &Array[T]) -> &T:
    ...
```

y:

```science
def map[T, U](items: &Array[T], f: Function[T, U]) -> Array[U]:
    ...
```

No usar:

```science
def largest of T(...):
    ...
```

---

## 3.5 Interfaces

Mantener una sintaxis descriptiva:

```science
interface Summarize:
    def summarize(self) -> String
```

Implementación:

```science
Doc implements Summarize:
    def summarize(self) -> String:
        ...
```

No convertir obligatoriamente esto en:

```science
impl Summarize for Doc:
```

La palabra `implements` forma parte de la identidad legible de Science.

---

## 3.6 Métodos asociados

Mantener:

```science
Doc has:
    def new(...) -> Doc:
        ...
```

Esto representa métodos propios/asociados del tipo.

---

# 4. Organización del ecosistema

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

# 5. Librería estándar — Core

## 5.1 `core`

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

## 5.2 `prelude`

Exportaciones de uso frecuente.

Objetivo:

```science
import prelude
```

o una inclusión automática según la evolución del lenguaje.

Debe evitar imports excesivos para operaciones fundamentales.

---

## 5.3 `string`

Responsabilidades:

- String
- UTF-8
- slicing
- búsqueda
- concatenación
- formateo
- parsing textual

---

## 5.4 `collections`

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

## 5.5 `mem`

Responsabilidades:

- alocación
- desaloación
- memoria de bajo nivel
- punteros/handles según el modelo definitivo
- operaciones explícitas de memoria cuando sean necesarias

No exponer primitivas peligrosas si pueden mantenerse encapsuladas.

---

# 6. Núcleo matemático y numérico

## 6.1 `math`

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

## 6.2 `complex`

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

## 6.3 `random`

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

## 6.4 `stats`

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

# 7. Arrays y Tensores

## 7.1 `tensor`

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

## 7.2 `ndarray`

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

## 7.3 Broadcasting

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

# 8. Álgebra lineal

## 8.1 `linalg`

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

# 9. Performance

## 9.1 `simd`

Exponer operaciones SIMD de forma segura cuando sea apropiado.

Debe existir una abstracción suficientemente estable para que librerías como `tensor` puedan aprovechar SIMD sin duplicar APIs.

---

## 9.2 BLAS/LAPACK

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

## 9.3 GPU

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

## 9.4 `parallel`

Debe permitir paralelización de operaciones y loops sin requerir manejo manual de threads para casos comunes.

Ejemplo conceptual:

```science
parallel for item in items:
    process(item)
```

Debe integrarse con el modelo de memoria y concurrencia de Science.

---

# 10. Machine Learning

## 10.1 `autograd`

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

## 10.2 `nn`

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

## 10.3 `optim`

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

## 10.4 `model`

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

# 11. Datos

## 11.1 `dataframe`

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

## 11.2 `csv`

Lectura/escritura robusta.

---

## 11.3 `parquet`

Soporte eficiente para datasets científicos.

---

## 11.4 `json`

Parseo y serialización.

Debe integrarse con estructuras de datos de Science.

---

## 11.5 `encoding`

Debe incluir:

- Base64
- Hex
- UTF-8
- UTF-16
- conversiones binarias

---

# 12. Visualización

## `plot`

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

# 13. Sistema y entorno

## `os`

Debe incluir:

- environment variables
- arguments
- process information
- process spawning
- signals donde sea apropiado

---

## `fs`

Debe incluir:

- archivos
- directorios
- lectura
- escritura
- metadata
- permisos cuando sea seguro y necesario

---

## `path`

API cross-platform.

Debe evitar concatenaciones manuales de paths.

---

## `io`

Debe incluir:

- stdin
- stdout
- stderr
- streams
- buffers
- readers
- writers

---

## `time` / `datetime`

Debe cubrir:

- timestamp
- duration
- timezone
- parsing
- formatting
- monotonic clocks
- measurement de performance

---

# 14. Networking y concurrencia

## `net`

Debe cubrir:

- sockets
- TCP
- UDP

---

## `http`

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

## `async`

Modelo de async/await.

Debe integrarse con el runtime.

---

## `thread`

Debe permitir creación y coordinación de threads.

---

## `sync`

Debe incluir:

- mutex/locks
- atomics
- semaphores
- synchronization primitives
- channels según diseño

---

# 15. Interoperabilidad

## 15.1 `c-ffi`

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

## 15.2 `python-ffi`

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

# 16. Seguridad

## `hash`

Debe cubrir hashes usados para:

- integridad
- identificación
- verificación de datasets
- cachés
- artefactos

---

## `crypto`

Primitivas criptográficas maduras y bien auditadas cuando sea posible.

No inventar algoritmos criptográficos propios.

---

## `tls`

Integración con TLS para HTTP y networking.

---

# 17. Testing y benchmarking

## `test`

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

## `bench`

Debe permitir medir performance.

Ejemplo conceptual:

```science
bench "matrix multiplication":
    linalg.matmul(A, B)
```

Debe producir métricas útiles.

---

# 18. Ecosistema de agentes

## `agents`

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

## 18.1 `llm`

Cliente unificado para modelos.

Debe abstraer proveedores locales y remotos.

Debe contemplar:

- prompts
- responses
- streaming
- model configuration
- provider abstraction

---

## 18.2 `tools`

Sistema de herramientas tipadas.

Una herramienta debe tener:

- nombre
- descripción
- schema de argumentos
- función ejecutable
- resultado tipado cuando sea posible

---

## 18.3 `agent-loop`

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

## 18.4 `memory`

Memoria de:

- corto plazo
- largo plazo
- sesión
- almacenamiento persistente

---

## 18.5 `prompt`

Debe incluir:

- templates
- variables
- few-shot examples
- versioning
- composición

---

## 18.6 `embeddings`

Debe permitir:

- generar embeddings
- comparar vectores
- almacenar embeddings
- batch processing

---

## 18.7 `vector-db`

Cliente/abstracción para índices vectoriales.

Debe permitir:

- insert
- search
- filter
- metadata
- delete

---

## 18.8 `chunking`

División de documentos en fragmentos.

Debe contemplar:

- size
- overlap
- semantic chunking
- metadata preservation

---

## 18.9 `tokenizer`

Debe permitir:

- tokenize
- detokenize cuando sea posible
- token counts
- limits
- truncation

---

## 18.10 `multi-agent`

Coordinación entre agentes.

Debe soportar roles sin imponer una arquitectura única.

---

## 18.11 `mcp` / `protocol`

Soporte para protocolos estándar de conexión entre agentes y herramientas.

No acoplarlo al módulo HTTP.

---

## 18.12 `sandbox`

Aislamiento para ejecución de acciones o código generado.

Debe tratarse como componente de seguridad crítica.

---

## 18.13 `trace` / `eval`

Observabilidad de agentes:

- traces
- events
- tool calls
- latency
- token usage
- evaluation metrics

---

## 18.14 `guardrails`

Validación de:

- outputs
- tool arguments
- tool results
- schemas
- límites
- políticas definidas por la aplicación

---

# 19. Framework Web/API

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

## 19.1 `http-server`

Servidor HTTP base.

---

## 19.2 `router`

Routing:

```text
GET /users/:id
POST /chat
DELETE /items/:id
```

Debe mantener el diseño simple y componible.

---

## 19.3 `middleware`

Capas como:

- auth
- logging
- CORS
- rate limiting
- tracing
- compression

Debe ser componible.

---

## 19.4 `serde`-like

Serialización/deserialización:

```text
struct <-> JSON
struct <-> binary
struct <-> protobuf
```

Debe integrarse profundamente con el type system.

---

## 19.5 `orm` / `db-driver`

Conectores a bases de datos.

No convertir esto inmediatamente en un ORM gigante.

Primero una interfaz común y drivers sólidos.

---

## 19.6 `websocket`

Comunicación bidireccional.

Importante para:

- streaming LLM
- dashboards
- realtime apps

---

## 19.7 `grpc` / `protobuf`

RPC binario tipado.

Especialmente útil para:

- microservicios
- inference servers
- sistemas distribuidos

---

# 20. Package manager — `scargo`

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

# 21. Modelo de dependencias

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

# 22. Backend abstraction

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

# 23. Serialization architecture

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

# 24. Logging

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

---

# 25. Observabilidad

Crear una base reutilizable para:

- logs
- metrics
- traces
- profiling

Los agentes y servidores deberían poder enviar información estructurada sin duplicar infraestructura.

---

# 26. Interfaz entre Science y native code

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

# 27. Developer tooling

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

# 28. Documentation system

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

# 29. Testing strategy

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

# 30. Diagnostics

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

# 31. Formatter

El formatter debe ser una herramienta oficial.

Ejemplo:

```bash
sci-fmt .
```

Debe producir output determinista.

La sintaxis del formatter debe mantenerse alineada con la gramática.

No permitir que documentación o tests contengan sintaxis antigua.

---

# 32. Compatibility policy

Durante el desarrollo inicial se puede cambiar la sintaxis si mejora el diseño.

Sin embargo, una vez establecida una versión de lenguaje:

- documentar breaking changes
- usar versiones del lenguaje
- proveer migraciones cuando sea razonable
- evitar deprecaciones interminables

---

# 33. Roadmap recomendado

## Phase 0 — Language foundation

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

## Phase 1 — Standard library

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

## Phase 2 — Package manager

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

## Phase 3 — Numerical foundation

Implementar:

```text
tensor
ndarray
linalg
stats
```

con CPU primero.

---

## Phase 4 — Native acceleration

Agregar:

```text
simd
BLAS
LAPACK
parallel
```

---

## Phase 5 — GPU architecture

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

## Phase 6 — ML

Implementar:

```text
autograd
nn
optim
model
```

---

## Phase 7 — Data

Implementar:

```text
dataframe
csv
parquet
json
encoding
```

---

## Phase 8 — Plotting

Implementar:

```text
plot
```

con exportación a imágenes.

---

## Phase 9 — Networking

Implementar:

```text
net
http
async
thread
sync
```

---

## Phase 10 — Web framework

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

## Phase 11 — Agents

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

## Phase 12 — Interoperability

Implementar:

```text
c-ffi
python-ffi
```

con ejemplos reales.

---

# 34. Prioridades

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

# 35. Reglas para el agente de código

Cuando trabajes en este repositorio:

## Regla 1

Primero inspecciona la arquitectura existente.

No inventes una arquitectura paralela si ya existe un módulo apropiado.

---

## Regla 2

Prioriza consistencia.

Si una feature nueva requiere una excepción sintáctica, evalúa primero si puede expresarse usando una construcción existente.

---

## Regla 3

No implementes "fake functionality".

No crear APIs que parezcan funcionar pero no tengan implementación real.

Cuando algo sea un stub, marcarlo explícitamente.

---

## Regla 4

No romper comportamiento existente sin causa.

Si un cambio es breaking:

- documentarlo
- actualizar tests
- actualizar ejemplos
- actualizar documentación

---

## Regla 5

Cada feature debe tener tests.

---

## Regla 6

No duplicar abstracciones.

Ejemplo:

No crear cinco tipos distintos de tensor si uno puede tener backends correctamente diseñados.

---

## Regla 7

Mantener las dependencias acíclicas.

---

## Regla 8

Usar interfaces/traits para extensibilidad.

---

## Regla 9

Separar API pública de detalles internos.

---

## Regla 10

No introducir dependencias externas grandes sin justificar su necesidad y aislamiento.

---

# 36. Migration task — syntax cleanup

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

# 37. Example target project

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

# 38. Example agent + API project

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

# 39. What NOT to do

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

# 40. Definition of Done

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

# 41. Objetivo final

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

# 42. Final instruction for coding agents

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
