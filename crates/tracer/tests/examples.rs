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

#[test]
fn same_input_gives_the_same_trace() {
    assert_eq!(trace_example("02_lista_enlazada"), trace_example("02_lista_enlazada"));
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
    for name in ["01_structs", "02_lista_enlazada", "entrada_estandar"] {
        let t = trace_example(name);
        let errors: Vec<String> = validator.iter_errors(&t).take(3).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}
