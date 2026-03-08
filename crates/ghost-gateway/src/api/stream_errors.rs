use ghost_agent_loop::circuit_breaker::{classify_llm_error, FailureType};
use ghost_agent_loop::runner::{AgentStreamErrorType, AgentStreamEvent, RunError};

#[derive(Debug, Clone)]
pub struct StreamFailure {
    pub message: String,
    pub partial_output: bool,
    pub failure_type: FailureType,
    pub error_type: Option<AgentStreamErrorType>,
}

impl StreamFailure {
    pub fn from_run_error(error: &RunError) -> Self {
        let (message, partial_output) = match error {
            RunError::LLMError {
                message,
                partial_output,
            } => (message.clone(), *partial_output),
            other => (other.to_string(), false),
        };

        let failure_type = classify_llm_error(&message);
        let error_type = match failure_type {
            FailureType::Transient | FailureType::RateLimit => {
                Some(AgentStreamErrorType::ProviderUnavailable)
            }
            FailureType::AuthFailure => Some(AgentStreamErrorType::AuthFailed),
            FailureType::ModelRefusal | FailureType::Fatal => {
                Some(AgentStreamErrorType::RuntimeError)
            }
        };

        Self {
            message,
            partial_output,
            failure_type,
            error_type,
        }
    }

    pub fn can_fallback(&self, has_fallback: bool) -> bool {
        has_fallback
            && !self.partial_output
            && matches!(
                self.failure_type,
                FailureType::Transient | FailureType::RateLimit | FailureType::AuthFailure
            )
    }

    pub fn as_stream_error(
        &self,
        provider: Option<String>,
        fallback: bool,
        terminal: bool,
    ) -> AgentStreamEvent {
        match self.error_type {
            Some(error_type) => AgentStreamEvent::structured_error(
                self.message.clone(),
                error_type,
                provider,
                fallback,
                terminal,
            ),
            None => AgentStreamEvent::terminal_error(self.message.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_provider_failure_allows_fallback_before_output() {
        let failure = StreamFailure::from_run_error(&RunError::streaming_llm_error(
            "503 Service Unavailable",
            false,
        ));

        assert_eq!(
            failure.error_type,
            Some(AgentStreamErrorType::ProviderUnavailable)
        );
        assert!(failure.can_fallback(true));
    }

    #[test]
    fn auth_failure_allows_fallback_before_output() {
        let failure = StreamFailure::from_run_error(&RunError::streaming_llm_error(
            "401 Unauthorized",
            false,
        ));

        assert_eq!(failure.error_type, Some(AgentStreamErrorType::AuthFailed));
        assert!(failure.can_fallback(true));
    }

    #[test]
    fn partial_output_blocks_fallback() {
        let failure = StreamFailure::from_run_error(&RunError::streaming_llm_error(
            "timeout after 300s",
            true,
        ));

        assert_eq!(
            failure.error_type,
            Some(AgentStreamErrorType::ProviderUnavailable)
        );
        assert!(!failure.can_fallback(true));
    }

    #[test]
    fn model_refusal_stays_terminal() {
        let failure = StreamFailure::from_run_error(&RunError::streaming_llm_error(
            "context length exceeded",
            false,
        ));

        assert_eq!(failure.error_type, Some(AgentStreamErrorType::RuntimeError));
        assert!(!failure.can_fallback(true));
    }
}
