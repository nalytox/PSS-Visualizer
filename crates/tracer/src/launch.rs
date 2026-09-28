//! Lanzamiento del programa bajo ptrace, con un entorno fijo para que las direcciones y la traza
//! sean reproducibles.

use crate::limits::Limits;
use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::resource::{Resource, setrlimit};
use nix::unistd::{ForkResult, Pid, dup2_stderr, dup2_stdin, dup2_stdout, execve, fork};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::Path;

pub struct Launched {
    pub pid: Pid,
    /// Extremo de escritura del stdin; se mantiene abierto si la entrada no termina con EOF.
    pub stdin_writer: Option<File>,
}

/// Capacidad de un pipe en Linux: el stdin precargado se escribe completo sin bloquear.
pub const STDIN_MAX: usize = 65536;

pub fn launch(dir: &Path, binary_name: &str, stdin: &[u8], stdin_eof: bool, limits: &Limits) -> nix::Result<Launched> {
    let (stdin_r, stdin_w) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)?;
    // stdout y stderr van a una pseudo-terminal: así printf tiene buffer de línea, como en una
    // terminal real. La salida se captura en las syscalls write; el maestro solo se vacía.
    let pty = nix::pty::openpty(None, None)?;
    let dir_c = CString::new(dir.as_os_str().as_encoded_bytes()).unwrap();
    let prog = CString::new(format!("./{binary_name}")).unwrap();
    let argv = [prog.clone()];
    let env = [
        c"PATH=/usr/local/bin:/usr/bin:/bin".to_owned(),
        c"LANG=C.UTF-8".to_owned(),
        c"HOME=/tmp".to_owned(),
        c"TERM=dumb".to_owned(),
    ];
    let mem = limits.memory_bytes;
    let cpu = limits.wall_time_ms.div_ceil(1000) + 1;

    match unsafe { fork()? } {
        ForkResult::Child => {
            // Solo llamadas seguras tras fork: si algo falla, se sale con 127.
            let fail = || unsafe { libc::_exit(127) };
            if dup2_stdin(&stdin_r).is_err() || dup2_stdout(&pty.slave).is_err() || dup2_stderr(&pty.slave).is_err() {
                fail();
            }
            // El programa solo hereda 0, 1 y 2: nada de los pipes o terminales del tracer (ni de
            // otras trazas que corran en paralelo en el mismo proceso).
            unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) };
            let _ = personality::set(Persona::ADDR_NO_RANDOMIZE);
            let _ = setrlimit(Resource::RLIMIT_AS, mem, mem);
            let _ = setrlimit(Resource::RLIMIT_CPU, cpu, cpu);
            let _ = setrlimit(Resource::RLIMIT_FSIZE, 1 << 20, 1 << 20);
            let _ = setrlimit(Resource::RLIMIT_CORE, 0, 0);
            if unsafe { libc::chdir(dir_c.as_ptr()) } != 0 || ptrace::traceme().is_err() {
                fail();
            }
            let _ = execve(&prog, &argv, &env);
            fail();
            unreachable!()
        }
        ForkResult::Parent { child } => {
            drop(stdin_r);
            drop(pty.slave);
            drain(pty.master);
            let mut writer = File::from(stdin_w);
            let _ = writer.write_all(&stdin[..stdin.len().min(STDIN_MAX)]);
            Ok(Launched {
                pid: child,
                stdin_writer: if stdin_eof { None } else { Some(writer) },
            })
        }
    }
}

fn drain(master: OwnedFd) {
    std::thread::spawn(move || {
        let mut f = File::from(master);
        let mut buf = [0u8; 4096];
        while matches!(f.read(&mut buf), Ok(n) if n > 0) {}
    });
}
