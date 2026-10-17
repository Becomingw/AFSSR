use axum::http;

use super::{classified, ClassifiedRoute};

#[path = "admin/basic_families.rs"]
mod basic_families;
#[path = "admin/endpoints_families.rs"]
mod endpoints_families;
#[path = "admin/model_provider_families.rs"]
mod model_provider_families;
#[path = "admin/observability_families.rs"]
mod observability_families;
#[path = "admin/operations_families.rs"]
mod operations_families;
#[path = "admin/provider_ops_routes.rs"]
mod provider_ops_routes;
#[path = "admin/routing_families.rs"]
mod routing_families;
#[path = "admin/system_families.rs"]
mod system_families;

use basic_families::classify_admin_basic_family_route;
use endpoints_families::classify_admin_endpoints_family_route;
use model_provider_families::classify_admin_model_provider_family_route;
use observability_families::classify_admin_observability_family_route;
use operations_families::classify_admin_operations_family_route;
use provider_ops_routes::classify_admin_provider_ops_routes;
use routing_families::classify_admin_routing_family_route;
use system_families::classify_admin_system_family_route;

pub(super) fn classify_admin_route(
    method: &http::Method,
    normalized_path: &str,
) -> Option<ClassifiedRoute> {
    let normalized_path_no_trailing = normalized_path.trim_end_matches('/');
    let normalized_path_no_trailing = if normalized_path_no_trailing.is_empty() {
        "/"
    } else {
        normalized_path_no_trailing
    };

    if method == http::Method::POST && normalized_path_no_trailing == "/api/admin/access-preview" {
        Some(classified(
            "admin_proxy",
            "access_preview",
            "access_preview",
            "admin:users",
            false,
        ))
    } else if let Some(route) = classify_compatibility(method, normalized_path_no_trailing) {
        Some(route)
    } else if let Some(route) = classify_feedback(method, normalized_path_no_trailing) {
        Some(route)
    } else if method == http::Method::GET
        && matches!(
            normalized_path,
            "/api/admin/providers" | "/api/admin/providers/"
        )
    {
        Some(classified(
            "admin_proxy",
            "providers_manage",
            "list_providers",
            "admin:providers",
            false,
        ))
    } else if let Some(route) =
        classify_admin_basic_family_route(method, normalized_path, normalized_path_no_trailing)
    {
        Some(route)
    } else if let Some(route) = classify_admin_observability_family_route(
        method,
        normalized_path,
        normalized_path_no_trailing,
    ) {
        Some(route)
    } else if let Some(route) =
        classify_admin_operations_family_route(method, normalized_path, normalized_path_no_trailing)
    {
        Some(route)
    } else if let Some(route) =
        classify_admin_system_family_route(method, normalized_path, normalized_path_no_trailing)
    {
        Some(route)
    } else if let Some(route) =
        classify_admin_routing_family_route(method, normalized_path_no_trailing)
    {
        Some(route)
    } else if let Some(route) = classify_admin_provider_ops_routes(method, normalized_path) {
        Some(route)
    } else if let Some(route) = classify_admin_model_provider_family_route(method, normalized_path)
    {
        Some(route)
    } else {
        classify_admin_endpoints_family_route(method, normalized_path)
    }
}

fn classify_compatibility(method: &http::Method, path: &str) -> Option<ClassifiedRoute> {
    let kind = match (method, path) {
        (&http::Method::GET, "/api/admin/compatibility") => "compatibility_list",
        (&http::Method::POST, "/api/admin/compatibility/profiles") => "compatibility_profile",
        (&http::Method::POST, "/api/admin/compatibility/runs") => "compatibility_run",
        _ => return None,
    };
    Some(classified(
        "admin_proxy",
        "compatibility_manage",
        kind,
        "admin:compatibility",
        false,
    ))
}

/// A feedback id is a UUID or the human-friendly issue number (`#N`).
fn feedback_id_is_valid(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
        || (!id.is_empty() && id.len() <= 9 && id.chars().all(|c| c.is_ascii_digit()))
}

fn classify_feedback(method: &http::Method, path: &str) -> Option<ClassifiedRoute> {
    let tail = path.strip_prefix("/api/admin/feedback")?;
    let parts: Vec<_> = tail
        .strip_prefix('/')
        .unwrap_or(tail)
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let kind = match (method, parts.as_slice()) {
        (&http::Method::GET, []) if tail.is_empty() => "feedback_list",
        (&http::Method::POST, []) if tail.is_empty() => "feedback_create",
        (&http::Method::GET, ["mentionable"]) => "feedback_detail",
        (&http::Method::GET, [id]) if feedback_id_is_valid(id) => "feedback_detail",
        (&http::Method::POST, [id, "commands"]) if feedback_id_is_valid(id) => "feedback_command",
        _ => return None,
    };
    Some(classified(
        "admin_proxy",
        "feedback_manage",
        kind,
        "admin:feedback",
        false,
    ))
}
