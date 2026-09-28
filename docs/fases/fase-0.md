# Fase 0: base

## Qué funciona

- **Contrato**: `schema/trace.schema.json` (JSON Schema 2020-12), explicado en `schema/README.md`.
  Los tipos de TypeScript se generan desde él (`make types`, verificado en CI) y los de Rust
  (`crates/trace-model`) se prueban contra él.
- **Trazas sintéticas** (`traces/synthetic/`), generadas por `tools/synth/` y reproducibles byte a
  byte:
  - `fork_pipe`: pipe, fork con cuatro cables, write, read bloqueante, EOF, close, wait, zombie y
    recogido.
  - `threads_mutex`: dos hilos, mutex con espera, join.
  - `signal_handler`: sigaction, sleep con reloj virtual, kill, pause, desvío al handler, SIGCHLD
    ignorada.
  - `structs_heap`: structs anidados, arreglo de structs, lista enlazada, free, puntero colgante y
    fugas.
  - `fork_tree`: 16 procesos con huérfanos adoptados por init; sirve de prueba de carga.
- **Validación**: cada traza pasa el esquema (Ajv y el crate `jsonschema`) y un conjunto de
  invariantes semánticas (`web/src/trace/invariants.ts`): fds y extremos de pipe coherentes, un
  solo hilo ejecutando, la línea ejecutada coincide con donde estaba el hilo, etc.
- **Reproductor** (`web/`):
  - Cuadrados de proceso en árbol genealógico con posiciones estables, estados (ejecutando, listo,
    bloqueado, zombie, recogido como silueta), puertos de fd, puerto de señales y consola por
    proceso.
  - Carriles de hilos sobre el reloj global `t`: tramo sólido si ejecutó, punteado si no le tocó,
    ámbar rayado si estaba bloqueado, desvío elevado durante un handler, subrayado del mutex tomado,
    ramas de `pthread_create` y flechas de join.
  - Tubos diagonales con cápsulas, nivel `5 B / 64 KiB`, cables a cada extremo abierto (los cuatro
    tras un fork) y desenchufe animado al cerrar.
  - Bloques de señal (emisor, kernel) con pulso por el cable.
  - Memoria al estilo Python Tutor: globales, una pila por hilo, frame de handler, heap en cadena
    siguiendo las flechas, punteros a la celda exacta, NULL como tierra, colgantes y liberados en
    rojo, valores cambiados en durazno, liberados rayados y fugas marcadas al final.
  - Animaciones por evento (cápsulas, pulso, división al hacer fork, destello de memoria copiada),
    reproducidas en reversa al retroceder un paso.
  - Panel de código con cursor de color por hilo, línea ejecutada y próxima, y clic en el margen
    para avanzar hasta esa línea.
  - Narración de cada paso en español, terminal global, línea de tiempo con marcadores y todos los
    atajos de la sección 10.
  - Zoom y desplazamiento; la vista sigue a lo que ocurre en cada paso.
  - Tema claro y oscuro, tooltips accesibles con teclado, `prefers-reduced-motion` y estado en la
    URL.
- **Ejecución local**: `./pss` (Linux o WSL2) y `docker compose up`. El servidor `pss-server`
  (axum) escucha solo en localhost.
- **Pruebas**: 3 de Rust, 92 unitarias de la interfaz (incluido el contraste AA de toda la paleta)
  y 10 de extremo a extremo con Playwright. CI en `.github/workflows/ci.yml`.

## Mediciones

- Un paso con 16 procesos en pantalla: mediana de 40 ms y máximo de 58 ms en la compilación de
  producción (criterio: < 100 ms). La prueba de extremo a extremo lo verifica.
- Las animaciones de eventos corren a 60 fps (un cuadro cada ~16 ms).

## Desviaciones respecto del spec

- **Texto secundario**: `#796D95` en vez de `#7A6E96`. El tono del spec da 4,47:1 sobre el fondo
  `#FBF9FF`, bajo el mínimo AA (4,5:1); este es un paso más oscuro. Lo detectó la prueba de
  contraste.
- **Color por hilo**: el hilo principal usa el color del proceso y cada hilo adicional un tono de la
  misma rotación, para que "una pila por hilo, del color de su carril" se pueda distinguir.
- **Árbol**: disposición propia para anchos variables en vez de d3-hierarchy, que asume nodos del
  mismo tamaño. Los cuadrados crecen con su memoria.
- **Tubo de un pipe entre padre e hijo**: va al costado del árbol y no entre ambos cuadrados, para
  que los cables no crucen la línea que los une.
- **Zoom automático**: no baja de 0,8 para que la memoria se lea; si todo no cabe, la vista sigue
  al proceso activo.
- **Ligaduras desactivadas** en el código: JetBrains Mono dibujaría `->` como una flecha.

## Pendiente (fases siguientes)

- Editor editable, compilación y tracer real (fase 1). El panel de código es de solo lectura.
- Minimapa con más de 8 procesos, nodo virtual `init (1)` con líneas punteadas, caja negra tras
  `exec` (fase 2).
- Casos límite de pipes: SIGPIPE, tubo lleno, tapón de fd sin cerrar calculado por el tracer
  (fase 3). El esquema y el reproductor ya los contemplan.
- Máscaras de señales, Ctrl+C, alarmas (fase 4). Variables de condición, semáforos, planificación
  manual con ramas y deadlock (fase 5).
- Exportar a GIF o WebM e introducción animada (fase 6).
- **Docker**: validé por separado las etapas de la interfaz, del servidor y la imagen final (solo
  lectura, usuario sin privilegios, responde en el puerto 8000). El `apt-get install gcc` de la
  imagen final no se pudo probar en este entorno, cuya red bloquea `deb.debian.org`.
