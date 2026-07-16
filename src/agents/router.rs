use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::models::Message;
use anyhow::Result;
use async_trait::async_trait;
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

    async fn process(
        &self,
        messages: &[Message],
        _context: &serde_json::Value,
    ) -> Result<AgentOutput> {
        let system_prompt = format!(
            "{}\n\nTareas:\n\
            1. Clasifica la consulta del usuario.\n\
            2. Intenciones posibles: ['educational', 'investigation'].\n\
            3. Identifica si el usuario menciona empresas o tickers específicos.\n\
            4. Si el usuario NO menciona empresas pero pide recomendaciones o búsquedas sobre un tema (ej. 'acciones de IA', 'ETFs de litio'), marca `requires_discovery: true` y pon el tema en `discovery_topic`.\n\n\
            Devuelve JSON: {{ \n\
                \"intent\": \"educational\" | \"investigation\", \n\
                \"companies\": [\"AAPL\", \"MSFT\"], \n\
                \"requires_discovery\": true | false, \n\
                \"discovery_topic\": \"...\" \n\
            }}",
            self.base.system_prompt
        );

        let mut chat_messages: Vec<crate::ollama::ChatMessage> = vec![crate::ollama::ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
            images: None,
        }];

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
                    "intent": { "type": "string", "enum": ["educational", "investigation"] },
                    "companies": { "type": "array", "items": { "type": "string" } },
                    "requires_discovery": { "type": "boolean" },
                    "discovery_topic": { "type": "string" }
                },
                "required": ["intent", "companies", "requires_discovery", "discovery_topic"]
            })),
            options: None,
        };

        let res = self.base.client.chat(req).await?;
        let data: serde_json::Value = crate::agents::parse_llm_json(&res.message.content)?;
        Ok(AgentOutput::Structured(data))
    }
}
