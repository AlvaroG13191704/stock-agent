use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::models::Message;
use crate::ollama::{ChatMessage, ChatRequest};
use anyhow::Result;
use async_trait::async_trait;

pub struct FormatterAgent {
    pub base: BaseAgent,
}

impl FormatterAgent {
    pub fn new(base: BaseAgent) -> Self {
        Self { base }
    }
}

#[async_trait]
impl Agent for FormatterAgent {
    fn name(&self) -> &str {
        &self.base.name
    }

    async fn process(
        &self,
        messages: &[Message],
        context: &serde_json::Value,
    ) -> Result<AgentOutput> {
        let mut chat_messages: Vec<ChatMessage> = Vec::new();
        chat_messages.push(ChatMessage {
            role: "system".to_string(),
            content: format!(
                "{}\n\nContext for final formatting:\n{}",
                self.base.system_prompt, context
            ),
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
        self.base
            .record_usage(res.prompt_eval_count, res.eval_count);
        Ok(AgentOutput::Text(res.message.content))
    }
}
