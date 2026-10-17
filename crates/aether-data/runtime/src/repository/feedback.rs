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
                can_view_feedback(i, q.viewer_id.as_deref())
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
            .filter(|i| can_view_feedback(&i.summary, viewer))
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
            .find(|issue| issue.summary.number == number)
            .map(|issue| issue.summary.id.clone()))
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
        // Successful pre-upgrade commands may predate today's write validation.
        validate_command(input)?;
        if issue.summary.version != input.expected_version {
            return Err(FeedbackError::Conflict);
        }
        store.seq += 1;
        let seq = store.seq;
        let now = chrono::Utc::now().timestamp();
        let mut issue = issue;
        let (kind, content, from_status, to_status) = match &input.action {
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
            // Pin changes are system events that every viewer may see.
            visibility: if kind == "pin" {
                "public".into()
            } else {
                input.visibility.clone()
            },
            content,
            from_status,
            to_status,
            created_at: now,
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
