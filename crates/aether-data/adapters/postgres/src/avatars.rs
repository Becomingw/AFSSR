use aether_data_contracts::repository::avatars::*;
use aether_data_contracts::DataLayerError;
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PostgresAvatarRepository {
    pool: PgPool,
}

impl PostgresAvatarRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn failed(error: sqlx::Error) -> DataLayerError {
    DataLayerError::UnexpectedValue(format!("avatar storage failed: {error}"))
}

#[async_trait]
impl UserAvatarRepository for PostgresAvatarRepository {
    async fn put(
        &self,
        user_id: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<i64, DataLayerError> {
        // GREATEST keeps versions strictly increasing even for two uploads in one millisecond.
        sqlx::query_scalar(
            "INSERT INTO user_avatars(user_id,content_type,data,updated_at) VALUES($1,$2,$3,$4) \
             ON CONFLICT(user_id) DO UPDATE SET content_type=EXCLUDED.content_type,data=EXCLUDED.data,\
             updated_at=GREATEST(EXCLUDED.updated_at,user_avatars.updated_at+1) RETURNING updated_at",
        )
        .bind(user_id)
        .bind(content_type)
        .bind(data)
        .bind(chrono::Utc::now().timestamp_millis())
        .fetch_one(&self.pool)
        .await
        .map_err(failed)
    }
    async fn get(&self, user_id: &str) -> Result<Option<StoredAvatar>, DataLayerError> {
        let row =
            sqlx::query("SELECT content_type,data,updated_at FROM user_avatars WHERE user_id=$1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(failed)?;
        row.map(|row| {
            Ok(StoredAvatar {
                content_type: row.try_get("content_type").map_err(failed)?,
                data: row.try_get("data").map_err(failed)?,
                version: row.try_get("updated_at").map_err(failed)?,
            })
        })
        .transpose()
    }
    async fn delete(&self, user_id: &str) -> Result<bool, DataLayerError> {
        let result = sqlx::query("DELETE FROM user_avatars WHERE user_id=$1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(result.rows_affected() > 0)
    }
    async fn versions(&self, user_ids: &[String]) -> Result<HashMap<String, i64>, DataLayerError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query("SELECT user_id,updated_at FROM user_avatars WHERE user_id=ANY($1)")
            .bind(user_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(failed)?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get("user_id").map_err(failed)?,
                    row.try_get("updated_at").map_err(failed)?,
                ))
            })
            .collect()
    }
}
