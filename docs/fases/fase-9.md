# Fase 9: teoría paso a paso y código redimensionable

## Qué se hizo

- **Lecciones de los 17 ejemplos** (`examples/<id>.md`, junto a cada `.c`). Cada una tiene:
  - título y objetivo;
  - la teoría de introducción;
  - una nota por cada línea que se ejecuta (`## Línea N · \`código\``), en el orden de la ejecución;
  - un cierre («Al terminar») y ejercicios («Para practicar»).

  Las notas se escribieron mirando la traza de referencia de cada ejemplo, así que describen lo que
  realmente se ve: cuántos bytes lee el `read`, qué devuelve cada `fork`, cuándo se interrumpe un
  `sleep`, en qué paso se forma el deadlock. Cuando algo solo se ve interviniendo (Ctrl+C en
  `11_sigint`, una planificación aleatoria en `14_productor_consumidor`, EOF en `entrada_estandar`),
  la lección dice cómo provocarlo.
- **Fichas de conceptos** (`web/src/lessons/conceptos.md`): 42 fichas con la teoría de cada tipo de
  evento: procesos, fork, exec, wait, zombies y huérfanos, pipes, dup2, EOF, SIGPIPE, señales,
  handlers, acciones por defecto, mutex, variables de condición, semáforos, malloc, free, errores de
  memoria, deadlock y límites. `conceptsOf(step)` (`web/src/lessons/index.ts`) elige las fichas de
  los eventos de cada paso, así que también acompañan a los programas propios.
- **Panel Teoría** (`web/src/lessons/TheoryPanel.tsx`), una tercera columna que se oculta y recuerda
  si estaba oculta. Muestra:
  - en el paso 0, la introducción;
  - después, la nota de la línea recién ejecutada y las fichas de los eventos del paso;
  - en el último paso, el cierre y los ejercicios.

  **Lección completa** (`LessonDialog.tsx`) muestra toda la lección en un diálogo modal accesible.
  «Ir a esta línea» lleva al paso que ejecuta esa línea (`stepRunningLine` en
  `web/src/trace/query.ts`): ese paso muestra la nota. El texto usa un Markdown mínimo
  (`Markdown.tsx`) que arma elementos de React y nunca inserta HTML.
- **Código redimensionable** (`web/src/app/CodeResize.tsx`).
  - Las cuatro esquinas del panel de código se arrastran: las de la derecha y las de abajo agrandan
    hacia afuera, y las de la izquierda y las de arriba al arrastrar hacia la izquierda o hacia arriba
    (el panel está pegado al borde).
  - El ancho es el de la columna; el alto, el del panel.
  - Hay límites para que el lienzo conserve al menos 360 px.
  - Doble clic restablece el tamaño. La esquina inferior derecha también funciona con teclado
    (flechas, Shift para pasos grandes, Inicio para restablecer).
  - El tamaño se guarda en `localStorage`.
- **Corrección del tracer.** Si una llamada a una función del usuario era lo último de su línea, al
  retornar se caía justo al inicio de la línea siguiente, y esa dirección no se revisaba como
  parada: la línea se ejecutaba dentro del paso del `return`. En `01_structs`, las líneas 26 y 27
  no tenían paso propio. Ahora la dirección de retorno se revisa como parada. De las referencias,
  solo cambió la de `01_structs`, y su prueba verifica que cada línea de `main` sea un paso.

- **Imagen Docker.** La etapa que compila la interfaz no copiaba `examples/`. Sin esa carpeta, la
  galería de Docker solo tenía las trazas grabadas, sin los programas editables ni sus lecciones.
  Ahora se copia.

## Cómo se verificó

- `web/src/lessons/lessons.test.tsx`:
  - cada ejemplo tiene lección, con todas sus partes;
  - el código citado en cada nota está en esa línea del `.c`;
  - cada nota corresponde a una línea que la traza de referencia ejecuta, salvo las que necesitan
    intervenir, que están listadas;
  - existe una ficha por cada concepto y ninguna sobra;
  - todo evento de las trazas de referencia y sintéticas lleva a fichas que existen;
  - el Markdown no deja asteriscos ni comillas invertidas sueltos.
- `web/src/trace/loaded.test.ts`: `stepRunningLine` coincide con buscar paso por paso en las 17
  trazas de referencia, desde el inicio, la mitad y el final.
- `web/e2e/theory.spec.ts`:
  - la teoría de `03_fork_simple` en el inicio, en el fork y al final;
  - la lección completa: orden, teclas bloqueadas, «Ir a esta línea» y Escape;
  - la accesibilidad del diálogo;
  - las fichas sin lección;
  - ocultar la teoría y que se recuerde;
  - arrastrar las esquinas, el teclado, la persistencia y el doble clic.

  Las pruebas de accesibilidad existentes revisan el panel Teoría en los dos temas, y las que
  recorren cada ejemplo completo lo hacen con el panel abierto.
