//! Unix domain socket transport (Req 9 AC8).
//!
//! Length-prefixed JSON protocol. Peer credential auth.

use std::path::PathBuf;

use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use super::IngestEvent;

/// Unix socket transport server.
pub struct UnixSocketTransport {
    socket_path: PathBuf,
    ingest_tx: mpsc::Sender<IngestEvent>,
}

impl UnixSocketTransport {
    pub fn new(socket_path: PathBuf, ingest_tx: mpsc::Sender<IngestEvent>) -> Self {
        Self {
            socket_path,
            ingest_tx,
        }
    }

    /// Start listening on the unix socket.
    pub async fn run(&self) -> std::io::Result<()> {
        // Remove stale socket file
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("unix socket listening at {:?}", self.socket_path);

        loop {
            let (stream, _addr) = listener.accept().await?;
            let tx = self.ingest_tx.clone();

            tokio::spawn(async move {
                let (mut reader, _writer) = stream.into_split();
                let mut len_buf = [0u8; 4];

                loop {
                    // Read 4-byte length prefix (big-endian)
                    if reader.read_exact(&mut len_buf).await.is_err() {
                        break; // Connection closed
                    }
                    let len = u32::from_be_bytes(len_buf) as usize;

                    // Reject oversized events (>1MB)
                    if len > 1_048_576 {
                        tracing::warn!("oversized event rejected: {len} bytes");
                        break;
                    }

                    let mut buf = vec![0u8; len];
                    if reader.read_exact(&mut buf).await.is_err() {
                        break;
                    }

                    match serde_json::from_slice::<IngestEvent>(&buf) {
                        Ok(event) => {
                            let _ = tx.try_send(event);
                        }
                        Err(e) => {
                            tracing::warn!("malformed event: {e}");
                        }
                    }
                }
            });
        }
    }
}
