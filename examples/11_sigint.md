# Ctrl+C: atrapar SIGINT

> **Objetivo:** ver qué hace Ctrl+C (enviar `SIGINT`), cómo un programa puede atraparla para terminar
> de forma ordenada, y por qué el handler solo marca una bandera.

Al presionar Ctrl+C en una terminal, no se «cierra» el programa directamente: la terminal le pide al
kernel que envíe la señal `SIGINT` a los procesos en primer plano. La acción por defecto de `SIGINT`
es terminar el proceso, pero un programa puede instalar un handler y decidir qué hacer: guardar su
trabajo, liberar recursos, o ignorar la interrupción.

Este programa cuenta 10 vueltas de un segundo. Si recibe `SIGINT`, el handler marca la bandera
`interrumpido` y el bucle se detiene en la vuelta en que iba.

**Cómo probarlo:** la ejecución normal llega a 10 sin interrupciones. Avanza unos pasos, presiona
el botón **Ctrl+C** de los controles y el programa se vuelve a ejecutar enviando `SIGINT` justo
después del paso actual. Verás la señal llegar desde la terminal, el handler ejecutarse y el mensaje
cambiar.

## Línea 13 · `signal(SIGINT, al_ctrl_c);`

Se instala el handler para `SIGINT`. Desde ahora Ctrl+C ya no termina el proceso: ejecuta
`al_ctrl_c`.

## Línea 14 · `int vuelta = 0;`

El contador de vueltas, en el stack de `main`.

## Línea 15 · `while (!interrumpido && vuelta < 10) {`

El bucle sigue mientras nadie haya interrumpido y falten vueltas. La bandera se revisa **en cada
vuelta**: es el punto donde el programa reacciona a la señal, en un lugar seguro del código y no en
medio de cualquier instrucción.

## Línea 16 · `vuelta++;`

Se cuenta la vuelta **antes** de dormir: si la interrupción llega durante el `sleep`, `vuelta` ya
dice en qué vuelta iba el programa.

## Línea 17 · `sleep(1);`

El proceso duerme un segundo. En el visualizador el tiempo es virtual: si todos duermen, el reloj
salta directo al próximo despertar, así que las 10 vueltas no tardan 10 segundos reales.

Si llega `SIGINT` mientras duerme, el kernel lo despierta para ejecutar el handler y `sleep` retorna
antes de tiempo.

## Línea 9 · `interrumpido = 1;`

Esto solo ocurre si presionaste Ctrl+C. El handler interrumpió a `main` donde estuviera (mira el
frame de `al_ctrl_c` encima del de `main`) y marca la bandera. No hace nada más: no imprime ni
termina el programa. El bucle verá la bandera en su próxima comprobación.

## Línea 10 · `}`

El handler termina y `main` continúa exactamente donde fue interrumpido. En su siguiente paso, la
condición del `while` encuentra `interrumpido` en 1 y el bucle termina.

## Línea 19 · `if (interrumpido)`

Ya fuera del bucle, se decide qué mensaje imprimir según cómo terminó.

## Línea 20 · `printf("me interrumpiste en la vuelta %d\n", vuelta);`

Solo con Ctrl+C: imprime la vuelta en la que llegó la interrupción. El programa termina de forma
normal, con código 0: decidió él mismo cómo reaccionar.

## Línea 22 · `printf("llegué a 10 sin interrupciones\n");`

Sin Ctrl+C, el bucle completó las 10 vueltas.

## Línea 23 · `return 0;`

El programa termina con código 0, haya sido interrumpido o no.

## Línea 24 · `}`

Fin de `main`: el proceso termina con código 0. Que haya recibido `SIGINT` no cambia su código de
salida, porque la manejó él mismo en vez de dejar que lo matara.

## Al terminar

- Ctrl+C envía `SIGINT` a los procesos en primer plano; por defecto los termina.
- Con un handler, el programa decide: aquí termina ordenadamente, informando dónde iba.
- El handler solo marca una bandera; el programa la revisa en un punto seguro.
- Una señal con handler interrumpe `sleep`.

## Para practicar

- Presiona Ctrl+C en pasos distintos. ¿La vuelta informada coincide con la que iba el contador?
- Comenta la línea 13 y presiona Ctrl+C. ¿Qué pasa con el proceso y con el mensaje final?
- Cambia el handler por `signal(SIGINT, SIG_IGN);` en la línea 13. ¿Qué hace ahora Ctrl+C?
