# Structs y punteros

> **Objetivo:** entender cómo se guarda un `struct` en memoria, cuándo se usa `.` y cuándo `->`,
> y por qué una función solo puede cambiar los datos de quien la llama si recibe un puntero.

Un `struct` agrupa variables relacionadas bajo un solo tipo. Sus **campos** quedan uno tras otro en
memoria, en el orden en que se declaran (el compilador puede agregar bytes de relleno para que cada
campo quede alineado). Declarar `struct punto { int x; int y; };` no reserva memoria: solo describe
la forma. La memoria aparece al declarar una variable de ese tipo.

Un struct puede contener otro: `struct rect` tiene dentro un `struct punto` completo (no un
puntero a uno), además de dos enteros y un arreglo de 12 caracteres. Todo eso forma un único bloque.

Hay dos formas de llegar a un campo:

- con la variable: `r.ancho`, `pts[1].x`;
- con un puntero a la variable: `pr->ancho`, que es una forma corta de escribir `(*pr).ancho`.

La idea más importante del ejemplo: en C **todos los argumentos se pasan por valor**. Una función
recibe copias; si cambia la copia, el original no se entera. Para que `mover` cambie un punto de
`main`, `main` le pasa la **dirección** del punto, y `mover` escribe en esa dirección.

Todas las variables de `main` son locales: viven en el **stack**, dentro del *frame* de `main`. En la
memoria del proceso puedes ver cada una con su dirección, su tamaño y su valor, y las flechas de los
punteros hacia lo que apuntan.

## Línea 21 · `struct rect r = {{1, 2}, 10, 5, "caja"};`

Se crea `r` en el frame de `main` y se inicializa campo por campo, en orden: la `esquina` recibe
`{1, 2}`, `ancho` 10, `alto` 5 y `nombre` el texto `"caja"`. Como el arreglo tiene 12 bytes y
`"caja"` ocupa 5 (4 letras más el `'\0'` que termina todo string), los 7 restantes quedan en 0: en
una inicialización con llaves, lo que no se nombra se llena con ceros.

Mira en la memoria cómo `r` es un solo bloque con todos sus campos contiguos.

## Línea 22 · `struct punto pts[3] = {{0, 0}, {3, 4}, {6, 8}};`

Un arreglo de tres structs: tres puntos seguidos en memoria, cada uno de 8 bytes (dos `int` de 4).
`pts[0]` empieza en la dirección de `pts`, `pts[1]` 8 bytes después y `pts[2]` 16 bytes después. El
nombre del arreglo, usado en una expresión, se convierte en la dirección de su primer elemento.

## Línea 23 · `struct punto *pp = &pts[1];`

`&` obtiene la **dirección** de algo. `pp` es un puntero: una variable de 8 bytes cuyo valor es la
dirección de `pts[1]`. No se copia el punto; `pp` solo dice dónde está. En la memoria, `pp` aparece
con una flecha hacia `pts[1]`.

## Línea 24 · `struct rect *pr = &r;`

Otro puntero, ahora al rectángulo completo. Con él, `pr->ancho` lee el campo `ancho` de `r`, y
`&pr->esquina` es la dirección del punto que está **dentro** de `r` (el operador `->` se aplica antes
que `&`).

## Línea 25 · `mover(pp, 1, 1);`

Se llama a `mover` con una copia de `pp` (la dirección de `pts[1]`) y las copias de 1 y 1. Al entrar,
se crea un frame nuevo en el stack, encima del de `main`, con los parámetros `p`, `dx` y `dy`. `p`
apunta al mismo lugar que `pp`.

## Línea 16 · `p->x += dx;`

`p->x` es el campo `x` del punto al que apunta `p`. Como `p` tiene la dirección de un punto que
vive en el frame de `main`, esta escritura **cambia la variable de `main`**, no una copia. Si
`mover` hubiera recibido un `struct punto` por valor, cambiaría solo su copia local y `main` no vería
nada.

## Línea 17 · `p->y += dy;`

Lo mismo con el campo `y`. Observa que el frame de `mover` no cambia: lo que cambia está en el frame
de `main`, a donde apunta la flecha de `p`.

## Línea 18 · `}`

`mover` termina: su frame desaparece del stack y sus parámetros (`p`, `dx`, `dy`) dejan de existir.
Los cambios que hizo a través del puntero quedan, porque se escribieron en la memoria de `main`.

## Línea 26 · `mover(&pr->esquina, -1, 0);`

Ahora se pasa la dirección de la `esquina` que está dentro de `r`. A `mover` no le importa que ese
punto sea parte de un rectángulo: recibe la dirección de un `struct punto` y trabaja con ella. Así
se reutiliza una función sobre cualquier punto, esté donde esté.

## Línea 27 · `printf("pts[1] = (%d, %d)\n", pts[1].x, pts[1].y);`

`pts[1]` era `(3, 4)` y `mover` le sumó 1 a cada coordenada a través de `pp`: se imprime `(4, 5)`.
Aquí `printf` hace la escritura real a la salida estándar (un `write` al descriptor 1).

## Línea 28 · `printf("esquina = (%d, %d)\n", r.esquina.x, r.esquina.y);`

La esquina era `(1, 2)` y se movió `(-1, 0)`: queda en `(0, 2)`. `r.esquina.x` encadena dos accesos
con punto: el campo `esquina` de `r` y, dentro de él, el campo `x`.

## Línea 29 · `return 0;`

`main` devuelve 0: el código de salida del proceso, que por convención significa «todo bien».

## Línea 30 · `}`

Al cerrar `main`, el frame y todas sus variables locales desaparecen y el proceso termina con
`exit(0)`.

## Al terminar

- Un struct es un bloque de memoria con campos contiguos; un struct anidado vive dentro del otro.
- `.` se usa con la variable y `->` con un puntero: `p->x` es `(*p).x`.
- Los argumentos se pasan por valor. Para que una función modifique algo de quien la llama, recibe
  su dirección.
- Las variables locales viven en el frame de su función y desaparecen al retornar.

## Para practicar

- Cambia `mover` para que reciba `struct punto p` (sin puntero) y use `p.x += dx`. ¿Qué imprime el
  programa? ¿Por qué?
- Escribe `void agrandar(struct rect *r, int k)` que multiplique `ancho` y `alto` por `k`, y llámala
  con `pr`.
- ¿Cuántos bytes ocupa `struct rect`? Calcúlalo y compruébalo con `printf("%zu\n", sizeof r);`.
