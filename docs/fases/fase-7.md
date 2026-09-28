# Fase 7: arm64 (aarch64)

## Qué se hizo

- **Capa de arquitectura** (`crates/tracer/src/arch.rs`): todo lo que depende del procesador quedó
  en un solo módulo con una implementación para x86_64 y otra para aarch64:
  - registros (pc, sp, frame pointer, retorno, argumentos) con `PTRACE_GETREGSET`;
  - breakpoint (`int3` de 1 byte y `brk #0` de 4 bytes) y dónde queda el pc al detenerse en él;
  - dirección de retorno al entrar a una función (en la pila con `call`, en `lr` con `bl`);
  - saltarse una syscall (`orig_rax` en x86_64, regset `NT_ARM_SYSTEM_CALL` en aarch64);
  - números de syscall: las que aarch64 no tiene (`fork`, `vfork`, `pipe`, `dup2`, `open`,
    `alarm`, `pause`, `getpgrp`) valen allí un número imposible, y glibc usa sus reemplazos
    (`clone`, `pipe2`, `dup3`, `openat`, `setitimer`, `ppoll`, `getpgid`), que ya se modelan.
- **CFA desde la información de unwind**: en aarch64 el CFA no está a 16 bytes del frame pointer como
  en x86_64, sino que depende del tamaño de cada frame. Ahora sale de `.eh_frame` (gimli) para cada
  función al empezar su cuerpo, en ambas arquitecturas. Una prueba verifica que en x86_64 da 16.
- **Syscalls nuevas modeladas en ambas arquitecturas**:
  - `setitimer(ITIMER_REAL)` usa el reloj virtual igual que `alarm`;
  - `ppoll` sin descriptores bloquea como `pause`, o como un sleep si tiene tiempo límite.
- **Trazas de referencia por arquitectura**: las de x86_64 siguen en `traces/reference/`; las de
  aarch64 van en `traces/reference/aarch64/`. Si una falta, la prueba valida solo sus propiedades
  (salidas, eventos, estados), que no dependen de la arquitectura.
- **CI**: el job de Rust corre también en `ubuntu-24.04-arm`, un runner arm64 real (ptrace no
  funciona bajo emulación con qemu). Ese job genera las referencias de aarch64 y las sube como
  artefacto para revisarlas y agregarlas al repositorio.
- **Docker**: la imagen no depende de la arquitectura. En un Mac con Apple Silicon,
  `docker compose up` corre la versión arm64.

## Verificación

- x86_64: 60 pruebas de Rust (se agregaron setitimer, ppoll y el CFA desde `.eh_frame`), 146
  unitarias de la interfaz y 47 de extremo a extremo. Todas las trazas de referencia siguen
  idénticas: el cambio no alteró el comportamiento en x86_64.
- aarch64: todas las pruebas de Rust pasan en el runner `ubuntu-24.04-arm` de CI (hardware arm64
  real), incluidas las de determinismo (dos corridas iguales dan la misma traza).
- Lo que apareció al correr en arm64 real, ya corregido:
  - en `-O0` gcc deja el CFA relativo a `sp`; el desplazamiento respecto de `x29` se saca de donde
    se guardó el frame pointer (antes se suponía `fp + 16` y se envenenaba la dirección de
    retorno: SIGBUS al volver de `main`);
  - las funciones hoja no guardan frame pointer sin `-mno-omit-leaf-frame-pointer`;
  - Ubuntu firma las direcciones de retorno (PAC) con claves distintas en cada ejecución; se
    compila con `-mbranch-protection=none`.
- De paso, en la interfaz: con la CPU lenta se perdían teclas (React llegaba a su límite de
  actualizaciones anidadas); hay una prueba de extremo a extremo con la CPU 8 veces más lenta.

## Pendiente

- Agregar al repositorio las referencias de `traces/reference/aarch64/`: el job de arm64 las sube
  como artefacto `referencias-aarch64` (esta sesión no pudo descargarlo). Mientras falten, en arm64
  cada ejemplo valida sus propiedades y el determinismo, pero no se compara byte a byte.
