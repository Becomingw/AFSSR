use super::super::internal;
use crate::admin_api;
use crate::audit::attach_admin_audit_event;
use crate::control::{
    validate_admin_role_route_permission, validate_management_token_admin_route_permission,
    GatewayPublicRequestContext,
};
use crate::{AppState, GatewayError};
use axum::body::{Body, Bytes};
use axum::http::{self, Response};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tracing::warn;

pub(super) async fn maybe_build_local_internal_proxy_response(
    state: &AppState,
    request_context: &GatewayPublicRequestContext,
    remote_addr: &std::net::SocketAddr,
    request_headers: &http::HeaderMap,
    request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    internal::maybe_build_local_internal_proxy_response_impl(
        state,
        request_context,
        remote_addr,
        request_headers,
        request_body,
    )
    .await
}

pub(crate) async fn maybe_build_local_admin_proxy_response(
    state: &AppState,
    request_context: &GatewayPublicRequestContext,
    remote_addr: &std::net::SocketAddr,
    request_headers: &http::HeaderMap,
    request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    let Some(decision) = request_context.control_decision.as_ref() else {
        return Ok(None);
    };
    if decision.route_class.as_deref() != Some("admin_proxy") {
        return Ok(None);
    }
    if decision.admin_principal.is_none() {
        return Ok(None);
    }
    if let Some(response) = maybe_build_management_token_permission_denied_response(request_context)
    {
        return Ok(Some(response));
    }
    let principal = decision
        .admin_principal
        .as_ref()
        .expect("admin principal checked");
    let sensitive_user_action = !matches!(
        request_context.request_method,
        http::Method::GET | http::Method::HEAD | http::Method::OPTIONS
    ) || decision.route_kind.as_deref() == Some("reveal_user_api_key");
    if sensitive_user_action && !crate::roles::is_full_admin_role(&principal.user_role) {
        if let Some(user_id) = decision
            .public_path
            .strip_prefix("/api/admin/users/")
            .and_then(|path| path.split('/').next())
        {
            if state
                .find_user_auth_by_id(user_id)
                .await?
                .is_some_and(|user| crate::roles::can_access_admin_console(&user.role))
            {
                return Ok(Some((http::StatusCode::FORBIDDEN, Json(json!({"detail":"Only a super administrator may manage administrator accounts and credentials"}))).into_response()));
            }
        }
    }

    if let Some(response) = crate::api::diagnostics::admin_response(state, request_context).await? {
        return Ok(Some(response));
    }
    if let Some(response) =
        crate::api::access_preview::admin_response(state, request_context, request_body).await?
    {
        return Ok(Some(response));
    }
    if let Some(response) =
        crate::api::compatibility::admin_response(state, request_context, request_body).await?
    {
        return Ok(Some(response));
    }
    if let Some(response) =
        crate::api::feedback::admin_response(state, request_context, request_body).await?
    {
        return Ok(Some(response));
    }
    admin_api::maybe_build_local_admin_response(admin_api::AdminRouteRequest::new(
        state,
        request_context,
        remote_addr,
        request_headers,
        request_body,
    ))
    .await
}

fn maybe_build_management_token_permission_denied_response(
    request_context: &GatewayPublicRequestContext,
) -> Option<Response<Body>> {
    let decision = request_context.control_decision.as_ref()?;
    let admin_principal = decision.admin_principal.as_ref()?;
    let denied = validate_admin_role_route_permission(&request_context.request_method, decision)
        .and_then(|_| {
            validate_management_token_admin_route_permission(
                &request_context.request_method,
                decision,
                admin_principal.management_token_permissions.as_deref(),
            )
        })
        .err()?;
    let actor_id = admin_principal
        .management_token_id
        .as_deref()
        .unwrap_or(admin_principal.user_id.as_str());

    warn!(
        trace_id = %request_context.trace_id,
        admin_actor_id = %actor_id,
        admin_user_role = %admin_principal.user_role,
        route_family = decision.route_family.as_deref().unwrap_or("unknown"),
        route_kind = decision.route_kind.as_deref().unwrap_or("unknown"),
        required_permission = %denied.required_permission,
        "admin route permission denied"
    );

    let mut response = (
        http::StatusCode::FORBIDDEN,
        Json(json!({
            "detail": "management token permission denied",
            "required_permission": denied.required_permission,
            "route_family": decision.route_family.as_deref(),
            "route_kind": decision.route_kind.as_deref(),
            "request_path": request_context.request_path,
        })),
    )
        .into_response();
    attach_admin_audit_event(
        &mut response,
        "admin_route_permission_denied",
        "permission_denied",
        "admin_route_permission",
        actor_id,
    );
    Some(response)
}

#[cfg(test)]
mod scoped_admin_tests {
    use super::*;
    use crate::control::{classify_control_route, GatewayAdminPrincipalContext};

    fn context(role: &str, method: http::Method, path: &str) -> GatewayPublicRequestContext {
        let uri = path.parse().unwrap();
        let mut context = GatewayPublicRequestContext::from_request_parts(
            "role-test",
            &method,
            &uri,
            &http::HeaderMap::new(),
            None,
        );
        let mut decision = classify_control_route(&method, &uri, &http::HeaderMap::new()).unwrap();
        decision.admin_principal = Some(GatewayAdminPrincipalContext {
            user_id: "role-operator".into(),
            user_role: role.into(),
            session_id: Some("fixture".into()),
            management_token_id: None,
            management_token_permissions: None,
        });
        context.control_decision = Some(decision);
        context
    }

    async fn call(
        state: &AppState,
        role: &str,
        method: http::Method,
        path: &str,
        body: serde_json::Value,
    ) -> http::StatusCode {
        maybe_build_local_admin_proxy_response(
            state,
            &context(role, method, path),
            &"127.0.0.1:12345".parse().unwrap(),
            &http::HeaderMap::new(),
            Some(&Bytes::from(serde_json::to_vec(&body).unwrap())),
        )
        .await
        .unwrap()
        .unwrap()
        .status()
    }

    #[tokio::test]
    async fn scoped_admin_dispatch_rejects_cross_domain_and_account_escalation_before_mutation() {
        use aether_data::repository::users::StoredUserAuthRecord;
        let user = |id: &str, role: &str| {
            StoredUserAuthRecord::new(
                id.into(),
                None,
                false,
                id.into(),
                None,
                role.into(),
                "local".into(),
                None,
                None,
                None,
                true,
                false,
                None,
                None,
            )
            .unwrap()
        };
        let state = AppState::new()
            .unwrap()
            .with_auth_users_for_tests([user("ordinary", "user"), user("root-admin", "admin")])
            .with_data_state_for_tests(
                crate::data::GatewayDataState::disabled()
                    .with_system_config_values_for_tests(Vec::<(String, serde_json::Value)>::new()),
            );
        assert_eq!(
            call(
                &state,
                "service_admin",
                http::Method::PUT,
                "/api/admin/users/ordinary",
                json!({"is_active":false})
            )
            .await,
            http::StatusCode::FORBIDDEN
        );
        assert!(
            state
                .find_user_auth_by_id("ordinary")
                .await
                .unwrap()
                .unwrap()
                .is_active
        );
        assert_eq!(
            call(
                &state,
                "user_admin",
                http::Method::PUT,
                "/api/admin/users/ordinary",
                json!({"is_active":false})
            )
            .await,
            http::StatusCode::OK
        );
        assert!(
            !state
                .find_user_auth_by_id("ordinary")
                .await
                .unwrap()
                .unwrap()
                .is_active
        );
        for target_role in ["admin", "audit_admin", "user_admin", "service_admin"] {
            assert_eq!(
                call(
                    &state,
                    "user_admin",
                    http::Method::PUT,
                    "/api/admin/users/ordinary",
                    json!({"role":target_role})
                )
                .await,
                http::StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            state
                .find_user_auth_by_id("ordinary")
                .await
                .unwrap()
                .unwrap()
                .role,
            "user"
        );
        for (method, path) in [
            (http::Method::PUT, "/api/admin/users/root-admin"),
            (http::Method::POST, "/api/admin/users/root-admin/api-keys"),
        ] {
            assert_eq!(
                call(&state, "user_admin", method, path, json!({})).await,
                http::StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            call(
                &state,
                "user_admin",
                http::Method::POST,
                "/api/admin/providers",
                json!({})
            )
            .await,
            http::StatusCode::FORBIDDEN
        );
        let key = "/api/admin/system/configs/internal_access_policy";
        let policy = json!({"value":{"default":{"monthly_budget_usd":10,"models":{}}},"expected_value":null});
        assert_eq!(
            call(
                &state,
                "service_admin",
                http::Method::PUT,
                key,
                policy.clone()
            )
            .await,
            http::StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(&state, "user_admin", http::Method::PUT, key, policy).await,
            http::StatusCode::OK
        );
        assert_eq!(
            call(&state, "audit_admin", http::Method::GET, key, json!({})).await,
            http::StatusCode::OK
        );
        assert_eq!(
            call(&state, "audit_admin", http::Method::PUT, key, json!({})).await,
            http::StatusCode::FORBIDDEN
        );
    }
}
