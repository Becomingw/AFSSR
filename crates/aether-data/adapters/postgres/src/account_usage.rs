use crate::error::SqlxResultExt;
use aether_data_contracts::{repository::account_usage::*, DataLayerError};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[cfg(test)]
#[path = "account_usage_tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub struct PostgresAccountUsageRepository {
    pool: PgPool,
}
impl PostgresAccountUsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl AccountUsageRepository for PostgresAccountUsageRepository {
    async fn read_windows(
        &self,
        user: &str,
        now: u64,
        windows: &[AccountUsageWindow],
    ) -> Result<Vec<AccountUsageAmounts>, DataLayerError> {
        validate_windows(user, now, windows)?;
        let payload=serde_json::Value::Array(windows.iter().enumerate().map(|(i,w)|serde_json::json!({"idx":i,"model":w.model,"models":w.models,"starts":w.from,"ends":w.until})).collect());
        let mut tx = self.pool.begin().await.map_postgres_err()?;
        sqlx::query("SET LOCAL statement_timeout = '8s'")
            .execute(&mut *tx)
            .await
            .map_postgres_err()?;
        // Same ledger and expiry rules as cost admission; a single snapshot covers every pool.
        let rows=sqlx::query(r#"
WITH windows AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS w(idx integer,model text,models text[],starts bigint,ends bigint))
SELECT w.idx,
 COALESCE(SUM(r.actual_cost_units) FILTER (WHERE r.state='finalized'),0)::bigint AS finalized,
 COALESCE(SUM(r.reserved_cost_units) FILTER (WHERE r.state='reserved' AND r.reservation_expires_at>TO_TIMESTAMP($3::double precision)),0)::bigint AS reserved
FROM windows w LEFT JOIN usage_cost_reservations r ON r.subject_id=$2
 AND r.admitted_at>=TO_TIMESTAMP(w.starts::double precision)
 AND r.admitted_at<TO_TIMESTAMP(w.ends::double precision)
 AND (w.model IS NULL OR r.model=w.model)
 AND (w.models IS NULL OR r.model=ANY(w.models))
GROUP BY w.idx ORDER BY w.idx
"#).bind(payload).bind(user).bind(now as f64).fetch_all(&mut *tx).await.map_postgres_err()?;
        tx.commit().await.map_postgres_err()?;
        rows.into_iter()
            .map(|r| {
                Ok(AccountUsageAmounts {
                    finalized_units: r
                        .try_get::<i64, _>("finalized")
                        .map_postgres_err()?
                        .try_into()
                        .map_err(|_| {
                            DataLayerError::UnexpectedValue("Negative quota total".into())
                        })?,
                    reserved_units: r
                        .try_get::<i64, _>("reserved")
                        .map_postgres_err()?
                        .try_into()
                        .map_err(|_| {
                            DataLayerError::UnexpectedValue("Negative quota reservation".into())
                        })?,
                })
            })
            .collect()
    }
}
