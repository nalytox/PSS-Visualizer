# Un pipe del hijo al padre

> **Objetivo:** comunicar dos procesos con un pipe, entender por qué cada uno cierra el extremo que
> no usa, y ver cómo `read` se bloquea, recibe datos y finalmente detecta el fin de archivo.

Después de un `fork`, padre e hijo no comparten memoria: una variable que cambia el hijo no la ve
el padre. Para pasarse datos necesitan un canal del kernel. El más simple es el **pipe**: un buffer
dentro del kernel con dos extremos, uno para escribir y otro para leer. Los bytes salen en el mismo
orden en que entraron.

`pipe(fd)` crea el pipe y devuelve dos descriptores: `fd[0]` para **leer** y `fd[1]` para
**escribir** (una forma de recordarlo: 0 es la entrada estándar y 1 la salida). El truco es crearlo
**antes** del `fork`: el hijo hereda los mismos descriptores, apuntando al mismo pipe.

Quedan entonces cuatro descriptores hacia el pipe: lectura y escritura en el padre y en el hijo. El
pipe solo informa fin de archivo al lector cuando **todos** los descriptores de escritura están
cerrados. Por eso cada proceso cierra los extremos que no usa: aquí el hijo escribe, así que cierra
la lectura; el padre lee, así que cierra la escritura.

En el lienzo, el pipe aparece entre los procesos con flechas desde cada descriptor que lo tiene
abierto, y los bytes viajan por él.

## Línea 8 · `pipe(fd);`

El kernel crea el pipe y le da al proceso los descriptores libres más bajos: 3 para leer y 4 para
escribir (0, 1 y 2 ya están ocupados). El arreglo `fd` queda con `{3, 4}`.

## Línea 9 · `if (fork() == 0) {`

Se crea el hijo, que hereda los descriptores 3 y 4: ahora el pipe tiene dos lectores y dos
escritores posibles. Fíjate en las flechas nuevas en el lienzo. En el hijo, `fork` devolvió 0 y
entra al bloque; el padre salta a la línea 16. Si este paso es del hijo, es su primer paso.

## Línea 10 · `close(fd[0]);`

El hijo no va a leer: cierra su extremo de lectura. Desaparece la flecha del hijo hacia la lectura
del pipe.

## Línea 11 · `char *msg = "hola papá";`

`msg` apunta a un string constante que está en la memoria de solo lectura del programa. La «á»
ocupa 2 bytes en UTF-8, así que el texto tiene 10 bytes.

## Línea 12 · `write(fd[1], msg, strlen(msg));`

El hijo escribe los 10 bytes en el pipe. `write` copia los bytes al buffer del kernel y retorna
enseguida: **no espera** a que alguien los lea. No se escribe el `'\0'` final, porque `strlen` no lo
cuenta: por un pipe viajan bytes, no strings.

Si el padre estaba bloqueado en `read`, esta escritura lo despierta.

## Línea 13 · `close(fd[1]);`

El hijo cierra su extremo de escritura. Si el padre ya cerró el suyo, el pipe se queda **sin
escritores**: cuando el padre vacíe lo que queda, su `read` devolverá 0.

## Línea 14 · `return 0;`

El hijo termina. Al terminar, el kernel cierra cualquier descriptor que hubiera quedado abierto,
así que el `close` anterior era buena práctica, pero salir también lo habría cerrado.

## Línea 16 · `close(fd[1]);`

El padre cierra su extremo de **escritura**. Esta línea es la clave del ejemplo: si el padre la
olvidara, él mismo seguiría contando como escritor y su `read` nunca recibiría fin de archivo
(es lo que muestra el ejemplo 07).

## Línea 19 · `while ((n = read(fd[0], buf, sizeof buf - 1)) > 0) {`

El padre lee hasta 31 bytes (deja uno libre para el `'\0'`). Según el momento, este paso muestra
cosas distintas:

- si el pipe está vacío y el hijo aún puede escribir, `read` **bloquea** al padre;
- si hay datos, los devuelve (aquí, los 10 bytes de una vez) y el bucle entra;
- si el pipe está vacío y ya no quedan escritores, devuelve **0**: fin de archivo, y el bucle termina.

## Línea 20 · `buf[n] = '\0';`

`read` entrega bytes sin terminar en `'\0'`. Para usarlos como string con `printf("%s")`, el padre
agrega el terminador a mano en la posición `n`.

## Línea 21 · `printf("el padre leyó: %s\n", buf);`

Imprime `el padre leyó: hola papá`. Luego el bucle vuelve a la línea 19 a leer más.

## Línea 23 · `close(fd[0]);`

El padre ya terminó de leer y cierra la lectura. El pipe ya no tiene descriptores abiertos y el
kernel lo libera.

## Línea 24 · `wait(NULL);`

El padre recoge al hijo, que probablemente ya era zombie. Así no queda ningún proceso sin recoger.

## Línea 25 · `return 0;`

El padre termina con código 0.

## Línea 26 · `}`

Fin de `main` (en el padre o en el hijo, según quién avanzó en este paso).

## Al terminar

- `pipe(fd)`: `fd[0]` lee, `fd[1]` escribe. Se crea antes del `fork` para que ambos lo hereden.
- Cada proceso cierra los extremos que no usa; el lector ve fin de archivo solo cuando se cerraron
  todos los extremos de escritura.
- `read` bloquea si no hay datos, devuelve lo que haya (puede ser menos de lo pedido) y 0 en EOF.
- Por un pipe viajan bytes: el `'\0'` hay que agregarlo al recibir.

## Para practicar

- Haz que el hijo escriba dos mensajes con dos `write`. ¿Los recibe el padre en uno o en dos `read`?
  Prueba distintas planificaciones.
- Cambia `sizeof buf - 1` por 4. ¿Cuántas vueltas da el bucle?
- Agrega un segundo pipe para que el padre responda «hola hijo». ¿Qué tiene que cerrar cada uno?
