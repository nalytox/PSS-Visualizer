//! Compilación del programa con gcc y lectura de sus diagnósticos.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use trace_model::{CompileResult, Diagnostic, Severity};

pub const SOURCE_NAME: &str = "prog.c";
pub const BINARY_NAME: &str = "prog";

// -O0 y frame pointer: cada línea corresponde a instrucciones reconocibles y las variables viven en
// el stack. -z now resuelve las funciones de biblioteca al cargar, así el salto por la PLT va
// directo a la función real.
const FLAGS: &[&str] = &[
    "-g",
    "-O0",
    "-fno-omit-frame-pointer",
    "-pthread",
    "-no-pie",
    "-Wall",
    "-Wl,-z,now",
];

pub fn command_line() -> String {
    format!("gcc {} -o {BINARY_NAME} {SOURCE_NAME}", FLAGS.join(" "))
}

pub struct Compiled {
    pub result: CompileResult,
    pub binary: PathBuf,
}

pub fn compile(dir: &Path, source: &str) -> std::io::Result<Compiled> {
    std::fs::write(dir.join(SOURCE_NAME), source)?;
    let mut child = Command::new("gcc")
        .args(FLAGS)
        .args(["-fdiagnostics-color=never", "-o", BINARY_NAME, SOURCE_NAME])
        .current_dir(dir)
        .env("LC_ALL", "C")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break Some(s);
        }
        if start.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stderr = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut stderr);
    }
    let mut diagnostics = parse_diagnostics(&stderr);
    let ok = status.is_some_and(|s| s.success());
    if !ok && !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        let message = if status.is_none() {
            "la compilación tardó demasiado".to_string()
        } else {
            stderr.trim().to_string()
        };
        diagnostics.push(Diagnostic {
            line: 0,
            col: 0,
            severity: Severity::Error,
            message,
        });
    }
    Ok(Compiled {
        result: CompileResult {
            ok,
            command: command_line(),
            diagnostics,
        },
        binary: dir.join(BINARY_NAME),
    })
}

/// Líneas `prog.c:12:5: error: mensaje`.
pub fn parse_diagnostics(stderr: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let Some(rest) = line.strip_prefix(&format!("{SOURCE_NAME}:")) else {
            continue;
        };
        let mut parts = rest.splitn(4, ':');
        let (Some(l), Some(c), Some(kind), Some(msg)) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(line), Ok(col)) = (l.trim().parse(), c.trim().parse()) else {
            continue;
        };
        let severity = match kind.trim() {
            "error" | "fatal error" => Severity::Error,
            "warning" => Severity::Warning,
            "note" => Severity::Note,
            _ => continue,
        };
        out.push(Diagnostic {
            line,
            col,
            severity,
            message: msg.trim().to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gcc_diagnostics() {
        let text = "prog.c: In function 'main':\n\
                    prog.c:4:5: error: 'x' undeclared (first use in this function)\n\
                    prog.c:3:9: warning: unused variable 'y' [-Wunused-variable]\n\
                    prog.c:4:5: note: each undeclared identifier is reported only once\n";
        let d = parse_diagnostics(text);
        assert_eq!(d.len(), 3);
        assert_eq!((d[0].line, d[0].col, d[0].severity), (4, 5, Severity::Error));
        assert_eq!(d[1].severity, Severity::Warning);
        assert!(d[0].message.starts_with("'x' undeclared"));
    }
}
