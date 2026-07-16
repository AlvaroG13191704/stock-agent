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
            2. Intenciones posibles: ['educational', 'investigation', 'out_of_scope'].\n\
            3. Si la consulta no trata sobre educación financiera, mercados, acciones, ETFs, bonos, cartera, riesgo, ahorro, inversión o datos económicos, usa `out_of_scope`. Ejemplos: recetas, pizza, deportes, viajes, entretenimiento, programación general y tareas domésticas.\n\
            4. Identifica si el usuario menciona empresas o tickers específicos.\n\
            5. Si el usuario NO menciona empresas pero pide recomendaciones o búsquedas sobre un tema financiero (ej. 'acciones de IA', 'ETFs de litio'), marca `requires_discovery: true` y pon el tema en `discovery_topic`.\n\
            6. Para símbolos fuera de EE. UU., incluye siempre el sufijo de mercado compatible (por ejemplo `.T` para Tokio, `.L` para Londres, `.HK` para Hong Kong o `.TO` para Toronto). Nunca devuelvas un ticker numérico internacional sin sufijo.\n\
            Devuelve únicamente JSON válido, sin Markdown ni texto adicional. No uses null: usa [] para companies, false para requires_discovery y \"\" para discovery_topic. {{ \
                \"intent\": \"educational\" | \"investigation\" | \"out_of_scope\", \
                \"companies\": [\"AAPL\", \"MSFT\"], \
                \"requires_discovery\": true | false, \
                \"discovery_topic\": \"...\" \
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
                    "intent": { "type": "string", "enum": ["educational", "investigation", "out_of_scope"] },
                    "companies": { "type": "array", "items": { "type": "string" } },
                    "requires_discovery": { "type": "boolean" },
                    "discovery_topic": { "type": "string" }
                },
                "required": ["intent", "companies", "requires_discovery", "discovery_topic"]
            })),
            options: None,
        };

        let res = self.base.client.chat(req).await?;
        self.base
            .record_usage(res.prompt_eval_count, res.eval_count);
        let data: serde_json::Value = crate::agents::parse_llm_json(&res.message.content)?;
        Ok(AgentOutput::Structured(data))
    }
}
