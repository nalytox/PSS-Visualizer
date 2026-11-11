# Fase 8: ejecuciones largas (hasta un millón de pasos)

## Qué se hizo

- **Traza compacta (versión 2)** (`schema/compact.schema.json`, `crates/trace-model/src/compact.rs`).
  La versión 1 guarda en cada paso el estado completo y una instantánea de memoria casi por paso:
  4.388 pasos ocupaban 28 MB. La versión 2 tiene el mismo contenido, pero:
  - los pasos van en bloques de hasta 1.000, comprimidos con gzip. Cada bloque abre con un paso
    completo y sigue con parches respecto del anterior (`crates/trace-model/src/delta.rs`);
  - las instantáneas van en el bloque que las usa, completas o como parche de la anterior del mismo
    proceso. Cada bloque se decodifica por su cuenta;
  - un índice por paso (actor, línea donde quedó y banderas de los eventos) permite dibujar la línea
    de tiempo y navegar sin abrir bloques.

  Resultado: unos 25 bytes por paso, 25 MB para un millón. La versión 1 sigue siendo válida: las
  trazas grabadas, las sintéticas y las de referencia siguen en ese formato.
- **Tracer en streaming.** El motor entrega cada paso a un `Encoder` y solo guarda las instantáneas
  que algún proceso usa en ese momento: la memoria del tracer no depende de la cantidad de pasos
  (20 MB con 200.000). Por defecto el CLI escribe la versión 2; `--format v1` escribe la completa.
- **Rendimiento del tracer.**
  - La memoria se lee de a páginas: con un arreglo de 100 enteros, de 139 lecturas por paso a 4.
  - La deduplicación ya no serializa cada instantánea a JSON: se compara con la anterior del
    mismo proceso.
  - Los parches de memoria se calculan sobre los tipos y solo convierten lo que cambió.
  - La planificación ya no recorre todos los pasos anteriores, lo que hacía cuadrático el total.
  - Los bloques liberados del heap se olvidan cuando dejan de dibujarse; un doble free se sigue
    detectando por la dirección.
- **Avance y Detener.**
  - El servidor tiene `POST /api/run/stream`, que responde por líneas (NDJSON): el id, el avance
    cada 250 ms y al final la traza.
  - `POST /api/run/{id}/stop` envía SIGUSR1 al tracer, que termina el paso y entrega lo recorrido,
    con el motivo de corte `stopped`.
  - La interfaz muestra "Ejecutando: N pasos" con un botón Detener.
- **Interfaz perezosa** (`web/src/trace/loaded.ts`).
  - `LoadedTrace` envuelve las dos versiones: `step(t)`, `snapshot(id)` y las columnas del índice.
  - De la versión 2 solo descomprime los bloques que se miran, con `fflate`, y guarda los últimos 8.
  - La línea de tiempo, el índice de procesos y la navegación (siguiente evento, paso del hilo,
    hasta la línea) usan las columnas.
  - La narración de cada marcador se arma al mirarlo.
- **Límites** (`config/limits.toml`):
  - 1.000.000 de pasos;
  - 10 minutos como red de seguridad;
  - 128 MiB de traza comprimida, con el nuevo corte `traceSize` en el tracer.
- **Planificación manual larga**: viaja en tramos (`1000:1000*40`), por archivo al tracer y en la
  URL, así que elegir en el paso 500.000 no produce un enlace ni un comando enormes.

## Verificación

- Rust:
  - ida y vuelta v1 → v2 → v1 de todas las trazas grabadas, con bloques de 3 y de 1.000 pasos;
  - las trazas decodificadas validan contra `trace.schema.json` y las compactas contra
    `compact.schema.json`;
  - el índice coincide paso a paso con los eventos;
  - los parches sobre tipos reconstruyen la instantánea;
  - una ejecución de más de 10.000 pasos se escribe en bloques;
  - SIGUSR1 detiene el CLI y conserva lo recorrido.
- Interfaz:
  - las trazas compactas de `traces/compact/` (generadas por Rust) se leen igual que sus originales,
    con columnas, índice y navegación idénticos, y se puede saltar a cualquier paso en cualquier orden;
  - 169 pruebas unitarias.
- Extremo a extremo (50 pruebas):
  - un programa de más de 200.000 pasos muestra su avance, se recorre hasta el final, salta a la
    mitad con la línea de tiempo y cada paso responde en menos de un segundo;
  - Detener corta un bucle infinito y muestra lo recorrido.
- Medición manual con un millón de pasos (en esta máquina virtual):
  - 171 s de ejecución;
  - 50 MB de memoria de JavaScript al cargar y 92 MB después de recorrer;
  - unos 100 ms al saltar con la línea de tiempo y unos 20 ms por paso.
- Las trazas de referencia solo cambiaron en los límites que registran y en la numeración de
  instantáneas (ahora se reutiliza la anterior del proceso, no cualquiera igual). Se verificó que la
  memoria de cada paso es la misma.

## Pendiente

- **El singlestep domina el tiempo.** En una máquina virtual anidada cada instrucción cuesta unos
  40 µs, y una línea con 9 instrucciones cuesta unos 400 µs: un millón de pasos de ese tipo tarda
  varios minutos. En Linux sin virtualizar el costo es varias veces menor.
- **Breakpoints en vez de singlestep.** Avanzar hasta el próximo inicio de línea con breakpoints
  reduciría las interrupciones a unas dos por paso. Exige tratar llamadas, retornos, handlers e
  hilos de otra forma; queda como mejora con una prueba de equivalencia contra el modo actual.
