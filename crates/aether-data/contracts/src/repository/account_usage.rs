use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUsageWindow {
    pub model: Option<String>,
    #[serde(default)]
    pub models: Option<Vec<String>>,
    pub from: u64,
    pub until: u64,
}
#[derive(Debug, Clone, Default)]
pub struct AccountUsageAmounts {
    pub finalized_units: u64,
    pub reserved_units: u64,
}
pub fn validate_windows(
    user: &str,
    now: u64,
    windows: &[AccountUsageWindow],
) -> Result<(), crate::DataLayerError> {
    if user.is_empty()
        || user.len() > 128
        || now > 253402300799
        || windows.is_empty()
        || windows.len() > 128
        || windows.iter().any(|w| {
            w.from >= w.until
                || w.until > 253402300799
                || w.until - w.from > 32 * 86400
                || w.model
                    .as_ref()
                    .is_some_and(|m| m.is_empty() || m.len() > 128)
                || w.models.as_ref().is_some_and(|models| {
                    w.model.is_some()
                        || models.is_empty()
                        || models.len() > 256
                        || models
                            .iter()
                            .enumerate()
                            .any(|(i, m)| m.is_empty() || m.len() > 128 || models[..i].contains(m))
                })
        })
    {
        return Err(crate::DataLayerError::InvalidInput(
            "Invalid account usage windows".into(),
        ));
    }
    Ok(())
}
#[async_trait]
pub trait AccountUsageRepository: Send + Sync + std::fmt::Debug {
    async fn read_windows(
        &self,
        user: &str,
        now: u64,
        windows: &[AccountUsageWindow],
    ) -> Result<Vec<AccountUsageAmounts>, crate::DataLayerError>;
}
