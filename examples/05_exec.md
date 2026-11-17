# exec: el hijo se convierte en otro programa

> **Objetivo:** entender que `exec` no crea un proceso sino que **reemplaza el programa** del proceso
> actual, y ver la combinación `fork` + `exec` + `wait`, que es exactamente lo que hace un shell al
> ejecutar un comando.

`fork` crea un proceso, pero copia del mismo programa. Para ejecutar **otro** programa se usa la
familia `exec`: el proceso descarta su código, sus datos, su heap y su stack, carga el programa nuevo
y empieza a ejecutarlo desde su `main`. Lo que **no** cambia:

- el PID y el padre: sigue siendo el mismo proceso;
- los descriptores abiertos: el programa nuevo escribe donde escribía el viejo;
- el directorio de trabajo, el usuario y otros atributos del proceso.

Si `exec` funciona, **nunca retorna**: el código que lo llamó ya no existe. Por eso lo que sigue a
un `exec` solo se ejecuta si falló.

El shell hace esto con cada comando: `fork` para tener un hijo, `exec` en el hijo para cargar el
comando, y `wait` en el padre para esperar a que termine antes de mostrar el prompt otra vez.

El programa cargado (`ls`) no se compiló con información de depuración, así que el visualizador lo
muestra como una **caja negra**: se ven sus llamadas al sistema y su salida, pero no sus líneas.

## Línea 6 · `printf("antes del exec\n");`

Se imprime antes de crear al hijo. Como la salida va a una terminal, el `'\n'` vacía el buffer de
`printf` de inmediato. Si la salida fuera un archivo o un pipe, el texto podría seguir en el buffer
al hacer `fork`, el hijo heredaría una copia, y el mensaje saldría dos veces.

## Línea 7 · `pid_t pid = fork();`

Se crea el hijo, todavía con el mismo programa. En el padre `pid` es el PID del hijo; en el hijo, 0.

## Línea 8 · `if (pid == 0) {`

El hijo entra al bloque para cambiar de programa; el padre lo salta y va a esperar.

## Línea 9 · `execlp("ls", "ls", NULL);`

El hijo pide cargar `ls`. Los argumentos de `execlp`:

- `"ls"`: el programa. La `p` del nombre significa que se busca en los directorios de `PATH` (aquí
  se encuentra `/usr/bin/ls`);
- `"ls", NULL`: la lista de argumentos que recibirá el programa, `argv[0]`, `argv[1]`…, terminada en
  `NULL`. Por convención `argv[0]` es el nombre del programa.

En el lienzo, el proceso hijo conserva su PID pero ahora ejecuta `ls`, como caja negra. Sus pasos
siguientes no tienen línea de C: son los de `ls`, que escribe la lista de archivos en el descriptor
1 que heredó, y termina.

Las líneas 10 y 11 (`perror` y `return 1`) solo se ejecutarían si `exec` fallara, por ejemplo si el
programa no existiera. Prueba cambiar `"ls"` por `"lss"`.

## Línea 13 · `waitpid(pid, NULL, 0);`

El padre espera **a ese hijo en particular** (`waitpid` con su PID; `wait` esperaría a cualquiera).
`NULL` indica que no le interesa el código de salida. El padre queda bloqueado mientras `ls` trabaja,
y despierta cuando termina: este paso aparece al bloquearse y otra vez al recogerlo.

## Línea 14 · `printf("ls terminó\n");`

Gracias al `waitpid`, este mensaje sale siempre **después** de la salida de `ls`. Sin él, el padre
podría imprimir antes, o terminar mientras `ls` todavía escribe.

## Línea 15 · `return 0;`

El padre termina con código 0.

## Línea 16 · `}`

Fin del programa. Si lo ejecutara un shell, ahora mostraría el prompt otra vez.

## Al terminar

- `exec` reemplaza el programa del proceso; el PID y los descriptores se conservan.
- Si `exec` retorna, falló: lo que viene después es manejo de errores.
- `fork` + `exec` + `wait` es la forma en que un shell ejecuta cada comando.
- `waitpid(pid, …)` espera a un hijo específico.

## Para practicar

- Cambia el comando por `execlp("ls", "ls", "-l", NULL);`. ¿Qué cambia en la salida?
- Cambia `"ls"` por un programa que no existe. ¿Qué líneas se ejecutan ahora y qué imprime `perror`?
- Quita el `waitpid`. ¿Puede salir «ls terminó» antes que la lista de archivos?
