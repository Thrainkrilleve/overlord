# 🛡️ Discord Overlord

> **High-Performance Discord Security, Moderation, Support Tickets & Server Operations Suite written in Rust.**

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Serenity](https://img.shields.io/badge/serenity-0.12-blue.svg)](https://github.com/serenity-rs/serenity)
[![Poise](https://img.shields.io/badge/poise-0.6-purple.svg)](https://github.com/serenity-rs/poise)
[![Docker](https://img.shields.io/badge/Docker-Supported-2496ED.svg)](https://www.docker.com/)

---

## ⚡ Key Capabilities

### 1. 🚨 Security & Honeypot Bot Traps
* **Honeypot Trap Channel**: Designate an invisible or restricted trap channel. Any scraper, self-bot, or automated spam script sending a message is **instantly banned** and purged from the server. Staff posts are deleted and logged as a test, never banned.
* **Account Age Filter**: Detects newly registered accounts (e.g., < 3 days) or default-avatar accounts and sends security alerts to that server's staff log.
* **Anti-Raid Join Tracker**: Sliding-window join monitor detects raid surges (5+ joins within 10 seconds) per server and alerts that server's staff, at most once a minute during a sustained raid.

### 2. 🛡️ Verification Gate & Onboarding
* **Code Verification**: Button-triggered modal (`/setup_verification`) showing a random one-time code per user in that server, valid for 5 minutes. A code from one server cannot be used in another.
* **Automated Role Provisioning**: Automatically revokes the `Unverified` role and assigns the verified `Member` role upon successful completion.

### 3. 🎫 Interactive Support Ticket System
* **Pre-Ticket Category Hub**: Deploy a persistent panel (`/setup_tickets`) with buttons for **General Support**, **Report User / Scam**, and **VIP / Roles**.
* **Modal Collection**: Prompts users for Subject and Detailed Explanation before spinning up a channel.
* **Isolated Permissions**: Automatically scopes private channel access to the ticket creator and staff roles (`@Staff`).
* **In-Channel Controls**:
  - `[ 🙋 Claim Ticket ]`: Claims ownership to prevent staff overlap (staff only).
  - `[ 🔒 Close & Archive ]`: Fetches the full conversation history, builds a timestamped text transcript, uploads it to `#transcripts`, and only then deletes the channel (staff only).
* **One open ticket per user** to prevent channel spam.

### 4. 🔨 Moderation & Auto-Defense
* **AutoMod Message Engine**: Automatic detection (in new and edited messages) and purging of unauthorized Discord invite links and mass-mention spam (> 5 mentions), complete with automatic 10-minute timeouts and SQLite case logging. Staff are exempt.
* **Role hierarchy enforced**: moderators can only act on members whose top role is below their own.
* **Moderation Slash Commands**:
  - `/warn <user> <reason>`: Issues a warning, DMs the user, and records the infraction in SQLite.
  - `/timeout <user> <minutes> <reason>`: Applies a native Discord communication timeout.
  - `/kick <user> <reason>`: DMs the user, kicks them, and logs the case.
  - `/ban <user> <reason> [days]`: DMs the user, bans them, prunes recent messages, and logs the case.
  - `/purge <count>`: Bulk-deletes messages (1–100, newer than 14 days).
  - `/infractions <user>`: Displays past moderation case history.

### 5. 🔊 Dynamic Temporary Voice (Join-to-Create)
* Users connect to a designated master channel (e.g., `➕ Join to Create`).
* Overlord automatically spins up a dedicated voice channel `🔊 {Username}'s Room`, moves the member in, and tracks the room.
* When everyone leaves, the room is automatically deleted.

---

## 🚀 Setup & Installation

> **New here? Follow the step-by-step [SETUP.md](SETUP.md) guide.** It covers creating the bot, roles,
> channels, permissions, configuration, first run, and a test checklist. The summary below is for reference.

### Prerequisites
1. **Discord Bot Token**:
   - Create an application on the [Discord Developer Portal](https://discord.com/developers/applications).
   - Under **Bot**, enable the following **Privileged Gateway Intents**:
     - `SERVER MEMBERS INTENT`
     - `MESSAGE CONTENT INTENT`
2. **Invite Link**:
   - Under **OAuth2 -> URL Generator**, select `bot` and `applications.commands`.
   - Permissions needed: `Administrator` (or View Channels, Manage Channels, Manage Roles, Kick, Ban, Timeout, Manage Messages, Move Members).

### One process, many servers

Invite the same bot to every server you moderate. Each server keeps its own channels, roles, account-age rule, raid policy, and ticket limits in SQLite. A log, transcript, role grant, or raid count from one server is never applied in another. A feature that has not been set in a server does nothing there.

`GUILD_ID` and the channel and role IDs in `.env` are only the **first-run copy** for that one server. After the first start, change them in Discord or in the dashboard. Later edits to those `.env` values do not overwrite a server that already has saved settings.

### Configuration (`.env`)
```bash
cp .env.example .env
```

Fill in `DISCORD_TOKEN`. Set `GUILD_ID` to the server that should receive the initial channel and role IDs. Leave `DASHBOARD_ENABLED=false` until you want the web UI.

### Per-server slash commands

Administrators configure the server they are standing in:

| Command | What it does |
| :--- | :--- |
| `/config show` | Shows this server's saved settings. Only visible to you. |
| `/config set` | Sets one channel, role, or the minimum account age. The choice must belong to this server. |
| `/config clear` | Turns a channel or role feature off. Clearing account age resets it to 3 days. |
| `/security_status` | Same picture as `/config show`, including honeypot, raid window, and ticket limit. |
| `/setup_verification` | Posts the verification button and remembers this channel as the verify channel. |
| `/setup_tickets` | Posts the ticket panel in the current channel. |

Case numbers start at 1 in each server. `/infractions` only lists cases from the server where you ran it.

### Web dashboard

The dashboard is optional and served by the same process, on the same SQLite file. A save is live: the next join, message, or ticket uses the new value.

Turn it on with `DASHBOARD_ENABLED=true` and the `DASHBOARD_*` values in `.env.example`. Sign in with **Continue with Discord** (OAuth2 authorization code, PKCE, HttpOnly `SameSite=Lax` cookies). The server list is only communities where you have **Manage Server** or **Administrator**. If Overlord is already in that server, open its dashboard and edit that server's settings. If not, the card opens `/install?guild_id=...`. The separate **Add Overlord to a server** card is `/install` with no server pre-selected. Every save checks the permission again with Discord, rejects a channel or role that is not in that server, and appends a row to the audit log (who, which setting, old value, new value, when).

`GET /install` is public. It answers **302** with Discord's bot invite (`bot` and `applications.commands`, permissions from `DASHBOARD_INVITE_PERMISSIONS`). Use it as the application's **Custom Install URL**. The "Add Overlord to a server" card calls `/install?guild_id=...` so the same route can pre-select a server.

Example, with a placeholder hostname:

```text
https://overlord.example.com/install
https://overlord.example.com/auth/callback
```

Replace `overlord.example.com` with the hostname your tunnel serves. Do not commit that hostname or the OAuth client secret.

### Self-host with Docker and a Cloudflare Tunnel

The container does not publish the dashboard on the public internet. `docker-compose.yml` binds `127.0.0.1` only. The host port is `DASHBOARD_HOST_PORT` (default `8080`). Change it in `.env` when 8080 is already taken. Inside the container the process still listens on `8080`.

```bash
cp .env.example .env
# set DISCORD_TOKEN, GUILD_ID, and the DASHBOARD_* values
docker compose up -d --build
docker compose logs -f
```

Point a Cloudflare Tunnel at the loopback port. With the default `DASHBOARD_HOST_PORT`:

```bash
cloudflared tunnel --url http://127.0.0.1:8080
```

If you set `DASHBOARD_HOST_PORT=8081`, use `http://127.0.0.1:8081` instead. For a named tunnel, the public hostname is whatever you configure in Cloudflare (for example `https://overlord.example.com`) and the origin is that same loopback URL. If `cloudflared` runs as another container on the compose network, use `http://overlord:8080` (the port inside the container, not `DASHBOARD_HOST_PORT`) and you can drop the host port. Set `DASHBOARD_TRUST_CLOUDFLARE=true` only when that tunnel is the only client, so rate limits use `CF-Connecting-IP`.

In the Discord Developer Portal:

1. **OAuth2 → Redirects:** `https://overlord.example.com/auth/callback`
2. **Installation → Install Link → Custom URL:** `https://overlord.example.com/install`

The step-by-step version, including roles, channels, and the first-run database copy, is [SETUP.md](SETUP.md).

---

## 🐳 Local cargo run

Docker and the tunnel are covered above. To run on the host instead:

```bash
# Requires Rust 1.88+
cargo test --locked
cargo run --release
```

Stop a Docker deploy with `docker compose down`. The database stays in `./data`.

---

## 🛠️ Slash Commands Reference

| Command | Permission | Description |
| :--- | :--- | :--- |
| `/setup_verification` | Administrator | Deploys the interactive human verification gate in the current channel. |
| `/setup_tickets` | Administrator | Deploys the support & ticket assistance hub in the current channel. |
| `/security_status` | Administrator | Displays this server's honeypot, log channels, roles, and security policy. Only visible to you. |
| `/config show` | Administrator | Shows this server's saved settings. Only visible to you. |
| `/config set` | Administrator | Sets one channel, role, or the minimum account age for this server. |
| `/config clear` | Administrator | Turns off one channel or role feature in this server. Clearing the account age resets it to 3 days. |
| `/warn` | Moderate Members | Issues a warning and creates an infraction case. |
| `/timeout` | Moderate Members | Applies a native communication timeout for $N$ minutes. |
| `/kick` | Kick Members | Kicks a member from the guild. |
| `/ban` | Ban Members | Bans a member and prunes up to 7 days of messages. |
| `/purge` | Manage Messages | Deletes up to 100 messages in the channel. |
| `/infractions` | Moderate Members | Displays this server's infraction history for a user. Case numbers start at 1 in each server. |

---

## 📂 Architecture

```
src/
├── main.rs                   # Framework initialization, intents, per-guild event routing
├── config.rs                 # Process settings (token, database, optional dev guild)
├── guild_settings.rs         # Per-server channels, roles, and account-age policy
├── util.rs                   # Embed limits, mod-log posting, same-server checks
├── database.rs               # SQLite pool, schema migrations, cases, tickets, temp voice
├── audit.rs                  # Who changed which setting, old value and new value
├── dashboard/                # Optional web UI, Discord login, and GET /install
└── modules/
    ├── settings.rs           # /config show, set, and clear
    ├── security/             # Honeypot, account age filters, per-guild anti-raid
    ├── verification/         # Verification gate, per-server codes, role grants
    ├── moderation/           # AutoMod regex/mention filters, slash commands
    ├── tickets/              # Ticket hub panel, modal builder, transcripts
    └── voice/                # Join-to-create temporary voice channels
```


## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) and the [code of conduct](CODE_OF_CONDUCT.md).

To report a vulnerability, follow [SECURITY.md](SECURITY.md) instead of opening a public issue.
## License

MIT. See [LICENSE](LICENSE).
