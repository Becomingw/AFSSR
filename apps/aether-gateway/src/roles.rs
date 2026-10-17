pub(crate) const ROLE_USER: &str = "user";
pub(crate) const ROLE_ADMIN: &str = "admin";
pub(crate) const ROLE_AUDIT_ADMIN: &str = "audit_admin";
pub(crate) const ROLE_USER_ADMIN: &str = "user_admin";
pub(crate) const ROLE_SERVICE_ADMIN: &str = "service_admin";

pub(crate) fn is_full_admin_role(role: &str) -> bool {
    role.trim().eq_ignore_ascii_case(ROLE_ADMIN)
}

pub(crate) fn is_audit_admin_role(role: &str) -> bool {
    role.trim().eq_ignore_ascii_case(ROLE_AUDIT_ADMIN)
}

pub(crate) fn can_access_admin_console(role: &str) -> bool {
    normalize_assignable_user_role(role).is_some_and(|role| role != ROLE_USER)
}

pub(crate) fn can_manage_users(role: &str) -> bool {
    is_full_admin_role(role) || role.trim().eq_ignore_ascii_case(ROLE_USER_ADMIN)
}

pub(crate) fn can_manage_services(role: &str) -> bool {
    is_full_admin_role(role) || role.trim().eq_ignore_ascii_case(ROLE_SERVICE_ADMIN)
}

pub(crate) fn role_has_management_permission(role: &str, permission: &str) -> bool {
    let Some((scope, access)) = permission
        .strip_prefix("admin:")
        .and_then(|p| p.rsplit_once(':'))
    else {
        return false;
    };
    if !matches!(access, "read" | "write" | "admin") || scope == "unknown" {
        return false;
    }
    if is_full_admin_role(role) {
        return true;
    }
    let role = normalize_assignable_user_role(role);
    if scope == "compatibility" && role.is_some_and(|r| r != ROLE_USER) {
        return access == "read" || (access == "write" && can_manage_services(role.unwrap()));
    }
    if scope == "feedback" && role.is_some_and(|r| r != ROLE_USER) {
        return matches!(access, "read" | "write");
    }
    if role == Some(ROLE_AUDIT_ADMIN) {
        return access == "read";
    }
    let shared_read = matches!(
        scope,
        "monitoring" | "stats" | "usage" | "modules" | "endpoints_health"
    );
    match role {
        Some(ROLE_USER_ADMIN) => {
            matches!(scope, "users" | "api_keys" | "announcements")
                || (access == "read" && (shared_read || matches!(scope, "models" | "providers")))
        }
        Some(ROLE_SERVICE_ADMIN) => {
            matches!(
                scope,
                "adaptive"
                    | "endpoints_manage"
                    | "endpoints_health"
                    | "endpoints_rpm"
                    | "models"
                    | "pool"
                    | "provider_ops"
                    | "provider_oauth"
                    | "provider_query"
                    | "provider_strategy"
                    | "providers"
                    | "proxy_nodes"
                    | "routing_profiles"
                    | "tasks"
                    | "video_tasks"
                    | "gemini_files"
            ) || (access == "read" && shared_read)
        }
        _ => false,
    }
}

pub(crate) fn can_write_admin_console(role: &str) -> bool {
    is_full_admin_role(role)
}

pub(crate) fn normalize_assignable_user_role(role: &str) -> Option<&'static str> {
    match role.trim().to_ascii_lowercase().as_str() {
        ROLE_USER => Some(ROLE_USER),
        ROLE_ADMIN => Some(ROLE_ADMIN),
        ROLE_AUDIT_ADMIN => Some(ROLE_AUDIT_ADMIN),
        ROLE_USER_ADMIN => Some(ROLE_USER_ADMIN),
        ROLE_SERVICE_ADMIN => Some(ROLE_SERVICE_ADMIN),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        can_access_admin_console, can_write_admin_console, normalize_assignable_user_role,
    };

    #[test]
    fn audit_admin_can_access_but_not_write_admin_console() {
        assert!(can_access_admin_console("audit_admin"));
        assert!(!can_write_admin_console("audit_admin"));
    }

    #[test]
    fn assignable_roles_include_audit_admin() {
        assert_eq!(
            normalize_assignable_user_role(" audit_admin "),
            Some("audit_admin")
        );
        assert_eq!(normalize_assignable_user_role("admin"), Some("admin"));
        assert_eq!(normalize_assignable_user_role("user"), Some("user"));
        assert_eq!(normalize_assignable_user_role("owner"), None);
    }

    #[test]
    fn scoped_admin_matrix_never_grants_other_domains_or_global_security() {
        for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
            assert!(can_access_admin_console(role));
            assert_eq!(normalize_assignable_user_role(role), Some(role));
        }
        assert!(!can_access_admin_console("user"));
        for permission in [
            "admin:users:read",
            "admin:users:write",
            "admin:api_keys:admin",
        ] {
            assert!(super::role_has_management_permission(
                "user_admin",
                permission
            ));
            assert!(!super::role_has_management_permission(
                "service_admin",
                permission
            ));
        }
        for permission in [
            "admin:providers:write",
            "admin:pool:admin",
            "admin:proxy_nodes:admin",
        ] {
            assert!(super::role_has_management_permission(
                "service_admin",
                permission
            ));
            assert!(!super::role_has_management_permission(
                "user_admin",
                permission
            ));
        }
        for role in ["audit_admin", "user_admin", "service_admin"] {
            for permission in [
                "admin:system:admin",
                "admin:security:admin",
                "admin:management_tokens:admin",
            ] {
                assert!(!super::role_has_management_permission(role, permission));
                assert!(super::role_has_management_permission("admin", permission));
            }
            assert!(super::role_has_management_permission(
                role,
                "admin:monitoring:read"
            ));
        }
        assert!(!super::role_has_management_permission(
            "audit_admin",
            "admin:users:write"
        ));
        assert!(!super::role_has_management_permission(
            "unknown",
            "admin:users:read"
        ));
        assert!(!super::role_has_management_permission(
            "service_admin",
            "admin:unknown:read"
        ));
    }
}
