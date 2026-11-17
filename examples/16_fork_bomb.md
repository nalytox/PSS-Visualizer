# Una fork bomb (con límite)

> **Objetivo:** ver qué pasa cuando un programa hace `fork` sin control, entender por qué la cantidad
> de procesos crece de forma exponencial y por qué los sistemas limitan cuántos procesos puede tener
> un usuario.

Este programa hace `fork` dentro de un bucle infinito. Cada proceso nuevo ejecuta el mismo bucle, así
que también hace `fork` sin parar. Es una **fork bomb**: un ataque de denegación de servicio muy
simple, porque llena la tabla de procesos y agota la memoria, y la máquina deja de responder.

La cantidad de procesos se **duplica** en cada ronda en la que todos alcanzan a hacer un `fork`: 1,
2, 4, 8, 16, 32… En unas pocas decenas de rondas habría millones.

Lo que la detiene son los **límites de recursos**. En Linux, `ulimit -u` (o `RLIMIT_NPROC`) fija
cuántos procesos puede tener un usuario; al alcanzarlo, `fork` falla, devuelve -1 y `errno` vale
`EAGAIN`. Este programa no revisa el resultado, así que seguiría intentando para siempre, gastando
CPU.

El visualizador tiene su propio límite: **32 procesos**. Cuando un `fork` lo superaría, la traza se
corta y puedes revisar todo lo que alcanzó a ocurrir.

## Línea 5 · `fork();`

Cada paso con el evento de fork agrega un proceso al árbol. Observa cómo cambia la forma del árbol:
los primeros procesos tienen muchos hijos (uno por cada vuelta que alcanzaron a dar) y los últimos
ninguno. Los pasos sin evento son el primer paso de un hijo recién creado, que termina esta línea
en su copia.

Al llegar a 32 procesos, el siguiente `fork` no se ejecuta y la traza termina con la razón del corte.

## Al terminar

- Un `fork` en un bucle sin condición crece de forma exponencial.
- Los límites de recursos (procesos por usuario, memoria, CPU) protegen al sistema de programas así.
- Siempre hay que revisar el resultado de `fork`: -1 significa que no se creó el proceso.

## Para practicar

- Cambia el bucle por `for (int i = 0; i < 4; i++)`. ¿Cuántos procesos habrá? (Compara con el
  ejemplo 04.)
- Reemplaza `fork();` por `if (fork() == -1) { perror("fork"); break; }`. En un sistema real, ¿qué
  imprimiría al alcanzar el límite?
- ¿Por qué un `ulimit -u` bajo protege a los demás usuarios de la máquina, pero no a la sesión del
  propio usuario que lanzó la bomba?
