use crate::handlers::admin::model::build_admin_model_catalog_payload;
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::handlers::admin::shared::attach_admin_audit_response;
use crate::GatewayError;
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

const ADMIN_MODEL_CATALOG_DATA_UNAVAILABLE_DETAIL: &str = "Admin model catalog data unavailable";

fn build_admin_model_catalog_data_unavailable_response() -> Response<Body> {
    (
        http::StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "detail": ADMIN_MODEL_CATALOG_DATA_UNAVAILABLE_DETAIL })),
    )
        .into_response()
}

pub(crate) async fn maybe_build_local_admin_model_catalog_response(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
    request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    let Some(decision) = request_context.decision() else {
        return Ok(None);
    };

    if decision.route_family.as_deref() == Some("model_catalog_manage")
        && decision.route_kind.as_deref() == Some("catalog")
        && request_context.method() == http::Method::GET
        && request_context.path() == "/api/admin/models/catalog"
    {
        if !state.has_global_model_data_reader() || !state.has_provider_catalog_data_reader() {
            return Ok(Some(build_admin_model_catalog_data_unavailable_response()));
        }
        let Some(payload) = build_admin_model_catalog_payload(state).await else {
            return Ok(Some(build_admin_model_catalog_data_unavailable_response()));
        };
        return Ok(Some(Json(payload).into_response()));
    }

    if decision.route_family.as_deref() == Some("model_external_manage")
        && decision.route_kind.as_deref() == Some("external")
        && request_context.method() == http::Method::GET
        && request_context.path() == "/api/admin/models/external"
    {
        return Ok(Some(
            match state
                .read_admin_external_models_cache(request_context.trace_id())
                .await?
            {
                Some(payload) => Json(payload).into_response(),
                None => (
                    http::StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({
                        "detail": "External models catalog unavailable"
                    })),
                )
                    .into_response(),
            },
        ));
    }

    if decision.route_family.as_deref() == Some("model_external_manage")
        && decision.route_kind.as_deref() == Some("external_config_get")
        && request_context.method() == http::Method::GET
        && request_context.path() == "/api/admin/models/external/config"
    {
        return Ok(Some(
            Json(state.build_admin_external_models_config_payload().await?).into_response(),
        ));
    }

    if decision.route_family.as_deref() == Some("model_external_manage")
        && decision.route_kind.as_deref() == Some("external_config_set")
        && request_context.method() == http::Method::PUT
        && request_context.path() == "/api/admin/models/external/config"
    {
        let Some(request_body) = request_body else {
            return Ok(Some(
                (
                    http::StatusCode::BAD_REQUEST,
                    Json(json!({ "detail": "请求数据验证失败" })),
                )
                    .into_response(),
            ));
        };
        return Ok(Some(
            match state
                .apply_admin_external_models_config_update(request_body)
                .await?
            {
                Ok(payload) => attach_admin_audit_response(
                    Json(payload).into_response(),
                    "admin_external_models_config_updated",
                    "update_external_models_config",
                    "external_models_catalog",
                    "global",
                ),
                Err((status, payload)) => (status, Json(payload)).into_response(),
            },
        ));
    }

    if decision.route_family.as_deref() == Some("model_external_manage")
        && decision.route_kind.as_deref() == Some("refresh_external_catalog")
        && request_context.method() == http::Method::POST
        && request_context.path() == "/api/admin/models/external/refresh"
    {
        return Ok(Some(
            match super::external_cache::refresh_admin_external_models_catalog(
                state,
                request_context.trace_id(),
            )
            .await?
            {
                Ok(payload) => attach_admin_audit_response(
                    Json(payload).into_response(),
                    "admin_external_models_refreshed",
                    "refresh_external_models",
                    "external_models_catalog",
                    "global",
                ),
                Err((status, payload)) => (status, Json(payload)).into_response(),
            },
        ));
    }

    if decision.route_family.as_deref() == Some("model_external_manage")
        && decision.route_kind.as_deref() == Some("clear_external_cache")
        && request_context.method() == http::Method::DELETE
        && request_context.path() == "/api/admin/models/external/cache"
    {
        return Ok(Some(
            Json(state.clear_admin_external_models_cache().await?).into_response(),
        ));
    }

    Ok(None)
}
