//! Nombre de la función de biblioteca a la que salta el programa (printf, malloc, …), a partir del
//! mapa de memoria del proceso y la tabla de símbolos del .so correspondiente.

use crate::process::Mapping;
use object::{Object, ObjectSymbol};
use std::collections::HashMap;

#[derive(Default)]
pub struct Symbols {
    by_path: HashMap<String, Vec<(u64, u64, String)>>,
}

impl Symbols {
    pub fn resolve(&mut self, maps: &[Mapping], addr: u64) -> Option<String> {
        let m = maps
            .iter()
            .find(|m| addr >= m.start && addr < m.end && !m.path.is_empty())?;
        let base = maps
            .iter()
            .filter(|x| x.path == m.path)
            .map(|x| x.start - x.offset)
            .min()?;
        let table = self.by_path.entry(m.path.clone()).or_insert_with(|| load(&m.path));
        let rel = addr - base;
        let mut best: Option<&String> = None;
        for (start, size, name) in table.iter() {
            if rel >= *start && rel < start + (*size).max(1) {
                // Entre alias en la misma dirección se prefiere el nombre público: printf antes que _IO_printf.
                if best.is_none_or(|b| rank(name) < rank(b)) {
                    best = Some(name);
                }
            }
        }
        best.cloned()
    }
}

fn rank(name: &str) -> (usize, usize) {
    (name.chars().take_while(|c| *c == '_').count(), name.len())
}

fn load(path: &str) -> Vec<(u64, u64, String)> {
    let Ok(data) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(obj) = object::File::parse(&*data) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for s in obj.dynamic_symbols().chain(obj.symbols()) {
        if s.kind() != object::SymbolKind::Text || s.address() == 0 {
            continue;
        }
        if let Ok(name) = s.name() {
            let name = name.split('@').next().unwrap_or(name).to_string();
            out.push((s.address(), s.size(), name));
        }
    }
    out
}
