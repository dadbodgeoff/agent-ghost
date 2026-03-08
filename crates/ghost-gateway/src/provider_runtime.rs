use std::sync::Arc;

use ghost_agent_loop::runner::LLMFallbackChain;
use ghost_llm::fallback::AuthProfile;
use ghost_llm::provider::{
    AnthropicProvider, ChatMessage, GeminiProvider, OllamaProvider, OpenAICompatProvider,
    OpenAIProvider, ToolSchema,
};
use ghost_llm::streaming::StreamChunkStream;

use crate::config::ProviderConfig;
use crate::state::AppState;

pub fn ordered_provider_configs(state: &AppState) -> Vec<ProviderConfig> {
    let mut providers = state.model_providers.clone();
    if let Some(default_name) = state.default_model_provider.as_deref() {
        if let Some(index) = providers
            .iter()
            .position(|provider| provider.name == default_name)
        {
            let provider = providers.remove(index);
            providers.insert(0, provider);
        }
    }
    providers
}

pub fn build_fallback_chain(providers: &[ProviderConfig]) -> LLMFallbackChain {
    let mut chain = LLMFallbackChain::new();

    for provider in providers {
        match provider.name.as_str() {
            "ollama" => {
                chain.add_provider(
                    Arc::new(OllamaProvider {
                        model: provider.model.clone().unwrap_or_else(|| "llama3.1".into()),
                        base_url: provider
                            .base_url
                            .clone()
                            .unwrap_or_else(|| "http://localhost:11434".into()),
                    }),
                    vec![],
                );
            }
            "anthropic" => {
                if let Some(key) = provider_api_key(provider, "ANTHROPIC_API_KEY") {
                    chain.add_provider(
                        Arc::new(AnthropicProvider {
                            model: provider
                                .model
                                .clone()
                                .unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
                            api_key: std::sync::RwLock::new(key.clone()),
                        }),
                        vec![AuthProfile {
                            api_key: key,
                            org_id: None,
                        }],
                    );
                }
            }
            "openai" => {
                if let Some(key) = provider_api_key(provider, "OPENAI_API_KEY") {
                    chain.add_provider(
                        Arc::new(OpenAIProvider {
                            model: provider.model.clone().unwrap_or_else(|| "gpt-4o".into()),
                            api_key: std::sync::RwLock::new(key.clone()),
                        }),
                        vec![AuthProfile {
                            api_key: key,
                            org_id: None,
                        }],
                    );
                }
            }
            "gemini" => {
                if let Some(key) = provider_api_key(provider, "GEMINI_API_KEY") {
                    chain.add_provider(
                        Arc::new(GeminiProvider {
                            model: provider
                                .model
                                .clone()
                                .unwrap_or_else(|| "gemini-2.0-flash".into()),
                            api_key: std::sync::RwLock::new(key.clone()),
                        }),
                        vec![AuthProfile {
                            api_key: key,
                            org_id: None,
                        }],
                    );
                }
            }
            "openai_compat" => {
                if let Some(key) = provider_api_key(provider, "OPENAI_API_KEY") {
                    chain.add_provider(
                        Arc::new(OpenAICompatProvider {
                            model: provider.model.clone().unwrap_or_else(|| "default".into()),
                            api_key: std::sync::RwLock::new(key.clone()),
                            base_url: provider
                                .base_url
                                .clone()
                                .unwrap_or_else(|| "http://localhost:8080".into()),
                            context_window_size: 128_000,
                        }),
                        vec![AuthProfile {
                            api_key: key,
                            org_id: None,
                        }],
                    );
                }
            }
            _ => {}
        }
    }

    chain
}

pub fn build_provider_stream(
    provider: &ProviderConfig,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolSchema>,
) -> StreamChunkStream {
    match provider.name.as_str() {
        "ollama" => OllamaProvider {
            model: provider.model.clone().unwrap_or_else(|| "llama3.1".into()),
            base_url: provider
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".into()),
        }
        .stream_chat(&messages, &tools),
        "anthropic" => ghost_llm::provider::complete_stream_shim(
            Arc::new(AnthropicProvider {
                model: provider
                    .model
                    .clone()
                    .unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
                api_key: std::sync::RwLock::new(
                    provider_api_key(provider, "ANTHROPIC_API_KEY").unwrap_or_default(),
                ),
            }),
            messages,
            tools,
        ),
        "openai" => ghost_llm::provider::complete_stream_shim(
            Arc::new(OpenAIProvider {
                model: provider.model.clone().unwrap_or_else(|| "gpt-4o".into()),
                api_key: std::sync::RwLock::new(
                    provider_api_key(provider, "OPENAI_API_KEY").unwrap_or_default(),
                ),
            }),
            messages,
            tools,
        ),
        "gemini" => ghost_llm::provider::complete_stream_shim(
            Arc::new(GeminiProvider {
                model: provider
                    .model
                    .clone()
                    .unwrap_or_else(|| "gemini-2.0-flash".into()),
                api_key: std::sync::RwLock::new(
                    provider_api_key(provider, "GEMINI_API_KEY").unwrap_or_default(),
                ),
            }),
            messages,
            tools,
        ),
        "openai_compat" => OpenAICompatProvider {
            model: provider.model.clone().unwrap_or_else(|| "default".into()),
            api_key: std::sync::RwLock::new(
                provider_api_key(provider, "OPENAI_API_KEY").unwrap_or_default(),
            ),
            base_url: provider
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:8080".into()),
            context_window_size: 128_000,
        }
        .stream_chat(&messages, &tools),
        _ => OllamaProvider {
            model: provider.model.clone().unwrap_or_else(|| "llama3.1".into()),
            base_url: provider
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".into()),
        }
        .stream_chat(&messages, &tools),
    }
}

fn provider_api_key(provider: &ProviderConfig, default_env: &str) -> Option<String> {
    crate::state::get_api_key(provider.api_key_env.as_deref().unwrap_or(default_env))
        .filter(|key| !key.is_empty())
}
