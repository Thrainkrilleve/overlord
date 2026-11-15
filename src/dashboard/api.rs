use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    Voice,
    Category,
    Forum,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelChoice {
    pub id: u64,
    pub name: String,
    pub kind: ChannelKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleChoice {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct UserGuild {
    pub id: u64,
    pub name: String,
    /// Discord icon hash from the user's guild list (`GET /users/@me/guilds`), the same hash the bot cache stores.
    pub icon: Option<String>,
    pub owner: bool,
    pub permissions: u64,
}

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}

#[derive(Debug, Clone)]
pub struct DiscordUser {
    pub id: u64,
    pub username: String,
    pub avatar: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Unavailable,
}

#[async_trait::async_trait]
pub trait DiscordApi: Send + Sync {
    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<TokenSet, ApiError>;
    async fn identity(&self, access_token: &str) -> Result<DiscordUser, ApiError>;
    async fn user_guilds(&self, access_token: &str) -> Result<Vec<UserGuild>, ApiError>;
    async fn bot_guild_ids(&self) -> Result<Vec<u64>, ApiError>;
    async fn channels(&self, guild_id: u64) -> Result<Vec<ChannelChoice>, ApiError>;
    async fn roles(&self, guild_id: u64) -> Result<Vec<RoleChoice>, ApiError>;
    async fn display_name(&self, user_id: u64) -> Result<String, ApiError>;
    async fn bot_access(&self, guild_id: u64) -> Result<BotAccess, ApiError>;
    async fn create_channel(
        &self,
        guild_id: u64,
        spec: &NewChannel,
        reason: &str,
    ) -> Result<ChannelChoice, ApiError>;
    async fn create_role(
        &self,
        guild_id: u64,
        spec: &NewRole,
        reason: &str,
    ) -> Result<RoleChoice, ApiError>;
}

#[derive(Debug, Clone, Copy)]
pub struct BotAccess {
    pub permissions: u64,
    pub highest_role_position: u32,
    pub bot_id: u64,
}

#[derive(Debug, Clone)]
pub struct NewChannel {
    pub name: String,
    pub kind: ChannelKind,
    pub parent_id: Option<u64>,
    pub private_overwrites: Vec<Overwrite>,
}

#[derive(Debug, Clone)]
pub struct Overwrite {
    pub id: u64,
    pub member: bool,
    pub allow: u64,
    pub deny: u64,
}

#[derive(Debug, Clone)]
pub struct NewRole {
    pub name: String,
    pub color: u32,
    pub hoist: bool,
    pub mentionable: bool,
    /// Placed strictly under the bot's highest role when that role is above @everyone.
    pub position: Option<u32>,
}

pub struct LiveDiscord {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    redirect_url: String,
    bot_token: String,
    names: Mutex<HashMap<u64, (String, Instant)>>,
}

impl LiveDiscord {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_url: String,
        bot_token: String,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("OverlordDashboard/0.1")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            client_id,
            client_secret,
            redirect_url,
            bot_token,
            names: Mutex::new(HashMap::new()),
        }
    }

    async fn bot_write(
        &self,
        method: &str,
        url: &str,
        body: serde_json::Value,
        reason: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let response = self
            .http
            .request(
                method.parse().map_err(|_| ApiError::Unavailable)?,
                url,
            )
            .header("Authorization", format!("Bot {}", self.bot_token))
            .header("X-Audit-Log-Reason", reason)
            .json(&body)
            .send()
            .await
            .map_err(|_| ApiError::Unavailable)?;
        status_json(response).await
    }

    async fn bot_json(&self, url: &str) -> Result<serde_json::Value, ApiError> {
        let response = self
            .http
            .get(url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .send()
            .await
            .map_err(|_| ApiError::Unavailable)?;
        status_json(response).await
    }
}

#[async_trait::async_trait]
impl DiscordApi for LiveDiscord {
    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<TokenSet, ApiError> {
        token_request(
            &self.http,
            &[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.redirect_url.as_str()),
                ("code_verifier", verifier),
            ],
        )
        .await
    }

    async fn identity(&self, access_token: &str) -> Result<DiscordUser, ApiError> {
        let value = user_json(
            &self.http,
            access_token,
            "https://discord.com/api/v10/users/@me",
        )
        .await?;
        let raw: RawUser = serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
        Ok(DiscordUser {
            id: raw.id.parse().map_err(|_| ApiError::Unavailable)?,
            username: raw.global_name.unwrap_or(raw.username),
            avatar: raw.avatar,
        })
    }

    async fn user_guilds(&self, access_token: &str) -> Result<Vec<UserGuild>, ApiError> {
        let mut guilds = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut url = "https://discord.com/api/v10/users/@me/guilds?limit=200".to_string();
            if let Some(after) = &after {
                url.push_str("&after=");
                url.push_str(after);
            }
            let value = user_json(&self.http, access_token, &url).await?;
            let page: Vec<RawGuild> =
                serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
            if page.is_empty() {
                break;
            }
            let full = page.len() == 200;
            after = page.last().map(|guild| guild.id.clone());
            for guild in page {
                let Ok(id) = guild.id.parse() else { continue };
                let Ok(permissions) = guild.permissions.parse() else {
                    continue;
                };
                guilds.push(UserGuild {
                    id,
                    name: guild.name,
                    icon: guild.icon,
                    owner: guild.owner,
                    permissions,
                });
            }
            if !full {
                break;
            }
        }
        Ok(guilds)
    }

    async fn bot_guild_ids(&self) -> Result<Vec<u64>, ApiError> {
        let mut ids = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut url = "https://discord.com/api/v10/users/@me/guilds?limit=200".to_string();
            if let Some(after) = &after {
                url.push_str("&after=");
                url.push_str(after);
            }
            let value = self.bot_json(&url).await?;
            let page: Vec<RawId> =
                serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
            if page.is_empty() {
                break;
            }
            let full = page.len() == 200;
            after = page.last().map(|guild| guild.id.clone());
            ids.extend(
                page.into_iter()
                    .filter_map(|guild| guild.id.parse::<u64>().ok()),
            );
            if !full {
                break;
            }
        }
        Ok(ids)
    }

    async fn channels(&self, guild_id: u64) -> Result<Vec<ChannelChoice>, ApiError> {
        let value = self
            .bot_json(&format!(
                "https://discord.com/api/v10/guilds/{guild_id}/channels"
            ))
            .await?;
        let raw: Vec<RawChannel> =
            serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
        let mut channels: Vec<_> = raw
            .into_iter()
            .filter_map(|channel| {
                let id = channel.id.parse().ok()?;
                Some(ChannelChoice {
                    id,
                    name: channel.name.unwrap_or_else(|| id.to_string()),
                    kind: match channel.kind {
                        0 | 5 => ChannelKind::Text,
                        2 => ChannelKind::Voice,
                        4 => ChannelKind::Category,
                        15 => ChannelKind::Forum,
                        _ => ChannelKind::Other,
                    },
                })
            })
            .collect();
        channels.sort_by_key(|channel| channel.name.to_lowercase());
        Ok(channels)
    }

    async fn roles(&self, guild_id: u64) -> Result<Vec<RoleChoice>, ApiError> {
        let value = self
            .bot_json(&format!(
                "https://discord.com/api/v10/guilds/{guild_id}/roles"
            ))
            .await?;
        let raw: Vec<RawRole> = serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
        let mut roles: Vec<_> = raw
            .into_iter()
            .filter_map(|role| {
                let id = role.id.parse().ok()?;
                if id == guild_id {
                    return None;
                }
                Some(RoleChoice {
                    id,
                    name: role.name,
                })
            })
            .collect();
        roles.sort_by_key(|role| role.name.to_lowercase());
        Ok(roles)
    }

    async fn bot_access(&self, guild_id: u64) -> Result<BotAccess, ApiError> {
        let guild = self
            .bot_json(&format!("https://discord.com/api/v10/guilds/{guild_id}"))
            .await?;
        let permissions = guild
            .get("permissions")
            .and_then(|value| value.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let bot_id = self
            .client_id
            .parse()
            .map_err(|_| ApiError::Unavailable)?;
        let member = self
            .bot_json(&format!(
                "https://discord.com/api/v10/guilds/{guild_id}/members/{bot_id}"
            ))
            .await?;
        let member_roles = member
            .get("roles")
            .and_then(|value| value.as_array())
            .map(|roles| {
                roles
                    .iter()
                    .filter_map(|role| role.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let roles = self
            .bot_json(&format!("https://discord.com/api/v10/guilds/{guild_id}/roles"))
            .await?;
        let highest = roles
            .as_array()
            .map(|roles| {
                roles
                    .iter()
                    .filter(|role| {
                        role.get("id")
                            .and_then(|id| id.as_str())
                            .is_some_and(|id| member_roles.iter().any(|mine| mine == id))
                    })
                    .filter_map(|role| role.get("position").and_then(|pos| pos.as_u64()))
                    .max()
                    .unwrap_or(0) as u32
            })
            .unwrap_or(0);
        Ok(BotAccess {
            permissions,
            highest_role_position: highest,
            bot_id,
        })
    }

    async fn create_channel(
        &self,
        guild_id: u64,
        spec: &NewChannel,
        reason: &str,
    ) -> Result<ChannelChoice, ApiError> {
        let kind = match spec.kind {
            ChannelKind::Text => 0,
            ChannelKind::Voice => 2,
            ChannelKind::Category => 4,
            ChannelKind::Forum => 15,
            ChannelKind::Other => return Err(ApiError::Unavailable),
        };
        let mut body = serde_json::json!({
            "name": spec.name,
            "type": kind,
        });
        if let Some(parent) = spec.parent_id {
            body["parent_id"] = serde_json::json!(parent.to_string());
        }
        if !spec.private_overwrites.is_empty() {
            body["permission_overwrites"] = serde_json::json!(spec
                .private_overwrites
                .iter()
                .map(|rule| serde_json::json!({
                    "id": rule.id.to_string(),
                    "type": if rule.member { 1 } else { 0 },
                    "allow": rule.allow.to_string(),
                    "deny": rule.deny.to_string(),
                }))
                .collect::<Vec<_>>());
        }
        let created = self
            .bot_write(
                "POST",
                &format!("https://discord.com/api/v10/guilds/{guild_id}/channels"),
                body,
                reason,
            )
            .await?;
        let id = created
            .get("id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
            .ok_or(ApiError::Unavailable)?;
        Ok(ChannelChoice {
            id,
            name: spec.name.clone(),
            kind: spec.kind,
        })
    }

    async fn create_role(
        &self,
        guild_id: u64,
        spec: &NewRole,
        reason: &str,
    ) -> Result<RoleChoice, ApiError> {
        let created = self
            .bot_write(
                "POST",
                &format!("https://discord.com/api/v10/guilds/{guild_id}/roles"),
                serde_json::json!({
                    "name": spec.name,
                    "permissions": "0",
                    "color": spec.color,
                    "hoist": spec.hoist,
                    "mentionable": spec.mentionable,
                }),
                reason,
            )
            .await?;
        let id: u64 = created
            .get("id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
            .ok_or(ApiError::Unavailable)?;
        if let Some(position) = spec.position {
            let _ = self
                .bot_write(
                    "PATCH",
                    &format!("https://discord.com/api/v10/guilds/{guild_id}/roles"),
                    serde_json::json!([{ "id": id.to_string(), "position": position }]),
                    reason,
                )
                .await;
        }
        Ok(RoleChoice {
            id,
            name: spec.name.clone(),
        })
    }

    async fn display_name(&self, user_id: u64) -> Result<String, ApiError> {
        {
            let cache = self.names.lock().unwrap_or_else(|err| err.into_inner());
            if let Some((name, at)) = cache.get(&user_id) {
                if at.elapsed() < Duration::from_secs(600) {
                    return Ok(name.clone());
                }
            }
        }
        let value = self
            .bot_json(&format!("https://discord.com/api/v10/users/{user_id}"))
            .await?;
        let raw: RawUser = serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
        let name = raw.global_name.unwrap_or(raw.username);
        self.names
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(user_id, (name.clone(), Instant::now()));
        Ok(name)
    }
}

async fn token_request(
    http: &reqwest::Client,
    pairs: &[(&str, &str)],
) -> Result<TokenSet, ApiError> {
    let mut body = String::new();
    for (key, value) in pairs {
        if !body.is_empty() {
            body.push('&');
        }
        body.push_str(&form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>());
        body.push('=');
        body.push_str(&form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>());
    }
    let response = http
        .post("https://discord.com/api/v10/oauth2/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| ApiError::Unavailable)?;
    let value = status_json(response).await?;
    let raw: RawToken = serde_json::from_value(value).map_err(|_| ApiError::Unavailable)?;
    Ok(TokenSet {
        access_token: raw.access_token,
        refresh_token: raw.refresh_token,
        expires_in: raw.expires_in.max(30),
    })
}

async fn user_json(
    http: &reqwest::Client,
    access_token: &str,
    url: &str,
) -> Result<serde_json::Value, ApiError> {
    let response = http
        .get(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|_| ApiError::Unavailable)?;
    status_json(response).await
}

async fn status_json(response: reqwest::Response) -> Result<serde_json::Value, ApiError> {
    let status = response.status();
    if status.as_u16() == 401 {
        return Err(ApiError::Unauthorized);
    }
    if !status.is_success() {
        return Err(ApiError::Unavailable);
    }
    response.json().await.map_err(|_| ApiError::Unavailable)
}

#[derive(Deserialize)]
struct RawToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Deserialize)]
struct RawUser {
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
}

#[derive(Deserialize)]
struct RawGuild {
    id: String,
    name: String,
    icon: Option<String>,
    owner: bool,
    permissions: String,
}

#[derive(Deserialize)]
struct RawId {
    id: String,
}

#[derive(Deserialize)]
struct RawChannel {
    id: String,
    name: Option<String>,
    #[serde(rename = "type")]
    kind: u8,
}

#[derive(Deserialize)]
struct RawRole {
    id: String,
    name: String,
}
