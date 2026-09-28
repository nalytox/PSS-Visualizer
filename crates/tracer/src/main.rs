//! pss-tracer: compila un programa C, lo ejecuta bajo ptrace y escribe su traza JSON.
//!
//!   pss-tracer --source prog.c [--stdin entrada.txt] [--stdin-eof] [--limits config/limits.toml] [--out traza.json]

use pss_tracer::{limits, tracer};

use std::io::Write;

fn usage() -> ! {
    eprintln!(
        "uso: pss-tracer --source prog.c [--stdin archivo] [--stdin-eof] [--limits archivo] [--inject t:SIGINT] [--out archivo]"
    );
    std::process::exit(2);
}

fn main() {
    let mut source = None;
    let mut stdin = Vec::new();
    let mut stdin_eof = false;
    let mut limits = limits::Limits::default();
    let mut out = None;
    let mut injections = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--source" => source = Some(args.next().unwrap_or_else(|| usage())),
            "--stdin" => {
                stdin = std::fs::read(args.next().unwrap_or_else(|| usage()))
                    .unwrap_or_else(|e| fail(&format!("stdin: {e}")))
            }
            "--stdin-eof" => stdin_eof = true,
            "--limits" => {
                let text = std::fs::read_to_string(args.next().unwrap_or_else(|| usage()))
                    .unwrap_or_else(|e| fail(&format!("límites: {e}")));
                limits = limits::Limits::parse(&text).unwrap_or_else(|e| fail(&e));
            }
            "--out" => out = Some(args.next().unwrap_or_else(|| usage())),
            "--inject" => {
                let spec = args.next().unwrap_or_else(|| usage());
                injections.push(parse_injection(&spec).unwrap_or_else(|| fail(&format!("inyección inválida: {spec}"))));
            }
            _ => usage(),
        }
    }
    let source_path = source.unwrap_or_else(|| usage());
    let source = std::fs::read_to_string(&source_path).unwrap_or_else(|e| fail(&format!("{source_path}: {e}")));
    let trace = tracer::run(&tracer::Options {
        source,
        stdin,
        stdin_eof,
        limits,
        injections,
    });
    let json = serde_json::to_string(&trace).unwrap();
    match out {
        Some(path) => std::fs::write(&path, json).unwrap_or_else(|e| fail(&format!("{path}: {e}"))),
        None => {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(json.as_bytes());
        }
    }
}

/// `12:SIGINT`: después del paso 12, la terminal envía SIGINT al grupo en primer plano.
fn parse_injection(spec: &str) -> Option<(u64, i32)> {
    let (t, sig) = spec.split_once(':')?;
    let sig: nix::sys::signal::Signal = sig.parse().ok()?;
    Some((t.parse().ok()?, sig as i32))
}

fn fail(msg: &str) -> ! {
    eprintln!("pss-tracer: {msg}");
    std::process::exit(1);
}
