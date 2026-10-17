pub use aether_data_contracts::repository::avatars::*;
#[cfg(feature = "postgres")]
pub use aether_data_postgres::PostgresAvatarRepository;

use async_trait::async_trait;
use std::{collections::HashMap, sync::Mutex};

use crate::DataLayerError;

/// In-memory implementation for tests and for builds without PostgreSQL.
#[derive(Default)]
pub struct InMemoryAvatarRepository {
    inner: Mutex<HashMap<String, StoredAvatar>>,
}

impl std::fmt::Debug for InMemoryAvatarRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InMemoryAvatarRepository([contents omitted])")
    }
}

fn unavailable() -> DataLayerError {
    DataLayerError::UnexpectedValue("avatar store lock poisoned".to_string())
}

#[async_trait]
impl UserAvatarRepository for InMemoryAvatarRepository {
    async fn put(
        &self,
        user_id: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<i64, DataLayerError> {
        let mut store = self.inner.lock().map_err(|_| unavailable())?;
        // Two uploads in the same millisecond must still differ, or caches would keep the old one.
        let now = chrono::Utc::now().timestamp_millis();
        let version = store
            .get(user_id)
            .map_or(now, |previous| now.max(previous.version + 1));
        store.insert(
            user_id.to_string(),
            StoredAvatar {
                content_type: content_type.to_string(),
                data: data.to_vec(),
                version,
            },
        );
        Ok(version)
    }
    async fn get(&self, user_id: &str) -> Result<Option<StoredAvatar>, DataLayerError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| unavailable())?
            .get(user_id)
            .cloned())
    }
    async fn delete(&self, user_id: &str) -> Result<bool, DataLayerError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| unavailable())?
            .remove(user_id)
            .is_some())
    }
    async fn versions(&self, user_ids: &[String]) -> Result<HashMap<String, i64>, DataLayerError> {
        let store = self.inner.lock().map_err(|_| unavailable())?;
        Ok(user_ids
            .iter()
            .filter_map(|id| store.get(id).map(|avatar| (id.clone(), avatar.version)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pictures_are_per_user_versioned_and_removable() {
        let repo = InMemoryAvatarRepository::default();
        assert_eq!(repo.get("alice").await.unwrap(), None);
        let first = repo.put("alice", "image/png", b"one").await.unwrap();
        let second = repo.put("alice", "image/png", b"two").await.unwrap();
        assert!(second > first, "a replacement always gets a newer version");
        repo.put("bob", "image/webp", b"bob").await.unwrap();
        assert_eq!(repo.get("alice").await.unwrap().unwrap().data, b"two");
        let versions = repo
            .versions(&["alice".into(), "bob".into(), "nobody".into()])
            .await
            .unwrap();
        assert_eq!(versions.len(), 2, "users without a picture are absent");
        assert_eq!(versions["alice"], second);
        assert!(repo.delete("alice").await.unwrap());
        assert!(!repo.delete("alice").await.unwrap());
        assert_eq!(repo.get("alice").await.unwrap(), None);
        assert!(
            repo.get("bob").await.unwrap().is_some(),
            "other users are untouched"
        );
    }
}
