//! Acceso a la memoria y al mapa de memoria del proceso rastreado.

use nix::unistd::Pid;
use std::fs::File;
use std::os::unix::fs::FileExt;

pub struct Tracee {
    pub pid: Pid,
    mem: File,
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub start: u64,
    pub end: u64,
    pub perms: String,
    pub offset: u64,
    pub path: String,
}

impl Tracee {
    pub fn attach(pid: Pid) -> std::io::Result<Self> {
        // /proc/pid/mem permite escribir incluso en páginas de solo lectura (breakpoints en .text).
        let mem = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/proc/{pid}/mem"))?;
        Ok(Tracee { pid, mem })
    }

    pub fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.mem.read_exact_at(&mut buf, addr).ok()?;
        Some(buf)
    }

    pub fn read_u64(&self, addr: u64) -> Option<u64> {
        self.read(addr, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn write(&self, addr: u64, bytes: &[u8]) -> bool {
        self.mem.write_all_at(bytes, addr).is_ok()
    }

    /// Cadena C (hasta el primer \0 o `max` bytes).
    pub fn read_cstr(&self, addr: u64, max: usize) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let mut a = addr;
        while out.len() < max {
            let chunk = self.read(a, 16).or_else(|| self.read(a, 1))?;
            for b in chunk {
                if b == 0 || out.len() >= max {
                    return Some(out);
                }
                out.push(b);
            }
            a += 16;
        }
        Some(out)
    }

    pub fn maps(&self) -> Vec<Mapping> {
        let Ok(text) = std::fs::read_to_string(format!("/proc/{}/maps", self.pid)) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let (range, perms, offset) = (it.next()?, it.next()?, it.next()?);
                let _dev = it.next()?;
                let _inode = it.next()?;
                let path = it.collect::<Vec<_>>().join(" ");
                let (s, e) = range.split_once('-')?;
                Some(Mapping {
                    start: u64::from_str_radix(s, 16).ok()?,
                    end: u64::from_str_radix(e, 16).ok()?,
                    perms: perms.to_string(),
                    offset: u64::from_str_radix(offset, 16).ok()?,
                    path,
                })
            })
            .collect()
    }
}
