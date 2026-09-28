//! Capa de arquitectura: registros, instrucción de breakpoint y números de syscall.
//! Hoy solo x86_64; aarch64 se agrega en la fase 7 implementando esta misma interfaz.

#[cfg(not(target_arch = "x86_64"))]
compile_error!("pss-tracer soporta por ahora solo x86_64 (aarch64 llega en la fase 7)");

use nix::sys::ptrace;
use nix::unistd::Pid;

pub const BREAKPOINT: u8 = 0xCC;

pub const SYS_READ: u64 = libc::SYS_read as u64;
pub const SYS_WRITE: u64 = libc::SYS_write as u64;
pub const SYS_WRITEV: u64 = libc::SYS_writev as u64;

#[derive(Clone, Copy)]
pub struct Regs(pub libc::user_regs_struct);

impl Regs {
    pub fn get(pid: Pid) -> nix::Result<Self> {
        ptrace::getregs(pid).map(Regs)
    }

    pub fn set(&self, pid: Pid) -> nix::Result<()> {
        ptrace::setregs(pid, self.0)
    }

    pub fn pc(&self) -> u64 {
        self.0.rip
    }

    pub fn set_pc(&mut self, pc: u64) {
        self.0.rip = pc;
    }

    pub fn sp(&self) -> u64 {
        self.0.rsp
    }

    pub fn fp(&self) -> u64 {
        self.0.rbp
    }

    /// Valor de retorno de una función o de una syscall.
    pub fn ret(&self) -> u64 {
        self.0.rax
    }

    /// Argumentos enteros de una llamada según la ABI System V.
    pub fn arg(&self, i: usize) -> u64 {
        [self.0.rdi, self.0.rsi, self.0.rdx, self.0.rcx, self.0.r8, self.0.r9][i]
    }

    pub fn syscall_nr(&self) -> u64 {
        self.0.orig_rax
    }

    pub fn syscall_arg(&self, i: usize) -> u64 {
        [self.0.rdi, self.0.rsi, self.0.rdx, self.0.r10, self.0.r8, self.0.r9][i]
    }
}

/// Tras un breakpoint el contador de programa queda una instrucción más adelante.
pub fn pc_after_breakpoint(addr: u64) -> u64 {
    addr + 1
}

/// Con `-fno-omit-frame-pointer`, en el cuerpo de una función: CFA = rbp + 16,
/// dirección de retorno en rbp + 8 y el rbp del llamador en rbp.
pub fn cfa_of(fp: u64) -> u64 {
    fp + 16
}

pub fn return_address_slot(fp: u64) -> u64 {
    fp + 8
}
