//! Servidor local: sirve la interfaz y ejecuta programas con pss-tracer.

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tower_http::compression::CompressionLayer;
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
    /// Pocas ejecuciones a la vez: cada una ocupa un núcleo mientras hace singlestep.
    slots: Semaphore,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    source: String,
    #[serde(default)]
    stdin: String,
    #[serde(default)]
    stdin_eof: bool,
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

async fn run(State(state): State<Arc<AppState>>, Json(req): Json<RunRequest>) -> Response {
    if !state.tracer.exists() {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "no se encontró pss-tracer junto al servidor",
        );
    }
    let Ok(_slot) = state.slots.acquire().await else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "el servidor se está cerrando");
    };
    let dir = std::env::temp_dir().join(format!("pss-run-{}-{}", std::process::id(), next_id()));
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("no se pudo preparar la ejecución: {e}"),
        );
    }
    let source_path = dir.join("entrada.c");
    let stdin_path = dir.join("stdin");
    // El stdin llega como texto; se envía tal cual en UTF-8.
    let prepared = async {
        tokio::fs::write(&source_path, &req.source).await?;
        tokio::fs::write(&stdin_path, req.stdin.as_bytes()).await
    }
    .await;
    if let Err(e) = prepared {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("no se pudo preparar la ejecución: {e}"),
        );
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
    if let Some(l) = &state.limits {
        cmd.arg("--limits").arg(l);
    }
    // El tracer corta por su cuenta al vencer el límite; esto cubre la compilación y un tracer colgado.
    let output = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await;
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

fn next_id() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn app(config: &Config) -> Router {
    let state = Arc::new(AppState {
        tracer: config.tracer.clone(),
        limits: config.limits.clone(),
        slots: Semaphore::new(2),
    });
    let index = config.web_dist.join("index.html");
    Router::new()
        .route("/api/health", get(health))
        .route("/api/run", post(run))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .with_state(state)
        .fallback_service(ServeDir::new(&config.web_dist).fallback(ServeFile::new(index)))
        .layer(CompressionLayer::new())
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
