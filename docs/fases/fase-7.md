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
- aarch64: el tracer y el modelo compilan y pasan `clippy -D warnings` para
  `aarch64-unknown-linux-gnu`.

## Pendiente

- **No se pudo ejecutar en arm64 real en esta sesión**: el entorno es x86_64 y ptrace no funciona bajo
  qemu-user. La primera corrida del job `ubuntu-24.04-arm` de CI es la verificación real. Si el
  repositorio es privado y su plan no incluye runners arm64, hay que correr `cargo test` en una
  máquina arm64 (una Raspberry Pi 4/5 con Linux de 64 bits o un Mac con Apple Silicon usando
  Docker).
- Después, agregar al repositorio las referencias de `traces/reference/aarch64/` que genera ese job.
