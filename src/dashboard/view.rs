use crate::guild_settings::GuildSettings;

use super::api::{ChannelChoice, ChannelKind, RoleChoice, UserGuild};
use super::guard::esc;
use super::store::{InfractionRow, WeekStats};
use crate::audit::AuditRow;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Overview,
    Moderation,
    Security,
    Verification,
    Tickets,
    Voice,
    Logs,
    Audit,
}

pub struct Shell<'a> {
    pub guild_id: u64,
    pub guild_name: &'a str,
    pub guilds: &'a [UserGuild],
    pub username: &'a str,
    pub avatar: Option<&'a str>,
    pub user_id: u64,
    pub nav: Nav,
    pub configured: bool,
    pub flash: Option<&'static str>,
    pub csrf: &'a str,
    pub body: String,
}

pub fn login_page(flash: Option<&str>) -> String {
    let flash = flash
        .map(|text| format!("<p class=\"flash\">{}</p>", esc(text)))
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">{head}
<body class="login-body">
<div class="login-split">
  <section class="login-hero">
    <a class="brand" href="/">{mark_sm} <span>Overlord</span></a>
    <div class="hero-copy">
      <h1>Watch every gate.<br>Keep every server.</h1>
      <p>A professional dashboard for Overlord — moderation, anti-raid, verification, tickets, and temp voice, with an audit trail of every change.</p>
      <ul class="feature-list">
        <li><span class="feature-icon">{icon_shield}</span><div><strong>Security &amp; anti-raid</strong><p>Account-age filters, join-surge detection, and a honeypot that stops scrapers cold.</p></div></li>
        <li><span class="feature-icon">{icon_ticket}</span><div><strong>Tickets, verification, temp voice</strong><p>Onboarding gates, staff-scoped support tickets, and join-to-create rooms — configured per server.</p></div></li>
        <li><span class="feature-icon">{icon_doc}</span><div><strong>A full audit trail</strong><p>Every dashboard change is logged with who, when, and the old and new values.</p></div></li>
      </ul>
    </div>
    <div class="hero-mark" aria-hidden="true">{mark_lg}</div>
  </section>
  <section class="login-panel">
    <div class="signin">
      <h2>Sign in</h2>
      <p class="muted">Use your Discord account. You'll only see servers where you have the Manage Server permission.</p>
      {flash}
      <a class="discord-btn" href="/auth/discord">{icon_discord} Continue with Discord</a>
      <div class="secure-rule"><span>Secure sign-in</span></div>
      <div class="secure-note">{icon_shield} <p>Overlord never posts one server's logs, tickets, or raid alerts into another. Each community keeps its own settings.</p></div>
      <p class="fine">By continuing, you authorize Overlord to read your server list and apply the settings you save.</p>
    </div>
    <footer class="login-foot"><span>Overlord dashboard</span><span>Sessions stay on this server</span></footer>
  </section>
</div>
</body>
</html>"#,
        head = document_head("Sign in · Overlord"),
        mark_sm = mark("mark mark-sm"),
        mark_lg = mark("mark mark-lg"),
        icon_shield = icon_shield(),
        icon_ticket = icon_ticket(),
        icon_doc = icon_doc(),
        icon_discord = icon_discord(),
    )
}

pub fn servers_page(
    guilds: &[UserGuild],
    bot_guilds: &[u64],
    configured: &[u64],
    username: &str,
    avatar: Option<&str>,
    user_id: u64,
    flash: Option<&str>,
) -> String {
    let mut cards = String::new();
    for guild in guilds {
        let present = bot_guilds.contains(&guild.id);
        let ready = present && configured.contains(&guild.id);
        let badge = if ready {
            "<span class=\"pill ok\">Configured</span>".to_string()
        } else {
            "<span class=\"pill warn\">Needs setup</span>".to_string()
        };
        let href = if present {
            format!("/servers/{}", guild.id)
        } else {
            format!("/install?guild_id={}", guild.id)
        };
        let action = if !present {
            "Add the bot"
        } else if ready {
            "Open dashboard"
        } else {
            "Finish setup"
        };
        cards.push_str(&format!(
            r#"<a class="server-card" data-server="{key}" href="{href}">{badge}<div class="server-main">{avatar}<div><strong>{name}</strong><p class="muted">Manage Server</p></div></div><span class="card-action">{action} <span aria-hidden="true">›</span></span></a>"#,
            key = esc(&guild.name.to_lowercase()),
            href = esc(&href),
            avatar = guild_avatar(guild.id, &guild.name, guild.icon.as_deref(), "avatar", 36),
            name = esc(&guild.name),
        ));
    }
    let invite = "/install";
    let flash = flash
        .map(|text| format!("<p class=\"flash\">{text}</p>"))
        .unwrap_or_default();
    let needs = guilds
        .iter()
        .filter(|guild| !(bot_guilds.contains(&guild.id) && configured.contains(&guild.id)))
        .count();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">{head}
<body>
<header class="topbar"><a class="brand" href="/servers">{mark} <span>Overlord</span></a><div class="who">{avatar_html}<div><strong>{user}</strong><p class="muted">Signed in with Discord</p></div></div></header>
<main class="picker">
  <p class="crumbs">Home / Servers</p>
  <div class="picker-head"><div><h1>Select a server</h1><p class="muted">Only servers where you have Manage Server are listed. Invite Overlord to any others you own or moderate.</p></div><label class="search"><span class="sr">Search servers</span><input data-server-search type="search" placeholder="Search servers"></label></div>
  {flash}
  <div class="server-grid">{cards}<a class="server-card add-card" href="{invite}"><span class="plus">+</span><strong>Add Overlord to a server</strong><p class="muted">Opens the Discord authorization screen. You'll need Manage Server there too.</p></a></div>
  <footer class="picker-foot"><span>Showing {count} servers where you have Manage Server</span><span>{needs} need setup</span></footer>
</main>
<script src="/assets/dashboard.js"></script>
</body></html>"#,
        head = document_head("Servers · Overlord"),
        user = esc(username),
        count = guilds.len(),
        mark = mark("mark mark-20"),
        avatar_html = avatar_html(user_id, avatar, username),
    )
}

pub fn app_page(shell: Shell<'_>) -> String {
    let nav = nav_link(shell.nav, shell.guild_id);
    let badge = if shell.configured {
        "<span class=\"pill ok\">Configured</span>"
    } else {
        "<span class=\"pill warn\">Needs setup</span>"
    };
    let flash = shell
        .flash
        .map(|text| format!("<p class=\"flash\">{text}</p>"))
        .unwrap_or_default();
    let current_icon = shell
        .guilds
        .iter()
        .find(|guild| guild.id == shell.guild_id)
        .and_then(|guild| guild.icon.as_deref());
    let switcher = shell
        .guilds
        .iter()
        .filter(|guild| guild.id != shell.guild_id)
        .take(12)
        .map(|guild| {
            format!(
                r#"<a href="/servers/{id}">{avatar}<span>{name}</span></a>"#,
                id = guild.id,
                avatar = guild_avatar(guild.id, &guild.name, guild.icon.as_deref(), "avatar", 36),
                name = esc(&guild.name)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let summary_avatar = guild_avatar(shell.guild_id, shell.guild_name, current_icon, "avatar", 36);
    let crumb_avatar = guild_avatar(
        shell.guild_id,
        shell.guild_name,
        current_icon,
        "avatar avatar-crumb",
        16,
    );
    format!(
        r#"<!DOCTYPE html>
<html lang="en">{head}
<body class="app">
<aside class="sidebar">
  <a class="brand" href="/servers">{mark} <span>Overlord</span></a>
  <details class="switcher"><summary>{summary_avatar}<span><strong>{guild}</strong><small class="muted">Manage Server</small></span></summary><div class="switcher-menu"><a href="/servers">All servers</a>{switcher}</div></details>
  <p class="nav-label">Manage</p>
  <nav class="sidenav">{nav}</nav>
  <div class="sidebar-user">{avatar}<div><strong>{user}</strong><p class="muted">Server manager</p></div><form method="post" action="/logout"><input type="hidden" name="csrf" value="{csrf}"><button class="icon-btn" type="submit" title="Sign out">↪</button></form></div>
</aside>
<div class="content">
  <header class="content-head"><p class="crumbs">Servers / {crumb_avatar} {guild} / {section}</p><div class="head-pills"><span class="pill ok">Bot online</span>{badge}</div></header>
  {flash}
  {body}
</div>
<nav class="tabbar">{mobile}</nav>
<script src="/assets/dashboard.js"></script>
</body></html>"#,
        head = document_head(&format!(
            "{} · {} · Overlord",
            section_name(shell.nav),
            shell.guild_name
        )),
        guild = esc(shell.guild_name),
        mark = mark("mark mark-18"),
        summary_avatar = summary_avatar,
        crumb_avatar = crumb_avatar,
        avatar = avatar_html(shell.user_id, shell.avatar, shell.username),
        user = esc(shell.username),
        section = esc(section_name(shell.nav)),
        body = shell.body,
        csrf = esc(shell.csrf),
        mobile = mobile_nav(shell.nav, shell.guild_id),
    )
}

pub fn overview_body(
    stats: &WeekStats,
    rows: &[InfractionRow],
    names: &dyn Fn(u64) -> String,
    settings: &GuildSettings,
) -> String {
    let delta = stats.infractions_this_week - stats.infractions_last_week;
    let delta_label = if delta >= 0 {
        format!("+{delta} vs last week")
    } else {
        format!("{delta} vs last week")
    };
    let last_raid = stats
        .last_raid_at
        .as_deref()
        .map(time_tag)
        .unwrap_or_else(|| "No alerts yet".into());
    let actions = if rows.is_empty() {
        "<p class=\"empty\">No moderation actions recorded in this server yet.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid\"><thead><tr><th>When</th><th>Moderator</th><th>Action</th><th>Member</th><th>Reason</th></tr></thead><tbody>");
        for row in rows {
            html.push_str(&action_row(row, names));
        }
        html.push_str("</tbody></table>");
        html
    };
    let icon = icon_shield();
    let honeypot = if settings.honeypot_active() {
        format!("<li><span class=\"status-icon ok\">{icon}</span><div><strong>Honeypot armed</strong><p>Trap channel is live. Staff test posts are logged, never banned.</p></div></li>")
    } else {
        format!("<li><span class=\"status-icon\">{icon}</span><div><strong>Honeypot off</strong><p>Turn it on under Security &amp; Anti-raid when the trap channel exists.</p></div></li>")
    };
    let verification = if settings.member_role_id.is_some() {
        format!("<li><span class=\"status-icon ok\">{icon}</span><div><strong>Verification active</strong><p>Passing the code grants the member role. Codes expire after 5 minutes.</p></div></li>")
    } else {
        format!("<li><span class=\"status-icon\">{icon}</span><div><strong>Verification needs a member role</strong><p>Set it on the Verification page. Until then, a correct code does not grant access.</p></div></li>")
    };
    format!(
        r#"<h1>Overview</h1><p class="lede">Activity and health for this server. Times follow your device.</p>
<section class="stats">
  <article><p>Infractions this week</p><strong>{infra}</strong><span class="gold">{delta}</span></article>
  <article><p>Open tickets</p><strong>{open}</strong><span class="muted">{claimed} claimed</span></article>
  <article><p>Raid alerts</p><strong>{raids}</strong><span class="muted">Last alert {last_raid}</span></article>
  <article><p>Temp voice rooms</p><strong>{rooms}</strong><span class="muted">Open right now</span></article>
</section>
<div class="split">
  <section class="panel"><header><h2>Recent mod actions</h2><span class="muted">This week</span></header>{actions}</section>
  <section class="panel"><header><h2>System status</h2></header><ul class="status-list">{honeypot}{verification}<li><span class="status-icon ok">{icon}</span><div><strong>Anti-raid monitoring</strong><p>Alert when {threshold} joins land within {window} seconds.</p></div></li><li><span class="status-icon">{icon}</span><div><strong>Temp voice</strong><p>{rooms} rooms open from the join-to-create channel.</p></div></li></ul></section>
</div>"#,
        infra = stats.infractions_this_week,
        delta = delta_label,
        open = stats.open_tickets,
        claimed = stats.claimed_tickets,
        raids = stats.raid_alerts_this_week,
        rooms = stats.temp_rooms,
        threshold = settings.raid_threshold(),
        window = settings.raid_window_secs(),
        icon = icon,
    )
}

pub fn moderation_body(rows: &[InfractionRow], names: &dyn Fn(u64) -> String) -> String {
    let table = if rows.is_empty() {
        "<p class=\"empty\">No cases yet. Moderators still use /warn, /timeout, /kick, and /ban in Discord. Cases show up here.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid\"><thead><tr><th>Case</th><th>When</th><th>Moderator</th><th>Action</th><th>Member</th><th>Reason</th></tr></thead><tbody>");
        for row in rows {
            html.push_str(&format!(
                "<tr><td>#{case}</td><td>{when}</td><td>{mod}</td><td><span class=\"action {kind}\">{action}</span></td><td>{member}</td><td>{reason}</td></tr>",
                case = row.case_number,
                when = time_tag(&row.created_at),
                mod = esc(&names(row.moderator_id as u64)),
                kind = esc(&row.action),
                action = esc(&row.action),
                member = esc(&names(row.user_id as u64)),
                reason = esc(&row.reason),
            ));
        }
        html.push_str("</tbody></table>");
        html
    };
    format!(
        r#"<h1>Moderation</h1><p class="lede">Case history for this server. Actions are still taken in Discord so a browser session cannot ban someone by itself.</p><section class="panel">{table}</section>"#
    )
}

pub fn security_body(
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    csrf: &str,
) -> String {
    let age = settings.min_account_age_days.clamp(0, 3650);
    let age_max = age.max(14);
    format!(
        r#"<h1>Security &amp; Anti-raid</h1><p class="lede">Filters, join-surge detection, and the honeypot trap for this server only.</p>
<form method="post" data-save>
<input type="hidden" name="csrf" value="{csrf}">
<div class="form-grid">
  <section class="panel"><header><h2>Minimum account age</h2><output data-age-out>{age} days</output></header><p class="muted">Newer accounts, and those still using a default avatar, raise a staff alert in the mod log.</p><input data-age type="range" min="0" max="{age_max}" name="min_account_age_days" value="{age}"></section>
  <section class="panel"><header><h2>Raid threshold</h2><span class="muted">{threshold} / {window}s</span></header><p class="muted">A sliding window of joins. During a sustained raid, staff are alerted at most once a minute.</p><div class="inline-fields"><label>Joins<input type="number" min="2" max="50" name="raid_join_threshold" value="{threshold}"></label><label>Seconds<input type="number" min="5" max="120" name="raid_window_seconds" value="{window}"></label></div></section>
  <section class="panel wide"><h2>Action on raid</h2><p class="muted">What Overlord should do when the join threshold is crossed. The default only alerts staff.</p><div class="segs">{segs}</div></section>
  <section class="panel"><h2>Unverified role</h2><p class="muted">Granted on join and removed when a member passes the verification gate.</p>{unverified}</section>
  <section class="panel"><h2>Mod log channel</h2><p class="muted">Raid alerts, honeypot hits, and account-age warnings are posted here.</p>{modlog}</section>
  <section class="panel"><header><h2>Honeypot trap</h2><label class="switch"><input type="checkbox" name="honeypot_enabled" value="1" {checked}><span></span></label></header><p class="muted">Anyone who posts in the trap channel is banned and purged. Staff test posts are deleted and logged, never banned.</p>{honeypot}</section>
</div>
<div class="savebar is-hidden" data-savebar><span>Unsaved changes</span><div><button type="reset" data-discard class="ghost">Discard</button><button type="submit" class="gold">Save changes</button></div></div>
</form>"#,
        csrf = esc(csrf),
        threshold = settings.raid_threshold(),
        window = settings.raid_window_secs(),
        segs = segment(
            "raid_action",
            settings.raid_action_kind(),
            &[
                ("alert", "Alert staff only"),
                ("lockdown", "Lockdown and alert"),
                ("ban", "Ban new joins")
            ]
        ),
        unverified = select_roles("unverified_role_id", settings.unverified_role_id, roles),
        modlog = select_channels(
            "mod_log_channel_id",
            settings.mod_log_channel_id,
            channels,
            ChannelKind::Text
        ),
        checked = if settings.honeypot_enabled {
            "checked"
        } else {
            ""
        },
        honeypot = select_channels(
            "honeypot_channel_id",
            settings.honeypot_channel_id,
            channels,
            ChannelKind::Text
        ),
    )
}

pub fn verification_body(
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    csrf: &str,
) -> String {
    format!(
        r#"<h1>Verification</h1><p class="lede">The code gate for this server. Post the button with /setup_verification in the channel members can see before they verify.</p>
<form method="post" data-save>
<input type="hidden" name="csrf" value="{csrf}">
<div class="form-grid">
  <section class="panel"><h2>Verify channel</h2><p class="muted">Shown in /security_status. The button can live here.</p>{verify}</section>
  <section class="panel"><h2>Member role</h2><p class="muted">Granted when the code matches. Without it, a correct code tells the member to contact staff.</p>{member}</section>
  <section class="panel"><h2>Unverified role</h2><p class="muted">Given on join and removed after a successful check.</p>{unverified}</section>
  <section class="panel"><h2>How the gate works</h2><p class="muted">Each person gets a one-time code in this server, valid for 5 minutes. A code from another server does not work here.</p></section>
</div>
<div class="savebar is-hidden" data-savebar><span>Unsaved changes</span><div><button type="reset" data-discard class="ghost">Discard</button><button type="submit" class="gold">Save changes</button></div></div>
</form>"#,
        csrf = esc(csrf),
        verify = select_channels(
            "verify_channel_id",
            settings.verify_channel_id,
            channels,
            ChannelKind::Text
        ),
        member = select_roles("member_role_id", settings.member_role_id, roles),
        unverified = select_roles("unverified_role_id", settings.unverified_role_id, roles),
    )
}

pub fn tickets_body(
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    open_now: i64,
    closed_week: i64,
    csrf: &str,
) -> String {
    let selected: Vec<u64> = settings.staff_role_list();
    let chips = if selected.is_empty() {
        "<p class=\"muted\">No staff roles yet.</p>".into()
    } else {
        selected
            .iter()
            .map(|id| {
                let name = roles.iter().find(|role| role.id == *id).map(|role| role.name.as_str()).unwrap_or("role");
                format!("<span class=\"chip\">{name}<input type=\"hidden\" name=\"staff_role\" value=\"{id}\"></span>", name = esc(name), id = id)
            })
            .collect::<Vec<_>>()
            .join("")
    };
    format!(
        r#"<h1>Tickets</h1><p class="lede">Where new tickets are created, who can claim them, and where transcripts are archived.</p>
<div class="ticket-layout">
<form method="post" data-save>
<input type="hidden" name="csrf" value="{csrf}">
<section class="panel"><h2>Ticket category</h2><p class="muted">New ticket channels are created under this category, with access limited to the opener and staff.</p>{category}</section>
<section class="panel"><h2>Transcript channel</h2><p class="muted">When a ticket is closed, Overlord uploads a timestamped transcript here, then deletes the channel.</p>{transcript}</section>
<section class="panel"><h2>Staff roles</h2><p class="muted">These roles can claim, close, and read every ticket. Members only see their own.</p><div class="chips">{chips}</div><label class="add-role">Add a role {add}</label></section>
<section class="panel"><h2>Max open tickets per user</h2><p class="muted">Prevents channel spam. The ticket panel offers General Support, Report User / Scam, and VIP / Roles.</p><input type="number" min="1" max="5" name="max_open_tickets" value="{max}"></section>
<div class="savebar is-hidden" data-savebar><span>Unsaved changes</span><div><button type="reset" data-discard class="ghost">Discard</button><button type="submit" class="gold">Save changes</button></div></div>
</form>
<aside class="panel preview"><p class="eyebrow">Ticket panel preview</p><div class="preview-card"><div class="preview-title">{mark} <strong>Overlord</strong> <span class="pill">Bot</span></div><h3>Open a support ticket</h3><p class="muted">Choose a category and tell us what's going on. A private channel will be created for you and the staff team.</p><div class="preview-btn">General Support</div><div class="preview-btn">Report User / Scam</div><div class="preview-btn">VIP / Roles</div><p class="fine">Posted in the help channel with /setup_tickets.</p></div><footer class="preview-stats"><span><strong>{open_now}</strong><small>Open now</small></span><span><strong>{closed_week}</strong><small>Closed this week</small></span></footer></aside>
</div>"#,
        csrf = esc(csrf),
        category = select_channels(
            "ticket_category_id",
            settings.ticket_category_id,
            channels,
            ChannelKind::Category
        ),
        transcript = select_channels(
            "transcript_channel_id",
            settings.transcript_channel_id,
            channels,
            ChannelKind::Text
        ),
        add = select_roles("staff_role", None, roles),
        max = settings.max_tickets(),
        mark = mark("mark mark-16"),
    )
}

pub fn voice_body(
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    rooms: i64,
    csrf: &str,
) -> String {
    format!(
        r#"<h1>Temp voice</h1><p class="lede">Join-to-create rooms for this server. {rooms} temporary rooms are open right now.</p>
<form method="post" data-save>
<input type="hidden" name="csrf" value="{csrf}">
<div class="form-grid">
  <section class="panel"><h2>Join-to-create channel</h2><p class="muted">A member who joins this voice channel is moved into their own room.</p>{join}</section>
  <section class="panel"><h2>Voice category</h2><p class="muted">New rooms are created under this category. A category from another server is ignored.</p>{category}</section>
</div>
<div class="savebar is-hidden" data-savebar><span>Unsaved changes</span><div><button type="reset" data-discard class="ghost">Discard</button><button type="submit" class="gold">Save changes</button></div></div>
</form>"#,
        csrf = esc(csrf),
        join = select_channels(
            "join_to_create_channel_id",
            settings.join_to_create_channel_id,
            channels,
            ChannelKind::Voice
        ),
        category = select_channels(
            "voice_category_id",
            settings.voice_category_id,
            channels,
            ChannelKind::Category
        ),
    )
}

pub fn logs_body(
    rows: &[InfractionRow],
    raids: &[(String, String)],
    names: &dyn Fn(u64) -> String,
) -> String {
    let raids_html = if raids.is_empty() {
        "<p class=\"empty\">No raid alerts recorded yet.</p>".into()
    } else {
        raids
            .iter()
            .map(|(when, detail)| {
                format!(
                    "<li>{when} — {detail}</li>",
                    when = time_tag(when),
                    detail = esc(detail)
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };
    format!(
        r#"<h1>Logs</h1><p class="lede">Moderation cases and raid alerts stored for this server. Channel messages themselves stay in Discord.</p>
<section class="panel"><h2>Raid alerts</h2><ul class="plain">{raids_html}</ul></section>
<section class="panel"><h2>Cases</h2>{}</section>"#,
        moderation_table(rows, names)
    )
}

pub fn audit_body(rows: &[AuditRow], days: i64, guild_name: &str) -> String {
    let body = if rows.is_empty() {
        "<p class=\"empty\">No dashboard changes in this window.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid\"><thead><tr><th>When</th><th>Who</th><th>Setting</th><th>Change</th></tr></thead><tbody>");
        for row in rows {
            html.push_str(&format!(
                "<tr><td>{when}</td><td>{who}</td><td>{field}</td><td><span class=\"old\">{old}</span> <span aria-hidden=\"true\">→</span> <span class=\"new\">{new}</span></td></tr>",
                when = time_tag(&row.created_at),
                who = esc(&row.username),
                field = esc(&row.field),
                old = esc(&row.old_value),
                new = esc(&row.new_value),
            ));
        }
        html.push_str("</tbody></table>");
        html
    };
    format!(
        r#"<h1>Audit log</h1><p class="lede">Dashboard changes for this server. Times follow your device. {guild} only — no other servers.</p>
<form class="filters" method="get"><label>Window<select name="days"><option value="1" {d1}>Last 24 hours</option><option value="7" {d7}>Last 7 days</option><option value="14" {d14}>Last 14 days</option><option value="30" {d30}>Last 30 days</option></select></label><button class="ghost" type="submit">Apply</button><span class="muted">{count} changes</span></form>
<section class="panel">{body}</section>"#,
        guild = esc(guild_name),
        d1 = if days == 1 { "selected" } else { "" },
        d7 = if days == 7 { "selected" } else { "" },
        d14 = if days == 14 { "selected" } else { "" },
        d30 = if days == 30 { "selected" } else { "" },
        count = rows.len(),
    )
}

pub fn message_page(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html lang="en">{head}<body class="login-body"><main class="signin"><h2>{title}</h2><p>{body}</p><p><a href="/">Back to sign in</a></p></main></body></html>"#,
        head = document_head(title),
        title = esc(title),
        body = esc(body),
    )
}

pub fn invite_url(client_id: &str, permissions: u64, guild_id: Option<u64>) -> String {
    let mut url = format!(
        "https://discord.com/oauth2/authorize?client_id={client_id}&scope=bot%20applications.commands&permissions={permissions}"
    );
    if let Some(id) = guild_id {
        url.push_str(&format!("&guild_id={id}&disable_guild_select=true"));
    }
    url
}

fn moderation_table(rows: &[InfractionRow], names: &dyn Fn(u64) -> String) -> String {
    if rows.is_empty() {
        return "<p class=\"empty\">No cases yet.</p>".into();
    }
    let mut html = String::from("<table class=\"grid\"><thead><tr><th>When</th><th>Moderator</th><th>Action</th><th>Member</th><th>Reason</th></tr></thead><tbody>");
    for row in rows {
        html.push_str(&action_row(row, names));
    }
    html.push_str("</tbody></table>");
    html
}

fn action_row(row: &InfractionRow, names: &dyn Fn(u64) -> String) -> String {
    format!(
        "<tr><td>{when}</td><td>{mod}</td><td><span class=\"action {kind}\">{action}</span></td><td>{member}</td><td>{reason}</td></tr>",
        when = time_tag(&row.created_at),
        mod = esc(&names(row.moderator_id as u64)),
        kind = esc(&row.action),
        action = esc(&row.action),
        member = esc(&names(row.user_id as u64)),
        reason = esc(&row.reason),
    )
}

fn select_channels(
    name: &str,
    current: Option<u64>,
    channels: &[ChannelChoice],
    kind: ChannelKind,
) -> String {
    let mut html = format!("<select name=\"{name}\"><option value=\"\">Not set</option>");
    for channel in channels.iter().filter(|channel| channel.kind == kind) {
        let selected = current == Some(channel.id);
        html.push_str(&format!(
            "<option value=\"{id}\" {selected}>{label}</option>",
            id = channel.id,
            selected = if selected { "selected" } else { "" },
            label = esc(&format!("#{}", channel.name)),
        ));
    }
    html.push_str("</select>");
    html
}

fn select_roles(name: &str, current: Option<u64>, roles: &[RoleChoice]) -> String {
    let mut html = format!("<select name=\"{name}\"><option value=\"\">Not set</option>");
    for role in roles {
        let selected = current == Some(role.id);
        html.push_str(&format!(
            "<option value=\"{id}\" {selected}>{label}</option>",
            id = role.id,
            selected = if selected { "selected" } else { "" },
            label = esc(&format!("@{}", role.name)),
        ));
    }
    html.push_str("</select>");
    html
}

fn segment(name: &str, current: &str, options: &[(&str, &str)]) -> String {
    options
        .iter()
        .map(|(value, label)| {
            let checked = if *value == current { "checked" } else { "" };
            format!(
                "<label class=\"seg\"><input type=\"radio\" name=\"{name}\" value=\"{value}\" {checked}><span>{label}</span></label>",
                label = esc(label)
            )
        })
        .collect()
}

fn nav_link(active: Nav, guild_id: u64) -> String {
    let items = [
        (Nav::Overview, "Overview", ""),
        (Nav::Moderation, "Moderation", "/moderation"),
        (Nav::Security, "Security & Anti-raid", "/security"),
        (Nav::Verification, "Verification", "/verification"),
        (Nav::Tickets, "Tickets", "/tickets"),
        (Nav::Voice, "Temp Voice", "/voice"),
        (Nav::Logs, "Logs", "/logs"),
        (Nav::Audit, "Audit log", "/audit"),
    ];
    items
        .into_iter()
        .map(|(nav, label, path)| {
            let class = if nav == active {
                " class=\"active\""
            } else {
                ""
            };
            format!("<a href=\"/servers/{guild_id}{path}\"{class}>{label}</a>")
        })
        .collect()
}

fn mobile_nav(active: Nav, guild_id: u64) -> String {
    let items = [
        (Nav::Overview, "Overview", ""),
        (Nav::Security, "Security", "/security"),
        (Nav::Tickets, "Tickets", "/tickets"),
        (Nav::Audit, "More", "/audit"),
    ];
    items
        .into_iter()
        .map(|(nav, label, path)| {
            let class = if nav == active {
                " class=\"active\""
            } else {
                ""
            };
            format!("<a href=\"/servers/{guild_id}{path}\"{class}>{label}</a>")
        })
        .collect()
}

fn section_name(nav: Nav) -> &'static str {
    match nav {
        Nav::Overview => "Overview",
        Nav::Moderation => "Moderation",
        Nav::Security => "Security & Anti-raid",
        Nav::Verification => "Verification",
        Nav::Tickets => "Tickets",
        Nav::Voice => "Temp Voice",
        Nav::Logs => "Logs",
        Nav::Audit => "Audit log",
    }
}

fn time_tag(sqlite: &str) -> String {
    let iso = chrono::NaiveDateTime::parse_from_str(sqlite, "%Y-%m-%d %H:%M:%S")
        .map(|value| value.and_utc().to_rfc3339())
        .unwrap_or_else(|_| sqlite.to_string());
    format!(
        "<time datetime=\"{}\">{}</time>",
        esc(&iso),
        esc(&relative(sqlite))
    )
}

fn relative(sqlite: &str) -> String {
    let Some(parsed) = chrono::NaiveDateTime::parse_from_str(sqlite, "%Y-%m-%d %H:%M:%S").ok()
    else {
        return sqlite.to_string();
    };
    let secs = (chrono::Utc::now() - parsed.and_utc()).num_seconds().max(0);
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{} hr ago", secs / 3600)
    } else if secs < 172_800 {
        "Yesterday".into()
    } else {
        format!("{} days ago", secs / 86_400)
    }
}

struct GuildIconUrls {
    png: String,
    gif: Option<String>,
}

/// Discord CDN icon for a guild. Animated hashes (`a_…`) use a GIF, with the PNG kept as the static frame.
fn guild_icon_urls(guild_id: u64, hash: Option<&str>) -> Option<GuildIconUrls> {
    let hash = hash.map(str::trim).filter(|hash| valid_icon_hash(hash))?;
    let png = format!("https://cdn.discordapp.com/icons/{guild_id}/{hash}.png?size=128");
    let gif = hash
        .starts_with("a_")
        .then(|| format!("https://cdn.discordapp.com/icons/{guild_id}/{hash}.gif?size=128"));
    Some(GuildIconUrls { png, gif })
}

fn valid_icon_hash(hash: &str) -> bool {
    !hash.is_empty()
        && hash.len() <= 64
        && hash
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn guild_avatar(guild_id: u64, name: &str, icon: Option<&str>, class: &str, size: u16) -> String {
    let Some(urls) = guild_icon_urls(guild_id, icon) else {
        return format!(
            "<span class=\"{class} solid\">{}</span>",
            esc(&initials(name))
        );
    };
    let img = format!(
        "<img class=\"{class}\" src=\"{}\" alt=\"{}\" width=\"{size}\" height=\"{size}\" loading=\"lazy\">",
        esc(&urls.png),
        esc(name)
    );
    match urls.gif {
        Some(gif) => format!(
            "<picture class=\"{class}\"><source srcset=\"{}\" type=\"image/gif\">{img}</picture>",
            esc(&gif)
        ),
        None => img,
    }
}

fn initials(name: &str) -> String {
    let mut letters = String::new();
    for word in name.split_whitespace() {
        if let Some(ch) = word.chars().find(|ch| ch.is_alphanumeric()) {
            letters.push(ch);
        }
        if letters.chars().count() == 2 {
            break;
        }
    }
    if letters.is_empty() {
        "•".into()
    } else {
        letters.to_uppercase()
    }
}

fn avatar_html(user_id: u64, avatar: Option<&str>, name: &str) -> String {
    match avatar {
        Some(hash) if hash.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') => format!(
            "<img class=\"avatar\" alt=\"\" src=\"https://cdn.discordapp.com/avatars/{user_id}/{hash}.png?size=64\">"
        ),
        _ => format!("<span class=\"avatar solid\">{}</span>", esc(&initials(name))),
    }
}

fn document_head(title: &str) -> String {
    format!(
        r#"<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{}</title>
<link rel="icon" href="/favicon.ico" sizes="32x32">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/assets/dashboard.css">
</head>"#,
        esc(title)
    )
}

fn mark(class: &str) -> String {
    format!(
        r#"<svg class="{class}" viewBox="0 0 64 64" aria-hidden="true"><path d="M32 6 54 16v16c0 14-9 22-22 26C19 54 10 46 10 32V16Z"/><circle cx="32" cy="30" r="7"/><circle class="pupil" cx="32" cy="30" r="2.5"/></svg>"#
    )
}

fn icon_shield() -> &'static str {
    r#"<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3 20 7v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V7Z"/></svg>"#
}

fn icon_ticket() -> &'static str {
    r#"<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v2a2 2 0 0 0 0 4v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-2a2 2 0 0 0 0-4Z"/></svg>"#
}

fn icon_doc() -> &'static str {
    r#"<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 3h7l5 5v13a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z"/><path d="M14 3v6h6"/></svg>"#
}

fn icon_discord() -> &'static str {
    r#"<svg class="icon icon-solid" viewBox="0 0 24 24" aria-hidden="true"><path d="M19.3 5.2A17 17 0 0 0 15 4l-.4.8a15 15 0 0 1 4 1.6 16 16 0 0 0-13.2 0A15 15 0 0 1 9.4 4.8L9 4a17 17 0 0 0-4.3 1.2C2.2 8.6 1.5 12 1.8 15.3A17 17 0 0 0 7 18l.8-1.1a11 11 0 0 1-1.6-.8l.4-.3c3.2 1.5 6.6 1.5 9.8 0l.4.3a11 11 0 0 1-1.6.8l.8 1.1a17 17 0 0 0 5.2-2.7c.5-3.8-.6-7.1-2.9-10.1ZM9.2 14.2c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.7.8 1.6 1.8-.7 1.8-1.6 1.8Zm5.6 0c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.7.8 1.6 1.8-.7 1.8-1.6 1.8Z"/></svg>"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_names_are_escaped() {
        let guilds = vec![UserGuild {
            id: 1,
            name: "<img src=x onerror=alert(1)>".into(),
            icon: None,
            owner: true,
            permissions: 0,
        }];
        let html = servers_page(&guilds, &[1], &[], "mod", None, 9, None);
        assert!(html.contains("href=\"/install\""));
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img"));
        assert!(!html.contains("Continue with Discord"));
        assert!(html.contains("Select a server"));
        let missing = servers_page(&guilds, &[], &[], "mod", None, 9, None);
        assert!(missing.contains("/install?guild_id=1"));
    }

    #[test]
    fn install_url_sets_bot_scopes_and_optional_guild() {
        let open = invite_url("123", 99, None);
        assert!(open.starts_with("https://discord.com/oauth2/authorize?"));
        assert!(open.contains("client_id=123"));
        assert!(open.contains("scope=bot%20applications.commands"));
        assert!(open.contains("permissions=99"));
        assert!(!open.contains("guild_id="));
        let targeted = invite_url("123", 99, Some(55));
        assert!(targeted.contains("guild_id=55"));
        assert!(targeted.contains("disable_guild_select=true"));
    }

    #[test]
    fn guild_icon_urls_cover_static_animated_and_missing() {
        let static_icon = guild_icon_urls(42, Some("abcDEF123")).unwrap();
        assert_eq!(
            static_icon.png,
            "https://cdn.discordapp.com/icons/42/abcDEF123.png?size=128"
        );
        assert!(static_icon.gif.is_none());

        let animated = guild_icon_urls(7, Some("a_deadbeef")).unwrap();
        assert_eq!(
            animated.gif.as_deref(),
            Some("https://cdn.discordapp.com/icons/7/a_deadbeef.gif?size=128")
        );
        assert_eq!(
            animated.png,
            "https://cdn.discordapp.com/icons/7/a_deadbeef.png?size=128"
        );

        assert!(guild_icon_urls(1, None).is_none());
        assert!(guild_icon_urls(1, Some("")).is_none());
        assert!(guild_icon_urls(1, Some("a_/../x")).is_none());
        assert!(guild_icon_urls(1, Some("not a hash")).is_none());
    }

    #[test]
    fn guild_icons_render_on_the_list_header_and_switcher() {
        let guilds = vec![
            UserGuild {
                id: 10,
                name: "Pixel Forge".into(),
                icon: Some("abc123".into()),
                owner: true,
                permissions: 0,
            },
            UserGuild {
                id: 11,
                name: "Night Market".into(),
                icon: Some("a_cafe".into()),
                owner: false,
                permissions: 0,
            },
            UserGuild {
                id: 12,
                name: "No Icon".into(),
                icon: None,
                owner: false,
                permissions: 0,
            },
        ];
        let list = servers_page(&guilds, &[10, 11, 12], &[], "mod", None, 9, None);
        assert!(list.contains(
            "src=\"https://cdn.discordapp.com/icons/10/abc123.png?size=128\""
        ));
        assert!(list.contains("alt=\"Pixel Forge\""));
        assert!(list.contains("width=\"36\""));
        assert!(list.contains("height=\"36\""));
        assert!(list.contains("loading=\"lazy\""));
        assert!(list.contains(
            "srcset=\"https://cdn.discordapp.com/icons/11/a_cafe.gif?size=128\""
        ));
        assert!(list.contains(
            "src=\"https://cdn.discordapp.com/icons/11/a_cafe.png?size=128\""
        ));
        assert!(list.contains(">NI<"));
        assert!(!list.to_ascii_lowercase().contains("style="));

        let page = app_page(Shell {
            guild_id: 10,
            guild_name: "Pixel Forge",
            guilds: &guilds,
            username: "mod",
            avatar: None,
            user_id: 9,
            nav: Nav::Overview,
            configured: true,
            flash: None,
            csrf: "token",
            body: String::new(),
        });
        assert!(page.contains("class=\"crumbs\""));
        assert!(page.contains("avatar avatar-crumb"));
        assert!(page.contains("width=\"16\""));
        assert!(page.contains(
            "src=\"https://cdn.discordapp.com/icons/10/abc123.png?size=128\""
        ));
        assert!(page.contains("alt=\"Pixel Forge\""));
        assert!(page.contains(
            "srcset=\"https://cdn.discordapp.com/icons/11/a_cafe.gif?size=128\""
        ));
        assert!(page.contains(">NI<"));
        assert!(!page.to_ascii_lowercase().contains("style="));
        assert!(!page.to_ascii_lowercase().contains("<style"));
    }

    #[test]
    fn login_page_offers_discord() {
        let html = login_page(Some("Sign-in was cancelled."));
        assert!(html.contains("Continue with Discord"));
        assert!(html.contains("href=\"/auth/discord\""));
        assert!(!html.contains("action=\"/auth/discord\""));
        assert!(!html.contains("<form"));
        assert!(html.contains("Sign-in was cancelled."));
    }
}
