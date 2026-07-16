use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::RwLock;

use crate::models::{PriceHistory, PricePoint};

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn fetch_price_history(&self, ticker: &str) -> Result<PriceHistory>;
}

struct CacheEntry {
    value: PriceHistory,
    stored_at: Instant,
}

pub struct FallbackMarketDataProvider {
    primary: Arc<dyn MarketDataProvider>,
    fallback: Arc<dyn MarketDataProvider>,
}

impl FallbackMarketDataProvider {
    pub fn new(
        primary: Arc<dyn MarketDataProvider>,
        fallback: Arc<dyn MarketDataProvider>,
    ) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl MarketDataProvider for FallbackMarketDataProvider {
    async fn fetch_price_history(&self, ticker: &str) -> Result<PriceHistory> {
        match self.primary.fetch_price_history(ticker).await {
            Ok(value) => Ok(value),
            Err(primary_error) => match self.fallback.fetch_price_history(ticker).await {
                Ok(value) => Ok(value),
                Err(fallback_error) => Err(anyhow!(
                    "Primary market provider failed: {primary_error}; fallback provider failed: {fallback_error}"
                )),
            },
        }
    }
}

pub struct CachedMarketDataProvider {
    inner: Arc<dyn MarketDataProvider>,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: StdDuration,
}

impl CachedMarketDataProvider {
    pub fn new(inner: Arc<dyn MarketDataProvider>, ttl: StdDuration) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }
}

#[async_trait]
impl MarketDataProvider for CachedMarketDataProvider {
    async fn fetch_price_history(&self, ticker: &str) -> Result<PriceHistory> {
        let ticker = normalize_ticker(ticker)?;
        if let Some(entry) = self.cache.read().await.get(&ticker)
            && entry.stored_at.elapsed() < self.ttl
        {
            return Ok(entry.value.clone());
        }

        let value = self.inner.fetch_price_history(&ticker).await?;
        self.cache.write().await.insert(
            ticker,
            CacheEntry {
                value: value.clone(),
                stored_at: Instant::now(),
            },
        );
        Ok(value)
    }
}

pub struct YahooFinanceProvider {
    base_url: String,
    client: Client,
}

impl YahooFinanceProvider {
    pub fn new(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(StdDuration::from_secs(10))
            .timeout(StdDuration::from_secs(30))
            .user_agent("StockAgent/1.0")
            .build()
            .context("Failed to build market-data HTTP client")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn from_env() -> Result<Self> {
        let base_url = env::var("MARKET_DATA_BASE_URL")
            .unwrap_or_else(|_| "https://query1.finance.yahoo.com".to_string());
        Self::new(base_url)
    }
}

#[async_trait]
impl MarketDataProvider for YahooFinanceProvider {
    async fn fetch_price_history(&self, ticker: &str) -> Result<PriceHistory> {
        let ticker = normalize_ticker(ticker)?;
        let source_url = format!("{}/v8/finance/chart/{ticker}", self.base_url);
        for attempt in 0..3 {
            let response = match self
                .client
                .get(&source_url)
                .query(&[("range", "1y"), ("interval", "1d"), ("events", "history")])
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    if attempt < 2 && (error.is_connect() || error.is_timeout()) {
                        tokio::time::sleep(market_retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(error).context("Market-data request failed");
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .context("Failed to read market-data error response")?;
                if attempt < 2 && is_retryable_market_status(status) {
                    tokio::time::sleep(market_retry_delay(attempt)).await;
                    continue;
                }
                return Err(anyhow!(
                    "Market-data provider returned {status}: {}",
                    truncate_for_error(&body)
                ));
            }

            let body = response
                .text()
                .await
                .context("Failed to read market-data response")?;
            return parse_chart_response(&body, &source_url);
        }

        Err(anyhow!("Market-data request retries exhausted"))
    }
}

pub fn normalize_ticker(value: &str) -> Result<String> {
    let ticker = value.trim().to_ascii_uppercase();
    if ticker.is_empty()
        || ticker.len() > 20
        || !ticker.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '^')
        })
    {
        return Err(anyhow!("Invalid ticker symbol: {value}"));
    }
    Ok(ticker)
}

fn parse_chart_response(body: &str, source_url: &str) -> Result<PriceHistory> {
    let payload: YahooApiResponse = serde_json::from_str(body).with_context(|| {
        format!(
            "Failed to decode market-data response: {}",
            truncate_for_error(body)
        )
    })?;

    let chart = payload.chart;
    let result = match chart.result.and_then(|mut results| results.pop()) {
        Some(result) => result,
        None => {
            let description = chart
                .error
                .and_then(|error| error.description)
                .unwrap_or_else(|| "No chart data was returned".to_string());
            return Err(anyhow!("Market-data provider error: {description}"));
        }
    };

    let timestamps = result.timestamp.unwrap_or_default();
    let closes = result
        .indicators
        .quote
        .into_iter()
        .next()
        .and_then(|quote| quote.close)
        .unwrap_or_default();

    let mut points = timestamps
        .into_iter()
        .zip(closes)
        .filter_map(|(timestamp, close)| {
            let price = close?;
            if !price.is_finite() || price < 0.0 {
                return None;
            }
            Some(PricePoint {
                price,
                as_of: DateTime::from_timestamp(timestamp, 0)?,
            })
        })
        .collect::<Vec<_>>();
    points.sort_by_key(|point| point.as_of);

    let current = match (
        result.meta.regular_market_price,
        result.meta.regular_market_time,
    ) {
        (Some(price), Some(timestamp))
            if price.is_finite()
                && price >= 0.0
                && DateTime::from_timestamp(timestamp, 0).is_some() =>
        {
            Some(PricePoint {
                price,
                as_of: DateTime::from_timestamp(timestamp, 0)
                    .context("Invalid regular market timestamp")?,
            })
        }
        _ => points.last().cloned(),
    }
    .ok_or_else(|| anyhow!("Market-data provider returned no valid prices"))?;

    let one_week_target = current.as_of - Duration::days(7);
    let one_year_target = current.as_of - Duration::days(365);

    Ok(PriceHistory {
        today: Some(current.clone()),
        one_week: closest_point(&points, one_week_target),
        one_year: closest_point(&points, one_year_target),
        currency: result
            .meta
            .currency
            .unwrap_or_else(|| "UNKNOWN".to_string()),
        exchange: result.meta.exchange_name,
        source_url: source_url.to_string(),
    })
}

fn closest_point(points: &[PricePoint], target: DateTime<Utc>) -> Option<PricePoint> {
    points
        .iter()
        .min_by_key(|point| (point.as_of - target).num_seconds().abs())
        .cloned()
}

fn is_retryable_market_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn market_retry_delay(attempt: u32) -> StdDuration {
    StdDuration::from_millis(250 * 2_u64.pow(attempt))
}

fn truncate_for_error(value: &str) -> String {
    value.chars().take(2_000).collect()
}

#[derive(Debug, Deserialize)]
struct YahooApiResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooChartResult>>,
    error: Option<YahooApiError>,
}

#[derive(Debug, Deserialize)]
struct YahooApiError {
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YahooChartResult {
    meta: YahooMeta,
    timestamp: Option<Vec<i64>>,
    indicators: YahooIndicators,
}

#[derive(Debug, Deserialize)]
struct YahooMeta {
    currency: Option<String>,
    #[serde(rename = "exchangeName")]
    exchange_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "regularMarketTime")]
    regular_market_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    close: Option<Vec<Option<f64>>>,
}

pub struct FinnhubProvider {
    base_url: String,
    api_key: String,
    client: Client,
}

impl FinnhubProvider {
    pub fn new(base_url: String, api_key: String) -> Result<Self> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(anyhow!("Finnhub API key cannot be empty"));
        }

        let client = Client::builder()
            .connect_timeout(StdDuration::from_secs(10))
            .timeout(StdDuration::from_secs(30))
            .user_agent("StockAgent/1.0")
            .build()
            .context("Failed to build Finnhub HTTP client")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client,
        })
    }

    pub fn from_env() -> Result<Self> {
        let api_key = env::var("MARKET_API")
            .context("MARKET_API is required when using the Finnhub provider")?;
        let base_url = env::var("FINNHUB_BASE_URL")
            .unwrap_or_else(|_| "https://finnhub.io/api/v1".to_string());
        Self::new(base_url, api_key)
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        mut params: Vec<(&str, String)>,
    ) -> Result<T> {
        params.push(("token", self.api_key.clone()));
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        for attempt in 0..3 {
            let response = match self.client.get(&url).query(&params).send().await {
                Ok(response) => response,
                Err(error) => {
                    if attempt < 2 && (error.is_connect() || error.is_timeout()) {
                        tokio::time::sleep(market_retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(error)
                        .with_context(|| format!("Finnhub request failed for {endpoint}"));
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .context("Failed to read Finnhub error response")?;
                if attempt < 2 && is_retryable_market_status(status) {
                    tokio::time::sleep(market_retry_delay(attempt)).await;
                    continue;
                }
                let access_hint = if status == reqwest::StatusCode::FORBIDDEN {
                    " Access denied: the Finnhub plan/key may not include this endpoint or market; free plans are generally restricted to US market data."
                } else {
                    ""
                };
                return Err(anyhow!(
                    "Finnhub returned {status} for {endpoint}:{access_hint} {}",
                    truncate_for_error(&body)
                ));
            }

            let body = response
                .text()
                .await
                .with_context(|| format!("Failed to read Finnhub response for {endpoint}"))?;
            return serde_json::from_str(&body).with_context(|| {
                format!(
                    "Failed to decode Finnhub response for {endpoint}: {}",
                    truncate_for_error(&body)
                )
            });
        }

        Err(anyhow!("Finnhub request retries exhausted for {endpoint}"))
    }
}

#[async_trait]
impl MarketDataProvider for FinnhubProvider {
    async fn fetch_price_history(&self, ticker: &str) -> Result<PriceHistory> {
        let ticker = normalize_ticker(ticker)?;
        let now = Utc::now();
        let from = (now - Duration::days(370)).timestamp().to_string();
        let to = now.timestamp().to_string();

        let quote: FinnhubQuote = self
            .get_json("quote", vec![("symbol", ticker.clone())])
            .await?;
        let candle: FinnhubCandle = self
            .get_json(
                "stock/candle",
                vec![
                    ("symbol", ticker.clone()),
                    ("resolution", "D".to_string()),
                    ("from", from),
                    ("to", to),
                ],
            )
            .await?;
        let profile: FinnhubProfile = self
            .get_json("stock/profile2", vec![("symbol", ticker.clone())])
            .await?;

        let source_url = format!(
            "{}/stock/candle?symbol={ticker}&resolution=D",
            self.base_url
        );
        build_finnhub_price_history(quote, candle, profile, source_url)
    }
}

fn build_finnhub_price_history(
    quote: FinnhubQuote,
    candle: FinnhubCandle,
    profile: FinnhubProfile,
    source_url: String,
) -> Result<PriceHistory> {
    if candle.status.as_deref() != Some("ok") {
        return Err(anyhow!(
            "Finnhub returned no historical data: {}",
            candle
                .status
                .unwrap_or_else(|| "unknown status".to_string())
        ));
    }

    let timestamps = candle.timestamps.unwrap_or_default();
    let closes = candle.closes.unwrap_or_default();
    let mut points = timestamps
        .into_iter()
        .zip(closes)
        .filter_map(|(timestamp, price)| {
            if !price.is_finite() || price < 0.0 {
                return None;
            }
            Some(PricePoint {
                price,
                as_of: DateTime::from_timestamp(timestamp, 0)?,
            })
        })
        .collect::<Vec<_>>();
    points.sort_by_key(|point| point.as_of);

    let current = match (quote.current, quote.timestamp) {
        (Some(price), Some(timestamp))
            if price.is_finite()
                && price >= 0.0
                && DateTime::from_timestamp(timestamp, 0).is_some() =>
        {
            Some(PricePoint {
                price,
                as_of: DateTime::from_timestamp(timestamp, 0)
                    .context("Invalid Finnhub quote timestamp")?,
            })
        }
        _ => points.last().cloned(),
    }
    .ok_or_else(|| anyhow!("Finnhub returned no valid current price"))?;

    Ok(PriceHistory {
        today: Some(current.clone()),
        one_week: closest_point(&points, current.as_of - Duration::days(7)),
        one_year: closest_point(&points, current.as_of - Duration::days(365)),
        currency: profile.currency.unwrap_or_else(|| "UNKNOWN".to_string()),
        exchange: profile.exchange,
        source_url,
    })
}

#[derive(Debug, Deserialize)]
struct FinnhubQuote {
    #[serde(rename = "c")]
    current: Option<f64>,
    #[serde(rename = "t")]
    timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FinnhubCandle {
    #[serde(rename = "c")]
    closes: Option<Vec<f64>>,
    #[serde(rename = "t")]
    timestamps: Option<Vec<i64>>,
    #[serde(rename = "s")]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FinnhubProfile {
    currency: Option<String>,
    exchange: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockMarketDataProvider;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn normalize_ticker_should_reject_unsafe_symbols() {
        assert!(normalize_ticker("AAPL").is_ok());
        assert!(normalize_ticker("AAPL/../../secret").is_err());
    }

    #[test]
    fn parse_chart_response_should_preserve_market_metadata() {
        let body = r#"
        {
          "chart": {
            "result": [{
              "meta": {
                "currency": "USD",
                "exchangeName": "NMS",
                "regularMarketPrice": 125.5,
                "regularMarketTime": 1735689600
              },
              "timestamp": [1704067200, 1704672000, 1735689600],
              "indicators": {"quote": [{"close": [100.0, 110.0, 125.0]}]}
            }],
            "error": null
          }
        }
        "#;

        let history = parse_chart_response(body, "https://example.test/chart/AAPL")
            .expect("valid chart response should parse");
        assert_eq!(history.currency, "USD");
        assert_eq!(history.exchange.as_deref(), Some("NMS"));
        assert_eq!(history.today.as_ref().map(|point| point.price), Some(125.5));
        assert_eq!(history.source_url, "https://example.test/chart/AAPL");
    }

    #[test]
    fn parse_finnhub_data_should_preserve_quote_and_profile_metadata() {
        let history = build_finnhub_price_history(
            FinnhubQuote {
                current: Some(125.5),
                timestamp: Some(1735689600),
            },
            FinnhubCandle {
                closes: Some(vec![100.0, 110.0, 125.0]),
                timestamps: Some(vec![1704067200, 1704672000, 1735689600]),
                status: Some("ok".to_string()),
            },
            FinnhubProfile {
                currency: Some("USD".to_string()),
                exchange: Some("NASDAQ".to_string()),
            },
            "https://finnhub.io/api/v1/stock/candle?symbol=AAPL&resolution=D".to_string(),
        )
        .expect("valid Finnhub data should parse");

        assert_eq!(history.currency, "USD");
        assert_eq!(history.exchange.as_deref(), Some("NASDAQ"));
        assert_eq!(history.today.as_ref().map(|point| point.price), Some(125.5));
    }

    struct CountingProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MarketDataProvider for CountingProvider {
        async fn fetch_price_history(&self, _ticker: &str) -> Result<PriceHistory> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(PriceHistory {
                today: None,
                one_week: None,
                one_year: None,
                currency: "USD".to_string(),
                exchange: None,
                source_url: "https://example.test".to_string(),
            })
        }
    }

    struct FailingProvider;

    #[async_trait]
    impl MarketDataProvider for FailingProvider {
        async fn fetch_price_history(&self, _ticker: &str) -> Result<PriceHistory> {
            Err(anyhow!("primary provider unavailable"))
        }
    }

    #[tokio::test]
    async fn mock_market_provider_should_return_configured_success_and_count_calls() {
        let provider = MockMarketDataProvider::success(42.0);
        let history = provider
            .fetch_price_history("AAPL")
            .await
            .expect("mock provider should return configured data");

        assert_eq!(history.today.as_ref().map(|point| point.price), Some(42.0));
        assert_eq!(provider.calls(), 1);
    }

    #[tokio::test]
    async fn mock_market_provider_should_return_configured_failure() {
        let provider = MockMarketDataProvider::failure("offline");
        let error = provider
            .fetch_price_history("AAPL")
            .await
            .expect_err("mock provider should return configured failure");

        assert!(error.to_string().contains("offline"));
        assert_eq!(provider.calls(), 1);
    }

    #[tokio::test]
    async fn fallback_provider_should_use_secondary_provider_after_primary_failure() {
        let fallback = FallbackMarketDataProvider::new(
            Arc::new(FailingProvider),
            Arc::new(CountingProvider {
                calls: Arc::new(AtomicUsize::new(0)),
            }),
        );

        let history = fallback
            .fetch_price_history("AAPL")
            .await
            .expect("fallback provider should return secondary data");

        assert_eq!(history.currency, "USD");
    }

    #[tokio::test]
    async fn cached_provider_should_reuse_fresh_price_history() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = CachedMarketDataProvider::new(
            Arc::new(CountingProvider {
                calls: calls.clone(),
            }),
            StdDuration::from_secs(60),
        );

        provider
            .fetch_price_history("aapl")
            .await
            .expect("first lookup should succeed");
        provider
            .fetch_price_history("AAPL")
            .await
            .expect("cached lookup should succeed");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
