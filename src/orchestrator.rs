use anyhow::{Context, Result, anyhow};
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
use crate::events::{RunEvent, TokenUsage};
use crate::market_data::MarketDataProvider;
use crate::models::{Message, QueryIntent, QueryPlan, Role, RouteDecision};
use crate::ollama::OllamaClient;
use crate::storage::Storage;

const MAX_RUN_DURATION: Duration = Duration::from_secs(5 * 60);
const FINANCIAL_DISCLAIMER: &str = "\n\n> **Aviso:** Este informe es únicamente educativo e informativo. No constituye asesoramiento financiero, fiscal o legal. Verifica los datos y considera tu situación y tolerancia al riesgo antes de tomar decisiones.";
const OUT_OF_SCOPE_RESPONSE: &str = "Solo puedo ayudarte con educación financiera, mercados, acciones, ETFs, bonos, carteras, riesgo y datos económicos. No puedo resolver tareas ajenas al dominio, como preparar una pizza. Reformula tu pregunta desde una perspectiva financiera y estaré encantado de ayudarte.";

pub(crate) fn is_retryable_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "timeout",
        "timed out",
        "request failed",
        "connection",
        "network",
        "temporarily unavailable",
        "returned 429",
        "returned 500",
        "returned 501",
        "returned 502",
        "returned 503",
        "returned 504",
        "api error 429",
        "api error 5",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

pub(crate) fn is_obviously_out_of_scope(query: &str) -> bool {
    let normalized = query.trim().to_ascii_lowercase();
    let financial_context = [
        "acción",
        "acciones",
        "stock",
        "etf",
        "bono",
        "invert",
        "ticker",
        "mercado",
        "cartera",
        "portfolio",
        "precio",
        "cotización",
        "financ",
        "riesgo",
        "dividendo",
        "earnings",
        "share",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    if financial_context {
        return false;
    }

    [
        "pizza",
        "receta",
        "cocina",
        "cocinar",
        "cocin",
        "hornear",
        "ingredientes para",
        "weather",
        "clima",
        "fútbol",
        "futbol",
        "football",
        "videojuego",
        "videojuegos",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn truncate_for_trace(value: &str) -> String {
    const MAX_TRACE_BYTES: usize = 1_500;
    if value.len() <= MAX_TRACE_BYTES {
        return value.to_string();
    }
    value.chars().take(MAX_TRACE_BYTES).collect::<String>() + "…"
}

pub struct Orchestrator {
    pub client: Arc<OllamaClient>,
    pub storage: Arc<Storage>,
    pub model: String,
    pub market_data: Arc<dyn MarketDataProvider>,
}

impl Orchestrator {
    pub fn new(
        client: Arc<OllamaClient>,
        storage: Arc<Storage>,
        model: String,
        market_data: Arc<dyn MarketDataProvider>,
    ) -> Self {
        Self {
            client,
            storage,
            model,
            market_data,
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
                retryable: is_retryable_error(&error),
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
                        retryable: is_retryable_error(&error),
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
                    retryable: is_retryable_error(&error),
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
                    retryable: true,
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

        if is_obviously_out_of_scope(&query) {
            self.trace(
                &event_tx,
                run_id,
                "🛡️ Guardrail: consulta fuera del dominio financiero; se bloquea el flujo de agentes.",
            );
            return self
                .save_and_return_assistant_msg(conversation_id, OUT_OF_SCOPE_RESPONSE.to_string())
                .await;
        }

        let original_messages = self.storage.get_messages(conversation_id).await?;
        let mut profile = self.storage.get_profile().await?;

        let active_messages = if original_messages.len() > 10 {
            self.trace(&event_tx, run_id, "Comprimiendo contexto (>10 mensajes)...");
            self.compress_context(&original_messages, run_id, &event_tx)
                .await?
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
            self.stage(&event_tx, run_id, "Profile", 1, 5);
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
        self.stage(&event_tx, run_id, "Router", 2, 5);
        let router_out = router.process(&active_messages, &json!({})).await?;
        let decision: RouteDecision = match router_out {
            AgentOutput::Structured(data) => serde_json::from_value(data.clone()).with_context(|| {
                format!(
                    "La respuesta del Router no cumple el esquema de ruta esperado. JSON recibido: {}",
                    truncate_for_trace(&data.to_string())
                )
            })?,
            AgentOutput::Text(_) => {
                return Err(anyhow!("El Router no produjo una decisión estructurada."));
            }
        };
        let plan = match decision.intent {
            QueryIntent::Educational => QueryPlan::Educational,
            QueryIntent::OutOfScope => QueryPlan::OutOfScope,
            QueryIntent::Investigation => QueryPlan::Investigation {
                companies: decision.companies,
                discovery_topic: if decision.requires_discovery
                    && !decision.discovery_topic.trim().is_empty()
                {
                    Some(decision.discovery_topic)
                } else {
                    None
                },
            },
        };
        self.trace(&event_tx, run_id, &format!("Plan detectado: {plan:?}"));

        if matches!(&plan, QueryPlan::OutOfScope) {
            self.trace(
                &event_tx,
                run_id,
                "🛡️ Guardrail del Router: consulta fuera del dominio financiero.",
            );
            return self
                .save_and_return_assistant_msg(conversation_id, OUT_OF_SCOPE_RESPONSE.to_string())
                .await;
        }

        let final_content = if let QueryPlan::Investigation {
            companies: target_companies,
            discovery_topic,
        } = plan
        {
            let requires_discovery = discovery_topic.is_some();
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
                    "Fetch verified historical prices.",
                )
                .with_events(run_id, event_tx.clone()),
                self.market_data.clone(),
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
            self.stage(&event_tx, run_id, "NewsSearcher", 3, 5);
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
                        "discovery_topic": discovery_topic.unwrap_or_default()
                    }),
                )
                .await?;
            let stock_actions = match news_out {
                AgentOutput::Structured(data) => data,
                AgentOutput::Text(_) => json!([]),
            };

            self.stage(&event_tx, run_id, "MarketData", 4, 5);
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

            self.stage(&event_tx, run_id, "Formatter", 5, 5);
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
                    r#"Crea un resumen ejecutivo en Markdown premium.
IMPORTANTE:
1. Usa tablas formateadas profesionalmente.
2. Separa datos verificables, interpretación de noticias y riesgos.
3. Incluye las fuentes con título, URL y fecha de recuperación.
4. No inventes precios, fechas, monedas ni recomendaciones de compra/venta.
Responde en ESPAÑOL."#,
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
            self.stage(&event_tx, run_id, "Informer", 3, 4);
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
                    "Sintetiza la información educativa en ESPAÑOL. No presentes la respuesta como asesoramiento financiero personalizado.",
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

            self.stage(&event_tx, run_id, "Formatter", 4, 4);
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

        self.save_and_return_assistant_msg(
            conversation_id,
            format!("{final_content}{FINANCIAL_DISCLAIMER}"),
        )
        .await
    }

    fn stage(
        &self,
        event_tx: &UnboundedSender<RunEvent>,
        run_id: Uuid,
        agent: &str,
        current: usize,
        total: usize,
    ) {
        let _ = event_tx.send(RunEvent::Stage {
            run_id,
            agent: agent.to_string(),
            current,
            total,
        });
    }

    fn trace(&self, event_tx: &UnboundedSender<RunEvent>, run_id: Uuid, message: &str) {
        let timestamp = Utc::now().format("%H:%M:%S");
        let _ = event_tx.send(RunEvent::Trace {
            run_id,
            message: format!("[{timestamp}] {message}"),
        });
    }

    async fn compress_context(
        &self,
        messages: &[Message],
        run_id: Uuid,
        event_tx: &UnboundedSender<RunEvent>,
    ) -> Result<Vec<Message>> {
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
        let usage = TokenUsage {
            prompt_tokens: response.prompt_eval_count.unwrap_or_default(),
            completion_tokens: response.eval_count.unwrap_or_default(),
        };
        if usage.total() > 0 {
            let _ = event_tx.send(RunEvent::Usage { run_id, usage });
        }
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

#[cfg(test)]
mod tests {
    use super::is_obviously_out_of_scope;

    #[test]
    fn guardrail_should_block_obvious_cooking_queries() {
        assert!(is_obviously_out_of_scope("¿Cómo preparo una pizza?"));
    }

    #[test]
    fn guardrail_should_allow_financial_queries() {
        assert!(!is_obviously_out_of_scope(
            "¿Qué riesgos tiene invertir en un ETF?"
        ));
        assert!(!is_obviously_out_of_scope(
            "¿Qué perspectivas tiene Pizza Pizza como acción?"
        ));
    }
}
