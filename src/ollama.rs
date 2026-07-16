use anyhow::{Context, Result, anyhow};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

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
    pub fn new(base_url: String, api_key: Option<String>) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http_client,
        })
    }

    pub fn from_env() -> Result<Self> {
        let base_url =
            env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let api_key = env::var("OLLAMA_API_KEY").ok();
        Self::new(base_url, api_key)
    }

    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let headers = self.headers(true)?;

        let response = self
            .http_client
            .post(url)
            .headers(headers)
            .json(&req)
            .send()
            .await
            .context("Ollama chat request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .context("Failed to read Ollama error response")?;
            return Err(anyhow!(
                "Ollama API error {status}: {}",
                truncate_for_error(&body)
            ));
        }

        let body = response
            .text()
            .await
            .context("Failed to read Ollama response")?;
        serde_json::from_str::<ChatResponse>(&body).with_context(|| {
            format!(
                "Failed to decode Ollama response: {}",
                truncate_for_error(&body)
            )
        })
    }

    pub async fn web_search(&self, query: String) -> Result<WebSearchResponse> {
        let url = "https://ollama.com/api/web_search";
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow!("OLLAMA_API_KEY is required for web search"))?;
        let mut headers = self.headers(false)?;
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let req = WebSearchRequest {
            query,
            max_results: Some(5),
        };

        let response = self
            .http_client
            .post(url)
            .headers(headers)
            .json(&req)
            .send()
            .await
            .context("Ollama web search request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .context("Failed to read Ollama web search error response")?;
            return Err(anyhow!(
                "Ollama web search error {status}: {}",
                truncate_for_error(&body)
            ));
        }

        response
            .json::<WebSearchResponse>()
            .await
            .context("Failed to decode Ollama web search response")
    }

    fn headers(&self, include_user_agent: bool) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if include_user_agent {
            headers.insert(USER_AGENT, HeaderValue::from_static("StockAgent/1.0"));
        }
        if let Some(key) = &self.api_key {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))?,
            );
        }
        Ok(headers)
    }
}

fn truncate_for_error(value: &str) -> String {
    const MAX_ERROR_BYTES: usize = 2_000;
    if value.len() <= MAX_ERROR_BYTES {
        return value.to_string();
    }

    format!("{}…", &value[..MAX_ERROR_BYTES])
}
