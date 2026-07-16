use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

use crate::models::{Conversation, Message, Role, UserProfile};

pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    pub async fn new(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let storage = Self { pool };
        storage.recover_running_runs().await?;
        Ok(storage)
    }

    pub async fn get_profile(&self) -> Result<UserProfile> {
        let row = sqlx::query(
            "SELECT experience, knowledge, platforms, holdings, is_complete
             FROM user_profiles WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(UserProfile::default());
        };

        let holdings = row
            .try_get::<Option<String>, _>("holdings")?
            .map(|value| {
                serde_json::from_str(&value)
                    .context("Stored user profile holdings are not valid JSON")
            })
            .transpose()?;

        Ok(UserProfile {
            experience: row.try_get("experience")?,
            knowledge: row.try_get("knowledge")?,
            platforms: row.try_get("platforms")?,
            holdings,
            is_complete: row.try_get("is_complete")?,
        })
    }

    pub async fn save_profile(&self, profile: &UserProfile) -> Result<()> {
        let holdings_json = serde_json::to_string(profile.holdings.as_deref().unwrap_or(&[]))?;

        sqlx::query(
            "INSERT OR REPLACE INTO user_profiles
             (id, experience, knowledge, platforms, holdings, is_complete)
             VALUES (1, ?, ?, ?, ?, ?)",
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

        sqlx::query("INSERT INTO conversations (id, title, created_at) VALUES (?, ?, ?)")
            .bind(conversation.id.to_string())
            .bind(&conversation.title)
            .bind(conversation.created_at)
            .execute(&self.pool)
            .await?;

        Ok(conversation)
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let rows = sqlx::query(
            "SELECT id, title, created_at
             FROM conversations ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let id: String = row.try_get(0)?;
                Ok(Conversation {
                    id: Uuid::parse_str(&id)
                        .with_context(|| format!("Invalid conversation UUID in storage: {id}"))?,
                    title: row.try_get(1)?,
                    created_at: row.try_get(2)?,
                })
            })
            .collect()
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
            "INSERT INTO messages
             (id, conversation_id, role, content, thinking, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(msg.id.to_string())
        .bind(msg.conversation_id.to_string())
        .bind(role_to_db(&msg.role))
        .bind(&msg.content)
        .bind(&msg.thinking)
        .bind(msg.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_messages(&self, conversation_id: Uuid) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            "SELECT id, conversation_id, role, content, thinking, created_at
             FROM messages WHERE conversation_id = ? ORDER BY created_at ASC",
        )
        .bind(conversation_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let id: String = row.try_get(0)?;
                let stored_conversation_id: String = row.try_get(1)?;
                let role: String = row.try_get(2)?;

                Ok(Message {
                    id: Uuid::parse_str(&id)
                        .with_context(|| format!("Invalid message UUID in storage: {id}"))?,
                    conversation_id: Uuid::parse_str(&stored_conversation_id).with_context(
                        || {
                            format!(
                                "Invalid conversation UUID in message storage: {stored_conversation_id}"
                            )
                        },
                    )?,
                    role: role_from_db(&role)?,
                    content: row.try_get(3)?,
                    thinking: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            })
            .collect()
    }

    async fn recover_running_runs(&self) -> Result<()> {
        sqlx::query(
            "UPDATE runs
             SET status = 'failed',
                 error = COALESCE(error, 'La aplicación se cerró antes de completar la ejecución.'),
                 finished_at = ?
             WHERE status = 'running'",
        )
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn start_run(&self, run_id: Uuid, conversation_id: Uuid) -> Result<()> {
        sqlx::query(
            "INSERT INTO runs
             (id, conversation_id, status, started_at)
             VALUES (?, ?, 'running', ?)",
        )
        .bind(run_id.to_string())
        .bind(conversation_id.to_string())
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn complete_run(&self, run_id: Uuid) -> Result<()> {
        self.finish_run(run_id, "completed", None).await
    }

    pub async fn fail_run(&self, run_id: Uuid, error: &str) -> Result<()> {
        self.finish_run(run_id, "failed", Some(error)).await
    }

    async fn finish_run(&self, run_id: Uuid, status: &str, error: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE runs SET status = ?, error = ?, finished_at = ? WHERE id = ?")
            .bind(status)
            .bind(error)
            .bind(Utc::now())
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_profile(&self) -> Result<()> {
        sqlx::query("DELETE FROM user_profiles WHERE id = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn role_to_db(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn role_from_db(role: &str) -> Result<Role> {
    match role {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        "tool" => Ok(Role::Tool),
        other => Err(anyhow!("Unknown message role in storage: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn storage_should_round_trip_messages_and_runs() {
        let path = std::env::temp_dir().join(format!("stock-agent-test-{}.db", Uuid::new_v4()));
        let database_url = format!("sqlite:{}", path.display());
        let storage = Storage::new(&database_url)
            .await
            .expect("test database should initialize");

        let conversation = storage
            .create_conversation("Test")
            .await
            .expect("conversation should be created");
        storage
            .save_message(Message {
                id: Uuid::new_v4(),
                conversation_id: conversation.id,
                role: Role::User,
                content: "hello".to_string(),
                created_at: Utc::now(),
                thinking: None,
            })
            .await
            .expect("message should be saved");

        let messages = storage
            .get_messages(conversation.id)
            .await
            .expect("messages should be readable");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::User);

        let run_id = Uuid::new_v4();
        storage
            .start_run(run_id, conversation.id)
            .await
            .expect("run should start");
        storage
            .complete_run(run_id)
            .await
            .expect("run should complete");

        drop(storage);
        let _ = std::fs::remove_file(path);
    }
}
