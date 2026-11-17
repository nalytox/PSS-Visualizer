# Hilos y una condición de carrera

> **Objetivo:** crear hilos con `pthread_create`, ver que comparten las variables globales, y
> entender por qué dos hilos que suman al mismo contador sin protección pierden sumas.

Un **hilo** es otra línea de ejecución **dentro del mismo proceso**. A diferencia de `fork`, no se
copia nada: todos los hilos ven las mismas variables globales y el mismo heap. Cada uno tiene su
propio stack, así que sus variables locales (como `i` y `leido`) son privadas.

Compartir memoria es cómodo, pero peligroso. `contador = contador + 1` parece una sola operación,
pero la CPU la hace en tres pasos: **leer** el valor, **sumar** 1 y **guardar** el resultado. Este
ejemplo los separa en tres líneas para que se vean. Si el planificador cambia de hilo entre la
lectura y la escritura, los dos hilos leen el mismo valor y uno pisa la suma del otro:

- el hilo A lee 0; el hilo B lee 0;
- A guarda 1; B guarda 1. Hubo dos sumas, pero el contador solo subió en 1.

Eso es una **condición de carrera**: el resultado depende del orden exacto en que se intercalan los
hilos. Cada hilo suma 3, así que el resultado correcto es 6; con la planificación por turnos, que
alterna una línea de cada hilo, se pierden la mitad y sale 3.

Lo peor de una carrera en un programa real es que casi nunca se nota: los cambios de hilo justo en el
momento malo son raros, el error aparece de vez en cuando y es difícil de reproducir.

## Línea 19 · `pthread_create(&a, NULL, sumar, NULL);`

Se crea el primer hilo, que empezará ejecutando `sumar(NULL)`. Su identificador queda en `a`. El
segundo argumento (`NULL`) pide atributos por defecto, y el último es el argumento para `sumar`. En
el lienzo, el hilo nuevo aparece **dentro** del mismo proceso, con su propio stack.

## Línea 20 · `pthread_create(&b, NULL, sumar, NULL);`

Segundo hilo, con la misma función. Ahora hay tres hilos en el proceso: `main` y los dos que suman.

## Línea 21 · `pthread_join(a, NULL);`

`main` espera a que termine el hilo `a`. Mientras tanto queda bloqueado y los otros dos avanzan.

## Línea 9 · `for (int i = 0; i < 3; i++) {`

Cada hilo tiene su propio `i`, en su propio stack: los dos bucles cuentan por separado. Selecciona
cada hilo para ver su stack.

## Línea 10 · `int leido = contador;`

**Leer.** El hilo copia el valor global a su variable local `leido`. Observa si el otro hilo acaba
de leer el mismo valor: si es así, una de las dos sumas se va a perder.

## Línea 11 · `leido = leido + 1;`

**Sumar.** El cálculo ocurre en la copia local: el global todavía no cambió, y el otro hilo puede
leerlo con el valor viejo.

## Línea 12 · `contador = leido;`

**Guardar.** El hilo escribe su resultado en el global. Si el otro hilo guardó en el medio, este
valor lo pisa: la suma del otro se pierde.

## Línea 14 · `return NULL;`

El hilo terminó sus tres vueltas y devuelve `NULL` como resultado. Su stack desaparecerá.

## Línea 15 · `}`

Fin de la función del hilo: el hilo termina. Si `main` estaba esperándolo con `pthread_join`,
despierta.

## Línea 22 · `pthread_join(b, NULL);`

`main` espera al segundo hilo. Si ya terminó, retorna enseguida.

## Línea 23 · `printf("contador = %d (debería ser 6)\n", contador);`

Con los dos hilos terminados, `main` imprime el resultado. Con la planificación por turnos sale
`contador = 3`: se perdieron 3 de las 6 sumas.

## Línea 24 · `return 0;`

`main` retorna y el proceso termina. (Si `main` terminara antes que los hilos, el proceso entero
terminaría y se los llevaría: por eso los espera.)

## Línea 25 · `}`

Fin de `main`. Los dos hilos ya terminaron y fueron recogidos con `pthread_join`, así que el proceso
termina sin dejar a nadie trabajando a medias.

## Al terminar

- Los hilos de un proceso comparten globales y heap; cada uno tiene su stack.
- `x = x + 1` no es atómico: es leer, sumar y guardar. Intercalar esos pasos pierde actualizaciones.
- Una condición de carrera depende del orden de ejecución, por eso aparece y desaparece.
- `pthread_join` espera a un hilo, como `wait` espera a un proceso.

## Para practicar

- Cambia la planificación a **Manual** y elige tú el orden: haz que cada hilo complete sus tres
  líneas sin interrupciones. ¿Llegas a 6?
- Con la planificación **Aleatoria**, prueba varias semillas. ¿Qué valores aparecen? ¿Cuál es el
  mínimo posible?
- Corrige el programa con un mutex (compáralo con el ejemplo 13).
