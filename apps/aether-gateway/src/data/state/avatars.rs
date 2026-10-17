impl super::GatewayDataState {
    pub(crate) fn avatar_repository(
        &self,
    ) -> Option<std::sync::Arc<dyn aether_data::repository::avatars::UserAvatarRepository>> {
        self.backends.as_ref().and_then(|backend| backend.avatars())
    }
    #[cfg(test)]
    pub(crate) fn with_avatar_repository_for_tests(
        mut self,
        repository: std::sync::Arc<dyn aether_data::repository::avatars::UserAvatarRepository>,
    ) -> Self {
        self.backends = Some(
            self.backends
                .take()
                .unwrap_or_default()
                .with_avatar_repository(repository),
        );
        self
    }
}
