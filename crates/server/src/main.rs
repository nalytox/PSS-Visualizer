//! Servidor local: sirve la interfaz y ejecuta programas con pss-tracer.

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio::sync::Semaphore;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
use tower_http::services::{ServeDir, ServeFile};

struct Config {
    addr: SocketAddr,
    web_dist: PathBuf,
    tracer: PathBuf,
    limits: Option<PathBuf>,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        // Por defecto solo escucha en localhost: el servidor ejecuta código C y no debe quedar
        // expuesto a la red local. Docker necesita PSS_HOST=0.0.0.0 dentro del contenedor.
        let host: IpAddr = match std::env::var("PSS_HOST") {
            Ok(h) => h.parse().map_err(|_| format!("PSS_HOST inválido: {h}"))?,
            Err(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        };
        let port: u16 = match std::env::var("PSS_PORT") {
            Ok(p) => p.parse().map_err(|_| format!("PSS_PORT inválido: {p}"))?,
            Err(_) => 8000,
        };
        let web_dist = std::env::var("PSS_WEB_DIST")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("web/dist"));
        // El tracer se instala junto al servidor.
        let tracer = std::env::var("PSS_TRACER").map(PathBuf::from).unwrap_or_else(|_| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("pss-tracer")))
                .unwrap_or_default()
        });
        let limits = std::env::var("PSS_LIMITS")
            .map(PathBuf::from)
            .ok()
            .or_else(|| Some(PathBuf::from("config/limits.toml")))
            .filter(|p| p.exists());
        Ok(Self {
            addr: SocketAddr::new(host, port),
            web_dist,
            tracer,
            limits,
        })
    }
}

struct AppState {
    tracer: PathBuf,
    limits: Option<PathBuf>,
    /// Cuánto puede tardar una ejecución completa: el límite de tiempo del tracer, más la
    /// compilación y la escritura de la traza.
    timeout: Duration,
    /// Pocas ejecuciones a la vez: cada una ocupa un núcleo mientras hace singlestep.
    slots: Semaphore,
    /// Ejecuciones en curso con progreso: id → PID del tracer, para poder detenerlas.
    runs: Mutex<HashMap<u64, u32>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    source: String,
    #[serde(default)]
    stdin: String,
    #[serde(default)]
    stdin_eof: bool,
    /// Señales de la terminal (Ctrl+C) que llegan después de un paso.
    #[serde(default)]
    injections: Vec<InjectionRequest>,
    /// round_robin, random o manual (como en la traza).
    #[serde(default)]
    policy: Option<String>,
    #[serde(default)]
    seed: u64,
    /// Tareas elegidas a mano; `n` repite la misma tarea varios pasos seguidos.
    #[serde(default)]
    schedule: Vec<TaskRequest>,
}

#[derive(Deserialize)]
struct TaskRequest {
    pid: u32,
    tid: u32,
    #[serde(default = "one")]
    n: u64,
}

fn one() -> u64 {
    1
}

#[derive(Deserialize)]
struct InjectionRequest {
    t: u64,
    signal: String,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "canRun": state.tracer.exists(),
    }))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn failed(status: StatusCode, message: &str) -> Box<Response> {
    Box::new(error(status, message))
}

/// Prepara el directorio de la ejecución y el comando del tracer.
async fn prepare(state: &AppState, req: &RunRequest) -> Result<(PathBuf, tokio::process::Command), Box<Response>> {
    if !state.tracer.exists() {
        return Err(failed(
            StatusCode::SERVICE_UNAVAILABLE,
            "no se encontró pss-tracer junto al servidor",
        ));
    }
    let policy = match req.policy.as_deref() {
        None | Some("round_robin") => None,
        Some(p @ ("random" | "manual")) => Some(p),
        Some(_) => return Err(failed(StatusCode::BAD_REQUEST, "política inválida")),
    };
    for inj in &req.injections {
        let valid = inj.signal.len() <= 12
            && inj.signal.starts_with("SIG")
            && inj.signal[3..]
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        if !valid {
            return Err(failed(StatusCode::BAD_REQUEST, "señal inválida"));
        }
    }
    if req.schedule.iter().map(|t| t.n).sum::<u64>() > 10_000_000 {
        return Err(failed(StatusCode::BAD_REQUEST, "planificación demasiado larga"));
    }
    let dir = std::env::temp_dir().join(format!("pss-run-{}-{}", std::process::id(), next_id()));
    let source_path = dir.join("entrada.c");
    let stdin_path = dir.join("stdin");
    let schedule_path = dir.join("planificacion");
    let schedule: Vec<String> = req
        .schedule
        .iter()
        .map(|t| format!("{}:{}*{}", t.pid, t.tid, t.n))
        .collect();
    // El stdin llega como texto; se envía tal cual en UTF-8.
    let prepared = async {
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(&source_path, &req.source).await?;
        tokio::fs::write(&stdin_path, req.stdin.as_bytes()).await?;
        tokio::fs::write(&schedule_path, schedule.join(",")).await
    }
    .await;
    if let Err(e) = prepared {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(failed(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("no se pudo preparar la ejecución: {e}"),
        ));
    }
    let mut cmd = tokio::process::Command::new(&state.tracer);
    cmd.arg("--source")
        .arg(&source_path)
        .arg("--stdin")
        .arg(&stdin_path)
        .kill_on_drop(true);
    if req.stdin_eof {
        cmd.arg("--stdin-eof");
    }
    if let Some(p) = policy {
        cmd.arg("--policy").arg(p).arg("--seed").arg(req.seed.to_string());
    }
    if !req.schedule.is_empty() {
        cmd.arg("--schedule-file").arg(&schedule_path);
    }
    for inj in req.injections.iter().take(32) {
        cmd.arg("--inject").arg(format!("{}:{}", inj.t, inj.signal));
    }
    if let Some(l) = &state.limits {
        cmd.arg("--limits").arg(l);
    }
    Ok((dir, cmd))
}

async fn run(State(state): State<Arc<AppState>>, Json(req): Json<RunRequest>) -> Response {
    let Ok(_slot) = state.slots.acquire().await else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "el servidor se está cerrando");
    };
    let (dir, mut cmd) = match prepare(&state, &req).await {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let output = tokio::time::timeout(state.timeout, cmd.output()).await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
    match output {
        Ok(Ok(out)) if out.status.success() => {
            ([(header::CONTENT_TYPE, "application/json")], out.stdout).into_response()
        }
        Ok(Ok(out)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&out.stderr).trim(),
        ),
        Ok(Err(e)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("no se pudo ejecutar pss-tracer: {e}"),
        ),
        Err(_) => error(StatusCode::GATEWAY_TIMEOUT, "la ejecución tardó demasiado"),
    }
}

fn line(v: serde_json::Value) -> Bytes {
    let mut b = serde_json::to_vec(&v).expect("JSON");
    b.push(b'\n');
    Bytes::from(b)
}

/// Como `run`, pero responde NDJSON: `started` con el id (para detenerla), `progress` con los
/// pasos recorridos, y al final `trace` (la traza) o `error`.
async fn run_stream(State(state): State<Arc<AppState>>, Json(req): Json<RunRequest>) -> Response {
    let (dir, mut cmd) = match prepare(&state, &req).await {
        Ok(x) => x,
        Err(r) => return *r,
    };
    cmd.arg("--progress")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let id = next_id();
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(16);
    let _ = tx.try_send(line(serde_json::json!({ "type": "started", "id": id.to_string() })));
    let task_state = state.clone();
    tokio::spawn(async move {
        let state = task_state;
        let finished = async {
            let _slot = state
                .slots
                .acquire()
                .await
                .map_err(|_| "el servidor se está cerrando".to_string())?;
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("no se pudo ejecutar pss-tracer: {e}"))?;
            if let Some(pid) = child.id() {
                state.runs.lock().unwrap().insert(id, pid);
            }
            let mut stdout = child.stdout.take().expect("stdout");
            let reading = tokio::spawn(async move {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await.map(|_| buf)
            });
            let mut lines = tokio::io::BufReader::new(child.stderr.take().expect("stderr")).lines();
            let mut messages = Vec::new();
            let watched = tokio::time::timeout(state.timeout, async {
                while let Ok(Some(l)) = lines.next_line().await {
                    match l.strip_prefix("progress ").and_then(|n| n.parse::<u64>().ok()) {
                        Some(n) => {
                            if tx
                                .send(line(serde_json::json!({ "type": "progress", "steps": n })))
                                .await
                                .is_err()
                            {
                                // Quien pidió la ejecución se fue: no tiene sentido seguir.
                                return Err("cancelada".to_string());
                            }
                        }
                        None => messages.push(l),
                    }
                }
                Ok(())
            })
            .await;
            match watched {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err("la ejecución tardó demasiado".to_string()),
            }
            let status = child.wait().await.map_err(|e| e.to_string())?;
            let out = reading.await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(messages.join("\n"));
            }
            Ok(out)
        }
        .await;
        state.runs.lock().unwrap().remove(&id);
        let _ = tokio::fs::remove_dir_all(&dir).await;
        match finished {
            Ok(trace) => {
                // La traza ya es JSON: se envuelve sin volver a leerla.
                let mut b = Vec::with_capacity(trace.len() + 32);
                b.extend_from_slice(br#"{"type":"trace","trace":"#);
                b.extend_from_slice(&trace);
                b.extend_from_slice(b"}\n");
                let _ = tx.send(Bytes::from(b)).await;
            }
            Err(e) => {
                let _ = tx.send(line(serde_json::json!({ "type": "error", "error": e }))).await;
            }
        }
    });
    let body = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|b| (Ok::<_, std::io::Error>(b), rx))
    });
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}

/// Detiene una ejecución en curso: el tracer termina el paso y entrega lo recorrido.
async fn stop(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let pid = id
        .parse::<u64>()
        .ok()
        .and_then(|id| state.runs.lock().unwrap().get(&id).copied());
    match pid {
        Some(pid) => {
            unsafe { libc::kill(pid as libc::pid_t, libc::SIGUSR1) };
            StatusCode::NO_CONTENT.into_response()
        }
        None => error(StatusCode::NOT_FOUND, "esa ejecución ya terminó"),
    }
}

/// `wall_time_ms` del archivo de límites (el tracer lo lee entero; aquí basta este valor).
fn wall_time(limits: Option<&PathBuf>) -> Duration {
    let ms = limits
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| {
            text.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == "wall_time_ms")
                    .then(|| v.split('#').next()?.trim().replace('_', "").parse::<u64>().ok())?
            })
        })
        .unwrap_or(600_000);
    Duration::from_millis(ms) + Duration::from_secs(60)
}

fn next_id() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn app(config: &Config) -> Router {
    let state = Arc::new(AppState {
        tracer: config.tracer.clone(),
        limits: config.limits.clone(),
        timeout: wall_time(config.limits.as_ref()),
        slots: Semaphore::new(2),
        runs: Mutex::new(HashMap::new()),
    });
    let index = config.web_dist.join("index.html");
    Router::new()
        .route("/api/health", get(health))
        .route("/api/run", post(run))
        .route("/api/run/stream", post(run_stream))
        .route("/api/run/{id}/stop", post(stop))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .with_state(state)
        .fallback_service(ServeDir::new(&config.web_dist).fallback(ServeFile::new(index)))
        // El progreso tiene que llegar apenas se escribe: comprimido quedaría retenido.
        .layer(
            CompressionLayer::new()
                .compress_when(DefaultPredicate::new().and(NotForContentType::const_new("application/x-ndjson"))),
        )
}

#[tokio::main]
async fn main() {
    let config = Config::from_env().unwrap_or_else(|e| {
        eprintln!("pss-server: {e}");
        std::process::exit(2);
    });
    if !config.web_dist.join("index.html").exists() {
        eprintln!(
            "pss-server: no encuentro la interfaz compilada en {} (ejecuta `make web`)",
            config.web_dist.display()
        );
        std::process::exit(2);
    }
    if !config.tracer.exists() {
        eprintln!(
            "pss-server: aviso: no encuentro pss-tracer en {}; solo se podrán ver trazas grabadas",
            config.tracer.display()
        );
    }
    let listener = tokio::net::TcpListener::bind(config.addr).await.unwrap_or_else(|e| {
        eprintln!("pss-server: no se pudo abrir {}: {e}", config.addr);
        std::process::exit(1);
    });
    println!("PSS Visualizer escuchando en http://{}", config.addr);
    axum::serve(listener, app(&config))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
