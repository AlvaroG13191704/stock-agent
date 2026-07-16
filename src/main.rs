mod agents;
mod events;
mod market_data;
mod models;
mod ollama;
mod orchestrator;
mod storage;
#[cfg(test)]
mod test_support;
mod ui;

use crate::market_data::{
    CachedMarketDataProvider, FallbackMarketDataProvider, FinnhubProvider, MarketDataProvider,
    YahooFinanceProvider,
};
use crate::ollama::OllamaClient;
use crate::orchestrator::Orchestrator;
use crate::storage::Storage;
use anyhow::Result;
use dotenv::dotenv;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let client = Arc::new(OllamaClient::from_env()?);
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:stock_agent.db".to_string());
    let manager = Arc::new(Storage::new(&database_url).await?);

    let model = std::env::var("DEFAULT_MODEL").unwrap_or_else(|_| "gemma4:31b-cloud".to_string());

    let market_data_provider: Arc<dyn MarketDataProvider> =
        if std::env::var("MARKET_API").is_ok_and(|key| !key.trim().is_empty()) {
            let finnhub: Arc<dyn MarketDataProvider> = Arc::new(FinnhubProvider::from_env()?);
            let yahoo: Arc<dyn MarketDataProvider> = Arc::new(YahooFinanceProvider::from_env()?);
            Arc::new(FallbackMarketDataProvider::new(finnhub, yahoo))
        } else {
            Arc::new(YahooFinanceProvider::from_env()?)
        };
    let cache_ttl = std::env::var("MARKET_DATA_CACHE_TTL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(300);
    let market_data: Arc<dyn MarketDataProvider> = Arc::new(CachedMarketDataProvider::new(
        market_data_provider,
        std::time::Duration::from_secs(cache_ttl),
    ));
    let orchestrator = Arc::new(Orchestrator::new(
        client.clone(),
        manager.clone(),
        model,
        market_data,
    ));

    ui::run(orchestrator, manager).await?;

    Ok(())
}
