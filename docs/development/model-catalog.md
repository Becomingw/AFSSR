# Shared Model Catalog

The external model catalog is a shared models.dev snapshot. It is created and
replaced **only by an explicit refresh**; no read ever fetches it. Until the first
refresh (or after the snapshot has been cleared, or lost because the runtime
backend is in-memory and the gateway restarted) `GET /api/admin/models/external`
returns 503 with a hint to refresh, and imports simply find no catalog match. The
snapshot has no time-based expiry. Redis persistence and eviction policies still
apply, so use a persistent runtime backend if the snapshot must survive restarts.
Browsers reuse their memory/localStorage snapshot. Opening the model dialog does
not refresh the upstream catalog.

The refresh button in the global-model dialog calls
`POST /api/admin/models/external/refresh`. This authenticated service-management
operation fetches a replacement before discarding the old snapshot, then updates
existing global models in pages of 200. The response includes the catalog and
`sync.updated`, `unchanged`, `unmatched`, and failed model IDs. A failed fetch
preserves the previous catalog. Per-channel prices, aliases, access permissions,
activation flags and custom settings are not reset. Unsupported or missing prices
never overwrite existing global prices. Egress configuration changes take effect
on the next explicit refresh.

Provider upstream imports use the same indexed matching rules: a pinned source,
an exact model ID, a provider-qualified ID, or one unambiguous official source.
Similar names are not fuzzy-matched. Source provider/model IDs are retained in
`config.models_dev_pricing_source`; this also allows refreshes of public aliases.
The importer creates/reuses the global model and associates it with the provider.
Duplicate input IDs are ignored; requests are limited to 1,000 IDs.

Saving a key's model permissions (provider key dialog) runs the same importer for
every selected model that is not yet associated with the provider: the model is
matched against the stored catalog, the global model is created or reused, and
the provider association is added, so it appears in Model Management. Models that
cannot be matched or priced are created disabled and flagged as pending. Linking
is additive only; deselecting a model never removes an existing association.

Global models that have had no provider association for a long time are removed
automatically by a sweep that runs every six hours. A model found without
associations is stamped with `config.orphaned_since`; it is deleted only if it is
still unassociated once the retention period has passed, and the stamp is removed
as soon as it is associated again. The retention period is the system config
`global_model_orphan_retention_days` (default 30; `0` disables the sweep).

Imported metadata includes token/context limits, modalities, tool/reasoning
capabilities, release information, context/cache token prices, and processing-tier
prices when representable. `catalog_api_formats` is a native API hint, not a
verified compatibility result and not a replacement for channel endpoint routing.
Global display names and custom configuration are preserved on refresh.

Unmatched models receive a 32,768-token context / 4,096-token output placeholder
and `catalog_metadata_status=unmatched`. If no explicit price was supplied,
unmatched or unpriceable imports remain disabled until their price is completed;
unknown prices are not treated as free usage. The import response and UI report
these pending models. Explicitly configured zero prices remain valid.

MCP exposes `models_external_refresh` with the same authentication and mutation
permissions as the HTTP operation.

Compatibility profiles can select a provider, associated model and endpoint.
Their IDs are retained in `config.channel` and included in the fingerprint.
The current workbench remains offline synthetic protocol evidence: selecting a
channel does not send requests using its API key, apply its live network route,
or prove real upstream/client compatibility.
