# Link — Diseño de F0: el núcleo del lenguaje

Fecha: 2026-09-16
Estado: aprobado
Alcance: fase F0 de cinco. Este documento define **solo** el núcleo del lenguaje.

---

## 1. Qué es Link

Link es un lenguaje de programación compilado, orientado a agentes y modelos de
aprendizaje automático. Compila a binario nativo a través de LLVM. Gestiona la
memoria con ownership y préstamos verificados en tiempo de compilación, sin
recolector de basura.

Lo que distingue a Link de un lenguaje de sistemas convencional no está en F0:
son los agentes, las herramientas y los prompts como construcciones del
lenguaje, la durabilidad como propiedad verificada por el compilador, y los
tensores con forma en el sistema de tipos. F0 construye los cimientos sobre los
que eso se apoya.

- Archivos: `.link`
- Compilador: `linkc`
- Palabras clave en inglés; identificadores en el idioma del autor.

## 2. Las cinco fases

| Fase | Contenido | Depende de |
|---|---|---|
| **F0** | Núcleo: léxico, tipos, genéricos, traits, ownership con regiones, codegen LLVM | — |
| **F1** | Actores: `spawn`, `send`, `receive`, supervisión, runtime concurrente | F0 (semántica de move para mensajes zero-copy) |
| **F2** | `agent`, `tool`, `prompt`: FFI a proveedores, salida del modelo verificada contra el tipo declarado | F1 |
| **F3** | Durabilidad: bajada de cuerpos de agente a máquinas de estado, serialización, reanudación | F2 (define los puntos de suspensión) |
| **F4** | `Tensor[f32, (batch, 768)]`: aritmética de formas a nivel de tipos | F0 |

**Restricción que F0 debe respetar por adelantado.** F3 exige que ningún
préstamo siga vivo a través de un punto de suspensión, para que el estado
capturado de un agente sea serializable. El motor de regiones de F0 debe poder
responder "¿qué préstamos están vivos en este punto del programa?" como consulta
de primera clase, no como detalle interno del algoritmo. Se diseña así desde el
principio.

## 3. Decisiones de diseño y sus razones

Registro de las decisiones tomadas, con el motivo, para que revisarlas más
adelante sea un acto informado.

| Decisión | Alternativas descartadas | Motivo |
|---|---|---|
| Compilación AOT nativa vía LLVM | VM de bytecode, transpilación, intérprete | Rendimiento y binarios autónomos. Coste asumido: la durabilidad de F3 requiere bajar los cuerpos de agente a máquinas de estado explícitas, en vez de salir gratis del estado de una VM. |
| Ownership y préstamos, sin GC | GC por actor, ARC, GC global | `send` mueve el valor: mensajes entre agentes sin copia y sin aliasing. Coste asumido: es el sistema de tipos más caro de implementar. |
| Inferencia total de regiones | Lifetimes explícitos; préstamos de segunda clase | Máxima expresividad sin anotaciones. Coste asumido: análisis interprocedural y diagnósticos que deben reconstruir el razonamiento del compilador. |
| Fundamentos completos antes de agentes | Vertical delgado; camino intermedio | El núcleo queda sólido y no hay que reescribirlo. Coste asumido: F0 es largo y la tesis del lenguaje no se demuestra hasta F2. |
| Bloques por indentación | Llaves | Legibilidad y afinidad con el público de ML y Python. |
| Genéricos con corchetes | Ángulos | Sin ambigüedad de parseo con los operadores de comparación, luego sin turbofish. Y `Tensor[f32, (batch, 768)]` lee bien en F4. |
| Arquitectura de queries (`salsa`) desde F0 | Compilación por lotes | Recompilación incremental y un futuro language server sin pagar la migración que a rustc le costó años. Coste asumido: más ceremonia en cada fase. |
| Compilador escrito en Rust | C++, OCaml | Mejores librerías de compilador vivas, `inkwell` para LLVM, y modelo mental compartido con lo que se está implementando. |

## 4. Sintaxis

### 4.1 Estructura léxica

**Codificación.** UTF-8. Los identificadores admiten letras Unicode.

**Indentación.** Los bloques se delimitan por sangría. El léxer emite tokens
`INDENT` y `DEDENT` sintéticos.

- Solo espacios. Un tabulador en la sangría es un error léxico (`LK0003`); no se
  intenta interpretarlo.
- Se mantiene una pila de niveles. Mayor sangría que la cima emite un `INDENT` y
  apila; menor emite tantos `DEDENT` como niveles se cierren. Una sangría que no
  coincide con ningún nivel de la pila es un error (`LK0004`).
- Las líneas en blanco y las que solo contienen un comentario no participan en
  el cálculo de sangría.
- Dentro de paréntesis o corchetes sin cerrar, los saltos de línea y la sangría
  se ignoran por completo: la continuación de línea es implícita.

**Comentarios.** `#` hasta el final de la línea. No hay comentarios de bloque.

**Literales.**

- Enteros: `42`, `1_000_000`, `0xFF`, `0b1010`, `0o777`. Sufijo de tipo
  opcional: `42i32`, `7u8`.
- Flotantes: `3.14`, `1e-9`, `2.5f32`.
- Cadenas: `"hola"`, con escapes `\n`, `\t`, `\r`, `\\`, `\"`, `\0`, `\u{1F600}`.
- Caracteres: `'a'`, `'\n'`.
- Booleanos: `true`, `false`.
- Unidad: `()`.

**Palabras reservadas.** `fn`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`,
`while`, `loop`, `return`, `break`, `continue`, `struct`, `enum`, `trait`,
`impl`, `use`, `mod`, `pub`, `true`, `false`, `self`, `Self`, `as`, `dyn`,
`where`, `and`, `or`, `not`.

Reservadas desde ahora aunque F0 no las use, para que F1 a F4 no rompan
compatibilidad: `agent`, `tool`, `prompt`, `spawn`, `send`, `receive`,
`durable`, `checkpoint`, `resume`, `supervise`, `async`, `await`, `tensor`,
`shape`, `model`.

### 4.2 Bloques

Un bloque se introduce con `:` seguido de salto de línea, `INDENT`, sentencias,
`DEDENT`.

Existe además una **forma en línea** para construcciones de una sola expresión,
válida solo si cabe entera en una línea:

```link
if a.len() > b.len(): a else: b
```

### 4.3 Declaraciones

```link
fn longest(a: &String, b: &String) -> &String:
    if a.len() > b.len(): a else: b

fn main():
    let greeting = "hola"
    let mut count = 0
    count = count + 1
```

El tipo de retorno omitido significa unidad `()`. El valor de una función es su
última expresión; `return` existe para salida temprana.

`let` liga de forma inmutable; `let mut` permite reasignar. La anotación de tipo
es opcional en `let` y obligatoria en los parámetros y el retorno de `fn`.

**Structs.**

```link
struct Doc:
    title: String
    body: String

struct View:
    source: &Doc          # permitido: la región se infiere
```

Construcción con argumentos nombrados obligatorios, en cualquier orden:

```link
let d = Doc(title: "a", body: "...")
```

**Enums.** Variantes con carga posicional. Sin variantes con campos nombrados en
F0.

```link
enum Option[T]:
    Some(T)
    None

enum Result[T, E]:
    Ok(T)
    Err(E)
```

**Traits.** Con métodos por defecto.

```link
trait Summarize:
    fn summarize(&self) -> String

    fn preview(&self) -> String:
        self.summarize().truncate(80)

impl Summarize for Doc:
    fn summarize(&self) -> String:
        self.body.truncate(200)
```

**Genéricos y cotas.**

```link
fn largest[T: Ord](items: &Array[T]) -> &T:
    let mut best = items.get(0)
    for item in items:
        if item > best: best = item
    best

struct Pair[A, B]:
    first: A
    second: B
```

Cotas múltiples con `+`, y cláusula `where` para firmas largas:

```link
fn describe[T](x: &T) -> String where T: Summarize + Clone:
    x.summarize()
```

**Despacho dinámico.** `dyn Trait` solo tras una indirección: `&dyn Summarize` o
`Box[dyn Summarize]`. Un trait es compatible con `dyn` si ningún método usa
`Self` por valor ni parámetros genéricos propios.

**Módulos.** Un fichero es un módulo; un directorio con `mod.link` es un módulo
que contiene a sus hermanos.

```link
use text.parser
use text.parser (Token, lex)
```

### 4.4 Expresiones y control de flujo

`if`/`else` es una expresión; ambas ramas deben tener el mismo tipo, salvo que
no haya `else`, en cuyo caso el tipo es `()`.

`match` es exhaustivo. La no exhaustividad es un error (`LK0210`) que lista los
patrones que faltan.

```link
match result:
    Ok(value): value
    Err(e): panic(e)

match point:
    (0, 0): "origen"
    (x, 0): "sobre el eje X"
    (_, _): "en otro sitio"
```

Patrones: literales, comodín `_`, ligaduras, variantes de enum, structs, tuplas,
y alternativas con `|`.

`for x in iterable:` recorre cualquier cosa que implemente `Iterate`.
`while cond:` y `loop:` con `break` y `continue`.

**Operador `?`.** Sobre `Result[T, E]` desenvuelve `Ok` o retorna `Err`
propagando el error a través de `From`; sobre `Option[T]` desenvuelve `Some` o
retorna `None`.

```link
fn read_config(path: &String) -> Result[Config, Error]:
    let text = read_file(path)?
    let config = parse(&text)?
    Ok(config)
```

**Precedencia**, de mayor a menor:

```
llamada, índice, acceso a campo, ?
- not & &mut          (unarios)
as
* / %
+ -
<< >>
&
^
|
== != < > <= >=
and
or
=                     (asignación, no asociativa)
```

## 5. Sistema de tipos

### 5.1 Tipos primitivos

`I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`, `F32`, `F64`, `Bool`,
`Char`, `String`, `()`.

`Int` es alias de `I64`, `Float` de `F64`. Los literales enteros sin sufijo
adoptan `Int` salvo que el contexto imponga otro. No hay conversiones
implícitas entre tipos numéricos: se escriben con `as`, y `as` es explícito
también cuando pierde precisión.

### 5.2 Inferencia

Local. Las firmas de las funciones se anotan por completo; dentro de un cuerpo
se infiere todo. El algoritmo es unificación con variables de tipo, resuelto por
función. Las cotas de trait se comprueban tras la unificación.

Hindley-Milner global queda descartado: no cierra en presencia de traits ni de
regiones, y sus errores señalan lugares arbitrariamente lejanos del fallo real.

### 5.3 Genéricos

Por monomorfización: cada instanciación distinta genera código especializado.
Coste nulo en ejecución, a cambio de tiempo de compilación y tamaño de binario.
El despacho dinámico existe para cuando esa compensación no interesa, y en F2
será necesario: la lista de herramientas de un agente es heterogénea.

### 5.4 Traits

Resolución estática. Un `impl` solo es legal si el trait o el tipo pertenecen al
módulo que lo declara (regla del huérfano), lo que garantiza que no puedan
existir dos implementaciones en conflicto. Sin especialización y sin tipos
asociados en F0: se añaden si F1 o F2 los necesitan.

Traits de la biblioteca reconocidos por el compilador: `Copy`, `Clone`, `Drop`,
`Eq`, `Ord`, `Iterate`, `From`.

### 5.5 Ausencia y fallo

No existe `null`. La ausencia es `Option[T]`; el fallo es `Result[T, E]`.
Esto importa desde F0 aunque parezca prematuro: en F2 la salida de un modelo es
el caso arquetípico de "esto puede no encajar con lo que pediste", y el lenguaje
debe llegar allí con la forma correcta de expresarlo ya establecida.

## 6. Ownership y regiones

### 6.1 Reglas

1. Todo valor tiene exactamente un dueño.
2. Asignar, pasar o devolver un valor **mueve** su propiedad, salvo que su tipo
   implemente `Copy`.
3. Usar un valor movido es un error (`LK0301`).
4. Un valor puede prestarse compartido (`&T`) tantas veces como se quiera, o de
   forma exclusiva (`&mut T`) una sola vez, pero nunca ambas a la vez.
5. Ningún préstamo puede sobrevivir a su referente.
6. Al salir de ámbito, un valor se destruye; si implementa `Drop`, se ejecuta su
   destructor.

No hay sintaxis de lifetimes. El programador nunca escribe una región.

### 6.2 Cómo se infieren las regiones

Una región es un conjunto de puntos del programa: aquellos donde un préstamo
concreto sigue vivo. Eso es una propiedad del flujo de control, luego el
análisis no puede hacerse sobre un árbol; necesita un grafo de flujo explícito.
De ahí que el pipeline tenga una IR de nivel medio (§7).

El algoritmo, sobre el MIR de cada función:

1. Asignar una variable de región fresca a cada préstamo y a cada referencia que
   aparezca en un tipo.
2. Recoger restricciones de supervivencia (`'a: 'b`, "la región a contiene a la
   b") desde las asignaciones, las llamadas y los retornos.
3. Propagar por punto fijo sobre el grafo de flujo hasta que ninguna región
   crezca.
4. Comprobar conflictos: todo uso de un lugar prestado mientras el préstamo
   sigue vivo, y todo préstamo que sobreviva a su referente.

**Interprocedural.** Como una firma no lleva anotaciones, las regiones de los
parámetros y del retorno de una función son parte de su resultado de análisis,
no de su declaración. Por tanto las funciones se analizan **en orden topológico
inverso del grafo de llamadas**: primero las hojas, después quien las llama.
Los ciclos (recursión mutua) se resuelven por punto fijo sobre cada componente
fuertemente conexa, empezando por la aproximación más permisiva y restringiendo
hasta estabilizar.

**Regiones en los campos de un struct.** Un struct con campos de referencia
recibe parámetros de región implícitos, inferidos en el mismo análisis y
propagados a cada sitio donde se instancia el tipo.

### 6.3 Procedencia, y por qué no es opcional

Al inferir las regiones en vez de leerlas de una anotación, el compilador se
queda sin nada del usuario a lo que culpar. Un error no puede decir "tu
anotación es incorrecta" porque no hay anotación. Tiene que reconstruir el
razonamiento: dónde nació el préstamo, qué lo mantiene vivo, y dónde choca.

En consecuencia, **cada restricción recogida en el paso 2 guarda su procedencia**
—el span y la causa que la originaron— y el solucionador conserva esa cadena
mientras propaga. Un error de región se explica recorriendo la cadena de
restricciones que llevó al conflicto.

Esto no se puede añadir después: un solucionador escrito sin procedencia es un
solucionador que hay que reescribir entero para tenerla. Se construye así desde
el primer commit.

### 6.4 Interfaz para F3

El motor de regiones expone `borrows_live_at(punto) -> Set[Borrow]` como
consulta pública. F3 la usará para rechazar cualquier préstamo vivo en un punto
de suspensión, que es lo que hace serializable el estado de un agente.

## 7. Arquitectura del compilador

### 7.1 Pipeline

```
  .link
    |  lexer              INDENT/DEDENT; span en cada token
   tokens
    |  parser
    AST                   fiel al fuente, con span en cada nodo
    |  resolución         módulos, ámbitos, imports
    HIR                   nombres resueltos a identificadores únicos
    |  tipos + traits     inferencia local, cotas comprobadas
   THIR                   completamente tipado
    |  bajada             lugares y temporales explícitos, CFG
    MIR  <---- inferencia de regiones + chequeo de ownership
    |  monomorfización    una copia por instanciación
    |  codegen            inkwell -> LLVM IR
    |  enlazado
  binario nativo
```

### 7.2 Crates

| Crate | Responsabilidad |
|---|---|
| `link-diagnostics` | Spans, errores, renderizado, sugerencias. Del que dependen todos. |
| `link-lexer` | Texto a tokens, incluida la pila de indentación. |
| `link-parser` | Tokens a AST. |
| `link-resolve` | AST a HIR: módulos, ámbitos, resolución de nombres. |
| `link-types` | HIR a THIR: inferencia, resolución de traits, exhaustividad. |
| `link-mir` | THIR a MIR: bajada a grafo de flujo. |
| `link-regions` | Inferencia de regiones y chequeo de ownership sobre MIR. |
| `link-codegen` | Monomorfización y emisión de LLVM IR vía `inkwell`. |
| `link-rt` | Runtime mínimo, enlazado estáticamente en cada binario. |
| `link-db` | Definición de las queries de `salsa` y la base de datos del compilador. |
| `linkc` | Driver: línea de órdenes, orquestación, enlazado. |

### 7.3 Arquitectura de queries

Cada fase se expresa como consultas memoizadas de `salsa` sobre un grafo de
dependencias, no como una función que se llama una vez. Cambiar un fichero
invalida solo las consultas que dependían de él.

Consultas principales:

```
source_text(FileId) -> String                    # entrada
tokens(FileId) -> Vec<Token>
ast(FileId) -> Ast
module_tree() -> ModuleTree
hir(ModuleId) -> Hir
signature(DefId) -> Signature
type_of(DefId) -> Type
thir(DefId) -> Thir
mir(DefId) -> Mir
call_graph() -> CallGraph
region_result(DefId) -> RegionResult              # requiere las hojas primero
mono_items() -> Vec<MonoItem>
llvm_module(CodegenUnit) -> LlvmIr
```

`region_result` es la única consulta que depende del grafo de llamadas en lugar
de solo de sus propias entradas, por lo explicado en §6.2. Se implementa
pidiendo `region_result` de cada callee, lo que deja que `salsa` construya el
orden correcto por sí mismo; la recursión se detecta y se resuelve con el punto
fijo de la componente.

## 8. Runtime y biblioteca estándar

`link-rt` es un runtime mínimo enlazado estáticamente: reserva de memoria sobre
el asignador del sistema, `panic` con mensaje y aborto, y las representaciones
de los tipos de la biblioteca. Sin recolector de basura y sin hilos en F0.

Biblioteca estándar de F0, deliberadamente pequeña:

- `Option[T]`, `Result[T, E]`, `Box[T]`
- `String` (UTF-8, con propiedad) y `&String` para vistas
- `Array[T]` (crecible, contigua), `Map[K, V]` (tabla hash)
- `print`, `println`
- `read_file`, `write_file`
- Traits: `Copy`, `Clone`, `Drop`, `Eq`, `Ord`, `Iterate`, `From`

Todo lo demás es F1 o posterior.

## 9. Diagnósticos

Cada nodo del AST, del HIR y del MIR lleva su span desde el léxer, sin
excepción. Un span perdido en una fase temprana es un error imposible de
localizar en una tardía.

Un diagnóstico tiene código (`LK0142`), severidad, mensaje, span primario, spans
secundarios con sus etiquetas y, cuando procede, una sugerencia aplicable
automáticamente.

Rangos de códigos:

| Rango | Fase |
|---|---|
| `LK0001`–`LK0099` | Léxico |
| `LK0100`–`LK0199` | Sintaxis |
| `LK0200`–`LK0299` | Resolución y tipos |
| `LK0300`–`LK0399` | Ownership y regiones |
| `LK0400`–`LK0499` | Codegen y enlazado |

## 10. Estrategia de pruebas

Cuatro capas, todas desde el primer commit. El desarrollo es dirigido por
pruebas: el test antes que el código.

1. **Unitarias por crate.** Cada crate prueba sus propias invariantes.
2. **Snapshot** con `insta`, sobre volcados de AST, HIR y MIR. Detectan
   regresiones en fases intermedias que ninguna prueba de ejecución observa.
3. **UI tests** al estilo de rustc: un `tests/ui/*.link` que debe fallar, junto a
   su `.stderr` esperado, comparado literalmente. Es el único método conocido
   que impide que los mensajes de error se degraden con el tiempo. Para un
   lenguaje con regiones inferidas no es opcional.
4. **De ejecución.** Compilar a binario, ejecutarlo, comparar salida estándar y
   código de retorno.

## 11. Definición de terminado

F0 está terminado cuando se cumplen todas estas condiciones:

1. `linkc` compila a binario nativo un programa que use, a la vez: genéricos con
   cotas, traits con despacho estático y dinámico, `match` exhaustivo sobre
   enums, `Option` y `Result` con `?`, y un struct que guarde un préstamo en un
   campo.
2. Toda violación de ownership produce un error con el span correcto y la cadena
   de préstamos que lo explica, cubierta por UI tests.
3. Un `match` no exhaustivo se rechaza listando los patrones que faltan.
4. La recompilación es incremental: modificar uno de tres ficheros no recompila
   los tres.
5. La suite completa pasa en limpio.

## 12. Fuera de alcance

Lo que F0 **no** incluye, para que no se cuele por la puerta de atrás:

- Agentes, herramientas, prompts, tensores, concurrencia, durabilidad (son F1–F4).
- Tipos asociados, especialización de traits, genéricos const.
- Closures que capturan por referencia. Las closures de F0 capturan por
  propiedad; el caso por referencia se revisa en F1.
- Macros de cualquier clase.
- Language server, formateador, gestor de paquetes.
- Compilación cruzada y objetivos distintos del anfitrión.
- Optimizaciones propias: se delega todo en los pases de LLVM.
