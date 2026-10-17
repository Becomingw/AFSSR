impl super::GatewayDataState {
    pub(crate) fn feedback_repository(
        &self,
    ) -> Option<std::sync::Arc<dyn aether_data::repository::feedback::FeedbackRepository>> {
        self.backends
            .as_ref()
            .and_then(|backend| backend.feedback())
    }
    #[cfg(test)]
    pub(crate) fn with_feedback_repository_for_tests(
        mut self,
        repository: std::sync::Arc<dyn aether_data::repository::feedback::FeedbackRepository>,
    ) -> Self {
        self.backends = Some(
            self.backends
                .take()
                .unwrap_or_default()
                .with_feedback_repository(repository),
        );
        self
    }
}
