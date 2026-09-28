//! Instantánea de memoria: frames (por unwind con frame pointer), globales y heap, convertidos al
//! árbol de valores del contrato.

use crate::arch::{self, Regs};
use crate::dwarf::{DebugInfo, Loc, TypeId, TypeKind, VarDecl};
use crate::heap::Heap;
use crate::process::{Mapping, Tracee};
use std::collections::{BTreeMap, HashMap};
use trace_model::{Frame, HeapBlock, MemorySnapshot, Scalar, Value, Var};

/// Byte con que se rellenan las variables locales y los bloques de malloc recién creados: una
/// variable cuyos bytes siguen siendo todos POISON nunca fue escrita.
pub const POISON: u8 = 0xBE;

const MAX_ITEMS: u64 = 64;
const MAX_DEPTH: usize = 8;
const MAX_STRING: usize = 64;

pub fn hex(a: u64) -> String {
    format!("{a:#x}")
}

#[derive(Debug, Clone)]
pub struct FrameInfo {
    pub func: usize,
    pub cfa: u64,
    pub pc: u64,
    pub line: u32,
}

/// Recorre la pila siguiendo rbp mientras la dirección de retorno siga en código del usuario.
pub fn unwind(debug: &DebugInfo, tracee: &Tracee, regs: &Regs, current_line: u32) -> Vec<FrameInfo> {
    let mut frames = Vec::new();
    let mut pc = regs.pc();
    let mut fp = regs.fp();
    let mut line = current_line;
    for _ in 0..256 {
        let Some(func) = debug.functions.iter().position(|f| pc >= f.low && pc < f.high) else {
            break;
        };
        frames.push(FrameInfo {
            func,
            cfa: arch::cfa_of(fp),
            pc,
            line,
        });
        let (Some(ret), Some(next_fp)) = (tracee.read_u64(arch::return_address_slot(fp)), tracee.read_u64(fp)) else {
            break;
        };
        pc = ret;
        fp = next_fp;
        // En un llamador, la línea es la de la instrucción call, justo antes de la dirección de retorno.
        line = debug.line_of(pc.saturating_sub(1)).unwrap_or(0);
    }
    frames
}

pub fn visible_locals<'d>(debug: &'d DebugInfo, f: &FrameInfo) -> impl Iterator<Item = &'d VarDecl> {
    let func = &debug.functions[f.func];
    let pc = f.pc;
    let line = f.line;
    func.vars
        .iter()
        .filter(move |v| v.is_param || (v.scope.is_none_or(|(lo, hi)| pc >= lo && pc < hi) && v.decl_line <= line))
}

/// Rellena con POISON las locales de una función al entrar en ella (no los parámetros, que el
/// prólogo ya guardó).
pub fn poison_locals(debug: &DebugInfo, tracee: &Tracee, func: usize, cfa: u64) {
    for v in &debug.functions[func].vars {
        if v.is_param {
            continue;
        }
        if let Loc::Frame(off) = v.loc {
            let size = debug.size_of(v.ty).min(1 << 16) as usize;
            if size > 0 {
                tracee.write(cfa.wrapping_add_signed(off), &vec![POISON; size]);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Region {
    Stack,
    Heap,
    ProgramData,
    Elsewhere,
    Unmapped,
}

pub struct Reader<'a> {
    pub debug: &'a DebugInfo,
    pub tracee: &'a Tracee,
    maps: Vec<Mapping>,
    binary_path: String,
    heap: &'a Heap,
    /// Punteros encontrados (destino, tipo apuntado): con ellos se infiere el tipo de cada bloque.
    pointers: Vec<(u64, TypeId)>,
    /// Funciones del usuario por dirección, para punteros a función.
    fn_names: HashMap<u64, String>,
}

impl<'a> Reader<'a> {
    pub fn new(
        debug: &'a DebugInfo,
        tracee: &'a Tracee,
        heap: &'a Heap,
        binary_path: String,
        maps: Vec<Mapping>,
    ) -> Self {
        let fn_names = debug.functions.iter().map(|f| (f.low, f.name.clone())).collect();
        Reader {
            debug,
            tracee,
            maps,
            binary_path,
            heap,
            pointers: Vec::new(),
            fn_names,
        }
    }

    fn region(&self, addr: u64) -> Region {
        let Some(m) = self.maps.iter().find(|m| addr >= m.start && addr < m.end) else {
            return Region::Unmapped;
        };
        match m.path.as_str() {
            "[stack]" => Region::Stack,
            "[heap]" => Region::Heap,
            p if p == self.binary_path && m.perms.starts_with("rw") => Region::ProgramData,
            _ => Region::Elsewhere,
        }
    }

    pub fn snapshot(&mut self, frames: &[FrameInfo], tid: u32, t: u64) -> MemorySnapshot {
        let globals: Vec<Var> = self
            .debug
            .globals
            .iter()
            .filter_map(|g| match g.loc {
                Loc::Addr(a) => Some(self.var(&display_name(&g.name), g.ty, a)),
                _ => None,
            })
            .collect();
        let mut stack = Vec::new();
        for f in frames {
            let func = &self.debug.functions[f.func];
            let mut params = Vec::new();
            let mut locals = Vec::new();
            for v in visible_locals(self.debug, f) {
                let Loc::Frame(off) = v.loc else { continue };
                let var = self.var(&v.name, v.ty, f.cfa.wrapping_add_signed(off));
                if v.is_param { params.push(var) } else { locals.push(var) }
            }
            stack.push(Frame {
                func: func.name.clone(),
                line: f.line,
                params,
                locals,
                signal: None,
            });
        }
        let heap = self.heap_blocks(t);
        MemorySnapshot {
            globals,
            stacks: BTreeMap::from([(tid, stack)]),
            heap,
        }
    }

    /// Bloques del heap con su tipo inferido de los punteros que los apuntan. Se itera porque un
    /// bloque tipado (un nodo) revela el tipo del siguiente.
    fn heap_blocks(&mut self, t: u64) -> Vec<HeapBlock> {
        let blocks: Vec<_> = self.heap.visible(t).cloned().collect();
        let mut typed: HashMap<u64, TypeId> = HashMap::new();
        let mut values: HashMap<u64, (Value, String)> = HashMap::new();
        for _ in 0..blocks.len() + 1 {
            let mut changed = false;
            let ptrs = std::mem::take(&mut self.pointers);
            for (target, ty) in &ptrs {
                if let Some(b) = blocks.iter().find(|b| b.addr == *target)
                    && !typed.contains_key(&b.addr)
                    && self.debug.size_of(*ty) > 0
                {
                    typed.insert(b.addr, *ty);
                    changed = true;
                }
            }
            for b in &blocks {
                if values.contains_key(&b.addr) {
                    continue;
                }
                if let Some(ty) = typed.get(&b.addr) {
                    let v = self.block_value(*ty, b.addr, b.size);
                    values.insert(b.addr, v);
                }
            }
            if !changed {
                break;
            }
        }
        blocks
            .iter()
            .map(|b| {
                let (value, ty) = values.remove(&b.addr).unwrap_or_else(|| {
                    (
                        Value::Opaque {
                            note: format!("{} bytes de tipo desconocido", b.size),
                        },
                        String::new(),
                    )
                });
                HeapBlock {
                    addr: hex(b.addr),
                    size: b.size,
                    ty: if ty.is_empty() { None } else { Some(ty) },
                    alloc_at: b.alloc_at,
                    alloc_line: b.alloc_line,
                    freed_at: b.freed_at,
                    value,
                }
            })
            .collect()
    }

    fn block_value(&mut self, ty: TypeId, addr: u64, size: u64) -> (Value, String) {
        let elem = self.debug.size_of(ty).max(1);
        let n = size / elem;
        if n <= 1 {
            let (v, _) = self.value(ty, addr, 0);
            return (v, self.debug.type_name(ty));
        }
        let name = format!("{}[{n}]", self.debug.type_name(ty));
        (self.array(ty, n, addr, 0).0, name)
    }

    pub fn var(&mut self, name: &str, ty: TypeId, addr: u64) -> Var {
        let (value, uninit) = self.value(ty, addr, 0);
        Var {
            name: name.to_string(),
            ty: self.debug.type_name(ty),
            addr: hex(addr),
            size: self.debug.size_of(ty),
            value,
            uninit,
        }
    }

    fn poisoned(&self, addr: u64, size: u64) -> bool {
        size > 0
            && self
                .tracee
                .read(addr, size.min(4096) as usize)
                .is_some_and(|b| b.iter().all(|x| *x == POISON))
    }

    /// Devuelve el valor y si está sin inicializar.
    pub fn value(&mut self, ty: TypeId, addr: u64, depth: usize) -> (Value, bool) {
        let debug = self.debug;
        let st = debug.strip(ty);
        let size = debug.size_of(st);
        if depth > MAX_DEPTH {
            return (Value::Opaque { note: "…".into() }, false);
        }
        let uninit = self.poisoned(addr, size);
        let value = match &debug.types[st] {
            TypeKind::Base { .. } | TypeKind::Enum { .. } | TypeKind::Pointer { .. } => {
                match self.tracee.read(addr, size as usize) {
                    Some(bytes) => self.from_bytes(st, &bytes),
                    None => Value::Opaque {
                        note: "memoria no legible".into(),
                    },
                }
            }
            TypeKind::Struct { members, union, .. } => {
                let fields = members
                    .clone()
                    .iter()
                    .map(|m| {
                        let a = addr + m.offset;
                        let (value, un) = self.value(m.ty, a, depth + 1);
                        Var {
                            name: m.name.clone(),
                            ty: debug.type_name(m.ty),
                            addr: hex(a),
                            size: debug.size_of(m.ty),
                            value,
                            uninit: un,
                        }
                    })
                    .collect();
                if *union {
                    Value::Union { fields }
                } else {
                    Value::Struct { fields }
                }
            }
            TypeKind::Array { elem, count } => return (self.array(*elem, count.unwrap_or(0), addr, depth).0, uninit),
            _ => Value::Opaque {
                note: debug.type_name(ty),
            },
        };
        (value, uninit)
    }

    fn array(&mut self, elem: TypeId, count: u64, addr: u64, depth: usize) -> (Value, bool) {
        let debug = self.debug;
        let es = debug.size_of(elem);
        let shown = count.min(MAX_ITEMS);
        let is_char = matches!(&debug.types[debug.strip(elem)], TypeKind::Base { size: 1, enc, .. }
            if *enc == gimli::DW_ATE_signed_char || *enc == gimli::DW_ATE_unsigned_char);
        let mut items = Vec::new();
        for i in 0..shown {
            items.push(self.value(elem, addr + i * es, depth + 1).0);
        }
        let text = if is_char {
            self.tracee.read(addr, count.min(4096) as usize).map(|b| {
                let end = b.iter().position(|x| *x == 0).unwrap_or(b.len());
                latin1(&b[..end])
            })
        } else {
            None
        };
        (
            Value::Array {
                length: count,
                items,
                text,
            },
            self.poisoned(addr, count * es),
        )
    }

    /// Escalares, enums y punteros a partir de sus bytes (también sirve para valores de retorno).
    pub fn from_bytes(&mut self, st: TypeId, bytes: &[u8]) -> Value {
        let debug = self.debug;
        let raw = {
            let mut b = [0u8; 8];
            let n = bytes.len().min(8);
            b[..n].copy_from_slice(&bytes[..n]);
            u64::from_le_bytes(b)
        };
        match &debug.types[st] {
            TypeKind::Base { size, enc, .. } => base_value(*size, *enc, raw, bytes),
            TypeKind::Enum { size, values, .. } => {
                let v = sign_extend(raw, *size);
                let repr = values.iter().find(|(_, n)| *n == v).map(|(name, _)| name.clone());
                Value::Scalar {
                    value: Scalar::Number(v.into()),
                    repr,
                }
            }
            TypeKind::Pointer { target } => self.pointer(raw, *target),
            _ => Value::Opaque {
                note: debug.type_name(st),
            },
        }
    }

    fn pointer(&mut self, target: u64, pointee: Option<TypeId>) -> Value {
        if target == 0 {
            return Value::Pointer {
                target: None,
                func: None,
                outside: false,
                text: None,
            };
        }
        let debug = self.debug;
        let pointee = pointee.map(|p| debug.strip(p));
        if let Some(p) = pointee
            && matches!(debug.types[p], TypeKind::Function { .. })
        {
            let name = self.fn_names.get(&target).cloned();
            return Value::Pointer {
                target: Some(hex(target)),
                func: name.or(Some("?".into())),
                outside: false,
                text: None,
            };
        }
        let region = self.region(target);
        let is_char = pointee.is_some_and(|p| matches!(&debug.types[p], TypeKind::Base { size: 1, .. }));
        if let Some(p) = pointee
            && !matches!(debug.types[p], TypeKind::Void)
        {
            self.pointers.push((target, p));
        }
        // Memoria válida que no se dibuja (literales de texto, datos de libc): no es un puntero colgante.
        let outside = region == Region::Elsewhere;
        let text = if outside && is_char {
            self.tracee.read_cstr(target, MAX_STRING).map(|b| latin1(&b))
        } else {
            None
        };
        Value::Pointer {
            target: Some(hex(target)),
            func: None,
            outside,
            text,
        }
    }
}

fn display_name(name: &str) -> String {
    match name.split_once("::") {
        Some((f, v)) => format!("{v} (static en {f})"),
        None => name.to_string(),
    }
}

pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| *b as char).collect()
}

fn sign_extend(raw: u64, size: u64) -> i64 {
    if size >= 8 {
        return raw as i64;
    }
    let shift = 64 - size * 8;
    ((raw << shift) as i64) >> shift
}

fn char_repr(c: u8) -> String {
    match c {
        0 => "'\\0'".into(),
        b'\n' => "'\\n'".into(),
        b'\t' => "'\\t'".into(),
        b'\r' => "'\\r'".into(),
        b'\'' => "'\\''".into(),
        b'\\' => "'\\\\'".into(),
        0x20..=0x7e => format!("'{}'", c as char),
        _ => format!("'\\x{c:02x}'"),
    }
}

fn number_or_text(v: i128) -> Scalar {
    // Un double representa exactos los enteros hasta 2^53; más allá se envían como texto.
    if v.unsigned_abs() <= (1u128 << 53) {
        if v < 0 {
            Scalar::Number((v as i64).into())
        } else {
            Scalar::Number((v as u64).into())
        }
    } else {
        Scalar::Text(v.to_string())
    }
}

fn base_value(size: u64, enc: gimli::DwAte, raw: u64, bytes: &[u8]) -> Value {
    let scalar = |value, repr| Value::Scalar { value, repr };
    match enc {
        gimli::DW_ATE_boolean => scalar(Scalar::Bool(raw != 0), None),
        gimli::DW_ATE_float => {
            let f = match size {
                4 => f32::from_le_bytes(bytes[..4].try_into().unwrap()) as f64,
                8 => f64::from_le_bytes(bytes[..8].try_into().unwrap()),
                _ => {
                    return Value::Opaque {
                        note: "long double".into(),
                    };
                }
            };
            match serde_json::Number::from_f64(f) {
                Some(n) => scalar(Scalar::Number(n), Some(format!("{f}"))),
                None => scalar(Scalar::Text(format!("{f}")), None),
            }
        }
        gimli::DW_ATE_signed_char | gimli::DW_ATE_unsigned_char if size == 1 => {
            let c = raw as u8;
            let v = if enc == gimli::DW_ATE_signed_char {
                c as i8 as i128
            } else {
                c as i128
            };
            scalar(number_or_text(v), Some(char_repr(c)))
        }
        gimli::DW_ATE_signed | gimli::DW_ATE_signed_char => {
            scalar(number_or_text(sign_extend(raw, size) as i128), None)
        }
        _ => {
            let v = if size >= 8 {
                raw
            } else {
                raw & ((1u64 << (size * 8)) - 1)
            };
            scalar(number_or_text(v as i128), None)
        }
    }
}
