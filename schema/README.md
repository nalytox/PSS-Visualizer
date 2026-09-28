# Contrato de la traza

`trace.schema.json` (JSON Schema 2020-12) define la única interfaz entre el motor que ejecuta
programas y el reproductor que los anima. El motor solo escribe trazas; el reproductor solo las lee.

## Ideas centrales

- **Pasos con estado completo.** `steps[t]` describe todo el sistema después del paso `t`:
  procesos, hilos, fds, pipes, señales en vuelo y objetos de sincronización. Retroceder es leer
  `steps[t - 1]`; nunca se reconstruye aplicando diferencias.
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

## Cambios

Cualquier cambio al esquema debe:

1. Regenerar los tipos: `make types`.
2. Actualizar `crates/trace-model` (la prueba `roundtrip` falla si no coinciden).
3. Regenerar las trazas sintéticas si cambia su forma: `make synth`.
