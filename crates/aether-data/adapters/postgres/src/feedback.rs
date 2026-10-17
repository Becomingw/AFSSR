use aether_data_contracts::repository::feedback::*;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

#[derive(Debug, Clone)]
pub struct PostgresFeedbackRepository {
    pool: PgPool,
}
impl PostgresFeedbackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
fn unavailable(_: sqlx::Error) -> FeedbackError {
    FeedbackError::Unavailable
}
fn hash(value: serde_json::Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
const ISSUE_SELECT:&str="SELECT (to_jsonb(i)-'creation_key'-'creation_hash') || jsonb_build_object('owner_username',u.username) AS data FROM feedback_issues i LEFT JOIN users u ON u.id=i.owner_id";
async fn issue_tx(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    id: &str,
) -> Result<FeedbackIssue, FeedbackError> {
    let row = sqlx::query(&format!("{ISSUE_SELECT} WHERE i.id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?
        .ok_or(FeedbackError::NotFound)?;
    serde_json::from_value(row.try_get("data").map_err(unavailable)?)
        .map_err(|_| FeedbackError::Unavailable)
}

/// Unread notifications of issues that still exist.
async fn unread_count(pool: &PgPool, user_id: &str) -> Result<i64, FeedbackError> {
    sqlx::query_scalar("SELECT COUNT(*) FROM feedback_notifications n JOIN feedback_issues i ON i.id=n.issue_id AND i.deleted_at IS NULL WHERE n.user_id=$1 AND n.read_at IS NULL")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(unavailable)
}

/// Loads (and locks) a reply of the issue; anything that is not a reply is treated as absent.
async fn load_reply(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    issue_id: &str,
    seq: i64,
) -> Result<FeedbackEvent, FeedbackError> {
    let row = sqlx::query("SELECT (to_jsonb(e)-'client_request_id'-'payload_hash') AS data FROM feedback_events e WHERE e.issue_id=$1 AND e.seq=$2 FOR UPDATE")
        .bind(issue_id)
        .bind(seq)
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?
        .ok_or(FeedbackError::NotFound)?;
    serde_json::from_value(row.try_get("data").map_err(unavailable)?)
        .map_err(|_| FeedbackError::Unavailable)
}

#[async_trait]
impl FeedbackRepository for PostgresFeedbackRepository {
    async fn list(&self, q: &FeedbackQuery) -> Result<FeedbackPage, FeedbackError> {
        if q.viewer_id.is_some() && q.owner_id.is_some() {
            return Err(FeedbackError::Forbidden);
        }
        let mut b=QueryBuilder::<Postgres>::new("WITH filtered AS (SELECT i.*,u.username AS owner_username FROM feedback_issues i LEFT JOIN users u ON u.id=i.owner_id WHERE TRUE");
        if q.after.is_none() {
            // Incremental readers also receive deleted issues (as tombstones) so they can drop them.
            b.push(" AND i.deleted_at IS NULL");
        }
        if let Some(viewer) = &q.viewer_id {
            b.push(" AND (i.visibility='public' OR i.owner_id=")
                .push_bind(viewer)
                .push(")");
        }
        if let Some(owner) = &q.owner_id {
            b.push(" AND i.owner_id=").push_bind(owner);
        }
        if let Some(status) = &q.status {
            b.push(" AND i.status=").push_bind(status);
        }
        if let Some(category) = &q.category {
            b.push(" AND i.category=").push_bind(category);
        }
        if let Some(after) = q.after {
            b.push(" AND i.activity_seq>").push_bind(after);
        }
        if let Some(search) = &q.search {
            b.push(" AND strpos(lower(i.title),lower(")
                .push_bind(search)
                .push("))>0");
        }
        b.push(") SELECT jsonb_build_object('total',(SELECT COUNT(*) FROM filtered),'items',COALESCE((SELECT jsonb_agg(to_jsonb(p)-'description'-'creation_key'-'creation_hash') FROM (SELECT * FROM filtered ORDER BY ");
        b.push(if q.after.is_some() {
            "activity_seq ASC"
        } else {
            "(pinned_at IS NULL), pinned_at DESC, activity_seq DESC"
        })
        .push(" LIMIT ")
        .push_bind(q.limit.clamp(1, 100) as i64)
        .push(" OFFSET ")
        .push_bind(q.offset as i64)
        .push(") p),'[]'::jsonb)) AS data");
        let row = b.build().fetch_one(&self.pool).await.map_err(unavailable)?;
        let data: serde_json::Value = row.try_get("data").map_err(unavailable)?;
        let items: Vec<FeedbackSummary> = serde_json::from_value(data["items"].clone())
            .map_err(|_| FeedbackError::Unavailable)?;
        let next_cursor = items.iter().map(|i| i.activity_seq).max().or(q.after);
        Ok(FeedbackPage {
            items,
            total: data["total"].as_i64().unwrap_or(0),
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
        // A single snapshot keeps version and event history consistent for CAS clients.
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        let issue = issue_tx(&mut tx, id).await?;
        if issue.summary.deleted_at.is_some() || !can_view_feedback(&issue.summary, viewer) {
            return Err(FeedbackError::NotFound);
        }
        let rows=sqlx::query("SELECT (to_jsonb(e)-'client_request_id'-'payload_hash') || jsonb_build_object('actor_username',u.username) AS data FROM feedback_events e LEFT JOIN users u ON u.id=e.actor_id WHERE e.issue_id=$1 AND e.seq>$2 ORDER BY e.seq LIMIT $3")
            .bind(id).bind(after).bind(limit.clamp(1,100) as i64+1).fetch_all(&mut *tx).await.map_err(unavailable)?;
        let has_more = rows.len() > limit.clamp(1, 100);
        let scanned: Vec<FeedbackEvent> = rows
            .into_iter()
            .take(limit.clamp(1, 100))
            .map(|row| {
                serde_json::from_value(row.try_get("data").map_err(unavailable)?)
                    .map_err(|_| FeedbackError::Unavailable)
            })
            .collect::<Result<_, _>>()?;
        let next_event_cursor = scanned.last().map(|e| e.seq).or(Some(after));
        let events = scanned
            .iter()
            .filter_map(|e| project_feedback_event(e, &issue.summary, viewer))
            .collect();
        tx.commit().await.map_err(unavailable)?;
        Ok(FeedbackDetail {
            issue,
            next_event_cursor,
            events,
            has_more,
        })
    }
    async fn id_for_number(&self, number: i64) -> Result<Option<String>, FeedbackError> {
        sqlx::query_scalar("SELECT id FROM feedback_issues WHERE number=$1 AND deleted_at IS NULL")
            .bind(number)
            .fetch_optional(&self.pool)
            .await
            .map_err(unavailable)
    }
    async fn add_notifications(
        &self,
        items: &[NewFeedbackNotification],
    ) -> Result<(), FeedbackError> {
        if items.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        let mut users: Vec<&str> = Vec::new();
        for item in items {
            let snippet: String = item.snippet.chars().take(500).collect();
            sqlx::query("INSERT INTO feedback_notifications(user_id,kind,issue_id,event_seq,actor_id,snippet,created_at) VALUES($1,'mention',$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
                .bind(&item.user_id)
                .bind(&item.issue_id)
                .bind(item.event_seq)
                .bind(&item.actor_id)
                .bind(snippet)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
            if !users.contains(&item.user_id.as_str()) {
                users.push(&item.user_id);
            }
        }
        for user in users {
            sqlx::query("DELETE FROM feedback_notifications WHERE user_id=$1 AND id NOT IN (SELECT id FROM feedback_notifications WHERE user_id=$1 ORDER BY id DESC LIMIT $2)")
                .bind(user)
                .bind(MAX_NOTIFICATIONS_PER_USER as i64)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
        }
        tx.commit().await.map_err(unavailable)
    }
    async fn list_notifications(
        &self,
        user_id: &str,
        unread_only: bool,
        before: Option<i64>,
        limit: usize,
    ) -> Result<NotificationPage, FeedbackError> {
        let limit = limit.clamp(1, 100);
        let rows = sqlx::query("SELECT n.id,n.kind,n.issue_id,i.number AS issue_number,i.title AS issue_title,n.event_seq,a.username AS actor_username,n.snippet,n.created_at,n.read_at FROM feedback_notifications n JOIN feedback_issues i ON i.id=n.issue_id AND i.deleted_at IS NULL LEFT JOIN users a ON a.id=n.actor_id WHERE n.user_id=$1 AND ($2 IS FALSE OR n.read_at IS NULL) AND ($3::bigint IS NULL OR n.id<$3) ORDER BY n.id DESC LIMIT $4")
            .bind(user_id)
            .bind(unread_only)
            .bind(before)
            .bind(limit as i64 + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(unavailable)?;
        let has_more = rows.len() > limit;
        let items: Vec<FeedbackNotification> = rows
            .iter()
            .take(limit)
            .map(|row| {
                Ok(FeedbackNotification {
                    id: row.try_get("id").map_err(unavailable)?,
                    kind: row.try_get("kind").map_err(unavailable)?,
                    issue_id: row.try_get("issue_id").map_err(unavailable)?,
                    issue_number: row.try_get("issue_number").map_err(unavailable)?,
                    issue_title: row.try_get("issue_title").map_err(unavailable)?,
                    event_seq: row.try_get("event_seq").map_err(unavailable)?,
                    actor_username: row.try_get("actor_username").map_err(unavailable)?,
                    snippet: row.try_get("snippet").map_err(unavailable)?,
                    created_at: row.try_get("created_at").map_err(unavailable)?,
                    read_at: row.try_get("read_at").map_err(unavailable)?,
                })
            })
            .collect::<Result<_, FeedbackError>>()?;
        let next_before = has_more.then(|| items.last().map(|n| n.id)).flatten();
        let unread_count = unread_count(&self.pool, user_id).await?;
        Ok(NotificationPage {
            items,
            unread_count,
            next_before,
        })
    }
    async fn mark_notifications_read(
        &self,
        user_id: &str,
        ids: Option<&[i64]>,
    ) -> Result<i64, FeedbackError> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query("UPDATE feedback_notifications SET read_at=$3 WHERE user_id=$1 AND read_at IS NULL AND ($2::bigint[] IS NULL OR id=ANY($2))")
            .bind(user_id)
            .bind(ids.map(<[i64]>::to_vec))
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(unavailable)?;
        unread_count(&self.pool, user_id).await
    }
    async fn create(
        &self,
        owner: &str,
        role: &str,
        input: &CreateFeedback,
    ) -> Result<FeedbackMutation, FeedbackError> {
        authorize_feedback_create(owner, role)?;
        validate_create(input)?;
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        // Sequence allocation must commit in order or incremental pollers could skip a transaction.
        sqlx::query("SELECT pg_advisory_xact_lock(1939824281)")
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        let digest = hash(serde_json::to_value(input).expect("feedback input serializes"));
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let inserted=sqlx::query("INSERT INTO feedback_issues(id,owner_id,title,description,category,model,request_id,status,version,activity_seq,created_at,updated_at,creation_key,creation_hash,visibility,number) VALUES($1,$2,$3,$4,$5,$6,$7,'open',1,0,$8,$8,$9,$10,$11,(SELECT COALESCE(MAX(number),0)+1 FROM feedback_issues)) ON CONFLICT(owner_id,creation_key) DO NOTHING RETURNING id")
            .bind(&id).bind(owner).bind(&input.title).bind(&input.description).bind(&input.category).bind(&input.model).bind(&input.request_id).bind(now).bind(&input.client_request_id).bind(&digest).bind(&input.visibility).fetch_optional(&mut *tx).await.map_err(unavailable)?;
        let (id, replayed) = if inserted.is_some() {
            let seq:i64=sqlx::query_scalar("INSERT INTO feedback_events(issue_id,actor_id,actor_role,kind,to_status,created_at) VALUES($1,$2,$3,'created','open',$4) RETURNING seq").bind(&id).bind(owner).bind(role).bind(now).fetch_one(&mut *tx).await.map_err(unavailable)?;
            sqlx::query("UPDATE feedback_issues SET activity_seq=$2 WHERE id=$1")
                .bind(&id)
                .bind(seq)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
            (id, false)
        } else {
            let row=sqlx::query("SELECT id,creation_hash FROM feedback_issues WHERE owner_id=$1 AND creation_key=$2").bind(owner).bind(&input.client_request_id).fetch_one(&mut *tx).await.map_err(unavailable)?;
            if row
                .try_get::<String, _>("creation_hash")
                .map_err(unavailable)?
                != digest
            {
                return Err(FeedbackError::Conflict);
            }
            (row.try_get("id").map_err(unavailable)?, true)
        };
        let issue = issue_tx(&mut tx, &id).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(FeedbackMutation { issue, replayed })
    }
    async fn command(
        &self,
        id: &str,
        actor: &str,
        role: &str,
        _owner_only: bool,
        input: &FeedbackCommand,
    ) -> Result<FeedbackMutation, FeedbackError> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(1939824281)")
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        let found = sqlx::query("SELECT id FROM feedback_issues WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(unavailable)?;
        if found.is_none() {
            return Err(FeedbackError::NotFound);
        }
        let issue = issue_tx(&mut tx, id).await?;
        authorize_feedback_command(&issue.summary, actor, role, input)?;
        let digest = hash(serde_json::json!([id, input]));
        if let Some(row) = sqlx::query(
            "SELECT payload_hash FROM feedback_events WHERE actor_id=$1 AND client_request_id=$2",
        )
        .bind(actor)
        .bind(&input.client_request_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?
        {
            if row
                .try_get::<String, _>("payload_hash")
                .map_err(unavailable)?
                != digest
            {
                return Err(FeedbackError::Conflict);
            }
            let issue = issue_tx(&mut tx, id).await?;
            tx.commit().await.map_err(unavailable)?;
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
        let mut pin_change: Option<bool> = None;
        let mut target_seq: Option<i64> = None;
        let mut edit: (Option<String>, Option<String>, Option<String>) = (None, None, None);
        let mut edited_issue = false;
        let mut deleted_issue = false;
        let owned_content: Option<String>;
        let (kind, content, from_status, to_status) = match &input.action {
            FeedbackAction::Reply { content } => ("reply", Some(content.as_str()), None, None),
            FeedbackAction::EditIssue {
                title,
                description,
                category,
            } => {
                // Only fields that really differ count; an edit that changes nothing is a conflict.
                edit.0 = title.clone().filter(|v| v != &issue.summary.title);
                edit.1 = description.clone().filter(|v| v != &issue.description);
                edit.2 = category.clone().filter(|v| v != &issue.summary.category);
                let changed: Vec<&str> = [
                    ("title", edit.0.is_some()),
                    ("description", edit.1.is_some()),
                    ("category", edit.2.is_some()),
                ]
                .iter()
                .filter(|(_, c)| *c)
                .map(|(n, _)| *n)
                .collect();
                if changed.is_empty() {
                    return Err(FeedbackError::Conflict);
                }
                edited_issue = true;
                owned_content = Some(changed.join(","));
                ("issue_edit", owned_content.as_deref(), None, None)
            }
            FeedbackAction::DeleteIssue => {
                deleted_issue = true;
                ("issue_delete", None, None, None)
            }
            FeedbackAction::EditReply { seq, content } => {
                let reply = load_reply(&mut tx, id, *seq).await?;
                authorize_feedback_reply_change(&reply, actor, role, false)?;
                if reply.content.as_deref() == Some(content.as_str()) {
                    return Err(FeedbackError::Conflict);
                }
                sqlx::query("UPDATE feedback_events SET content=$2,edited_at=$3 WHERE seq=$1")
                    .bind(seq)
                    .bind(content)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(unavailable)?;
                target_seq = Some(*seq);
                ("reply_edit", None, None, None)
            }
            FeedbackAction::DeleteReply { seq } => {
                let reply = load_reply(&mut tx, id, *seq).await?;
                authorize_feedback_reply_change(&reply, actor, role, true)?;
                sqlx::query("UPDATE feedback_events SET content=NULL,deleted_at=$2 WHERE seq=$1")
                    .bind(seq)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(unavailable)?;
                target_seq = Some(*seq);
                ("reply_delete", None, None, None)
            }
            FeedbackAction::SetPinned { pinned } => {
                if *pinned == issue.summary.pinned_at.is_some() {
                    return Err(FeedbackError::Conflict);
                }
                pin_change = Some(*pinned);
                (
                    "pin",
                    Some(if *pinned { "pinned" } else { "unpinned" }),
                    None,
                    None,
                )
            }
            FeedbackAction::SetStatus { status, comment } => {
                if status == &issue.summary.status {
                    return Err(FeedbackError::Conflict);
                }
                (
                    "status",
                    comment.as_deref(),
                    Some(issue.summary.status.as_str()),
                    Some(status.as_str()),
                )
            }
        };
        // Pin changes are public system events; edit/delete markers are only for the people involved.
        let event_visibility = match kind {
            "pin" => "public",
            "issue_edit" | "issue_delete" | "reply_edit" | "reply_delete" => "private",
            _ => input.visibility.as_str(),
        };
        let seq:i64=sqlx::query_scalar("INSERT INTO feedback_events(issue_id,actor_id,actor_role,kind,content,from_status,to_status,created_at,client_request_id,payload_hash,visibility,target_seq) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING seq")
            .bind(id).bind(actor).bind(role).bind(kind).bind(content).bind(from_status).bind(to_status).bind(now).bind(&input.client_request_id).bind(digest).bind(event_visibility).bind(target_seq).fetch_one(&mut *tx).await.map_err(unavailable)?;
        sqlx::query("UPDATE feedback_issues SET status=COALESCE($2,status),version=version+1,activity_seq=$3,updated_at=$4,pinned_at=CASE WHEN $5::boolean IS NULL THEN pinned_at WHEN $5 THEN $4 ELSE NULL END,title=COALESCE($6,title),description=COALESCE($7,description),category=COALESCE($8,category),edited_at=CASE WHEN $9 THEN $4 ELSE edited_at END,deleted_at=CASE WHEN $10 THEN $4 ELSE deleted_at END WHERE id=$1").bind(id).bind(to_status).bind(seq).bind(now).bind(pin_change).bind(&edit.0).bind(&edit.1).bind(&edit.2).bind(edited_issue).bind(deleted_issue).execute(&mut *tx).await.map_err(unavailable)?;
        let issue = issue_tx(&mut tx, id).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(FeedbackMutation {
            issue,
            replayed: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires a disposable loopback PostgreSQL afssr_feedback_test database"]
    async fn feedback_postgres_migration_persistence_cas_and_feed() {
        use std::str::FromStr;
        let url = std::env::var("AFSSR_FEEDBACK_TEST_DATABASE_URL")
            .expect("disposable feedback test database required");
        let options = sqlx::postgres::PgConnectOptions::from_str(&url).unwrap();
        assert!(matches!(options.get_host(), "127.0.0.1" | "localhost"));
        assert_eq!(options.get_database(), Some("afssr_feedback_test"));
        let schema = format!("feedback_test_{}", uuid::Uuid::new_v4().simple());
        let setup = PgPool::connect_with(options.clone()).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&setup)
            .await
            .unwrap();
        let path = schema.clone();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .after_connect(move |connection, _| {
                let schema = path.clone();
                Box::pin(async move {
                    sqlx::query(&format!("SET search_path TO {schema}"))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE users(id VARCHAR(36) PRIMARY KEY,username VARCHAR(200) NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let migration = include_str!("../migrations/20261015000300_feedback_issues.sql");
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
        let owner = uuid::Uuid::new_v4().to_string();
        let admin = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id,username)VALUES($1,'Owner'),($2,'Admin')")
            .bind(&owner)
            .bind(&admin)
            .execute(&pool)
            .await
            .unwrap();
        // Seed a real pre-upgrade issue/reply, then apply only the additive visibility migration.
        let legacy_id = uuid::Uuid::new_v4().to_string();
        let legacy_input: CreateFeedback = serde_json::from_value(serde_json::json!({"title":"Legacy private","description":"Legacy description","category":"bug","client_request_id":uuid::Uuid::new_v4().to_string()})).unwrap();
        let legacy_hash = hash(serde_json::to_value(&legacy_input).unwrap());
        sqlx::query("INSERT INTO feedback_issues(id,owner_id,title,description,category,status,version,activity_seq,created_at,updated_at,creation_key,creation_hash) VALUES($1,$2,'Legacy private','Legacy description','bug','open',1,0,1,1,$3,$4)")
            .bind(&legacy_id).bind(&owner).bind(&legacy_input.client_request_id).bind(legacy_hash).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO feedback_events(issue_id,actor_id,actor_role,kind,content,created_at) VALUES($1,$2,'user','reply','legacy-private-reply',1)")
            .bind(&legacy_id).bind(&owner).execute(&pool).await.unwrap();
        let legacy_resolve: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"set_status","status":"resolved"})).unwrap();
        let legacy_preimage = serde_json::json!([
            legacy_id,
            {"expected_version":1,"client_request_id":legacy_resolve.client_request_id,"action":"set_status","status":"resolved","comment":null}
        ]);
        assert_eq!(
            serde_json::json!([legacy_id, legacy_resolve]),
            legacy_preimage
        );
        let legacy_seq: i64 = sqlx::query_scalar("INSERT INTO feedback_events(issue_id,actor_id,actor_role,kind,from_status,to_status,created_at,client_request_id,payload_hash) VALUES($1,$2,'user','status','open','resolved',1,$3,$4) RETURNING seq")
            .bind(&legacy_id).bind(&owner).bind(&legacy_resolve.client_request_id).bind(hash(legacy_preimage)).fetch_one(&pool).await.unwrap();
        sqlx::query(
            "UPDATE feedback_issues SET status='resolved',version=2,activity_seq=$2 WHERE id=$1",
        )
        .bind(&legacy_id)
        .bind(legacy_seq)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../migrations/20261016000000_feedback_visibility.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let repo = PostgresFeedbackRepository::new(pool.clone());
        let legacy = repo.detail(&legacy_id, Some(&owner), 0, 100).await.unwrap();
        assert_eq!(legacy.issue.summary.visibility, "private");
        assert_eq!(legacy.events[0].visibility, "private");
        assert!(matches!(
            repo.detail(&legacy_id, Some(&admin), 0, 100).await,
            Err(FeedbackError::NotFound)
        ));
        assert!(
            repo.create(&owner, "user", &legacy_input)
                .await
                .unwrap()
                .replayed,
            "old creation hash must survive default visibility"
        );
        let replay = repo
            .command(&legacy_id, &owner, "user", true, &legacy_resolve)
            .await
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.issue.summary.status, "resolved");
        assert_eq!(replay.issue.summary.version, 2);
        let mut new_resolve = legacy_resolve.clone();
        new_resolve.expected_version = 2;
        new_resolve.client_request_id = uuid::Uuid::new_v4().to_string();
        assert!(matches!(
            repo.command(&legacy_id, &owner, "user", true, &new_resolve)
                .await,
            Err(FeedbackError::Invalid(_))
        ));
        let mut changed = legacy_resolve.clone();
        changed.action = FeedbackAction::SetStatus {
            status: "resolved".into(),
            comment: Some("Different payload".into()),
        };
        assert!(matches!(
            repo.command(&legacy_id, &owner, "user", true, &changed)
                .await,
            Err(FeedbackError::Conflict)
        ));
        let unchanged = repo.detail(&legacy_id, Some(&owner), 0, 100).await.unwrap();
        assert_eq!(unchanged.issue, legacy.issue);
        assert_eq!(unchanged.events, legacy.events);
        let reopen: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":2,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"set_status","status":"open"})).unwrap();
        repo.command(&legacy_id, &owner, "user", true, &reopen)
            .await
            .unwrap();
        let later_reply: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":3,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"Later activity"})).unwrap();
        let current = repo
            .command(&legacy_id, &owner, "user", true, &later_reply)
            .await
            .unwrap()
            .issue;
        assert_eq!(current.summary.status, "open");
        assert_eq!(current.summary.version, 4);
        let creation_replay = repo.create(&owner, "user", &legacy_input).await.unwrap();
        assert!(creation_replay.replayed);
        assert_eq!(creation_replay.issue, current);
        for old in [&legacy_resolve, &reopen] {
            let replay = repo
                .command(&legacy_id, &owner, "user", true, old)
                .await
                .unwrap();
            assert!(replay.replayed);
            assert_eq!(replay.issue, current);
        }
        assert_eq!(
            repo.detail(&legacy_id, Some(&owner), 0, 100)
                .await
                .unwrap()
                .events
                .len(),
            4
        );
        let input = CreateFeedback {
            visibility: private_visibility(),
            title: "Persisted issue".into(),
            description: "Synthetic only".into(),
            category: "bug".into(),
            model: None,
            request_id: None,
            client_request_id: uuid::Uuid::new_v4().to_string(),
        };
        let created = repo.create(&owner, "user", &input).await.unwrap();
        assert_eq!(
            created.issue.summary.owner_username.as_deref(),
            Some("Owner")
        );
        let id = created.issue.summary.id.clone();
        assert!(repo.create(&owner, "user", &input).await.unwrap().replayed);
        assert!(matches!(
            repo.detail(&id, Some(&admin), 0, 20).await,
            Err(FeedbackError::NotFound)
        ));
        let first = FeedbackCommand {
            visibility: private_visibility(),
            expected_version: 1,
            client_request_id: uuid::Uuid::new_v4().to_string(),
            action: FeedbackAction::Reply {
                content: "Concurrent reply A".into(),
            },
        };
        let second = FeedbackCommand {
            visibility: private_visibility(),
            expected_version: 1,
            client_request_id: uuid::Uuid::new_v4().to_string(),
            action: FeedbackAction::Reply {
                content: "Concurrent reply B".into(),
            },
        };
        let (a, b) = tokio::join!(
            repo.command(&id, &admin, "service_admin", false, &first),
            repo.command(&id, &admin, "service_admin", false, &second)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(matches!(
            if a.is_err() { &a } else { &b },
            Err(FeedbackError::Conflict)
        ));
        let winner = if a.is_ok() { &first } else { &second };
        assert!(
            repo.command(&id, &admin, "service_admin", false, winner)
                .await
                .unwrap()
                .replayed
        );
        let close = FeedbackCommand {
            visibility: private_visibility(),
            expected_version: 2,
            client_request_id: uuid::Uuid::new_v4().to_string(),
            action: FeedbackAction::SetStatus {
                status: "resolved".into(),
                comment: Some("Synthetic verification complete".into()),
            },
        };
        repo.command(&id, &owner, "user", true, &close)
            .await
            .unwrap();
        let persisted = PostgresFeedbackRepository::new(pool.clone());
        let detail = persisted.detail(&id, Some(&owner), 0, 1).await.unwrap();
        assert_eq!(detail.issue.summary.status, "resolved");
        assert!(detail.has_more);
        assert_eq!(
            persisted
                .detail(&id, None, detail.next_event_cursor.unwrap(), 20)
                .await
                .unwrap()
                .events
                .len(),
            2
        );
        let page = persisted
            .list(&FeedbackQuery {
                after: Some(created.issue.summary.activity_seq),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].version, 3);
        let public_input = CreateFeedback {
            visibility: "public".into(),
            client_request_id: uuid::Uuid::new_v4().to_string(),
            ..input.clone()
        };
        let public = repo
            .create(&owner, "user", &public_input)
            .await
            .unwrap()
            .issue;
        let public_id = &public.summary.id;
        let viewer = uuid::Uuid::new_v4().to_string();
        let list = repo
            .list(&FeedbackQuery {
                viewer_id: Some(viewer.clone()),
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].id, *public_id);
        let private_reply: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":1,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"private-participant-canary"})).unwrap();
        repo.command(public_id, &admin, "user", false, &private_reply)
            .await
            .unwrap();
        let hidden = repo
            .detail(public_id, Some(&viewer), public.summary.activity_seq, 1)
            .await
            .unwrap();
        assert!(hidden.events.is_empty());
        assert!(hidden.next_event_cursor.unwrap() > public.summary.activity_seq);
        assert!(repo
            .detail(public_id, Some(&admin), 0, 100)
            .await
            .unwrap()
            .events
            .iter()
            .any(|e| e.content.as_deref() == Some("private-participant-canary")));
        let invalid_resolve: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":2,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"set_status","status":"resolved"})).unwrap();
        assert!(matches!(
            repo.command(public_id, &admin, "user", false, &invalid_resolve)
                .await,
            Err(FeedbackError::Forbidden)
        ));
        assert!(matches!(
            repo.command(public_id, &owner, "user", true, &invalid_resolve)
                .await,
            Err(FeedbackError::Invalid(_))
        ));
        let resolve: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":2,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"set_status","status":"resolved","comment":"private-final-solution"})).unwrap();
        repo.command(public_id, &owner, "user", true, &resolve)
            .await
            .unwrap();
        let projected = repo.detail(public_id, Some(&viewer), 0, 100).await.unwrap();
        assert_eq!(projected.issue.summary.status, "resolved");
        assert!(!serde_json::to_string(&projected)
            .unwrap()
            .contains("private-final-solution"));
        assert!(projected.events.iter().any(|e| e.kind == "status"
            && e.to_status.as_deref() == Some("resolved")
            && e.content.is_none()));
        let close: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":3,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"set_status","status":"closed"})).unwrap();
        assert!(matches!(
            repo.command(public_id, &owner, "user", true, &close).await,
            Err(FeedbackError::Forbidden)
        ));
        repo.command(public_id, &admin, "service_admin", false, &close)
            .await
            .unwrap();
        assert!(matches!(
            repo.command(public_id, &admin, "user", false, &close).await,
            Err(FeedbackError::Forbidden)
        ));
        assert!(
            repo.command(public_id, &admin, "service_admin", false, &close)
                .await
                .unwrap()
                .replayed
        );
        let public_reply: FeedbackCommand = serde_json::from_value(serde_json::json!({"expected_version":4,"client_request_id":uuid::Uuid::new_v4().to_string(),"action":"reply","content":"public-response","visibility":"public"})).unwrap();
        repo.command(public_id, &owner, "user", true, &public_reply)
            .await
            .unwrap();
        assert!(repo
            .detail(public_id, Some(&viewer), 0, 100)
            .await
            .unwrap()
            .events
            .iter()
            .any(|e| e.content.as_deref() == Some("public-response")));
        assert!(
            matches!(
                repo.command(&id, &admin, "user", false, &first).await,
                Err(FeedbackError::NotFound)
            ),
            "authorization must precede old command replay on private issues"
        );
        for role in [
            "user",
            "admin",
            "audit_admin",
            "user_admin",
            "service_admin",
        ] {
            let role_input = CreateFeedback {
                client_request_id: uuid::Uuid::new_v4().to_string(),
                ..input.clone()
            };
            let created = repo.create(&admin, role, &role_input).await.unwrap();
            let replay = repo.create(&admin, "user", &role_input).await.unwrap();
            assert!(replay.replayed);
            assert_eq!(replay.issue, created.issue);
            let detail = repo
                .detail(&created.issue.summary.id, None, 0, 100)
                .await
                .unwrap();
            assert_eq!(detail.events[0].actor_role, role);
            assert_eq!(detail.events[0].actor_id.as_deref(), Some(admin.as_str()));
            assert!(matches!(
                repo.create(&admin, "unknown", &role_input).await,
                Err(FeedbackError::Forbidden)
            ));
        }
        // User deletion anonymizes authors without destroying the support/audit thread.
        sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(&owner)
            .execute(&pool)
            .await
            .unwrap();
        assert!(persisted
            .detail(&id, None, 0, 20)
            .await
            .unwrap()
            .issue
            .summary
            .owner_id
            .is_none());
        pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&setup)
            .await
            .unwrap();
        setup.close().await;
    }
}
