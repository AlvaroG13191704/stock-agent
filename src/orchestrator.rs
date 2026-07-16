use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{Duration, timeout};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agents::formatter::FormatterAgent;
use crate::agents::informer::InformerAgent;
use crate::agents::investigation::{NewsSearcherAgent, StockDataAgent};
use crate::agents::profile::ProfileAgent;
use crate::agents::router::RouterAgent;
use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::events::RunEvent;
use crate::models::{Message, Role};
use crate::ollama::OllamaClient;
use crate::storage::Storage;

const MAX_RUN_DURATION: Duration = Duration::from_secs(5 * 60);

pub struct Orchestrator {
    pub client: Arc<OllamaClient>,
    pub storage: Arc<Storage>,
    pub model: String,
}

impl Orchestrator {
    pub fn new(client: Arc<OllamaClient>, storage: Arc<Storage>, model: String) -> Self {
        Self {
            client,
            storage,
            model,
        }
    }

    pub async fn handle_query(
        &self,
        conversation_id: Uuid,
        query: String,
        run_id: Uuid,
        cancellation: CancellationToken,
        event_tx: UnboundedSender<RunEvent>,
    ) -> Result<String> {
        let _ = event_tx.send(RunEvent::Started {
            run_id,
            conversation_id,
        });
        if let Err(error) = self.storage.start_run(run_id, conversation_id).await {
            let message = error.to_string();
            let _ = event_tx.send(RunEvent::Failed {
                run_id,
                conversation_id,
                message,
            });
            return Err(error);
        }

        let result = timeout(
            MAX_RUN_DURATION,
            async {
                tokio::select! {
                    _ = cancellation.cancelled() => Err(anyhow!("La ejecución fue cancelada.")),
                    result = self.handle_query_inner(conversation_id, query, run_id, event_tx.clone()) => result,
                }
            },
        )
        .await;

        match result {
            Ok(Ok(content)) => {
                if let Err(error) = self.storage.complete_run(run_id).await {
                    let message = error.to_string();
                    let _ = event_tx.send(RunEvent::Failed {
                        run_id,
                        conversation_id,
                        message,
                    });
                    return Err(error);
                }
                let _ = event_tx.send(RunEvent::Completed {
                    run_id,
                    conversation_id,
                });
                Ok(content)
            }
            Ok(Err(error)) => {
                let message = error.to_string();
                let _ = self.storage.fail_run(run_id, &message).await;
                let _ = event_tx.send(RunEvent::Failed {
                    run_id,
                    conversation_id,
                    message: message.clone(),
                });
                Err(error)
            }
            Err(_) => {
                let error = anyhow!("La ejecución superó el límite de {:?}.", MAX_RUN_DURATION);
                let message = error.to_string();
                let _ = self.storage.fail_run(run_id, &message).await;
                let _ = event_tx.send(RunEvent::Failed {
                    run_id,
                    conversation_id,
                    message,
                });
                Err(error)
            }
        }
    }

    async fn handle_query_inner(
        &self,
        conversation_id: Uuid,
        query: String,
        run_id: Uuid,
        event_tx: UnboundedSender<RunEvent>,
    ) -> Result<String> {
        self.trace(&event_tx, run_id, &format!("Nueva consulta: {query}"));

        let user_msg = Message {
            id: Uuid::new_v4(),
            conversation_id,
            role: Role::User,
            content: query.clone(),
            created_at: Utc::now(),
            thinking: None,
        };
        self.storage.save_message(user_msg).await?;

        let original_messages = self.storage.get_messages(conversation_id).await?;
        let mut profile = self.storage.get_profile().await?;

        let active_messages = if original_messages.len() > 10 {
            self.trace(&event_tx, run_id, "Comprimiendo contexto (>10 mensajes)...");
            self.compress_context(&original_messages).await?
        } else {
            original_messages
        };

        let profile_agent = ProfileAgent::new(
            BaseAgent::new(
                "ProfileAnalyzer",
                self.client.clone(),
                &self.model,
                "Analizar si el usuario proporcionó información de perfil.",
            )
            .with_events(run_id, event_tx.clone()),
        );

        if !profile.is_complete {
            self.trace(
                &event_tx,
                run_id,
                "🧩 Agente Perfil: Analizando requisitos faltantes...",
            );
            profile = profile_agent
                .update_profile_from_msg(&active_messages, &profile)
                .await?;
            self.storage.save_profile(&profile).await?;
        }

        if !profile.is_complete {
            self.trace(
                &event_tx,
                run_id,
                "🧩 Agente Perfil: Solicitando información al usuario...",
            );
            let output = profile_agent
                .process(
                    &active_messages,
                    &json!({ "is_complete": false, "profile": profile }),
                )
                .await?;
            return match output {
                AgentOutput::Text(text) => {
                    self.save_and_return_assistant_msg(conversation_id, text)
                        .await
                }
                AgentOutput::Structured(_) => {
                    Err(anyhow!("El agente de perfil no pudo producir texto."))
                }
            };
        }

        let router = RouterAgent::new(
            BaseAgent::new(
                "Router",
                self.client.clone(),
                &self.model,
                "Determinar la intención: educativa o investigación.",
            )
            .with_events(run_id, event_tx.clone()),
        );

        self.trace(
            &event_tx,
            run_id,
            "🔀 Agente Router: Identificando intención...",
        );
        let router_out = router.process(&active_messages, &json!({})).await?;
        let (intent, target_companies, requires_discovery, discovery_topic) = match router_out {
            AgentOutput::Structured(data) => (
                data["intent"].as_str().unwrap_or("educational").to_string(),
                data["companies"]
                    .as_array()
                    .map(|companies| {
                        companies
                            .iter()
                            .filter_map(|company| company.as_str())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                data["requires_discovery"].as_bool().unwrap_or(false),
                data["discovery_topic"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
            AgentOutput::Text(_) => ("educational".to_string(), Vec::new(), false, String::new()),
        };
        self.trace(&event_tx, run_id, &format!("Intención detectada: {intent}"));

        let final_content = if intent == "investigation" {
            let news_searcher = NewsSearcherAgent::new(
                BaseAgent::new(
                    "NewsSearcher",
                    self.client.clone(),
                    &self.model,
                    "Research market news.",
                )
                .with_events(run_id, event_tx.clone()),
            );
            let stock_data = StockDataAgent::new(
                BaseAgent::new(
                    "StockData",
                    self.client.clone(),
                    &self.model,
                    "Fetch historical prices.",
                )
                .with_events(run_id, event_tx.clone()),
            );

            self.trace(&event_tx, run_id, "🔍 Iniciando flujo de INVESTIGACIÓN...");
            let mut companies_to_research = target_companies;
            if companies_to_research.is_empty()
                && !requires_discovery
                && let Some(holdings) = &profile.holdings
            {
                companies_to_research = holdings.clone();
                self.trace(
                    &event_tx,
                    run_id,
                    "Utilizando empresas del perfil para investigación.",
                );
            }

            self.trace(
                &event_tx,
                run_id,
                &format!("Empresas a investigar: {companies_to_research:?}"),
            );
            self.trace(
                &event_tx,
                run_id,
                "📰 Agente NewsSearcher: Procesando noticias...",
            );
            let news_out = news_searcher
                .process(
                    &active_messages,
                    &json!({
                        "target_companies": companies_to_research,
                        "requires_discovery": requires_discovery,
                        "discovery_topic": discovery_topic
                    }),
                )
                .await?;
            let stock_actions = match news_out {
                AgentOutput::Structured(data) => data,
                AgentOutput::Text(_) => json!([]),
            };

            self.trace(
                &event_tx,
                run_id,
                "📊 Agente StockData: Extrayendo precios históricos...",
            );
            let prices_out = stock_data
                .process(
                    &active_messages,
                    &json!({ "investigative_results": stock_actions }),
                )
                .await?;
            let final_results = match prices_out {
                AgentOutput::Structured(data) => data,
                AgentOutput::Text(_) => json!([]),
            };

            self.trace(
                &event_tx,
                run_id,
                "📝 Agente Formatter: Generando reporte ejecutivo...",
            );
            let formatter = FormatterAgent::new(
                BaseAgent::new(
                    "Formatter",
                    self.client.clone(),
                    &self.model,
                    "Crea un resumen ejecutivo en Markdown premium. \
                     IMPORTANTE:\n\
                     1. Usa tablas formateadas profesionalmente.\n\
                     2. Si el resultado tiene 'sources', incluye los links al final de cada análisis como [Fuente](url).\n\
                     3. Sé directo y visualmente atractivo. Responde en ESPAÑOL.",
                )
                .with_events(run_id, event_tx.clone()),
            );
            let context = json!({
                "investigative_results": final_results,
                "query": query,
                "profile": profile
            });

            match formatter.process(&active_messages, &context).await? {
                AgentOutput::Text(text) => text,
                AgentOutput::Structured(_) => {
                    "Investigación completada pero falló el formateo de los resultados.".to_string()
                }
            }
        } else {
            self.trace(&event_tx, run_id, "🎓 Iniciando flujo EDUCATIVO...");
            let informer = InformerAgent::new(
                BaseAgent::new(
                    "Informer",
                    self.client.clone(),
                    &self.model,
                    "Proporciona conocimientos educativos y generales sobre inversión. Responde en ESPAÑOL.",
                )
                .with_events(run_id, event_tx.clone()),
            );
            let formatter = FormatterAgent::new(
                BaseAgent::new(
                    "Formatter",
                    self.client.clone(),
                    &self.model,
                    "Sintetiza la información educativa en ESPAÑOL.",
                )
                .with_events(run_id, event_tx.clone()),
            );

            self.trace(
                &event_tx,
                run_id,
                "💡 Agente Informer: Generando respuesta...",
            );
            let informer_text = match informer.process(&active_messages, &json!({})).await? {
                AgentOutput::Text(text) => text,
                AgentOutput::Structured(_) => String::new(),
            };

            self.trace(
                &event_tx,
                run_id,
                "📝 Agente Formatter: Estilizando respuesta...",
            );
            let context = json!({ "agent_results": informer_text, "profile": profile });
            match formatter.process(&active_messages, &context).await? {
                AgentOutput::Text(text) => {
                    self.trace(&event_tx, run_id, "✅ Flujo completado.");
                    text
                }
                AgentOutput::Structured(_) => "Educational response failed.".to_string(),
            }
        };

        self.save_and_return_assistant_msg(conversation_id, final_content)
            .await
    }

    fn trace(&self, event_tx: &UnboundedSender<RunEvent>, run_id: Uuid, message: &str) {
        let timestamp = Utc::now().format("%H:%M:%S");
        let _ = event_tx.send(RunEvent::Trace {
            run_id,
            message: format!("[{timestamp}] {message}"),
        });
    }

    async fn compress_context(&self, messages: &[Message]) -> Result<Vec<Message>> {
        const LIMIT: usize = 10;
        if messages.len() <= LIMIT {
            return Ok(messages.to_vec());
        }

        let to_summarize = &messages[..messages.len() - 2];
        let last_2 = &messages[messages.len() - 2..];
        let summary_prompt = "Eres un gestor de contexto. Resume la conversación previa con precisión en un único mensaje de sistema en ESPAÑOL. Enfócate en descubrimientos del perfil del usuario, investigación de acciones ya realizada y preguntas clave. Sé conciso pero mantén todo el contexto relevante para la investigación de seguimiento.";
        let sum_chat_msgs = to_summarize
            .iter()
            .map(|msg| crate::ollama::ChatMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: msg.content.clone(),
                images: None,
            })
            .collect::<Vec<_>>();

        let request = crate::ollama::ChatRequest {
            model: self.model.clone(),
            messages: vec![
                crate::ollama::ChatMessage {
                    role: "system".to_string(),
                    content: summary_prompt.to_string(),
                    images: None,
                },
                crate::ollama::ChatMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Por favor resume el siguiente contexto para el próximo turno:\n{sum_chat_msgs:?}"
                    ),
                    images: None,
                },
            ],
            stream: false,
            format: None,
            options: None,
        };

        let response = self.client.chat(request).await?;
        let mut final_context = vec![Message {
            id: Uuid::new_v4(),
            conversation_id: messages[0].conversation_id,
            role: Role::System,
            content: format!(
                "Resumen de la conversación previa: {}",
                response.message.content
            ),
            created_at: Utc::now(),
            thinking: None,
        }];
        final_context.extend_from_slice(last_2);
        Ok(final_context)
    }

    async fn save_and_return_assistant_msg(
        &self,
        conversation_id: Uuid,
        content: String,
    ) -> Result<String> {
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
