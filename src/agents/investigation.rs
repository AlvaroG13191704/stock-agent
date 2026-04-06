use anyhow::Result;
use async_trait::async_trait;
use crate::agents::{Agent, AgentOutput, BaseAgent};
use crate::models::{Message, StockAction, PriceHistory};
use serde_json::json;

/// Researches latest news and outputs a list of structured buy/sell reasoning
pub struct NewsSearcherAgent {
    pub base: BaseAgent,
}

impl NewsSearcherAgent {
    pub fn new(base: BaseAgent) -> Self {
        Self { base }
    }
}

#[async_trait]
impl Agent for NewsSearcherAgent {
    fn name(&self) -> &str {
        &self.base.name
    }

    async fn process(&self, _messages: &[Message], context: &serde_json::Value) -> Result<AgentOutput> {
        let companies = context["target_companies"].as_array().map(|arr| {
            arr.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect::<Vec<String>>()
        }).unwrap_or_default();

        let mut results = Vec::new();
        for company in companies {
            let query = format!("latest stock news and market sentiment for {} {}", company, company);
            let search_res = self.base.client.web_search(query).await?;
            let mut news_text = String::new();
            for item in search_res.results.iter().take(3) {
                news_text.push_str(&format!("Source: {}\nContent: {}\n", item.url, item.content));
            }

            let system_prompt = format!("Analiza las noticias de {}. Genera un razonamiento con recomendaciones de compra/venta basado en el sentimiento actual del mercado en ESPAÑOL.\nDevuelve JSON: {{ \"company\": \"...\", \"ticker\": \"...\", \"reasoning\": \"...\" }}", company);
            let req = crate::ollama::ChatRequest {
                model: self.base.model.clone(),
                messages: vec![crate::ollama::ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                    images: None,
                }, crate::ollama::ChatMessage {
                    role: "user".to_string(),
                    content: format!("News for {}:\n{}", company, news_text),
                    images: None,
                }],
                stream: false,
                format: Some(json!({
                    "type": "object",
                    "properties": {
                        "company": { "type": "string" },
                        "ticker": { "type": "string" },
                        "reasoning": { "type": "string" }
                    },
                    "required": ["company", "ticker", "reasoning"]
                })),
                options: None,
            };
            let res = self.base.client.chat(req).await?;
            let stock_action: StockAction = crate::agents::parse_llm_json(&res.message.content)?;
            results.push(stock_action);
        }

        Ok(AgentOutput::Structured(json!(results)))
    }
}

/// Fetches price history info (today, 1w, 1y) via web search
pub struct StockDataAgent {
    pub base: BaseAgent,
}

impl StockDataAgent {
    pub fn new(base: BaseAgent) -> Self {
        Self { base }
    }
}

#[async_trait]
impl Agent for StockDataAgent {
    fn name(&self) -> &str {
        &self.base.name
    }

    async fn process(&self, _messages: &[Message], context: &serde_json::Value) -> Result<AgentOutput> {
         let mut actions: Vec<StockAction> = serde_json::from_value(context["investigative_results"].clone())?;
         
         for action in actions.iter_mut() {
             let query = format!("{} stock historical prices today vs 1 week ago vs 1 year ago at google finance bloomberg", action.ticker);
             let search_res = self.base.client.web_search(query).await?;
             let mut data_text = String::new();
             for item in search_res.results.iter().take(3) {
                 data_text.push_str(&format!("- {}\n", item.content));
             }

             let sys_prompt = format!("Extrae precios numéricos precisos para {}. Un precio para hoy, un precio de hace 7 días y un precio de hace 1 año.\nDevuelve JSON: {{ \"today\": 0.0, \"one_week\": 0.0, \"one_year\": 0.0 }}", action.ticker);
             let req = crate::ollama::ChatRequest {
                model: self.base.model.clone(),
                messages: vec![crate::ollama::ChatMessage {
                    role: "system".to_string(),
                    content: sys_prompt,
                    images: None,
                }, crate::ollama::ChatMessage {
                    role: "user".to_string(),
                    content: format!("Search results:\n{}", data_text),
                    images: None,
                }],
                stream: false,
                format: Some(json!({
                    "type": "object",
                    "properties": {
                        "today": { "type": "number" },
                        "one_week": { "type": "number" },
                        "one_year": { "type": "number" }
                    },
                    "required": ["today", "one_week", "one_year"]
                })),
                options: None,
            };
            let res = self.base.client.chat(req).await?;
            let prices: PriceHistory = crate::agents::parse_llm_json(&res.message.content)?;
            action.prices = Some(prices);
         }

         Ok(AgentOutput::Structured(json!(actions)))
    }
}
