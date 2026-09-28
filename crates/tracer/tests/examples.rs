//! Cada ejemplo de examples/ se traza y se compara con su traza de referencia en traces/reference/.
//! Para regenerarlas tras un cambio intencional: UPDATE_REFERENCE=1 cargo test -p pss-tracer --test examples

use pss_tracer::limits::Limits;
use pss_tracer::tracer::{Options, run};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Mismos ajustes que usa la interfaz al ejecutar un ejemplo: stdin precargado y abierto.
fn trace_example(name: &str) -> serde_json::Value {
    let dir = root().join("examples");
    let source = std::fs::read_to_string(dir.join(format!("{name}.c"))).unwrap();
    let stdin = std::fs::read(dir.join(format!("{name}.stdin"))).unwrap_or_default();
    let limits = Limits::parse(&std::fs::read_to_string(root().join("config/limits.toml")).unwrap()).unwrap();
    let trace = run(&Options {
        source,
        stdin,
        stdin_eof: false,
        limits,
        injections: vec![],
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    });
    serde_json::to_value(&trace).unwrap()
}

/// Las referencias dependen de la arquitectura (direcciones, syscalls que usa glibc): las de x86_64
/// están en traces/reference/ y las de aarch64 en traces/reference/aarch64/.
fn reference_path(name: &str) -> PathBuf {
    if cfg!(target_arch = "aarch64") {
        root().join(format!("traces/reference/aarch64/{name}.json"))
    } else {
        root().join(format!("traces/reference/{name}.json"))
    }
}

/// Las direcciones de pila dependen de la máquina (el frame de una señal crece con las extensiones
/// XSAVE de la CPU, las pilas de hilos con el kernel): se comparan las trazas sin ellas. En una misma
/// máquina la traza es idéntica byte a byte (ver same_input_gives_the_same_trace).
fn without_addresses(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) if s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit()) => {
            "0x".into()
        }
        serde_json::Value::Array(a) => a.iter().map(without_addresses).collect(),
        serde_json::Value::Object(o) => o.iter().map(|(k, x)| (k.clone(), without_addresses(x))).collect(),
        x => x.clone(),
    }
}

fn check(name: &str) -> serde_json::Value {
    let actual = trace_example(name);
    let path = reference_path(name);
    if std::env::var_os("UPDATE_REFERENCE").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&actual).unwrap() + "\n").unwrap();
        return actual;
    }
    if !path.exists() {
        // Sin referencia grabada para esta arquitectura: se validan solo las propiedades de cada prueba.
        eprintln!("{name}: sin referencia en {}", path.display());
        return actual;
    }
    let expected: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        without_addresses(&actual) == without_addresses(&expected),
        "{name}: la traza cambió respecto de la referencia (UPDATE_REFERENCE=1 para regenerarla)"
    );
    actual
}

fn outcome(t: &serde_json::Value) -> &str {
    t["outcome"]["kind"].as_str().unwrap()
}

#[test]
fn structs() {
    let t = check("01_structs");
    assert_eq!(outcome(&t), "exited");
    let out: Vec<&str> = t["output"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["bytes"].as_str().unwrap())
        .collect();
    assert_eq!(out, ["pts[1] = (4, 5)\n", "esquina = (0, 2)\n"]);
}

#[test]
fn linked_list_reports_leaks() {
    let t = check("02_lista_enlazada");
    assert_eq!(outcome(&t), "exited");
    let leaks = t["summary"]["leaks"].as_array().unwrap();
    assert_eq!(leaks.len(), 3, "4 nodos, 1 liberado");
    assert!(leaks.iter().all(|l| l["type"] == "struct nodo"));
}

#[test]
fn stdin_asks_for_more_when_it_runs_out() {
    let t = check("entrada_estandar");
    assert_eq!(outcome(&t), "awaitingInput");
    assert_eq!(t["steps"].as_array().unwrap().last().unwrap()["stdin"]["consumed"], 6);
}

fn processes_at(t: &serde_json::Value, step: usize) -> &Vec<serde_json::Value> {
    t["steps"][step]["processes"].as_array().unwrap()
}

fn outputs(t: &serde_json::Value) -> Vec<(u64, String)> {
    t["output"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["pid"].as_u64().unwrap(), c["bytes"].as_str().unwrap().to_string()))
        .collect()
}

fn events_of<'a>(t: &'a serde_json::Value, kind: &str) -> Vec<&'a serde_json::Value> {
    t["steps"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["events"].as_array().unwrap())
        .filter(|e| e["type"] == kind)
        .collect()
}

#[test]
fn fork_wait_reaps_the_zombie() {
    let t = check("03_fork_simple");
    assert_eq!(t["outcome"]["code"], 0);
    let out = outputs(&t);
    assert!(out.contains(&(1001, "hijo: x = 15\n".into())));
    assert!(out.contains(&(1000, "padre: x = 5, mi hijo es 1001\n".into())));
    assert_eq!(out.last().unwrap().1, "el hijo termin\u{c3}\u{b3} con 3\n");
    let wait = events_of(&t, "wait");
    assert_eq!(wait.len(), 1);
    assert_eq!(wait[0]["reaped"], 1001);
    assert_eq!(wait[0]["status"]["code"], 3);
    // El hijo pasa por zombie antes de que el padre lo recoja.
    let states: Vec<&str> = (0..t["steps"].as_array().unwrap().len())
        .filter_map(|k| processes_at(&t, k).iter().find(|p| p["pid"] == 1001))
        .map(|p| p["state"].as_str().unwrap())
        .collect();
    let zombie = states.iter().position(|s| *s == "zombie").unwrap();
    assert!(states[zombie..].iter().all(|s| *s == "zombie" || *s == "reaped"));
    assert_eq!(*states.last().unwrap(), "reaped");
}

#[test]
fn three_forks_make_eight_processes() {
    let t = check("04_fork_bucle");
    let last = processes_at(&t, t["steps"].as_array().unwrap().len() - 1);
    assert_eq!(last.len(), 8);
    let mut lines: Vec<String> = outputs(&t).into_iter().map(|(_, s)| s).collect();
    lines.sort();
    assert_eq!(lines.len(), 8);
    // Los hijos cuyo padre ya terminó ven a init como padre.
    assert!(!events_of(&t, "reparent").is_empty());
}

#[test]
fn exec_turns_the_child_into_a_black_box() {
    let t = check("05_exec");
    let exec = events_of(&t, "exec");
    assert_eq!(exec.len(), 1);
    assert_eq!(exec[0]["pid"], 1001);
    assert_eq!(exec[0]["argv"], serde_json::json!(["ls"]));
    assert!(exec[0]["path"].as_str().unwrap().ends_with("/ls"));
    let out = outputs(&t);
    assert_eq!(out[1], (1001, "prog  prog.c\n".into()));
    assert_eq!(out[2], (1000, "ls termin\u{c3}\u{b3}\n".into()));
    let after = t["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["events"].as_array().unwrap().iter().any(|e| e["type"] == "exec"))
        .unwrap();
    let child = after["processes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pid"] == 1001)
        .unwrap();
    assert_eq!(child["image"]["kind"], "blackbox");
    assert!(child["mem"].is_null());
}

#[test]
fn fork_bomb_stops_at_the_process_limit() {
    let t = check("16_fork_bomb");
    assert_eq!(t["truncated"], true);
    assert_eq!(t["truncatedReason"], "processes");
    let last = processes_at(&t, t["steps"].as_array().unwrap().len() - 1);
    assert_eq!(last.len(), 32);
}

#[test]
fn pipe_carries_bytes_from_child_to_parent_until_eof() {
    let t = check("06_pipe_padre_hijo");
    assert_eq!(t["outcome"]["code"], 0);
    assert_eq!(
        outputs(&t),
        [(1000, "el padre ley\u{c3}\u{b3}: hola pap\u{c3}\u{a1}\n".to_string())]
    );
    let reads = events_of(&t, "read");
    assert_eq!(reads.first().unwrap()["pipe"], "p0");
    assert_eq!(reads.last().unwrap()["eof"], true);
}

#[test]
fn an_unclosed_write_end_blocks_the_reader_forever() {
    let t = check("07_pipe_sin_cerrar");
    assert_eq!(outcome(&t), "deadlock");
    let last = t["steps"].as_array().unwrap().last().unwrap();
    let warnings = last["pipes"][0]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["pid"], 1000);
    assert_eq!(warnings[0]["end"], "w");
}

#[test]
fn pipeline_connects_three_black_boxes() {
    let t = check("08_pipeline");
    assert_eq!(outputs(&t), [(1003, "1\n".to_string())]);
    assert_eq!(events_of(&t, "exec").len(), 3);
    let dups: Vec<(u64, u64)> = events_of(&t, "dup")
        .iter()
        .map(|e| (e["oldfd"].as_u64().unwrap(), e["newfd"].as_u64().unwrap()))
        .collect();
    assert_eq!(dups.len(), 4);
    assert!(dups.contains(&(3, 0)) && dups.contains(&(4, 1)) && dups.contains(&(6, 1)) && dups.contains(&(5, 0)));
}

#[test]
fn sigusr1_runs_the_handler_in_the_child() {
    let t = check("09_sigusr1");
    let send = events_of(&t, "signalSend");
    assert_eq!(send.len(), 1);
    assert_eq!(
        send[0]["from"],
        serde_json::json!({"kind": "process", "pid": 1000, "via": "kill"})
    );
    let deliver = events_of(&t, "signalDeliver");
    assert_eq!(deliver[0]["handler"], "manejador");
    // Durante el handler, la pila muestra el handler encima de main.
    let step = t["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["processes"][1]["threads"][0]["inHandler"] == "SIGUSR1")
        .unwrap();
    let mem = step["processes"][1]["mem"].as_str().unwrap();
    let frames = t["snapshots"][mem]["stacks"]["1001"].as_array().unwrap();
    assert_eq!(frames[0]["fn"], "manejador");
    assert_eq!(frames[0]["signal"], "SIGUSR1");
    assert_eq!(frames.last().unwrap()["fn"], "main");
    assert!(frames.last().unwrap().get("signal").is_none());
    assert_eq!(events_of(&t, "signalReturn").len(), 1);
}

#[test]
fn sigchld_comes_from_the_kernel_and_the_handler_reaps() {
    let t = check("10_sigchld");
    let send = events_of(&t, "signalSend");
    assert_eq!(
        send[0]["from"],
        serde_json::json!({"kind": "kernel", "cause": "SIGCHLD"})
    );
    assert_eq!(events_of(&t, "wait")[0]["reaped"], 1001);
    assert_eq!(outputs(&t).last().unwrap().1, "padre: termino\n");
}

#[test]
fn ctrl_c_is_injected_after_the_chosen_step() {
    let t = check("11_sigint");
    assert_eq!(
        outputs(&t),
        [(1000, "llegu\u{c3}\u{a9} a 10 sin interrupciones\n".to_string())]
    );
    let dir = root().join("examples");
    let trace = run(&Options {
        source: std::fs::read_to_string(dir.join("11_sigint.c")).unwrap(),
        stdin: vec![],
        stdin_eof: false,
        limits: Limits::default(),
        injections: vec![(12, libc_sigint())],
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    });
    let v = serde_json::to_value(&trace).unwrap();
    assert_eq!(outputs(&v), [(1000, "me interrumpiste en la vuelta 2\n".to_string())]);
    assert_eq!(
        v["run"]["injections"][0],
        serde_json::json!({"t": 12, "signal": "SIGINT", "target": "foreground"})
    );
    assert_eq!(v["steps"][13]["events"][0]["from"]["kind"], "terminal");
    // Hasta el paso 12 la traza es la misma que sin Ctrl+C.
    assert_eq!(v["steps"][12], t["steps"][12]);
}

fn run_example(
    name: &str,
    policy: trace_model::Policy,
    seed: u64,
    schedule: Vec<trace_model::TaskRef>,
) -> serde_json::Value {
    let source = std::fs::read_to_string(root().join("examples").join(format!("{name}.c"))).unwrap();
    let trace = run(&Options {
        source,
        stdin: vec![],
        stdin_eof: false,
        limits: Limits::default(),
        injections: vec![],
        policy,
        seed,
        schedule,
    });
    serde_json::to_value(&trace).unwrap()
}

fn task(pid: u32, tid: u32) -> trace_model::TaskRef {
    trace_model::TaskRef { pid, tid }
}

#[test]
fn race_loses_updates_and_a_manual_schedule_avoids_it() {
    let t = check("12_hilos_carrera");
    let out = outputs(&t);
    assert!(out[0].1.starts_with("contador = 3 "), "{out:?}");
    // Hilos visibles: dos creaciones, dos fines de hilo con join.
    assert_eq!(events_of(&t, "threadCreate").len(), 2);
    assert_eq!(events_of(&t, "join").len(), 2);
    // Manual: main crea los dos hilos, luego corre A entero y después B.
    let mut schedule = vec![task(1000, 1000), task(1000, 1000)];
    schedule.extend(std::iter::repeat_n(task(1000, 1001), 20));
    schedule.extend(std::iter::repeat_n(task(1000, 1002), 20));
    let m = run_example("12_hilos_carrera", trace_model::Policy::Manual, 0, schedule);
    assert!(outputs(&m)[0].1.starts_with("contador = 6 "), "{:?}", outputs(&m));
    assert_eq!(m["run"]["policy"], "manual");
}

#[test]
fn mutex_serializes_and_shows_the_wait() {
    let t = check("13_hilos_mutex");
    assert_eq!(outputs(&t), [(1000, "contador = 6\n".to_string())]);
    let blocked = events_of(&t, "mutex")
        .iter()
        .filter(|e| e["result"] == "blocked")
        .count();
    assert!(blocked > 0, "algún hilo tuvo que esperar el mutex");
    let with_owner = t["steps"].as_array().unwrap().iter().any(|s| {
        s["sync"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "mutex" && o["name"] == "candado" && !o["owner"].is_null())
    });
    assert!(with_owner);
}

#[test]
fn producer_consumer_passes_every_item() {
    let t = check("14_productor_consumidor");
    let items: Vec<String> = outputs(&t).into_iter().map(|(_, s)| s).collect();
    assert_eq!(
        items,
        [
            "consum\u{c3}\u{ad} 1\n",
            "consum\u{c3}\u{ad} 2\n",
            "consum\u{c3}\u{ad} 3\n",
            "consum\u{c3}\u{ad} 4\n"
        ]
    );
    assert!(events_of(&t, "cond").iter().any(|e| e["op"] == "signal"));
}

#[test]
fn two_mutexes_in_opposite_order_deadlock() {
    let t = check("15_deadlock");
    assert_eq!(outcome(&t), "deadlock");
    let last = t["steps"].as_array().unwrap().last().unwrap();
    let threads = last["processes"][0]["threads"].as_array().unwrap();
    let waiting: Vec<&str> = threads
        .iter()
        .filter_map(|th| th["blockedOn"]["kind"].as_str())
        .collect();
    assert_eq!(waiting, ["join", "mutex", "mutex"]);
}

#[test]
fn random_policy_is_reproducible_with_its_seed() {
    let a = run_example("12_hilos_carrera", trace_model::Policy::Random, 7, vec![]);
    let b = run_example("12_hilos_carrera", trace_model::Policy::Random, 7, vec![]);
    assert_eq!(a, b);
    assert_eq!(a["run"]["seed"], 7);
}

fn libc_sigint() -> i32 {
    2
}

#[test]
fn same_input_gives_the_same_trace() {
    assert_eq!(trace_example("02_lista_enlazada"), trace_example("02_lista_enlazada"));
    assert_eq!(trace_example("04_fork_bucle"), trace_example("04_fork_bucle"));
}

#[test]
fn compile_errors_come_back_with_their_line() {
    let t = run(&Options {
        source: "int main(void) {\n    return x;\n}\n".into(),
        stdin: vec![],
        stdin_eof: true,
        limits: Limits::default(),
        injections: vec![],
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    });
    assert!(matches!(t.outcome, trace_model::Outcome::CompileError));
    assert!(
        t.compile
            .diagnostics
            .iter()
            .any(|d| d.line == 2 && d.severity == trace_model::Severity::Error)
    );
}

#[test]
fn infinite_loops_are_truncated() {
    let limits = Limits {
        max_steps: 200,
        ..Limits::default()
    };
    let t = run(&Options {
        source: "int main(void) {\n    int i = 0;\n    while (1)\n        i++;\n}\n".into(),
        stdin: vec![],
        stdin_eof: true,
        limits,
        injections: vec![],
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    });
    assert!(
        t.truncated,
        "outcome {:?}, {} pasos, eventos finales {:?}",
        t.outcome,
        t.steps.len(),
        t.steps.last().map(|s| &s.events)
    );
    assert_eq!(t.truncated_reason, Some(trace_model::TruncatedReason::Steps));
    assert_eq!(t.steps.len(), 200);
}

#[test]
fn segfault_is_reported() {
    let src = "int main(void) {\n    int *p = 0;\n    *p = 1;\n    return 0;\n}\n";
    let t = run(&Options {
        source: src.into(),
        stdin: vec![],
        stdin_eof: true,
        limits: Limits::default(),
        injections: vec![],
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    });
    assert!(
        matches!(&t.outcome, trace_model::Outcome::Signaled { signal } if signal == "SIGSEGV"),
        "outcome {:?}, reason {:?}, {} pasos",
        t.outcome,
        t.truncated_reason,
        t.steps.len()
    );
    assert_eq!(t.summary.mem_errors.len(), 1);
}

#[test]
fn output_before_exit_is_captured_and_attributed() {
    let src = "#include <stdio.h>\nint main(void) {\n    printf(\"a\");\n    printf(\"b\\n\");\n    return 0;\n}\n";
    let t = run(&Options {
        source: src.into(),
        stdin: vec![],
        stdin_eof: true,
        limits: Limits::default(),
        injections: vec![],
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    });
    let all: String = t.output.iter().map(|c| c.bytes.clone()).collect();
    assert_eq!(all, "ab\n");
}

#[test]
fn every_reference_trace_matches_the_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("schema/trace.schema.json")).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for name in [
        "01_structs",
        "02_lista_enlazada",
        "entrada_estandar",
        "03_fork_simple",
        "04_fork_bucle",
        "05_exec",
        "16_fork_bomb",
        "06_pipe_padre_hijo",
        "07_pipe_sin_cerrar",
        "08_pipeline",
        "09_sigusr1",
        "10_sigchld",
        "11_sigint",
        "12_hilos_carrera",
        "13_hilos_mutex",
        "14_productor_consumidor",
        "15_deadlock",
    ] {
        let t = trace_example(name);
        let errors: Vec<String> = validator.iter_errors(&t).take(3).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}
