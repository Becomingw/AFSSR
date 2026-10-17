use super::*;

impl GatewayDataState {
    // Deliberately no in-memory fallback: an unavailable DB must never report a saved run.
    pub(crate) async fn save_compatibility_snapshot(
        &self,
        expected: Option<&serde_json::Value>,
        replacement: &serde_json::Value,
    ) -> Result<bool, DataLayerError> {
        let Some(backends) = self.backends.as_ref() else {
            return Err(DataLayerError::sql("Compatibility persistence unavailable"));
        };
        backends
            .compare_and_swap_system_config_json(
                crate::api::compatibility::CONFIG_KEY,
                expected,
                replacement,
            )
            .await
    }
}
