# Un mutex arregla la carrera

> **Objetivo:** proteger una sección crítica con un mutex, ver cómo un hilo se bloquea cuando el
> mutex está tomado y comprobar que ya no se pierden sumas.

Es el mismo programa del ejemplo 12 con dos líneas nuevas: `pthread_mutex_lock` antes de leer el
contador y `pthread_mutex_unlock` después de guardarlo. Las tres líneas del medio forman la
**sección crítica**: el código que toca el dato compartido y que no debe ejecutar más de un hilo a la
vez.

Un **mutex** (de *mutual exclusion*) es un candado con dueño:

- `lock` lo toma si está libre; si lo tiene otro hilo, el que llama **se bloquea** hasta que se
  libere;
- `unlock` lo suelta y, si alguien esperaba, lo despierta para que lo tome.

El planificador puede seguir cambiando de hilo en medio de la sección crítica: el mutex no lo impide.
Lo que impide es que el **otro** hilo entre a la sección mientras tanto: llega al `lock` y se
duerme. Así, leer-sumar-guardar se vuelve indivisible para los demás hilos.

El mutex `candado` es una variable **global**, compartida por los hilos: un mutex local de cada hilo
no protegería nada. `PTHREAD_MUTEX_INITIALIZER` lo deja listo y libre sin llamar a
`pthread_mutex_init`.

En el lienzo, el mutex aparece con su dueño, y los hilos que esperan se ven bloqueados en él.

## Línea 20 · `pthread_create(&a, NULL, sumar, NULL);`

Se crea el primer hilo que suma.

## Línea 21 · `pthread_create(&b, NULL, sumar, NULL);`

Y el segundo. Los dos ejecutarán `sumar` en paralelo.

## Línea 22 · `pthread_join(a, NULL);`

`main` espera al primer hilo.

## Línea 8 · `for (int i = 0; i < 3; i++) {`

Cada hilo cuenta sus tres vueltas con su propio `i`.

## Línea 9 · `pthread_mutex_lock(&candado);`

El hilo pide el candado. Si este paso muestra que lo **obtuvo**, ahora es su dueño y entra a la
sección crítica. Si muestra que quedó **bloqueado**, el otro hilo está dentro: este espera, sin usar
CPU, hasta que el otro haga `unlock`.

## Línea 10 · `int leido = contador;`

Leer, con el candado tomado. El otro hilo no puede estar leyendo al mismo tiempo: si lo intentó,
está bloqueado en la línea 9.

## Línea 11 · `leido = leido + 1;`

Sumar. Aunque el planificador le dé el turno al otro hilo aquí, ese hilo no puede avanzar más allá
del `lock`.

## Línea 12 · `contador = leido;`

Guardar. Nadie pudo modificar el contador entre la lectura y esta escritura: la suma no se pierde.

## Línea 13 · `pthread_mutex_unlock(&candado);`

El hilo suelta el candado. Si el otro estaba esperando, se despierta y lo obtiene. Olvidar el
`unlock` dejaría al otro hilo bloqueado para siempre.

## Línea 15 · `return NULL;`

El hilo terminó sus vueltas.

## Línea 16 · `}`

El hilo termina; si `main` lo esperaba en `pthread_join`, despierta.

## Línea 23 · `pthread_join(b, NULL);`

`main` espera al segundo hilo.

## Línea 24 · `printf("contador = %d\n", contador);`

Ahora siempre imprime `contador = 6`, con cualquier planificación.

## Línea 25 · `return 0;`

`main` retorna y el proceso termina.

## Línea 26 · `}`

Fin de `main`. Compara con el ejemplo 12: mismo trabajo y misma planificación, pero ninguna suma
se perdió.

## Al terminar

- Un mutex hace que una sección crítica la ejecute un solo hilo a la vez.
- Todo acceso al dato compartido debe hacerse con el mutex tomado; proteger solo algunos no sirve.
- Un hilo que pide un mutex ocupado se bloquea sin gastar CPU.
- La sección crítica debe ser corta: mientras un hilo la ocupa, los demás esperan.

## Para practicar

- Prueba la planificación **Aleatoria** con varias semillas, o **Manual** intentando provocar la
  carrera. ¿Puedes conseguir un resultado distinto de 6?
- Mueve el `lock` y el `unlock` fuera del `for` (antes y después del bucle). ¿Sigue dando 6? ¿Qué se
  pierde?
- Quita solo el `unlock`. ¿Qué detecta el visualizador?
