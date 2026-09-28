# Fase 5: hilos

## Qué funciona

- **Hilos bajo el planificador** (`PTRACE_O_TRACECLONE`): cada hilo es una tarea que avanza por
  líneas. Los hilos de un proceso comparten su PID y reciben TIDs virtuales (1001, 1002…);
  `getpid` y `gettid` devuelven esos números. El estado del proceso (heap, fds, señales) vive en
  cada hilo: el que avanza toma la copia del hilo principal y la reparte al terminar su paso.
- **pthread_create**: el hilo nuevo aparece con su función y su argumento (evento
  `threadCreate`), corre desde start_thread de glibc hasta su función y se sigue línea por línea.
  La instantánea del proceso junta las pilas de todos sus hilos.
- **Fin y join**: el valor de retorno (o el de `pthread_exit`) queda en el evento `exit` del hilo y
  en `join`. `pthread_exit` en `main` deja terminar a los demás; `exit` o una señal fatal terminan
  todos los hilos.
- **Esperas por futex**: mutex, variables de condición, semáforos y `pthread_join` esperan en un
  futex. El tracer retiene al hilo en la entrada de `futex` con su motivo (mutex y dueño, condición,
  semáforo, hilo esperado) y lo despierta cuando el valor del futex cambia; el kernel devuelve
  EAGAIN y glibc vuelve a intentar.
- **Objetos de sincronización** (`sync`): mutex con dueño y cola de espera, condiciones y semáforos
  con su valor y nombre de variable. Eventos `mutex` (tomado, bloqueado, ocupado, liberado),
  `cond` (wait, signal con a quién despertó, broadcast) y `sem`. En la memoria, `pthread_mutex_t`,
  `pthread_cond_t`, `sem_t` y `pthread_t` se muestran por su estado ("mutex tomado por el hilo
  1001", "hilo 1002") y no por sus bytes internos.
- **Planificación**: round-robin (por defecto), aleatoria con semilla visible y manual. En modo
  manual, cada hilo listo tiene un botón para que dé el siguiente paso: se vuelve a ejecutar con los
  pasos anteriores iguales y ese hilo en el siguiente (`--schedule` en el tracer, `plan=` y `orden=`
  en la URL).
- **Deadlock**: si todos los hilos vivos esperan, la traza termina con el evento `deadlock`.
- **Pruebas**: 57 de Rust (4 ejemplos nuevos, planificación manual que corrige la carrera, semilla
  reproducible, argumento y retorno de un hilo, `pthread_exit` en main), 134 unitarias de la
  interfaz y 36 de extremo a extremo (entre ellas el modo manual contra el servidor real).

## Ejemplos de la sección 14

| Ejemplo | Criterio | Estado |
| --- | --- | --- |
| `12_hilos_carrera.c` | En modo manual se puede forzar un resultado incorrecto | Cumple: con round-robin se pierden sumas (3 de 6); en modo manual se elige el orden (con A completo antes que B da 6) |
| `13_hilos_mutex.c` | Tramos subrayados con el color del mutex, espera visible | Cumple: da 6; el hilo que espera queda en ámbar con el candado y su dueño |
| `14_productor_consumidor.c` | Flechas de signal y wait entre carriles | Cumple: los 4 datos pasan en orden; eventos `cond` signal y esperas por el mutex |
| `15_deadlock.c` | Evento deadlock y ambos carriles en ámbar | Cumple: cada hilo tiene un mutex y espera el otro |

## Desviaciones y decisiones

- **El paso es una línea**: `contador++` en una sola línea es atómico para el planificador. Por eso
  `12` escribe leer, sumar y guardar en tres líneas: así la carrera se ve y se puede forzar.
- **`15` duerme 1 ms tras el primer lock**: con round-robin, sin esa pausa el primer hilo alcanzaría
  a tomar los dos mutex antes de que el otro empiece.
- **Futex por cambio de valor** en vez de dejar al hilo dentro del kernel: el resultado es el mismo y
  la traza no depende de cuándo el kernel despierta a cada uno.
- **Breakpoints compartidos**: un hilo detenido dentro de una llamada quita su breakpoint de la
  memoria común (lo vuelve a poner al reanudar) para que otro hilo no lo pise.
- **Máscaras de señales por proceso**, no por hilo.

## Pendiente

- `pthread_cancel`, `pthread_barrier`, `pthread_rwlock` y los mutex recursivos o con PI.
- `exec` desde un proceso con varios hilos.
