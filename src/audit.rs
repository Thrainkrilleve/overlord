use crate::database::DbPool;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AuditRow {
    pub user_id: i64,
    pub username: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub created_at: String,
}

fn clip(value: &str) -> String {
    value.chars().take(300).collect()
}

pub async fn record(
    pool: &DbPool,
    guild_id: u64,
    user_id: u64,
    username: &str,
    field: &str,
    old_value: &str,
    new_value: &str,
) -> anyhow::Result<()> {
    if old_value == new_value {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO settings_audit (guild_id, user_id, username, field, old_value, new_value)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(guild_id as i64)
    .bind(user_id as i64)
    .bind(clip(username))
    .bind(clip(field))
    .bind(clip(old_value))
    .bind(clip(new_value))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(
    pool: &DbPool,
    guild_id: u64,
    days: i64,
    limit: i64,
) -> anyhow::Result<Vec<AuditRow>> {
    let days = days.clamp(1, 90);
    let limit = limit.clamp(1, 100);
    let since = format!("-{days} days");
    let rows = sqlx::query_as::<_, AuditRow>(
        r#"
        SELECT user_id, username, field, old_value, new_value, created_at
        FROM settings_audit
        WHERE guild_id = ? AND created_at >= datetime('now', ?)
        ORDER BY id DESC
        LIMIT ?
        "#,
    )
    .bind(guild_id as i64)
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn audit_rows_stay_inside_their_guild() {
        let db = crate::database::test_pool("audit-iso").await;
        record(
            &db,
            1,
            9,
            "maple_admin",
            "Min account age",
            "1 day",
            "3 days",
        )
        .await
        .unwrap();
        record(&db, 1, 9, "maple_admin", "Honeypot trap", "Off", "On")
            .await
            .unwrap();
        record(&db, 2, 8, "other", "Min account age", "3 days", "9 days")
            .await
            .unwrap();
        record(
            &db,
            1,
            9,
            "maple_admin",
            "Min account age",
            "3 days",
            "3 days",
        )
        .await
        .unwrap();

        let rows = list(&db, 1, 14, 20).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].field, "Honeypot trap");
        assert_eq!(rows[0].old_value, "Off");
        assert_eq!(rows[0].new_value, "On");
        assert_eq!(rows[1].old_value, "1 day");
        assert!(list(&db, 2, 14, 20)
            .await
            .unwrap()
            .iter()
            .all(|row| row.username == "other"));
        assert!(list(&db, 3, 14, 20).await.unwrap().is_empty());
    }
}
