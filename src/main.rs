mod agents;
mod events;
mod models;
mod ollama;
mod orchestrator;
mod storage;
mod ui;

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
    let manager = Arc::new(Storage::new("sqlite:stock_agent.db").await?);

    let model = std::env::var("DEFAULT_MODEL").unwrap_or_else(|_| "gemma4:31b-cloud".to_string());

    let orchestrator = Arc::new(Orchestrator::new(client.clone(), manager.clone(), model));

    ui::run(orchestrator, manager).await?;

    Ok(())
}
