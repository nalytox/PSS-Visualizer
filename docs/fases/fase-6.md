# Fase 6: introducción y pulido

## Qué funciona

- **Modo introducción** (`web/src/intro/`): cuatro capítulos animados (Forks 31 s, Threads 37 s,
  Pipes 31 s, Señales 27 s). Siguen el guion de la sección 11.2, con un subtítulo en lenguaje simple
  por escena. Se abre solo en la primera visita y después queda en el botón "Introducción" de la
  barra superior. No se abre al llegar con un enlace a un programa o una traza.
- **Motor determinista** (`engine.ts`): un capítulo es una lista declarativa de tweens (elemento,
  propiedad, valores inicial y final, intervalo y easing) y subtítulos. Cada fotograma es una
  función pura de `t`, así que adelantar, retroceder y arrastrar la barra dan siempre el mismo
  dibujo. No hay videos, GIF ni imágenes: todo es SVG calculado en el navegador.
- **Mismo vocabulario visual**: cuadrados de proceso, carriles, tubo con cápsulas (`Capsule` del
  reproductor), cables, bloque de señal y patrones de `Defs`, con las mismas clases y tokens de
  color del visualizador.
- **Controles** (sección 11.3): reproducir y pausar, capítulo anterior y siguiente, barra de
  progreso desplazable, velocidad de 0,5x a 2x y "Saltar introducción" siempre visible (también
  con Escape). Al terminar cada capítulo, "Probar este ejemplo" carga su programa: 03, 12, 06 y 09.
- **Movimiento reducido**: con `prefers-reduced-motion` no hay animación; cada capítulo se recorre
  por fotogramas clave (el final de cada escena) con clic o con las flechas.
- **Accesibilidad**: axe-core (WCAG 2.1 AA) revisa en las pruebas de extremo a extremo el
  visualizador (procesos y pipes, hilos, señales) y la introducción en tema claro y oscuro, sin
  problemas. Lo que encontró y se corrigió:
  - contraste de `--text-2` sobre `--surface-2` (4,26 → 4,76);
  - PID de la terminal con texto del color del proceso (3,2);
  - marcadores de la línea de tiempo anidados dentro del control deslizante;
  - panel de código no enfocable para desplazarlo con el teclado.
- **Tokens de color finales**: `--text-2` ajustado y `--blackbox`/`--blackbox-text` en ambos temas;
  las pruebas de contraste cubren ahora 16 pares por tema.
- **Pruebas**: 57 de Rust, 146 unitarias de la interfaz (motor determinista, duraciones del spec,
  subtítulos que cubren cada capítulo, tamaño del código) y 47 de extremo a extremo (introducción,
  movimiento reducido, accesibilidad en ambos temas).

## Criterios de la sección 11 y globales

| Criterio | Estado |
| --- | --- |
| Los cuatro capítulos enlazan a 03, 12, 06 y 09 | Cumple (prueba unitaria y de extremo a extremo) |
| 60 fps en un portátil común | Cumple: 60 fps medidos en Chromium sin GPU (la línea base de una página vacía es 60) |
| Código de la introducción bajo 150 KB sin comprimir | Cumple: 29 KB |
| Teclado, tema claro y oscuro, desde 1280 px | Cumple: sin problemas de axe en ambos temas; controles con teclado |
| Un paso responde en menos de 100 ms con 16 procesos | Cumple (prueba de extremo a extremo, fase 0) |

## Desviaciones y decisiones

- **Sin GSAP**: el motor propio tiene menos de 100 líneas y no necesita dependencias.
- **Sin `backdrop-filter`** en el fondo de la introducción: el desenfoque bajaba la animación a
  22 fps sin aceleración gráfica.
- **Piezas visuales, no los componentes completos**: el cuadrado de proceso del visualizador
  depende de una traza y su contexto; la introducción usa las mismas clases, tokens y componentes
  hoja (`Capsule`, `Defs`), así se ve igual sin necesitar una traza.
