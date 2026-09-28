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
use crate::fds::{self, FdTable, Pipes};
use crate::heap::Heap;
use crate::launch;
use crate::limits::Limits;
use crate::memory::{self, Reader, hex, latin1};
use crate::process::{Mapping, Tracee};
use crate::signals::{self, Action, SigState};
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
    /// Señales enviadas desde la terminal (Ctrl+C) justo después del paso indicado.
    pub injections: Vec<(u64, i32)>,
    pub policy: Policy,
    pub seed: u64,
    /// Tareas elegidas a mano para los primeros pasos; después sigue la política.
    pub schedule: Vec<TaskRef>,
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
        arch: arch::ARCH,
        source: opts.source.clone(),
        stdin: latin1(&opts.stdin),
        run: RunConfig {
            policy: opts.policy,
            seed: opts.seed,
            stdin_eof: opts.stdin_eof,
            schedule: opts.schedule.clone(),
            injections: opts
                .injections
                .iter()
                .map(|(t, sig)| Injection {
                    t: *t,
                    signal: signals::name(*sig),
                    target: InjectionTarget::Foreground,
                })
                .collect(),
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
                message: match e {
                    nix::errno::Errno::EACCES => {
                        "no se pudo ejecutar el programa compilado: el directorio temporal no permite \
                                                  ejecutar archivos (¿está montado con noexec?)"
                            .into()
                    }
                    e => format!("no se pudo lanzar el programa: {e}"),
                },
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
    AwaitingInput(u32, u32),
    Deadlock(Vec<TaskRef>),
    Truncated(TruncatedReason),
}

/// Por qué un proceso dejó de avanzar antes de llegar a su siguiente línea.
enum Halt {
    Blocked(BlockReason),
    Exited(WaitStatus),
    /// Cargó otro programa: el paso termina en el exec.
    Exec,
    /// Entró a un handler de señal: se sigue avanzando por sus líneas.
    Diverted,
    /// El paso termina con el proceso dentro de una llamada a biblioteca (volvió de un handler
    /// que interrumpió esa llamada).
    Paused,
    /// El hilo principal llamó a pthread_exit con otros hilos vivos: queda detenido en su exit.
    ThreadDone,
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
    /// Frame de un handler: al volver, el proceso retoma donde lo interrumpió la señal.
    signal: Option<i32>,
}

/// Lo que interrumpió una señal con handler, para retomarlo al volver.
#[derive(Clone)]
struct HandlerCtx {
    sig: i32,
    regs: Regs,
    at: At,
    last: Option<Stop>,
}

/// Llamada a biblioteca en curso: el breakpoint en `ret_addr` marca su fin.
#[derive(Clone)]
struct LibCall {
    name: String,
    args: [u64; 4],
    ret_addr: u64,
    orig: Vec<u8>,
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
    /// Hilo recién creado: corre hasta la primera instrucción de su función.
    Start {
        addr: u64,
        orig: Vec<u8>,
    },
    /// main ya volvió: corre libc hasta el exit.
    Exiting,
    Blackbox,
}

#[derive(Clone, PartialEq)]
enum PState {
    Ready,
    Blocked(BlockReason),
    /// Hilo que terminó; su proceso puede seguir vivo.
    Gone,
    Zombie,
    Reaped,
}

/// Una tarea: un hilo. Los hilos de un proceso comparten su `vpid`; el principal tiene `tid == vpid`.
/// Los campos del proceso (heap, fds, señales…) se copian en cada hilo: el que avanza toma la copia
/// del principal antes de su paso y la reparte al terminarlo.
struct Proc {
    vpid: u32,
    tid: u32,
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
    fds: FdTable,
    sigs: SigState,
    handlers: Vec<HandlerCtx>,
    /// Máscara que pasó a sigsuspend mientras espera.
    suspend_mask: Option<u64>,
    /// Pila del hilo en su última parada: la instantánea del proceso junta las de todos sus hilos.
    stack: Vec<Frame>,
    start: Option<ThreadStart>,
    retval: Option<Value>,
    /// Valor de `pthread_t` del hilo, para reconocerlo en pthread_join.
    pthread: u64,
    /// Espera en un futex: (dirección, valor esperado). Despierta cuando el valor cambia.
    futex: Option<(u64, u32)>,
}

impl Proc {
    /// El hilo puede avanzar o está esperando.
    fn alive(&self) -> bool {
        matches!(self.state, PState::Ready | PState::Blocked(_))
    }

    /// El proceso no terminó (aunque este hilo sí).
    fn proc_alive(&self) -> bool {
        !matches!(self.state, PState::Zombie | PState::Reaped)
    }

    fn is_leader(&self) -> bool {
        self.tid == self.vpid
    }

    /// Copia en `self` los campos del proceso que tiene `from`.
    fn take_shared(&mut self, from: &Proc) {
        self.ppid = from.ppid;
        self.pgid = from.pgid;
        self.created_at = from.created_at;
        self.heap = from.heap.clone();
        self.maps = from.maps.clone();
        self.image = from.image.clone();
        self.exit = from.exit.clone();
        self.exec_args = from.exec_args.clone();
        self.vfork_parent = from.vfork_parent;
        self.vfork_child = from.vfork_child;
        self.mem = from.mem.clone();
        self.output_bytes = from.output_bytes;
        self.fds = from.fds.clone();
        self.sigs = from.sigs.clone();
    }

    fn user_image(&self) -> bool {
        matches!(self.image, ProcessImage::User { .. })
    }
}

/// Mutex, variable de condición o semáforo que el programa usó.
struct SyncEntry {
    id: String,
    kind: char,
    pid: u32,
    addr: u64,
}

struct Engine<'a> {
    debug: &'a DebugInfo,
    opts: &'a Options,
    syms: Symbols,
    binary_path: String,
    _stdin_writer: Option<File>,
    stdin_consumed: u64,
    procs: Vec<Proc>,
    pipes: Pipes,
    /// alarm(): (proceso, instante del reloj virtual en que llega SIGALRM).
    timers: Vec<(u32, u64)>,
    injected: usize,
    actor: Option<usize>,
    /// Hilo creado en la llamada a pthread_create en curso (para anotar su pthread_t al volver).
    last_thread: Option<usize>,
    sync: Vec<SyncEntry>,
    rng: u64,
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
            tid: VPID,
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
            fds: fds::std_table(),
            sigs: SigState::default(),
            handlers: Vec::new(),
            suspend_mask: None,
            stack: Vec::new(),
            start: None,
            retval: None,
            pthread: 0,
            futex: None,
        };
        Engine {
            debug,
            opts,
            syms: Symbols::default(),
            binary_path,
            _stdin_writer: stdin_writer,
            stdin_consumed: 0,
            procs: vec![root],
            pipes: Pipes::default(),
            timers: Vec::new(),
            injected: 0,
            actor: None,
            last_thread: None,
            sync: Vec::new(),
            rng: opts.seed ^ 0x9e37_79b9_7f4a_7c15,
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
            | ptrace::Options::PTRACE_O_TRACEEXEC
            | ptrace::Options::PTRACE_O_TRACECLONE;
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
            if self.procs.iter().all(|p| !p.proc_alive()) {
                return End::Done;
            }
            self.inject();
            self.fire_timers();
            self.wake_sleepers();
            self.wake_signals();
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
            self.actor = Some(i);
            self.pull(i);
            let result = self.advance(i);
            self.actor = None;
            match result {
                Ok(Some((regs, line))) => self.capture(i, &regs, line),
                Ok(None) | Err(Halt::Exec) => {}
                Err(Halt::Blocked(reason)) => self.block(i, reason),
                Err(Halt::Exited(status)) => self.on_task_exit(i, status),
                Err(Halt::ThreadDone) => self.thread_exit(i),
                Err(Halt::Paused) => self.capture_in_call(i),
                Err(Halt::Diverted) => unreachable!("advance sigue por el handler"),
                Err(Halt::End(end)) => return end,
            }
            self.push(i);
            self.disarm(i);
            self.wake_io();
            self.wake_futex();
            self.wake_signals();
            self.commit(Some(i), executed, choices);
            self.cursor = i;
        }
    }

    /// Lleva el proceso inicial hasta la primera línea de main: ese es el paso 0.
    fn start_root(&mut self) -> Res<()> {
        let main_idx = self.debug.functions.iter().position(|f| f.name == "main").unwrap();
        let main = &self.debug.functions[main_idx];
        let orig = self.procs[0]
            .tracee
            .read(main.body_start, arch::BREAKPOINT.len())
            .ok_or_else(lost)?;
        self.run_to(0, main.body_start, &orig)?;
        let p = &mut self.procs[0];
        p.maps = p.tracee.maps();
        let regs = Regs::get(p.pid).map_err(|_| lost())?;
        let cfa = self.debug.cfa_at(regs.pc(), regs.fp());
        let ret_addr = p.tracee.read_u64(arch::return_address_slot(regs.fp())).unwrap_or(0);
        p.calls.push(CallFrame {
            func: main_idx,
            ret_addr,
            cfa,
            poisoned: true,
            signal: None,
        });
        memory::poison_locals(self.debug, &p.tracee, main_idx, cfa);
        let line = self
            .debug
            .stmt_row_at(regs.pc())
            .map(|r| r.line)
            .unwrap_or(main.decl_line);
        // Lo que hizo el loader antes de main (abrir y cerrar bibliotecas) no es del programa.
        self.events.clear();
        self.procs[0].fds = fds::std_table();
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
                tid: p.tid,
            })
            .collect()
    }

    /// Round-robin: el siguiente proceso listo después del último que avanzó.
    fn pick(&mut self) -> Option<usize> {
        let ready: Vec<usize> = (0..self.procs.len())
            .filter(|&i| self.procs[i].state == PState::Ready)
            .collect();
        let done = self.steps.iter().filter(|s| s.actor.is_some()).count();
        if let Some(want) = self.opts.schedule.get(done)
            && let Some(&i) = ready
                .iter()
                .find(|&&i| self.procs[i].vpid == want.pid && self.procs[i].tid == want.tid)
        {
            return Some(i);
        }
        if self.opts.policy == Policy::Random && !ready.is_empty() {
            // xorshift64*: la misma semilla elige siempre la misma secuencia.
            let mut x = self.rng.max(1);
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.rng = x;
            return Some(ready[(x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % ready.len()]);
        }
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
            .chain(self.timers.iter().map(|(_, at)| *at))
            .min();
        if let Some(until) = wake {
            self.clock = self.clock.max(until);
            self.fire_timers();
            self.wake_sleepers();
            self.wake_signals();
            self.commit(None, None, Vec::new());
            return None;
        }
        if let Some(p) = self
            .procs
            .iter()
            .find(|p| matches!(&p.state, PState::Blocked(BlockReason::Read { stdin: true, .. })))
        {
            return Some(End::AwaitingInput(p.vpid, p.tid));
        }
        let tasks = self
            .procs
            .iter()
            .filter(|p| p.alive())
            .map(|p| TaskRef {
                pid: p.vpid,
                tid: p.tid,
            })
            .collect();
        Some(End::Deadlock(tasks))
    }

    /// Procesos que alcanza un kill: un PID, el propio grupo (0), todos (-1) o un grupo (< -1).
    fn targets(&self, i: usize, target: i32) -> Vec<usize> {
        let alive = |p: &Proc| p.is_leader() && p.proc_alive();
        match target {
            t if t > 0 => self
                .index_of(t as u32)
                .filter(|&k| alive(&self.procs[k]))
                .into_iter()
                .collect(),
            0 => {
                let g = self.procs[i].pgid;
                (0..self.procs.len())
                    .filter(|&k| alive(&self.procs[k]) && self.procs[k].pgid == g)
                    .collect()
            }
            -1 => (0..self.procs.len()).filter(|&k| alive(&self.procs[k])).collect(),
            t => (0..self.procs.len())
                .filter(|&k| alive(&self.procs[k]) && self.procs[k].pgid == t.unsigned_abs())
                .collect(),
        }
    }

    /// Registra un envío: el kernel ya lo hizo (o el tracer lo hace aquí), la entrega llega cuando
    /// el destino vuelva a avanzar. SIGKILL no espera: el destino muere de inmediato.
    fn signal_sent(&mut self, from: SignalSource, targets: Vec<usize>, to: i32, sig: i32) {
        if sig == 0 {
            return;
        }
        self.events.push(Event::SignalSend {
            from: from.clone(),
            to,
            signal: signals::name(sig),
        });
        for k in targets {
            if sig == libc::SIGKILL {
                let (vpid, tid) = (self.procs[k].vpid, self.procs[k].tid);
                self.events.push(Event::SignalDeliver {
                    pid: vpid,
                    tid,
                    signal: "SIGKILL".into(),
                    action: DeliverAction::Terminate,
                    handler: None,
                });
                if k != self.cursor_actor() {
                    let pid = self.procs[k].pid;
                    loop {
                        match waitpid(pid, Some(WaitPidFlag::__WALL)) {
                            Ok(st @ (WaitStatus::Exited(..) | WaitStatus::Signaled(..))) => {
                                self.on_exit(k, st);
                                break;
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                }
                continue;
            }
            self.procs[k].sigs.push(sig, from.clone());
        }
    }

    /// El proceso que está avanzando en este paso (sus paradas las espera su propio bucle).
    fn cursor_actor(&self) -> usize {
        self.actor.unwrap_or(usize::MAX)
    }

    /// Ctrl+C (u otra señal de la terminal) al grupo en primer plano, justo después del paso t.
    fn inject(&mut self) {
        while let Some(&(t, sig)) = self.opts.injections.get(self.injected) {
            if t != self.t {
                break;
            }
            self.injected += 1;
            let targets = self.targets(0, -(VPID as i32));
            for &k in &targets {
                let _ = nix::sys::signal::kill(self.procs[k].pid, Signal::try_from(sig).ok());
            }
            self.signal_sent(SignalSource::Terminal, targets, -(VPID as i32), sig);
        }
    }

    fn fire_timers(&mut self) {
        let due: Vec<u32> = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= self.clock)
            .map(|(p, _)| *p)
            .collect();
        self.timers.retain(|(_, at)| *at > self.clock);
        for vpid in due {
            if let Some(k) = self.index_of(vpid) {
                let _ = nix::sys::signal::kill(self.procs[k].pid, Signal::SIGALRM);
                self.signal_sent(SignalSource::Timer { pid: vpid }, vec![k], vpid as i32, libc::SIGALRM);
            }
        }
    }

    /// Una señal que se entregará despierta a quien espera: al reanudarse, el kernel interrumpe la
    /// espera (EINTR) y corre el handler o termina al proceso.
    fn wake_signals(&mut self) {
        for i in 0..self.procs.len() {
            let p = &self.procs[i];
            if !matches!(p.state, PState::Blocked(_)) {
                continue;
            }
            let suspend = matches!(p.state, PState::Blocked(BlockReason::Sigsuspend))
                .then_some(p.suspend_mask)
                .flatten();
            if p.sigs.interrupts(suspend) {
                self.unblock(i);
            }
        }
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
        self.procs.iter().find(|p| p.pid.as_raw() as i64 == real).map(|p| p.tid)
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
                let r = self.finish_call(i, *call).and_then(|_| self.step_lines(i, true));
                return self.follow_handlers(i, r).map(Some);
            }
            At::Start { addr, orig } => {
                let r = self.run_to(i, addr, &orig).and_then(|_| {
                    self.procs[i].at = At::User;
                    self.procs[i].last = None;
                    self.step_lines(i, true)
                });
                return self.follow_handlers(i, r).map(Some);
            }
            At::User => {}
        }
        let r = self.step_lines(i, false);
        self.follow_handlers(i, r).map(Some)
    }

    /// Si una señal desvió al proceso a su handler, el paso sigue por las líneas del handler.
    fn follow_handlers(&mut self, i: usize, mut r: Res<(Regs, u32)>) -> Res<(Regs, u32)> {
        while let Err(Halt::Diverted) = r {
            r = self.step_lines(i, false);
        }
        r
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
                if f.signal.is_some() {
                    return self.leave_handler(i);
                }
                if self.procs[i].calls.is_empty() {
                    if !self.procs[i].is_leader() {
                        self.procs[i].retval = Some(pointer_value(regs.ret()));
                    }
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
                        let ret_addr = regs.entry_return_address(|a| p.tracee.read_u64(a));
                        p.calls.push(CallFrame {
                            func: fi,
                            ret_addr,
                            cfa: regs.entry_cfa(),
                            poisoned: false,
                            signal: None,
                        });
                        continue;
                    }
                    if pc < f.body_start {
                        continue;
                    }
                    let Some(row) = debug.stmt_row_at(pc) else {
                        continue;
                    };
                    let cfa = debug.cfa_at(pc, regs.fp());
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
                    let ret_addr = regs.entry_return_address(|a| self.procs[i].tracee.read_u64(a));
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
    fn run_to(&mut self, i: usize, addr: u64, orig: &[u8]) -> Res<()> {
        // Se reescribe siempre: un hijo de vfork que comparte la memoria pudo haberlo quitado.
        self.procs[i].tracee.write(addr, arch::BREAKPOINT);
        loop {
            self.resume_syscall(i)?;
            match self.wait(i)? {
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                    let pid = self.procs[i].pid;
                    let mut regs = Regs::get(pid).map_err(|_| lost())?;
                    if regs.pc() == arch::pc_after_breakpoint(addr) {
                        self.procs[i].tracee.write(addr, orig);
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
                        self.procs[i].tracee.write(addr, orig);
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
        let s = sig as i32;
        let vpid = self.procs[i].vpid;
        let tid = self.procs[i].tid;
        let known = self.procs[i].sigs.take(s).is_some();
        let deliver = |action, handler| Event::SignalDeliver {
            pid: vpid,
            tid,
            signal: sig.as_str().into(),
            action,
            handler,
        };
        match self.procs[i].sigs.actions.get(&s).cloned() {
            Some(Action::Handler(name)) if self.procs[i].user_image() && !matches!(self.procs[i].at, At::Exiting) => {
                self.events.push(deliver(DeliverAction::Handler, Some(name)));
                self.enter_handler(i, sig)
            }
            Some(Action::Handler(name)) => {
                self.events.push(deliver(DeliverAction::Handler, Some(name)));
                self.procs[i].sig = Some(sig);
                Ok(())
            }
            Some(Action::Ignore) => {
                if known {
                    self.events.push(deliver(DeliverAction::Ignore, None));
                }
                self.procs[i].sig = Some(sig);
                Ok(())
            }
            None if signals::default_ignored(s) => {
                self.procs[i].sig = Some(sig);
                Ok(())
            }
            None if signals::job_control(s) => Ok(()),
            None => Err(self.deliver_fatal(i, sig)),
        }
    }

    /// Entrega con handler: un singlestep con la señal deja al proceso en la primera instrucción
    /// del handler. Se guarda lo interrumpido para retomarlo al volver.
    fn enter_handler(&mut self, i: usize, sig: Signal) -> Res<()> {
        let pid = self.procs[i].pid;
        let regs = Regs::get(pid).map_err(|_| lost())?;
        ptrace::step(pid, sig).map_err(|_| lost())?;
        match self.wait(i)? {
            WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
            WaitStatus::Stopped(_, other) => self.procs[i].sig = Some(other),
            _ => {}
        }
        let hregs = Regs::get(pid).map_err(|_| lost())?;
        let Some(fi) = self.debug.functions.iter().position(|f| f.low == hregs.pc()) else {
            return Ok(());
        };
        let p = &mut self.procs[i];
        let ret_addr = hregs.entry_return_address(|a| p.tracee.read_u64(a));
        p.calls.push(CallFrame {
            func: fi,
            ret_addr,
            cfa: hregs.entry_cfa(),
            poisoned: false,
            signal: Some(sig as i32),
        });
        p.handlers.push(HandlerCtx {
            sig: sig as i32,
            regs,
            at: p.at.clone(),
            last: p.last.clone(),
        });
        p.at = At::User;
        Err(Halt::Diverted)
    }

    /// El handler volvió a su trampolín de libc: se corre hasta que rt_sigreturn restaura lo
    /// interrumpido.
    fn leave_handler(&mut self, i: usize) -> Res<(Regs, u32)> {
        let pid = self.procs[i].pid;
        loop {
            self.resume_syscall(i)?;
            match self.wait(i)? {
                WaitStatus::PtraceSyscall(_) => {
                    let info = ptrace::syscall_info(pid).map_err(|_| lost())?;
                    let returning = matches!(self.procs[i].sys, Some((nr, _)) if nr == arch::SYS_RT_SIGRETURN);
                    if info.op == libc::PTRACE_SYSCALL_INFO_EXIT && returning {
                        self.procs[i].sys = None;
                        break;
                    }
                    self.syscall_stop(i)?;
                }
                WaitStatus::PtraceEvent(_, _, ev) => self.ptrace_event(i, ev)?,
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
                WaitStatus::Stopped(_, s) => self.signal_stop(i, s)?,
                _ => {}
            }
        }
        let p = &mut self.procs[i];
        let Some(ctx) = p.handlers.pop() else {
            return Err(lost());
        };
        p.last = ctx.last;
        p.at = ctx.at;
        self.events.push(Event::SignalReturn {
            pid: p.vpid,
            tid: p.tid,
            signal: signals::name(ctx.sig),
        });
        if matches!(p.at, At::Lib(_)) {
            return Err(Halt::Paused);
        }
        let regs = Regs::get(pid).map_err(|_| lost())?;
        match self
            .debug
            .line_of(regs.pc())
            .filter(|_| self.debug.function_at(regs.pc()).is_some())
        {
            Some(line) => Ok((regs, line)),
            // Interrumpido en la PLT: se sigue hasta la próxima línea.
            None => Err(Halt::Diverted),
        }
    }

    fn deliver_fatal(&mut self, i: usize, sig: Signal) -> Halt {
        let vpid = self.procs[i].vpid;
        let tid = self.procs[i].tid;
        if sig == Signal::SIGPIPE {
            self.events.push(Event::SignalSend {
                from: SignalSource::Kernel {
                    cause: KernelCause::Sigpipe,
                },
                to: vpid as i32,
                signal: sig.as_str().into(),
            });
        }
        let core = matches!(
            sig,
            Signal::SIGSEGV
                | Signal::SIGABRT
                | Signal::SIGFPE
                | Signal::SIGILL
                | Signal::SIGBUS
                | Signal::SIGQUIT
                | Signal::SIGTRAP
                | Signal::SIGSYS
        );
        self.events.push(Event::SignalDeliver {
            pid: vpid,
            tid,
            signal: sig.as_str().into(),
            action: if core {
                DeliverAction::Core
            } else {
                DeliverAction::Terminate
            },
            handler: None,
        });
        if sig == Signal::SIGSEGV {
            let addr = ptrace::getsiginfo(self.procs[i].pid)
                .map(|si| unsafe { si.si_addr() } as u64)
                .unwrap_or(0);
            self.events.push(Event::MemError {
                pid: vpid,
                tid,
                kind: MemErrorKind::Segfault,
                addr: hex(addr),
            });
            self.mem_errors.push(MemErrorRef {
                t: self.t + 1,
                pid: vpid,
                tid,
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
        let args = [entry.arg(0), entry.arg(1), entry.arg(2), entry.arg(3)];
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
        self.note_sync_call(i, &name, &args);
        if name == "pthread_exit" {
            self.procs[i].retval = Some(pointer_value(args[0]));
        }
        let summary = call_summary(&name, &args, &self.procs[i].tracee);
        self.events.push(Event::Call {
            func: name.clone(),
            summary,
        });
        let orig = self.procs[i]
            .tracee
            .read(ret_addr, arch::BREAKPOINT.len())
            .ok_or_else(lost)?;
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
        self.run_to(i, call.ret_addr, &call.orig)?;
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

    fn after_library_call(&mut self, i: usize, name: &str, args: [u64; 4], ret: u64) {
        self.sync_event(i, name, &args, ret);
        let t = self.t + 1;
        let vpid = self.procs[i].vpid;
        let tid = self.procs[i].tid;
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
                        tid,
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
            arch::SYS_READ | arch::SYS_READV => match self.read_blocks(i, args[0] as u32) {
                Some(r) => Err(Halt::Blocked(r)),
                None => Ok(()),
            },
            arch::SYS_WRITE | arch::SYS_WRITEV => {
                let n = if nr == arch::SYS_WRITE {
                    args[2] as usize
                } else {
                    self.iov_len(i, args[1], args[2])
                };
                match self.write_blocks(i, args[0] as u32, n) {
                    Some(r) => Err(Halt::Blocked(r)),
                    None => Ok(()),
                }
            }
            arch::SYS_CLONE | arch::SYS_CLONE3 if self.clone_is_thread(i, nr, &args) => {
                let vpid = self.procs[i].vpid;
                let threads = self.procs.iter().filter(|p| p.vpid == vpid && p.alive()).count();
                if threads as u32 >= self.opts.limits.max_threads_per_process {
                    return Err(Halt::End(End::Truncated(TruncatedReason::Threads)));
                }
                Ok(())
            }
            arch::SYS_CLONE | arch::SYS_CLONE3 | arch::SYS_FORK | arch::SYS_VFORK => {
                let alive = self
                    .procs
                    .iter()
                    .filter(|p| p.is_leader() && p.state != PState::Reaped)
                    .count();
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
            // ppoll sin descriptores es como pause (glibc implementa así pause en aarch64) o, con
            // tiempo límite, como un sleep.
            arch::SYS_PPOLL if args[1] == 0 => {
                if args[2] == 0 {
                    return Err(Halt::Blocked(BlockReason::Pause));
                }
                let t = &self.procs[i].tracee;
                let ms = match (t.read_u64(args[2]), t.read_u64(args[2] + 8)) {
                    (Some(sec), Some(nsec)) => sec.saturating_mul(1000).saturating_add(nsec.div_ceil(1_000_000)),
                    _ => 0,
                };
                let pid = self.procs[i].pid;
                let mut regs = Regs::get(pid).map_err(|_| lost())?;
                arch::skip_syscall(pid, &mut regs).map_err(|_| lost())?;
                if ms == 0 {
                    return Ok(());
                }
                Err(Halt::Blocked(BlockReason::Sleep { until: self.clock + ms }))
            }
            arch::SYS_FUTEX => {
                let op = args[1] as i32 & 0x7f;
                if op != libc::FUTEX_WAIT && op != libc::FUTEX_WAIT_BITSET {
                    return Ok(());
                }
                let now = self.procs[i]
                    .tracee
                    .read(args[0], 4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()));
                if now != Some(args[2] as u32) {
                    return Ok(());
                }
                self.procs[i].futex = Some((args[0], args[2] as u32));
                Err(Halt::Blocked(self.futex_reason(i, args[0])))
            }
            // pthread_exit del hilo principal: su exit no se completa hasta que terminen los demás.
            arch::SYS_EXIT if self.procs[i].is_leader() && self.siblings_alive(i) => Err(Halt::ThreadDone),
            arch::SYS_RT_SIGSUSPEND => {
                self.procs[i].suspend_mask = self.procs[i].tracee.read_u64(args[0]);
                Err(Halt::Blocked(BlockReason::Sigsuspend))
            }
            // alarm usa el reloj virtual: el tracer envía SIGALRM cuando el reloj llega a su hora.
            // setitimer(ITIMER_REAL) es como alarm (glibc lo usa para alarm en aarch64).
            arch::SYS_ALARM | arch::SYS_SETITIMER if nr == arch::SYS_ALARM || args[0] == 0 => {
                let pid = self.procs[i].pid;
                let mut regs = Regs::get(pid).map_err(|_| lost())?;
                arch::skip_syscall(pid, &mut regs).map_err(|_| lost())
            }
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
                arch::skip_syscall(pid, &mut regs).map_err(|_| lost())?;
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
        let tid = self.procs[i].tid;
        let pid = self.procs[i].pid;
        let rewrite = |v: u64| -> Res<()> {
            let mut regs = Regs::get(pid).map_err(|_| lost())?;
            regs.set_ret(v);
            regs.set(pid).map_err(|_| lost())
        };
        match nr {
            arch::SYS_NANOSLEEP | arch::SYS_CLOCK_NANOSLEEP => rewrite(0)?,
            arch::SYS_PPOLL if args[1] == 0 && args[2] != 0 => rewrite(0)?,
            arch::SYS_READ | arch::SYS_READV | arch::SYS_WRITE | arch::SYS_WRITEV => {
                return Ok(self.io_exit(i, nr, args, ret));
            }
            arch::SYS_ALARM => {
                let left = self.set_timer(i, args[0] * 1000);
                rewrite(left.div_ceil(1000))?
            }
            arch::SYS_SETITIMER if args[0] == 0 => {
                // struct itimerval { it_interval, it_value }: cada timeval es (segundos, microsegundos).
                let t = &self.procs[i].tracee;
                let ms = match (t.read_u64(args[1] + 16), t.read_u64(args[1] + 24)) {
                    (Some(sec), Some(usec)) if args[1] != 0 => sec * 1000 + usec.div_ceil(1000),
                    _ => 0,
                };
                let left = self.set_timer(i, ms);
                if args[2] != 0 {
                    let mut old = vec![0u8; 32];
                    old[16..24].copy_from_slice(&(left / 1000).to_le_bytes());
                    old[24..32].copy_from_slice(&((left % 1000) * 1000).to_le_bytes());
                    self.procs[i].tracee.write(args[2], &old);
                }
                rewrite(0)?
            }
            _ if ret < 0 => return Ok(false),
            arch::SYS_KILL | arch::SYS_TKILL | arch::SYS_TGKILL => {
                let (target, sig, via) = match nr {
                    arch::SYS_KILL => (args[0] as i32, args[1] as i32, SignalVia::Kill),
                    arch::SYS_TKILL => (args[0] as i32, args[1] as i32, SignalVia::Raise),
                    _ => (args[0] as i32, args[2] as i32, SignalVia::Raise),
                };
                let target = if nr == arch::SYS_KILL || target != self.procs[i].pid.as_raw() {
                    target
                } else {
                    vpid as i32
                };
                self.signal_sent(
                    SignalSource::Process { pid: vpid, via },
                    self.targets(i, target),
                    target,
                    sig,
                );
            }
            arch::SYS_RT_SIGACTION if args[1] != 0 => {
                let handler = self.procs[i].tracee.read_u64(args[1]).unwrap_or(0);
                let debug = self.debug;
                self.procs[i].sigs.set_action(args[0] as i32, handler, |addr| {
                    debug
                        .functions
                        .iter()
                        .find(|f| f.low == addr)
                        .map_or_else(|| hex(addr), |f| f.name.clone())
                });
            }
            arch::SYS_RT_SIGPROCMASK if args[1] != 0 => {
                let set = self.procs[i].tracee.read_u64(args[1]).unwrap_or(0);
                self.procs[i].sigs.set_mask(args[0] as i32, set);
            }
            arch::SYS_PIPE | arch::SYS_PIPE2 => {
                let Some(b) = self.procs[i].tracee.read(args[0], 8) else {
                    return Ok(false);
                };
                let r = i32::from_le_bytes(b[..4].try_into().unwrap()) as u32;
                let w = i32::from_le_bytes(b[4..].try_into().unwrap()) as u32;
                let cloexec = nr == arch::SYS_PIPE2 && args[1] as i32 & libc::O_CLOEXEC != 0;
                let id = self.pipes.create(vpid);
                for (fd, end) in [(r, PipeEndKind::Read), (w, PipeEndKind::Write)] {
                    let entry = Fd::Pipe {
                        pipe: id.clone(),
                        end,
                        cloexec: None,
                    };
                    self.procs[i].fds.insert(fd, fds::with_cloexec(&entry, cloexec));
                }
                self.events.push(Event::Pipe {
                    pid: vpid,
                    pipe: id,
                    fds: [r, w],
                });
            }
            arch::SYS_DUP | arch::SYS_DUP2 | arch::SYS_DUP3 => {
                let cloexec = nr == arch::SYS_DUP3 && args[2] as i32 & libc::O_CLOEXEC != 0;
                self.dup_event(i, args[0] as u32, ret as u32, cloexec);
            }
            arch::SYS_FCNTL => match args[1] as i32 {
                libc::F_DUPFD | libc::F_DUPFD_CLOEXEC => {
                    self.dup_event(i, args[0] as u32, ret as u32, args[1] as i32 == libc::F_DUPFD_CLOEXEC)
                }
                libc::F_SETFD => {
                    if let Some(e) = self.procs[i].fds.get_mut(&(args[0] as u32)) {
                        *e = fds::with_cloexec(e, args[2] as i32 & libc::FD_CLOEXEC != 0);
                    }
                }
                _ => {}
            },
            arch::SYS_CLOSE => {
                if let Some(was) = self.procs[i].fds.remove(&(args[0] as u32)) {
                    self.events.push(Event::Close {
                        pid: vpid,
                        fd: args[0] as u32,
                        was,
                    });
                }
            }
            // Solo los archivos que abre el programa del usuario; los de una caja negra (sus
            // bibliotecas, el directorio que lista ls) no se dibujan.
            arch::SYS_OPEN | arch::SYS_OPENAT if self.procs[i].user_image() => {
                let (path, flags) = if nr == arch::SYS_OPEN {
                    (args[0], args[1])
                } else {
                    (args[1], args[2])
                };
                let path = self.procs[i].tracee.read_cstr(path, 256).unwrap_or_default();
                let flags = flags as i32;
                let mode = match flags & libc::O_ACCMODE {
                    _ if flags & libc::O_APPEND != 0 => FileMode::A,
                    libc::O_WRONLY => FileMode::W,
                    libc::O_RDWR => FileMode::Rw,
                    _ => FileMode::R,
                };
                let entry = Fd::File {
                    path: String::from_utf8_lossy(&path).into_owned(),
                    mode,
                    cloexec: None,
                };
                self.procs[i]
                    .fds
                    .insert(ret as u32, fds::with_cloexec(&entry, flags & libc::O_CLOEXEC != 0));
            }

            arch::SYS_GETPID => rewrite(vpid as u64)?,
            arch::SYS_GETTID => rewrite(tid as u64)?,
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
                let status = self.index_of(child).and_then(|c| self.procs[c].exit.clone());
                self.set_proc_state(child, PState::Reaped);
                self.events.push(Event::Wait {
                    pid: vpid,
                    target,
                    reaped: Some(child),
                    status,
                });
            }
            _ => {}
        }
        Ok(false)
    }

    /// Programa (o cancela, con 0) la alarma del proceso; devuelve los ms que le quedaban a la anterior.
    fn set_timer(&mut self, i: usize, ms: u64) -> u64 {
        let vpid = self.procs[i].vpid;
        let left = self
            .timers
            .iter()
            .find(|(p, _)| *p == vpid)
            .map_or(0, |(_, at)| at.saturating_sub(self.clock));
        self.timers.retain(|(p, _)| *p != vpid);
        if ms > 0 {
            self.timers.push((vpid, self.clock + ms));
        }
        left
    }

    fn dup_event(&mut self, i: usize, old: u32, new: u32, cloexec: bool) {
        if old == new {
            return;
        }
        if let Some(replaced) = fds::dup(&mut self.procs[i].fds, old, new, cloexec) {
            self.events.push(Event::Dup {
                pid: self.procs[i].vpid,
                oldfd: old,
                newfd: new,
                replaced,
            });
        }
    }

    fn pipe_ends(&self, id: &str, kind: PipeEndKind) -> usize {
        let tables = self
            .procs
            .iter()
            .filter(|p| p.is_leader() && p.proc_alive())
            .map(|p| (p.vpid, &p.fds));
        fds::ends(tables, id, kind).len()
    }

    /// ¿Bloquearía un read en `fd`? stdin agotado sin EOF, o pipe vacío que todavía tiene escritores.
    fn read_blocks(&self, i: usize, fd: u32) -> Option<BlockReason> {
        match self.procs[i].fds.get(&fd)? {
            Fd::Stdin { .. } if !self.opts.stdin_eof && self.stdin_consumed >= self.stdin_len() => {
                Some(BlockReason::Read {
                    fd,
                    pipe: None,
                    stdin: true,
                })
            }
            Fd::Pipe {
                pipe,
                end: PipeEndKind::Read,
                ..
            } if self.pipes.len(pipe) == 0 && self.pipe_ends(pipe, PipeEndKind::Write) > 0 => Some(BlockReason::Read {
                fd,
                pipe: Some(pipe.clone()),
                stdin: false,
            }),
            _ => None,
        }
    }

    /// ¿Bloquearía un write de `n` bytes? Solo si el pipe tiene lectores y no le cabe.
    fn write_blocks(&self, i: usize, fd: u32, n: usize) -> Option<BlockReason> {
        match self.procs[i].fds.get(&fd)? {
            Fd::Pipe {
                pipe,
                end: PipeEndKind::Write,
                ..
            } if self.pipe_ends(pipe, PipeEndKind::Read) > 0
                && self.pipes.len(pipe) + n.min(fds::PIPE_CAPACITY) > fds::PIPE_CAPACITY =>
            {
                Some(BlockReason::Write { fd, pipe: pipe.clone() })
            }
            _ => None,
        }
    }

    /// Tras cada paso: quien esperaba un pipe despierta si ya hay datos, espacio o EOF.
    fn wake_io(&mut self) {
        for i in 0..self.procs.len() {
            let still = match &self.procs[i].state {
                PState::Blocked(BlockReason::Read { fd, pipe: Some(_), .. }) => self.read_blocks(i, *fd).is_some(),
                PState::Blocked(BlockReason::Write { fd, .. }) => {
                    let n = match self.procs[i].sys {
                        Some((nr, a)) if nr == arch::SYS_WRITE => a[2] as usize,
                        Some((_, a)) => self.iov_len(i, a[1], a[2]),
                        None => 0,
                    };
                    self.write_blocks(i, *fd, n).is_some()
                }
                _ => true,
            };
            if !still {
                self.unblock(i);
            }
        }
    }

    fn iov_len(&self, i: usize, iov: u64, count: u64) -> usize {
        let t = &self.procs[i].tracee;
        (0..count.min(64))
            .map(|k| t.read_u64(iov + k * 16 + 8).unwrap_or(0) as usize)
            .sum()
    }

    fn gather(&self, i: usize, vector: bool, buf: u64, count: u64, n: usize) -> Vec<u8> {
        let t = &self.procs[i].tracee;
        let mut bytes = Vec::new();
        if vector {
            for k in 0..count.min(64) {
                let base = t.read_u64(buf + k * 16).unwrap_or(0);
                let len = t.read_u64(buf + k * 16 + 8).unwrap_or(0);
                bytes.extend(t.read(base, len.min(1 << 16) as usize).unwrap_or_default());
            }
        } else {
            bytes = t.read(buf, n.min(1 << 16)).unwrap_or_default();
        }
        bytes.truncate(n);
        bytes
    }

    /// read, write y sus variantes vectoriales, según a qué apunta el fd. Devuelve true si hubo
    /// E/S visible (terminal, stdin o pipe).
    fn io_exit(&mut self, i: usize, nr: u64, args: [u64; 6], ret: i64) -> bool {
        let vpid = self.procs[i].vpid;
        let tid = self.procs[i].tid;
        let fd = args[0] as u32;
        let write = nr == arch::SYS_WRITE || nr == arch::SYS_WRITEV;
        let vector = nr == arch::SYS_WRITEV || nr == arch::SYS_READV;
        let entry = self.procs[i].fds.get(&fd).cloned();
        let pipe = fds::pipe_of(entry.as_ref()).map(|(id, _)| id.to_string());
        if ret < 0 {
            if write && ret == -(libc::EPIPE as i64) && pipe.is_some() {
                self.events.push(Event::Write {
                    pid: vpid,
                    tid,
                    fd,
                    pipe,
                    terminal: false,
                    bytes: String::new(),
                    n: 0,
                    epipe: true,
                });
                return true;
            }
            return false;
        }
        let bytes = self.gather(i, vector, args[1], args[2], ret as usize);
        let shown = latin1(&bytes[..bytes.len().min(256)]);
        match (entry, write) {
            (Some(Fd::Terminal { .. }), true) => self.terminal_write(i, fd, &bytes),
            (Some(Fd::Pipe { pipe, .. }), true) => {
                self.pipes.write(&pipe, &bytes);
                self.events.push(Event::Write {
                    pid: vpid,
                    tid,
                    fd,
                    pipe: Some(pipe),
                    terminal: false,
                    bytes: shown,
                    n: ret as u64,
                    epipe: false,
                });
            }
            (Some(Fd::Stdin { .. } | Fd::Pipe { .. }), false) => {
                match &pipe {
                    Some(id) => self.pipes.read(id, ret as usize),
                    None => self.stdin_consumed += ret as u64,
                }
                self.events.push(Event::Read {
                    pid: vpid,
                    tid,
                    fd,
                    stdin: pipe.is_none(),
                    pipe,
                    bytes: shown,
                    n: ret as u64,
                    eof: ret == 0,
                    into: (!vector).then(|| hex(args[1])),
                });
            }
            _ => return false,
        }
        true
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
            tid: p.tid,
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
                let child = Pid::from_raw(child as i32);
                match self.procs[i].sys {
                    Some((nr, args)) if ev == libc::PTRACE_EVENT_CLONE && self.clone_is_thread(i, nr, &args) => {
                        self.spawn_thread(i, child)
                    }
                    _ => self.spawn(i, child, ev == libc::PTRACE_EVENT_VFORK),
                }
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
            tid: vpid,
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
            fds: p.fds.clone(),
            sigs: p.sigs.for_child(),
            handlers: p.handlers.clone(),
            suspend_mask: None,
            stack: p.stack.clone(),
            start: None,
            retval: None,
            pthread: 0,
            futex: None,
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
                self.procs[pi].tracee.write(call.ret_addr, &call.orig);
            }
            self.release_vfork_parent(parent);
        }
        let p = &mut self.procs[i];
        p.tracee = Tracee::attach(p.pid).map_err(|_| lost())?;
        p.maps = p.tracee.maps();
        p.heap = Heap::default();
        fds::close_on_exec(&mut p.fds);
        p.sigs.exec();
        p.handlers.clear();
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
        let tid = self.procs[i].tid;
        if self.procs[i].user_image() && matches!(self.procs[i].at, At::Lib(_)) {
            self.capture_in_call(i);
        }
        self.sync_block_event(i, &reason);
        self.procs[i].state = PState::Blocked(reason.clone());
        self.events.push(Event::Block { pid: vpid, tid, reason });
    }

    fn unblock(&mut self, i: usize) {
        let p = &mut self.procs[i];
        p.state = PState::Ready;
        self.events.push(Event::Unblock {
            pid: p.vpid,
            tid: p.tid,
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
        p.sigs = SigState::default();
        p.handlers.clear();
        p.fds.clear();
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
        self.timers.retain(|(p, _)| *p != vpid);
        // El kernel avisa al padre con SIGCHLD; solo se dibuja si el padre lo atiende.
        if let Some(pi) = self.procs[i].ppid.and_then(|pp| self.index_of(pp))
            && self.procs[pi].proc_alive()
            && matches!(
                self.procs[pi].sigs.actions.get(&libc::SIGCHLD),
                Some(Action::Handler(_))
            )
        {
            let to = self.procs[pi].vpid;
            self.signal_sent(
                SignalSource::Kernel {
                    cause: KernelCause::Sigchld,
                },
                vec![pi],
                to as i32,
                libc::SIGCHLD,
            );
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
        let mut frames = memory::unwind(self.debug, &p.tracee, regs, line);
        let func = frames
            .first()
            .map(|f| self.debug.functions[f.func].name.clone())
            .unwrap_or_default();
        // Debajo de cada handler, la pila que la señal interrumpió.
        let mut marks = Vec::new();
        for h in p.handlers.iter().rev() {
            marks.push((frames.len(), signals::name(h.sig)));
            let (r, l) = match &h.at {
                At::Lib(call) => {
                    let mut r = call.regs;
                    r.set_pc(arch::call_site(call.ret_addr));
                    (r, self.debug.line_of(arch::call_site(call.ret_addr)).unwrap_or(0))
                }
                _ => (h.regs, h.last.as_ref().map_or(0, |s| s.line)),
            };
            frames.extend(memory::unwind(self.debug, &p.tracee, &r, l));
        }
        let mut snap = {
            let mut reader = Reader::new(self.debug, &p.tracee, &p.heap, self.binary_path.clone(), p.maps.clone());
            reader.extra_stacks(self.thread_stacks(i));
            reader.snapshot(&frames, p.tid, t)
        };
        if let Some(stack) = snap.stacks.get_mut(&p.tid) {
            let mut from = 0;
            for (end, sig) in marks {
                for f in stack.iter_mut().take(end).skip(from) {
                    f.signal = Some(sig.clone());
                }
                from = end;
            }
        }
        let vpid = p.vpid;
        self.describe_sync(vpid, &mut snap);
        let p = &self.procs[i];
        let stack = snap.stacks.remove(&p.tid).unwrap_or_default();
        let vpid = p.vpid;
        self.procs[i].stack = stack;
        snap.stacks = self
            .procs
            .iter()
            .filter(|q| q.vpid == vpid && q.alive())
            .map(|q| (q.tid, q.stack.clone()))
            .collect();
        let mem = self.snapshot_id(snap);
        let p = &mut self.procs[i];
        p.mem = Some(mem);
        p.last = Some(Stop {
            pc: regs.pc(),
            line,
            cfa: self.debug.cfa_at(regs.pc(), regs.fp()),
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
        regs.set_pc(arch::call_site(call.ret_addr));
        let line = self.debug.line_of(arch::call_site(call.ret_addr)).unwrap_or(0);
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

    fn stdin_state(&self) -> StdinState {
        StdinState {
            size: self.stdin_len(),
            consumed: self.stdin_consumed,
            eof: self.opts.stdin_eof,
        }
    }

    fn thread_view(&self, k: usize, actor: Option<usize>, t: u64) -> Thread {
        let p = &self.procs[k];
        let running = actor == Some(k) && t > 0;
        let (tstate, blocked_on) = match &p.state {
            PState::Blocked(r) => (ThreadState::Blocked, Some(r.clone())),
            PState::Ready if running => (ThreadState::Running, None),
            PState::Ready => (ThreadState::Ready, None),
            _ => (ThreadState::Exited, None),
        };
        let (line, func) = match &p.last {
            Some(s) if p.alive() && p.user_image() => (Some(s.line), Some(s.func.clone())),
            _ => (None, None),
        };
        let in_call = match &p.at {
            At::Lib(c) if p.alive() => Some(c.name.clone()),
            _ => None,
        };
        Thread {
            tid: p.tid,
            main: p.is_leader(),
            state: tstate,
            line,
            func,
            in_call,
            blocked_on,
            start: p.start.clone(),
            holds: self.holds(k),
            in_handler: p.handlers.last().filter(|_| p.alive()).map(|h| signals::name(h.sig)),
            retval: p.retval.clone().filter(|_| !p.alive()),
        }
    }

    /// El proceso cuyo hilo principal es `i`, con todos sus hilos.
    fn view(&self, i: usize, actor: Option<usize>, t: u64) -> Process {
        let p = &self.procs[i];
        let threads: Vec<Thread> = (0..self.procs.len())
            .filter(|&k| self.procs[k].vpid == p.vpid)
            .map(|k| self.thread_view(k, actor, t))
            .collect();
        let pstate = match p.state {
            PState::Zombie => ProcessState::Zombie,
            PState::Reaped => ProcessState::Reaped,
            _ if threads.iter().any(|th| th.state == ThreadState::Running) => ProcessState::Running,
            _ if threads.iter().any(|th| th.state == ThreadState::Ready) => ProcessState::Ready,
            _ => ProcessState::Blocked,
        };
        Process {
            pid: p.vpid,
            ppid: p.ppid,
            pgid: p.pgid,
            state: pstate,
            created_at: p.created_at,
            image: p.image.clone(),
            exit: p.exit.clone(),
            fds: if p.proc_alive() { p.fds.clone() } else { BTreeMap::new() },
            signals: p.sigs.view(),
            threads,
            mem: if p.state == PState::Reaped { None } else { p.mem.clone() },
        }
    }

    /// Mutex que tiene tomados el hilo `k`.
    fn holds(&self, k: usize) -> Vec<String> {
        let tid = self.procs[k].tid;
        self.sync
            .iter()
            .filter(|e| e.kind == 'm' && e.pid == self.procs[k].vpid && self.mutex_owner(e) == Some(tid))
            .map(|e| e.id.clone())
            .collect()
    }

    // ---------- hilos ----------

    fn leader_idx(&self, i: usize) -> usize {
        self.index_of(self.procs[i].vpid).unwrap_or(i)
    }

    fn copy_shared(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let (src, dst) = if from < to {
            let (l, r) = self.procs.split_at_mut(to);
            (&l[from], &mut r[0])
        } else {
            let (l, r) = self.procs.split_at_mut(from);
            (&r[0], &mut l[to])
        };
        dst.take_shared(src);
    }

    /// Antes del paso de un hilo: toma el estado del proceso que guarda el hilo principal.
    fn pull(&mut self, i: usize) {
        let l = self.leader_idx(i);
        self.copy_shared(l, i);
    }

    /// Después del paso: el estado del proceso que dejó el hilo pasa a todos sus hermanos.
    fn push(&mut self, i: usize) {
        let vpid = self.procs[i].vpid;
        for k in 0..self.procs.len() {
            if k != i && self.procs[k].vpid == vpid {
                self.copy_shared(i, k);
            }
        }
    }

    /// Un hilo que se detuvo dentro de una llamada deja su breakpoint en memoria compartida: se
    /// quita para que otro hilo no lo pise (se vuelve a poner al reanudar).
    fn disarm(&mut self, i: usize) {
        if let At::Lib(call) = &self.procs[i].at
            && self.procs[i].alive()
        {
            self.procs[i].tracee.write(call.ret_addr, &call.orig);
        }
    }

    fn siblings_alive(&self, i: usize) -> bool {
        let p = &self.procs[i];
        self.procs
            .iter()
            .any(|q| q.vpid == p.vpid && q.tid != p.tid && q.alive())
    }

    fn set_proc_state(&mut self, vpid: u32, state: PState) {
        for p in self.procs.iter_mut().filter(|p| p.vpid == vpid) {
            if state == PState::Reaped {
                p.mem = None;
            }
            p.state = state.clone();
        }
    }

    fn clone_is_thread(&self, i: usize, nr: u64, args: &[u64; 6]) -> bool {
        let flags = if nr == arch::SYS_CLONE3 {
            self.procs[i].tracee.read_u64(args[0]).unwrap_or(0)
        } else {
            args[0]
        };
        flags & libc::CLONE_THREAD as u64 != 0
    }

    /// pthread_create: el hilo nuevo parte en start_thread de glibc y corre hasta su función.
    fn spawn_thread(&mut self, i: usize, child: Pid) -> Res<()> {
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
        let tid = VPID + self.procs.len() as u32;
        let (start_addr, arg) = match &self.procs[i].at {
            At::Lib(call) => (call.args[2], call.args[3]),
            _ => (0, 0),
        };
        let f = self.debug.functions.iter().find(|f| f.low == start_addr);
        let (name, decl) = f.map_or((hex(start_addr), 0), |f| (f.name.clone(), f.decl_line));
        let orig = tracee.read(start_addr, arch::BREAKPOINT.len()).unwrap_or_default();
        let p = &self.procs[i];
        let start = ThreadStart {
            func: name.clone(),
            arg: pointer_value(arg),
        };
        let t = Proc {
            vpid: p.vpid,
            tid,
            pid: child,
            ppid: p.ppid,
            pgid: p.pgid,
            created_at: p.created_at,
            tracee,
            heap: p.heap.clone(),
            maps: p.maps.clone(),
            image: p.image.clone(),
            at: At::Start { addr: start_addr, orig },
            state: PState::Ready,
            exit: None,
            last: Some(Stop {
                pc: start_addr,
                line: decl,
                cfa: 0,
                func: name.clone(),
            }),
            calls: Vec::new(),
            sys: None,
            sig: None,
            exec_args: None,
            vfork_parent: None,
            vfork_child: None,
            mem: p.mem.clone(),
            output_bytes: p.output_bytes,
            fds: p.fds.clone(),
            sigs: p.sigs.clone(),
            handlers: Vec::new(),
            suspend_mask: None,
            stack: vec![Frame {
                func: name.clone(),
                line: decl,
                params: Vec::new(),
                locals: Vec::new(),
                signal: None,
            }],
            start: Some(start.clone()),
            retval: None,
            pthread: 0,
            futex: None,
        };
        let (pid, creator) = (p.vpid, p.tid);
        self.procs.push(t);
        self.last_thread = Some(self.procs.len() - 1);
        self.events.push(Event::ThreadCreate {
            pid,
            creator,
            tid,
            func: name,
            arg: start.arg,
        });
        Ok(())
    }

    /// Un hilo terminó. Si era el último (o fue exit_group, o una señal), termina el proceso.
    fn on_task_exit(&mut self, i: usize, status: WaitStatus) {
        let only_thread = matches!(status, WaitStatus::Exited(..))
            && matches!(self.procs[i].sys, Some((nr, _)) if nr == arch::SYS_EXIT);
        if only_thread && (self.siblings_alive(i) || !self.procs[i].is_leader()) {
            self.live.lock().unwrap().retain(|p| *p != self.procs[i].pid);
            self.thread_exit(i);
            let l = self.leader_idx(i);
            // pthread_exit en main y ya no queda nadie: el proceso termina de verdad.
            if !self.siblings_alive(l) && self.procs[l].state == PState::Gone && self.procs[l].proc_alive() {
                let _ = ptrace::syscall(self.procs[l].pid, None);
                if let Ok(st @ (WaitStatus::Exited(..) | WaitStatus::Signaled(..))) =
                    waitpid(self.procs[l].pid, Some(WaitPidFlag::__WALL))
                {
                    self.process_exit(l, st);
                }
            }
            return;
        }
        self.process_exit(i, status);
    }

    fn process_exit(&mut self, i: usize, status: WaitStatus) {
        let vpid = self.procs[i].vpid;
        // Los demás hilos mueren con el proceso: se recogen antes que el principal.
        for k in 0..self.procs.len() {
            if k == i
                || self.procs[k].vpid != vpid
                || !(self.procs[k].alive() || self.procs[k].is_leader() && self.procs[k].state == PState::Gone)
            {
                continue;
            }
            let pid = self.procs[k].pid;
            let _ = nix::sys::signal::kill(pid, Signal::SIGKILL);
            while let Ok(st) = waitpid(pid, Some(WaitPidFlag::__WALL)) {
                if matches!(st, WaitStatus::Exited(..) | WaitStatus::Signaled(..)) {
                    break;
                }
            }
            self.live.lock().unwrap().retain(|p| *p != pid);
            self.procs[k].state = PState::Gone;
        }
        let l = self.leader_idx(i);
        self.copy_shared(i, l);
        self.on_exit(l, status);
        let state = self.procs[l].state.clone();
        self.push(l);
        self.set_proc_state(vpid, state);
    }

    fn thread_exit(&mut self, i: usize) {
        let p = &mut self.procs[i];
        p.state = PState::Gone;
        p.futex = None;
        self.events.push(Event::Exit {
            pid: p.vpid,
            tid: Some(p.tid),
            scope: ExitScope::Thread,
            code: None,
            signal: None,
            retval: p.retval.clone(),
        });
    }

    fn joined_tid(&self, i: usize, pthread: u64) -> u32 {
        let vpid = self.procs[i].vpid;
        self.procs
            .iter()
            .find(|p| p.vpid == vpid && p.pthread == pthread && pthread != 0)
            .map_or(0, |p| p.tid)
    }

    /// Por qué espera un hilo en un futex, según la llamada de pthread en la que está.
    fn futex_reason(&mut self, i: usize, uaddr: u64) -> BlockReason {
        let pid = self.procs[i].vpid;
        let At::Lib(call) = self.procs[i].at.clone() else {
            return BlockReason::Pause;
        };
        match call.name.as_str() {
            "pthread_mutex_lock" | "pthread_mutex_timedlock" => {
                let id = self.sync_id(pid, call.args[0], 'm');
                BlockReason::Mutex {
                    owner: self.mutex_owner_at(pid, call.args[0]),
                    id,
                }
            }
            "pthread_cond_wait" | "pthread_cond_timedwait" if (call.args[1]..call.args[1] + 40).contains(&uaddr) => {
                let id = self.sync_id(pid, call.args[1], 'm');
                BlockReason::Mutex {
                    owner: self.mutex_owner_at(pid, call.args[1]),
                    id,
                }
            }
            "pthread_cond_wait" | "pthread_cond_timedwait" => BlockReason::Cond {
                id: self.sync_id(pid, call.args[0], 'c'),
                mutex: self.sync_id(pid, call.args[1], 'm'),
            },
            "pthread_join" => BlockReason::Join {
                tid: self.joined_tid(i, call.args[0]),
            },
            "sem_wait" | "sem_timedwait" => BlockReason::Sem {
                id: self.sync_id(pid, call.args[0], 's'),
            },
            _ => BlockReason::Mutex {
                id: format!("futex {uaddr:#x}"),
                owner: None,
            },
        }
    }

    /// Quien espera en un futex despierta cuando el valor cambió: el kernel devolverá EAGAIN y
    /// glibc vuelve a intentar (tomar el mutex, ver la señal de la condición, el fin del hilo…).
    fn wake_futex(&mut self) {
        for i in 0..self.procs.len() {
            let Some((addr, val)) = self.procs[i].futex else {
                continue;
            };
            if !matches!(self.procs[i].state, PState::Blocked(_)) {
                continue;
            }
            let now = self.procs[i]
                .tracee
                .read(addr, 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()));
            if now == Some(val) {
                continue;
            }
            self.procs[i].futex = None;
            if let PState::Blocked(BlockReason::Cond { id, .. }) = &self.procs[i].state {
                let tid = self.procs[i].tid;
                for ev in self.events.iter_mut() {
                    if let Event::Cond {
                        op: CondOp::Signal | CondOp::Broadcast,
                        id: cid,
                        woke: Some(woke),
                        ..
                    } = ev
                        && cid == id
                    {
                        woke.push(tid);
                    }
                }
            }
            self.unblock(i);
        }
    }

    // ---------- mutex, variables de condición y semáforos ----------

    fn sync_id(&mut self, pid: u32, addr: u64, kind: char) -> String {
        if let Some(e) = self.sync.iter().find(|e| e.pid == pid && e.addr == addr) {
            return e.id.clone();
        }
        let n = self.sync.iter().filter(|e| e.kind == kind).count();
        let id = format!("{kind}{n}");
        self.sync.push(SyncEntry {
            id: id.clone(),
            kind,
            pid,
            addr,
        });
        id
    }

    fn note_sync_call(&mut self, i: usize, name: &str, args: &[u64; 4]) {
        let pid = self.procs[i].vpid;
        if name.starts_with("pthread_mutex_") {
            self.sync_id(pid, args[0], 'm');
        } else if name.starts_with("pthread_cond_") {
            self.sync_id(pid, args[0], 'c');
            if name.contains("wait") {
                self.sync_id(pid, args[1], 'm');
            }
        } else if name.starts_with("sem_") {
            self.sync_id(pid, args[0], 's');
        }
    }

    fn read_u32(&self, pid: u32, addr: u64) -> Option<u32> {
        let k = self.index_of(pid)?;
        self.procs[k]
            .tracee
            .read(addr, 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    /// Dueño de un mutex de glibc: el campo `__owner` guarda el TID real de quien lo tiene.
    fn mutex_owner_at(&self, pid: u32, addr: u64) -> Option<u32> {
        let owner = self.read_u32(pid, addr + 8)? as i64;
        (owner != 0).then(|| self.vpid_of_real(owner)).flatten()
    }

    fn mutex_owner(&self, e: &SyncEntry) -> Option<u32> {
        self.mutex_owner_at(e.pid, e.addr)
    }

    fn sem_value(&self, pid: u32, addr: u64) -> i64 {
        self.read_u32(pid, addr).unwrap_or(0) as i64
    }

    fn sync_event(&mut self, i: usize, name: &str, args: &[u64; 4], ret: u64) {
        let (pid, tid) = (self.procs[i].vpid, self.procs[i].tid);
        let ok = ret as i32 == 0;
        let ev = match name {
            "pthread_mutex_lock" | "pthread_mutex_trylock" | "pthread_mutex_unlock" => Event::Mutex {
                op: match name {
                    "pthread_mutex_lock" => MutexOp::Lock,
                    "pthread_mutex_trylock" => MutexOp::Trylock,
                    _ => MutexOp::Unlock,
                },
                id: self.sync_id(pid, args[0], 'm'),
                tid,
                result: match name {
                    "pthread_mutex_unlock" => SyncResult::Released,
                    _ if ok => SyncResult::Acquired,
                    _ => SyncResult::Busy,
                },
            },
            "pthread_cond_wait" | "pthread_cond_timedwait" => Event::Cond {
                op: CondOp::Wake,
                id: self.sync_id(pid, args[0], 'c'),
                tid,
                woke: None,
            },
            "pthread_cond_signal" | "pthread_cond_broadcast" => Event::Cond {
                op: if name.ends_with("signal") {
                    CondOp::Signal
                } else {
                    CondOp::Broadcast
                },
                id: self.sync_id(pid, args[0], 'c'),
                tid,
                woke: Some(Vec::new()),
            },
            "sem_wait" | "sem_trywait" | "sem_post" => Event::Sem {
                op: match name {
                    "sem_wait" => SemOp::Wait,
                    "sem_trywait" => SemOp::Trywait,
                    _ => SemOp::Post,
                },
                id: self.sync_id(pid, args[0], 's'),
                tid,
                value: self.sem_value(pid, args[0]),
                result: match name {
                    "sem_post" => SyncResult::Posted,
                    _ if ok => SyncResult::Acquired,
                    _ => SyncResult::Busy,
                },
            },
            "pthread_create" => {
                if let Some(k) = self.last_thread.take() {
                    self.procs[k].pthread = self.procs[i].tracee.read_u64(args[0]).unwrap_or(0);
                }
                return;
            }
            "pthread_join" => {
                let target = self.joined_tid(i, args[0]);
                let retval = self
                    .procs
                    .iter()
                    .find(|p| p.vpid == pid && p.tid == target)
                    .and_then(|p| p.retval.clone());
                Event::Join { tid, target, retval }
            }
            _ => return,
        };
        self.events.push(ev);
    }

    fn sync_block_event(&mut self, i: usize, reason: &BlockReason) {
        let (pid, tid) = (self.procs[i].vpid, self.procs[i].tid);
        let ev = match reason {
            BlockReason::Mutex { id, .. } if !id.starts_with("futex") => Event::Mutex {
                op: MutexOp::Lock,
                id: id.clone(),
                tid,
                result: SyncResult::Blocked,
            },
            BlockReason::Cond { id, .. } => Event::Cond {
                op: CondOp::Wait,
                id: id.clone(),
                tid,
                woke: None,
            },
            BlockReason::Sem { id } => {
                let addr = self.sync.iter().find(|e| e.id == *id).map_or(0, |e| e.addr);
                Event::Sem {
                    op: SemOp::Wait,
                    id: id.clone(),
                    tid,
                    value: self.sem_value(pid, addr),
                    result: SyncResult::Blocked,
                }
            }
            _ => return,
        };
        self.events.push(ev);
    }

    /// Nombre de la variable del programa que vive en `addr`, si la instantánea la muestra.
    fn var_name(&self, pid: u32, addr: u64) -> Option<String> {
        let k = self.index_of(pid)?;
        let snap = self.snapshots.get(self.procs[k].mem.as_ref()?)?;
        let want = hex(addr);
        snap.globals
            .iter()
            .chain(
                snap.stacks
                    .values()
                    .flatten()
                    .flat_map(|f| f.params.iter().chain(&f.locals)),
            )
            .find(|v| v.addr == want)
            .map(|v| v.name.clone())
    }

    fn sync_view(&self) -> Vec<SyncObject> {
        let waiting = |pred: &dyn Fn(&BlockReason) -> bool, pid: u32| -> Vec<u32> {
            self.procs
                .iter()
                .filter(|p| p.vpid == pid && matches!(&p.state, PState::Blocked(r) if pred(r)))
                .map(|p| p.tid)
                .collect()
        };
        self.sync
            .iter()
            .filter(|e| self.index_of(e.pid).is_some_and(|k| self.procs[k].proc_alive()))
            .map(|e| {
                let (id, pid, addr, name) = (e.id.clone(), e.pid, hex(e.addr), self.var_name(e.pid, e.addr));
                match e.kind {
                    'm' => SyncObject::Mutex {
                        waiters: waiting(&|r| matches!(r, BlockReason::Mutex { id: m, .. } if *m == e.id), pid),
                        owner: self.mutex_owner(e),
                        id,
                        pid,
                        addr,
                        name,
                    },
                    'c' => SyncObject::Cond {
                        waiters: waiting(&|r| matches!(r, BlockReason::Cond { id: c, .. } if *c == e.id), pid),
                        id,
                        pid,
                        addr,
                        name,
                    },
                    _ => SyncObject::Sem {
                        waiters: waiting(&|r| matches!(r, BlockReason::Sem { id: s } if *s == e.id), pid),
                        value: self.sem_value(pid, e.addr),
                        id,
                        pid,
                        addr,
                        name,
                    },
                }
            })
            .collect()
    }

    /// Los tipos de pthread son opacos: en vez de sus bytes internos (que guardan TIDs reales) se
    /// muestra su estado.
    fn describe_sync(&self, pid: u32, snap: &mut MemorySnapshot) {
        fn walk(e: &Engine, pid: u32, v: &mut Var) {
            let addr = u64::from_str_radix(v.addr.trim_start_matches("0x"), 16).unwrap_or(0);
            let note = match v.ty.as_str() {
                "pthread_mutex_t" => Some(match e.mutex_owner_at(pid, addr) {
                    Some(t) => format!("mutex tomado por el hilo {t}"),
                    None => "mutex libre".to_string(),
                }),
                "pthread_cond_t" => Some("variable de condición".to_string()),
                "sem_t" => Some(format!("semáforo = {}", e.sem_value(pid, addr))),
                "pthread_t" => {
                    let k = e.index_of(pid);
                    let value = k.and_then(|k| e.procs[k].tracee.read_u64(addr)).unwrap_or(0);
                    e.procs
                        .iter()
                        .find(|p| p.vpid == pid && p.pthread == value && value != 0)
                        .map(|p| format!("hilo {}", p.tid))
                }
                _ => None,
            };
            if let Some(note) = note {
                v.value = Value::Opaque { note };
                v.uninit = false;
                return;
            }
            match &mut v.value {
                Value::Struct { fields } | Value::Union { fields } => fields.iter_mut().for_each(|f| walk(e, pid, f)),
                _ => {}
            }
        }
        for v in snap.globals.iter_mut() {
            walk(self, pid, v);
        }
        for f in snap.stacks.values_mut().flatten() {
            for v in f.params.iter_mut().chain(f.locals.iter_mut()) {
                walk(self, pid, v);
            }
        }
    }

    /// Mapeos que contienen la pila de cada hilo del proceso de `i`.
    fn thread_stacks(&self, i: usize) -> Vec<(u64, u64)> {
        let vpid = self.procs[i].vpid;
        let maps = &self.procs[i].maps;
        self.procs
            .iter()
            .filter(|p| p.vpid == vpid && !p.is_leader())
            .filter_map(|p| p.last.as_ref().map(|s| s.cfa).filter(|c| *c != 0))
            .filter_map(|cfa| {
                maps.iter()
                    .find(|m| cfa >= m.start && cfa <= m.end)
                    .map(|m| (m.start, m.end))
            })
            .collect()
    }

    fn commit(&mut self, actor: Option<usize>, executed: Option<ExecutedLine>, choices: Vec<TaskRef>) {
        let t = if self.steps.is_empty() { 0 } else { self.t + 1 };
        self.take_output(t);
        let processes = (0..self.procs.len())
            .filter(|&k| self.procs[k].is_leader())
            .map(|k| self.view(k, actor, t))
            .collect();
        self.steps.push(Step {
            t,
            actor: actor.map(|a| TaskRef {
                pid: self.procs[a].vpid,
                tid: self.procs[a].tid,
            }),
            executed,
            choices,
            clock: self.clock,
            events: std::mem::take(&mut self.events),
            processes,
            pipes: self.pipe_view(),
            stdin: self.stdin_state(),
            signals: self
                .procs
                .iter()
                .filter(|p| p.is_leader())
                .flat_map(|p| p.sigs.in_flight(p.vpid))
                .collect(),
            timers: self
                .timers
                .iter()
                .map(|(pid, at)| Timer {
                    pid: *pid,
                    signal: "SIGALRM".into(),
                    fire_at: *at,
                })
                .collect(),
            sync: self.sync_view(),
        });
        self.t = t;
    }

    fn pipe_view(&self) -> Vec<Pipe> {
        let alive: Vec<(u32, &FdTable)> = self
            .procs
            .iter()
            .filter(|p| p.is_leader() && p.proc_alive())
            .map(|p| (p.vpid, &p.fds))
            .collect();
        let reading: Vec<(u32, String)> = self
            .procs
            .iter()
            .filter_map(|p| match &p.state {
                PState::Blocked(BlockReason::Read { pipe: Some(id), .. }) => Some((p.vpid, id.clone())),
                _ => None,
            })
            .collect();
        fds::view(&self.pipes, &alive, &reading, latin1)
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
        let pending: Vec<&Proc> = self
            .procs
            .iter()
            .filter(|p| p.proc_alive() && (p.state != PState::Gone || p.is_leader()))
            .collect();
        for p in &pending {
            let _ = nix::sys::signal::kill(p.pid, Signal::SIGKILL);
        }
        // El kernel no informa la muerte del hilo principal hasta que se recogen los demás hilos.
        for p in pending
            .iter()
            .filter(|p| !p.is_leader())
            .chain(pending.iter().filter(|p| p.is_leader()))
        {
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
            End::AwaitingInput(pid, tid) => {
                self.push_final_event(Event::StdinNeeded { pid, tid });
                trace.outcome = Outcome::AwaitingInput { pid, tid };
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

fn call_summary(name: &str, args: &[u64; 4], tracee: &Tracee) -> Option<String> {
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

/// Un `void *` como lo muestra la traza: el argumento y el valor de retorno de un hilo.
fn pointer_value(v: u64) -> Value {
    Value::Scalar {
        value: Scalar::Number(v.into()),
        repr: Some(if v == 0 { "NULL".into() } else { hex(v) }),
    }
}
