use crate::guild_settings::{GuildSettings, SettingField};

use super::api::{ChannelChoice, ChannelKind, NewChannel, Overwrite, RoleChoice};

pub const VIEW_CHANNEL: u64 = 1 << 10;
pub const MANAGE_CHANNELS: u64 = 1 << 4;
pub const MANAGE_ROLES: u64 = 1 << 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceTarget {
    ModLog,
    Honeypot,
    Verify,
    Transcript,
    JoinToCreate,
    TicketCategory,
    VoiceCategory,
    UnverifiedRole,
    MemberRole,
    StaffRole,
}

impl ResourceTarget {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "mod_log_channel_id" => Self::ModLog,
            "honeypot_channel_id" => Self::Honeypot,
            "verify_channel_id" => Self::Verify,
            "transcript_channel_id" => Self::Transcript,
            "join_to_create_channel_id" => Self::JoinToCreate,
            "ticket_category_id" => Self::TicketCategory,
            "voice_category_id" => Self::VoiceCategory,
            "unverified_role_id" => Self::UnverifiedRole,
            "member_role_id" => Self::MemberRole,
            "staff_role" => Self::StaffRole,
            _ => return None,
        })
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::ModLog | Self::Honeypot => "security",
            Self::Verify | Self::UnverifiedRole | Self::MemberRole => "verification",
            Self::Transcript | Self::TicketCategory | Self::StaffRole => "tickets",
            Self::JoinToCreate | Self::VoiceCategory => "voice",
        }
    }

    pub fn is_role(self) -> bool {
        matches!(
            self,
            Self::UnverifiedRole | Self::MemberRole | Self::StaffRole
        )
    }

    pub fn channel_kind(self) -> Option<ChannelKind> {
        Some(match self {
            Self::ModLog | Self::Honeypot | Self::Verify | Self::Transcript => ChannelKind::Text,
            Self::JoinToCreate => ChannelKind::Voice,
            Self::TicketCategory | Self::VoiceCategory => ChannelKind::Category,
            Self::UnverifiedRole | Self::MemberRole | Self::StaffRole => return None,
        })
    }

    pub fn setting(self) -> Option<SettingField> {
        Some(match self {
            Self::ModLog => SettingField::ModLogChannel,
            Self::Honeypot => SettingField::HoneypotChannel,
            Self::Verify => SettingField::VerifyChannel,
            Self::Transcript => SettingField::TranscriptChannel,
            Self::JoinToCreate => SettingField::JoinToCreateChannel,
            Self::TicketCategory => SettingField::TicketCategory,
            Self::VoiceCategory => SettingField::VoiceCategory,
            Self::UnverifiedRole => SettingField::UnverifiedRole,
            Self::MemberRole => SettingField::MemberRole,
            Self::StaffRole => return None,
        })
    }
}

pub fn channel_permission_error(permissions: u64) -> Option<&'static str> {
    if permissions & MANAGE_CHANNELS == 0 {
        Some("Overlord needs the Manage Channels permission in this server before it can create channels.")
    } else {
        None
    }
}

pub fn role_permission_error(permissions: u64) -> Option<&'static str> {
    if permissions & MANAGE_ROLES == 0 {
        Some("Overlord needs the Manage Roles permission in this server before it can create roles.")
    } else {
        None
    }
}

/// Position strictly under the bot's highest role. None when the bot cannot place one there.
pub fn role_position_below_bot(highest: u32) -> Option<u32> {
    highest.checked_sub(1).filter(|position| *position >= 1)
}

pub fn parse_optional_id(value: &str) -> Result<Option<u64>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .map_err(|_| "That id is not valid.")
}

pub fn validate_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("Names must be between 1 and 100 characters.");
    }
    if name.chars().any(|ch| ch.is_control() || ch == '#' || ch == '@') {
        return Err("Names cannot include #, @, or control characters.");
    }
    Ok(name.to_string())
}

pub fn parse_color(raw: &str) -> Result<u32, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(0);
    }
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Role color must be a hex value like #5865F2.");
    }
    u32::from_str_radix(hex, 16).map_err(|_| "Role color must be a hex value like #5865F2.")
}

pub fn kind_for_target(target: ResourceTarget, requested: &str) -> Result<ChannelKind, &'static str> {
    let kind = match requested {
        "text" => ChannelKind::Text,
        "voice" => ChannelKind::Voice,
        "forum" => ChannelKind::Forum,
        "category" => ChannelKind::Category,
        _ => return Err("Choose a channel type."),
    };
    let ok = match target.channel_kind() {
        Some(ChannelKind::Text) => matches!(kind, ChannelKind::Text | ChannelKind::Forum),
        Some(expected) => kind == expected,
        None => false,
    };
    if ok {
        Ok(kind)
    } else {
        Err("That type does not fit this setting.")
    }
}

pub fn private_overwrites(guild_id: u64, staff: &[u64], bot_id: u64) -> Vec<Overwrite> {
    let mut rules = vec![Overwrite {
        id: guild_id,
        member: false,
        allow: 0,
        deny: VIEW_CHANNEL,
    }];
    for id in staff {
        if *id != 0 && *id != guild_id {
            rules.push(Overwrite {
                id: *id,
                member: false,
                allow: VIEW_CHANNEL,
                deny: 0,
            });
        }
    }
    rules.push(Overwrite {
        id: bot_id,
        member: true,
        allow: VIEW_CHANNEL,
        deny: 0,
    });
    rules
}

pub fn reason_header(username: &str) -> String {
    let user: String = username
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    let plain = format!("Created via Overlord dashboard by {user}");
    percent_encode(&plain)
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutAction {
    Reuse(u64),
    CreateChannel {
        name: &'static str,
        kind: ChannelKind,
    },
    CreateRole {
        name: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutItem {
    pub label: &'static str,
    pub field: LayoutField,
    pub action: LayoutAction,
    /// Creates that share a group happen once and the id is reused.
    pub group: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutField {
    Channel(SettingField),
    Staff,
}

pub fn recommended_layout(
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
) -> Vec<LayoutItem> {
    let category = category_action(settings, channels);
    let category_group = match category {
        LayoutAction::CreateChannel { .. } => Some("overlord-category"),
        LayoutAction::Reuse(_) => None,
        LayoutAction::CreateRole { .. } => None,
    };
    let mut items = vec![
        LayoutItem {
            label: "Overlord category",
            field: LayoutField::Channel(SettingField::TicketCategory),
            action: category.clone(),
            group: category_group,
        },
        LayoutItem {
            label: "Voice category",
            field: LayoutField::Channel(SettingField::VoiceCategory),
            action: if let Some(id) = existing_channel(
                settings.voice_category_id,
                channels,
                ChannelKind::Category,
            ) {
                LayoutAction::Reuse(id)
            } else {
                category.clone()
            },
            group: if existing_channel(
                settings.voice_category_id,
                channels,
                ChannelKind::Category,
            )
            .is_some()
            {
                None
            } else {
                category_group
            },
        },
        channel_item(
            "Mod log",
            SettingField::ModLogChannel,
            settings.mod_log_channel_id,
            channels,
            ChannelKind::Text,
            "mod-logs",
        ),
        channel_item(
            "Verification",
            SettingField::VerifyChannel,
            settings.verify_channel_id,
            channels,
            ChannelKind::Text,
            "verification",
        ),
        channel_item(
            "Transcripts",
            SettingField::TranscriptChannel,
            settings.transcript_channel_id,
            channels,
            ChannelKind::Text,
            "transcripts",
        ),
        channel_item(
            "Join to create",
            SettingField::JoinToCreateChannel,
            settings.join_to_create_channel_id,
            channels,
            ChannelKind::Voice,
            "Join to Create",
        ),
    ];
    if settings.honeypot_enabled {
        items.push(channel_item(
            "Honeypot",
            SettingField::HoneypotChannel,
            settings.honeypot_channel_id,
            channels,
            ChannelKind::Text,
            "honeypot",
        ));
    }
    items.push(role_item(
        "Verified",
        SettingField::MemberRole,
        settings.member_role_id,
        roles,
        "Verified",
    ));
    items.push(role_item(
        "Unverified",
        SettingField::UnverifiedRole,
        settings.unverified_role_id,
        roles,
        "Unverified",
    ));
    let staff_set = settings
        .mod_role_id
        .filter(|id| roles.iter().any(|role| role.id == *id))
        .or_else(|| {
            settings
                .staff_role_list()
                .into_iter()
                .find(|id| roles.iter().any(|role| role.id == *id))
        });
    items.push(LayoutItem {
        label: "Staff",
        field: LayoutField::Staff,
        action: match staff_set {
            Some(id) => LayoutAction::Reuse(id),
            None => LayoutAction::CreateRole { name: "Staff" },
        },
        group: None,
    });
    items
}

pub fn layout_needs_channels(items: &[LayoutItem]) -> bool {
    items.iter().any(|item| {
        matches!(
            item.action,
            LayoutAction::CreateChannel { .. }
        )
    })
}

pub fn layout_needs_roles(items: &[LayoutItem]) -> bool {
    items
        .iter()
        .any(|item| matches!(item.action, LayoutAction::CreateRole { .. }))
}

fn category_action(settings: &GuildSettings, channels: &[ChannelChoice]) -> LayoutAction {
    if let Some(id) = existing_channel(settings.ticket_category_id, channels, ChannelKind::Category)
    {
        return LayoutAction::Reuse(id);
    }
    if let Some(id) = existing_channel(settings.voice_category_id, channels, ChannelKind::Category)
    {
        return LayoutAction::Reuse(id);
    }
    LayoutAction::CreateChannel {
        name: "Overlord",
        kind: ChannelKind::Category,
    }
}

fn channel_item(
    label: &'static str,
    field: SettingField,
    current: Option<u64>,
    channels: &[ChannelChoice],
    kind: ChannelKind,
    name: &'static str,
) -> LayoutItem {
    LayoutItem {
        label,
        field: LayoutField::Channel(field),
        action: match existing_channel(current, channels, kind) {
            Some(id) => LayoutAction::Reuse(id),
            None => LayoutAction::CreateChannel { name, kind },
        },
        group: None,
    }
}

fn role_item(
    label: &'static str,
    field: SettingField,
    current: Option<u64>,
    roles: &[RoleChoice],
    name: &'static str,
) -> LayoutItem {
    LayoutItem {
        label,
        field: LayoutField::Channel(field),
        action: match current.filter(|id| roles.iter().any(|role| role.id == *id)) {
            Some(id) => LayoutAction::Reuse(id),
            None => LayoutAction::CreateRole { name },
        },
        group: None,
    }
}

fn existing_channel(
    current: Option<u64>,
    channels: &[ChannelChoice],
    kind: ChannelKind,
) -> Option<u64> {
    let id = current?;
    channels
        .iter()
        .find(|channel| channel.id == id && channel_fits(channel.kind, kind))
        .map(|channel| channel.id)
}

pub fn channel_fits(actual: ChannelKind, wanted: ChannelKind) -> bool {
    match wanted {
        ChannelKind::Text => matches!(actual, ChannelKind::Text | ChannelKind::Forum),
        other => actual == other,
    }
}

pub fn describe(action: &LayoutAction, channels: &[ChannelChoice], roles: &[RoleChoice]) -> String {
    match action {
        LayoutAction::Reuse(id) => channels
            .iter()
            .find(|channel| channel.id == *id)
            .map(|channel| format!("Reuse #{}", channel.name))
            .or_else(|| {
                roles
                    .iter()
                    .find(|role| role.id == *id)
                    .map(|role| format!("Reuse @{}", role.name))
            })
            .unwrap_or_else(|| format!("Reuse {id}")),
        LayoutAction::CreateChannel { name, kind } => {
            let kind = match kind {
                ChannelKind::Text => "text channel",
                ChannelKind::Voice => "voice channel",
                ChannelKind::Category => "category",
                ChannelKind::Forum => "forum",
                ChannelKind::Other => "channel",
            };
            format!("Create {kind} {name}")
        }
        LayoutAction::CreateRole { name } => format!("Create role {name}"),
    }
}

pub fn new_channel(
    name: String,
    kind: ChannelKind,
    parent_id: Option<u64>,
    private: bool,
    guild_id: u64,
    staff: &[u64],
    bot_id: u64,
) -> NewChannel {
    NewChannel {
        name,
        kind,
        parent_id: if kind == ChannelKind::Category {
            None
        } else {
            parent_id
        },
        private_overwrites: if private && kind != ChannelKind::Category {
            private_overwrites(guild_id, staff, bot_id)
        } else {
            Vec::new()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access_bits(bits: u64) -> u64 {
        bits
    }

    #[test]
    fn default_invite_includes_manage_channels_and_roles() {
        let perms = 1_099_798_014_998u64;
        assert_eq!(perms & MANAGE_CHANNELS, MANAGE_CHANNELS);
        assert_eq!(perms & MANAGE_ROLES, MANAGE_ROLES);
    }

    #[test]
    fn missing_manage_channels_and_roles_are_rejected() {
        assert!(channel_permission_error(0).unwrap().contains("Manage Channels"));
        assert!(channel_permission_error(access_bits(MANAGE_CHANNELS)).is_none());
        assert!(role_permission_error(MANAGE_CHANNELS)
            .unwrap()
            .contains("Manage Roles"));
        assert!(role_permission_error(MANAGE_ROLES).is_none());
    }

    #[test]
    fn new_roles_sit_below_the_bot() {
        assert_eq!(role_position_below_bot(5), Some(4));
        assert_eq!(role_position_below_bot(1), None);
        assert_eq!(role_position_below_bot(0), None);
    }

    #[test]
    fn names_and_colors_are_checked() {
        assert!(validate_name("  mod-logs ").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("#secret").is_err());
        assert!(validate_name("@staff").is_err());
        assert_eq!(parse_color("").unwrap(), 0);
        assert_eq!(parse_color("#5865F2").unwrap(), 0x5865F2);
        assert!(parse_color("blue").is_err());
        assert!(kind_for_target(ResourceTarget::JoinToCreate, "forum").is_err());
        assert_eq!(
            kind_for_target(ResourceTarget::ModLog, "forum").unwrap(),
            ChannelKind::Forum
        );
    }

    #[test]
    fn private_channels_hide_everyone_and_allow_staff_and_the_bot() {
        let rules = private_overwrites(10, &[20, 10], 99);
        assert_eq!(rules[0].id, 10);
        assert!(!rules[0].member);
        assert_eq!(rules[0].deny, VIEW_CHANNEL);
        assert!(rules.iter().any(|rule| rule.id == 20 && rule.allow == VIEW_CHANNEL));
        assert!(rules
            .iter()
            .any(|rule| rule.id == 99 && rule.member && rule.allow == VIEW_CHANNEL));
        assert!(!rules.iter().any(|rule| rule.id == 10 && rule.allow == VIEW_CHANNEL && !rule.member && rule.deny == 0));
    }

    #[test]
    fn reason_names_the_dashboard_user_and_strips_control_characters() {
        let header = reason_header("maple\nadmin");
        assert!(!header.contains('\n'));
        assert!(header.starts_with("Created%20via%20Overlord%20dashboard%20by%20"));
        assert!(header.contains("maple"));
        assert!(header.contains("admin"));
    }

    #[test]
    fn recommended_layout_reuses_saved_ids_and_is_idempotent() {
        let fresh = recommended_layout(&GuildSettings::unconfigured(1), &[], &[]);
        assert!(fresh.iter().any(|item| matches!(
            item.action,
            LayoutAction::CreateChannel {
                name: "Overlord",
                kind: ChannelKind::Category
            }
        )));
        assert!(fresh.iter().any(|item| matches!(
            item.action,
            LayoutAction::CreateChannel {
                name: "mod-logs",
                ..
            }
        )));
        assert!(layout_needs_channels(&fresh));
        assert!(layout_needs_roles(&fresh));

        let mut settings = GuildSettings::unconfigured(1);
        settings.ticket_category_id = Some(30);
        settings.voice_category_id = Some(30);
        settings.mod_log_channel_id = Some(31);
        settings.verify_channel_id = Some(32);
        settings.transcript_channel_id = Some(33);
        settings.join_to_create_channel_id = Some(34);
        settings.honeypot_channel_id = Some(35);
        settings.member_role_id = Some(20);
        settings.unverified_role_id = Some(21);
        settings.mod_role_id = Some(22);
        let channels = vec![
            ChannelChoice {
                id: 30,
                name: "Overlord".into(),
                kind: ChannelKind::Category,
            },
            ChannelChoice {
                id: 31,
                name: "mod-logs".into(),
                kind: ChannelKind::Text,
            },
            ChannelChoice {
                id: 32,
                name: "verification".into(),
                kind: ChannelKind::Text,
            },
            ChannelChoice {
                id: 33,
                name: "transcripts".into(),
                kind: ChannelKind::Forum,
            },
            ChannelChoice {
                id: 34,
                name: "Join to Create".into(),
                kind: ChannelKind::Voice,
            },
            ChannelChoice {
                id: 35,
                name: "honeypot".into(),
                kind: ChannelKind::Text,
            },
        ];
        let roles = vec![
            RoleChoice {
                id: 20,
                name: "Verified".into(),
            },
            RoleChoice {
                id: 21,
                name: "Unverified".into(),
            },
            RoleChoice {
                id: 22,
                name: "Staff".into(),
            },
        ];
        let again = recommended_layout(&settings, &channels, &roles);
        assert!(again.iter().all(|item| matches!(item.action, LayoutAction::Reuse(_))));
        assert!(!layout_needs_channels(&again));
        assert!(!layout_needs_roles(&again));
        let honeypot = again
            .iter()
            .find(|item| item.label == "Honeypot")
            .unwrap();
        assert_eq!(honeypot.action, LayoutAction::Reuse(35));
    }
}
