use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, de};
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
pub struct SourceCitation {
    pub title: String,
    pub url: String,
    pub retrieved_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StockAction {
    pub company: String,
    pub ticker: String,
    #[serde(default)]
    pub sentiment: String,
    pub reasoning: String,
    #[serde(default)]
    pub catalysts: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    pub sources: Option<Vec<SourceCitation>>,
    pub prices: Option<PriceHistory>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PricePoint {
    pub price: f64,
    pub as_of: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PriceHistory {
    pub today: Option<PricePoint>,
    pub one_week: Option<PricePoint>,
    pub one_year: Option<PricePoint>,
    pub currency: String,
    pub exchange: Option<String>,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QueryIntent {
    Educational,
    Investigation,
    OutOfScope,
}

impl<'de> Deserialize<'de> for QueryIntent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "educational" => Ok(Self::Educational),
            "investigation" => Ok(Self::Investigation),
            "out_of_scope" | "outofscope" => Ok(Self::OutOfScope),
            value => Err(de::Error::unknown_variant(
                value,
                &["educational", "investigation", "out_of_scope"],
            )),
        }
    }
}

fn deserialize_nullable_strings<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(values)) => values
            .into_iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| de::Error::custom("company tickers must be strings"))
            })
            .collect(),
        Some(serde_json::Value::String(value)) => Ok(vec![value]),
        Some(_) => Err(de::Error::custom(
            "companies must be an array of strings or a string",
        )),
    }
}

fn deserialize_nullable_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<bool>::deserialize(deserializer)?.unwrap_or(false))
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub intent: QueryIntent,
    #[serde(default, deserialize_with = "deserialize_nullable_strings")]
    pub companies: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_nullable_bool")]
    pub requires_discovery: bool,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub discovery_topic: String,
}

#[derive(Debug, Clone)]
pub enum QueryPlan {
    Educational,
    OutOfScope,
    Investigation {
        companies: Vec<String>,
        discovery_topic: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::{QueryIntent, RouteDecision};

    #[test]
    fn route_decision_should_accept_nullable_optional_fields() {
        let decision: RouteDecision = serde_json::from_str(
            r#"{"intent":"Investigation","companies":null,"requires_discovery":null,"discovery_topic":null}"#,
        )
        .expect("nullable route fields should be normalized");

        assert_eq!(decision.intent, QueryIntent::Investigation);
        assert!(decision.companies.is_empty());
        assert!(!decision.requires_discovery);
        assert!(decision.discovery_topic.is_empty());
    }
}
