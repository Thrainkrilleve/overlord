use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::async_trait;
use axum::extract::{ConnectInfo, FromRequest, Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::audit;
use crate::config::DashboardConfig;
use crate::database::DbPool;
use crate::guild_settings;

use super::api::{ApiError, DiscordApi, LiveDiscord, UserGuild};
use super::create;
use super::guard::{self, client_ip, cookie_value, set_cookie, RateLimiter};
use super::store::{self, Session};
use super::validate::{self, Write};
use super::view::{self, Nav, Shell};

#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub cfg: DashboardConfig,
    pub discord: Arc<dyn DiscordApi>,
    pub limiter: Arc<RateLimiter>,
}

pub async fn spawn(cfg: DashboardConfig, db: DbPool, bot_token: String) -> anyhow::Result<()> {
    let discord = Arc::new(LiveDiscord::new(
        cfg.client_id.clone(),
        cfg.client_secret.clone(),
        cfg.redirect_url.clone(),
        bot_token,
    ));
    let state = AppState {
        db,
        cfg,
        discord,
        limiter: Arc::new(RateLimiter::default()),
    };
    let listener = tokio::net::TcpListener::bind(state.cfg.bind).await?;
    tracing::info!(
        "Dashboard listening on {} (public {})",
        state.cfg.bind,
        state.cfg.public_url
    );
    let app = router(state);
    tokio::spawn(async move {
        if let Err(err) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
            tracing::error!("dashboard server stopped: {}", err);
        }
    });
    Ok(())
}

pub fn router(state: AppState) -> Router {
    let secure = state.clone();
    let limited = state.clone();
    Router::new()
        .route("/", get(home))
        .route("/install", get(install))
        .route("/auth/discord", get(start_oauth))
        .route("/auth/callback", get(oauth_callback))
        .route("/logout", post(logout))
        .route("/servers", get(servers))
        .route("/servers/:guild_id", get(overview))
        .route("/servers/:guild_id/resources", post(create_resource))
        .route("/servers/:guild_id/layout", get(layout_page).post(apply_layout))
        .route("/servers/:guild_id/moderation", get(moderation))
        .route(
            "/servers/:guild_id/security",
            get(security).post(save_security),
        )
        .route(
            "/servers/:guild_id/verification",
            get(verification).post(save_verification),
        )
        .route(
            "/servers/:guild_id/tickets",
            get(tickets).post(save_tickets),
        )
        .route("/servers/:guild_id/voice", get(voice).post(save_voice))
        .route("/servers/:guild_id/logs", get(logs))
        .route("/servers/:guild_id/audit", get(audit_page))
        .route("/assets/app.css", get(css))
        .route("/assets/dashboard.css", get(css))
        .route("/assets/fonts/Geist-Variable.ttf", get(font_geist))
        .route("/assets/fonts/Geist-Italic.ttf", get(font_geist_italic))
        .route("/assets/fonts/GeistMono-Variable.woff2", get(font_mono))
        .route("/assets/dashboard.js", get(js))
        .route("/favicon.ico", get(favicon_ico))
        .route("/favicon.svg", get(favicon_svg))
        .layer(from_fn_with_state(limited, limit_requests))
        .layer(from_fn_with_state(secure, secure_headers))
        .with_state(state)
}

async fn secure_headers(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    insert(headers, "x-content-type-options", "nosniff");
    insert(headers, "x-frame-options", "DENY");
    insert(headers, "referrer-policy", "no-referrer");
    insert(
        headers,
        "permissions-policy",
        "camera=(), microphone=(), geolocation=()",
    );
    insert(
        headers,
        "content-security-policy",
        guard::CONTENT_SECURITY_POLICY,
    );
    if state.cfg.cookie_secure {
        insert(
            headers,
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        );
    }
    response
}

async fn limit_requests(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let ip = client_ip(
        peer,
        request
            .headers()
            .get("cf-connecting-ip")
            .and_then(|value| value.to_str().ok()),
        state.cfg.trust_cloudflare,
    );
    let post = request.method() == axum::http::Method::POST;
    let path = request.uri().path();
    let auth = path.starts_with("/auth/");
    let creating = post && (path.ends_with("/resources") || path.ends_with("/layout"));
    let (limit, key) = if auth {
        (20, format!("auth:{ip}"))
    } else if creating {
        (8, format!("create:{ip}"))
    } else if post {
        (30, format!("post:{ip}"))
    } else {
        (180, format!("get:{ip}"))
    };
    if !state.limiter.allow(&key, limit, Duration::from_secs(60)) {
        return html(
            StatusCode::TOO_MANY_REQUESTS,
            view::message_page(
                "Slow down",
                "Too many requests. Wait a minute and try again.",
            ),
        );
    }
    next.run(request).await
}

fn insert(headers: &mut HeaderMap, name: &'static str, value: &'static str) {
    let name = header::HeaderName::from_static(name);
    let value = header::HeaderValue::from_static(value);
    headers.insert(name, value);
}

fn html(status: StatusCode, body: String) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "private, no-store"),
            (
                header::HeaderName::from_static("cdn-cache-control"),
                "no-store",
            ),
        ],
        body,
    )
        .into_response()
}

fn redirect(location: &str, cookies: &[String]) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, location)
        .body(axum::body::Body::empty())
        .unwrap();
    for cookie in cookies {
        if let Ok(value) = header::HeaderValue::from_str(cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    response
}

fn notice(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("saved") => Some("Changes saved. The bot is using them now."),
        Some("cancelled") => Some("Sign-in was cancelled."),
        Some("state") => Some("Sign-in could not be verified. Start again."),
        Some("discord") => Some("Discord could not be reached. Try again in a moment."),
        Some("permission") => Some("You need Manage Server in that community."),
        Some("invalid") => Some("A channel or role in that form is not part of this server."),
        Some("csrf") => Some("That form expired. Reload the page and try again."),
        Some("created") => Some("Created and assigned. The bot is using it now."),
        _ => None,
    }
}

#[derive(Deserialize, Default)]
struct NoticeQuery {
    notice: Option<String>,
    error: Option<String>,
    days: Option<i64>,
}

async fn home(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NoticeQuery>,
) -> Response {
    if session_from(&state, &headers).await.is_some() {
        return redirect("/servers", &[]);
    }
    let code =
        query
            .notice
            .as_deref()
            .or(query.error.as_deref())
            .or(match query.error.as_deref() {
                Some("access_denied") => Some("cancelled"),
                _ => None,
            });
    let flash = if query.error.as_deref() == Some("access_denied") {
        notice(Some("cancelled"))
    } else {
        notice(code)
    };
    html(StatusCode::OK, view::login_page(flash))
}

#[derive(Deserialize, Default)]
struct InstallQuery {
    guild_id: Option<String>,
}

/// Public install link. Discord's custom install URL points here, and the
/// "Add Overlord" card passes `guild_id` through when a server is already chosen.
async fn install(State(state): State<AppState>, Query(query): Query<InstallQuery>) -> Response {
    let guild_id = match query.guild_id.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(value) => match value.parse::<u64>() {
            Ok(id) if id > 0 => Some(id),
            _ => {
                return html(
                    StatusCode::BAD_REQUEST,
                    view::message_page("Bad request", "guild_id must be a server id."),
                );
            }
        },
    };
    let target = view::invite_url(&state.cfg.client_id, state.cfg.invite_permissions, guild_id);
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, target)
        .header(header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn start_oauth(State(state): State<AppState>) -> Response {
    let state_token = guard::random_token();
    let verifier = guard::random_token();
    let challenge = guard::code_challenge(&verifier);
    let mut url = "https://discord.com/api/oauth2/authorize?".to_string();
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", &state.cfg.client_id)
            .append_pair("redirect_uri", &state.cfg.redirect_url)
            .append_pair("response_type", "code")
            .append_pair("scope", "identify guilds")
            .append_pair("state", &state_token)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("prompt", "consent")
            .finish(),
    );
    let cookie = set_cookie(
        "overlord_oauth",
        &format!("{state_token}.{verifier}"),
        600,
        state.cfg.cookie_secure,
    );
    redirect(&url, &[cookie])
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn oauth_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let clear = set_cookie("overlord_oauth", "gone", 0, state.cfg.cookie_secure);
    if query.error.is_some() {
        return redirect("/?notice=cancelled", &[clear]);
    }
    let (Some(code), Some(returned_state)) = (query.code, query.state) else {
        return redirect("/?notice=state", &[clear]);
    };
    let Some(cookie) = cookie_value(
        headers.get(header::COOKIE).and_then(|v| v.to_str().ok()),
        "overlord_oauth",
    ) else {
        return redirect("/?notice=state", &[clear]);
    };
    let Some((expected_state, verifier)) = cookie.split_once('.') else {
        return redirect("/?notice=state", &[clear]);
    };
    if !guard::ct_eq(expected_state, &returned_state) {
        return redirect("/?notice=state", &[clear]);
    }
    let tokens = match state.discord.exchange_code(&code, verifier).await {
        Ok(tokens) => tokens,
        Err(_) => return redirect("/?notice=discord", &[clear]),
    };
    let user = match state.discord.identity(&tokens.access_token).await {
        Ok(user) => user,
        Err(_) => return redirect("/?notice=discord", &[clear]),
    };
    let now = chrono::Utc::now().timestamp();
    let session = Session {
        id: guard::random_token(),
        user_id: user.id,
        username: user.username,
        avatar: user.avatar,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        token_expires_at: now + tokens.expires_in,
        csrf_token: guard::random_token(),
        expires_at: now + 60 * 60 * 24 * 7,
    };
    if store::create_session(&state.db, &session).await.is_err() {
        return redirect("/?notice=discord", &[clear]);
    }
    let session_cookie = set_cookie(
        "overlord_session",
        &session.id,
        60 * 60 * 24 * 7,
        state.cfg.cookie_secure,
    );
    redirect("/servers", &[session_cookie, clear])
}

#[derive(Deserialize)]
struct CsrfForm {
    #[serde(default)]
    csrf: String,
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    HtmlForm(form): HtmlForm<CsrfForm>,
) -> Response {
    let Some(session) = session_from(&state, &headers).await else {
        return redirect("/", &[]);
    };
    if !form_csrf_ok(&state, &headers, &session.csrf_token, &form.csrf) {
        return redirect("/servers?notice=csrf", &[]);
    }
    let _ = store::delete_session(&state.db, &session.id).await;
    let clear = set_cookie("overlord_session", "gone", 0, state.cfg.cookie_secure);
    redirect("/", &[clear])
}

async fn servers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NoticeQuery>,
) -> Response {
    let Some(session) = session_from(&state, &headers).await else {
        return redirect("/", &[]);
    };
    let Ok(guilds) = manageable(&state, &session).await else {
        return redirect("/?notice=discord", &[]);
    };
    let bot = state.discord.bot_guild_ids().await.unwrap_or_default();
    let mut ready = Vec::new();
    for guild in &guilds {
        if !bot.contains(&guild.id) {
            continue;
        }
        if let Ok(settings) = guild_settings::load(&state.db, guild.id).await {
            if settings.configured
                && (settings.mod_log_channel_id.is_some()
                    || settings.member_role_id.is_some()
                    || settings.ticket_category_id.is_some())
            {
                ready.push(guild.id);
            }
        }
    }
    html(
        StatusCode::OK,
        view::servers_page(
            &guilds,
            &bot,
            &ready,
            &session.username,
            session.avatar.as_deref(),
            session.user_id,
            notice(query.notice.as_deref()),
        ),
    )
}

async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    Query(query): Query<NoticeQuery>,
) -> Response {
    page(
        &state,
        &headers,
        guild_id,
        Nav::Overview,
        query.notice.as_deref(),
        |ctx| {
            let state = ctx.state.clone();
            let settings = ctx.settings.clone();
            let guild_name = ctx.guild_name.clone();
            Box::pin(async move {
                let stats =
                    store::week_stats(&state.db, guild_id)
                        .await
                        .unwrap_or(store::WeekStats {
                            infractions_this_week: 0,
                            infractions_last_week: 0,
                            open_tickets: 0,
                            claimed_tickets: 0,
                            closed_this_week: 0,
                            raid_alerts_this_week: 0,
                            last_raid_at: None,
                            temp_rooms: 0,
                        });
                let rows = store::recent_infractions(&state.db, guild_id, 8)
                    .await
                    .unwrap_or_default();
                let names = lookup_names(&state, &rows).await;
                view::overview_body(
                    guild_id,
                    &guild_name,
                    &stats,
                    &rows,
                    &|id| names.get(&id).cloned().unwrap_or_else(|| id.to_string()),
                    &settings,
                )
            })
        },
    )
    .await
}

async fn moderation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
) -> Response {
    page(&state, &headers, guild_id, Nav::Moderation, None, |ctx| {
        let state = ctx.state.clone();
        let guild_name = ctx.guild_name.clone();
        Box::pin(async move {
            let rows = store::recent_infractions(&state.db, guild_id, 40)
                .await
                .unwrap_or_default();
            let names = lookup_names(&state, &rows).await;
            view::moderation_body(guild_id, &guild_name, &rows, &|id| {
                names.get(&id).cloned().unwrap_or_else(|| id.to_string())
            })
        })
    })
    .await
}

async fn security(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    Query(query): Query<NoticeQuery>,
) -> Response {
    page(
        &state,
        &headers,
        guild_id,
        Nav::Security,
        query.notice.as_deref(),
        |ctx| {
            let state = ctx.state.clone();
            let settings = ctx.settings.clone();
            let csrf = ctx.session.csrf_token.clone();
            let guild_name = ctx.guild_name.clone();
            Box::pin(async move {
                let channels = state.discord.channels(guild_id).await.unwrap_or_default();
                let roles = state.discord.roles(guild_id).await.unwrap_or_default();
                view::security_body(guild_id, &guild_name, &settings, &channels, &roles, &csrf)
            })
        },
    )
    .await
}

async fn verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    Query(query): Query<NoticeQuery>,
) -> Response {
    page(
        &state,
        &headers,
        guild_id,
        Nav::Verification,
        query.notice.as_deref(),
        |ctx| {
            let state = ctx.state.clone();
            let settings = ctx.settings.clone();
            let csrf = ctx.session.csrf_token.clone();
            let guild_name = ctx.guild_name.clone();
            Box::pin(async move {
                let channels = state.discord.channels(guild_id).await.unwrap_or_default();
                let roles = state.discord.roles(guild_id).await.unwrap_or_default();
                view::verification_body(guild_id, &guild_name, &settings, &channels, &roles, &csrf)
            })
        },
    )
    .await
}

async fn tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    Query(query): Query<NoticeQuery>,
) -> Response {
    page(
        &state,
        &headers,
        guild_id,
        Nav::Tickets,
        query.notice.as_deref(),
        |ctx| {
            let state = ctx.state.clone();
            let settings = ctx.settings.clone();
            let csrf = ctx.session.csrf_token.clone();
            let guild_name = ctx.guild_name.clone();
            Box::pin(async move {
                let channels = state.discord.channels(guild_id).await.unwrap_or_default();
                let roles = state.discord.roles(guild_id).await.unwrap_or_default();
                let stats = store::week_stats(&state.db, guild_id).await.ok();
                view::tickets_body(
                    guild_id,
                    &guild_name,
                    &settings,
                    &channels,
                    &roles,
                    stats.as_ref().map(|s| s.open_tickets).unwrap_or(0),
                    stats.as_ref().map(|s| s.closed_this_week).unwrap_or(0),
                    &csrf,
                )
            })
        },
    )
    .await
}

async fn voice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    Query(query): Query<NoticeQuery>,
) -> Response {
    page(
        &state,
        &headers,
        guild_id,
        Nav::Voice,
        query.notice.as_deref(),
        |ctx| {
            let state = ctx.state.clone();
            let settings = ctx.settings.clone();
            let csrf = ctx.session.csrf_token.clone();
            let guild_name = ctx.guild_name.clone();
            Box::pin(async move {
                let channels = state.discord.channels(guild_id).await.unwrap_or_default();
                let rooms = store::week_stats(&state.db, guild_id)
                    .await
                    .map(|s| s.temp_rooms)
                    .unwrap_or(0);
                view::voice_body(guild_id, &guild_name, &settings, &channels, rooms, &csrf)
            })
        },
    )
    .await
}

async fn logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
) -> Response {
    page(&state, &headers, guild_id, Nav::Logs, None, |ctx| {
        let state = ctx.state.clone();
        let guild_name = ctx.guild_name.clone();
        Box::pin(async move {
            let rows = store::recent_infractions(&state.db, guild_id, 30)
                .await
                .unwrap_or_default();
            let names = lookup_names(&state, &rows).await;
            let raids = raid_lines(&state.db, guild_id).await;
            view::logs_body(guild_id, &guild_name, &rows, &raids, &|id| {
                names.get(&id).cloned().unwrap_or_else(|| id.to_string())
            })
        })
    })
    .await
}

async fn audit_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    Query(query): Query<NoticeQuery>,
) -> Response {
    let days = query.days.unwrap_or(14);
    page(
        &state,
        &headers,
        guild_id,
        Nav::Audit,
        query.notice.as_deref(),
        |ctx| {
            let state = ctx.state.clone();
            let name = ctx.guild_name.clone();
            Box::pin(async move {
                let rows = audit::list(&state.db, guild_id, days, 50)
                    .await
                    .unwrap_or_default();
                view::audit_body(guild_id, &rows, days.clamp(1, 90), &name)
            })
        },
    )
    .await
}

#[derive(Deserialize)]
struct SecurityForm {
    #[serde(default)]
    csrf: String,
    min_account_age_days: i64,
    raid_join_threshold: i64,
    raid_window_seconds: i64,
    raid_action: String,
    unverified_role_id: String,
    mod_log_channel_id: String,
    honeypot_channel_id: String,
    honeypot_enabled: Option<String>,
}

async fn save_security(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    HtmlForm(form): HtmlForm<SecurityForm>,
) -> Response {
    save_with(
        &state,
        &headers,
        guild_id,
        &form.csrf,
        |settings, channels, roles| {
            let input = validate::SecurityInput {
                min_account_age_days: form.min_account_age_days,
                raid_join_threshold: form.raid_join_threshold,
                raid_window_seconds: form.raid_window_seconds,
                raid_action: form.raid_action.clone(),
                unverified_role_id: optional_id(&form.unverified_role_id)?,
                mod_log_channel_id: optional_id(&form.mod_log_channel_id)?,
                honeypot_enabled: form.honeypot_enabled.as_deref() == Some("1"),
                honeypot_channel_id: optional_id(&form.honeypot_channel_id)?,
            };
            validate::security_changes(settings, &input, channels, roles)
        },
        "security",
    )
    .await
}

#[derive(Deserialize)]
struct VerificationForm {
    #[serde(default)]
    csrf: String,
    verify_channel_id: String,
    member_role_id: String,
    unverified_role_id: String,
}

async fn save_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    HtmlForm(form): HtmlForm<VerificationForm>,
) -> Response {
    save_with(
        &state,
        &headers,
        guild_id,
        &form.csrf,
        |settings, channels, roles| {
            let input = validate::VerificationInput {
                verify_channel_id: optional_id(&form.verify_channel_id)?,
                member_role_id: optional_id(&form.member_role_id)?,
                unverified_role_id: optional_id(&form.unverified_role_id)?,
            };
            validate::verification_changes(settings, &input, channels, roles)
        },
        "verification",
    )
    .await
}

#[derive(Deserialize)]
struct TicketForm {
    #[serde(default)]
    csrf: String,
    ticket_category_id: String,
    transcript_channel_id: String,
    max_open_tickets: i64,
    #[serde(default)]
    staff_role: Vec<String>,
}

async fn save_tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    HtmlForm(form): HtmlForm<TicketForm>,
) -> Response {
    save_with(
        &state,
        &headers,
        guild_id,
        &form.csrf,
        |settings, channels, roles| {
            let mut staff = Vec::new();
            for value in &form.staff_role {
                if let Some(id) = optional_id(value)? {
                    staff.push(id);
                }
            }
            let input = validate::TicketInput {
                ticket_category_id: optional_id(&form.ticket_category_id)?,
                transcript_channel_id: optional_id(&form.transcript_channel_id)?,
                staff_role_ids: staff,
                max_open_tickets: form.max_open_tickets,
            };
            validate::ticket_changes(settings, &input, channels, roles)
        },
        "tickets",
    )
    .await
}

#[derive(Deserialize)]
struct VoiceForm {
    #[serde(default)]
    csrf: String,
    join_to_create_channel_id: String,
    voice_category_id: String,
}

async fn save_voice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    HtmlForm(form): HtmlForm<VoiceForm>,
) -> Response {
    save_with(
        &state,
        &headers,
        guild_id,
        &form.csrf,
        |settings, channels, _roles| {
            let input = validate::VoiceInput {
                join_to_create_channel_id: optional_id(&form.join_to_create_channel_id)?,
                voice_category_id: optional_id(&form.voice_category_id)?,
            };
            validate::voice_changes(settings, &input, channels)
        },
        "voice",
    )
    .await
}

#[derive(Deserialize)]
struct CreateForm {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    resource: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    parent_id: String,
    #[serde(default)]
    category_name: String,
    #[serde(default)]
    private: Option<String>,
    #[serde(default)]
    color: String,
    #[serde(default)]
    hoist: Option<String>,
    #[serde(default)]
    mentionable: Option<String>,
}

async fn create_resource(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    HtmlForm(form): HtmlForm<CreateForm>,
) -> Response {
    let Some(session) = session_from(&state, &headers).await else {
        return redirect("/", &[]);
    };
    let Some(target) = create::ResourceTarget::parse(&form.target) else {
        return message(StatusCode::BAD_REQUEST, "That setting cannot be created from here.");
    };
    if !form_csrf_ok(&state, &headers, &session.csrf_token, &form.csrf) {
        return redirect(
            &format!("/servers/{guild_id}/{}?notice=csrf", target.slug()),
            &[],
        );
    }
    if !user_can_manage(&state, &session, guild_id).await {
        return message(
            StatusCode::FORBIDDEN,
            "You need Manage Server in that community.",
        );
    }
    let Ok(access) = state.discord.bot_access(guild_id).await else {
        return message(
            StatusCode::BAD_GATEWAY,
            "Discord could not confirm Overlord's permissions in that server.",
        );
    };
    let reason = create::reason_header(&session.username);
    let result = if form.resource == "role" || target.is_role() {
        create_and_assign_role(&state, guild_id, &session, target, &form, &access, &reason).await
    } else {
        create_and_assign_channel(&state, guild_id, &session, target, &form, &access, &reason).await
    };
    match result {
        Ok(()) => redirect(
            &format!("/servers/{guild_id}/{}?notice=created", target.slug()),
            &[],
        ),
        Err(text) => message(StatusCode::BAD_REQUEST, text),
    }
}

async fn create_and_assign_channel(
    state: &AppState,
    guild_id: u64,
    session: &store::Session,
    target: create::ResourceTarget,
    form: &CreateForm,
    access: &super::api::BotAccess,
    reason: &str,
) -> Result<(), &'static str> {
    if let Some(text) = create::channel_permission_error(access.permissions) {
        return Err(text);
    }
    let kind = create::kind_for_target(target, &form.kind)?;
    let name = create::validate_name(&form.name)?;
    let settings = guild_settings::load(&state.db, guild_id)
        .await
        .map_err(|_| "The settings database could not be read.")?;
    let mut parent = create::parse_optional_id(&form.parent_id)?;
    if !form.category_name.trim().is_empty() {
        let category_name = create::validate_name(&form.category_name)?;
        let category = state
            .discord
            .create_channel(
                guild_id,
                &create::new_channel(
                    category_name.clone(),
                    super::api::ChannelKind::Category,
                    None,
                    false,
                    guild_id,
                    &[],
                    access.bot_id,
                ),
                reason,
            )
            .await
            .map_err(|_| "Discord did not create that category.")?;
        record_create(
            state,
            guild_id,
            session,
            "Created category",
            &category.name,
        )
        .await?;
        parent = Some(category.id);
    }
    let private = form.private.as_deref() == Some("1");
    let channel = state
        .discord
        .create_channel(
            guild_id,
            &create::new_channel(
                name,
                kind,
                parent,
                private,
                guild_id,
                &settings.staff_role_list(),
                access.bot_id,
            ),
            reason,
        )
        .await
        .map_err(|_| "Discord did not create that channel.")?;
    assign_channel(state, guild_id, target, channel.id).await?;
    record_create(state, guild_id, session, "Created channel", &channel.name).await
}

async fn create_and_assign_role(
    state: &AppState,
    guild_id: u64,
    session: &store::Session,
    target: create::ResourceTarget,
    form: &CreateForm,
    access: &super::api::BotAccess,
    reason: &str,
) -> Result<(), &'static str> {
    if let Some(text) = create::role_permission_error(access.permissions) {
        return Err(text);
    }
    let name = create::validate_name(&form.name)?;
    let color = create::parse_color(&form.color)?;
    let role = state
        .discord
        .create_role(
            guild_id,
            &super::api::NewRole {
                name,
                color,
                hoist: form.hoist.as_deref() == Some("1"),
                mentionable: form.mentionable.as_deref() == Some("1"),
                position: create::role_position_below_bot(access.highest_role_position),
            },
            reason,
        )
        .await
        .map_err(|_| "Discord did not create that role.")?;
    assign_role(state, guild_id, target, role.id).await?;
    record_create(state, guild_id, session, "Created role", &role.name).await
}

async fn layout_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
) -> Response {
    page(&state, &headers, guild_id, Nav::Overview, None, |ctx| {
        let state = ctx.state.clone();
        let settings = ctx.settings.clone();
        let csrf = ctx.session.csrf_token.clone();
        let guild_name = ctx.guild_name.clone();
        Box::pin(async move {
            let channels = state.discord.channels(guild_id).await.unwrap_or_default();
            let roles = state.discord.roles(guild_id).await.unwrap_or_default();
            let plan = create::recommended_layout(&settings, &channels, &roles);
            view::layout_body(guild_id, &guild_name, &plan, &channels, &roles, &csrf)
        })
    })
    .await
}

async fn apply_layout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
    HtmlForm(form): HtmlForm<CsrfForm>,
) -> Response {
    let Some(session) = session_from(&state, &headers).await else {
        return redirect("/", &[]);
    };
    if !form_csrf_ok(&state, &headers, &session.csrf_token, &form.csrf) {
        return redirect(&format!("/servers/{guild_id}/layout?notice=csrf"), &[]);
    }
    if !user_can_manage(&state, &session, guild_id).await {
        return message(
            StatusCode::FORBIDDEN,
            "You need Manage Server in that community.",
        );
    }
    let settings = match guild_settings::load(&state.db, guild_id).await {
        Ok(settings) => settings,
        Err(_) => return message(StatusCode::BAD_REQUEST, "The settings database could not be read."),
    };
    let channels = state.discord.channels(guild_id).await.unwrap_or_default();
    let roles = state.discord.roles(guild_id).await.unwrap_or_default();
    let plan = create::recommended_layout(&settings, &channels, &roles);
    let Ok(access) = state.discord.bot_access(guild_id).await else {
        return message(
            StatusCode::BAD_GATEWAY,
            "Discord could not confirm Overlord's permissions in that server.",
        );
    };
    if create::layout_needs_channels(&plan) {
        if let Some(text) = create::channel_permission_error(access.permissions) {
            return message(StatusCode::FORBIDDEN, text);
        }
    }
    if create::layout_needs_roles(&plan) {
        if let Some(text) = create::role_permission_error(access.permissions) {
            return message(StatusCode::FORBIDDEN, text);
        }
    }
    let reason = create::reason_header(&session.username);
    let mut groups: HashMap<&str, u64> = HashMap::new();
    let mut category_id = match plan.iter().find(|item| item.label == "Overlord category") {
        Some(item) => match item.action {
            create::LayoutAction::Reuse(id) => Some(id),
            _ => None,
        },
        None => None,
    };
    for item in &plan {
        let id = if let (Some(group), Some(id)) = (item.group, item.group.and_then(|group| groups.get(group).copied())) {
            let _ = group;
            id
        } else {
            match &item.action {
                create::LayoutAction::Reuse(id) => *id,
                create::LayoutAction::CreateChannel { name, kind } => {
                    let parent = if *kind == super::api::ChannelKind::Category {
                        None
                    } else {
                        category_id
                    };
                    let created = match state
                        .discord
                        .create_channel(
                            guild_id,
                            &create::new_channel(
                                (*name).to_string(),
                                *kind,
                                parent,
                                false,
                                guild_id,
                                &settings.staff_role_list(),
                                access.bot_id,
                            ),
                            &reason,
                        )
                        .await
                    {
                        Ok(created) => created,
                        Err(_) => {
                            return message(StatusCode::BAD_GATEWAY, "Discord did not create one of the channels. Reload the preview and try again.")
                        }
                    };
                    if *kind == super::api::ChannelKind::Category {
                        category_id = Some(created.id);
                    }
                    if let Err(text) = record_create(&state, guild_id, &session, "Created channel", &created.name).await {
                        return message(StatusCode::BAD_REQUEST, text);
                    }
                    if let Some(group) = item.group {
                        groups.insert(group, created.id);
                    }
                    created.id
                }
                create::LayoutAction::CreateRole { name } => {
                    let created = match state
                        .discord
                        .create_role(
                            guild_id,
                            &super::api::NewRole {
                                name: (*name).to_string(),
                                color: 0,
                                hoist: false,
                                mentionable: false,
                                position: create::role_position_below_bot(access.highest_role_position),
                            },
                            &reason,
                        )
                        .await
                    {
                        Ok(created) => created,
                        Err(_) => {
                            return message(StatusCode::BAD_GATEWAY, "Discord did not create one of the roles. Reload the preview and try again.")
                        }
                    };
                    if let Err(text) = record_create(&state, guild_id, &session, "Created role", &created.name).await {
                        return message(StatusCode::BAD_REQUEST, text);
                    }
                    created.id
                }
            }
        };
        if let Err(text) = assign_field(&state, guild_id, item.field, id).await {
            return message(StatusCode::BAD_REQUEST, text);
        }
    }
    redirect(&format!("/servers/{guild_id}?notice=created"), &[])
}

async fn assign_channel(
    state: &AppState,
    guild_id: u64,
    target: create::ResourceTarget,
    id: u64,
) -> Result<(), &'static str> {
    let Some(field) = target.setting() else {
        return Err("That setting cannot store a channel.");
    };
    guild_settings::set_field(&state.db, guild_id, field, Some(id as i64))
        .await
        .map_err(|_| "The settings database could not be written.")?;
    Ok(())
}

async fn assign_role(
    state: &AppState,
    guild_id: u64,
    target: create::ResourceTarget,
    id: u64,
) -> Result<(), &'static str> {
    if target == create::ResourceTarget::StaffRole {
        return assign_field(state, guild_id, create::LayoutField::Staff, id).await;
    }
    let Some(field) = target.setting() else {
        return Err("That setting cannot store a role.");
    };
    guild_settings::set_field(&state.db, guild_id, field, Some(id as i64))
        .await
        .map_err(|_| "The settings database could not be written.")?;
    Ok(())
}

async fn assign_field(
    state: &AppState,
    guild_id: u64,
    field: create::LayoutField,
    id: u64,
) -> Result<(), &'static str> {
    match field {
        create::LayoutField::Channel(setting) => {
            guild_settings::set_field(&state.db, guild_id, setting, Some(id as i64))
                .await
                .map_err(|_| "The settings database could not be written.")?;
        }
        create::LayoutField::Staff => {
            let settings = guild_settings::load(&state.db, guild_id)
                .await
                .map_err(|_| "The settings database could not be read.")?;
            let mut ids = settings.staff_role_list();
            if !ids.contains(&id) {
                ids.push(id);
            }
            let csv = ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            guild_settings::set_text(
                &state.db,
                guild_id,
                guild_settings::TextSetting::StaffRoleIds,
                &csv,
            )
            .await
            .map_err(|_| "The settings database could not be written.")?;
            if settings.mod_role_id.is_none() {
                guild_settings::set_field(
                    &state.db,
                    guild_id,
                    guild_settings::SettingField::ModRole,
                    Some(id as i64),
                )
                .await
                .map_err(|_| "The settings database could not be written.")?;
            }
        }
    }
    Ok(())
}

async fn record_create(
    state: &AppState,
    guild_id: u64,
    session: &store::Session,
    field: &str,
    name: &str,
) -> Result<(), &'static str> {
    audit::record(
        &state.db,
        guild_id,
        session.user_id,
        &session.username,
        field,
        "",
        name,
    )
    .await
    .map_err(|_| "The audit log could not be written.")?;
    Ok(())
}

async fn user_can_manage(state: &AppState, session: &store::Session, guild_id: u64) -> bool {
    manageable(state, session)
        .await
        .ok()
        .and_then(|guilds| {
            guilds.into_iter().find(|guild| {
                guild.id == guild_id && guard::can_manage_server(guild.owner, guild.permissions)
            })
        })
        .is_some()
}

fn message(status: StatusCode, text: &'static str) -> Response {
    html(status, view::message_page("Couldn't create that", text))
}

async fn favicon_ico() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/x-icon"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("favicon.ico").as_slice(),
    )
        .into_response()
}

async fn favicon_svg() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("favicon.svg"),
    )
        .into_response()
}

async fn css() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        include_str!("app.css"),
    )
        .into_response()
}

async fn font_geist() -> Response {
    font_response("font/ttf", include_bytes!("fonts/Geist-Variable.ttf"))
}

async fn font_geist_italic() -> Response {
    font_response("font/ttf", include_bytes!("fonts/Geist-Italic.ttf"))
}

async fn font_mono() -> Response {
    font_response("font/woff2", include_bytes!("fonts/GeistMono-Variable.woff2"))
}

fn font_response(content_type: &'static str, bytes: &'static [u8]) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

async fn js() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        include_str!("dashboard.js"),
    )
        .into_response()
}

struct PageCtx {
    state: AppState,
    session: Session,
    settings: crate::guild_settings::GuildSettings,
    guild_name: String,
}

async fn page<F, Fut>(
    state: &AppState,
    headers: &HeaderMap,
    guild_id: u64,
    nav: Nav,
    notice_code: Option<&str>,
    render: F,
) -> Response
where
    F: FnOnce(PageCtx) -> Fut,
    Fut: std::future::Future<Output = String>,
{
    let Some(session) = session_from(state, headers).await else {
        return redirect("/", &[]);
    };
    let guilds = match manageable(state, &session).await {
        Ok(guilds) => guilds,
        Err(ApiError::Unauthorized) => return redirect("/?notice=state", &[]),
        Err(_) => return redirect("/?notice=discord", &[]),
    };
    let Some(guild) = guilds.iter().find(|guild| guild.id == guild_id) else {
        return html(
            StatusCode::NOT_FOUND,
            view::message_page(
                "Not found",
                "That server is not in your Manage Server list.",
            ),
        );
    };
    let bot = match state.discord.bot_guild_ids().await {
        Ok(ids) => ids,
        Err(_) => return redirect("/?notice=discord", &[]),
    };
    if !bot.contains(&guild_id) {
        return redirect(&format!("/install?guild_id={guild_id}"), &[]);
    }
    let settings = match guild_settings::load(&state.db, guild_id).await {
        Ok(settings) => settings,
        Err(_) => {
            return html(
                StatusCode::INTERNAL_SERVER_ERROR,
                view::message_page("Unavailable", "The settings database could not be read."),
            )
        }
    };
    let configured = settings.configured
        && (settings.mod_log_channel_id.is_some() || settings.member_role_id.is_some());
    let body = render(PageCtx {
        state: state.clone(),
        session: session.clone(),
        settings,
        guild_name: guild.name.clone(),
    })
    .await;
    html(
        StatusCode::OK,
        view::app_page(Shell {
            guild_id,
            guild_name: &guild.name,
            guilds: &guilds,
            username: &session.username,
            avatar: session.avatar.as_deref(),
            user_id: session.user_id,
            nav,
            configured,
            flash: notice(notice_code),
            csrf: &session.csrf_token,
            body,
        }),
    )
}

async fn save_with<F>(
    state: &AppState,
    headers: &HeaderMap,
    guild_id: u64,
    csrf: &str,
    build: F,
    slug: &str,
) -> Response
where
    F: FnOnce(
        &crate::guild_settings::GuildSettings,
        &[super::api::ChannelChoice],
        &[super::api::RoleChoice],
    ) -> Result<Vec<validate::Change>, &'static str>,
{
    let Some(session) = session_from(state, headers).await else {
        return redirect("/", &[]);
    };
    if !form_csrf_ok(state, headers, &session.csrf_token, csrf) {
        return redirect(&format!("/servers/{guild_id}/{slug}?notice=csrf"), &[]);
    }
    if manageable(state, &session)
        .await
        .ok()
        .and_then(|guilds| {
            guilds.into_iter().find(|guild| {
                guild.id == guild_id && guard::can_manage_server(guild.owner, guild.permissions)
            })
        })
        .is_none()
    {
        return html(
            StatusCode::FORBIDDEN,
            view::message_page("Forbidden", "You need Manage Server in that community."),
        );
    }
    let bot_in = state.discord.bot_guild_ids().await.unwrap_or_default();
    if !bot_in.contains(&guild_id) {
        return html(
            StatusCode::FORBIDDEN,
            view::message_page("Forbidden", "Overlord is not in that server."),
        );
    }
    let settings = match guild_settings::load(&state.db, guild_id).await {
        Ok(settings) => settings,
        Err(_) => return redirect(&format!("/servers/{guild_id}/{slug}?notice=discord"), &[]),
    };
    let channels = match state.discord.channels(guild_id).await {
        Ok(channels) => channels,
        Err(_) => return redirect(&format!("/servers/{guild_id}/{slug}?notice=discord"), &[]),
    };
    let roles = match state.discord.roles(guild_id).await {
        Ok(roles) => roles,
        Err(_) => return redirect(&format!("/servers/{guild_id}/{slug}?notice=discord"), &[]),
    };
    let changes = match build(&settings, &channels, &roles) {
        Ok(changes) => changes,
        Err(_) => return redirect(&format!("/servers/{guild_id}/{slug}?notice=invalid"), &[]),
    };
    for change in &changes {
        let wrote = match &change.write {
            Write::Int(field, value) => {
                guild_settings::set_field(&state.db, guild_id, *field, *value).await
            }
            Write::Text(field, value) => {
                guild_settings::set_text(&state.db, guild_id, *field, value).await
            }
        };
        if wrote.is_err() {
            return redirect(&format!("/servers/{guild_id}/{slug}?notice=discord"), &[]);
        }
        if audit::record(
            &state.db,
            guild_id,
            session.user_id,
            &session.username,
            &change.field,
            &change.old_value,
            &change.new_value,
        )
        .await
        .is_err()
        {
            return redirect(&format!("/servers/{guild_id}/{slug}?notice=discord"), &[]);
        }
    }
    redirect(&format!("/servers/{guild_id}/{slug}?notice=saved"), &[])
}

/// HTML form body. `serde_html_form` accepts a repeated key as zero, one, or many
/// values, which `serde_urlencoded` rejects when only one value is present.
struct HtmlForm<T>(T);

#[async_trait]
impl<S, T> FromRequest<S> for HtmlForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum_extra::extract::Form::<T>::from_request(request, state).await {
            Ok(axum_extra::extract::Form(value)) => Ok(HtmlForm(value)),
            Err(rejection) => {
                tracing::warn!("rejected dashboard form: {rejection}");
                Err(html(
                    StatusCode::BAD_REQUEST,
                    view::message_page(
                        "Could not save those changes",
                        "The form was missing a field or sent a value this page cannot use. Go back and try again.",
                    ),
                ))
            }
        }
    }
}

fn form_csrf_ok(
    state: &AppState,
    headers: &HeaderMap,
    session_token: &str,
    submitted: &str,
) -> bool {
    let token_ok = guard::ct_eq(session_token, submitted.trim());
    let source_ok = guard::csrf_source_ok(guard::CsrfSource {
        origin: header_text(headers, header::ORIGIN),
        sec_fetch_site: header_text(headers, "sec-fetch-site"),
        public_url: &state.cfg.public_url,
        host: header_text(headers, header::HOST),
        forwarded_host: header_text(headers, "x-forwarded-host"),
        forwarded_proto: header_text(headers, "x-forwarded-proto"),
        trust_cloudflare: state.cfg.trust_cloudflare,
    });
    if !source_ok {
        tracing::warn!("dashboard form rejected because the browser origin is not this site");
    } else if !token_ok {
        tracing::warn!("dashboard form rejected because the csrf token does not match this session");
    }
    token_ok && source_ok
}

fn header_text<K>(headers: &HeaderMap, name: K) -> Option<&str>
where
    K: header::AsHeaderName,
{
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn optional_id(value: &str) -> Result<Option<u64>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| "That id is not valid.")
}

async fn session_from(state: &AppState, headers: &HeaderMap) -> Option<Session> {
    let id = cookie_value(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        "overlord_session",
    )?;
    let session = store::load_session(&state.db, id).await.ok()??;
    if session.expires_at < chrono::Utc::now().timestamp() {
        let _ = store::delete_session(&state.db, &session.id).await;
        return None;
    }
    Some(session)
}

async fn manageable(state: &AppState, session: &Session) -> Result<Vec<UserGuild>, ApiError> {
    let guilds = state.discord.user_guilds(&session.access_token).await?;
    Ok(guilds
        .into_iter()
        .filter(|guild| guard::can_manage_server(guild.owner, guild.permissions))
        .collect())
}

async fn lookup_names(state: &AppState, rows: &[store::InfractionRow]) -> HashMap<u64, String> {
    let mut names = HashMap::new();
    for row in rows {
        for id in [row.user_id as u64, row.moderator_id as u64] {
            if names.contains_key(&id) {
                continue;
            }
            let name = state
                .discord
                .display_name(id)
                .await
                .unwrap_or_else(|_| id.to_string());
            names.insert(id, name);
        }
    }
    names
}

async fn raid_lines(db: &DbPool, guild_id: u64) -> Vec<(String, String)> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT created_at, detail FROM security_events WHERE guild_id = ? AND kind = 'raid' ORDER BY id DESC LIMIT 20",
    )
    .bind(guild_id as i64)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[derive(Default)]
    struct Fake {
        guilds: Vec<UserGuild>,
        channels: Vec<super::super::api::ChannelChoice>,
        roles: Vec<super::super::api::RoleChoice>,
        bot_guilds: Vec<u64>,
        access: Option<super::super::api::BotAccess>,
        made: std::sync::Mutex<Vec<u64>>,
    }

    #[async_trait::async_trait]
    impl DiscordApi for Fake {
        async fn exchange_code(
            &self,
            _: &str,
            _: &str,
        ) -> Result<super::super::api::TokenSet, ApiError> {
            Err(ApiError::Unavailable)
        }
        async fn identity(&self, _: &str) -> Result<super::super::api::DiscordUser, ApiError> {
            Err(ApiError::Unavailable)
        }
        async fn user_guilds(&self, _: &str) -> Result<Vec<UserGuild>, ApiError> {
            Ok(self.guilds.clone())
        }
        async fn bot_guild_ids(&self) -> Result<Vec<u64>, ApiError> {
            Ok(self.bot_guilds.clone())
        }
        async fn channels(
            &self,
            _: u64,
        ) -> Result<Vec<super::super::api::ChannelChoice>, ApiError> {
            Ok(self.channels.clone())
        }
        async fn roles(&self, _: u64) -> Result<Vec<super::super::api::RoleChoice>, ApiError> {
            Ok(self.roles.clone())
        }
        async fn display_name(&self, user_id: u64) -> Result<String, ApiError> {
            Ok(user_id.to_string())
        }
        async fn bot_access(&self, _: u64) -> Result<super::super::api::BotAccess, ApiError> {
            self.access.ok_or(ApiError::Unavailable)
        }
        async fn create_channel(
            &self,
            _: u64,
            spec: &super::super::api::NewChannel,
            _: &str,
        ) -> Result<super::super::api::ChannelChoice, ApiError> {
            let mut made = self.made.lock().unwrap_or_else(|err| err.into_inner());
            let id = 900 + made.len() as u64;
            made.push(id);
            Ok(super::super::api::ChannelChoice {
                id,
                name: spec.name.clone(),
                kind: spec.kind,
            })
        }
        async fn create_role(
            &self,
            _: u64,
            spec: &super::super::api::NewRole,
            _: &str,
        ) -> Result<super::super::api::RoleChoice, ApiError> {
            let mut made = self.made.lock().unwrap_or_else(|err| err.into_inner());
            let id = 700 + made.len() as u64;
            made.push(id);
            Ok(super::super::api::RoleChoice {
                id,
                name: spec.name.clone(),
            })
        }
    }

    #[tokio::test]
    async fn install_redirects_to_discord_with_optional_guild() {
        let cfg = crate::config::dashboard_from_parts(
            true,
            Some("127.0.0.1:8080"),
            Some("https://overlord.example.com"),
            Some("123456789012345678"),
            Some("placeholder-secret"),
            None,
            None,
            false,
            Some("1099798014998"),
        )
        .unwrap()
        .unwrap();
        let state = AppState {
            db: crate::database::test_pool("install-route").await,
            cfg,
            discord: Arc::new(Fake::default()),
            limiter: Arc::new(RateLimiter::default()),
        };
        let response = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/install")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("client_id=123456789012345678"));
        assert!(location.contains("scope=bot%20applications.commands"));
        assert!(location.contains("permissions=1099798014998"));
        assert!(!location.contains("guild_id="));

        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/install?guild_id=42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("guild_id=42"));
        assert!(location.contains("disable_guild_select=true"));
    }

    fn dashboard_cfg() -> crate::config::DashboardConfig {
        crate::config::dashboard_from_parts(
            true,
            Some("127.0.0.1:8080"),
            Some("https://overlord.example.com"),
            Some("123456789012345678"),
            Some("placeholder-secret"),
            None,
            None,
            false,
            Some("1099798014998"),
        )
        .unwrap()
        .unwrap()
    }

    async fn session(db: &crate::database::DbPool) -> store::Session {
        let now = chrono::Utc::now().timestamp();
        let session = store::Session {
            id: "session-test".into(),
            user_id: 9,
            username: "maple_admin".into(),
            avatar: None,
            access_token: "user-token".into(),
            refresh_token: None,
            token_expires_at: now + 3600,
            csrf_token: "csrf-token".into(),
            expires_at: now + 3600,
        };
        store::create_session(db, &session).await.unwrap();
        session
    }

    fn security_body(csrf: &str, channel: &str) -> String {
        format!(
            "csrf={csrf}&min_account_age_days=4&raid_join_threshold=5&raid_window_seconds=10&raid_action=alert&unverified_role_id=&mod_log_channel_id={channel}&honeypot_channel_id="
        )
    }

    #[tokio::test]
    async fn writes_require_a_session_manage_server_and_are_audited() {
        let db = crate::database::test_pool("dash-auth").await;
        let session = session(&db).await;
        let channel = super::super::api::ChannelChoice {
            id: 555,
            name: "mod-logs".into(),
            kind: super::super::api::ChannelKind::Text,
        };
        let allowed = Fake {
            guilds: vec![UserGuild {
                id: 10,
                name: "Pixel Forge".into(),
                icon: None,
                owner: false,
                permissions: guard::MANAGE_GUILD,
            }],
            channels: vec![channel],
            roles: Vec::new(),
            bot_guilds: vec![10],
            ..Fake::default()
        };
        let denied = Fake {
            guilds: vec![UserGuild {
                id: 10,
                name: "Pixel Forge".into(),
                icon: None,
                owner: false,
                permissions: 0,
            }],
            channels: allowed.channels.clone(),
            roles: Vec::new(),
            bot_guilds: vec![10],
            ..Fake::default()
        };

        let anonymous = router(AppState {
            db: db.clone(),
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake::default()),
            limiter: Arc::new(RateLimiter::default()),
        });
        let response = anonymous
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/servers/10/security")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(security_body("csrf-token", "555")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap().to_str().unwrap(),
            "/"
        );

        let forbidden = router(AppState {
            db: db.clone(),
            cfg: dashboard_cfg(),
            discord: Arc::new(denied),
            limiter: Arc::new(RateLimiter::default()),
        });
        let response = forbidden
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/servers/10/security")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", format!("overlord_session={}", session.id))
                    .body(Body::from(security_body("csrf-token", "555")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(crate::audit::list(&db, 10, 14, 20).await.unwrap().is_empty());

        let allowed_app = router(AppState {
            db: db.clone(),
            cfg: dashboard_cfg(),
            discord: Arc::new(allowed),
            limiter: Arc::new(RateLimiter::default()),
        });
        let response = allowed_app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/servers/10/security")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", format!("overlord_session={}", session.id))
                    .body(Body::from(security_body("csrf-token", "555")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let settings = crate::guild_settings::load(&db, 10).await.unwrap();
        assert_eq!(settings.mod_log_channel_id, Some(555));
        assert_eq!(settings.min_account_age_days, 4);
        let rows = crate::audit::list(&db, 10, 14, 20).await.unwrap();
        assert!(rows.iter().any(|row| row.field == "Mod log channel" && row.new_value == "#mod-logs"));
        assert!(rows.iter().all(|row| row.username == "maple_admin"));
        assert!(crate::audit::list(&db, 11, 14, 20).await.unwrap().is_empty());
    }

    fn ticket_app(db: crate::database::DbPool) -> Router {
        router(AppState {
            db,
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake {
                guilds: vec![UserGuild {
                    id: 10,
                    name: "Pixel Forge".into(),
                    icon: None,
                    owner: false,
                    permissions: guard::MANAGE_GUILD,
                }],
                channels: vec![
                    super::super::api::ChannelChoice {
                        id: 30,
                        name: "tickets".into(),
                        kind: super::super::api::ChannelKind::Category,
                    },
                    super::super::api::ChannelChoice {
                        id: 31,
                        name: "transcripts".into(),
                        kind: super::super::api::ChannelKind::Text,
                    },
                ],
                roles: vec![
                    super::super::api::RoleChoice {
                        id: 20,
                        name: "Staff".into(),
                    },
                    super::super::api::RoleChoice {
                        id: 21,
                        name: "Mods".into(),
                    },
                ],
                bot_guilds: vec![10],
                ..Fake::default()
            }),
            limiter: Arc::new(RateLimiter::default()),
        })
    }

    fn ticket_body(staff: &[&str]) -> String {
        let mut body = String::from(
            "csrf=csrf-token&ticket_category_id=30&transcript_channel_id=31&max_open_tickets=2",
        );
        for id in staff {
            body.push_str("&staff_role=");
            body.push_str(id);
        }
        body
    }

    async fn post_tickets(app: Router, body: String) -> Response {
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/servers/10/tickets")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("cookie", "overlord_session=session-test")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn staff_roles_accept_zero_one_or_many_values() {
        let db = crate::database::test_pool("dash-staff").await;
        session(&db).await;

        let response = post_tickets(ticket_app(db.clone()), ticket_body(&[])).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let saved = crate::guild_settings::load(&db, 10).await.unwrap();
        assert!(saved.staff_role_list().is_empty());

        let response = post_tickets(ticket_app(db.clone()), ticket_body(&[""])).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let saved = crate::guild_settings::load(&db, 10).await.unwrap();
        assert!(saved.staff_role_list().is_empty());

        let response = post_tickets(ticket_app(db.clone()), ticket_body(&["20"])).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let saved = crate::guild_settings::load(&db, 10).await.unwrap();
        assert_eq!(saved.staff_role_list(), vec![20]);

        let response = post_tickets(ticket_app(db.clone()), ticket_body(&["20", ""])).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let saved = crate::guild_settings::load(&db, 10).await.unwrap();
        assert_eq!(saved.staff_role_list(), vec![20]);

        let response = post_tickets(ticket_app(db.clone()), ticket_body(&["20", "21"])).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let saved = crate::guild_settings::load(&db, 10).await.unwrap();
        assert_eq!(saved.staff_role_list(), vec![20, 21]);
    }

    fn settings_app(db: crate::database::DbPool) -> Router {
        router(AppState {
            db,
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake {
                guilds: vec![UserGuild {
                    id: 10,
                    name: "Pixel Forge".into(),
                    icon: None,
                    owner: false,
                    permissions: guard::MANAGE_GUILD,
                }],
                channels: vec![
                    super::super::api::ChannelChoice {
                        id: 555,
                        name: "mod-logs".into(),
                        kind: super::super::api::ChannelKind::Text,
                    },
                    super::super::api::ChannelChoice {
                        id: 30,
                        name: "tickets".into(),
                        kind: super::super::api::ChannelKind::Category,
                    },
                    super::super::api::ChannelChoice {
                        id: 31,
                        name: "transcripts".into(),
                        kind: super::super::api::ChannelKind::Text,
                    },
                    super::super::api::ChannelChoice {
                        id: 40,
                        name: "create".into(),
                        kind: super::super::api::ChannelKind::Voice,
                    },
                ],
                roles: vec![super::super::api::RoleChoice {
                    id: 20,
                    name: "Staff".into(),
                }],
                bot_guilds: vec![10],
                ..Fake::default()
            }),
            limiter: Arc::new(RateLimiter::default()),
        })
    }

    fn settings_csrf(html: &str) -> String {
        let start = html
            .find("<form method=\"post\" data-save")
            .expect("settings form");
        let form = &html[start..];
        let end = form.find("</form>").expect("form end");
        let form = &form[..end];
        let marker = "name=\"csrf\" value=\"";
        let at = form.find(marker).expect("csrf field") + marker.len();
        let value = &form[at..];
        value[..value.find('"').expect("csrf end")].to_string()
    }

    async fn settings_request(db: &crate::database::DbPool, request: Request<Body>) -> Response {
        settings_app(db.clone())
            .oneshot(request)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn each_settings_page_saves_with_its_own_token() {
        let db = crate::database::test_pool("dash-csrf").await;
        let mut session = session(&db).await;
        session.csrf_token = "ab-cd_ef".into();
        sqlx::query("UPDATE dashboard_sessions SET csrf_token = ? WHERE id = ?")
            .bind(&session.csrf_token)
            .bind(&session.id)
            .execute(&db)
            .await
            .unwrap();
        let pages = [
            (
                "/servers/10/security",
                "min_account_age_days=4&raid_join_threshold=5&raid_window_seconds=10&raid_action=alert&unverified_role_id=&mod_log_channel_id=555&honeypot_channel_id=",
                "mod log",
            ),
            (
                "/servers/10/verification",
                "verify_channel_id=555&member_role_id=20&unverified_role_id=",
                "member role",
            ),
            (
                "/servers/10/tickets",
                "ticket_category_id=30&transcript_channel_id=31&max_open_tickets=2",
                "ticket category",
            ),
            (
                "/servers/10/voice",
                "join_to_create_channel_id=40&voice_category_id=30",
                "join to create",
            ),
        ];
        for (path, fields, _label) in pages {
            let response = settings_request(
                &db,
                Request::builder()
                    .uri(path)
                    .header("cookie", "overlord_session=session-test")
                    .header("origin", "https://overlord.example.com")
                    .header("sec-fetch-site", "same-origin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let html = String::from_utf8(
                axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            let token = settings_csrf(&html);
            assert_eq!(token, session.csrf_token, "{path}");
            assert!(html.contains("name=\"csrf\""));

            let wrong = settings_request(
                &db,
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", "overlord_session=session-test")
                    .header("origin", "https://overlord.example.com")
                    .header("sec-fetch-site", "same-origin")
                    .body(Body::from(format!("csrf=not-the-token&{fields}")))
                    .unwrap(),
            )
            .await;
            assert_eq!(wrong.status(), StatusCode::SEE_OTHER, "{path} wrong token");
            assert!(wrong
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("notice=csrf"));

            let missing = settings_request(
                &db,
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", "overlord_session=session-test")
                    .header("origin", "https://overlord.example.com")
                    .header("sec-fetch-site", "same-origin")
                    .body(Body::from(fields.to_string()))
                    .unwrap(),
            )
            .await;
            assert_eq!(missing.status(), StatusCode::SEE_OTHER, "{path} missing token");
            assert!(missing
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("notice=csrf"));

            let foreign = settings_request(
                &db,
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", "overlord_session=session-test")
                    .header("origin", "https://evil.example")
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::from(format!("csrf={token}&{fields}")))
                    .unwrap(),
            )
            .await;
            assert!(foreign
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("notice=csrf"));

            let saved = settings_request(
                &db,
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", "overlord_session=session-test")
                    .header("host", "overlord.example.com")
                    .header("origin", "https://OVERLORD.EXAMPLE.COM:443")
                    .body(Body::from(format!("csrf={token}&{fields}")))
                    .unwrap(),
            )
            .await;
            assert_eq!(saved.status(), StatusCode::SEE_OTHER, "{path} fresh token");
            assert!(saved
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("notice=saved"));
        }
        let settings = crate::guild_settings::load(&db, 10).await.unwrap();
        assert_eq!(settings.mod_log_channel_id, Some(555));
        assert_eq!(settings.member_role_id, Some(20));
        assert_eq!(settings.ticket_category_id, Some(30));
        assert_eq!(settings.join_to_create_channel_id, Some(40));
    }

    #[tokio::test]
    async fn a_bad_form_is_an_html_page() {
        let db = crate::database::test_pool("dash-form-error").await;
        session(&db).await;
        let response = router(AppState {
            db,
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake::default()),
            limiter: Arc::new(RateLimiter::default()),
        })
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/servers/10/security")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("cookie", "overlord_session=session-test")
                .body(Body::from("csrf=csrf-token&min_account_age_days=nope"))
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html"));
        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("Could not save those changes"));
        assert!(!body.contains("deserialize"));
        assert!(!body.contains("<style"));
        assert!(!body.to_ascii_lowercase().contains("style="));
    }

    fn assert_no_inline_active_content(html: &str) {
        let lower = html.to_ascii_lowercase();
        assert!(!lower.contains("style="), "style attribute in {html}");
        assert!(!lower.contains("<style"), "style element in page");
        let mut rest = html;
        while let Some(index) = rest.find("<script") {
            let tag = &rest[index..];
            let end = tag.find('>').expect("script tag");
            let open = &tag[..=end];
            let external = open.contains("src=");
            let nonce = open.contains("nonce=");
            assert!(
                external || nonce,
                "inline script without a nonce: {open}"
            );
            rest = &tag[end + 1..];
        }
    }

    #[tokio::test]
    async fn every_page_matches_the_csp_and_has_no_inline_styles_or_scripts() {
        let db = crate::database::test_pool("csp-pages").await;
        let session = session(&db).await;
        let state = AppState {
            db,
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake {
                guilds: vec![UserGuild {
                    id: 10,
                    name: "Pixel Forge".into(),
                    icon: None,
                    owner: false,
                    permissions: guard::MANAGE_GUILD,
                }],
                channels: vec![super::super::api::ChannelChoice {
                    id: 555,
                    name: "mod-logs".into(),
                    kind: super::super::api::ChannelKind::Text,
                }],
                roles: vec![super::super::api::RoleChoice {
                    id: 20,
                    name: "Staff".into(),
                }],
                bot_guilds: vec![10],
                ..Fake::default()
            }),
            limiter: Arc::new(RateLimiter::default()),
        };
        let cookie = format!("overlord_session={}", session.id);
        let pages = [
            ("/", None),
            ("/servers", Some(cookie.as_str())),
            ("/servers/10", Some(cookie.as_str())),
            ("/servers/10/moderation", Some(cookie.as_str())),
            ("/servers/10/security", Some(cookie.as_str())),
            ("/servers/10/verification", Some(cookie.as_str())),
            ("/servers/10/tickets", Some(cookie.as_str())),
            ("/servers/10/voice", Some(cookie.as_str())),
            ("/servers/10/logs", Some(cookie.as_str())),
            ("/servers/10/audit", Some(cookie.as_str())),
        ];
        for (path, cookie) in pages {
            let mut builder = Request::builder().uri(path);
            if let Some(cookie) = cookie {
                builder = builder.header("cookie", cookie);
            }
            let response = router(state.clone())
                .oneshot(builder.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                response
                    .headers()
                    .get("content-security-policy")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                guard::CONTENT_SECURITY_POLICY,
                "{path}"
            );
            let policy = guard::CONTENT_SECURITY_POLICY;
            assert!(policy.contains("script-src 'self'"));
            assert!(policy.contains("style-src 'self'"));
            assert!(policy.contains("form-action 'self'"));
            assert!(!policy.contains("unsafe-inline"));
            assert!(!policy.contains("unsafe-eval"));
            assert!(!policy.contains("cloudflareinsights"));
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let html = String::from_utf8(body.to_vec()).unwrap();
            assert_no_inline_active_content(&html);
            assert!(html.contains("/favicon.svg"), "{path}");
            if path.starts_with("/servers/10") {
                assert!(html.contains("tabbar"), "{path} mobile nav");
            }
        }
        let css = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/assets/app.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let css_body = String::from_utf8(
            axum::body::to_bytes(css.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(css_body.contains("@media (max-width: 980px)"));
        assert!(css_body.contains(".m-tabbar"));
        assert!(css_body.contains("font-family: \"Geist\""));
        assert!(css_body.contains("nav.crumbs ol {\n  display: flex;\n  flex-direction: row;"));
        assert!(css_body.contains("nav.crumbs li::marker {\n  content: \"\";\n}"));
        let missing = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/servers/99")
                    .header("cookie", cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            missing
                .headers()
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap(),
            guard::CONTENT_SECURITY_POLICY
        );
        let missing_html = String::from_utf8(
            axum::body::to_bytes(missing.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert_no_inline_active_content(&missing_html);
        assert!(missing_html.contains("/favicon.ico"));
        let login = router(state.clone())
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let login_html = String::from_utf8(
            axum::body::to_bytes(login.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(login_html.contains("href=\"/auth/discord\""));
        assert!(!login_html.contains("<form"));

        let icon = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/favicon.ico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(icon.status(), StatusCode::OK);
        assert_eq!(
            icon.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/x-icon"
        );
        let svg = router(state)
            .oneshot(
                Request::builder()
                    .uri("/favicon.svg")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(svg.status(), StatusCode::OK);
        let svg_body = axum::body::to_bytes(svg.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(svg_body.starts_with(b"<svg"));
    }

    #[tokio::test]
    async fn creating_a_channel_requires_manage_channels_and_is_audited() {
        let db = crate::database::test_pool("dash-create").await;
        session(&db).await;
        let denied = router(AppState {
            db: db.clone(),
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake {
                guilds: vec![UserGuild {
                    id: 10,
                    name: "Pixel Forge".into(),
                    icon: None,
                    owner: false,
                    permissions: guard::MANAGE_GUILD,
                }],
                bot_guilds: vec![10],
                access: Some(super::super::api::BotAccess {
                    permissions: 0,
                    highest_role_position: 5,
                    bot_id: 1,
                }),
                ..Fake::default()
            }),
            limiter: Arc::new(RateLimiter::default()),
        });
        let denied = denied
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/servers/10/resources")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", "overlord_session=session-test")
                    .body(Body::from(
                        "csrf=csrf-token&resource=channel&target=mod_log_channel_id&name=mod-logs&kind=text",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::BAD_REQUEST);
        let denied_html = String::from_utf8(
            axum::body::to_bytes(denied.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(denied_html.contains("Manage Channels"));
        assert!(crate::audit::list(&db, 10, 14, 20).await.unwrap().is_empty());

        let allowed = router(AppState {
            db: db.clone(),
            cfg: dashboard_cfg(),
            discord: Arc::new(Fake {
                guilds: vec![UserGuild {
                    id: 10,
                    name: "Pixel Forge".into(),
                    icon: None,
                    owner: false,
                    permissions: guard::MANAGE_GUILD,
                }],
                bot_guilds: vec![10],
                access: Some(super::super::api::BotAccess {
                    permissions: create::MANAGE_CHANNELS | create::MANAGE_ROLES,
                    highest_role_position: 5,
                    bot_id: 1,
                }),
                ..Fake::default()
            }),
            limiter: Arc::new(RateLimiter::default()),
        });
        let response = allowed
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/servers/10/resources")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", "overlord_session=session-test")
                    .body(Body::from(
                        "csrf=csrf-token&resource=channel&target=mod_log_channel_id&name=mod-logs&kind=text",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let settings = crate::guild_settings::load(&db, 10).await.unwrap();
        assert_eq!(settings.mod_log_channel_id, Some(900));
        let rows = crate::audit::list(&db, 10, 14, 20).await.unwrap();
        assert!(rows.iter().any(|row| row.field == "Created channel" && row.new_value == "mod-logs"));
    }
}
