//! Capa de arquitectura: registros, breakpoint, números de syscall y convención de llamadas, para
//! x86_64 y aarch64. El resto del tracer solo usa esta interfaz.

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("pss-tracer soporta x86_64 y aarch64");

use nix::sys::ptrace;
use nix::unistd::Pid;

#[cfg(target_arch = "x86_64")]
pub const ARCH: trace_model::Arch = trace_model::Arch::X86_64;
#[cfg(target_arch = "aarch64")]
pub const ARCH: trace_model::Arch = trace_model::Arch::Aarch64;

/// int3 en x86_64; `brk #0` en aarch64.
#[cfg(target_arch = "x86_64")]
pub const BREAKPOINT: &[u8] = &[0xCC];
#[cfg(target_arch = "aarch64")]
pub const BREAKPOINT: &[u8] = &[0x00, 0x00, 0x20, 0xd4];

/// Syscalls que aarch64 no tiene (glibc usa clone, pipe2, dup3, openat, setitimer, ppoll…): allí
/// valen un número imposible distinto para cada una, así nunca coinciden.
macro_rules! sys {
    ($name:ident, $x86:ident, $k:expr) => {
        #[cfg(target_arch = "x86_64")]
        pub const $name: u64 = libc::$x86 as u64;
        #[cfg(target_arch = "aarch64")]
        pub const $name: u64 = u64::MAX - $k;
    };
}

pub const SYS_READ: u64 = libc::SYS_read as u64;
pub const SYS_WRITE: u64 = libc::SYS_write as u64;
pub const SYS_WRITEV: u64 = libc::SYS_writev as u64;
pub const SYS_CLONE: u64 = libc::SYS_clone as u64;
pub const SYS_CLONE3: u64 = libc::SYS_clone3 as u64;
sys!(SYS_FORK, SYS_fork, 1);
sys!(SYS_VFORK, SYS_vfork, 2);
pub const SYS_EXECVE: u64 = libc::SYS_execve as u64;
pub const SYS_WAIT4: u64 = libc::SYS_wait4 as u64;
pub const SYS_KILL: u64 = libc::SYS_kill as u64;
pub const SYS_TKILL: u64 = libc::SYS_tkill as u64;
pub const SYS_TGKILL: u64 = libc::SYS_tgkill as u64;
pub const SYS_GETPID: u64 = libc::SYS_getpid as u64;
pub const SYS_GETTID: u64 = libc::SYS_gettid as u64;
pub const SYS_GETPPID: u64 = libc::SYS_getppid as u64;
pub const SYS_GETPGID: u64 = libc::SYS_getpgid as u64;
sys!(SYS_GETPGRP, SYS_getpgrp, 3);
pub const SYS_SETPGID: u64 = libc::SYS_setpgid as u64;
pub const SYS_GETSID: u64 = libc::SYS_getsid as u64;
pub const SYS_NANOSLEEP: u64 = libc::SYS_nanosleep as u64;
pub const SYS_CLOCK_NANOSLEEP: u64 = libc::SYS_clock_nanosleep as u64;
pub const SYS_READV: u64 = libc::SYS_readv as u64;
sys!(SYS_PIPE, SYS_pipe, 4);
pub const SYS_PIPE2: u64 = libc::SYS_pipe2 as u64;
pub const SYS_DUP: u64 = libc::SYS_dup as u64;
sys!(SYS_DUP2, SYS_dup2, 5);
pub const SYS_DUP3: u64 = libc::SYS_dup3 as u64;
pub const SYS_FCNTL: u64 = libc::SYS_fcntl as u64;
pub const SYS_CLOSE: u64 = libc::SYS_close as u64;
sys!(SYS_OPEN, SYS_open, 6);
pub const SYS_OPENAT: u64 = libc::SYS_openat as u64;
pub const SYS_RT_SIGACTION: u64 = libc::SYS_rt_sigaction as u64;
pub const SYS_RT_SIGPROCMASK: u64 = libc::SYS_rt_sigprocmask as u64;
pub const SYS_RT_SIGRETURN: u64 = libc::SYS_rt_sigreturn as u64;
sys!(SYS_ALARM, SYS_alarm, 7);
pub const SYS_FUTEX: u64 = libc::SYS_futex as u64;
pub const SYS_EXIT: u64 = libc::SYS_exit as u64;
sys!(SYS_PAUSE, SYS_pause, 8);
pub const SYS_PPOLL: u64 = libc::SYS_ppoll as u64;
pub const SYS_RT_SIGSUSPEND: u64 = libc::SYS_rt_sigsuspend as u64;
pub const SYS_SETITIMER: u64 = libc::SYS_setitimer as u64;

#[derive(Clone, Copy)]
pub struct Regs(pub libc::user_regs_struct);

impl Regs {
    pub fn get(pid: Pid) -> nix::Result<Self> {
        ptrace::getregs(pid).map(Regs)
    }

    pub fn set(&self, pid: Pid) -> nix::Result<()> {
        ptrace::setregs(pid, self.0)
    }
}

#[cfg(target_arch = "x86_64")]
impl Regs {
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

    pub fn set_ret(&mut self, v: u64) {
        self.0.rax = v;
    }

    /// Argumentos enteros de una llamada según la ABI System V.
    pub fn arg(&self, i: usize) -> u64 {
        [self.0.rdi, self.0.rsi, self.0.rdx, self.0.rcx, self.0.r8, self.0.r9][i]
    }

    /// En la parada de entrada a una syscall, cambia el argumento que verá el kernel.
    pub fn set_syscall_arg(&mut self, i: usize, v: u64) {
        let r = &mut self.0;
        *[&mut r.rdi, &mut r.rsi, &mut r.rdx, &mut r.r10, &mut r.r8, &mut r.r9][i] = v;
    }

    /// Dirección de retorno en la primera instrucción de una función: `call` la dejó en la pila.
    pub fn entry_return_address(&self, read_u64: impl Fn(u64) -> Option<u64>) -> u64 {
        read_u64(self.sp()).unwrap_or(0)
    }

    /// CFA en la primera instrucción: la pila del llamador antes de `call`.
    pub fn entry_cfa(&self) -> u64 {
        self.sp() + 8
    }
}

#[cfg(target_arch = "aarch64")]
impl Regs {
    pub fn pc(&self) -> u64 {
        self.0.pc
    }

    pub fn set_pc(&mut self, pc: u64) {
        self.0.pc = pc;
    }

    pub fn sp(&self) -> u64 {
        self.0.sp
    }

    pub fn fp(&self) -> u64 {
        self.0.regs[29]
    }

    pub fn ret(&self) -> u64 {
        self.0.regs[0]
    }

    pub fn set_ret(&mut self, v: u64) {
        self.0.regs[0] = v;
    }

    /// Argumentos en x0…x7 (AAPCS64).
    pub fn arg(&self, i: usize) -> u64 {
        self.0.regs[i]
    }

    /// Los argumentos de una syscall también van en x0…x5.
    pub fn set_syscall_arg(&mut self, i: usize, v: u64) {
        self.0.regs[i] = v;
    }

    /// `bl` deja la dirección de retorno en x30 (lr), no en la pila.
    pub fn entry_return_address(&self, _read_u64: impl Fn(u64) -> Option<u64>) -> u64 {
        self.0.regs[30]
    }

    pub fn entry_cfa(&self) -> u64 {
        self.sp()
    }
}

/// Hace que el kernel se salte la syscall detenida en su entrada (la salida devuelve -ENOSYS).
#[cfg(target_arch = "x86_64")]
pub fn skip_syscall(pid: Pid, regs: &mut Regs) -> nix::Result<()> {
    regs.0.orig_rax = u64::MAX;
    regs.set(pid)
}

/// En aarch64 el número de syscall no está en los registros generales: se cambia con el regset
/// NT_ARM_SYSTEM_CALL.
#[cfg(target_arch = "aarch64")]
pub fn skip_syscall(pid: Pid, _regs: &mut Regs) -> nix::Result<()> {
    const NT_ARM_SYSTEM_CALL: libc::c_int = 0x404;
    let mut nr: libc::c_int = -1;
    let mut iov = libc::iovec {
        iov_base: (&mut nr as *mut libc::c_int).cast(),
        iov_len: std::mem::size_of::<libc::c_int>(),
    };
    let r = unsafe {
        libc::ptrace(
            libc::PTRACE_SETREGSET,
            pid.as_raw(),
            NT_ARM_SYSTEM_CALL,
            &mut iov as *mut libc::iovec,
        )
    };
    nix::errno::Errno::result(r).map(drop)
}

/// Dónde queda el contador de programa al detenerse en un breakpoint puesto en `addr`.
#[cfg(target_arch = "x86_64")]
pub fn pc_after_breakpoint(addr: u64) -> u64 {
    addr + 1
}

#[cfg(target_arch = "aarch64")]
pub fn pc_after_breakpoint(addr: u64) -> u64 {
    addr
}

/// Una dirección dentro de la instrucción de llamada, dada su dirección de retorno.
#[cfg(target_arch = "x86_64")]
pub fn call_site(ret_addr: u64) -> u64 {
    ret_addr - 1
}

#[cfg(target_arch = "aarch64")]
pub fn call_site(ret_addr: u64) -> u64 {
    ret_addr - 4
}

/// Registro de frame pointer en la numeración DWARF (rbp = 6, x29 = 29).
#[cfg(target_arch = "x86_64")]
pub const DWARF_FP: u16 = 6;
#[cfg(target_arch = "aarch64")]
pub const DWARF_FP: u16 = 29;

/// Con frame pointer, ambas arquitecturas guardan el frame pointer del llamador en [fp] y la
/// dirección de retorno en [fp + 8].
pub fn return_address_slot(fp: u64) -> u64 {
    fp + 8
}
