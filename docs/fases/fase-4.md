# Fase 4: señales

## Qué funciona

- **Modelo de señales** (`crates/tracer/src/signals.rs`, con pruebas unitarias): acciones
  instaladas con `sigaction`/`signal`, máscara de `sigprocmask` y señales pendientes por proceso.
  Se heredan en `fork` (sin las pendientes) y `exec` devuelve los handlers a su acción por defecto.
- **Envíos**: `kill`, `raise`/`tgkill`, el kernel (SIGCHLD cuando el padre la atiende, SIGPIPE),
  `alarm` y la terminal (Ctrl+C) quedan como eventos `signalSend` con su origen. Mientras el destino
  no avanza, la señal se ve en vuelo, pendiente o bloqueada por la máscara.
- **Entregas**: con handler, un singlestep con la señal deja al proceso en la primera instrucción
  del handler y se avanza por sus líneas. La pila muestra el frame del handler (marcado con la
  señal) encima de lo que interrumpió, también si la señal llegó dentro de una llamada a libc
  (`pause`, `sleep`, `wait`). Al volver, `rt_sigreturn` restaura el contexto y queda el evento
  `signalReturn`. Ignoradas, por defecto y fatales (con core o sin él) también se registran.
- **Esperas interrumpibles**: una señal que se va a entregar despierta a quien espera en `pause`,
  `sigsuspend` (con su máscara), `sleep`, `wait` o `read`; el kernel devuelve EINTR o reinicia la
  syscall con `SA_RESTART`.
- **SIGKILL** mata al destino de inmediato, aunque esté bloqueado.
- **alarm** usa el reloj virtual: el tracer envía SIGALRM cuando el reloj llega a la hora, y el
  temporizador aparece en `timers`.
- **Ctrl+C**: el botón de los controles vuelve a ejecutar el programa con SIGINT al grupo en primer
  plano después del paso actual (`--inject t:SIGINT` en el tracer, `injections` en `/api/run` y
  `ctrlc=` en la URL). Hasta ese paso la traza nueva es idéntica a la anterior.
- **Pruebas**: 48 de Rust (4 del modelo de señales, los 3 ejemplos, Ctrl+C inyectado, máscara,
  alarm y SIGKILL), 126 unitarias de la interfaz y 31 de extremo a extremo (entre ellas Ctrl+C
  contra el servidor real).

## Ejemplos de la sección 14

| Ejemplo | Criterio | Estado |
| --- | --- | --- |
| `09_sigusr1.c` | Bloque emisor, pulso, desvío del handler y frame de handler | Cumple: `kill` de 1000 a 1001, el hijo sale de `pause` y corre `manejador` con su frame sobre `main` |
| `10_sigchld.c` | El bloque Kernel emite al padre cuando el hijo termina | Cumple: SIGCHLD del kernel interrumpe el `sleep` y el handler recoge al hijo con `waitpid` |
| `11_sigint.c` | El botón Ctrl+C reejecuta y el handler se ve en la nueva traza | Cumple: sin Ctrl+C cuenta hasta 10; con Ctrl+C en t=12 imprime "me interrumpiste en la vuelta 2" |

## Desviaciones y decisiones

- **alarm con reloj virtual**: la syscall se salta y el tracer envía SIGALRM cuando corresponde, así
  la traza no depende del tiempo real. `setitimer` todavía no se modela.
- **Detener procesos** (SIGSTOP, SIGTSTP) no se modela: esas señales no se entregan y el estado
  `stopped` queda para más adelante.
- **SIGCHLD del kernel solo se dibuja** si el padre tiene un handler para ella; si no, se entrega en
  silencio (su acción por defecto es ignorarla).
- **Los handlers de una caja negra** corren sin seguirse línea por línea (no tienen símbolos).

## Pendiente

- `sigqueue` y señales de tiempo real, `setitimer`, `signalfd`.
- Procesos detenidos y `SIGCONT` visibles.
