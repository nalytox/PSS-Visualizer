//! Traza compacta (versión 2), para ejecuciones largas. Lleva el mismo contenido que la traza de
//! la versión 1, pero:
//!
//! - los pasos van en bloques de hasta [`CHUNK_STEPS`], cada uno comprimido con gzip y en base64.
//!   Un bloque empieza con un paso completo y sigue con parches respecto del anterior (ver
//!   [`crate::delta`]); para calcular el parche, el paso anterior se toma con `events` vacío;
//! - las instantáneas de memoria van en el bloque que las usa: completas o como parche de otra
//!   instantánea del mismo bloque (`{"$from": id, "$p": parche}`). Todo lo que un bloque nombra
//!   está definido en él, así que cada bloque se decodifica por su cuenta;
//! - un índice por paso, sin comprimir pasos: quién avanzó, en qué línea quedó y qué eventos hubo
//!   (banderas, ver [`FLAGS`]). Con él la línea de tiempo y la navegación no abren bloques.
//!
//! Lo escribe [`Encoder`] a medida que el tracer avanza, sin guardar la ejecución entera.

use crate::delta::{apply, diff, snapshot_patch};
use crate::*;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};

pub const VERSION: u32 = 2;
/// Pasos por bloque: decodificar uno en el navegador toma unos pocos milisegundos.
pub const CHUNK_STEPS: usize = 1000;
/// Un bloque se cierra antes si su JSON sin comprimir pasa de este tamaño (memoria muy grande).
pub const CHUNK_RAW_BYTES: usize = 4 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactTrace {
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
    pub output: Vec<OutputChunk>,
    pub summary: Summary,
    pub step_count: u64,
    pub index: StepIndex,
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepIndex {
    /// Columnas de `stepCount` enteros sin signo de 32 bits (little-endian), con gzip y base64.
    /// 0 = sin actor.
    pub actor_pid: String,
    pub actor_tid: String,
    /// Próxima línea del actor al terminar el paso (0 = ninguna).
    pub actor_line: String,
    /// Banderas de los eventos del paso, en el orden de [`FLAGS`].
    pub flags: String,
    /// Procesos, hilos y mutex en orden de aparición; forks en orden.
    pub pids: Vec<Pid>,
    pub threads: Vec<TaskRef>,
    pub forks: Vec<ForkEdge>,
    pub mutexes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkEdge {
    pub parent: Pid,
    pub child: Pid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chunk {
    /// Primer paso del bloque.
    pub start: u64,
    pub count: u64,
    /// Las instantáneas de este bloque son `m{firstSnapshot}` en adelante.
    pub first_snapshot: u64,
    /// `{"steps": [...], "snapshots": {...}}` con gzip y en base64.
    pub data: String,
}

/// Banderas del índice: un bit por clase de evento. Algunas distinguen variantes que la línea de
/// tiempo dibuja distinto (un exit de proceso o de hilo, un wait que recogió a un hijo).
pub const FLAGS: [&str; 32] = [
    "call",
    "return",
    "fork",
    "threadCreate",
    "exec",
    "exitProcess",
    "exitThread",
    "wait",
    "waitReaped",
    "join",
    "reparent",
    "pipe",
    "dup",
    "close",
    "read",
    "write",
    "writePipe",
    "block",
    "unblock",
    "signalSend",
    "signalDeliver",
    "signalReturn",
    "mutex",
    "mutexAcquired",
    "mutexReleased",
    "cond",
    "sem",
    "malloc",
    "free",
    "memError",
    "deadlock",
    // stdinNeeded y truncated: solo aparecen en el último paso.
    "other",
];

fn bit(name: &str) -> u32 {
    1 << FLAGS.iter().position(|f| *f == name).expect("bandera conocida")
}

pub fn event_flags(events: &[Event]) -> u32 {
    let mut f = 0;
    for ev in events {
        f |= match ev {
            Event::Call { .. } => bit("call"),
            Event::Return { .. } => bit("return"),
            Event::Fork { .. } => bit("fork"),
            Event::ThreadCreate { .. } => bit("threadCreate"),
            Event::Exec { .. } => bit("exec"),
            Event::Exit {
                scope: ExitScope::Process,
                ..
            } => bit("exitProcess"),
            Event::Exit { .. } => bit("exitThread"),
            Event::Wait { reaped, .. } => bit("wait") | if reaped.is_some() { bit("waitReaped") } else { 0 },
            Event::Join { .. } => bit("join"),
            Event::Reparent { .. } => bit("reparent"),
            Event::Pipe { .. } => bit("pipe"),
            Event::Dup { .. } => bit("dup"),
            Event::Close { .. } => bit("close"),
            Event::Read { .. } => bit("read"),
            Event::Write { pipe, .. } => bit("write") | if pipe.is_some() { bit("writePipe") } else { 0 },
            Event::Block { .. } => bit("block"),
            Event::Unblock { .. } => bit("unblock"),
            Event::SignalSend { .. } => bit("signalSend"),
            Event::SignalDeliver { .. } => bit("signalDeliver"),
            Event::SignalReturn { .. } => bit("signalReturn"),
            Event::Mutex { result, .. } => {
                bit("mutex")
                    | match result {
                        SyncResult::Acquired => bit("mutexAcquired"),
                        SyncResult::Released => bit("mutexReleased"),
                        _ => 0,
                    }
            }
            Event::Cond { .. } => bit("cond"),
            Event::Sem { .. } => bit("sem"),
            Event::Malloc { .. } => bit("malloc"),
            Event::Free { .. } => bit("free"),
            Event::MemError { .. } => bit("memError"),
            Event::Deadlock { .. } => bit("deadlock"),
            Event::StdinNeeded { .. } | Event::Truncated { .. } => bit("other"),
        };
    }
    f
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).expect("escribir en memoria");
    e.finish().expect("escribir en memoria")
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    GzDecoder::new(bytes).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn encode_column(values: &[u32]) -> String {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    BASE64.encode(gzip(&bytes))
}

pub fn decode_column(text: &str) -> Result<Vec<u32>, String> {
    let bytes = gunzip(&BASE64.decode(text).map_err(|e| e.to_string())?)?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

/// Bloque en construcción.
#[derive(Default)]
struct Building {
    start: u64,
    count: u64,
    first_snapshot: u64,
    /// Entradas de `steps`, ya en JSON y separadas por comas.
    steps: Vec<u8>,
    /// Miembros de `snapshots`, ya en JSON y separados por comas.
    snapshots: Vec<u8>,
    /// Identificador del tracer → identificador en este bloque.
    ids: HashMap<String, String>,
    /// Última instantánea definida para cada proceso: base del parche de la siguiente.
    last: HashMap<Pid, (String, MemorySnapshot)>,
    /// Paso anterior, con `events` vacío.
    prev: Option<Json>,
}

/// Escribe la traza compacta paso a paso.
pub struct Encoder {
    chunk_steps: usize,
    chunks: Vec<Chunk>,
    cur: Building,
    next_snapshot: u64,
    steps: u64,
    compressed: u64,
    actor_pid: Vec<u32>,
    actor_tid: Vec<u32>,
    actor_line: Vec<u32>,
    flags: Vec<u32>,
    pids: Vec<Pid>,
    threads: Vec<TaskRef>,
    forks: Vec<ForkEdge>,
    mutexes: Vec<String>,
    seen_threads: HashSet<TaskRef>,
    seen_mutexes: HashSet<String>,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new(CHUNK_STEPS)
    }
}

impl Encoder {
    pub fn new(chunk_steps: usize) -> Self {
        Encoder {
            chunk_steps: chunk_steps.max(1),
            chunks: Vec::new(),
            cur: Building::default(),
            next_snapshot: 0,
            steps: 0,
            compressed: 0,
            actor_pid: Vec::new(),
            actor_tid: Vec::new(),
            actor_line: Vec::new(),
            flags: Vec::new(),
            pids: Vec::new(),
            threads: Vec::new(),
            forks: Vec::new(),
            mutexes: Vec::new(),
            seen_threads: HashSet::new(),
            seen_mutexes: HashSet::new(),
        }
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Bytes comprimidos de los bloques ya cerrados.
    pub fn compressed_bytes(&self) -> u64 {
        self.compressed
    }

    /// Agrega un paso. `snapshot` entrega las instantáneas que nombra el paso.
    pub fn push<'s>(&mut self, step: &Step, snapshot: impl Fn(&str) -> Option<&'s MemorySnapshot>) {
        self.index(step);
        if self.cur.count as usize >= self.chunk_steps
            || self.cur.steps.len() + self.cur.snapshots.len() >= CHUNK_RAW_BYTES
        {
            self.flush();
        }
        let mut value = serde_json::to_value(step).expect("un paso se serializa");
        if let Some(procs) = value.get_mut("processes").and_then(Json::as_array_mut) {
            for p in procs {
                let pid = p.get("pid").and_then(Json::as_u64).unwrap_or(0) as Pid;
                if let Some(Json::String(id)) = p.get_mut("mem") {
                    *id = self.snapshot_id(pid, id, &snapshot);
                }
            }
        }
        let entry = match &self.cur.prev {
            None => value.clone(),
            Some(prev) => diff(prev, &value).unwrap_or_else(|| Json::Object(Default::default())),
        };
        if self.cur.count > 0 {
            self.cur.steps.push(b',');
        }
        serde_json::to_writer(&mut self.cur.steps, &entry).expect("escribir en memoria");
        value["events"] = Json::Array(Vec::new());
        self.cur.prev = Some(value);
        self.cur.count += 1;
        self.steps += 1;
    }

    fn snapshot_id<'s>(
        &mut self,
        pid: Pid,
        engine_id: &str,
        snapshot: &impl Fn(&str) -> Option<&'s MemorySnapshot>,
    ) -> String {
        if let Some(id) = self.cur.ids.get(engine_id) {
            return id.clone();
        }
        let Some(value) = snapshot(engine_id) else {
            return engine_id.to_string();
        };
        let entry = match self.cur.last.get(&pid) {
            Some((base, base_value)) => match snapshot_patch(base_value, value) {
                // El mismo contenido con otro nombre: se reutiliza.
                None => {
                    let base = base.clone();
                    self.cur.ids.insert(engine_id.to_string(), base.clone());
                    return base;
                }
                Some(p) => serde_json::json!({ "$from": base, "$p": p }),
            },
            None => serde_json::to_value(value).expect("una instantánea se serializa"),
        };
        let id = format!("m{}", self.next_snapshot);
        self.next_snapshot += 1;
        if !self.cur.snapshots.is_empty() {
            self.cur.snapshots.push(b',');
        }
        serde_json::to_writer(&mut self.cur.snapshots, &id).expect("escribir en memoria");
        self.cur.snapshots.push(b':');
        serde_json::to_writer(&mut self.cur.snapshots, &entry).expect("escribir en memoria");
        self.cur.ids.insert(engine_id.to_string(), id.clone());
        self.cur.last.insert(pid, (id.clone(), value.clone()));
        id
    }

    fn index(&mut self, step: &Step) {
        let actor = step.actor;
        self.actor_pid.push(actor.map_or(0, |a| a.pid));
        self.actor_tid.push(actor.map_or(0, |a| a.tid));
        let line = actor
            .and_then(|a| step.processes.iter().find(|p| p.pid == a.pid))
            .and_then(|p| p.threads.iter().find(|th| Some(th.tid) == actor.map(|a| a.tid)))
            .and_then(|th| th.line);
        self.actor_line.push(line.unwrap_or(0));
        self.flags.push(event_flags(&step.events));
        for p in &step.processes {
            if !self.pids.contains(&p.pid) {
                self.pids.push(p.pid);
            }
            for th in &p.threads {
                let r = TaskRef {
                    pid: p.pid,
                    tid: th.tid,
                };
                if self.seen_threads.insert(r) {
                    self.threads.push(r);
                }
            }
        }
        for ev in &step.events {
            if let Event::Fork { parent, child, .. } = ev {
                self.forks.push(ForkEdge {
                    parent: *parent,
                    child: *child,
                });
            }
        }
        for s in &step.sync {
            if let SyncObject::Mutex { id, .. } = s
                && self.seen_mutexes.insert(id.clone())
            {
                self.mutexes.push(id.clone());
            }
        }
    }

    fn flush(&mut self) {
        if self.cur.count == 0 {
            return;
        }
        let cur = std::mem::take(&mut self.cur);
        let mut body = Vec::with_capacity(cur.steps.len() + cur.snapshots.len() + 32);
        body.extend_from_slice(b"{\"steps\":[");
        body.extend_from_slice(&cur.steps);
        body.extend_from_slice(b"],\"snapshots\":{");
        body.extend_from_slice(&cur.snapshots);
        body.extend_from_slice(b"}}");
        let gz = gzip(&body);
        self.compressed += gz.len() as u64;
        self.chunks.push(Chunk {
            start: cur.start,
            count: cur.count,
            first_snapshot: cur.first_snapshot,
            data: BASE64.encode(gz),
        });
        self.cur.start = self.steps;
        self.cur.first_snapshot = self.next_snapshot;
    }

    /// Cierra la traza. `header` aporta todo lo que no son pasos ni instantáneas.
    pub fn finish(mut self, header: &Trace) -> CompactTrace {
        self.flush();
        CompactTrace {
            version: VERSION,
            arch: header.arch,
            source: header.source.clone(),
            stdin: header.stdin.clone(),
            run: header.run.clone(),
            compile: header.compile.clone(),
            outcome: header.outcome.clone(),
            truncated: header.truncated,
            truncated_reason: header.truncated_reason,
            output: header.output.clone(),
            summary: header.summary.clone(),
            step_count: self.steps,
            index: StepIndex {
                actor_pid: encode_column(&self.actor_pid),
                actor_tid: encode_column(&self.actor_tid),
                actor_line: encode_column(&self.actor_line),
                flags: encode_column(&self.flags),
                pids: self.pids,
                threads: self.threads,
                forks: self.forks,
                mutexes: self.mutexes,
            },
            chunks: self.chunks,
        }
    }
}

/// Convierte una traza completa en compacta (para pruebas y herramientas).
pub fn encode(trace: &Trace, chunk_steps: usize) -> CompactTrace {
    let mut e = Encoder::new(chunk_steps);
    for step in &trace.steps {
        e.push(step, |id| trace.snapshots.get(id));
    }
    e.finish(trace)
}

/// Pasos e instantáneas de un bloque.
pub fn decode_chunk(chunk: &Chunk) -> Result<(Vec<Step>, BTreeMap<SnapshotId, MemorySnapshot>), String> {
    let bytes = gunzip(&BASE64.decode(&chunk.data).map_err(|e| e.to_string())?)?;
    let body: Json = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let entries = body["steps"].as_array().ok_or("bloque sin pasos")?;
    let mut steps = Vec::with_capacity(entries.len());
    let mut prev: Option<Json> = None;
    for entry in entries {
        let value = match &prev {
            None => entry.clone(),
            Some(p) => apply(p, entry),
        };
        steps.push(serde_json::from_value::<Step>(value.clone()).map_err(|e| e.to_string())?);
        let mut base = value;
        base["events"] = Json::Array(Vec::new());
        prev = Some(base);
    }
    let mut resolved: HashMap<String, Json> = HashMap::new();
    let mut snapshots = BTreeMap::new();
    if let Some(defs) = body["snapshots"].as_object() {
        // Cada parche nombra una instantánea anterior del mismo bloque.
        let mut ordered: Vec<(&String, &Json)> = defs.iter().collect();
        ordered.sort_by_key(|(id, _)| id[1..].parse::<u64>().unwrap_or(u64::MAX));
        for (id, def) in ordered {
            let value = match def.get("$from").and_then(Json::as_str) {
                Some(base) => apply(resolved.get(base).ok_or("instantánea base inexistente")?, &def["$p"]),
                None => def.clone(),
            };
            snapshots.insert(
                id.clone(),
                serde_json::from_value::<MemorySnapshot>(value.clone()).map_err(|e| e.to_string())?,
            );
            resolved.insert(id.clone(), value);
        }
    }
    Ok((steps, snapshots))
}

/// Reconstruye la traza completa (versión 1).
pub fn decode(c: &CompactTrace) -> Result<Trace, String> {
    let mut steps = Vec::with_capacity(c.step_count as usize);
    let mut snapshots = BTreeMap::new();
    for chunk in &c.chunks {
        let (s, snaps) = decode_chunk(chunk)?;
        steps.extend(s);
        snapshots.extend(snaps);
    }
    Ok(Trace {
        version: 1,
        arch: c.arch,
        source: c.source.clone(),
        stdin: c.stdin.clone(),
        run: c.run.clone(),
        compile: c.compile.clone(),
        outcome: c.outcome.clone(),
        truncated: c.truncated,
        truncated_reason: c.truncated_reason,
        steps,
        snapshots,
        output: c.output.clone(),
        summary: c.summary.clone(),
    })
}

/// Lee una traza de cualquiera de las dos versiones.
pub fn read_any(text: &str) -> Result<Trace, String> {
    let value: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if value.get("version").and_then(Json::as_u64) == Some(VERSION as u64) {
        decode(&serde_json::from_value(value).map_err(|e| e.to_string())?)
    } else {
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}
