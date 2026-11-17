# El pipe que nunca termina

> **Objetivo:** ver en acción el error más común con pipes: un lector que no cierra su propio extremo
> de escritura espera para siempre un fin de archivo que nunca llega.

Este programa es igual al ejemplo 06 salvo por una línea que **falta**: el padre no cierra `fd[1]`
(el comentario de la línea 14 marca dónde debería estar).

Recuerda la regla: `read` sobre un pipe vacío devuelve 0 (fin de archivo) solo si **no queda ningún
descriptor de escritura abierto**, en ningún proceso. El kernel no sabe si alguien piensa escribir:
solo cuenta descriptores. Mientras el padre tenga su `fd[1]` abierto, para el kernel existe un
escritor posible, aunque ese escritor sea el mismo proceso que está leyendo.

El resultado es un **interbloqueo** (*deadlock*) de un solo proceso: el padre espera datos que solo
él podría escribir, y no puede escribirlos porque está esperando. El visualizador lo detecta cuando
todas las tareas vivas están bloqueadas y nada puede despertarlas.

## Línea 7 · `pipe(fd);`

Se crea el pipe: `fd[0]` = 3 para leer y `fd[1]` = 4 para escribir.

## Línea 8 · `if (fork() == 0) {`

El hijo hereda los dos extremos. Ahora hay dos escritores posibles: el hijo y el padre.

## Línea 9 · `close(fd[0]);`

El hijo cierra la lectura, que no usa. Hasta aquí todo es correcto.

## Línea 10 · `write(fd[1], "dato", 4);`

El hijo escribe 4 bytes. Si el padre estaba bloqueado leyendo, despierta.

## Línea 11 · `close(fd[1]);`

El hijo cierra su escritura. Pero el pipe **todavía tiene un escritor**: el `fd[1]` del padre, que
nunca se cerró. Fíjate en el lienzo: sigue habiendo una flecha de escritura hacia el pipe, y sale
del padre.

## Línea 12 · `return 0;`

El hijo termina. Como nadie hace `wait`, queda zombie.

## Línea 17 · `while ((n = read(fd[0], buf, sizeof buf)) > 0) {`

La primera vez que el padre llega aquí, el pipe puede estar vacío: se bloquea hasta que el hijo
escribe, y entonces recibe los 4 bytes.

La segunda vez el pipe está vacío y el hijo ya cerró su extremo, pero el padre no cerró el suyo:
`read` **bloquea para siempre**. No hay nadie más que pueda escribir ni nada que despierte al padre.

## Línea 18 · `printf("leí %d bytes\n", n);`

Imprime `leí 4 bytes`. Después el bucle vuelve a llamar a `read`, y ahí se queda.

## Línea 22 · `}`

Este es el final del hijo. Después de este paso todas las tareas que quedan (solo el padre) están
bloqueadas: el visualizador declara el deadlock. La línea 20 (`printf("fin de archivo\n")`) nunca se
ejecuta.

## Al terminar

- El fin de archivo de un pipe depende de que se cierren **todos** los descriptores de escritura, en
  todos los procesos, incluido el propio lector.
- Un descriptor olvidado no da error: el programa simplemente se queda esperando para siempre.
- Regla práctica: inmediatamente después del `fork`, cada proceso cierra los extremos que no va a
  usar.

## Para practicar

- Agrega `close(fd[1]);` en la línea 14 y vuelve a ejecutar. ¿Qué cambia en el lienzo y en la salida?
- En vez de eso, agrega `close(fd[1]);` justo antes del `while`, pero dentro del bloque del hijo
  quita su `close(fd[1])`. ¿Termina el programa? ¿Por qué?
- ¿Qué pasaría si el **hijo** olvidara cerrar `fd[0]` y el padre cerrara la lectura antes de que el
  hijo escriba? (Pista: busca `SIGPIPE`.)
