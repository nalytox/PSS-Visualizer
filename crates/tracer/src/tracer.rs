//! Motor de la fase 1: un proceso, un hilo. Avanza por línea de código con singlestep dentro del
//! código del usuario; cada llamada a una biblioteca es un paso atómico que corre hasta su dirección
//! de retorno (sin singlestep: el loader y libc harían inviable el tiempo, ver la propuesta).

use crate::arch::{self, Regs};
use crate::compile::{self, BINARY_NAME, SOURCE_NAME};
use crate::dwarf::DebugInfo;
use crate::heap::Heap;
use crate::launch;
use crate::limits::Limits;
use crate::memory::{self, Reader, hex, latin1};
use crate::process::Tracee;
use crate::syms::Symbols;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::Pid;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use trace_model::*;

/// PID y TID que ve la traza: fijos para que la misma entrada produzca la misma traza.
pub const VPID: u32 = 1000;

pub struct Options {
    pub source: String,
    pub stdin: Vec<u8>,
    pub stdin_eof: bool,
    pub limits: Limits,
}

pub fn run(opts: &Options) -> Trace {
    let dir = work_dir();
    let trace = run_in(&dir, opts);
    let _ = std::fs::remove_dir_all(&dir);
    trace
}

fn work_dir() -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pss-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("no se pudo crear el directorio de trabajo");
    dir
}

fn empty_trace(opts: &Options, compile: CompileResult, outcome: Outcome) -> Trace {
    Trace {
        version: 1,
        arch: Arch::X86_64,
        source: opts.source.clone(),
        stdin: latin1(&opts.stdin),
        run: RunConfig {
            policy: Policy::RoundRobin,
            seed: 0,
            stdin_eof: opts.stdin_eof,
            schedule: Vec::new(),
            injections: Vec::new(),
            limits: opts.limits.to_model(),
        },
        compile,
        outcome,
        truncated: false,
        truncated_reason: None,
        steps: Vec::new(),
        snapshots: BTreeMap::new(),
        output: Vec::new(),
        summary: Summary {
            leaks: Vec::new(),
            mem_errors: Vec::new(),
        },
    }
}

fn run_in(dir: &Path, opts: &Options) -> Trace {
    let compiled = match compile::compile(dir, &opts.source) {
        Ok(c) => c,
        Err(e) => {
            let result = CompileResult {
                ok: false,
                command: compile::command_line(),
                diagnostics: vec![Diagnostic {
                    line: 0,
                    col: 0,
                    severity: Severity::Error,
                    message: format!("no se pudo ejecutar gcc: {e}"),
                }],
            };
            return empty_trace(opts, result, Outcome::CompileError);
        }
    };
    if !compiled.result.ok {
        return empty_trace(opts, compiled.result, Outcome::CompileError);
    }
    let debug = match DebugInfo::load(&compiled.binary, SOURCE_NAME) {
        Ok(d) => d,
        Err(message) => {
            let mut result = compiled.result;
            result.ok = false;
            result.diagnostics.push(Diagnostic {
                line: 0,
                col: 0,
                severity: Severity::Error,
                message,
            });
            return empty_trace(opts, result, Outcome::CompileError);
        }
    };
    let mut trace = empty_trace(opts, compiled.result, Outcome::Truncated);
    let launched = match launch::launch(dir, BINARY_NAME, &opts.stdin, opts.stdin_eof, &opts.limits) {
        Ok(l) => l,
        Err(e) => {
            trace.compile.diagnostics.push(Diagnostic {
                line: 0,
                col: 0,
                severity: Severity::Error,
                message: format!("no se pudo lanzar el programa: {e}"),
            });
            trace.compile.ok = false;
            trace.outcome = Outcome::CompileError;
            return trace;
        }
    };
    let binary_path = std::fs::canonicalize(&compiled.binary)
        .unwrap_or(compiled.binary)
        .to_string_lossy()
        .into_owned();
    // /proc/pid/mem se abre recién tras la parada del execve: abierto antes, apuntaría a la
    // memoria anterior al exec y toda lectura fallaría.
    match waitpid(launched.pid, None) {
        Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {}
        _ => {
            let _ = nix::sys::signal::kill(launched.pid, Signal::SIGKILL);
            trace.compile.diagnostics.push(Diagnostic {
                line: 0,
                col: 0,
                severity: Severity::Error,
                message: "el programa no pudo iniciarse".into(),
            });
            trace.outcome = Outcome::CompileError;
            return trace;
        }
    }
    let session = Session::new(&debug, opts, launched.pid, launched.stdin_writer, binary_path);
    session.run(&mut trace);
    trace
}

/// Por qué terminó el recorrido.
enum End {
    Exited(WaitStatus),
    AwaitingInput,
    Truncated(TruncatedReason),
}

#[derive(Clone)]
struct Stop {
    pc: u64,
    line: u32,
    cfa: u64,
    func: String,
}

struct CallFrame {
    func: usize,
    ret_addr: u64,
    cfa: u64,
    poisoned: bool,
}

struct Session<'a> {
    debug: &'a DebugInfo,
    opts: &'a Options,
    pid: Pid,
    tracee: Tracee,
    syms: Symbols,
    heap: Heap,
    binary_path: String,
    _stdin_writer: Option<File>,
    stdin_consumed: u64,
    t: u64,
    steps: Vec<Step>,
    snapshots: BTreeMap<String, MemorySnapshot>,
    snapshot_ids: HashMap<String, String>,
    output: Vec<OutputChunk>,
    output_bytes: u64,
    events: Vec<Event>,
    pending_output: Vec<(u32, Stream, String)>,
    last: Option<Stop>,
    calls: Vec<CallFrame>,
    in_syscall: Option<(u64, [u64; 3])>,
    mem_errors: Vec<MemErrorRef>,
    started: Instant,
    timed_out: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    last_mem: Option<String>,
    /// Mapa de memoria del proceso: solo cambia dentro de libc (malloc, mmap), así que se relee
    /// después de cada llamada a biblioteca y no en cada paso.
    maps: Vec<crate::process::Mapping>,
}

impl<'a> Session<'a> {
    fn new(debug: &'a DebugInfo, opts: &'a Options, pid: Pid, stdin_writer: Option<File>, binary_path: String) -> Self {
        let timed_out = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        // Un programa bloqueado en el kernel no devuelve el control: el vigilante lo mata al vencer el tiempo.
        {
            let flag = timed_out.clone();
            let ms = opts.limits.wall_time_ms;
            let done = finished.clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_millis(ms);
                while Instant::now() < deadline {
                    if done.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                // Si la traza ya terminó, ese PID puede pertenecer a otro proceso: no se toca.
                if !done.load(Ordering::SeqCst) {
                    flag.store(true, Ordering::SeqCst);
                    let _ = nix::sys::signal::kill(pid, Signal::SIGKILL);
                }
            });
        }
        Session {
            debug,
            opts,
            pid,
            tracee: Tracee::attach(pid).expect("no se pudo abrir /proc/pid/mem"),
            syms: Symbols::default(),
            heap: Heap::default(),
            binary_path,
            _stdin_writer: stdin_writer,
            stdin_consumed: 0,
            t: 0,
            steps: Vec::new(),
            snapshots: BTreeMap::new(),
            snapshot_ids: HashMap::new(),
            output: Vec::new(),
            output_bytes: 0,
            events: Vec::new(),
            pending_output: Vec::new(),
            last: None,
            calls: Vec::new(),
            in_syscall: None,
            mem_errors: Vec::new(),
            started: Instant::now(),
            timed_out,
            finished,
            last_mem: None,
            maps: Vec::new(),
        }
    }

    fn run(mut self, trace: &mut Trace) {
        let end = self.drive();
        self.finish(end, trace);
    }

    fn wait(&self) -> nix::Result<WaitStatus> {
        waitpid(self.pid, None)
    }

    fn drive(&mut self) -> End {
        let _ = ptrace::setoptions(
            self.pid,
            ptrace::Options::PTRACE_O_TRACESYSGOOD | ptrace::Options::PTRACE_O_EXITKILL,
        );
        self.maps = self.tracee.maps();
        let main_idx = self.debug.functions.iter().position(|f| f.name == "main").unwrap();
        let main = &self.debug.functions[main_idx];
        if let Err(end) = self.run_until(main.body_start) {
            return end;
        }
        self.maps = self.tracee.maps();
        let regs = match Regs::get(self.pid) {
            Ok(r) => r,
            Err(_) => return End::Truncated(TruncatedReason::Time),
        };
        let cfa = arch::cfa_of(regs.fp());
        let ret_addr = self.tracee.read_u64(arch::return_address_slot(regs.fp())).unwrap_or(0);
        self.calls.push(CallFrame {
            func: main_idx,
            ret_addr,
            cfa,
            poisoned: true,
        });
        memory::poison_locals(self.debug, &self.tracee, main_idx, cfa);
        let line = self
            .debug
            .stmt_row_at(regs.pc())
            .map(|r| r.line)
            .unwrap_or(main.decl_line);
        self.record(&regs, line, None);

        loop {
            if let Some(end) = self.limit_reached() {
                return end;
            }
            if ptrace::step(self.pid, None).is_err() {
                return End::Truncated(TruncatedReason::Time);
            }
            match self.wait() {
                Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {}
                Ok(WaitStatus::Stopped(_, sig)) => return self.deliver_fatal(sig),
                Ok(status) => return End::Exited(status),
                Err(_) => return End::Truncated(TruncatedReason::Time),
            }
            let Ok(regs) = Regs::get(self.pid) else {
                return End::Truncated(TruncatedReason::Time);
            };
            let pc = regs.pc();

            // ¿Acaba de ejecutar el ret de una función del usuario?
            if let Some(top) = self.calls.last()
                && pc == top.ret_addr
                && regs.sp() == top.cfa
            {
                let f = self.calls.pop().unwrap();
                self.return_event(f.func, &regs);
                if self.calls.is_empty() {
                    return self.run_to_exit();
                }
                continue;
            }

            match self.debug.functions.iter().position(|f| pc >= f.low && pc < f.high) {
                Some(fi) => {
                    let f = &self.debug.functions[fi];
                    if pc == f.low {
                        let ret_addr = self.tracee.read_u64(regs.sp()).unwrap_or(0);
                        self.calls.push(CallFrame {
                            func: fi,
                            ret_addr,
                            cfa: regs.sp() + 8,
                            poisoned: false,
                        });
                        continue;
                    }
                    if pc < f.body_start {
                        continue;
                    }
                    let Some(row) = self.debug.stmt_row_at(pc) else {
                        continue;
                    };
                    let cfa = arch::cfa_of(regs.fp());
                    let last = self.last.clone().unwrap();
                    if cfa == last.cfa && row.line == last.line && pc > last.pc {
                        continue;
                    }
                    if let Some(top) = self.calls.last_mut()
                        && top.cfa == cfa
                        && !top.poisoned
                    {
                        top.poisoned = true;
                        memory::poison_locals(self.debug, &self.tracee, fi, cfa);
                    }
                    self.record(&regs, row.line, Some(&last));
                }
                None => {
                    let ret_addr = self.tracee.read_u64(regs.sp()).unwrap_or(0);
                    if self.debug.function_at(ret_addr).is_some() {
                        if let Err(end) = self.library_call(regs, ret_addr) {
                            return end;
                        }
                    } else {
                        return self.run_to_exit();
                    }
                }
            }
        }
    }

    fn limit_reached(&self) -> Option<End> {
        if self.steps.len() as u64 >= self.opts.limits.max_steps {
            return Some(End::Truncated(TruncatedReason::Steps));
        }
        if self.started.elapsed() > Duration::from_millis(self.opts.limits.wall_time_ms)
            || self.timed_out.load(Ordering::SeqCst)
        {
            return Some(End::Truncated(TruncatedReason::Time));
        }
        if self.output_bytes > self.opts.limits.output_bytes_per_process {
            return Some(End::Truncated(TruncatedReason::Output));
        }
        None
    }

    /// Señal que el programa no maneja (SIGSEGV, SIGFPE, SIGABRT…): se entrega y el proceso muere.
    fn deliver_fatal(&mut self, sig: Signal) -> End {
        if sig == Signal::SIGSEGV {
            let addr = ptrace::getsiginfo(self.pid)
                .map(|si| unsafe { si.si_addr() } as u64)
                .unwrap_or(0);
            self.events.push(Event::MemError {
                pid: VPID,
                tid: VPID,
                kind: MemErrorKind::Segfault,
                addr: hex(addr),
            });
            self.mem_errors.push(MemErrorRef {
                t: self.t + 1,
                pid: VPID,
                tid: VPID,
                kind: MemErrorKind::Segfault,
                addr: hex(addr),
            });
        }
        let _ = ptrace::cont(self.pid, sig);
        loop {
            match self.wait() {
                Ok(WaitStatus::Stopped(_, s)) => {
                    let _ = ptrace::cont(self.pid, s);
                }
                Ok(status) => return End::Exited(status),
                Err(_) => return End::Truncated(TruncatedReason::Time),
            }
        }
    }

    /// Continúa (con paradas en syscalls) hasta llegar a `addr` con un breakpoint temporal.
    fn run_until(&mut self, addr: u64) -> Result<(), End> {
        let orig = self.tracee.read(addr, 1).ok_or(End::Truncated(TruncatedReason::Time))?;
        self.tracee.write(addr, &[arch::BREAKPOINT]);
        let mut sig: Option<Signal> = None;
        loop {
            if ptrace::syscall(self.pid, sig.take()).is_err() {
                return Err(End::Truncated(TruncatedReason::Time));
            }
            match self.wait() {
                Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {
                    let mut regs = Regs::get(self.pid).map_err(|_| End::Truncated(TruncatedReason::Time))?;
                    if regs.pc() == arch::pc_after_breakpoint(addr) {
                        self.tracee.write(addr, &orig);
                        regs.set_pc(addr);
                        let _ = regs.set(self.pid);
                        return Ok(());
                    }
                }
                Ok(WaitStatus::PtraceSyscall(_)) => self.syscall_stop()?,
                Ok(WaitStatus::Stopped(_, s)) => {
                    if s == Signal::SIGSEGV || s == Signal::SIGABRT || s == Signal::SIGFPE {
                        self.tracee.write(addr, &orig);
                        return Err(self.deliver_fatal(s));
                    }
                    sig = Some(s);
                }
                Ok(status) => return Err(End::Exited(status)),
                Err(_) => return Err(End::Truncated(TruncatedReason::Time)),
            }
            if self.timed_out.load(Ordering::SeqCst) {
                return Err(End::Truncated(TruncatedReason::Time));
            }
        }
    }

    fn run_to_exit(&mut self) -> End {
        let mut sig: Option<Signal> = None;
        loop {
            if ptrace::syscall(self.pid, sig.take()).is_err() {
                return End::Truncated(TruncatedReason::Time);
            }
            match self.wait() {
                Ok(WaitStatus::PtraceSyscall(_)) => {
                    if let Err(end) = self.syscall_stop() {
                        return end;
                    }
                }
                Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {}
                Ok(WaitStatus::Stopped(_, s)) => sig = Some(s),
                Ok(status) => return End::Exited(status),
                Err(_) => return End::Truncated(TruncatedReason::Time),
            }
        }
    }

    fn library_call(&mut self, entry: Regs, ret_addr: u64) -> Result<(), End> {
        let args = [entry.arg(0), entry.arg(1), entry.arg(2)];
        // Atraviesa el stub de la PLT hasta llegar a la función real.
        let mut pc = entry.pc();
        for _ in 0..8 {
            let in_exe = self
                .maps
                .iter()
                .any(|m| pc >= m.start && pc < m.end && m.path == self.binary_path);
            if !in_exe {
                break;
            }
            let _ = ptrace::step(self.pid, None);
            match self.wait() {
                Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {}
                Ok(status) => return Err(End::Exited(status)),
                Err(_) => return Err(End::Truncated(TruncatedReason::Time)),
            }
            pc = Regs::get(self.pid).map(|r| r.pc()).unwrap_or(0);
        }
        let name = match self.syms.resolve(&self.maps, pc) {
            Some(n) => n,
            None => {
                self.maps = self.tracee.maps();
                self.syms.resolve(&self.maps, pc).unwrap_or_else(|| "?".into())
            }
        };
        let summary = call_summary(&name, &args, &self.tracee);
        self.events.push(Event::Call {
            func: name.clone(),
            summary,
        });
        self.run_until(ret_addr)?;
        if matches!(
            name.as_str(),
            "malloc" | "calloc" | "realloc" | "reallocarray" | "free" | "strdup" | "strndup" | "mmap" | "sbrk" | "brk"
        ) {
            self.maps = self.tracee.maps();
        }
        let regs = Regs::get(self.pid).map_err(|_| End::Truncated(TruncatedReason::Time))?;
        self.after_library_call(&name, args, regs.ret());
        Ok(())
    }

    fn line_now(&self) -> u32 {
        self.last.as_ref().map(|s| s.line).unwrap_or(0)
    }

    fn after_library_call(&mut self, name: &str, args: [u64; 3], ret: u64) {
        let t = self.t + 1;
        let line = self.line_now();
        let alloc = |s: &mut Self, fname: &str, addr: u64, size: u64, poison: bool| {
            if addr == 0 {
                s.events.push(Event::Malloc {
                    pid: VPID,
                    func: fname.into(),
                    addr: None,
                    size,
                    old_addr: None,
                });
                return;
            }
            s.heap.alloc(addr, size, t, line);
            if poison && size > 0 {
                s.tracee.write(addr, &vec![memory::POISON; size.min(1 << 16) as usize]);
            }
            s.events.push(Event::Malloc {
                pid: VPID,
                func: fname.into(),
                addr: Some(hex(addr)),
                size,
                old_addr: None,
            });
        };
        match name {
            "malloc" => alloc(self, name, ret, args[0], true),
            "calloc" => alloc(self, name, ret, args[0].saturating_mul(args[1]), false),
            "strdup" | "strndup" => {
                let len = self
                    .tracee
                    .read_cstr(ret, 1 << 16)
                    .map(|b| b.len() as u64 + 1)
                    .unwrap_or(0);
                alloc(self, name, ret, len, false)
            }
            "realloc" | "reallocarray" => {
                let size = if name == "realloc" {
                    args[1]
                } else {
                    args[1].saturating_mul(args[2])
                };
                let old = args[0];
                let old_size = self
                    .heap
                    .blocks
                    .iter()
                    .find(|b| b.addr == old && b.freed_at.is_none())
                    .map(|b| b.size);
                if ret != 0 && old != 0 && ret != old {
                    let _ = self.heap.free(old, t);
                }
                if ret != 0 {
                    self.heap.alloc(ret, size, t, line);
                    if let Some(os) = old_size
                        && size > os
                    {
                        self.tracee
                            .write(ret + os, &vec![memory::POISON; (size - os).min(1 << 16) as usize]);
                    }
                }
                self.events.push(Event::Malloc {
                    pid: VPID,
                    func: name.into(),
                    addr: if ret == 0 { None } else { Some(hex(ret)) },
                    size,
                    old_addr: if old == 0 { None } else { Some(hex(old)) },
                });
            }
            "free" if args[0] != 0 => {
                let error = self.heap.free(args[0], t).err();
                if let Some(e) = error {
                    let kind = if e == FreeError::DoubleFree {
                        MemErrorKind::DoubleFree
                    } else {
                        MemErrorKind::InvalidFree
                    };
                    self.mem_errors.push(MemErrorRef {
                        t,
                        pid: VPID,
                        tid: VPID,
                        kind,
                        addr: hex(args[0]),
                    });
                }
                self.events.push(Event::Free {
                    pid: VPID,
                    addr: hex(args[0]),
                    error,
                });
            }
            _ => {}
        }
    }

    fn syscall_stop(&mut self) -> Result<(), End> {
        let regs = Regs::get(self.pid).map_err(|_| End::Truncated(TruncatedReason::Time))?;
        match self.in_syscall.take() {
            None => {
                let nr = regs.syscall_nr();
                let args = [regs.syscall_arg(0), regs.syscall_arg(1), regs.syscall_arg(2)];
                // stdin agotado y todavía abierto: en vez de bloquear, se pide más entrada.
                if nr == arch::SYS_READ
                    && args[0] == 0
                    && !self.opts.stdin_eof
                    && self.stdin_consumed >= self.stdin_len()
                {
                    return Err(End::AwaitingInput);
                }
                self.in_syscall = Some((nr, args));
            }
            Some((nr, args)) => {
                let ret = regs.ret() as i64;
                if ret < 0 {
                    return Ok(());
                }
                match nr {
                    arch::SYS_WRITE if args[0] == 1 || args[0] == 2 => {
                        let bytes = self.tracee.read(args[1], ret as usize).unwrap_or_default();
                        self.terminal_write(args[0] as u32, &bytes);
                    }
                    arch::SYS_WRITEV if args[0] == 1 || args[0] == 2 => {
                        let mut bytes = Vec::new();
                        for i in 0..args[2].min(64) {
                            let base = self.tracee.read_u64(args[1] + i * 16).unwrap_or(0);
                            let len = self.tracee.read_u64(args[1] + i * 16 + 8).unwrap_or(0);
                            bytes.extend(self.tracee.read(base, len.min(1 << 16) as usize).unwrap_or_default());
                        }
                        bytes.truncate(ret as usize);
                        self.terminal_write(args[0] as u32, &bytes);
                    }
                    arch::SYS_READ if args[0] == 0 => {
                        let bytes = self.tracee.read(args[1], ret as usize).unwrap_or_default();
                        self.stdin_consumed += ret as u64;
                        self.events.push(Event::Read {
                            pid: VPID,
                            tid: VPID,
                            fd: 0,
                            pipe: None,
                            stdin: true,
                            bytes: latin1(&bytes[..bytes.len().min(256)]),
                            n: ret as u64,
                            eof: ret == 0,
                            into: Some(hex(args[1])),
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn stdin_len(&self) -> u64 {
        self.opts.stdin.len().min(launch::STDIN_MAX) as u64
    }

    fn terminal_write(&mut self, fd: u32, bytes: &[u8]) {
        let stream = if fd == 2 { Stream::Stderr } else { Stream::Stdout };
        self.output_bytes += bytes.len() as u64;
        let text = latin1(bytes);
        self.events.push(Event::Write {
            pid: VPID,
            tid: VPID,
            fd,
            pipe: None,
            terminal: true,
            bytes: latin1(&bytes[..bytes.len().min(256)]),
            n: bytes.len() as u64,
            epipe: false,
        });
        self.pending_output.push((fd, stream, text));
    }

    fn return_event(&mut self, func: usize, regs: &Regs) {
        let f = &self.debug.functions[func];
        let value = f.ret.and_then(|ty| {
            let st = self.debug.strip(ty);
            let size = self.debug.size_of(st);
            if size == 0 || size > 8 {
                return None;
            }
            let bytes = regs.ret().to_le_bytes();
            let mut reader = Reader::new(
                self.debug,
                &self.tracee,
                &self.heap,
                self.binary_path.clone(),
                self.maps.clone(),
            );
            Some(reader.from_bytes(st, &bytes[..size as usize]))
        });
        self.events.push(Event::Return {
            func: f.name.clone(),
            value,
        });
    }

    fn snapshot_id(&mut self, snap: MemorySnapshot) -> String {
        let key = serde_json::to_string(&snap).unwrap();
        if let Some(id) = self.snapshot_ids.get(&key) {
            return id.clone();
        }
        let id = format!("m{}", self.snapshot_ids.len());
        self.snapshot_ids.insert(key, id.clone());
        self.snapshots.insert(id.clone(), snap);
        id
    }

    fn take_output(&mut self, t: u64) {
        for (fd, stream, bytes) in self.pending_output.drain(..) {
            self.output.push(OutputChunk {
                t,
                pid: VPID,
                fd,
                stream,
                bytes,
            });
        }
    }

    fn std_fds() -> BTreeMap<Fdnum, Fd> {
        BTreeMap::from([
            (0, Fd::Stdin { cloexec: None }),
            (1, Fd::Terminal { cloexec: None }),
            (2, Fd::Terminal { cloexec: None }),
        ])
    }

    fn stdin_state(&self) -> StdinState {
        StdinState {
            size: self.stdin_len(),
            consumed: self.stdin_consumed,
            eof: self.opts.stdin_eof,
        }
    }

    fn record(&mut self, regs: &Regs, line: u32, previous: Option<&Stop>) {
        let t = if self.steps.is_empty() { 0 } else { self.t + 1 };
        let frames = memory::unwind(self.debug, &self.tracee, regs, line);
        let func = frames
            .first()
            .map(|f| self.debug.functions[f.func].name.clone())
            .unwrap_or_default();
        let snap = {
            let mut reader = Reader::new(
                self.debug,
                &self.tracee,
                &self.heap,
                self.binary_path.clone(),
                self.maps.clone(),
            );
            reader.snapshot(&frames, VPID, t)
        };
        let mem = self.snapshot_id(snap);
        self.last_mem = Some(mem.clone());
        // En t = 0 nadie avanzó todavía: el hilo está listo, no ejecutando.
        let (thread_state, process_state) = if t == 0 {
            (ThreadState::Ready, ProcessState::Ready)
        } else {
            (ThreadState::Running, ProcessState::Running)
        };
        let thread = Thread {
            tid: VPID,
            main: true,
            state: thread_state,
            line: Some(line),
            func: Some(func.clone()),
            in_call: None,
            blocked_on: None,
            start: None,
            holds: Vec::new(),
            in_handler: None,
            retval: None,
        };
        let process = self.process(process_state, thread, Some(mem), None);
        self.push_step(t, previous, process);
        self.last = Some(Stop {
            pc: regs.pc(),
            line,
            cfa: arch::cfa_of(regs.fp()),
            func,
        });
    }

    fn process(&self, state: ProcessState, thread: Thread, mem: Option<String>, exit: Option<ExitStatus>) -> Process {
        Process {
            pid: VPID,
            ppid: None,
            pgid: VPID,
            state,
            created_at: 0,
            image: ProcessImage::User {
                path: BINARY_NAME.into(),
            },
            exit: exit.clone(),
            fds: if exit.is_some() {
                BTreeMap::new()
            } else {
                Self::std_fds()
            },
            signals: ProcessSignals {
                mask: Vec::new(),
                pending: Vec::new(),
                actions: BTreeMap::new(),
            },
            threads: vec![thread],
            mem,
        }
    }

    fn push_step(&mut self, t: u64, previous: Option<&Stop>, process: Process) {
        self.take_output(t);
        let actor = (t > 0).then_some(TaskRef { pid: VPID, tid: VPID });
        self.steps.push(Step {
            t,
            actor,
            executed: previous.map(|p| ExecutedLine {
                line: p.line,
                func: p.func.clone(),
            }),
            choices: actor.into_iter().collect(),
            clock: 0,
            events: std::mem::take(&mut self.events),
            processes: vec![process],
            pipes: Vec::new(),
            stdin: self.stdin_state(),
            signals: Vec::new(),
            timers: Vec::new(),
            sync: Vec::new(),
        });
        self.t = t;
    }

    fn finish(mut self, end: End, trace: &mut Trace) {
        let _ = nix::sys::signal::kill(self.pid, Signal::SIGKILL);
        let _ = waitpid(self.pid, None);
        self.finished.store(true, Ordering::SeqCst);
        let previous = self.last.clone();
        let t = self.t + 1;
        match end {
            End::Exited(status) => {
                let (exit, event, outcome) = match status {
                    WaitStatus::Exited(_, code) => (
                        ExitStatus::Code { code },
                        Event::Exit {
                            pid: VPID,
                            tid: None,
                            scope: ExitScope::Process,
                            code: Some(code),
                            signal: None,
                            retval: None,
                        },
                        Outcome::Exited { code },
                    ),
                    WaitStatus::Signaled(_, sig, core) => (
                        ExitStatus::Signal {
                            signal: sig.as_str().into(),
                            core,
                        },
                        Event::Exit {
                            pid: VPID,
                            tid: None,
                            scope: ExitScope::Process,
                            code: None,
                            signal: Some(sig.as_str().into()),
                            retval: None,
                        },
                        Outcome::Signaled {
                            signal: sig.as_str().into(),
                        },
                    ),
                    _ => (
                        ExitStatus::Code { code: 0 },
                        Event::Truncated {
                            reason: "estado inesperado".into(),
                        },
                        Outcome::Truncated,
                    ),
                };
                // Cuando el vigilante mató el proceso, es un corte por tiempo y no una salida.
                if self.timed_out.load(Ordering::SeqCst) {
                    return self.truncate(TruncatedReason::Time, trace);
                }
                self.events.push(event);
                let mem = self.final_memory(t);
                let thread = exited_thread();
                let process = self.process(ProcessState::Zombie, thread, mem, Some(exit));
                self.push_step(t, previous.as_ref(), process);
                trace.outcome = outcome;
                for b in self.heap.live() {
                    trace.summary.leaks.push(Leak {
                        pid: VPID,
                        addr: hex(b.addr),
                        size: b.size,
                        ty: self.heap_type(b.addr),
                        alloc_at: b.alloc_at,
                        alloc_line: b.alloc_line,
                    });
                }
            }
            End::AwaitingInput => {
                self.events.push(Event::StdinNeeded { pid: VPID, tid: VPID });
                self.events.push(Event::Block {
                    pid: VPID,
                    tid: VPID,
                    reason: BlockReason::Read {
                        fd: 0,
                        pipe: None,
                        stdin: true,
                    },
                });
                let mut thread = exited_thread();
                thread.state = ThreadState::Blocked;
                thread.line = previous.as_ref().map(|p| p.line);
                thread.func = previous.as_ref().map(|p| p.func.clone());
                thread.in_call = Some("read".into());
                thread.blocked_on = Some(BlockReason::Read {
                    fd: 0,
                    pipe: None,
                    stdin: true,
                });
                let mem = self.last_mem.clone();
                let process = self.process(ProcessState::Blocked, thread, mem, None);
                self.push_step(t, previous.as_ref(), process);
                trace.outcome = Outcome::AwaitingInput { pid: VPID, tid: VPID };
            }
            End::Truncated(reason) => return self.truncate(reason, trace),
        }
        self.write_into(trace);
    }

    fn truncate(mut self, reason: TruncatedReason, trace: &mut Trace) {
        let text = match reason {
            TruncatedReason::Steps => format!("se alcanzó el límite de {} pasos", self.opts.limits.max_steps),
            TruncatedReason::Time => format!("se alcanzó el límite de {} s", self.opts.limits.wall_time_ms / 1000),
            TruncatedReason::Output => "el programa escribió más salida de la permitida".to_string(),
            _ => "se alcanzó un límite".to_string(),
        };
        self.take_output(self.t);
        if let Some(last) = self.steps.last_mut() {
            last.events.push(Event::Truncated { reason: text });
        }
        trace.outcome = Outcome::Truncated;
        trace.truncated = true;
        trace.truncated_reason = Some(reason);
        self.write_into(trace);
    }

    fn heap_type(&self, addr: u64) -> Option<String> {
        let id = self.last_mem.as_ref()?;
        self.snapshots
            .get(id)?
            .heap
            .iter()
            .find(|b| b.addr == hex(addr))
            .and_then(|b| b.ty.clone())
    }

    /// Memoria al salir: sin pilas, pero con globales y heap para ver las fugas.
    fn final_memory(&mut self, t: u64) -> Option<String> {
        let id = self.last_mem.clone()?;
        let mut snap = self.snapshots.get(&id)?.clone();
        snap.stacks = BTreeMap::from([(VPID, Vec::new())]);
        let visible: Vec<String> = self.heap.visible(t).map(|b| hex(b.addr)).collect();
        snap.heap.retain(|b| visible.contains(&b.addr));
        Some(self.snapshot_id(snap))
    }

    fn write_into(self, trace: &mut Trace) {
        trace.steps = self.steps;
        trace.snapshots = self.snapshots;
        trace.output = self.output;
        trace.summary.mem_errors = self.mem_errors;
    }
}

fn exited_thread() -> Thread {
    Thread {
        tid: VPID,
        main: true,
        state: ThreadState::Exited,
        line: None,
        func: None,
        in_call: None,
        blocked_on: None,
        start: None,
        holds: Vec::new(),
        in_handler: None,
        retval: None,
    }
}

fn call_summary(name: &str, args: &[u64; 3], tracee: &Tracee) -> Option<String> {
    Some(match name {
        "malloc" => format!("malloc({})", args[0]),
        "calloc" => format!("calloc({}, {})", args[0], args[1]),
        "realloc" => format!("realloc({:#x}, {})", args[0], args[1]),
        "free" => format!("free({:#x})", args[0]),
        "strlen" | "puts" | "strdup" => {
            let s = tracee.read_cstr(args[0], 40).unwrap_or_default();
            format!("{name}(\"{}\")", String::from_utf8_lossy(&s).escape_default())
        }
        _ => return None,
    })
}
