use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::events::{RunEvent, TokenUsage};
use crate::models::Message;
use crate::ollama::OllamaClient;

pub fn parse_llm_json<T: serde::de::DeserializeOwned>(content: &str) -> Result<T> {
    let trimmed = content.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    for block in trimmed.split("```").skip(1).step_by(2) {
        let json_block = block
            .strip_prefix("json")
            .or_else(|| block.strip_prefix("JSON"))
            .unwrap_or(block)
            .trim();
        if let Ok(value) = serde_json::from_str(json_block) {
            return Ok(value);
        }
    }

    for (index, character) in content.char_indices() {
        if !matches!(character, '{' | '[') {
            continue;
        }

        let mut stream = serde_json::Deserializer::from_str(&content[index..]).into_iter::<T>();
        if let Some(Ok(value)) = stream.next() {
            return Ok(value);
        }
    }

    Err(anyhow!(
        "No valid JSON value found in LLM response: {}",
        truncate_response(content)
    ))
}

fn truncate_response(content: &str) -> String {
    content.chars().take(2_000).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn parse_clean_json() {
        let input = r#"{"name": "test", "value": 42}"#;
        let parsed: TestData = parse_llm_json(input).expect("clean JSON should parse");
        assert_eq!(
            parsed,
            TestData {
                name: "test".to_string(),
                value: 42
            }
        );
    }

    #[test]
    fn parse_json_with_markdown_block() {
        let input = "Aquí está el JSON:\n```json\n{\"name\": \"test\", \"value\": 42}\n```\nEspero que sirva.";
        let parsed: TestData = parse_llm_json(input).expect("JSON code block should parse");
        assert_eq!(
            parsed,
            TestData {
                name: "test".to_string(),
                value: 42
            }
        );
    }

    #[test]
    fn parse_json_array() {
        let input = "Resultados: [\"A\", \"B\", \"C\"]";
        let parsed: Vec<String> = parse_llm_json(input).expect("JSON array should parse");
        assert_eq!(
            parsed,
            vec!["A".to_string(), "B".to_string(), "C".to_string()]
        );
    }

    #[test]
    fn parse_json_with_braces_inside_string() {
        let input = r#"Respuesta: {"name":"{quoted}","value":42}"#;
        let parsed: TestData = parse_llm_json(input).expect("braces inside strings should parse");
        assert_eq!(parsed.name, "{quoted}");
    }
}

pub mod formatter;
pub mod informer;
pub mod investigation;
pub mod profile;
pub mod router;

#[async_trait]
pub trait Agent: Send + Sync {
    #[expect(
        dead_code,
        reason = "Agent names are part of the public orchestration contract"
    )]
    fn name(&self) -> &str;
    async fn process(
        &self,
        messages: &[Message],
        context: &serde_json::Value,
    ) -> Result<AgentOutput>;
}

pub enum AgentOutput {
    Text(String),
    Structured(serde_json::Value),
}

pub struct BaseAgent {
    pub name: String,
    pub client: Arc<OllamaClient>,
    pub model: String,
    pub system_prompt: String,
    run_id: Option<Uuid>,
    event_tx: Option<UnboundedSender<RunEvent>>,
}

impl BaseAgent {
    pub fn new(name: &str, client: Arc<OllamaClient>, model: &str, system_prompt: &str) -> Self {
        Self {
            name: name.to_string(),
            client,
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            run_id: None,
            event_tx: None,
        }
    }

    pub fn with_events(mut self, run_id: Uuid, event_tx: UnboundedSender<RunEvent>) -> Self {
        self.run_id = Some(run_id);
        self.event_tx = Some(event_tx);
        self
    }

    pub fn add_trace(&self, message: &str) {
        let (Some(run_id), Some(event_tx)) = (self.run_id, &self.event_tx) else {
            return;
        };

        let timestamp = chrono::Utc::now().format("%H:%M:%S");
        let _ = event_tx.send(RunEvent::Trace {
            run_id,
            message: format!("[{timestamp}] {}: {message}", self.name),
        });
    }

    pub fn record_usage(&self, prompt_tokens: Option<u64>, completion_tokens: Option<u64>) {
        let (Some(run_id), Some(event_tx)) = (self.run_id, &self.event_tx) else {
            return;
        };

        let usage = TokenUsage {
            prompt_tokens: prompt_tokens.unwrap_or_default(),
            completion_tokens: completion_tokens.unwrap_or_default(),
        };
        if usage.total() > 0 {
            let _ = event_tx.send(RunEvent::Usage { run_id, usage });
        }
    }
}
