use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, Method};
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;

use crate::mcp::server::McpServerState;

#[derive(Clone)]
struct SseState {
    mcp: Arc<McpServerState>,
    tx: broadcast::Sender<String>,
}

/// Run the MCP server over SSE/HTTP.
/// POST /mcp - receives JSON-RPC requests
/// GET /sse - streams JSON-RPC responses via SSE
pub async fn run_sse(
    state: Arc<McpServerState>,
    host: &str,
    port: u16,
) -> crate::error::Result<()> {
    let (tx, _) = broadcast::channel::<String>(100);

    let sse_state = SseState { mcp: state, tx };

    // Restrictive CORS: no cross-origin requests allowed by default.
    // Prevents browser-based CSRF attacks against the local SSE server.
    let cors = CorsLayer::new()
        .allow_origin(HeaderValue::from_static("null")) // deny all real origins
        .allow_methods([Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/sse", get(sse_handler))
        .route("/mcp", post(mcp_handler))
        .layer(cors)
        .with_state(sse_state);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    eprintln!("MCP SSE server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn sse_handler(
    State(state): State<SseState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(data) => Some(Ok(Event::default().data(data))),
        Err(_) => None,
    });

    Sse::new(stream)
}

async fn mcp_handler(
    State(state): State<SseState>,
    Json(request): Json<serde_json::Value>,
) -> impl IntoResponse {
    let response = state.mcp.handle_request_async(&request).await;
    Json(response)
}
