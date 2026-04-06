use anyhow::{Result, Context};
use async_trait::async_trait;
use crate::ollama::OllamaClient;
use crate::models::Message;
use std::sync::Arc;

pub fn parse_llm_json<T: serde::de::DeserializeOwned>(content: &str) -> Result<T> {
    let cleaned = content.trim();
    let cleaned = if cleaned.starts_with("```json") {
         cleaned.strip_prefix("```json").unwrap_or(cleaned)
            .strip_suffix("```").unwrap_or(cleaned)
            .trim()
    } else if cleaned.starts_with("```") {
        cleaned.strip_prefix("```").unwrap_or(cleaned)
            .strip_suffix("```").unwrap_or(cleaned)
            .trim()
    } else {
        cleaned
    };

    serde_json::from_str(cleaned)
        .with_context(|| format!("Failed to parse JSON from LLM.\nCLEANED: {}\nORIGINAL: {}", cleaned, content))
}

pub mod informer;
pub mod formatter;
pub mod router;
pub mod profile;
pub mod investigation;

#[async_trait]
pub trait Agent: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    async fn process(&self, messages: &[Message], context: &serde_json::Value) -> Result<AgentOutput>;
}

pub enum AgentOutput {
    Text(String),
    Structured(serde_json::Value),
}

pub struct BaseAgent {
    #[allow(dead_code)]
    pub name: String,
    pub client: Arc<OllamaClient>,
    pub model: String,
    pub system_prompt: String,
}

impl BaseAgent {
    pub fn new(name: &str, client: Arc<OllamaClient>, model: &str, system_prompt: &str) -> Self {
        Self {
            name: name.to_string(),
            client,
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
        }
    }
}
