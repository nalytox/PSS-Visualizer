# fork dentro de un bucle

> **Objetivo:** predecir cuántos procesos crea un `fork` dentro de un bucle, dibujar el árbol de
> procesos que resulta y ver qué pasa con los hijos cuyo padre termina primero (huérfanos).

Cada hijo creado por `fork` sigue ejecutando **el mismo código desde el mismo punto** que su padre,
con una copia de sus variables, incluido el contador del bucle. Por eso un hijo creado en la primera
vuelta también hará las vueltas que faltan, y creará sus propios hijos.

Cuenta vuelta por vuelta: antes del bucle hay 1 proceso. En cada vuelta, **todos** los procesos que
existen hacen `fork`, así que la cantidad se duplica: 2, 4 y 8. Con 3 vueltas hay 2³ = 8 procesos y
hubo 7 llamadas exitosas a `fork`. Con `n` vueltas serían 2ⁿ: crece muy rápido.

El árbol no es parejo: el proceso original tiene 3 hijos (uno por vuelta), su primer hijo tiene 2,
y los creados en la última vuelta no tienen ninguno. Puedes reconocer en qué vuelta nació cada uno
mirando el árbol del lienzo.

Nadie hace `wait` en este programa. Cuando un padre termina antes que sus hijos, los hijos quedan
**huérfanos** y el kernel los entrega a **init** (PID 1), que los recogerá al terminar.

## Línea 5 · `for (int i = 0; i < 3; i++) {`

Cada proceso evalúa su propia copia de `i`. Un hijo creado cuando el padre tenía `i` = 0 empieza con
`i` = 0 y en este paso hace `i++`: le quedan las vueltas con `i` = 1 y 2. Uno creado con `i` = 2 sale
del bucle de inmediato. Selecciona un proceso para ver su `i` en la memoria.

## Línea 6 · `fork();`

Si este paso tiene el evento de fork, el proceso que avanzó acaba de crear un hijo (míralo aparecer
en el árbol). Si no lo tiene, es el primer paso de un hijo recién creado, que termina esta misma
línea en su copia.

Aquí no se guarda el valor que devuelve `fork`, así que padre e hijo hacen exactamente lo mismo de
ahí en adelante: nada los distingue salvo su PID.

## Línea 8 · `printf("soy %d, hijo de %d\n", getpid(), getppid());`

Cada uno de los 8 procesos imprime su PID y el de su padre. `getpid()` devuelve el propio PID y
`getppid()` el del padre **en este momento**. Al proceso original lo lanzó el visualizador, que
queda fuera de la traza: por eso aparece como hijo de 1.

Si un padre ya hubiera terminado al llegar aquí, su hijo diría «hijo de 1»: ya fue adoptado por init.

## Línea 9 · `return 0;`

Cada proceso sale de `main` con código 0.

## Línea 10 · `}`

El proceso termina. Si todavía tenía hijos vivos, quedan **huérfanos**: en el lienzo verás que pasan
a colgar de init (1). Como nadie hace `wait`, los que terminan antes que su padre quedan **zombies**
hasta que el padre termina y ellos pasan a init, que los recoge.

## Al terminar

- Un hijo hereda el punto de ejecución y las variables: continúa el bucle desde donde estaba su padre.
- `n` vueltas con un `fork` cada una producen 2ⁿ procesos (2ⁿ − 1 llamadas a `fork`).
- Sin `wait`, los hijos quedan zombies o huérfanos; init adopta y recoge a los huérfanos.
- El orden de los mensajes depende de la planificación, no del orden de creación.

## Para practicar

- Antes de ejecutarlo con 4 vueltas, predice cuántas líneas «soy…» saldrán. Compruébalo.
- Cambia `fork();` por `if (fork() == 0) break;`. ¿Cuántos procesos hay ahora y qué forma tiene el
  árbol? (Solo el original sigue en el bucle.)
- Agrega `while (wait(NULL) > 0);` antes del `return`. ¿Siguen apareciendo huérfanos?
