use anyhow::Result;
use async_trait::async_trait;
use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::models::{Message};
use serde_json::json;

pub struct RouterAgent {
    pub base: BaseAgent,
}

impl RouterAgent {
    pub fn new(base: BaseAgent) -> Self {
        Self { base }
    }
}

#[async_trait]
impl Agent for RouterAgent {
    fn name(&self) -> &str {
        &self.base.name
    }

    async fn process(&self, messages: &[Message], _context: &serde_json::Value) -> Result<AgentOutput> {
        let system_prompt = format!(
            "{}\n\nTareas:\n1. Clasifica la consulta del usuario.\n2. Intenciones posibles: ['educational', 'investigation'].\n3. Educational: Preguntas generales de mercado, terminología, 'cómo-hacer-algo'.\n4. Investigation: Empresas específicas, precios, noticias, análisis de acciones.\nDevuelve JSON: {{ \"intent\": \"educational\" | \"investigation\" }}",
            self.base.system_prompt
        );

        let mut chat_messages: Vec<crate::ollama::ChatMessage> = vec![
            crate::ollama::ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
                images: None,
            }
        ];

        for msg in messages.iter().rev().take(5).rev() {
            chat_messages.push(crate::ollama::ChatMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: msg.content.clone(),
                images: None,
            });
        }

        let req = crate::ollama::ChatRequest {
            model: self.base.model.clone(),
            messages: chat_messages,
            stream: false,
            format: Some(json!({
                "type": "object",
                "properties": {
                    "intent": { "type": "string", "enum": ["educational", "investigation"] }
                },
                "required": ["intent"]
            })),
            options: None,
        };

        let res = self.base.client.chat(req).await?;
        let data: serde_json::Value = crate::agents::parse_llm_json(&res.message.content)?;
        Ok(AgentOutput::Structured(data))
    }
}
