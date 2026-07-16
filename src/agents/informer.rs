use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::models::Message;
use crate::ollama::{ChatMessage, ChatRequest};
use anyhow::Result;
use async_trait::async_trait;

pub struct InformerAgent {
    pub base: BaseAgent,
}

impl InformerAgent {
    pub fn new(base: BaseAgent) -> Self {
        Self { base }
    }
}

#[async_trait]
impl Agent for InformerAgent {
    fn name(&self) -> &str {
        &self.base.name
    }

    async fn process(
        &self,
        messages: &[Message],
        _context: &serde_json::Value,
    ) -> Result<AgentOutput> {
        let mut chat_messages: Vec<ChatMessage> = Vec::new();
        chat_messages.push(ChatMessage {
            role: "system".to_string(),
            content: self.base.system_prompt.clone(),
            images: None,
        });

        for msg in messages {
            chat_messages.push(ChatMessage {
                role: serde_json::to_string(&msg.role)?
                    .trim_matches('"')
                    .to_string(),
                content: msg.content.clone(),
                images: None,
            });
        }

        let req = ChatRequest {
            model: self.base.model.clone(),
            messages: chat_messages,
            stream: false,
            format: None,
            options: None,
        };

        let res = self.base.client.chat(req).await?;
        Ok(AgentOutput::Text(res.message.content))
    }
}
