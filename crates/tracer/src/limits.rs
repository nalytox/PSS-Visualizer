//! Límites de ejecución (sección 13), leídos de `config/limits.toml`.

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_processes: u32,
    pub max_threads_per_process: u32,
    pub max_steps: u64,
    pub wall_time_ms: u64,
    pub memory_bytes: u64,
    pub output_bytes_per_process: u64,
    pub trace_bytes_compressed: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_processes: 32,
            max_threads_per_process: 16,
            max_steps: 5000,
            wall_time_ms: 10_000,
            memory_bytes: 256 << 20,
            output_bytes_per_process: 64 << 10,
            trace_bytes_compressed: 20 << 20,
        }
    }
}

impl Limits {
    /// Formato mínimo: líneas `clave = número`, con comentarios `#`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut l = Limits::default();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("línea {}: se esperaba clave = valor", i + 1))?;
            let v: u64 = v
                .trim()
                .replace('_', "")
                .parse()
                .map_err(|_| format!("línea {}: número inválido", i + 1))?;
            match k.trim() {
                "max_processes" => l.max_processes = v as u32,
                "max_threads_per_process" => l.max_threads_per_process = v as u32,
                "max_steps" => l.max_steps = v,
                "wall_time_ms" => l.wall_time_ms = v,
                "memory_bytes" => l.memory_bytes = v,
                "output_bytes_per_process" => l.output_bytes_per_process = v,
                "trace_bytes_compressed" => l.trace_bytes_compressed = v,
                other => return Err(format!("línea {}: límite desconocido {other}", i + 1)),
            }
        }
        Ok(l)
    }

    pub fn to_model(&self) -> trace_model::Limits {
        trace_model::Limits {
            max_processes: self.max_processes,
            max_threads_per_process: self.max_threads_per_process,
            max_steps: self.max_steps,
            wall_time_ms: self.wall_time_ms,
            memory_bytes: self.memory_bytes,
            output_bytes_per_process: self.output_bytes_per_process,
            trace_bytes_compressed: self.trace_bytes_compressed,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_the_repository_config() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/limits.toml")).unwrap();
        let l = super::Limits::parse(&text).unwrap();
        assert_eq!(l.max_steps, 5000);
        assert_eq!(l.memory_bytes, 268435456);
    }
}
