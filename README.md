# Visualizador de procesos en C

Aplicación local que ejecuta un programa en C y lo reproduce como una animación paso a paso de
procesos, hilos, pipes, señales y memoria, al estilo de Python Tutor. Está pensada para el curso de
Programación de Sistemas: `fork`, `exec`, `wait`, `pipe`, `dup2`, señales, `pthreads` y `mutex`.

Todo corre en tu computador. No se sube código a ningún servidor.

> **Estado: fase 0.** El reproductor funciona con trazas sintéticas escritas a mano. Todavía no
> compila ni ejecuta programas: eso llega en la fase 1 (ver `docs/propuesta-fase0.md`).

## Cómo ejecutarlo

### Con Docker (cualquier sistema)

```sh
docker compose up --build
```

Luego abre <http://localhost:8000>.

### Nativo (Linux o Windows con WSL2)

Necesitas [Rust](https://rustup.rs) y [Node.js](https://nodejs.org) 20 o superior.

```sh
./pss
```

El comando compila lo que falte, inicia el servidor en <http://localhost:8000> y abre el navegador.
Para otro puerto: `PSS_PORT=9000 ./pss`.

## Controles

| Acción | Atajo |
| --- | --- |
| Paso adelante / atrás | → / ← |
| Siguiente / anterior evento (fork, pipe, señal, bloqueo, exit) | Shift + → / Shift + ← |
| Siguiente paso del hilo seleccionado | Alt + → (clic en el nombre del hilo para seleccionarlo) |
| Primer / último paso | Inicio / Fin |
| Reproducir / pausar | Espacio |
| Avanzar hasta una línea | Clic en el margen del código |

Pasa el mouse (o navega con Tab) sobre cualquier proceso, carril, tubo, cable o señal para ver el
detalle técnico. Al pasar sobre un nodo de un carril se muestra la memoria de ese instante. La URL
guarda la traza y el paso actual, así que se puede compartir.

## Estructura

```
schema/          contrato de la traza (trace.schema.json) y su explicación
crates/
  trace-model/   tipos Rust del contrato, validados contra el esquema
  server/        servidor local (axum): sirve la interfaz; desde la fase 1 compila y traza
web/             interfaz: Vite + React + TypeScript
  src/styles/tokens.css   paleta única (sección 12 del spec)
  src/trace/              tipos generados del esquema, consultas y textos por paso
  src/player/             reproductor SVG: procesos, carriles, tubos, señales, memoria
  src/editor/             panel de código (CodeMirror 6)
tools/synth/     guiones que generan las trazas sintéticas
traces/          trazas sintéticas (fase 0) y de referencia (desde la fase 1)
config/          límites de ejecución (sección 13)
examples/        programas de la sección 14 (desde la fase 1)
```

## Desarrollo

```sh
make test     # Rust (fmt, clippy, pruebas) + interfaz (tipos, typecheck, vitest)
make e2e      # pruebas de extremo a extremo con Playwright
make synth    # regenera las trazas sintéticas
make types    # regenera los tipos TS desde el esquema
cd web && npm run dev   # interfaz con recarga en caliente en http://localhost:5173
```

El esquema `schema/trace.schema.json` es la fuente de verdad: los tipos de TypeScript se generan
desde él y los tipos de Rust se verifican contra él en las pruebas.
