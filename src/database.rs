use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};

pub type DbPool = Pool<Sqlite>;

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct Infraction {
    pub id: i64,
    pub case_number: i64,
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

    migrate(&pool).await?;
    Ok(pool)
}

/// Create the schema and upgrade databases written by older versions.
/// Safe to run on every start, including the Docker container's existing volume.
async fn migrate(pool: &DbPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS infractions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            guild_id INTEGER NOT NULL,
            moderator_id INTEGER NOT NULL,
            action TEXT NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            case_number INTEGER
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
            guild_id INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS guild_settings (
            guild_id INTEGER PRIMARY KEY,
            honeypot_channel_id INTEGER,
            verify_channel_id INTEGER,
            mod_log_channel_id INTEGER,
            transcript_channel_id INTEGER,
            join_to_create_channel_id INTEGER,
            voice_category_id INTEGER,
            ticket_category_id INTEGER,
            unverified_role_id INTEGER,
            member_role_id INTEGER,
            mod_role_id INTEGER,
            min_account_age_days INTEGER NOT NULL DEFAULT 3,
            honeypot_enabled INTEGER NOT NULL DEFAULT 1,
            raid_join_threshold INTEGER NOT NULL DEFAULT 5,
            raid_window_seconds INTEGER NOT NULL DEFAULT 10,
            raid_action TEXT NOT NULL DEFAULT 'alert',
            staff_role_ids TEXT NOT NULL DEFAULT '',
            max_open_tickets INTEGER NOT NULL DEFAULT 1,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS settings_audit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            username TEXT NOT NULL,
            field TEXT NOT NULL,
            old_value TEXT NOT NULL,
            new_value TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_settings_audit_guild ON settings_audit(guild_id, id);

        CREATE TABLE IF NOT EXISTS dashboard_sessions (
            id TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            username TEXT NOT NULL,
            avatar TEXT,
            access_token TEXT NOT NULL,
            refresh_token TEXT,
            token_expires_at INTEGER NOT NULL,
            csrf_token TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS security_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            detail TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_security_events_guild ON security_events(guild_id, id);
        "#,
    )
    .execute(pool)
    .await?;

    // Tables created before these columns existed are left as-is by CREATE IF NOT EXISTS.
    ensure_column(pool, "infractions", "case_number", "INTEGER").await?;
    ensure_column(pool, "active_temp_voice", "guild_id", "INTEGER").await?;
    ensure_column(pool, "guild_settings", "honeypot_enabled", "INTEGER").await?;
    ensure_column(pool, "guild_settings", "raid_join_threshold", "INTEGER").await?;
    ensure_column(pool, "guild_settings", "raid_window_seconds", "INTEGER").await?;
    ensure_column(pool, "guild_settings", "raid_action", "TEXT").await?;
    ensure_column(pool, "guild_settings", "staff_role_ids", "TEXT").await?;
    ensure_column(pool, "guild_settings", "max_open_tickets", "INTEGER").await?;
    backfill_guild_setting_defaults(pool).await?;
    backfill_case_numbers(pool).await?;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_infractions_guild_case ON infractions(guild_id, case_number)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

fn validate_sql_ident(name: &str) -> anyhow::Result<()> {
    let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        anyhow::bail!("invalid SQL identifier");
    }
}

async fn column_exists(pool: &DbPool, table: &str, column: &str) -> anyhow::Result<bool> {
    validate_sql_ident(table)?;
    validate_sql_ident(column)?;
    let sql = format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = '{column}'");
    let found: Option<i64> = sqlx::query_scalar(&sql).fetch_optional(pool).await?;
    Ok(found.is_some())
}

async fn ensure_column(pool: &DbPool, table: &str, column: &str, decl: &str) -> anyhow::Result<()> {
    validate_sql_ident(decl)?;
    if column_exists(pool, table, column).await? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {decl}");
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

async fn backfill_guild_setting_defaults(pool: &DbPool) -> anyhow::Result<()> {
    // Columns added to an existing database are nullable. Fill them so every server keeps
    // the previous behavior: honeypot follows its channel, raids alert at 5 joins / 10 seconds,
    // and each person can have one open ticket.
    sqlx::query(
        r#"
        UPDATE guild_settings SET
            honeypot_enabled = COALESCE(honeypot_enabled, 1),
            raid_join_threshold = COALESCE(raid_join_threshold, 5),
            raid_window_seconds = COALESCE(raid_window_seconds, 10),
            raid_action = COALESCE(raid_action, 'alert'),
            staff_role_ids = COALESCE(staff_role_ids, ''),
            max_open_tickets = COALESCE(max_open_tickets, 1)
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn backfill_case_numbers(pool: &DbPool) -> anyhow::Result<()> {
    // One number sequence per guild, in the order cases were recorded. Rows that already have a
    // number are left alone so a later start cannot renumber history.
    sqlx::query(
        r#"
        UPDATE infractions
        SET case_number = (
            SELECT COUNT(*)
            FROM infractions AS older
            WHERE older.guild_id = infractions.guild_id
              AND older.id <= infractions.id
        )
        WHERE case_number IS NULL
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_infraction(
    pool: &DbPool,
    user_id: u64,
    guild_id: u64,
    moderator_id: u64,
    action: &str,
    reason: &str,
) -> anyhow::Result<i64> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let next: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(case_number), 0) + 1 FROM infractions WHERE guild_id = ?",
    )
    .bind(guild_id as i64)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO infractions (user_id, guild_id, moderator_id, action, reason, case_number)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .bind(moderator_id as i64)
    .bind(action)
    .bind(reason)
    .bind(next)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(next)
}

pub async fn get_infractions(
    pool: &DbPool,
    user_id: u64,
    guild_id: u64,
) -> anyhow::Result<Vec<Infraction>> {
    let records = sqlx::query_as::<_, Infraction>(
        r#"
        SELECT id, COALESCE(case_number, id) AS case_number, user_id, guild_id, moderator_id,
               action, reason, created_at
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

pub async fn get_ticket(
    pool: &DbPool,
    ticket_channel_id: u64,
) -> anyhow::Result<Option<TicketRecord>> {
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
#[allow(dead_code)]
pub async fn find_open_ticket(
    pool: &DbPool,
    user_id: u64,
    guild_id: u64,
) -> anyhow::Result<Option<u64>> {
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

pub async fn record_temp_voice(
    pool: &DbPool,
    channel_id: u64,
    owner_id: u64,
    guild_id: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO active_temp_voice (channel_id, owner_id, guild_id)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(channel_id as i64)
    .bind(owner_id as i64)
    .bind(guild_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

/// True when this channel is a temp room tracked for `guild_id`.
/// Rows written before `guild_id` existed have a null guild and still match, because a Discord
/// channel id belongs to only one server.
pub async fn is_temp_voice(pool: &DbPool, channel_id: u64, guild_id: u64) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM active_temp_voice
        WHERE channel_id = ? AND (guild_id = ? OR guild_id IS NULL)
        "#,
    )
    .bind(channel_id as i64)
    .bind(guild_id as i64)
    .fetch_one(pool)
    .await?;

    Ok(count.0 > 0)
}

pub async fn list_open_tickets(
    pool: &DbPool,
    user_id: u64,
    guild_id: u64,
) -> anyhow::Result<Vec<u64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        r#"
        SELECT ticket_channel_id FROM tickets
        WHERE user_id = ? AND guild_id = ? AND status != 'closed'
        ORDER BY id ASC
        "#,
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| row.0 as u64).collect())
}

pub async fn record_security_event(
    pool: &DbPool,
    guild_id: u64,
    kind: &str,
    detail: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO security_events (guild_id, kind, detail) VALUES (?, ?, ?)
        "#,
    )
    .bind(guild_id as i64)
    .bind(kind)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
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
pub async fn test_pool(name: &str) -> DbPool {
    let path = std::env::temp_dir().join(format!(
        "overlord-test-{}-{}-{}.db",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    init_db(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ticket_lifecycle_round_trips() {
        let db = test_pool("tickets").await;
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
        let db = test_pool("infractions").await;
        record_infraction(&db, 1, 10, 99, "warn", "first")
            .await
            .unwrap();
        record_infraction(&db, 1, 10, 99, "timeout", "second")
            .await
            .unwrap();
        record_infraction(&db, 1, 20, 99, "warn", "other guild")
            .await
            .unwrap();

        let history = get_infractions(&db, 1, 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].reason, "second");
        assert_eq!(history[1].action, "warn");
        assert!(get_infractions(&db, 1, 20)
            .await
            .unwrap()
            .iter()
            .all(|row| row.guild_id == 20));
    }

    #[tokio::test]
    async fn case_numbers_restart_in_each_guild() {
        let db = test_pool("cases").await;
        let first_a = record_infraction(&db, 1, 10, 9, "warn", "a").await.unwrap();
        let first_b = record_infraction(&db, 1, 20, 9, "warn", "b").await.unwrap();
        let second_a = record_infraction(&db, 2, 10, 9, "kick", "c").await.unwrap();

        assert_eq!((first_a, second_a, first_b), (1, 2, 1));

        let history = get_infractions(&db, 1, 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].case_number, 1);
        let other = get_infractions(&db, 1, 20).await.unwrap();
        assert_eq!(other[0].case_number, 1);
    }

    #[tokio::test]
    async fn temp_voice_tracking() {
        let db = test_pool("voice").await;
        assert!(!is_temp_voice(&db, 5, 100).await.unwrap());
        record_temp_voice(&db, 5, 7, 100).await.unwrap();
        assert!(is_temp_voice(&db, 5, 100).await.unwrap());
        remove_temp_voice(&db, 5).await.unwrap();
        assert!(!is_temp_voice(&db, 5, 100).await.unwrap());
    }

    #[tokio::test]
    async fn temp_voice_does_not_match_another_guild() {
        let db = test_pool("voice-guild").await;
        record_temp_voice(&db, 5, 7, 100).await.unwrap();
        assert!(is_temp_voice(&db, 5, 100).await.unwrap());
        assert!(!is_temp_voice(&db, 5, 200).await.unwrap());
    }

    #[tokio::test]
    async fn legacy_database_gains_guild_columns_without_losing_rows() {
        let path = std::env::temp_dir().join(format!(
            "overlord-test-legacy-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&path);
        let url = format!("sqlite://{}?mode=rwc", path.display());

        {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .unwrap();
            // Schema from before per-guild settings: no case_number, no guild on temp voice.
            sqlx::query(
                r#"
                CREATE TABLE infractions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    user_id INTEGER NOT NULL,
                    guild_id INTEGER NOT NULL,
                    moderator_id INTEGER NOT NULL,
                    action TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE TABLE tickets (
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
                CREATE TABLE active_temp_voice (
                    channel_id INTEGER PRIMARY KEY,
                    owner_id INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                "#,
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO infractions (user_id, guild_id, moderator_id, action, reason) VALUES (1, 10, 9, 'warn', 'old-a')")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO infractions (user_id, guild_id, moderator_id, action, reason) VALUES (1, 10, 9, 'kick', 'old-b')")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO infractions (user_id, guild_id, moderator_id, action, reason) VALUES (1, 20, 9, 'warn', 'old-c')")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO active_temp_voice (channel_id, owner_id) VALUES (8, 9)")
                .execute(&pool)
                .await
                .unwrap();
        }

        init_db(&url).await.unwrap();
        // Running the migration again must be a no-op for existing case numbers.
        let db = init_db(&url).await.unwrap();

        let guild_10 = get_infractions(&db, 1, 10).await.unwrap();
        assert_eq!(guild_10.len(), 2);
        assert_eq!(guild_10[0].reason, "old-b");
        assert_eq!(guild_10[0].case_number, 2);
        assert_eq!(guild_10[1].case_number, 1);

        let guild_20 = get_infractions(&db, 1, 20).await.unwrap();
        assert_eq!(guild_20.len(), 1);
        assert_eq!(guild_20[0].case_number, 1);

        let next = record_infraction(&db, 1, 10, 9, "ban", "new")
            .await
            .unwrap();
        assert_eq!(next, 3);
        assert_eq!(
            record_infraction(&db, 1, 20, 9, "ban", "new")
                .await
                .unwrap(),
            2
        );

        // Pre-upgrade rooms have a null guild and still belong to the channel's server.
        assert!(is_temp_voice(&db, 8, 10).await.unwrap());
        assert!(column_exists(&db, "guild_settings", "guild_id")
            .await
            .unwrap());
        assert!(column_exists(&db, "active_temp_voice", "guild_id")
            .await
            .unwrap());
    }
}
