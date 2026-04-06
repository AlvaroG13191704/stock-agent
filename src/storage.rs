use sqlx::sqlite::{SqlitePool, SqliteConnectOptions};
use sqlx::{Row};
use anyhow::Result;
use crate::models::{Conversation, Message, Role, UserProfile};
use uuid::Uuid;
use chrono::Utc;
use std::str::FromStr;
use serde_json;

pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    pub async fn new(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;
        
        // Simple manual migrations for now
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at DATETIME NOT NULL
            )"
        ).execute(&pool).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                thinking TEXT,
                created_at DATETIME NOT NULL,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            )"
        ).execute(&pool).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_profiles (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                experience TEXT,
                knowledge TEXT,
                platforms TEXT,
                holdings TEXT,
                is_complete BOOLEAN DEFAULT FALSE
            )"
        ).execute(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn get_profile(&self) -> Result<UserProfile> {
        let row = sqlx::query("SELECT experience, knowledge, platforms, holdings, is_complete FROM user_profiles WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => {
                let holdings_str: Option<String> = r.get("holdings");
                let holdings: Option<Vec<String>> = holdings_str.and_then(|h| serde_json::from_str(&h).ok());
                Ok(UserProfile {
                    experience: r.get("experience"),
                    knowledge: r.get("knowledge"),
                    platforms: r.get("platforms"),
                    holdings,
                    is_complete: r.get("is_complete"),
                })
            }
            None => Ok(UserProfile::default()),
        }
    }

    pub async fn save_profile(&self, profile: &UserProfile) -> Result<()> {
        let holdings_json = serde_json::to_string(&profile.holdings.as_ref().unwrap_or(&vec![])).unwrap_or("[]".to_string());
        sqlx::query(
            "INSERT OR REPLACE INTO user_profiles (id, experience, knowledge, platforms, holdings, is_complete)
            VALUES (1, ?, ?, ?, ?, ?)"
        )
        .bind(&profile.experience)
        .bind(&profile.knowledge)
        .bind(&profile.platforms)
        .bind(holdings_json)
        .bind(profile.is_complete)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_conversation(&self, title: &str) -> Result<Conversation> {
        let conversation = Conversation {
            id: Uuid::new_v4(),
            title: title.to_string(),
            created_at: Utc::now(),
        };

        sqlx::query(
            "INSERT INTO conversations (id, title, created_at) VALUES (?, ?, ?)"
        )
        .bind(conversation.id.to_string())
        .bind(&conversation.title)
        .bind(conversation.created_at)
        .execute(&self.pool)
        .await?;

        Ok(conversation)
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let rows = sqlx::query("SELECT id, title, created_at FROM conversations ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let convs = rows.into_iter().map(|row| {
            Conversation {
                id: Uuid::parse_str(row.get(0)).unwrap(),
                title: row.get(1),
                created_at: row.get(2),
            }
        }).collect();

        Ok(convs)
    }

    pub async fn delete_conversation(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_message(&self, msg: Message) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, thinking, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(msg.id.to_string())
        .bind(msg.conversation_id.to_string())
        .bind(serde_json::to_string(&msg.role)?.trim_matches('"'))
        .bind(&msg.content)
        .bind(&msg.thinking)
        .bind(msg.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_messages(&self, conversation_id: Uuid) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            "SELECT id, conversation_id, role, content, thinking, created_at FROM messages WHERE conversation_id = ? ORDER BY created_at ASC"
        )
        .bind(conversation_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let msgs = rows.into_iter().map(|row| {
            let role_str: String = row.get(2);
            let role: Role = serde_json::from_str(&format!("\"{}\"", role_str)).unwrap();
            Message {
                id: Uuid::parse_str(row.get(0)).unwrap(),
                conversation_id: Uuid::parse_str(row.get(1)).unwrap(),
                role,
                content: row.get(3),
                thinking: row.get(4),
                created_at: row.get(5),
            }
        }).collect();

        Ok(msgs)
    }

    pub async fn delete_profile(&self) -> Result<()> {
        sqlx::query("DELETE FROM user_profiles WHERE id = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
