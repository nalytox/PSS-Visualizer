# Fase 3: pipes

## Qué funciona

- **Modelo de fds y pipes** (`crates/tracer/src/fds.rs`, con pruebas unitarias): cada proceso tiene
  su tabla de fds, que se hereda en `fork`, pierde los `FD_CLOEXEC` en `exec` y se vacía al salir.
  Se siguen `pipe`, `pipe2`, `dup`, `dup2`, `dup3`, `fcntl` (`F_DUPFD`, `F_SETFD`), `close` y los
  `open`/`openat` del programa del usuario.
- **read y write según a qué apunta el fd**: terminal, stdin o pipe (también tras un `dup2` sobre 0
  o 1), incluidas `readv` y `writev`. El buffer de cada pipe se refleja byte a byte.
- **Bloqueos de pipe**: `read` de un pipe vacío con escritores y `write` que no cabe (64 KiB)
  bloquean; despiertan cuando hay datos, espacio o ya no quedan escritores (EOF).
- **SIGPIPE**: escribir sin lectores deja el evento `write` con `epipe`, el envío del kernel y la
  entrega que termina al proceso.
- **Tapón gris**: si un proceso espera leer un pipe cuyo extremo de escritura él mismo mantiene
  abierto, el pipe lleva el aviso `unclosedEnd` y la narración del deadlock lo explica.
- **Cajas negras en tuberías**: `ls | grep c | wc -l` corre con tres programas sin símbolos; cada
  lectura o escritura suya es un paso.
- **Interfaz**: tubos, cables, cápsulas y puertos de la fase 0 funcionan con trazas reales.
- **Pruebas**: 36 de Rust (4 del modelo de fds, 3 ejemplos nuevos, SIGPIPE y pipe lleno), 120
  unitarias de la interfaz y 27 de extremo a extremo.

## Ejemplos de la sección 14

| Ejemplo | Criterio | Estado |
| --- | --- | --- |
| `06_pipe_padre_hijo.c` | El tubo baja al padre y las cápsulas llegan al buffer | Cumple: 10 bytes en `p0`, el padre lee y luego recibe EOF |
| `07_pipe_sin_cerrar.c` | Tapón gris y lector bloqueado para siempre | Cumple: termina en deadlock, fd 4 del padre marcado y la narración dice por qué |
| `08_pipeline.c` | Tres cuadrados y dos tubos en escalera, puertos 0 y 1 reetiquetados | Cumple: imprime `1`; los puertos muestran `stdin → p0` y `stdout → p1` |

## Desviaciones y decisiones

- **Las disposiciones de señales se reinician antes del `execve`**: Rust ignora SIGPIPE y esa
  disposición se heredaba al programa, que nunca moría por SIGPIPE.
- **Un write que no cabe** espera a que quepa entero (hasta 64 KiB); el kernel escribiría una parte
  y bloquearía. El resultado final es el mismo.
- **Archivos abiertos por cajas negras** (bibliotecas, el directorio de `ls`) no se dibujan.
- **Texto de `char[]`**: termina en el `\0` o donde empiezan los bytes nunca escritos, así un
  buffer a medio llenar muestra solo lo leído.

## Pendiente

- `select`/`poll`, `splice` y `tee` sobre pipes.
- FIFOs con nombre (`mkfifo`).
