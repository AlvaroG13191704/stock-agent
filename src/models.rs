use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Role {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: Role,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub thinking: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UserProfile {
    pub experience: Option<String>,
    pub knowledge: Option<String>,
    pub platforms: Option<String>,
    pub holdings: Option<Vec<String>>,
    pub is_complete: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StockAction {
    pub company: String,
    pub ticker: String,
    pub reasoning: String,
    pub sources: Option<Vec<String>>,
    pub prices: Option<PriceHistory>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PriceHistory {
    pub today: Option<f64>,
    pub one_week: Option<f64>,
    pub one_year: Option<f64>,
}
