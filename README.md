# Visualizador de procesos en C

Aplicación local que ejecuta un programa en C y lo reproduce como una animación paso a paso de
procesos, hilos, pipes, señales y memoria, al estilo de Python Tutor. Está pensada para el curso de
Programación de Sistemas: `fork`, `exec`, `wait`, `pipe`, `dup2`, señales, `pthreads` y `mutex`.

Todo corre en tu computador. No se sube código a ningún servidor.

> **Estado: fase 6.** Compila y traza programas con varios procesos (`fork`, `exec` como caja
> negra, `wait`, zombies y huérfanos), pipes con `dup2`, señales (handlers, máscaras, SIGCHLD, alarm
> y Ctrl+C) e hilos (mutex, variables de condición, semáforos, deadlock y planificación round-robin,
> aleatoria o manual), con memoria completa y entrada estándar. Incluye el modo introducción con
> cuatro capítulos animados. Falta arm64 (fase 7); ver `docs/fases/`.

## Cómo ejecutarlo

### Con Docker (cualquier sistema)

```sh
docker compose up --build
```

Luego abre <http://localhost:8000>.

### Nativo (Linux o Windows con WSL2)

Necesitas gcc, [Rust](https://rustup.rs) y [Node.js](https://nodejs.org) 20 o superior.

```sh
./pss
```

El comando compila lo que falte, inicia el servidor en <http://localhost:8000> y abre el navegador.
Para otro puerto: `PSS_PORT=9000 ./pss`.

## Uso

Elige un programa de la galería o escribe el tuyo y presiona **Ejecutar** (Ctrl + Enter). Los
errores de gcc aparecen en su línea. La entrada estándar se escribe antes de ejecutar; si el
programa pide más de lo que hay, la animación se detiene y te deja escribir más (o enviar EOF), y
la ejecución continúa desde ese mismo paso. Sin marcar "Terminar con EOF", al acabarse la entrada
el programa espera como lo haría en una terminal.

La primera vez se abre una introducción de cuatro capítulos (forks, hilos, pipes y señales); después
está en el botón **Introducción** de la barra superior.

## Controles

| Acción | Atajo |
| --- | --- |
| Paso adelante / atrás | → / ← |
| Siguiente / anterior evento (fork, pipe, señal, bloqueo, exit) | Shift + → / Shift + ← |
| Siguiente paso del hilo seleccionado | Alt + → (clic en el nombre del hilo para seleccionarlo) |
| Primer / último paso | Inicio / Fin |
| Reproducir / pausar | Espacio |
| Avanzar hasta una línea | Clic en el margen del código |
| Ctrl+C desde el paso actual (con servidor) | Botón Ctrl+C |
| Elegir qué hilo avanza (modo manual, con servidor) | Planificación → Manual y el botón ▶ junto a cada hilo |

Pasa el mouse (o navega con Tab) sobre cualquier proceso, carril, tubo, cable o señal para ver el
detalle técnico. Al pasar sobre un nodo de un carril se muestra la memoria de ese instante. La URL
guarda la traza y el paso actual, así que se puede compartir.

## Estructura

```
schema/          contrato de la traza (trace.schema.json) y su explicación
crates/
  trace-model/   tipos Rust del contrato, validados contra el esquema
  tracer/        pss-tracer: compila con gcc, ejecuta bajo ptrace y lee DWARF (gimli)
  server/        servidor local (axum): sirve la interfaz y ejecuta pss-tracer
web/             interfaz: Vite + React + TypeScript
  src/styles/tokens.css   paleta única (sección 12 del spec)
  src/trace/              tipos generados del esquema, consultas y textos por paso
  src/player/             reproductor SVG: procesos, carriles, tubos, señales, memoria
  src/editor/             panel de código (CodeMirror 6)
  src/intro/              modo introducción: motor determinista y los cuatro capítulos
tools/synth/     guiones que generan las trazas sintéticas
traces/          trazas sintéticas (fase 0) y de referencia de cada ejemplo (CI las compara)
config/          límites de ejecución (sección 13)
examples/        programas de la sección 14 (con su .stdin si leen entrada)
```

## Desarrollo

```sh
make test     # Rust (fmt, clippy, pruebas) + interfaz (tipos, typecheck, vitest)
make e2e      # pruebas de extremo a extremo con Playwright
make synth    # regenera las trazas sintéticas
UPDATE_REFERENCE=1 cargo test -p pss-tracer --test examples   # regenera las trazas de referencia
./target/release/pss-tracer --source prog.c --stdin entrada.txt --out traza.json   # sin interfaz
make types    # regenera los tipos TS desde el esquema
cd web && npm run dev   # interfaz con recarga en caliente en http://localhost:5173
```

El esquema `schema/trace.schema.json` es la fuente de verdad: los tipos de TypeScript se generan
desde él y los tipos de Rust se verifican contra él en las pruebas.
