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
pub const SYS_CLONE: u64 = libc::SYS_clone as u64;
pub const SYS_CLONE3: u64 = libc::SYS_clone3 as u64;
pub const SYS_FORK: u64 = libc::SYS_fork as u64;
pub const SYS_VFORK: u64 = libc::SYS_vfork as u64;
pub const SYS_EXECVE: u64 = libc::SYS_execve as u64;
pub const SYS_WAIT4: u64 = libc::SYS_wait4 as u64;
pub const SYS_KILL: u64 = libc::SYS_kill as u64;
pub const SYS_TKILL: u64 = libc::SYS_tkill as u64;
pub const SYS_TGKILL: u64 = libc::SYS_tgkill as u64;
pub const SYS_GETPID: u64 = libc::SYS_getpid as u64;
pub const SYS_GETTID: u64 = libc::SYS_gettid as u64;
pub const SYS_GETPPID: u64 = libc::SYS_getppid as u64;
pub const SYS_GETPGID: u64 = libc::SYS_getpgid as u64;
pub const SYS_GETPGRP: u64 = libc::SYS_getpgrp as u64;
pub const SYS_SETPGID: u64 = libc::SYS_setpgid as u64;
pub const SYS_GETSID: u64 = libc::SYS_getsid as u64;
pub const SYS_NANOSLEEP: u64 = libc::SYS_nanosleep as u64;
pub const SYS_CLOCK_NANOSLEEP: u64 = libc::SYS_clock_nanosleep as u64;
pub const SYS_READV: u64 = libc::SYS_readv as u64;
pub const SYS_PIPE: u64 = libc::SYS_pipe as u64;
pub const SYS_PIPE2: u64 = libc::SYS_pipe2 as u64;
pub const SYS_DUP: u64 = libc::SYS_dup as u64;
pub const SYS_DUP2: u64 = libc::SYS_dup2 as u64;
pub const SYS_DUP3: u64 = libc::SYS_dup3 as u64;
pub const SYS_FCNTL: u64 = libc::SYS_fcntl as u64;
pub const SYS_CLOSE: u64 = libc::SYS_close as u64;
pub const SYS_OPEN: u64 = libc::SYS_open as u64;
pub const SYS_OPENAT: u64 = libc::SYS_openat as u64;
pub const SYS_PAUSE: u64 = libc::SYS_pause as u64;
pub const SYS_RT_SIGSUSPEND: u64 = libc::SYS_rt_sigsuspend as u64;

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

    /// En la parada de entrada a una syscall, cambia el argumento que verá el kernel.
    pub fn set_syscall_arg(&mut self, i: usize, v: u64) {
        let r = &mut self.0;
        *[&mut r.rdi, &mut r.rsi, &mut r.rdx, &mut r.r10, &mut r.r8, &mut r.r9][i] = v;
    }

    /// En la parada de entrada, un número inválido hace que el kernel se salte la syscall.
    pub fn skip_syscall(&mut self) {
        self.0.orig_rax = u64::MAX;
    }

    pub fn set_ret(&mut self, v: u64) {
        self.0.rax = v;
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
