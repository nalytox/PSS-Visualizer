//! Motor de trazas: compila un programa C, lo ejecuta bajo ptrace y produce su traza.

pub mod arch;
pub mod compile;
pub mod dwarf;
pub mod fds;
pub mod heap;
pub mod launch;
pub mod limits;
pub mod memory;
pub mod process;
pub mod signals;
pub mod syms;
pub mod tracer;
