pub use aether_data_contracts::repository::feedback::*;
#[cfg(feature = "postgres")]
pub use aether_data_postgres::PostgresFeedbackRepository;

use async_trait::async_trait;
use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
pub struct InMemoryFeedbackRepository {
    inner: Mutex<Store>,
}
#[derive(Default)]
struct Store {
    issues: HashMap<String, FeedbackIssue>,
    events: Vec<FeedbackEvent>,
    commands: HashMap<(String, String), (String, FeedbackMutation)>,
    seq: i64,
    notifications: Vec<StoredNotification>,
    notification_seq: i64,
}
struct StoredNotification {
    id: i64,
    user_id: String,
    issue_id: String,
    event_seq: i64,
    actor_id: Option<String>,
    snippet: String,
    created_at: i64,
    read_at: Option<i64>,
}
impl Store {
    /// Unread notifications of issues that still exist.
    fn unread_for(&self, user_id: &str) -> i64 {
        self.notifications
            .iter()
            .filter(|n| {
                n.user_id == user_id && n.read_at.is_none() && self.issue_exists(&n.issue_id)
            })
            .count() as i64
    }
    fn issue_exists(&self, issue_id: &str) -> bool {
        self.issues
            .get(issue_id)
            .is_some_and(|issue| issue.summary.deleted_at.is_none())
    }
}
impl std::fmt::Debug for InMemoryFeedbackRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InMemoryFeedbackRepository([contents omitted])")
    }
}
fn key(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("feedback input serializes")
}

#[async_trait]
impl FeedbackRepository for InMemoryFeedbackRepository {
    async fn list(&self, q: &FeedbackQuery) -> Result<FeedbackPage, FeedbackError> {
        if q.viewer_id.is_some() && q.owner_id.is_some() {
            return Err(FeedbackError::Forbidden);
        }
        let store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let mut items: Vec<_> = store
            .issues
            .values()
            .map(|i| i.summary.clone())
            .filter(|i| {
                // Incremental readers also receive deleted issues so they can drop them.
                (q.after.is_some() || i.deleted_at.is_none())
                    && can_view_feedback(i, q.viewer_id.as_deref())
                    && q.owner_id
                        .as_ref()
                        .is_none_or(|owner| i.owner_id.as_ref() == Some(owner))
                    && q.status.as_ref().is_none_or(|s| &i.status == s)
                    && q.category.as_ref().is_none_or(|s| &i.category == s)
                    && q.after.is_none_or(|s| i.activity_seq > s)
                    && q.search
                        .as_ref()
                        .is_none_or(|s| i.title.to_lowercase().contains(&s.to_lowercase()))
            })
            .collect();
        items.sort_by_key(|i| {
            if q.after.is_some() {
                (0, 0, i.activity_seq)
            } else {
                // Pinned issues first (most recently pinned on top), then newest activity.
                (
                    i64::from(i.pinned_at.is_none()),
                    -i.pinned_at.unwrap_or(0),
                    -i.activity_seq,
                )
            }
        });
        let total = items.len() as i64;
        let items: Vec<_> = items
            .into_iter()
            .skip(q.offset)
            .take(q.limit.clamp(1, 100))
            .collect();
        let next_cursor = items.iter().map(|i| i.activity_seq).max().or(q.after);
        Ok(FeedbackPage {
            items,
            total,
            next_cursor,
        })
    }
    async fn detail(
        &self,
        id: &str,
        viewer: Option<&str>,
        after: i64,
        limit: usize,
    ) -> Result<FeedbackDetail, FeedbackError> {
        let store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let issue = store
            .issues
            .get(id)
            .filter(|i| i.summary.deleted_at.is_none() && can_view_feedback(&i.summary, viewer))
            .ok_or(FeedbackError::NotFound)?
            .clone();
        let events: Vec<_> = store
            .events
            .iter()
            .filter(|e| e.issue_id == id && e.seq > after)
            .take(limit.clamp(1, 100) + 1)
            .cloned()
            .collect();
        let has_more = events.len() > limit.clamp(1, 100);
        let scanned: Vec<_> = events.into_iter().take(limit.clamp(1, 100)).collect();
        let next_event_cursor = scanned.last().map(|e| e.seq).or(Some(after));
        let events = scanned
            .iter()
            .filter_map(|e| project_feedback_event(e, &issue.summary, viewer))
            .collect();
        Ok(FeedbackDetail {
            issue,
            next_event_cursor,
            events,
            has_more,
        })
    }
    async fn id_for_number(&self, number: i64) -> Result<Option<String>, FeedbackError> {
        let store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        Ok(store
            .issues
            .values()
            .find(|issue| issue.summary.number == number && issue.summary.deleted_at.is_none())
            .map(|issue| issue.summary.id.clone()))
    }
    async fn add_notifications(
        &self,
        items: &[NewFeedbackNotification],
    ) -> Result<(), FeedbackError> {
        let mut store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let now = chrono::Utc::now().timestamp();
        for item in items {
            if store.notifications.iter().any(|n| {
                n.user_id == item.user_id
                    && n.issue_id == item.issue_id
                    && n.event_seq == item.event_seq
            }) {
                continue;
            }
            store.notification_seq += 1;
            let id = store.notification_seq;
            store.notifications.push(StoredNotification {
                id,
                user_id: item.user_id.clone(),
                issue_id: item.issue_id.clone(),
                event_seq: item.event_seq,
                actor_id: item.actor_id.clone(),
                snippet: item.snippet.chars().take(500).collect(),
                created_at: now,
                read_at: None,
            });
        }
        // Keep only the newest MAX_NOTIFICATIONS_PER_USER per user.
        let users: std::collections::HashSet<String> =
            items.iter().map(|i| i.user_id.clone()).collect();
        for user in users {
            let mut mine: Vec<i64> = store
                .notifications
                .iter()
                .filter(|n| n.user_id == user)
                .map(|n| n.id)
                .collect();
            mine.sort_unstable_by(|a, b| b.cmp(a));
            if let Some(&cutoff) = mine.get(MAX_NOTIFICATIONS_PER_USER) {
                store
                    .notifications
                    .retain(|n| n.user_id != user || n.id > cutoff);
            }
        }
        Ok(())
    }
    async fn list_notifications(
        &self,
        user_id: &str,
        unread_only: bool,
        before: Option<i64>,
        limit: usize,
    ) -> Result<NotificationPage, FeedbackError> {
        let store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let limit = limit.clamp(1, 100);
        let mut rows: Vec<&StoredNotification> = store
            .notifications
            .iter()
            .filter(|n| {
                n.user_id == user_id
                    && store.issue_exists(&n.issue_id)
                    && (!unread_only || n.read_at.is_none())
                    && before.is_none_or(|b| n.id < b)
            })
            .collect();
        rows.sort_by_key(|n| std::cmp::Reverse(n.id));
        let has_more = rows.len() > limit;
        let items: Vec<FeedbackNotification> = rows
            .into_iter()
            .take(limit)
            .map(|n| {
                let issue = &store.issues[&n.issue_id];
                FeedbackNotification {
                    id: n.id,
                    kind: "mention".into(),
                    issue_id: n.issue_id.clone(),
                    issue_number: issue.summary.number,
                    issue_title: issue.summary.title.clone(),
                    event_seq: n.event_seq,
                    actor_username: n.actor_id.clone(),
                    snippet: n.snippet.clone(),
                    created_at: n.created_at,
                    read_at: n.read_at,
                }
            })
            .collect();
        let next_before = has_more.then(|| items.last().map(|n| n.id)).flatten();
        Ok(NotificationPage {
            items,
            unread_count: store.unread_for(user_id),
            next_before,
        })
    }
    async fn mark_notifications_read(
        &self,
        user_id: &str,
        ids: Option<&[i64]>,
    ) -> Result<i64, FeedbackError> {
        let mut store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let now = chrono::Utc::now().timestamp();
        for n in store.notifications.iter_mut() {
            if n.user_id == user_id
                && n.read_at.is_none()
                && ids.is_none_or(|ids| ids.contains(&n.id))
            {
                n.read_at = Some(now);
            }
        }
        Ok(store.unread_for(user_id))
    }
    async fn create(
        &self,
        owner: &str,
        role: &str,
        input: &CreateFeedback,
    ) -> Result<FeedbackMutation, FeedbackError> {
        authorize_feedback_create(owner, role)?;
        validate_create(input)?;
        let mut store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let command_key = (format!("create:{owner}"), input.client_request_id.clone());
        let hash = key(input);
        if let Some((old, result)) = store.commands.get(&command_key) {
            if old != &hash {
                return Err(FeedbackError::Conflict);
            }
            let issue = store
                .issues
                .get(&result.issue.summary.id)
                .ok_or(FeedbackError::NotFound)?
                .clone();
            return Ok(FeedbackMutation {
                issue,
                replayed: true,
            });
        }
        store.seq += 1;
        let seq = store.seq;
        let now = chrono::Utc::now().timestamp();
        let id = uuid::Uuid::new_v4().to_string();
        let number = store
            .issues
            .values()
            .map(|issue| issue.summary.number)
            .max()
            .unwrap_or(0)
            + 1;
        let issue = FeedbackIssue {
            summary: FeedbackSummary {
                id: id.clone(),
                owner_id: Some(owner.into()),
                owner_username: None,
                title: input.title.clone(),
                category: input.category.clone(),
                model: input.model.clone(),
                request_id: input.request_id.clone(),
                status: "open".into(),
                visibility: input.visibility.clone(),
                number,
                pinned_at: None,
                edited_at: None,
                deleted_at: None,
                version: 1,
                activity_seq: seq,
                created_at: now,
                updated_at: now,
            },
            description: input.description.clone(),
        };
        store.events.push(FeedbackEvent {
            seq,
            issue_id: id.clone(),
            actor_id: Some(owner.into()),
            actor_username: None,
            actor_role: role.into(),
            kind: "created".into(),
            visibility: "private".into(),
            content: None,
            from_status: None,
            to_status: Some("open".into()),
            created_at: now,
            target_seq: None,
            edited_at: None,
            deleted_at: None,
        });
        store.issues.insert(id, issue.clone());
        let result = FeedbackMutation {
            issue,
            replayed: false,
        };
        store.commands.insert(command_key, (hash, result.clone()));
        Ok(result)
    }
    async fn command(
        &self,
        id: &str,
        actor: &str,
        role: &str,
        _owner_only: bool,
        input: &FeedbackCommand,
    ) -> Result<FeedbackMutation, FeedbackError> {
        let mut store = self.inner.lock().map_err(|_| FeedbackError::Unavailable)?;
        let issue = store.issues.get(id).ok_or(FeedbackError::NotFound)?.clone();
        authorize_feedback_command(&issue.summary, actor, role, input)?;
        let command_key = (actor.to_owned(), input.client_request_id.clone());
        let hash = key(&(id, input));
        if let Some((old, _)) = store.commands.get(&command_key) {
            if old != &hash {
                return Err(FeedbackError::Conflict);
            }
            return Ok(FeedbackMutation {
                issue,
                replayed: true,
            });
        }
        // A deleted issue is gone for every new command (a replay above already returned).
        if issue.summary.deleted_at.is_some() {
            return Err(FeedbackError::NotFound);
        }
        // Successful pre-upgrade commands may predate today's write validation.
        validate_command(input)?;
        if issue.summary.version != input.expected_version {
            return Err(FeedbackError::Conflict);
        }
        let now = chrono::Utc::now().timestamp();
        let mut issue = issue;
        let mut target_seq: Option<i64> = None;
        // Replies are changed in place; the marker event below only tells incremental readers.
        let reply_index = match &input.action {
            FeedbackAction::EditReply { seq, .. } | FeedbackAction::DeleteReply { seq } => {
                let index = store
                    .events
                    .iter()
                    .position(|e| e.issue_id == id && e.seq == *seq)
                    .ok_or(FeedbackError::NotFound)?;
                let delete = matches!(input.action, FeedbackAction::DeleteReply { .. });
                authorize_feedback_reply_change(&store.events[index], actor, role, delete)?;
                target_seq = Some(*seq);
                Some(index)
            }
            _ => None,
        };
        let (kind, content, from_status, to_status) = match &input.action {
            FeedbackAction::EditIssue {
                title,
                description,
                category,
            } => {
                let title = title.clone().filter(|v| v != &issue.summary.title);
                let description = description.clone().filter(|v| v != &issue.description);
                let category = category.clone().filter(|v| v != &issue.summary.category);
                let changed: Vec<&str> = [
                    ("title", title.is_some()),
                    ("description", description.is_some()),
                    ("category", category.is_some()),
                ]
                .iter()
                .filter(|(_, c)| *c)
                .map(|(n, _)| *n)
                .collect();
                if changed.is_empty() {
                    return Err(FeedbackError::Conflict);
                }
                if let Some(v) = title {
                    issue.summary.title = v;
                }
                if let Some(v) = description {
                    issue.description = v;
                }
                if let Some(v) = category {
                    issue.summary.category = v;
                }
                issue.summary.edited_at = Some(now);
                ("issue_edit", Some(changed.join(",")), None, None)
            }
            FeedbackAction::DeleteIssue => {
                issue.summary.deleted_at = Some(now);
                ("issue_delete", None, None, None)
            }
            FeedbackAction::EditReply { content, .. } => {
                let reply = &mut store.events[reply_index.expect("reply located above")];
                if reply.content.as_deref() == Some(content.as_str()) {
                    return Err(FeedbackError::Conflict);
                }
                reply.content = Some(content.clone());
                reply.edited_at = Some(now);
                ("reply_edit", None, None, None)
            }
            FeedbackAction::DeleteReply { .. } => {
                let reply = &mut store.events[reply_index.expect("reply located above")];
                reply.content = None;
                reply.deleted_at = Some(now);
                ("reply_delete", None, None, None)
            }
            FeedbackAction::Reply { content } => ("reply", Some(content.clone()), None, None),
            FeedbackAction::SetStatus { status, comment } => {
                if status == &issue.summary.status {
                    return Err(FeedbackError::Conflict);
                }
                let old = std::mem::replace(&mut issue.summary.status, status.clone());
                ("status", comment.clone(), Some(old), Some(status.clone()))
            }
            FeedbackAction::SetPinned { pinned } => {
                if *pinned == issue.summary.pinned_at.is_some() {
                    return Err(FeedbackError::Conflict);
                }
                issue.summary.pinned_at = pinned.then_some(now);
                (
                    "pin",
                    Some(if *pinned { "pinned" } else { "unpinned" }.to_string()),
                    None,
                    None,
                )
            }
        };
        store.seq += 1;
        let seq = store.seq;
        issue.summary.version += 1;
        issue.summary.activity_seq = seq;
        issue.summary.updated_at = now;
        store.events.push(FeedbackEvent {
            seq,
            issue_id: id.into(),
            actor_id: Some(actor.into()),
            actor_username: None,
            actor_role: role.into(),
            kind: kind.into(),
            // Pin changes are system events that every viewer may see; edit/delete markers only
            // concern the people involved.
            visibility: match kind {
                "pin" => "public".into(),
                "issue_edit" | "issue_delete" | "reply_edit" | "reply_delete" => "private".into(),
                _ => input.visibility.clone(),
            },
            content,
            from_status,
            to_status,
            created_at: now,
            target_seq,
            edited_at: None,
            deleted_at: None,
        });
        store.issues.insert(id.into(), issue.clone());
        let result = FeedbackMutation {
            issue,
            replayed: false,
        };
        store.commands.insert(command_key, (hash, result.clone()));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create_input() -> CreateFeedback {
        CreateFeedback {
            visibility: private_visibility(),
            title: "Test".into(),
            description: "Synthetic".into(),
            category: "bug".into(),
            model: None,
            request_id: None,
            client_request_id: uuid::Uuid::new_v4().to_string(),
        }
    }
    fn cmd(version: i64, action: serde_json::Value) -> FeedbackCommand {
        let mut value = serde_json::json!({
            "expected_version": version,
            "client_request_id": uuid::Uuid::new_v4().to_string(),
        });
        value
            .as_object_mut()
            .unwrap()
            .extend(action.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn edit_and_delete_commands_accept_only_their_own_fields() {
        let ok = |v: serde_json::Value| serde_json::from_value::<FeedbackCommand>(v).is_ok();
        let base = |extra: serde_json::Value| {
            let mut v = serde_json::json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string()});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            v
        };
        assert!(ok(base(
            serde_json::json!({"action":"edit_issue","title":"t"})
        )));
        assert!(ok(base(
            serde_json::json!({"action":"edit_reply","seq":3,"content":"c"})
        )));
        assert!(ok(base(
            serde_json::json!({"action":"delete_reply","seq":3})
        )));
        assert!(ok(base(serde_json::json!({"action":"delete_issue"}))));
        // Foreign fields and missing fields are rejected, including on the old actions.
        assert!(!ok(base(
            serde_json::json!({"action":"delete_issue","seq":3})
        )));
        assert!(!ok(base(serde_json::json!({"action":"delete_reply"}))));
        assert!(!ok(base(
            serde_json::json!({"action":"edit_reply","seq":3})
        )));
        assert!(!ok(base(
            serde_json::json!({"action":"edit_issue","seq":3,"title":"t"})
        )));
        assert!(!ok(base(
            serde_json::json!({"action":"reply","content":"c","title":"t"})
        )));
        assert!(!ok(base(
            serde_json::json!({"action":"set_pinned","pinned":true,"seq":1})
        )));
    }

    #[tokio::test]
    async fn owner_and_admin_edit_issue_but_strangers_cannot_and_no_op_conflicts() {
        let r = InMemoryFeedbackRepository::default();
        let mut input = create_input();
        input.visibility = "public".into();
        let issue = r.create("owner", "user", &input).await.unwrap().issue;
        let id = issue.summary.id.clone();
        let edit = |v| {
            cmd(
                v,
                serde_json::json!({"action":"edit_issue","title":"New title","description":"New body","category":"model"}),
            )
        };
        // A different ordinary user can see a public issue but not change it.
        assert!(matches!(
            r.command(&id, "stranger", "user", true, &edit(1)).await,
            Err(FeedbackError::Forbidden)
        ));
        let edited = r
            .command(&id, "owner", "user", true, &edit(1))
            .await
            .unwrap()
            .issue;
        assert_eq!(edited.summary.title, "New title");
        assert_eq!(edited.description, "New body");
        assert_eq!(edited.summary.category, "model");
        assert!(edited.summary.edited_at.is_some());
        assert_eq!(edited.summary.version, 2);
        // Nothing left to change: conflict instead of a silent version bump.
        let same = cmd(
            2,
            serde_json::json!({"action":"edit_issue","title":"New title"}),
        );
        assert!(matches!(
            r.command(&id, "owner", "user", true, &same).await,
            Err(FeedbackError::Conflict)
        ));
        // Administrators may edit any issue; the marker event is visible to the owner only.
        let by_admin = cmd(
            2,
            serde_json::json!({"action":"edit_issue","title":"Triaged"}),
        );
        let triaged = r
            .command(&id, "root", "admin", false, &by_admin)
            .await
            .unwrap()
            .issue;
        assert_eq!(triaged.summary.title, "Triaged");
        let events = |viewer: Option<&str>| {
            let r = &r;
            let id = id.clone();
            let viewer = viewer.map(str::to_owned);
            async move {
                r.detail(&id, viewer.as_deref(), 0, 100)
                    .await
                    .unwrap()
                    .events
            }
        };
        assert!(events(Some("owner"))
            .await
            .iter()
            .any(|e| e.kind == "issue_edit"));
        assert!(!events(Some("stranger"))
            .await
            .iter()
            .any(|e| e.kind == "issue_edit"));
        // Invalid edits never reach storage.
        for bad in [
            serde_json::json!({"action":"edit_issue","title":"  "}),
            serde_json::json!({"action":"edit_issue","category":"nope"}),
            serde_json::json!({"action":"edit_issue","description":""}),
        ] {
            assert!(matches!(
                r.command(&id, "owner", "user", true, &cmd(3, bad)).await,
                Err(FeedbackError::Invalid(_))
            ));
        }
    }

    #[tokio::test]
    async fn replies_are_edited_by_their_author_and_deleted_by_author_or_admin() {
        let r = InMemoryFeedbackRepository::default();
        let mut input = create_input();
        input.visibility = "public".into();
        let issue = r.create("owner", "user", &input).await.unwrap().issue;
        let id = issue.summary.id.clone();
        let reply = |v, text: &str| {
            cmd(
                v,
                serde_json::json!({"action":"reply","content":text,"visibility":"public"}),
            )
        };
        r.command(&id, "owner", "user", true, &reply(1, "mine"))
            .await
            .unwrap();
        r.command(&id, "helper", "user", true, &reply(2, "theirs"))
            .await
            .unwrap();
        let events = r.detail(&id, None, 0, 100).await.unwrap().events;
        let mine = events
            .iter()
            .find(|e| e.content.as_deref() == Some("mine"))
            .unwrap()
            .seq;
        let theirs = events
            .iter()
            .find(|e| e.content.as_deref() == Some("theirs"))
            .unwrap()
            .seq;

        // Only the author edits; not the issue owner, not even an administrator.
        let edit = |v, seq| {
            cmd(
                v,
                serde_json::json!({"action":"edit_reply","seq":seq,"content":"fixed"}),
            )
        };
        assert!(matches!(
            r.command(&id, "owner", "user", true, &edit(3, theirs))
                .await,
            Err(FeedbackError::Forbidden)
        ));
        assert!(matches!(
            r.command(&id, "root", "admin", false, &edit(3, theirs))
                .await,
            Err(FeedbackError::Forbidden)
        ));
        let after = r
            .command(&id, "helper", "user", true, &edit(3, theirs))
            .await
            .unwrap()
            .issue;
        assert_eq!(after.summary.version, 4);
        let events = r
            .detail(&id, Some("stranger"), 0, 100)
            .await
            .unwrap()
            .events;
        let changed = events.iter().find(|e| e.seq == theirs).unwrap();
        assert_eq!(changed.content.as_deref(), Some("fixed"));
        assert!(changed.edited_at.is_some());
        // Repeating the same text is not a change.
        assert!(matches!(
            r.command(&id, "helper", "user", true, &edit(4, theirs))
                .await,
            Err(FeedbackError::Conflict)
        ));

        // Deleting: the issue owner cannot remove someone else's reply, an administrator can.
        let del = |v, seq| cmd(v, serde_json::json!({"action":"delete_reply","seq":seq}));
        assert!(matches!(
            r.command(&id, "owner", "user", true, &del(4, theirs)).await,
            Err(FeedbackError::Forbidden)
        ));
        r.command(&id, "root", "admin", false, &del(4, theirs))
            .await
            .unwrap();
        r.command(&id, "owner", "user", true, &del(5, mine))
            .await
            .unwrap();
        let events = r
            .detail(&id, Some("stranger"), 0, 100)
            .await
            .unwrap()
            .events;
        for seq in [mine, theirs] {
            let gone = events.iter().find(|e| e.seq == seq).unwrap();
            assert_eq!(gone.content, None, "deleted content must not remain");
            assert!(gone.deleted_at.is_some());
        }
        // A deleted reply cannot be edited or deleted again, and other kinds are not replies.
        assert!(matches!(
            r.command(&id, "helper", "user", true, &edit(6, theirs))
                .await,
            Err(FeedbackError::NotFound)
        ));
        assert!(matches!(
            r.command(&id, "root", "admin", false, &del(6, theirs))
                .await,
            Err(FeedbackError::NotFound)
        ));
        let created = events.iter().find(|e| e.kind == "created").unwrap().seq;
        assert!(matches!(
            r.command(&id, "owner", "user", true, &del(6, created))
                .await,
            Err(FeedbackError::NotFound)
        ));
        assert!(matches!(
            r.command(&id, "owner", "user", true, &del(6, 9999)).await,
            Err(FeedbackError::NotFound)
        ));
    }

    #[tokio::test]
    async fn deleting_an_issue_hides_it_keeps_the_number_and_tells_incremental_readers() {
        let r = InMemoryFeedbackRepository::default();
        let first = r
            .create("owner", "user", &create_input())
            .await
            .unwrap()
            .issue;
        let id = first.summary.id.clone();
        let number = first.summary.number;
        let del = cmd(1, serde_json::json!({"action":"delete_issue"}));
        // A stranger cannot delete; the owner can.
        assert!(matches!(
            r.command(&id, "stranger", "user", true, &del).await,
            Err(FeedbackError::NotFound) | Err(FeedbackError::Forbidden)
        ));
        let deleted = r.command(&id, "owner", "user", true, &del).await.unwrap();
        assert!(deleted.issue.summary.deleted_at.is_some());
        // Retrying the same command is a replay, not an error.
        assert!(
            r.command(&id, "owner", "user", true, &del)
                .await
                .unwrap()
                .replayed
        );
        // Gone for everyone, including administrators; later commands find nothing.
        for viewer in [Some("owner"), None] {
            assert!(matches!(
                r.detail(&id, viewer, 0, 10).await,
                Err(FeedbackError::NotFound)
            ));
        }
        assert_eq!(r.id_for_number(number).await.unwrap(), None);
        let reply = cmd(2, serde_json::json!({"action":"reply","content":"late"}));
        assert!(matches!(
            r.command(&id, "owner", "user", true, &reply).await,
            Err(FeedbackError::NotFound)
        ));
        let listed = |after: Option<i64>| {
            let r = &r;
            async move {
                r.list(&FeedbackQuery {
                    viewer_id: None,
                    after,
                    limit: 100,
                    ..Default::default()
                })
                .await
                .unwrap()
            }
        };
        assert_eq!(listed(None).await.total, 0);
        // Incremental readers receive the tombstone so they can drop the issue.
        let tomb = listed(Some(0)).await;
        assert_eq!(tomb.items.len(), 1);
        assert!(tomb.items[0].deleted_at.is_some());
        // A new issue never reuses the number of a deleted one.
        let next = r
            .create("owner", "user", &create_input())
            .await
            .unwrap()
            .issue;
        assert!(next.summary.number > number);
    }

    fn mention(user: &str, issue: &str, seq: i64) -> NewFeedbackNotification {
        NewFeedbackNotification {
            user_id: user.into(),
            issue_id: issue.into(),
            event_seq: seq,
            actor_id: Some("author".into()),
            snippet: format!("mentioned at {seq}"),
        }
    }

    #[tokio::test]
    async fn notifications_are_per_user_idempotent_ordered_and_read_state_is_private() {
        let r = InMemoryFeedbackRepository::default();
        let issue = r
            .create("author", "user", &create_input())
            .await
            .unwrap()
            .issue;
        let id = issue.summary.id.clone();
        r.add_notifications(&[mention("bob", &id, 1), mention("carol", &id, 1)])
            .await
            .unwrap();
        // The same (user, issue, event) twice is one notification; a later event is another.
        r.add_notifications(&[mention("bob", &id, 1), mention("bob", &id, 2)])
            .await
            .unwrap();
        let page = r.list_notifications("bob", false, None, 20).await.unwrap();
        assert_eq!(page.unread_count, 2);
        assert_eq!(
            page.items.iter().map(|n| n.event_seq).collect::<Vec<_>>(),
            vec![2, 1],
            "newest first"
        );
        assert_eq!(page.items[0].issue_number, issue.summary.number);
        assert_eq!(page.items[0].issue_title, issue.summary.title);
        assert_eq!(
            r.list_notifications("dave", false, None, 20)
                .await
                .unwrap()
                .items
                .len(),
            0
        );

        // Paging walks back through older ones.
        let first = r.list_notifications("bob", false, None, 1).await.unwrap();
        assert_eq!(first.items.len(), 1);
        let older = r
            .list_notifications("bob", false, first.next_before, 1)
            .await
            .unwrap();
        assert_eq!(older.items[0].event_seq, 1);
        assert_eq!(older.next_before, None);

        // Reading is per user and per id.
        let bobs = page.items[0].id;
        assert_eq!(
            r.mark_notifications_read("carol", Some(&[bobs]))
                .await
                .unwrap(),
            1,
            "carol cannot read bob's"
        );
        assert_eq!(
            r.list_notifications("bob", false, None, 20)
                .await
                .unwrap()
                .unread_count,
            2
        );
        assert_eq!(
            r.mark_notifications_read("bob", Some(&[bobs]))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            r.list_notifications("bob", true, None, 20)
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(r.mark_notifications_read("bob", None).await.unwrap(), 0);
        assert_eq!(
            r.list_notifications("bob", false, None, 20)
                .await
                .unwrap()
                .items
                .len(),
            2,
            "read ones stay"
        );
    }

    #[tokio::test]
    async fn notifications_keep_only_the_newest_per_user_and_ignore_deleted_topics() {
        let r = InMemoryFeedbackRepository::default();
        let issue = r
            .create("author", "user", &create_input())
            .await
            .unwrap()
            .issue;
        let id = issue.summary.id.clone();
        let many: Vec<_> = (1..=(MAX_NOTIFICATIONS_PER_USER as i64 + 5))
            .map(|seq| mention("bob", &id, seq))
            .collect();
        r.add_notifications(&many).await.unwrap();
        let mut seen = 0;
        let mut before = None;
        let mut newest = 0;
        loop {
            let page = r
                .list_notifications("bob", false, before, 100)
                .await
                .unwrap();
            if before.is_none() {
                newest = page.items[0].event_seq;
            }
            seen += page.items.len();
            before = page.next_before;
            if before.is_none() {
                break;
            }
        }
        assert_eq!(seen, MAX_NOTIFICATIONS_PER_USER);
        assert_eq!(newest, MAX_NOTIFICATIONS_PER_USER as i64 + 5);

        // Deleting the topic hides its notifications and removes them from the unread count.
        let delete = cmd(1, serde_json::json!({"action":"delete_issue"}));
        r.command(&id, "author", "user", true, &delete)
            .await
            .unwrap();
        let page = r.list_notifications("bob", false, None, 100).await.unwrap();
        assert_eq!((page.items.len(), page.unread_count), (0, 0));
    }

    #[tokio::test]
    async fn feedback_creation_requires_known_identity_records_role_and_preserves_hashes() {
        let repo = InMemoryFeedbackRepository::default();
        let input = create_input();
        for role in [
            "user",
            "admin",
            "audit_admin",
            "user_admin",
            "service_admin",
        ] {
            let owner = format!("creator-{role}");
            let created = repo.create(&owner, role, &input).await.unwrap();
            assert_eq!(
                created.issue.summary.owner_id.as_deref(),
                Some(owner.as_str())
            );
            assert_eq!(created.issue.summary.visibility, "private");
            let replay = repo
                .create(
                    &owner,
                    if role == "user" { "admin" } else { "user" },
                    &input,
                )
                .await
                .unwrap();
            assert!(
                replay.replayed,
                "trusted role is not part of the historical payload hash"
            );
            assert_eq!(replay.issue, created.issue);
            for denied_role in ["", "unknown", "super_admin", "owner"] {
                assert!(matches!(
                    repo.create(&owner, denied_role, &input).await,
                    Err(FeedbackError::Forbidden)
                ));
            }
            let detail = repo
                .detail(&created.issue.summary.id, Some(&owner), 0, 100)
                .await
                .unwrap();
            assert_eq!(detail.events.len(), 1);
            assert_eq!(detail.events[0].actor_role, role);
            assert_eq!(detail.events[0].actor_id.as_deref(), Some(owner.as_str()));
        }
        for owner in ["", "  "] {
            assert!(matches!(
                repo.create(owner, "admin", &input).await,
                Err(FeedbackError::Forbidden)
            ));
        }
        for field in ["owner_id", "actor_id", "actor_role", "role"] {
            let mut value = serde_json::to_value(&input).unwrap();
            value[field] = serde_json::json!("forged");
            assert!(serde_json::from_value::<CreateFeedback>(value).is_err());
        }
        assert_eq!(
            repo.list(&FeedbackQuery {
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap()
            .total,
            5
        );
    }

    #[tokio::test]
    async fn feedback_repository_cas_idempotency_history_and_cursors() {
        let r = InMemoryFeedbackRepository::default();
        let input = create_input();
        let issue = r.create("owner", "user", &input).await.unwrap().issue;
        let id = &issue.summary.id;
        assert!(r.create("owner", "user", &input).await.unwrap().replayed);
        let mut mismatch = input.clone();
        mismatch.title = "Changed".into();
        assert!(matches!(
            r.create("owner", "user", &mismatch).await,
            Err(FeedbackError::Conflict)
        ));
        assert!(matches!(
            r.detail(id, Some("other"), 0, 10).await,
            Err(FeedbackError::NotFound)
        ));
        let command:FeedbackCommand=serde_json::from_value(serde_json::json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"reply"})).unwrap();
        let updated = r
            .command(id, "admin", "service_admin", false, &command)
            .await
            .unwrap();
        assert_eq!(updated.issue.summary.version, 2);
        assert!(
            r.command(id, "admin", "service_admin", false, &command)
                .await
                .unwrap()
                .replayed
        );
        let mut stale = command.clone();
        stale.client_request_id = uuid::Uuid::new_v4().to_string();
        assert!(matches!(
            r.command(id, "admin", "service_admin", false, &stale).await,
            Err(FeedbackError::Conflict)
        ));
        let detail = r.detail(id, Some("owner"), 0, 1).await.unwrap();
        assert!(detail.has_more);
        assert_eq!(detail.events.len(), 1);
        assert_eq!(
            r.detail(id, Some("owner"), detail.next_event_cursor.unwrap(), 20)
                .await
                .unwrap()
                .events
                .len(),
            1
        );
        let page = r
            .list(&FeedbackQuery {
                after: Some(issue.summary.activity_seq),
                limit: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].version, 2);
        let q = FeedbackQuery {
            owner_id: Some("other".into()),
            limit: 20,
            ..Default::default()
        };
        assert_eq!(r.list(&q).await.unwrap().total, 0);
    }
    #[tokio::test]
    async fn feedback_legacy_resolve_replay_preserves_current_snapshot_and_new_write_rules() {
        #[derive(serde::Serialize)]
        struct LegacyResolve<'a> {
            expected_version: i64,
            client_request_id: &'a str,
            action: &'a str,
            status: &'a str,
            comment: Option<&'a str>,
        }
        let repo = InMemoryFeedbackRepository::default();
        let mut input = create_input();
        input.visibility = "public".into();
        let created = repo.create("owner", "user", &input).await.unwrap().issue;
        let id = &created.summary.id;
        let legacy = command(
            1,
            serde_json::json!({"action":"set_status","status":"resolved"}),
            "private",
        );
        let legacy_preimage = key(&(
            id,
            LegacyResolve {
                expected_version: 1,
                client_request_id: &legacy.client_request_id,
                action: "set_status",
                status: "resolved",
                comment: None,
            },
        ));
        assert_eq!(key(&(id, &legacy)), legacy_preimage);
        // Seed an upgrade-era success, without running today's final-solution validation.
        {
            let mut store = repo.inner.lock().unwrap();
            store.seq += 1;
            let mut historical = created.clone();
            historical.summary.status = "resolved".into();
            historical.summary.version = 2;
            historical.summary.activity_seq = store.seq;
            let mut event = store.events[0].clone();
            event.seq = store.seq;
            event.kind = "status".into();
            event.from_status = Some("open".into());
            event.to_status = Some("resolved".into());
            store.events.push(event);
            store.issues.insert(id.clone(), historical.clone());
            store.commands.insert(
                ("owner".into(), legacy.client_request_id.clone()),
                (
                    legacy_preimage,
                    FeedbackMutation {
                        issue: historical,
                        replayed: false,
                    },
                ),
            );
        }
        let replay = repo
            .command(id, "owner", "user", true, &legacy)
            .await
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.issue.summary.status, "resolved");
        assert_eq!(replay.issue.summary.version, 2);
        let before = repo.detail(id, Some("owner"), 0, 100).await.unwrap();
        let mut new_resolve = legacy.clone();
        new_resolve.client_request_id = uuid::Uuid::new_v4().to_string();
        new_resolve.expected_version = 2;
        assert!(matches!(
            repo.command(id, "owner", "user", true, &new_resolve).await,
            Err(FeedbackError::Invalid(_))
        ));
        let mut changed = legacy.clone();
        changed.action = FeedbackAction::SetStatus {
            status: "resolved".into(),
            comment: Some("Different payload".into()),
        };
        assert!(matches!(
            repo.command(id, "owner", "user", true, &changed).await,
            Err(FeedbackError::Conflict)
        ));
        let after = repo.detail(id, Some("owner"), 0, 100).await.unwrap();
        assert_eq!(after.issue, before.issue);
        assert_eq!(after.events, before.events);
        assert!(matches!(
            repo.command(id, "participant", "user", false, &legacy)
                .await,
            Err(FeedbackError::Forbidden)
        ));
        let reopen = command(
            2,
            serde_json::json!({"action":"set_status","status":"open"}),
            "private",
        );
        repo.command(id, "owner", "user", true, &reopen)
            .await
            .unwrap();
        let reply = command(
            3,
            serde_json::json!({"action":"reply","content":"Later activity"}),
            "private",
        );
        let current = repo
            .command(id, "owner", "user", true, &reply)
            .await
            .unwrap()
            .issue;
        assert_eq!(current.summary.status, "open");
        assert_eq!(current.summary.version, 4);
        let creation_replay = repo.create("owner", "user", &input).await.unwrap();
        assert!(creation_replay.replayed);
        assert_eq!(creation_replay.issue, current);
        for old in [&legacy, &reopen] {
            let replay = repo.command(id, "owner", "user", true, old).await.unwrap();
            assert!(replay.replayed);
            assert_eq!(replay.issue, current);
        }
        assert_eq!(
            repo.detail(id, Some("owner"), 0, 100)
                .await
                .unwrap()
                .events
                .len(),
            4
        );
    }

    #[test]
    fn feedback_commands_reject_unknown_actions_and_forged_identity() {
        for input in [
            serde_json::json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"x","actor_id":"admin"}),
            serde_json::json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"delete"}),
        ] {
            assert!(serde_json::from_value::<FeedbackCommand>(input).is_err());
        }
    }

    fn command(version: i64, action: serde_json::Value, visibility: &str) -> FeedbackCommand {
        let mut action = action;
        action["expected_version"] = serde_json::json!(version);
        action["client_request_id"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
        action["visibility"] = serde_json::json!(visibility);
        serde_json::from_value(action).unwrap()
    }

    #[tokio::test]
    async fn in_memory_feedback_numbers_issues_and_only_admins_can_pin() {
        let repo = InMemoryFeedbackRepository::default();
        let first = repo.create("owner", "user", &create_input()).await.unwrap();
        let second = repo.create("owner", "user", &create_input()).await.unwrap();
        assert_eq!(
            (first.issue.summary.number, second.issue.summary.number),
            (1, 2)
        );
        assert_eq!(
            repo.id_for_number(1).await.unwrap().as_deref(),
            Some(first.issue.summary.id.as_str())
        );
        assert_eq!(repo.id_for_number(7).await.unwrap(), None);

        let pin = |version: i64, pinned: bool| FeedbackCommand {
            visibility: private_visibility(),
            expected_version: version,
            client_request_id: uuid::Uuid::new_v4().to_string(),
            action: FeedbackAction::SetPinned { pinned },
        };
        let id = &first.issue.summary.id;
        assert!(matches!(
            repo.command(id, "owner", "user", true, &pin(1, true)).await,
            Err(FeedbackError::Forbidden)
        ));
        let pinned = repo
            .command(id, "root", "admin", false, &pin(1, true))
            .await
            .unwrap();
        assert!(pinned.issue.summary.pinned_at.is_some());
        assert!(matches!(
            repo.command(id, "root", "admin", false, &pin(2, true))
                .await,
            Err(FeedbackError::Conflict)
        ));
        let page = repo
            .list(&FeedbackQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.items[0].id, *id, "pinned issue sorts first");
        let detail = repo.detail(id, Some("owner"), 0, 50).await.unwrap();
        assert!(detail
            .events
            .iter()
            .any(|event| event.kind == "pin" && event.content.as_deref() == Some("pinned")));
    }
    #[tokio::test]
    async fn feedback_public_visibility_private_events_cursor_and_lifecycle_are_authoritative() {
        use serde_json::json;
        let repo = InMemoryFeedbackRepository::default();
        let private = repo
            .create("owner", "user", &create_input())
            .await
            .unwrap()
            .issue;
        let mut public_input = create_input();
        public_input.visibility = "public".into();
        let public = repo
            .create("owner", "user", &public_input)
            .await
            .unwrap()
            .issue;
        let id = &public.summary.id;
        let ordinary = FeedbackQuery {
            viewer_id: Some("participant".into()),
            limit: 20,
            ..Default::default()
        };
        assert_eq!(repo.list(&ordinary).await.unwrap().items.len(), 1);
        assert_eq!(
            repo.list(&FeedbackQuery {
                viewer_id: Some("owner".into()),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap()
            .items
            .len(),
            2
        );
        assert!(
            matches!(
                repo.list(&FeedbackQuery {
                    viewer_id: Some("participant".into()),
                    owner_id: Some("owner".into()),
                    limit: 20,
                    ..Default::default()
                })
                .await,
                Err(FeedbackError::Forbidden)
            ),
            "owner filter is administrative, never viewer identity"
        );
        assert!(matches!(
            repo.detail(&private.summary.id, Some("participant"), 0, 20)
                .await,
            Err(FeedbackError::NotFound)
        ));
        let leaked = command(
            1,
            json!({"action":"reply","content":"public reply on private issue"}),
            "public",
        );
        repo.command(&private.summary.id, "admin", "admin", false, &leaked)
            .await
            .unwrap();
        assert!(matches!(
            repo.detail(&private.summary.id, Some("participant"), 0, 20)
                .await,
            Err(FeedbackError::NotFound)
        ));
        // Even a false legacy owner_only flag is never an administration grant.
        assert!(matches!(
            repo.command(&private.summary.id, "participant", "user", false, &leaked)
                .await,
            Err(FeedbackError::NotFound)
        ));
        let reply = command(
            1,
            json!({"action":"reply","content":"private participant reply"}),
            "private",
        );
        repo.command(id, "participant", "user", false, &reply)
            .await
            .unwrap();
        let hidden_page = repo
            .detail(id, Some("spectator"), public.summary.activity_seq, 1)
            .await
            .unwrap();
        assert!(hidden_page.events.is_empty());
        assert!(hidden_page.next_event_cursor.unwrap() > public.summary.activity_seq);
        let reply_cursor = hidden_page.next_event_cursor.unwrap();
        for viewer in [None, Some("owner"), Some("participant")] {
            assert!(repo
                .detail(id, viewer, 0, 20)
                .await
                .unwrap()
                .events
                .iter()
                .any(|e| e.content.as_deref() == Some("private participant reply")));
        }
        let public_reply = command(
            2,
            json!({"action":"reply","content":"public participant reply"}),
            "public",
        );
        repo.command(id, "participant", "user", true, &public_reply)
            .await
            .unwrap();
        assert!(repo
            .detail(id, Some("spectator"), reply_cursor, 1)
            .await
            .unwrap()
            .events
            .iter()
            .any(|e| e.content.as_deref() == Some("public participant reply")));
        for status in ["resolved", "open", "closed"] {
            let attempted = command(
                3,
                json!({"action":"set_status","status":status,"comment":"Not the owner"}),
                "public",
            );
            assert!(matches!(
                repo.command(id, "participant", "user", false, &attempted)
                    .await,
                Err(FeedbackError::Forbidden)
            ));
        }
        for comment in [None, Some("   "), Some("")] {
            let attempted = command(
                3,
                json!({"action":"set_status","status":"resolved","comment":comment}),
                "private",
            );
            assert!(matches!(
                repo.command(id, "owner", "user", true, &attempted).await,
                Err(FeedbackError::Invalid(_))
            ));
        }
        let oversized = command(
            3,
            json!({"action":"set_status","status":"resolved","comment":"解".repeat(6667)}),
            "private",
        );
        assert!(matches!(
            repo.command(id, "owner", "user", true, &oversized).await,
            Err(FeedbackError::Invalid(_))
        ));
        assert_eq!(
            repo.detail(id, Some("owner"), 0, 20)
                .await
                .unwrap()
                .issue
                .summary
                .version,
            3
        );
        let resolve = command(
            3,
            json!({"action":"set_status","status":"resolved","comment":"Final private solution"}),
            "private",
        );
        repo.command(id, "owner", "user", true, &resolve)
            .await
            .unwrap();
        let visible = repo.detail(id, Some("spectator"), 0, 100).await.unwrap();
        let status = visible.events.iter().find(|e| e.kind == "status").unwrap();
        assert_eq!(status.to_status.as_deref(), Some("resolved"));
        assert!(status.content.is_none());
        assert!(repo
            .detail(id, Some("owner"), 0, 100)
            .await
            .unwrap()
            .events
            .iter()
            .any(|e| e.content.as_deref() == Some("Final private solution")));
        let close = command(
            4,
            json!({"action":"set_status","status":"closed"}),
            "private",
        );
        assert!(matches!(
            repo.command(id, "owner", "user", true, &close).await,
            Err(FeedbackError::Forbidden)
        ));
        // Authority is checked before an exact replay; a demoted non-owner cannot replay a close.
        repo.command(id, "admin", "audit_admin", false, &close)
            .await
            .unwrap();
        assert!(matches!(
            repo.command(id, "admin", "user", false, &close).await,
            Err(FeedbackError::Forbidden)
        ));
        assert!(
            repo.command(id, "admin", "audit_admin", false, &close)
                .await
                .unwrap()
                .replayed
        );
        let reopen = command(
            5,
            json!({"action":"set_status","status":"open","comment":"Reopening"}),
            "public",
        );
        repo.command(id, "owner", "user", true, &reopen)
            .await
            .unwrap();
        let mut version = 6;
        for role in ["admin", "audit_admin", "user_admin", "service_admin"] {
            let resolve = command(
                version,
                json!({"action":"set_status","status":"resolved","comment":"Final verified solution"}),
                "public",
            );
            repo.command(id, "admin", role, false, &resolve)
                .await
                .unwrap();
            version += 1;
            let close = command(
                version,
                json!({"action":"set_status","status":"closed"}),
                "private",
            );
            repo.command(id, "admin", role, false, &close)
                .await
                .unwrap();
            version += 1;
            let reopen = command(
                version,
                json!({"action":"set_status","status":"open"}),
                "private",
            );
            repo.command(id, "admin", role, false, &reopen)
                .await
                .unwrap();
            version += 1;
        }
        let owner_admin_close = command(
            version,
            json!({"action":"set_status","status":"closed"}),
            "private",
        );
        repo.command(id, "owner", "admin", false, &owner_admin_close)
            .await
            .unwrap();
        assert!(matches!(
            repo.command(id, "owner", "user", false, &owner_admin_close)
                .await,
            Err(FeedbackError::Forbidden)
        ));
        assert!(
            repo.command(id, "owner", "admin", false, &owner_admin_close)
                .await
                .unwrap()
                .replayed
        );
    }

    #[test]
    fn feedback_visibility_defaults_and_legacy_idempotency_hash_preimages_are_preserved() {
        use serde_json::json;
        let legacy_summary: FeedbackSummary = serde_json::from_value(json!({"id":"legacy","owner_id":"owner","owner_username":null,"title":"Legacy","category":"bug","model":null,"request_id":null,"status":"open","version":1,"activity_seq":1,"created_at":1,"updated_at":1})).unwrap();
        assert_eq!(legacy_summary.visibility, "private");
        assert_eq!(
            serde_json::to_value(legacy_summary).unwrap()["visibility"],
            "private"
        );
        let legacy_event: FeedbackEvent = serde_json::from_value(json!({"seq":1,"issue_id":"legacy","actor_id":"owner","actor_username":null,"actor_role":"user","kind":"reply","content":"Legacy private","from_status":null,"to_status":null,"created_at":1})).unwrap();
        assert_eq!(legacy_event.visibility, "private");
        assert_eq!(
            serde_json::to_value(legacy_event).unwrap()["visibility"],
            "private"
        );
        let mut raw = serde_json::to_value(create_input()).unwrap();
        assert!(raw.get("visibility").is_none());
        let old: CreateFeedback = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(old.visibility, "private");
        raw["visibility"] = json!("private");
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<CreateFeedback>(raw).unwrap()).unwrap(),
            serde_json::to_value(old).unwrap()
        );
        let old = json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"legacy"});
        let parsed: FeedbackCommand = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(parsed.visibility, "private");
        assert_eq!(serde_json::to_value(parsed).unwrap(), old);
        for visibility in ["unknown", "", "PUBLIC"] {
            let mut input = create_input();
            input.visibility = visibility.into();
            assert!(validate_create(&input).is_err());
            let input = command(1, json!({"action":"reply","content":"x"}), visibility);
            assert!(validate_command(&input).is_err());
        }
    }
}
