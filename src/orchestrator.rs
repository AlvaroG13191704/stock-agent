use anyhow::Result;
use std::sync::Arc;
use crate::ollama::OllamaClient;
use crate::storage::Storage;
use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::agents::informer::InformerAgent;
use crate::agents::formatter::FormatterAgent;
use crate::agents::router::RouterAgent;
use crate::agents::profile::ProfileAgent;
use crate::agents::investigation::{NewsSearcherAgent, StockDataAgent};
use crate::models::{Message, Role};
use serde_json::json;
use uuid::Uuid;
use chrono::Utc;

pub struct Orchestrator {
    pub client: Arc<OllamaClient>,
    pub storage: Arc<Storage>,
    pub model: String,
    pub trace_log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Orchestrator {
    pub fn new(client: Arc<OllamaClient>, storage: Arc<Storage>, model: String) -> Self {
        Self { 
            client, 
            storage, 
            model,
            trace_log: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn add_trace(&self, msg: &str) {
        if let Ok(mut logs) = self.trace_log.lock() {
            // Horario UTC-6
            let offset = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
            let now = Utc::now().with_timezone(&offset);
            logs.push(format!("[{}] {}", now.format("%H:%M:%S"), msg));
            if logs.len() > 30 {
                logs.remove(0);
            }
        }
    }

    pub async fn handle_query(&self, conversation_id: Uuid, query: String) -> Result<String> {
        self.add_trace(&format!("Nueva consulta: {}", query));
        // 1. Save user msg
        let user_msg = Message {
            id: Uuid::new_v4(),
            conversation_id,
            role: Role::User,
            content: query.clone(),
            created_at: Utc::now(),
            thinking: None,
        };
        self.storage.save_message(user_msg.clone()).await?;

        // 2. Load context and profile
        let original_messages = self.storage.get_messages(conversation_id).await?;
        let mut profile = self.storage.get_profile().await?;

        // 3. Compress Context if > 10 messages
        let active_messages = if original_messages.len() > 10 {
            self.add_trace("Comprimiendo contexto (>10 mensajes)...");
            self.compress_context(&original_messages).await?
        } else {
            original_messages.clone()
        };

        // 4. Handle Profiling / Onboarding
        let profile_agent = ProfileAgent::new(BaseAgent::new(
            "ProfileAnalyzer",
            self.client.clone(),
            &self.model,
            "Analizar si el usuario proporcionó información de perfil."
        ));

        if !profile.is_complete {
            self.add_trace("🧩 Agente Perfil: Analizando requisitos faltantes...");
            // See if this message or context finishes the profile
            profile = profile_agent.update_profile_from_msg(&active_messages, &profile).await?;
            self.storage.save_profile(&profile).await?;
        }

        if !profile.is_complete {
            self.add_trace("🧩 Agente Perfil: Solicitando información al usuario...");
            // Still need más información, deja que el agente pregunte
            match profile_agent.process(&active_messages, &json!({ "is_complete": false, "profile": profile })).await? {
                AgentOutput::Text(t) => return self.save_and_return_assistant_msg(conversation_id, t).await,
                _ => return Err(anyhow::anyhow!("El agente de perfil no pudo producir texto")),
            }
        }

        // 5. Intent Routing
        let router = RouterAgent::new(BaseAgent::new(
            "Router",
            self.client.clone(),
            &self.model,
            "Determinar la intención: educativa o investigación."
        ));

        self.add_trace("🔀 Agente Router: Identificando intención...");
        let router_out = router.process(&active_messages, &json!({})).await?;
        let intent = if let AgentOutput::Structured(data) = router_out {
            data["intent"].as_str().unwrap_or("educational").to_string()
        } else {
            "educational".to_string()
        };
        self.add_trace(&format!("Intención detectada: {}", intent));

        // 6. Specialist Logic
        let final_content = if intent == "investigation" {
            // INVESTIGATION FLOW
            let news_searcher = NewsSearcherAgent::new(BaseAgent::new(
                "NewsSearcher",
                self.client.clone(),
                &self.model,
                "Research market news."
            ));
            let stock_data = StockDataAgent::new(BaseAgent::new(
                "StockData",
                self.client.clone(),
                &self.model,
                "Fetch historical prices."
            ));

            self.add_trace("🔍 Iniciando flujo de INVESTIGACIÓN...");
            
            // Start step-by-step pipeline
            let companies_to_research = if let Some(h) = &profile.holdings {
                h.clone()
            } else {
                vec![query.clone()]
            };
            self.add_trace(&format!("Empresas a investigar: {:?}", companies_to_research));

            self.add_trace("📰 Agente NewsSearcher: Buscando noticias y sentimiento...");
            let news_out = news_searcher.process(&active_messages, &json!({ "target_companies": companies_to_research })).await?;
            let stock_actions = if let AgentOutput::Structured(data) = news_out {
                data
            } else {
                json!([])
            };

            self.add_trace("📊 Agente StockData: Extrayendo precios históricos...");
            let prices_out = stock_data.process(&active_messages, &json!({ "investigative_results": stock_actions })).await?;
            let final_results = if let AgentOutput::Structured(data) = prices_out {
                 data
            } else {
                json!([])
            };

            // Format investigate summary
            self.add_trace("📝 Agente Formatter: Generando reporte ejecutivo (RESUMIDO)...");
            let formatter = FormatterAgent::new(BaseAgent::new(
                "Formatter",
                self.client.clone(),
                &self.model,
                "Crea un resumen ejecutivo MUY CONCISO en Markdown premium. Usa una tabla simple para precios. Un párrafo corto para razonamiento. Sé directo, sin introducciones largas. Responde en ESPAÑOL."
            ));
            
            let context = json!({ 
                "investigative_results": final_results,
                "query": query,
                "profile": profile
            });
            
            match formatter.process(&active_messages, &context).await? {
                AgentOutput::Text(t) => t,
                _ => "Investigación completada pero falló el formateo de los resultados.".to_string()
            }
        } else {
            // EDUCATIONAL FLOW
            self.add_trace("🎓 Iniciando flujo EDUCATIVO...");
            let informer = InformerAgent::new(BaseAgent::new(
                "Informer",
                self.client.clone(),
                &self.model,
                "Proporciona conocimientos educativos y generales sobre inversión. Responde en ESPAÑOL."
            ));
            let formatter = FormatterAgent::new(BaseAgent::new(
                "Formatter",
                self.client.clone(),
                &self.model,
                "Sintetiza la información educativa en ESPAÑOL."
            ));

            self.add_trace("💡 Agente Informer: Generando respuesta...");
            let informer_text = match informer.process(&active_messages, &json!({})).await? {
                AgentOutput::Text(t) => t,
                _ => "".to_string()
            };

            self.add_trace("📝 Agente Formatter: Estilizando respuesta...");
            let context = json!({ "agent_results": informer_text, "profile": profile });
            match formatter.process(&active_messages, &context).await? {
                 AgentOutput::Text(t) => {
                    self.add_trace("✅ Flujo completado.");
                    t
                 },
                _ => "Educational response failed.".to_string()
            }
        };

        self.save_and_return_assistant_msg(conversation_id, final_content).await
    }

    async fn compress_context(&self, messages: &[Message]) -> Result<Vec<Message>> {
        const LIMIT: usize = 10;
        if messages.len() <= LIMIT {
            return Ok(messages.to_vec());
        }

        // Summarize messages 0 to length-3 (keep last 2: current query and previous response)
        let to_summarize = &messages[..messages.len() - 2];
        let last_2 = &messages[messages.len() - 2..];

        let summary_prompt = "Eres un gestor de contexto. Resume la conversación previa con precisión en un único mensaje de sistema en ESPAÑOL. Enfócate en descubrimientos del perfil del usuario, investigación de acciones ya realizada y preguntas clave. Sé conciso pero mantén todo el contexto relevante para la investigación de seguimiento.";
        
        let mut sum_chat_msgs = Vec::new();
        for msg in to_summarize {
            sum_chat_msgs.push(crate::ollama::ChatMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: msg.content.clone(),
                images: None,
            });
        }

        let req = crate::ollama::ChatRequest {
            model: self.model.clone(),
            messages: vec![
                crate::ollama::ChatMessage {
                    role: "system".to_string(),
                    content: summary_prompt.to_string(),
                    images: None,
                },
                crate::ollama::ChatMessage {
                    role: "user".to_string(),
                    content: format!("Por favor resume el siguiente contexto para el próximo turno:\n{:?}", sum_chat_msgs),
                    images: None,
                }
            ],
            stream: false,
            format: None,
            options: None,
        };

        let res = self.client.chat(req).await?;
        
        let mut final_context = vec![Message {
            id: Uuid::new_v4(),
            conversation_id: messages[0].conversation_id,
            role: Role::System,
            content: format!("Resumen de la conversación previa: {}", res.message.content),
            created_at: Utc::now(),
            thinking: None,
        }];
        
        final_context.extend_from_slice(last_2);
        Ok(final_context)
    }

    async fn save_and_return_assistant_msg(&self, conversation_id: Uuid, content: String) -> Result<String> {
        let assistant_msg = Message {
            id: Uuid::new_v4(),
            conversation_id,
            role: Role::Assistant,
            content: content.clone(),
            created_at: Utc::now(),
            thinking: None,
        };
        self.storage.save_message(assistant_msg).await?;
        Ok(content)
    }
}
