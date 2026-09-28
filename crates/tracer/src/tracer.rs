//! Motor de trazas: un planificador determinista sobre ptrace. En cada paso avanza una sola tarea
//! y el resto queda detenida, así el orden de la traza lo decide el tracer y no el kernel.
//!
//! Dentro del código del usuario se avanza por línea con singlestep; cada llamada a una biblioteca
//! es atómica y corre hasta su dirección de retorno (sin singlestep: el loader y libc harían
//! inviable el tiempo, ver la propuesta). Un programa ajeno cargado con exec es una caja negra:
//! solo se siguen sus syscalls.

use crate::arch::{self, Regs};
use crate::compile::{self, BINARY_NAME, SOURCE_NAME};
use crate::dwarf::DebugInfo;
use crate::heap::Heap;
use crate::launch;
use crate::limits::Limits;
use crate::memory::{self, Reader, hex, latin1};
use crate::process::{Mapping, Tracee};
use crate::syms::Symbols;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use trace_model::*;

/// PID del proceso inicial en la traza; sus descendientes reciben 1001, 1002… en orden de
/// creación. El programa ve estos mismos números en getpid, fork y wait.
pub const VPID: u32 = 1000;
/// El nodo virtual `init (1)`, que adopta a los huérfanos.
const INIT: u32 = 1;
/// PID que no puede existir: reemplaza a los PIDs desconocidos en kill y wait para que el programa
/// nunca alcance procesos ajenos a la traza.
const NO_PID: i64 = i32::MAX as i64;
/// Lo que cuesta un paso en el reloj virtual. Sin este costo, un proceso que espera activamente
/// (un bucle con waitpid y WNOHANG) no dejaría despertar nunca a uno que duerme.
const STEP_MS: u64 = 1;

pub struct Options {
    pub source: String,
    pub stdin: Vec<u8>,
    pub stdin_eof: bool,
    pub limits: Limits,
}

pub fn run(opts: &Options) -> Trace {
    // Los huérfanos pasan al tracer y no al init del sistema: así el tracer los recoge.
    unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) };
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
    let started = matches!(waitpid(launched.pid, None), Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)));
    let tracee = started.then(|| Tracee::attach(launched.pid).ok()).flatten();
    let Some(tracee) = tracee else {
        let _ = nix::sys::signal::kill(launched.pid, Signal::SIGKILL);
        let _ = waitpid(launched.pid, None);
        trace.compile.diagnostics.push(Diagnostic {
            line: 0,
            col: 0,
            severity: Severity::Error,
            message: "el programa no pudo iniciarse".into(),
        });
        trace.outcome = Outcome::CompileError;
        return trace;
    };
    let engine = Engine::new(&debug, opts, launched.pid, tracee, launched.stdin_writer, binary_path);
    engine.run(&mut trace);
    trace
}

/// Por qué terminó el recorrido completo.
enum End {
    /// Todos los procesos terminaron.
    Done,
    AwaitingInput(u32),
    Deadlock(Vec<TaskRef>),
    Truncated(TruncatedReason),
}

/// Por qué un proceso dejó de avanzar antes de llegar a su siguiente línea.
enum Halt {
    Blocked(BlockReason),
    Exited(WaitStatus),
    /// Cargó otro programa: el paso termina en el exec.
    Exec,
    End(End),
}

type Res<T> = Result<T, Halt>;

/// ptrace falló: el proceso desapareció, en la práctica porque el vigilante lo mató.
fn lost() -> Halt {
    Halt::End(End::Truncated(TruncatedReason::Time))
}

#[derive(Clone)]
struct Stop {
    pc: u64,
    line: u32,
    cfa: u64,
    func: String,
}

#[derive(Clone)]
struct CallFrame {
    func: usize,
    ret_addr: u64,
    cfa: u64,
    poisoned: bool,
}

/// Llamada a biblioteca en curso: el breakpoint en `ret_addr` marca su fin.
#[derive(Clone)]
struct LibCall {
    name: String,
    args: [u64; 3],
    ret_addr: u64,
    orig: u8,
    /// Registros al entrar: con ellos se lee la pila del llamador mientras la llamada no vuelve.
    regs: Regs,
}

/// Dónde está detenido el hilo del proceso.
#[derive(Clone)]
enum At {
    /// En una línea del código del usuario.
    User,
    /// Dentro de una llamada a biblioteca (un hijo recién creado, o un proceso bloqueado en wait).
    Lib(Box<LibCall>),
    /// main ya volvió: corre libc hasta el exit.
    Exiting,
    Blackbox,
}

#[derive(Clone, PartialEq)]
enum PState {
    Ready,
    Blocked(BlockReason),
    Zombie,
    Reaped,
}

struct Proc {
    vpid: u32,
    pid: Pid,
    ppid: Option<u32>,
    pgid: u32,
    created_at: u64,
    tracee: Tracee,
    heap: Heap,
    /// Mapa de memoria: solo cambia dentro de libc (malloc, mmap), así que se relee después de
    /// esas llamadas y no en cada paso.
    maps: Vec<Mapping>,
    image: ProcessImage,
    at: At,
    state: PState,
    exit: Option<ExitStatus>,
    last: Option<Stop>,
    calls: Vec<CallFrame>,
    /// Syscall en curso (número y argumentos de la entrada), para interpretar su salida.
    sys: Option<(u64, [u64; 6])>,
    /// Señal no fatal (SIGCHLD…) que se reinyecta al reanudar.
    sig: Option<Signal>,
    exec_args: Option<(String, Vec<String>)>,
    /// Hijo de vfork: comparte la memoria del padre, que queda detenido hasta el exec o el exit.
    vfork_parent: Option<u32>,
    /// Padre de vfork: el hijo que lo retiene.
    vfork_child: Option<u32>,
    mem: Option<String>,
    output_bytes: u64,
}

impl Proc {
    fn alive(&self) -> bool {
        !matches!(self.state, PState::Zombie | PState::Reaped)
    }

    fn user_image(&self) -> bool {
        matches!(self.image, ProcessImage::User { .. })
    }
}

struct Engine<'a> {
    debug: &'a DebugInfo,
    opts: &'a Options,
    syms: Symbols,
    binary_path: String,
    _stdin_writer: Option<File>,
    stdin_consumed: u64,
    procs: Vec<Proc>,
    t: u64,
    clock: u64,
    cursor: usize,
    steps: Vec<Step>,
    snapshots: BTreeMap<String, MemorySnapshot>,
    snapshot_ids: HashMap<String, String>,
    output: Vec<OutputChunk>,
    events: Vec<Event>,
    pending_output: Vec<(u32, u32, Stream, String)>,
    mem_errors: Vec<MemErrorRef>,
    leaks: Vec<Leak>,
    started: Instant,
    timed_out: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    /// PIDs reales vivos: el vigilante los mata a todos al vencer el tiempo.
    live: Arc<Mutex<Vec<Pid>>>,
}

impl<'a> Engine<'a> {
    fn new(
        debug: &'a DebugInfo,
        opts: &'a Options,
        pid: Pid,
        tracee: Tracee,
        stdin_writer: Option<File>,
        binary_path: String,
    ) -> Self {
        let timed_out = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let live = Arc::new(Mutex::new(vec![pid]));
        // Un programa bloqueado en el kernel no devuelve el control: el vigilante lo mata al vencer el tiempo.
        {
            let flag = timed_out.clone();
            let ms = opts.limits.wall_time_ms;
            let done = finished.clone();
            let live = live.clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_millis(ms);
                while Instant::now() < deadline {
                    if done.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                // Con la traza terminada esos PIDs pueden pertenecer a otros procesos: no se tocan.
                let live = live.lock().unwrap();
                if !done.load(Ordering::SeqCst) {
                    flag.store(true, Ordering::SeqCst);
                    for p in live.iter() {
                        let _ = nix::sys::signal::kill(*p, Signal::SIGKILL);
                    }
                }
            });
        }
        let root = Proc {
            vpid: VPID,
            pid,
            ppid: None,
            pgid: VPID,
            created_at: 0,
            maps: tracee.maps(),
            tracee,
            heap: Heap::default(),
            image: ProcessImage::User {
                path: BINARY_NAME.into(),
            },
            at: At::User,
            state: PState::Ready,
            exit: None,
            last: None,
            calls: Vec::new(),
            sys: None,
            sig: None,
            exec_args: None,
            vfork_parent: None,
            vfork_child: None,
            mem: None,
            output_bytes: 0,
        };
        Engine {
            debug,
            opts,
            syms: Symbols::default(),
            binary_path,
            _stdin_writer: stdin_writer,
            stdin_consumed: 0,
            procs: vec![root],
            t: 0,
            clock: 0,
            cursor: 0,
            steps: Vec::new(),
            snapshots: BTreeMap::new(),
            snapshot_ids: HashMap::new(),
            output: Vec::new(),
            events: Vec::new(),
            pending_output: Vec::new(),
            mem_errors: Vec::new(),
            leaks: Vec::new(),
            started: Instant::now(),
            timed_out,
            finished,
            live,
        }
    }

    fn run(mut self, trace: &mut Trace) {
        let end = self.drive();
        self.finish(end, trace);
    }

    // ---------- planificación ----------

    fn drive(&mut self) -> End {
        let options = ptrace::Options::PTRACE_O_TRACESYSGOOD
            | ptrace::Options::PTRACE_O_EXITKILL
            | ptrace::Options::PTRACE_O_TRACEFORK
            | ptrace::Options::PTRACE_O_TRACEVFORK
            | ptrace::Options::PTRACE_O_TRACEEXEC;
        let _ = ptrace::setoptions(self.procs[0].pid, options);
        if let Err(halt) = self.start_root() {
            return match halt {
                Halt::End(end) => end,
                _ => End::Truncated(TruncatedReason::Time),
            };
        }
        loop {
            if let Some(end) = self.limit_reached() {
                return end;
            }
            if self.procs.iter().all(|p| !p.alive()) {
                return End::Done;
            }
            self.wake_sleepers();
            let choices = self.runnable();
            let Some(i) = self.pick() else {
                if let Some(end) = self.idle() {
                    return end;
                }
                continue;
            };
            let executed = self.procs[i]
                .last
                .as_ref()
                .filter(|_| self.procs[i].user_image())
                .map(|s| ExecutedLine {
                    line: s.line,
                    func: s.func.clone(),
                });
            self.clock += STEP_MS;
            match self.advance(i) {
                Ok(Some((regs, line))) => self.capture(i, &regs, line),
                Ok(None) | Err(Halt::Exec) => {}
                Err(Halt::Blocked(reason)) => self.block(i, reason),
                Err(Halt::Exited(status)) => self.on_exit(i, status),
                Err(Halt::End(end)) => return end,
            }
            self.commit(Some(i), executed, choices);
            self.cursor = i;
        }
    }

    /// Lleva el proceso inicial hasta la primera línea de main: ese es el paso 0.
    fn start_root(&mut self) -> Res<()> {
        let main_idx = self.debug.functions.iter().position(|f| f.name == "main").unwrap();
        let main = &self.debug.functions[main_idx];
        let orig = self.procs[0].tracee.read(main.body_start, 1).ok_or_else(lost)?;
        self.run_to(0, main.body_start, orig[0])?;
        let p = &mut self.procs[0];
        p.maps = p.tracee.maps();
        let regs = Regs::get(p.pid).map_err(|_| lost())?;
        let cfa = arch::cfa_of(regs.fp());
        let ret_addr = p.tracee.read_u64(arch::return_address_slot(regs.fp())).unwrap_or(0);
        p.calls.push(CallFrame {
            func: main_idx,
            ret_addr,
            cfa,
            poisoned: true,
        });
        memory::poison_locals(self.debug, &p.tracee, main_idx, cfa);
        let line = self
            .debug
            .stmt_row_at(regs.pc())
            .map(|r| r.line)
            .unwrap_or(main.decl_line);
        self.capture(0, &regs, line);
        self.commit(None, None, Vec::new());
        Ok(())
    }

    fn runnable(&self) -> Vec<TaskRef> {
        self.procs
            .iter()
            .filter(|p| p.state == PState::Ready)
            .map(|p| TaskRef {
                pid: p.vpid,
                tid: p.vpid,
            })
            .collect()
    }

    /// Round-robin: el siguiente proceso listo después del último que avanzó.
    fn pick(&self) -> Option<usize> {
        let n = self.procs.len();
        let first = if self.steps.len() <= 1 { 0 } else { self.cursor + 1 };
        (0..n)
            .map(|k| (first + k) % n)
            .find(|&i| self.procs[i].state == PState::Ready)
    }

    /// Nadie puede avanzar: el reloj salta al próximo despertar, o la traza termina.
    fn idle(&mut self) -> Option<End> {
        let wake = self
            .procs
            .iter()
            .filter_map(|p| match p.state {
                PState::Blocked(BlockReason::Sleep { until }) => Some(until),
                _ => None,
            })
            .min();
        if let Some(until) = wake {
            self.clock = self.clock.max(until);
            self.wake_sleepers();
            self.commit(None, None, Vec::new());
            return None;
        }
        if let Some(p) = self
            .procs
            .iter()
            .find(|p| matches!(&p.state, PState::Blocked(BlockReason::Read { fd: 0, stdin: true, .. })))
        {
            return Some(End::AwaitingInput(p.vpid));
        }
        let tasks = self
            .procs
            .iter()
            .filter(|p| p.alive())
            .map(|p| TaskRef {
                pid: p.vpid,
                tid: p.vpid,
            })
            .collect();
        Some(End::Deadlock(tasks))
    }

    fn wake_sleepers(&mut self) {
        for i in 0..self.procs.len() {
            if matches!(self.procs[i].state, PState::Blocked(BlockReason::Sleep { until }) if until <= self.clock) {
                self.unblock(i);
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
        if self
            .procs
            .iter()
            .any(|p| p.output_bytes > self.opts.limits.output_bytes_per_process)
        {
            return Some(End::Truncated(TruncatedReason::Output));
        }
        None
    }

    fn index_of(&self, vpid: u32) -> Option<usize> {
        self.procs.iter().position(|p| p.vpid == vpid)
    }

    fn vpid_of_real(&self, real: i64) -> Option<u32> {
        self.procs
            .iter()
            .find(|p| p.pid.as_raw() as i64 == real)
            .map(|p| p.vpid)
    }

    /// PID virtual (o -pgid virtual) que el programa pasa al kernel → PID real.
    fn real_target(&self, v: i64) -> i64 {
        let real = |v: i64| {
            self.procs
                .iter()
                .find(|p| p.vpid as i64 == v && p.state != PState::Reaped)
                .map(|p| p.pid.as_raw() as i64)
                .unwrap_or(NO_PID)
        };
        match v {
            0 => 0,
            // "Todos los procesos" quedaría fuera de la traza: se limita al grupo del programa.
            -1 => -(self.procs[0].pid.as_raw() as i64),
            v if v > 0 => real(v),
            v => -real(-v),
        }
    }

    // ---------- avance de un proceso ----------

    fn wait(&mut self, i: usize) -> Res<WaitStatus> {
        let status = waitpid(self.procs[i].pid, Some(WaitPidFlag::__WALL)).map_err(|_| lost())?;
        if self.timed_out.load(Ordering::SeqCst) {
            return Err(lost());
        }
        match status {
            WaitStatus::Exited(..) | WaitStatus::Signaled(..) => Err(Halt::Exited(status)),
            s => Ok(s),
        }
    }

    fn resume_step(&mut self, i: usize) -> Res<()> {
        let p = &mut self.procs[i];
        ptrace::step(p.pid, p.sig.take()).map_err(|_| lost())
    }

    fn resume_syscall(&mut self, i: usize) -> Res<()> {
        let p = &mut self.procs[i];
        ptrace::syscall(p.pid, p.sig.take()).map_err(|_| lost())
    }

    /// Un paso del proceso `i`: hasta su siguiente línea (con sus registros), o hasta que se
    /// bloquee, termine o cargue otro programa. `None` = paso de una caja negra.
    fn advance(&mut self, i: usize) -> Res<Option<(Regs, u32)>> {
        match self.procs[i].at.clone() {
            At::Blackbox => {
                self.free_run(i, true)?;
                return Ok(None);
            }
            At::Exiting => {
                self.free_run(i, false)?;
                return Ok(None);
            }
            At::Lib(call) => {
                self.finish_call(i, *call)?;
                return self.step_lines(i, true).map(Some);
            }
            At::User => {}
        }
        self.step_lines(i, false).map(Some)
    }

    /// Singlestep hasta la siguiente línea. `check_first`: la posición actual ya puede ser una
    /// parada (una llamada a biblioteca que vuelve justo al inicio de la línea siguiente).
    fn step_lines(&mut self, i: usize, check_first: bool) -> Res<(Regs, u32)> {
        let debug = self.debug;
        let mut check_here = check_first;
        // Un salto hacia atrás dentro de la misma función (otra vuelta de un bucle escrito en una
        // sola línea) cuenta como un paso nuevo aunque la línea no cambie.
        let mut prev: Option<(usize, u64)> = None;
        let mut looped = false;
        loop {
            if !std::mem::take(&mut check_here) {
                self.resume_step(i)?;
                match self.wait(i)? {
                    WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
                    WaitStatus::Stopped(_, sig) => {
                        self.signal_stop(i, sig)?;
                        continue;
                    }
                    WaitStatus::PtraceEvent(_, _, ev) => {
                        self.ptrace_event(i, ev)?;
                        continue;
                    }
                    _ => continue,
                }
            }
            let regs = Regs::get(self.procs[i].pid).map_err(|_| lost())?;
            let pc = regs.pc();

            // ¿Acaba de ejecutar el ret de una función del usuario?
            if let Some(top) = self.procs[i].calls.last()
                && pc == top.ret_addr
                && regs.sp() == top.cfa
            {
                let f = self.procs[i].calls.pop().unwrap();
                self.return_event(i, f.func, &regs);
                if self.procs[i].calls.is_empty() {
                    self.procs[i].at = At::Exiting;
                    self.free_run(i, false)?;
                }
                continue;
            }

            match debug.functions.iter().position(|f| pc >= f.low && pc < f.high) {
                Some(fi) => {
                    let f = &debug.functions[fi];
                    let p = &mut self.procs[i];
                    if let Some((pf, ppc)) = prev.replace((fi, pc))
                        && pf == fi
                        && pc <= ppc
                    {
                        looped = true;
                    }
                    if pc == f.low {
                        let ret_addr = p.tracee.read_u64(regs.sp()).unwrap_or(0);
                        p.calls.push(CallFrame {
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
                    let Some(row) = debug.stmt_row_at(pc) else {
                        continue;
                    };
                    let cfa = arch::cfa_of(regs.fp());
                    if let Some(last) = &p.last
                        && cfa == last.cfa
                        && row.line == last.line
                        && pc > last.pc
                        && !looped
                    {
                        continue;
                    }
                    if let Some(top) = p.calls.last_mut()
                        && top.cfa == cfa
                        && !top.poisoned
                    {
                        top.poisoned = true;
                        memory::poison_locals(debug, &p.tracee, fi, cfa);
                    }
                    return Ok((regs, row.line));
                }
                None => {
                    let ret_addr = self.procs[i].tracee.read_u64(regs.sp()).unwrap_or(0);
                    if debug.function_at(ret_addr).is_some() {
                        self.library_call(i, regs, ret_addr)?;
                        check_here = true;
                    } else {
                        self.procs[i].at = At::Exiting;
                        self.free_run(i, false)?;
                    }
                }
            }
        }
    }

    /// Corre con paradas en syscalls. Con `one_write`, el paso termina tras cada escritura a la
    /// terminal (así la salida de una caja negra se ve de a poco); si no, hasta que el proceso muera.
    fn free_run(&mut self, i: usize, one_write: bool) -> Res<()> {
        loop {
            self.resume_syscall(i)?;
            match self.wait(i)? {
                WaitStatus::PtraceSyscall(_) => {
                    if self.syscall_stop(i)? && one_write {
                        return Ok(());
                    }
                }
                WaitStatus::PtraceEvent(_, _, ev) => self.ptrace_event(i, ev)?,
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
                WaitStatus::Stopped(_, sig) => self.signal_stop(i, sig)?,
                _ => {}
            }
        }
    }

    /// Continúa (con paradas en syscalls) hasta llegar a `addr`, donde pone un breakpoint.
    fn run_to(&mut self, i: usize, addr: u64, orig: u8) -> Res<()> {
        // Se reescribe siempre: un hijo de vfork que comparte la memoria pudo haberlo quitado.
        self.procs[i].tracee.write(addr, &[arch::BREAKPOINT]);
        loop {
            self.resume_syscall(i)?;
            match self.wait(i)? {
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                    let pid = self.procs[i].pid;
                    let mut regs = Regs::get(pid).map_err(|_| lost())?;
                    if regs.pc() == arch::pc_after_breakpoint(addr) {
                        self.procs[i].tracee.write(addr, &[orig]);
                        regs.set_pc(addr);
                        let _ = regs.set(pid);
                        return Ok(());
                    }
                }
                WaitStatus::PtraceSyscall(_) => {
                    self.syscall_stop(i)?;
                }
                WaitStatus::PtraceEvent(_, _, ev) => self.ptrace_event(i, ev)?,
                WaitStatus::Stopped(_, sig) => {
                    if is_fatal(sig) {
                        self.procs[i].tracee.write(addr, &[orig]);
                    }
                    self.signal_stop(i, sig)?;
                }
                _ => {}
            }
        }
    }

    /// Señal en espera de entrega. Las que por omisión se ignoran se reinyectan; el resto (SIGSEGV,
    /// SIGFPE, SIGABRT…) mata al proceso.
    fn signal_stop(&mut self, i: usize, sig: Signal) -> Res<()> {
        match sig {
            Signal::SIGCHLD | Signal::SIGWINCH | Signal::SIGURG | Signal::SIGCONT => {
                self.procs[i].sig = Some(sig);
                Ok(())
            }
            Signal::SIGSTOP | Signal::SIGTSTP | Signal::SIGTTIN | Signal::SIGTTOU => Ok(()),
            _ => Err(self.deliver_fatal(i, sig)),
        }
    }

    fn deliver_fatal(&mut self, i: usize, sig: Signal) -> Halt {
        let vpid = self.procs[i].vpid;
        if sig == Signal::SIGSEGV {
            let addr = ptrace::getsiginfo(self.procs[i].pid)
                .map(|si| unsafe { si.si_addr() } as u64)
                .unwrap_or(0);
            self.events.push(Event::MemError {
                pid: vpid,
                tid: vpid,
                kind: MemErrorKind::Segfault,
                addr: hex(addr),
            });
            self.mem_errors.push(MemErrorRef {
                t: self.t + 1,
                pid: vpid,
                tid: vpid,
                kind: MemErrorKind::Segfault,
                addr: hex(addr),
            });
        }
        let pid = self.procs[i].pid;
        let _ = ptrace::cont(pid, sig);
        loop {
            match self.wait(i) {
                Ok(WaitStatus::Stopped(_, s)) => {
                    let _ = ptrace::cont(pid, s);
                }
                Ok(_) => {
                    let _ = ptrace::cont(pid, None);
                }
                Err(halt) => return halt,
            }
        }
    }

    fn library_call(&mut self, i: usize, entry: Regs, ret_addr: u64) -> Res<()> {
        let args = [entry.arg(0), entry.arg(1), entry.arg(2)];
        // Atraviesa el stub de la PLT hasta llegar a la función real.
        let mut pc = entry.pc();
        for _ in 0..8 {
            let in_exe = self.procs[i]
                .maps
                .iter()
                .any(|m| pc >= m.start && pc < m.end && m.path == self.binary_path);
            if !in_exe {
                break;
            }
            self.resume_step(i)?;
            match self.wait(i)? {
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
                WaitStatus::Stopped(_, sig) => self.signal_stop(i, sig)?,
                _ => {}
            }
            pc = Regs::get(self.procs[i].pid).map(|r| r.pc()).unwrap_or(0);
        }
        let name = match self.syms.resolve(&self.procs[i].maps, pc) {
            Some(n) => n,
            None => {
                let p = &mut self.procs[i];
                p.maps = p.tracee.maps();
                self.syms.resolve(&p.maps, pc).unwrap_or_else(|| "?".into())
            }
        };
        if name == "free" || name == "realloc" {
            let p = &mut self.procs[i];
            let tracee = &p.tracee;
            p.heap
                .remember(args[0], |size| tracee.read(args[0], size.min(1 << 16) as usize));
        }
        let summary = call_summary(&name, &args, &self.procs[i].tracee);
        self.events.push(Event::Call {
            func: name.clone(),
            summary,
        });
        let orig = self.procs[i].tracee.read(ret_addr, 1).ok_or_else(lost)?[0];
        let call = LibCall {
            name,
            args,
            ret_addr,
            orig,
            regs: entry,
        };
        self.procs[i].at = At::Lib(Box::new(call.clone()));
        self.finish_call(i, call)
    }

    fn finish_call(&mut self, i: usize, call: LibCall) -> Res<()> {
        self.run_to(i, call.ret_addr, call.orig)?;
        let p = &mut self.procs[i];
        p.at = At::User;
        if matches!(
            call.name.as_str(),
            "malloc" | "calloc" | "realloc" | "reallocarray" | "free" | "strdup" | "strndup" | "mmap" | "sbrk" | "brk"
        ) {
            p.maps = p.tracee.maps();
        }
        let regs = Regs::get(p.pid).map_err(|_| lost())?;
        self.after_library_call(i, &call.name, call.args, regs.ret());
        Ok(())
    }

    fn after_library_call(&mut self, i: usize, name: &str, args: [u64; 3], ret: u64) {
        let t = self.t + 1;
        let vpid = self.procs[i].vpid;
        let line = self.procs[i].last.as_ref().map(|s| s.line).unwrap_or(0);
        let alloc = |s: &mut Self, fname: &str, addr: u64, size: u64, poison: bool| {
            if addr == 0 {
                s.events.push(Event::Malloc {
                    pid: vpid,
                    func: fname.into(),
                    addr: None,
                    size,
                    old_addr: None,
                });
                return;
            }
            let p = &mut s.procs[i];
            p.heap.alloc(addr, size, t, line);
            if poison && size > 0 {
                p.tracee.write(addr, &vec![memory::POISON; size.min(1 << 16) as usize]);
            }
            s.events.push(Event::Malloc {
                pid: vpid,
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
                let len = self.procs[i]
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
                let p = &mut self.procs[i];
                let old_size = p
                    .heap
                    .blocks
                    .iter()
                    .find(|b| b.addr == old && b.freed_at.is_none())
                    .map(|b| b.size);
                if ret != 0 && old != 0 && ret != old {
                    let _ = p.heap.free(old, t);
                }
                if ret != 0 {
                    p.heap.alloc(ret, size, t, line);
                    if let Some(os) = old_size
                        && size > os
                    {
                        p.tracee
                            .write(ret + os, &vec![memory::POISON; (size - os).min(1 << 16) as usize]);
                    }
                }
                self.events.push(Event::Malloc {
                    pid: vpid,
                    func: name.into(),
                    addr: if ret == 0 { None } else { Some(hex(ret)) },
                    size,
                    old_addr: if old == 0 { None } else { Some(hex(old)) },
                });
            }
            "free" if args[0] != 0 => {
                let error = self.procs[i].heap.free(args[0], t).err();
                if let Some(e) = error {
                    let kind = if e == FreeError::DoubleFree {
                        MemErrorKind::DoubleFree
                    } else {
                        MemErrorKind::InvalidFree
                    };
                    self.mem_errors.push(MemErrorRef {
                        t,
                        pid: vpid,
                        tid: vpid,
                        kind,
                        addr: hex(args[0]),
                    });
                }
                self.events.push(Event::Free {
                    pid: vpid,
                    addr: hex(args[0]),
                    error,
                });
            }
            _ => {}
        }
    }

    // ---------- syscalls ----------

    /// Devuelve true si terminó una escritura a la terminal.
    fn syscall_stop(&mut self, i: usize) -> Res<bool> {
        let info = ptrace::syscall_info(self.procs[i].pid).map_err(|_| lost())?;
        match info.op {
            libc::PTRACE_SYSCALL_INFO_ENTRY => {
                let e = unsafe { info.u.entry };
                self.procs[i].sys = Some((e.nr, e.args));
                self.syscall_entry(i, e.nr, e.args)?;
                Ok(false)
            }
            libc::PTRACE_SYSCALL_INFO_EXIT => {
                let x = unsafe { info.u.exit };
                match self.procs[i].sys.take() {
                    Some((nr, args)) => self.syscall_exit(i, nr, args, x.sval),
                    None => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    fn set_args(&self, i: usize, changes: &[(usize, i64)]) -> Res<()> {
        let pid = self.procs[i].pid;
        let mut regs = Regs::get(pid).map_err(|_| lost())?;
        for &(k, v) in changes {
            regs.set_syscall_arg(k, v as u64);
        }
        regs.set(pid).map_err(|_| lost())
    }

    fn syscall_entry(&mut self, i: usize, nr: u64, args: [u64; 6]) -> Res<()> {
        let as_pid = |v: u64| v as i32 as i64;
        match nr {
            // stdin agotado y todavía abierto: el proceso espera más entrada.
            arch::SYS_READ if args[0] == 0 && !self.opts.stdin_eof && self.stdin_consumed >= self.stdin_len() => {
                Err(Halt::Blocked(BlockReason::Read {
                    fd: 0,
                    pipe: None,
                    stdin: true,
                }))
            }
            arch::SYS_CLONE | arch::SYS_CLONE3 | arch::SYS_FORK | arch::SYS_VFORK => {
                let alive = self.procs.iter().filter(|p| p.state != PState::Reaped).count();
                if alive as u32 >= self.opts.limits.max_processes {
                    return Err(Halt::End(End::Truncated(TruncatedReason::Processes)));
                }
                Ok(())
            }
            arch::SYS_WAIT4 => {
                let target = as_pid(args[0]);
                self.set_args(i, &[(0, self.real_target(target))])?;
                if self.wait_would_block(i, target as i32, args[2] as i32) {
                    return Err(Halt::Blocked(BlockReason::Wait { target: target as i32 }));
                }
                Ok(())
            }
            arch::SYS_KILL | arch::SYS_GETPGID | arch::SYS_GETSID => {
                self.set_args(i, &[(0, self.real_target(as_pid(args[0])))])
            }
            arch::SYS_SETPGID => self.set_args(
                i,
                &[
                    (0, self.real_target(as_pid(args[0]))),
                    (1, self.real_target(as_pid(args[1]))),
                ],
            ),
            arch::SYS_TKILL => self.set_args(i, &[(0, self.real_tid(i, as_pid(args[0])))]),
            arch::SYS_TGKILL => self.set_args(
                i,
                &[
                    (0, self.real_target(as_pid(args[0]))),
                    (1, self.real_tid(i, as_pid(args[1]))),
                ],
            ),
            arch::SYS_EXECVE => {
                let p = &self.procs[i];
                let path = p.tracee.read_cstr(args[0], 4096).unwrap_or_default();
                let mut argv = Vec::new();
                for k in 0..64 {
                    let ptr = p.tracee.read_u64(args[1] + k * 8).unwrap_or(0);
                    if ptr == 0 {
                        break;
                    }
                    argv.push(String::from_utf8_lossy(&p.tracee.read_cstr(ptr, 256).unwrap_or_default()).into_owned());
                }
                let path = String::from_utf8_lossy(&path).into_owned();
                self.procs[i].exec_args = Some((path, argv));
                Ok(())
            }
            arch::SYS_PAUSE => Err(Halt::Blocked(BlockReason::Pause)),
            arch::SYS_RT_SIGSUSPEND => Err(Halt::Blocked(BlockReason::Sigsuspend)),
            // El reloj es virtual: la espera no ocurre en el kernel, el proceso queda bloqueado
            // hasta que el reloj llegue a su hora.
            arch::SYS_NANOSLEEP | arch::SYS_CLOCK_NANOSLEEP => {
                let req = if nr == arch::SYS_NANOSLEEP { args[0] } else { args[2] };
                let Some(ts) = self.procs[i].tracee.read(req, 16) else {
                    return Ok(());
                };
                let sec = i64::from_le_bytes(ts[..8].try_into().unwrap()).max(0) as u64;
                let nsec = i64::from_le_bytes(ts[8..].try_into().unwrap()).max(0) as u64;
                let ms = sec.saturating_mul(1000).saturating_add(nsec.div_ceil(1_000_000));
                let pid = self.procs[i].pid;
                let mut regs = Regs::get(pid).map_err(|_| lost())?;
                regs.skip_syscall();
                regs.set(pid).map_err(|_| lost())?;
                if ms == 0 {
                    return Ok(());
                }
                Err(Halt::Blocked(BlockReason::Sleep { until: self.clock + ms }))
            }
            _ => Ok(()),
        }
    }

    /// En tkill/tgkill el TID puede venir de gettid (virtual) o de la estructura interna de glibc
    /// (real, como lo escribió el kernel).
    fn real_tid(&self, i: usize, v: i64) -> i64 {
        if v == self.procs[i].pid.as_raw() as i64 {
            v
        } else {
            self.real_target(v)
        }
    }

    fn wait_matches(&self, parent: usize, child: &Proc, target: i32) -> bool {
        match target {
            -1 => true,
            0 => child.pgid == self.procs[parent].pgid,
            t if t > 0 => child.vpid == t as u32,
            t => child.pgid == t.unsigned_abs(),
        }
    }

    fn wait_would_block(&self, i: usize, target: i32, options: i32) -> bool {
        if options & libc::WNOHANG != 0 {
            return false;
        }
        let vpid = self.procs[i].vpid;
        let children: Vec<&Proc> = self
            .procs
            .iter()
            .filter(|c| c.ppid == Some(vpid) && c.state != PState::Reaped && self.wait_matches(i, c, target))
            .collect();
        !children.is_empty() && !children.iter().any(|c| c.state == PState::Zombie)
    }

    fn syscall_exit(&mut self, i: usize, nr: u64, args: [u64; 6], ret: i64) -> Res<bool> {
        let vpid = self.procs[i].vpid;
        let pid = self.procs[i].pid;
        let rewrite = |v: u64| -> Res<()> {
            let mut regs = Regs::get(pid).map_err(|_| lost())?;
            regs.set_ret(v);
            regs.set(pid).map_err(|_| lost())
        };
        match nr {
            arch::SYS_NANOSLEEP | arch::SYS_CLOCK_NANOSLEEP => rewrite(0)?,
            _ if ret < 0 => return Ok(false),
            arch::SYS_GETPID | arch::SYS_GETTID => rewrite(vpid as u64)?,
            arch::SYS_GETPPID | arch::SYS_GETPGID | arch::SYS_GETPGRP | arch::SYS_GETSID => {
                rewrite(self.vpid_of_real(ret).unwrap_or(INIT) as u64)?
            }
            arch::SYS_CLONE | arch::SYS_CLONE3 | arch::SYS_FORK | arch::SYS_VFORK if ret > 0 => {
                rewrite(self.vpid_of_real(ret).unwrap_or(INIT) as u64)?
            }
            arch::SYS_SETPGID => {
                let target = args[0] as i32;
                let who = if target == 0 {
                    Some(i)
                } else {
                    self.index_of(target as u32)
                };
                if let Some(w) = who {
                    let group = args[1] as i32;
                    self.procs[w].pgid = if group == 0 { self.procs[w].vpid } else { group as u32 };
                }
            }
            arch::SYS_WAIT4 => {
                let target = args[0] as i32;
                if ret == 0 {
                    self.events.push(Event::Wait {
                        pid: vpid,
                        target,
                        reaped: None,
                        status: None,
                    });
                    return Ok(false);
                }
                let child = self.vpid_of_real(ret).unwrap_or(INIT);
                rewrite(child as u64)?;
                let status = self.index_of(child).and_then(|c| {
                    let c = &mut self.procs[c];
                    c.state = PState::Reaped;
                    c.mem = None;
                    c.exit.clone()
                });
                self.events.push(Event::Wait {
                    pid: vpid,
                    target,
                    reaped: Some(child),
                    status,
                });
            }
            arch::SYS_WRITE if args[0] == 1 || args[0] == 2 => {
                let bytes = self.procs[i].tracee.read(args[1], ret as usize).unwrap_or_default();
                self.terminal_write(i, args[0] as u32, &bytes);
                return Ok(true);
            }
            arch::SYS_WRITEV if args[0] == 1 || args[0] == 2 => {
                let t = &self.procs[i].tracee;
                let mut bytes = Vec::new();
                for k in 0..args[2].min(64) {
                    let base = t.read_u64(args[1] + k * 16).unwrap_or(0);
                    let len = t.read_u64(args[1] + k * 16 + 8).unwrap_or(0);
                    bytes.extend(t.read(base, len.min(1 << 16) as usize).unwrap_or_default());
                }
                bytes.truncate(ret as usize);
                self.terminal_write(i, args[0] as u32, &bytes);
                return Ok(true);
            }
            arch::SYS_READ if args[0] == 0 => {
                let bytes = self.procs[i].tracee.read(args[1], ret as usize).unwrap_or_default();
                self.stdin_consumed += ret as u64;
                self.events.push(Event::Read {
                    pid: vpid,
                    tid: vpid,
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
        Ok(false)
    }

    fn stdin_len(&self) -> u64 {
        self.opts.stdin.len().min(launch::STDIN_MAX) as u64
    }

    fn terminal_write(&mut self, i: usize, fd: u32, bytes: &[u8]) {
        let stream = if fd == 2 { Stream::Stderr } else { Stream::Stdout };
        let p = &mut self.procs[i];
        p.output_bytes += bytes.len() as u64;
        self.events.push(Event::Write {
            pid: p.vpid,
            tid: p.vpid,
            fd,
            pipe: None,
            terminal: true,
            bytes: latin1(&bytes[..bytes.len().min(256)]),
            n: bytes.len() as u64,
            epipe: false,
        });
        self.pending_output.push((p.vpid, fd, stream, latin1(bytes)));
    }

    // ---------- ciclo de vida ----------

    fn ptrace_event(&mut self, i: usize, ev: i32) -> Res<()> {
        match ev {
            libc::PTRACE_EVENT_FORK | libc::PTRACE_EVENT_VFORK | libc::PTRACE_EVENT_CLONE => {
                let child = ptrace::getevent(self.procs[i].pid).map_err(|_| lost())?;
                self.spawn(i, Pid::from_raw(child as i32), ev == libc::PTRACE_EVENT_VFORK)
            }
            libc::PTRACE_EVENT_EXEC => self.on_exec(i),
            _ => Ok(()),
        }
    }

    /// El hijo nace dentro de la misma llamada que el padre (fork), con una copia de su modelo.
    fn spawn(&mut self, i: usize, child: Pid, vfork: bool) -> Res<()> {
        loop {
            match waitpid(child, Some(WaitPidFlag::__WALL)) {
                Ok(WaitStatus::Stopped(_, Signal::SIGSTOP)) => break,
                Ok(WaitStatus::Stopped(..)) | Ok(WaitStatus::PtraceEvent(..)) => {
                    let _ = ptrace::cont(child, None);
                }
                _ => return Err(lost()),
            }
        }
        self.live.lock().unwrap().push(child);
        let tracee = Tracee::attach(child).map_err(|_| lost())?;
        let vpid = VPID + self.procs.len() as u32;
        let p = &self.procs[i];
        let c = Proc {
            vpid,
            pid: child,
            ppid: Some(p.vpid),
            pgid: p.pgid,
            created_at: self.t + 1,
            tracee,
            heap: p.heap.clone(),
            maps: p.maps.clone(),
            image: p.image.clone(),
            at: p.at.clone(),
            state: PState::Ready,
            exit: None,
            last: p.last.clone(),
            calls: p.calls.clone(),
            sys: None,
            sig: None,
            exec_args: None,
            vfork_parent: vfork.then_some(p.vpid),
            vfork_child: None,
            mem: None,
            output_bytes: 0,
        };
        let parent = p.vpid;
        self.procs.push(c);
        let ci = self.procs.len() - 1;
        if self.procs[ci].user_image() && matches!(self.procs[ci].at, At::Lib(_)) {
            self.capture_in_call(ci);
        }
        self.events.push(Event::Fork {
            parent,
            child: vpid,
            vfork,
        });
        if vfork {
            self.procs[i].vfork_child = Some(vpid);
            return Err(Halt::Blocked(BlockReason::Wait { target: vpid as i32 }));
        }
        Ok(())
    }

    fn on_exec(&mut self, i: usize) -> Res<()> {
        let (path, argv) = self.procs[i].exec_args.take().unwrap_or_default();
        if let Some(parent) = self.procs[i].vfork_parent.take() {
            // La memoria compartida ya no es del hijo: se quita su breakpoint del padre.
            if let (At::Lib(call), Some(pi)) = (&self.procs[i].at, self.index_of(parent)) {
                self.procs[pi].tracee.write(call.ret_addr, &[call.orig]);
            }
            self.release_vfork_parent(parent);
        }
        let p = &mut self.procs[i];
        p.tracee = Tracee::attach(p.pid).map_err(|_| lost())?;
        p.maps = p.tracee.maps();
        p.heap = Heap::default();
        p.calls.clear();
        p.last = None;
        p.mem = None;
        p.at = At::Blackbox;
        p.image = ProcessImage::Blackbox {
            path: path.clone(),
            argv: argv.clone(),
        };
        self.events.push(Event::Exec {
            pid: p.vpid,
            path,
            argv,
            blackbox: true,
        });
        Err(Halt::Exec)
    }

    fn release_vfork_parent(&mut self, parent: u32) {
        if let Some(pi) = self.index_of(parent)
            && self.procs[pi].vfork_child.take().is_some()
        {
            self.unblock(pi);
        }
    }

    fn block(&mut self, i: usize, reason: BlockReason) {
        let vpid = self.procs[i].vpid;
        if self.procs[i].user_image() && matches!(self.procs[i].at, At::Lib(_)) {
            self.capture_in_call(i);
        }
        self.procs[i].state = PState::Blocked(reason.clone());
        self.events.push(Event::Block {
            pid: vpid,
            tid: vpid,
            reason,
        });
    }

    fn unblock(&mut self, i: usize) {
        let p = &mut self.procs[i];
        p.state = PState::Ready;
        self.events.push(Event::Unblock {
            pid: p.vpid,
            tid: p.vpid,
        });
    }

    fn on_exit(&mut self, i: usize, status: WaitStatus) {
        let t = self.t + 1;
        let vpid = self.procs[i].vpid;
        let (exit, code, signal) = match status {
            WaitStatus::Signaled(_, sig, core) => (
                ExitStatus::Signal {
                    signal: sig.as_str().into(),
                    core,
                },
                None,
                Some(sig.as_str().to_string()),
            ),
            WaitStatus::Exited(_, code) => (ExitStatus::Code { code }, Some(code), None),
            _ => (ExitStatus::Code { code: 0 }, Some(0), None),
        };
        self.live.lock().unwrap().retain(|p| *p != self.procs[i].pid);
        self.events.push(Event::Exit {
            pid: vpid,
            tid: None,
            scope: ExitScope::Process,
            code,
            signal,
            retval: None,
        });
        let leaks: Vec<Leak> = self.procs[i]
            .heap
            .live()
            .map(|b| Leak {
                pid: vpid,
                addr: hex(b.addr),
                size: b.size,
                ty: self.heap_type(i, b.addr),
                alloc_at: b.alloc_at,
                alloc_line: b.alloc_line,
            })
            .collect();
        self.leaks.extend(leaks);
        let mem = self.final_memory(i, t);
        let p = &mut self.procs[i];
        p.exit = Some(exit);
        p.at = At::User;
        // Un huérfano lo recoge init de inmediato.
        if p.ppid == Some(INIT) {
            p.state = PState::Reaped;
            p.mem = None;
        } else {
            p.state = PState::Zombie;
            p.mem = mem;
        }
        for c in 0..self.procs.len() {
            if self.procs[c].ppid != Some(vpid) || self.procs[c].state == PState::Reaped {
                continue;
            }
            self.events.push(Event::Reparent {
                pid: self.procs[c].vpid,
                from: vpid,
                to: INIT,
            });
            let c = &mut self.procs[c];
            c.ppid = Some(INIT);
            if c.state == PState::Zombie {
                c.state = PState::Reaped;
                c.mem = None;
                // Ahora es hijo del tracer (subreaper): hay que recogerlo de verdad.
                let _ = waitpid(c.pid, Some(WaitPidFlag::__WALL | WaitPidFlag::WNOHANG));
            }
        }
        if let Some(parent) = self.procs[i].vfork_parent.take() {
            self.release_vfork_parent(parent);
        }
        if let Some(pi) = self.procs[i].ppid.and_then(|pp| self.index_of(pp))
            && let PState::Blocked(BlockReason::Wait { target }) = self.procs[pi].state
            && self.procs[pi].vfork_child.is_none()
            && self.wait_matches(pi, &self.procs[i], target)
        {
            self.unblock(pi);
        }
    }

    // ---------- instantáneas y pasos ----------

    fn return_event(&mut self, i: usize, func: usize, regs: &Regs) {
        let f = &self.debug.functions[func];
        let p = &self.procs[i];
        let value = f.ret.and_then(|ty| {
            let st = self.debug.strip(ty);
            let size = self.debug.size_of(st);
            if size == 0 || size > 8 {
                return None;
            }
            let bytes = regs.ret().to_le_bytes();
            let mut reader = Reader::new(self.debug, &p.tracee, &p.heap, self.binary_path.clone(), p.maps.clone());
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

    /// Instantánea del proceso detenido en una línea; actualiza su posición.
    fn capture(&mut self, i: usize, regs: &Regs, line: u32) {
        let t = if self.steps.is_empty() { 0 } else { self.t + 1 };
        let p = &self.procs[i];
        let frames = memory::unwind(self.debug, &p.tracee, regs, line);
        let func = frames
            .first()
            .map(|f| self.debug.functions[f.func].name.clone())
            .unwrap_or_default();
        let snap = {
            let mut reader = Reader::new(self.debug, &p.tracee, &p.heap, self.binary_path.clone(), p.maps.clone());
            reader.snapshot(&frames, p.vpid, t)
        };
        let mem = self.snapshot_id(snap);
        let p = &mut self.procs[i];
        p.mem = Some(mem);
        p.last = Some(Stop {
            pc: regs.pc(),
            line,
            cfa: arch::cfa_of(regs.fp()),
            func,
        });
    }

    /// Instantánea de un proceso dentro de una llamada a biblioteca: se lee la pila del llamador
    /// con los registros que tenía al hacer la llamada.
    fn capture_in_call(&mut self, i: usize) {
        let At::Lib(call) = &self.procs[i].at else {
            return;
        };
        let mut regs = call.regs;
        regs.set_pc(call.ret_addr - 1);
        let line = self.debug.line_of(call.ret_addr - 1).unwrap_or(0);
        self.capture(i, &regs, line);
    }

    fn take_output(&mut self, t: u64) {
        for (pid, fd, stream, bytes) in self.pending_output.drain(..) {
            self.output.push(OutputChunk {
                t,
                pid,
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

    fn view(&self, i: usize, actor: Option<usize>, t: u64) -> Process {
        let p = &self.procs[i];
        let running = actor == Some(i) && t > 0;
        let (pstate, tstate, blocked_on) = match &p.state {
            PState::Zombie => (ProcessState::Zombie, ThreadState::Exited, None),
            PState::Reaped => (ProcessState::Reaped, ThreadState::Exited, None),
            PState::Blocked(r) => (ProcessState::Blocked, ThreadState::Blocked, Some(r.clone())),
            PState::Ready if running => (ProcessState::Running, ThreadState::Running, None),
            PState::Ready => (ProcessState::Ready, ThreadState::Ready, None),
        };
        let (line, func) = match (&p.last, tstate) {
            (Some(s), ThreadState::Ready | ThreadState::Running | ThreadState::Blocked) if p.user_image() => {
                (Some(s.line), Some(s.func.clone()))
            }
            _ => (None, None),
        };
        let in_call = match &p.at {
            At::Lib(c) if p.alive() => Some(c.name.clone()),
            _ => None,
        };
        Process {
            pid: p.vpid,
            ppid: p.ppid,
            pgid: p.pgid,
            state: pstate,
            created_at: p.created_at,
            image: p.image.clone(),
            exit: p.exit.clone(),
            fds: if p.alive() { Self::std_fds() } else { BTreeMap::new() },
            signals: ProcessSignals {
                mask: Vec::new(),
                pending: Vec::new(),
                actions: BTreeMap::new(),
            },
            threads: vec![Thread {
                tid: p.vpid,
                main: true,
                state: tstate,
                line,
                func,
                in_call,
                blocked_on,
                start: None,
                holds: Vec::new(),
                in_handler: None,
                retval: None,
            }],
            mem: if p.state == PState::Reaped { None } else { p.mem.clone() },
        }
    }

    fn commit(&mut self, actor: Option<usize>, executed: Option<ExecutedLine>, choices: Vec<TaskRef>) {
        let t = if self.steps.is_empty() { 0 } else { self.t + 1 };
        self.take_output(t);
        let processes = (0..self.procs.len()).map(|k| self.view(k, actor, t)).collect();
        self.steps.push(Step {
            t,
            actor: actor.map(|a| TaskRef {
                pid: self.procs[a].vpid,
                tid: self.procs[a].vpid,
            }),
            executed,
            choices,
            clock: self.clock,
            events: std::mem::take(&mut self.events),
            processes,
            pipes: Vec::new(),
            stdin: self.stdin_state(),
            signals: Vec::new(),
            timers: Vec::new(),
            sync: Vec::new(),
        });
        self.t = t;
    }

    fn heap_type(&self, i: usize, addr: u64) -> Option<String> {
        let id = self.procs[i].mem.as_ref()?;
        self.snapshots
            .get(id)?
            .heap
            .iter()
            .find(|b| b.addr == hex(addr))
            .and_then(|b| b.ty.clone())
    }

    /// Memoria al salir: sin pilas, pero con globales y heap para ver las fugas.
    fn final_memory(&mut self, i: usize, t: u64) -> Option<String> {
        let id = self.procs[i].mem.clone()?;
        let mut snap = self.snapshots.get(&id)?.clone();
        let p = &self.procs[i];
        snap.stacks = BTreeMap::from([(p.vpid, Vec::new())]);
        let visible: Vec<String> = p.heap.visible(t).map(|b| hex(b.addr)).collect();
        snap.heap.retain(|b| visible.contains(&b.addr));
        Some(self.snapshot_id(snap))
    }

    // ---------- cierre ----------

    fn kill_all(&mut self) {
        for p in &self.procs {
            if !p.alive() {
                continue;
            }
            let _ = nix::sys::signal::kill(p.pid, Signal::SIGKILL);
            loop {
                match waitpid(p.pid, Some(WaitPidFlag::__WALL)) {
                    Ok(WaitStatus::Exited(..)) | Ok(WaitStatus::Signaled(..)) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        }
        self.finished.store(true, Ordering::SeqCst);
    }

    fn finish(mut self, end: End, trace: &mut Trace) {
        self.kill_all();
        // Cuando el vigilante mató a los procesos, es un corte por tiempo y no una salida.
        let end = if self.timed_out.load(Ordering::SeqCst) {
            End::Truncated(TruncatedReason::Time)
        } else {
            end
        };
        match end {
            End::Done => {
                trace.outcome = match &self.procs[0].exit {
                    Some(ExitStatus::Signal { signal, .. }) => Outcome::Signaled { signal: signal.clone() },
                    Some(ExitStatus::Code { code }) => Outcome::Exited { code: *code },
                    None => Outcome::Truncated,
                };
            }
            End::AwaitingInput(vpid) => {
                self.push_final_event(Event::StdinNeeded { pid: vpid, tid: vpid });
                trace.outcome = Outcome::AwaitingInput { pid: vpid, tid: vpid };
            }
            End::Deadlock(tasks) => {
                self.push_final_event(Event::Deadlock { tasks: tasks.clone() });
                trace.outcome = Outcome::Deadlock { tasks };
            }
            End::Truncated(reason) => return self.truncate(reason, trace),
        }
        self.write_into(trace);
    }

    fn push_final_event(&mut self, event: Event) {
        if let Some(last) = self.steps.last_mut() {
            last.events.push(event);
        }
    }

    fn truncate(mut self, reason: TruncatedReason, trace: &mut Trace) {
        let text = match reason {
            TruncatedReason::Steps => format!("se alcanzó el límite de {} pasos", self.opts.limits.max_steps),
            TruncatedReason::Time => format!("se alcanzó el límite de {} s", self.opts.limits.wall_time_ms / 1000),
            TruncatedReason::Output => "el programa escribió más salida de la permitida".to_string(),
            TruncatedReason::Processes => format!(
                "se alcanzó el límite de {} procesos: el siguiente fork no se ejecutó",
                self.opts.limits.max_processes
            ),
            _ => "se alcanzó un límite".to_string(),
        };
        self.pending_output.clear();
        self.push_final_event(Event::Truncated { reason: text });
        trace.outcome = Outcome::Truncated;
        trace.truncated = true;
        trace.truncated_reason = Some(reason);
        self.write_into(trace);
    }

    fn write_into(self, trace: &mut Trace) {
        trace.steps = self.steps;
        trace.snapshots = self.snapshots;
        trace.output = self.output;
        trace.summary.mem_errors = self.mem_errors;
        trace.summary.leaks = self.leaks;
    }
}

fn is_fatal(sig: Signal) -> bool {
    !matches!(
        sig,
        Signal::SIGCHLD
            | Signal::SIGWINCH
            | Signal::SIGURG
            | Signal::SIGCONT
            | Signal::SIGSTOP
            | Signal::SIGTSTP
            | Signal::SIGTTIN
            | Signal::SIGTTOU
    )
}

fn call_summary(name: &str, args: &[u64; 3], tracee: &Tracee) -> Option<String> {
    Some(match name {
        "malloc" => format!("malloc({})", args[0]),
        "calloc" => format!("calloc({}, {})", args[0], args[1]),
        "realloc" => format!("realloc({:#x}, {})", args[0], args[1]),
        "free" => format!("free({:#x})", args[0]),
        "waitpid" => format!("waitpid({}, …)", args[0] as i32),
        "strlen" | "puts" | "strdup" => {
            let s = tracee.read_cstr(args[0], 40).unwrap_or_default();
            format!("{name}(\"{}\")", String::from_utf8_lossy(&s).escape_default())
        }
        "execlp" | "execl" | "execvp" | "execv" | "execve" | "execle" => {
            let s = tracee.read_cstr(args[0], 60).unwrap_or_default();
            format!("{name}(\"{}\", …)", String::from_utf8_lossy(&s).escape_default())
        }
        _ => return None,
    })
}
