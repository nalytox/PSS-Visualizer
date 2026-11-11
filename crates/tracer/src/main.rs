//! pss-tracer: compila un programa C, lo ejecuta bajo ptrace y escribe su traza JSON.
//!
//!   pss-tracer --source prog.c [--stdin entrada.txt] [--stdin-eof] [--limits config/limits.toml] [--out traza.json]
//!
//! Por defecto escribe la traza compacta (versión 2); `--format v1` escribe todos los pasos
//! completos. Con `--progress` informa por stderr cuántos pasos lleva (`progress N`). SIGUSR1
//! detiene la ejecución y escribe lo recorrido hasta ahí.

use pss_tracer::{limits, tracer};

use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn usage() -> ! {
    eprintln!(
        "uso: pss-tracer --source prog.c [--stdin archivo] [--stdin-eof] [--limits archivo] [--inject t:SIGINT] [--policy round-robin|random|manual] [--seed N] [--schedule pid:tid*n,…] [--schedule-file archivo] [--format v1|v2] [--progress] [--out archivo]"
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
    let mut policy = trace_model::Policy::RoundRobin;
    let mut seed = 0;
    let mut schedule = Vec::new();
    let mut compact = true;
    let mut progress = false;
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
            "--policy" => {
                policy = match args.next().as_deref() {
                    Some("round-robin") => trace_model::Policy::RoundRobin,
                    Some("random") => trace_model::Policy::Random,
                    Some("manual") => trace_model::Policy::Manual,
                    _ => usage(),
                }
            }
            "--seed" => seed = args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "--schedule" => {
                let spec = args.next().unwrap_or_else(|| usage());
                schedule = parse_schedule(&spec).unwrap_or_else(|| fail(&format!("planificación inválida: {spec}")));
            }
            "--schedule-file" => {
                let path = args.next().unwrap_or_else(|| usage());
                let spec = std::fs::read_to_string(&path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
                schedule = parse_schedule(spec.trim()).unwrap_or_else(|| fail("planificación inválida"));
            }
            "--inject" => {
                let spec = args.next().unwrap_or_else(|| usage());
                injections.push(parse_injection(&spec).unwrap_or_else(|| fail(&format!("inyección inválida: {spec}"))));
            }
            "--format" => {
                compact = match args.next().as_deref() {
                    Some("v1") => false,
                    Some("v2") => true,
                    _ => usage(),
                }
            }
            "--progress" => progress = true,
            _ => usage(),
        }
    }
    let source_path = source.unwrap_or_else(|| usage());
    let source = std::fs::read_to_string(&source_path).unwrap_or_else(|e| fail(&format!("{source_path}: {e}")));
    unsafe {
        libc::signal(libc::SIGUSR1, on_stop as *const () as libc::sighandler_t);
    }
    if progress {
        std::thread::spawn(|| {
            let mut last = u64::MAX;
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let n = tracer::PROGRESS.load(Ordering::Relaxed);
                if n != last {
                    eprintln!("progress {n}");
                    last = n;
                }
            }
        });
    }
    let opts = tracer::Options {
        source,
        stdin,
        stdin_eof,
        limits,
        injections,
        policy,
        seed,
        schedule,
    };
    let json = if compact {
        serde_json::to_vec(&tracer::run_compact(&opts))
    } else {
        serde_json::to_vec(&tracer::run(&opts))
    }
    .unwrap();
    match out {
        Some(path) => std::fs::write(&path, json).unwrap_or_else(|e| fail(&format!("{path}: {e}"))),
        None => {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(&json);
        }
    }
}

extern "C" fn on_stop(_: libc::c_int) {
    tracer::STOP.store(true, Ordering::Relaxed);
}

/// `1000:1000,1000:1001*40`: cada tarea, con cuántos pasos seguidos la eligió la persona.
fn parse_schedule(spec: &str) -> Option<Vec<trace_model::TaskRef>> {
    let mut out = Vec::new();
    for item in spec.split(',').filter(|x| !x.is_empty()) {
        let (task, n) = match item.split_once('*') {
            Some((task, n)) => (task, n.parse::<usize>().ok()?),
            None => (item, 1),
        };
        let (p, t) = task.split_once(':')?;
        let r = trace_model::TaskRef {
            pid: p.parse().ok()?,
            tid: t.parse().ok()?,
        };
        if out.len() + n > 10_000_000 {
            return None;
        }
        out.extend(std::iter::repeat_n(r, n));
    }
    Some(out)
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
