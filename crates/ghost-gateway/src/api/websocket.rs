//! WebSocket upgrade handler for real-time events (Req 25 AC3).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

/// GET /api/ws — WebSocket upgrade.
pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!(msg = %text, "WebSocket message received");
                // Echo for now
                if socket.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
}
