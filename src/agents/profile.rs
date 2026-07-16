use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::models::{Message, UserProfile};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

pub struct ProfileAgent {
    pub base: BaseAgent,
}

impl ProfileAgent {
    pub fn new(base: BaseAgent) -> Self {
        Self { base }
    }

    /// Determines if a profile should be updated based on a message
    pub async fn update_profile_from_msg(
        &self,
        messages: &[Message],
        current_profile: &UserProfile,
    ) -> Result<UserProfile> {
        let system_prompt = format!(
            "Eres un analista de perfiles. Analiza el diálogo y extrae información del perfil de inversión del usuario.\n\n\
            Perfil Actual: {:?}\n\n\
            Actualiza los campos si el usuario proporciona nueva información:\n\
            1. Experiencia (beginner, intermediate, advanced)\n\
            2. Nivel de conocimiento (low, med, high)\n\
            3. Plataformas (ej: Robinhood, Fidelity)\n\
            4. Holdings (Lista de símbolos: AAPL, TSLA, etc)\n\
            Determina si toda la información está presente para marcar is_complete = true (se requiere experiencia, conocimiento, plataformas y holdings).\n\n\
            Devuelve JSON: {{ \"experience\": \"...\", \"knowledge\": \"...\", \"platforms\": \"...\", \"holdings\": [\"...\"], \"is_complete\": true/false }}",
            current_profile
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
                    "experience": { "type": "string" },
                    "knowledge": { "type": "string" },
                    "platforms": { "type": "string" },
                    "holdings": { "type": "array", "items": { "type": "string" } },
                    "is_complete": { "type": "boolean" }
                }
            })),
            options: None,
        };

        let res = self.base.client.chat(req).await?;
        let profile: UserProfile = crate::agents::parse_llm_json(&res.message.content)?;
        Ok(profile)
    }
}

#[async_trait]
impl Agent for ProfileAgent {
    fn name(&self) -> &str {
        &self.base.name
    }

    async fn process(
        &self,
        _messages: &[Message],
        context: &serde_json::Value,
    ) -> Result<AgentOutput> {
        // Si estamos aquí, es porque el orquestador necesita información de perfil faltante
        let is_complete = context["is_complete"].as_bool().unwrap_or(false);
        if is_complete {
            return Ok(AgentOutput::Text(
                "Perfil listo. ¿En qué podemos ayudarte hoy?".to_string(),
            ));
        }

        let profile = &context["profile"];

        let system_prompt = format!(
            "Eres un asistente de incorporación de inversiones amigable. Tu objetivo es completar el perfil del usuario en ESPAÑOL.\n\n\
            Perfil actual del usuario: {}\n\n\
            REGLAS:\n\
            1. Mira qué campos faltan o están vacíos (knowledge, experience, platforms, holdings).\n\
            2. Pregunta ÚNICAMENTE por la información que falta de forma amable y natural.\n\
            3. No repitas preguntas sobre lo que ya sabemos (ej: si ya sabemos que usa Interactive Brokers, no preguntes por plataformas).\n\
            4. Si falta el nivel de conocimiento o los activos actuales, pregunta por ellos.\n\
            5. Mantén un tono profesional pero acogedor.",
            profile
        );

        let req = crate::ollama::ChatRequest {
            model: self.base.model.clone(),
            messages: vec![crate::ollama::ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
                images: None,
            }],
            stream: false,
            format: None,
            options: None,
        };

        let res = self.base.client.chat(req).await?;
        Ok(AgentOutput::Text(res.message.content))
    }
}
