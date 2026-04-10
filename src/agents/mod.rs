use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use crate::ollama::OllamaClient;
use crate::models::Message;
use std::sync::Arc;

pub fn parse_llm_json<T: serde::de::DeserializeOwned>(content: &str) -> Result<T> {
    let start = content.find('{');
    let end = content.rfind('}');

    match (start, end) {
        (Some(s), Some(e)) if e > s => {
            let json_str = &content[s..=e];
            serde_json::from_str(json_str)
                .with_context(|| format!("Failed to parse extracted JSON from LLM.\nEXTRACTED: {}", json_str))
        }
        _ => {
             // Fallback to original logic if no braces found (maybe it's a list [])
             let start_arr = content.find('[');
             let end_arr = content.rfind(']');
             match (start_arr, end_arr) {
                 (Some(s), Some(e)) if e > s => {
                     let json_str = &content[s..=e];
                     serde_json::from_str(json_str)
                        .with_context(|| format!("Failed to parse extracted JSON Array from LLM.\nEXTRACTED: {}", json_str))
                 }
                 _ => Err(anyhow!("No JSON braces or brackets found in LLM response: {}", content))
             }
        }
    }
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
    fn test_parse_clean_json() {
        let input = r#"{"name": "test", "value": 42}"#;
        let parsed: TestData = parse_llm_json(input).unwrap();
        assert_eq!(parsed, TestData { name: "test".to_string(), value: 42 });
    }

    #[test]
    fn test_parse_json_with_markdown_block() {
        let input = "Aquí está el JSON:\n```json\n{\"name\": \"test\", \"value\": 42}\n```\nEspero que sirva.";
        let parsed: TestData = parse_llm_json(input).expect("Should find JSON in block");
        assert_eq!(parsed, TestData { name: "test".to_string(), value: 42 });
    }

    #[test]
    fn test_parse_json_array() {
        let input = "Resultados: [\"A\", \"B\", \"C\"]";
        let parsed: Vec<String> = parse_llm_json(input).expect("Should find Array in text");
        assert_eq!(parsed, vec!["A".to_string(), "B".to_string(), "C".to_string()]);
    }
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
    pub trace_log: Option<Arc<std::sync::Mutex<Vec<String>>>>,
}

impl BaseAgent {
    pub fn new(name: &str, client: Arc<OllamaClient>, model: &str, system_prompt: &str) -> Self {
        Self {
            name: name.to_string(),
            client,
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            trace_log: None,
        }
    }

    pub fn with_trace(mut self, trace: Arc<std::sync::Mutex<Vec<String>>>) -> Self {
        self.trace_log = Some(trace);
        self
    }

    pub fn add_trace(&self, msg: &str) {
        if let Some(trace) = &self.trace_log {
            if let Ok(mut logs) = trace.lock() {
                let offset = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
                let now = chrono::Utc::now().with_timezone(&offset);
                logs.push(format!("[{}] {}", now.format("%H:%M:%S"), msg));
            }
        }
    }
}

