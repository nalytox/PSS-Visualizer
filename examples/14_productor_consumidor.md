# Productor y consumidor

> **Objetivo:** coordinar dos hilos que comparten un buffer limitado usando un mutex y dos variables
> de condición, y entender por qué la espera se escribe con `while` y no con `if`.

Es un problema clásico de sincronización. Un hilo **productor** genera datos (los números del 1 al 4)
y los pone en un buffer con capacidad para `N` = 2 elementos. Un hilo **consumidor** los saca y los
imprime. Hay dos situaciones en las que un hilo no puede avanzar:

- el productor, si el buffer está **lleno**: debe esperar a que haya espacio;
- el consumidor, si el buffer está **vacío**: debe esperar a que haya datos.

El mutex `m` protege todas las variables compartidas (`buffer`, `cantidad`, `entra`, `sale`). Pero
un mutex solo da exclusión: no sirve para *esperar a que algo cambie*. Esperar dando vueltas
(«mientras esté lleno, revisar otra vez») gastaría CPU y además tendría el mutex tomado, impidiendo
que el otro hilo cambie la situación.

Para eso están las **variables de condición**. `pthread_cond_wait(&c, &m)` hace dos cosas en un solo
paso indivisible: **suelta el mutex** y **duerme** en la condición `c`. Cuando otro hilo hace
`pthread_cond_signal(&c)`, el dormido despierta, **vuelve a tomar el mutex** y recién entonces
retorna. Aquí hay dos condiciones: `hay_espacio` (la espera el productor) y `hay_datos` (la espera el
consumidor).

El buffer es **circular**: `entra` es la posición donde el productor escribe y `sale` la posición
donde el consumidor lee; las dos avanzan y vuelven a 0 al llegar a `N`, con `% N`.

Con la planificación por turnos, el mutex alterna a los hilos de tal modo que el buffer nunca se
llena ni se vacía en el momento justo, y no se llega a ver ningún `pthread_cond_wait`. Para verlos,
elige la planificación **Aleatoria** y prueba semillas hasta que un hilo se duerma en una condición.

## Línea 44 · `pthread_create(&p, NULL, productor, NULL);`

Se crea el hilo productor.

## Línea 45 · `pthread_create(&c, NULL, consumidor, NULL);`

Se crea el hilo consumidor. Desde aquí los dos compiten por el mutex.

## Línea 46 · `pthread_join(p, NULL);`

`main` espera al productor; después esperará al consumidor.

## Línea 14 · `for (int i = 1; i <= TOTAL; i++) {`

El productor genera los valores 1, 2, 3 y 4.

## Línea 15 · `pthread_mutex_lock(&m);`

Antes de mirar o tocar el buffer, el productor toma el mutex. Si el consumidor lo tiene, se bloquea
aquí hasta que lo suelte.

## Línea 16 · `while (cantidad == N)`

Si el buffer está lleno, el productor no puede poner nada y debe esperar. Es un `while` y no un
`if`: al despertar, la condición se vuelve a comprobar, porque otro hilo pudo cambiarla antes de que
este volviera a correr, y porque `pthread_cond_wait` puede retornar sin motivo (*despertar
espurio*).

## Línea 17 · `pthread_cond_wait(&hay_espacio, &m);`

Buffer lleno: el productor suelta el mutex y se duerme en `hay_espacio`, en un solo paso. Si soltar y
dormir fueran dos pasos, el aviso del consumidor podría llegar justo en medio y perderse. Despertará
cuando el consumidor saque un elemento y haga `signal`, y retornará ya con el mutex tomado.

## Línea 18 · `buffer[entra] = i;`

Hay espacio: el productor escribe el valor en la posición `entra`.

## Línea 19 · `entra = (entra + 1) % N;`

Avanza la posición de escritura y, con `% N`, vuelve a 0 después de la última: 0, 1, 0, 1…

## Línea 20 · `cantidad++;`

Un elemento más en el buffer. `cantidad` es lo que ambos hilos consultan para decidir si esperar.

## Línea 21 · `pthread_cond_signal(&hay_datos);`

El productor avisa: «ahora hay datos». Si el consumidor dormía en `hay_datos`, despierta (pero no
corre hasta poder tomar el mutex). Si nadie dormía, el aviso **se pierde**, y no importa: el
consumidor revisa `cantidad` antes de dormir, así que no esperará en vano.

## Línea 22 · `pthread_mutex_unlock(&m);`

El productor suelta el mutex. Ahora el consumidor puede tomarlo.

## Línea 24 · `return NULL;`

El productor terminó de generar los 4 valores.

## Línea 25 · `}`

El hilo productor termina; `main` puede pasar a esperar al consumidor.

## Línea 28 · `for (int i = 0; i < TOTAL; i++) {`

El consumidor saca exactamente `TOTAL` elementos.

## Línea 29 · `pthread_mutex_lock(&m);`

El consumidor toma el mutex para mirar el buffer. Si el productor lo tiene, espera aquí.

## Línea 30 · `while (cantidad == 0)`

Si el buffer está vacío, no hay nada que sacar: hay que esperar. De nuevo `while`, por las mismas
razones que en el productor.

## Línea 31 · `pthread_cond_wait(&hay_datos, &m);`

Buffer vacío: el consumidor suelta el mutex y se duerme en `hay_datos`, hasta que el productor ponga
algo y lo avise.

## Línea 32 · `int x = buffer[sale];`

Hay datos: el consumidor copia el valor de la posición `sale` a su variable local `x`.

## Línea 33 · `sale = (sale + 1) % N;`

Avanza la posición de lectura, en círculo como la de escritura. Como cada uno avanza en el mismo
orden, los valores salen en el orden en que entraron (FIFO).

## Línea 34 · `cantidad--;`

Un elemento menos en el buffer. Junto con el aviso de la línea siguiente, es lo que deja seguir al
productor si estaba esperando espacio.

## Línea 35 · `pthread_cond_signal(&hay_espacio);`

El consumidor avisa: «ahora hay espacio». Despierta al productor si estaba dormido esperando lugar.

## Línea 36 · `pthread_mutex_unlock(&m);`

El consumidor suelta el mutex.

## Línea 37 · `printf("consumí %d\n", x);`

Imprime **fuera** de la sección crítica: usa su copia local `x`, así que no necesita el mutex, y
mientras imprime el productor puede seguir trabajando. Mantener las secciones críticas cortas
mejora el paralelismo.

## Línea 39 · `return NULL;`

El consumidor sacó los 4 valores.

## Línea 40 · `}`

El hilo consumidor termina.

## Línea 47 · `pthread_join(c, NULL);`

`main` espera al consumidor.

## Línea 48 · `return 0;`

Los dos hilos terminaron: se imprimió `consumí 1` a `consumí 4`, en orden, sin perder ni repetir
ningún valor.

## Línea 49 · `}`

Fin de `main`. El mutex y las condiciones son globales inicializadas estáticamente: no hace falta
destruirlos antes de terminar.

## Al terminar

- El mutex protege los datos compartidos; las variables de condición permiten **esperar** a que
  cambien sin gastar CPU.
- `pthread_cond_wait` suelta el mutex y duerme en un solo paso, y lo vuelve a tomar al despertar.
- La espera va siempre en un `while (condición)`, nunca en un `if`.
- Un `signal` sin nadie esperando se pierde; por eso el estado se guarda en variables (`cantidad`)
  que se revisan antes de dormir.

## Para practicar

- Con la planificación **Aleatoria**, busca una semilla en la que el productor espere en
  `hay_espacio` y otra en la que el consumidor espere en `hay_datos`.
- Cambia `N` a 1. ¿Cuántas veces tiene que esperar cada hilo?
- Agrega un segundo consumidor (con `TOTAL / 2` vueltas cada uno). ¿Sigue bastando con `signal`, o
  hace falta `broadcast`?
