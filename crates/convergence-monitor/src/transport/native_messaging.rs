//! Chrome/Firefox native messaging transport (Req 9 AC8).
//!
//! 4-byte length prefix (little-endian) stdin/stdout framing.

use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use super::IngestEvent;

/// Native messaging transport for browser extensions.
pub struct NativeMessagingTransport {
    ingest_tx: mpsc::Sender<IngestEvent>,
}

impl NativeMessagingTransport {
    pub fn new(ingest_tx: mpsc::Sender<IngestEvent>) -> Self {
        Self { ingest_tx }
    }

    /// Run the native messaging loop on stdin/stdout.
    pub async fn run(&self) -> std::io::Result<()> {
        let mut stdin = tokio::io::stdin();
        let mut len_buf = [0u8; 4];

        loop {
            // Read 4-byte length prefix (little-endian, per Chrome native messaging spec)
            if stdin.read_exact(&mut len_buf).await.is_err() {
                break; // EOF
            }
            let len = u32::from_le_bytes(len_buf) as usize;

            if len > 1_048_576 {
                tracing::warn!("oversized native message rejected: {len} bytes");
                continue;
            }

            let mut buf = vec![0u8; len];
            if stdin.read_exact(&mut buf).await.is_err() {
                break;
            }

            match serde_json::from_slice::<IngestEvent>(&buf) {
                Ok(event) => {
                    let _ = self.ingest_tx.try_send(event);
                }
                Err(e) => {
                    tracing::warn!("malformed native message: {e}");
                }
            }
        }

        Ok(())
    }
}
