use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use std::env;

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    #[serde(rename = "model")]
    pub _model: String,
    pub message: ChatMessage,
    #[serde(rename = "done")]
    pub _done: bool,
    #[serde(rename = "thinking")]
    pub _thinking: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchRequest {
    pub query: String,
    pub max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct WebSearchResponse {
    pub results: Vec<WebSearchResult>,
}

pub struct OllamaClient {
    base_url: String,
    api_key: Option<String>,
    http_client: reqwest::Client,
}

impl OllamaClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            base_url,
            api_key,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Result<Self> {
        let base_url = env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let api_key = env::var("OLLAMA_API_KEY").ok();
        Ok(Self::new(base_url, api_key))
    }

    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static("StockAgent/1.0"));
        
        if let Some(key) = &self.api_key {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", key))?);
        }

        let res = self.http_client.post(url)
            .headers(headers)
            .json(&req)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await?;
            return Err(anyhow!("Ollama API error {}: {}", status, err_text));
        }

        let text = res.text().await?;
        match serde_json::from_str::<ChatResponse>(&text) {
            Ok(json) => Ok(json),
            Err(e) => {
                eprintln!("DEBUG: Failed to parse JSON. Body: {}", text);
                Err(anyhow!("Failed to decode response: {}", e))
            }
        }
    }

    pub async fn web_search(&self, query: String) -> Result<WebSearchResponse> {
        let url = "https://ollama.com/api/web_search";
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        let api_key = self.api_key.as_ref().ok_or_else(|| anyhow!("OLLAMA_API_KEY is required for web search"))?;
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", api_key))?);

        let req = WebSearchRequest { query, max_results: Some(5) };

        let res = self.http_client.post(url)
            .headers(headers)
            .json(&req)
            .send()
            .await?;

        if !res.status().is_success() {
            let err_text = res.text().await?;
            return Err(anyhow!("Ollama Web Search error: {}", err_text));
        }

        let search_res = res.json::<WebSearchResponse>().await?;
        Ok(search_res)
    }
}
