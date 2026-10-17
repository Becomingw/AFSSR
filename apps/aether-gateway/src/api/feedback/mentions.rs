//! `@username` mentions in feedback: only administrators can be mentioned, and a mention e-mails
//! them the issue title and the text they were mentioned in.

use crate::email_delivery::{read_smtp_delivery_config, send_smtp_email, ComposedEmail};
use crate::email_layout::{render_email_html, EmailBlock};
use crate::AppState;
use aether_data::repository::feedback::{extract_mentions, is_feedback_admin, FeedbackSummary};
use std::collections::BTreeSet;

const MAX_MENTIONS_PER_MESSAGE: usize = 10;
const MAX_QUOTED_CHARS: usize = 3000;

/// How the mentioning text was written; only used for the wording of the e-mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MentionSource {
    Created,
    Reply,
    StatusComment,
}

pub(super) fn public_base_url() -> Option<String> {
    ["AETHER_PUBLIC_BASE_URL", "PUBLIC_BASE_URL"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .find(|value| value.starts_with("http://") || value.starts_with("https://"))
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.trim().to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out.trim().to_string()
}

fn category_label(value: &str) -> &str {
    match value {
        "bug" => "程序问题",
        "model" => "模型调用",
        "access" => "账号与权益",
        "suggestion" => "功能建议",
        "other" => "其他",
        other => other,
    }
}

fn status_label(value: &str) -> &str {
    match value {
        "open" => "待处理",
        "resolved" => "已解决",
        "closed" => "已关闭",
        other => other,
    }
}

fn source_phrase(source: MentionSource) -> &'static str {
    match source {
        MentionSource::Created => "在新建的反馈中",
        MentionSource::Reply => "在回复中",
        MentionSource::StatusComment => "在状态变更说明中",
    }
}

pub(super) struct MentionEmail {
    pub(super) subject: String,
    pub(super) html: String,
    pub(super) text: String,
}

pub(super) fn build_mention_email(
    app_name: &str,
    actor: &str,
    issue: &FeedbackSummary,
    source: MentionSource,
    content: &str,
    base_url: Option<&str>,
) -> MentionEmail {
    let title = truncate_chars(&issue.title, 120);
    let quoted = truncate_chars(content, MAX_QUOTED_CHARS);
    let link = base_url.map(|base| format!("{base}/admin/feedback/{}", issue.number));
    let intro = format!(
        "{actor} {}提到了你（#{}）。",
        source_phrase(source),
        issue.number
    );
    let mut blocks = vec![
        EmailBlock::Paragraph(intro.clone()),
        EmailBlock::Rows(vec![
            ("主题".into(), title.clone()),
            ("编号".into(), format!("#{}", issue.number)),
            ("类型".into(), category_label(&issue.category).into()),
            ("状态".into(), status_label(&issue.status).into()),
        ]),
        EmailBlock::Quote(quoted.clone()),
    ];
    if let Some(link) = &link {
        blocks.push(EmailBlock::Button {
            label: "查看反馈".into(),
            url: link.clone(),
        });
    }
    let subject = format!(
        "[{app_name}] {actor} 提到了你 · #{} {}",
        issue.number,
        truncate_chars(&issue.title, 60)
    );
    let html = render_email_html(
        app_name,
        &format!("你在反馈 #{} 中被提到", issue.number),
        &intro,
        &blocks,
        "你收到此邮件，是因为你是管理员并在反馈中被 @ 提到。此邮件由系统自动发送，请勿直接回复。",
    );
    let mut text = format!(
        "{intro}\n\n主题：{title}\n编号：#{}\n\n{quoted}\n",
        issue.number
    );
    if let Some(link) = link {
        text.push_str(&format!("\n查看反馈：{link}\n"));
    }
    MentionEmail {
        subject,
        html,
        text,
    }
}

/// Best-effort: failures are logged and never affect the feedback mutation that triggered them.
pub(super) async fn notify_mentions(
    state: AppState,
    issue: FeedbackSummary,
    actor_id: String,
    source: MentionSource,
    content: String,
) {
    let names: Vec<String> = extract_mentions(&content)
        .into_iter()
        .take(MAX_MENTIONS_PER_MESSAGE)
        .collect();
    if names.is_empty() {
        return;
    }
    let smtp = match read_smtp_delivery_config(&state).await {
        Ok(Some(config)) => config,
        Ok(None) => return,
        Err(err) => {
            tracing::warn!(error = ?err, "feedback mention e-mail skipped: SMTP configuration unreadable");
            return;
        }
    };
    let actor_name = match state.data.find_user_auth_by_id(&actor_id).await {
        Ok(Some(user)) => user.username,
        _ => "某位用户".to_string(),
    };
    let base_url = public_base_url();
    let mut notified: BTreeSet<String> = BTreeSet::new();
    for name in names {
        let Ok(Some(user)) = state.data.find_user_auth_by_username(&name).await else {
            continue;
        };
        let email = user.email.as_deref().map(str::trim).unwrap_or_default();
        if user.id == actor_id
            || !user.is_active
            || user.is_deleted
            || !is_feedback_admin(&user.role)
            || email.is_empty()
            || !notified.insert(user.id.clone())
        {
            continue;
        }
        let message = build_mention_email(
            &smtp.from_name,
            &actor_name,
            &issue,
            source,
            &content,
            base_url.as_deref(),
        );
        if let Err(err) = send_smtp_email(
            smtp.clone(),
            ComposedEmail {
                to_email: email.to_string(),
                subject: message.subject,
                html_body: message.html,
                text_body: message.text,
            },
        )
        .await
        {
            tracing::warn!(error = ?err, issue_id = %issue.id, mentioned = %user.id, "failed to send feedback mention e-mail");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue() -> FeedbackSummary {
        FeedbackSummary {
            id: "issue-1".into(),
            owner_id: Some("u1".into()),
            owner_username: Some("alice".into()),
            title: "模型 <gpt-5> 调用失败".into(),
            category: "bug".into(),
            model: None,
            request_id: None,
            status: "open".into(),
            visibility: "private".into(),
            number: 12,
            pinned_at: None,
            version: 1,
            activity_seq: 1,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn mention_email_carries_title_content_and_link_and_escapes_html() {
        let email = build_mention_email(
            "Arca",
            "alice",
            &issue(),
            MentionSource::Reply,
            "@root 请看 <img src=x onerror=alert(1)>\n第二行",
            Some("https://gw.example.com"),
        );
        assert!(email.subject.contains("#12"));
        assert!(email.subject.contains("模型 <gpt-5> 调用失败"));
        assert!(email.html.contains("模型 &lt;gpt-5&gt; 调用失败"));
        assert!(!email.html.contains("<img"));
        assert!(email
            .html
            .contains("https://gw.example.com/admin/feedback/12"));
        assert!(email.text.contains("@root 请看"));
        assert!(email.html.contains("程序问题") && email.html.contains("待处理"));
        assert!(email
            .text
            .contains("https://gw.example.com/admin/feedback/12"));
    }

    #[test]
    fn mention_email_without_base_url_has_no_button() {
        let email = build_mention_email(
            "Arca",
            "alice",
            &issue(),
            MentionSource::Created,
            "hi",
            None,
        );
        assert!(!email.html.contains("查看反馈"));
        assert!(!email.text.contains("查看反馈"));
    }

    #[test]
    fn long_content_is_truncated() {
        let email = build_mention_email(
            "Arca",
            "alice",
            &issue(),
            MentionSource::Reply,
            &"字".repeat(5000),
            None,
        );
        assert!(email.text.chars().count() < 3500);
        assert!(email.text.contains('…'));
    }
}
