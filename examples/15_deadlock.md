# Deadlock con dos mutex

> **Objetivo:** provocar un interbloqueo (*deadlock*) con dos hilos que toman dos mutex en orden
> opuesto, reconocer las cuatro condiciones que lo hacen posible y ver cómo se evita.

Un **deadlock** ocurre cuando un grupo de tareas queda esperando algo que tiene otra tarea del
mismo grupo: nadie puede avanzar y nadie va a soltar lo que tiene. El programa no falla ni termina:
simplemente se queda detenido para siempre.

Aquí hay dos mutex, `a` y `b`, y dos hilos:

- el hilo `uno` toma `a` y después `b`;
- el hilo `dos` toma `b` y después `a`.

Si cada uno alcanza a tomar su primer mutex antes de que el otro termine, `uno` tiene `a` y espera
`b`, mientras `dos` tiene `b` y espera `a`: un ciclo. El `usleep` entre los dos `lock` hace que eso
ocurra casi siempre, porque le da tiempo al otro hilo de tomar su primer mutex.

Para que haya deadlock tienen que cumplirse a la vez las cuatro **condiciones de Coffman**:

- **exclusión mutua**: un mutex lo tiene un solo hilo;
- **retener y esperar**: cada hilo pide el segundo mutex sin soltar el primero;
- **sin expropiación**: nadie puede quitarle un mutex a otro hilo;
- **espera circular**: `uno` espera a `dos` y `dos` espera a `uno`.

Basta romper una para evitarlo. La más práctica es la última: si **todos los hilos toman los mutex
en el mismo orden**, no puede formarse un ciclo.

## Línea 30 · `pthread_create(&t1, NULL, uno, NULL);`

Se crea el hilo `uno`.

## Línea 31 · `pthread_create(&t2, NULL, dos, NULL);`

Se crea el hilo `dos`.

## Línea 32 · `pthread_join(t1, NULL);`

`main` espera al hilo `uno`. Como `uno` nunca va a terminar, `main` también queda bloqueado para
siempre: el deadlock arrastra a todas las tareas que esperan a sus participantes.

## Línea 9 · `pthread_mutex_lock(&a);`

El hilo `uno` toma el mutex `a`. Mira en el lienzo cómo `a` queda con dueño.

## Línea 10 · `usleep(1000); // trabaja un poco con a tomado`

`uno` duerme 1 ms **con `a` tomado**. Mientras tanto el otro hilo avanza. Dormir o hacer trabajo largo
dentro de una sección crítica alarga el tiempo en que los demás pueden quedar esperando.

## Línea 19 · `pthread_mutex_lock(&b);`

El hilo `dos` toma el mutex `b`. Ahora cada hilo tiene uno de los dos mutex.

## Línea 20 · `usleep(1000);`

`dos` también duerme con su mutex tomado.

## Línea 11 · `pthread_mutex_lock(&b);`

`uno` pide `b`, pero lo tiene `dos`: `uno` se bloquea. Todavía no es un deadlock: si `dos` soltara
`b`, `uno` seguiría.

## Línea 21 · `pthread_mutex_lock(&a);`

`dos` pide `a`, pero lo tiene `uno`, que está esperando `b`, que tiene `dos`. Se cerró el ciclo. Las
tres tareas del proceso están bloqueadas y ninguna puede despertar a las otras: el visualizador
declara el deadlock. En el lienzo se ve el ciclo: cada mutex con su dueño y cada hilo esperando el
mutex del otro.

Las líneas que siguen (los `printf` y los `unlock`) nunca se ejecutan.

## Al terminar

- Un deadlock es una espera circular: cada tarea espera un recurso que tiene otra del ciclo.
- Necesita exclusión mutua, retener y esperar, sin expropiación y espera circular, todas a la vez.
- Se evita tomando los mutex siempre en el mismo orden (o con `pthread_mutex_trylock` y soltando todo
  si falla).
- Un deadlock no da error: el programa se queda detenido.

## Para practicar

- Cambia `dos` para que tome primero `a` y después `b`. ¿Puede haber deadlock ahora?
- Quita los dos `usleep`. ¿Sigue ocurriendo con la planificación por turnos? ¿Y con la **Manual**,
  eligiendo tú el orden?
- Reescribe `dos` con `pthread_mutex_trylock(&a)`: si falla, suelta `b`, espera un poco y vuelve a
  intentar desde el principio.
