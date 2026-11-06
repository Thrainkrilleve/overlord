use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};

pub type DbPool = Pool<Sqlite>;

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct Infraction {
    pub id: i64,
    pub user_id: i64,
    pub guild_id: i64,
    pub moderator_id: i64,
    pub action: String,
    pub reason: String,
    pub created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct TicketRecord {
    pub id: i64,
    pub ticket_channel_id: i64,
    pub user_id: i64,
    pub guild_id: i64,
    pub category: String,
    pub status: String,
    pub claimed_by: Option<i64>,
    pub created_at: String,
}

pub async fn init_db(database_url: &str) -> anyhow::Result<DbPool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Create tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS infractions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            guild_id INTEGER NOT NULL,
            moderator_id INTEGER NOT NULL,
            action TEXT NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_infractions_user ON infractions(user_id, guild_id);

        CREATE TABLE IF NOT EXISTS tickets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ticket_channel_id INTEGER UNIQUE NOT NULL,
            user_id INTEGER NOT NULL,
            guild_id INTEGER NOT NULL,
            category TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            claimed_by INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            closed_at TEXT
        );

        CREATE TABLE IF NOT EXISTS active_temp_voice (
            channel_id INTEGER PRIMARY KEY,
            owner_id INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

pub async fn record_infraction(
    pool: &DbPool,
    user_id: u64,
    guild_id: u64,
    moderator_id: u64,
    action: &str,
    reason: &str,
) -> anyhow::Result<i64> {
    let result = sqlx::query(
        r#"
        INSERT INTO infractions (user_id, guild_id, moderator_id, action, reason)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .bind(moderator_id as i64)
    .bind(action)
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn get_infractions(
    pool: &DbPool,
    user_id: u64,
    guild_id: u64,
) -> anyhow::Result<Vec<Infraction>> {
    let records = sqlx::query_as::<_, Infraction>(
        r#"
        SELECT id, user_id, guild_id, moderator_id, action, reason, created_at
        FROM infractions
        WHERE user_id = ? AND guild_id = ?
        ORDER BY id DESC
        LIMIT 25
        "#,
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn create_ticket(
    pool: &DbPool,
    ticket_channel_id: u64,
    user_id: u64,
    guild_id: u64,
    category: &str,
) -> anyhow::Result<i64> {
    let result = sqlx::query(
        r#"
        INSERT INTO tickets (ticket_channel_id, user_id, guild_id, category, status)
        VALUES (?, ?, ?, ?, 'open')
        "#,
    )
    .bind(ticket_channel_id as i64)
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .bind(category)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn get_ticket(pool: &DbPool, ticket_channel_id: u64) -> anyhow::Result<Option<TicketRecord>> {
    let record = sqlx::query_as::<_, TicketRecord>(
        r#"
        SELECT id, ticket_channel_id, user_id, guild_id, category, status, claimed_by, created_at
        FROM tickets
        WHERE ticket_channel_id = ?
        "#,
    )
    .bind(ticket_channel_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

/// Channel ID of the user's ticket that is not yet closed, if any.
pub async fn find_open_ticket(pool: &DbPool, user_id: u64, guild_id: u64) -> anyhow::Result<Option<u64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT ticket_channel_id FROM tickets
        WHERE user_id = ? AND guild_id = ? AND status != 'closed'
        LIMIT 1
        "#,
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.0 as u64))
}

pub async fn claim_ticket(
    pool: &DbPool,
    ticket_channel_id: u64,
    moderator_id: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE tickets
        SET claimed_by = ?, status = 'claimed'
        WHERE ticket_channel_id = ?
        "#,
    )
    .bind(moderator_id as i64)
    .bind(ticket_channel_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn close_ticket(pool: &DbPool, ticket_channel_id: u64) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE tickets
        SET status = 'closed', closed_at = datetime('now')
        WHERE ticket_channel_id = ?
        "#,
    )
    .bind(ticket_channel_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn record_temp_voice(pool: &DbPool, channel_id: u64, owner_id: u64) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO active_temp_voice (channel_id, owner_id)
        VALUES (?, ?)
        "#,
    )
    .bind(channel_id as i64)
    .bind(owner_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn is_temp_voice(pool: &DbPool, channel_id: u64) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM active_temp_voice WHERE channel_id = ?
        "#,
    )
    .bind(channel_id as i64)
    .fetch_one(pool)
    .await?;

    Ok(count.0 > 0)
}

pub async fn remove_temp_voice(pool: &DbPool, channel_id: u64) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        DELETE FROM active_temp_voice WHERE channel_id = ?
        "#,
    )
    .bind(channel_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh on-disk database per test (in-memory SQLite would give each pooled connection its own DB)
    async fn test_db(name: &str) -> DbPool {
        let path = std::env::temp_dir().join(format!("overlord-test-{}-{}.db", name, std::process::id()));
        let _ = std::fs::remove_file(&path);
        init_db(&format!("sqlite://{}?mode=rwc", path.display())).await.unwrap()
    }

    #[tokio::test]
    async fn ticket_lifecycle_round_trips() {
        let db = test_db("tickets").await;
        let id = create_ticket(&db, 111, 222, 333, "Report").await.unwrap();

        assert_eq!(find_open_ticket(&db, 222, 333).await.unwrap(), Some(111));

        claim_ticket(&db, 111, 444).await.unwrap();
        let ticket = get_ticket(&db, 111).await.unwrap().unwrap();
        assert_eq!(ticket.id, id);
        assert_eq!(ticket.user_id, 222);
        assert_eq!(ticket.category, "Report");
        assert_eq!(ticket.status, "claimed");
        assert_eq!(ticket.claimed_by, Some(444));
        // Claimed tickets still count as open
        assert_eq!(find_open_ticket(&db, 222, 333).await.unwrap(), Some(111));

        close_ticket(&db, 111).await.unwrap();
        assert_eq!(find_open_ticket(&db, 222, 333).await.unwrap(), None);
        assert!(get_ticket(&db, 999).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn infractions_are_per_guild_newest_first() {
        let db = test_db("infractions").await;
        record_infraction(&db, 1, 10, 99, "warn", "first").await.unwrap();
        record_infraction(&db, 1, 10, 99, "timeout", "second").await.unwrap();
        record_infraction(&db, 1, 20, 99, "warn", "other guild").await.unwrap();

        let history = get_infractions(&db, 1, 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].reason, "second");
        assert_eq!(history[1].action, "warn");
    }

    #[tokio::test]
    async fn temp_voice_tracking() {
        let db = test_db("voice").await;
        assert!(!is_temp_voice(&db, 5).await.unwrap());
        record_temp_voice(&db, 5, 7).await.unwrap();
        assert!(is_temp_voice(&db, 5).await.unwrap());
        remove_temp_voice(&db, 5).await.unwrap();
        assert!(!is_temp_voice(&db, 5).await.unwrap());
    }
}
