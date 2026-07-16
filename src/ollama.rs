use anyhow::{Context, Result, anyhow};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default)]
    pub prompt_eval_count: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u64>,
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

        let mut response = None;
        for attempt in 0..3 {
            match self
                .http_client
                .post(&url)
                .headers(headers.clone())
                .json(&req)
                .send()
                .await
            {
                Ok(candidate) if candidate.status().is_success() => {
                    response = Some(candidate);
                    break;
                }
                Ok(candidate) => {
                    let status = candidate.status();
                    let body = candidate
                        .text()
                        .await
                        .context("Failed to read Ollama error response")?;
                    if attempt < 2 && is_retryable_status(status) {
                        tokio::time::sleep(retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(anyhow!(
                        "Ollama API error {status}: {}",
                        truncate_for_error(&body)
                    ));
                }
                Err(error) => {
                    if attempt < 2 && (error.is_connect() || error.is_timeout()) {
                        tokio::time::sleep(retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(error).context("Ollama chat request failed");
                }
            }
        }

        let response =
            response.ok_or_else(|| anyhow!("Ollama chat request did not return a response"))?;
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

        for attempt in 0..3 {
            let response = match self
                .http_client
                .post(url)
                .headers(headers.clone())
                .json(&req)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    if attempt < 2 && (error.is_connect() || error.is_timeout()) {
                        tokio::time::sleep(retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(error).context("Ollama web search request failed");
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .context("Failed to read Ollama web search error response")?;
                if attempt < 2 && is_retryable_status(status) {
                    tokio::time::sleep(retry_delay(attempt)).await;
                    continue;
                }
                return Err(anyhow!(
                    "Ollama web search error {status}: {}",
                    truncate_for_error(&body)
                ));
            }

            return response
                .json::<WebSearchResponse>()
                .await
                .context("Failed to decode Ollama web search response");
        }

        Err(anyhow!("Ollama web search retries exhausted"))
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

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_millis(250 * 2_u64.pow(attempt))
}

fn truncate_for_error(value: &str) -> String {
    const MAX_ERROR_BYTES: usize = 2_000;
    if value.len() <= MAX_ERROR_BYTES {
        return value.to_string();
    }

    format!("{}…", &value[..MAX_ERROR_BYTES])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn mock_ollama_server(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock server should bind");
        let address = listener
            .local_addr()
            .expect("mock server should expose its address");
        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("mock server should accept a request");
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("mock server should write a response");
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn chat_should_parse_token_usage_from_mock_server() {
        let base_url = mock_ollama_server(
            r#"{"model":"mock","message":{"role":"assistant","content":"respuesta"},"done":true,"prompt_eval_count":12,"eval_count":7}"#,
        )
        .await;
        let client = OllamaClient::new(base_url, None).expect("mock client should initialize");
        let response = client
            .chat(ChatRequest {
                model: "mock".to_string(),
                messages: Vec::new(),
                stream: false,
                format: None,
                options: None,
            })
            .await
            .expect("mock Ollama response should decode");

        assert_eq!(response.message.content, "respuesta");
        assert_eq!(response.prompt_eval_count, Some(12));
        assert_eq!(response.eval_count, Some(7));
    }
}
