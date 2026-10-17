//! `@username` mentions in feedback: only administrators can be mentioned, and a mention e-mails
//! them the issue title and the text they were mentioned in.

use crate::email_delivery::{read_smtp_delivery_config, send_smtp_email, ComposedEmail};
use crate::email_layout::{render_email_html, EmailBlock};
use crate::AppState;
use aether_data::repository::feedback::{
    extract_mentions, is_feedback_admin, FeedbackSummary, NewFeedbackNotification,
    NOTIFICATION_SNIPPET_CHARS,
};
use std::collections::BTreeSet;

const MAX_MENTIONS_PER_MESSAGE: usize = 20;
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
        MentionSource::Created => "在新发起的主题中",
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
            label: "查看主题".into(),
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
        &format!("你在主题 #{} 中被提到", issue.number),
        &intro,
        &blocks,
        "你收到此邮件，是因为你是管理员并在讨论与反馈中被 @ 提到。此邮件由系统自动发送，请勿直接回复。",
    );
    let mut text = format!(
        "{intro}\n\n主题：{title}\n编号：#{}\n\n{quoted}\n",
        issue.number
    );
    if let Some(link) = link {
        text.push_str(&format!("\n查看主题：{link}\n"));
    }
    MentionEmail {
        subject,
        html,
        text,
    }
}

fn skip_mention(issue: &FeedbackSummary, name: &str, reason: &'static str) {
    tracing::info!(
        event_name = "feedback_mention_email_skipped",
        reason,
        issue_id = %issue.id,
        mentioned = name,
        "feedback mention e-mail skipped"
    );
}

struct Recipient {
    id: String,
    name: String,
    email: Option<String>,
    is_admin: bool,
}

/// Only administrators with an address are mailed; everyone else only gets the in-app notification.
fn email_decision(recipient: &Recipient) -> Result<(), &'static str> {
    if !recipient.is_admin {
        Err("email_is_admin_only")
    } else if recipient.email.is_none() {
        Err("no_email_address")
    } else {
        Ok(())
    }
}

/// Anybody active can be mentioned. Everyone mentioned gets an in-app notification; only
/// administrators additionally get an e-mail. Best-effort: failures are logged and never affect
/// the feedback mutation that triggered them.
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
    let actor_name = match state.find_user_auth_by_id(&actor_id).await {
        Ok(Some(user)) => user.username,
        _ => "某位用户".to_string(),
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut recipients: Vec<Recipient> = Vec::new();
    for name in names {
        let Ok(Some(user)) = state.find_user_auth_by_username(&name).await else {
            skip_mention(&issue, &name, "user_not_found");
            continue;
        };
        let is_admin = is_feedback_admin(&user.role);
        let reason = if user.id == actor_id {
            Some("mentioned_self")
        } else if !user.is_active || user.is_deleted {
            Some("user_inactive")
        } else if issue.visibility != "public"
            && !is_admin
            && issue.owner_id.as_deref() != Some(user.id.as_str())
        {
            // Topics created before private ones were removed keep their old audience.
            Some("cannot_view_topic")
        } else if !seen.insert(user.id.clone()) {
            Some("already_notified")
        } else {
            None
        };
        if let Some(reason) = reason {
            skip_mention(&issue, &name, reason);
            continue;
        }
        recipients.push(Recipient {
            id: user.id.clone(),
            name,
            email: user
                .email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .map(str::to_owned),
            is_admin,
        });
    }
    if recipients.is_empty() {
        return;
    }

    // In-app notifications for everybody mentioned.
    if let Some(repository) = state.data.feedback_repository() {
        let snippet = truncate_chars(&content, NOTIFICATION_SNIPPET_CHARS);
        let items: Vec<NewFeedbackNotification> = recipients
            .iter()
            .map(|recipient| NewFeedbackNotification {
                user_id: recipient.id.clone(),
                issue_id: issue.id.clone(),
                event_seq: issue.activity_seq,
                actor_id: Some(actor_id.clone()),
                snippet: snippet.clone(),
            })
            .collect();
        if let Err(err) = repository.add_notifications(&items).await {
            tracing::warn!(error = ?err, issue_id = %issue.id, "failed to store mention notifications");
        }
    }

    // E-mail is reserved for administrators.
    let mailable: Vec<&Recipient> = recipients
        .iter()
        .filter(|recipient| match email_decision(recipient) {
            Ok(()) => true,
            Err(reason) => {
                skip_mention(&issue, &recipient.name, reason);
                false
            }
        })
        .collect();
    if mailable.is_empty() {
        return;
    }
    let smtp = match read_smtp_delivery_config(&state).await {
        Ok(Some(config)) => config,
        Ok(None) => {
            tracing::info!(
                event_name = "feedback_mention_email_skipped",
                reason = "smtp_not_configured",
                issue_id = %issue.id,
                "feedback mention e-mail skipped: smtp_host or smtp_from_email is not set"
            );
            return;
        }
        Err(err) => {
            tracing::warn!(error = ?err, issue_id = %issue.id, "feedback mention e-mail skipped: SMTP configuration unreadable");
            return;
        }
    };
    let base_url = public_base_url();
    for recipient in mailable {
        let Some(to_email) = recipient.email.clone() else {
            continue;
        };
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
                to_email,
                subject: message.subject,
                html_body: message.html,
                text_body: message.text,
            },
        )
        .await
        {
            tracing::warn!(error = ?err, issue_id = %issue.id, mentioned = %recipient.id, "failed to send feedback mention e-mail");
        } else {
            tracing::info!(
                event_name = "feedback_mention_email_sent",
                issue_id = %issue.id,
                mentioned = %recipient.id,
                "feedback mention e-mail sent"
            );
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
            edited_at: None,
            deleted_at: None,
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
        assert!(!email.html.contains("查看主题"));
        assert!(!email.text.contains("查看主题"));
    }

    #[test]
    fn only_administrators_with_an_address_are_mailed() {
        let recipient = |is_admin: bool, email: Option<&str>| Recipient {
            id: "u".into(),
            name: "n".into(),
            email: email.map(str::to_owned),
            is_admin,
        };
        assert_eq!(email_decision(&recipient(true, Some("a@b.co"))), Ok(()));
        assert_eq!(
            email_decision(&recipient(false, Some("a@b.co"))),
            Err("email_is_admin_only")
        );
        assert_eq!(
            email_decision(&recipient(true, None)),
            Err("no_email_address")
        );
        assert_eq!(
            email_decision(&recipient(false, None)),
            Err("email_is_admin_only")
        );
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
