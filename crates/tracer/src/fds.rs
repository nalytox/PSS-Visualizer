//! Modelo de descriptores y pipes. Refleja lo que el kernel tiene: el tracer lo actualiza en cada
//! syscall que crea, duplica o cierra un fd, y con él decide si un read o un write bloquea.

use std::collections::BTreeMap;
use trace_model::{Fd, Pipe, PipeEnd, PipeEndKind, PipeWarning, PipeWarningKind};

pub type FdTable = BTreeMap<u32, Fd>;

/// Capacidad de un pipe en Linux (16 páginas).
pub const PIPE_CAPACITY: usize = 65536;
/// Bytes del buffer que viajan en la traza; `size` dice cuántos hay de verdad.
const SHOWN: usize = 256;

pub fn std_table() -> FdTable {
    BTreeMap::from([
        (0, Fd::Stdin { cloexec: None }),
        (1, Fd::Terminal { cloexec: None }),
        (2, Fd::Terminal { cloexec: None }),
    ])
}

pub fn with_cloexec(fd: &Fd, on: bool) -> Fd {
    let c = on.then_some(true);
    match fd.clone() {
        Fd::Stdin { .. } => Fd::Stdin { cloexec: c },
        Fd::Terminal { .. } => Fd::Terminal { cloexec: c },
        Fd::Pipe { pipe, end, .. } => Fd::Pipe { pipe, end, cloexec: c },
        Fd::File { path, mode, .. } => Fd::File { path, mode, cloexec: c },
        Fd::Other { label, .. } => Fd::Other { label, cloexec: c },
    }
}

fn cloexec_of(fd: &Fd) -> bool {
    let c = match fd {
        Fd::Stdin { cloexec } | Fd::Terminal { cloexec } => cloexec,
        Fd::Pipe { cloexec, .. } | Fd::File { cloexec, .. } | Fd::Other { cloexec, .. } => cloexec,
    };
    *c == Some(true)
}

/// exec cierra los fds marcados con FD_CLOEXEC.
pub fn close_on_exec(table: &mut FdTable) {
    table.retain(|_, fd| !cloexec_of(fd));
}

/// dup, dup2, dup3 y F_DUPFD: `new` apunta a lo mismo que `old`. Devuelve lo que había en `new`.
pub fn dup(table: &mut FdTable, old: u32, new: u32, cloexec: bool) -> Option<Option<Fd>> {
    let entry = with_cloexec(table.get(&old)?, cloexec);
    Some(table.insert(new, entry))
}

pub fn pipe_of(fd: Option<&Fd>) -> Option<(&str, PipeEndKind)> {
    match fd {
        Some(Fd::Pipe { pipe, end, .. }) => Some((pipe.as_str(), *end)),
        _ => None,
    }
}

pub struct PipeBuf {
    pub id: String,
    pub created_by: u32,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub struct Pipes {
    pub list: Vec<PipeBuf>,
}

impl Pipes {
    pub fn create(&mut self, pid: u32) -> String {
        let id = format!("p{}", self.list.len());
        self.list.push(PipeBuf {
            id: id.clone(),
            created_by: pid,
            data: Vec::new(),
        });
        id
    }

    pub fn get(&self, id: &str) -> Option<&PipeBuf> {
        self.list.iter().find(|p| p.id == id)
    }

    pub fn write(&mut self, id: &str, bytes: &[u8]) {
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            p.data.extend_from_slice(bytes);
        }
    }

    pub fn read(&mut self, id: &str, n: usize) {
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            p.data.drain(..n.min(p.data.len()));
        }
    }

    pub fn len(&self, id: &str) -> usize {
        self.get(id).map_or(0, |p| p.data.len())
    }
}

/// Extremos abiertos de un pipe entre los procesos vivos.
pub fn ends<'a>(tables: impl Iterator<Item = (u32, &'a FdTable)>, id: &str, kind: PipeEndKind) -> Vec<PipeEnd> {
    let mut out = Vec::new();
    for (pid, table) in tables {
        for (fd, entry) in table {
            if pipe_of(Some(entry)) == Some((id, kind)) {
                out.push(PipeEnd { pid, fd: *fd });
            }
        }
    }
    out
}

/// Pipes que siguen abiertos en algún proceso vivo, como van en la traza. `reading` son los procesos
/// bloqueados leyendo cada pipe: si alguno tiene abierto el extremo de escritura de ese mismo pipe,
/// nunca verá EOF (el tapón gris).
pub fn view(
    pipes: &Pipes,
    tables: &[(u32, &FdTable)],
    reading: &[(u32, String)],
    latin1: fn(&[u8]) -> String,
) -> Vec<Pipe> {
    let mut out = Vec::new();
    for p in &pipes.list {
        let readers = ends(tables.iter().map(|(a, b)| (*a, *b)), &p.id, PipeEndKind::Read);
        let writers = ends(tables.iter().map(|(a, b)| (*a, *b)), &p.id, PipeEndKind::Write);
        if readers.is_empty() && writers.is_empty() {
            continue;
        }
        let warnings = writers
            .iter()
            .filter(|w| reading.iter().any(|(pid, id)| *pid == w.pid && *id == p.id))
            .map(|w| PipeWarning {
                kind: PipeWarningKind::UnclosedEnd,
                pid: w.pid,
                fd: w.fd,
                end: PipeEndKind::Write,
            })
            .collect();
        out.push(Pipe {
            id: p.id.clone(),
            created_by: p.created_by,
            size: p.data.len() as u64,
            buffer: latin1(&p.data[..p.data.len().min(SHOWN)]),
            capacity: PIPE_CAPACITY as u64,
            broken: readers.is_empty(),
            readers,
            writers,
            warnings,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe_fd(id: &str, end: PipeEndKind) -> Fd {
        Fd::Pipe {
            pipe: id.into(),
            end,
            cloexec: None,
        }
    }

    fn latin1(b: &[u8]) -> String {
        b.iter().map(|&c| c as char).collect()
    }

    #[test]
    fn dup2_replaces_the_target_and_reports_it() {
        let mut t = std_table();
        t.insert(3, pipe_fd("p0", PipeEndKind::Read));
        let replaced = dup(&mut t, 3, 0, false).unwrap();
        assert_eq!(replaced, Some(Fd::Stdin { cloexec: None }));
        assert_eq!(pipe_of(t.get(&0)), Some(("p0", PipeEndKind::Read)));
        assert!(dup(&mut t, 9, 4, false).is_none(), "dup de un fd cerrado falla");
    }

    #[test]
    fn exec_closes_only_cloexec_descriptors() {
        let mut t = std_table();
        t.insert(3, with_cloexec(&pipe_fd("p0", PipeEndKind::Read), true));
        t.insert(4, pipe_fd("p0", PipeEndKind::Write));
        close_on_exec(&mut t);
        assert_eq!(t.keys().copied().collect::<Vec<_>>(), [0, 1, 2, 4]);
    }

    #[test]
    fn buffer_is_fifo() {
        let mut p = Pipes::default();
        let id = p.create(1000);
        p.write(&id, b"hola");
        p.read(&id, 2);
        assert_eq!(p.get(&id).unwrap().data, b"la");
        p.read(&id, 10);
        assert_eq!(p.len(&id), 0);
    }

    #[test]
    fn view_lists_ends_and_flags_the_reader_holding_a_write_end() {
        let mut pipes = Pipes::default();
        let id = pipes.create(1000);
        let mut parent = std_table();
        parent.insert(3, pipe_fd(&id, PipeEndKind::Read));
        parent.insert(4, pipe_fd(&id, PipeEndKind::Write));
        let child = std_table();
        let reading = [(1000, id.clone())];
        let v = view(&pipes, &[(1000, &parent), (1001, &child)], &reading, latin1);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].readers, [PipeEnd { pid: 1000, fd: 3 }]);
        assert_eq!(v[0].warnings.len(), 1);
        assert_eq!(v[0].warnings[0].fd, 4);
        assert!(!v[0].broken);
        // Sin extremos abiertos el pipe desaparece de la traza.
        assert!(view(&pipes, &[(1001, &child)], &[], latin1).is_empty());
    }
}
