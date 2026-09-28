//! Información de depuración del programa del usuario: tabla de líneas, funciones, variables con
//! su alcance y tipos. Se lee una sola vez, con gimli, desde el ELF compilado con `-g -O0`.

use gimli::{AttributeValue, DwAte, DwTag, EndianSlice, Operation, RunTimeEndian, UnitOffset};
use object::{Object, ObjectSection};
use std::collections::HashMap;
use std::path::Path;

type R = EndianSlice<'static, RunTimeEndian>;

pub type TypeId = usize;

#[derive(Debug, Clone)]
pub enum TypeKind {
    Void,
    Base {
        name: String,
        size: u64,
        enc: DwAte,
    },
    Pointer {
        target: Option<TypeId>,
    },
    Struct {
        name: Option<String>,
        size: u64,
        members: Vec<Member>,
        union: bool,
    },
    Array {
        elem: TypeId,
        count: Option<u64>,
    },
    Enum {
        name: Option<String>,
        size: u64,
        values: Vec<(String, i64)>,
    },
    Typedef {
        name: String,
        target: Option<TypeId>,
    },
    Qualified {
        qual: &'static str,
        target: Option<TypeId>,
    },
    Function {
        ret: Option<TypeId>,
        params: Vec<Option<TypeId>>,
    },
}

#[derive(Debug, Clone)]
pub struct Member {
    pub name: String,
    pub ty: TypeId,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loc {
    /// Desplazamiento desde la base del frame (DW_OP_fbreg); la base es el CFA.
    Frame(i64),
    /// Dirección fija (globales y `static`).
    Addr(u64),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub ty: TypeId,
    pub loc: Loc,
    pub decl_line: u32,
    /// Rango del bloque léxico que la contiene; None = toda la función.
    pub scope: Option<(u64, u64)>,
    pub is_param: bool,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub low: u64,
    pub high: u64,
    pub decl_line: u32,
    /// Primera dirección después del prólogo: antes de ella el frame no está armado.
    pub body_start: u64,
    /// En el cuerpo, CFA = frame pointer + `cfa_fp` (16 en x86_64; en aarch64 depende del tamaño
    /// del frame). Sale de la información de unwind (.eh_frame).
    pub cfa_fp: u64,
    pub ret: Option<TypeId>,
    pub vars: Vec<VarDecl>,
}

#[derive(Debug, Clone, Copy)]
pub struct LineRow {
    pub addr: u64,
    pub line: u32,
    pub is_stmt: bool,
    pub end: bool,
}

pub struct DebugInfo {
    pub rows: Vec<LineRow>,
    pub functions: Vec<Function>,
    pub globals: Vec<VarDecl>,
    pub types: Vec<TypeKind>,
}

impl DebugInfo {
    pub fn load(binary: &Path, source_name: &str) -> Result<Self, String> {
        let data: &'static [u8] = Box::leak(
            std::fs::read(binary)
                .map_err(|e| format!("no se pudo leer {}: {e}", binary.display()))?
                .into_boxed_slice(),
        );
        let obj = object::File::parse(data).map_err(|e| format!("ELF inválido: {e}"))?;
        let endian = if obj.is_little_endian() {
            RunTimeEndian::Little
        } else {
            RunTimeEndian::Big
        };
        let load = |id: gimli::SectionId| -> Result<R, String> {
            let bytes = obj
                .section_by_name(id.name())
                .and_then(|s| s.uncompressed_data().ok())
                .map(|c| &*Box::leak(c.into_owned().into_boxed_slice()))
                .unwrap_or(&[]);
            Ok(EndianSlice::new(bytes, endian))
        };
        let dwarf = gimli::Dwarf::load(load)?;
        let mut out = DebugInfo {
            rows: Vec::new(),
            functions: Vec::new(),
            globals: Vec::new(),
            types: Vec::new(),
        };
        let mut units = dwarf.units();
        while let Some(header) = units.next().map_err(err)? {
            let unit = dwarf.unit(header).map_err(err)?;
            let mut p = UnitParser {
                dwarf: &dwarf,
                unit: &unit,
                cache: HashMap::new(),
                out: &mut out,
            };
            if !p.is_user_unit(source_name)? {
                continue;
            }
            p.parse_lines(source_name)?;
            p.parse_entries()?;
        }
        out.rows.sort_by_key(|r| (r.addr, r.end));
        out.functions.sort_by_key(|f| f.low);
        let body_starts: Vec<u64> = out.functions.iter().map(|f| out.body_start(f)).collect();
        for (f, b) in out.functions.iter_mut().zip(body_starts) {
            f.body_start = b;
        }
        let cfi = frame_offsets(&obj, endian, &out.functions);
        for (f, off) in out.functions.iter_mut().zip(cfi) {
            if let Some(off) = off {
                f.cfa_fp = off;
            }
        }
        if out.functions.iter().all(|f| f.name != "main") {
            return Err("el programa no tiene una función main con información de depuración".into());
        }
        Ok(out)
    }

    /// CFA del frame que ejecuta `pc`, a partir de su frame pointer.
    pub fn cfa_at(&self, pc: u64, fp: u64) -> u64 {
        fp + self.function_at(pc).map_or(16, |f| f.cfa_fp)
    }

    pub fn function_at(&self, pc: u64) -> Option<&Function> {
        self.functions.iter().find(|f| pc >= f.low && pc < f.high)
    }

    /// Fila de la tabla de líneas que empieza exactamente en `pc` (solo is_stmt).
    pub fn stmt_row_at(&self, pc: u64) -> Option<LineRow> {
        let i = self.rows.partition_point(|r| r.addr < pc);
        self.rows[i..]
            .iter()
            .take_while(|r| r.addr == pc)
            .find(|r| r.is_stmt && !r.end)
            .copied()
    }

    /// Línea a la que pertenece `pc` (la última fila que empieza en o antes de pc).
    pub fn line_of(&self, pc: u64) -> Option<u32> {
        let i = self.rows.partition_point(|r| r.addr <= pc);
        self.rows[..i].iter().rev().find(|r| !r.end).map(|r| r.line)
    }

    fn body_start(&self, f: &Function) -> u64 {
        let rows: Vec<&LineRow> = self
            .rows
            .iter()
            .filter(|r| r.addr >= f.low && r.addr < f.high && !r.end)
            .collect();
        let header = rows.first().map(|r| r.line).unwrap_or(f.decl_line);
        rows.iter()
            .find(|r| r.addr > f.low && r.line != header)
            .or_else(|| rows.get(1))
            .map(|r| r.addr)
            .unwrap_or(f.low)
    }

    pub fn strip(&self, mut ty: TypeId) -> TypeId {
        for _ in 0..32 {
            match &self.types[ty] {
                TypeKind::Typedef { target: Some(t), .. } | TypeKind::Qualified { target: Some(t), .. } => ty = *t,
                _ => break,
            }
        }
        ty
    }

    pub fn size_of(&self, ty: TypeId) -> u64 {
        match &self.types[self.strip(ty)] {
            TypeKind::Base { size, .. } | TypeKind::Struct { size, .. } | TypeKind::Enum { size, .. } => *size,
            TypeKind::Pointer { .. } => 8,
            TypeKind::Array { elem, count } => count.unwrap_or(0) * self.size_of(*elem),
            _ => 0,
        }
    }

    /// Nombre del tipo como se escribe en C: `struct nodo *`, `char[16]`, `void (*)(int)`.
    pub fn type_name(&self, ty: TypeId) -> String {
        self.name_with(ty, String::new())
    }

    fn name_with(&self, ty: TypeId, inner: String) -> String {
        let join = |base: String, inner: &str| {
            if inner.is_empty() {
                base
            } else {
                format!("{base} {inner}")
            }
        };
        match &self.types[ty] {
            TypeKind::Void => join("void".into(), &inner),
            TypeKind::Base { name, .. } | TypeKind::Typedef { name, .. } => join(name.clone(), &inner),
            TypeKind::Struct { name, union, .. } => {
                let kw = if *union { "union" } else { "struct" };
                join(format!("{kw} {}", name.as_deref().unwrap_or("<anónimo>")), &inner)
            }
            TypeKind::Enum { name, .. } => join(format!("enum {}", name.as_deref().unwrap_or("<anónimo>")), &inner),
            TypeKind::Qualified { qual, target } => {
                let base = target
                    .map(|t| self.name_with(t, String::new()))
                    .unwrap_or_else(|| "void".into());
                join(format!("{qual} {base}"), &inner)
            }
            TypeKind::Pointer { target } => {
                let inner = format!("*{inner}");
                match target {
                    Some(t) if matches!(self.types[*t], TypeKind::Function { .. } | TypeKind::Array { .. }) => {
                        self.name_with(*t, format!("({inner})"))
                    }
                    Some(t) => self.name_with(*t, inner),
                    None => format!("void {inner}"),
                }
            }
            TypeKind::Array { elem, count } => {
                let n = count.map(|c| c.to_string()).unwrap_or_default();
                self.name_with(*elem, format!("{inner}[{n}]"))
            }
            TypeKind::Function { ret, params } => {
                let ret = ret.map(|t| self.type_name(t)).unwrap_or_else(|| "void".into());
                let ps: Vec<String> = params
                    .iter()
                    .map(|p| p.map(|t| self.type_name(t)).unwrap_or_else(|| "...".into()))
                    .collect();
                let ps = if ps.is_empty() {
                    "void".to_string()
                } else {
                    ps.join(", ")
                };
                format!("{ret} {inner}({ps})").replace(" *", " *").replace("  ", " ")
            }
        }
        .replace("* ", "*")
        .replace(" [", "[")
    }
}

fn err(e: gimli::Error) -> String {
    format!("DWARF: {e}")
}

struct UnitParser<'a> {
    dwarf: &'a gimli::Dwarf<R>,
    unit: &'a gimli::Unit<R>,
    cache: HashMap<UnitOffset, TypeId>,
    out: &'a mut DebugInfo,
}

enum Ctx {
    Cu,
    Func(usize),
    Block(Option<(u64, u64)>),
    Other,
}

impl UnitParser<'_> {
    fn string(&self, v: AttributeValue<R>) -> Option<String> {
        self.dwarf
            .attr_string(self.unit, v)
            .ok()
            .map(|s| s.to_string_lossy().into_owned())
    }

    fn name(&self, e: &gimli::DebuggingInformationEntry<R>) -> Option<String> {
        e.attr_value(gimli::DW_AT_name).and_then(|v| self.string(v))
    }

    fn is_user_unit(&self, source_name: &str) -> Result<bool, String> {
        let mut cursor = self.unit.entries();
        let Some(root) = cursor.next_dfs().map_err(err)? else {
            return Ok(false);
        };
        Ok(self
            .name(root)
            .is_some_and(|n| Path::new(&n).file_name().is_some_and(|f| f == source_name)))
    }

    fn parse_lines(&mut self, source_name: &str) -> Result<(), String> {
        let Some(program) = self.unit.line_program.clone() else {
            return Ok(());
        };
        let mut rows = program.rows();
        while let Some((header, row)) = rows.next_row().map_err(err)? {
            let ours = row
                .file(header)
                .and_then(|f| self.string(f.path_name()))
                .is_some_and(|p| Path::new(&p).file_name().is_some_and(|f| f == source_name));
            if !ours && !row.end_sequence() {
                continue;
            }
            self.out.rows.push(LineRow {
                addr: row.address(),
                line: row.line().map(|l| l.get() as u32).unwrap_or(0),
                is_stmt: row.is_stmt(),
                end: row.end_sequence(),
            });
        }
        Ok(())
    }

    fn range(&self, e: &gimli::DebuggingInformationEntry<R>) -> Option<(u64, u64)> {
        let low = match e.attr_value(gimli::DW_AT_low_pc)? {
            AttributeValue::Addr(a) => a,
            _ => return None,
        };
        let high = match e.attr_value(gimli::DW_AT_high_pc)? {
            AttributeValue::Addr(a) => a,
            v => low + v.udata_value()?,
        };
        Some((low, high))
    }

    fn location(&self, e: &gimli::DebuggingInformationEntry<R>) -> Loc {
        let Some(AttributeValue::Exprloc(expr)) = e.attr_value(gimli::DW_AT_location) else {
            return Loc::Unknown;
        };
        let mut bytes = expr.0;
        match Operation::parse(&mut bytes, self.unit.encoding()) {
            Ok(Operation::FrameOffset { offset }) => Loc::Frame(offset),
            Ok(Operation::Address { address }) => Loc::Addr(address),
            _ => Loc::Unknown,
        }
    }

    fn var(
        &mut self,
        e: &gimli::DebuggingInformationEntry<R>,
        scope: Option<(u64, u64)>,
        is_param: bool,
    ) -> Option<VarDecl> {
        let name = self.name(e)?;
        let ty = match e.attr_value(gimli::DW_AT_type) {
            Some(AttributeValue::UnitRef(off)) => self.type_at(off),
            _ => self.void(),
        };
        Some(VarDecl {
            name,
            ty,
            loc: self.location(e),
            decl_line: e
                .attr_value(gimli::DW_AT_decl_line)
                .and_then(|v| v.udata_value())
                .unwrap_or(0) as u32,
            scope,
            is_param,
        })
    }

    fn parse_entries(&mut self) -> Result<(), String> {
        let mut cursor = self.unit.entries();
        let mut stack: Vec<Ctx> = Vec::new();
        while let Some(e) = cursor.next_dfs().map_err(err)? {
            let e = e.clone();
            let depth = cursor.depth().max(0) as usize;
            stack.truncate(depth);
            let parent_func = stack
                .iter()
                .rev()
                .find_map(|c| if let Ctx::Func(i) = c { Some(*i) } else { None });
            let scope = stack
                .iter()
                .rev()
                .find_map(|c| if let Ctx::Block(r) = c { Some(*r) } else { None })
                .flatten();
            let ctx = match e.tag() {
                gimli::DW_TAG_compile_unit => Ctx::Cu,
                gimli::DW_TAG_subprogram => match (self.name(&e), self.range(&e)) {
                    (Some(name), Some((low, high))) => {
                        let ret = match e.attr_value(gimli::DW_AT_type) {
                            Some(AttributeValue::UnitRef(off)) => Some(self.type_at(off)),
                            _ => None,
                        };
                        let decl_line = e
                            .attr_value(gimli::DW_AT_decl_line)
                            .and_then(|v| v.udata_value())
                            .unwrap_or(0) as u32;
                        self.out.functions.push(Function {
                            cfa_fp: 16,
                            name,
                            low,
                            high,
                            decl_line,
                            body_start: low,
                            ret,
                            vars: Vec::new(),
                        });
                        Ctx::Func(self.out.functions.len() - 1)
                    }
                    _ => Ctx::Other,
                },
                gimli::DW_TAG_lexical_block => Ctx::Block(self.range(&e)),
                gimli::DW_TAG_formal_parameter | gimli::DW_TAG_variable => {
                    let is_param = e.tag() == gimli::DW_TAG_formal_parameter;
                    match (parent_func, stack.last()) {
                        (Some(fi), _) => {
                            if let Some(v) = self.var(&e, scope, is_param) {
                                if let Loc::Addr(_) = v.loc {
                                    let fname = self.out.functions[fi].name.clone();
                                    self.out.globals.push(VarDecl {
                                        name: format!("{}::{}", fname, v.name),
                                        ..v
                                    });
                                } else {
                                    self.out.functions[fi].vars.push(v);
                                }
                            }
                        }
                        (None, Some(Ctx::Cu)) => {
                            if let Some(v) = self.var(&e, None, false)
                                && matches!(v.loc, Loc::Addr(_))
                            {
                                self.out.globals.push(v);
                            }
                        }
                        _ => {}
                    }
                    Ctx::Other
                }
                _ => Ctx::Other,
            };
            stack.push(ctx);
        }
        Ok(())
    }

    fn void(&mut self) -> TypeId {
        if let Some(i) = self.out.types.iter().position(|t| matches!(t, TypeKind::Void)) {
            return i;
        }
        self.out.types.push(TypeKind::Void);
        self.out.types.len() - 1
    }

    fn type_ref(&mut self, e: &gimli::DebuggingInformationEntry<R>) -> Option<TypeId> {
        match e.attr_value(gimli::DW_AT_type) {
            Some(AttributeValue::UnitRef(off)) => Some(self.type_at(off)),
            _ => None,
        }
    }

    fn type_at(&mut self, off: UnitOffset) -> TypeId {
        if let Some(id) = self.cache.get(&off) {
            return *id;
        }
        // Se reserva el id antes de leer los hijos: los tipos recursivos (struct nodo *sig) lo reutilizan.
        self.out.types.push(TypeKind::Void);
        let id = self.out.types.len() - 1;
        self.cache.insert(off, id);
        let kind = self.read_type(off).unwrap_or(TypeKind::Void);
        self.out.types[id] = kind;
        id
    }

    fn children(&self, off: UnitOffset) -> Vec<gimli::DebuggingInformationEntry<R>> {
        let mut out = Vec::new();
        let Ok(mut cursor) = self.unit.entries_at_offset(off) else {
            return out;
        };
        if cursor.next_dfs().ok().flatten().is_none() {
            return out;
        }
        let base = cursor.depth();
        while let Ok(Some(e)) = cursor.next_dfs() {
            let e = e.clone();
            let d = cursor.depth();
            if d <= base {
                break;
            }
            if d == base + 1 {
                out.push(e);
            }
        }
        out
    }

    fn read_type(&mut self, off: UnitOffset) -> Option<TypeKind> {
        let e = self.unit.entry(off).ok()?;
        let size = e
            .attr_value(gimli::DW_AT_byte_size)
            .and_then(|v| v.udata_value())
            .unwrap_or(0);
        let tag: DwTag = e.tag();
        Some(match tag {
            gimli::DW_TAG_base_type => TypeKind::Base {
                name: self.name(&e).unwrap_or_default(),
                size,
                enc: match e.attr_value(gimli::DW_AT_encoding) {
                    Some(AttributeValue::Encoding(enc)) => enc,
                    _ => gimli::DW_ATE_signed,
                },
            },
            gimli::DW_TAG_pointer_type => TypeKind::Pointer {
                target: self.type_ref(&e),
            },
            gimli::DW_TAG_typedef => TypeKind::Typedef {
                name: self.name(&e).unwrap_or_default(),
                target: self.type_ref(&e),
            },
            gimli::DW_TAG_const_type => TypeKind::Qualified {
                qual: "const",
                target: self.type_ref(&e),
            },
            gimli::DW_TAG_volatile_type => TypeKind::Qualified {
                qual: "volatile",
                target: self.type_ref(&e),
            },
            gimli::DW_TAG_restrict_type => TypeKind::Qualified {
                qual: "restrict",
                target: self.type_ref(&e),
            },
            gimli::DW_TAG_atomic_type => TypeKind::Qualified {
                qual: "_Atomic",
                target: self.type_ref(&e),
            },
            gimli::DW_TAG_structure_type | gimli::DW_TAG_union_type => {
                let mut members = Vec::new();
                for c in self.children(off) {
                    if c.tag() != gimli::DW_TAG_member {
                        continue;
                    }
                    let offset = match c.attr_value(gimli::DW_AT_data_member_location) {
                        Some(AttributeValue::Exprloc(expr)) => {
                            let mut b = expr.0;
                            match Operation::parse(&mut b, self.unit.encoding()) {
                                Ok(Operation::PlusConstant { value }) => value,
                                _ => 0,
                            }
                        }
                        Some(v) => v.udata_value().unwrap_or(0),
                        None => 0,
                    };
                    let name = self.name(&c).unwrap_or_else(|| "<anónimo>".into());
                    let ty = self.type_ref(&c).unwrap_or_else(|| self.void());
                    members.push(Member { name, ty, offset });
                }
                TypeKind::Struct {
                    name: self.name(&e),
                    size,
                    members,
                    union: tag == gimli::DW_TAG_union_type,
                }
            }
            gimli::DW_TAG_array_type => {
                let elem = self.type_ref(&e).unwrap_or_else(|| self.void());
                let dims: Vec<Option<u64>> = self
                    .children(off)
                    .iter()
                    .filter(|c| c.tag() == gimli::DW_TAG_subrange_type)
                    .map(|c| {
                        c.attr_value(gimli::DW_AT_count)
                            .and_then(|v| v.udata_value())
                            .or_else(|| {
                                c.attr_value(gimli::DW_AT_upper_bound)
                                    .and_then(|v| v.udata_value())
                                    .map(|u| u + 1)
                            })
                    })
                    .collect();
                // int m[2][3] → arreglo de 2 arreglos de 3.
                let mut ty = elem;
                for d in dims.iter().skip(1).rev() {
                    self.out.types.push(TypeKind::Array { elem: ty, count: *d });
                    ty = self.out.types.len() - 1;
                }
                TypeKind::Array {
                    elem: ty,
                    count: dims.first().copied().flatten(),
                }
            }
            gimli::DW_TAG_enumeration_type => {
                let values = self
                    .children(off)
                    .iter()
                    .filter(|c| c.tag() == gimli::DW_TAG_enumerator)
                    .filter_map(|c| {
                        let v = c.attr_value(gimli::DW_AT_const_value)?;
                        let n = v.sdata_value().or_else(|| v.udata_value().map(|u| u as i64))?;
                        Some((self.name(c)?, n))
                    })
                    .collect();
                TypeKind::Enum {
                    name: self.name(&e),
                    size,
                    values,
                }
            }
            gimli::DW_TAG_subroutine_type => {
                let ret = self.type_ref(&e);
                let params = self
                    .children(off)
                    .iter()
                    .filter(|c| c.tag() == gimli::DW_TAG_formal_parameter)
                    .map(|c| self.type_ref(c))
                    .collect();
                TypeKind::Function { ret, params }
            }
            _ => TypeKind::Void,
        })
    }
}

/// Para cada función, el desplazamiento del CFA respecto del frame pointer al empezar su cuerpo,
/// según .eh_frame. `None` si no hay información o la regla no usa el frame pointer.
fn frame_offsets(obj: &object::File<'static>, endian: RunTimeEndian, functions: &[Function]) -> Vec<Option<u64>> {
    use gimli::UnwindSection;
    use object::{Object, ObjectSection};
    let Some(section) = obj.section_by_name(".eh_frame") else {
        return vec![None; functions.len()];
    };
    let Ok(data) = section.data() else {
        return vec![None; functions.len()];
    };
    let eh = gimli::EhFrame::new(data, endian);
    let mut bases = gimli::BaseAddresses::default().set_eh_frame(section.address());
    if let Some(text) = obj.section_by_name(".text") {
        bases = bases.set_text(text.address());
    }
    let mut ctx = gimli::UnwindContext::new();
    functions
        .iter()
        .map(|f| {
            let fde = eh
                .fde_for_address(&bases, f.body_start, gimli::EhFrame::cie_from_offset)
                .ok()?;
            let row = fde.unwind_info_for_address(&eh, &bases, &mut ctx, f.body_start).ok()?;
            match row.cfa() {
                gimli::CfaRule::RegisterAndOffset { register, offset } if register.0 == crate::arch::DWARF_FP => {
                    u64::try_from(*offset).ok()
                }
                _ => None,
            }
        })
        .collect()
}
