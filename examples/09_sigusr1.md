# Señales entre procesos: SIGUSR1

> **Objetivo:** instalar un handler con `sigaction`, enviar una señal con `kill`, y ver cómo el
> kernel interrumpe al proceso destino para ejecutar el handler y después lo deja continuar.

Una **señal** es un aviso asíncrono: puede llegar en cualquier momento, entre dos instrucciones
cualesquiera del proceso que la recibe. `SIGUSR1` y `SIGUSR2` no tienen un significado fijo: están
reservadas para que los programas las usen como quieran. Su acción por defecto es **terminar** el
proceso, así que para usarlas hay que instalar un **handler** antes de que lleguen.

Cuando llega una señal con handler, el kernel guarda el estado del proceso, ejecuta el handler como
si fuera una llamada a función, y al terminar restaura el estado: el programa sigue donde estaba.

Un handler puede interrumpir al programa en cualquier punto, así que debe hacer muy poco. Lo
recomendado es solo marcar una variable global de tipo `volatile sig_atomic_t` y que el programa la
revise después:

- `volatile` obliga al compilador a leerla de memoria cada vez (podría cambiar «por fuera»);
- `sig_atomic_t` garantiza que se lee y escribe de una sola vez, sin quedar a medias.

## Línea 14 · `struct sigaction sa = {0};`

`struct sigaction` describe qué hacer con una señal: el handler, qué otras señales bloquear mientras
corre y opciones. Se inicializa todo en cero: máscara vacía y sin opciones especiales.

## Línea 15 · `sa.sa_handler = manejador;`

Se indica la función que atenderá la señal. Un handler recibe un `int`: el número de la señal.

## Línea 16 · `sigaction(SIGUSR1, &sa, NULL);`

Se instala el handler para `SIGUSR1`. Se hace **antes** del `fork` a propósito: el hijo hereda la
tabla de acciones de señales, así que también tendrá el handler, sin ventana en la que una señal
temprana lo mate.

## Línea 17 · `pid_t pid = fork();`

Se crea el hijo, con el handler ya instalado.

## Línea 18 · `if (pid == 0) {`

El hijo entra a esperar la señal; el padre salta a la línea 24.

## Línea 19 · `while (!llego)`

El hijo revisa la bandera antes de dormir: si la señal ya hubiera llegado, no debe esperar otra. Y
después de cada despertar vuelve a revisar, porque `pause` también retorna por cualquier otra señal
con handler.

Queda una carrera pequeña: si la señal llegara justo entre esta comprobación y el `pause`, el hijo
dormiría para siempre. La solución robusta es bloquear la señal y esperar con `sigsuspend`.

## Línea 20 · `pause();`

El hijo se bloquea hasta que llegue una señal que ejecute un handler. No consume CPU mientras espera.
Cuando llega `SIGUSR1`, el kernel lo despierta y en este mismo punto entrega la señal: el siguiente
paso del hijo ya está dentro de `manejador`.

## Línea 9 · `llego = 1;`

Ya dentro del handler. Fíjate en el stack del hijo: el frame de `manejador` está **encima** del de
`main`, que quedó detenido en `pause` como si `manejador` hubiera sido llamado desde ahí. El handler
marca la bandera.

## Línea 10 · `printf("hijo: recibí la señal %d\n", sig);`

Imprime `hijo: recibí la señal 10` (el número de `SIGUSR1` en Linux). En un programa real se evita
`printf` en un handler: si la señal interrumpió otro `printf` a la mitad, el buffer interno podría
quedar inconsistente. Lo seguro es `write`.

## Línea 11 · `}`

El handler termina: el kernel restaura el estado guardado y el hijo vuelve a `pause`, que retorna -1
(interrumpido por una señal, `errno` = `EINTR`). El bucle revisa `llego`, que ahora vale 1, y sale.

## Línea 21 · `printf("hijo: sigo y termino\n");`

El hijo sigue con su trabajo normal después de atender la señal.

## Línea 22 · `return 0;`

El hijo termina con código 0.

## Línea 24 · `sleep(1);`

El padre duerme un segundo para darle tiempo al hijo de llegar a `pause`. Es una forma simple pero
débil de sincronizar: no garantiza nada, solo lo hace probable. Gracias a la bandera, el programa
funciona igual si la señal llega antes de que el hijo duerma.

## Línea 25 · `printf("padre: envío SIGUSR1 a %d\n", pid);`

El padre anuncia el envío.

## Línea 26 · `kill(pid, SIGUSR1);`

A pesar del nombre, `kill` solo **envía** una señal; que el proceso muera depende de la acción que
tenga para ella. La señal queda pendiente en el hijo y el kernel lo despierta de `pause`. `kill`
retorna de inmediato: no espera a que el handler se ejecute.

## Línea 27 · `wait(NULL);`

El padre espera a que el hijo termine y lo recoge.

## Línea 28 · `return 0;`

El padre termina con código 0.

## Línea 29 · `}`

Fin de `main`, en el padre o en el hijo según quién avanzó en este paso.

## Al terminar

- Una señal interrumpe al proceso destino en cualquier punto; si tiene handler, se ejecuta y el
  programa continúa donde estaba.
- `kill` envía; no espera ni mata necesariamente.
- Las acciones de señales se heredan en el `fork`: instala los handlers antes de crear hijos.
- El handler hace lo mínimo (una bandera `volatile sig_atomic_t`) y el programa reacciona después.

## Para practicar

- Comenta la línea 16 (`sigaction`). ¿Cómo termina el hijo y qué recibe el padre en `wait`?
- Quita el `sleep(1)` del padre. ¿Sigue funcionando? ¿En qué línea está el hijo cuando llega la señal?
- Haz que el padre envíe dos `SIGUSR1` seguidos. ¿Cuántas veces se ejecuta el handler? Prueba
  distintas planificaciones: dos señales iguales pendientes a la vez cuentan como una sola.
