# Un pipeline: ls | grep c | wc -l

> **Objetivo:** construir a mano lo que hace el shell con `ls | grep c | wc -l`: dos pipes, tres
> hijos, `dup2` para redirigir la entrada y la salida de cada uno, y `exec` para cargar cada comando.

Los programas de Unix leen de la entrada estándar (descriptor 0) y escriben en la salida estándar
(descriptor 1) sin saber qué hay detrás. El shell aprovecha eso: para un pipeline, crea los pipes,
conecta la salida de cada comando con la entrada del siguiente, y los programas funcionan sin
cambiar una línea.

La herramienta para conectar es `dup2(viejo, nuevo)`: hace que el descriptor `nuevo` apunte a lo
mismo que `viejo`. Después de `dup2(a[1], 1)`, escribir en el descriptor 1 es escribir en el pipe `a`.
Y como `exec` conserva los descriptores, el programa cargado hereda esa redirección.

El plan:

- pipe `a` conecta `ls` con `grep`; pipe `b` conecta `grep` con `wc`;
- hijo 1: salida → `a`, luego `exec ls`;
- hijo 2: entrada ← `a`, salida → `b`, luego `exec grep c`;
- hijo 3: entrada ← `b`, luego `exec wc -l` (su salida sigue siendo la terminal);
- el padre cierra todo y espera a los tres.

Lo más delicado es **cerrar**: cada proceso debe cerrar todos los extremos que no usa. Si quedara
abierto un extremo de escritura de `a` en cualquier proceso, `grep` nunca vería fin de archivo y no
terminaría; y entonces `wc` tampoco.

## Línea 8 · `pipe(a);`

Pipe `a`: `a[0]` = 3 (lectura) y `a[1]` = 4 (escritura).

## Línea 9 · `pipe(b);`

Pipe `b`: `b[0]` = 5 y `b[1]` = 6. El padre tiene ahora 4 descriptores hacia pipes, y cada hijo que
cree los heredará todos.

## Línea 10 · `if (fork() == 0) {`

Primer hijo, el que será `ls`. Hereda los descriptores 3, 4, 5 y 6.

## Línea 11 · `dup2(a[1], 1);`

En el primer hijo, el descriptor 1 deja de ser la terminal y pasa a ser la escritura del pipe `a`.
Mira en el lienzo cómo la salida de este proceso ahora apunta al pipe.

## Línea 12 · `close(a[0]); close(a[1]); close(b[0]); close(b[1]);`

Se cierran los cuatro descriptores originales. `a[1]` ya está duplicado en el 1, así que cerrar el 4
no corta la conexión: el pipe sigue abierto por el descriptor 1. Los demás no le sirven a `ls`.

Esta misma línea aparece en los tres hijos (líneas 12, 19 y 25) y en el padre (29): es el patrón
«después de redirigir, cerrar todo lo demás».

## Línea 13 · `execlp("ls", "ls", NULL);`

El primer hijo se convierte en `ls` (caja negra). `ls` escribe la lista de archivos en su descriptor
1, que ahora es el pipe `a`. Al terminar, su salida se cierra y el pipe `a` pierde a su único
escritor.

## Línea 16 · `if (fork() == 0) {`

Segundo hijo, el que será `grep`. Es hijo del padre (no del primer hijo): el primer hijo nunca llega
a esta línea porque hizo `exec`.

## Línea 17 · `dup2(a[0], 0);`

La entrada estándar del segundo hijo pasa a ser la lectura del pipe `a`: `grep` leerá lo que
escriba `ls`.

## Línea 18 · `dup2(b[1], 1);`

Y su salida estándar pasa a ser la escritura del pipe `b`.

## Línea 19 · `close(a[0]); close(a[1]); close(b[0]); close(b[1]);`

Otra vez se cierran los originales. Cerrar `a[1]` aquí es imprescindible: si `grep` conservara un
extremo de escritura de `a`, sería escritor de su propia entrada y nunca vería fin de archivo.

## Línea 20 · `execlp("grep", "grep", "c", NULL);`

El segundo hijo se convierte en `grep c`, que copia a su salida las líneas que contienen la letra c.
Lee de `a` hasta EOF, escribe en `b`, y termina.

## Línea 23 · `if (fork() == 0) {`

Tercer hijo, el que será `wc`.

## Línea 24 · `dup2(b[0], 0);`

Su entrada pasa a ser la lectura del pipe `b`. Su salida no se toca: sigue siendo la terminal, y ahí
aparecerá el resultado final.

## Línea 25 · `close(a[0]); close(a[1]); close(b[0]); close(b[1]);`

Se cierran los originales también en el tercer hijo.

## Línea 26 · `execlp("wc", "wc", "-l", NULL);`

El tercer hijo se convierte en `wc -l`, que cuenta las líneas que recibe hasta EOF e imprime el total
en la terminal.

## Línea 29 · `close(a[0]); close(a[1]); close(b[0]); close(b[1]);`

El padre no usa ningún pipe, así que cierra los cuatro extremos. Si no lo hiciera, el padre sería
escritor de `a` y de `b`, y ni `grep` ni `wc` verían jamás fin de archivo: el pipeline se colgaría.

## Línea 30 · `while (wait(NULL) > 0)`

El padre recoge a los tres hijos, en el orden en que vayan terminando. `wait` devuelve el PID de un
hijo recogido, o -1 cuando ya no le quedan hijos: ahí termina el bucle. El cuerpo es solo `;` (no hay
nada más que hacer).

## Línea 32 · `return 0;`

Con los tres hijos recogidos, el padre termina.

## Línea 33 · `}`

Fin del programa. En la terminal quedó un solo número: cuántas líneas del listado contienen una c.

## Al terminar

- `dup2(viejo, nuevo)` redirige un descriptor; `exec` conserva esa redirección.
- Cada etapa del pipeline es un proceso; los pipes conectan la salida de uno con la entrada del
  siguiente.
- Hay que cerrar todos los extremos no usados, también en el padre, o alguna etapa no verá fin de
  archivo.
- Así es como el shell implementa `|`, `<` y `>`.

## Para practicar

- Comenta el `close(a[1])` del padre (línea 29). ¿Qué procesos quedan bloqueados?
- Cambia `"c"` por otra letra y predice el número antes de ejecutar.
- Agrega una cuarta etapa, `sort`, entre `grep` y `wc`. ¿Cuántos pipes y cuántos `close` necesitas?
