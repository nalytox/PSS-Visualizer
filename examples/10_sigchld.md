# SIGCHLD: recoger hijos sin quedarse esperando

> **Objetivo:** usar la señal `SIGCHLD`, que el kernel envía al padre cuando un hijo termina, para
> recogerlo con `waitpid(…, WNOHANG)` desde un handler, sin bloquear al padre en `wait`.

`wait` tiene un problema: bloquea. Un servidor que atiende pedidos no puede quedarse detenido
esperando a que terminen sus hijos; pero si nunca los recoge, se le acumulan zombies.

La solución es que el kernel **avise**. Cada vez que un hijo termina, el kernel envía `SIGCHLD` a su
padre. Por defecto esa señal se ignora (el hijo queda zombie igual), pero con un handler el padre
puede recoger al hijo en el momento justo, sin dejar de hacer su trabajo.

Dentro del handler se usa `waitpid(-1, &status, WNOHANG)`:

- `-1` significa «cualquier hijo»;
- `WNOHANG` significa «no bloquear»: si ningún hijo terminó, devuelve 0 en vez de esperar.

Y se usa en un **bucle**, porque las señales no se acumulan: si dos hijos terminan casi a la vez,
puede llegar un solo `SIGCHLD` por los dos.

## Línea 14 · `signal(SIGCHLD, al_terminar_un_hijo);`

Se instala el handler para `SIGCHLD` con `signal`, la forma antigua y más corta. En Linux con glibc
equivale a un `sigaction` con la opción `SA_RESTART`; para código portable se prefiere `sigaction`.

## Línea 15 · `if (fork() == 0)`

Se crea el hijo. Hereda el handler, aunque a él no le sirve: no tiene hijos.

## Línea 16 · `return 7;`

El hijo termina enseguida con código 7. En el mismo instante el kernel lo deja zombie y envía
`SIGCHLD` al padre: fíjate en la señal que viaja desde el kernel hacia el padre en el lienzo.

## Línea 17 · `printf("padre: sigo trabajando\n");`

El padre continúa con lo suyo, sin `wait`.

## Línea 18 · `sleep(2);`

El padre se duerme dos segundos, simulando trabajo. Esta línea aparece varias veces:

- al bloquearse en `sleep`;
- cuando llega `SIGCHLD`: el kernel lo despierta y, en este mismo punto, le entrega la señal; su
  siguiente paso ya está dentro del handler;
- al volver del handler. Aquí hay una sorpresa: `sleep` **no se reanuda**. Una señal con handler
  interrumpe el sueño, y `sleep` retorna antes, devolviendo los segundos que faltaban. Por eso
  «padre: termino» sale mucho antes de los 2 segundos. Mira el reloj de la traza.

## Línea 9 · `while ((hijo = waitpid(-1, &status, WNOHANG)) > 0)`

Dentro del handler. La primera vuelta recoge al hijo zombie: `waitpid` devuelve su PID y el zombie
desaparece del lienzo. En la segunda vuelta no quedan hijos y `waitpid` devuelve -1 (si quedaran
hijos vivos, devolvería 0). En ambos casos el bucle termina sin bloquear.

## Línea 10 · `printf("padre: recogí a %d, salió con %d\n", hijo, WEXITSTATUS(status));`

Imprime el PID del hijo recogido y su código de salida, 7. (Recuerda que `printf` en un handler no es
seguro en programas reales; aquí se usa para verlo.)

## Línea 11 · `}`

El handler termina y el padre vuelve a donde estaba: el `sleep` de la línea 18, que retorna antes de
tiempo porque fue interrumpido.

## Línea 19 · `printf("padre: termino\n");`

El padre termina su trabajo. No dejó zombies, y nunca se bloqueó en `wait`.

## Línea 20 · `return 0;`

El padre sale con código 0.

## Línea 21 · `}`

Fin de `main`: el hijo, que salió por el `return 7` sin entrar al resto, también pasa por aquí.

## Al terminar

- El kernel envía `SIGCHLD` al padre cuando un hijo termina.
- Un handler de `SIGCHLD` con `waitpid(-1, &st, WNOHANG)` en bucle recoge a todos los hijos
  terminados sin bloquear.
- `WNOHANG` devuelve 0 si hay hijos pero ninguno terminó, y -1 si no hay hijos.
- Una señal con handler interrumpe `sleep`, que retorna antes de tiempo.

## Para practicar

- Cambia el `while` del handler por un `if`. Con un solo hijo funciona igual: ¿qué podría fallar
  con muchos hijos que terminan juntos?
- Crea tres hijos con `return` distintos. ¿Cuántas veces se ejecuta el handler y cuántas vueltas da
  su bucle? Prueba distintas planificaciones.
- Cambia `sleep(2);` por `while (sleep(2) > 0);`. ¿Qué cambia y por qué?
