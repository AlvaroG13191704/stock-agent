mod models;
mod ollama;
mod storage;
mod agents;
mod orchestrator;
mod ui;

use anyhow::Result;
use dotenv::dotenv;
use std::sync::Arc;
use crate::ollama::OllamaClient;
use crate::storage::Storage;
use crate::orchestrator::Orchestrator;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let client = Arc::new(OllamaClient::from_env()?);
    let manager = Arc::new(Storage::new("sqlite:stock_agent.db").await?);
    
    let model = std::env::var("DEFAULT_MODEL")
        .unwrap_or_else(|_| "gemma4:31b-cloud".to_string());
    
    let orchestrator = Arc::new(Orchestrator::new(client.clone(), manager.clone(), model));

    ui::run(orchestrator, manager).await?;

    Ok(())
}
