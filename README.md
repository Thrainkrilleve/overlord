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
* **Account Age Filter**: Detects newly registered accounts (e.g., < 3 days) or default-avatar accounts and sends security alerts to staff logs.
* **Anti-Raid Join Tracker**: Sliding-window join monitor detects raid surges (5+ joins within 10 seconds) and alerts staff, at most once a minute during a sustained raid.

### 2. 🛡️ Verification Gate & Onboarding
* **Code Verification**: Button-triggered modal (`/setup_verification`) showing a random one-time code per user, valid for 5 minutes.
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

### Configuration (`.env`)
Copy `.env.example` to `.env` and fill in your server IDs:

```bash
cp .env.example .env
```

---

## 🐳 Running with Docker (Home Server / Ubuntu)

### Using Docker Compose (Recommended)
```bash
docker compose up -d --build
```

View logs:
```bash
docker compose logs -f
```

Stop the bot:
```bash
docker compose down
```

### Direct Cargo Run (Local Testing)
```bash
# Requires Rust 1.88+
cargo test
cargo run --release
```

---

## 🛠️ Slash Commands Reference

| Command | Permission | Description |
| :--- | :--- | :--- |
| `/setup_verification` | Administrator | Deploys the interactive human verification gate in the current channel. |
| `/setup_tickets` | Administrator | Deploys the support & ticket assistance hub in the current channel. |
| `/security_status` | Administrator | Displays live status of honeypots, log channels, roles, and security policies. |
| `/warn` | Moderate Members | Issues a warning and creates an infraction case. |
| `/timeout` | Moderate Members | Applies a native communication timeout for $N$ minutes. |
| `/kick` | Kick Members | Kicks a member from the guild. |
| `/ban` | Ban Members | Bans a member and prunes up to 7 days of messages. |
| `/purge` | Manage Messages | Deletes up to 100 messages in the channel. |
| `/infractions` | Moderate Members | Displays past infraction history for a user from SQLite. |

---

## 📂 Architecture

```
src/
├── main.rs                   # Framework initialization, intents, slash command router
├── config.rs                 # Environment configuration loader
├── util.rs                   # Embed text limits, mod-log posting, error logging
├── database.rs               # SQLite connection pool and schema migrations
└── modules/
    ├── security/             # Honeypot, account age filters, anti-raid heuristics
    ├── verification/         # Verification gate, modals, role grants
    ├── moderation/           # AutoMod regex/mention filters, slash commands
    ├── tickets/              # Ticket hub panel, modal builder, transcripts
    └── voice/                # Join-to-create temporary voice channels
```


## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) and the [code of conduct](CODE_OF_CONDUCT.md).

To report a vulnerability, follow [SECURITY.md](SECURITY.md) instead of opening a public issue.
## License

MIT. See [LICENSE](LICENSE).
