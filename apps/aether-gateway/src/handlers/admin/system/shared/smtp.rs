use crate::email_delivery::{
    probe_smtp_connection, send_smtp_email, system_config_u16, ComposedEmail, SmtpDeliveryConfig,
};
use crate::email_layout::{render_email_html, EmailBlock};
use crate::handlers::admin::request::AdminAppState;
use crate::handlers::shared::{
    decrypt_or_migrate_smtp_password, smtp_password_binding, system_config_bool,
    system_config_string,
};
use crate::GatewayError;
use axum::body::Bytes;
use serde::Deserialize;
use serde_json::json;

#[derive(Default, Deserialize)]
struct AdminSmtpTestRequest {
    smtp_host: Option<serde_json::Value>,
    smtp_port: Option<serde_json::Value>,
    smtp_user: Option<serde_json::Value>,
    smtp_password: Option<serde_json::Value>,
    smtp_use_tls: Option<serde_json::Value>,
    smtp_use_ssl: Option<serde_json::Value>,
    smtp_from_email: Option<serde_json::Value>,
    smtp_from_name: Option<serde_json::Value>,
    /// When set, a real message is sent to this address instead of only logging in.
    send_to: Option<String>,
}

#[derive(Clone)]
struct ResolvedSmtpConfig {
    host: Option<String>,
    port: u16,
    user: Option<String>,
    password: Option<String>,
    use_tls: bool,
    use_ssl: bool,
    from_email: Option<String>,
    #[allow(dead_code)]
    from_name: String,
}

pub(crate) async fn build_admin_smtp_test_payload(
    state: &AdminAppState<'_>,
    request_body: Option<&Bytes>,
) -> Result<serde_json::Value, GatewayError> {
    let request = match request_body {
        Some(body) if !body.is_empty() => serde_json::from_slice::<AdminSmtpTestRequest>(body)
            .map_err(|err| GatewayError::Internal(err.to_string()))?,
        _ => AdminSmtpTestRequest::default(),
    };
    let send_to = request
        .send_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let config = resolve_admin_smtp_config(state, request).await?;
    let missing_fields = missing_smtp_fields(&config);
    if !missing_fields.is_empty() {
        return Ok(json!({
            "success": false,
            "message": format!("SMTP 配置不完整，请检查 {}", missing_fields.join(", ")),
        }));
    }

    if let Some(to_email) = send_to {
        if !plausible_address(&to_email) {
            return Ok(json!({ "success": false, "message": "收件邮箱格式不正确" }));
        }
        let delivery = config.into_delivery_config();
        let message = compose_test_email(&delivery, &to_email);
        return Ok(match send_smtp_email(delivery, message).await {
            Ok(()) => json!({
                "success": true,
                "message": format!("测试邮件已被 SMTP 服务器接受，收件人 {to_email}。若收件箱没有，请查看垃圾邮件箱。"),
            }),
            Err(error) => json!({
                "success": false,
                "message": format!("发送失败：{}", translate_smtp_error(&smtp_gateway_error_message(&error))),
            }),
        });
    }

    let result = probe_smtp_connection(config.into_delivery_config()).await;
    Ok(match result {
        Ok(()) => json!({ "success": true, "message": "SMTP 连接测试成功" }),
        Err(error) => {
            json!({ "success": false, "message": translate_smtp_error(&smtp_gateway_error_message(&error)) })
        }
    })
}

/// Deliberately loose: the SMTP server is the authority; this only catches obvious typos.
fn plausible_address(value: &str) -> bool {
    value.len() <= 320
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ','))
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        })
}

fn compose_test_email(config: &SmtpDeliveryConfig, to_email: &str) -> ComposedEmail {
    let sent_at = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    let intro = "这是一封来自网关的测试邮件。收到它说明 SMTP 设置可以真正发信。";
    let blocks = vec![
        EmailBlock::Paragraph(intro.to_string()),
        EmailBlock::Rows(vec![
            ("发件人".into(), config.from_email.clone()),
            (
                "SMTP 服务器".into(),
                format!("{}:{}", config.host, config.port),
            ),
            ("发送时间".into(), sent_at.clone()),
        ]),
    ];
    ComposedEmail {
        to_email: to_email.to_string(),
        subject: format!("[{}] SMTP 测试邮件", config.from_name),
        html_body: render_email_html(
            &config.from_name,
            "SMTP 测试邮件",
            intro,
            &blocks,
            "此邮件由管理员在系统设置中触发，用于检查邮件发送。",
        ),
        text_body: format!(
            "{intro}\n\n发件人：{}\nSMTP 服务器：{}:{}\n发送时间：{sent_at}\n",
            config.from_email, config.host, config.port
        ),
    }
}

async fn resolve_admin_smtp_config(
    state: &AdminAppState<'_>,
    request: AdminSmtpTestRequest,
) -> Result<ResolvedSmtpConfig, GatewayError> {
    let smtp_host = state.read_system_config_json_value("smtp_host").await?;
    let smtp_port = state.read_system_config_json_value("smtp_port").await?;
    let smtp_user = state.read_system_config_json_value("smtp_user").await?;
    let smtp_password = state.read_system_config_json_value("smtp_password").await?;
    let smtp_use_tls = state.read_system_config_json_value("smtp_use_tls").await?;
    let smtp_use_ssl = state.read_system_config_json_value("smtp_use_ssl").await?;
    let smtp_from_email = state
        .read_system_config_json_value("smtp_from_email")
        .await?;
    let smtp_from_name = state
        .read_system_config_json_value("smtp_from_name")
        .await?;

    let host = request
        .smtp_host
        .as_ref()
        .and_then(|value| system_config_string(Some(value)))
        .or_else(|| system_config_string(smtp_host.as_ref()));
    let port = request
        .smtp_port
        .as_ref()
        .map(|value| system_config_u16(Some(value), 587))
        .unwrap_or_else(|| system_config_u16(smtp_port.as_ref(), 587));
    let user = request
        .smtp_user
        .as_ref()
        .and_then(|value| system_config_string(Some(value)))
        .or_else(|| system_config_string(smtp_user.as_ref()));
    let use_tls = request
        .smtp_use_tls
        .as_ref()
        .map(|value| system_config_bool(Some(value), true))
        .unwrap_or_else(|| system_config_bool(smtp_use_tls.as_ref(), true));
    let use_ssl = request
        .smtp_use_ssl
        .as_ref()
        .map(|value| system_config_bool(Some(value), false))
        .unwrap_or_else(|| system_config_bool(smtp_use_ssl.as_ref(), false));
    let requested_password = request
        .smtp_password
        .as_ref()
        .and_then(|value| system_config_string(Some(value)));
    let password = if requested_password.is_some() {
        requested_password
    } else {
        let saved_binding = system_config_string(smtp_host.as_ref()).and_then(|saved_host| {
            smtp_password_binding(
                &saved_host,
                system_config_u16(smtp_port.as_ref(), 587),
                system_config_string(smtp_user.as_ref()).as_deref(),
                system_config_bool(smtp_use_tls.as_ref(), true),
                system_config_bool(smtp_use_ssl.as_ref(), false),
            )
        });
        let effective_binding = host.as_deref().and_then(|effective_host| {
            smtp_password_binding(effective_host, port, user.as_deref(), use_tls, use_ssl)
        });
        match (saved_binding.as_ref(), effective_binding.as_ref()) {
            (Some(saved), Some(effective)) if saved == effective => {
                match system_config_string(smtp_password.as_ref()) {
                    Some(value) => Some(
                        decrypt_or_migrate_smtp_password(state.as_ref(), effective, value).await?,
                    ),
                    None => None,
                }
            }
            _ => None,
        }
    };

    Ok(ResolvedSmtpConfig {
        host,
        port,
        user,
        password,
        use_tls,
        use_ssl,
        from_email: request
            .smtp_from_email
            .as_ref()
            .and_then(|value| system_config_string(Some(value)))
            .or_else(|| system_config_string(smtp_from_email.as_ref())),
        from_name: request
            .smtp_from_name
            .as_ref()
            .and_then(|value| system_config_string(Some(value)))
            .or_else(|| system_config_string(smtp_from_name.as_ref()))
            .unwrap_or_else(|| "Arca".to_string()),
    })
}

impl ResolvedSmtpConfig {
    fn into_delivery_config(self) -> SmtpDeliveryConfig {
        SmtpDeliveryConfig {
            host: self.host.unwrap_or_default(),
            port: self.port,
            user: self.user,
            password: self.password,
            use_tls: self.use_tls,
            use_ssl: self.use_ssl,
            from_email: self.from_email.unwrap_or_default(),
            from_name: self.from_name,
        }
    }
}

fn missing_smtp_fields(config: &ResolvedSmtpConfig) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if config
        .host
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        fields.push("smtp_host");
    }
    if config
        .user
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        fields.push("smtp_user");
    }
    if config
        .password
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        fields.push("smtp_password");
    }
    if config
        .from_email
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        fields.push("smtp_from_email");
    }
    fields
}

fn smtp_gateway_error_message(error: &GatewayError) -> String {
    match error {
        GatewayError::Internal(message) => message.clone(),
        _ => format!("{error:?}"),
    }
}

fn translate_smtp_error(error: &str) -> String {
    let error_lower = error.to_ascii_lowercase();

    if error_lower.contains("username and password not accepted") {
        return "用户名或密码错误，请检查 SMTP 凭据".to_string();
    }
    if error_lower.contains("authentication failed")
        || error_lower.contains("auth") && error_lower.contains("535")
    {
        return "认证失败，请检查用户名和密码".to_string();
    }
    if error_lower.contains("invalid credentials") || error_lower.contains("badcredentials") {
        return "凭据无效，请检查用户名和密码".to_string();
    }
    if error_lower.contains("smtp auth extension is not supported") {
        return "服务器不支持认证，请尝试使用 TLS 或 SSL 加密".to_string();
    }
    if error_lower.contains("connection refused") || error_lower.contains("os error 61") {
        return "连接被拒绝，请检查服务器地址和端口".to_string();
    }
    if error_lower.contains("connection timed out")
        || error_lower.contains("timed out")
        || error_lower.contains("operation timed out")
    {
        return "连接超时，请检查网络或服务器地址".to_string();
    }
    if error_lower.contains("name or service not known")
        || error_lower.contains("getaddrinfo failed")
        || error_lower.contains("nodename nor servname provided")
        || error_lower.contains("failed to lookup address information")
    {
        return "无法解析服务器地址，请检查 SMTP 服务器地址".to_string();
    }
    if error_lower.contains("network is unreachable") {
        return "网络不可达，请检查网络连接".to_string();
    }
    if error_lower.contains("certificate") && error_lower.contains("verify") {
        return "SSL 证书验证失败，请检查服务器证书或尝试其他加密方式".to_string();
    }
    if error_lower.contains("ssl") && error_lower.contains("wrong version") {
        return "SSL 版本不匹配，请尝试其他加密方式".to_string();
    }
    if error_lower.contains("starttls") {
        return "STARTTLS 握手失败，请检查加密设置".to_string();
    }
    if error_lower.contains("sender address rejected") {
        return "发件人地址被拒绝，请检查发件人邮箱设置".to_string();
    }
    if error_lower.contains("relay access denied") {
        return "中继访问被拒绝，请检查 SMTP 服务器配置".to_string();
    }

    error.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recipient_check_catches_typos_only() {
        for good in ["a@b.co", "first.last+tag@sub.example.org"] {
            assert!(plausible_address(good), "{good}");
        }
        for bad in [
            "",
            "plain",
            "a@b",
            "@b.co",
            "a@.co",
            "a@b.",
            "a b@c.de",
            "a@b.co,c@d.ee",
            "<a@b.co>",
            "a@b.co\r\nRCPT TO:<x@y.zz>",
        ] {
            assert!(!plausible_address(bad), "{bad:?}");
        }
    }

    #[test]
    fn test_email_names_the_server_and_renders_both_bodies() {
        let config = SmtpDeliveryConfig {
            host: "smtp.example.com".into(),
            port: 587,
            user: Some("u".into()),
            password: Some("p".into()),
            use_tls: true,
            use_ssl: false,
            from_email: "ops@example.com".into(),
            from_name: "Arca".into(),
        };
        let email = compose_test_email(&config, "me@example.org");
        assert_eq!(email.to_email, "me@example.org");
        assert!(email.subject.contains("SMTP 测试邮件"));
        assert!(email.text_body.contains("smtp.example.com:587"));
        assert!(email.html_body.contains("ops@example.com"));
        assert!(!email.html_body.contains("password"));
    }

    #[test]
    fn reports_missing_python_required_fields() {
        let config = ResolvedSmtpConfig {
            host: None,
            port: 587,
            user: None,
            password: None,
            use_tls: true,
            use_ssl: false,
            from_email: None,
            from_name: "Aether".to_string(),
        };
        assert_eq!(
            missing_smtp_fields(&config),
            vec!["smtp_host", "smtp_user", "smtp_password", "smtp_from_email"]
        );
    }

    #[test]
    fn translates_common_smtp_errors() {
        assert_eq!(
            translate_smtp_error("connection refused"),
            "连接被拒绝，请检查服务器地址和端口"
        );
        assert_eq!(
            translate_smtp_error("535 authentication failed"),
            "认证失败，请检查用户名和密码"
        );
        assert_eq!(
            translate_smtp_error("nodename nor servname provided"),
            "无法解析服务器地址，请检查 SMTP 服务器地址"
        );
    }
}
