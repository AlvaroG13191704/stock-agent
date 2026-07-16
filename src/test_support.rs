//! Deterministic test doubles used by unit tests.

use crate::market_data::MarketDataProvider;
use crate::models::{PriceHistory, PricePoint};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone)]
pub struct MockMarketDataProvider {
    response: Result<PriceHistory, String>,
    calls: Arc<AtomicUsize>,
}

impl MockMarketDataProvider {
    pub fn success(price: f64) -> Self {
        Self {
            response: Ok(PriceHistory {
                today: Some(PricePoint {
                    price,
                    as_of: Utc::now(),
                }),
                one_week: None,
                one_year: None,
                currency: "USD".to_string(),
                exchange: Some("MOCK".to_string()),
                source_url: "https://mock.invalid/market-data".to_string(),
            }),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn failure(message: &str) -> Self {
        Self {
            response: Err(message.to_string()),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl MarketDataProvider for MockMarketDataProvider {
    async fn fetch_price_history(&self, _ticker: &str) -> Result<PriceHistory> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.response
            .clone()
            .map_err(|message| anyhow!("mock market provider: {message}"))
    }
}
