//! Servidor local: sirve la interfaz y, desde la fase 1, compila y traza programas.

use axum::{Json, Router, routing::get};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

struct Config {
    addr: SocketAddr,
    web_dist: PathBuf,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        // Por defecto solo escucha en localhost: el servidor ejecutará código C y no debe quedar
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
        Ok(Self {
            addr: SocketAddr::new(host, port),
            web_dist,
        })
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

fn app(web_dist: &std::path::Path) -> Router {
    let index = web_dist.join("index.html");
    Router::new()
        .route("/api/health", get(health))
        .fallback_service(ServeDir::new(web_dist).fallback(ServeFile::new(index)))
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
    let listener = tokio::net::TcpListener::bind(config.addr).await.unwrap_or_else(|e| {
        eprintln!("pss-server: no se pudo abrir {}: {e}", config.addr);
        std::process::exit(1);
    });
    println!("PSS Visualizer escuchando en http://{}", config.addr);
    axum::serve(listener, app(&config.web_dist))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
