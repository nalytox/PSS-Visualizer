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
    });
    serde_json::to_value(&trace).unwrap()
}

fn check(name: &str) -> serde_json::Value {
    let actual = trace_example(name);
    let path = root().join(format!("traces/reference/{name}.json"));
    if std::env::var_os("UPDATE_REFERENCE").is_some() {
        std::fs::write(&path, serde_json::to_string(&actual).unwrap() + "\n").unwrap();
        return actual;
    }
    let expected: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        actual == expected,
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
    ] {
        let t = trace_example(name);
        let errors: Vec<String> = validator.iter_errors(&t).take(3).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}
