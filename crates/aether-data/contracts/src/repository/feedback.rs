use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedbackSummary {
    pub id: String,
    pub owner_id: Option<String>,
    pub owner_username: Option<String>,
    pub title: String,
    pub category: String,
    pub model: Option<String>,
    pub request_id: Option<String>,
    pub status: String,
    #[serde(default = "private_visibility")]
    pub visibility: String,
    /// Human-friendly issue number (`#N`), unique and assigned in creation order.
    #[serde(default)]
    pub number: i64,
    /// Set while an administrator keeps the issue pinned to the top of the list.
    #[serde(default)]
    pub pinned_at: Option<i64>,
    /// Set once the owner or an administrator changed the title, description or category.
    #[serde(default)]
    pub edited_at: Option<i64>,
    /// Set when the issue was deleted. Deleted issues are hidden everywhere except as tombstones in
    /// incremental (`after`) listings, so polling clients learn that they are gone.
    #[serde(default)]
    pub deleted_at: Option<i64>,
    pub version: i64,
    pub activity_seq: i64,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedbackIssue {
    #[serde(flatten)]
    pub summary: FeedbackSummary,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedbackEvent {
    pub seq: i64,
    pub issue_id: String,
    pub actor_id: Option<String>,
    pub actor_username: Option<String>,
    pub actor_role: String,
    pub kind: String,
    #[serde(default = "private_visibility")]
    pub visibility: String,
    pub content: Option<String>,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub created_at: i64,
    /// `reply_edit` / `reply_delete` markers point at the reply they changed.
    #[serde(default)]
    pub target_seq: Option<i64>,
    /// Set on a reply whose author changed it afterwards.
    #[serde(default)]
    pub edited_at: Option<i64>,
    /// Set on a reply that was deleted; its content is gone.
    #[serde(default)]
    pub deleted_at: Option<i64>,
}
#[derive(Debug, Clone, Default)]
pub struct FeedbackQuery {
    /// None is a trusted administrator read. This is identity, never an owner filter.
    pub viewer_id: Option<String>,
    pub owner_id: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub after: Option<i64>,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackPage {
    pub items: Vec<FeedbackSummary>,
    pub total: i64,
    pub next_cursor: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackDetail {
    pub issue: FeedbackIssue,
    pub events: Vec<FeedbackEvent>,
    pub next_event_cursor: Option<i64>,
    pub has_more: bool,
}
/// An in-app notification. Only `mention` exists today: somebody wrote `@you` in a topic or reply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedbackNotification {
    pub id: i64,
    pub kind: String,
    pub issue_id: String,
    pub issue_number: i64,
    pub issue_title: String,
    /// Event that carries the mention (`activity_seq` of the issue when it was written).
    pub event_seq: i64,
    pub actor_id: Option<String>,
    pub actor_username: Option<String>,
    pub snippet: String,
    pub created_at: i64,
    pub read_at: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFeedbackNotification {
    pub user_id: String,
    pub issue_id: String,
    pub event_seq: i64,
    pub actor_id: Option<String>,
    pub snippet: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationPage {
    pub items: Vec<FeedbackNotification>,
    pub unread_count: i64,
    /// Pass as `before` to read the next (older) page.
    pub next_before: Option<i64>,
}
/// Notifications kept per user; older ones are dropped when new ones arrive.
pub const MAX_NOTIFICATIONS_PER_USER: usize = 200;
pub const NOTIFICATION_SNIPPET_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFeedback {
    // Omit default only for input hashing, preserving pre-visibility idempotency hashes.
    #[serde(
        default = "private_visibility",
        skip_serializing_if = "is_private_visibility"
    )]
    pub visibility: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub model: Option<String>,
    pub request_id: Option<String>,
    pub client_request_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum FeedbackAction {
    Reply {
        content: String,
    },
    SetStatus {
        status: String,
        comment: Option<String>,
    },
    /// Administrators only: keep the issue at the top of the list (or release it).
    SetPinned {
        pinned: bool,
    },
    /// The owner or an administrator changes the issue; at least one field must differ.
    EditIssue {
        title: Option<String>,
        description: Option<String>,
        category: Option<String>,
    },
    /// The owner or an administrator deletes the issue (it stays stored but disappears).
    DeleteIssue,
    /// The author of a reply changes its text.
    EditReply {
        seq: i64,
        content: String,
    },
    /// The author of a reply, or an administrator, removes its content.
    DeleteReply {
        seq: i64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "FeedbackCommandWire")]
pub struct FeedbackCommand {
    #[serde(
        default = "private_visibility",
        skip_serializing_if = "is_private_visibility"
    )]
    pub visibility: String,
    pub expected_version: i64,
    pub client_request_id: String,
    #[serde(flatten)]
    pub action: FeedbackAction,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeedbackCommandWire {
    #[serde(default = "private_visibility")]
    visibility: String,
    expected_version: i64,
    client_request_id: String,
    action: String,
    content: Option<String>,
    status: Option<String>,
    comment: Option<String>,
    pinned: Option<bool>,
    title: Option<String>,
    description: Option<String>,
    category: Option<String>,
    seq: Option<i64>,
}
impl TryFrom<FeedbackCommandWire> for FeedbackCommand {
    type Error = &'static str;
    fn try_from(w: FeedbackCommandWire) -> Result<Self, Self::Error> {
        // Every action accepts exactly its own fields; anything else is rejected.
        let present = |names: &[&str]| -> bool {
            let all = [
                ("content", w.content.is_some()),
                ("status", w.status.is_some()),
                ("comment", w.comment.is_some()),
                ("pinned", w.pinned.is_some()),
                ("title", w.title.is_some()),
                ("description", w.description.is_some()),
                ("category", w.category.is_some()),
                ("seq", w.seq.is_some()),
            ];
            all.iter().all(|(name, set)| !set || names.contains(name))
        };
        let action = match w.action.as_str() {
            "reply" if present(&["content"]) => FeedbackAction::Reply {
                content: w.content.ok_or("Reply content required")?,
            },
            "set_status" if present(&["status", "comment"]) => FeedbackAction::SetStatus {
                status: w.status.ok_or("Status required")?,
                comment: w.comment,
            },
            "set_pinned" if present(&["pinned"]) => FeedbackAction::SetPinned {
                pinned: w.pinned.ok_or("Pinned flag required")?,
            },
            "edit_issue" if present(&["title", "description", "category"]) => {
                FeedbackAction::EditIssue {
                    title: w.title,
                    description: w.description,
                    category: w.category,
                }
            }
            "delete_issue" if present(&[]) => FeedbackAction::DeleteIssue,
            "edit_reply" if present(&["seq", "content"]) => FeedbackAction::EditReply {
                seq: w.seq.ok_or("Reply seq required")?,
                content: w.content.ok_or("Reply content required")?,
            },
            "delete_reply" if present(&["seq"]) => FeedbackAction::DeleteReply {
                seq: w.seq.ok_or("Reply seq required")?,
            },
            _ => return Err("Invalid feedback action fields"),
        };
        Ok(Self {
            visibility: w.visibility,
            expected_version: w.expected_version,
            client_request_id: w.client_request_id,
            action,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackMutation {
    pub issue: FeedbackIssue,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackError {
    NotFound,
    Forbidden,
    Conflict,
    Invalid(&'static str),
    Unavailable,
}

pub fn private_visibility() -> String {
    "private".into()
}
fn is_private_visibility(value: &String) -> bool {
    value == "private"
}
pub fn valid_visibility(value: &str) -> bool {
    matches!(value, "public" | "private")
}
pub fn is_feedback_admin(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "admin" | "audit_admin" | "user_admin" | "service_admin"
    )
}
pub fn authorize_feedback_create(owner: &str, role: &str) -> Result<(), FeedbackError> {
    if owner.trim().is_empty() || (role != "user" && !is_feedback_admin(role)) {
        return Err(FeedbackError::Forbidden);
    }
    Ok(())
}
pub fn can_view_feedback(issue: &FeedbackSummary, viewer: Option<&str>) -> bool {
    viewer.is_none() || issue.visibility == "public" || viewer == issue.owner_id.as_deref()
}
/// Enforce current authority before any idempotency lookup. Route flags cannot grant administration.
pub fn authorize_feedback_command(
    issue: &FeedbackSummary,
    actor: &str,
    role: &str,
    command: &FeedbackCommand,
) -> Result<(), FeedbackError> {
    let admin = is_feedback_admin(role);
    let owner = issue.owner_id.as_deref() == Some(actor);
    if !admin && !can_view_feedback(issue, Some(actor)) {
        return Err(FeedbackError::NotFound);
    }
    match &command.action {
        FeedbackAction::SetStatus { status, .. } if !admin && (!owner || status == "closed") => {
            return Err(FeedbackError::Forbidden);
        }
        FeedbackAction::SetPinned { .. } if !admin => return Err(FeedbackError::Forbidden),
        FeedbackAction::EditIssue { .. } | FeedbackAction::DeleteIssue if !admin && !owner => {
            return Err(FeedbackError::Forbidden);
        }
        _ => {}
    }
    Ok(())
}

/// Reply edits belong to the reply's author; deleting is also open to administrators. Replies that
/// were already deleted, and every other event kind, cannot be changed.
pub fn authorize_feedback_reply_change(
    reply: &FeedbackEvent,
    actor: &str,
    role: &str,
    delete: bool,
) -> Result<(), FeedbackError> {
    if reply.kind != "reply" || reply.deleted_at.is_some() {
        return Err(FeedbackError::NotFound);
    }
    let author = reply.actor_id.as_deref() == Some(actor);
    if author || (delete && is_feedback_admin(role)) {
        Ok(())
    } else {
        Err(FeedbackError::Forbidden)
    }
}
/// Issue visibility must already have been authorized; a public reply never opens a private issue.
pub fn project_feedback_event(
    event: &FeedbackEvent,
    issue: &FeedbackSummary,
    viewer: Option<&str>,
) -> Option<FeedbackEvent> {
    let privileged = viewer.is_none()
        || viewer == issue.owner_id.as_deref()
        || viewer == event.actor_id.as_deref();
    if privileged || event.visibility == "public" {
        return Some(event.clone());
    }
    if !matches!(event.kind.as_str(), "status" | "created" | "pin") {
        return None;
    }
    let mut event = event.clone();
    event.content = None;
    Some(event)
}

pub fn valid_status(value: &str) -> bool {
    matches!(value, "open" | "resolved" | "closed")
}
pub fn valid_category(value: &str) -> bool {
    matches!(value, "bug" | "model" | "access" | "suggestion" | "other")
}
pub fn validate_create(value: &CreateFeedback) -> Result<(), FeedbackError> {
    if value.title.trim().is_empty()
        || value.title.chars().count() > 200
        || value.description.trim().is_empty()
        || value.description.len() > 20000
        || !valid_category(&value.category)
        || !valid_visibility(&value.visibility)
        || value.model.as_ref().is_some_and(|s| s.len() > 200)
        || value.request_id.as_ref().is_some_and(|s| s.len() > 128)
        || value.client_request_id.len() != 36
        || uuid::Uuid::parse_str(&value.client_request_id).is_err()
    {
        return Err(FeedbackError::Invalid(
            "Invalid feedback title, description, category, references or idempotency UUID",
        ));
    }
    Ok(())
}
pub fn validate_command(value: &FeedbackCommand) -> Result<(), FeedbackError> {
    if value.expected_version < 1
        || !valid_visibility(&value.visibility)
        || value.client_request_id.len() != 36
        || uuid::Uuid::parse_str(&value.client_request_id).is_err()
    {
        return Err(FeedbackError::Invalid(
            "A positive expected_version and stable client_request_id UUID are required",
        ));
    }
    match &value.action {
        FeedbackAction::Reply { content } if content.trim().is_empty() || content.len() > 20000 => {
            Err(FeedbackError::Invalid("Reply must be 1-20000 bytes"))
        }
        FeedbackAction::SetStatus { status, comment }
            if !valid_status(status) || comment.as_ref().is_some_and(|c| c.len() > 20000)
                || (status == "resolved" && comment.as_ref().is_none_or(|c| c.trim().is_empty())) =>
        {
            Err(FeedbackError::Invalid("Resolve requires a nonblank final solution; comments must not exceed 20000 UTF-8 bytes"))
        }
        FeedbackAction::EditReply { seq, content }
            if *seq < 1 || content.trim().is_empty() || content.len() > 20000 =>
        {
            Err(FeedbackError::Invalid("Reply must be 1-20000 bytes"))
        }
        FeedbackAction::DeleteReply { seq } if *seq < 1 => {
            Err(FeedbackError::Invalid("A positive reply seq is required"))
        }
        FeedbackAction::EditIssue {
            title,
            description,
            category,
        } if (title.is_none() && description.is_none() && category.is_none())
            || title
                .as_ref()
                .is_some_and(|t| t.trim().is_empty() || t.chars().count() > 200)
            || description
                .as_ref()
                .is_some_and(|d| d.trim().is_empty() || d.len() > 20000)
            || category.as_ref().is_some_and(|c| !valid_category(c)) =>
        {
            Err(FeedbackError::Invalid(
                "Edit needs a title (1-200 characters), description (1-20000 bytes) or valid category",
            ))
        }
        _ => Ok(()),
    }
}

#[async_trait]
pub trait FeedbackRepository: Send + Sync + std::fmt::Debug {
    async fn list(&self, query: &FeedbackQuery) -> Result<FeedbackPage, FeedbackError>;
    // viewer=None is a trusted actual administrator; never use it for ordinary users.
    async fn detail(
        &self,
        id: &str,
        viewer: Option<&str>,
        after: i64,
        limit: usize,
    ) -> Result<FeedbackDetail, FeedbackError>;
    /// Resolves a human-friendly issue number (`#N`) to the issue id. No visibility is checked
    /// here: callers must read the issue through `detail`, which enforces it.
    async fn id_for_number(&self, number: i64) -> Result<Option<String>, FeedbackError>;
    /// Records mention notifications (best effort for callers). Repeating the same
    /// (user, issue, event) is ignored, and each user keeps only the newest
    /// `MAX_NOTIFICATIONS_PER_USER`.
    async fn add_notifications(
        &self,
        items: &[NewFeedbackNotification],
    ) -> Result<(), FeedbackError>;
    /// Newest first. Notifications of deleted issues are not returned or counted.
    async fn list_notifications(
        &self,
        user_id: &str,
        unread_only: bool,
        before: Option<i64>,
        limit: usize,
    ) -> Result<NotificationPage, FeedbackError>;
    /// Marks the given notifications (or all, with `None`) of this user as read and returns what
    /// is still unread. Other users' notifications are never touched.
    async fn mark_notifications_read(
        &self,
        user_id: &str,
        ids: Option<&[i64]>,
    ) -> Result<i64, FeedbackError>;
    async fn create(
        &self,
        owner: &str,
        role: &str,
        input: &CreateFeedback,
    ) -> Result<FeedbackMutation, FeedbackError>;
    async fn command(
        &self,
        id: &str,
        actor: &str,
        role: &str,
        owner_only: bool, // Legacy route hint only; implementations MUST derive administrator authority from role.
        input: &FeedbackCommand,
    ) -> Result<FeedbackMutation, FeedbackError>;
}

/// Usernames mentioned as `@name` in markdown text. An `@` only counts when it starts a word, so
/// e-mail addresses (`a@b.com`) are ignored. Names are returned once each, in order of first use.
pub fn extract_mentions(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut prev: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '@' && prev.is_none_or(|p| !(p.is_alphanumeric() || p == '_')) {
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || matches!(next, '_' | '.' | '-') {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            let name = name.trim_end_matches(['.', '-']).to_string();
            if !name.is_empty()
                && name.chars().count() <= 64
                && !out.iter().any(|seen| seen.eq_ignore_ascii_case(&name))
            {
                out.push(name);
            }
            prev = Some('@');
            continue;
        }
        prev = Some(ch);
    }
    out
}

#[cfg(test)]
mod mention_tests {
    use super::extract_mentions;

    #[test]
    fn extracts_mentions_once_in_order_and_ignores_email_addresses() {
        assert_eq!(
            extract_mentions("请 @alice 和 @Bob.Lee 看下，cc @alice，联系 ops@example.com"),
            vec!["alice", "Bob.Lee"]
        );
    }

    #[test]
    fn trailing_punctuation_and_bare_at_signs_are_not_part_of_the_name() {
        assert_eq!(extract_mentions("(@root). @ @- 结束@"), vec!["root"]);
        assert!(extract_mentions("no mentions here").is_empty());
    }
}
