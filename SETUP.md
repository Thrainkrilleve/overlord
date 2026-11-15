# Setting up Discord Overlord on your server

This guide takes you from nothing to a running bot, about 20–30 minutes. Do the steps in order:
the bot needs the roles and channels to exist before you can fill in its settings.

One Overlord process can be in many servers. Each server has its own channels, roles, and
account-age rule. Nothing from one server is posted or applied in another.

**You will need:** a Discord server where you are the owner or an Administrator, and a computer that
stays on with [Docker Desktop](https://www.docker.com/products/docker-desktop/) (or Docker on Linux) installed.

---

## 1. Create the bot in the Discord Developer Portal

1. Go to <https://discord.com/developers/applications> and click **New Application**. Name it (e.g. "Overlord").
2. Open the **Bot** tab.
   - Click **Reset Token**, then **Copy**. This is your `DISCORD_TOKEN`. Keep it secret: anyone with it controls your bot.
     If it ever leaks, reset it again here.
   - Under **Privileged Gateway Intents**, turn on **Server Members Intent** and **Message Content Intent**, then **Save Changes**.
     Without these the bot will not start, or will not see joins and messages.
   - Optional: turn off **Public Bot** so only you can add it to servers.
3. Open the **General Information** tab and copy the **Application ID**. You need it for the invite link below.

## 2. Invite the bot to your server

Open this link in your browser, with `YOUR_APPLICATION_ID` replaced by the ID you just copied:

```
https://discord.com/oauth2/authorize?client_id=YOUR_APPLICATION_ID&scope=bot+applications.commands&permissions=1099798014998
```

Pick your server and click **Authorize**. This grants exactly the permissions the bot uses: View Channels,
Send Messages, Embed Links, Attach Files, Read Message History, Manage Messages, Manage Channels,
Manage Roles, Kick Members, Ban Members, Timeout Members, Connect and Move Members.
(Granting Administrator instead also works, but gives the bot more power than it needs.)

## 3. Turn on Developer Mode (to copy IDs)

In Discord: **User Settings → Advanced → Developer Mode → on**. From now on you can right-click any
server, channel, category or role and choose **Copy ID**.

## 4. Create the roles

In **Server Settings → Roles**, create:

| Role | Purpose |
| :--- | :--- |
| `Member` | Given to people after they pass verification. This is what unlocks the server. |
| `Unverified` | Given automatically on join, removed on verification. Useful as a label for staff; optional. |
| `Staff` | Your moderators. Can see tickets and use the ticket Claim/Close buttons. |

Then **drag the bot's own role (named after your bot) above `Member`, `Unverified` and `Staff`**, and above
every role whose holders you want it to be able to time out, kick or ban. Discord does not let a bot
manage anyone whose top role is above its own. This is the most common cause of setup problems.

Give `Staff` the permissions your moderators need: **Timeout Members** for `/warn`, `/timeout` and
`/infractions`, plus **Kick Members**, **Ban Members** and **Manage Messages** for `/kick`, `/ban` and `/purge`.

## 5. Lock the server behind verification

The idea: nobody sees anything until they verify, except the verification channel.

1. **Server Settings → Roles → @everyone**: turn **off** View Channels.
2. **Server Settings → Roles → Member**: turn **on** View Channels.
3. Create a text channel `#verify`. In its **Permissions**, give **@everyone** a green check on
   **View Channel** and **Read Message History**, and a red X on **Send Messages**.
   (Optional: give **Member** a red X on View Channel, so verified people no longer see it.)

## 6. Create the channels

| Channel | How to set it up |
| :--- | :--- |
| `#mod-log` (text) | Visible to `Staff` only. Security alerts and moderation actions are posted here. |
| `#transcripts` (text) | Visible to `Staff` only. Closed ticket transcripts are uploaded here as `.txt` files. |
| `#support` (text) | Visible to `Member`. The "open a ticket" panel lives here; set Send Messages to off so it stays tidy. |
| `Tickets` (category) | `@everyone`: red X on View Channel. Ticket channels are created in here; each one is visible only to its opener and `Staff`. |
| `#honeypot` (text) | The trap: **any non-staff message here gets the sender banned instantly**. See the note below. |
| `Voice Rooms` (category) | Temporary voice rooms are created in here. |
| `➕ Join to Create` (voice) | Put it inside `Voice Rooms`. Joining it creates a personal room and moves you in. |

**Honeypot channel:** make it visible to `Member` (scripts that verify and then spam every channel get
caught), and put a clear topic on it, e.g. "⚠️ Do not post here. Automated ban trap." Staff (the
`Staff` role, the server owner, or anyone with Manage Messages or Administrator) are never banned:
their message is deleted and a "🧪 Honeypot Test" note is posted to `#mod-log` instead. That is also
how you test the trap without a second account.

Make sure the bot can see and post in `#mod-log`, `#transcripts`, `#verify` and `#support`. If you
restricted a channel to `Staff`, also add the bot's role to that channel's permissions.

## 7. Fill in the configuration file

In the project folder, copy the example file:

```bash
cp .env.example .env
```

Open `.env` in a text editor. The token is required. `GUILD_ID` and the channel and role IDs are
the **starting settings for this first server**. The first time the bot starts, it copies them
into the database for that server only. After that, change them in Discord with `/config set`.
Editing the IDs in `.env` again does not overwrite a server that already has saved settings.

| Setting | What to copy |
| :--- | :--- |
| `DISCORD_TOKEN` | The bot token from step 1 |
| `GUILD_ID` | Right-click your **server icon** → Copy Server ID. This is the server that receives the IDs below. |
| `DEV_GUILD_ONLY` | Leave unset. Set to `true` only on a development machine, to register slash commands in `GUILD_ID` and nowhere else. |
| `DATABASE_URL` | Leave as is. Docker overrides it to store data in `./data`. |
| `HONEYPOT_CHANNEL_ID` | `#honeypot` |
| `VERIFY_CHANNEL_ID` | `#verify` (shown in `/security_status`; optional). `/setup_verification` also saves the channel you run it in. |
| `MOD_LOG_CHANNEL_ID` | `#mod-log` |
| `TRANSCRIPT_CHANNEL_ID` | `#transcripts`. **Required for tickets**: a ticket can't be closed until it's set, so no history is ever lost. |
| `JOIN_TO_CREATE_CHANNEL_ID` | The `➕ Join to Create` voice channel |
| `VOICE_CATEGORY_ID` | The `Voice Rooms` category |
| `TICKET_CATEGORY_ID` | The `Tickets` category |
| `UNVERIFIED_ROLE_ID` | `Unverified` role (Server Settings → Roles → right-click) |
| `MEMBER_ROLE_ID` | `Member` role |
| `MOD_ROLE_ID` | `Staff` role |
| `MIN_ACCOUNT_AGE_DAYS` | Accounts younger than this get flagged in `#mod-log`. Default 3. |

To leave a feature off for this first server, delete its line (for example remove `HONEYPOT_CHANNEL_ID`).
In a server that is already saved, use `/config clear` instead.

## 8. Start the bot

From the project folder:

```bash
docker compose up -d --build
```

The first build takes a few minutes. Then watch the logs:

```bash
docker compose logs -f
```

You should see `Bot online as Overlord#1234`, `Registering slash commands globally`, and
`Registered slash commands in guild ...` for each server the bot is already in. A line about
saving `.env` settings means this first server was copied into the database. Any
`WARN Guild ... is unset` lines tell you which features are still off. Press `Ctrl+C` to stop
watching; the bot keeps running. It restarts automatically after crashes or reboots (as long as Docker is running).

Slash commands are registered in every joined server, including when the bot is invited to a new
one, and also globally. A server shows one copy of each command. With `DEV_GUILD_ONLY=true`,
commands are registered only in `GUILD_ID`.

## 9. Post the panels

In Discord:

1. In `#verify`, run `/setup_verification`. This posts the **Verify Identity** button.
2. In `#support`, run `/setup_tickets`. This posts the three ticket buttons.
3. Anywhere, run `/config show` or `/security_status` to confirm the channels and roles were picked up.
   Both replies are visible only to you, because they name the honeypot channel.

The panels keep working across bot restarts; you only post them once.

## 10. Test everything with a second account

Use an alt account (or a friend) that is **not** staff:

- [ ] **Join**: gets the `Unverified` role and sees only `#verify`. A brand-new account shows up in `#mod-log` as flagged.
- [ ] **Verify**: click **Verify Identity**, type the code shown in the box, and gain the `Member` role and the rest of the server.
- [ ] **Ticket**: open one from `#support`. A private channel appears under `Tickets`. Opening a second one is refused.
      As staff, click **Claim**, then **Close & Archive**; a transcript appears in `#transcripts` and the channel is deleted.
      The alt clicking Claim/Close gets "Only support staff can use this control."
- [ ] **AutoMod**: the alt posts `discord.gg/test`. The message disappears, the alt is timed out for 10 minutes, and it's logged.
      (Remove the timeout via right-click → Remove Timeout.)
- [ ] **Moderation**: `/warn` the alt. It receives a DM, and the case appears in `/infractions` and `#mod-log`.
- [ ] **Voice**: join `➕ Join to Create`, get moved into your own room, leave, and the room disappears.
- [ ] **Honeypot (as staff, no alt needed)**: post anything in `#honeypot` yourself. The message disappears
      and `#mod-log` shows "🧪 Honeypot Test: Trap Working".
- [ ] **Honeypot (real ban, optional, do last)**: the alt posts in `#honeypot` and is banned. If `#mod-log`
      says "the ban failed", the bot's role is too low. Unban the alt in Server Settings → Bans.

## 11. Add another server

Invite the same bot with the link from step 2. In the new server, create the roles and channels
you want (steps 4–6), then set them in Discord. Nothing is copied from the first server.

1. Run `/config set` once per value you want on: mod log, transcript, staff role, member role,
   unverified role, honeypot, verify channel, ticket category, join-to-create channel, temp voice
   category, and minimum account age. Pick the channel or role from that server. A channel or role
   from a different server is rejected.
2. Run `/config show` and check each field.
3. In `#verify`, run `/setup_verification`. In `#support`, run `/setup_tickets`.
4. Until a feature is set, it stays off in that server. Closing a ticket tells staff to set a
   transcript channel. Verification tells the member when no member role is set.

`/security_status`, mod logs, raid alerts, role grants, ticket channels, and transcripts all use
the server the command or event happened in.

## 12. Optional web dashboard

The dashboard is off until you set `DASHBOARD_ENABLED=true`. It runs in the same process and the
same `./data/overlord.db`, so a saved setting is what the bot uses on the next event. Do not open
the dashboard port on your router. Put it behind a Cloudflare Tunnel. The host side stays on
`127.0.0.1`.

1. In the [Discord Developer Portal](https://discord.com/developers/applications), open your
   application → **OAuth2**. Add a redirect:
   `https://overlord.example.com/auth/callback`
   Copy the client id and create a client secret. Those are `DASHBOARD_DISCORD_CLIENT_ID` and
   `DASHBOARD_DISCORD_CLIENT_SECRET`.
2. Open **Installation**. Set **Install Link** to **Custom URL** and use
   `https://overlord.example.com/install`
   That address is a public `GET /install` on the dashboard. It answers **302** with Discord's bot
   invite (`scope` `bot` and `applications.commands`, permissions from `DASHBOARD_INVITE_PERMISSIONS`).
   The "Add Overlord to a server" card calls `/install?guild_id=...` so the same route can pre-select
   a server. Replace `overlord.example.com` with the hostname your tunnel serves. Do not commit that
   hostname.
3. In `.env`:

```bash
DASHBOARD_ENABLED=true
DASHBOARD_PUBLIC_URL=https://overlord.example.com
DASHBOARD_OAUTH_REDIRECT_URL=https://overlord.example.com/auth/callback
DASHBOARD_DISCORD_CLIENT_ID=123456789012345678
DASHBOARD_DISCORD_CLIENT_SECRET=your-oauth-client-secret
DASHBOARD_COOKIE_SECURE=true
DASHBOARD_TRUST_CLOUDFLARE=true
DASHBOARD_INVITE_PERMISSIONS=1099798014998
# Host port on 127.0.0.1. Leave 8080, or change it when that port is already taken.
DASHBOARD_HOST_PORT=8080
```

4. Point the tunnel at the dashboard. `docker-compose.yml` publishes
   `127.0.0.1:${DASHBOARD_HOST_PORT:-8080}` only. A tunnel on the same machine uses
   `http://127.0.0.1:8080`, or `http://127.0.0.1:<DASHBOARD_HOST_PORT>` when you changed it.
   A tunnel container on the compose network uses `http://overlord:8080` (the port inside the
   container) and does not need the host port.
   Set `DASHBOARD_TRUST_CLOUDFLARE=true` only when that tunnel is the only way in. It tells the
   rate limiter to read `CF-Connecting-IP`. Leave it false if anything else can reach the process.
5. Restart: `docker compose up -d --build`. Sign in with **Continue with Discord**. You only see
   servers where you have Manage Server. Saving a form checks that permission again, checks that
   each channel and role belongs to that server, and writes an audit row.

`DASHBOARD_COOKIE_SECURE=false` is only for a local trial at `http://127.0.0.1:<DASHBOARD_HOST_PORT>`
(default `http://127.0.0.1:8080`). The tunnel deployment stays `true`.

## Troubleshooting

| Symptom | Fix |
| :--- | :--- |
| Bot doesn't start; logs mention **Disallowed intents** | Turn on both privileged intents (step 1) and restart: `docker compose restart`. |
| Slash commands don't show up | Leave `DEV_GUILD_ONLY` unset so commands register in every server. Global registration can take up to an hour if the per-server update failed; check the logs for `Could not register slash commands`. Re-invite with the link in step 2 if `applications.commands` was missing. |
| "Verification Incomplete" / roles not assigned / "Missing Permissions" in the logs | The bot's role is too low. Drag it above `Member` and `Unverified` (step 4). |
| `/ban` or `/kick` says "You can't moderate a member whose highest role is equal to or above yours" | Working as intended: moderators can only act on people below them. |
| "Transcript could not be archived" when closing a ticket | Run `/config set` and choose this server's transcript channel. The bot must be able to post and attach files there. |
| A new server does nothing (no role, no log, no rooms) | That server has no saved settings yet. Run `/config set` there. Settings are never copied from another server. |
| AutoMod or honeypot does nothing | Message Content Intent is off, or the bot can't see the channel. Staff are exempt from AutoMod by design. |
| Temp voice rooms aren't created | Bot needs Manage Channels, Connect and Move Members in the `Voice Rooms` category. |

## Updating and backups

From the project folder, with the existing `.env` and `./data` directory left in place:

```bash
git pull
docker compose up -d --build
docker compose logs -f
```

On startup the bot opens `./data/overlord.db` (the same file as before) and applies any new
schema itself. You do not run a migration command, and you do not delete `./data`. The first start
after this upgrade copies the channel, role, and account-age values from `.env` into the `GUILD_ID`
server **only if that server has no saved settings yet**. Later edits in `.env` do not replace them.
Use `/config set` for changes, and for every additional server.

- **Backup:** infractions, tickets, temp-voice rooms, and per-server settings are in `data/overlord.db`. Copy that file somewhere safe before upgrading.
- **Stop:** `docker compose down`. Your data stays in `./data`.
- **Run without Docker** (for testing): install Rust 1.88+, then `cargo run --release` in the project folder. The database file is the one in `DATABASE_URL`, and the same migration runs on startup.
