# Contrato de la traza

`trace.schema.json` (JSON Schema 2020-12) define la única interfaz entre el motor que ejecuta
programas y el reproductor que los anima. El motor solo escribe trazas; el reproductor solo las lee.

## Ideas centrales

- **Pasos con estado completo.** `steps[t]` describe todo el sistema después del paso `t`:
  procesos, hilos, fds, pipes, señales en vuelo y objetos de sincronización. Retroceder es leer
  `steps[t - 1]`. (La traza compacta, más abajo, guarda diferencias, pero al leerla se obtienen
  estos mismos pasos completos.)
- **Memoria por referencia.** `process.mem` apunta a una instantánea completa en `snapshots`. Las
  instantáneas se deduplican por contenido: en cada paso solo cambia la memoria del proceso que
  avanzó, así que los demás reutilizan la suya.
- **Salida como registro.** `output` guarda cada escritura a la terminal con su `t`. La consola de un
  proceso en el paso `t` son sus fragmentos con `chunk.t ≤ t`.
- **Bytes exactos.** `bytes`, `buffer` y `stdin` son strings donde cada carácter U+0000–U+00FF es un
  byte. El reproductor los decodifica como UTF-8 para mostrarlos.
- **Reproducible.** Sin marcas de tiempo ni rutas del computador: la misma entrada, semilla y
  planificación producen el mismo archivo, byte a byte.

## Semántica de un paso

- `actor`: la tarea (proceso, hilo) que avanzó. `null` en `t = 0` y en pasos del kernel, por ejemplo
  cuando el reloj virtual salta porque todos esperan.
- `executed`: la línea que el actor acaba de ejecutar (flecha verde de Python Tutor). Es siempre la
  línea donde ese hilo estaba detenido al terminar el paso anterior.
- `thread.line`: la próxima línea que ejecutará ese hilo (flecha roja).
- `choices`: las tareas listas entre las que eligió el planificador; la política manual las ofrece
  como botones "avanzar este".
- Un hilo que no fue elegido en el paso `t` estuvo, durante ese paso, en el estado que tenía al
  terminar `t − 1`.

## Valores de memoria

Árbol con `kind`: `scalar`, `pointer` (con `target` o `null`), `struct`/`union` (con `fields`),
`array` (con `length` total e `items`, que puede traer menos) y `opaque`. Cada variable y campo
lleva su dirección (`addr`) y su tamaño (`size`), con lo que el reproductor resuelve a qué celda
apunta cada puntero sin interpretar C. Un arreglo sin `addr` por elemento calcula
`addr + i · size / length`.

## Traza compacta (versión 2)

Para ejecuciones largas (hasta un millón de pasos) el tracer escribe `compact.schema.json`: el mismo
contenido, pero sin repetir el estado completo en cada paso.

- **Bloques.** Los pasos van en bloques de hasta 1.000, cada uno con gzip y en base64. El primero de
  cada bloque es completo; los demás son parches del anterior, tomado con `events` vacío.
- **Parches.** Un valor primitivo o `null` reemplaza; `{"$s": v}` reemplaza por un objeto o arreglo;
  en un objeto, cada clave lleva su parche y `{"$d": 1}` la borra; en un arreglo, las posiciones
  llevan su parche y `$a` da el largo nuevo si cambió. Ninguna clave de la traza empieza con `$`.
- **Instantáneas por bloque.** Un bloque define todas las instantáneas que nombran sus pasos, completas
  o como `{"$from": id, "$p": parche}` de otra del mismo bloque. Las de un bloque son
  `m{firstSnapshot}` en adelante; así cada bloque se decodifica por su cuenta.
- **Índice.** Por cada paso: PID y TID del actor, la línea donde quedó y un entero con un bit por
  clase de evento (el orden está en el esquema). Con eso la línea de tiempo, el siguiente evento,
  el paso de un hilo y "hasta la línea" no abren bloques. También van los procesos, hilos y mutex
  en orden de aparición y los forks, que dan los colores y la genealogía.

`crates/trace-model` codifica (`compact::Encoder`) y decodifica (`compact::decode`) esta versión;
la interfaz la lee con `web/src/trace/loaded.ts`. Las trazas de `traces/compact/` las genera Rust y
las pruebas de la interfaz comprueban que se leen igual que sus originales.

## Cambios

Cualquier cambio al esquema debe:

1. Regenerar los tipos: `make types`.
2. Actualizar `crates/trace-model` (la prueba `roundtrip` falla si no coinciden) y regenerar las
   trazas compactas de muestra: `UPDATE_REFERENCE=1 cargo test -p trace-model`.
3. Regenerar las trazas sintéticas si cambia su forma: `make synth`.
