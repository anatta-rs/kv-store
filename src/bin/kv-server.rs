//! HTTP server wrapping any `KvStore` implementation.
//!
//! Exposes the [`KvStore`] trait operations as REST endpoints.
//! Uses the `RocksDB` backend.
//! Designed to run as a standalone service in docker-compose.

use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, head, put};
use axum::{Json, Router};
use serde::Deserialize;

use kv_store::{KvStore, RocksDbStore};

type AppState = Arc<RocksDbStore>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kv_store=debug,tower_http=debug")
        .init();

    let data_path = std::env::var("KV_DATA_PATH").unwrap_or_else(|_| "/data/kv".to_string());
    let port = std::env::var("KV_PORT").unwrap_or_else(|_| "8080".to_string());
    let bind = format!("0.0.0.0:{port}");

    let store = RocksDbStore::open(Path::new(&data_path))?;
    let state: AppState = Arc::new(store);

    let app = Router::new()
        .route("/health", get(health))
        .route("/kv/{key}", get(kv_get))
        .route("/kv/{key}", put(kv_put))
        .route("/kv/{key}", delete(kv_delete))
        .route("/kv/{key}", head(kv_exists))
        .route("/kv", get(kv_scan))
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024)) // 50 MiB
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("kv-server listening on {bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn kv_get(
    State(store): State<AppState>,
    AxumPath(key): AxumPath<String>,
) -> impl IntoResponse {
    match store.get(&key).await {
        Ok(Some(value)) => (StatusCode::OK, value).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn kv_put(
    State(store): State<AppState>,
    AxumPath(key): AxumPath<String>,
    body: Bytes,
) -> impl IntoResponse {
    match store.put(&key, body.to_vec()).await {
        Ok(()) => StatusCode::CREATED,
        Err(e) => {
            tracing::error!("put failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn kv_delete(
    State(store): State<AppState>,
    AxumPath(key): AxumPath<String>,
) -> impl IntoResponse {
    match store.delete(&key).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("delete failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn kv_exists(
    State(store): State<AppState>,
    AxumPath(key): AxumPath<String>,
) -> impl IntoResponse {
    match store.exists(&key).await {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(e) => {
            tracing::error!("exists failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[derive(Deserialize)]
struct ScanQuery {
    prefix: String,
}

async fn kv_scan(
    State(store): State<AppState>,
    Query(params): Query<ScanQuery>,
) -> impl IntoResponse {
    match store.scan_prefix(&params.prefix).await {
        Ok(entries) => {
            let items: Vec<serde_json::Value> = entries
                .into_iter()
                .map(|(k, v)| {
                    serde_json::json!({
                        "key": k,
                        "value": base64_encode(&v),
                    })
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::with_capacity(data.len() * 4 / 3 + 4);
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).unwrap();
    }
    String::from_utf8(buf).unwrap()
}

/// Minimal base64 encoder (avoids adding a dependency).
struct Base64Encoder<'a> {
    out: &'a mut Vec<u8>,
}

impl<'a> Base64Encoder<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self { out }
    }
}

impl std::io::Write for Base64Encoder<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        const CHARS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for chunk in buf.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            self.out.push(CHARS[((b0 >> 2) & 0x3F) as usize]);
            self.out
                .push(CHARS[(((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0F)) as usize]);
            if chunk.len() > 1 {
                self.out
                    .push(CHARS[(((b1 & 0x0F) << 2) | ((b2 >> 6) & 0x03)) as usize]);
            } else {
                self.out.push(b'=');
            }
            if chunk.len() > 2 {
                self.out.push(CHARS[(b2 & 0x3F) as usize]);
            } else {
                self.out.push(b'=');
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
