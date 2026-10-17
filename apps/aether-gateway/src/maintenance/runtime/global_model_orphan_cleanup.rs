use aether_data_contracts::repository::global_models::{
    AdminGlobalModelListQuery, StoredAdminGlobalModel, UpdateAdminGlobalModelRecord,
};
use aether_data_contracts::DataLayerError;
use serde_json::{json, Value};

use crate::{AppState, GatewayError};

use super::config::system_config_u64;
use super::now_unix_secs;

const ORPHANED_SINCE_KEY: &str = "orphaned_since";
const RETENTION_DAYS_CONFIG_KEY: &str = "global_model_orphan_retention_days";
const DEFAULT_RETENTION_DAYS: u64 = 30;
const PAGE_SIZE: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct GlobalModelOrphanCleanupSummary {
    pub(crate) marked: usize,
    pub(crate) cleared: usize,
    pub(crate) deleted: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OrphanAction {
    Keep,
    /// First time the model is seen without any provider association.
    Mark,
    /// The model got a provider association again after being marked.
    Clear,
    /// The model stayed unassociated for the whole retention period.
    Delete,
}

pub(super) fn plan_orphan_action(
    provider_count: u64,
    orphaned_since: Option<u64>,
    now_unix_secs: u64,
    retention_secs: u64,
) -> OrphanAction {
    match (provider_count, orphaned_since) {
        (0, None) => OrphanAction::Mark,
        (0, Some(since)) if now_unix_secs >= since.saturating_add(retention_secs) => {
            OrphanAction::Delete
        }
        (0, Some(_)) => OrphanAction::Keep,
        (_, Some(_)) => OrphanAction::Clear,
        (_, None) => OrphanAction::Keep,
    }
}

fn orphaned_since(model: &StoredAdminGlobalModel) -> Option<u64> {
    model
        .config
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|config| config.get(ORPHANED_SINCE_KEY))
        .and_then(Value::as_u64)
}

fn config_with_orphan_marker(config: Option<&Value>, marker: Option<u64>) -> Option<Value> {
    let mut object = config
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    match marker {
        Some(since) => {
            object.insert(ORPHANED_SINCE_KEY.to_string(), json!(since));
        }
        None => {
            object.remove(ORPHANED_SINCE_KEY);
        }
    }
    if object.is_empty() {
        None
    } else {
        Some(Value::Object(object))
    }
}

async fn write_marker(
    app: &AppState,
    model: &StoredAdminGlobalModel,
    marker: Option<u64>,
) -> Result<(), GatewayError> {
    let record = UpdateAdminGlobalModelRecord::new(
        model.id.clone(),
        model.display_name.clone(),
        model.is_active,
        model.default_price_per_request,
        model.default_tiered_pricing.clone(),
        model.supported_capabilities.clone(),
        config_with_orphan_marker(model.config.as_ref(), marker),
    )
    .map_err(|err| GatewayError::Internal(err.to_string()))?;
    app.update_admin_global_model(&record).await?;
    Ok(())
}

/// Removes global models that have had no provider association for a long time.
///
/// A model without associations is first stamped with `config.orphaned_since`; it is deleted
/// only if it is still unassociated once the retention period (system config
/// `global_model_orphan_retention_days`, default 30, `0` disables the cleanup) has passed. The
/// stamp is removed again as soon as the model is associated with a provider.
pub(crate) async fn cleanup_orphaned_global_models_once(
    app: &AppState,
) -> Result<GlobalModelOrphanCleanupSummary, DataLayerError> {
    let mut summary = GlobalModelOrphanCleanupSummary::default();
    if !app.data.has_global_model_reader() || !app.data.has_global_model_writer() {
        return Ok(summary);
    }
    let retention_days =
        system_config_u64(&app.data, RETENTION_DAYS_CONFIG_KEY, DEFAULT_RETENTION_DAYS).await?;
    if retention_days == 0 {
        return Ok(summary);
    }
    let retention_secs = retention_days.saturating_mul(24 * 60 * 60);
    let now = now_unix_secs();

    let mut offset = 0usize;
    loop {
        let page = app
            .list_admin_global_models(&AdminGlobalModelListQuery {
                offset,
                limit: PAGE_SIZE,
                is_active: None,
                search: None,
            })
            .await
            .map_err(|err| DataLayerError::UnexpectedValue(format!("{err:?}")))?;
        if page.items.is_empty() {
            break;
        }
        offset += page.items.len();
        for model in &page.items {
            let action = plan_orphan_action(
                model.provider_count,
                orphaned_since(model),
                now,
                retention_secs,
            );
            let outcome = match action {
                OrphanAction::Keep => continue,
                OrphanAction::Mark => write_marker(app, model, Some(now)).await.map(|_| {
                    summary.marked += 1;
                }),
                OrphanAction::Clear => write_marker(app, model, None).await.map(|_| {
                    summary.cleared += 1;
                }),
                OrphanAction::Delete => {
                    app.delete_admin_global_model(&model.id)
                        .await
                        .map(|deleted| {
                            if deleted {
                                summary.deleted += 1;
                            }
                        })
                }
            };
            if let Err(err) = outcome {
                // One bad row must not stop the sweep; it is retried on the next run.
                tracing::warn!(
                    event_name = "global_model_orphan_cleanup_item_failed",
                    log_type = "ops",
                    global_model_id = %model.id,
                    error = ?err,
                    "failed to update an orphaned global model"
                );
            }
        }
        if offset >= page.total {
            break;
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;

    #[test]
    fn first_sighting_without_providers_only_marks() {
        assert_eq!(
            plan_orphan_action(0, None, 1_000, 30 * DAY),
            OrphanAction::Mark
        );
    }

    #[test]
    fn marked_model_is_kept_until_the_retention_period_has_passed() {
        let since = 1_000;
        assert_eq!(
            plan_orphan_action(0, Some(since), since + 30 * DAY - 1, 30 * DAY),
            OrphanAction::Keep
        );
        assert_eq!(
            plan_orphan_action(0, Some(since), since + 30 * DAY, 30 * DAY),
            OrphanAction::Delete
        );
    }

    #[test]
    fn associated_model_loses_its_marker_and_is_never_deleted() {
        assert_eq!(
            plan_orphan_action(2, Some(5), 10 * DAY, DAY),
            OrphanAction::Clear
        );
        assert_eq!(
            plan_orphan_action(1, None, 10 * DAY, DAY),
            OrphanAction::Keep
        );
    }

    #[test]
    fn marker_is_added_and_removed_without_touching_other_config() {
        let config = json!({"streaming": true, "context_limit": 1000});
        let marked = config_with_orphan_marker(Some(&config), Some(42)).unwrap();
        assert_eq!(marked["orphaned_since"], json!(42));
        assert_eq!(marked["streaming"], json!(true));
        let cleared = config_with_orphan_marker(Some(&marked), None).unwrap();
        assert_eq!(cleared, config);
        assert_eq!(
            config_with_orphan_marker(Some(&json!({"orphaned_since": 1})), None),
            None
        );
    }
}
