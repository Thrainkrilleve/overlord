# Setting up Discord Overlord on your server

This guide takes you from nothing to a running bot, about 20–30 minutes. Do the steps in order:
the bot needs the roles and channels to exist before you can fill in its settings.

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

Open `.env` in a text editor and fill in each value (right-click the item in Discord → **Copy ID**):

| Setting | What to copy |
| :--- | :--- |
| `DISCORD_TOKEN` | The bot token from step 1 |
| `GUILD_ID` | Right-click your **server icon** → Copy Server ID. Makes slash commands appear instantly. |
| `DATABASE_URL` | Leave as is. Docker overrides it to store data in `./data`. |
| `HONEYPOT_CHANNEL_ID` | `#honeypot` |
| `VERIFY_CHANNEL_ID` | `#verify` (shown in `/security_status`; optional) |
| `MOD_LOG_CHANNEL_ID` | `#mod-log` |
| `TRANSCRIPT_CHANNEL_ID` | `#transcripts`. **Required for tickets**: a ticket can't be closed until it's set, so no history is ever lost. |
| `JOIN_TO_CREATE_CHANNEL_ID` | The `➕ Join to Create` voice channel |
| `VOICE_CATEGORY_ID` | The `Voice Rooms` category |
| `TICKET_CATEGORY_ID` | The `Tickets` category |
| `UNVERIFIED_ROLE_ID` | `Unverified` role (Server Settings → Roles → right-click) |
| `MEMBER_ROLE_ID` | `Member` role |
| `MOD_ROLE_ID` | `Staff` role |
| `MIN_ACCOUNT_AGE_DAYS` | Accounts younger than this get flagged in `#mod-log`. Default 3. |

To switch a feature off, delete its line (e.g. remove `HONEYPOT_CHANNEL_ID` to disable the trap).

## 8. Start the bot

From the project folder:

```bash
docker compose up -d --build
```

The first build takes a few minutes. Then watch the logs:

```bash
docker compose logs -f
```

You should see `Bot online as Overlord#1234` and `Registering slash commands to guild ID: ...`.
Any `WARN ... unset` lines tell you which settings are missing. Press `Ctrl+C` to stop watching;
the bot keeps running. It restarts automatically after crashes or reboots (as long as Docker is running).

## 9. Post the panels

In Discord:

1. In `#verify`, run `/setup_verification`. This posts the **Verify Identity** button.
2. In `#support`, run `/setup_tickets`. This posts the three ticket buttons.
3. Anywhere, run `/security_status` to confirm the channels and roles were picked up.

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

## Troubleshooting

| Symptom | Fix |
| :--- | :--- |
| Bot doesn't start; logs mention **Disallowed intents** | Turn on both privileged intents (step 1) and restart: `docker compose restart`. |
| Slash commands don't show up | Check `GUILD_ID` is your server's ID. Without it, commands register globally and can take up to an hour. Re-invite with the link in step 2 if `applications.commands` was missing. |
| "Verification Incomplete" / roles not assigned / "Missing Permissions" in the logs | The bot's role is too low. Drag it above `Member` and `Unverified` (step 4). |
| `/ban` or `/kick` says "You can't moderate a member whose highest role is equal to or above yours" | Working as intended: moderators can only act on people below them. |
| "Transcript could not be archived" when closing a ticket | Set `TRANSCRIPT_CHANNEL_ID` and make sure the bot can post and attach files there. |
| AutoMod or honeypot does nothing | Message Content Intent is off, or the bot can't see the channel. Staff are exempt from AutoMod by design. |
| Temp voice rooms aren't created | Bot needs Manage Channels, Connect and Move Members in the `Voice Rooms` category. |

## Updating and backups

- **Update:** pull the new code, then run `docker compose up -d --build` again.
- **Backup:** all data (infractions, tickets) is in `data/overlord.db`. Copy that file somewhere safe.
- **Stop:** `docker compose down`. Your data stays in `./data`.
- **Run without Docker** (for testing): install Rust 1.88+, then `cargo run --release` in the project folder.
