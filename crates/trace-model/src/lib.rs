//! Tipos de la traza, en correspondencia 1:1 con `schema/trace.schema.json`.
//!
//! El esquema es la fuente de verdad. Las pruebas de `tests/roundtrip.rs` comprueban que toda
//! traza de `traces/` se lee y se vuelve a escribir sin perder ni agregar nada.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Pid = u32;
pub type Tid = u32;
pub type Fdnum = u32;
/// Dirección en hexadecimal, `"0x7fffffffe3d0"`.
pub type Addr = String;
pub type SnapshotId = String;
pub type SignalName = String;
/// Cada carácter U+0000–U+00FF representa exactamente un byte.
pub type Bytes = String;

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Trace {
    pub version: u32,
    pub arch: Arch,
    pub source: String,
    pub stdin: Bytes,
    pub run: RunConfig,
    pub compile: CompileResult,
    pub outcome: Outcome,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_reason: Option<TruncatedReason>,
    pub steps: Vec<Step>,
    pub snapshots: BTreeMap<SnapshotId, MemorySnapshot>,
    pub output: Vec<OutputChunk>,
    pub summary: Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arch {
    #[serde(rename = "x86_64")]
    X86_64,
    #[serde(rename = "aarch64")]
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TruncatedReason {
    Steps,
    Time,
    Processes,
    Threads,
    Memory,
    Output,
    TraceSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRef {
    pub pid: Pid,
    pub tid: Tid,
}

// ---------- ejecución ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunConfig {
    pub policy: Policy,
    pub seed: u64,
    pub stdin_eof: bool,
    pub schedule: Vec<TaskRef>,
    pub injections: Vec<Injection>,
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    RoundRobin,
    Random,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Injection {
    pub t: u64,
    pub signal: SignalName,
    pub target: InjectionTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InjectionTarget {
    Foreground,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_processes: u32,
    pub max_threads_per_process: u32,
    pub max_steps: u64,
    pub wall_time_ms: u64,
    pub memory_bytes: u64,
    pub output_bytes_per_process: u64,
    pub trace_bytes_compressed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileResult {
    pub ok: bool,
    pub command: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub line: u32,
    pub col: u32,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Outcome {
    Exited { code: i32 },
    Signaled { signal: SignalName },
    Deadlock { tasks: Vec<TaskRef> },
    AwaitingInput { pid: Pid, tid: Tid },
    Truncated,
    CompileError,
}

// ---------- pasos ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub t: u64,
    pub actor: Option<TaskRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed: Option<ExecutedLine>,
    pub choices: Vec<TaskRef>,
    pub clock: u64,
    pub events: Vec<Event>,
    pub processes: Vec<Process>,
    pub pipes: Vec<Pipe>,
    pub stdin: StdinState,
    pub signals: Vec<InFlightSignal>,
    pub timers: Vec<Timer>,
    pub sync: Vec<SyncObject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutedLine {
    pub line: u32,
    #[serde(rename = "fn")]
    pub func: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StdinState {
    pub size: u64,
    pub consumed: u64,
    pub eof: bool,
}

// ---------- procesos e hilos ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Process {
    pub pid: Pid,
    pub ppid: Option<Pid>,
    pub pgid: Pid,
    pub state: ProcessState,
    pub created_at: u64,
    pub image: ProcessImage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitStatus>,
    pub fds: BTreeMap<Fdnum, Fd>,
    pub signals: ProcessSignals,
    pub threads: Vec<Thread>,
    pub mem: Option<SnapshotId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessState {
    Running,
    Ready,
    Blocked,
    Stopped,
    Zombie,
    Reaped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProcessImage {
    User { path: String },
    Blackbox { path: String, argv: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExitStatus {
    Code { code: i32 },
    Signal { signal: SignalName, core: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessSignals {
    pub mask: Vec<SignalName>,
    pub pending: Vec<SignalName>,
    pub actions: BTreeMap<SignalName, SignalAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum SignalAction {
    Handler {
        #[serde(rename = "fn")]
        func: String,
    },
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Fd {
    Stdin {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloexec: Option<bool>,
    },
    Terminal {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloexec: Option<bool>,
    },
    Pipe {
        pipe: String,
        end: PipeEndKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloexec: Option<bool>,
    },
    File {
        path: String,
        mode: FileMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloexec: Option<bool>,
    },
    Other {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloexec: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeEndKind {
    #[serde(rename = "r")]
    Read,
    #[serde(rename = "w")]
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    R,
    W,
    Rw,
    A,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Thread {
    pub tid: Tid,
    pub main: bool,
    pub state: ThreadState,
    /// Próxima línea a ejecutar.
    pub line: Option<u32>,
    #[serde(rename = "fn")]
    pub func: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_on: Option<BlockReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<ThreadStart>,
    pub holds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_handler: Option<SignalName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retval: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadState {
    Running,
    Ready,
    Blocked,
    Stopped,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadStart {
    #[serde(rename = "fn")]
    pub func: String,
    pub arg: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum BlockReason {
    Read {
        fd: Fdnum,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pipe: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        stdin: bool,
    },
    Write {
        fd: Fdnum,
        pipe: String,
    },
    Wait {
        /// -1 = cualquier hijo.
        target: i32,
    },
    Join {
        tid: Tid,
    },
    Mutex {
        id: String,
        owner: Option<Tid>,
    },
    Cond {
        id: String,
        mutex: String,
    },
    Sem {
        id: String,
    },
    Sleep {
        until: u64,
    },
    Pause,
    Sigsuspend,
}

// ---------- pipes, señales y sincronización ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pipe {
    pub id: String,
    pub created_by: Pid,
    pub size: u64,
    pub buffer: Bytes,
    pub capacity: u64,
    pub readers: Vec<PipeEnd>,
    pub writers: Vec<PipeEnd>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub broken: bool,
    pub warnings: Vec<PipeWarning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipeEnd {
    pub pid: Pid,
    pub fd: Fdnum,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipeWarning {
    pub kind: PipeWarningKind,
    pub pid: Pid,
    pub fd: Fdnum,
    pub end: PipeEndKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PipeWarningKind {
    UnclosedEnd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SignalSource {
    Process { pid: Pid, via: SignalVia },
    Kernel { cause: KernelCause },
    Timer { pid: Pid },
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalVia {
    Kill,
    Raise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KernelCause {
    #[serde(rename = "SIGCHLD")]
    Sigchld,
    #[serde(rename = "SIGPIPE")]
    Sigpipe,
    #[serde(rename = "fault")]
    Fault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InFlightSignal {
    pub signal: SignalName,
    pub from: SignalSource,
    pub to: Pid,
    pub status: SignalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalStatus {
    Pending,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Timer {
    pub pid: Pid,
    pub signal: SignalName,
    pub fire_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SyncObject {
    Mutex {
        id: String,
        pid: Pid,
        addr: Addr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        waiters: Vec<Tid>,
        owner: Option<Tid>,
    },
    Cond {
        id: String,
        pid: Pid,
        addr: Addr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        waiters: Vec<Tid>,
    },
    Sem {
        id: String,
        pid: Pid,
        addr: Addr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        waiters: Vec<Tid>,
        value: i64,
    },
}

// ---------- memoria ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySnapshot {
    pub globals: Vec<Var>,
    /// Clave = tid; frame [0] = el más reciente.
    pub stacks: BTreeMap<Tid, Vec<Frame>>,
    pub heap: Vec<HeapBlock>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    #[serde(rename = "fn")]
    pub func: String,
    pub line: u32,
    pub params: Vec<Var>,
    pub locals: Vec<Var>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<SignalName>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Var {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub addr: Addr,
    pub size: u64,
    pub value: Value,
    #[serde(default, skip_serializing_if = "is_false")]
    pub uninit: bool,
}

/// Mismo contenido que [`Var`]; se mantiene separado porque el esquema los distingue.
pub type Field = Var;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeapBlock {
    pub addr: Addr,
    pub size: u64,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    pub alloc_at: u64,
    pub alloc_line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freed_at: Option<u64>,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Value {
    Scalar {
        value: Scalar,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repr: Option<String>,
    },
    Pointer {
        target: Option<Addr>,
        #[serde(rename = "fn", default, skip_serializing_if = "Option::is_none")]
        func: Option<String>,
        /// Memoria válida que no se dibuja (literales de texto, datos de libc).
        #[serde(default, skip_serializing_if = "is_false")]
        outside: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<Bytes>,
    },
    Struct {
        fields: Vec<Field>,
    },
    Union {
        fields: Vec<Field>,
    },
    Array {
        length: u64,
        items: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<Bytes>,
    },
    Opaque {
        note: String,
    },
}

/// Enteros de 64 bits que no caben exactos en un double, NaN e Inf van como string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Bool(bool),
    Number(serde_json::Number),
    Text(String),
}

// ---------- salida y resumen ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputChunk {
    pub t: u64,
    pub pid: Pid,
    pub fd: Fdnum,
    pub stream: Stream,
    pub bytes: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    pub leaks: Vec<Leak>,
    pub mem_errors: Vec<MemErrorRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Leak {
    pub pid: Pid,
    pub addr: Addr,
    pub size: u64,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    pub alloc_at: u64,
    pub alloc_line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemErrorRef {
    pub t: u64,
    pub pid: Pid,
    pub tid: Tid,
    pub kind: MemErrorKind,
    pub addr: Addr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemErrorKind {
    UseAfterFree,
    DoubleFree,
    InvalidFree,
    Segfault,
}

// ---------- eventos ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Event {
    Call {
        #[serde(rename = "fn")]
        func: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    Return {
        #[serde(rename = "fn")]
        func: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
    },
    Fork {
        parent: Pid,
        child: Pid,
        #[serde(default, skip_serializing_if = "is_false")]
        vfork: bool,
    },
    ThreadCreate {
        pid: Pid,
        creator: Tid,
        tid: Tid,
        #[serde(rename = "fn")]
        func: String,
        arg: Value,
    },
    Exec {
        pid: Pid,
        path: String,
        argv: Vec<String>,
        blackbox: bool,
    },
    Exit {
        pid: Pid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tid: Option<Tid>,
        scope: ExitScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<SignalName>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retval: Option<Value>,
    },
    Wait {
        pid: Pid,
        target: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reaped: Option<Pid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<ExitStatus>,
    },
    Join {
        tid: Tid,
        target: Tid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retval: Option<Value>,
    },
    Reparent {
        pid: Pid,
        from: Pid,
        to: Pid,
    },
    Pipe {
        pid: Pid,
        pipe: String,
        fds: [Fdnum; 2],
    },
    Dup {
        pid: Pid,
        oldfd: Fdnum,
        newfd: Fdnum,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaced: Option<Fd>,
    },
    Close {
        pid: Pid,
        fd: Fdnum,
        was: Fd,
    },
    Read {
        pid: Pid,
        tid: Tid,
        fd: Fdnum,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pipe: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        stdin: bool,
        bytes: Bytes,
        n: u64,
        eof: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        into: Option<Addr>,
    },
    Write {
        pid: Pid,
        tid: Tid,
        fd: Fdnum,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pipe: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        terminal: bool,
        bytes: Bytes,
        n: u64,
        #[serde(default, skip_serializing_if = "is_false")]
        epipe: bool,
    },
    Block {
        pid: Pid,
        tid: Tid,
        reason: BlockReason,
    },
    Unblock {
        pid: Pid,
        tid: Tid,
    },
    SignalSend {
        from: SignalSource,
        /// Negativo = grupo de procesos.
        to: i32,
        signal: SignalName,
    },
    SignalDeliver {
        pid: Pid,
        tid: Tid,
        signal: SignalName,
        action: DeliverAction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handler: Option<String>,
    },
    SignalReturn {
        pid: Pid,
        tid: Tid,
        signal: SignalName,
    },
    Mutex {
        op: MutexOp,
        id: String,
        tid: Tid,
        result: SyncResult,
    },
    Cond {
        op: CondOp,
        id: String,
        tid: Tid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        woke: Option<Vec<Tid>>,
    },
    Sem {
        op: SemOp,
        id: String,
        tid: Tid,
        value: i64,
        result: SyncResult,
    },
    Malloc {
        pid: Pid,
        #[serde(rename = "fn")]
        func: String,
        addr: Option<Addr>,
        size: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_addr: Option<Addr>,
    },
    Free {
        pid: Pid,
        addr: Addr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<FreeError>,
    },
    MemError {
        pid: Pid,
        tid: Tid,
        kind: MemErrorKind,
        addr: Addr,
    },
    StdinNeeded {
        pid: Pid,
        tid: Tid,
    },
    Deadlock {
        tasks: Vec<TaskRef>,
    },
    Truncated {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExitScope {
    Process,
    Thread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliverAction {
    Handler,
    Ignore,
    Terminate,
    Core,
    Stop,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MutexOp {
    Lock,
    Trylock,
    Unlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CondOp {
    Wait,
    Signal,
    Broadcast,
    Wake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemOp {
    Wait,
    Trywait,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncResult {
    Acquired,
    Blocked,
    Busy,
    Released,
    Posted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FreeError {
    DoubleFree,
    InvalidPointer,
}
