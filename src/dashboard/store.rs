use sqlx::FromRow;

use crate::database::DbPool;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user_id: u64,
    pub username: String,
    pub avatar: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_expires_at: i64,
    pub csrf_token: String,
    pub expires_at: i64,
}

#[derive(Debug, FromRow)]
struct SessionRow {
    id: String,
    user_id: i64,
    username: String,
    avatar: Option<String>,
    access_token: String,
    refresh_token: Option<String>,
    token_expires_at: i64,
    csrf_token: String,
    expires_at: i64,
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id as u64,
            username: row.username,
            avatar: row.avatar,
            access_token: row.access_token,
            refresh_token: row.refresh_token,
            token_expires_at: row.token_expires_at,
            csrf_token: row.csrf_token,
            expires_at: row.expires_at,
        }
    }
}

pub async fn create_session(pool: &DbPool, session: &Session) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO dashboard_sessions (
            id, user_id, username, avatar, access_token, refresh_token,
            token_expires_at, csrf_token, expires_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&session.id)
    .bind(session.user_id as i64)
    .bind(&session.username)
    .bind(&session.avatar)
    .bind(&session.access_token)
    .bind(&session.refresh_token)
    .bind(session.token_expires_at)
    .bind(&session.csrf_token)
    .bind(session.expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_session(pool: &DbPool, id: &str) -> anyhow::Result<Option<Session>> {
    let row = sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT id, user_id, username, avatar, access_token, refresh_token,
               token_expires_at, csrf_token, expires_at
        FROM dashboard_sessions WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Session::from))
}

pub async fn delete_session(pool: &DbPool, id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM dashboard_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct InfractionRow {
    pub case_number: i64,
    pub user_id: i64,
    pub moderator_id: i64,
    pub action: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
struct InfractionSql {
    case_number: i64,
    user_id: i64,
    moderator_id: i64,
    action: String,
    reason: String,
    created_at: String,
}

pub async fn recent_infractions(
    pool: &DbPool,
    guild_id: u64,
    limit: i64,
) -> anyhow::Result<Vec<InfractionRow>> {
    let rows = sqlx::query_as::<_, InfractionSql>(
        r#"
        SELECT COALESCE(case_number, id) AS case_number, user_id, moderator_id, action, reason, created_at
        FROM infractions
        WHERE guild_id = ?
        ORDER BY id DESC
        LIMIT ?
        "#,
    )
    .bind(guild_id as i64)
    .bind(limit.clamp(1, 50))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| InfractionRow {
            case_number: row.case_number,
            user_id: row.user_id,
            moderator_id: row.moderator_id,
            action: row.action,
            reason: row.reason,
            created_at: row.created_at,
        })
        .collect())
}

pub async fn count_since(
    pool: &DbPool,
    sql: &str,
    guild_id: u64,
    since: &str,
) -> anyhow::Result<i64> {
    let count: (i64,) = sqlx::query_as(sql)
        .bind(guild_id as i64)
        .bind(since)
        .fetch_one(pool)
        .await?;
    Ok(count.0)
}

pub async fn count_simple(pool: &DbPool, sql: &str, guild_id: u64) -> anyhow::Result<i64> {
    let count: (i64,) = sqlx::query_as(sql)
        .bind(guild_id as i64)
        .fetch_one(pool)
        .await?;
    Ok(count.0)
}

#[derive(Debug, Clone)]
pub struct WeekStats {
    pub infractions_this_week: i64,
    pub infractions_last_week: i64,
    pub open_tickets: i64,
    pub claimed_tickets: i64,
    pub closed_this_week: i64,
    pub raid_alerts_this_week: i64,
    pub last_raid_at: Option<String>,
    pub temp_rooms: i64,
}

pub async fn week_stats(pool: &DbPool, guild_id: u64) -> anyhow::Result<WeekStats> {
    let this_week = "-7 days";
    let infractions_this_week = count_since(
        pool,
        "SELECT COUNT(*) FROM infractions WHERE guild_id = ? AND created_at >= datetime('now', ?)",
        guild_id,
        this_week,
    )
    .await?;
    let infractions_prev: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM infractions WHERE guild_id = ? AND created_at >= datetime('now', '-14 days') AND created_at < datetime('now', '-7 days')",
    )
    .bind(guild_id as i64)
    .fetch_one(pool)
    .await?;
    let open_tickets = count_simple(
        pool,
        "SELECT COUNT(*) FROM tickets WHERE guild_id = ? AND status != 'closed'",
        guild_id,
    )
    .await?;
    let claimed_tickets = count_simple(
        pool,
        "SELECT COUNT(*) FROM tickets WHERE guild_id = ? AND status = 'claimed'",
        guild_id,
    )
    .await?;
    let closed_this_week = count_since(
        pool,
        "SELECT COUNT(*) FROM tickets WHERE guild_id = ? AND status = 'closed' AND closed_at >= datetime('now', ?)",
        guild_id,
        this_week,
    )
    .await?;
    let raid_alerts_this_week = count_since(
        pool,
        "SELECT COUNT(*) FROM security_events WHERE guild_id = ? AND kind = 'raid' AND created_at >= datetime('now', ?)",
        guild_id,
        this_week,
    )
    .await?;
    let last_raid: Option<(String,)> = sqlx::query_as(
        "SELECT created_at FROM security_events WHERE guild_id = ? AND kind = 'raid' ORDER BY id DESC LIMIT 1",
    )
    .bind(guild_id as i64)
    .fetch_optional(pool)
    .await?;
    let temp_rooms = count_simple(
        pool,
        "SELECT COUNT(*) FROM active_temp_voice WHERE guild_id = ?",
        guild_id,
    )
    .await?;
    Ok(WeekStats {
        infractions_this_week,
        infractions_last_week: infractions_prev.0,
        open_tickets,
        claimed_tickets,
        closed_this_week,
        raid_alerts_this_week,
        last_raid_at: last_raid.map(|row| row.0),
        temp_rooms,
    })
}
