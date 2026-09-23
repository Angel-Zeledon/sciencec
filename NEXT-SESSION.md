# Traspaso — estado al 2026-09-23

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

**El paralelismo ya no hace falta acotarlo.** La carrera del scratch se cerró
en `be46f94`: cada test end-to-end tiene su propio directorio. Tres corridas
consecutivas a paralelismo completo dieron 2230/0. Si volvés a ver conjuntos de
fallos *distintos* entre corridas, es una carrera nueva, no esta.

## El número

**17 de 20 ejemplos** compilan, enlazan, corren y tienen su salida fijada byte
por byte. El denominador es 20: `17_modules` importa módulos que la biblioteca
F0 no tiene y `20_extern` es una biblioteca sin `main` a propósito, las dos
cerradas por decisión documentada.

Suite: **2231 en verde**.

## Los tres ejemplos que faltan

Cada bloqueo está bisecado y verificado corriendo programas.

### `03_structs` — el front end tira los argumentos del receptor

`Grid[Int, 3, 3].area()` baja a MIR como `_4 = area()`: **sin argumentos y sin
receptor**. `area` no toma `self` y su firma `() -> Int` no menciona `T`,
`ROWS` ni `COLS`, así que `Mono::solve_call` —que recupera genéricos unificando
tipos declarados contra reales— no tiene nada contra qué unificar.

Más arriba, `check`'s `associated_call` **calcula el tipo concreto del receptor
y lo descarta**: el nodo del callee se empuja en `Ty::ERROR` y nunca se
actualiza, y `mir::Callee::Def(DefId)` no lleva argumentos.

La mitad de sustitución **ya está hecha** (`d0ebda1`): `Instance::const_arg` y
la consulta al `MonoSet` en `lower_operand`. Lo que falta es que
`science-types` conserve los argumentos del receptor y probablemente que
`science-mir` le dé a `Callee` dónde ponerlos. **Es un cambio de front end.**

### `06_traits` — `Display` necesita `Formatter`

**Decisión tomada por el autor: gana el spec.** `strings-formatting-and-docs.md`
§3.1 decide `def display(self, into: mutable &Formatter)` con un conjunto
cerrado de cinco métodos, y el corpus —que escribe `def display(self) -> String`
en `06` y `00`— es el que está desactualizado.

Hay trabajo a medias en la rama **`wip/agents-formatter-constgenerics`**: 1221
líneas que declaran `Formatter`, `FormatSpec` y los cuatro `choice`
(`Align`, `Sign`, `Code`, `Grouping`), más el renderer de `science-rt` con sus
tests. **No está mezclado a propósito**, por dos razones:

1. Murió una edición antes de declarar `Display` en `INTERFACE_DECLS`, que era
   el punto.
2. Rompe **32 snapshots** de `science-resolve` — `Module main #372` → `#409`,
   deriva mecánica de `DefId` por las definiciones nuevas del preludio.

Al retomar: terminar la entrada de `Display` primero y re-bendecir los
snapshots **en el mismo commit**, para que el corrimiento y su razón lleguen
juntos.

**Riesgo conocido:** declarar métodos en una interfaz del preludio que no los
tenía ya rompió este corpus una vez, con `Clone.clone`. Si vuelve a pasar,
revertir la declaración y explicar por qué — **no** tocar el ejemplo para
taparlo.

### `00_kitchen_sink` — cierres con capturas, boxes, operadores

Es el último por construcción. No es una tarea: es la unión de tres.

## Un bug de corrección, aparte de los ejemplos

**`BUG-field-of-method-result.md`** en la raíz. `a.scaled(2.0).x` —leer un campo
del resultado de una llamada a **método**— pasa `check` y el backend lo rechaza.
Ligarlo a un nombre primero funciona; con una **función libre** también funciona.

Está localizado exacto: `as_place` en `science-mir` materializa el resultado de
un `ExprKind::Call` en un temporal y **no tiene arm para `MethodCall`**.

**Dos arreglos probados y ambos fallaron**, con su evidencia en el documento.
No los repitas sin información nueva:

1. Ampliar el arm a ambos tipos → rompe `for c in text.chars():`, que *depende*
   de que un método no tenga lugar.
2. Materializar sólo en el arm de `Field` → deja `science-mir` en 130/0 y
   **cuelga los tests de arrays**.

El cuelgue de (2) es lo que hay que entender antes de intentar un tercero.

## Reglas que esta sesión pagó caro por aprender

- **Evidencia:** "compila" no prueba nada. Construí, enlazá, **corré**, y mirá
  la salida. Y si el resultado es raro, verificá que ningún `cargo` esté
  reescribiendo el binario en ese momento — medí un corpus en 10/22 que en
  realidad era 17/22 por eso.
- **Nunca debilitar una prueba.** Si una tiene que cambiar, citá antes y después.
- **Nunca `git stash` pelado** — la pila se comparte entre worktrees.
- **Nunca tocar la config de git.** Pasá `GIT_AUTHOR_*` **y** `GIT_COMMITTER_*`.
- **No commitees en `master` mientras un agente trabaja en el mismo checkout.**
  Mover `HEAD` le borró ediciones sin commitear a un agente en esta sesión. Lo
  detectó y las reaplicó, pero pudo perderse trabajo en silencio.
- **Verificá a los agentes corriendo su código.** Esta sesión: uno metió un
  espacio suelto en un archivo ajeno, otro eligió un arreglo peor que el que
  prescribía la nota que citaba, y varios reportaron "terminado" mientras
  esperaban tests. Las citas que dieron, en cambio, resultaron **exactas** las
  cuatro veces que las verifiqué.
- **Repartí por *feature*, no por ejemplo, y por *crate*, no por tarea.** `00`
  no es una tarea. Y varias features convergen en `science-codegen-llvm`: dos
  agentes editando ese archivo se pisan.

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
- **`Formatter` está especificado**, entero, con una `Decision`. El comentario
  que decía lo contrario se corrigió.

## Primer movimiento sugerido

Medí antes de creerle a este documento:

```bash
for f in examples/*.science; do
  ./target/debug/sciencec test "$PWD/$f" >/dev/null 2>&1 \
    && echo "ok   $f" || echo "FAIL $f"
done
```

Después agarrá **`03`**: es el único de los tres cuyo bloqueo está localizado
hasta la línea y cuya mitad de backend ya está construida y probada.
