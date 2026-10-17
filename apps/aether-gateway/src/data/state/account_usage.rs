impl super::GatewayDataState {
    pub(crate) fn account_usage_repository(
        &self,
    ) -> Option<std::sync::Arc<dyn aether_data::repository::account_usage::AccountUsageRepository>>
    {
        self.backends.as_ref().and_then(|b| b.account_usage())
    }
    #[cfg(test)]
    pub(crate) fn with_account_usage_for_tests(
        mut self,
        repository: std::sync::Arc<
            dyn aether_data::repository::account_usage::AccountUsageRepository,
        >,
    ) -> Self {
        self.backends = Some(
            self.backends
                .take()
                .unwrap_or_default()
                .with_account_usage(repository),
        );
        self
    }
}
