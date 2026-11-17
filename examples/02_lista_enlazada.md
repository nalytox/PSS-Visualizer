# Lista enlazada con malloc

> **Objetivo:** ver la diferencia entre la memoria del stack y la del heap, construir una lista
> enlazada con `malloc`, recorrerla con un puntero y liberar un nodo con `free`.

Las variables locales viven en el **stack** y desaparecen cuando su función retorna. Pero una lista
debe sobrevivir a la función que crea cada nodo: por eso los nodos se piden al **heap** con
`malloc`. Un bloque del heap existe hasta que alguien lo libera con `free`, sin importar qué
función lo creó.

Cada nodo es un `struct nodo` con un valor y un puntero `sig` al nodo siguiente. El último apunta a
`NULL`, que marca el final. La lista se maneja con un solo puntero, `lista`, que apunta al primer
nodo (la *cabeza*).

`agregar` inserta **al principio**: el nodo nuevo apunta a la cabeza anterior y pasa a ser la nueva
cabeza. Por eso, al agregar 10, 20, 30 y 40, la lista queda 40 → 30 → 20 → 10.

En la memoria del proceso verás aparecer cada bloque del heap con su tamaño y las flechas de `sig`
encadenándolos. Los bytes que nadie escribió todavía se marcan como sin inicializar.

## Línea 17 · `struct nodo *lista = NULL;`

La lista empieza vacía: el puntero vale `NULL` (dirección 0), que nunca es un lugar válido. Es la
forma estándar de decir «no apunta a nada».

## Línea 18 · `for (int i = 1; i <= 4; i++)`

El bucle se repite para `i` = 1, 2, 3 y 4. Esta línea se ejecuta al empezar (inicializa `i`) y
después de cada vuelta (incrementa `i` y vuelve a comparar con 4).

## Línea 19 · `lista = agregar(lista, i * 10);`

Se llama a `agregar` con la cabeza actual y el valor nuevo. La función devuelve la dirección del
nodo recién creado, que pasa a ser la cabeza. El patrón `lista = agregar(lista, …)` es necesario
porque `agregar` recibe una **copia** del puntero: no puede cambiar la variable `lista` de `main`,
así que devuelve el valor nuevo para que `main` lo guarde.

## Línea 10 · `struct nodo *n = malloc(sizeof *n);`

`malloc` reserva un bloque en el heap del tamaño de un `struct nodo` y devuelve su dirección.
`sizeof *n` es el tamaño de lo apuntado: un `int` (4 bytes), 4 bytes de relleno para alinear el
puntero y el puntero `sig` (8 bytes), 16 en total. Escribirlo así evita equivocarse de tipo.

El bloque aún no tiene valores: su contenido es basura hasta que se escriba. Un programa real
revisaría si `n` es `NULL` (no hubo memoria).

## Línea 11 · `n->valor = valor;`

Se escribe el valor en el nodo nuevo, a través del puntero `n`. En la memoria, ese campo deja de
estar sin inicializar.

## Línea 12 · `n->sig = cabeza;`

El nodo nuevo apunta a la cabeza anterior (en la primera vuelta, `NULL`). Así se encadena **delante**
de los que ya existían. Fíjate en la flecha nueva entre bloques del heap.

## Línea 13 · `return n;`

Se devuelve la dirección del nodo. La variable `n` es local y desaparece al retornar, pero el
**bloque** al que apunta sigue en el heap: esa es la razón de usar `malloc`. Si el nodo hubiera sido
una variable local, devolver su dirección sería un error grave (apuntaría a un frame que ya no
existe).

## Línea 14 · `}`

Termina `agregar` y su frame se destruye. El valor devuelto llega a `main`, que lo guarda en `lista`.

## Línea 21 · `int suma = 0;`

Empieza el recorrido. Para recorrer una lista no se necesita saber cuántos nodos tiene: se sigue
la cadena de `sig` hasta encontrar `NULL`.

## Línea 22 · `for (struct nodo *p = lista; p != NULL; p = p->sig)`

El recorrido clásico: `p` empieza en la cabeza; mientras no sea `NULL`, se procesa el nodo y se
avanza con `p = p->sig`. Observa cómo la flecha de `p` salta de un bloque al siguiente.

Se usa una variable aparte (`p`) para no perder la cabeza: si avanzáramos con `lista`, al terminar ya
no tendríamos cómo llegar al primer nodo.

## Línea 23 · `suma += p->valor;`

Se suma el valor del nodo actual. Las vueltas suman 40, 30, 20 y 10.

## Línea 24 · `printf("suma = %d\n", suma);`

Imprime `suma = 100`.

## Línea 26 · `struct nodo *segundo = lista->sig;`

Antes de liberar la cabeza, se guarda la dirección del segundo nodo. Después de `free(lista)` ya no
se puede leer `lista->sig`: el bloque no nos pertenece.

## Línea 27 · `free(lista);`

Se devuelve al asignador el bloque del primer nodo (el de 40). El puntero `lista` conserva la
dirección, pero ahora es un **puntero colgante**: usarlo sería un error de *uso después de free*.

## Línea 28 · `lista = segundo;`

La cabeza pasa a ser el antiguo segundo nodo. La lista quedó 30 → 20 → 10.

## Línea 29 · `printf("ahora la lista empieza en %d\n", lista->valor);`

Imprime `ahora la lista empieza en 30`. Es correcto porque `lista` ya apunta a un bloque vivo.

## Línea 30 · `return 0;`

`main` termina sin liberar los tres nodos que quedan.

## Línea 31 · `}`

El proceso termina y el sistema operativo recupera toda su memoria, heap incluido. Aun así, los tres
bloques sin `free` son **fugas** (*leaks*): el visualizador los reporta al final. En un programa que
corre mucho tiempo, las fugas hacen crecer el consumo de memoria sin límite.

## Al terminar

- El stack guarda las variables locales y se vacía al retornar; el heap guarda lo que debe
  sobrevivir, hasta que se libera con `free`.
- Una lista enlazada son bloques del heap unidos por punteros; `NULL` marca el final.
- Para recorrer se usa un puntero auxiliar; para borrar un nodo, primero se guarda lo que se necesita
  de él y después se libera.
- Cada `malloc` debe tener su `free`: aquí quedaron 3 fugas.

## Para practicar

- Agrega antes del `return` un bucle que libere toda la lista sin fugas. Pista: guarda `p->sig` antes
  de hacer `free(p)`.
- Escribe `agregar_al_final`, que inserte al final en vez de al principio. ¿Qué tiene que recorrer?
- Cambia la línea 29 por `printf("%d\n", segundo->valor);` y después por un acceso a
  `lista->valor` *antes* de la línea 28. ¿Cuál de los dos es un uso después de free?
