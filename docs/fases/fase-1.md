# Fase 1: un proceso

## Qué funciona

- **Tracer real** (`crates/tracer`, binario `pss-tracer`):
  - Compila con `gcc -g -O0 -fno-omit-frame-pointer -pthread -no-pie -Wall -Wl,-z,now` y devuelve
    errores y advertencias con línea y columna.
  - Ejecuta bajo ptrace con un entorno fijo: ASLR desactivado, argv y variables de entorno
    constantes, límites de memoria, CPU y archivos, y solo los fds 0, 1 y 2 heredados.
  - Avanza por línea con singlestep dentro del código del usuario, usando la tabla de líneas DWARF
    (filas `is_stmt`, después del prólogo). Una vuelta de un bucle en la misma línea también es un
    paso.
  - Cada llamada a libc (`printf`, `malloc`, `scanf`…) es un paso atómico: breakpoint en la
    dirección de retorno y avance con paradas en syscalls. El nombre de la función sale de
    `/proc/pid/maps` y la tabla de símbolos del `.so`.
  - Memoria completa en cada paso, leída con gimli: globales y `static`, frames por unwind con frame
    pointer, parámetros y locales visibles según su bloque léxico, structs y unions anidados,
    arreglos (también de structs y multidimensionales), `char[]` como texto, enums, punteros y
    punteros a función.
  - **Sin inicializar**: al entrar a una función sus locales se rellenan con `0xBE`, igual que los
    bloques de `malloc`. Una variable cuyos bytes siguen así nunca fue escrita y se muestra como `?`.
  - **Heap**: `malloc`, `calloc`, `realloc`, `strdup` y `free`, con el tipo de cada bloque inferido
    de los punteros que lo apuntan (sirve para listas enlazadas). Detecta doble `free` y `free`
    inválido; los bloques liberados quedan rayados unos pasos y las fugas se reportan al salir.
  - Valores de retorno de las funciones del usuario, salida capturada en las syscalls `write` y
    `writev` (incluida la que libc vacía al salir) y stdout como terminal (buffer de línea).
  - Segfault reportado con su dirección; límites de pasos, tiempo real y salida con traza cortada y
    motivo.
- **Entrada estándar** precargada + "pedir más": si el programa lee con la entrada agotada y sin
  EOF, la traza termina con `awaitingInput`. La interfaz pide más texto o EOF, reejecuta y vuelve al
  mismo paso.
- **Servidor**: `POST /api/run` (código + stdin → traza con gzip), dos ejecuciones a la vez,
  cuerpo de hasta 512 KiB.
- **Interfaz**:
  - Editor editable con Ctrl + Enter y errores de gcc marcados en su línea.
  - Galería de programas y trazas grabadas; sin servidor, los ejemplos se abren con su traza de
    referencia.
  - Stdin editable, estado completo en la URL (código y stdin comprimidos).
  - Punteros a literales y a memoria de libc se muestran con su texto, no como colgantes.
  - Las flechas eligen la celda por tipo (`&r.esquina` apunta al campo, no a `r`).
- **Pruebas**:
  - 16 de Rust: diagnósticos, límites, heap, modelo de la traza y 9 de integración, con los ejemplos comparados contra
    `traces/reference/`, determinismo, error de compilación, bucle infinito, segfault y salida.
  - 99 unitarias de la interfaz (esquema e invariantes también sobre las trazas reales).
  - 14 de extremo a extremo contra el servidor real (ejecutar, error de compilación, pedir más
    entrada, enlace compartible).

## Ejemplos de la sección 14

| Ejemplo | Criterio | Estado |
| --- | --- | --- |
| `01_structs.c` | Campos anidados visibles y flechas al campo correcto | Cumple: `p` apunta a `r.esquina` en la segunda llamada |
| `02_lista_enlazada.c` | Heap en cadena y fuga final reportada | Cumple: 4 nodos en cadena; 3 fugas de tipo `struct nodo` |

## Mediciones

- `02_lista_enlazada` (45 pasos): 0,16 s.
- Programa con bucle hasta el límite de 5000 pasos: 4,35 s (límite de tiempo: 10 s). La traza pesa
  19 MB en JSON y 177 KB con gzip, que es como la envía el servidor.
- Casi todo el tiempo es de sistema, en las paradas de ptrace (~13 000 por segundo en esta VM).

## Desviaciones y decisiones

- **Esquema**: `PointerValue` suma `outside` y `text`, para punteros a memoria válida que no se
  dibuja (literales de texto, datos de libc). Así no se confunden con punteros colgantes.
- **Syscalls con PTRACE_SYSCALL** (solo mientras corre código de libc) en vez de seccomp-bpf. Con
  un solo proceso el costo es bajo; seccomp llega cuando haya varios procesos y hace falta filtrar.
- **PID fijo en la traza (1000)**: el programa ve su PID real si llama a `getpid()`. Los namespaces
  de PID, que hacen reproducible también ese valor, llegan en la fase 2 junto con `fork`.
- **Docker**: el perfil seccomp por defecto bloquea `personality(ADDR_NO_RANDOMIZE)` y las
  direcciones cambiaban en cada ejecución (verificado). `docker-compose.yml` usa
  `seccomp:unconfined`; el contenedor sigue sin privilegios, sin root y de solo lectura.
- **Trazas de referencia**: dependen de gcc y glibc. Se generaron en Ubuntu 24.04 (gcc 13.3,
  glibc 2.39), igual que el runner de CI. En Debian las direcciones del stack se corren 0x20 bytes.
- **Dos errores corregidos por las pruebas en paralelo**:
  - `/proc/pid/mem` se abría antes del `execve` del hijo y a veces quedaba apuntando a la memoria
    vieja.
  - Los programas heredaban descriptores de otras trazas.

## Pendiente

- Funciones del usuario llamadas desde libc (el comparador de `qsort`, handlers de `atexit`): hoy
  corren sin pasos propios. Se resuelve junto con los handlers de señales (fase 4) y el inicio de
  hilos (fase 5).
- Arreglos de largo variable (VLA) y variables en registros no se muestran.
- Optimización: breakpoints en cada fila de la tabla de líneas en vez de singlestep, si hace falta
  más velocidad.
