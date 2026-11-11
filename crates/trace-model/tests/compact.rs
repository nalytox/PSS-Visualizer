use std::fs;
use std::path::{Path, PathBuf};
use trace_model::Trace;
use trace_model::compact::{self, CompactTrace, FLAGS};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn traces() -> Vec<(PathBuf, Trace)> {
    let mut out = Vec::new();
    for dir in ["traces/synthetic", "traces/reference"] {
        for entry in fs::read_dir(repo_root().join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let trace = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
                out.push((path, trace));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// La traza con cada `mem` reemplazado por el contenido de su instantánea: los nombres de las
/// instantáneas cambian al compactar, lo que muestran no.
fn resolved(trace: &Trace) -> serde_json::Value {
    let mut v = serde_json::to_value(trace).unwrap();
    let snapshots = v["snapshots"].take();
    for step in v["steps"].as_array_mut().unwrap() {
        for p in step["processes"].as_array_mut().unwrap() {
            if let Some(id) = p["mem"].as_str() {
                p["mem"] = snapshots[id].clone();
            }
        }
    }
    v
}

#[test]
fn every_trace_survives_compaction() {
    for (path, trace) in traces() {
        // Bloques chicos para cruzar muchos bordes de bloque; y el tamaño real.
        for chunk in [3, 1000] {
            let c = compact::encode(&trace, chunk);
            // Pasa por JSON, como en el servidor.
            let c: CompactTrace = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
            let back = compact::decode(&c).unwrap();
            assert_eq!(
                resolved(&back),
                resolved(&trace),
                "{} (bloques de {chunk})",
                path.display()
            );
            assert_eq!(c.step_count as usize, trace.steps.len());
            assert_eq!(c.chunks.len(), trace.steps.len().div_ceil(chunk));
        }
    }
}

#[test]
fn decoded_traces_match_the_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("schema/trace.schema.json")).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for (path, trace) in traces() {
        let back = compact::decode(&compact::encode(&trace, 4)).unwrap();
        let instance = serde_json::to_value(&back).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .take(5)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
    }
}

#[test]
fn compact_traces_match_their_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("schema/compact.schema.json")).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for (path, trace) in traces() {
        let instance = serde_json::to_value(compact::encode(&trace, 5)).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .take(5)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
    }
}

#[test]
fn the_index_answers_without_opening_chunks() {
    for (path, trace) in traces() {
        let c = compact::encode(&trace, 1000);
        let pids = compact::decode_column(&c.index.actor_pid).unwrap();
        let tids = compact::decode_column(&c.index.actor_tid).unwrap();
        let lines = compact::decode_column(&c.index.actor_line).unwrap();
        let flags = compact::decode_column(&c.index.flags).unwrap();
        for (t, step) in trace.steps.iter().enumerate() {
            assert_eq!(pids[t], step.actor.map_or(0, |a| a.pid), "{} t={t}", path.display());
            assert_eq!(tids[t], step.actor.map_or(0, |a| a.tid));
            let line = step.actor.and_then(|a| {
                let p = step.processes.iter().find(|p| p.pid == a.pid)?;
                p.threads.iter().find(|th| th.tid == a.tid)?.line
            });
            assert_eq!(lines[t], line.unwrap_or(0));
            assert_eq!(flags[t], compact::event_flags(&step.events));
            // Cada clase de evento del paso tiene su bandera.
            for ev in &step.events {
                let name = serde_json::to_value(ev).unwrap()["type"].as_str().unwrap().to_string();
                let expected = match name.as_str() {
                    "exit" | "stdinNeeded" | "truncated" => continue,
                    n => n.to_string(),
                };
                let bit = FLAGS.iter().position(|f| *f == expected).unwrap();
                assert!(
                    flags[t] & (1 << bit) != 0,
                    "{} t={t} sin bandera {expected}",
                    path.display()
                );
            }
        }
        let first_pid = trace.steps[0].processes[0].pid;
        assert_eq!(c.index.pids[0], first_pid);
    }
}

#[test]
fn unchanged_memory_is_not_repeated() {
    // En 04_fork_bucle varios procesos esperan sin cambiar su memoria: no se vuelve a escribir.
    let (_, trace) = traces()
        .into_iter()
        .find(|(p, _)| p.ends_with("04_fork_bucle.json"))
        .unwrap();
    let full = serde_json::to_string(&trace).unwrap().len();
    let c = compact::encode(&trace, 1000);
    let packed: usize = c.chunks.iter().map(|ch| ch.data.len()).sum();
    assert!(packed * 10 < full, "{packed} bytes compactos contra {full} completos");
}

#[test]
fn typed_snapshot_patches_rebuild_the_snapshot() {
    use trace_model::delta::{apply, snapshot_patch};
    for (path, trace) in traces() {
        let snaps: Vec<_> = trace.snapshots.values().collect();
        // Cada instantánea contra las cercanas y contra unas lejanas: parecidas y distintas.
        for (i, a) in snaps.iter().enumerate() {
            for b in snaps.iter().skip(i).take(4).chain(snaps.iter().step_by(7)) {
                let ja = serde_json::to_value(a).unwrap();
                let jb = serde_json::to_value(b).unwrap();
                let back = snapshot_patch(a, b).map_or(ja.clone(), |p| apply(&ja, &p));
                assert_eq!(back, jb, "{}", path.display());
            }
        }
    }
}

/// Trazas compactas de muestra para las pruebas de la interfaz (web/src/trace/loaded.test.ts):
/// la interfaz debe leerlas igual que las originales. `UPDATE_REFERENCE=1` las regenera.
#[test]
fn compact_fixtures_are_up_to_date() {
    let dir = repo_root().join("traces/compact");
    for name in ["02_lista_enlazada", "04_fork_bucle", "10_sigchld", "13_hilos_mutex"] {
        let trace: Trace = serde_json::from_str(
            &fs::read_to_string(repo_root().join(format!("traces/reference/{name}.json"))).unwrap(),
        )
        .unwrap();
        let text = serde_json::to_string(&compact::encode(&trace, 5)).unwrap() + "\n";
        let path = dir.join(format!("{name}.json"));
        if std::env::var_os("UPDATE_REFERENCE").is_some() {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&path, &text).unwrap();
        } else {
            let saved = fs::read_to_string(&path).unwrap_or_default();
            assert!(
                saved == text,
                "{} está desactualizada: UPDATE_REFERENCE=1 cargo test -p trace-model",
                path.display()
            );
        }
    }
}
