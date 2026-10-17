use super::*;
use aether_data_contracts::repository::usage::{UsageActivityQuery, UsageActivityRow};

impl SqlxUsageReadRepository {
    pub async fn summarize_usage_activity(
        &self,
        q: &UsageActivityQuery,
    ) -> Result<Vec<UsageActivityRow>, DataLayerError> {
        q.validate()?;
        let mut tx = self.pool.begin().await.map_postgres_err()?;
        sqlx::query("SET LOCAL statement_timeout = '8s'")
            .execute(&mut *tx)
            .await
            .map_postgres_err()?;
        let rows = sqlx::query(
            r#"
SELECT user_id,
 FLOOR((EXTRACT(EPOCH FROM created_at) + $3::bigint * 60) / 86400)::bigint AS day,
 MOD(FLOOR((EXTRACT(EPOCH FROM created_at) + $3::bigint * 60) / 3600)::bigint, 24) AS hour,
 COUNT(*)::bigint AS requests,
 (COUNT(DISTINCT FLOOR(EXTRACT(EPOCH FROM created_at) / 300)) * 5)::bigint AS active_minutes,
 FLOOR(EXTRACT(EPOCH FROM MIN(created_at)))::bigint AS first_at
FROM usage_billing_facts
WHERE created_at >= TO_TIMESTAMP($1::double precision)
 AND created_at < TO_TIMESTAMP($2::double precision)
 AND status = 'completed' AND (status_code IS NULL OR status_code BETWEEN 200 AND 299)
 AND user_id IS NOT NULL AND provider_name NOT IN ('unknown', 'pending')
GROUP BY user_id, day, hour
ORDER BY user_id, day, hour
LIMIT 100001
"#,
        )
        .bind(q.from as f64)
        .bind(q.until as f64)
        .bind(i64::from(q.tz_offset_minutes))
        .fetch_all(&mut *tx)
        .await
        .map_postgres_err()?;
        if rows.len() > 100000 {
            return Err(DataLayerError::InvalidInput(
                "Activity result too large; select a shorter range".into(),
            ));
        }
        let result = rows
            .into_iter()
            .map(|r| {
                Ok(UsageActivityRow {
                    user_id: r.try_get("user_id").map_postgres_err()?,
                    day: r.try_get("day").map_postgres_err()?,
                    hour: r.try_get("hour").map_postgres_err()?,
                    requests: r.try_get::<i64, _>("requests").map_postgres_err()? as u64,
                    active_minutes: r.try_get::<i64, _>("active_minutes").map_postgres_err()?
                        as u64,
                    first_at: r.try_get::<i64, _>("first_at").map_postgres_err()? as u64,
                })
            })
            .collect::<Result<Vec<_>, DataLayerError>>()?;
        tx.commit().await.map_postgres_err()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires a disposable loopback afssr_feedback_test database"]
    async fn leaderboard_activity_postgres_buckets_timezone_and_boundaries() {
        use std::str::FromStr;
        let options = sqlx::postgres::PgConnectOptions::from_str(
            &std::env::var("AFSSR_ACTIVITY_TEST_DATABASE_URL").expect("explicit test DB required"),
        )
        .unwrap();
        assert!(matches!(options.get_host(), "localhost" | "127.0.0.1"));
        assert_eq!(options.get_database(), Some("afssr_feedback_test"));
        let setup = PgPool::connect_with(options.clone()).await.unwrap();
        let schema = format!("activity_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&setup)
            .await
            .unwrap();
        let search = schema.clone();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .after_connect(move |connection, _| {
                let search = search.clone();
                Box::pin(async move {
                    sqlx::query(&format!("SET search_path TO {search}"))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE usage_billing_facts(user_id varchar(36),created_at timestamptz,status text,status_code integer,provider_name text); INSERT INTO usage_billing_facts SELECT 'user-a',to_timestamp(t),status,200,'Fixture' FROM (VALUES (172801,'completed'),(172802,'completed'),(173100,'completed'),(176399,'completed'),(176400,'completed'),(259200,'completed'),(173600,'failed')) AS x(t,status);").execute(&pool).await.unwrap();
        let repo = SqlxUsageReadRepository::new(pool.clone());
        let q = UsageActivityQuery {
            from: 172800,
            until: 259200,
            tz_offset_minutes: 480,
        };
        let rows = repo.summarize_usage_activity(&q).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].hour, 8);
        assert_eq!(rows[0].day, 2);
        assert_eq!(rows[0].requests, 4);
        assert_eq!(rows[0].active_minutes, 15);
        assert_eq!(rows[0].first_at, 172801);
        assert_eq!(rows[1].active_minutes, 5);
        let west = repo
            .summarize_usage_activity(&UsageActivityQuery {
                tz_offset_minutes: -480,
                ..q
            })
            .await
            .unwrap();
        assert_eq!(west[0].day, 1);
        assert_eq!(west[0].hour, 16);
        pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&setup)
            .await
            .unwrap();
        setup.close().await;
    }
}
