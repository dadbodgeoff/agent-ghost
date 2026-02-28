//! A2A client: discover agents, submit/get/cancel tasks via JSON-RPC 2.0.

use uuid::Uuid;

use crate::error::MeshError;
use crate::protocol::methods;
use crate::types::{AgentCard, DelegationRequest, MeshMessage, MeshTask};

/// A2A protocol client for communicating with remote agents.
pub struct A2AClient {
    /// HTTP client timeout in seconds.
    timeout_secs: u64,
}

impl A2AClient {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }

    /// Discover an agent by fetching its AgentCard from `{endpoint}/.well-known/agent.json`.
    pub async fn discover_agent(&self, endpoint: &str) -> Result<AgentCard, MeshError> {
        let url = format!("{}/.well-known/agent.json", endpoint.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()
            .map_err(|e| MeshError::ProtocolError(format!("http client: {e}")))?;

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|_e| MeshError::Timeout {
                duration_secs: self.timeout_secs,
            })?;

        if !resp.status().is_success() {
            return Err(MeshError::AgentNotFound {
                agent_id: Uuid::nil(),
            });
        }

        let card: AgentCard = resp
            .json()
            .await
            .map_err(|e| MeshError::ProtocolError(format!("invalid agent card: {e}")))?;

        // Verify signature before trusting.
        if !card.verify_signature() {
            return Err(MeshError::AuthenticationFailed {
                reason: "agent card signature verification failed".to_string(),
            });
        }

        Ok(card)
    }

    /// Submit a task to a target agent via JSON-RPC `tasks/send`.
    pub async fn submit_task(
        &self,
        endpoint: &str,
        request: &DelegationRequest,
    ) -> Result<MeshTask, MeshError> {
        let msg = MeshMessage::request(
            methods::TASKS_SEND,
            serde_json::to_value(request)
                .map_err(|e| MeshError::ProtocolError(format!("serialize: {e}")))?,
            serde_json::json!(Uuid::new_v4().to_string()),
        );

        let resp = self.send_jsonrpc(endpoint, &msg).await?;
        self.parse_task_response(resp)
    }

    /// Get the current status of a delegated task via JSON-RPC `tasks/get`.
    pub async fn get_task_status(
        &self,
        endpoint: &str,
        task_id: &Uuid,
    ) -> Result<MeshTask, MeshError> {
        let msg = MeshMessage::request(
            methods::TASKS_GET,
            serde_json::json!({"task_id": task_id.to_string()}),
            serde_json::json!(Uuid::new_v4().to_string()),
        );

        let resp = self.send_jsonrpc(endpoint, &msg).await?;
        self.parse_task_response(resp)
    }

    /// Cancel a delegated task via JSON-RPC `tasks/cancel`.
    pub async fn cancel_task(
        &self,
        endpoint: &str,
        task_id: &Uuid,
    ) -> Result<(), MeshError> {
        let msg = MeshMessage::request(
            methods::TASKS_CANCEL,
            serde_json::json!({"task_id": task_id.to_string()}),
            serde_json::json!(Uuid::new_v4().to_string()),
        );

        let resp = self.send_jsonrpc(endpoint, &msg).await?;
        if let Some(err) = resp.error {
            return Err(MeshError::ProtocolError(err.message));
        }
        Ok(())
    }

    /// Subscribe to task updates via JSON-RPC `tasks/sendSubscribe`.
    ///
    /// Returns a stream of `MeshTask` updates via SSE. The caller receives
    /// each update as it arrives. The stream ends when the task reaches a
    /// terminal state (Completed, Failed, Canceled).
    ///
    /// In v1, this sends the subscribe request and returns the initial task.
    /// Full SSE streaming will be wired through the gateway's axum SSE support.
    pub async fn subscribe_task(
        &self,
        endpoint: &str,
        request: &DelegationRequest,
    ) -> Result<MeshTask, MeshError> {
        let msg = MeshMessage::request(
            methods::TASKS_SEND_SUBSCRIBE,
            serde_json::to_value(request)
                .map_err(|e| MeshError::ProtocolError(format!("serialize: {e}")))?,
            serde_json::json!(Uuid::new_v4().to_string()),
        );

        let resp = self.send_jsonrpc(endpoint, &msg).await?;
        self.parse_task_response(resp)
    }

    /// Send a JSON-RPC 2.0 message to the A2A endpoint.
    async fn send_jsonrpc(
        &self,
        endpoint: &str,
        msg: &MeshMessage,
    ) -> Result<MeshMessage, MeshError> {
        let url = format!("{}/a2a", endpoint.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()
            .map_err(|e| MeshError::ProtocolError(format!("http client: {e}")))?;

        let resp = client
            .post(&url)
            .json(msg)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    MeshError::Timeout {
                        duration_secs: self.timeout_secs,
                    }
                } else {
                    MeshError::ProtocolError(format!("request failed: {e}"))
                }
            })?;

        resp.json::<MeshMessage>()
            .await
            .map_err(|e| MeshError::ProtocolError(format!("invalid response: {e}")))
    }

    /// Parse a JSON-RPC response into a MeshTask.
    fn parse_task_response(&self, resp: MeshMessage) -> Result<MeshTask, MeshError> {
        if let Some(err) = resp.error {
            return Err(MeshError::ProtocolError(err.message));
        }
        let result = resp
            .result
            .ok_or_else(|| MeshError::ProtocolError("missing result in response".to_string()))?;
        serde_json::from_value(result)
            .map_err(|e| MeshError::ProtocolError(format!("invalid task in response: {e}")))
    }
}

impl Default for A2AClient {
    fn default() -> Self {
        Self::new(10)
    }
}
