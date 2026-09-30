# Traspaso — estado al 2026-09-30

> **Lo primero, porque cuesta horas cada vez.** Los mensajes de rechazo de este
> compilador envejecen peor que el código que los rodea. Nadie los relee cuando
> arregla la cosa que describen. La sesión anterior persiguió **cinco** agujeros
> que ya estaban cerrados o que culpaban a la fase equivocada.
>
> **Antes de perseguir cualquier bloqueo: reproducilo en un programa mínimo y
> creele a lo que imprime, no a lo que dice el mensaje.**

## Cómo construir y correr

```bash
export PATH="$HOME/.cargo/bin:$HOME/.homebrew/bin:$PATH"
export SCIENCE_LLVM_PREFIX="$HOME/.homebrew/opt/llvm@18"
cargo build -p sciencec --features llvm
cargo test --workspace --features llvm
```

LLVM 18 está en `~/.homebrew`, no en `/opt/homebrew`. Sin `--features llvm` el
binario rehúsa con `SC0400`, y `cargo test --workspace` **sin** features
reemplaza `target/debug/sciencec` por uno sin backend — si de golpe todos los
ejemplos fallan, es eso.

**El paralelismo ya no hace falta acotarlo por el scratch.** Esa carrera se
cerró en `be46f94`: cada test end-to-end tiene su propio directorio. Si volvés a
ver conjuntos de fallos *distintos* entre corridas, es una carrera nueva, no
esta. Lo que sí conviene, en una máquina con muchos worktrees, es medir en
serie (`-- --test-threads=1`): el daemon de verificación de macOS serializa el
primer arranque de cada binario recién enlazado contra el presupuesto de diez
segundos de `harness::RUN_BUDGET`, y un `killed after not exiting within 10s`
es la cola, no tu cambio.

## El número

> **No lo copies a mano.** Vive en `UNMEASURED`, en
> `crates/sciencec/tests/corpus_output.rs`: una fila por cada ejemplo que *no*
> se mide, con su razón, y `tally` al lado deriva la cuenta de esa tabla más el
> listado del directorio. Las cifras de acá abajo las relee
> `no_document_quotes_a_corpus_count_this_file_does_not` contra esa tabla, así
> que una cifra vieja en este archivo es un test en rojo y no una sorpresa para
> el que venga. Llegaron a convivir **cuatro** denominadores en el repo (20, 21,
> 22 y un 15/20 suelto) justo porque cada uno estaba tipeado a mano al lado de
> la frase que lo necesitaba.

**20 de 20 ejemplos** compilan, enlazan, corren y tienen su salida fijada byte
por byte. El denominador es 20 y no 22 programas porque dos de los 22 archivos
no son programas, cada uno por su propia razón y las dos buenas:

- **`20_extern`** es una biblioteca sin `main` a propósito — declaraciones
  `extern`, un tipo handle y tres wrappers —, y §11 tiene `SC0403` justamente
  para rechazarla. Lo pincha desde los dos lados
  `a_pure_declarations_file_has_no_entry_point_by_design`: `sciencec test` la
  rechaza con `SC0403`, `sciencec check` la acepta limpia.
- **`17_modules`** importa `text.parser` y `compiler.frontend.lexer`: módulos
  **del propio crate**, resueltos contra el directorio del archivo de entrada,
  que a propósito no existen. **No es un agujero de la biblioteca F0** — esa
  frase estaba acá y era falsa, prestada del ítem de `ffi.Span`, donde sí
  corresponde, y no transfiere: §8 no tiene nada que ver. El argumento real
  está en `UNRESOLVED`, en `crates/sciencec/tests/cli.rs`, que además fija la
  cuenta de diagnósticos en cuatro: el archivo existe para mostrar las dos
  formas de `use` que escribe el spec, y escribir un `examples/text/parser.science`
  para conformarlo convertiría un ejemplo de sintaxis en un fixture y taparía
  justo el diagnóstico que este corpus mide.

**No falta ninguno.** `00_kitchen_sink` cerró en `7914ce0` y con él la Puerta A.

Suite: **2395 en verde**, medida en serie (`--test-threads=1`; ver la regla sobre
el reloj de pared más abajo).

> **Un pin presente no prueba nada por sí solo.** Arreglar `Drop::drop` de
> usuario agregó **11 líneas en tres `.stdout`** (`06_traits` +1,
> `18_ownership` +8, `19_stdlib` +2), todas altas puras: ni una línea borrada
> ni cambiada. `19_stdlib.science:249` imprime `"record dropped"` dentro de un
> bloque `Drop` y su `.stdout` commiteado tenía **cero** ocurrencias. Tres
> expectativas habían sido bendecidas contra un defecto real y lo estaban
> certificando. Un test que le da la razón a un bug en silencio es peor que no
> tener test.

## El ejemplo que falta

Cada bloqueo está bisecado y verificado corriendo programas.

### ~~`03_structs`~~ — cerrado

`Grid[Int, 3, 3].area()` imprime `9`. El canal que faltaba se construyó en dos
commits: `mir::Callee::Def` pasó a variante de struct con un
`self_ty: Option<Ty>`, y `thir::ExprKind::Call` ganó el campo del mismo nombre
que `check`'s `associated_call` llena con el tipo concreto del receptor que ya
calculaba y tiraba. `Mono::solve_call` lo unifica contra el self type declarado
del bloque —la misma unificación que ya hacía contra `args[0]`, para una
llamada que no tiene `args[0]`— y resuelve `T`, `ROWS` y `COLS` de una vez.

El cambio fue **más chico que lo presupuestado**: 50 sitios era el conteo de
`Callee` entero, y `science_codegen::backend::Callee` es un enum **distinto**
(`Runtime`/`Science`/`Foreign`/`Intrinsic`/`Indirect`) que no se tocó. Los
sitios reales de `mir::Callee::Def` eran ~17 en siete archivos.

Pinchado por `an_associated_call_on_a_generic_type_is_solved_from_its_receiver`
(`crates/science-codegen/tests/mono.rs`), que falla si el campo vuelve a `None`
en cualquier punto del recorrido — el dump de MIR no lo muestra y nada más se
daría cuenta.

### ~~`06_traits`~~ — cerrado

`Display` se declaró con el `Formatter` de §3.1 en `baf5b70` y el ejemplo
imprime un tipo de usuario. **Se ganó el spec, como estaba decidido.** El
riesgo que este documento anticipaba —declarar métodos en una interfaz del
preludio que no los tenía rompió este corpus una vez, con `Clone.clone`— no se
materializó.

### ~~`00_kitchen_sink`~~ — cerrado, y con él la Puerta A

Compila, enlaza, corre y sale 0. Su salida está fijada en
`examples/00_kitchen_sink.stdout`, su fila salió de `UNMEASURED`, y el corpus
es **20 de 20**. Eso es la condición 1 del "definition of done" de §11 y la
Puerta A1 de `self-hosting.md`: el backend de F0 está terminado.

Lo que faltaba era el vocabulario de cadenas —`iterate`, `discard`, `map`,
`take`, `sorted(by:)`, `collect`— y está en el preludio con una bajada fusionada
en `science-mir` (`Builder::lower_chain_collect`). **Seis de los treinta y ocho
combinadores de §1.4 están transcriptos**, exactamente los que el ejemplo
escribe; los otros treinta y dos siguen en silencio por la fila de
`WHOLLY_OPEN`, que dice por qué. Lo que no baja, y lo dice al rechazarlo:

- una cadena **guardada en una variable** (§2.3), porque ningún adaptador tiene
  representación en tiempo de ejecución;
- un eslabón **después** de `sorted(by:)` (§1.4 lo llama barrera), porque qué
  hacer con el buffer de ítems propios es una pregunta de pertenencia que la
  nota no contesta.

## Un bug de corrección, aparte de los ejemplos — **cerrado**

**`BUG-field-of-method-result.md`** en la raíz. `a.scaled(2.0).x` y
`h.items()[0]` —leer un campo o un índice del resultado de una llamada a
**método**— pasaban `check` y el backend los rechazaba con `SC0400`. Los dos
arms de `as_place` que lo necesitaban (`Field` e `Index`) materializan hoy una
base sin lugar propio en un temporal.

El documento se conserva porque dos de las tres cosas que se aprendieron ahí son
**callejones sin salida**, y borrarlo es invitar a recorrerlos otra vez:

1. §3: ampliar el arm de `ExprKind::Call` a `MethodCall` rompe
   `for c in text.chars():`, que *depende* de que un método no tenga lugar. Es
   un invariante real, no un snapshot por actualizar.
2. §4: el "cuelgue de los tests de arrays" **no existía**. Era el daemon de
   verificación de macOS serializando el primer arranque de cada binario recién
   enlazado, contra el presupuesto de diez segundos de `harness::RUN_BUDGET`.
   Si volvés a ver un timeout en un solo test de ejecución, volcá el MIR con y
   sin el cambio y comparalo antes de creerle.
3. §5.1: lo único que el arreglo de `Index` **sí** cambia en un programa que hoy
   compila es `h.items()[0] be 5` — un destino de asignación que `check` acepta
   y que antes se descartaba en silencio. Sigue sin efecto, pero ahora el índice
   se verifica. Que el front end acepte ese destino queda abierto.

## Reglas que esta sesión pagó caro por aprender

- **Evidencia:** "compila" no prueba nada. Construí, enlazá, **corré**, y mirá
  la salida. Y si el resultado es raro, verificá que ningún `cargo` esté
  reescribiendo el binario en ese momento — medí un corpus en 10/22 que en
  realidad era 17/22 por eso.
- **Nunca debilitar una prueba.** Si una tiene que cambiar, citá antes y después.
- **Un ejemplo que no se mide tiene que estar nombrado.** El arm
  `Verdict::NotBuilt` de `corpus_output.rs` era un `continue` pelado, y por eso
  `00_kitchen_sink` estaba fuera del corpus sin aparecer en un solo mensaje,
  dentro de una exclusión que todo el mundo describía como de dos archivos.
  Hoy la lista es `UNMEASURED` y la corrida falla en las dos direcciones: un
  archivo que deja de construir y no está listado, y un archivo listado que
  construye igual.
- **Nunca `git stash` pelado** — la pila se comparte entre worktrees.
- **Nunca tocar la config de git.** Pasá `GIT_AUTHOR_*` **y** `GIT_COMMITTER_*`.
- **No commitees en `master` mientras un agente trabaja en el mismo checkout.**
  Mover `HEAD` le borró ediciones sin commitear a un agente en esta sesión. Lo
  detectó y las reaplicó, pero pudo perderse trabajo en silencio.
- **Verificá a los agentes corriendo su código.** Las citas que dan resultan
  exactas casi siempre; las *conclusiones* no. Verificando cambió el resultado
  cuatro veces en una sola sesión: una fuga de `Map` era un tercio de lo
  reportado, un "cuelgue" no existía, una regresión atribuida a un cambio era un
  staticlib viejo, y tres tandas de fallos eran artefactos de máquina.
- **Repartí por *feature*, no por ejemplo, y por *crate*, no por tarea.** `00`
  no es una tarea. Y varias features convergen en `science-codegen-llvm`: dos
  agentes editando ese archivo se pisan. La convención que ese archivo ya sigue
  y conviene mantener: agregá un campo y un builder, no cambies una firma.
- **Medí en serie.** `harness::RUN_BUDGET` son diez segundos de **reloj de
  pared**, y macOS serializa a nivel de máquina el primer arranque de cada
  binario recién enlazado y sin firmar. Un mismo árbol dio *33 pasan / 17 fallan
  en 37,8 s* bajo carga y *50 / 0 en 11,6 s* solo. Si ves
  `[harness] killed after not exiting within 10s`, es la cola, no tu cambio.
- **Mirá el disco antes de creerle a un fallo.** 76 worktrees a ~2 GB de
  `target` cada uno llenaron un volumen de 460 GB. Un `No space left on device`
  se ve exactamente igual que un test roto: una corrida dio 13 fallos y otra
  115, y el conteo de "no space" coincidía 1 a 1 en las dos.
- **Para medir una fuga, mantené constante el conjunto vivo.** Insertá la misma
  clave muchas veces, o asigná y soltá en un bucle. Un informe reportó 130 MB de
  fuga que eran 300 000 claves distintas reteniéndose legítimamente, y yo caí en
  lo mismo antes de darme cuenta.
- **Una prueba también envejece, y miente más caro que un comentario.** Tres
  `.stdout` estaban bendecidos contra un defecto real: `19_stdlib.science:249`
  imprime `"record dropped"` dentro de un `Drop` y su pin tenía cero
  ocurrencias. Y el harness de tests construía un programa **distinto** del que
  construye el driver — se saltaba los dos pasos de revelado de
  `check_and_lower`—, así que un alias `extern` andaba desde la línea de
  comandos y la suite lo rechazaba.
- **Un número corregido al lado de una razón vieja es la mitad más cara.** Un
  barrido arregló el conteo de una fila de `DREAM.md` y dejó intacta la
  explicación, que nombraba dos cosas que ya funcionaban. Un número equivocado
  se le nota a cualquiera que corra el corpus; una razón equivocada manda al
  próximo lector al crate equivocado y nada de lo que corra lo contradice.
- **Los agentes mueren sin commitear.** Rescatá antes de evaluar: dos cayeron con
  2092 y 245 líneas sueltas, y uno que nunca obtuvo worktree dejó 529 líneas en
  el checkout principal que un merge habría pisado. Lo frenó git, no el proceso.

## Lo que resultó no estar roto

Verificado corriendo programas, no leyendo código. Si un mensaje te manda a uno
de estos, el mensaje miente:

- **La monomorfización.** Funciona. El agujero era `Clone::clone` sobre
  escalares, cerrado en `1da5b75`.
- **`for` sobre un `Iterate` propio.** Cerrado desde `757aa5d`. Un rango nunca
  pasó por ese camino.
- **Operadores sobre tipos de usuario.** `a + b` sobre un `implements Add`
  compila y corre. `Unresolved::Operator` no se construía en ningún lado y se
  eliminó en `19473a4`.
- **`Formatter` está especificado**, entero, con una `Decision` — y desde
  `baf5b70` está además **implementado**: `Display` se declara con él y
  `06_traits` imprime un tipo de usuario.
- **`19_stdlib` ya no depende del directorio desde el que lo corras.** Desde
  `2653270` el ejemplo **crea** el archivo que lee, en vez de esperar que
  `science.toml` esté ahí. La advertencia que había acá quedó vieja.

## Primer movimiento sugerido

Medí antes de creerle a este documento. Corré cada ejemplo en un directorio
**vacío**, que es lo que hace `corpus_output.rs`: un archivo que el corpus no
versiona es un archivo que el corpus no tiene que ver.

```bash
for f in "$PWD"/examples/*.science; do
  d=$(mktemp -d)
  ( cd "$d" && "$OLDPWD/target/debug/sciencec" test "$f" >/dev/null 2>&1 ) \
    && echo "ok   $(basename "$f")" || echo "FAIL $(basename "$f")"
  rm -rf "$d"
done
```

Esperá `FAIL` en exactamente `17_modules` y `20_extern` — los dos que están en
`UNMEASURED`, y nada más. Si aparece un tercero, el corpus también te lo va a
decir: `cargo test -p sciencec --features llvm --test corpus_output` lo nombra
en vez de saltearlo, y falla igual si un archivo *listado* empieza a construir.

## Y después, qué

La Puerta A está cerrada: el backend de F0 construye los veinte programas que el
proyecto se propuso. Lo que sigue son las dos compuertas que §10 todavía tiene
abiertas, y las dos están **medidas**, no supuestas.

**Compuerta C1, etapa 4 — cerrada.** La prueba de ejecución que pide está en
`crates/sciencec/tests/cli.rs`,
`gate_c1_builds_its_collections_and_writes_a_dump_through_a_buffered_writer`:
`Map[String, Int]`, `Set[DefId]`, `Array[Box[Expr]]`, un archivo partido con
`lines()`, un volcado escrito por un `BufferedWriter` sobre un `File`, dos
corridas desde directorios vacíos y salida y archivo idénticos byte por byte.
La mitad de *"dos máquinas"* no la puede mostrar una prueba en una sola.

**`io` es el primer módulo de la biblioteca escrito en Science**:
`crates/science-resolve/stdlib/io.science`, empaquetado con `include_str!` y
alcanzado por `use io` como un archivo hermano, a través del `open` de
`collect_crate` — en el driver, en `tests/ui.rs` y en el harness de
`science-codegen-llvm`, los tres iguales. Un `io.science` del usuario al lado
de la entrada gana (§6.3: la toolchain va última). Lo que ese archivo decide
por su cuenta está escrito en su cabecera: `File` vive en `io` y no en el
preludio (el preludio es datos y no puede tener cuerpos), el error es
`FileError` y no `IoError`, `create` toma `&String` porque no hay `Path`, y
`BufferedWriter` vacía el buffer al soltarse sin poder reportar el error.

Para llegar hubo que arreglar, cada uno con su prueba:

- **Dos miscompilaciones silenciosas.** `IoError.message()` se bajaba como
  despacho por vtable sobre un byte (trap, o nada impreso y salida 0), y
  `emit(&mut c)` contra `sink: &mut W` tomaba el préstamo dos veces: el
  callee escribía en el temporal y `c` quedaba igual. Además `&c` contra
  `&mut W` pasaba el chequeo.
- **Toda reasignación perdía memoria**: `x be f()` no soltaba el valor viejo.
  `science-mir` hace ahora *drop-and-replace* (valor nuevo a un temporal,
  `Drop` del destino, move), y `drops.rs`/`moves.rs` elaboran un `Drop`
  proyectado por campo.
- `moves::decompose` perdía de vista un campo cuyo tipo tiene `Drop` propio.
- **Decision 12, AMENDMENT 4**: el `drop` del tipo corre antes que sus campos
  (el orden de Rust). Con el orden viejo `BufferedWriter` vaciaba un buffer ya
  liberado en un archivo ya cerrado.
- Un `drop` de un tipo genérico se instancia por tipo concreto:
  `MonoSet::user_drops`, llenado recorriendo cada terminador `Drop`.
- Una cota en el bloque (`Holder[W: Speak] has:`) no llegaba a los métodos;
  una llamada no podía escribir directo en un campo; `Error?` en el preludio
  era otro tipo que en el código del usuario; `Write.write` se confundía con
  la función libre `write`.

Siguen abiertos, fuera de la compuerta: `match` sobre un `&Box[Expr]` no ve a
través del `Box` (un método sí), y `String.parse_int` no tiene bajada.

**Compuerta C2, etapa 5 — cerrada en `18550da`.** `&Array[T]` → `ffi.Span[T]`
existe, y según ese commit `cblas_ddot` corre contra el BLAS de Accelerate (no lo volví a medir).

Lo demás que quedó abierto está nombrado donde vive, no acá: una cadena guardada
en variable y un eslabón después de `sorted(by:)` se rechazan diciendo por qué;
las AMENDMENT 13 y 15 de `collections-and-chains.md` no están; `Ord` sigue sin
un `Ordering` que ninguna nota especifica; y `a is a` da `SC0334`, que es una
limitación preexistente de `science-regions` que `is` volvió alcanzable.
