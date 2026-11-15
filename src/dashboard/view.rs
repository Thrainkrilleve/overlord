use crate::audit::AuditRow;
use crate::guild_settings::GuildSettings;

use super::api::{ChannelChoice, ChannelKind, RoleChoice, UserGuild};
use super::guard::esc;
use super::store::{InfractionRow, WeekStats};

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
        .map(|text| format!("<p class=\"flash\">{text}</p>", text = esc(text)))
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">{head}
<body>
<div class="login">
  <section class="login-brand">
    <div class="gridbg"></div>
    <div class="watermark">{mark_lg}</div>
    <header class="logo lg">{mark_lg_sm}<span>Overlord</span></header>
    <div class="hero">
      <h1>Watch every gate.<br>Keep every server.</h1>
      <p class="tag">A professional dashboard for Overlord — moderation, anti-raid, verification, tickets, and temp voice, with an audit trail of every change.</p>
    </div>
    <div class="features">
      <div class="feat"><div class="ico">{icon_shield}</div><div><h3>Security &amp; anti-raid</h3><p>Account-age filters, join-surge detection, and a honeypot trap that stops scrapers cold.</p></div></div>
      <div class="feat"><div class="ico">{icon_ticket}</div><div><h3>Tickets, verification, temp voice</h3><p>Onboarding gates, staff-scoped support tickets, and join-to-create rooms — configured per server.</p></div></div>
      <div class="feat"><div class="ico">{icon_doc}</div><div><h3>A full audit trail</h3><p>Every dashboard change is logged with who, when, and the old and new values.</p></div></div>
    </div>
  </section>
  <section class="login-panel">
    <div class="login-card">
      <h2>Sign in</h2>
      <p class="lede">Use your Discord account. You'll only see servers where you have the Manage Server permission.</p>
      {flash}
      <a class="btn discord" href="/auth/discord">{icon_discord} Continue with Discord</a>
      <div class="divider">Secure sign-in</div>
      <div class="owner-note">{icon_info}<span>Overlord never posts one server's logs, tickets, or raid alerts into another. Each community keeps its own settings.</span></div>
      <p class="login-fine">By continuing, you authorize Overlord to read your server list and apply the settings you save.</p>
    </div>
    <div class="login-footer"><span>Overlord dashboard</span><span>Sessions stay on this server</span></div>
  </section>
</div>
</body></html>"#,
        head = document_head("Sign in · Overlord"),
        mark_lg = mark("mark"),
        mark_lg_sm = mark("mark lg"),
        icon_shield = icon_shield(),
        icon_ticket = icon_ticket(),
        icon_doc = icon_doc(),
        icon_discord = icon_discord(),
        icon_info = icon_info(),
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
    let mut needs = 0usize;
    for guild in guilds {
        let present = bot_guilds.contains(&guild.id);
        let ready = present && configured.contains(&guild.id);
        if !ready {
            needs += 1;
        }
        let href = if present {
            format!("/servers/{}", guild.id)
        } else {
            format!("/install?guild_id={}", guild.id)
        };
        let (badge, action) = if !present {
            (
                "<span class=\"badge warn\"><span class=\"dot\"></span>Needs setup</span>",
                "Add the bot",
            )
        } else if ready {
            (
                "<span class=\"badge ok\"><span class=\"dot\"></span>Configured</span>",
                "Open dashboard",
            )
        } else {
            (
                "<span class=\"badge warn\"><span class=\"dot\"></span>Needs setup</span>",
                "Finish setup",
            )
        };
        cards.push_str(&format!(
            r#"<a class="card scard" data-server="{key}" href="{href}"><div class="top">{icon}{badge}</div><h3>{name}</h3><div class="foot"><span class="muted card-foot">{action}</span><span class="muted">{chev}</span></div></a>"#,
            key = esc(&guild.name.to_lowercase()),
            href = esc(&href),
            icon = guild_face(guild.id, &guild.name, guild.icon.as_deref(), "sicon lg"),
            name = esc(&guild.name),
            chev = chev(),
        ));
    }
    let flash = flash
        .map(|text| format!("<p class=\"flash\">{text}</p>", text = esc(text)))
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">{head}
<body>
<div class="servers-page">
  <header class="topbar">
    <a class="logo sm" href="/servers">{mark}<span>Overlord</span></a>
    <div class="sidebar-user bare">{avatar}<div class="meta"><div class="name">{user}</div><div class="sub">Signed in with Discord</div></div></div>
  </header>
  <div class="servers-inner">
    <div class="page-head">
      <div>
        {crumbs}
        <h1>Select a server</h1>
        <p class="lede">Only servers where you have Manage Server are listed. Invite Overlord to any others you own or moderate.</p>
      </div>
      <div class="head-actions">
        <label class="search">{icon_search}<span class="sr">Search servers</span><input data-server-search type="search" placeholder="Search servers"></label>
      </div>
    </div>
    {flash}
    <div class="server-grid">{cards}<a class="card scard add" href="/install"><div class="plus">{icon_plus}</div><h3>Add Overlord to a server</h3><p>Opens the Discord authorization screen. You'll need Manage Server there too.</p></a></div>
    <div class="servers-foot"><span>Showing {count} servers where you have Manage Server</span><span>{needs} need setup</span></div>
  </div>
</div>
<script src="/assets/dashboard.js"></script>
</body></html>"#,
        head = document_head("Servers · Overlord"),
        mark = mark("mark sm"),
        avatar = person_face(user_id, avatar, username, "avatar sm"),
        user = esc(username),
        crumbs = crumbs(None, "Servers"),
        icon_search = icon_search(),
        icon_plus = icon_plus(),
        count = guilds.len(),
    )
}

pub fn app_page(shell: Shell<'_>) -> String {
    let current = shell
        .guilds
        .iter()
        .find(|guild| guild.id == shell.guild_id);
    let icon = current.and_then(|guild| guild.icon.as_deref());
    let face = guild_face(shell.guild_id, shell.guild_name, icon, "sicon md");
    let mut menu = String::from("<a href=\"/servers\">All servers</a>");
    for guild in shell
        .guilds
        .iter()
        .filter(|guild| guild.id != shell.guild_id)
        .take(12)
    {
        menu.push_str(&format!(
            "<a href=\"/servers/{id}\">{face}<span>{name}</span></a>",
            id = guild.id,
            face = guild_face(guild.id, &guild.name, guild.icon.as_deref(), "sicon sm"),
            name = esc(&guild.name),
        ));
    }
    let flash = shell
        .flash
        .map(|text| format!("<p class=\"flash\">{text}</p>", text = esc(text)))
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">{head}
<body>
<div class="app">
  <aside class="sidebar">
    <a class="brand" href="/servers">{mark}<span class="word">Overlord</span></a>
    <details class="server-switch">
      <summary>{face}<div class="meta"><div class="name">{guild}</div><div class="sub">{status}</div></div>{chev}</summary>
      <div class="server-menu">{menu}</div>
    </details>
    <div class="nav"><div class="nav-label">Manage</div>{nav}</div>
    <div class="sidebar-user">{user_face}<div class="meta"><div class="name">{user}</div><div class="sub">Server manager</div></div>
      <form method="post" action="/logout"><input type="hidden" name="csrf" value="{csrf}"><button class="icon-btn" type="submit" title="Sign out">{icon_out}</button></form>
    </div>
  </aside>
  <header class="m-top">
    <a class="icon-btn" href="/servers" aria-label="All servers">{icon_menu}</a>
    {face_sm}
    <div class="grow"><div class="sname">{guild}</div></div>
    <span class="badge ok"><span class="dot"></span>Online</span>
    {user_face}
  </header>
  <main class="main">{flash}{body}</main>
</div>
<nav class="m-tabbar">{mobile}</nav>
<script src="/assets/dashboard.js"></script>
</body></html>"#,
        head = document_head(&format!(
            "{} · {} · Overlord",
            section_name(shell.nav),
            shell.guild_name
        )),
        mark = mark("mark md"),
        status = if shell.configured {
            "Configured"
        } else {
            "Needs setup"
        },
        guild = esc(shell.guild_name),
        chev = chev(),
        nav = nav_links(shell.nav, shell.guild_id),
        user_face = person_face(shell.user_id, shell.avatar, shell.username, "avatar sm"),
        user = esc(shell.username),
        csrf = esc(shell.csrf),
        icon_out = icon_logout(),
        icon_menu = icon_menu(),
        face_sm = guild_face(shell.guild_id, shell.guild_name, icon, "sicon sm"),
        mobile = mobile_nav(shell.nav, shell.guild_id),
        body = shell.body,
    )
}

pub fn overview_body(
    guild_id: u64,
    guild_name: &str,
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
    let delta_class = if delta >= 0 { "delta up" } else { "delta" };
    let waiting = (stats.open_tickets - stats.claimed_tickets).max(0);
    let last_raid = stats
        .last_raid_at
        .as_deref()
        .map(|when| format!("Last alert {}", time_text(when)))
        .unwrap_or_else(|| "No alerts yet".into());
    let table = if rows.is_empty() {
        "<p class=\"empty-note\">No moderation actions recorded in this server yet.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid desk-only\"><thead><tr><th>When</th><th>Moderator</th><th>Action</th><th>Member</th><th>Reason</th></tr></thead><tbody>");
        for row in rows {
            html.push_str(&action_row(row, names));
        }
        html.push_str("</tbody></table><div class=\"mob-only\">");
        for row in rows {
            html.push_str(&mobile_action(row, names));
        }
        html.push_str("</div>");
        html
    };
    let raid_line = format!(
        "{} if {} joins land within {} seconds.",
        super::validate::action_label(settings.raid_action_kind()),
        settings.raid_threshold(),
        settings.raid_window_secs()
    );
    let honeypot = if settings.honeypot_active() {
        ("ok", "Honeypot armed", "Trap channel is live. Staff test posts are logged, never banned.")
    } else {
        ("watch", "Honeypot off", "Turn it on under Security &amp; Anti-raid when the trap channel exists.")
    };
    let verification = if settings.member_role_id.is_some() {
        ("ok", "Verification active", "Code gate in the welcome channel. Codes expire after 5 minutes.")
    } else {
        ("watch", "Verification needs a member role", "Set it on the Verification page. Until then, a correct code does not grant access.")
    };
    format!(
        r#"<div class="main-inner">
  <div class="page-head">
    <div>
      {crumbs}
      <h1>Overview</h1>
      <p class="lede">Activity and health for this server. Times follow your device.</p>
    </div>
    <div class="head-actions"><a class="btn ghost" href="/servers/{guild_id}/layout">Set up recommended layout</a><span class="badge ok"><span class="dot"></span>Bot online</span></div>
  </div>
  <div class="stats">
    <div class="card stat"><div class="label">Infractions this week {icon_shield}</div><div class="value">{infra}</div><div class="{delta_class}">{delta_label}</div></div>
    <div class="card stat"><div class="label">Open tickets {icon_ticket}</div><div class="value">{open}</div><div class="delta">{claimed} claimed · {waiting} waiting</div></div>
    <div class="card stat"><div class="label">Raid alerts {icon_alert}</div><div class="value">{raids}</div><div class="delta good">{last_raid}</div></div>
    <div class="card stat"><div class="label">Temp voice rooms {icon_mic}</div><div class="value">{rooms}</div><div class="delta">Open right now</div></div>
  </div>
  <div class="row-2">
    <section class="card stretch"><div class="card-head"><h2>Recent mod actions</h2><span class="muted">This week</span></div><div class="card-body flush">{table}</div></section>
    <section class="card"><div class="card-head"><h2>System status</h2></div><div class="status-list">
      <div class="status-row"><div class="ico {hp_class}">{icon_shield}</div><div><h3>{hp_title}</h3><p>{hp_body}</p></div></div>
      <div class="status-row"><div class="ico {ver_class}">{icon_shield}</div><div><h3>{ver_title}</h3><p>{ver_body}</p></div></div>
      <div class="status-row"><div class="ico watch">{icon_eye}</div><div><h3>Anti-raid monitoring</h3><p>{raid_line}</p></div></div>
      <div class="status-row"><div class="ico info">{icon_mic}</div><div><h3>Temp voice</h3><p>{rooms} rooms open right now from the join-to-create channel.</p></div></div>
    </div></section>
  </div>
</div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Overview"),
        icon_shield = icon_shield(),
        icon_ticket = icon_ticket(),
        icon_alert = icon_alert(),
        icon_mic = icon_mic(),
        icon_eye = icon_eye(),
        infra = stats.infractions_this_week,
        open = stats.open_tickets,
        claimed = stats.claimed_tickets,
        raids = stats.raid_alerts_this_week,
        rooms = stats.temp_rooms,
        hp_class = honeypot.0,
        hp_title = honeypot.1,
        hp_body = honeypot.2,
        ver_class = verification.0,
        ver_title = verification.1,
        ver_body = verification.2,
    )
}

pub fn moderation_body(
    guild_id: u64,
    guild_name: &str,
    rows: &[InfractionRow],
    names: &dyn Fn(u64) -> String,
) -> String {
    let table = if rows.is_empty() {
        "<p class=\"empty-note\">No cases yet. Moderators still use /warn, /timeout, /kick, and /ban in Discord. Cases show up here.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid\"><thead><tr><th>Case</th><th>When</th><th>Moderator</th><th>Action</th><th>Member</th><th>Reason</th></tr></thead><tbody>");
        for row in rows {
            html.push_str(&format!(
                "<tr><td>#{case}</td>{rest}",
                case = row.case_number,
                rest = action_cells(row, names),
            ));
        }
        html.push_str("</tbody></table>");
        html
    };
    format!(
        r#"<div class="main-inner"><div class="page-head"><div>{crumbs}<h1>Moderation</h1><p class="lede">Case history for this server. Actions are still taken in Discord so a browser session cannot ban someone by itself.</p></div></div><section class="card stretch"><div class="card-body flush">{table}</div></section></div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Moderation"),
    )
}

pub fn security_body(
    guild_id: u64,
    guild_name: &str,
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    csrf: &str,
) -> String {
    let age = settings.min_account_age_days.clamp(0, 3650);
    let age_max = age.max(14);
    format!(
        r#"<div class="main-form">
<div class="main-inner">
  <div class="page-head"><div>{crumbs}<h1>Security &amp; Anti-raid</h1><p class="lede">Filters, join-surge detection, and the honeypot trap for this server only.</p></div><div class="head-actions"><span class="badge ok"><span class="dot"></span>Monitoring</span></div></div>
  <div class="settings-grid">
    <div class="card field"><div class="field-label"><h3>Minimum account age</h3><span class="value-chip" data-age-out>{age} days</span></div><p class="field-help">Newer accounts, and those still using a default avatar, raise a staff alert in the mod log.</p><input form="settings" data-age type="range" min="0" max="{age_max}" name="min_account_age_days" value="{age}" aria-label="Minimum account age in days"></div>
    <div class="card field"><div class="field-label"><h3>Raid threshold</h3><span class="value-chip">{threshold} / {window}s</span></div><p class="field-help">A sliding window of joins. During a sustained raid, staff are alerted at most once a minute.</p><div class="input-row"><input form="settings" class="num" type="number" min="2" max="50" name="raid_join_threshold" value="{threshold}" aria-label="Joins"><span class="muted">joins per</span><input form="settings" class="num" type="number" min="5" max="120" name="raid_window_seconds" value="{window}" aria-label="Seconds"><span class="muted">seconds</span></div></div>
    <div class="card field"><div class="field-label"><h3>Action on raid</h3></div><p class="field-help">What Overlord should do when the join threshold is crossed.</p><div class="seg">{segs}</div></div>
    <div class="card field"><div class="field-label"><h3>Unverified role</h3></div><p class="field-help">Granted on join and removed when a member passes the verification gate.</p>{unverified}</div>
    <div class="card field"><div class="field-label"><h3>Mod log channel</h3></div><p class="field-help">Raid alerts, honeypot hits, and account-age warnings are posted here.</p>{modlog}</div>
    <div class="card field"><div class="toggle-row"><div><div class="field-label tight"><h3>Honeypot trap</h3></div><p class="field-help flush">Anyone who posts in the trap channel is banned and purged. Staff test posts are deleted and logged, never banned.</p></div><label class="toggle">{honeypot_input}<span class="knob"></span></label></div><div class="select-gap">{honeypot}</div></div>
  </div>
</div>
{savebar}
</div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Security & Anti-raid"),
        threshold = settings.raid_threshold(),
        window = settings.raid_window_secs(),
        segs = segment(
            "raid_action",
            settings.raid_action_kind(),
            &[
                ("alert", "Alert staff only"),
                ("lockdown", "Lockdown and alert"),
                ("ban", "Ban new joins"),
            ],
        ),
        unverified = select_roles(
            "unverified_role_id",
            settings.unverified_role_id,
            roles,
            guild_id,
            csrf,
        ),
        modlog = select_channels(
            "mod_log_channel_id",
            settings.mod_log_channel_id,
            channels,
            ChannelKind::Text,
            guild_id,
            csrf,
        ),
        honeypot_input = if settings.honeypot_enabled {
            "<input form=\"settings\" type=\"checkbox\" name=\"honeypot_enabled\" value=\"1\" checked aria-label=\"Honeypot trap\">"
        } else {
            "<input form=\"settings\" type=\"checkbox\" name=\"honeypot_enabled\" value=\"1\" aria-label=\"Honeypot trap\">"
        },
        honeypot = select_channels(
            "honeypot_channel_id",
            settings.honeypot_channel_id,
            channels,
            ChannelKind::Text,
            guild_id,
            csrf,
        ),
        savebar = savebar(csrf),
    )
}

pub fn verification_body(
    guild_id: u64,
    guild_name: &str,
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    csrf: &str,
) -> String {
    format!(
        r#"<div class="main-form">
<div class="main-inner">
  <div class="page-head"><div>{crumbs}<h1>Verification</h1><p class="lede">The code gate for this server. Post the button with /setup_verification in the channel members can see before they verify.</p></div></div>
  <div class="settings-grid">
    <div class="card field"><div class="field-label"><h3>Verify channel</h3></div><p class="field-help">Shown in /security_status. The button can live here.</p>{verify}</div>
    <div class="card field"><div class="field-label"><h3>Member role</h3></div><p class="field-help">Granted when the code matches. Without it, a correct code tells the member to contact staff.</p>{member}</div>
    <div class="card field"><div class="field-label"><h3>Unverified role</h3></div><p class="field-help">Given on join and removed after a successful check.</p>{unverified}</div>
    <div class="card field"><div class="field-label"><h3>How the gate works</h3></div><p class="field-help flush">Each person gets a one-time code in this server, valid for 5 minutes. A code from another server does not work here.</p></div>
  </div>
</div>
{savebar}
</div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Verification"),
        verify = select_channels(
            "verify_channel_id",
            settings.verify_channel_id,
            channels,
            ChannelKind::Text,
            guild_id,
            csrf,
        ),
        member = select_roles(
            "member_role_id",
            settings.member_role_id,
            roles,
            guild_id,
            csrf,
        ),
        unverified = select_roles(
            "unverified_role_id",
            settings.unverified_role_id,
            roles,
            guild_id,
            csrf,
        ),
        savebar = savebar(csrf),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn tickets_body(
    guild_id: u64,
    guild_name: &str,
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    open_now: i64,
    closed_week: i64,
    csrf: &str,
) -> String {
    let selected = settings.staff_role_list();
    let chips = if selected.is_empty() {
        "<p class=\"empty-note\">No staff roles yet.</p>".into()
    } else {
        selected
            .iter()
            .map(|id| {
                let name = roles
                    .iter()
                    .find(|role| role.id == *id)
                    .map(|role| role.name.as_str())
                    .unwrap_or("role");
                format!(
                    "<span class=\"chip\"><span class=\"at\">@</span> {name}<input form=\"settings\" type=\"hidden\" name=\"staff_role\" value=\"{id}\"></span>",
                    name = esc(name),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };
    format!(
        r#"<div class="main-form">
<div class="main-inner">
  <div class="page-head"><div>{crumbs}<h1>Tickets</h1><p class="lede">Where new tickets are created, who can claim them, and where transcripts are archived.</p></div><div class="head-actions"><span class="muted">Post the panel in Discord with <span class="hash inherit">/setup_tickets</span></span></div></div>
  <div class="row-2 tickets-layout">
    <div class="tickets-col">
      <div class="card field"><div class="field-label"><h3>Ticket category</h3></div><p class="field-help">New ticket channels are created under this category, with access limited to the opener and staff.</p>{category}</div>
      <div class="card field"><div class="field-label"><h3>Transcript channel</h3></div><p class="field-help">When a ticket is closed, Overlord uploads a timestamped transcript here, then deletes the channel.</p>{transcript}</div>
      <div class="card field"><div class="field-label"><h3>Staff roles</h3></div><p class="field-help">These roles can claim, close, and read every ticket. Members only see their own.</p><div class="chips">{chips}</div><label class="chip add">Add a role {add}</label></div>
      <div class="card field"><div class="field-label"><h3>Max open tickets per user</h3></div><p class="field-help">Prevents channel spam. The ticket panel offers General Support, Report User / Scam, and VIP / Roles.</p><input form="settings" class="num" type="number" min="1" max="5" name="max_open_tickets" value="{max}" aria-label="Max open tickets per user"></div>
    </div>
    <aside class="card preview-card"><div class="card-head"><h2>Ticket panel preview</h2></div><div class="card-body"><div class="preview-title">{mark} <strong>Overlord</strong> <span class="badge quiet">Bot</span></div><h3>Open a support ticket</h3><p class="muted">Choose a category and tell us what's going on. A private channel will be created for you and the staff team.</p><div class="tbtn">General Support</div><div class="tbtn">Report User / Scam</div><div class="tbtn">VIP / Roles</div><p class="muted preview-note">Posted in the help channel with <span class="hash inherit">/setup_tickets</span>.</p><div class="preview-stats"><div><strong>{open_now}</strong><div class="muted">Open now</div></div><div><strong>{closed_week}</strong><div class="muted">Closed this week</div></div></div></div></aside>
  </div>
</div>
{savebar}
</div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Tickets"),
        category = select_channels(
            "ticket_category_id",
            settings.ticket_category_id,
            channels,
            ChannelKind::Category,
            guild_id,
            csrf,
        ),
        transcript = select_channels(
            "transcript_channel_id",
            settings.transcript_channel_id,
            channels,
            ChannelKind::Text,
            guild_id,
            csrf,
        ),
        add = select_roles("staff_role", None, roles, guild_id, csrf),
        max = settings.max_tickets(),
        mark = mark("mark sm"),
        savebar = savebar(csrf),
    )
}

pub fn voice_body(
    guild_id: u64,
    guild_name: &str,
    settings: &GuildSettings,
    channels: &[ChannelChoice],
    rooms: i64,
    csrf: &str,
) -> String {
    format!(
        r#"<div class="main-form">
<div class="main-inner">
  <div class="page-head"><div>{crumbs}<h1>Temp voice</h1><p class="lede">Join-to-create rooms for this server. {rooms} temporary rooms are open right now.</p></div></div>
  <div class="settings-grid">
    <div class="card field"><div class="field-label"><h3>Join-to-create channel</h3></div><p class="field-help">A member who joins this voice channel is moved into their own room.</p>{join}</div>
    <div class="card field"><div class="field-label"><h3>Voice category</h3></div><p class="field-help">New rooms are created under this category. A category from another server is ignored.</p>{category}</div>
  </div>
</div>
{savebar}
</div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Temp Voice"),
        join = select_channels(
            "join_to_create_channel_id",
            settings.join_to_create_channel_id,
            channels,
            ChannelKind::Voice,
            guild_id,
            csrf,
        ),
        category = select_channels(
            "voice_category_id",
            settings.voice_category_id,
            channels,
            ChannelKind::Category,
            guild_id,
            csrf,
        ),
        savebar = savebar(csrf),
    )
}

pub fn logs_body(
    guild_id: u64,
    guild_name: &str,
    rows: &[InfractionRow],
    raids: &[(String, String)],
    names: &dyn Fn(u64) -> String,
) -> String {
    let raids_html = if raids.is_empty() {
        "<p class=\"empty-note\">No raid alerts recorded yet.</p>".into()
    } else {
        let mut html = String::from("<ul class=\"status-list\">");
        for (when, detail) in raids {
            html.push_str(&format!(
                "<li class=\"status-row\"><div><h3>{when}</h3><p>{detail}</p></div></li>",
                when = time_tag(when),
                detail = esc(detail),
            ));
        }
        html.push_str("</ul>");
        html
    };
    let cases = if rows.is_empty() {
        "<p class=\"empty-note\">No cases yet.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid\"><thead><tr><th>When</th><th>Moderator</th><th>Action</th><th>Member</th><th>Reason</th></tr></thead><tbody>");
        for row in rows {
            html.push_str(&action_row(row, names));
        }
        html.push_str("</tbody></table>");
        html
    };
    format!(
        r#"<div class="main-inner"><div class="page-head"><div>{crumbs}<h1>Logs</h1><p class="lede">Moderation cases and raid alerts stored for this server. Channel messages themselves stay in Discord.</p></div></div><div class="row-2"><section class="card"><div class="card-head"><h2>Raid alerts</h2></div>{raids_html}</section><section class="card"><div class="card-head"><h2>Cases</h2></div><div class="card-body flush">{cases}</div></section></div></div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Logs"),
    )
}

pub fn audit_body(guild_id: u64, rows: &[AuditRow], days: i64, guild_name: &str) -> String {
    let body = if rows.is_empty() {
        "<p class=\"empty-note\">No dashboard changes in this window.</p>".into()
    } else {
        let mut html = String::from("<table class=\"grid\"><thead><tr><th>When</th><th>Who</th><th>Setting</th><th>Change</th></tr></thead><tbody>");
        for row in rows {
            let face = format!(
                "<span class=\"avatar sm {tint}\">{letters}</span>",
                tint = tint_class(row.user_id as u64),
                letters = esc(&initials(&row.username)),
            );
            html.push_str(&format!(
                "<tr><td class=\"muted nowrap\">{when}</td><td><div class=\"who\">{face}{who}</div></td><td>{field}</td><td><span class=\"delta-old\">{old}</span> <span aria-hidden=\"true\">→</span> <span class=\"delta-new\">{new}</span></td></tr>",
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
        r#"<div class="main-inner">
  <div class="page-head"><div>{crumbs}<h1>Audit log</h1><p class="lede">Dashboard changes for this server. Times follow your device. {guild} only — no other servers.</p></div>
  <form class="head-actions filters" method="get"><label class="sr" for="days">Window</label><select class="field-select auto" id="days" name="days"><option value="1" {d1}>Last 24 hours</option><option value="7" {d7}>Last 7 days</option><option value="14" {d14}>Last 14 days</option><option value="30" {d30}>Last 30 days</option></select><button class="btn ghost" type="submit">Apply</button></form></div>
  <section class="card stretch"><div class="card-head"><h2>{count} changes</h2><span class="muted">{guild} only · no other servers</span></div><div class="card-body flush">{body}</div></section>
</div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Audit log"),
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
        r#"<!DOCTYPE html><html lang="en">{head}<body><div class="login"><section class="login-panel"><div class="login-card"><h2>{title}</h2><p class="lede">{body}</p><p><a class="btn ghost" href="/">Back</a></p></div></section></div></body></html>"#,
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

fn savebar(csrf: &str) -> String {
    format!(
        r#"<form method="post" data-save id="settings"><input type="hidden" name="csrf" value="{csrf}"><div class="savebar is-hidden" data-savebar><span class="pulse"></span><div class="msg">Unsaved changes</div><button type="reset" data-discard class="btn ghost">Discard</button><button type="submit" class="btn primary">Save changes</button></div></form>"#,
        csrf = esc(csrf),
    )
}

fn crumbs(server: Option<(u64, &str)>, current: &str) -> String {
    let mut items = String::new();
    if let Some((guild_id, guild_name)) = server {
        items.push_str("<li><a href=\"/servers\">Servers</a></li>");
        items.push_str(&format!(
            "<li><a href=\"/servers/{guild_id}\">{}</a></li>",
            esc(guild_name)
        ));
    }
    items.push_str(&format!(
        "<li aria-current=\"page\">{}</li>",
        esc(current)
    ));
    format!(r#"<nav class="crumbs" aria-label="Breadcrumb"><ol>{items}</ol></nav>"#)
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
<link rel="stylesheet" href="/assets/app.css">
</head>"#,
        esc(title)
    )
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

fn nav_links(active: Nav, guild_id: u64) -> String {
    let items = [
        (Nav::Overview, "Overview", "", icon_grid()),
        (Nav::Moderation, "Moderation", "/moderation", icon_shield()),
        (Nav::Security, "Security & Anti-raid", "/security", icon_shield()),
        (Nav::Verification, "Verification", "/verification", icon_shield()),
        (Nav::Tickets, "Tickets", "/tickets", icon_ticket()),
        (Nav::Voice, "Temp Voice", "/voice", icon_mic()),
        (Nav::Logs, "Logs", "/logs", icon_doc()),
        (Nav::Audit, "Audit log", "/audit", icon_doc()),
    ];
    items
        .into_iter()
        .map(|(nav, label, slug, icon)| {
            let class = if nav == active {
                "nav-item active"
            } else {
                "nav-item"
            };
            format!(
                "<a class=\"{class}\" href=\"/servers/{guild_id}{slug}\">{icon}{label}</a>"
            )
        })
        .collect()
}

fn mobile_nav(active: Nav, guild_id: u64) -> String {
    let items = [
        (Nav::Overview, "Overview", "", icon_grid()),
        (Nav::Security, "Security", "/security", icon_shield()),
        (Nav::Tickets, "Tickets", "/tickets", icon_ticket()),
        (Nav::Audit, "More", "/audit", icon_more()),
    ];
    items
        .into_iter()
        .map(|(nav, label, slug, icon)| {
            let class = if nav == active { "m-tab on" } else { "m-tab" };
            format!("<a class=\"{class}\" href=\"/servers/{guild_id}{slug}\">{icon}{label}</a>")
        })
        .collect()
}

struct GuildIconUrls {
    png: String,
    gif: Option<String>,
}

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

fn guild_face(guild_id: u64, name: &str, icon: Option<&str>, class: &str) -> String {
    let size: u16 = if class.contains("lg") {
        44
    } else if class.contains("sm") {
        28
    } else {
        36
    };
    let Some(urls) = guild_icon_urls(guild_id, icon) else {
        return format!(
            "<span class=\"{class} {tint}\">{letters}</span>",
            tint = tint_class(guild_id),
            letters = esc(&initials(name)),
        );
    };
    let img = format!(
        "<img class=\"{class}\" src=\"{}\" alt=\"{}\" width=\"{size}\" height=\"{size}\" loading=\"lazy\">",
        esc(&urls.png),
        esc(name),
    );
    match urls.gif {
        Some(gif) => format!(
            "<picture class=\"{class}\"><source srcset=\"{}\" type=\"image/gif\">{img}</picture>",
            esc(&gif)
        ),
        None => img,
    }
}

fn person_face(user_id: u64, avatar: Option<&str>, name: &str, class: &str) -> String {
    match avatar {
        Some(hash) if valid_icon_hash(hash) => format!(
            "<img class=\"{class}\" alt=\"{}\" width=\"28\" height=\"28\" loading=\"lazy\" src=\"https://cdn.discordapp.com/avatars/{user_id}/{hash}.png?size=64\">",
            esc(name),
        ),
        _ => format!(
            "<span class=\"{class} g-user\">{}</span>",
            esc(&initials(name))
        ),
    }
}

fn tint_class(id: u64) -> &'static str {
    const TINTS: [&str; 12] = [
        "g-forge", "g-owls", "g-harbor", "g-lantern", "g-drift", "g-user", "g-maple", "g-coast",
        "g-finch", "g-drop", "g-raid", "g-scam",
    ];
    TINTS[(id % TINTS.len() as u64) as usize]
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

fn time_tag(raw: &str) -> String {
    format!(
        "<time datetime=\"{}\">{}</time>",
        esc(raw),
        esc(&time_text(raw))
    )
}

fn time_text(raw: &str) -> String {
    let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") else {
        return raw.to_string();
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

fn action_row(row: &InfractionRow, names: &dyn Fn(u64) -> String) -> String {
    format!("<tr><td class=\"muted nowrap\">{when}</td>{rest}</tr>", when = time_tag(&row.created_at), rest = action_cells(row, names))
}

fn action_cells(row: &InfractionRow, names: &dyn Fn(u64) -> String) -> String {
    let moderator = names(row.moderator_id as u64);
    let member = names(row.user_id as u64);
    let mod_face = format!(
        "<span class=\"avatar sm {tint}\">{letters}</span>",
        tint = tint_class(row.moderator_id as u64),
        letters = esc(&initials(&moderator)),
    );
    let member_face = format!(
        "<span class=\"avatar sm {tint}\">{letters}</span>",
        tint = tint_class(row.user_id as u64),
        letters = esc(&initials(&member)),
    );
    format!(
        "<td><div class=\"who\">{mod_face}{mod}</div></td><td><span class=\"action-pill {kind}\">{action}</span></td><td><div class=\"who\">{member_face}{member}</div></td><td>{reason}</td>",
        mod = esc(&moderator),
        kind = action_class(&row.action),
        action = esc(&row.action),
        member = esc(&member),
        reason = esc(&row.reason),
    )
}

fn mobile_action(row: &InfractionRow, names: &dyn Fn(u64) -> String) -> String {
    let moderator = names(row.moderator_id as u64);
    let member = names(row.user_id as u64);
    format!(
        r#"<div class="m-action"><span class="avatar sm {tint}">{letters}</span><div class="copy"><div class="line"><strong>{mod}</strong><span class="action-pill {kind}">{action}</span></div><div class="detail">{member} · {reason}</div><div class="when">{when}</div></div></div>"#,
        tint = tint_class(row.moderator_id as u64),
        letters = esc(&initials(&moderator)),
        mod = esc(&moderator),
        kind = action_class(&row.action),
        action = esc(&row.action),
        member = esc(&member),
        reason = esc(&row.reason),
        when = time_tag(&row.created_at),
    )
}

fn action_class(action: &str) -> &'static str {
    let lower = action.to_ascii_lowercase();
    if lower.contains("ban") {
        "ban"
    } else if lower.contains("kick") {
        "kick"
    } else if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("warn") {
        "warn"
    } else {
        "purge"
    }
}

fn select_channels(
    name: &str,
    current: Option<u64>,
    channels: &[ChannelChoice],
    kind: ChannelKind,
    guild_id: u64,
    csrf: &str,
) -> String {
    let prefix = if kind == ChannelKind::Category { "" } else { "#" };
    let mut html = format!(
        "<label class=\"sr\" for=\"{name}\">{name}</label><select form=\"settings\" class=\"field-select\" id=\"{name}\" name=\"{name}\"><option value=\"\">Not set</option>"
    );
    for channel in channels.iter().filter(|channel| super::create::channel_fits(channel.kind, kind)) {
        let selected = if current == Some(channel.id) {
            " selected"
        } else {
            ""
        };
        html.push_str(&format!(
            "<option value=\"{id}\"{selected}>{prefix}{label}</option>",
            id = channel.id,
            label = esc(&channel.name),
        ));
    }
    html.push_str("</select>");
    html.push_str(&create_channel_form(name, kind, guild_id, csrf, channels));
    html
}

fn select_roles(
    name: &str,
    current: Option<u64>,
    roles: &[RoleChoice],
    guild_id: u64,
    csrf: &str,
) -> String {
    let mut html = format!(
        "<label class=\"sr\" for=\"{name}\">{name}</label><select form=\"settings\" class=\"field-select\" id=\"{name}\" name=\"{name}\"><option value=\"\">Not set</option>"
    );
    for role in roles {
        let selected = if current == Some(role.id) {
            " selected"
        } else {
            ""
        };
        html.push_str(&format!(
            "<option value=\"{id}\"{selected}>@{label}</option>",
            id = role.id,
            label = esc(&role.name),
        ));
    }
    html.push_str("</select>");
    html.push_str(&create_role_form(name, guild_id, csrf));
    html
}

fn create_channel_form(
    target: &str,
    kind: ChannelKind,
    guild_id: u64,
    csrf: &str,
    channels: &[ChannelChoice],
) -> String {
    let summary = if kind == ChannelKind::Category {
        "Create category"
    } else {
        "Create channel"
    };
    let types: String = match kind {
        ChannelKind::Voice => "<input type=\"hidden\" name=\"kind\" value=\"voice\">".into(),
        ChannelKind::Category => "<input type=\"hidden\" name=\"kind\" value=\"category\">".into(),
        _ => "<label>Type <select name=\"kind\"><option value=\"text\">Text</option><option value=\"forum\">Forum</option></select></label>".into(),
    };
    let parents = if kind == ChannelKind::Category {
        String::new()
    } else {
        let mut html = String::from("<label>Category <select name=\"parent_id\"><option value=\"\">No category</option>");
        for channel in channels.iter().filter(|channel| channel.kind == ChannelKind::Category) {
            html.push_str(&format!(
                "<option value=\"{id}\">{name}</option>",
                id = channel.id,
                name = esc(&channel.name),
            ));
        }
        html.push_str("</select></label><label>Or new category <input type=\"text\" name=\"category_name\" maxlength=\"100\"></label>");
        if kind != ChannelKind::Voice {
            html.push_str("<label class=\"checkline\"><input type=\"checkbox\" name=\"private\" value=\"1\"> Private to staff and Overlord</label>");
        }
        html
    };
    format!(
        r#"<details class="create-box"><summary>{summary}</summary><form class="create-grid" method="post" action="/servers/{guild_id}/resources"><input type="hidden" name="csrf" value="{csrf}"><input type="hidden" name="resource" value="channel"><input type="hidden" name="target" value="{target}"><label>Name <input type="text" name="name" maxlength="100" required></label>{types}{parents}<button class="btn ghost" type="submit">{summary}</button></form></details>"#,
        csrf = esc(csrf),
    )
}

fn create_role_form(target: &str, guild_id: u64, csrf: &str) -> String {
    format!(
        r#"<details class="create-box"><summary>Create role</summary><form class="create-grid" method="post" action="/servers/{guild_id}/resources"><input type="hidden" name="csrf" value="{csrf}"><input type="hidden" name="resource" value="role"><input type="hidden" name="target" value="{target}"><label>Name <input type="text" name="name" maxlength="100" required></label><label>Color <input type="color" name="color" value='#99aab5'></label><label class="checkline"><input type="checkbox" name="hoist" value="1"> Show separately</label><label class="checkline"><input type="checkbox" name="mentionable" value="1"> Allow anyone to mention</label><button class="btn ghost" type="submit">Create role</button></form></details>"#,
        csrf = esc(csrf),
    )
}

pub fn layout_body(
    guild_id: u64,
    guild_name: &str,
    plan: &[super::create::LayoutItem],
    channels: &[ChannelChoice],
    roles: &[RoleChoice],
    csrf: &str,
) -> String {
    let mut rows = String::new();
    for item in plan {
        rows.push_str(&format!(
            "<tr><td>{label}</td><td>{action}</td></tr>",
            label = esc(item.label),
            action = esc(&super::create::describe(&item.action, channels, roles)),
        ));
    }
    format!(
        r#"<div class="main-inner"><div class="page-head"><div>{crumbs}<h1>Set up recommended layout</h1><p class="lede">Overlord will create only what is missing, then assign it. Anything already saved is left in place.</p></div></div><section class="card stretch"><div class="card-body flush"><table class="grid"><thead><tr><th>Setting</th><th>Plan</th></tr></thead><tbody>{rows}</tbody></table></div></section><form method="post" data-save><input type="hidden" name="csrf" value="{csrf}"><div class="savebar"><span class="pulse"></span><div class="msg">Create the missing channels and roles</div><button class="btn primary" type="submit">Create and assign</button></div></form></div>"#,
        crumbs = crumbs(Some((guild_id, guild_name)), "Layout"),
        csrf = esc(csrf),
    )
}

fn segment(name: &str, current: &str, options: &[(&str, &str)]) -> String {
    options
        .iter()
        .map(|(value, label)| {
            let checked = if *value == current { " checked" } else { "" };
            format!(
                "<label><input form=\"settings\" type=\"radio\" name=\"{name}\" value=\"{value}\"{checked}><span>{label}</span></label>",
                label = esc(label),
            )
        })
        .collect()
}

fn mark(class: &str) -> String {
    format!(
        r#"<svg class="{class}" viewBox="0 0 32 32" aria-hidden="true"><path class="shield" d="M16 2.6 27.2 7.2v8.6c0 6.35-4.72 11.05-11.2 13.5C9.52 26.85 4.8 22.15 4.8 15.8V7.2L16 2.6z"/><path class="eye" d="M9.6 15.2c1.7-3.1 4.05-4.7 6.4-4.7s4.7 1.6 6.4 4.7c-1.7 3.1-4.05 4.7-6.4 4.7s-4.7-1.6-6.4-4.7z"/><circle class="iris" cx="16" cy="15.2" r="2.05"/><circle class="glint" cx="16.7" cy="14.5" r="0.55"/></svg>"#
    )
}

fn line_icon(path: &str) -> String {
    format!(r#"<svg class="icon-line" viewBox="0 0 24 24" aria-hidden="true">{path}</svg>"#)
}

fn icon_shield() -> String {
    line_icon(r#"<path d="M12 3l8 3v6c0 5-3.4 8.6-8 10-4.6-1.4-8-5-8-10V6l8-3z"/>"#)
}
fn icon_ticket() -> String {
    line_icon(r#"<path d="M3 9a2 2 0 0 0 2-2V6a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v1a2 2 0 0 0 2 2v6a2 2 0 0 0-2 2v1a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-1a2 2 0 0 0-2-2V9z"/><path d="M12 7v10"/>"#)
}
fn icon_doc() -> String {
    line_icon(r#"<path d="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2"/><rect x="9" y="3" width="6" height="4" rx="1"/><path d="M9 12h6M9 16h4"/>"#)
}
fn icon_grid() -> String {
    line_icon(r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#)
}
fn icon_mic() -> String {
    line_icon(r#"<path d="M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z"/><path d="M19 11a7 7 0 0 1-14 0"/><path d="M12 18v3"/>"#)
}
fn icon_alert() -> String {
    line_icon(r#"<path d="M10.3 4.2 2.8 17a2 2 0 0 0 1.7 3h15a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0z"/><path d="M12 9v4M12 17h.01"/>"#)
}
fn icon_eye() -> String {
    line_icon(r#"<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>"#)
}
fn icon_search() -> String {
    line_icon(r#"<circle cx="11" cy="11" r="7"/><path d="M20 20l-3-3"/>"#)
}
fn icon_plus() -> String {
    line_icon(r#"<path d="M12 5v14M5 12h14"/>"#)
}
fn icon_info() -> String {
    line_icon(r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 8h.01"/>"#)
}
fn icon_menu() -> String {
    line_icon(r#"<path d="M5 7h14M5 12h14M5 17h14"/>"#)
}
fn icon_more() -> String {
    r#"<svg class="icon-solid" viewBox="0 0 24 24" aria-hidden="true"><circle cx="5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/></svg>"#.into()
}
fn icon_logout() -> String {
    line_icon(r#"<path d="M9 6H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3"/><path d="M14 16l4-4-4-4M10 12h8"/>"#)
}
fn icon_discord() -> String {
    r#"<svg class="icon-solid" viewBox="0 0 24 24" aria-hidden="true"><path d="M19.3 5.2A17 17 0 0 0 15 4l-.4.8a15 15 0 0 1 4 1.6 16 16 0 0 0-13.2 0A15 15 0 0 1 9.4 4.8L9 4a17 17 0 0 0-4.3 1.2C2.2 8.6 1.5 12 1.8 15.3A17 17 0 0 0 7 18l.8-1.1a11 11 0 0 1-1.6-.8l.4-.3c3.2 1.5 6.6 1.5 9.8 0l.4.3a11 11 0 0 1-1.6.8l.8 1.1a17 17 0 0 0 5.2-2.7c.5-3.8-.6-7.1-2.9-10.1ZM9.2 14.2c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.7.8 1.6 1.8-.7 1.8-1.6 1.8Zm5.6 0c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.7.8 1.6 1.8-.7 1.8-1.6 1.8Z"/></svg>"#.into()
}
fn chev() -> String {
    r#"<svg class="chev icon-line" viewBox="0 0 24 24" aria-hidden="true"><path d="M8 10l4 4 4-4"/></svg>"#.into()
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
    }

    #[test]
    fn guild_icons_render_on_the_list_and_switcher() {
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
        assert!(list.contains("https://cdn.discordapp.com/icons/10/abc123.png?size=128"));
        assert!(list.contains("alt=\"Pixel Forge\""));
        assert!(list.contains("width=\"44\""));
        assert!(list.contains("loading=\"lazy\""));
        assert!(list.contains("https://cdn.discordapp.com/icons/11/a_cafe.gif?size=128"));
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
        assert!(page.contains("server-switch"));
        assert!(page.contains("alt=\"Pixel Forge\""));
        assert!(page.contains("https://cdn.discordapp.com/icons/11/a_cafe.gif?size=128"));
        assert!(page.contains("m-tabbar"));
        assert!(!page.to_ascii_lowercase().contains("style="));
    }

    #[test]
    fn dump_pages_when_requested() {
        let Ok(dir) = std::env::var("DUMP_DASH") else {
            return;
        };
        let guilds = vec![
            UserGuild {
                id: 10,
                name: "Pixel Forge".into(),
                icon: None,
                owner: true,
                permissions: 0,
            },
            UserGuild {
                id: 11,
                name: "Night Owls".into(),
                icon: None,
                owner: false,
                permissions: 0,
            },
        ];
        let settings = GuildSettings::unconfigured(10);
        let channels = vec![
            ChannelChoice {
                id: 555,
                name: "mod-logs".into(),
                kind: ChannelKind::Text,
            },
            ChannelChoice {
                id: 30,
                name: "Support Tickets".into(),
                kind: ChannelKind::Category,
            },
        ];
        let roles = vec![RoleChoice {
            id: 20,
            name: "Staff".into(),
        }];
        let shell = |nav, body: String| Shell {
            guild_id: 10,
            guild_name: "Pixel Forge",
            guilds: &guilds,
            username: "modbot_fan",
            avatar: None,
            user_id: 9,
            nav,
            configured: true,
            flash: None,
            csrf: "token",
            body,
        };
        let stats = WeekStats {
            infractions_this_week: 2,
            infractions_last_week: 1,
            open_tickets: 1,
            claimed_tickets: 0,
            closed_this_week: 3,
            raid_alerts_this_week: 0,
            last_raid_at: None,
            temp_rooms: 0,
        };
        let files = [
            ("login.html", login_page(None)),
            (
                "servers.html",
                servers_page(&guilds, &[10], &[10], "modbot_fan", None, 9, None),
            ),
            (
                "overview.html",
                app_page(shell(
                    Nav::Overview,
                    overview_body(10, "Pixel Forge", &stats, &[], &|id| id.to_string(), &settings),
                )),
            ),
            (
                "security.html",
                app_page(shell(
                    Nav::Security,
                    security_body(10, "Pixel Forge", &settings, &channels, &roles, "token"),
                )),
            ),
            (
                "tickets.html",
                app_page(shell(
                    Nav::Tickets,
                    tickets_body(10, "Pixel Forge", &settings, &channels, &roles, 1, 3, "token"),
                )),
            ),
            (
                "audit.html",
                app_page(shell(Nav::Audit, audit_body(10, &[], 14, "Pixel Forge"))),
            ),
        ];
        for (name, html) in files {
            std::fs::write(format!("{dir}/{name}"), html).unwrap();
        }
    }

    #[test]
    fn settings_breadcrumbs_link_the_server_list_and_overview() {
        let settings = GuildSettings::unconfigured(10);
        let channels = Vec::new();
        let roles = Vec::new();
        let name = "Thrain's server";
        let pages = [
            security_body(10, name, &settings, &channels, &roles, "token"),
            verification_body(10, name, &settings, &channels, &roles, "token"),
            tickets_body(10, name, &settings, &channels, &roles, 0, 0, "token"),
            voice_body(10, name, &settings, &channels, 0, "token"),
            moderation_body(10, name, &[], &|id| id.to_string()),
            logs_body(10, name, &[], &[], &|id| id.to_string()),
            audit_body(10, &[], 14, name),
            overview_body(
                10,
                name,
                &WeekStats {
                    infractions_this_week: 0,
                    infractions_last_week: 0,
                    open_tickets: 0,
                    claimed_tickets: 0,
                    closed_this_week: 0,
                    raid_alerts_this_week: 0,
                    last_raid_at: None,
                    temp_rooms: 0,
                },
                &[],
                &|id| id.to_string(),
                &settings,
            ),
        ];
        for html in pages {
            assert!(html.contains("aria-label=\"Breadcrumb\""));
            assert!(html.contains("<ol>"));
            assert!(html.contains("href=\"/servers\""));
            assert!(html.contains("href=\"/servers/10\""));
            assert!(html.contains("aria-current=\"page\""));
            assert!(html.contains("Thrain&#39;s server"));
            assert!(!html.to_ascii_lowercase().contains("style="));
        }
        let list = servers_page(&[], &[], &[], "mod", None, 1, None);
        assert!(list.contains("aria-current=\"page\">Servers"));
        assert!(!list.contains("href=\"/servers/10\""));
    }

    #[test]
    fn login_page_offers_discord() {
        let html = login_page(Some("Sign-in was cancelled."));
        assert!(html.contains("Continue with Discord"));
        assert!(html.contains("href=\"/auth/discord\""));
        assert!(!html.contains("<form"));
        assert!(html.contains("Sign-in was cancelled."));
        assert!(html.contains("/assets/app.css"));
    }
}
