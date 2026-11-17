# Fichas de conceptos

Cada ficha explica la teoría de un tipo de evento. El panel Teoría muestra las fichas de los eventos
que ocurren en el paso actual, en cualquier programa.

## proceso · Qué es un proceso

Un **proceso** es un programa en ejecución. El sistema operativo le da un identificador único, el
**PID**, y un espacio de memoria propio que ningún otro proceso puede tocar directamente:

- **código** (las instrucciones del programa) y **datos globales**;
- **heap**: la memoria que se pide con `malloc` y vive hasta `free`;
- **stack**: un *frame* por cada función activa, con sus parámetros y variables locales. Al
  retornar la función, su frame desaparece.

Además tiene una **tabla de descriptores de archivo**: números pequeños que identifican lo que tiene
abierto. Todo proceso parte con 0 (entrada estándar), 1 (salida estándar) y 2 (salida de errores).

Un proceso puede estar **ejecutándose**, **listo** (puede correr pero el planificador eligió a otro)
o **bloqueado** (espera algo: datos, que termine un hijo, un mutex, que pase el tiempo). En este
visualizador los PID son virtuales, desde 1000, y en cada paso avanza una sola tarea.

## fork · fork(): un proceso se vuelve dos

`fork()` crea un proceso **hijo** que es una copia del que la llama (el **padre**):

- la misma memoria: código, globales, heap y stack, con los mismos valores;
- los mismos descriptores abiertos, que apuntan a los mismos archivos y pipes;
- el mismo punto de ejecución: los dos siguen desde la instrucción siguiente al `fork`.

La única diferencia es lo que devuelve: en el padre, el **PID del hijo**; en el hijo, **0**; y -1 si
no se pudo crear (por ejemplo, por el límite de procesos). Por eso el patrón es
`if (pid == 0) { /* hijo */ } else { /* padre */ }`.

Después del fork las memorias son **independientes**: si uno cambia una variable, el otro no lo ve.
Linux no copia todo de inmediato: usa *copy-on-write*, y una página se copia recién cuando alguno
de los dos escribe en ella. Por eso `fork` es barato.

Cuál de los dos corre primero lo decide el planificador: un programa correcto no debe depender de
ese orden. Ojo con `printf`: si había texto en su buffer sin vaciar, el hijo también lo hereda y
puede salir dos veces.

## pthread-create · pthread_create(): un hilo nuevo en el mismo proceso

Un **hilo** es otra línea de ejecución dentro del mismo proceso. Los hilos de un proceso
**comparten** el código, las variables globales, el heap y los descriptores; cada uno tiene **su
propio stack** (sus variables locales) y sus registros.

`pthread_create(&t, NULL, f, arg)` crea un hilo que empieza ejecutando `f(arg)` y guarda su
identificador en `t`. Devuelve 0 si funcionó o un código de error (no usa `errno`).

A diferencia de `fork`, no hay copia: si un hilo cambia una global, los demás ven el cambio. Eso
hace fácil compartir datos, pero obliga a **sincronizar** el acceso (mutex, variables de condición)
para evitar carreras.

El hilo termina al retornar de `f` o con `pthread_exit`. Si `main` retorna, el proceso termina y se
llevan a todos sus hilos: por eso `main` espera con `pthread_join`.

## exec · exec: el proceso cambia de programa

La familia `exec` (`execl`, `execlp`, `execv`, `execvp`, `execve`, …) **reemplaza** el programa del
proceso actual por otro: se descarta su memoria (código, datos, heap y stack) y se carga el nuevo
programa, que empieza desde su `main`. No se crea un proceso:

- el **PID** y el padre siguen siendo los mismos;
- los **descriptores abiertos se conservan** (salvo los marcados con `FD_CLOEXEC`): así un programa
  hereda la entrada y salida que se le preparó con `dup2`;
- las señales con handler vuelven a su acción por defecto (las ignoradas siguen ignoradas).

Si `exec` funciona, **no retorna**: el código que sigue ya no existe. Si retorna, falló (el programa
no existe, falta permiso) y hay que informar el error y terminar.

En el nombre, `l` significa argumentos en lista terminada en `NULL`, `v` un arreglo, y `p` que busca
el programa en `PATH`. `fork` seguido de `exec` es lo que hace un shell para ejecutar un comando.
Aquí el programa cargado no tiene información de depuración y se ve como una **caja negra**: solo
sus llamadas al sistema y su salida.

## fin-hilo · Un hilo termina

Un hilo termina al retornar de su función o al llamar a `pthread_exit(valor)`. Su stack desaparece,
pero su **valor de retorno** se guarda hasta que otro hilo lo recoja con `pthread_join`.

Nunca devuelvas un puntero a una variable local del hilo: esa memoria ya no existe cuando alguien
la lea. Usa memoria del heap o una global.

## exit · Un proceso termina

Un proceso termina al retornar de `main` (equivale a `exit(valor)`), al llamar a `exit` o `_exit`, o
al recibir una señal cuya acción es terminar. `exit` vacía los buffers de `stdio` y ejecuta lo
registrado con `atexit`; `_exit` no.

El kernel libera su memoria y **cierra todos sus descriptores**: si era el último escritor de un
pipe, el lector verá fin de archivo.

El **código de salida** (solo sus 8 bits bajos, 0 a 255) queda guardado para el padre. Por
convención 0 significa éxito. El kernel avisa al padre con la señal `SIGCHLD`.

## zombie · Zombie: terminó, pero nadie lo recogió

Cuando un proceso termina, el kernel no lo borra del todo: conserva su entrada en la tabla de
procesos (PID, código de salida) para que el padre la lea con `wait`. Mientras tanto el proceso es un
**zombie** (estado `Z` en `ps`).

Un zombie no usa memoria ni CPU, pero **ocupa un PID**. Un padre que crea muchos hijos y nunca hace
`wait` puede llenar la tabla de procesos. El zombie desaparece cuando el padre lo recoge; si el padre
muere antes, el hijo pasa a init, que lo recoge.

## wait · wait() y waitpid(): recoger a un hijo

`wait(&status)` **bloquea** al padre hasta que termine alguno de sus hijos; si ya hay uno zombie,
retorna de inmediato. Devuelve el PID del hijo recogido, o -1 si no tiene hijos.
`waitpid(pid, &status, opciones)` espera a un hijo en particular (`pid > 0`) o a cualquiera (`-1`).

`status` no es el código de salida: trae varios datos empaquetados. Se lee con macros:

- `WIFEXITED(status)`: terminó normalmente; `WEXITSTATUS(status)` da su código;
- `WIFSIGNALED(status)`: lo mató una señal; `WTERMSIG(status)` dice cuál.

Al recogerlo, el zombie desaparece. `wait` también sirve para **sincronizar**: después de él, el
padre sabe que el hijo terminó y que su salida ya se escribió.

## wnohang · WNOHANG: preguntar sin esperar

`waitpid(-1, &status, WNOHANG)` **no bloquea**: devuelve el PID de un hijo que ya terminó, 0 si hay
hijos pero ninguno terminó todavía, o -1 si ya no hay hijos.

Se usa para revisar sin detenerse, y sobre todo en un bucle dentro del handler de `SIGCHLD`: las
señales iguales no se acumulan, así que un solo aviso puede corresponder a varios hijos terminados.

## join · pthread_join(): esperar a un hilo

`pthread_join(t, &ret)` bloquea hasta que termina el hilo `t` y recibe su valor de retorno (un
`void *`). Además libera los recursos del hilo: es el equivalente de `wait` para hilos.

Sin `join`, `main` podría terminar mientras los hilos todavía trabajan, y con él terminaría todo el
proceso. Cada hilo debe recibir un solo `join` (o crearse *detached* si nadie lo va a esperar).

## huerfano · Huérfanos y la adopción por init

Si un padre termina antes que sus hijos, los hijos quedan **huérfanos**. El kernel les asigna un
padre nuevo: **init** (PID 1) o el proceso marcado como *subreaper* más cercano. Desde ese momento
`getppid()` en el hijo devuelve 1.

init hace `wait` por los huérfanos cuando terminan, así no quedan zombies para siempre. En este
visualizador, init (1) se dibuja como un nodo virtual.

## pipe · pipe(): un canal entre procesos

`pipe(fd)` crea en el kernel un canal **unidireccional** con dos descriptores: `fd[0]` para **leer**
y `fd[1]` para **escribir**. Lo que se escribe en `fd[1]` se lee por `fd[0]` en el mismo orden
(FIFO), como un flujo de bytes: no se conservan los límites entre un `write` y otro.

El pipe tiene un buffer de capacidad limitada (64 KiB por defecto en Linux). Para comunicar procesos,
el pipe se crea **antes** del `fork`: el hijo hereda los dos descriptores y después cada uno cierra
el extremo que no usa.

Reglas que hay que saber de memoria:

- `read` bloquea si el pipe está vacío y queda algún escritor;
- `read` devuelve 0 (fin de archivo) si está vacío y **ya no hay escritores**;
- `write` bloquea si el pipe está lleno;
- `write` sin ningún lector produce `SIGPIPE`.

## dup2 · dup2(): redirigir un descriptor

`dup2(viejo, nuevo)` hace que el descriptor `nuevo` apunte a lo mismo que `viejo`; si `nuevo`
estaba abierto, primero se cierra. Desde ahí los dos números se refieren al mismo archivo o pipe.

El uso típico es antes de un `exec`: `dup2(fd[1], 1)` hace que todo lo que el programa escriba en su
salida estándar (fd 1) vaya al pipe, sin que el programa lo sepa. Así implementa el shell `>`, `<`
y `|`.

Después hay que **cerrar el descriptor original**: ya está duplicado en 1, y dejarlo abierto suma un
extremo de más (un escritor de más impide que el lector vea fin de archivo).

## close-pipe · Cerrar un extremo de un pipe

Cada descriptor abierto hacia un pipe cuenta, en cualquier proceso. El lector ve fin de archivo solo
cuando se cerraron **todos** los descriptores de escritura de ese pipe, en todos los procesos; y
`write` falla con `SIGPIPE` solo cuando se cerraron todos los de lectura.

Por eso, después de un `fork`, **cada proceso cierra los extremos que no usa**. El olvido más común
es que el lector deje abierto su propio descriptor de escritura: queda esperando un fin de archivo
que nunca llega.

## close · close(): soltar un descriptor

`close(fd)` libera ese número en la tabla del proceso. El archivo o pipe se libera de verdad cuando
ya ningún descriptor, de ningún proceso, lo usa. El número queda libre y lo reutiliza la próxima
apertura: el kernel siempre entrega el **menor número libre**.

## stdin · La entrada estándar

La entrada estándar es el descriptor 0. Leer de ella (`scanf`, `fgets`, `read`) **bloquea** hasta
que haya datos: en una terminal, hasta que el usuario presiona Enter. El fin de la entrada (EOF) se
envía con Ctrl+D en una terminal, o llega solo si la entrada viene de un archivo.

`scanf` devuelve cuántos valores logró convertir: 1 si leyó el número pedido, 0 si lo que venía no
era un número (y ese texto queda sin consumir), o `EOF` al terminar la entrada. Por eso el bucle
correcto es `while (scanf("%d", &n) == 1)`.

En el visualizador la entrada se escribe antes de ejecutar; si se acaba sin EOF, la reproducción se
detiene y puedes escribir más o enviar EOF.

## eof · Fin de archivo: read devuelve 0

`read` devuelve cuántos bytes leyó (pueden ser menos de los pedidos), **0 en fin de archivo** y -1 si
hubo un error. En un pipe, 0 significa que está vacío **y** que ya nadie puede escribir en él.

Es la condición que termina el bucle de lectura: `while ((n = read(fd, buf, tam)) > 0)`.

## read-pipe · Leer de un pipe

`read(fd, buf, max)` copia hasta `max` bytes de los que hay en el pipe y los retira de él. Si hay
menos, devuelve los que hay: no espera a completar `max`. Por eso se lee en un bucle hasta recibir 0.

`read` trabaja con bytes y no agrega `'\0'`: para usar lo leído como string hay que terminarlo a mano
(`buf[n] = '\0'`), dejando un espacio libre en el buffer para ese byte.

## sigpipe · SIGPIPE: escribir sin lectores

Escribir en un pipe cuyos extremos de lectura están todos cerrados genera la señal `SIGPIPE`, cuya
acción por defecto **termina el proceso**. Si `SIGPIPE` se ignora o tiene handler, `write` falla con
`errno = EPIPE`. Así termina, por ejemplo, el primer programa de `yes | head`.

## salida-estandar · printf y la salida estándar

`printf` no escribe directamente: guarda el texto en un **buffer** dentro del proceso, y la escritura
real (`write(1, …)`) ocurre después. Hacia una terminal el buffer se vacía con cada `'\n'`; hacia un
pipe o un archivo, recién cuando se llena, con `fflush` o al terminar el proceso con `exit`.
`stderr` no usa buffer.

Esto explica dos sorpresas típicas: el orden en que aparecen los mensajes de varios procesos depende
de cuándo vacía cada uno su buffer, y un `fork` con texto pendiente en el buffer lo duplica. Aquí se
ve el momento del `write` real; el programa escribe como si su salida fuera una terminal.

## write-pipe · Escribir en un pipe

`write(fd, datos, n)` copia los bytes al buffer del pipe y, si hay espacio, retorna de inmediato: **no
espera a que alguien los lea**. Si el pipe está lleno, bloquea hasta que el lector saque datos.

Las escrituras de hasta `PIPE_BUF` bytes (4096 en Linux) son atómicas: no se mezclan con las de
otros escritores del mismo pipe.

## bloqueo-io · Bloqueado esperando datos o espacio

Un proceso bloqueado **no usa CPU**: el kernel lo saca de la cola de listos hasta que ocurre lo que
espera (llegan datos, se libera espacio, se cierra el último escritor). Así se sincronizan los
procesos que comparten un pipe: el lector avanza al ritmo del escritor, sin esperas activas.

## mutex · Mutex: exclusión mutua

Un **mutex** protege una **sección crítica**: el código que accede a datos compartidos y que no
puede ejecutar más de un hilo a la vez.

- `pthread_mutex_lock(&m)` lo toma; si otro hilo lo tiene, el hilo **se bloquea** hasta que se
  libere.
- `pthread_mutex_unlock(&m)` lo suelta; solo debe hacerlo el hilo que lo tomó.

Todas las lecturas y escrituras del dato compartido deben hacerse con el mutex tomado: protegerlo
solo en algunos lugares no sirve. Conviene que la sección crítica sea corta, y nunca hay que olvidar
el `unlock` (también en los caminos de error), o el siguiente que lo pida esperará para siempre.

## cond · Variables de condición

Una variable de condición permite **esperar a que se cumpla una condición** sobre datos compartidos
sin gastar CPU. Siempre se usa junto a un mutex:

- `pthread_cond_wait(&c, &m)` suelta el mutex y duerme, **en un solo paso**; al despertar vuelve a
  tomar el mutex antes de retornar;
- `pthread_cond_signal(&c)` despierta a uno de los hilos que esperan; `pthread_cond_broadcast`, a
  todos.

El patrón obligatorio es `while (!condicion) pthread_cond_wait(&c, &m);`, con `while` y no `if`:
otro hilo puede cambiar la condición antes de que el despertado vuelva a correr, y existen
despertares espurios. Un `signal` sin nadie esperando se pierde: por eso la condición se guarda en
variables y se revisa antes de esperar.

## sem · Semáforos

Un semáforo es un contador que nunca baja de cero. `sem_wait` lo decrementa; si está en 0, el hilo se
bloquea hasta que otro haga `sem_post`, que lo incrementa y despierta a uno que espera.

Con valor inicial 1 se comporta como un mutex; con N limita a N hilos a la vez; con 0 sirve para
avisar que algo ocurrió. A diferencia de un mutex, no tiene dueño: cualquiera puede hacer `post`.

## sleep · sleep(): dormir

`sleep(s)`, `usleep(us)` y `nanosleep` bloquean al proceso al menos ese tiempo, sin usar CPU. Si
llega una señal con handler, el sueño se interrumpe antes (`sleep` devuelve los segundos que
faltaban).

`sleep` **no sincroniza**: «esperar un rato a que el otro termine» no garantiza nada, solo hace menos
probable el problema. En el visualizador el tiempo es virtual: cada paso suma 1 ms y, si todos
esperan, el reloj salta al próximo despertar.

## pause · pause(): esperar una señal

`pause()` suspende al proceso hasta que llegue una señal que ejecute un handler (o que lo termine).
Retorna -1 con `errno = EINTR` después del handler.

Tiene una carrera clásica: si la señal llega entre que el programa revisa su bandera y llama a
`pause`, la espera no termina nunca. `sigsuspend`, con la señal bloqueada de antemano, cambia la
máscara y espera en un solo paso, sin esa ventana.

## kill · kill(): enviar una señal

`kill(pid, sig)` envía la señal `sig` al proceso `pid` (con `pid` 0, a todo el grupo de procesos del
que la envía). Hace falta permiso: normalmente, ser el mismo usuario.

`kill` no espera a que la señal se atienda: la señal queda **pendiente** en el destino y se entrega
cuando ese proceso vuelve a correr y no la tiene bloqueada. Varias señales iguales pendientes cuentan
como una sola: no se acumulan.

## sigchld · SIGCHLD: un hijo terminó

El kernel envía `SIGCHLD` al padre cuando uno de sus hijos termina (o se detiene). Su acción por
defecto es ignorarla, y el hijo queda zombie hasta que el padre haga `wait`.

Con un handler, el padre puede recoger a sus hijos sin quedarse bloqueado en `wait`. Como las señales
no se acumulan, un solo `SIGCHLD` puede representar a varios hijos: el handler debe recoger en un
bucle, `while (waitpid(-1, &st, WNOHANG) > 0)`.

## senales · Señales

Una **señal** es un aviso asíncrono que el kernel entrega a un proceso. Puede venir del hardware
(`SIGSEGV` al acceder mal a la memoria), del kernel (`SIGCHLD`, `SIGPIPE`), de la terminal
(`SIGINT` con Ctrl+C) o de otro proceso (`kill`).

Cada señal tiene una **acción**: la por defecto (terminar, terminar con *core*, ignorar, detener o
continuar), ignorarla (`SIG_IGN`) o ejecutar un **handler**. Un proceso puede **bloquear** señales
con su máscara (`sigprocmask`): quedan pendientes hasta que las desbloquee. `SIGKILL` y `SIGSTOP` no
se pueden manejar, ignorar ni bloquear.

## alarm · alarm(): una señal programada

`alarm(s)` pide al kernel enviar `SIGALRM` al proceso dentro de `s` segundos. Hay una sola alarma por
proceso: una nueva reemplaza a la anterior, y `alarm(0)` la cancela. Por defecto `SIGALRM` termina el
proceso; con un handler sirve para poner plazos a una operación o hacer algo periódicamente.

## ctrl-c · Ctrl+C: SIGINT desde la terminal

Al presionar Ctrl+C, la terminal envía `SIGINT` a **todos los procesos del grupo en primer plano**, no
solo a uno. La acción por defecto los termina; con un handler, el programa puede terminar
ordenadamente (guardar, liberar) o ignorar la interrupción.

En el visualizador, el botón Ctrl+C vuelve a ejecutar el programa enviando `SIGINT` después del paso
actual.

## handler · Handler de señal

Cuando llega una señal con handler, el kernel **interrumpe** al proceso donde esté (entre dos
instrucciones cualesquiera), guarda su estado y ejecuta el handler. Al terminar, el estado se
restaura y el programa sigue como si nada. Mientras el handler corre, esa misma señal queda
bloqueada.

Reglas para escribir handlers:

- hacer poco: normalmente, solo marcar una variable global `volatile sig_atomic_t` que el programa
  revisa después;
- llamar solo funciones *async-signal-safe*: `write` sí; `printf` y `malloc` no, porque la señal pudo
  interrumpirlas a la mitad (los ejemplos usan `printf` para que se vea, pero en un programa real se
  evita);
- una llamada bloqueante interrumpida (`read`, `sleep`, `pause`) puede retornar con `EINTR`.

Para instalar handlers conviene `sigaction`, cuyo comportamiento es el mismo en todos los sistemas.

## accion-por-defecto · La acción por defecto de una señal

Si una señal no tiene handler ni está ignorada, se aplica su acción por defecto:

- **terminar**: `SIGINT`, `SIGTERM`, `SIGKILL`, `SIGUSR1`, `SIGUSR2`, `SIGPIPE`, `SIGALRM`;
- **terminar con core** (volcado de memoria para depurar): `SIGSEGV`, `SIGABRT`, `SIGFPE`, `SIGQUIT`;
- **ignorar**: `SIGCHLD`, `SIGWINCH`, `SIGURG`;
- **detener** y **continuar**: `SIGSTOP`, `SIGTSTP` y `SIGCONT`.

Un proceso terminado por una señal no vuelve de `main`: su padre lo ve con `WIFSIGNALED` y
`WTERMSIG` (en el shell, el código aparece como 128 + número de la señal).

## malloc · malloc(): memoria en el heap

`malloc(n)` reserva `n` bytes en el **heap** y devuelve un puntero al bloque, o `NULL` si no hay
memoria (hay que revisarlo). La memoria **no se inicializa**: tiene lo que hubiera antes (`calloc`, en
cambio, la llena de ceros).

Un bloque vive hasta que se libera con `free`, sin importar qué función lo pidió: por eso sirve para
datos que deben sobrevivir al retorno de una función, como los nodos de una lista. Para el tamaño
conviene `malloc(sizeof *p)`, que toma el tamaño del tipo apuntado. El visualizador marca los bytes
nunca escritos como sin inicializar.

## free · free(): devolver la memoria

`free(p)` devuelve el bloque al asignador. El puntero `p` conserva la misma dirección, pero ya **no
se puede usar**: es un *puntero colgante*. Cada `malloc` debe tener exactamente un `free`.

La memoria que nunca se libera es una **fuga** (*leak*): en un programa que corre mucho tiempo, el
consumo crece sin parar. Al terminar el proceso, el sistema recupera toda su memoria; aun así las
fugas indican un error de diseño. Después de `free(p)` conviene escribir `p = NULL`.

## free-invalido · free inválido

Liberar dos veces el mismo bloque (*double free*) o liberar algo que no vino de `malloc` (una
variable local, un puntero al medio de un bloque) daña las estructuras internas del asignador. El
programa puede abortar en ese momento (`free(): double free detected`) o fallar mucho después, en
otro lugar. Herramientas como `valgrind` o `-fsanitize=address` encuentran estos errores.

## uso-tras-free · Uso después de free

Leer o escribir un bloque ya liberado es **comportamiento indefinido**: a veces «funciona» porque los
bytes siguen ahí, a veces aparece basura porque el asignador reutilizó esa memoria para sus datos o
para otro `malloc`. Es uno de los errores más peligrosos (está detrás de muchas vulnerabilidades).
Después de `free`, no uses el puntero y actualiza los demás punteros que apuntaban a ese bloque.

## segfault · Segmentation fault

Acceder a una dirección inválida (`NULL`, memoria no asignada al proceso, escribir en memoria de solo
lectura) provoca una falla que el kernel convierte en `SIGSEGV`; su acción por defecto termina el
proceso con *core*. Causas típicas: desreferenciar `NULL`, salirse de un arreglo, usar punteros sin
inicializar o colgantes, y una recursión infinita que agota el stack.

## pipe-sin-cerrar · El pipe que nunca da fin de archivo

Si un proceso lee de un pipe pero conserva abierto un descriptor de **escritura** del mismo pipe
(porque no lo cerró después del `fork`), el kernel ve que todavía hay un escritor posible: `read`
nunca devuelve 0 y el lector espera para siempre, aunque nadie más vaya a escribir. La regla:
**cada proceso cierra los extremos que no usa**.

## deadlock · Deadlock (interbloqueo)

Un **deadlock** es un grupo de tareas en el que cada una espera algo que tiene otra del grupo: ninguna
puede avanzar, nunca. Ocurre cuando se cumplen a la vez las cuatro condiciones de Coffman:

- **exclusión mutua**: el recurso lo usa uno a la vez;
- **retener y esperar**: se pide un recurso sin soltar los que ya se tienen;
- **sin expropiación**: nadie puede quitarle un recurso a otro;
- **espera circular**: A espera a B, que espera a A.

La forma práctica de evitarlo es romper la espera circular: que todos tomen los mutex en el **mismo
orden**. Otra opción es `pthread_mutex_trylock` y soltar todo si falla. El visualizador detecta el
deadlock cuando todas las tareas vivas están bloqueadas y nada puede despertarlas.

## limites · Límites de recursos

Cada usuario y proceso tiene límites (`ulimit`, `setrlimit`): cuántos procesos puede tener, cuánta
memoria, tiempo de CPU y archivos abiertos. Cuando `fork` no puede crear otro proceso, devuelve -1 con
`errno = EAGAIN`: por eso hay que revisar su resultado.

El visualizador también tiene límites (procesos, pasos y tiempo): al llegar a uno, la traza se corta y
se puede ver todo lo que alcanzó a ejecutarse.
