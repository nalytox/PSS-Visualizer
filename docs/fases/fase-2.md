# Fase 2: procesos

## Qué funciona

- **Varios procesos bajo un planificador determinista** (`crates/tracer/src/tracer.rs`):
  - En cada paso avanza un solo proceso (round-robin) y el resto queda detenido. `choices` registra
    quiénes podían avanzar.
  - `fork`, `vfork` (también el de `system()` y `posix_spawn`) y `exec` se siguen con
    `PTRACE_O_TRACEFORK | TRACEVFORK | TRACEEXEC`.
  - El hijo nace dentro de la misma llamada a `fork` que el padre, con una copia de su modelo:
    memoria, heap, pila de llamadas y línea.
  - `exit`, `return` desde `main` y muerte por señal (`SIGSEGV`, `abort`…) dejan al proceso zombie,
    con su código o su señal.
  - `wait`, `waitpid` y `WNOHANG` recogen al hijo: el evento `wait` lleva el estado que el padre
    recibió.
- **PIDs virtuales y estables**:
  - El proceso inicial es el 1000 y sus descendientes son 1001, 1002… en orden de creación.
  - El programa ve esos mismos números en `getpid`, `getppid`, `getpgrp`, el valor de `fork` y el
    de `wait`.
  - El tracer traduce los PIDs en `kill`, `tgkill`, `wait4`, `setpgid` y `getpgid`.
  - Un PID que no es de la traza se reemplaza por uno inexistente, y `kill(-1, …)` queda limitado al
    grupo del programa: nunca se alcanzan procesos ajenos.
- **Huérfanos**:
  - Si el padre termina antes, sus hijos pasan a `init (1)` (evento `reparent`) y `getppid`
    devuelve 1.
  - init recoge de inmediato a cada huérfano que termina.
  - El tracer es *subreaper*, así que los recoge de verdad y no quedan zombies en el sistema.
- **Bloqueos y reloj virtual**:
  - `wait` sin hijos terminados, `sleep`, `usleep` y `nanosleep`, `pause` y la lectura de stdin
    agotado bloquean al proceso; el resto sigue avanzando.
  - Cada paso cuesta 1 ms de reloj virtual. Cuando todos esperan, el reloj salta al próximo
    despertar (sin dormir de verdad).
  - Si nadie puede avanzar, la traza termina con `deadlock`, o con `awaitingInput` cuando alguien
    espera stdin.
- **Caja negra**:
  - Tras un `exec` el proceso conserva PID, color y fds, pero su interior pasa a ser una caja negra
    con el programa y su argv (por ejemplo `/usr/bin/ls`).
  - De ella solo se siguen las syscalls y la salida, un paso por cada escritura a la terminal.
- **Límite de procesos**: el `fork` que superaría 32 procesos vivos o zombies no se ejecuta. La
  traza termina truncada con motivo `processes` y un mensaje claro.
- **Interfaz**:
  - Etiqueta `fork() = 1001` en el padre y `fork() = 0` en el hijo, sobre sus carriles.
  - Línea punteada ámbar con reloj de arena entre el padre en `wait` y los hijos que puede
    recoger. Al recogerlo, el código de salida viaja por esa línea hacia el padre.
  - Nodo `init (1)` con líneas punteadas a los huérfanos vivos, que corren por un riel sobre el
    árbol.
  - Caja negra que aparece con un fundido tras el `exec`, y carril que muestra "ejecutando ls".
  - Minimapa con más de ocho procesos: clic o arrastre para mover la vista.
  - Narración de `exec`, huérfanos, `wait` con señal y `WNOHANG`.
- **Pruebas**:
  - 27 de Rust: 4 de los ejemplos nuevos contra `traces/reference/` y 7 de casos de procesos.
    Estos cubren huérfanos con `sleep`, un hijo con segfault y otro con `abort`, `system()`,
    espera activa con `WNOHANG`, deadlock, stdin compartido y PIDs virtuales.
  - 114 unitarias de la interfaz: línea de espera, token de recogida, caja negra, init y que los 8
    procesos de `04` no se superpongan.
  - 22 de extremo a extremo, entre ellas `exec` y el fork bomb contra el servidor real.

## Ejemplos de la sección 14

| Ejemplo | Criterio | Estado |
| --- | --- | --- |
| `03_fork_simple.c` | Dos cuadrados con retornos 0 y PID; el zombie se recoge | Cumple: `fork() = 1001` y `fork() = 0`; 1001 pasa a zombie con código 3 y el `wait` del padre lo recoge |
| `04_fork_bucle.c` | 8 procesos en árbol, estable y legible | Cumple: 8 cuadrados sin superponerse; los huérfanos cuelgan de `init (1)` |
| `05_exec.c` | El hijo conserva su PID y se vuelve caja negra con su salida | Cumple: 1001 ejecuta `/usr/bin/ls`, imprime `prog  prog.c` y el padre lo recoge |
| `16_fork_bomb.c` | Traza truncada por límite de procesos, sin afectar al servidor | Cumple: se corta con 32 procesos en 0,12 s; todos mueren al terminar la traza |

## Mediciones

| Ejemplo | Pasos | Tiempo | JSON | gzip |
| --- | --- | --- | --- | --- |
| `03_fork_simple` | 17 | 0,08 s | 21 KB | 2 KB |
| `04_fork_bucle` | 53 | 0,11 s | 147 KB | 4 KB |
| `05_exec` | 14 | 0,09 s | 16 KB | 2 KB |
| `16_fork_bomb` | 63 | 0,12 s | 449 KB | 8 KB |

Los cuatro ejemplos se verificaron también en Debian (imagen `rust:1-bookworm`), como usuario sin
privilegios y con `seccomp:unconfined`. Dieron las mismas salidas y la misma traza en dos
ejecuciones seguidas.

## Desviaciones y decisiones

- **PIDs virtuales reescribiendo syscalls, no con un namespace de PID** (la propuesta de la fase 0
  decía namespace, con un init propio como PID 1):
  - Crear un namespace exige privilegios o namespaces de usuario sin privilegios. Ubuntu 24.04 los
    restringe con AppArmor, y el perfil seccomp por defecto de Docker bloquea `unshare`.
  - Reescribir en las paradas de syscall funciona igual en todos lados y deja además números
    pequeños y estables (1000, 1001…).
  - El nodo `init (1)` es virtual; en el sistema real, el que adopta es el tracer (subreaper).
- **Las esperas se retienen en la entrada de la syscall**. El spec (3.3) dice dejar la tarea
  dentro del kernel; aquí el proceso queda detenido en la entrada de `wait4`, `nanosleep`,
  `pause` o `read(0)`, y se reanuda cuando su condición se cumple en el modelo.
  - El efecto visible es el mismo y no hay carreras con el kernel: la traza es determinista.
  - Los pipes de la fase 3 seguirán este mismo esquema.
- **Reloj virtual de 1 ms por paso**. Sin ese costo, una espera activa con `WNOHANG` nunca dejaría
  despertar a un hijo que duerme.
- **Syscalls con `PTRACE_SYSCALL`** mientras corre libc, igual que en la fase 1, leídas con
  `PTRACE_GET_SYSCALL_INFO`. Con estos tiempos seccomp-bpf todavía no hace falta.
- **Caja negra por escritura**: un programa ajeno avanza hasta cada escritura a la terminal o hasta
  terminar. Si hace su propio `fork`, sus hijos también son cajas negras.
- **Tres errores de la fase 1 corregidos** (cambian sus trazas de referencia):
  - Si una llamada a libc volvía justo al inicio de la línea siguiente, esa parada se saltaba (en
    `03`, `printf` y `wait` quedaban en un solo paso).
  - Un bucle escrito en una sola línea (`while (waitpid(…) == 0) n++;`) no terminaba su paso: el
    salto hacia atrás se medía contra la última parada y no contra la instrucción anterior.
  - Un bloque liberado mostraba los datos internos de malloc, que incluyen una clave aleatoria por
    proceso: la traza no era determinista dentro de un mismo proceso de pruebas. Ahora el bloque
    rayado muestra lo que tenía al momento del `free`.
- **`init (1)`**: sus líneas corren por un riel sobre el árbol en vez de ir en diagonal a cada
  huérfano. Así no cruzan el lienzo.

## Pendiente

- Hilos (`clone` con `CLONE_THREAD`) en la fase 5: hoy un programa con hilos corre sus hilos sin
  rastrear.
- `waitid`, `WUNTRACED` y procesos detenidos llegan con las señales de la fase 4.
- Un `exec` de un programa propio con símbolos se muestra igual como caja negra.
- Una syscall que bloquea y no está modelada (un `read` de pipe, por ahora) solo termina con el
  límite de tiempo. La fase 3 modela los pipes.
