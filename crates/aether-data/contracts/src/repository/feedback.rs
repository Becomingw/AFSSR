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
}
impl TryFrom<FeedbackCommandWire> for FeedbackCommand {
    type Error = &'static str;
    fn try_from(w: FeedbackCommandWire) -> Result<Self, Self::Error> {
        let action = match w.action.as_str() {
            "reply" if w.status.is_none() && w.comment.is_none() => FeedbackAction::Reply {
                content: w.content.ok_or("Reply content required")?,
            },
            "set_status" if w.content.is_none() => FeedbackAction::SetStatus {
                status: w.status.ok_or("Status required")?,
                comment: w.comment,
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
    if let FeedbackAction::SetStatus { status, .. } = &command.action {
        if !admin && (!owner || status == "closed") {
            return Err(FeedbackError::Forbidden);
        }
    }
    Ok(())
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
    if !matches!(event.kind.as_str(), "status" | "created") {
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
