# Leer números de la entrada estándar

> **Objetivo:** entender cómo un programa lee de la entrada estándar, por qué la lectura bloquea, qué
> devuelve `scanf` y cómo se detecta el fin de la entrada (EOF).

Todo proceso nace con tres descriptores abiertos: 0 (**entrada estándar**), 1 (salida estándar) y 2
(salida de errores). Leer del descriptor 0 significa leer lo que el usuario escribe en la terminal,
o lo que viene de un archivo o un pipe si se redirigió (`./prog < datos.txt`, `cat datos | ./prog`).
El programa no sabe ni le importa cuál de esos es.

Leer de la entrada **bloquea**: si no hay datos, el proceso se duerme hasta que lleguen. En una
terminal, los datos llegan cuando el usuario presiona Enter. La entrada termina con **EOF** (*end of
file*): en una terminal se envía con Ctrl+D; si la entrada es un archivo, llega sola al final.

`scanf` no lee del kernel byte a byte: la biblioteca estándar hace un `read` grande, guarda los
bytes en un **buffer** dentro del proceso, y cada `scanf` toma de ahí lo que necesita. Solo cuando el
buffer se vacía vuelve a llamar a `read`.

Este ejemplo trae la entrada `3 4` (Enter) `5` (Enter) precargada y abierta, sin EOF: cuando el
programa la consuma toda y pida más, la reproducción se detiene y podrás escribir más números o
enviar EOF desde el panel de entrada.

## Línea 4 · `int n, suma = 0, cuantos = 0;`

Se declaran tres enteros. `n` no se inicializa: tiene basura hasta que `scanf` escriba en él. `suma`
y `cuantos` empiezan en 0.

## Línea 5 · `printf("Escribe números (EOF para terminar):\n");`

Se escribe el mensaje en la salida estándar (descriptor 1). Como el texto no tiene formato, el
compilador cambió este `printf` por `puts`, que hace lo mismo más rápido.

## Línea 6 · `while (scanf("%d", &n) == 1) {`

`scanf("%d", &n)` intenta leer un entero y guardarlo en `n`; por eso recibe `&n`, la dirección de
`n` (si recibiera `n` solo tendría una copia). Devuelve cuántas conversiones logró:

- **1**: leyó un número, y el bucle sigue;
- **0**: lo que venía no era un número (por ejemplo, una letra), y esa letra queda sin consumir;
- **EOF** (-1): se acabó la entrada.

La primera vez, el buffer está vacío y `scanf` llama a `read(0, …)`: llegan de una vez los 6 bytes
`"3 4\n5\n"`. Las dos vueltas siguientes toman los números del buffer **sin llamar al kernel**.
En la cuarta, el buffer está vacío otra vez y el `read` bloquea: no hay más datos y tampoco EOF.
Ahí la reproducción se detiene y te pide más entrada.

Si envías EOF, `read` devuelve 0, `scanf` devuelve EOF y el bucle termina. Si escribes una letra,
`scanf` devuelve 0 y el bucle también termina (la letra queda en el buffer).

## Línea 7 · `suma += n;`

Se acumula el número leído.

## Línea 8 · `cuantos++;`

Se cuenta. Después, el bucle vuelve a la línea 6 para leer el siguiente.

## Línea 10 · `printf("Leí %d números; la suma es %d\n", cuantos, suma);`

Con la entrada de ejemplo y EOF, imprime `Leí 3 números; la suma es 12`. Para llegar hasta aquí hay
que enviar EOF (o escribir algo que no sea un número) cuando el programa pida más.

## Al terminar

- La entrada estándar es el descriptor 0; leerla bloquea hasta que hay datos o EOF.
- `scanf` devuelve la cantidad de valores convertidos: el bucle correcto compara con 1, no con
  «distinto de 0» (EOF es -1, que también es «verdadero»).
- La biblioteca estándar lee por bloques y guarda un buffer: varios `scanf` pueden salir de un solo
  `read`.
- EOF no es un carácter que se lee: es la condición de `read` devolviendo 0.

## Para practicar

- Ejecuta el ejemplo, escribe `10 20` cuando lo pida y luego envía EOF. ¿Cuántos `read` hubo?
- Escribe `7 x 8` como entrada. ¿Qué devuelve `scanf` en la segunda vuelta y por qué el 8 no se suma?
- Cambia la condición por `while (scanf("%d", &n))` y envía EOF. ¿Qué pasa y por qué?
