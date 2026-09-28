use std::fs;
use std::path::{Path, PathBuf};
use trace_model::Trace;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn trace_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["traces/synthetic", "traces/reference"] {
        for entry in fs::read_dir(repo_root().join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn every_trace_roundtrips_without_loss() {
    let files = trace_files();
    assert!(!files.is_empty());
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        let original: serde_json::Value = serde_json::from_str(&text).unwrap();
        let trace: Trace =
            serde_json::from_value(original.clone()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let back = serde_json::to_value(&trace).unwrap();
        assert_eq!(original, back, "{} cambia al leer y reescribir", path.display());
    }
}

#[test]
fn every_trace_matches_the_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("schema/trace.schema.json")).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for path in trace_files() {
        let instance: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .take(5)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
    }
}

#[test]
fn serialized_model_matches_the_schema() {
    // Lo que produzca el tracer (serializando estos tipos) también debe validar.
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("schema/trace.schema.json")).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for path in trace_files() {
        let trace: Trace = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let value = serde_json::to_value(&trace).unwrap();
        assert!(validator.is_valid(&value), "{}", path.display());
    }
}
