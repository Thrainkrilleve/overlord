use crate::guild_settings::{GuildSettings, SettingField, TextSetting};

use super::api::{ChannelChoice, ChannelKind, RoleChoice};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Int(SettingField, Option<i64>),
    Text(TextSetting, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub write: Write,
}

#[derive(Debug, Clone)]
pub struct SecurityInput {
    pub min_account_age_days: i64,
    pub raid_join_threshold: i64,
    pub raid_window_seconds: i64,
    pub raid_action: String,
    pub unverified_role_id: Option<u64>,
    pub mod_log_channel_id: Option<u64>,
    pub honeypot_enabled: bool,
    pub honeypot_channel_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct VerificationInput {
    pub verify_channel_id: Option<u64>,
    pub member_role_id: Option<u64>,
    pub unverified_role_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TicketInput {
    pub ticket_category_id: Option<u64>,
    pub transcript_channel_id: Option<u64>,
    pub staff_role_ids: Vec<u64>,
    pub max_open_tickets: i64,
}

#[derive(Debug, Clone)]
pub struct VoiceInput {
    pub join_to_create_channel_id: Option<u64>,
    pub voice_category_id: Option<u64>,
}

pub fn security_changes(
    old: &GuildSettings,
    input: &SecurityInput,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
) -> Result<Vec<Change>, &'static str> {
    if !(0..=3650).contains(&input.min_account_age_days) {
        return Err("Minimum account age must be between 0 and 3650 days.");
    }
    if !(2..=50).contains(&input.raid_join_threshold) {
        return Err("Raid join threshold must be between 2 and 50.");
    }
    if !(5..=120).contains(&input.raid_window_seconds) {
        return Err("Raid window must be between 5 and 120 seconds.");
    }
    let action = match input.raid_action.as_str() {
        "alert" | "lockdown" | "ban" => input.raid_action.as_str(),
        _ => return Err("Choose an action on raid."),
    };
    let unverified = require_role(input.unverified_role_id, roles)?;
    let mod_log = require_channel(input.mod_log_channel_id, channels, ChannelKind::Text)?;
    let honeypot = require_channel(input.honeypot_channel_id, channels, ChannelKind::Text)?;

    let mut changes = Vec::new();
    push_int(
        &mut changes,
        "Minimum account age",
        &format!("{} days", old.min_account_age_days),
        &format!("{} days", input.min_account_age_days),
        SettingField::MinAccountAge,
        Some(input.min_account_age_days),
        old.min_account_age_days != input.min_account_age_days,
    );
    let old_raid = format!(
        "{} joins / {}s",
        old.raid_threshold(),
        old.raid_window_secs()
    );
    let new_raid = format!(
        "{} joins / {}s",
        input.raid_join_threshold, input.raid_window_seconds
    );
    if old_raid != new_raid {
        changes.push(Change {
            field: "Raid threshold".into(),
            old_value: old_raid,
            new_value: new_raid,
            write: Write::Int(
                SettingField::RaidJoinThreshold,
                Some(input.raid_join_threshold),
            ),
        });
        changes.push(Change {
            field: "Raid window".into(),
            old_value: format!("{} seconds", old.raid_window_secs()),
            new_value: format!("{} seconds", input.raid_window_seconds),
            write: Write::Int(
                SettingField::RaidWindowSeconds,
                Some(input.raid_window_seconds),
            ),
        });
    }
    let new_action = action_label(action);
    let old_action = action_label(old.raid_action_kind());
    if old_action != new_action {
        changes.push(Change {
            field: "Action on raid".into(),
            old_value: old_action.into(),
            new_value: new_action.into(),
            write: Write::Text(TextSetting::RaidAction, action.into()),
        });
    }
    push_id(
        &mut changes,
        "Unverified role",
        role_label(old.unverified_role_id, roles),
        role_label(unverified, roles),
        SettingField::UnverifiedRole,
        unverified,
    );
    push_id(
        &mut changes,
        "Mod log channel",
        channel_label(old.mod_log_channel_id, channels),
        channel_label(mod_log, channels),
        SettingField::ModLogChannel,
        mod_log,
    );
    let old_trap = if old.honeypot_enabled { "On" } else { "Off" };
    let new_trap = if input.honeypot_enabled { "On" } else { "Off" };
    if old_trap != new_trap {
        changes.push(Change {
            field: "Honeypot trap".into(),
            old_value: old_trap.into(),
            new_value: new_trap.into(),
            write: Write::Int(
                SettingField::HoneypotEnabled,
                Some(i64::from(input.honeypot_enabled)),
            ),
        });
    }
    push_id(
        &mut changes,
        "Honeypot channel",
        channel_label(old.honeypot_channel_id, channels),
        channel_label(honeypot, channels),
        SettingField::HoneypotChannel,
        honeypot,
    );
    Ok(changes)
}

pub fn verification_changes(
    old: &GuildSettings,
    input: &VerificationInput,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
) -> Result<Vec<Change>, &'static str> {
    let verify = require_channel(input.verify_channel_id, channels, ChannelKind::Text)?;
    let member = require_role(input.member_role_id, roles)?;
    let unverified = require_role(input.unverified_role_id, roles)?;
    let mut changes = Vec::new();
    push_id(
        &mut changes,
        "Verify channel",
        channel_label(old.verify_channel_id, channels),
        channel_label(verify, channels),
        SettingField::VerifyChannel,
        verify,
    );
    push_id(
        &mut changes,
        "Member role",
        role_label(old.member_role_id, roles),
        role_label(member, roles),
        SettingField::MemberRole,
        member,
    );
    push_id(
        &mut changes,
        "Unverified role",
        role_label(old.unverified_role_id, roles),
        role_label(unverified, roles),
        SettingField::UnverifiedRole,
        unverified,
    );
    Ok(changes)
}

pub fn ticket_changes(
    old: &GuildSettings,
    input: &TicketInput,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
) -> Result<Vec<Change>, &'static str> {
    if !(1..=5).contains(&input.max_open_tickets) {
        return Err("Max open tickets must be between 1 and 5.");
    }
    if input.staff_role_ids.len() > 10 {
        return Err("Choose at most 10 staff roles.");
    }
    let category = require_channel(input.ticket_category_id, channels, ChannelKind::Category)?;
    let transcript = require_channel(input.transcript_channel_id, channels, ChannelKind::Text)?;
    let mut staff = Vec::new();
    for id in &input.staff_role_ids {
        let Some(id) = require_role(Some(*id), roles)? else {
            continue;
        };
        if !staff.contains(&id) {
            staff.push(id);
        }
    }
    let mut changes = Vec::new();
    push_id(
        &mut changes,
        "Ticket category",
        channel_label(old.ticket_category_id, channels),
        channel_label(category, channels),
        SettingField::TicketCategory,
        category,
    );
    push_id(
        &mut changes,
        "Transcript channel",
        channel_label(old.transcript_channel_id, channels),
        channel_label(transcript, channels),
        SettingField::TranscriptChannel,
        transcript,
    );
    let old_staff = staff_label(old.staff_role_list(), roles);
    let new_staff = staff_label(staff.clone(), roles);
    if old_staff != new_staff {
        let primary = staff.first().copied();
        changes.push(Change {
            field: "Staff roles".into(),
            old_value: old_staff,
            new_value: new_staff,
            write: Write::Text(
                TextSetting::StaffRoleIds,
                staff
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        });
        changes.push(Change {
            field: "Staff role".into(),
            old_value: role_label(old.mod_role_id, roles),
            new_value: role_label(primary, roles),
            write: Write::Int(SettingField::ModRole, primary.map(|id| id as i64)),
        });
    }
    if old.max_tickets() != input.max_open_tickets {
        changes.push(Change {
            field: "Max open tickets".into(),
            old_value: format!("{} per user", old.max_tickets()),
            new_value: format!("{} per user", input.max_open_tickets),
            write: Write::Int(SettingField::MaxOpenTickets, Some(input.max_open_tickets)),
        });
    }
    Ok(changes)
}

pub fn voice_changes(
    old: &GuildSettings,
    input: &VoiceInput,
    channels: &[ChannelChoice],
) -> Result<Vec<Change>, &'static str> {
    let join = require_channel(
        input.join_to_create_channel_id,
        channels,
        ChannelKind::Voice,
    )?;
    let category = require_channel(input.voice_category_id, channels, ChannelKind::Category)?;
    let mut changes = Vec::new();
    push_id(
        &mut changes,
        "Join-to-create channel",
        channel_label(old.join_to_create_channel_id, channels),
        channel_label(join, channels),
        SettingField::JoinToCreateChannel,
        join,
    );
    push_id(
        &mut changes,
        "Temp voice category",
        channel_label(old.voice_category_id, channels),
        channel_label(category, channels),
        SettingField::VoiceCategory,
        category,
    );
    Ok(changes)
}

fn require_channel(
    id: Option<u64>,
    channels: &[ChannelChoice],
    kind: ChannelKind,
) -> Result<Option<u64>, &'static str> {
    let Some(id) = id else {
        return Ok(None);
    };
    match channels.iter().find(|channel| channel.id == id) {
        Some(channel) if super::create::channel_fits(channel.kind, kind) => Ok(Some(id)),
        Some(_) => Err("That channel is the wrong type for this setting."),
        None => Err("That channel is not in this server."),
    }
}

fn require_role(id: Option<u64>, roles: &[RoleChoice]) -> Result<Option<u64>, &'static str> {
    let Some(id) = id else {
        return Ok(None);
    };
    if roles.iter().any(|role| role.id == id) {
        Ok(Some(id))
    } else {
        Err("That role is not in this server.")
    }
}

fn push_int(
    changes: &mut Vec<Change>,
    field: &str,
    old_value: &str,
    new_value: &str,
    setting: SettingField,
    value: Option<i64>,
    changed: bool,
) {
    if changed {
        changes.push(Change {
            field: field.into(),
            old_value: old_value.into(),
            new_value: new_value.into(),
            write: Write::Int(setting, value),
        });
    }
}

fn push_id(
    changes: &mut Vec<Change>,
    field: &str,
    old_value: String,
    new_value: String,
    setting: SettingField,
    id: Option<u64>,
) {
    if old_value != new_value {
        changes.push(Change {
            field: field.into(),
            old_value,
            new_value,
            write: Write::Int(setting, id.map(|id| id as i64)),
        });
    }
}

pub fn action_label(action: &str) -> &'static str {
    match action {
        "lockdown" => "Lockdown and alert",
        "ban" => "Ban new joins",
        _ => "Alert staff only",
    }
}

pub fn channel_label(id: Option<u64>, channels: &[ChannelChoice]) -> String {
    match id {
        None => "Not set".into(),
        Some(id) => channels
            .iter()
            .find(|channel| channel.id == id)
            .map(|channel| format!("#{}", channel.name))
            .unwrap_or_else(|| format!("#{id}")),
    }
}

pub fn role_label(id: Option<u64>, roles: &[RoleChoice]) -> String {
    match id {
        None => "Not set".into(),
        Some(id) => roles
            .iter()
            .find(|role| role.id == id)
            .map(|role| format!("@{}", role.name))
            .unwrap_or_else(|| format!("@{id}")),
    }
}

fn staff_label(ids: Vec<u64>, roles: &[RoleChoice]) -> String {
    if ids.is_empty() {
        return "None".into();
    }
    ids.into_iter()
        .map(|id| role_label(Some(id), roles))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels() -> Vec<ChannelChoice> {
        vec![
            ChannelChoice {
                id: 10,
                name: "mod-logs".into(),
                kind: ChannelKind::Text,
            },
            ChannelChoice {
                id: 11,
                name: "do-not-type-here".into(),
                kind: ChannelKind::Text,
            },
            ChannelChoice {
                id: 12,
                name: "Support Tickets".into(),
                kind: ChannelKind::Category,
            },
            ChannelChoice {
                id: 13,
                name: "Create".into(),
                kind: ChannelKind::Voice,
            },
        ]
    }

    fn roles() -> Vec<RoleChoice> {
        vec![RoleChoice {
            id: 20,
            name: "Unverified".into(),
        }]
    }

    #[test]
    fn a_channel_from_outside_the_guild_list_is_rejected() {
        let old = GuildSettings::unconfigured(1);
        let mut input = sample(&old);
        input.mod_log_channel_id = Some(999);
        let err = security_changes(&old, &input, &channels(), &roles()).unwrap_err();
        assert!(err.contains("not in this server"));
    }

    #[test]
    fn a_voice_channel_cannot_be_saved_as_the_mod_log() {
        let old = GuildSettings::unconfigured(1);
        let mut input = sample(&old);
        input.mod_log_channel_id = Some(13);
        let err = security_changes(&old, &input, &channels(), &roles()).unwrap_err();
        assert!(err.contains("wrong type"));
    }

    #[test]
    fn saving_security_records_only_what_changed() {
        let old = GuildSettings::unconfigured(1);
        let mut input = sample(&old);
        input.min_account_age_days = 3;
        input.mod_log_channel_id = Some(10);
        input.honeypot_enabled = true;
        input.honeypot_channel_id = Some(11);
        input.raid_action = "lockdown".into();
        let changes = security_changes(&old, &input, &channels(), &roles()).unwrap();
        let fields: Vec<_> = changes.iter().map(|change| change.field.as_str()).collect();
        assert!(fields.contains(&"Mod log channel"));
        assert!(fields.contains(&"Honeypot channel"));
        assert!(fields.contains(&"Action on raid"));
        assert!(!fields.contains(&"Minimum account age"));
        let log = changes
            .iter()
            .find(|change| change.field == "Mod log channel")
            .unwrap();
        assert_eq!(log.old_value, "Not set");
        assert_eq!(log.new_value, "#mod-logs");
        assert_eq!(log.write, Write::Int(SettingField::ModLogChannel, Some(10)));
    }

    fn sample(old: &GuildSettings) -> SecurityInput {
        SecurityInput {
            min_account_age_days: old.min_account_age_days,
            raid_join_threshold: old.raid_join_threshold,
            raid_window_seconds: old.raid_window_seconds,
            raid_action: old.raid_action.clone(),
            unverified_role_id: old.unverified_role_id,
            mod_log_channel_id: old.mod_log_channel_id,
            honeypot_enabled: old.honeypot_enabled,
            honeypot_channel_id: old.honeypot_channel_id,
        }
    }
}
