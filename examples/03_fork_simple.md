# fork: un proceso se vuelve dos

> **Objetivo:** ver qué copia `fork`, cómo se distingue al padre del hijo por el valor que devuelve,
> por qué sus memorias quedan separadas, y cómo el padre espera al hijo y lee su código de salida.

`fork()` es la única forma de crear un proceso en Unix. El proceso que la llama (el **padre**)
obtiene un **hijo** que es una copia casi exacta de sí mismo: la misma memoria con los mismos
valores, los mismos archivos abiertos y el mismo punto de ejecución. Los dos continúan desde la
instrucción siguiente al `fork`, como si ambos hubieran hecho la llamada.

¿Cómo sabe cada uno quién es? Por el **valor de retorno**:

- en el padre, `fork` devuelve el **PID del hijo** (un número positivo);
- en el hijo, devuelve **0**;
- si falla, devuelve -1 en el padre y no hay hijo.

Desde ese momento son procesos independientes: cada uno tiene **su propia copia** de las variables.
Lo que uno cambie, el otro no lo ve. El orden en que avanzan lo decide el planificador del sistema
operativo; aquí, con la planificación por turnos, avanzan alternados una línea cada uno.

Al final, el padre usa `wait` para esperar a que el hijo termine y leer su código de salida. Mientras
el hijo terminó pero nadie lo recogió, es un **zombie**.

## Línea 6 · `int x = 10;`

Por ahora hay un solo proceso, con `x` = 10 en su stack.

## Línea 7 · `pid_t pid = fork();`

Aquí aparece el segundo proceso. Si este paso es del **padre**, acaba de ejecutar `fork`: en el
lienzo nace el hijo, conectado a su padre, con una copia de toda su memoria (incluida `x` = 10) y de
sus descriptores. En el padre, `pid` recibe el PID del hijo.

Si este paso es del **hijo**, es su primer paso: termina la misma línea, pero en su copia `fork`
devolvió 0, así que su `pid` vale 0. Es la misma variable en el código, pero son dos variables
distintas, una en cada proceso.

## Línea 8 · `if (pid == 0) {`

La misma comparación da resultados distintos en cada proceso. En el hijo `pid` es 0 y entra al
bloque; en el padre `pid` es el PID del hijo y salta a la línea 13. Este `if` es el patrón con el que
todo programa separa «lo que hace el hijo» de «lo que hace el padre».

## Línea 9 · `x += 5;`

Solo el hijo ejecuta esto: su `x` pasa a 15. Compara la memoria de los dos procesos: la `x` del
padre no cambió. Linux no copia toda la memoria en el `fork`: comparte las páginas y copia una
recién cuando alguno escribe en ella (*copy-on-write*). Para el programa, el efecto es el mismo que
una copia completa.

## Línea 10 · `printf("hijo: x = %d\n", x);`

El hijo imprime `hijo: x = 15`. Ambos procesos escriben en la misma terminal porque el hijo heredó
el descriptor 1 del padre.

## Línea 11 · `return 3;`

El hijo sale de `main` con el valor 3, que se convierte en su **código de salida** (como
`exit(3)`). El kernel cierra sus descriptores y libera su memoria, pero conserva el código para el
padre: el hijo queda **zombie** hasta que el padre lo recoja.

## Línea 13 · `x -= 5;`

Solo el padre llega aquí (el hijo salió por el `return` dentro del `if`). Su `x` pasa de 10 a 5,
sin afectar a la del hijo.

## Línea 14 · `printf("padre: x = %d, mi hijo es %d\n", x, pid);`

El padre imprime su `x` (5) y el PID de su hijo. El orden de las líneas en la terminal depende de qué
proceso avanzó primero; con otra planificación puede salir primero el mensaje del hijo.

## Línea 16 · `wait(&status);`

`wait` bloquea al padre hasta que alguno de sus hijos termine. Si el hijo ya terminó (es zombie),
`wait` retorna de inmediato. Este paso puede verse dos veces: cuando el padre se bloquea y cuando
despierta porque el hijo terminó.

Al retornar, el zombie desaparece (fue **recogido**) y `status` contiene el código de salida
empaquetado junto a otros datos.

## Línea 17 · `printf("el hijo terminó con %d\n", WEXITSTATUS(status));`

`WEXITSTATUS` extrae el código de salida de `status`: imprime `el hijo terminó con 3`. Si el hijo
pudiera haber muerto por una señal, antes habría que preguntar `WIFEXITED(status)`.

## Línea 18 · `return 0;`

El padre termina con código 0.

## Línea 19 · `}`

Fin de `main`: el proceso termina. Este paso también aparece para el hijo, al salir de `main` con su
`return 3`.

## Al terminar

- `fork` crea un proceso copiando al que lo llama; los dos siguen desde el mismo punto.
- El valor devuelto distingue al padre (PID del hijo) del hijo (0).
- Después del fork, las memorias son independientes: la `x` del hijo terminó en 15 y la del padre en 5.
- `wait` espera a un hijo, lo recoge (deja de ser zombie) y entrega su código de salida.

## Para practicar

- Cambia la planificación a *Aleatoria* y prueba varias semillas. ¿Cambia el orden de los mensajes?
  ¿Cambian los valores de `x`?
- Quita el `return 3;` del hijo. ¿Qué líneas ejecuta ahora el hijo? ¿Cuántas veces aparece
  «el hijo terminó…»?
- Quita el `wait`. ¿Qué estado tiene el hijo cuando el padre termina?
