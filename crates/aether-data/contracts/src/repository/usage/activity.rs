use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct UsageActivityQuery {
    pub from: u64,
    pub until: u64,
    pub tz_offset_minutes: i32,
}

impl UsageActivityQuery {
    pub fn validate(&self) -> Result<(), crate::DataLayerError> {
        if self.until <= self.from
            || self.until - self.from > 31 * 86400
            || !(-720..=840).contains(&self.tz_offset_minutes)
            || self.tz_offset_minutes % 15 != 0
        {
            return Err(crate::DataLayerError::InvalidInput(
                "Activity range must be 1-31 days with a quarter-hour UTC offset".into(),
            ));
        }
        Ok(())
    }
}

/// Local calendar day index and hour; no prompts, keys or request bodies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageActivityRow {
    pub user_id: String,
    pub day: i64,
    pub hour: i64,
    pub requests: u64,
    pub active_minutes: u64,
    pub first_at: u64,
}
